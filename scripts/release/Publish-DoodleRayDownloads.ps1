<#
.SYNOPSIS
Publishes immutable DoodleRay release artifacts to the first-party downloads host.

.DESCRIPTION
Uploads a local artifact directory to:
  /srv/doodleray-downloads/public/releases/<channel>/<version>/
Then updates:
  /srv/doodleray-downloads/public/channels/<channel>/manifest.json
  /srv/doodleray-downloads/public/channels/<channel>/latest.json (when present)

The script never mutates existing versioned artifacts unless -Force is passed.
Use this for direct and store-win32 channels instead of GitHub Releases as CDN.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][ValidatePattern('^\d+\.\d+\.\d+([-.][A-Za-z0-9.]+)?$')]
  [string]$Version,

  [ValidateSet('direct', 'store-win32')]
  [string]$Channel = 'direct',

  [Parameter(Mandatory = $true)]
  [string]$ArtifactDir,

  [string]$HostName = 'doodleray.clickflare.click',
  [string]$User = 'root',
  [int]$Port = 22,
  [string]$RemoteRoot = '/srv/doodleray-downloads',
  [string]$SshKeyPath = $env:DOODLERAY_DOWNLOADS_SSH_KEY,
  [switch]$Force
)

$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $ArtifactDir)) { throw "ArtifactDir not found: $ArtifactDir" }

$repoRoot = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$work = Join-Path $repoRoot ".release-upload\$Channel-$Version"
Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $work | Out-Null

$patterns = @('*.exe', '*.msi', '*.zip', '*.dmg', '*.tar.gz', '*.sig', 'latest.json', 'latest-store-win32.json')
$files = foreach ($pattern in $patterns) {
  Get-ChildItem -LiteralPath $ArtifactDir -Filter $pattern -File -ErrorAction SilentlyContinue
}
$files = @($files | Sort-Object FullName -Unique)
if ($files.Count -eq 0) { throw "No releasable artifacts found in $ArtifactDir" }

foreach ($file in $files) {
  $name = if ($file.Name -eq 'latest-store-win32.json') { 'latest.json' } else { $file.Name }
  Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $work $name) -Force
}

$artifactRows = Get-ChildItem -LiteralPath $work -File | Sort-Object Name | ForEach-Object {
  [pscustomobject]@{
    name = $_.Name
    size = $_.Length
    sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    url = "https://$HostName/releases/$Channel/$Version/$($_.Name)"
  }
}
$artifactRows | ForEach-Object { "$($_.sha256)  $($_.name)" } |
  Set-Content -LiteralPath (Join-Path $work 'sha256.txt') -Encoding ascii

$manifest = [ordered]@{
  product = 'DoodleRay'
  version = $Version
  channel = $Channel
  createdAtUtc = (Get-Date).ToUniversalTime().ToString('o')
  immutableBaseUrl = "https://$HostName/releases/$Channel/$Version/"
  files = @($artifactRows)
}
$manifest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $work 'manifest.json') -Encoding utf8

$archive = Join-Path (Split-Path $work -Parent) "$Channel-$Version.tar.gz"
Remove-Item -LiteralPath $archive -Force -ErrorAction SilentlyContinue
tar -czf $archive -C $work .

$sshArgs = @('-p', $Port.ToString(), '-o', 'StrictHostKeyChecking=accept-new')
$scpArgs = @('-P', $Port.ToString(), '-o', 'StrictHostKeyChecking=accept-new')
if ($SshKeyPath) {
  $sshArgs += @('-i', $SshKeyPath)
  $scpArgs += @('-i', $SshKeyPath)
}
$remote = "$User@$HostName"
$releaseId = "$Channel-$Version"
$remoteArchive = "$RemoteRoot/staging/$releaseId.tar.gz"

Write-Host "Uploading $archive to ${remote}:$remoteArchive"
& ssh @sshArgs $remote "mkdir -p '$RemoteRoot/staging'"
if ($LASTEXITCODE -ne 0) { throw "ssh mkdir failed" }
& scp @scpArgs $archive "${remote}:$remoteArchive"
if ($LASTEXITCODE -ne 0) { throw "scp upload failed" }

$forceValue = if ($Force) { '1' } else { '0' }
$remoteScript = @"
set -euo pipefail
remote_root='$RemoteRoot'
channel='$Channel'
version='$Version'
archive='$remoteArchive'
force='$forceValue'
dest="`$remote_root/public/releases/`$channel/`$version"
tmp="`$dest.tmp.$$"
if [ -e "`$dest" ] && [ "`$force" != "1" ]; then
  echo "refusing to overwrite existing immutable release: `$dest" >&2
  exit 23
fi
rm -rf "`$tmp"
mkdir -p "`$tmp"
tar -xzf "`$archive" -C "`$tmp"
find "`$tmp" -type d -exec chmod 0755 {} +
find "`$tmp" -type f -exec chmod 0644 {} +
if [ -e "`$dest" ]; then
  rm -rf "`$dest"
fi
mv "`$tmp" "`$dest"
mkdir -p "`$remote_root/public/channels/`$channel"
cp "`$dest/manifest.json" "`$remote_root/public/channels/`$channel/manifest.json"
if [ -f "`$dest/latest.json" ]; then
  cp "`$dest/latest.json" "`$remote_root/public/channels/`$channel/latest.json"
fi
ln -sfn "../../releases/`$channel/`$version" "`$remote_root/public/channels/`$channel/current"
rm -f "`$archive"
echo "published `$channel `$version"
"@
& ssh @sshArgs $remote $remoteScript
if ($LASTEXITCODE -ne 0) { throw "remote publish failed" }

$manifestUrl = "https://$HostName/channels/$Channel/manifest.json"
Write-Host "Verifying $manifestUrl"
$response = Invoke-WebRequest -Uri $manifestUrl -UseBasicParsing -TimeoutSec 30
if ($response.StatusCode -lt 200 -or $response.StatusCode -ge 300) {
  throw "manifest verification failed: HTTP $($response.StatusCode)"
}

Write-Host "Published DoodleRay $Version ($Channel)." -ForegroundColor Green
Write-Host "Manifest: $manifestUrl"
Write-Host "Base URL: https://$HostName/releases/$Channel/$Version/"
