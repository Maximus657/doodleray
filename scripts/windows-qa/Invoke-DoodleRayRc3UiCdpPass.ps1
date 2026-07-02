param(
    [string] $SecretPath = (Join-Path $PSScriptRoot "..\..\secrets\doodlevpn-server-access.md")
)

# RC3 UI pass over WebView2 CDP on the Play2Go stand.
# Drives the real installed app (C:\Program Files\DoodleRay\DoodleRay.exe)
# through the existing DoodleRayCodexCDP scheduled task and CDP port 9333.
# Selectors are structure/ASCII-based (no locale strings) so the pass works
# regardless of the stand UI language.

$ErrorActionPreference = "Stop"

$helpers = Get-Content (Join-Path $PSScriptRoot "CdpQaHelpers.ps1") -Raw

$remoteBody = @'
$evidenceDir = "C:\DoodleRayQA\evidence\rc3-ui"
New-Item -ItemType Directory -Force -Path $evidenceDir | Out-Null

# --- Step 1: fresh app launch with CDP -------------------------------------
Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3
$launched = Start-AppWithCdp
Add-Step "launch_installed_app_cdp" $launched "task=DoodleRayCodexCDP port=9333"
if (-not $launched) {
    [pscustomobject]@{ ok = $false; steps = $steps } | ConvertTo-Json -Depth 8
    exit 1
}

$version = Invoke-CdpEval '(document.body.innerText.match(/v\d+\.\d+\.\d+/) || [""])[0]'
$installedVersion = [string](Get-ServiceStatus).service_version
Add-Step "ui_shows_installed_version" ($installedVersion -and $version -eq "v$installedVersion") "ui=$version service=$installedVersion"

# --- Step 2: subscription refresh ------------------------------------------
$navServers = Invoke-CdpEval '(() => { const a = [...document.querySelectorAll("a[href]")].find(x => x.getAttribute("href").endsWith("/servers")); if (!a) return "no-link"; a.click(); return "clicked"; })()'
Start-Sleep -Seconds 2
$hasRefresh = Invoke-CdpEval 'document.querySelector("[title=\"Refresh subscription\"]") !== null'
$refreshResult = "skipped"
if ($hasRefresh -eq $true) {
    Invoke-CdpEval 'document.querySelector("[title=\"Refresh subscription\"]").click()' | Out-Null
    $done = Wait-CdpCondition '(() => { const b = document.querySelector("[title=\"Refresh subscription\"]"); return b && !b.disabled && !b.querySelector(".animate-spin"); })()' 60
    $refreshResult = if ($done) { "completed" } else { "timeout" }
}
$serverRows = Invoke-CdpEval 'document.body.innerText.length'
Invoke-CdpEval '(() => { const a = [...document.querySelectorAll("a[href]")].find(x => /(^|\/)$/.test(x.getAttribute("href")) || x.getAttribute("href").endsWith("#/")); if (a) a.click(); return "home"; })()' | Out-Null
Start-Sleep -Seconds 2
Add-Step "subscription_refresh" ($hasRefresh -eq $true -and $refreshResult -eq "completed") "present=$hasRefresh result=$refreshResult"

# --- Step 3: Whole Computer connect -----------------------------------------
Switch-Mode 0 "protected" | Out-Null
Start-Sleep -Seconds 2
Invoke-CdpEval 'document.querySelector("#connect-button").click()' | Out-Null
$connected = Wait-CdpCondition $exprConnected 120 3
$svc = Get-ServiceStatus
$wininet = Get-WinInet
$runtimeHttp = if ($svc) { $svc.runtime_http_port } else { $null }
$winInetPointsToService = ($wininet.ProxyEnable -eq 1 -and $runtimeHttp -and ([string]$wininet.ProxyServer).Contains("127.0.0.1:$runtimeHttp"))
$verdictOk = $svc -and @("protected", "protected_degraded") -contains ([string]$svc.health_verdict)
$portsOk = $svc -and $svc.runtime_socks_port -gt 0 -and $svc.runtime_http_port -gt 0
Add-Step "whole_computer_connect" ($connected -and $verdictOk -and $portsOk) `
    ("ui_connected=$connected verdict=$($svc.health_verdict) state=$($svc.state) gen=$($svc.service_generation) socks=$($svc.runtime_socks_port) http=$($svc.runtime_http_port) api=$($svc.runtime_api_port)")
Add-Step "wininet_points_to_service_http" $winInetPointsToService "proxyEnable=$($wininet.ProxyEnable) runtimeHttp=$runtimeHttp"

$fastHealth = Invoke-CdpEval ('window.__TAURI_INTERNALS__.invoke("get_connection_health", {proxyMode: "tun", systemProxyMode: "set", socksPort: ' + [int]$svc.runtime_socks_port + ', httpPort: ' + [int]$svc.runtime_http_port + '})') 30
$fastVerdict = if ($fastHealth -and $fastHealth.verdict) { [string]$fastHealth.verdict } else { "unavailable" }
$quicWarn = $false
if ($fastHealth -and $fastHealth.service_warning_checks) {
    $quicWarn = @($fastHealth.service_warning_checks | Where-Object { $_ -match "QUIC" }).Count -gt 0
}
Add-Step "fast_health_protected" (@("protected","protected_degraded") -contains $fastVerdict) "verdict=$fastVerdict quic_nonclaim_warning=$quicWarn"
$svc | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $evidenceDir "connect-service-status.json") -Encoding UTF8

# --- Step 4: mode switch chain: Proxy -> Whole -> Proxy -> Manual -> Whole ---
# -> Browsers (proxy mode)
Switch-Mode 1 "browsers" | Out-Null
$uiOk = Wait-CdpCondition $exprConnected 90 3
$svcProxy = Wait-ServiceState @("disconnected") 60
$wiProxy = Get-WinInet
$proxyModeOk = $uiOk -and $svcProxy -and ([string]$svcProxy.state) -eq "disconnected" -and $wiProxy.ProxyEnable -eq 1
Add-Step "switch_to_browsers" $proxyModeOk "ui=$uiOk service=$($svcProxy.state) winInet=$($wiProxy.ProxyEnable)"

# -> Whole computer
Switch-Mode 0 "protected" | Out-Null
$uiOk = Wait-CdpCondition $exprConnected 120 3
$svcTun = Wait-ServiceState @("connected") 90
$tunOk = $uiOk -and $svcTun -and ([string]$svcTun.state) -eq "connected" -and @("protected","protected_degraded") -contains ([string]$svcTun.health_verdict)
Add-Step "switch_back_to_whole" $tunOk "ui=$uiOk service=$($svcTun.state) verdict=$($svcTun.health_verdict) gen=$($svcTun.service_generation)"

# -> Browsers again
Switch-Mode 1 "browsers" | Out-Null
$uiOk = Wait-CdpCondition $exprConnected 90 3
$svcProxy2 = Wait-ServiceState @("disconnected") 60
Add-Step "switch_to_browsers_again" ($uiOk -and ([string]$svcProxy2.state) -eq "disconnected") "ui=$uiOk service=$($svcProxy2.state)"

# -> Manual proxy (no system mutation)
Switch-Mode 2 "manual" | Out-Null
$uiOk = Wait-CdpCondition $exprConnected 90 3
Start-Sleep -Seconds 3
$wiManual = Get-WinInet
$manualOk = $uiOk -and $wiManual.ProxyEnable -eq 0
Add-Step "switch_to_manual" $manualOk "ui=$uiOk winInetEnable=$($wiManual.ProxyEnable) (restored, not mutated)"

# -> Whole computer final
Switch-Mode 0 "protected" | Out-Null
$uiOk = Wait-CdpCondition $exprConnected 120 3
$svcFinal = Wait-ServiceState @("connected") 90
$finalTunOk = $uiOk -and ([string]$svcFinal.state) -eq "connected" -and @("protected","protected_degraded") -contains ([string]$svcFinal.health_verdict)
Add-Step "switch_final_whole" $finalTunOk "ui=$uiOk service=$($svcFinal.state) verdict=$($svcFinal.health_verdict) gen=$($svcFinal.service_generation)"

# --- Step 5: UI crash / reload while protected ------------------------------
$genBefore = $svcFinal.service_generation
Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 5
$svcAfterUiKill = Get-ServiceStatus
$serviceSurvived = $svcAfterUiKill -and ([string]$svcAfterUiKill.state) -eq "connected" -and $svcAfterUiKill.service_generation -eq $genBefore
Add-Step "ui_kill_service_survives" $serviceSurvived "state=$($svcAfterUiKill.state) gen=$($svcAfterUiKill.service_generation) singboxPid=$($svcAfterUiKill.singbox_pid)"

$relaunched = Start-AppWithCdp
$uiReattached = $false
$wiReassert = $null
if ($relaunched) {
    $uiReattached = Wait-CdpCondition $exprConnected 90 3
    Start-Sleep -Seconds 3
    $wiReassert = Get-WinInet
}
$svcReattach = Get-ServiceStatus
$reassertOk = $uiReattached -and $wiReassert -and $wiReassert.ProxyEnable -eq 1 -and ([string]$wiReassert.ProxyServer).Contains("127.0.0.1:$($svcReattach.runtime_http_port)")
Add-Step "ui_reload_reattach" ($relaunched -and $uiReattached) "relaunched=$relaunched uiConnected=$uiReattached serviceGen=$($svcReattach.service_generation)"
Add-Step "ui_reload_wininet_reassert" $reassertOk "proxyEnable=$($wiReassert.ProxyEnable) runtimeHttp=$($svcReattach.runtime_http_port)"
Add-Step "no_statsquery_after_reload" ((Get-StatsQueryOrphanCount) -eq 0) "count=$(Get-StatsQueryOrphanCount)"

# --- Step 6: service-owned core crash ---------------------------------------
$coreKilled = $false
$svcNow = Get-ServiceStatus
if ($svcNow -and $svcNow.singbox_pid) {
    Stop-Process -Id $svcNow.singbox_pid -Force -ErrorAction SilentlyContinue
    $coreKilled = $true
}
$uiLeftGreen = Wait-CdpCondition $exprNotConnected 90 3
Start-Sleep -Seconds 5
$svcAfterCrash = Get-ServiceStatus
$wiAfterCrash = Get-WinInet
$crashCleanupOk = $uiLeftGreen -and $svcAfterCrash -and @("disconnected","failed") -contains ([string]$svcAfterCrash.state) -and
    (-not $svcAfterCrash.runtime_socks_port) -and $wiAfterCrash.ProxyEnable -eq 0
$engineOrphans = @(Get-Process xray, sing-box -ErrorAction SilentlyContinue).Count
Add-Step "core_crash_no_fake_green" ($coreKilled -and $uiLeftGreen) "killed=$coreKilled uiLeftConnected=$uiLeftGreen"
Add-Step "core_crash_cleanup" ($crashCleanupOk -and $engineOrphans -eq 0) "service=$($svcAfterCrash.state) winInet=$($wiAfterCrash.ProxyEnable) engines=$engineOrphans statsquery=$(Get-StatsQueryOrphanCount)"

# --- Step 7: support bundle after failure -----------------------------------
$bundlePath = Invoke-CdpEval 'window.__TAURI_INTERNALS__.invoke("export_support_bundle", {proxyMode: "tun", systemProxyMode: "set", socksPort: 1080, httpPort: 1081, failureMarker: "rc3-ui-cdp-core-crash"})' 90
$bundleOk = $false
$bundleDetail = "no path"
if ($bundlePath -is [string] -and (Test-Path $bundlePath)) {
    $bundle = Get-Content $bundlePath -Raw
    $hasMarker = $bundle.Contains("failure_marker=rc3-ui-cdp-core-crash")
    $hasService = $bundle.Contains("## Tunnel Service")
    $hasSignatures = ($bundle -match "thumbprint")
    $hasNetSummary = ($bundle -match "WinINet|ProxyEnable")
    $noUrls = -not ($bundle -match "://")
    $noUuids = -not ($bundle -match "[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
    $bundleOk = $hasMarker -and $hasService -and $hasSignatures -and $hasNetSummary -and $noUrls -and $noUuids
    $bundleDetail = "marker=$hasMarker service=$hasService signer=$hasSignatures net=$hasNetSummary noUrls=$noUrls noUuids=$noUuids bytes=$($bundle.Length)"
    Copy-Item $bundlePath (Join-Path $evidenceDir "support-bundle-after-core-crash.txt") -Force
}
Add-Step "support_bundle_redacted" $bundleOk $bundleDetail

# --- Step 8: reconnect once more, then final cleanup -------------------------
Invoke-CdpEval 'document.querySelector("#connect-button").click()' | Out-Null
$reconnected = Wait-CdpCondition $exprConnected 120 3
$svcRe = Get-ServiceStatus
Add-Step "reconnect_after_crash" ($reconnected -and @("protected","protected_degraded") -contains ([string]$svcRe.health_verdict)) "ui=$reconnected verdict=$($svcRe.health_verdict) gen=$($svcRe.service_generation)"

Invoke-CdpEval 'window.__TAURI_INTERNALS__.invoke("vpn_disconnect")' 60 | Out-Null
Start-Sleep -Seconds 5
Invoke-CdpEval 'window.__TAURI_INTERNALS__.invoke("quit_app")' 15 | Out-Null
Start-Sleep -Seconds 5
Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3

$svcEnd = Get-ServiceStatus
$wiEnd = Get-WinInet
$adapter = [bool](Get-NetAdapter -Name "DoodleRay Tunnel" -ErrorAction SilentlyContinue)
$nrpt = @(Get-DnsClientNrptRule -ErrorAction SilentlyContinue | Where-Object { ($_.Namespace -match "doodleray") -or ($_.Comment -match "DoodleRay") }).Count
$engines = @(Get-Process xray, sing-box -ErrorAction SilentlyContinue).Count
$marker = Test-Path "C:\ProgramData\DoodleRay\runtime\active-session.marker"
$cleanOk = ([string]$svcEnd.state) -eq "disconnected" -and $wiEnd.ProxyEnable -eq 0 -and (-not $wiEnd.ProxyServer) -and
    (-not $adapter) -and $nrpt -eq 0 -and $engines -eq 0 -and (Get-StatsQueryOrphanCount) -eq 0 -and (-not $marker)
Add-Step "final_cleanup_clean" $cleanOk "service=$($svcEnd.state) winInet=$($wiEnd.ProxyEnable) adapter=$adapter nrpt=$nrpt engines=$engines marker=$marker"

$allOk = @($steps | Where-Object { -not $_.ok }).Count -eq 0
$result = [pscustomobject]@{ ok = $allOk; steps = $steps }
$result | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $evidenceDir "rc3-ui-pass-summary.json") -Encoding UTF8
$result | ConvertTo-Json -Depth 8
'@

$remoteScript = $helpers + "`n" + $remoteBody
& (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -Command $remoteScript -SecretPath $SecretPath
exit $LASTEXITCODE
