<#
.SYNOPSIS
Signs every first-party DoodleRay PE artifact via the repo signing helper.

.DESCRIPTION
Wraps src-tauri\sign-windows-if-configured.ps1 (signtool /sha <thumbprint>,
SHA256 + RFC3161 timestamp) over the full PE set. Fails closed: if
WINDOWS_CODESIGN_THUMBPRINT is not set this script throws with an actionable
message — it never fake-passes.

Vendored third-party binaries (sing-box.exe, xray.exe, wintun.dll) normally
ship with the vendor's valid signature; they are only re-signed when you pass
-IncludeVendored (e.g. org policy requires our chain on every PE). Already
validly-signed files are skipped unless -Force.

Use scripts\verify-signatures.ps1 afterwards as the gate.
#>
[CmdletBinding()]
param(
  [switch]$IncludeVendored,
  [switch]$IncludeBuiltApp,
  [string]$InstallerPath,
  [string[]]$ExtraPaths = @(),
  [switch]$Force
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
$signer = Join-Path $repoRoot 'src-tauri\sign-windows-if-configured.ps1'

if ([string]::IsNullOrWhiteSpace($env:WINDOWS_CODESIGN_THUMBPRINT)) {
  throw ('WINDOWS_CODESIGN_THUMBPRINT is not set. Import the code-signing certificate into the ' +
    'Windows certificate store and set its thumbprint. This script never fake-passes without a cert.')
}
$env:WINDOWS_CODESIGN_REQUIRED = 'true'

$targets = @((Join-Path $repoRoot 'src-tauri\DoodleRayService.exe'))
if ($IncludeVendored) {
  $targets += @(
    (Join-Path $repoRoot 'src-tauri\sing-box.exe'),
    (Join-Path $repoRoot 'src-tauri\wintun.dll'),
    (Join-Path $repoRoot 'src-tauri\xray-core\xray.exe'),
    (Join-Path $repoRoot 'src-tauri\xray-core\wintun.dll')
  )
}
if ($IncludeBuiltApp) { $targets += (Join-Path $repoRoot 'src-tauri\target\release\DoodleRay.exe') }
if ($InstallerPath)   { $targets += $InstallerPath }
$targets += $ExtraPaths

$signed = 0; $skipped = 0
foreach ($path in $targets) {
  if (-not (Test-Path -LiteralPath $path)) { throw "Cannot sign missing file: $path" }
  if (-not $Force) {
    $sig = Get-AuthenticodeSignature -LiteralPath $path
    if ($sig.Status -eq 'Valid') {
      Write-Host "skip (already signed): $path"
      $skipped++
      continue
    }
  }
  Write-Host "signing: $path"
  & $signer -Path $path
  $signed++
}

Write-Host ("Done. signed={0} skipped={1}. Run scripts\verify-signatures.ps1 as the gate." -f $signed, $skipped) -ForegroundColor Green
