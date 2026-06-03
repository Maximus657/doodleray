# DoodleRay Release Gate

This project must not publish a new production version for every isolated fix.
Release only after a complete local validation pass and an explicit approval to ship.

## Hard Rules

- Do not push a version tag until the local installed NSIS app has been tested.
- Do not treat `cargo build`, `npm run build`, or `target/release/DoodleRay.exe` as production validation.
- Test the NSIS-installed app from `C:\Program Files\DoodleRay`.
- Do not publish while a known Full Computer / TUN blocker is still reproducing.
- Do not publish while updater UI, service install, connect/disconnect, or shutdown behavior is visibly broken.
- Do not publish multiple patch versions for small follow-up edits. Batch fixes locally, then release one approved version.

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
- Confirm no per-connect UAC prompt.
- Confirm no `taskkill.exe` windows or shutdown blockers.
- Confirm update banner text is readable and does not cover primary controls.

## Production Release Approval

Only after the installed test passes, ask for explicit approval to ship.
If approval is not given, keep changes local and do not tag.
