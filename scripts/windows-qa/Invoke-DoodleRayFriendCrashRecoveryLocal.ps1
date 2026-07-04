param(
    [string] $SubscriptionPath = "C:\DoodleRayQA\secrets\doodlevpn-test-subscription-url.txt",
    [string] $EvidenceRoot = "C:\DoodleRayQA\evidence\friend-crash-recovery"
)

$ErrorActionPreference = "Continue"
$ProgressPreference = "SilentlyContinue"

$runId = Get-Date -Format "yyyyMMdd-HHmmss"
$evidence = Join-Path $EvidenceRoot $runId
New-Item -ItemType Directory -Force -Path $evidence | Out-Null

$steps = New-Object System.Collections.Generic.List[object]
function Save-Json {
    param([string] $Name, $Object)
    $Object | ConvertTo-Json -Depth 14 | Set-Content -LiteralPath (Join-Path $evidence $Name) -Encoding UTF8
}

function Add-Step {
    param([string] $Name, [bool] $Ok, $Detail = "")
    $steps.Add([pscustomobject]@{
        step = $Name
        ok = $Ok
        detail = $Detail
        at = (Get-Date).ToString("o")
    })
    Save-Json "progress.json" $steps
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
    param([int] $TimeoutSec = 90)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $status = Invoke-QaControl "/status" 5
        if ($status -and $status.PSObject.Properties["app_version"]) { return $status }
        Start-Sleep -Seconds 2
    }
    return $null
}

function Get-ServiceJson {
    $serviceExe = "C:\Program Files\DoodleRay\DoodleRayService.exe"
    if (-not (Test-Path -LiteralPath $serviceExe)) { return $null }
    $raw = (& $serviceExe status 2>&1 | Out-String).Trim()
    try { return $raw | ConvertFrom-Json } catch { return [pscustomobject]@{ raw = $raw } }
}

function Get-WinInet {
    Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings" -ErrorAction SilentlyContinue |
        Select-Object ProxyEnable, ProxyServer, ProxyOverride, AutoConfigURL, AutoDetect
}

function Get-StatsQueryOrphanCount {
    @(Get-CimInstance Win32_Process -Filter "Name = 'xray.exe'" -ErrorAction SilentlyContinue |
        Where-Object { $_.CommandLine -match "api\s+statsquery" }).Count
}

function Start-DoodleRayQaApp {
    $appExe = "C:\Program Files\DoodleRay\DoodleRay.exe"
    if (-not (Test-Path -LiteralPath $appExe)) { throw "DoodleRay.exe missing: $appExe" }
    Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 3
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9333 --remote-allow-origins=*"
    $env:DOODLERAY_QA_CONTROL = "1"
    Start-Process -FilePath $appExe -WorkingDirectory (Split-Path -Parent $appExe)
    return (Wait-QaControl 90)
}

function Ensure-Subscription {
    $status = Invoke-QaControl "/status" 8
    if ($status.frontend -and [int]$status.frontend.subscriptions_count -gt 0 -and [int]$status.frontend.servers_count -gt 0) {
        return $status
    }
    if (-not (Test-Path -LiteralPath $SubscriptionPath)) { return $status }
    $subUrl = (Get-Content -LiteralPath $SubscriptionPath -Raw).Trim()
    $encoded = [uri]::EscapeDataString($subUrl)
    Invoke-QaControl "/import-subscription?url=$encoded" 30 | Out-Null
    $deadline = (Get-Date).AddSeconds(90)
    while ((Get-Date) -lt $deadline) {
        $status = Invoke-QaControl "/status" 8
        if ($status.frontend -and [int]$status.frontend.subscriptions_count -gt 0 -and [int]$status.frontend.servers_count -gt 0) {
            return $status
        }
        Start-Sleep -Seconds 3
    }
    return $status
}

function Wait-ConnectedOrTerminal {
    param([int] $TimeoutSec = 220)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $status = Invoke-QaControl "/status" 8
        $frontState = if ($status.frontend) { [string]$status.frontend.status } else { "" }
        $svcState = if ($status.service) { [string]$status.service.state } else { "" }
        if ([bool]$status.app_connected -or $frontState -eq "connected" -or $frontState -eq "failed" -or $svcState -eq "failed") {
            return $status
        }
        Start-Sleep -Seconds 3
    }
    return (Invoke-QaControl "/status" 8)
}

function Stop-QaTunnelHard {
    param([int] $TimeoutSec = 90)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $svc = Get-ServiceJson
        $qa = Invoke-QaControl "/status" 5
        $winInet = Get-WinInet
        $appConnected = [bool]($qa -and $qa.app_connected)
        $frontendState = if ($qa.frontend) { [string]$qa.frontend.status } else { "" }
        $loopbackProxy = [bool]($winInet.ProxyEnable -eq 1 -and ([string]$winInet.ProxyServer) -match '127\.0\.0\.1:')
        $serviceInactive = (-not $svc -or ([string]$svc.state) -in @("disconnected", "failed"))
        if ($serviceInactive -and -not $appConnected -and ($frontendState -eq "" -or $frontendState -eq "disconnected") -and -not $loopbackProxy) {
            return "disconnected"
        }
        Invoke-QaControl "/disconnect" 8 | Out-Null
        Start-Sleep -Seconds 5
    }
    Restart-Service DoodleRayTunnelService -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 8
    return "service-restart"
}

$transcript = Join-Path $evidence "transcript.log"
Start-Transcript -LiteralPath $transcript -Force | Out-Null

try {
    $isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    Add-Step "admin_context" $isAdmin "user=$(whoami)"
    if (-not $isAdmin) { throw "Admin context required." }

    $status = Start-DoodleRayQaApp
    Add-Step "qa_control_ready" ([bool]$status) $(if ($status) { "app=$($status.app_version)" } else { "timeout" })
    if (-not $status) { throw "QA control did not become ready." }

    Stop-QaTunnelHard 90 | Out-Null
    $status = Ensure-Subscription
    $subOk = [bool]($status.frontend -and [int]$status.frontend.subscriptions_count -gt 0 -and [int]$status.frontend.servers_count -gt 0)
    Add-Step "subscription_ready" $subOk $(if ($status.frontend) { "subs=$($status.frontend.subscriptions_count) servers=$($status.frontend.servers_count)" } else { "no frontend status" })
    if (-not $subOk) { throw "No subscription/profiles available." }

    Invoke-QaControl "/switch-mode?mode=tun" 15 | Out-Null
    Start-Sleep -Seconds 2
    Invoke-QaControl "/connect" 15 | Out-Null
    $connected = Wait-ConnectedOrTerminal 220
    Save-Json "connected-before-ui-kill.json" $connected
    $connectedOk = [bool]$connected.app_connected -or ($connected.frontend -and [string]$connected.frontend.status -eq "connected")
    $initialMode = if ($connected.frontend) { [string]$connected.frontend.product_mode } else { "" }
    $initialServiceState = if ($connected.service) { [string]$connected.service.state } else { "" }
    $initialUsesServiceTunnel = $initialServiceState -eq "connected" -and $initialMode -eq "protected"
    Add-Step "initial_tun_or_fallback_connected" $connectedOk $(if ($connected.frontend) { "mode=$($connected.frontend.product_mode) state=$($connected.frontend.status) svc=$($connected.service.state) verdict=$($connected.service.health_verdict)" } else { "" })
    if (-not $connectedOk) { throw "Could not connect before crash recovery test." }

    Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 5
    $svcAfterUiKill = Get-ServiceJson
    Save-Json "service-after-ui-kill.json" $svcAfterUiKill
    $svcAlive = $svcAfterUiKill -and ([string]$svcAfterUiKill.state) -eq "connected"
    if ($initialUsesServiceTunnel) {
        Add-Step "ui_kill_service_truth_survives" $svcAlive "service=$($svcAfterUiKill.state) verdict=$($svcAfterUiKill.health_verdict)"
    } else {
        Add-Step "ui_kill_service_truth_not_required_for_fallback" (-not $svcAlive) "initialMode=$initialMode service=$($svcAfterUiKill.state) verdict=$($svcAfterUiKill.health_verdict)"
    }

    $status = Start-DoodleRayQaApp
    $reattach = Invoke-QaControl "/status" 8
    Save-Json "reattach-status.json" $reattach
    $reattachOk = [bool]$reattach.app_connected -or ($reattach.frontend -and [string]$reattach.frontend.status -eq "connected")
    if ($initialUsesServiceTunnel) {
        Add-Step "ui_reattach_reflects_runtime" $reattachOk $(if ($reattach.frontend) { "mode=$($reattach.frontend.product_mode) state=$($reattach.frontend.status)" } else { "" })
    } else {
        Add-Step "ui_reattach_does_not_fake_fallback_connection" (-not $reattachOk) $(if ($reattach.frontend) { "mode=$($reattach.frontend.product_mode) state=$($reattach.frontend.status)" } else { "" })
    }

    $targetPid = $null
    if ($reattach.service -and $reattach.service.singbox_pid) { $targetPid = [int]$reattach.service.singbox_pid }
    if (-not $targetPid) {
        $svc = Get-ServiceJson
        if ($svc -and $svc.singbox_pid) { $targetPid = [int]$svc.singbox_pid }
    }
    Add-Step "connected_core_pid_found_or_not_applicable" ([bool]$targetPid -or -not $initialUsesServiceTunnel) "pid=$targetPid initialMode=$initialMode"

    if ($targetPid) {
        Stop-Process -Id $targetPid -Force -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 12
        $afterCoreKill = Invoke-QaControl "/status" 8
        Save-Json "after-core-kill-status.json" $afterCoreKill
        $frontState = if ($afterCoreKill.frontend) { [string]$afterCoreKill.frontend.status } else { "" }
        $svcState = if ($afterCoreKill.service) { [string]$afterCoreKill.service.state } else { "" }
        $svcPid = if ($afterCoreKill.service) { $afterCoreKill.service.singbox_pid } else { $null }
        $fakeGreen = ([bool]$afterCoreKill.app_connected -or $frontState -eq "connected") -and
            ($svcState -ne "connected" -or -not $svcPid)
        Add-Step "core_kill_no_fake_green" (-not $fakeGreen) "front=$frontState app=$($afterCoreKill.app_connected) service=$svcState pid=$svcPid"
    } else {
        Add-Step "core_kill_no_fake_green" $true "not-applicable initialMode=$initialMode"
    }

    $teardown = Stop-QaTunnelHard 120
    Add-Step "teardown_disconnect" ($teardown -ne "still-connected") "via=$teardown"
    Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 5

    $svcEnd = Get-ServiceJson
    $wiEnd = Get-WinInet
    $engines = @(Get-Process xray, sing-box -ErrorAction SilentlyContinue).Count
    $marker = Test-Path "C:\ProgramData\DoodleRay\runtime\active-session.marker"
    $adapterEnd = [bool](Get-NetAdapter -Name "DoodleRay Tunnel" -ErrorAction SilentlyContinue)
    $cleanOk = ([string]$svcEnd.state) -eq "disconnected" -and $wiEnd.ProxyEnable -eq 0 -and
        $engines -eq 0 -and (Get-StatsQueryOrphanCount) -eq 0 -and (-not $marker) -and (-not $adapterEnd)
    Add-Step "final_cleanup_clean" $cleanOk "service=$($svcEnd.state) winInet=$($wiEnd.ProxyEnable) engines=$engines marker=$marker adapter=$adapterEnd"
} catch {
    Add-Step "script_exception" $false $_.Exception.Message
} finally {
    try { Stop-Transcript | Out-Null } catch {}
    $result = [pscustomobject]@{
        ok = (@($steps | Where-Object { -not $_.ok }).Count -eq 0)
        evidence = $evidence
        steps = $steps
    }
    Save-Json "crash-recovery-summary.json" $result
    $result | ConvertTo-Json -Depth 10
    if (-not $result.ok) { exit 1 }
}
