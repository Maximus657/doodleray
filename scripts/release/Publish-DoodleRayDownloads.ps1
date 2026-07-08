<#
.SYNOPSIS
Publishes immutable DoodleRay release artifacts to the first-party downloads host.

.DESCRIPTION
Uploads a local artifact directory to:
  /srv/doodleray-downloads/public/releases/<channel>/<version>/
Then updates:
  /srv/doodleray-downloads/public/channels/<channel>/manifest.json
  /srv/doodleray-downloads/public/channels/<channel>/latest.json (when present)
For the direct Windows channel it also updates:
  /srv/doodleray-downloads/public/download/windows/latest.exe
  /srv/doodleray-downloads/public/download/windows/index.html

The script never mutates existing versioned artifacts unless -Force is passed.
Use this for direct and store-win32 channels instead of GitHub Releases as CDN.
By default it does not update the public Windows download alias. Unsigned
self-hosted installers can trigger Microsoft Defender SmartScreen on fresh
domains, so update the public alias only when the artifact is signed/reputed or
when -PublicWindowsDownloadUrl points to a known public release URL.
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
  [switch]$UpdatePublicWindowsAlias,
  [string]$PublicWindowsDownloadUrl = '',
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

$logoSource = Join-Path $repoRoot 'public\assets\mascot.png'
if (-not (Test-Path -LiteralPath $logoSource)) {
  $logoSource = Join-Path $repoRoot 'devil_icon.png'
}
if (Test-Path -LiteralPath $logoSource) {
  $siteAssets = Join-Path $work '_site-assets'
  New-Item -ItemType Directory -Force -Path $siteAssets | Out-Null
  Copy-Item -LiteralPath $logoSource -Destination (Join-Path $siteAssets 'doodleray-logo.png') -Force
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
$updatePublicWindowsAliasValue = if ($UpdatePublicWindowsAlias -or $PublicWindowsDownloadUrl) { '1' } else { '0' }
$remoteScript = @"
set -euo pipefail
remote_root='$RemoteRoot'
channel='$Channel'
version='$Version'
archive='$remoteArchive'
force='$forceValue'
update_public_windows_alias='$updatePublicWindowsAliasValue'
public_windows_download_url='$PublicWindowsDownloadUrl'
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
if [ -f "`$dest/_site-assets/doodleray-logo.png" ]; then
  mkdir -p "`$remote_root/public/assets"
  cp "`$dest/_site-assets/doodleray-logo.png" "`$remote_root/public/assets/doodleray-logo.png"
  chmod 0644 "`$remote_root/public/assets/doodleray-logo.png"
fi
if [ "`$channel" = "direct" ] && [ "`$update_public_windows_alias" = "1" ]; then
  installer=""
  for candidate in "`$dest"/DoodleRay_*_x64-setup.exe "`$dest"/*setup.exe "`$dest"/*.exe; do
    if [ -f "`$candidate" ]; then
      installer="`$candidate"
      break
    fi
  done
  if [ -n "`$public_windows_download_url" ]; then
    mkdir -p "`$remote_root/public/download/windows"
    rm -f "`$remote_root/public/download/windows/latest.exe"
    cp "`$dest/manifest.json" "`$remote_root/public/download/windows/latest.json"
    cat > "`$remote_root/public/download/windows/index.html" <<HTML
<!doctype html>
<html lang="ru">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Скачать DoodleRay для Windows</title>
    <meta http-equiv="refresh" content="1; url=`$public_windows_download_url">
    <style>
      body { min-height: 100vh; margin: 0; display: grid; place-items: center; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background: #17090f; color: #fff7f2; }
      main { width: min(560px, calc(100vw - 40px)); padding: 32px; border: 1px solid rgba(255,255,255,.14); border-radius: 22px; background: rgba(255,255,255,.075); }
      a { color: #ff9d45; font-weight: 800; }
    </style>
  </head>
  <body>
    <main>
      <h1>Скачивание начинается...</h1>
      <p>DoodleRay для Windows.</p>
      <p>Если скачивание не началось автоматически, <a href="`$public_windows_download_url">нажмите здесь</a>.</p>
    </main>
  </body>
</html>
HTML
    cat > "`$remote_root/public/index.html" <<HTML
<!doctype html>
<html lang="ru">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Скачать DoodleRay VPN</title>
    <meta name="description" content="Скачать DoodleRay VPN для Windows.">
    <style>
      :root { color-scheme: dark; --bg: #17090f; --panel: rgba(255,255,255,.075); --border: rgba(255,255,255,.14); --text: #fff7f2; --muted: rgba(255,247,242,.68); --accent: #ff7a2f; }
      * { box-sizing: border-box; }
      body { margin: 0; min-height: 100vh; display: grid; place-items: center; padding: 32px; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background: radial-gradient(circle at 25% 10%, rgba(255,122,47,.24), transparent 34%), radial-gradient(circle at 82% 80%, rgba(255,59,114,.18), transparent 38%), var(--bg); color: var(--text); }
      main { width: min(720px, 100%); padding: 42px; border: 1px solid var(--border); border-radius: 28px; background: linear-gradient(145deg, rgba(255,255,255,.11), rgba(255,255,255,.04)); box-shadow: 0 30px 90px rgba(0,0,0,.36); }
      .brand { display: flex; align-items: center; gap: 14px; margin-bottom: 28px; font-weight: 800; font-size: 24px; letter-spacing: -.02em; }
      .mark { width: 48px; height: 48px; display: grid; place-items: center; border-radius: 14px; background: linear-gradient(135deg, #ffb000, #ff6724); color: #210b05; font-weight: 900; overflow: hidden; box-shadow: 0 12px 30px rgba(255, 122, 47, .24); }
      .mark img { display: block; width: 100%; height: 100%; object-fit: cover; }
      .mark span { display: none; }
      .mark--fallback span { display: block; }
      h1 { margin: 0 0 12px; font-size: clamp(34px, 7vw, 58px); line-height: .95; letter-spacing: -.05em; }
      p { margin: 0; color: var(--muted); font-size: 18px; line-height: 1.55; }
      .actions { display: flex; flex-wrap: wrap; gap: 12px; margin-top: 32px; }
      a.button { display: inline-flex; align-items: center; justify-content: center; min-height: 54px; padding: 0 22px; border-radius: 16px; color: #190904; background: linear-gradient(135deg, #ff9d45, var(--accent)); text-decoration: none; font-weight: 800; }
      a.secondary { color: var(--text); background: var(--panel); border: 1px solid var(--border); }
      .note { margin-top: 22px; font-size: 14px; }
    </style>
  </head>
  <body>
    <main>
      <div class="brand"><div class="mark"><img src="/assets/doodleray-logo.png" alt="" onerror="this.remove();this.parentElement.classList.add('mark--fallback');"><span>DR</span></div><span>DoodleRay VPN</span></div>
      <h1>Скачать для Windows</h1>
      <p>Официальный установщик DoodleRay для Windows.</p>
      <div class="actions">
        <a class="button" href="`$public_windows_download_url">Скачать DoodleRay для Windows</a>
        <a class="button secondary" href="https://github.com/Maximus657/doodleray/releases/latest">Что изменилось</a>
      </div>
    </main>
  </body>
</html>
HTML
    echo "updated public Windows download URL: `$public_windows_download_url"
  elif [ -n "`$installer" ]; then
    installer_name="`$(basename "`$installer")"
    mkdir -p "`$remote_root/public/download/windows"
    ln -sfn "../../releases/`$channel/`$version/`$installer_name" "`$remote_root/public/download/windows/latest.exe"
    cp "`$dest/manifest.json" "`$remote_root/public/download/windows/latest.json"
    cat > "`$remote_root/public/download/windows/index.html" <<HTML
<!doctype html>
<html lang="ru">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Скачать DoodleRay для Windows</title>
    <meta http-equiv="refresh" content="1; url=/download/windows/latest.exe">
    <style>
      body { min-height: 100vh; margin: 0; display: grid; place-items: center; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background: #17090f; color: #fff7f2; }
      main { width: min(560px, calc(100vw - 40px)); padding: 32px; border: 1px solid rgba(255,255,255,.14); border-radius: 22px; background: rgba(255,255,255,.075); }
      a { color: #ff9d45; font-weight: 800; }
    </style>
  </head>
  <body>
    <main>
      <h1>Скачивание начинается...</h1>
      <p>DoodleRay `$version для Windows.</p>
      <p>Если скачивание не началось автоматически, <a href="/download/windows/latest.exe">нажмите здесь</a>.</p>
    </main>
  </body>
</html>
HTML
    cat > "`$remote_root/public/index.html" <<HTML
<!doctype html>
<html lang="ru">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Скачать DoodleRay VPN</title>
    <meta name="description" content="Скачать DoodleRay VPN для Windows с официального хоста загрузок.">
    <style>
      :root { color-scheme: dark; --bg: #17090f; --panel: rgba(255,255,255,.075); --border: rgba(255,255,255,.14); --text: #fff7f2; --muted: rgba(255,247,242,.68); --accent: #ff7a2f; }
      * { box-sizing: border-box; }
      body { margin: 0; min-height: 100vh; display: grid; place-items: center; padding: 32px; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background: radial-gradient(circle at 25% 10%, rgba(255,122,47,.24), transparent 34%), radial-gradient(circle at 82% 80%, rgba(255,59,114,.18), transparent 38%), var(--bg); color: var(--text); }
      main { width: min(720px, 100%); padding: 42px; border: 1px solid var(--border); border-radius: 28px; background: linear-gradient(145deg, rgba(255,255,255,.11), rgba(255,255,255,.04)); box-shadow: 0 30px 90px rgba(0,0,0,.36); }
      .brand { display: flex; align-items: center; gap: 14px; margin-bottom: 28px; font-weight: 800; font-size: 24px; letter-spacing: -.02em; }
      .mark { width: 48px; height: 48px; display: grid; place-items: center; border-radius: 14px; background: linear-gradient(135deg, #ffb000, #ff6724); color: #210b05; font-weight: 900; overflow: hidden; box-shadow: 0 12px 30px rgba(255, 122, 47, .24); }
      .mark img { display: block; width: 100%; height: 100%; object-fit: cover; }
      .mark span { display: none; }
      .mark--fallback span { display: block; }
      h1 { margin: 0 0 12px; font-size: clamp(34px, 7vw, 58px); line-height: .95; letter-spacing: -.05em; }
      p { margin: 0; color: var(--muted); font-size: 18px; line-height: 1.55; }
      .actions { display: flex; flex-wrap: wrap; gap: 12px; margin-top: 32px; }
      a.button { display: inline-flex; align-items: center; justify-content: center; min-height: 54px; padding: 0 22px; border-radius: 16px; color: #190904; background: linear-gradient(135deg, #ff9d45, var(--accent)); text-decoration: none; font-weight: 800; }
      a.secondary { color: var(--text); background: var(--panel); border: 1px solid var(--border); }
      .note { margin-top: 22px; font-size: 14px; }
    </style>
  </head>
  <body>
    <main>
      <div class="brand"><div class="mark"><img src="/assets/doodleray-logo.png" alt="" onerror="this.remove();this.parentElement.classList.add('mark--fallback');"><span>DR</span></div><span>DoodleRay VPN</span></div>
      <h1>Скачать для Windows</h1>
      <p>Официальный установщик DoodleRay `$version для Windows.</p>
      <div class="actions">
        <a class="button" href="/download/windows/latest.exe">Скачать DoodleRay для Windows</a>
        <a class="button secondary" href="/download/windows/latest.json">Информация о релизе</a>
      </div>
      <p class="note">Файл версии: /releases/direct/`$version/`$installer_name</p>
    </main>
  </body>
</html>
HTML
    echo "updated direct Windows download alias: /download/windows/latest.exe -> `$installer_name"
  else
    echo "warning: direct channel published without a Windows installer alias" >&2
  fi
elif [ "`$channel" = "direct" ]; then
  echo "public Windows download alias not updated; pass -UpdatePublicWindowsAlias or -PublicWindowsDownloadUrl to update it"
fi
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
if ($Channel -eq 'direct' -and ($UpdatePublicWindowsAlias -or $PublicWindowsDownloadUrl)) {
  Write-Host "Windows download: https://$HostName/download/windows"
  if ($PublicWindowsDownloadUrl) {
    Write-Host "Public Windows URL: $PublicWindowsDownloadUrl"
  } else {
    Write-Host "Stable installer: https://$HostName/download/windows/latest.exe"
  }
}
