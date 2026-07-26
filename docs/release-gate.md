# Production release gate

`release-production.yml` is the only production workflow. Run it first with
`dry_run=true`; external publication stays disabled until every applicable
gate below has evidence.

## Automated gates

- `npm test`, production frontend build, workflow policy checks;
- Rust format, library tests, service tests, and checks for both Windows bins;
- release metadata, compatibility identities, updater endpoint/public key;
- pinned SHA-256 verification for Xray, sing-box, and Wintun downloads;
- exactly one Windows Tauri build with its mandatory updater `.sig` file;
- cryptographic verification of that `.sig` against the exact updater public
  key in `src-tauri/tauri.conf.json` before release staging;
- Apple signing, provisioning, sandbox, entitlements, and App Store upload
  credentials for the enabled macOS target;
- immutable artifact hashes and exact source SHA at current `main`;
- `git diff --check`.

Windows Authenticode is not used and is not a pass/fail criterion. Tauri
updater signing remains mandatory. Apple signing remains mandatory.

## Windows evidence

- clean install: Windows 10 22H2, Windows 11 23H2/24H2, Server 2022;
- in-place update from 5.9.1 and supported 6.x, including an active VPN;
- login/device/session/storage migration without re-entry;
- connect/disconnect, reboot, sleep/wake, and network change;
- stale WinINet/PAC/NRPT/routes/adapters/orphan recovery;
- no impact on third-party Xray, sing-box, proxies, or VPN clients;
- identical SHA-256 bytes in QA, GitHub Release, and CDN;
- staging updater smoke through `latest.json`.

Use `docs/windows-pc-qa-play2go.md` and `scripts/windows-qa/`. Unsigned local
installers are RC-only; they do not need a Windows certificate.

## macOS evidence

- clean install and App Store 5.9.1 upgrade;
- host and Network Extension signature/entitlement validation;
- session/keychain migration, connect/disconnect, sleep/wake, network change;
- TestFlight smoke on Intel and Apple Silicon;
- confirmation that the App Store build has no Tauri self-updater.

## Publication rule

Do not publish merely because compilation passed. Production requires explicit
approval after the external OS matrix. Missing evidence means:

```text
RC only, production blocked.
```

After release, verify external hashes and updater behavior. Never overwrite a
published version; fix or rollback with a new SemVer version.
