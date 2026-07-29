# macOS App Store readiness

Status: **the signed universal 6.0.2 (60018) bundle passed full local App Store
verification and was uploaded to App Store Connect on 2026-07-29; production
remains blocked pending TestFlight QA and the explicit submission handoff.**

This is the canonical macOS release document. Historical build notes are not a
release gate and must not be treated as evidence for the current source SHA.
The dated portal and release-state audit is in
[`audits/macos-app-store-audit-2026-07-29.md`](audits/macos-app-store-audit-2026-07-29.md).

## Immutable product contracts

- host bundle ID: `com.doodleray.doodleray`;
- Packet Tunnel Provider: `com.doodleray.doodleray.DoodleRayVPN`;
- App Group: `group.com.doodleray.doodleray`;
- App Sandbox and `packet-tunnel-provider` entitlements on host and extension;
- Apple-managed updates: App Store builds compile without the Tauri updater and
  set `createUpdaterArtifacts` to `false`;
- no direct `xray`/`sing-box` executables, administrator scripts, `.dmg`, or
  direct-macOS Tauri overlay in the App Store bundle path.

`release/release.json` is the only release input for marketing version and
`macBuild`. The production build passes those values to both the Tauri host and
Xcode extension, and signed-bundle verification reads the same file directly.

## Required GitHub secrets

The production workflow checks names only and fails closed if any value is
missing:

- `APPLE_DISTRIBUTION_CERTIFICATE_BASE64`;
- `APPLE_DISTRIBUTION_CERTIFICATE_PASSWORD`;
- `MAC_INSTALLER_DISTRIBUTION_CERTIFICATE_BASE64`;
- `MAC_INSTALLER_DISTRIBUTION_CERTIFICATE_PASSWORD`;
- `MACOS_APP_STORE_HOST_PROFILE_BASE64`;
- `MACOS_APP_STORE_EXTENSION_PROFILE_BASE64`;
- `APPLE_TEAM_ID`;
- `APP_STORE_CONNECT_API_KEY_ID`;
- `APP_STORE_CONNECT_ISSUER_ID`;
- `APP_STORE_CONNECT_PRIVATE_KEY`.

Both P12 files are imported into the same temporary keychain. Both supplied
profiles are decoded, installed on the clean runner, and verified against the
Team ID, exact bundle IDs, App Group, Network Extension entitlement, and
distribution profile type. Manual signing never uses
`allowProvisioningUpdates` to replace or repair supplied profiles.

## Reproducible gates

Portable static verification, including Windows Git Bash:

```bash
scripts/macos/verify-app-store-readiness.sh --static
```

A green source tree exits zero and prints both `STATIC PASS` and
`MACOS RELEASE BLOCKED`. The latter is deliberate: Windows cannot prove Apple
signatures, installed identities, embedded profiles, or a real bundle.

Full verification on a Mac after the signed build:

```bash
scripts/macos/verify-app-store-readiness.sh --full
```

Full mode hard-fails when macOS tools, either installed profile, Apple
Distribution identity, Mac Installer Distribution identity, Team ID, signed
host, signed extension, exact entitlements, privacy manifest, architectures,
or release versions are missing or inconsistent.

The 2026-07-29 signed handoff passed full mode for `6.0.2 (60018)`: host and
Packet Tunnel extension are universal, sandboxed, Apple-signed, exactly
provisioned, and match the release manifest. This proves the local signing and
bundle contract only; it does not prove a live VPN connection or an App Review
outcome.

Production also queries the official App Store Connect API. A new tuple must
have both a newer SemVer and a higher `macBuild`; an exact existing
`com.doodleray.doodleray` + version + build in `PROCESSING` or `VALID` state is
an upload no-op. API and upload errors remain fatal. Apple does not expose the
uploaded artifact SHA, so exact remote byte equality cannot be proven; the
GitHub Release retains source SHA and local signed-handoff digests with that
limitation explicit.

## Migration truth

The old direct 5.9.1 application uses bundle ID
`com.doodlevpn.doodleray`, while the App Store host is
`com.doodleray.doodleray`. macOS therefore treats them as different
applications. Ordinary in-place container and Keychain migration is
**unproven**, and access to the old sandbox container or Keychain items must
not be assumed from source inspection.

Do not replace or retire 5.9.1 until a real Mac/TestFlight exercise proves the
intended user path, session/device continuity (or an explicit re-login path),
Keychain behavior, Network Extension replacement, and rollback/support plan.
The 5.9.1 source and artifacts remain preserved by tags and Releases, not a
permanent branch.

## Remaining Mac-only release gates

- App Store Connect now has a manual-release `6.0.2` draft with its existing
  metadata and screenshot preserved. The remote `60017` build is actually
  `6.0.0`, so it cannot be attached to the `6.0.2` record. The verified
  `6.0.2 (60018)` build is uploaded and awaiting processing; attach it only
  after it becomes valid. Neither action is a release.
- TestFlight clean install and the real 5.9.1 transition path on a QA Mac where
  changing the VPN is safe;
- Intel and Apple Silicon connect/disconnect, DNS/IPv4/IPv6, sleep/wake, network
  change, relaunch, and uninstall checks;
- accepted-size screenshots from the exact Store build and completed
  accessibility labels based on that QA evidence;
- App Store Connect metadata/reviewer credentials and legal/export review;
- explicit human approval after attaching evidence for the exact source SHA.

## Verified source invariants

- The renderer-accessible `vpn_connect` command is fail-closed in an App Store
  build. Only `app_connect_location` may call the App Store tunnel path after it
  has obtained and validated a fresh closed-control-plane connection profile.
  `scripts/macos/app-store-static.test.mjs` verifies this call boundary.
- A signed routing policy constrains raw Xray configuration before App Store
  Packet Tunnel preparation: it retains only managed outbounds, removes
  balancers, and drops rules targeting removed outbounds. The Rust
  characterization test `managed_full_tunnel_removes_raw_alternate_bypasses`
  covers an untrusted alternate direct route.

Until those gates exist: **RC only, production blocked.**

References: [Tauri App Store distribution](https://v2.tauri.app/distribute/app-store/),
[Apple distribution signing](https://developer.apple.com/documentation/xcode/creating-distribution-signed-code-for-the-mac/),
[Network Extension entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.networking.networkextension),
and [App Store Connect API tokens](https://developer.apple.com/documentation/appstoreconnectapi/generating-tokens-for-api-requests).
