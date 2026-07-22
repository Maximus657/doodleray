<#
.SYNOPSIS
Verifies a machine is clean of DoodleRay after uninstall (QA stand / VM only).

.DESCRIPTION
Read-only checks; safe to run anywhere, intended for the Play2Go stand or a
clean Windows VM after `uninstall.exe /S`. Mirrors the cleanup contract from
docs/windows-pc-qa-play2go.md: service gone, no orphan engine processes,
WinINet not left pointing at DoodleRay, no DoodleRay NRPT/routes/adapters,
no runtime marker, app-owned scheduled tasks removed, Apps&Features entry gone.

Exit code 0 = clean, 1 = leftovers found (each printed as FAIL).
#>
[CmdletBinding()]
param(
  [string]$InstallDir = 'C:\Program Files\DoodleRay'
)

$ErrorActionPreference = 'SilentlyContinue'
$failures = New-Object System.Collections.Generic.List[string]

function Assert-Clean([bool]$Ok, [string]$Label, [string]$Detail = '') {
  if ($Ok) { Write-Host ("PASS  {0}" -f $Label) -ForegroundColor Green }
  else {
    Write-Host ("FAIL  {0}  {1}" -f $Label, $Detail) -ForegroundColor Red
    $script:failures.Add($Label)
  }
}

# 1. Service removed (DoodleRayTunnelService)
$svc = Get-Service -Name 'DoodleRayTunnelService' -ErrorAction SilentlyContinue
Assert-Clean ($null -eq $svc) 'service DoodleRayTunnelService absent' ("state={0}" -f $svc.Status)

# 2. No orphan processes (app, service, engines, xray api statsquery)
$procNames = @('DoodleRay', 'DoodleRayService', 'xray', 'sing-box')
$running = Get-Process -Name $procNames -ErrorAction SilentlyContinue
Assert-Clean (-not $running) 'no DoodleRay/engine processes' (($running | ForEach-Object Name) -join ',')
$statsOrphans = @(Get-CimInstance Win32_Process -Filter "name = 'xray.exe'" |
  Where-Object { $_.CommandLine -match 'api\s+statsquery' }).Count
Assert-Clean ($statsOrphans -eq 0) 'no xray api statsquery orphans' "count=$statsOrphans"

# 3. WinINet proxy not left pointing at DoodleRay loopback ports
$wininet = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings'
$proxyDirty = ($wininet.ProxyEnable -eq 1 -and $wininet.ProxyServer -match '127\.0\.0\.1:(10808|10809|20808|20809)')
Assert-Clean (-not $proxyDirty) 'WinINet proxy clean' ("ProxyEnable={0} ProxyServer={1}" -f $wininet.ProxyEnable, $wininet.ProxyServer)
$staleServer = ($wininet.ProxyEnable -ne 1 -and $wininet.ProxyServer -match '127\.0\.0\.1:(10808|10809|20808|20809)')
Assert-Clean (-not $staleServer) 'no stale WinINet ProxyServer value' ("ProxyServer={0}" -f $wininet.ProxyServer)
$pacDirty = ($wininet.AutoConfigURL -match 'doodleray')
Assert-Clean (-not $pacDirty) 'no DoodleRay PAC/AutoConfigURL' ("AutoConfigURL={0}" -f $wininet.AutoConfigURL)

# 4. No DoodleRay NRPT rules
$nrpt = @(Get-DnsClientNrptRule | Where-Object { "$($_.Comment)$($_.DisplayName)" -match 'DoodleRay' })
Assert-Clean ($nrpt.Count -eq 0) 'no DoodleRay NRPT rules' "count=$($nrpt.Count)"

# 5. No DoodleRay tunnel adapter or routes bound to it
$adapters = @(Get-NetAdapter -Name 'DoodleRay*' -ErrorAction SilentlyContinue)
Assert-Clean ($adapters.Count -eq 0) 'no DoodleRay tunnel adapter' (($adapters | ForEach-Object Name) -join ',')
$routes = @(Get-NetRoute -InterfaceAlias 'DoodleRay*' -ErrorAction SilentlyContinue)
Assert-Clean ($routes.Count -eq 0) 'no routes on DoodleRay adapter' "count=$($routes.Count)"

# 6. Runtime marker consumed/removed
$marker = 'C:\ProgramData\DoodleRay\runtime\active-session.marker'
Assert-Clean (-not (Test-Path $marker)) 'no active-session.marker' $marker

# 7. App-owned scheduled tasks removed
$tasks = @(Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { $_.TaskName -match 'DoodleRay' })
Assert-Clean ($tasks.Count -eq 0) 'no DoodleRay scheduled tasks' (($tasks | ForEach-Object TaskName) -join ',')

# 8. Apps & Features entry gone
$uninstKeys = @('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*')
$entry = Get-ItemProperty $uninstKeys | Where-Object { $_.DisplayName -like 'DoodleRay*' }
Assert-Clean ($null -eq $entry) 'no Apps & Features entry' ($entry.DisplayName -join ',')

# 9. Install dir has no leftover executables (logs/config leftovers are warned, not failed)
if (Test-Path $InstallDir) {
  $peLeft = @(Get-ChildItem $InstallDir -Recurse -Include '*.exe', '*.dll' -ErrorAction SilentlyContinue)
  Assert-Clean ($peLeft.Count -eq 0) 'no leftover PE files in install dir' "count=$($peLeft.Count)"
  if ($peLeft.Count -eq 0) { Write-Warning "install dir still exists (non-PE leftovers): $InstallDir" }
} else {
  Write-Host "PASS  install dir removed" -ForegroundColor Green
}

if ($failures.Count -gt 0) {
  Write-Host ("`nUNCLEAN: {0} check(s) failed." -f $failures.Count) -ForegroundColor Red
  exit 1
}
Write-Host "`nCLEAN: machine has no DoodleRay leftovers." -ForegroundColor Green
exit 0
