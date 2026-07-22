# VPN Practic Report Coverage Matrix

Date: 2026-07-02

Source report: `D:\DoodleRayAPP\docs\vpn-practic-report.md`.

This matrix tracks whether DoodleRay PC actually uses the report findings.
It is intentionally strict: "tested" means verified on the Play2Go Windows QA
stand or by automated tests, not inferred from code shape.

## Verdict

DoodleRay PC now uses the most important Windows protected-mode lessons from
the report, but it is not yet honest to claim "perfect for every Windows
device." The current RC is much stronger for service-owned TUN, runtime ports,
health, WebView2 installer mode, redaction, crash cleanup, auto-fallback, and
Play2Go QA. The Server 2022 full stand matrix passed end-to-end, including
previous-version updates, active-VPN-during-update, stale-state repair, UI
mode switching, protected early-failure auto-fallback to Browsers, and deep
snapshot. The remaining gap is breadth and hardening: clean Win10/Win11 VM
coverage, signed-CI artifact proof, real sleep/wake reassertion proof, IPv6
leak-proof evidence, and controlled QUIC proof still need work.

## P0 / P1 Coverage

| Report requirement | Status | Evidence | Remaining work |
|---|---:|---|---|
| Service-first protected mode; UI asks, service owns TUN runtime truth. | Covered for protected runtime, QA pending for v6 | `src-tauri/src/bin/service.rs`, `src-tauri/src/tunnel_service.rs`; service owns xray/sing-box children, runtime ports, generation, engine kind, adapter/route/DNS readiness, effective state, and health verdict. | Proxy/browser lifecycle remains app-side by design for now; v6 release still needs signed artifact QA. |
| Structured runtime ports/generation/op_id instead of regex/log parsing. | Covered in v6 code | `TunnelStatus` and `ConnectionHealthReport` carry runtime SOCKS/HTTP/API ports, generation, op id. `src/lib/connection-health.ts` no longer parses ports from health text. | Verify after update from 5.4.x that old persisted ports are replaced by service runtime ports. |
| Fatal/degraded/warning lanes. | Covered in v6 code, QA pending | Service publishes `protected`, `protected_degraded`, `limited`, `repairing`, `failed`, and `cleanup_pending` verdicts plus separate fatal/degraded/warning arrays. UI accepts only `protected`/`protected_degraded` for TUN. | Play2Go and Win10/Win11 must verify fatal/degraded behavior on real failures. |
| WinINet only after loopback HTTP readiness; WinHTTP separate. | Covered | `sysproxy` readiness retries; deep QA checks WinINet and WinHTTP separately. | Add test where corporate PAC/autodetect is preconfigured and must be restored exactly. |
| WebView2 fixed/offline installer handling. | Partial | `tauri.conf.json` has `webviewInstallMode.offlineInstaller`; deep QA detects WebView2 runtime. | Fresh clean Win10/Win11 without WebView2 still needs automated VM install proof for the final release artifact. |
| Sign all distributables. | Covered in CI gate, local RC still unsigned | Release workflow verifies signing secrets and Authenticode validity for service, sing-box, xray, wintun, app, and NSIS installer artifacts. | Production readiness requires a CI build with real secrets; local unsigned artifacts remain QA-only. |
| Protected DNS path over proxied DoH with explicit fallback. | Partial+ | xray TUN bridge uses DoH; tests cover DNS health and Apple GET. v6 service marks DNS readiness and includes DNS policy in route explanations. | Explicit fallback ladder still needs product decision and QA on corporate DNS/captive networks. |
| Strict route / DNS leak prevention. | Partial | Route canaries and DNS route checks exist in full health; deep QA checks route/DNS. | Need WFP/strict-route leak evidence across more networks; prove no non-TUN DNS under IPv6/dual-stack/corporate DNS cases. |
| Deterministic endpoint bypass routes. | Partial | Protected route canaries and endpoint route exclusions exist in tests/config paths. | Need explicit per-active-endpoint route proof in support bundle and after network change. |
| Disconnect cleanup: WinINet, NRPT, routes, adapters, processes. | Partial | Play2Go cleanup verified WinINet off and no engine children; deep QA checks NRPT count. | Need stale NRPT/adapters/routes injection tests and "degraded-cleanup-pending" state if cleanup is incomplete. |
| Repair-first update/install path. | Covered + tested on Server 2022 | NSIS hooks/service repair, WebView2 installer mode. `Invoke-DoodleRayUpdatePathQa.ps1` passed for 5.4.3/5.4.4/5.4.5 (last with stale WinINet + corporate PAC preserved). `Invoke-DoodleRayActiveUpdateQa.ps1` passed 11/11: RC installed over a genuinely active protected tunnel, clean SCM-stop cleanup, startup repair cleared the orphaned WinINet, reconnect protected. | Repeat on Win10/Win11 images; extend injection to NRPT/routes/adapters variants. |
| Crash recovery after child core exit. | Covered + bring-up self-repair coded | Play2Go killed service-owned `sing-box.exe` while connected (UI honestly exits, cleanup, reconnect). Service crash covered by the unclean-shutdown marker test. Crash-during-connect now triggers the bounded bring-up repair retry (`tun_adapter_repair`) instead of surfacing `adapter is missing` to the user; classifier/message unit-tested. RC4 proved service-side bring-up repair twice; RC5 proved the user-facing early-failure auto-fallback path end-to-end. | Rewrite `Test-DoodleRayTunBringupRepair.ps1` v2 so it separates pure service-repair proof from UI fallback behavior; add xray-crash-while-connected variant. |
| UI crash/reload while protected service remains active. | Covered for tested case | Play2Go killed main `DoodleRay.exe`; service stayed connected, UI restarted, WinINet was reasserted to the service HTTP port, health returned `protected`, and no `xray api statsquery` orphan remained. | Add same scenario to committed harness and run it on Win10/Win11. |
| Sleep/wake and network change handled by service. | Covered in code, real-hardware QA pending | Power/netbind events mark the tunnel `Suspect` and schedule `repair_connected_runtime` (adapter snapshot, interface metric, route/DNS/port reassert). If the in-place reassert fails, the service now rotates the child generation once per event burst from the stored start request instead of leaving a dead-interface tunnel (reboot is not the repair path). | Prove on real sleep/resume hardware (the stand is a VM); the rotation path currently relies on unit-level and code review evidence. |
| Split-route verification: RU/direct, AI/Telegram tunnel, private direct, endpoint excluded. | Covered + explained | Deep QA verifies 2ip direct and Telegram/Discord/OpenAI/Claude probes. The service now derives `route policy:` explanation lines from the actual sing-box config (`summarize_route_policy`, unit-tested): default outbound, RU-direct split, private-LAN direct, direct ip_cidr bypass count, DNS hijack - published in `route_explanations`, so they land in status, diagnostics, and the support bundle. | Extend deep QA with per-rule deterministic domain assertions beyond 2ip. |
| DNS/HTTPS/UDP/WebSocket/SSE probes. | Covered in QA snapshot | `Get-DoodleRayDeepQaSnapshot.ps1` covers DNS, Apple/Google, WebSocket, SSE, UDP/STUN. | Move more probes service-side and add controlled owned endpoints instead of third-party-only probes. |
| QUIC probe in protected mode. | Explicit non-claim + conditional probe | Service pushes a structured warning `QUIC/HTTP3 is not verified by a controlled probe in this build; no QUIC claim` on every protected connect/repair. `Get-DoodleRayDeepQaSnapshot.ps1` now emits `quicProbe` with `verified`/`failed`/`unverified-no-tooling` and only claims `verified` after a real `--http3-only` request succeeds. | Ship curl-with-HTTP/3 or an owned QUIC reflector on QA stands so the probe can return `verified`; until then QUIC stays explicitly unclaimed. |
| Support bundle at failure time with redaction. | Covered in v6 code, QA pending | `export_support_bundle` includes `failure_marker`, signer status/thumbprints, service diagnostics, and Windows network summaries with redaction; the dashboard has one-click export. The service now writes an `active-session.marker` at connect and clears it only through owned cleanup; a marker found at service startup is published as `previous_unclean_shutdown` in `TunnelStatus`, surfaced as an `unclean shutdown marker:` warning in health, and lands in the bundle via service diagnostics. | Verify marker behavior on the stand (service kill -> restart -> marker reported once) and on Win10/Win11. |
| Auto fallback: Protected -> Browsers -> Manual, honest limited protection. | Covered + tested on Server 2022 | On a TUN adapter/route bring-up failure (after the service-side bounded repair), the app automatically retries in Browsers compatibility mode and shows explicit LIMITED-protection log + toast (en/ru/zh); the mode card switches to Browsers so no green "Protected" is claimed. Manual is never entered automatically and never mutates WinINet. RC5 targeted E2E and RC6 full matrix both observed a real protected start, no protected claim, WinINet loopback HTTP fallback, Apple captive GET `HTTP_CODE=200`, no TUN adapter during fallback, and clean teardown. | Repeat on Win10/Win11; consider service-owned Browsers lifecycle in v6.1. |
| NCSI/no-internet icon treated as warning only. | Partial | Deep QA checks NCSI and docs explain it; health treats data-plane separately. | UI should expose NCSI as diagnostic warning only when detected, not normal user-facing failure. |
| Multi-OS, multi-network, privilege matrix. | Missing broad coverage | One Play2Go Windows Server 2022 stand is configured. | Need Windows 10 22H2, Windows 11 23H2/24H2, non-admin run, corporate PAC, captive network, IPv6-present/absent, LTE/tether, overlapping LAN. |

## Play2Go Evidence Already Collected

- NSIS-installed app launched from `C:\Program Files\DoodleRay`.
- Protected / Whole Computer connected with service status `connected`.
- Fast and full health returned `protected`.
- Deep snapshot passed WebView2, VC++ runtime, service recovery config,
  WinINet, WinHTTP, NCSI, routes, DNS, WebSocket, SSE, UDP/STUN, Telegram,
  Discord, OpenAI, Claude, and split-route probes.
- Service-owned `sing-box.exe` crash was injected. UI left `CONNECTED`, logged
  fatal protected health, cleaned WinINet/processes, and reconnect succeeded
  without reboot.
- Main UI crash/reload was injected. The service-owned TUN stayed up, WinINet
  was restored by the new UI from service runtime ports, health returned
  `protected`, and no `xray api statsquery` orphan remained.
- A clean second reconnect after disconnect returned `protected` with a new
  service generation and new structured runtime ports.
- Protected early-failure auto-fallback was verified end-to-end on RC5: the
  service attempted protected TUN, UI did not claim protected, browser
  compatibility came up through WinINet loopback HTTP, Apple's captive probe
  returned 200 through that proxy, and teardown left the stand clean.
- The full one-command Server 2022 matrix passed on RC6 setup
  `85A8B3A7A6AF5539FCBA68A38EF87C1CF864F568324C022BBF3898DF7DBCBA22`:
  bootstrap, publish, install gate, unclean marker, update from 5.4.3/5.4.4/
  5.4.5 broken state, active-VPN-during-update, UI mode/crash pass,
  stale-state repair, Protected->Browsers auto-fallback, and deep snapshot.
- Final cleanup left service disconnected, WinINet disabled, and no xray or
  sing-box child processes.

## Friend LAN Dirty Windows 10 Evidence

Collected on 2026-07-04 against a real Windows 10 Pro 19045 desktop on the
local LAN. This machine was intentionally not clean: it had an old DoodleRay
5.4.5 install, Zapret/WinDivert, Outline, Happ, Hiddify, TAP/Wintun drivers,
and stale inactive WinINet proxy strings.

- Upgrade from the old installed client to 5.9.0 QA artifact succeeded.
- Real subscription import/refresh worked through the app control surface.
- Protected mode proved `protected_degraded` once with real data-plane probes:
  Apple captive GET, Google 204, Telegram, Discord, OpenAI, Claude,
  WebSocket, SSE, UDP/STUN, DNS, and 2ip/direct split.
- The same machine later reproduced the fragile TUN class (`adapter is
  missing` / IPv4 readiness), and 5.9.0 fell back to Browsers compatibility
  instead of leaving the user disconnected.
- Root cause for that fragile TUN class was then isolated and fixed: a stale
  non-present Wintun PnP ghost (`sing-tun Tunnel`, `CM_PROB_PHANTOM`) was
  invisible to `Get-NetAdapter` but blocked Wintun creation with
  `Cannot create a file when that file already exists | open existing adapter:
  Element not found`. The service now removes stale `SWD\WINTUN\*` ghosts that
  match DoodleRay/sing-tun ownership heuristics during owned cleanup/replace.
- After the ghost repair landed, the same dirty Win10 desktop connected in
  Whole Computer mode as `protected_degraded` with `adapter_alias=DoodleRay
  Tunnel`, `route_ready=true`, `dns_ready=true`, and working real traffic
  probes.
- After the same ghost repair, protected crash recovery was re-run on the dirty
  Win10 desktop: UI kill preserved service truth, UI reattach reflected the
  runtime, killing the service-owned core did not leave fake-green, and final
  cleanup was clean.
- Forced protected bring-up failure by killing `sing-box` twice proved the
  honest limited fallback path: no protected claim, WinINet loopback HTTP
  compatibility active, Apple captive GET `200`, no claimed TUN adapter, clean
  teardown.
- Direct Telegram was blocked on that machine while proxy Telegram worked.
  This is useful real-world coverage for users with local filtering tools where
  proxy mode is the only viable fallback.
- Manual mode preserved WinINet state.
- Final state was clean: service idle, WinINet disabled, no DoodleRay engines,
  no `statsquery`, no adapter, no DoodleRay NRPT, no marker.

New issues found and fixed from this dirty-machine pass:

- App-side `xray.exe` orphan after UI kill/fallback cleanup.
- Protected fake-green after a service-owned core crash before the slow monitor
  loop noticed fatal service health.
- Stale Wintun PnP ghost blocking TUN adapter creation.

## Do Not Claim Yet

- Do not claim "works on every Windows device" until the OS/network matrix is
  actually covered.
- Do not claim signed production readiness from local Codex artifacts; the
  local RC is unsigned.
- Do not claim IPv6 correctness. Current Windows behavior is intentionally
  IPv4-stable, while route tables can still show IPv6 coverage.
- Do not claim QUIC coverage; it is not tested by the current stand.
- Do not claim cross-OS update safety until the same automated previous-version
  and broken-state upgrade tests pass on clean Win10/Win11 signed artifacts.

## Reference Project Lessons Mapping

Compact status of what DoodleRay actually took from each reference family
(`implemented` = in code, `tested` = proven on the Play2Go stand or by unit
tests, `still missing` = not done; do not claim it).

| Reference family | Lesson applied in DoodleRay | Implemented | Tested | Still missing |
|---|---|---|---|---|
| WireGuard for Windows / WireGuardNT / Wintun | Service owns tunnel lifecycle; deterministic adapter name (`DoodleRay Tunnel`); kill-on-close job for children; adapter/route teardown on every stop; endpoint-bypass routes. | yes | connect/crash/cleanup cycles on Server 2022 incl. CDP UI pass | WireGuardNT-style kernel dataplane (not planned); stale-wintun-registration auto-repair beyond adapter cleanup. |
| Tailscale / NetBird | Runtime truth in privileged service; bugreport-style one-click redacted support bundle with failure marker + unclean-shutdown marker; DoodleRay-owned-only NRPT/route/WinINet repair; SCM failure recovery. | yes | marker crash-sim, bundle redaction asserts, stale-state injection/repair on Server 2022 | NRPT is not actively used by v6 DNS path (only cleaned); timed trace mode in bundle. |
| sing-box / mihomo | TUN + strict-route semantics; structured runtime ports instead of log scraping; DoH-over-tunnel DNS; explicit QUIC non-claim until a controlled probe passes; current-schema configs (no legacy FakeIP/DNS). | yes | runtime ports/health via service JSON in all harnesses; `quicProbe` honest verdict in deep snapshot | sing-box local gRPC API adoption; QUIC probe still `unverified-no-tooling` on the stand. |
| v2rayN / Clash Verge Rev / sing-box-windows | Desktop proxy UX: mode cards (Whole computer / Browsers / Manual), tray/system-proxy guard equivalents, hot mode switching with live reconnect, route-exclude support. | yes | mode-switch chain Proxy->Whole->Proxy->Manual->Whole via CDP UI pass | proxy-guard-style continuous WinINet watchdog is app-side only; no per-app split UI. |
| Microsoft / Tauri / WebView2 docs | Offline WebView2 installer mode; WinINet vs WinHTTP inspected separately; NCSI treated as warning; Authenticode fail-closed CI; no EV/SmartScreen myths; perMachine NSIS with service repair hooks. | yes | WebView2/VC++/WinHTTP/NCSI in deep snapshot; unsigned-RC gate + CI workflow checks (workflow itself not yet exercised with real secrets) | signed CI run with real certs; SmartScreen reputation plan; MSI (currently NSIS-only). |

## Next Required Work

1. Add a Windows VM matrix: Win10 22H2, Win11 23H2/24H2, Server 2022/2025.
2. Repeat `Invoke-DoodleRayFullStandQa.ps1` on clean Win10 22H2 and Win11
   24H2 signed artifacts; the Server 2022 RC6 run already covers
   5.4.3/5.4.4/5.4.5 update paths, broken-state update, active VPN update,
   stale-state repair, UI pass, and auto-fallback.
3. Prove sleep/wake and network-change reassertion on the stand (code exists:
   power/netbind events -> suspect -> `repair_connected_runtime`).
4. IPv6 policy is explicit `degraded_disabled` until leak proof exists; either
   collect the proof or keep the degraded lane (no code gap, evidence gap).
5. Ship HTTP/3-capable curl or an owned QUIC reflector to QA stands so the new
   `quicProbe` can return `verified`; until then QUIC is explicitly unclaimed.
6. Run signed CI Authenticode validation before production tag.
7. Move proxy/browser mode lifecycle under service authority if v6.1 expands beyond protected mode.
