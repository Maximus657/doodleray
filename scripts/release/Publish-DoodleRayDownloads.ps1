<#
.SYNOPSIS
Uploads an immutable direct-channel release or atomically promotes its updater manifest.

.DESCRIPTION
UploadImmutable is idempotent: same hashes are a no-op; different hashes for an
existing version hard fail. PromoteLatest is separate so latest.json can be the
last production mutation after App Store upload and GitHub Release publication.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][ValidatePattern('^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$')]
  [string]$Version,
  [Parameter(Mandatory = $true)][ValidateSet('UploadImmutable', 'PromoteLatest')]
  [string]$Mode,
  [string]$ArtifactDir,
  [ValidatePattern('^(?=.{1,253}$)(?:[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.)*[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?$')]
  [string]$HostName = 'doodleray.clickflare.click',
  [ValidatePattern('^[A-Za-z_][A-Za-z0-9_-]{0,31}$')]
  [string]$User = 'root',
  [ValidateRange(1, 65535)]
  [int]$Port = 22,
  [ValidatePattern('^/(?:[A-Za-z0-9._-]+/)*[A-Za-z0-9._-]+$')]
  [string]$RemoteRoot = '/srv/doodleray-downloads',
  [string]$SshKeyPath = $env:DOODLERAY_DOWNLOADS_SSH_KEY,
  [string]$SshKnownHostsPath = $env:DOODLERAY_DOWNLOADS_SSH_KNOWN_HOSTS,
  [ValidatePattern('^(?=.{1,253}$)(?:[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.)*[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?$')]
  [string]$PublicHostName = 'doodleray.clickflare.click'
)

$ErrorActionPreference = 'Stop'
if (-not $SshKeyPath -or -not (Test-Path -LiteralPath $SshKeyPath)) { throw 'A readable SSH key is required' }
if (-not $SshKnownHostsPath -or -not (Test-Path -LiteralPath $SshKnownHostsPath)) { throw 'A pinned SSH known-hosts file is required' }
if ($RemoteRoot.Split('/') | Where-Object { $_ -eq '.' -or $_ -eq '..' }) { throw 'RemoteRoot must contain only safe absolute path segments' }

function ConvertTo-PosixSingleQuotedLiteral([string]$Value) {
  $quote = [string][char]39
  $escapedQuote = $quote + [char]34 + $quote + [char]34 + $quote
  return $quote + $Value.Replace($quote, $escapedQuote) + $quote
}

$hostKeyArgs = @('-F', '/dev/null', '-o', 'BatchMode=yes', '-o', 'IdentitiesOnly=yes', '-o', 'StrictHostKeyChecking=yes', '-o', "UserKnownHostsFile=$SshKnownHostsPath", '-o', 'GlobalKnownHostsFile=/dev/null')
$sshArgs = @('-p', $Port.ToString()) + $hostKeyArgs + @('-i', $SshKeyPath)
$scpArgs = @('-P', $Port.ToString()) + $hostKeyArgs + @('-i', $SshKeyPath)
$remote = "$User@$HostName"
$destination = "$RemoteRoot/public/releases/direct/$Version"
$destinationLiteral = ConvertTo-PosixSingleQuotedLiteral $destination

if ($Mode -eq 'UploadImmutable') {
  if (-not $ArtifactDir -or -not (Test-Path -LiteralPath $ArtifactDir)) { throw 'ArtifactDir is required for UploadImmutable' }
  foreach ($name in @('latest.json', 'provenance.json', 'sha256.txt')) {
    if (-not (Test-Path -LiteralPath (Join-Path $ArtifactDir $name))) { throw "Required artifact is missing: $name" }
  }
  Push-Location $ArtifactDir
  try {
    Get-Content -LiteralPath 'sha256.txt' | ForEach-Object {
      if ($_ -notmatch '^[0-9a-f]{64}  (.+)$') { throw "Invalid sha256.txt row: $_" }
      $path = $Matches[1]
      if (-not (Test-Path -LiteralPath $path)) { throw "sha256.txt references a missing file: $path" }
      $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
      if ($actual -ne $_.Substring(0, 64)) { throw "Local artifact hash mismatch: $path" }
    }
    $archive = Join-Path $env:RUNNER_TEMP "doodleray-direct-$Version.tar.gz"
    Remove-Item -LiteralPath $archive -Force -ErrorAction SilentlyContinue
    tar -czf $archive .
  } finally {
    Pop-Location
  }

  $remoteArchive = "$RemoteRoot/staging/direct-$Version-$([guid]::NewGuid().ToString('N')).tar.gz"
  $remoteArchiveLiteral = ConvertTo-PosixSingleQuotedLiteral $remoteArchive
  $stagingLiteral = ConvertTo-PosixSingleQuotedLiteral "$RemoteRoot/staging"
  & ssh @sshArgs $remote "mkdir -p $stagingLiteral"
  if ($LASTEXITCODE -ne 0) { throw 'ssh staging setup failed' }
  & scp @scpArgs $archive "${remote}:$remoteArchive"
  if ($LASTEXITCODE -ne 0) { throw 'artifact upload failed' }

  $remoteScript = @"
set -euo pipefail
archive=$remoteArchiveLiteral
dest=$destinationLiteral
tmp="`$dest.tmp.$$"
verify_release_dir() {
  local dir="`$1"
  (cd "`$dir" &&
    sha256sum -c sha256.txt &&
    diff -u <(sed -E 's/^[0-9a-f]{64}  //' sha256.txt | sort) <(find . -maxdepth 1 -type f ! -name sha256.txt -printf '%f\n' | sort))
}
trap 'rm -rf "`$tmp" "`$archive"' EXIT
mkdir -p "`$tmp"
tar -xzf "`$archive" -C "`$tmp"
verify_release_dir "`$tmp"
if [ -e "`$dest" ]; then
  if cmp -s "`$tmp/sha256.txt" "`$dest/sha256.txt" && verify_release_dir "`$dest"; then
    echo 'same hashes: immutable release is a no-op'
    exit 0
  fi
  echo 'different hashes: refusing to overwrite immutable release' >&2
  exit 23
fi
mkdir -p "`$(dirname "`$dest")"
find "`$tmp" -type d -exec chmod 0755 {} +
find "`$tmp" -type f -exec chmod 0644 {} +
mv "`$tmp" "`$dest"
echo 'immutable release uploaded; latest.json was not promoted'
"@ -replace "`r`n", "`n"
  & ssh @sshArgs $remote $remoteScript
  if ($LASTEXITCODE -ne 0) { throw "immutable publish failed with exit code $LASTEXITCODE" }
  exit 0
}

$promoteScript = @"
set -euo pipefail
dest=$destinationLiteral
channel=$(ConvertTo-PosixSingleQuotedLiteral "$RemoteRoot/public/channels/direct")
test -f "`$dest/latest.json"
test -f "`$dest/provenance.json"
(cd "`$dest" && sha256sum -c sha256.txt)
mkdir -p "`$channel"
cp "`$dest/provenance.json" "`$channel/manifest.json.tmp.$$"
mv "`$channel/manifest.json.tmp.$$" "`$channel/manifest.json"
ln -sfn $(ConvertTo-PosixSingleQuotedLiteral "../../releases/direct/$Version") "`$channel/current"
cp "`$dest/latest.json" "`$channel/latest.json.tmp.$$"
mv "`$channel/latest.json.tmp.$$" "`$channel/latest.json"
echo 'latest.json promoted last'
"@ -replace "`r`n", "`n"
& ssh @sshArgs $remote $promoteScript
if ($LASTEXITCODE -ne 0) { throw 'latest.json promotion failed' }

$latestUrl = "https://$PublicHostName/channels/direct/latest.json"
$latest = Invoke-RestMethod -Uri $latestUrl -TimeoutSec 30
if ([string]$latest.version -ne $Version) { throw "Promoted updater version mismatch at $latestUrl" }
Write-Host "Promoted and verified DoodleRay ${Version}: $latestUrl"
