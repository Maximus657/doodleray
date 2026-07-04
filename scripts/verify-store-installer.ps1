<#
.SYNOPSIS
Validates the Store-flavor NSIS installer end-to-end (QA stand / clean VM ONLY).

.DESCRIPTION
MUTATES THE MACHINE (silent install, optional uninstall). Never run on a dev
box — use the Play2Go stand or a disposable Windows VM, per
docs/windows-pc-qa-play2go.md. Requires -Force to acknowledge that.

Checks (Partner Center Win32 EXE requirements):
  1. Installer file is Authenticode-signed.
  2. Silent install works with /S and returns 0.
  3. Apps & Features metadata (DisplayName/DisplayVersion/Publisher/uninstall string).
  4. Start Menu shortcut present.
  5. DoodleRayTunnelService registered.
  6. WebView2 runtime present (offlineInstaller mode must not need network).
  7. Installed PE files are signed.
  8. Optional -UninstallAfter: silent uninstall + scripts\verify-clean-uninstall.ps1.

Exit 0 = all checks passed.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$InstallerPath,
  [switch]$Force,
  [switch]$UninstallAfter,
  [string]$InstallDir = 'C:\Program Files\DoodleRay',
  [int]$InstallTimeoutSec = 600
)

$ErrorActionPreference = 'Stop'
if (-not $Force) {
  throw 'This script installs/uninstalls DoodleRay on THIS machine. Run on the QA stand or a clean VM and pass -Force to confirm.'
}
if (-not (Test-Path -LiteralPath $InstallerPath)) { throw "Installer not found: $InstallerPath" }

$failures = New-Object System.Collections.Generic.List[string]
function Step([bool]$Ok, [string]$Label, [string]$Detail = '') {
  if ($Ok) { Write-Host ("PASS  {0}" -f $Label) -ForegroundColor Green }
  else {
    Write-Host ("FAIL  {0}  {1}" -f $Label, $Detail) -ForegroundColor Red
    $script:failures.Add($Label)
  }
}

# 1. Installer signature
$sig = Get-AuthenticodeSignature -LiteralPath $InstallerPath
Step ($sig.Status -eq 'Valid') 'installer is Authenticode-signed' ("status={0}" -f $sig.Status)

# 2. Silent install /S
Write-Host "installing silently: $InstallerPath /S"
$p = Start-Process -FilePath $InstallerPath -ArgumentList '/S' -PassThru -Wait
Step ($p.ExitCode -eq 0) 'silent install exit code 0' ("exit={0}" -f $p.ExitCode)
# NSIS /S returns before per-machine work fully settles; wait for the service.
$deadline = (Get-Date).AddSeconds($InstallTimeoutSec)
do {
  $svc = Get-Service -Name 'DoodleRayTunnelService' -ErrorAction SilentlyContinue
  if ($svc) { break }
  Start-Sleep -Seconds 2
} while ((Get-Date) -lt $deadline)

# 3. Apps & Features metadata
$uninstKeys = @('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*')
$entry = Get-ItemProperty $uninstKeys -ErrorAction SilentlyContinue |
  Where-Object { $_.DisplayName -like 'DoodleRay*' } | Select-Object -First 1
Step ($null -ne $entry) 'Apps & Features entry present'
if ($entry) {
  Step (-not [string]::IsNullOrWhiteSpace($entry.DisplayVersion)) 'DisplayVersion set' $entry.DisplayVersion
  Step (-not [string]::IsNullOrWhiteSpace($entry.Publisher)) 'Publisher set' $entry.Publisher
  Step (-not [string]::IsNullOrWhiteSpace($entry.UninstallString)) 'UninstallString set'
}

# 4. Start Menu shortcut
$startMenu = @(
  "$env:ProgramData\Microsoft\Windows\Start Menu\Programs",
  "$env:APPDATA\Microsoft\Windows\Start Menu\Programs"
) | ForEach-Object { Get-ChildItem $_ -Recurse -Filter 'DoodleRay*.lnk' -ErrorAction SilentlyContinue } |
  Select-Object -First 1
Step ($null -ne $startMenu) 'Start Menu shortcut present' $startMenu.FullName

# 5. Service registered
$svc = Get-Service -Name 'DoodleRayTunnelService' -ErrorAction SilentlyContinue
Step ($null -ne $svc) 'DoodleRayTunnelService registered' ("state={0}" -f $svc.Status)

# 6. WebView2 runtime present (offline installer must have delivered it)
$wv2 = Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' -ErrorAction SilentlyContinue
if (-not $wv2) { $wv2 = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' -ErrorAction SilentlyContinue }
Step ($null -ne $wv2 -and $wv2.pv) 'WebView2 runtime present' ("pv={0}" -f $wv2.pv)

# 7. Installed PE files signed
$installedPe = @(Get-ChildItem $InstallDir -Recurse -Include '*.exe', '*.dll' -ErrorAction SilentlyContinue)
Step ($installedPe.Count -gt 0) 'installed PE files found' "count=$($installedPe.Count)"
$unsigned = @($installedPe | Where-Object { (Get-AuthenticodeSignature $_.FullName).Status -ne 'Valid' })
Step ($unsigned.Count -eq 0) 'all installed PE files signed' (($unsigned | Select-Object -First 5 | ForEach-Object Name) -join ',')

# 8. Optional silent uninstall + clean-machine verification
if ($UninstallAfter) {
  $uninst = $entry.QuietUninstallString
  if ([string]::IsNullOrWhiteSpace($uninst)) { $uninst = "$($entry.UninstallString) /S" }
  Write-Host "uninstalling silently: $uninst"
  # UninstallString may be quoted; run via cmd for faithful parsing.
  $u = Start-Process -FilePath 'cmd.exe' -ArgumentList '/c', $uninst -PassThru -Wait
  Step ($u.ExitCode -eq 0) 'silent uninstall exit code 0' ("exit={0}" -f $u.ExitCode)
  Start-Sleep -Seconds 10
  & (Join-Path $PSScriptRoot 'verify-clean-uninstall.ps1') -InstallDir $InstallDir
  Step ($LASTEXITCODE -eq 0) 'clean-uninstall verification'
}

if ($failures.Count -gt 0) {
  Write-Host ("`nFAILED: {0} check(s)." -f $failures.Count) -ForegroundColor Red
  exit 1
}
Write-Host "`nAll store-installer checks passed." -ForegroundColor Green
exit 0
