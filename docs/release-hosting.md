# Direct release hosting

The Windows updater uses the first-party endpoint:

```text
https://doodleray.clickflare.click/channels/direct/latest.json
```

Versioned files are immutable:

```text
https://doodleray.clickflare.click/releases/direct/<version>/<artifact>
```

Only `release-production.yml` may publish them. Its build job produces one
artifact set containing the installer, updater archive, its Tauri signature,
`latest.json`, provenance, and SHA-256 inventory. The deploy job uploads that
retained set without rebuilding.

`scripts/release/Publish-DoodleRayDownloads.ps1` has two fail-closed modes:

- `UploadImmutable`: identical existing hashes are a no-op; different hashes
  fail and nothing is overwritten.
- `PromoteLatest`: verifies the immutable directory and atomically replaces the
  channel manifest, with `latest.json` last.

Required secret: `DOWNLOADS_SSH_PRIVATE_KEY`. Optional variables are
`DOWNLOADS_SSH_HOST`, `DOWNLOADS_SSH_USER`, `DOWNLOADS_SSH_PORT`, and
`DOWNLOADS_REMOTE_ROOT`.

The Microsoft Store Win32 and direct-macOS distribution tracks are obsolete.
macOS production distribution is App Store Connect only.
