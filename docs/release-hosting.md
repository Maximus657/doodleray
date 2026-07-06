# DoodleRay Release Hosting

Goal: stop using GitHub Releases as the product CDN. GitHub may still build
artifacts, but users, the updater, and Microsoft Store Partner Center must read
from a first-party immutable downloads host.

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

Known current staging state:

- Dokploy compose `doodleray-downloads` exists and is healthy.
- It serves static files through nginx inside Docker.
- It is exposed on the host loopback as `127.0.0.1:18089`.
- Public domain routing still needs the host nginx vhost below if SSH/root
  access is not already restored.

For the current Dokploy-backed host, run as root:

```bash
cd /tmp
curl -fsSL -o bootstrap-downloads-host.sh https://raw.githubusercontent.com/Maximus657/doodleray/production/scripts/release/bootstrap-downloads-host.sh
DOMAIN=doodleray.clickflare.click \
UPSTREAM=http://127.0.0.1:18089 \
WEB_SERVER=nginx \
bash bootstrap-downloads-host.sh
```

If publishing directly to host storage instead of Dokploy volume, omit
`UPSTREAM`; nginx will serve `/srv/doodleray-downloads/public` directly.

## URL Contract

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
```

Never overwrite a file under `/releases/<channel>/<version>/` after it has been
submitted to Partner Center. Build a new version and publish a new URL.

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
2. Build signed artifacts in CI or locally.
3. Publish with:

```powershell
.\scripts\release\Publish-DoodleRayDownloads.ps1 `
  -Version 6.0.0 `
  -Channel store-win32 `
  -ArtifactDir .\dist-store `
  -HostName doodleray.clickflare.click
```

4. Verify:

```powershell
Invoke-WebRequest https://doodleray.clickflare.click/channels/store-win32/manifest.json
Invoke-WebRequest https://doodleray.clickflare.click/releases/store-win32/6.0.0/DoodleRay-store-win32-6.0.0-x64-setup.exe -Method Head
```

5. Partner Center uses the versioned `.exe` URL, not the channel URL.

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
