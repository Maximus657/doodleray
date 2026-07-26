# Production release runbook

Production has one entry point: GitHub Actions → `release-production` →
**Run workflow** from `main`.

## Inputs

- `source_sha`: the full 40-character SHA currently at `main`.
- `dry_run`: keep enabled for rehearsal. A dry run builds and validates both
  enabled targets but cannot create a tag, upload a build, publish a GitHub
  Release, submit to App Store Connect, or replace `latest.json`.

Version and targets come only from `release/release.json`; production currently
requires both target flags to be enabled. The workflow rejects
a version older than the live direct-channel version (equality is allowed only
for an exact-hash idempotent rerun), a SHA other
than current `main`, a conflicting tag, missing signing material, and any
attempt to reuse an immutable version with different bytes.

## Required secrets

Windows requires `TAURI_SIGNING_PRIVATE_KEY` and, when applicable,
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. These sign Tauri updater artifacts and
are unrelated to Windows Authenticode. No Windows PFX or certificate is a
release prerequisite.

The enabled App Store target fails closed without:

- `APPLE_CERTIFICATE` and `APPLE_CERTIFICATE_PASSWORD` (the imported keychain
  must contain Apple Distribution and Mac Installer Distribution identities);
- `MACOS_APP_STORE_HOST_PROFILE_BASE64`;
- `MACOS_APP_STORE_EXTENSION_PROFILE_BASE64`;
- `APP_STORE_CONNECT_API_KEY_ID`, `APP_STORE_CONNECT_ISSUER_ID`, and
  `APP_STORE_CONNECT_API_PRIVATE_KEY`.

CDN publication requires `DOWNLOADS_SSH_PRIVATE_KEY`; host/user/port/root may
be overridden with the documented `DOWNLOADS_*` repository variables.

## Order and idempotency

The Windows app is built once. The retained artifact is used for QA, the CDN,
and GitHub Release—deploy jobs never rebuild it. Publication order is:

1. upload or verify the immutable CDN version directory;
2. upload the signed macOS build to App Store Connect;
3. create or verify the exact tag and publish the GitHub Release;
4. atomically promote the CDN `latest.json` as the last mutation.

An existing file set with identical hashes is a no-op. Different hashes for an
existing version stop the release. Published artifacts are never overwritten;
rollback is a new SemVer release.

Apple review remains asynchronous. A successful workflow proves submission,
not App Store approval.

## Before production

Run the workflow with `dry_run=true`, then complete the Windows install/update
matrix and macOS TestFlight upgrade checks in `docs/release-gate.md`. Until
those external checks are attached, the status is **RC only, production
blocked**.
