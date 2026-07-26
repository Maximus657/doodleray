[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$SourceSha,
  [Parameter(Mandatory = $true)][string]$BundleDir,
  [Parameter(Mandatory = $true)][string]$OutputDir,
  [string]$TauriConfigPath,
  [string]$PublicHostName = 'doodleray.clickflare.click'
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$tauriConfigCandidate = if ([string]::IsNullOrWhiteSpace($TauriConfigPath)) {
  Join-Path $repoRoot 'src-tauri/tauri.conf.json'
} elseif ([IO.Path]::IsPathRooted($TauriConfigPath)) {
  $TauriConfigPath
} else {
  Join-Path $repoRoot $TauriConfigPath
}
$tauriConfigFullPath = [IO.Path]::GetFullPath($tauriConfigCandidate)
$release = Get-Content (Join-Path $repoRoot 'release/release.json') -Raw | ConvertFrom-Json
$version = [string]$release.version
$outputCandidate = if ([IO.Path]::IsPathRooted($OutputDir)) { $OutputDir } else { Join-Path (Get-Location) $OutputDir }
$outputFullPath = [IO.Path]::GetFullPath($outputCandidate)
$approvedOutput = [IO.Path]::GetFullPath((Join-Path $repoRoot 'windows-release'))

if ($SourceSha -notmatch '^[0-9a-f]{40}$') { throw 'SourceSha must be an exact lowercase commit SHA' }
if (-not (Test-Path -LiteralPath $BundleDir)) { throw "BundleDir not found: $BundleDir" }
if (-not (Test-Path -LiteralPath $tauriConfigFullPath -PathType Leaf)) { throw "Tauri config not found: $tauriConfigFullPath" }
if ($outputFullPath -ne $approvedOutput) { throw "OutputDir must be the dedicated repository staging directory: $approvedOutput" }

$installer = @(Get-ChildItem -LiteralPath $BundleDir -File -Filter '*.exe')
$updater = @(Get-ChildItem -LiteralPath $BundleDir -File -Filter '*.nsis.zip')
$updaterSignature = @(Get-ChildItem -LiteralPath $BundleDir -File -Filter '*.nsis.zip.sig')
foreach ($entry in @(
  @{ Name = 'installer'; Files = $installer },
  @{ Name = 'updater archive'; Files = $updater },
  @{ Name = 'updater archive signature'; Files = $updaterSignature }
)) {
  if ($entry.Files.Count -ne 1) { throw "Expected exactly one $($entry.Name), found $($entry.Files.Count)" }
}

& cargo run --quiet --locked --manifest-path (Join-Path $repoRoot 'src-tauri/Cargo.toml') `
  --example verify_updater_signature -- `
  $updater[0].FullName $updaterSignature[0].FullName $tauriConfigFullPath
if ($LASTEXITCODE -ne 0) { throw 'Updater signature verification failed before release staging' }

if (Test-Path -LiteralPath $outputFullPath) {
  if (Get-ChildItem -LiteralPath $outputFullPath -Force | Select-Object -First 1) { throw 'Windows release staging directory must be empty' }
} else {
  New-Item -ItemType Directory -Path $outputFullPath | Out-Null
}
$OutputDir = $outputFullPath
foreach ($file in @($installer[0], $updater[0], $updaterSignature[0])) {
  Copy-Item -LiteralPath $file.FullName -Destination $OutputDir
}

$pubDate = (git -C $repoRoot show -s --format=%cI $SourceSha).Trim()
if ($LASTEXITCODE -ne 0 -or -not $pubDate) { throw 'Could not resolve deterministic release timestamp from SourceSha' }
$baseUrl = "https://$PublicHostName/releases/direct/$version"
$latest = [ordered]@{
  version = $version
  notes = "DoodleRay $version"
  pub_date = $pubDate
  platforms = [ordered]@{
    'windows-x86_64' = [ordered]@{
      signature = (Get-Content -LiteralPath $updaterSignature[0].FullName -Raw).Trim()
      url = "$baseUrl/$($updater[0].Name)"
    }
  }
}
$latest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $OutputDir 'latest.json') -Encoding utf8

$payloadFiles = @(Get-ChildItem -LiteralPath $OutputDir -File | Sort-Object Name)
$payload = @($payloadFiles | ForEach-Object {
  [ordered]@{
    name = $_.Name
    size = $_.Length
    sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    url = "$baseUrl/$($_.Name)"
  }
})
$provenance = [ordered]@{
  product = 'DoodleRay'
  version = $version
  channel = 'direct'
  sourceSha = $SourceSha
  sourceDate = $pubDate
  immutableBaseUrl = "$baseUrl/"
  files = $payload
}
$provenance | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $OutputDir 'provenance.json') -Encoding utf8

Get-ChildItem -LiteralPath $OutputDir -File | Sort-Object Name | ForEach-Object {
  "{0}  {1}" -f (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $_.Name
} | Set-Content -LiteralPath (Join-Path $OutputDir 'sha256.txt') -Encoding ascii

Write-Host "Prepared immutable Windows release set for $version at $SourceSha"
