<#
.SYNOPSIS
Builds the Microsoft Store (Win32 EXE) flavor of DoodleRay.

.DESCRIPTION
Produces an NSIS EXE installer suitable for Partner Center Win32 EXE
submission, using src-tauri/tauri.microsoftstore.conf.json merged over the
base config. The store flavor:
- keeps WebView2 offlineInstaller (inherited from base config);
- bakes the store-win32 update channel into the frontend (in-app self-update
  disabled by default; UI opens the Store/support page instead);
- produces no direct-channel (GitHub latest.json) updater artifacts.

Signing: by default signing is REQUIRED (fail-closed). Use -AllowUnsigned for
a local RC-only smoke build. Production Store submissions must be signed CI
builds; unsigned output is never Store-submittable.

.PARAMETER AllowUnsigned
Local smoke build without Authenticode signing. Output is RC-only.

.PARAMETER EnableSelfUpdate
Bakes VITE_DOODLERAY_STORE_SELF_UPDATE=1 so the Store build performs signed,
user-initiated in-app updates from the store-win32 channel. Implies
-WithUpdaterArtifacts is usually needed on the release that feeds the channel.

.PARAMETER WithUpdaterArtifacts
Re-enables updater artifacts (zip+sig) for publishing the explicit
store-win32 channel manifest. Never publish these to the direct latest.json.

.PARAMETER StoreFallbackUrl
URL opened by the UI for user-initiated updates when self-update is disabled.
Replace with the Microsoft Store PDP link after listing.
#>
[CmdletBinding()]
param(
  [switch]$AllowUnsigned,
  [switch]$EnableSelfUpdate,
  [switch]$WithUpdaterArtifacts,
  [string]$StoreFallbackUrl = 'https://t.me/doodlevpn_support',
  [string]$OutDir = 'dist-store'
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
Set-Location $repoRoot

# --- Channel env (baked into the frontend by Vite during beforeBuildCommand) ---
$env:VITE_DOODLERAY_UPDATE_CHANNEL = 'store-win32'
$env:VITE_DOODLERAY_STORE_SELF_UPDATE = if ($EnableSelfUpdate) { '1' } else { '0' }
$env:VITE_DOODLERAY_STORE_FALLBACK_URL = $StoreFallbackUrl

# --- Signing policy: fail closed unless explicitly waived ---
if ($AllowUnsigned) {
  Write-Warning 'Building UNSIGNED store flavor: RC-only, NOT submittable to Partner Center.'
  $env:WINDOWS_CODESIGN_REQUIRED = 'false'
} else {
  $env:WINDOWS_CODESIGN_REQUIRED = 'true'
  if ([string]::IsNullOrWhiteSpace($env:WINDOWS_CODESIGN_THUMBPRINT)) {
    throw ('WINDOWS_CODESIGN_THUMBPRINT is not set. Store builds must be signed. ' +
      'Set the certificate thumbprint (cert must be in the machine/user store) or pass -AllowUnsigned for a local RC-only smoke build.')
  }
}

$configArgs = @('--config', 'src-tauri/tauri.microsoftstore.conf.json')
if ($WithUpdaterArtifacts) {
  # Inline override merged last: produce updater artifacts for the explicit
  # store-win32 channel (publish only to latest-store-win32.json).
  $configArgs += @('--config', '{"bundle":{"createUpdaterArtifacts":"v1Compatible"}}')
}

Write-Host "== DoodleRay store-win32 build ==" -ForegroundColor Cyan
Write-Host ("channel=store-win32 selfUpdate={0} signedRequired={1}" -f $env:VITE_DOODLERAY_STORE_SELF_UPDATE, $env:WINDOWS_CODESIGN_REQUIRED)

npx tauri build --bundles nsis @configArgs
if ($LASTEXITCODE -ne 0) { throw "tauri build failed with exit code $LASTEXITCODE" }

# --- Collect installer ---
$bundleDir = Join-Path $repoRoot 'src-tauri\target\release\bundle\nsis'
$installer = Get-ChildItem $bundleDir -Filter '*.exe' -ErrorAction Stop |
  Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $installer) { throw "No NSIS installer found in $bundleDir" }

$version = (Get-Content (Join-Path $repoRoot 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
$outPath = Join-Path $repoRoot $OutDir
New-Item -ItemType Directory -Force $outPath | Out-Null
# Immutable, versioned artifact name for the Partner Center HTTPS URL.
$dest = Join-Path $outPath ("DoodleRay-store-win32-{0}-x64-setup.exe" -f $version)
Copy-Item $installer.FullName $dest -Force

$hash = (Get-FileHash $dest -Algorithm SHA256).Hash
Write-Host "`nInstaller: $dest"
Write-Host "SHA256:    $hash"
Write-Host "Silent install parameter for Partner Center: /S"

# --- Verify signatures unless explicitly waived ---
if (-not $AllowUnsigned) {
  & (Join-Path $PSScriptRoot 'verify-signatures.ps1') -InstallerPath $dest -IncludeBuiltApp
  if ($LASTEXITCODE -ne 0) { throw 'Signature verification failed; store artifact is not submittable.' }
}

Write-Host "`nStore build complete." -ForegroundColor Green
