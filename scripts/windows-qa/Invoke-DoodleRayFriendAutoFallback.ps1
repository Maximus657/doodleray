param(
    [string] $EvidenceRoot = "C:\DoodleRayQA\evidence\friend-auto-fallback"
)

$ErrorActionPreference = "Continue"
$ProgressPreference = "SilentlyContinue"

. (Join-Path $PSScriptRoot "CdpQaHelpers.ps1")

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

$runId = Get-Date -Format "yyyyMMdd-HHmmss"
$evidenceDir = Join-Path $EvidenceRoot $runId
New-Item -ItemType Directory -Force -Path $evidenceDir | Out-Null

Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3
$launched = Start-AppWithCdp
Add-Step "launch_app_control" $launched "qaControl=$(Test-QaControlAvailable)"

Stop-QaTunnelHard 60 | Out-Null
Start-Sleep -Seconds 3
Switch-Mode 0 "protected" | Out-Null
Start-Sleep -Seconds 2

$serviceLogPath = "C:\ProgramData\DoodleRay\service.log"
$serviceLogStartLine = 0
if (Test-Path -LiteralPath $serviceLogPath) {
    $serviceLogStartLine = @((Get-Content -LiteralPath $serviceLogPath -ErrorAction SilentlyContinue)).Count
}

$connectChannel = Start-QaConnect
Add-Step "start_protected_connect" $true "channel=$connectChannel"

$killed = New-Object System.Collections.Generic.List[object]
$seenPids = @{}
$deadline = (Get-Date).AddSeconds(110)
while ((Get-Date) -lt $deadline -and $killed.Count -lt 2) {
    $svc = Get-ServiceStatus
    if ($svc -and ([string]$svc.state) -eq "connected") { break }
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
Add-Step "service_singbox_kill_attempts_recorded" ($killed.Count -gt 0) "count=$($killed.Count); detail=$(($killed | ConvertTo-Json -Depth 4 -Compress))"

$fallback = $false
$fallbackStatus = $null
$fallbackWinInet = $null
$fallbackPort = $null
$fallbackFetch = $null
$deadline = (Get-Date).AddSeconds(210)
while ((Get-Date) -lt $deadline -and -not $fallback) {
    $fallbackStatus = Invoke-QaControl "/status" 5
    $fallbackWinInet = Get-WinInet
    $fallbackPort = Get-HttpProxyPortFromWinInet $fallbackWinInet
    $serviceState = if ($fallbackStatus.service) { [string]$fallbackStatus.service.state } else { "" }
    $serviceVerdict = if ($fallbackStatus.service) { [string]$fallbackStatus.service.health_verdict } else { "" }
    $appConnected = [bool]$fallbackStatus.app_connected
    if ($appConnected -and $fallbackWinInet.ProxyEnable -eq 1 -and $fallbackPort -and
        -not ($serviceState -eq "connected" -and $serviceVerdict -match '^protected')) {
        $portReady = (Test-NetConnection 127.0.0.1 -Port $fallbackPort -WarningAction SilentlyContinue).TcpTestSucceeded
        if ($portReady) {
            $fallbackFetch = Test-HttpProxyFetch $fallbackPort
            if ($fallbackFetch.ok) { $fallback = $true }
        }
    }
    Start-Sleep -Seconds 3
}

$newServiceLines = @()
if (Test-Path -LiteralPath $serviceLogPath) {
    $newServiceLines = @(Get-Content -LiteralPath $serviceLogPath -ErrorAction SilentlyContinue | Select-Object -Skip $serviceLogStartLine)
}
$protectedAttemptObserved = [bool](@($newServiceLines | Where-Object { $_ -match 'StartTunnel accepted|start_tunnel generation' }).Count)
$serviceLogSnippet = (($newServiceLines | Where-Object { $_ -match 'StartTunnel accepted|start_tunnel generation|tun bring-up attempt|failed_cleanup|StopTunnel requested' } | Select-Object -First 14) -join ' | ')
Add-Step "protected_bringup_attempt_observed" $protectedAttemptObserved $serviceLogSnippet

$statusDetail = [pscustomobject]@{
    appConnected = [bool]$fallbackStatus.app_connected
    frontendStatus = if ($fallbackStatus.frontend) { [string]$fallbackStatus.frontend.status } else { $null }
    frontendMode = if ($fallbackStatus.frontend) { [string]$fallbackStatus.frontend.product_mode } else { $null }
    serviceState = if ($fallbackStatus.service) { [string]$fallbackStatus.service.state } else { $null }
    serviceVerdict = if ($fallbackStatus.service) { [string]$fallbackStatus.service.health_verdict } else { $null }
    winInetProxyEnable = $fallbackWinInet.ProxyEnable
    winInetProxyServer = $fallbackWinInet.ProxyServer
    httpPort = $fallbackPort
    fetchExit = if ($fallbackFetch) { $fallbackFetch.exit } else { $null }
    fetchOutput = if ($fallbackFetch) { $fallbackFetch.output } else { $null }
    recentLogs = if ($fallbackStatus.frontend) { @($fallbackStatus.frontend.recent_logs | Select-Object -Last 12) } else { @() }
}
$statusDetail | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $evidenceDir "auto-fallback-status.json") -Encoding UTF8
Add-Step "protected_failure_degraded_to_browsers" $fallback ($statusDetail | ConvertTo-Json -Depth 4 -Compress)

$adapterPresentDuringFallback = [bool](Get-NetAdapter -Name "DoodleRay Tunnel" -ErrorAction SilentlyContinue)
Add-Step "tun_not_claimed_during_limited_fallback" (-not $adapterPresentDuringFallback) "adapterPresent=$adapterPresentDuringFallback"

$teardown = Stop-QaTunnelHard 90
Add-Step "teardown_disconnect" ($teardown -ne "still-connected") "via=$teardown"
Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 5

$svcEnd = Get-ServiceStatus
$wiEnd = Get-WinInet
$engines = @(Get-Process xray, sing-box -ErrorAction SilentlyContinue).Count
$marker = Test-Path "C:\ProgramData\DoodleRay\runtime\active-session.marker"
$adapterEnd = [bool](Get-NetAdapter -Name "DoodleRay Tunnel" -ErrorAction SilentlyContinue)
$cleanOk = ([string]$svcEnd.state) -eq "disconnected" -and $wiEnd.ProxyEnable -eq 0 -and
    $engines -eq 0 -and (Get-StatsQueryOrphanCount) -eq 0 -and (-not $marker) -and (-not $adapterEnd)
Add-Step "final_cleanup_clean" $cleanOk "service=$($svcEnd.state) winInet=$($wiEnd.ProxyEnable) engines=$engines marker=$marker adapter=$adapterEnd"

$result = [pscustomobject]@{
    ok = (@($steps | Where-Object { -not $_.ok }).Count -eq 0)
    evidence = $evidenceDir
    steps = $steps
}
$result | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $evidenceDir "auto-fallback-summary.json") -Encoding UTF8
$result | ConvertTo-Json -Depth 8
if (-not $result.ok) { exit 1 }
