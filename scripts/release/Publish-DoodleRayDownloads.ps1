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
  [string]$ReleaseNotesFile = '',
  [switch]$UpdatePublicWindowsAlias,
  [string]$PublicWindowsDownloadUrl = '',
  [string]$PublicMacAppleSiliconDownloadUrl = '',
  [string]$PublicMacIntelDownloadUrl = '',
  [switch]$Force
)

$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $ArtifactDir)) { throw "ArtifactDir not found: $ArtifactDir" }

$repoRoot = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
if ([string]::IsNullOrWhiteSpace($ReleaseNotesFile)) {
  $ReleaseNotesFile = Join-Path $repoRoot "docs\release-notes\$Version.md"
}
if (-not (Test-Path -LiteralPath $ReleaseNotesFile)) {
  throw "Release notes are required for every public build. Create docs/release-notes/$Version.md with a plain-language summary and bullet list, or pass -ReleaseNotesFile."
}

function ConvertTo-PlainReleaseText {
  param([string]$Text)
  return (($Text -replace '<[^>]+>', '') -replace '\s+', ' ').Trim()
}

function ConvertTo-HtmlText {
  param([string]$Text)
  return [System.Net.WebUtility]::HtmlEncode((ConvertTo-PlainReleaseText $Text))
}

$releaseNoteLines = Get-Content -LiteralPath $ReleaseNotesFile -Encoding utf8
$releaseTitle = ($releaseNoteLines | Where-Object { $_ -match '^\s*#\s+(.+?)\s*$' } | Select-Object -First 1)
if ($releaseTitle) { $releaseTitle = ($releaseTitle -replace '^\s*#\s+', '').Trim() } else { $releaseTitle = "DoodleRay $Version" }
$releaseSummary = ($releaseNoteLines | Where-Object { $_ -match '^\s*Коротко:\s*(.+?)\s*$' } | Select-Object -First 1)
if ($releaseSummary) { $releaseSummary = ($releaseSummary -replace '^\s*Коротко:\s*', '').Trim() } else { throw "Release notes must contain a 'Коротко:' line: $ReleaseNotesFile" }
$releaseChanges = @($releaseNoteLines | Where-Object { $_ -match '^\s*-\s+(.+?)\s*$' } | ForEach-Object { ($_ -replace '^\s*-\s+', '').Trim() })
if ($releaseChanges.Count -eq 0) { throw "Release notes must contain at least one plain-language bullet starting with '- ': $ReleaseNotesFile" }

$uploadWorkRoot = Join-Path ([System.IO.Path]::GetTempPath()) 'doodleray-release-upload'
New-Item -ItemType Directory -Force -Path $uploadWorkRoot | Out-Null
$work = Join-Path $uploadWorkRoot "$Channel-$Version"
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

$logoSource = Join-Path $repoRoot 'src-tauri\icons\StoreLogo.png'
if (-not (Test-Path -LiteralPath $logoSource)) {
  $logoSource = Join-Path $repoRoot 'public\assets\mascot.png'
}
if (-not (Test-Path -LiteralPath $logoSource)) {
  $logoSource = Join-Path $repoRoot 'devil_icon.png'
}
if (Test-Path -LiteralPath $logoSource) {
  $siteAssets = Join-Path $work '_site-assets'
  New-Item -ItemType Directory -Force -Path $siteAssets | Out-Null
  Copy-Item -LiteralPath $logoSource -Destination (Join-Path $siteAssets 'doodleray-logo.png') -Force
}

$releaseNotes = [ordered]@{
  version = $Version
  channel = $Channel
  title = (ConvertTo-PlainReleaseText $releaseTitle)
  summary = (ConvertTo-PlainReleaseText $releaseSummary)
  changes = @($releaseChanges | ForEach-Object { ConvertTo-PlainReleaseText $_ })
  createdAtUtc = (Get-Date).ToUniversalTime().ToString('o')
}
$releaseNotes | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $work 'release-notes.json') -Encoding utf8

$historyUrl = "https://$HostName/channels/$Channel/history.json"
$previousHistory = @()
try {
  $historyResponse = Invoke-WebRequest -Uri $historyUrl -UseBasicParsing -TimeoutSec 10
  $historyJson = $historyResponse.Content | ConvertFrom-Json
  if ($historyJson.releases) { $previousHistory = @($historyJson.releases) }
} catch {
  $previousHistory = @()
}
$historyReleases = @($releaseNotes) + @($previousHistory | Where-Object { $_.version -ne $Version } | Select-Object -First 11)
$history = [ordered]@{
  product = 'DoodleRay'
  channel = $Channel
  updatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
  releases = @($historyReleases)
}
$history | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $work 'history.json') -Encoding utf8

$latestChangeItems = ($releaseChanges | ForEach-Object { "          <li>$(ConvertTo-HtmlText $_)</li>" }) -join "`n"
$releaseHistoryItems = ($historyReleases | Select-Object -Skip 1 | ForEach-Object {
  $itemVersion = ConvertTo-HtmlText $_.version
  $itemTitle = ConvertTo-HtmlText $_.title
  $itemSummary = ConvertTo-HtmlText $_.summary
  $itemChanges = @($_.changes | Select-Object -First 4 | ForEach-Object { "              <li>$(ConvertTo-HtmlText $_)</li>" }) -join "`n"
  @"
        <article class="release-item">
          <div class="release-version">v$itemVersion</div>
          <div>
            <h3>$itemTitle</h3>
            <p>$itemSummary</p>
            <ul>
$itemChanges
            </ul>
          </div>
        </article>
"@
}) -join "`n"
$releaseHistoryHtml = @"
      <section id="versions" class="release-history">
        <div class="section-title">
          <span>История версий</span>
          <a href="/channels/$Channel/history.json">JSON</a>
        </div>
        <article class="release-item release-item--latest">
          <div class="release-version">v$(ConvertTo-HtmlText $Version)</div>
          <div>
            <h3>$(ConvertTo-HtmlText $releaseTitle)</h3>
            <p>$(ConvertTo-HtmlText $releaseSummary)</p>
            <ul>
$latestChangeItems
            </ul>
          </div>
        </article>
$releaseHistoryItems
      </section>
"@
$releaseHistoryStyles = @"
      .release-history { margin-top: 34px; padding-top: 28px; border-top: 1px solid var(--border); }
      .section-title { display: flex; align-items: center; justify-content: space-between; gap: 16px; margin-bottom: 16px; color: var(--text); font-size: 18px; font-weight: 900; }
      .section-title a { color: var(--muted); font-size: 13px; text-decoration: none; }
      .release-item { display: grid; grid-template-columns: 88px 1fr; gap: 18px; padding: 18px 0; border-top: 1px solid rgba(255,255,255,.08); }
      .release-item:first-of-type { border-top: 0; padding-top: 0; }
      .release-item--latest { padding: 18px; border: 1px solid rgba(255,122,47,.30); border-radius: 18px; background: rgba(255,122,47,.08); }
      .release-version { color: #ffb15f; font-weight: 900; white-space: nowrap; }
      .release-item h3 { margin: 0 0 6px; color: var(--text); font-size: 18px; }
      .release-item p { margin: 0 0 10px; font-size: 15px; }
      .release-item ul { margin: 0; padding-left: 18px; color: var(--muted); line-height: 1.55; }
      .release-item li { margin: 5px 0; }
      @media (max-width: 640px) {
        .release-item { grid-template-columns: 1fr; gap: 8px; }
      }
"@
$platformSelectStyles = @"
      .platforms { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; margin-top: 30px; }
      .platform-card { min-height: 132px; padding: 18px; border-radius: 18px; border: 1px solid var(--border); background: rgba(255,255,255,.06); color: var(--text); text-decoration: none; display: flex; flex-direction: column; justify-content: space-between; transition: transform .18s ease, border-color .18s ease, background .18s ease; }
      .platform-card:hover { transform: translateY(-2px); border-color: rgba(255,122,47,.54); background: rgba(255,122,47,.10); }
      .platform-card--primary { border-color: rgba(255,122,47,.40); background: rgba(255,122,47,.10); }
      .platform-label { display: flex; align-items: center; gap: 10px; font-size: 18px; font-weight: 900; }
      .platform-icon { width: 34px; height: 34px; border-radius: 11px; display: grid; place-items: center; background: rgba(255,122,47,.16); color: #ffb15f; font-size: 12px; font-weight: 900; letter-spacing: .04em; }
      .platform-card p { margin: 12px 0 0; color: var(--muted); font-size: 14px; line-height: 1.45; }
      .platform-hint { margin-top: 12px; color: var(--muted); font-size: 13px; line-height: 1.45; }
      @media (max-width: 760px) {
        .platforms { grid-template-columns: 1fr; }
      }
"@
$platformSelectHtml = @"
      <section class="platforms" aria-label="Выбор версии DoodleRay">
        <a class="platform-card platform-card--primary" href="/download/windows/">
          <span class="platform-label"><span class="platform-icon">WIN</span>Windows</span>
          <p>Для Windows 10/11, 64-bit. Рекомендуемый вариант для большинства компьютеров.</p>
        </a>
        <a class="platform-card" href="/download/macos/apple-silicon/">
          <span class="platform-label"><span class="platform-icon">M</span>macOS Apple Silicon</span>
          <p>Для Mac на M1, M2, M3, M4 и новее.</p>
        </a>
        <a class="platform-card" href="/download/macos/intel/">
          <span class="platform-label"><span class="platform-icon">INT</span>macOS Intel</span>
          <p>Для Mac старше 2020 года. Технически это версия x86-64.</p>
        </a>
      </section>
      <p class="platform-hint">Если не знаете, какой Mac у вас: M1/M2/M3/M4 — Apple Silicon, старые Intel Mac — версия Intel.</p>
"@

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

$archive = Join-Path $uploadWorkRoot "$Channel-$Version.tar.gz"
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
$updatePublicWindowsAliasValue = if ($UpdatePublicWindowsAlias -or $PublicWindowsDownloadUrl -or $PublicMacAppleSiliconDownloadUrl -or $PublicMacIntelDownloadUrl) { '1' } else { '0' }
$remoteScript = @"
set -euo pipefail
remote_root='$RemoteRoot'
channel='$Channel'
version='$Version'
archive='$remoteArchive'
force='$forceValue'
update_public_windows_alias='$updatePublicWindowsAliasValue'
public_windows_download_url='$PublicWindowsDownloadUrl'
public_macos_apple_silicon_download_url='$PublicMacAppleSiliconDownloadUrl'
public_macos_intel_download_url='$PublicMacIntelDownloadUrl'
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
if [ -f "`$dest/release-notes.json" ]; then
  cp "`$dest/release-notes.json" "`$remote_root/public/channels/`$channel/latest-notes.json"
fi
if [ -f "`$dest/history.json" ]; then
  cp "`$dest/history.json" "`$remote_root/public/channels/`$channel/history.json"
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
  macos_apple_silicon=""
  for candidate in "`$dest"/DoodleRay_*_aarch64.dmg "`$dest"/*aarch64*.dmg "`$dest"/*arm64*.dmg; do
    if [ -f "`$candidate" ]; then
      macos_apple_silicon="`$candidate"
      break
    fi
  done
  macos_intel=""
  for candidate in "`$dest"/DoodleRay_*_x64.dmg "`$dest"/*x86_64*.dmg "`$dest"/*intel*.dmg; do
    if [ -f "`$candidate" ]; then
      macos_intel="`$candidate"
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
$platformSelectStyles
$releaseHistoryStyles
    </style>
  </head>
  <body>
    <main>
      <div class="brand"><div class="mark"><img src="/assets/doodleray-logo.png" alt="" onerror="this.remove();this.parentElement.classList.add('mark--fallback');"><span>DR</span></div><span>DoodleRay VPN</span></div>
      <h1>Скачать DoodleRay</h1>
      <p>Выберите версию под свое устройство. Windows, новые Mac на Apple Silicon и старые Intel Mac вынесены отдельно.</p>
$platformSelectHtml
      <div class="actions">
        <a class="button secondary" href="#versions">Что изменилось</a>
      </div>
$releaseHistoryHtml
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
$platformSelectStyles
$releaseHistoryStyles
    </style>
  </head>
  <body>
    <main>
      <div class="brand"><div class="mark"><img src="/assets/doodleray-logo.png" alt="" onerror="this.remove();this.parentElement.classList.add('mark--fallback');"><span>DR</span></div><span>DoodleRay VPN</span></div>
      <h1>Скачать DoodleRay</h1>
      <p>Выберите версию под свое устройство. Windows, новые Mac на Apple Silicon и старые Intel Mac вынесены отдельно.</p>
      $platformSelectHtml
      <div class="actions">
        <a class="button secondary" href="#versions">Что изменилось</a>
      </div>
      <p class="note">Файл версии: /releases/direct/`$version/`$installer_name</p>
$releaseHistoryHtml
    </main>
  </body>
</html>
HTML
    echo "updated direct Windows download alias: /download/windows/latest.exe -> `$installer_name"
  else
    echo "warning: direct channel published without a Windows installer alias" >&2
  fi
  mkdir -p "`$remote_root/public/download/macos" "`$remote_root/public/download/macos/apple-silicon" "`$remote_root/public/download/macos/intel"
  if [ -z "`$public_macos_apple_silicon_download_url" ] && [ -n "`$macos_apple_silicon" ]; then
    macos_apple_silicon_name="`$(basename "`$macos_apple_silicon")"
    ln -sfn "../../../releases/`$channel/`$version/`$macos_apple_silicon_name" "`$remote_root/public/download/macos/apple-silicon/latest.dmg"
    public_macos_apple_silicon_download_url="/download/macos/apple-silicon/latest.dmg"
  fi
  if [ -z "`$public_macos_intel_download_url" ] && [ -n "`$macos_intel" ]; then
    macos_intel_name="`$(basename "`$macos_intel")"
    ln -sfn "../../../releases/`$channel/`$version/`$macos_intel_name" "`$remote_root/public/download/macos/intel/latest.dmg"
    public_macos_intel_download_url="/download/macos/intel/latest.dmg"
  fi
  cat > "`$remote_root/public/download/macos/index.html" <<HTML
<!doctype html>
<html lang="ru">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Скачать DoodleRay для macOS</title>
    <style>
      :root { color-scheme: dark; --bg: #17090f; --panel: rgba(255,255,255,.075); --border: rgba(255,255,255,.14); --text: #fff7f2; --muted: rgba(255,247,242,.68); --accent: #ff7a2f; }
      * { box-sizing: border-box; }
      body { margin: 0; min-height: 100vh; display: grid; place-items: center; padding: 32px; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background: radial-gradient(circle at 25% 10%, rgba(255,122,47,.20), transparent 34%), var(--bg); color: var(--text); }
      main { width: min(720px, 100%); padding: 34px; border: 1px solid var(--border); border-radius: 24px; background: linear-gradient(145deg, rgba(255,255,255,.11), rgba(255,255,255,.04)); }
      h1 { margin: 0 0 10px; font-size: clamp(30px, 6vw, 48px); line-height: 1; }
      p { margin: 0; color: var(--muted); font-size: 17px; line-height: 1.55; }
$platformSelectStyles
    </style>
  </head>
  <body>
    <main>
      <h1>Выберите версию для macOS</h1>
      <p>Для новых Mac выбирайте Apple Silicon. Для старых Mac выбирайте Intel.</p>
      <section class="platforms" aria-label="Выбор версии macOS">
        <a class="platform-card platform-card--primary" href="/download/macos/apple-silicon/">
          <span class="platform-label"><span class="platform-icon">M</span>Apple Silicon</span>
          <p>Для Mac на M1, M2, M3, M4 и новее.</p>
        </a>
        <a class="platform-card" href="/download/macos/intel/">
          <span class="platform-label"><span class="platform-icon">INT</span>Intel</span>
          <p>Для Mac старше 2020 года. Технически это версия x86-64.</p>
        </a>
      </section>
    </main>
  </body>
</html>
HTML
  if [ -n "`$public_macos_apple_silicon_download_url" ]; then
    cat > "`$remote_root/public/download/macos/apple-silicon/index.html" <<HTML
<!doctype html>
<html lang="ru">
  <head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Скачать DoodleRay для macOS Apple Silicon</title><meta http-equiv="refresh" content="1; url=`$public_macos_apple_silicon_download_url"><style>body{min-height:100vh;margin:0;display:grid;place-items:center;font-family:Inter,ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;background:#17090f;color:#fff7f2}main{width:min(560px,calc(100vw - 40px));padding:32px;border:1px solid rgba(255,255,255,.14);border-radius:22px;background:rgba(255,255,255,.075)}a{color:#ff9d45;font-weight:800}</style></head>
  <body><main><h1>Скачивание начинается...</h1><p>DoodleRay для macOS Apple Silicon: M1, M2, M3, M4 и новее.</p><p>Если скачивание не началось автоматически, <a href="`$public_macos_apple_silicon_download_url">нажмите здесь</a>.</p></main></body>
</html>
HTML
  else
    cat > "`$remote_root/public/download/macos/apple-silicon/index.html" <<HTML
<!doctype html><html lang="ru"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>DoodleRay для macOS Apple Silicon</title><style>body{min-height:100vh;margin:0;display:grid;place-items:center;font-family:Inter,ui-sans-serif,system-ui;background:#17090f;color:#fff7f2}main{width:min(560px,calc(100vw - 40px));padding:32px;border:1px solid rgba(255,255,255,.14);border-radius:22px;background:rgba(255,255,255,.075)}p{color:rgba(255,247,242,.68)}</style></head><body><main><h1>Скачивание готовится</h1><p>Версия для Mac на M1/M2/M3/M4 пока не опубликована на этом хосте.</p></main></body></html>
HTML
  fi
  if [ -n "`$public_macos_intel_download_url" ]; then
    cat > "`$remote_root/public/download/macos/intel/index.html" <<HTML
<!doctype html>
<html lang="ru">
  <head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Скачать DoodleRay для macOS Intel</title><meta http-equiv="refresh" content="1; url=`$public_macos_intel_download_url"><style>body{min-height:100vh;margin:0;display:grid;place-items:center;font-family:Inter,ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;background:#17090f;color:#fff7f2}main{width:min(560px,calc(100vw - 40px));padding:32px;border:1px solid rgba(255,255,255,.14);border-radius:22px;background:rgba(255,255,255,.075)}a{color:#ff9d45;font-weight:800}</style></head>
  <body><main><h1>Скачивание начинается...</h1><p>DoodleRay для macOS Intel. Это версия для Mac старше 2020 года, x86-64.</p><p>Если скачивание не началось автоматически, <a href="`$public_macos_intel_download_url">нажмите здесь</a>.</p></main></body>
</html>
HTML
  else
    cat > "`$remote_root/public/download/macos/intel/index.html" <<HTML
<!doctype html><html lang="ru"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>DoodleRay для macOS Intel</title><style>body{min-height:100vh;margin:0;display:grid;place-items:center;font-family:Inter,ui-sans-serif,system-ui;background:#17090f;color:#fff7f2}main{width:min(560px,calc(100vw - 40px));padding:32px;border:1px solid rgba(255,255,255,.14);border-radius:22px;background:rgba(255,255,255,.075)}p{color:rgba(255,247,242,.68)}</style></head><body><main><h1>Скачивание готовится</h1><p>Версия для Mac старше 2020 года, x86-64, пока не опубликована на этом хосте.</p></main></body></html>
HTML
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
