param(
    [string] $SecretPath = (Join-Path $PSScriptRoot "..\..\secrets\doodlevpn-server-access.md")
)

# Stale-state repair proof: inject DoodleRay-shaped stale WinINet loopback
# proxy and a DoodleRay-commented NRPT rule while the app is closed, launch
# the installed app, and assert startup repair clears both without touching
# anything else. Stale routes/adapters variants are covered by the live
# bring-up/crash harnesses because they require an owned adapter to exist.

$ErrorActionPreference = "Stop"

$helpers = Get-Content (Join-Path $PSScriptRoot "CdpQaHelpers.ps1") -Raw

$remoteBody = @'
$evidenceDir = "C:\DoodleRayQA\evidence\stale-state-repair"
New-Item -ItemType Directory -Force -Path $evidenceDir | Out-Null

Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3

# Inject stale WinINet in the LEGACY DoodleRay shape (loopback :10809 map +
# game-bypass override). Ownership classification must treat this as
# DoodleRay-owned; an anonymous 127.0.0.1:9 proxy would rightly be left alone.
$key = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings"
Set-ItemProperty -Path $key -Name ProxyEnable -Value 1
Set-ItemProperty -Path $key -Name ProxyServer -Value "http=127.0.0.1:10809;https=127.0.0.1:10809"
Set-ItemProperty -Path $key -Name ProxyOverride -Value "<local>;*.riotgames.com"

# Inject a DoodleRay-marked NRPT rule (repair may only remove DoodleRay-owned).
$nrptInjected = $false
if (Get-Command Add-DnsClientNrptRule -ErrorAction SilentlyContinue) {
    try {
        Add-DnsClientNrptRule -Namespace ".doodleray-stale-test.invalid" -NameServers "127.0.0.1" -Comment "DoodleRay stale QA test" -ErrorAction Stop | Out-Null
        $nrptInjected = $true
    } catch {}
}
# A non-DoodleRay control rule must SURVIVE the repair.
$controlInjected = $false
if (Get-Command Add-DnsClientNrptRule -ErrorAction SilentlyContinue) {
    try {
        Add-DnsClientNrptRule -Namespace ".qa-third-party-control.invalid" -NameServers "127.0.0.1" -Comment "ThirdParty QA control" -ErrorAction Stop | Out-Null
        $controlInjected = $true
    } catch {}
}
Add-Step "stale_state_injected" ($nrptInjected -and $controlInjected) "winInet=1 nrpt=$nrptInjected control=$controlInjected"

# Launch installed app; startup repair must run.
$launched = Start-AppWithCdp
Add-Step "app_launched" $launched ""
Start-Sleep -Seconds 8

$wi = Get-WinInet
$doodleNrpt = @(Get-DnsClientNrptRule -ErrorAction SilentlyContinue | Where-Object {
    ($_.Namespace -match "doodleray-stale-test") -or ($_.Comment -match "DoodleRay stale QA test")
}).Count
$controlNrpt = @(Get-DnsClientNrptRule -ErrorAction SilentlyContinue | Where-Object {
    ($_.Namespace -match "qa-third-party-control") -or ($_.Comment -match "ThirdParty QA control")
}).Count

Add-Step "stale_wininet_repaired" ($wi.ProxyEnable -eq 0 -and -not $wi.ProxyServer) "proxyEnable=$($wi.ProxyEnable)"
Add-Step "doodleray_nrpt_repaired" ($doodleNrpt -eq 0) "doodleRayRules=$doodleNrpt"
Add-Step "third_party_nrpt_survived" ($controlNrpt -eq 1) "controlRules=$controlNrpt"

# Cleanup: remove the third-party control rule ourselves; quit app.
Get-DnsClientNrptRule -ErrorAction SilentlyContinue | Where-Object {
    ($_.Namespace -match "qa-third-party-control") -or ($_.Comment -match "ThirdParty QA control")
} | ForEach-Object { try { Remove-DnsClientNrptRule -Name $_.Name -Force -ErrorAction Stop } catch {} }
Invoke-CdpEval 'window.__TAURI_INTERNALS__.invoke("quit_app")' 15 | Out-Null
Start-Sleep -Seconds 4
Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

$allOk = @($steps | Where-Object { -not $_.ok }).Count -eq 0
$result = [pscustomobject]@{ ok = $allOk; steps = $steps }
$result | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $evidenceDir "stale-state-repair-summary.json") -Encoding UTF8
$result | ConvertTo-Json -Depth 8
if (-not $allOk) { exit 1 }
'@

$remoteScript = $helpers + "`n" + $remoteBody
& (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -Command $remoteScript -SecretPath $SecretPath
exit $LASTEXITCODE
