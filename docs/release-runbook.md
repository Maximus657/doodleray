# Production release runbook

Production has one entry point: GitHub Actions → `release-production` →
**Run workflow** from `main`.

## Inputs

- `source_sha`: the full 40-character SHA currently at `main`.
- `dry_run`: keep enabled for rehearsal. A dry run builds and validates every
  enabled target but cannot create a tag, upload a build, publish a GitHub
  Release, submit to App Store Connect, or replace `latest.json`.

Version and targets come only from `release/release.json`. Windows-only,
App-Store-only, and combined releases are supported; both target flags cannot
be disabled. For a Windows target, the workflow rejects a version older than
the live direct-channel version (equality is allowed only for an exact-hash
idempotent rerun), a conflicting tag, and any attempt to reuse an immutable
version with different bytes. Every target rejects a SHA other than current
`main` and missing applicable signing material.

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

CDN publication requires `DOWNLOADS_SSH_PRIVATE_KEY` and the pinned OpenSSH
known-hosts content in `DOWNLOADS_SSH_KNOWN_HOSTS`. Host/user/port/root may be
overridden with the documented `DOWNLOADS_*` repository variables. Host-key
learning is disabled: rotate the pin explicitly after verifying the new key out
of band. The secret contains complete OpenSSH `known_hosts` line(s), including
the `[host]:port` form when a non-default port is used; an unverified
`ssh-keyscan` result is not sufficient evidence for rotation.

## Order and idempotency

The Windows app is built once. The retained artifact is used for QA, the CDN,
and GitHub Release—deploy jobs never rebuild it. A dry run keeps the packaged
bytes on its ephemeral runner and does not upload artifacts or write caches.
When both targets are enabled, publication order is:

1. upload or verify the immutable CDN version directory and submit the signed
   macOS build to App Store Connect;
2. after both succeed, create or verify the exact tag and publish the GitHub
   Release;
3. atomically promote the CDN `latest.json` as the last mutation.

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
