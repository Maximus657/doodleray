# DoodleRay Transport Diagnostic Report

## 2026-07-23 Investigation: Windows fresh-install Xray TUN DNS loop

### Scope

- Route: Windows Full Computer mode using the service-owned Xray-to-sing-box TUN bridge.
- User-visible issue: a fresh v6.0.1 install takes tens of seconds to connect, retries once, then fails before it reports protected state.

### Evidence

- The supplied v6.0.1 support bundle reaches `adapter_ready`, `xray_ready`, and `routes_ready` on both attempts; Wintun creation, native route probes, and local listeners are not the first broken edge.
- Both attempts fail at the same service readiness check: the Windows resolver canary times out after TUN routes are installed.
- Xray records that it cannot resolve its own redacted VPN endpoint hostname. After failed cleanup, the machine's ordinary DNS and HTTPS diagnostics pass, so this is not a general resolver outage or a remote endpoint outage.
- `src-tauri/src/lib.rs` builds the Xray TUN bridge route rules in this order: sniff, hijack every DNS packet, then send `xray.exe` and `sing-box.exe` directly. DNS emitted by Xray therefore matches the earlier hijack rule, re-enters sing-box, and waits on the Xray proxy that needs that DNS result to connect.

### First Broken Edge

```text
Xray resolves its VPN endpoint during TUN startup
-> broad sing-box DNS hijack intercepts the engine's resolver packet
-> sing-box sends DNS through the Xray SOCKS proxy
-> Xray cannot connect until its endpoint has been resolved
-> DNS loop times out, readiness fails, and startup retries
```

### Classification

- Proven: the route-rule order creates this DNS loop for the service-owned Xray TUN bridge.
- Proven: the long duration is the two bounded DNS readiness attempts, not server latency.
- Proposed minimal fix: put the existing engine direct-bypass rule before the broad DNS-hijack rule, leaving user DNS interception and all routing policy rules unchanged.

### Windows Test Path

- Add a regression test that asserts engine bypass precedes DNS hijack in every Xray TUN bridge configuration.
- Build the Windows service and NSIS RC, then test a fresh Windows installation with a hostname-backed closed-API location: one protected connect, DNS/HTTPS canaries, and clean disconnect.

### Android and iOS Test Paths

- Not touched: this is Windows-only service and bridge configuration.

## 2026-07-23 Hotfix: Windows retained TUN routes after disconnect

### Scope

- Route: Windows Full Computer mode with the service-owned `DoodleRay Tunnel` adapter.
- User-visible risk: after Disconnect the child engine could exit while a retained Wintun interface still carried DoodleRay routes and DNS configuration; a following Connect also spent time in unnecessary cleanup.

### Evidence and fix

- On the Windows QA stand, after a completed disconnect the tunnel service was stopped while the named adapter remained `Up` with service-owned routes. This was a real cleanup failure, not a remote subscription or server failure.
- The service terminated its owned `sing-box` child, then assumed the Wintun adapter would disappear. Windows can retain that interface after process exit.
- The service now clears routes and resets DNS only on the exact adapter name `DoodleRay Tunnel`, then verifies that both counts are zero. A failed verification remains `cleanup_pending`; it is never silently treated as safe.
- The same cleanup, including the expensive stale-Wintun device inventory, is skipped only before a new connect when native inspection proves that there is neither an owned prior runtime nor a DoodleRay adapter. If first bring-up is repairable, its owned retry still runs the complete cleanup before attempt two. This removes a needless PowerShell startup delay without changing disconnect or repair cleanup.

### Verification

- Focused service tests cover route/DNS cleanup requirements and the absent-adapter fast path.
- A packaged Windows QA RC completed protected-mode split-routing/DNS validation twice after a clean disconnect: protected IPv4 and IPv6 canaries used `DoodleRay Tunnel`, a direct-process exclusion stayed direct, and DNS resolution completed.
- After the final normal Disconnect, the on-demand Windows service stopped and the QA stand reported zero retained DoodleRay adapter routes and zero retained DoodleRay adapter DNS servers.

## 2026-07-23 Hotfix: Windows protected mode exits before creating the TUN adapter

### Scope

- Route: Windows Full device mode with the Xray-to-sing-box TUN bridge.
- User-visible issue: connection waited for the adapter, then failed before the adapter appeared.

### Evidence and fix

- The service log identified a sing-box startup error from `dns-direct`: an explicit `detour: "direct"` targets sing-box's empty built-in direct outbound and is rejected by current sing-box.
- `dns-direct` remains the resolver selected for direct routing rules, including the RU/geo domain and IP selectors. Xray TUN continues to use real IP addresses, not FakeIP.
- Removed only the redundant explicit detour from the Windows physical UDP resolver. With no detour specified, sing-box uses its direct DNS behavior without binding the resolver to the empty outbound.
- Added a focused regression test that rejects reintroducing this invalid direct-detour configuration.

### Verification

- `cargo fmt --check` passed.
- `cargo test --release --lib` passed (101 passed, 3 ignored).

## 2026-06-16 Hotfix: Windows Shutdown Xray Teardown Error Flash

### Scope

- Route: Windows Browser & apps / system proxy mode and any in-process Xray-backed route.
- User-visible issue: during Windows shutdown, DoodleRay can show an Xray error for a few seconds before the computer continues shutting down.
- Platform: Windows desktop Tauri client.

### Evidence

- `src-tauri/src/lib.rs::run` only performed cleanup on final `tauri::RunEvent::Exit`; Tauri also emits `ExitRequested` earlier.
- `src-tauri/src/xray.rs` captured Xray stdout/stderr into `XRAY_LOGS` until the child process was killed and the pipe readers finished.
- `src/pages/Dashboard.tsx` polled `get_proxy_logs` while UI status was still `connected`, so late Xray socket teardown messages could be promoted to red UI errors during OS shutdown.
- Xray is already started with `CREATE_NO_WINDOW`, so the visible annoyance is the user-facing DoodleRay log strip, not a standalone Xray console window.

### Classification

- Proven: controlled shutdown/disconnect can produce late Xray stderr/stdout lines after Windows or DoodleRay starts tearing down sockets.
- Proven: those late lines are not a route-selection, subscription, or remote-server failure by themselves.
- Likely but unproven: the user's shutdown flash is one of these late Xray teardown/reset/cancel messages.
- Unknown because the exact user's shutdown line is not available: the precise Xray module and wording shown during shutdown.

### First Broken Edge

```text
Windows shutdown begins
-> DoodleRay UI still has status=connected briefly
-> Xray/local sockets are being torn down
-> Xray emits a reset/cancel/closed-connection line
-> Dashboard polls get_proxy_logs and classifies "failed" as error
-> user sees an unpleasant red Xray error during shutdown
```

### Fix Applied

- Added `xray::begin_shutdown()` and call it on `tauri::RunEvent::ExitRequested`, before final `Exit` cleanup.
- Added `XRAY_STOPPING` in `src-tauri/src/xray.rs`; while set, pipe-reader threads skip new Xray lines and `get_new_logs()` returns empty.
- `stop_xray()` now clears the Xray log buffer and keeps the stopping marker set until the next `start_xray()` resets it.
- Expanded the frontend filter in `src/pages/Dashboard.tsx` for non-actionable Xray teardown/reset lines with proxy context, while keeping startup/bind/readiness errors visible.
- Existing EOF response handling for subscription/traffic-limit diagnostics remains unchanged.

### Windows Test Path

- `npm run build`: passed for `5.3.1`; Vite reported only existing chunk/dynamic-import warnings.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`: passed, 26 passed, 2 ignored.
- `cargo build --release --manifest-path src-tauri/Cargo.toml --bin DoodleRayService`: passed.
- Fresh `src-tauri\DoodleRayService.exe` resource was replaced from `src-tauri\target\release\DoodleRayService.exe` and reports `ProductVersion/FileVersion = 5.3.1`.
- `npm run tauri build -- --bundles nsis`: produced:
  - `src-tauri\target\release\bundle\nsis\DoodleRay_5.3.1_x64-setup.exe`
  - `src-tauri\target\release\bundle\nsis\DoodleRay_5.3.1_x64-setup.nsis.zip`
- Local Tauri build exited non-zero only because `TAURI_SIGNING_PRIVATE_KEY` is not present locally; `latest.json` was not generated. Auto-update release must be produced by the GitHub Actions release workflow where the updater signing secret is available.
- Manual runtime follow-up:
  - Connect an Xray-backed profile in Browser & apps mode.
  - Quit DoodleRay from tray and verify no late red Xray reset/cancel error appears.
  - Reconnect after quit/restart and verify real Xray bind/start failures are still visible.
  - During Windows shutdown, verify the app closes without flashing an Xray teardown error.

### Android Test Path

- Not touched. No Android build or runtime path is changed.

### iOS Test Path

- Not touched. No iOS build or runtime path is changed.

## 2026-06-16 Investigation: Windows Browser & Apps HTTP Inbound Reset Warnings

### Scope

- Route: Windows Browser & apps / system proxy mode.
- User-visible issue: user sent Xray warnings like `APP/PROXYMAN/INBOUND: connection ends > proxy/http: failed to read http request > read tcp 127.0.0.1:<client>->127.0.0.1:<http-in>: wsarecv: An existing connection was forcibly closed by the remote host`.
- Platform: Windows desktop Tauri client.

### Evidence

- The screenshot shows Xray `PROXY/HTTP` inbound warnings, not a DoodleRay `FATAL` connect failure.
- The inbound listen side in the screenshot is a runtime HTTP proxy port, redacted here as `<http-in>`, not necessarily the default `10809`.
- `src-tauri/src/lib.rs::vpn_connect` reserves runtime loopback ports in non-TUN mode when requested proxy ports are busy, then updates `request.socks_port`, `request.http_port`, and `request.api_port`.
- `src-tauri/src/lib.rs::vpn_connect` waits for `request.http_port` before applying Windows system proxy when `system_proxy_mode == "set"`.
- `src-tauri/src/lib.rs::apply_system_proxy_mode` passes the final `request.http_port` to `sysproxy::apply_doodleray_proxy`.
- `src-tauri/src/sysproxy.rs::apply_doodleray_proxy` writes a simple WinINet `ProxyServer` value of `127.0.0.1:<http_port>` only after the HTTP proxy port is reachable.
- Local inspection on this workstation at the time of this entry showed no active DoodleRay system proxy and no listener on the default/runtime proxy ports, so the live user's exact Windows proxy state is not available here.

### Classification

- Proven: this log line is generated by the local HTTP proxy inbound after a loopback client closes a TCP connection before a complete HTTP request is read.
- Proven: current source code is intended to point WinINet at the final runtime HTTP port, including when the default `10809` is busy.
- Likely but unproven: if browsing still works, the warning is benign noise from browser/app preconnect, cancellation, probe, or shutdown behavior and should not be shown to users as a scary red error.
- Unknown because user diagnostics are missing: whether this user's Windows `ProxyServer` equals the runtime HTTP inbound port shown by the connected session.
- Unknown because user diagnostics are missing: whether another proxy/VPN/client changed WinINet after DoodleRay connected, leaving browsers pointed at a stale or wrong loopback port.

### First Broken Edge

For the screenshot alone, the first proven product edge is diagnostic/UX noise:

```text
browser/proxy-aware app opens local proxy TCP connection
-> client closes before sending a complete HTTP request
-> Xray logs PROXY/HTTP failed to read http request as warning
-> DoodleRay/user treats it as a connection failure
```

If the user's internet is actually broken, the first edge is not proven yet. The next check must prove whether Windows points at the active runtime HTTP inbound:

```text
DoodleRay connected in Browser & apps mode
-> Xray/sing-box HTTP inbound listens on 127.0.0.1:<http-in>
-> WinINet ProxyServer should be exactly 127.0.0.1:<http-in>
-> browser/app should connect to that port
```

### Minimal Fix Plan

- Ask the user for DoodleRay diagnostics immediately after the warning appears: active mode, connected message with SOCKS/HTTP ports, Windows `ProxyServer`, listeners for the active HTTP port, and whether pages actually fail to load.
- Treat isolated `failed to read http request` / `forcibly closed by the remote host` warnings from loopback HTTP inbound as non-fatal UI noise unless accompanied by a DoodleRay `FATAL`, closed HTTP port, wrong WinINet `ProxyServer`, or failed canary.
- Consider filtering or downgrading this specific inbound warning in the user-facing log strip while keeping it available in support diagnostics.
- If diagnostics show `ProxyServer` does not match the runtime HTTP port, fix the apply/restore/race path around runtime port selection and WinINet notification.
- If diagnostics show the active HTTP port is closed while `ProxyServer` still points to it, fix guardian/recovery so Windows proxy is restored immediately.

### Fix Applied

- Added a narrow frontend filter in `src/pages/Dashboard.tsx` for Xray HTTP inbound reset warnings that match all of:
  - `PROXY/HTTP`
  - `failed to read http request`
  - loopback-to-loopback endpoint pair
  - reset wording such as `forcibly closed by the remote host`, `connection reset by peer`, `wsarecv`, or `wsasend`
- The filter runs before generic `failed` lines are promoted to error logs, so this specific benign local reset no longer turns the user-facing log strip red.
- Other failed proxy lines remain visible as errors or warnings.

### Windows Test Path

- With `10809` deliberately occupied, connect Browser & apps mode and verify DoodleRay chooses runtime ports.
- Verify `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings\ProxyServer` equals `127.0.0.1:<runtime_http_port>`.
- Open Edge/Chrome and verify HTTPS pages load through the runtime HTTP proxy.
- Close/reopen a tab or cancel a page load and confirm any Xray `failed to read http request` warning does not change connection state to failed.
- Disconnect and verify WinINet proxy is restored or cleared according to the previous state.

### Android Test Path

- Not touched. No Android build or runtime path is changed.

### iOS Test Path

- Not touched. No iOS build or runtime path is changed.

## 2026-06-11 Hotfix: Windows TUN IPv4 Interface Readiness Race

### Scope

- Route: Windows Full Computer mode through `DoodleRayTunnelService`.
- User-visible issue: connect fails with `Full Computer components not installed or not ready: route readiness failed: DoodleRay Tunnel IPv4 interface metric is missing`.
- Platform: Windows desktop Tauri client.

### Evidence

- `src-tauri/src/bin/service.rs::start_tunnel` currently waits for the `DoodleRay Tunnel` adapter by name, then runs `set_doodleray_interface_metric`, then immediately starts route readiness polling.
- `set_doodleray_interface_metric` calls `Set-NetIPInterface -InterfaceAlias 'DoodleRay Tunnel' ... -ErrorAction SilentlyContinue`, so failure to find the IPv4 interface can be swallowed.
- `ensure_doodleray_route_preferred` treats a missing `Get-NetIPInterface -InterfaceAlias 'DoodleRay Tunnel' -AddressFamily IPv4` result as `DoodleRay Tunnel IPv4 interface metric is missing`, although the actual failing edge is the IPv4 interface object not being ready or not existing.
- The failure happens after the adapter name is visible and after `sing-box check -c` / process startup, so this is local Windows adapter/IP route readiness rather than a subscription, server, or remote blocking failure.

### Classification

- Proven: the current diagnostic message is misleading; it says the metric is missing when the IPv4 interface lookup itself failed.
- Proven: metric application is best-effort and silent before route readiness, so transient Windows adapter binding delays can surface as a hard Full Computer startup failure.
- Likely but unproven: affected machines have a race between Wintun adapter creation and IPv4 binding/route publication, or a local VPN/driver conflict that delays that binding beyond the current readiness window.
- Unknown because the user's diagnostics are not available here: whether IPv4 binding is disabled on `DoodleRay Tunnel`, a competing TUN is present, or Windows simply published the IPv4 interface late.

### First Broken Edge

```text
sing-box starts TUN
-> Windows exposes adapter named DoodleRay Tunnel
-> service marks adapter discovered
-> Get-NetIPInterface IPv4 still returns nothing
-> route readiness fails with misleading metric error
-> Full Computer mode aborts
```

### Fix Applied

- Add an explicit IPv4 interface readiness phase after adapter discovery and before route readiness.
- Apply the DoodleRay interface metric by adapter `InterfaceIndex`, not only by alias.
- Retry metric application while the IPv4 binding is coming up instead of doing a one-shot silent best-effort command.
- Improve failure text to distinguish adapter missing, IPv4 binding disabled/not ready, metric application failure, missing routes, and route preference failure.
- Keep the service-owned TUN architecture unchanged and do not add any local proxy/listener bridge.

### Windows Test Path

- `cargo check --manifest-path src-tauri/Cargo.toml --bin DoodleRayService`: passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`: passed, 24 passed, 2 ignored.
- Manual runtime follow-up:
  - Install the patched service.
  - Connect Full Computer mode five times after reboot and after sleep/resume.
  - Confirm phases include IPv4 readiness before route readiness.
  - Confirm diagnostics show `DoodleRay Tunnel` IPv4 interface metric `1`.
  - Confirm a failing machine now reports whether IPv4 binding is disabled/not ready instead of the old metric-missing message.

### Android Test Path

- Not touched. No Android build or runtime path is changed.

### iOS Test Path

- Not touched. No iOS build or runtime path is changed.

## 2026-06-07 Investigation: Gold Apple Split Routing Failure on Windows TUN

### Scope

- Route: Windows Full Computer mode through `DoodleRayTunnelService`.
- Engine: current installed `5.2.2` service path, `xray + sing-box TUN bridge`.
- User-visible issue: `2ip.ru` can look acceptable, but `https://goldapple.ru/` does not fully load; the same site reportedly loads through Happ.
- Platform: Windows desktop Tauri client.

### Evidence

- `DoodleRayService.exe status` reported `state=connected`, `phase=connected`, service version `5.2.2`.
- WinINet and WinHTTP proxy were disabled, so the observed route was Whole Computer/TUN, not Windows system proxy.
- `Test-NetConnection goldapple.ru -Port 443` succeeded through `DoodleRay Tunnel`; the failing edge is not raw TCP 443 reachability.
- System DNS for `goldapple.ru`, `www.goldapple.ru`, `sp.goldapple.ru`, and `mc.yandex.ru` returned sing-box fake-ip addresses from `198.18.0.0/15`, proving DNS hijack/fake-ip is active.
- Browser-like `curl` to `https://goldapple.ru/` returned HTTP 200, and the two main `/_static-files/...js` assets returned HTTP 200.
- Browser-like `GET https://goldapple.ru/web/api/v1/settings`, with home-page cookies, `Referer`, `Origin`, and Russian accept-language headers, returned HTTP 403. The page bootstrap calls this endpoint repeatedly, so a browser can appear stuck even when the shell HTML and static JS load.
- Playwright screenshot after 15 seconds under the current DoodleRay route showed only a partially loaded page shell/carousel, consistent with the settings API failing.
- External IP canary through the current DoodleRay route reported a non-RU/DE exit at the time of this investigation. Exact IP and hostname are redacted.
- `src/lib/connect-helpers.ts::buildConnectRequestFromState` sends Workshop routing rules only when `proxyMode === "tun"`.
- `src-tauri/src/lib.rs::build_singbox_config` compiles domain and process Workshop rules for the sing-box-owned paths.
- `src-tauri/src/lib.rs::build_xray_config` compiles domain Workshop rules for generated Xray configs.
- `src-tauri/src/lib.rs::inject_xray_inbounds` preserves raw Xray subscription routing and injects DoodleRay inbounds/API/DNS, but does not merge `req.routing_rules` into the raw Xray routing graph.
- In Windows `xray + TUN bridge`, sing-box mainly owns TUN capture and forwards to local Xray; domain routing for the proxied path must happen in Xray when Xray is the engine owner.

### Classification

- Proven: under the current DoodleRay Windows TUN route, `goldapple.ru` resolves and TCP connects, but the site bootstrap settings API returns HTTP 403.
- Proven: a simple `2ip.ru`-style external IP check is insufficient for this failure; the main HTML/static route and the API route have different outcomes.
- Proven: the current machine's active DoodleRay exit was not RU during the live check, despite the user's earlier observation that `2ip.ru` looked Russian.
- Proven: Workshop domain rules are compiled for generated Xray configs, but not merged into raw Xray configs handled by `inject_xray_inbounds`.
- Likely but unproven: Happ succeeds because it is using a different exit or route policy for the Gold Apple API edge.
- Likely but unproven: if the active DoodleRay server is a raw Xray subscription profile, user-added `goldapple.ru` / `*.goldapple.ru` Workshop rules will not affect Xray routing.
- Unknown because runtime config files are ACL-protected from the non-elevated client: whether the currently active raw Xray config already contains any Gold Apple or RU-site rule.

### First Broken Edge

Runtime edge proven by network checks:

```text
Gold Apple browser bootstrap
-> https://goldapple.ru/ returns 200
-> /_static-files JS returns 200
-> /web/api/v1/settings returns 403 over current DoodleRay route
-> app shell remains partially loaded / stuck
```

Product edge proven by code inspection:

```text
Raw Xray subscription profile
-> inject_xray_inbounds(raw, request)
-> DoodleRay inbounds/API/DNS injected
-> raw routing preserved and sanitized
-> request.routing_rules are not merged
-> Workshop domain rules cannot force Gold Apple direct/proxy/block on this path
```

### Minimal Fix Plan

- Add a shared helper that compiles Workshop domain rules into Xray routing rules using the same domain format currently used by `build_xray_config`.
- Call that helper from both `build_xray_config` and `inject_xray_inbounds`, inserting DoodleRay Workshop rules after the API/DNS rules and before preserved broad/default raw routing where possible.
- Preserve raw subscription routing and balancers; do not delete provider rules except existing unsupported-value sanitization.
- Add regression coverage showing a raw Xray config plus `goldapple.ru` and `*.goldapple.ru` Workshop rules emits the expected Xray field rules.
- Add a redacted diagnostics check for website bootstrap health: home page HTTP status, key static JS status, and `/web/api/v1/settings` status.
- UX follow-up: expose an actual exit-country canary from the active tunnel instead of relying on users to infer routing from `2ip.ru`.

### Windows Test Path

- `cargo test --manifest-path src-tauri/Cargo.toml --lib`.
- `npm run build`.
- Manual runtime follow-up:
  - Connect a raw Xray subscription profile in Whole Computer mode.
  - Add Workshop domain rules for `goldapple.ru` and `*.goldapple.ru` with the intended action.
  - Reconnect and verify the generated redacted routing summary includes those domain rules.
  - Load `https://goldapple.ru/` in a browser and verify `/web/api/v1/settings` no longer returns 403 for the intended route.
  - Confirm the active tunnel exit-country canary matches the user's expectation before judging site behavior.

### Android Test Path

- Not touched. No Android build or runtime path is changed.

### iOS Test Path

- Not touched. No iOS build or runtime path is changed.

## 2026-06-06 Hotfix: Windows Full Computer sing-box 1.13 Inbound Migration

### Scope

- Route: Windows Full Computer mode through `DoodleRayTunnelService`.
- Engine: bundled `sing-box.exe` 1.13.13 in this workspace.
- User-visible issue: connect to Germany failed at service readiness with `sing-box config check failed: initialize inbound[0]: legacy inbound fields are deprecated in sing-box 1.11.0 and removed in sing-box 1.13.0`.

### Evidence

- `src-tauri/singbox-core/go.mod` pins `github.com/sagernet/sing-box v1.13.2`, while `src-tauri/sing-box.exe version` reports 1.13.13 in this workspace; both are in the 1.13 line where legacy inbound fields are removed.
- `src-tauri/src/bin/service.rs::check_singbox_config` runs `sing-box check -c` before the service launches the TUN engine, so this failure happens before adapter creation.
- `src-tauri/src/lib.rs::tun_inbound_value` emitted TUN inbound fields `sniff: true` and `sniff_override_destination: false`.
- The official sing-box migration guide maps legacy inbound sniff fields to route actions such as `{ "action": "sniff" }`, which DoodleRay already emits in Full Computer route rules.
- `src/lib/config-generator.ts` also carried a stale helper generator shape with inbound `sniff` and a legacy DNS outbound route.

### Classification

- Proven: the service-owned TUN config check fails on sing-box 1.13.x because DoodleRay still generated legacy inbound sniff fields.
- Proven: the first failing edge is config validation, before any local process/core readiness, adapter readiness, DNS path, TCP HTTPS path, or exit canary can run.
- Proven: removing inbound sniff fields does not remove sniffing from the Full Computer route because route rule `{ "action": "sniff" }` is still emitted.
- Unknown because the failing live profile is unavailable here: whether the selected German exit has any independent server-side issue after config validation is fixed.

### First Broken Edge

```text
Full Computer connect
-> vpn_connect builds sing-box TUN config
-> tun_inbound_value emits inbound.sniff / inbound.sniff_override_destination
-> DoodleRayTunnelService writes singbox_tun_config.json
-> sing-box 1.13.x check -c rejects inbound[0]
-> tunnel never reaches adapter/DNS/TCP readiness
```

### Fix Applied

- Removed legacy `sniff`, `sniff_override_destination`, `sniff_timeout`, and `domain_strategy` exposure from generated TUN inbounds.
- Kept sniffing as the route action `{ "action": "sniff" }` before DNS hijack and routing rules.
- Modernized the frontend helper generator to avoid reintroducing inbound `sniff`, legacy TUN address fields, and the old DNS special outbound pattern.
- Added Rust regression coverage for the sing-box 1.13 route-action sniff shape.

### Safety Notes

- Production route material remains in Rust/service-owned config generation, not Flutter UI logs.
- No local SOCKS/HTTP bridge was added to the Full Computer path.
- The service still owns the TUN graph and validates config before launch.
- No secrets, UUIDs, SNI values, subscription URLs, or server hostnames were written into this report.

### Windows Test Path

- `cargo test --manifest-path src-tauri/Cargo.toml --lib`.
- `npm run build`.
- `src-tauri\sing-box.exe check -c <safe synthetic DoodleRay TUN config>`.
- Manual runtime follow-up: reconnect Full Computer mode, verify `sing-box config check failed` is gone, `DoodleRay Tunnel` appears, DNS and TCP HTTPS readiness pass, and disconnect leaves no service-owned `sing-box.exe`.

### 2026-06-06 Installed Smoke Result

- Bumped the hotfix release to `5.2.2` because an installed `5.2.1` build had already reached users and still reproduced the legacy inbound config failure.
- Built `DoodleRay_5.2.2_x64-setup.exe`; local build returned non-zero only after producing the installer because `TAURI_SIGNING_PRIVATE_KEY` is not present locally.
- Installed the NSIS build into `C:\Program Files\DoodleRay`.
- Verified installed `DoodleRay.exe` and `DoodleRayService.exe` report `ProductVersion/FileVersion = 5.2.2`.
- Verified `DoodleRayService.exe status` responds with `service_version = 5.2.2`.
- Verified service diagnostics include `network_snapshot`.
- Verified real Full Computer mode reached `connected` through `starting_xray`, `xray_ready`, `starting_tun`, `waiting_adapter`, `singbox_ready`, `adapter_ready`, `routes_ready`, and `total_connect`.
- Verified a TCP 443 canary used `DoodleRay Tunnel` as the source interface.
- Verified DNS fake-ip resolution returns a fake-ip address.

### Android Test Path

- Not touched. No Android build or runtime path is changed.

### iOS Test Path

- Not touched. No iOS build or runtime path is changed.

## 2026-06-05 Investigation: Workshop Routing Quality and Gaming Preset Merge

### Scope

- Route: Windows Workshop rules in Proxy mode and Whole computer mode.
- User-visible issue: two gaming presets were shown, with PUBG/BattlEye rules isolated in a separate local preset instead of merged into the existing "Геймерский — минимальный пинг" preset.
- UX issue: warnings and helper text used engineering terms like `TUN` and `split tunneling` that normal users do not understand.

### Evidence

- `src/lib/connect-helpers.ts::buildConnectRequestFromState` sends Workshop rules only when `proxyMode == "tun"`. In Proxy mode, the request intentionally sends an empty routing rule list.
- The Workshop API currently returns `Геймерский — минимальный пинг` with Steam/Epic/Riot/Blizzard/EA/Ubisoft, game process, Discord, Twitch, and YouTube rules.
- `src/stores/workshop-store.ts` also added a local `builtin-gaming-direct` preset containing only PUBG/BattlEye process rules, which created a second gaming card.
- `src-tauri/src/lib.rs::process_rule_names` normalizes executable rules to lower-case process names, and `push_process_route` emits sing-box `process_name` route rules.
- Existing Rust coverage proves PUBG process names are emitted as a direct rule in generated sing-box TUN config: `singbox_tun_routes_pubg_processes_direct`.

### Classification

- Proven: Workshop rules are intentionally not applied in Proxy mode.
- Proven: the duplicate gaming preset was caused by a local builtin PUBG-only preset being displayed next to the API gaming preset.
- Proven: app rules are process-name based, not full-path based.
- Likely but unproven: for normal games/apps this works well when every real child executable is covered.
- Unknown because instrumentation is missing: per-connection proof that a selected app's live traffic matched the intended outbound at runtime.

### First Broken Edge

```text
Workshop API preset: "Геймерский — минимальный пинг"
local builtin preset: PUBG/BattlEye only
-> merge by id only
-> UI shows two gaming presets
-> user has to understand and apply both
```

### Fixes Applied

- Renamed the local fallback preset to `Геймерский — минимальный пинг`.
- Merged PUBG/BattlEye rules into the existing API gaming preset by preset family instead of showing a second card.
- Added a fallback full gaming preset for API outage, including the low-ping baseline plus PUBG/BattlEye rules.
- Migrated previously applied local gaming presets to the unified gaming preset shape.
- Changed user-facing copy from `TUN` / `split tunneling` to "Whole computer" / "Весь компьютер" and "Workshop rules" / "правила Мастерской".

### Risk Notes

- Process-name routing is convenience routing, not a security boundary. A different process with the same executable name can match the rule.
- Some games launch multiple child processes; each must be covered for reliable direct/proxy/block behavior.
- Runtime proof still needs instrumentation that reports which Workshop rules were compiled into the active tunnel and, ideally, a redacted count of matched process/domain rules.

### Windows Test Path

- `npm run build`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`.
- Manual runtime follow-up: load Workshop presets with API reachable, verify only one gaming preset appears, expand it, and confirm PUBG/BattlEye rules are inside `Геймерский — минимальный пинг`.
- Apply that preset, switch to Whole computer mode, connect, and verify generated sing-box config contains direct process rules for Steam/game/PUBG executables.

### Android Test Path

- Not touched. No Android build or runtime path is changed.

### iOS Test Path

- Not touched. No iOS build or runtime path is changed.

## 2026-06-05 Fix Plan: Windows WS Whitelist Bypass and Server Selection Persistence

### Scope

- Route: Windows `system-proxy` and `tun` modes for VLESS/Xray WebSocket whitelist bypass.
- Build target: local `5.1.14`.
- Platform: Windows desktop Tauri client.

### Evidence

- User logs from `5.1.13` show Xray warnings for WebSocket transport and deprecated `headers.Host`.
- `src-tauri/src/lib.rs::vpn_connect` selected the Xray engine only for `transport == "xhttp"` or raw Xray JSON, so plain `vless://...?type=ws` profiles were not guaranteed to use the Xray path.
- `src-tauri/src/lib.rs::build_xray_config` already had a `wsSettings` branch, proving the missing edge was route selection/config modernization rather than complete absence of a WS builder.
- The generated Xray WS config used deprecated `wsSettings.headers.Host` instead of the newer independent `wsSettings.host`.
- `src/stores/app-store.ts::updateSubscription` replaced subscription servers with newly generated ids, then relied on `findMatchingServer`.
- `src/lib/server-selection.ts::getServerIdentityKey` previously included volatile display name/id-adjacent behavior and did not persist an independent endpoint selection key.
- `src/pages/Servers.tsx` marked selected groups by `activeServer?.id === server.id`, so refreshed subscription ids could visually unselect the current server even when the endpoint was still present.

### Classification

- Proven: WebSocket-generated Xray config existed but WS was not included in Xray engine selection.
- Proven: generated WS host placement used a deprecated Xray field shape.
- Proven: subscription refresh can replace server ids and break id-only UI selection checks.
- Likely but unproven: the user's "last server forgotten on restart" symptom is the same stable-selection-key gap after hydration or refresh.
- Unknown because live secret profile material is intentionally unavailable here: exact server-side CDN/WebSocket path and SNI/host values.

### First Broken Edge

```text
VLESS WebSocket subscription/link
-> transport parsed as ws
-> vpn_connect treats only xhttp/raw JSON as Xray-owned
-> WS bypass route does not reliably run through Xray WebSocket config
```

### Minimal Fix Plan

- Treat `ws` as an Xray-owned transport alongside `xhttp`.
- Generate Xray WS with `wsSettings.host` and TLS/REALITY settings, and normalize raw JSON configs away from deprecated `headers.Host`.
- Persist a stable selected-server key based on endpoint/auth/transport fields.
- Match active servers after refresh/hydration by stable key before falling back to fastest/default.
- Update UI active checks to use the same matcher instead of raw ids.

### Windows Test Path

- `npm run build`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`.
- `cargo check --manifest-path src-tauri/Cargo.toml --bin DoodleRay`.
- Manual runtime follow-up: connect a redacted VLESS WS whitelist-bypass profile in System Proxy and TUN, verify Xray starts, verify no `headers.Host` deprecation warning in generated configs, refresh subscription, restart client, and confirm the same server remains selected.

### Android Test Path

- Not touched. No Android build or runtime path is changed.

### iOS Test Path

- Not touched. No iOS build or runtime path is changed.

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

## 2026-06-03 Investigation: PUBG micro-drop and split-routing gap

### Scope

- Route observed by local machine state: DoodleRay Tunnel Service reported `disconnected`, while `happ-tun` was the active TUN adapter.
- User-visible symptom: PUBG/Discord-like traffic dropped briefly around 01:10 and 01:22 local time; for PUBG this is fatal even if the outage lasts only 1-2 seconds.
- Product gap: the Workshop application scanner did not surface PUBG, making it hard to add direct/bypass rules.

### Evidence

- `DoodleRayService.exe status` returned `state=disconnected`; therefore the active DoodleRay Windows TUN graph was not carrying traffic at the time of this investigation.
- `Get-NetAdapter` showed `happ-tun` up and no active `DoodleRay Tunnel` adapter.
- Running/logged PUBG processes were present: `TslGame.exe`, `TslGame_ZK.exe`, `TslGame_BE.exe`, `ExecPubg.exe`, and `BEService`.
- Windows System log around 01:10-01:22 contained PUBG/BattlEye driver activity: `BEDaisy` and `navagio` kernel driver service install events around 01:12 and 01:18.
- NetBT interface initialization errors appeared around 01:24.
- Route table showed both `Ethernet` and `happ-tun` default routes with route metric `0`; effective interface metric favored `happ-tun`.
- `src/pages/Workshop.tsx` previously offered a registry-based app scanner only through `scan_installed_apps`.
- `src-tauri/src/lib.rs::scan_installed_apps` previously read registry uninstall keys, LocalAppData apps, and AppX packages, but did not include running processes or Steam library executables.
- `src/lib/connect-helpers.ts::buildConnectRequestFromState` sends Workshop rules only in TUN mode, so process rules are intentionally ignored in regular Proxy mode.

### Classification

- Proven: DoodleRay's active service-owned TUN was not connected during this specific check; current machine routing was through `happ-tun`.
- Proven: PUBG's live/logged executable names are `TslGame.exe`, `TslGame_ZK.exe`, `TslGame_BE.exe`, and `ExecPubg.exe`, not a simple visible `PUBG.exe`.
- Proven: the old app scanner could miss Steam games because Steam game executables are not guaranteed to appear as top-level Windows uninstall entries.
- Proven: the generated sing-box TUN config now routes the PUBG process names to `direct`; covered by `singbox_tun_routes_pubg_processes_direct`.
- Likely but unproven: the reported 01:10/01:22 micro-drops were not caused by DoodleRay TUN because DoodleRay service was disconnected and `happ-tun` was active.
- Likely but unproven: PUBG/BattlEye driver activity or competing VPN/TUN route behavior contributed to the micro-drops.
- Unknown because instrumentation was missing at the exact moment: packet loss, DNS latency, TCP reachability, adapter state, and route changes during the exact 01:10/01:22 events.

### First Broken Edge

The first proven DoodleRay product broken edge is split-routing discoverability and rule coverage:

```text
Workshop app scanner
-> registry/app package scan only
-> Steam PUBG executables not shown
-> user cannot conveniently add direct game rules
-> PUBG remains on full VPN route in TUN mode
```

### Fixes Applied

- Extended `scan_installed_apps` to include live `.exe` processes via PowerShell `Get-Process`.
- Extended `scan_installed_apps` to include Steam library folders from `libraryfolders.vdf`, including nested folders such as `TslGame/Binaries/Win64`.
- Added a built-in Workshop preset, `Геймерский набор`, that users can choose explicitly. It routes PUBG and BattlEye process rules directly in TUN mode without adding a standalone PUBG-only button to the main rules UI.
- Updated the Workshop app search to match both app name and executable/path.
- Sorted/deduplicated generated sing-box process rules for stable diagnostics.
- Added Rust coverage: `singbox_tun_routes_pubg_processes_direct`.
- Extended `scripts/monitor-network.ps1` to capture ping, Happ/Windscribe/DoodleRay/PUBG/BattlEye/Steam process snapshots, routes, adapters, DNS servers, TCP checks, and DoodleRay service state.

### Active Monitor

- Started a one-hour monitor on 2026-06-03 at 02:49 local time after fixing Windows PowerShell ping compatibility.
- Log path: `logs/network-monitor-20260603-024932.jsonl`.
- PID: `42636`.

### Minimal Follow-up Plan

- If a new drop happens, compare the exact timestamp against `ping.gateway`, `ping.cloudflare`, `tcp.cloudflare_443`, `dns.discord`, route/default adapter changes, and process snapshots.
- For DoodleRay TUN gameplay, apply the `Геймерский набор` preset, switch to TUN, reconnect, then verify generated config contains the direct process rule before claiming the game is bypassed.

### Windows Test Path

- `npm run build` passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib` passed: 11/11.
- `cargo check --manifest-path src-tauri/Cargo.toml --bin DoodleRayService` passed.
- `cargo check --manifest-path src-tauri/Cargo.toml --bin DoodleRay` passed.

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
