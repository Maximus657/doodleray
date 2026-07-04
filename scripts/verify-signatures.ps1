<#
.SYNOPSIS
Verifies Authenticode signatures on every DoodleRay PE artifact (verify-only).

.DESCRIPTION
Release/CI gate: exits non-zero if any required PE is missing or not validly
signed. Never fake-passes: absence of a certificate does not soften the
verdict — unsigned is unsigned.

Required set (repo-relative):
  src-tauri\DoodleRayService.exe   (DoodleRayTunnelService binary)
  src-tauri\sing-box.exe
  src-tauri\wintun.dll
  src-tauri\xray-core\xray.exe
  src-tauri\xray-core\wintun.dll
Optional additions:
  -IncludeBuiltApp   src-tauri\target\release\DoodleRay.exe
  -InstallerPath     a built NSIS installer exe
  -ExtraPaths        any additional PE files (helpers/updaters)
#>
[CmdletBinding()]
param(
  [switch]$IncludeBuiltApp,
  [string]$InstallerPath,
  [string[]]$ExtraPaths = @()
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent

$required = @(
  'src-tauri\DoodleRayService.exe',
  'src-tauri\sing-box.exe',
  'src-tauri\wintun.dll',
  'src-tauri\xray-core\xray.exe',
  'src-tauri\xray-core\wintun.dll'
) | ForEach-Object { Join-Path $repoRoot $_ }

if ($IncludeBuiltApp) { $required += (Join-Path $repoRoot 'src-tauri\target\release\DoodleRay.exe') }
if ($InstallerPath)   { $required += $InstallerPath }
$required += $ExtraPaths

$failures = @()
$results = foreach ($path in $required) {
  if (-not (Test-Path -LiteralPath $path)) {
    $failures += $path
    [pscustomobject]@{ File = $path; Status = 'MISSING'; Signer = '-'; Thumbprint = '-' }
    continue
  }
  $sig = Get-AuthenticodeSignature -LiteralPath $path
  $ok = $sig.Status -eq 'Valid'
  if (-not $ok) { $failures += $path }
  [pscustomobject]@{
    File       = $path
    Status     = if ($ok) { 'SIGNED' } else { "UNSIGNED ($($sig.Status))" }
    Signer     = if ($sig.SignerCertificate) { $sig.SignerCertificate.Subject } else { '-' }
    Thumbprint = if ($sig.SignerCertificate) { $sig.SignerCertificate.Thumbprint } else { '-' }
  }
}

$results | Format-Table -AutoSize | Out-String | Write-Host

if ($failures.Count -gt 0) {
  Write-Host "FAIL: $($failures.Count) PE artifact(s) missing or unsigned:" -ForegroundColor Red
  $failures | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
  Write-Host @'
Action required:
  1. Ensure all vendored binaries (sing-box, xray, wintun) are the official signed releases.
  2. Sign first-party binaries: set WINDOWS_CODESIGN_THUMBPRINT (cert in Windows cert store),
     then run scripts\sign-all-pe.ps1.
  3. Re-run this script. Release/Store gate stays CLOSED until it passes.
'@
  exit 1
}

Write-Host 'OK: all required PE artifacts are validly signed.' -ForegroundColor Green
exit 0
