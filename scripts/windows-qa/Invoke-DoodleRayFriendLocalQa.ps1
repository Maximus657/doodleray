param(
    [string] $InstallerPath = "C:\DoodleRayQA\artifacts\DoodleRay_5.9.0_x64-setup.exe",
    [string] $SubscriptionPath = "C:\DoodleRayQA\secrets\doodlevpn-test-subscription-url.txt",
    [string] $EvidenceRoot = "C:\DoodleRayQA\evidence\friend-lan",
    [switch] $SkipInstall
)

$ErrorActionPreference = "Continue"
$ProgressPreference = "SilentlyContinue"

$runId = Get-Date -Format "yyyyMMdd-HHmmss"
$evidence = Join-Path $EvidenceRoot $runId
New-Item -ItemType Directory -Force -Path $evidence | Out-Null

$steps = New-Object System.Collections.Generic.List[object]
function Add-Step {
    param([string] $Name, [bool] $Ok, $Detail = "")
    $steps.Add([pscustomobject]@{
        step = $Name
        ok = $Ok
        detail = $Detail
        at = (Get-Date).ToString("o")
    })
}

function Save-Json {
    param([string] $Name, $Object)
    $Object | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $evidence $Name) -Encoding UTF8
}

function Invoke-QaControl {
    param([string] $Route, [int] $TimeoutSec = 20)
    try {
        return Invoke-RestMethod "http://127.0.0.1:48765$Route" -TimeoutSec $TimeoutSec
    } catch {
        return [pscustomobject]@{ ok = $false; error = $_.Exception.Message }
    }
}

function Wait-QaControl {
    param([int] $TimeoutSec = 60)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $status = Invoke-QaControl "/status" 5
        if ($status -and $status.PSObject.Properties["app_version"]) { return $status }
        Start-Sleep -Seconds 2
    }
    return $null
}

function Wait-FrontendConnected {
    param([int] $TimeoutSec = 180)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $status = Invoke-QaControl "/status" 8
        if ($status -and ([bool]$status.app_connected -or ($status.frontend -and [string]$status.frontend.status -eq "connected"))) {
            return $status
        }
        Start-Sleep -Seconds 3
    }
    return (Invoke-QaControl "/status" 8)
}

function Wait-FrontendDisconnected {
    param([int] $TimeoutSec = 90)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $status = Invoke-QaControl "/status" 8
        $front = if ($status.frontend) { [string]$status.frontend.status } else { "" }
        if ($status -and -not [bool]$status.app_connected -and ($front -eq "" -or $front -eq "disconnected")) {
            return $status
        }
        Start-Sleep -Seconds 3
    }
    return (Invoke-QaControl "/status" 8)
}

function Get-WinInet {
    Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings" -ErrorAction SilentlyContinue |
        Select-Object ProxyEnable, ProxyServer, ProxyOverride, AutoConfigURL, AutoDetect
}

function Get-ServiceJson {
    $serviceExe = "C:\Program Files\DoodleRay\DoodleRayService.exe"
    if (-not (Test-Path -LiteralPath $serviceExe)) { return $null }
    $raw = (& $serviceExe status 2>&1 | Out-String).Trim()
    try { return $raw | ConvertFrom-Json } catch { return [pscustomobject]@{ raw = $raw } }
}

function Get-CleanupSnapshot {
    [pscustomobject]@{
        service = Get-ServiceJson
        winInet = Get-WinInet
        engines = @(Get-Process xray, sing-box -ErrorAction SilentlyContinue | Select-Object ProcessName, Id, Path)
        statsquery = @(Get-CimInstance Win32_Process -Filter "Name = 'xray.exe'" -ErrorAction SilentlyContinue |
            Where-Object { $_.CommandLine -match "statsquery" } |
            Select-Object ProcessId, CommandLine)
        adapters = @(Get-NetAdapter -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like "*DoodleRay*" -or $_.InterfaceDescription -like "*Wintun*" } |
            Select-Object Name, Status, InterfaceDescription)
        nrpt = @(Get-DnsClientNrptRule -ErrorAction SilentlyContinue |
            Where-Object { ($_.Comment -match "DoodleRay") -or ($_.Namespace -match "DoodleRay") } |
            Select-Object Namespace, Comment)
        marker = (Test-Path "C:\ProgramData\DoodleRay\runtime\active-session.marker")
    }
}

$transcript = Join-Path $evidence "transcript.log"
Start-Transcript -LiteralPath $transcript -Force | Out-Null

try {
    $isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    Add-Step "admin_context" $isAdmin "user=$(whoami)"
    if (-not $isAdmin) { throw "Run this script from an elevated PowerShell window." }

    Save-Json "baseline-conflicts.json" (& (Join-Path $PSScriptRoot "Get-DoodleRayLanConflictInventory.ps1") | ConvertFrom-Json)

    if (-not $SkipInstall) {
        if (-not (Test-Path -LiteralPath $InstallerPath)) { throw "Installer not found: $InstallerPath" }
        $install = Start-Process -FilePath $InstallerPath -ArgumentList "/S" -Wait -PassThru
        Add-Step "silent_install" ($install.ExitCode -eq 0) "exit=$($install.ExitCode)"
        Start-Sleep -Seconds 8
    } else {
        Add-Step "silent_install" $true "skipped"
    }

    $appExe = "C:\Program Files\DoodleRay\DoodleRay.exe"
    Add-Step "app_exe_present" (Test-Path -LiteralPath $appExe) $appExe
    if (-not (Test-Path -LiteralPath $appExe)) { throw "DoodleRay.exe missing after install." }

    Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 3

    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9333 --remote-allow-origins=*"
    $env:DOODLERAY_QA_CONTROL = "1"
    Start-Process -FilePath $appExe -WorkingDirectory (Split-Path -Parent $appExe)
    $status = Wait-QaControl 90
    Add-Step "qa_control_ready" ([bool]$status) $(if ($status) { "app=$($status.app_version)" } else { "timeout" })
    if (-not $status) { throw "QA control did not become ready." }

    if (Test-Path -LiteralPath $SubscriptionPath) {
        $subUrl = (Get-Content -LiteralPath $SubscriptionPath -Raw).Trim()
        $encoded = [uri]::EscapeDataString($subUrl)
        Invoke-QaControl "/import-subscription?url=$encoded" 30 | Out-Null
        $imported = $false
        for ($i = 0; $i -lt 30; $i++) {
            $status = Invoke-QaControl "/status" 8
            if ($status.frontend -and [int]$status.frontend.subscriptions_count -gt 0 -and [int]$status.frontend.servers_count -gt 0) {
                $imported = $true
                break
            }
            Start-Sleep -Seconds 2
        }
        Add-Step "subscription_import" $imported $(if ($status.frontend) { "subs=$($status.frontend.subscriptions_count) servers=$($status.frontend.servers_count)" } else { "" })
    } else {
        Add-Step "subscription_import" $false "missing $SubscriptionPath"
    }

    # Whole computer mode.
    Invoke-QaControl "/switch-mode?mode=tun" 15 | Out-Null
    Start-Sleep -Seconds 2
    Invoke-QaControl "/connect" 15 | Out-Null
    $tunStatus = Wait-FrontendConnected 210
    Save-Json "tun-status.json" $tunStatus
    $tunOk = [bool]$tunStatus.app_connected -or ($tunStatus.frontend -and [string]$tunStatus.frontend.status -eq "connected")
    Add-Step "tun_connect" $tunOk $(if ($tunStatus.service) { "service=$($tunStatus.service.state) verdict=$($tunStatus.service.health_verdict)" } else { "" })
    & (Join-Path $PSScriptRoot "Get-DoodleRayDeepQaSnapshot.ps1") -Mode tun |
        Set-Content -LiteralPath (Join-Path $evidence "tun-deep-snapshot.json") -Encoding UTF8

    Invoke-QaControl "/disconnect" 15 | Out-Null
    Wait-FrontendDisconnected 90 | Out-Null
    Start-Sleep -Seconds 5

    # Browser compatibility mode.
    Invoke-QaControl "/switch-mode?mode=browsers" 15 | Out-Null
    Start-Sleep -Seconds 2
    Invoke-QaControl "/connect" 15 | Out-Null
    $proxyStatus = Wait-FrontendConnected 120
    Save-Json "browsers-status.json" $proxyStatus
    $proxyOk = [bool]$proxyStatus.app_connected -or ($proxyStatus.frontend -and [string]$proxyStatus.frontend.status -eq "connected")
    Add-Step "browsers_connect" $proxyOk $(if ($proxyStatus.frontend) { "mode=$($proxyStatus.frontend.product_mode) status=$($proxyStatus.frontend.status)" } else { "" })
    & (Join-Path $PSScriptRoot "Get-DoodleRayDeepQaSnapshot.ps1") -Mode proxy |
        Set-Content -LiteralPath (Join-Path $evidence "browsers-deep-snapshot.json") -Encoding UTF8

    Invoke-QaControl "/disconnect" 15 | Out-Null
    Wait-FrontendDisconnected 90 | Out-Null
    Start-Sleep -Seconds 5

    # Manual mode should not mutate WinINet by itself.
    $beforeManual = Get-WinInet
    Invoke-QaControl "/switch-mode?mode=manual" 15 | Out-Null
    Start-Sleep -Seconds 3
    $afterManual = Get-WinInet
    $manualOk = ($beforeManual.ProxyEnable -eq $afterManual.ProxyEnable -and [string]$beforeManual.ProxyServer -eq [string]$afterManual.ProxyServer)
    Add-Step "manual_does_not_mutate_wininet" $manualOk "before=$($beforeManual.ProxyEnable)/$($beforeManual.ProxyServer) after=$($afterManual.ProxyEnable)/$($afterManual.ProxyServer)"

    $bundle = Invoke-QaControl "/export-bundle" 60
    Save-Json "support-bundle-result.json" $bundle
    Add-Step "support_bundle_export" ([bool]$bundle.ok -or [bool]$bundle.path) $(($bundle | ConvertTo-Json -Depth 3 -Compress))

    Save-Json "final-cleanup-before.json" (Get-CleanupSnapshot)
    Invoke-QaControl "/disconnect" 15 | Out-Null
    Wait-FrontendDisconnected 90 | Out-Null
    Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 5
    Save-Json "final-cleanup-after.json" (Get-CleanupSnapshot)
    Save-Json "post-conflicts.json" (& (Join-Path $PSScriptRoot "Get-DoodleRayLanConflictInventory.ps1") | ConvertFrom-Json)
} catch {
    Add-Step "fatal_exception" $false $_.Exception.Message
} finally {
    $ok = @($steps | Where-Object { -not $_.ok }).Count -eq 0
    Save-Json "summary.json" ([pscustomobject]@{
        ok = $ok
        evidence = $evidence
        steps = $steps
    })
    Stop-Transcript | Out-Null
}

Get-Content -LiteralPath (Join-Path $evidence "summary.json") -Raw
if (@($steps | Where-Object { -not $_.ok }).Count -gt 0) { exit 1 }
