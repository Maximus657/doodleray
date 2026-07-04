param(
    [string] $SubscriptionPath = "C:\DoodleRayQA\secrets\doodlevpn-test-subscription-url.txt",
    [string] $EvidenceRoot = "C:\DoodleRayQA\evidence\friend-auto-fallback-local"
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

function Get-HttpProxyPortFromWinInet {
    param($WinInet)
    $server = [string]$WinInet.ProxyServer
    if (-not $server) { return $null }
    foreach ($part in ($server -split ';')) {
        $value = $part.Trim()
        if (-not $value) { continue }
        if ($value -match '^(?:http=)?127\.0\.0\.1:(\d+)$') { return [int]$Matches[1] }
        if ($value -match '^http://127\.0\.0\.1:(\d+)$') { return [int]$Matches[1] }
    }
    return $null
}

function Test-HttpProxyFetch {
    param([int] $Port)
    try {
        $out = & curl.exe --silent --show-error --ssl-no-revoke `
            --proxy "http://127.0.0.1:$Port" `
            --max-time 20 `
            --write-out " HTTP_CODE=%{http_code}" `
            "https://captive.apple.com/hotspot-detect.html" 2>&1
        return [pscustomobject]@{
            ok = ($LASTEXITCODE -eq 0 -and ([string]$out) -match 'HTTP_CODE=200')
            output = ([string]$out)
            exit = $LASTEXITCODE
        }
    } catch {
        return [pscustomobject]@{ ok = $false; output = $_.Exception.Message; exit = -1 }
    }
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
    if (-not $subOk) { throw "No subscription/profiles available for auto-fallback test." }

    Invoke-QaControl "/switch-mode?mode=tun" 15 | Out-Null
    Start-Sleep -Seconds 2
    Invoke-QaControl "/connect" 15 | Out-Null
    Add-Step "start_protected_connect" $true "qa-control"

    $killed = New-Object System.Collections.Generic.List[object]
    $seenPids = @{}
    $deadline = (Get-Date).AddSeconds(130)
    while ((Get-Date) -lt $deadline -and $killed.Count -lt 2) {
        $svc = Get-ServiceJson
        if ($svc -and ([string]$svc.state) -eq "connected" -and $killed.Count -eq 0) {
            # Healthy TUN came up before we could inject failure. This is not a
            # fallback proof, but it is a useful datapoint on problematic PCs.
            break
        }
        if ($svc -and ([string]$svc.state) -eq "connecting" -and $svc.singbox_pid) {
            $targetPid = [int]$svc.singbox_pid
            if (-not $seenPids.ContainsKey($targetPid)) {
                $seenPids[$targetPid] = $true
                try {
                    Stop-Process -Id $targetPid -Force -ErrorAction Stop
                    $killed.Add([pscustomobject]@{
                        pid = $targetPid
                        phase = [string]$svc.phase
                        generation = $svc.service_generation
                        at = (Get-Date).ToString("o")
                    }) | Out-Null
                } catch {
                    $killed.Add([pscustomobject]@{
                        pid = $targetPid
                        phase = [string]$svc.phase
                        generation = $svc.service_generation
                        error = $_.Exception.Message
                        at = (Get-Date).ToString("o")
                    }) | Out-Null
                }
            }
        }
        Start-Sleep -Milliseconds 250
    }
    Add-Step "service_singbox_kill_attempts_recorded" ($killed.Count -gt 0) "count=$($killed.Count); detail=$(($killed | ConvertTo-Json -Depth 5 -Compress))"

    $outcome = "timeout"
    $lastStatus = $null
    $lastWinInet = $null
    $lastFetch = $null
    $lastPort = $null
    $deadline = (Get-Date).AddSeconds(240)
    while ((Get-Date) -lt $deadline) {
        $lastStatus = Invoke-QaControl "/status" 8
        $lastWinInet = Get-WinInet
        $lastPort = Get-HttpProxyPortFromWinInet $lastWinInet
        $svcState = if ($lastStatus.service) { [string]$lastStatus.service.state } else { "" }
        $svcVerdict = if ($lastStatus.service) { [string]$lastStatus.service.health_verdict } else { "" }
        $frontendMode = if ($lastStatus.frontend) { [string]$lastStatus.frontend.product_mode } else { "" }
        $frontendState = if ($lastStatus.frontend) { [string]$lastStatus.frontend.status } else { "" }
        $appConnected = [bool]$lastStatus.app_connected

        if ($appConnected -and $svcState -eq "connected" -and $svcVerdict -match '^protected') {
            $outcome = "protected_repaired"
            break
        }
        if ($appConnected -and $lastWinInet.ProxyEnable -eq 1 -and $lastPort -and $frontendMode -match 'compat|browser') {
            $portReady = (Test-NetConnection 127.0.0.1 -Port $lastPort -WarningAction SilentlyContinue).TcpTestSucceeded
            if ($portReady) {
                $lastFetch = Test-HttpProxyFetch $lastPort
                if ($lastFetch.ok) {
                    $outcome = "fallback_browsers"
                    break
                }
            }
        }
        if ($frontendState -eq "failed" -or $svcState -eq "failed") {
            $outcome = "failed"
            break
        }
        Start-Sleep -Seconds 3
    }

    $detail = [pscustomobject]@{
        outcome = $outcome
        status = $lastStatus
        winInet = $lastWinInet
        httpPort = $lastPort
        fetch = $lastFetch
    }
    Save-Json "auto-fallback-status.json" $detail
    Add-Step "protected_repair_or_limited_fallback" ($outcome -in @("protected_repaired", "fallback_browsers")) ($detail | ConvertTo-Json -Depth 6 -Compress)

    $adapterPresent = [bool](Get-NetAdapter -Name "DoodleRay Tunnel" -ErrorAction SilentlyContinue)
    Add-Step "adapter_state_recorded" $true "adapterPresent=$adapterPresent outcome=$outcome"

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
    Save-Json "auto-fallback-summary.json" $result
    $result | ConvertTo-Json -Depth 10
    if (-not $result.ok) { exit 1 }
}
