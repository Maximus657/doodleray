# DoodleRay v6 — macOS App Store readiness

Status: **SIGNED APP READY FOR PACKAGE QA; SUBMISSION STILL BLOCKED.**

Last audited: 2026-07-17. Branch: `codex/v6-macos-app-store`.

Apple resources already registered for this release track:

- host bundle ID: `com.doodleray.doodleray`;
- Packet Tunnel Provider bundle ID: `com.doodleray.doodleray.DoodleRayVPN`;
- shared App Group: `group.com.doodleray.doodleray`;
- App Store Connect app name: `DoodleRay VPN`.

The v6 App Store flavor now compiles the Xray engine into a sandboxed
`NEPacketTunnelProvider`; it does not bundle or invoke the direct-distribution
`xray`, `sing-box`, administrator-script, or system-proxy runtime. A signed,
universal host app and extension pass the local bundle verifier. Submission is
still blocked by the missing Mac Installer Distribution certificate, the legal
seller/operator mismatch, incomplete review assets/information, and real-device
VPN QA.

## Release tracks

Keep these as separate products even if they share the React UI and most Rust
business logic:

| Track | VPN runtime | Packaging and updates |
|---|---|---|
| Direct macOS | Existing external engines; temporary administrator path | Developer ID signed + hardened runtime + notarized `.app`/`.dmg`; signed Tauri updater |
| Mac App Store | `NEPacketTunnelProvider` app extension controlled through `NEVPNManager`/`NETunnelProviderManager` | App Sandbox, Apple Distribution signing, Store provisioning profiles, App Store-managed updates |

The direct runtime must never be compiled into or reachable from the App Store
flavor. Runtime branching in JavaScript is not a sufficient boundary.

## Blocking engineering work

- [x] Confirm the seller is enrolled as an **organization** and Network
      Extensions/App Groups are enabled for the existing host and extension
      identifiers.
- [x] Add a native Packet Tunnel Provider app-extension target implementing
      `NEPacketTunnelProvider`.
- [x] Move the actual packet tunnel engine into the extension. The Store build
      cannot launch bundled command-line VPN engines with `osascript`, request
      administrator privileges, or use `networksetup` as its VPN architecture.
- [x] Add host-side `NEVPNManager` or `NETunnelProviderManager` integration for
      saving the protocol configuration, starting/stopping the tunnel, and
      observing `NEVPNStatus`.
- [x] Define a minimal shared App Group contract for configuration/status data;
      keep tokens and credentials in a correctly access-grouped Keychain.
- [x] Add separate host and extension entitlements: App Sandbox, outbound
      network access, approved `packet-tunnel-provider`, and the chosen App
      Group/Keychain groups.
- [x] Add matching Mac App Store provisioning profiles and Apple Distribution
      signing for both bundle identifiers. Both profiles and the distribution
      identity are installed on the release Mac.
- [x] Add an App Store-only Tauri config/build target with updater artifacts
      disabled. The frontend `app-store` channel is already implemented and
      delegates update installation to Apple.
- [ ] Add automated extension tests plus real-device connect/disconnect,
      sleep/wake, network-change, kill-switch, DNS/IPv6, upgrade, and uninstall
      tests. The host-safe and isolated-guest strategy is documented in
      [`vpn-isolated-qa.md`](vpn-isolated-qa.md); real Packet Tunnel acceptance
      remains pending an external SSD/QA Mac.

## Blocking product, privacy, and App Store Connect work

- [x] Publish a live HTTPS privacy policy and link it before sign-in/use. It
      must describe authentication/device data, VPN configuration, diagnostics,
      retention, deletion, subprocessors, and support-bundle behavior.
- [x] Disable automatic diagnostics telemetry by default. It now requires the
      explicit build flag `VITE_DOODLERAY_DIAGNOSTICS_TELEMETRY=1`.
- [ ] If diagnostics telemetry is ever enabled, add explicit consent and ensure
      App Store privacy answers and the privacy policy exactly match every sent
      field.
- [x] Confirm the Store build does not create accounts. It signs in to an
      existing subscription, so Apple's account-creation deletion rule does not
      apply to this version. The privacy policy provides a support deletion
      route; re-open this blocker if registration is added.
- [x] Show the VPN data-use disclosure required by guideline 5.4 before the user
      purchases or uses the VPN service.
- [ ] Prepare App Store privacy labels, support URL, marketing URL, review notes,
      demo credentials/instructions, screenshots, category, age rating, EULA,
      and export-compliance/encryption answers. Metadata, URLs, localizations,
      privacy labels, category, age rating, price, availability, App Store
      export questionnaire, build upload, and one real 1440x900 VM screenshot
      are saved; reviewer credentials/contact information, content rights, and
      the final build-selection save remain.
- [ ] Confirm and document the legal relationship between the App Store seller
      and the operator named in the privacy policy. Do not publish an invented
      controller/processor relationship.
- [ ] Complete App Store Connect tax/banking, metadata, privacy answers,
      provisioning profiles, build upload, and TestFlight/internal review. The
      app record, bundle IDs, capability switches, Developer Program agreement,
      signed package, processed build 60000, and initial screenshot are in
      place. Build 60000 is superseded by the login-contract fix; build 60001,
      final build selection, and TestFlight review remain.
- [x] Install a Mac Installer Distribution certificate, produce the signed
      `.pkg`, validate it, and upload build 60000 to App Store Connect.
- [x] Complete the Digital Services Act trader declaration in App Store
      Connect. The free-app agreement is active; a paid-app agreement is not
      required while the app has no paid download or in-app purchases.

## Hardening completed during this audit

- Platform-specific Tauri resources are separated, so macOS no longer requires
  placeholder Windows binaries to compile.
- The direct macOS CI path fails closed when signing/notarization secrets are
  missing and uses a real Developer ID hardened-runtime signature for bundled
  VPN engines instead of ad-hoc signing.
- Renderer state no longer writes secrets to `localStorage` before Keychain.
  Legacy plaintext app-data mirrors migrate one-way to Keychain and are deleted.
- First launch no longer silently enables autostart.
- Automatic launch/heartbeat/error telemetry is disabled unless explicitly
  enabled at build time.
- macOS process cleanup no longer uses global `pkill -f xray` or
  `pkill -f sing-box`; only app-owned processes are stopped.
- The embedded sing-box wrapper no longer writes full VPN configuration,
  server addresses, UUIDs, or credentials to its error log.
- The `app-store` update channel never invokes the Tauri self-updater.
- Xray-core is statically linked into the Packet Tunnel extension through a
  pinned universal libXray build; the Store bundle contains no child VPN engine
  executables.
- XcodeGen is the source of truth for extension Info.plist and entitlements, so
  regeneration no longer erases `NSExtension`, App Sandbox, Network Extension,
  or App Group declarations.
- The host and extension carry matching version `6.0.0` / build `60001`, exact
  application/team entitlements derived from installed provisioning profiles,
  and universal `arm64` + `x86_64` executables.
- App Store frontend and Rust build channels are both baked at compile time;
  the closed account API, Store-managed update policy, disabled autostart, and
  privacy URL cannot silently fall back to the direct flavor.
- The separately built Windows service is feature-gated and kept outside
  `src/bin`, preventing Tauri from copying it into the macOS application.
- The Store host waits for Network Extension to report `connected`; its health
  contract no longer checks direct-build SOCKS/HTTP listeners or tears down a
  healthy Packet Tunnel as a false failure.
- Direct-only controls that are not implemented by the Packet Tunnel track
  (Kill Switch, live core statistics, Windows diagnostics, and sandbox-temp
  support export) are not shown in the Store UI.
- The activation payload matches the strict backend device schema without
  speculative fields, and the bundle includes a valid tracking-free
  `PrivacyInfo.xcprivacy` matching the v6 API data inventory.
- The Packet Tunnel distribution target disables Xcode's base-entitlement
  injection, preventing the debug-only `get-task-allow` entitlement from
  entering the App Store extension signature.
- The Packet Tunnel extension declares the App Store-required display name,
  and the signed-bundle verifier fails closed if regeneration removes it.
- A host-safe libXray smoke test validates the embedded framework's lifecycle
  and loopback TCP proxying without touching host routes, DNS, or VPN state.

## Automated gate

Run:

```bash
./scripts/macos/verify-app-store-readiness.sh
```

The static gate currently passes all 34 checks. `build-app-store.sh`
additionally verifies the signed `.app`; `package-app-store.sh` fails closed if
the installer certificate is unavailable. `upload-app-store.sh` constructs a
symbol-bearing `.xcarchive`, validates it through Xcode, and uploads it to App
Store Connect using the signed-in Xcode account:

```bash
./scripts/macos/upload-app-store.sh
```

Passing the local gates means “ready for App Store upload,” not “approved for
submission.” TestFlight and the real-device VPN matrix remain mandatory.

## Current local evidence

- App Store `npm run build`: pass (3,472 modules).
- `cargo test --features app-store --lib`: 73 passed.
- `cargo clippy --features app-store --lib -- -D warnings`: pass.
- Windows-service source is feature-gated and separated from the macOS bundle
  graph. A full `x86_64-pc-windows-msvc` cross-compile cannot run on this Mac
  without the Windows C SDK/MSVC headers, so the Windows release still needs
  its native CI build before shipping.
- `npm audit --omit=dev`: 0 known vulnerabilities on 2026-07-17.
- `cargo audit`: 0 known vulnerabilities; warnings are transitive
  unmaintained/yanked/unsound advisories, mostly non-macOS or build-time paths,
  and require upstream Tauri dependency movement rather than an unsafe local
  override.
- `subsvc`, backend bot, and backend wlmaster: full Go tests pass;
  `govulncheck ./...` reports no reachable vulnerabilities after Go 1.26.5 and
  dependency updates.
- Local Apple Distribution signing identity: installed.
- Local Mac App Store installer signing identity: installed.
- Matching host and Packet Tunnel Provider App Store provisioning profiles:
  installed.
- Signed universal `DoodleRay VPN.app`: bundle verifier pass without launching
  it or interrupting the laptop's active VPN.
- Tart macOS 26 guest: a guest-specific development build passes signature
  validation and launches the v6 sign-in UI. Real tunnel acceptance still
  needs a dedicated reusable reviewer subscription code.
- Host-safe libXray loopback smoke: the exact embedded 26.7.11 framework starts,
  proxies TCP, and stops without creating a TUN device.
- Xcode validation accepted build 60000 and uploaded it to App Store Connect on
  2026-07-17. App Store Connect reports the binary as confirmed, symbols as
  included, non-exempt encryption as absent, and both `arm64` and `x86_64` as
  supported. That build is superseded; fixed build 60001 must be uploaded and
  selected before submission.
- A real 1440x900 screenshot from the isolated macOS guest is uploaded to the
  Store version; it shows the v6 sign-in UI and pre-use VPN data disclosure.
- Isolated Colima TUN smoke: Xray carries HTTPS and UDP DNS through a guest
  `/dev/net/tun`; host default route and DNS hashes stay unchanged.
- App Store Connect: English and Russian metadata, 4+ age rating, privacy
  labels, free pricing, manual release, and 173-market availability are saved;
  the privacy inventory is published.
- App Store preflight: 32 checks pass; only the installer identity fails.

## Primary references

- [Apple App Review Guidelines — 5.4 VPN Apps](https://developer.apple.com/app-store/review/guidelines/)
- [Apple App Sandbox](https://developer.apple.com/documentation/security/app-sandbox)
- [NEPacketTunnelProvider](https://developer.apple.com/documentation/networkextension/nepackettunnelprovider)
- [Network Extension entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.networking.networkextension)
- [TN3134: Network Extension provider deployment](https://developer.apple.com/documentation/technotes/tn3134-network-extension-provider-deployment)
- [App Store Connect: manage app privacy](https://developer.apple.com/help/app-store-connect/manage-app-information/manage-app-privacy/)
- [Tauri: App Store distribution](https://v2.tauri.app/distribute/app-store/)
- [Tauri: macOS code signing and notarization](https://v2.tauri.app/distribute/sign/macos/)
