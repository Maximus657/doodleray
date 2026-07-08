# DoodleRay Release Gate

This project must not publish a new production version for every isolated fix.
Release only after a complete local validation pass and an explicit approval to ship.

## Hard Rules

- Do not push a version tag until the local installed NSIS app has been tested.
- Do not push a Windows production tag for installer, updater, subscription
  import, proxy, protected/TUN, routing, DNS, WebView2, service, or diagnostics
  changes until the Play2Go Windows QA stand has also passed the relevant tests
  from `docs/windows-pc-qa-play2go.md`.
- Do not treat `cargo build`, `npm run build`, or `target/release/DoodleRay.exe` as production validation.
- Do not manually deploy a `cargo build --bin DoodleRay` app binary for Windows
  QA. It can behave like a dev Tauri build and try to load `127.0.0.1:1420`.
  Use `npm run tauri build -- --bundles nsis` and test the packaged or
  NSIS-installed app.
- Test the NSIS-installed app from `C:\Program Files\DoodleRay`.
- Do not publish while a known Full Computer / TUN blocker is still reproducing.
- Do not publish while updater UI, service install, connect/disconnect, or shutdown behavior is visibly broken.
- Do not publish multiple patch versions for small follow-up edits. Batch fixes locally, then release one approved version.
- Do not publish without public, plain-language release notes in
  `docs/release-notes/<version>.md`. These notes are shown on the download page
  and must explain what changed for a normal user, not only internal technical
  details.
- All subscription, proxy, protected/TUN, split-routing, and speed QA uses the
  canonical DoodleVPN test subscription from `docs/qa-test-subscription.md`.
- v6 Windows releases must be signed by CI. The Windows workflow must fail if
  updater signing secrets, code-signing secrets, or Authenticode verification
  for app/service/core/installer artifacts are missing or invalid.

## Required Local Checks

Run these before any release tag:

```powershell
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml --bin DoodleRay
cargo check --manifest-path src-tauri/Cargo.toml --bin DoodleRayService
cargo build --release --manifest-path src-tauri/Cargo.toml --bin DoodleRayService
Copy-Item src-tauri\target\release\DoodleRayService.exe src-tauri\DoodleRayService.exe -Force
npm run tauri build
```

The local Tauri build can fail at updater signing when the private key is not present.
That is acceptable only if the NSIS setup exe was produced successfully.

## Required Installed Tests

- Install the locally built NSIS setup.
- Verify `C:\Program Files\DoodleRay\DoodleRay.exe` opens the bundled UI, not `127.0.0.1`.
- Verify `C:\Program Files\DoodleRay\DoodleRayService.exe status` responds.
- Verify service diagnostics include `network_snapshot`.
- With competing VPNs closed, run 3 Full Computer connect/disconnect cycles.
- For service/runtime-health changes, kill a service-owned core child process
  and verify the UI exits connected state, cleanup runs, and reconnect works
  without reboot.
- For service/runtime-health changes, kill only the main `DoodleRay.exe` while
  protected mode is connected, restart the UI, and verify the service tunnel is
  preserved, WinINet is reasserted from structured service runtime ports, and
  health remains `protected`.
- After UI reload and after disconnect, verify no long-lived
  `xray.exe api statsquery` helper process remains.
- Confirm no per-connect UAC prompt.
- Confirm no `taskkill.exe` windows or shutdown blockers.
- Confirm update banner text is readable and does not cover primary controls.

## Required Play2Go Windows QA

For Windows RCs that touch networking, installer/updater, runtime prerequisites,
or subscription fetching, run the matching scenarios in
`docs/windows-pc-qa-play2go.md` on the dedicated Play2Go stand before tagging.
Use `docs/windows-network-qa-tooling.md` to pick the required network evidence
tools for the changed area.

For v6 protected-mode RCs, use the committed Play2Go harness:

```powershell
.\scripts\windows-qa\Publish-DoodleRayQaInstaller.ps1 -LocalInstaller .\src-tauri\target\release\bundle\nsis\DoodleRay_5.9.0_x64-setup.exe
.\scripts\windows-qa\Invoke-DoodleRayV6QaGate.ps1 -InjectStaleWinInet
.\scripts\windows-qa\Invoke-Play2GoPowerShell.ps1 -ScriptPath .\scripts\windows-qa\Get-DoodleRayDeepQaSnapshot.ps1
.\scripts\windows-qa\Invoke-DoodleRayUpdatePathQa.ps1 -FromVersion 5.4.3
.\scripts\windows-qa\Invoke-DoodleRayUpdatePathQa.ps1 -FromVersion 5.4.4
.\scripts\windows-qa\Invoke-DoodleRayUpdatePathQa.ps1 -FromVersion 5.4.5 -InjectStaleWinInet -InjectCorporatePac
```

`Invoke-DoodleRayV6QaGate.ps1` is the minimum install/update hardening gate. It
verifies silent install, service presence, Authenticode status for installed
app/service/core binaries, stale WinINet repair setup, service JSON status, and
absence of `xray api statsquery` orphans. It does not replace manual UI connect
and subscription import testing; it makes those tests start from a known-good
installed baseline.

`Invoke-DoodleRayUpdatePathQa.ps1` covers the previous-public-version upgrade
gate: it installs 5.4.3/5.4.4/5.4.5 from GitHub Releases on the stand, installs
the RC over it (optionally with injected stale loopback WinINet and a synthetic
corporate PAC), and fails unless the updated service reports the RC version as
JSON, no statsquery orphan exists, and the corporate PAC survived untouched.
All three source versions must pass before a v6 production tag.

`-AllowUnsignedLocalRc` is allowed only for local smoke QA. Production release
approval requires the default signed gate to pass without that flag.

Attach a redacted evidence note to the release checklist that includes:

- app version, service version, and installer/updater path tested;
- the public release notes file path, with a short confirmation that the text is
  understandable for non-technical users;
- subscription import/refresh result and server count;
  use the canonical DoodleVPN test subscription stored in the ignored
  `secrets/doodlevpn-test-subscription-url.txt` file; see
  `docs/qa-test-subscription.md`;
- proxy-mode and protected-mode connect/disconnect result;
- split-routing result for Russian/direct test sites when relevant;
- protected-mode crash/recovery result when TUN/service/runtime-health changed;
- service-authored verdict, effective state, generation, runtime ports, and any
  degraded/fatal/warning checks;
- support bundle redaction result;
- any prerequisite detected on a clean server, such as WebView2 or VC++ runtime.

Raw provider credentials, subscription URLs, UUIDs, endpoint IPs, and private
keys must stay out of committed logs and screenshots.

## Public Release Notes

Every user-facing release must have a file:

```text
docs/release-notes/X.Y.Z.md
```

Required format:

```markdown
# DoodleRay X.Y.Z

Дата: 8 июля 2026

Коротко: one simple sentence explaining why this update matters.

- A normal-user benefit in one short sentence.
- Another benefit or fix in one short sentence.
```

Write for people who do not know what TUN, WinINet, Wintun, NRPT, or xray are.
If a technical fix matters, translate it into user impact: for example,
“режим «Весь компьютер» теперь лучше восстанавливается без перезагрузки” instead
of “fixed stale adapter generation mismatch”.

`scripts/release/Publish-DoodleRayDownloads.ps1` refuses to publish if this file
is missing or does not contain `Коротко:` and at least one bullet. The publish
script copies the notes into the immutable release folder and updates the public
version history on the download page.

## Production Release Approval

Only after the installed test and required Play2Go QA pass, ask for explicit
approval to ship.
If approval is not given, keep changes local and do not tag.
