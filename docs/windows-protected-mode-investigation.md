# Windows Protected Mode Investigation

Date: 2026-06-29

## Current evidence

- Installed binaries on the affected machine were verified as DoodleRay app `5.4.4` and `DoodleRayService.exe` `5.4.4`.
- Proxy mode was working through the app-owned xray process.
- The active proxy HTTP port had dozens of established loopback connections, so local proxy readiness checks can run while the listener is under real load.
- Several `xray.exe api statsquery` helper processes were observed alive at the same time. Stats helpers must be bounded so they cannot accumulate.
- The protected-mode failure at 23:07 was not a simple "xray never started" case. Service diagnostics showed xray accepted protected-mode `http-in` traffic during the same attempt, while sing-box also logged failed connections to the same loopback HTTP bridge port.
- The user-facing failure came from the app-side WinINet compatibility proxy step: `HTTP proxy port 127.0.0.1:<port> is not ready`.
- Future reproduction and release-gate checks must use the canonical DoodleVPN
  test subscription from `docs/qa-test-subscription.md`; keep the raw URL only
  in the ignored `secrets/doodlevpn-test-subscription-url.txt` file.

## Current architecture

1. The React app builds a `vpn_connect` request with persisted local SOCKS/HTTP/API ports.
2. For Windows protected mode with xray-owned protocols, the app asks `DoodleRayTunnelService` to start both:
   - xray local SOCKS/HTTP/API inbounds;
   - sing-box TUN bridge.
3. The service waits for local proxy ports, adapter readiness, route readiness, and then returns `Connected`.
4. After the service has already opened the TUN path, the app applies WinINet system proxy compatibility.
5. If WinINet compatibility fails, the app stops the tunnel service and reports protected mode as failed.
6. Health currently treats WinINet proxy state as part of the protected-mode quorum when `systemProxyMode=set`.

## Likely root causes

### 1. Shared HTTP choke point

The xray TUN bridge currently sends TCP traffic from sing-box into xray's HTTP inbound, while the same HTTP inbound is also used for WinINet compatibility. That makes `http_port` a shared choke point during startup:

- TUN starts capturing normal system traffic.
- sing-box immediately opens many local connections to xray's HTTP inbound.
- The app simultaneously probes and applies WinINet proxy on the same HTTP port.

The logs showing "xray accepted http-in traffic" and "sing-box failed to connect to the same HTTP port" in the same window are consistent with a transient loopback/backlog/load race, not with a permanently missing listener.

### 2. Optional compatibility is treated as fatal

For "Whole computer" mode, the core protection is the TUN adapter and routes. WinINet proxy is a compatibility assist for proxy-aware apps, not the only path that makes apps use the VPN. Today, a transient WinINet compatibility failure tears down an otherwise-started TUN path. That converts a degraded compatibility feature into "VPN does not connect".

### 3. Port ownership is not explicit enough

The service starts the real engines, but the public success path does not return authoritative runtime ports, child PIDs, or phase timings to the UI. The UI mostly keeps using the persisted port settings and infers ports from health text. This makes retries, repair, and health reporting more fragile than necessary.

### 4. Helper process lifecycle was unbounded

Traffic stats polling starts `xray.exe api statsquery`. Without a timeout, failed stats calls can accumulate helper processes. This does not by itself explain the protected startup failure, but it makes the runtime dirtier and can add local pressure.

## Local fixes already prepared

- `sysproxy` loopback readiness now retries for up to 3 seconds instead of making a single 250 ms TCP attempt.
- xray stats collection now has a bounded timeout and kills the helper process if it does not exit.
- A local unsigned `5.4.5` installer was built for manual testing only. It was not released to production.

## Better next fixes

### A. Route TUN TCP through xray SOCKS, keep HTTP for WinINet only

Change the xray TUN bridge so sing-box uses xray's SOCKS inbound for TUN TCP and UDP. Keep xray HTTP inbound available only for WinINet/manual HTTP compatibility.

Expected effect:

- removes most TUN startup traffic from the HTTP compatibility port;
- reduces the chance that WinINet proxy apply races with the TUN bridge;
- keeps HTTP proxy support for apps that explicitly need HTTP proxy semantics.

### B. Do not fail protected mode only because WinINet compatibility failed

If service diagnostics prove TUN adapter, route coverage, xray/sing-box processes, and local SOCKS listener are alive, protected mode should stay up. WinINet should be reported as compatibility degraded and retried in the background.

Expected effect:

- Telegram/desktop apps can work through TUN even when WinINet is temporarily unavailable;
- users do not lose the whole tunnel because a secondary compatibility layer failed;
- health can distinguish `protected_core=ok` from `browser_compatibility=degraded`.

### C. Return authoritative runtime details from the service

Extend `TunnelStatus`/`ConnectResult.health` with:

- actual SOCKS/HTTP/API ports;
- xray and sing-box PIDs;
- phase timings;
- last successful local port probe time;
- service-side reason if a child exited.

Expected effect:

- UI no longer guesses ports from strings;
- support bundles can answer "which process should be listening on this port";
- automatic repair can target the right owner instead of generic cleanup.

### D. Two-phase protected startup

Longer-term, make protected mode startup explicit:

1. service prepares xray and returns local proxy readiness;
2. app applies WinINet compatibility while TUN is not yet flooding the local proxy;
3. service starts TUN and routes;
4. app verifies protected core health.

Expected effect:

- eliminates the current "TUN flood vs WinINet apply" race;
- preserves correct HKCU WinINet ownership because the app still writes user proxy settings.

## Release recommendation

Do not publish another production release until one local build is verified on the affected machine with:

1. proxy mode currently working;
2. install local candidate;
3. connect protected mode once;
4. confirm service diagnostics show connected state;
5. confirm no new stuck `xray.exe api statsquery` helpers accumulate;
6. confirm Telegram and browser traffic work;
7. confirm switching back to proxy mode restores/updates WinINet cleanly.
