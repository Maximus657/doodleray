$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$bundle = Join-Path $env:TEMP "doodleray-release-fixture-$([guid]::NewGuid().ToString('N'))"
$config = Join-Path $env:TEMP "doodleray-tauri-fixture-$([guid]::NewGuid().ToString('N')).json"
$output = Join-Path $repoRoot 'windows-release'

if (Test-Path -LiteralPath $output) { throw "Test staging path must not already exist: $output" }
New-Item -ItemType Directory -Path $bundle | Out-Null
try {
  Set-Content -LiteralPath (Join-Path $bundle 'DoodleRay_6.0.2_x64-setup.exe') -Value 'installer' -NoNewline
  Set-Content -LiteralPath (Join-Path $bundle 'DoodleRay_6.0.2_x64-setup.nsis.zip') -Value 'test' -NoNewline -Encoding utf8NoBOM
  $publicKey = "untrusted comment: minisign public key E7620F1842B4E81F`nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3"
  $signature = "untrusted comment: signature from minisign secret key`nRUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=`ntrusted comment: timestamp:1556193335`tfile:test`ny/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg=="
  $publicKeyOuter = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($publicKey))
  $signatureOuter = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($signature))
  Set-Content -LiteralPath (Join-Path $bundle 'DoodleRay_6.0.2_x64-setup.nsis.zip.sig') -Value $signatureOuter -NoNewline -Encoding ascii
  @{ plugins = @{ updater = @{ pubkey = $publicKeyOuter } } } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $config -Encoding utf8

  & (Join-Path $PSScriptRoot 'Prepare-WindowsRelease.ps1') `
    -SourceSha (git -C $repoRoot rev-parse HEAD).Trim() `
    -BundleDir $bundle `
    -OutputDir $output `
    -TauriConfigPath $config

  $latest = Get-Content (Join-Path $output 'latest.json') -Raw | ConvertFrom-Json
  if (@($latest.platforms.psobject.Properties).Count -ne 1) { throw 'latest.json must contain exactly one Windows updater target' }
  if ($latest.platforms.'windows-x86_64'.url -notlike '*.nsis.zip') { throw 'Windows updater must use the signed NSIS updater archive' }
  if (-not (Test-Path (Join-Path $output 'sha256.txt'))) { throw 'SHA-256 inventory is missing' }

  $unsafeOutput = Join-Path $env:TEMP "unsafe-release-output-$([guid]::NewGuid().ToString('N'))"
  try {
    & (Join-Path $PSScriptRoot 'Prepare-WindowsRelease.ps1') `
      -SourceSha (git -C $repoRoot rev-parse HEAD).Trim() `
      -BundleDir $bundle `
      -OutputDir $unsafeOutput `
      -TauriConfigPath $config
    throw 'Unsafe output directory was accepted'
  } catch {
    if ($_.Exception.Message -eq 'Unsafe output directory was accepted') { throw }
  }
} finally {
  Remove-Item -LiteralPath $bundle -Recurse -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $config -Force -ErrorAction SilentlyContinue
  if ([IO.Path]::GetFullPath($output) -eq [IO.Path]::GetFullPath((Join-Path $repoRoot 'windows-release'))) {
    Remove-Item -LiteralPath $output -Recurse -Force -ErrorAction SilentlyContinue
  }
}

Write-Host 'Prepare-WindowsRelease tests passed.'
