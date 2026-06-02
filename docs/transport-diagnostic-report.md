# DoodleRay Transport Diagnostic Report

## 2026-06-02 Hotfix: Windows TUN/Xray Connect Spinner

### Scope

- Route: `xray + TUN bridge`
- Build observed: local `5.0.5`
- Xray core: `v26.6.1`
- Platform: Windows

### Evidence

- `DoodleRay.exe`, `xray.exe`, and `sing-box.exe` remained running after the user tried to stop the connection.
- Xray was listening on local SOCKS/HTTP ports.
- `singbox_tun.log` showed active TUN routing through the local Xray SOCKS outbound.
- `tun_config.json` showed the expected bridge shape: TUN inbound -> local SOCKS outbound -> Xray.

### Classification

- Proven: the TUN bridge and Xray can remain alive while the UI is stuck in `connecting`.
- Proven: `vpn_disconnect` can return `Already disconnected` before cleaning engines if `CONNECTION_STATE` is still false.
- Proven: the connect button is disabled during `connecting`, so the user cannot cancel a hung connection from the main control.
- Likely: a frontend invoke that hangs or errors leaves the UI in `connecting` because there is no connect timeout or cancellation path.

### Minimal Fix Plan

- Make `vpn_disconnect` always attempt process and proxy cleanup.
- Allow the main connection button to cancel while `connecting`.
- Add a frontend connect timeout that calls `vpn_disconnect` and returns the UI to `disconnected`.
- Avoid dev-mode simulated success inside a real Tauri runtime.

### Android Test Path

- Not touched. No Android build or runtime path is changed.

### iOS Test Path

- Not touched. No iOS build or runtime path is changed.

## Windows Tunnel Service MVP Implementation Notes

### Implemented Service Path

- Added production Windows service protocol on `\\.\pipe\DoodleRay.TunnelService.v1`.
- Service name is `DoodleRayTunnelService`.
- Runtime files are written under `C:\ProgramData\DoodleRay\runtime`.
- `StartTunnel`, `StopTunnel`, `GetStatus`, `Hello`, and `PrepareForUpdate` use typed JSON commands.
- Routine Windows TUN connect/disconnect no longer uses `.bat`, `cmd.exe`, or per-connect `ShellExecuteW("runas")`.
- Service owns child `xray.exe`/`sing-box.exe` handles and a Windows Job Object with kill-on-close.
- Cleanup targets only service-owned child handles; it does not use global `taskkill /IM sing-box.exe`.
- Legacy Windows `stop_tun()`/`stop_tun_for_update()` now no-op in production Windows paths after the service-owned cleanup path runs, so they do not invoke visible `taskkill`, `cmd.exe`, or UAC during disconnect, update, quit, or shutdown.
- `DoodleRay Tunnel` is now the stable TUN interface name.
- The default DoodleRay TUN IPv4 prefix moved to `172.30.255.1/30` to avoid observed local conflicts with existing `happ-tun`/`tun0` prefixes.

### 2026-06-02 Prod-Ready Pass Updates

- NSIS is configured as `perMachine` and uses installer hooks to prepare/remove the old tunnel service before install, install the new `DoodleRayTunnelService`, start it, and verify `status` through IPC. A failed service install/status check aborts setup instead of leaving the app half-installed.
- The main Dashboard no longer shows a normal-flow `Install Tunnel Service` CTA. The bundled installer owns service installation; UI repair remains a diagnostics/backend action rather than the default connect path.
- Named-pipe service handling now runs multiple local pipe workers, flushes responses, and the client retries short connection races. This reduces the "service closed connection immediately" class of failures caused by single-instance IPC timing.
- The service creates the `DoodleRay VPN Users` local group during install, adds the installing user when available, configures service SID, configures SCM restart recovery, and locks `C:\ProgramData\DoodleRay` / `runtime` ACLs to SYSTEM and Administrators.
- The service validates `sing-box` configs with `sing-box check -c` before launching the TUN engine.
- Windows Xray cleanup no longer uses global `taskkill /IM xray.exe`; service-owned Full Computer mode uses child handles/job object cleanup instead.
- The service requires `sing-box.exe` next to the installed service binary and no longer falls back to a generic `sing-box` executable name on Windows.
- Fresh `DoodleRayService.exe` is copied into the Tauri resource bundle before NSIS packaging, because direct `cargo build` output is not automatically bundled as a resource.

### IPC Hang Fix

- Initial synchronous `StartTunnel` implementation could occupy the only pipe instance while readiness waited.
- That created the same user-facing failure mode as an endless connecting spinner: the tunnel could be up, but UI could not query status or cancel.
- `StartTunnel` now starts a background worker and returns `connecting` immediately.
- Backend polls `GetStatus` until `connected` or `failed`.
- `StopTunnel`, `PrepareForUpdate`, and a new start increment an operation generation token so stale workers cannot mark a newer tunnel as connected or failed.

### Smoke Results

- `npm run build`: passed.
- `cargo build --release --manifest-path src-tauri\Cargo.toml`: passed.
- `cargo test --manifest-path src-tauri\Cargo.toml`: passed, 6/6 tests.
- `npm run tauri build -- --bundles nsis`: produced `src-tauri\target\release\bundle\nsis\DoodleRay_5.1.4_x64-setup.exe`; local command exits non-zero only because `TAURI_SIGNING_PRIVATE_KEY` is unavailable for updater artifact signing.
- Service status IPC from non-elevated process: passed.
- `PrepareForUpdate`: passed; service stopped and released `DoodleRayService.exe`.
- Safe direct TUN smoke with `auto_route=false`: passed.
- Safe direct TUN startup timings observed:
  - `singbox_ready`: about 684 ms.
  - `adapter_ready`: about 1529 ms.
  - `total_connect`: about 1529 ms.
- Immediate cancel test:
  - `StartTunnel` returned `connecting`.
  - `StopTunnel` after roughly 100 ms returned `disconnected`.
  - No `DoodleRay Tunnel` adapter remained.
  - No service-owned `sing-box.exe` remained.
- Existing non-DoodleRay processes/adapters were left untouched:
  - `happ-tun`.
  - `tun0`.
  - unrelated `sing-box.exe` PIDs observed on the machine.

### Remaining Manual Validation Before Release

- Install the generated NSIS setup through UAC and verify the installed `C:\Program Files\DoodleRay\DoodleRayService.exe` is the new binary, service SID/recovery are configured, and Dashboard does not open `127.0.0.1` or show the old install CTA.
- Test a real subscription/profile in Full Computer mode with the service installed and UI running non-admin.
- Test Xray plus TUN with a real XHTTP/VLESS profile and verify SOCKS readiness before TUN readiness.
- Test five consecutive connect/disconnect cycles with no UAC prompts after one-time service install.
- Test app update from an installed build while connected and disconnected.
- Confirm installer bundles `DoodleRayService.exe`, `sing-box.exe`, `wintun.dll`, and `xray-core/*` in the final NSIS artifact.

## 2026-06-02 Windows Tunnel Service MVP Update

### Implemented Locally

- Added typed tunnel service protocol in Rust with `Hello`, `GetStatus`, `StartTunnel`, `StopTunnel`, and `PrepareForUpdate`.
- Replaced the pipe sketch with `DoodleRayTunnelService` command modes: `run-service`, `install`, `uninstall`, `start`, and `stop`.
- Moved Windows full-computer TUN startup to the service path for Xray+TUN and direct sing-box TUN.
- Removed Windows production fallback from per-app TUN routing: service failure now returns a setup/readiness error instead of silently starting the old elevated bat path.
- Added service-owned runtime files under `C:\ProgramData\DoodleRay\runtime`.
- Added a Windows Job Object with kill-on-job-close so service cleanup targets only child processes owned by the current tunnel graph.
- Added readiness phases for xray port readiness, sing-box startup, adapter discovery, routes readiness, connected, and failed.
- Added one-time UI install CTA for missing full-computer Windows components.
- Added updater preparation that asks the service to stop owned tunnel processes and then self-stop before the installer replaces binaries.

### Verified Locally

- `npm run build` passes.
- `cargo build --release --manifest-path src-tauri\Cargo.toml` passes and builds both `DoodleRay.exe` and `DoodleRayService.exe`.
- Running `DoodleRayService.exe` without args prints usage.
- Running `DoodleRayService.exe start` before install returns Windows service error `1060`, proving the binary reaches SCM control code.

### Still Requires Manual Windows Runtime Validation

- Install service through UAC and verify it appears as `DoodleRayTunnelService`.
- Connect/disconnect TUN five times from a non-admin UI without repeated UAC.
- Verify Xray+TUN and direct sing-box TUN create `DoodleRay Tunnel`, reach `connected`, and leave no orphaned child processes after cancel/failure.
- Verify updater can replace `DoodleRayService.exe` after `PrepareForUpdate` self-stop.
- Verify support/log output does not contain raw configs, UUIDs, subscription URLs, SNI, or private keys.

## 2026-06-02 Windows Tunnel Service Runtime Smoke Follow-up

### New Findings

- Proven: the first installed service instance exposed the pipe with default LocalSystem ACLs, and a non-elevated client received `Access denied` on `\\.\pipe\DoodleRay.TunnelService.v1`.
- Proven: `TunnelStatus.timings_ms` as `u128` produced `IPC decode failed: u128 is not supported`; status protocol must use `u64`.
- Proven: `src-tauri\sing-box` is not a Windows executable (`CF FA ED FE`, Mach-O), while the working Windows binary is `sing-box.exe` (`MZ`) from the installed app.
- Proven: SCM stop on the first service build could leave the service waiting inside `pipe.connect()` until another client connected.

### Fixes Applied

- Added explicit pipe `SECURITY_ATTRIBUTES` from SDDL:
  - LocalSystem full access.
  - Built-in Administrators full access.
  - `DoodleRay VPN Users` read/write access.
  - Remote pipe clients remain rejected through `PIPE_REJECT_REMOTE_CLIENTS`.
- Added a timeout around `pipe.connect()` so service stop/update can exit without waiting forever for a client.
- Changed tunnel timings from `u128` to `u64`.
- Added CLI smoke commands: `status` and `prepare-update`.
- Updated Windows release workflow to download `sing-box-1.13.2-windows-amd64.zip` and bundle `sing-box.exe`.
- Adjusted readiness phases so `adapter_ready` is recorded only after Windows reports `DoodleRay Tunnel`.

### Runtime Smoke Results

- Service installed and ran as `LocalSystem`.
- Non-admin `DoodleRayService.exe status` successfully connected to the pipe after the ACL fix.
- Non-admin `DoodleRayService.exe prepare-update` stopped the service and SCM reported `STOPPED`.
- Invalid `StartTunnel` smoke reached service IPC, launched the Windows `sing-box.exe`, failed on missing adapter as expected, and left no service-owned `sing-box.exe` child process.

## 2026-06-02 Investigation: Windows Full Computer Mode Startup Latency and UAC

### Scope

- Route: Windows `proxy_mode=tun`, user-facing "Full device / Whole computer" mode.
- Code observed: local Tauri desktop client, package/app version `5.0.8`.
- Engines: `xray-core` for XHTTP/raw Xray profiles plus `sing-box.exe` as the TUN bridge, or direct `sing-box.exe` TUN for non-Xray profiles.
- Platform: Windows.

### Evidence

- `src-tauri/src/lib.rs` calls `tun::start_tun_elevated(...)` for all full-computer TUN paths:
  - `xray + TUN bridge`
  - `sing-box TUN`
  - per-app TUN bridges used alongside system proxy when Workshop exe rules exist.
- `src-tauri/src/tun.rs` implements `start_tun_elevated` as:
  - stop existing `sing-box.exe`
  - write `tun_config.json`, `singbox_tun.log`, and `launch_singbox.bat` under `%TEMP%\DoodleRay`
  - if the app is already elevated, launch `cmd /c launch_singbox.bat`
  - otherwise call `ShellExecuteW(..., "runas", launch_singbox.bat, ...)`, which necessarily displays Windows UAC.
  - poll `tasklist` for `sing-box.exe` every 300 ms, then perform extra crash/log checks.
- `src-tauri/src/tun.rs` implements `stop_tun` by `taskkill /IM sing-box.exe /F /T`; if access is denied, it calls `ShellExecuteW(..., "runas", "cmd.exe", "/c taskkill ...")`, which can also trigger UAC during disconnect/cleanup.
- `src-tauri/src/bin/service.rs` and `src-tauri/src/ipc.rs` contain a named pipe service sketch (`\\.\pipe\DoodleRayServicePipe`), but current TUN start/stop paths do not call `ipc::send_command_to_service`; the service is not wired into `vpn_connect`/`vpn_disconnect`.
- `src-tauri/src/lib.rs` has `toggle_silent_autostart`, but that creates a scheduled task to launch the whole app with highest privileges at logon. It is not an always-running privileged tunnel manager and does not remove per-connect UAC unless the user is already running the app elevated.
- Before this investigation, `src/components/dashboard/ConnectionControls.tsx` disabled the main control while `status === "connecting"`, so a slow or stuck full-computer startup could not be cancelled from the primary button.
- Before this investigation, `ConnectionStatus` had no `disconnecting` state, so disconnect had no distinct animation or label.

### Classification

- Proven: the immediate UAC prompt is caused by `ShellExecuteW` with the `runas` verb in `start_tun_elevated` whenever the DoodleRay process is not already elevated.
- Proven: the startup latency includes process cleanup, temp file/script generation, Windows UAC/user decision time, external process spawn, and repeated `tasklist` polling before `vpn_connect` can return success.
- Proven: the existing named-pipe service is unused for the production TUN route, so the client currently lacks the installed privileged helper/service architecture used by mature Windows VPN clients to start/stop tunnels without prompting every connect.
- Proven: disconnect animation was missing at the state-model level; the UI only represented `disconnected`, `connecting`, and `connected`.
- Likely but unproven: Happ and similar mature clients avoid repeated UAC by installing a privileged Windows service/driver/helper once, then letting the non-elevated UI send authenticated local IPC commands to that service.
- Unknown because instrumentation is missing: exact per-phase timings for config generation, Xray startup, UAC acceptance, `sing-box.exe` startup, adapter creation, route application, and readiness.

### First Broken Edge

The first proven product-level broken edge is architectural:

```text
Non-elevated UI
-> vpn_connect
-> start_tun_elevated
-> ShellExecuteW("runas", launch_singbox.bat)
-> Windows UAC every TUN start
-> external sing-box process readiness polling
```

This path cannot deliver Happ-like one-click, no-prompt, fast full-computer mode because the privileged operation is performed by an ad-hoc elevated process at connection time instead of a pre-installed trusted tunnel manager.

### Quick Fixes Applied

- Added `disconnecting` to `ConnectionStatus`.
- The main power button now remains clickable during `connecting` and acts as cancel/cleanup.
- The UI now sets `disconnecting`, shows a spinner/`Unplug` animation, and displays localized disconnecting labels during manual disconnect and connect cancellation.
- Stale async connect completions are ignored with an operation id guard after the user cancels.

### Minimal Architecture Fix Plan

- Implement a Windows privileged tunnel helper as the production TUN owner:
  - installed once with UAC during app install, first-run setup, or an explicit "Enable full computer mode" setup step.
  - runs as LocalSystem or a tightly scoped service account.
  - exposes a local authenticated IPC surface to the Tauri UI.
  - validates caller identity and command schema.
  - owns `sing-box.exe` lifecycle, Wintun adapter creation, route changes, DNS/kill-switch state, and cleanup.
  - returns structured phase events: `stopping_previous`, `starting_core`, `creating_adapter`, `applying_routes`, `ready`, `failed`.
- Stop launching `.bat` files with `ShellExecuteW("runas")` for routine connect/disconnect.
- Move from process-name kill (`taskkill /IM sing-box.exe`) toward service-owned child process handles or job objects, so DoodleRay does not kill unrelated `sing-box.exe` processes.
- Add phase timing telemetry without secrets:
  - `xray_start_ms`
  - `xray_port_ready_ms`
  - `tun_helper_rpc_ms`
  - `singbox_spawn_ms`
  - `tun_adapter_ready_ms`
  - `route_apply_ms`
  - `total_connect_ms`
- Keep production invariants:
  - no raw route material in frontend logs
  - no profile secrets in support bundles
  - one active engine per tunnel
  - fail-closed cleanup on helper/service failure

### Windows Test Path

- Build frontend: `npm run build`.
- Build Rust/Tauri: `cargo check --manifest-path src-tauri/Cargo.toml`.
- Manual non-admin run:
  - select TUN/full-computer mode.
  - click Connect.
  - verify current version still prompts UAC for TUN start.
  - click the main button while connecting and verify it switches to disconnecting/cancel cleanup.
  - verify no orphaned `xray.exe` or `sing-box.exe` remains after cancellation.
- Manual elevated run:
  - launch DoodleRay as Administrator.
  - connect TUN mode.
  - verify no per-connect UAC prompt.
  - measure connect duration and compare with non-admin path.
- Future helper/service validation:
  - install helper once.
  - launch UI non-admin.
  - connect/disconnect TUN repeatedly.
  - verify no UAC prompts after install, structured phase progress appears, and cleanup is idempotent.

### Android Test Path

- Not touched. No Android build or runtime path is changed.

### iOS Test Path

- Not touched. No iOS build or runtime path is changed.
