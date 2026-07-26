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

- `APPLE_DISTRIBUTION_CERTIFICATE_BASE64` and
  `APPLE_DISTRIBUTION_CERTIFICATE_PASSWORD`;
- `MAC_INSTALLER_DISTRIBUTION_CERTIFICATE_BASE64` and
  `MAC_INSTALLER_DISTRIBUTION_CERTIFICATE_PASSWORD`;
- `MACOS_APP_STORE_HOST_PROFILE_BASE64`;
- `MACOS_APP_STORE_EXTENSION_PROFILE_BASE64`;
- `APPLE_TEAM_ID`;
- `APP_STORE_CONNECT_API_KEY_ID`, `APP_STORE_CONNECT_ISSUER_ID`, and
  `APP_STORE_CONNECT_PRIVATE_KEY`.

CDN publication requires `DOWNLOADS_SSH_PRIVATE_KEY`, the pinned OpenSSH
known-hosts content in `DOWNLOADS_SSH_KNOWN_HOSTS`. Host/user/port/root may be
configured with the documented `DOWNLOADS_*` repository variables.
`DOWNLOADS_SSH_USER` is mandatory and must be a dedicated least-privilege
deploy account; there is no `root` fallback. Host-key
learning is disabled: rotate the pin explicitly after verifying the new key out
of band. The secret contains complete OpenSSH `known_hosts` line(s), including
the `[host]:port` form when a non-default port is used; an unverified
`ssh-keyscan` result is not sufficient evidence for rotation.

## Order and idempotency

The Windows app is built once. The retained artifact is used for QA, the CDN,
and GitHub Release—deploy jobs never rebuild it. A dry run keeps the packaged
bytes on its ephemeral runner and does not upload artifacts or write caches.
When both targets are enabled, publication order is:

1. upload or verify the immutable CDN version directory when Windows is
   enabled;
2. submit the signed macOS build only after that Windows upload succeeds in a
   combined release;
3. after every enabled target succeeds, create or verify the exact tag and
   GitHub Release, including target/source provenance;
4. atomically promote the Windows CDN `latest.json` as the last mutation.

A macOS-only release still creates the immutable tag and GitHub Release with
the signed-handoff digest provenance, and never runs Windows latest promotion.
Apple exposes build identity but no artifact SHA, so an exact existing
bundle/version/macBuild rerun is a no-op with that residual limitation.

An existing file set with identical hashes is a no-op. Different hashes for an
existing version stop the release. Published artifacts are never overwritten;
rollback is a new SemVer release.

Apple review remains asynchronous. A successful workflow proves submission,
not App Store approval.

## Runtime component updates

`runtime-updates.yml` resolves each moving upstream release once, stores that
exact `runtime-versions.json` plus its digest as the workflow handoff, and uses
the same snapshot for every smoke and update job. It may open a pull request to
`main`; it never auto-merges or publishes production bytes. A human reviews the
resolved versions, hashes, tests, and diff before merge.

## Before production

Run the workflow with `dry_run=true`, then complete the Windows install/update
matrix and macOS TestFlight upgrade checks in `docs/release-gate.md`. Until
those external checks are attached, the status is **RC only, production
blocked**.
