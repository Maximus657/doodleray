param(
    [ValidateSet("5.4.3", "5.4.4", "5.4.5", "5.9.1")]
    [string] $FromVersion = "5.9.1",

    [string] $RemoteRcInstaller = "C:\DoodleRayQA\artifacts\DoodleRay-v6-rc-setup.exe",
    [string] $ExpectedRcVersion = "6.0.1",
    [string] $EvidenceDir = "C:\DoodleRayQA\evidence",
    [switch] $InjectStaleWinInet,
    [switch] $InjectCorporatePac,
    [string] $SecretPath = (Join-Path $PSScriptRoot "..\..\secrets\doodlevpn-server-access.md"),
    [string] $SubscriptionSecretPath = (Join-Path $PSScriptRoot "..\..\secrets\doodlevpn-test-subscription-url.txt")
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$injectStaleWinInetLiteral = if ($InjectStaleWinInet.IsPresent) { '$true' } else { '$false' }
$injectCorporatePacLiteral = if ($InjectCorporatePac.IsPresent) { '$true' } else { '$false' }

# Synthetic corporate PAC URL. It must survive the update untouched: DoodleRay
# may only clean DoodleRay-owned loopback proxy state, never corporate config.
$corporatePacUrl = "http://corp-proxy.qa-update-test.invalid/proxy.pac"

$remoteScript = @"
`$ErrorActionPreference = "Stop"
`$ProgressPreference = "SilentlyContinue"

function Get-ServiceJson {
    `$serviceExe = "C:\Program Files\DoodleRay\DoodleRayService.exe"
    if (-not (Test-Path -LiteralPath `$serviceExe)) {
        return `$null
    }
    `$raw = (& `$serviceExe status 2>&1 | Out-String).Trim()
    try {
        return [pscustomobject]@{ raw = `$null; parsed = (`$raw | ConvertFrom-Json) }
    } catch {
        return [pscustomobject]@{ raw = `$raw; parsed = `$null }
    }
}

function Get-StatsQueryOrphanCount {
    @(
        Get-CimInstance Win32_Process -Filter "name = 'xray.exe'" -ErrorAction SilentlyContinue |
        Where-Object { `$_.CommandLine -match "api\s+statsquery" }
    ).Count
}

function Get-WinInetSummary {
    Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings" |
        Select-Object ProxyEnable,ProxyServer,ProxyOverride,AutoDetect,AutoConfigURL
}

function Write-Evidence {
    param([Parameter(Mandatory = `$true)][string] `$Name, [Parameter(Mandatory = `$true)] `$Object)
    New-Item -ItemType Directory -Force -Path "$EvidenceDir" | Out-Null
    `$path = Join-Path "$EvidenceDir" `$Name
    `$Object | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath `$path -Encoding UTF8
    return `$path
}

function Install-Silently {
    param([Parameter(Mandatory = `$true)][string] `$Installer)
    `$process = Start-Process -FilePath `$Installer -ArgumentList "/S" -Wait -PassThru
    if (`$process.ExitCode -ne 0) {
        throw "installer `$Installer exited with code `$(`$process.ExitCode)"
    }
    Start-Sleep -Seconds 6
}

New-Item -ItemType Directory -Force -Path "C:\DoodleRayQA\artifacts" | Out-Null

`$oldInstaller = "C:\DoodleRayQA\artifacts\DoodleRay_${FromVersion}_x64-setup.exe"
if (-not (Test-Path -LiteralPath `$oldInstaller)) {
    `$url = "https://github.com/Maximus657/doodleray/releases/download/v$FromVersion/DoodleRay_${FromVersion}_x64-setup.exe"
    & curl.exe -L --fail --max-time 900 -o `$oldInstaller `$url
    if (`$LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath `$oldInstaller)) {
        throw "failed to download previous public installer v$FromVersion"
    }
}

if (-not (Test-Path -LiteralPath "$RemoteRcInstaller")) {
    throw "RC installer not found on stand: $RemoteRcInstaller"
}

`$before = [pscustomobject]@{
    timestampUtc = (Get-Date).ToUniversalTime().ToString("o")
    fromVersion = "$FromVersion"
    oldInstallerSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath `$oldInstaller).Hash
    rcInstallerSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath "$RemoteRcInstaller").Hash
    existingService = Get-Service DoodleRayTunnelService -ErrorAction SilentlyContinue | Select-Object Name,Status,StartType
    winInet = Get-WinInetSummary
}
`$beforePath = Write-Evidence -Name "update-before-$FromVersion.json" -Object `$before

Install-Silently `$oldInstaller

`$oldState = [pscustomobject]@{
    timestampUtc = (Get-Date).ToUniversalTime().ToString("o")
    installedAppSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath "C:\Program Files\DoodleRay\DoodleRay.exe").Hash
    installedServiceSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath "C:\Program Files\DoodleRay\DoodleRayService.exe").Hash
    service = Get-Service DoodleRayTunnelService -ErrorAction SilentlyContinue | Select-Object Name,Status,StartType
    serviceStatus = Get-ServiceJson
    winInet = Get-WinInetSummary
    doodleProcesses = @(Get-Process DoodleRay,DoodleRayService,xray,sing-box -ErrorAction SilentlyContinue |
        Select-Object ProcessName,Id)
}
`$oldStatePath = Write-Evidence -Name "update-old-installed-$FromVersion.json" -Object `$oldState

`$key = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings"
if ($injectStaleWinInetLiteral) {
    Set-ItemProperty -Path `$key -Name ProxyEnable -Value 1
    Set-ItemProperty -Path `$key -Name ProxyServer -Value "http=127.0.0.1:9;https=127.0.0.1:9"
    Set-ItemProperty -Path `$key -Name ProxyOverride -Value "<local>;*.local"
}
if ($injectCorporatePacLiteral) {
    Set-ItemProperty -Path `$key -Name AutoConfigURL -Value "$corporatePacUrl"
}

Install-Silently "$RemoteRcInstaller"

`$serviceExe = "C:\Program Files\DoodleRay\DoodleRayService.exe"
`$sig = Get-AuthenticodeSignature -LiteralPath `$serviceExe

`$service = Get-Service DoodleRayTunnelService -ErrorAction Stop
if (`$service.Status -ne "Running") {
    Start-Service DoodleRayTunnelService
    Start-Sleep -Seconds 3
}

`$updatedStatus = Get-ServiceJson
if (-not `$updatedStatus -or -not `$updatedStatus.parsed) {
    throw "updated DoodleRayService.exe status did not return parseable JSON"
}
`$updatedVersion = [string] `$updatedStatus.parsed.service_version
if (`$updatedVersion -ne "$ExpectedRcVersion") {
    throw "updated service version is `$updatedVersion, expected $ExpectedRcVersion"
}

`$orphans = Get-StatsQueryOrphanCount
if (`$orphans -gt 0) {
    throw "found xray api statsquery orphan count=`$orphans after update"
}

`$winInetAfter = Get-WinInetSummary
if ($injectCorporatePacLiteral) {
    if ([string] `$winInetAfter.AutoConfigURL -ne "$corporatePacUrl") {
        throw "corporate PAC AutoConfigURL was not preserved across the update"
    }
}

`$after = [pscustomobject]@{
    timestampUtc = (Get-Date).ToUniversalTime().ToString("o")
    fromVersion = "$FromVersion"
    expectedRcVersion = "$ExpectedRcVersion"
    updatedServiceVersion = `$updatedVersion
    updatedServiceState = [string] `$updatedStatus.parsed.state
    updatedServiceSignature = [string] `$sig.Status
    serviceWindowsStatus = Get-Service DoodleRayTunnelService | Select-Object Name,Status,StartType
    xrayStatsQueryCount = `$orphans
    staleWinInetInjected = $injectStaleWinInetLiteral
    corporatePacInjected = $injectCorporatePacLiteral
    corporatePacPreserved = if ($injectCorporatePacLiteral) { ([string] `$winInetAfter.AutoConfigURL -eq "$corporatePacUrl") } else { `$null }
    winInet = `$winInetAfter
    doodleProcesses = @(Get-Process DoodleRay,DoodleRayService,xray,sing-box -ErrorAction SilentlyContinue |
        Select-Object ProcessName,Id)
}
`$afterPath = Write-Evidence -Name "update-after-$FromVersion.json" -Object `$after

# QA-injected state cleanup so the stand is not left dirty by this harness.
if ($injectStaleWinInetLiteral) {
    Set-ItemProperty -Path `$key -Name ProxyEnable -Value 0 -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path `$key -Name ProxyServer -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path `$key -Name ProxyOverride -ErrorAction SilentlyContinue
}
if ($injectCorporatePacLiteral) {
    Remove-ItemProperty -Path `$key -Name AutoConfigURL -ErrorAction SilentlyContinue
}

[pscustomobject]@{
    ok = `$true
    fromVersion = "$FromVersion"
    updatedServiceVersion = `$updatedVersion
    beforeEvidence = `$beforePath
    oldInstalledEvidence = `$oldStatePath
    afterUpdateEvidence = `$afterPath
    evidenceDir = "$EvidenceDir"
} | ConvertTo-Json -Depth 8
"@

if ($FromVersion -eq "5.9.1") {
    if (-not (Test-Path -LiteralPath $SubscriptionSecretPath)) {
        throw "Subscription secret file not found: $SubscriptionSecretPath"
    }
    $helpers = Get-Content (Join-Path $PSScriptRoot "CdpQaHelpers.ps1") -Raw
    $prepareMigrationBody = @"
`$ErrorActionPreference = "Stop"
`$ProgressPreference = "SilentlyContinue"

# Make the migration test independent from sessions left by an earlier QA run.
if (Start-AppWithCdp) {
    `$status = Invoke-QaControl "/status" 5
    if (`$status.frontend -and [bool]`$status.frontend.app_session_logged_in) {
        Invoke-QaControl "/logout" | Out-Null
        `$deadline = (Get-Date).AddSeconds(30)
        do {
            Start-Sleep -Seconds 2
            `$status = Invoke-QaControl "/status" 5
        } while ((Get-Date) -lt `$deadline -and `$status.frontend -and [bool]`$status.frontend.app_session_logged_in)
        if (`$status.frontend -and [bool]`$status.frontend.app_session_logged_in) {
            throw "failed to clear the pre-existing v6 app session"
        }
    }
}

`$oldInstaller = "C:\DoodleRayQA\artifacts\DoodleRay_5.9.1_x64-setup.exe"
if (-not (Test-Path -LiteralPath `$oldInstaller)) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent `$oldInstaller) | Out-Null
    & curl.exe -L --fail --max-time 900 -o `$oldInstaller "https://github.com/Maximus657/doodleray/releases/download/v5.9.1/DoodleRay_5.9.1_x64-setup.exe"
    if (`$LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath `$oldInstaller)) {
        throw "failed to download previous public installer v5.9.1"
    }
}
`$install = Start-Process -FilePath `$oldInstaller -ArgumentList "/S" -Wait -PassThru
if (`$install.ExitCode -ne 0) { throw "v5.9.1 installer exited with code `$(`$install.ExitCode)" }
Start-Sleep -Seconds 6
[pscustomobject]@{ ok = `$true; preparedVersion = "5.9.1" } | ConvertTo-Json
"@
    & (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -Command ($helpers + "`n" + $prepareMigrationBody) -SecretPath $SecretPath
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    & (Join-Path $PSScriptRoot "Import-DoodleRayQaSubscription.ps1") `
        -SecretPath $SecretPath `
        -SubscriptionSecretPath $SubscriptionSecretPath
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

& (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -Command $remoteScript -SecretPath $SecretPath
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

if ($FromVersion -eq "5.9.1") {
    $helpers = Get-Content (Join-Path $PSScriptRoot "CdpQaHelpers.ps1") -Raw
    $verifyMigrationBody = @"
`$ErrorActionPreference = "Stop"
if (-not (Start-AppWithCdp)) { throw "updated v6 app did not expose the QA control surface" }
`$deadline = (Get-Date).AddSeconds(90)
`$status = `$null
do {
    Start-Sleep -Seconds 2
    `$status = Invoke-QaControl "/status" 5
    `$restored = [bool](`$status.frontend -and
        [bool]`$status.frontend.app_session_logged_in -and
        [int]`$status.frontend.servers_count -gt 0)
} while ((Get-Date) -lt `$deadline -and -not `$restored)
if (-not `$restored) {
    throw "v5.9.1 DoodleVPN subscription did not restore a v6 closed-API session"
}
`$result = [pscustomobject]@{
    ok = `$true
    fromVersion = "5.9.1"
    appVersion = [string]`$status.app_version
    appSessionLoggedIn = [bool]`$status.frontend.app_session_logged_in
    appSessionDeviceAllowed = `$status.frontend.app_session_device_allowed
    locationsCount = [int]`$status.frontend.servers_count
    subscriptionsCount = [int]`$status.frontend.subscriptions_count
    codeEntryRequired = `$false
}
`$result | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path "$EvidenceDir" "update-session-migration-5.9.1.json") -Encoding UTF8
`$result | ConvertTo-Json -Depth 6
"@
    & (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -Command ($helpers + "`n" + $verifyMigrationBody) -SecretPath $SecretPath
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

exit 0
