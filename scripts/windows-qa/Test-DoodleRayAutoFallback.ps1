param(
    [string] $SecretPath = (Join-Path $PSScriptRoot "..\..\secrets\doodlevpn-server-access.md"),
    [string] $PlinkPath = "C:\Program Files\PuTTY\plink.exe",
    [string] $PscpPath = "C:\Program Files\PuTTY\pscp.exe",
    [string] $RemoteScratch = "C:\DoodleRayQA\codex-run"
)

# End-to-end proof for the honest Protected -> Browsers fallback:
# start a real Whole Computer connect through the installed app QA control
# surface, kill the service-owned sing-box during BOTH bounded TUN bring-up
# attempts, and assert the app degrades to browser compatibility instead of
# showing a fake protected state or surfacing the raw TUN failure to the user.
#
# Pass means:
# - the app reports connected after the induced protected-mode failure;
# - the tunnel service is not claiming protected/connected;
# - WinINet points at a loopback HTTP proxy;
# - that HTTP proxy accepts connections and can fetch Apple's captive probe;
# - no DoodleRay Tunnel adapter/session marker/engine process is left after
#   teardown.

$ErrorActionPreference = "Stop"

function Get-SecretField {
    param([string] $Text, [string] $Name)
    $match = [regex]::Match($Text, "(?m)^\s*(?:-\s*)?$([regex]::Escape($Name))\s*:\s*(\S+)\s*$")
    if (-not $match.Success) { return $null }
    return $match.Groups[1].Value
}

$helpers = Get-Content (Join-Path $PSScriptRoot "CdpQaHelpers.ps1") -Raw

$remoteBody = @'
function Get-HttpProxyPortFromWinInet {
    param($WinInet)
    $server = [string]$WinInet.ProxyServer
    if (-not $server) { return $null }
    foreach ($part in ($server -split ';')) {
        $value = $part.Trim()
        if (-not $value) { continue }
        if ($value -match '^(?:http=)?127\.0\.0\.1:(\d+)$') { return [int]$Matches[1] }
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

$evidenceDir = "C:\DoodleRayQA\evidence\auto-fallback"
New-Item -ItemType Directory -Force -Path $evidenceDir | Out-Null

Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3
$launched = Start-AppWithCdp
Add-Step "launch_app_control" $launched "task=DoodleRayCodexCDP qaControl=$(Test-QaControlAvailable)"

Stop-QaTunnelHard 45 | Out-Null
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
$deadline = (Get-Date).AddSeconds(95)
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
Add-Step "service_singbox_kill_attempts_recorded" $true "count=$($killed.Count); detail=$(($killed | ConvertTo-Json -Depth 4 -Compress))"

$fallback = $false
$fallbackStatus = $null
$fallbackWinInet = $null
$fallbackPort = $null
$fallbackFetch = $null
$deadline = (Get-Date).AddSeconds(180)
while ((Get-Date) -lt $deadline -and -not $fallback) {
    $fallbackStatus = Invoke-QaControl "/status" 5
    $fallbackWinInet = Get-WinInet
    $fallbackPort = Get-HttpProxyPortFromWinInet $fallbackWinInet
    $serviceState = [string]$fallbackStatus.service.state
    $serviceVerdict = [string]$fallbackStatus.service.health_verdict
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
$serviceLogSnippet = (($newServiceLines | Where-Object { $_ -match 'StartTunnel accepted|start_tunnel generation|tun bring-up attempt|failed_cleanup|StopTunnel requested' } | Select-Object -First 12) -join ' | ')
Add-Step "protected_bringup_attempt_observed" $protectedAttemptObserved $serviceLogSnippet

$statusDetail = [pscustomobject]@{
    appConnected = [bool]$fallbackStatus.app_connected
    frontendStatus = if ($fallbackStatus.frontend) { [string]$fallbackStatus.frontend.status } else { $null }
    serviceState = [string]$fallbackStatus.service.state
    serviceVerdict = [string]$fallbackStatus.service.health_verdict
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

$svcEnd = $null
for ($i = 0; $i -lt 10 -and -not $svcEnd; $i++) {
    $svcEnd = Get-ServiceStatus
    if (-not $svcEnd) { Start-Sleep -Seconds 3 }
}
$wiEnd = Get-WinInet
$engines = @(Get-Process xray, sing-box -ErrorAction SilentlyContinue).Count
$marker = Test-Path "C:\ProgramData\DoodleRay\runtime\active-session.marker"
$adapterEnd = [bool](Get-NetAdapter -Name "DoodleRay Tunnel" -ErrorAction SilentlyContinue)
$cleanOk = ([string]$svcEnd.state) -eq "disconnected" -and $wiEnd.ProxyEnable -eq 0 -and
    $engines -eq 0 -and (Get-StatsQueryOrphanCount) -eq 0 -and (-not $marker) -and (-not $adapterEnd)
Add-Step "final_cleanup_clean" $cleanOk "service=$($svcEnd.state) winInet=$($wiEnd.ProxyEnable) engines=$engines marker=$marker adapter=$adapterEnd"

$result = [pscustomobject]@{
    ok = (@($steps | Where-Object { -not $_.ok }).Count -eq 0)
    steps = $steps
}
$result | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $evidenceDir "auto-fallback-summary.json") -Encoding UTF8
$result | ConvertTo-Json -Depth 8
'@

$remoteScript = $helpers + "`n" + $remoteBody

if (-not (Test-Path -LiteralPath $SecretPath)) { throw "Secret file not found: $SecretPath" }
if (-not (Test-Path -LiteralPath $PlinkPath)) { throw "PuTTY plink.exe not found: $PlinkPath" }
if (-not (Test-Path -LiteralPath $PscpPath)) { throw "PuTTY pscp.exe not found: $PscpPath" }

$secretText = Get-Content -LiteralPath $SecretPath -Raw
$hostName = Get-SecretField $secretText "host"
$userName = Get-SecretField $secretText "login_user"
$password = Get-SecretField $secretText "login_password"
$hostKey = Get-SecretField $secretText "ssh_hostkey"
if (-not $hostKey) { $hostKey = $env:DOODLERAY_PLAY2GO_HOSTKEY }
if (-not $hostName -or -not $userName -or -not $password -or -not $hostKey) {
    throw "Secret file must contain host, login_user, login_password, ssh_hostkey."
}

$sshTarget = "$userName@$hostName"
$localTemp = Join-Path $env:TEMP ("doodleray-auto-fallback-" + [guid]::NewGuid().ToString("N") + ".ps1")
$remoteScriptPath = $RemoteScratch.TrimEnd("\") + "\Test-DoodleRayAutoFallback.remote.ps1"
$remoteEvidenceDir = "C:\DoodleRayQA\evidence\auto-fallback"
$remoteSummary = "$remoteEvidenceDir\auto-fallback-summary.json"
$remoteOut = "$remoteEvidenceDir\detached.out"
$remoteErr = "$remoteEvidenceDir\detached.err"
$remotePid = "$remoteEvidenceDir\detached.pid"
$remoteTask = "DoodleRayAutoFallbackTest"

try {
    Set-Content -LiteralPath $localTemp -Value $remoteScript -Encoding UTF8
    $prep = "New-Item -ItemType Directory -Force -Path '$RemoteScratch', '$remoteEvidenceDir' | Out-Null"
    $prepEncoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($prep))
    & $PlinkPath -ssh $sshTarget -pw $password -batch -hostkey $hostKey "powershell -NoProfile -EncodedCommand $prepEncoded"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    & $PscpPath -batch -hostkey $hostKey -pw $password $localTemp "${sshTarget}:$remoteScriptPath" | Out-Null
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $launcher = @"
`$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path '$remoteEvidenceDir' | Out-Null
Remove-Item -LiteralPath '$remoteSummary', '$remoteOut', '$remoteErr', '$remotePid' -Force -ErrorAction SilentlyContinue
Start-Service DoodleRayTunnelService -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3
Unregister-ScheduledTask -TaskName '$remoteTask' -Confirm:`$false -ErrorAction SilentlyContinue
`$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument '-NoProfile -ExecutionPolicy Bypass -File "$remoteScriptPath"'
`$principal = New-ScheduledTaskPrincipal -UserId `$env:USERNAME -LogonType Interactive -RunLevel Highest
Register-ScheduledTask -TaskName '$remoteTask' -Action `$action -Principal `$principal -Force | Out-Null
Start-ScheduledTask -TaskName '$remoteTask'
Set-Content -LiteralPath '$remotePid' -Value '$remoteTask' -Encoding ASCII
[pscustomobject]@{ started = `$true; task = '$remoteTask'; summary = '$remoteSummary' } | ConvertTo-Json
"@
    & (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -Command $launcher -SecretPath $SecretPath
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $lastPoll = $null
    for ($i = 0; $i -lt 48; $i++) {
        Start-Sleep -Seconds 15
        $poll = @"
`$summary = '$remoteSummary'
`$pidPath = '$remotePid'
if (Test-Path -LiteralPath `$summary) {
    Get-Content -LiteralPath `$summary -Raw
    exit 0
}
`$taskState = `$null
try { `$taskState = (Get-ScheduledTask -TaskName '$remoteTask' -ErrorAction Stop).State.ToString() } catch {}
`$alive = `$taskState -eq 'Running'
`$outTail = if (Test-Path -LiteralPath '$remoteOut') { (Get-Content -LiteralPath '$remoteOut' -Tail 20 -ErrorAction SilentlyContinue) -join "`n" } else { '' }
`$errTail = if (Test-Path -LiteralPath '$remoteErr') { (Get-Content -LiteralPath '$remoteErr' -Tail 20 -ErrorAction SilentlyContinue) -join "`n" } else { '' }
[pscustomobject]@{ pending = `$true; alive = `$alive; taskState = `$taskState; outTail = `$outTail; errTail = `$errTail } | ConvertTo-Json -Depth 4
"@
        try {
            $lastPoll = & (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -Command $poll -SecretPath $SecretPath 2>&1 | Out-String
        } catch {
            $lastPoll = "poll transport failed: $($_.Exception.Message)"
            continue
        }
        if ($lastPoll -match '"steps"\s*:') {
            Write-Output $lastPoll.Trim()
            try {
                $jsonText = $lastPoll
                $clixmlAt = $jsonText.IndexOf("#< CLIXML")
                if ($clixmlAt -ge 0) {
                    $jsonText = $jsonText.Substring(0, $clixmlAt)
                }
                $json = $jsonText.Trim() | ConvertFrom-Json
                exit ([int](-not [bool]$json.ok))
            } catch {
                exit 1
            }
        }
        if ($lastPoll -match '"pending"\s*:\s*false') { break }
    }
    Write-Output $lastPoll
    throw "auto-fallback detached test did not produce a summary in time"
} finally {
    Remove-Item -LiteralPath $localTemp -Force -ErrorAction SilentlyContinue
}
