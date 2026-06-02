# Windows Full Computer / TUN Release QA Report

Date: 2026-06-02

## Scope

- Windows Full Computer / TUN service path.
- Installer-owned `DoodleRayTunnelService`.
- Connect/disconnect/update/shutdown cleanup.
- Privileged IPC/security review for the current working-tree patch.

## Automated Checks

- `cargo build --release --manifest-path src-tauri\Cargo.toml`: passed.
- `npm run build`: passed.
- `cargo test --manifest-path src-tauri\Cargo.toml`: passed, 6/6 tests.
- `npm run tauri build -- --bundles nsis`: produced the NSIS setup exe.
- Local NSIS command exit status: non-zero only because `TAURI_SIGNING_PRIVATE_KEY` is not present for updater artifact signing.

## Generated Installer

- Path: `src-tauri\target\release\bundle\nsis\DoodleRay_5.1.4_x64-setup.exe`
- Built after Windows service, IPC, runtime ACL, and cleanup fixes.

## Security Scan

- Codex Security diff scan completed for the Windows TUN/service working-tree patch.
- Final markdown report: `%TEMP%\codex-security-scans\DoodleRay PC\localpatch_20260602150515\report.md`
- Final HTML report: `%TEMP%\codex-security-scans\DoodleRay PC\localpatch_20260602150515\report.html`
- Surviving reportable findings: none.

## Fixed During QA

- Runtime config secret exposure risk:
  - `C:\ProgramData\DoodleRay` and `runtime` now get explicit ACL lockdown.
  - ACL inheritance is removed.
  - SYSTEM and Administrators get full control.
  - Authenticated Users and normal Users grants are removed.
- Blind process kill risk:
  - Managed-port cleanup no longer kills arbitrary PIDs.
  - Windows cleanup verifies that the PID executable is DoodleRay-owned and under the current app directory before invoking `taskkill`.
- Unreachable legacy cleanup noise:
  - Windows `stop_tun()` and `stop_tun_for_update()` no longer contain unreachable `taskkill`/`runas` cleanup bodies.

## Current Installed-Machine Status

- The current shell is non-admin.
- The new NSIS installer was not applied locally because it requires UAC.
- The currently installed service still shows old SCM settings:
  - `SERVICE_SID_TYPE: NONE`
  - failure recovery reset period `0`
- This proves the local installed service has not yet been upgraded by the new installer.

## Release Blocker

Before production release, one UAC-approved installed-app test is still required:

1. Run `DoodleRay_5.1.4_x64-setup.exe`.
2. Verify `DoodleRayTunnelService` is auto-installed and running as LocalSystem.
3. Verify `sc qsidtype DoodleRayTunnelService` is not `NONE`.
4. Verify `sc qfailure DoodleRayTunnelService` contains restart recovery actions.
5. Verify `C:\ProgramData\DoodleRay` and `C:\ProgramData\DoodleRay\runtime` are not readable by normal Users/Authenticated Users.
6. Start DoodleRay from Start Menu or Program Files, not `target/release`.
7. Verify the app does not open `127.0.0.1`.
8. Verify the main Dashboard does not show `Install Tunnel Service`.
9. Run five Full Computer connect/disconnect cycles with zero per-connect UAC prompts.
10. Verify no visible `taskkill`/OK dialogs on quit/shutdown/update preparation.

## Release Decision

Code and automated QA are ready for the final installed-app gate.

Do not publish to production until the UAC-approved installed-app test above passes on Windows.
