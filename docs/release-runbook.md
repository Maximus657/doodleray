# Release Runbook — from merged PR to users seeing the update banner

Learned the hard way 2026-07-25: shipping v6.0.2 required 3 separate manual
steps, not 1. Missing any of the last two means the GitHub Release looks
perfect but zero users ever see an update prompt.

## The three steps, in order

### 1. Merge the PR into `main`
Via GitHub UI (Claude Code's auto-mode classifier blocks `gh pr merge` —
don't fight it, ask the human to click the button).

### 2. Tag and push — builds + publishes the GitHub Release
```bash
git checkout main
git pull
git tag vX.Y.Z
git push origin vX.Y.Z
```
This triggers `.github/workflows/release.yml` (`on: push: tags: v*`).
- `build-windows` builds, signs, and creates the GitHub Release with all
  Windows assets including a **correct, complete `latest.json`** (both
  `windows-x86_64` and `windows-x86_64-nsis` keys, already valid — verified
  by hand-inspecting the asset content, no patch needed in practice).
- `build-macos` needs Apple signing secrets that are **not currently
  configured** in this repo. It will fail. This is expected and does not
  affect the Windows release — `build-windows` already stands on its own,
  release included.
- `patch-updater-metadata` needs `build-macos` and will show as skipped/failed
  as a result. Checked its actual job body: it only ever touches the Windows
  keys in `latest.json`, and those are already correct from step 1. **This
  failing is cosmetic, not a real blocker, as of 2026-07-25.**
- Verify the release directly if in doubt:
  `gh api repos/Maximus657/doodleray/releases/tags/vX.Y.Z --jq '.assets[].name'`
  should list the `.exe`, `.nsis.zip`, both `.sig` files, and `latest.json`.

### 3. Manually run `publish-downloads.yml` — this is the step everyone forgets
The app's real update-check endpoint is **not** GitHub — it's
`https://doodleray.clickflare.click/channels/direct/latest.json` (see
`src-tauri/tauri.conf.json` → `plugins.updater.endpoints`). The GitHub
Release existing changes nothing about what the CDN serves. Nobody's running
app will ever see a new version until this step runs.

`publish-downloads.yml` only triggers via `workflow_dispatch` (manual) — it
does **not** fire on tag push. Go to:
https://github.com/Maximus657/doodleray/actions/workflows/publish-downloads.yml
→ **Run workflow**, and set:
- **Semver version to publish**: `X.Y.Z` (no `v` prefix, must match the tag)
- **Release channel**: `direct` (matches the endpoint path in
  `tauri.conf.json` — `channels/direct/latest.json`; only change this if the
  app's own endpoint config changes too)
- **RC-only unsigned build**: leave **unchecked** for a real release (only
  check this for an internal unsigned RC that must never reach the CDN)
- **Upload artifacts to doodleray.clickflare.click**: **check this** — this
  is the actual switch that updates the CDN. Without it, the workflow runs
  and does nothing user-visible.

## How users actually find out

The app checks for updates on launch, then every 30 minutes
(`src/App.tsx`, `checkForUpdates` / the 30-min `setInterval`). There is no
push notification and no way to force it sooner from the server side — a
user who already has the app open won't see the banner until their next
periodic check or a full restart (closing to the tray does **not** count;
the process must actually relaunch).

## Sanity checklist before telling the user "it's live"

- [ ] GitHub Release for the tag exists and is not a draft.
- [ ] `latest.json` on the release has both Windows platform keys with
      signatures (spot check: download it, eyeball the two `platforms.*`
      entries).
- [ ] `publish-downloads.yml` was run manually with "Upload artifacts to
      doodleray.clickflare.click" checked.
- [ ] Only then say the update is live — merging the PR and pushing the tag
      are necessary but silently insufficient on their own.
