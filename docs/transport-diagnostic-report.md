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
