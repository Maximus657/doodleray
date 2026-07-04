param(
    [string] $SecretPath = (Join-Path $PSScriptRoot "..\..\secrets\doodlevpn-server-access.md")
)

# Targeted proof for the bounded TUN bring-up repair:
# start a Whole Computer connect through the real UI (CDP), kill the
# service-owned sing-box exactly once DURING the Connecting window (this makes
# the wintun adapter vanish, reproducing the production
# "DoodleRay Tunnel adapter is missing" path), then assert that the service
# self-repairs: the connect still ends protected/protected_degraded and the
# structured health carries the "TUN adapter repair retry ran after" warning.
# If the repair genuinely fails, the error must be the enriched actionable
# message and the stand must be left clean - that is the only acceptable
# fallback outcome.

$ErrorActionPreference = "Stop"

$helpers = Get-Content (Join-Path $PSScriptRoot "CdpQaHelpers.ps1") -Raw

$remoteBody = @'
$evidenceDir = "C:\DoodleRayQA\evidence\tun-bringup-repair"
New-Item -ItemType Directory -Force -Path $evidenceDir | Out-Null

Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3
$launched = Start-AppWithCdp
Add-Step "launch_app_cdp" $launched "task=DoodleRayCodexCDP"

Switch-Mode 0 "protected" | Out-Null
Start-Sleep -Seconds 2

$repairProven = $false
$honestFailure = $false
$attemptDetails = @()

for ($cycle = 1; $cycle -le 3 -and -not $repairProven; $cycle++) {
    # Make sure we start from disconnected.
    $svc = Get-ServiceStatus
    if ($svc -and ([string]$svc.state) -eq "connected") {
        Start-QaDisconnect | Out-Null
        Start-Sleep -Seconds 5
    }

    $connectChannel = Start-QaConnect

    # Kill the service-owned sing-box once, only during the Connecting window.
    $killed = $false
    $killPhase = $null
    $deadline = (Get-Date).AddSeconds(30)
    while ((Get-Date) -lt $deadline -and -not $killed) {
        $svc = Get-ServiceStatus
        if ($svc -and ([string]$svc.state) -eq "connected") { break }
        if ($svc -and ([string]$svc.state) -eq "connecting" -and $svc.singbox_pid) {
            try {
                Stop-Process -Id $svc.singbox_pid -Force -ErrorAction Stop
                $killed = $true
                $killPhase = [string]$svc.phase
            } catch {}
        }
        Start-Sleep -Milliseconds 250
    }

    # Wait for the outcome of this connect cycle.
    $outcome = "timeout"
    $final = $null
    $deadline = (Get-Date).AddSeconds(120)
    while ((Get-Date) -lt $deadline) {
        $final = Get-ServiceStatus
        $state = [string]$final.state
        if ($state -eq "connected") { $outcome = "connected"; break }
        if ($state -in @("failed", "disconnected") -and $final.last_repair_action -ne "tun_adapter_repair") {
            # Give the UI a moment: transient disconnected during repair is
            # guarded service-side; a stable failed/disconnected is terminal.
            Start-Sleep -Seconds 5
            $confirm = Get-ServiceStatus
            if (([string]$confirm.state) -in @("failed", "disconnected")) { $outcome = [string]$confirm.state; $final = $confirm; break }
        }
        Start-Sleep -Seconds 3
    }

    $retryWarning = @($final.warning_checks | Where-Object { $_ -match "TUN adapter repair retry ran after" }).Count -gt 0
    $attemptDetails += "cycle=$cycle channel=$connectChannel killed=$killed killPhase=$killPhase outcome=$outcome retryWarning=$retryWarning verdict=$($final.health_verdict) gen=$($final.service_generation)"

    if ($killed -and $outcome -eq "connected" -and $retryWarning -and
        @("protected", "protected_degraded") -contains ([string]$final.health_verdict)) {
        $repairProven = $true
        $final | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $evidenceDir "repaired-status.json") -Encoding UTF8
    } elseif ($killed -and $outcome -in @("failed", "disconnected")) {
        $err = [string]$final.error
        if ($err -like "DoodleRay could not create the Windows tunnel adapter*") {
            $honestFailure = $true
            $final | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $evidenceDir "honest-failure-status.json") -Encoding UTF8
        }
    }
}

Add-Step "bringup_crash_injected_and_repaired" $repairProven ($attemptDetails -join " | ")
if (-not $repairProven) {
    Add-Step "fallback_honest_enriched_failure" $honestFailure ($attemptDetails -join " | ")
}

# Final cleanliness regardless of outcome. The disconnect must be confirmed:
# a /disconnect during UI 'connecting' is ignored by design, so keep retrying
# and fall back to a clean service restart (owned SCM-stop cleanup).
$teardown = Stop-QaTunnelHard 60
Add-Step "teardown_disconnect" ($teardown -ne "still-connected") "via=$teardown"
Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3

# The teardown may have just restarted the service; give the pipe time.
$svcEnd = $null
for ($i = 0; $i -lt 10 -and -not $svcEnd; $i++) {
    $svcEnd = Get-ServiceStatus
    if (-not $svcEnd) { Start-Sleep -Seconds 3 }
}
$wiEnd = Get-WinInet
$engines = @(Get-Process xray, sing-box -ErrorAction SilentlyContinue).Count
$marker = Test-Path "C:\ProgramData\DoodleRay\runtime\active-session.marker"
$cleanOk = ([string]$svcEnd.state) -eq "disconnected" -and $wiEnd.ProxyEnable -eq 0 -and
    $engines -eq 0 -and (Get-StatsQueryOrphanCount) -eq 0 -and (-not $marker)
Add-Step "final_cleanup_clean" $cleanOk "service=$($svcEnd.state) winInet=$($wiEnd.ProxyEnable) engines=$engines marker=$marker"

$allOk = @($steps | Where-Object { -not $_.ok }).Count -eq 0
$result = [pscustomobject]@{ ok = $allOk; steps = $steps }
$result | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $evidenceDir "tun-bringup-repair-summary.json") -Encoding UTF8
$result | ConvertTo-Json -Depth 8
'@

$remoteScript = $helpers + "`n" + $remoteBody
& (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -Command $remoteScript -SecretPath $SecretPath
exit $LASTEXITCODE
