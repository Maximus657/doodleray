# External AI Prompt: DoodleRay Windows Full-Computer VPN Architecture

You are reviewing the Windows desktop architecture of a Tauri VPN client named DoodleRay. You do not have repository access, so this prompt includes the relevant facts from the code investigation. Please critique the architecture, compare it to mature Windows VPN client patterns, and propose a production-grade design for fast full-computer VPN connect/disconnect without repeated UAC prompts.

## Product Goal

DoodleRay has two routing modes:

- System Proxy mode: local SOCKS/HTTP proxy for browsers and apps that respect proxy settings.
- Full Computer / TUN mode: full-device VPN, used for games, apps that ignore proxy, Workshop split-tunneling rules, and kill switch.

Competitor behavior target: apps like Happ appear to enable full-computer mode very quickly, without asking for administrator confirmation on every connection, and with polished connect/disconnect progress states.

Current DoodleRay problems:

1. Full Computer mode can take a long time to connect.
2. Full Computer mode asks for Windows administrator confirmation.
3. Disconnect previously had no distinct animation/progress state.

## Stack

- Desktop app: Tauri v2, React, TypeScript, Zustand.
- Native backend: Rust.
- Windows TUN engine: external `sing-box.exe`.
- XHTTP/raw Xray profiles: external `xray.exe` plus a `sing-box.exe` TUN bridge.
- Non-Xray TUN profiles: direct `sing-box.exe` TUN.
- Wintun resource is bundled/copy-managed separately.

## Current TUN Route

For `proxy_mode = "tun"`, `src-tauri/src/lib.rs` calls `tun::start_tun_elevated(...)`.

This happens in these paths:

- `xray + TUN bridge`
- `sing-box TUN`
- per-app TUN bridge used with System Proxy mode when Workshop exe rules exist

The Xray + TUN route shape is:

```text
DoodleRay UI
-> Rust vpn_connect
-> start xray.exe on local SOCKS/HTTP ports
-> wait for SOCKS port
-> build sing-box config:
   TUN inbound
   DNS config
   route rules
   final outbound = local SOCKS 127.0.0.1:<socks_port>
-> start elevated sing-box.exe
```

The TUN bridge routes all TUN traffic to the local Xray SOCKS outbound. Workshop rules can add process-name direct/proxy/block routing.

## Current Elevation Implementation

`src-tauri/src/tun.rs::start_tun_elevated(config_json)` does the following:

1. Calls `stop_tun()`.
2. Resolves `sing-box.exe` from the resource/exe directory.
3. Writes temp files under `%TEMP%\DoodleRay`:
   - `tun_config.json`
   - `singbox_tun.log`
   - `launch_singbox.bat`
4. If the DoodleRay process is already elevated, runs:
   - `cmd /c launch_singbox.bat`
5. If the DoodleRay process is not elevated, calls Windows ShellExecuteW:
   - verb: `runas`
   - file: the `.bat` launcher
   - show: hidden
6. Polls `tasklist` for `sing-box.exe` every 300 ms.
7. If not running quickly, reads the log for `FATAL`, `ERROR`, or `panic`.

This means normal non-admin users get Windows UAC every time TUN mode starts.

`stop_tun()` currently runs:

```text
taskkill /IM sing-box.exe /F /T
```

If access is denied, it again calls `ShellExecuteW("runas", "cmd.exe", "/c taskkill ...")`, so disconnect/cleanup can also require elevation. It kills by process image name rather than by a service-owned process handle/job object.

## Existing But Unused Service Sketch

There is a Rust binary at `src-tauri/src/bin/service.rs`.

It creates a Windows named pipe:

```text
\\.\pipe\DoodleRayServicePipe
```

It accepts rough commands:

```text
StartTun <json>
StopTun
```

There is also `src-tauri/src/ipc.rs::send_command_to_service(command)`.

Important: this service path is not wired into production `vpn_connect` or `vpn_disconnect`. Current production TUN start/stop does not call `send_command_to_service`. The service also appears incomplete: it calls `tauri_app_lib::singbox::start_singbox`, which is not necessarily the same as the elevated external TUN path, and it needs hardening before production use.

## Existing Scheduled Task Feature

There is a setting called "Launch on Startup (Admin)" / `silentAdminAutostart`.

`toggle_silent_autostart` creates a scheduled task named `DoodleRay_SilentStart` with:

```text
/RL HIGHEST
/SC ONLOGON
```

This only launches the whole app elevated at user logon. It is not an always-running privileged tunnel manager. It avoids per-connect UAC only if the app process is already running elevated. It does not solve the general mature-client pattern where a non-elevated UI talks to a pre-installed privileged helper.

## Recent Quick UX Fixes

We added a frontend `disconnecting` status:

```ts
type ConnectionStatus = 'disconnected' | 'connecting' | 'connected' | 'disconnecting';
```

The main power button now:

- remains clickable during `connecting`
- acts as cancel/cleanup during `connecting`
- switches to `disconnecting`
- shows a spinner/Unplug animation
- ignores stale async connect completions using an operation id guard

This improves UX, but it does not fix the architectural UAC/latency root cause.

## Proven Findings

- Repeated UAC is caused by `ShellExecuteW` with the `runas` verb in `start_tun_elevated`.
- Startup latency includes cleanup, temp file/script generation, UAC/user decision time, external process spawn, adapter creation/routes, and process polling.
- The current app lacks a production wired privileged tunnel helper/service.
- A mature Windows VPN client likely installs a privileged service/helper once, then the non-admin UI sends authenticated local IPC commands for connect/disconnect.
- Exact timings are not yet instrumented, so per-phase bottlenecks are not measured.

## Security and Product Invariants

Do not weaken these:

- One active engine per tunnel.
- Fail-closed lifecycle.
- No raw VPN profile material in frontend logs, UI, support bundles, or telemetry.
- No secrets in diagnostics.
- No local proxy bridge unless explicitly part of the intended route.
- TUN mode owns full-device traffic.
- Cleanup must be idempotent.
- Kill switch and DNS behavior must not leak on failure.

## What We Need From You

Please produce a detailed architecture proposal for Windows that answers:

1. What is the best production pattern for fast full-computer VPN connect/disconnect without repeated UAC in a Tauri/Rust client?
2. Should DoodleRay use a Windows Service, a privileged helper process, scheduled task IPC, or another approach?
3. How should the service/helper be installed, updated, secured, and removed?
4. What account should it run under: LocalSystem, LocalService, or user elevated context?
5. How should IPC be designed and secured?
   - named pipe security descriptor
   - caller identity validation
   - command schema
   - replay/TOCTOU concerns
   - config secrecy
6. Should the helper own `sing-box.exe` as a child process, use a Windows Job Object, or link against a library instead of spawning?
7. How should DoodleRay avoid killing unrelated `sing-box.exe` processes?
8. What startup phases should be instrumented to identify actual latency?
9. What readiness signal is better than polling `tasklist`?
10. How should route/DNS/kill-switch cleanup work on crash, service restart, app exit, and Windows sleep/resume?
11. How should updates work if the helper is installed with privileges?
12. What migration plan can move from the current `ShellExecuteW("runas")` path to the helper architecture with minimal risk?
13. What should remain in the UI process, and what must move to the helper?
14. What test matrix is needed for Windows 10/11, admin/non-admin users, UAC on/off, Defender/AV interference, Wintun install states, and app upgrades?
15. What common mistakes in VPN helper/service implementations should we avoid?

Please be critical. Identify any unsafe assumptions in the current design. Propose an incremental implementation plan with milestones:

- immediate telemetry
- helper/service MVP
- secure IPC hardening
- lifecycle/cleanup hardening
- installer/updater integration
- full release test plan

Prefer practical Windows-native details over generic advice.
