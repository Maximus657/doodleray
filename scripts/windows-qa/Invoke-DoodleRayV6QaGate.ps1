param(
    [string] $RemoteInstallerPath = "C:\DoodleRayQA\artifacts\DoodleRay-v6-rc-setup.exe",
    [string] $EvidenceDir = "C:\DoodleRayQA\evidence",
    [switch] $InjectStaleWinInet,
    [switch] $UninstallAfter,
    [string] $SecretPath = (Join-Path $PSScriptRoot "..\..\secrets\doodlevpn-server-access.md")
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$injectStaleWinInetLiteral = if ($InjectStaleWinInet.IsPresent) { '$true' } else { '$false' }
$uninstallAfterLiteral = if ($UninstallAfter.IsPresent) { '$true' } else { '$false' }

$remoteScript = @"
`$ErrorActionPreference = "Stop"
`$ProgressPreference = "SilentlyContinue"

function Assert-FilePresent {
    param([Parameter(Mandatory = `$true)][string] `$Path)
    if (-not (Test-Path -LiteralPath `$Path)) {
        throw "missing file: `$Path"
    }
}

function Get-ServiceJson {
    `$serviceExe = "C:\Program Files\DoodleRay\DoodleRayService.exe"
    if (-not (Test-Path -LiteralPath `$serviceExe)) {
        return `$null
    }
    `$raw = (& `$serviceExe status 2>&1 | Out-String).Trim()
    try {
        return `$raw | ConvertFrom-Json
    } catch {
        throw "service status is not JSON: `$raw"
    }
}

function Assert-NoStatsQueryOrphan {
    `$stats = @(Get-CimInstance Win32_Process -Filter "name = 'xray.exe'" -ErrorAction SilentlyContinue |
        Where-Object { `$_.CommandLine -match "api\s+statsquery" })
    if (`$stats.Count -gt 0) {
        throw "found xray api statsquery orphan count=`$(`$stats.Count)"
    }
}

function Write-Evidence {
    param([Parameter(Mandatory = `$true)][string] `$Name, [Parameter(Mandatory = `$true)] `$Object)
    New-Item -ItemType Directory -Force -Path "$EvidenceDir" | Out-Null
    `$path = Join-Path "$EvidenceDir" `$Name
    `$Object | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath `$path -Encoding UTF8
    return `$path
}

if (-not (Test-Path -LiteralPath "$RemoteInstallerPath")) {
    throw "remote installer not found: $RemoteInstallerPath"
}

if ($injectStaleWinInetLiteral) {
    `$key = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings"
    Set-ItemProperty -Path `$key -Name ProxyEnable -Value 1
    Set-ItemProperty -Path `$key -Name ProxyServer -Value "http=127.0.0.1:9;https=127.0.0.1:9"
    Set-ItemProperty -Path `$key -Name ProxyOverride -Value "<local>;*.local"
}

`$before = [pscustomobject]@{
    timestampUtc = (Get-Date).ToUniversalTime().ToString("o")
    installer = "$RemoteInstallerPath"
    installerSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath "$RemoteInstallerPath").Hash
    existingService = Get-Service DoodleRayTunnelService -ErrorAction SilentlyContinue | Select-Object Name,Status,StartType
    xrayStatsQueryCount = @(
        Get-CimInstance Win32_Process -Filter "name = 'xray.exe'" -ErrorAction SilentlyContinue |
        Where-Object { `$_.CommandLine -match "api\s+statsquery" }
    ).Count
}
`$beforePath = Write-Evidence -Name "v6-before.json" -Object `$before

`$installerArgs = "/S"
`$process = Start-Process -FilePath "$RemoteInstallerPath" -ArgumentList `$installerArgs -Wait -PassThru
if (`$process.ExitCode -ne 0) {
    throw "installer exited with code `$(`$process.ExitCode)"
}
Start-Sleep -Seconds 5

`$appFiles = @(
    "C:\Program Files\DoodleRay\DoodleRay.exe",
    "C:\Program Files\DoodleRay\DoodleRayService.exe",
    "C:\Program Files\DoodleRay\sing-box.exe",
    "C:\Program Files\DoodleRay\xray-core\xray.exe"
)
foreach (`$file in `$appFiles) {
    Assert-FilePresent `$file
}

`$service = Get-Service DoodleRayTunnelService -ErrorAction Stop
if (`$service.StartType -ne "Manual") {
    throw "tunnel service must be demand-started, got StartType=`$(`$service.StartType)"
}
if (`$service.Status -ne "Stopped") {
    throw "tunnel service must be idle after install, got Status=`$(`$service.Status)"
}
`$idleAfterInstall = `$service | Select-Object Name,Status,StartType
Start-Service DoodleRayTunnelService
Start-Sleep -Seconds 3

`$status = Get-ServiceJson
if (-not `$status) {
    throw "DoodleRayService.exe status did not return a snapshot"
}
Assert-NoStatsQueryOrphan
Stop-Service DoodleRayTunnelService
`$service.WaitForStatus("Stopped", [TimeSpan]::FromSeconds(10))
if (Get-Process DoodleRayService -ErrorAction SilentlyContinue) {
    throw "tunnel service process remained after clean idle stop"
}

`$after = [pscustomobject]@{
    timestampUtc = (Get-Date).ToUniversalTime().ToString("o")
    installedFiles = `$appFiles | ForEach-Object {
        [pscustomobject]@{
            path = `$_.Replace("C:\Program Files\DoodleRay", "%ProgramFiles%\DoodleRay")
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath `$_).Hash
            signature = [string](Get-AuthenticodeSignature -LiteralPath `$_).Status
        }
    }
    serviceStatus = `$status
    serviceIdleAfterInstall = `$idleAfterInstall
    serviceIdleAfterSmoke = Get-Service DoodleRayTunnelService | Select-Object Name,Status,StartType
    xrayStatsQueryCount = @(
        Get-CimInstance Win32_Process -Filter "name = 'xray.exe'" -ErrorAction SilentlyContinue |
        Where-Object { `$_.CommandLine -match "api\s+statsquery" }
    ).Count
    winInet = Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings" |
        Select-Object ProxyEnable,ProxyServer,ProxyOverride,AutoDetect,AutoConfigURL
}
`$afterPath = Write-Evidence -Name "v6-after-install.json" -Object `$after

`$postSmokeCleanupPath = `$null
if ($injectStaleWinInetLiteral) {
    `$key = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings"
    Set-ItemProperty -Path `$key -Name ProxyEnable -Value 0 -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path `$key -Name ProxyServer -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path `$key -Name ProxyOverride -ErrorAction SilentlyContinue
    `$postSmokeCleanup = [pscustomobject]@{
        timestampUtc = (Get-Date).ToUniversalTime().ToString("o")
        reason = "post-smoke cleanup for injected stale WinINet"
        winInet = Get-ItemProperty `$key | Select-Object ProxyEnable,ProxyServer,ProxyOverride,AutoDetect,AutoConfigURL
    }
    `$postSmokeCleanupPath = Write-Evidence -Name "v6-post-smoke-cleanup.json" -Object `$postSmokeCleanup
}

if ($uninstallAfterLiteral) {
    `$uninstallers = @(Get-ChildItem "C:\Program Files\DoodleRay" -Filter "uninstall*.exe" -ErrorAction SilentlyContinue)
    if (`$uninstallers.Count -gt 0) {
        `$un = `$uninstallers | Select-Object -First 1
        `$unProc = Start-Process -FilePath `$un.FullName -ArgumentList "/S" -Wait -PassThru
        if (`$unProc.ExitCode -ne 0) {
            throw "uninstaller exited with code `$(`$unProc.ExitCode)"
        }
        Start-Sleep -Seconds 3
    }
    `$cleanup = [pscustomobject]@{
        timestampUtc = (Get-Date).ToUniversalTime().ToString("o")
        servicePresent = [bool](Get-Service DoodleRayTunnelService -ErrorAction SilentlyContinue)
        winInet = Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings" |
            Select-Object ProxyEnable,ProxyServer,ProxyOverride,AutoDetect,AutoConfigURL
        doodleProcesses = @(Get-Process DoodleRay,DoodleRayService,xray,sing-box -ErrorAction SilentlyContinue |
            Select-Object ProcessName,Id,Path)
    }
    `$cleanupPath = Write-Evidence -Name "v6-after-uninstall.json" -Object `$cleanup
}

[pscustomobject]@{
    ok = `$true
    beforeEvidence = `$beforePath
    afterInstallEvidence = `$afterPath
    postSmokeCleanupEvidence = `$postSmokeCleanupPath
    uninstallRequested = $uninstallAfterLiteral
    evidenceDir = "$EvidenceDir"
} | ConvertTo-Json -Depth 8
"@

& (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -Command $remoteScript -SecretPath $SecretPath
exit $LASTEXITCODE
