$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$bundle = Join-Path $env:TEMP "doodleray-release-fixture-$([guid]::NewGuid().ToString('N'))"
$output = Join-Path $repoRoot 'windows-release'

if (Test-Path -LiteralPath $output) { throw "Test staging path must not already exist: $output" }
New-Item -ItemType Directory -Path $bundle | Out-Null
try {
  Set-Content -LiteralPath (Join-Path $bundle 'DoodleRay_6.0.2_x64-setup.exe') -Value 'installer' -NoNewline
  Set-Content -LiteralPath (Join-Path $bundle 'DoodleRay_6.0.2_x64-setup.nsis.zip') -Value 'updater' -NoNewline
  Set-Content -LiteralPath (Join-Path $bundle 'DoodleRay_6.0.2_x64-setup.nsis.zip.sig') -Value 'updater-signature' -NoNewline

  & (Join-Path $PSScriptRoot 'Prepare-WindowsRelease.ps1') `
    -SourceSha (git -C $repoRoot rev-parse HEAD).Trim() `
    -BundleDir $bundle `
    -OutputDir $output

  $latest = Get-Content (Join-Path $output 'latest.json') -Raw | ConvertFrom-Json
  if (@($latest.platforms.psobject.Properties).Count -ne 1) { throw 'latest.json must contain exactly one Windows updater target' }
  if ($latest.platforms.'windows-x86_64'.url -notlike '*.nsis.zip') { throw 'Windows updater must use the signed NSIS updater archive' }
  if (-not (Test-Path (Join-Path $output 'sha256.txt'))) { throw 'SHA-256 inventory is missing' }

  $unsafeOutput = Join-Path $env:TEMP "unsafe-release-output-$([guid]::NewGuid().ToString('N'))"
  try {
    & (Join-Path $PSScriptRoot 'Prepare-WindowsRelease.ps1') `
      -SourceSha (git -C $repoRoot rev-parse HEAD).Trim() `
      -BundleDir $bundle `
      -OutputDir $unsafeOutput
    throw 'Unsafe output directory was accepted'
  } catch {
    if ($_.Exception.Message -eq 'Unsafe output directory was accepted') { throw }
  }
} finally {
  Remove-Item -LiteralPath $bundle -Recurse -Force -ErrorAction SilentlyContinue
  if ([IO.Path]::GetFullPath($output) -eq [IO.Path]::GetFullPath((Join-Path $repoRoot 'windows-release'))) {
    Remove-Item -LiteralPath $output -Recurse -Force -ErrorAction SilentlyContinue
  }
}

Write-Host 'Prepare-WindowsRelease tests passed.'
