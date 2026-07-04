param(
    [string] $RemoteRcInstaller = "C:\DoodleRayQA\artifacts\DoodleRay-v6-rc-setup.exe",
    [string] $ExpectedRcVersion = "5.9.0",
    [switch] $AllowUnsignedLocalRc,
    [string] $SecretPath = (Join-Path $PSScriptRoot "..\..\secrets\doodlevpn-server-access.md")
)

# Active-VPN-during-update QA:
# 1. drive the installed app over CDP into a protected (Whole Computer)
#    connection so the tunnel is genuinely active;
# 2. run the RC installer silently OVER the active install;
# 3. assert the NSIS pre-install SCM stop ran DoodleRay-owned cleanup
#    (no engine children, no orphans, no false unclean-shutdown marker);
# 4. relaunch the app and assert startup repair clears any stale
#    DoodleRay-owned WinINet left from the killed UI;
# 5. reconnect protected and verify health, then leave the stand clean.
#
# Note: the previous-public-version *idle* upgrade path (5.4.3/5.4.4/5.4.5)
# is covered by Invoke-DoodleRayUpdatePathQa.ps1. Driving an *active* old
# version through this harness would depend on the old UI selectors; the
# active-update scenario is exercised against the current RC install.

$ErrorActionPreference = "Stop"

$allowUnsignedLiteral = if ($AllowUnsignedLocalRc.IsPresent) { '$true' } else { '$false' }
$helpers = Get-Content (Join-Path $PSScriptRoot "CdpQaHelpers.ps1") -Raw

$remoteBody = @"
`$evidenceDir = "C:\DoodleRayQA\evidence\active-update"
New-Item -ItemType Directory -Force -Path `$evidenceDir | Out-Null

# --- Step 1: active protected connection ------------------------------------
Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3
`$launched = Start-AppWithCdp
Add-Step "launch_app_cdp" `$launched "task=DoodleRayCodexCDP"

Switch-Mode 0 "protected" | Out-Null
Start-Sleep -Seconds 2
Start-QaConnect | Out-Null
`$connected = Wait-CdpCondition `$exprConnected 120 3
`$svcBefore = Get-ServiceStatus
`$wiBefore = Get-WinInet
`$qaBefore = Invoke-QaControl "/status" 10
`$recentBefore = if (`$qaBefore.frontend -and `$qaBefore.frontend.recent_logs) {
    ((`$qaBefore.frontend.recent_logs | Select-Object -Last 6 | ForEach-Object { "`$(`$_.level): `$(`$_.message)" }) -join " || ")
} else { "" }
`$activeOk = `$connected -and `$svcBefore -and ([string]`$svcBefore.state) -eq "connected" -and
    @("protected", "protected_degraded") -contains ([string]`$svcBefore.health_verdict)
Add-Step "protected_active_before_update" `$activeOk "ui=`$connected state=`$(`$svcBefore.state) verdict=`$(`$svcBefore.health_verdict) gen=`$(`$svcBefore.service_generation) winInet=`$(`$wiBefore.ProxyEnable) logs=`$recentBefore"
`$svcBefore | ConvertTo-Json -Depth 6 | Set-Content (Join-Path `$evidenceDir "service-before-update.json") -Encoding UTF8

# --- Step 2: silent RC install over the ACTIVE install -----------------------
if (-not (Test-Path -LiteralPath "$RemoteRcInstaller")) {
    Add-Step "rc_installer_present" `$false "$RemoteRcInstaller"
} else {
    Add-Step "rc_installer_present" `$true (Get-FileHash -Algorithm SHA256 -LiteralPath "$RemoteRcInstaller").Hash
    `$proc = Start-Process -FilePath "$RemoteRcInstaller" -ArgumentList "/S" -Wait -PassThru
    Add-Step "active_update_install_exit0" (`$proc.ExitCode -eq 0) "exit=`$(`$proc.ExitCode)"
    Start-Sleep -Seconds 8
}

# --- Step 3: post-update service/cleanup truth --------------------------------
`$sig = Get-AuthenticodeSignature -LiteralPath "C:\Program Files\DoodleRay\DoodleRayService.exe"
if (`$sig.Status -ne "Valid" -and -not $allowUnsignedLiteral) {
    Add-Step "updated_service_signature" `$false ([string]`$sig.Status)
} else {
    Add-Step "updated_service_signature" `$true "status=`$(`$sig.Status) allowUnsignedLocalRc=$allowUnsignedLiteral"
}

`$service = Get-Service DoodleRayTunnelService -ErrorAction SilentlyContinue
if (`$service -and `$service.Status -ne "Running") {
    Start-Service DoodleRayTunnelService
    Start-Sleep -Seconds 4
}
`$svcAfter = Get-ServiceStatus
`$engineCount = @(Get-Process xray, sing-box -ErrorAction SilentlyContinue).Count
`$markerLeft = Test-Path "C:\ProgramData\DoodleRay\runtime\active-session.marker"
`$postOk = `$svcAfter -and ([string]`$svcAfter.service_version) -eq "$ExpectedRcVersion" -and
    ([string]`$svcAfter.state) -eq "disconnected" -and `$engineCount -eq 0 -and
    (Get-StatsQueryOrphanCount) -eq 0 -and (-not `$markerLeft)
Add-Step "post_update_service_clean" `$postOk "version=`$(`$svcAfter.service_version) state=`$(`$svcAfter.state) engines=`$engineCount statsquery=`$(Get-StatsQueryOrphanCount) markerLeft=`$markerLeft uncleanFlag=`$(`$svcAfter.previous_unclean_shutdown)"
`$wiStale = Get-WinInet
Add-Step "record_wininet_after_kill" `$true "proxyEnable=`$(`$wiStale.ProxyEnable) (stale loopback allowed here; startup repair must clear it)"

# --- Step 4: app relaunch runs startup repair --------------------------------
`$relaunched = Start-AppWithCdp
Add-Step "relaunch_after_update" `$relaunched ""
Start-Sleep -Seconds 6
`$wiRepaired = Get-WinInet
`$repairOk = `$relaunched -and `$wiRepaired.ProxyEnable -eq 0
Add-Step "startup_repair_cleared_stale_wininet" `$repairOk "proxyEnable=`$(`$wiRepaired.ProxyEnable) proxyServer=`$(if (`$wiRepaired.ProxyServer) { 'present' } else { 'empty' })"

# --- Step 5: reconnect protected on the updated build -------------------------
Switch-Mode 0 "protected" | Out-Null
Start-Sleep -Seconds 2
Start-QaConnect | Out-Null
`$reconnected = Wait-CdpCondition `$exprConnected 120 3
`$svcRe = Get-ServiceStatus
`$qaRe = Invoke-QaControl "/status" 10
`$recentRe = if (`$qaRe.frontend -and `$qaRe.frontend.recent_logs) {
    ((`$qaRe.frontend.recent_logs | Select-Object -Last 6 | ForEach-Object { "`$(`$_.level): `$(`$_.message)" }) -join " || ")
} else { "" }
`$reOk = `$reconnected -and @("protected", "protected_degraded") -contains ([string]`$svcRe.health_verdict)
Add-Step "reconnect_after_active_update" `$reOk "ui=`$reconnected verdict=`$(`$svcRe.health_verdict) gen=`$(`$svcRe.service_generation) socks=`$(`$svcRe.runtime_socks_port) http=`$(`$svcRe.runtime_http_port) logs=`$recentRe"

# --- Step 6: disconnect, quit, final cleanliness ------------------------------
Start-QaDisconnect | Out-Null
Start-Sleep -Seconds 5
Invoke-CdpEval 'window.__TAURI_INTERNALS__.invoke("quit_app")' 15 | Out-Null
Start-Sleep -Seconds 5
Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3

`$svcEnd = Get-ServiceStatus
`$wiEnd = Get-WinInet
`$adapter = [bool](Get-NetAdapter -Name "DoodleRay Tunnel" -ErrorAction SilentlyContinue)
`$nrpt = @(Get-DnsClientNrptRule -ErrorAction SilentlyContinue | Where-Object { (`$_.Namespace -match "doodleray") -or (`$_.Comment -match "DoodleRay") }).Count
`$engines = @(Get-Process xray, sing-box -ErrorAction SilentlyContinue).Count
`$marker = Test-Path "C:\ProgramData\DoodleRay\runtime\active-session.marker"
`$cleanOk = ([string]`$svcEnd.state) -eq "disconnected" -and `$wiEnd.ProxyEnable -eq 0 -and (-not `$wiEnd.ProxyServer) -and
    (-not `$adapter) -and `$nrpt -eq 0 -and `$engines -eq 0 -and (Get-StatsQueryOrphanCount) -eq 0 -and (-not `$marker)
Add-Step "final_cleanup_clean" `$cleanOk "service=`$(`$svcEnd.state) winInet=`$(`$wiEnd.ProxyEnable) adapter=`$adapter nrpt=`$nrpt engines=`$engines marker=`$marker"

`$allOk = @(`$steps | Where-Object { -not `$_.ok }).Count -eq 0
`$result = [pscustomobject]@{ ok = `$allOk; steps = `$steps }
`$result | ConvertTo-Json -Depth 8 | Set-Content (Join-Path `$evidenceDir "active-update-summary.json") -Encoding UTF8
`$result | ConvertTo-Json -Depth 8
if (-not `$allOk) { exit 1 }
"@

$remoteScript = $helpers + "`n" + $remoteBody
& (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -Command $remoteScript -SecretPath $SecretPath
exit $LASTEXITCODE
