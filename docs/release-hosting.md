# DoodleRay Release Hosting

Goal: stop using GitHub Releases as the product CDN. GitHub may still build
artifacts, but users, the updater, and Microsoft Store Partner Center must read
from a first-party immutable downloads host.

Current product decision: the primary Windows distribution path is the direct
first-party download page. Microsoft Store remains optional/later.

Unsigned Windows installers served from a fresh first-party domain can trigger
Microsoft Defender SmartScreen even when the same unsigned release on GitHub
does not. Until DoodleRay has a code-signing/reputation solution, the public
download page may point to the current GitHub release asset while this host keeps
immutable mirrors and future signed direct-channel artifacts.

## Host

- Domain: `doodleray.clickflare.click`
- Server: `87.120.166.237`
- Static root: `/srv/doodleray-downloads/public`
- Bootstrap script: `scripts/release/bootstrap-downloads-host.sh`

The host is the same Dokploy machine that serves bot/subsvc traffic. Do not
replace nginx/Dokploy or touch existing `doodlevpn.online` / `ddlvpn.lol`
routes. The live host currently uses nginx vhosts, not Caddy. The bootstrap
script auto-detects nginx/Caddy and only adds a separate vhost for
`doodleray.clickflare.click`.

Current host state:

- Dokploy compose `doodleray-downloads` exists and is healthy.
- Host nginx now serves `doodleray.clickflare.click` directly from
  `/srv/doodleray-downloads/public`.
- The old Dokploy compose may still exist on loopback `127.0.0.1:18089`, but it
  is no longer the public serving path for downloads.
- Let's Encrypt HTTPS is active for `https://doodleray.clickflare.click/`.
- `http://doodleray.clickflare.click/` redirects to HTTPS.

If this vhost ever has to be recreated, run as root:

```bash
cd /tmp
curl -fsSL -o bootstrap-downloads-host.sh https://raw.githubusercontent.com/Maximus657/doodleray/production/scripts/release/bootstrap-downloads-host.sh
DOMAIN=doodleray.clickflare.click \
WEB_SERVER=nginx \
bash bootstrap-downloads-host.sh
```

Do not pass `UPSTREAM` for the main direct-download path. Nginx should serve
`/srv/doodleray-downloads/public` directly, because the publish script uploads
release files there.

## URL Contract

Human-facing download URLs:

```text
https://doodleray.clickflare.click/
https://doodleray.clickflare.click/download/windows
https://doodleray.clickflare.click/download/windows/latest.exe
https://doodleray.clickflare.click/download/windows/latest.json
https://doodleray.clickflare.click/download/macos
https://doodleray.clickflare.click/download/macos/apple-silicon
https://doodleray.clickflare.click/download/macos/apple-silicon/latest.dmg
https://doodleray.clickflare.click/download/macos/intel
https://doodleray.clickflare.click/download/macos/intel/latest.dmg
https://doodleray.clickflare.click/channels/direct/history.json
https://doodleray.clickflare.click/channels/direct/latest-notes.json
```

`/download/windows/latest.exe` is optional. Do not publish it for unsigned
self-hosted installers unless the release has already been reputation-tested.
For unsigned public releases, prefer a GitHub release URL through
`-PublicWindowsDownloadUrl`.

Versioned artifacts are immutable:

```text
https://doodleray.clickflare.click/releases/direct/6.0.0/DoodleRay_6.0.0_x64-setup.exe
https://doodleray.clickflare.click/releases/store-win32/6.0.0/DoodleRay-store-win32-6.0.0-x64-setup.exe
```

Channel manifests are mutable pointers:

```text
https://doodleray.clickflare.click/channels/direct/latest.json
https://doodleray.clickflare.click/channels/store-win32/latest.json
https://doodleray.clickflare.click/channels/direct/manifest.json
https://doodleray.clickflare.click/channels/store-win32/manifest.json
https://doodleray.clickflare.click/channels/direct/history.json
https://doodleray.clickflare.click/channels/store-win32/history.json
https://doodleray.clickflare.click/channels/direct/latest-notes.json
https://doodleray.clickflare.click/channels/store-win32/latest-notes.json
```

Never overwrite a file under `/releases/<channel>/<version>/` after it has been
submitted to Partner Center. Build a new version and publish a new URL.

`/download/windows/latest.exe` is intentionally mutable when enabled. It is a
convenience alias for the newest direct Windows installer and is updated only by
the direct channel publish flow when explicitly requested.

## Branch Model

- `develop`: integration branch for v6 work, QA builds, design/API changes.
- `production`: release branch. Only merge from `develop` after QA evidence.
- `codex/*`: agent work branches.
- Tags on `production`:
  - `vX.Y.Z` for direct channel releases.
  - Store submissions use the `store-win32` workflow and immutable URL under
    `/releases/store-win32/X.Y.Z/`.

## Publish Flow

1. Merge tested changes into `production`.
2. Create public release notes at `docs/release-notes/<version>.md`.
   They must be written in plain user language and include `Коротко:` plus a
   short bullet list. These notes are shown on the download page.
3. Build signed artifacts in CI or locally.
4. Publish with:

```powershell
.\scripts\release\Publish-DoodleRayDownloads.ps1 `
  -Version 6.0.0 `
  -Channel store-win32 `
  -ArtifactDir .\dist-store `
  -HostName doodleray.clickflare.click
```

By default the publish script reads `docs/release-notes/<version>.md`. For an
emergency or non-standard build, pass `-ReleaseNotesFile <path>`.

The script publishes:

- immutable release files under `/releases/<channel>/<version>/`;
- `/releases/<channel>/<version>/release-notes.json`;
- `/channels/<channel>/latest-notes.json`;
- `/channels/<channel>/history.json`;
- a human-readable “История версий” block on the download page when the public
  Windows alias is updated.

5. Verify:

```powershell
Invoke-WebRequest https://doodleray.clickflare.click/channels/store-win32/manifest.json
Invoke-WebRequest https://doodleray.clickflare.click/channels/store-win32/history.json
Invoke-WebRequest https://doodleray.clickflare.click/releases/store-win32/6.0.0/DoodleRay-store-win32-6.0.0-x64-setup.exe -Method Head
```

6. Partner Center uses the versioned `.exe` URL, not the channel URL.

For direct website downloads, users can use:

```text
https://doodleray.clickflare.click/download/windows
```

For unsigned public releases that should keep GitHub download reputation:

```powershell
.\scripts\release\Publish-DoodleRayDownloads.ps1 `
  -Version 5.9.0 `
  -Channel direct `
  -ArtifactDir .\dist-direct `
  -HostName doodleray.clickflare.click `
  -PublicWindowsDownloadUrl https://github.com/Maximus657/doodleray/releases/download/v5.9.0/DoodleRay_5.9.0_x64-setup.exe `
  -PublicMacAppleSiliconDownloadUrl https://github.com/Maximus657/doodleray/releases/download/v5.9.0/DoodleRay_5.9.0_aarch64.dmg `
  -PublicMacIntelDownloadUrl https://github.com/Maximus657/doodleray/releases/download/v5.9.0/DoodleRay_5.9.0_x64.dmg
```

For signed/reputation-tested direct installers, add `-UpdatePublicWindowsAlias`
instead of `-PublicWindowsDownloadUrl`.

## GitHub Secrets For CI Publish

Required for upload:

- `DOWNLOADS_SSH_PRIVATE_KEY`

Optional repository variables:

- `DOWNLOADS_SSH_HOST` = `doodleray.clickflare.click`
- `DOWNLOADS_SSH_USER` = `root`
- `DOWNLOADS_SSH_PORT` = `22`
- `DOWNLOADS_REMOTE_ROOT` = `/srv/doodleray-downloads`

Store-signed builds additionally require:

- `STORE_CODESIGN_PFX_B64`
- `STORE_CODESIGN_PFX_PASSWORD`

Direct updater artifacts require:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
