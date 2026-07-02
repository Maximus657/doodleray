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
health, WebView2 installer mode, redaction, crash cleanup, and Play2Go QA.
The remaining gap is breadth and hardening: multi-OS VM coverage, full
sleep/wake reassertion proof, QUIC policy, update-from-broken-state evidence,
and clean Win10/Win11 release-artifact tests still need work.

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
| Crash recovery after child core exit. | Covered for tested case | Play2Go killed service-owned `sing-box.exe`; UI exited connected state; reconnect worked without reboot. | Add xray crash, service crash, and crash-during-connect tests. |
| UI crash/reload while protected service remains active. | Covered for tested case | Play2Go killed main `DoodleRay.exe`; service stayed connected, UI restarted, WinINet was reasserted to the service HTTP port, health returned `protected`, and no `xray api statsquery` orphan remained. | Add same scenario to committed harness and run it on Win10/Win11. |
| Sleep/wake and network change handled by service. | Partial in v6 code | Service accepts Windows power/network controls, increments `network_event_seq`, marks connected tunnels `Suspect`/degraded, and keeps the tunnel up. | Add real reassert-route/DNS/WinINet transaction and QA using sleep/resume or simulated interface changes. |
| Split-route verification: RU/direct, AI/Telegram tunnel, private direct, endpoint excluded. | Partial | Deep QA verified 2ip direct behavior, Telegram/Discord/OpenAI/Claude probes, private direct rules in tests. | Need deterministic domain/IP assertions per profile and support-bundle route explanation for active rules. |
| DNS/HTTPS/UDP/WebSocket/SSE probes. | Covered in QA snapshot | `Get-DoodleRayDeepQaSnapshot.ps1` covers DNS, Apple/Google, WebSocket, SSE, UDP/STUN. | Move more probes service-side and add controlled owned endpoints instead of third-party-only probes. |
| QUIC probe in protected mode. | Explicit non-claim + conditional probe | Service pushes a structured warning `QUIC/HTTP3 is not verified by a controlled probe in this build; no QUIC claim` on every protected connect/repair. `Get-DoodleRayDeepQaSnapshot.ps1` now emits `quicProbe` with `verified`/`failed`/`unverified-no-tooling` and only claims `verified` after a real `--http3-only` request succeeds. | Ship curl-with-HTTP/3 or an owned QUIC reflector on QA stands so the probe can return `verified`; until then QUIC stays explicitly unclaimed. |
| Support bundle at failure time with redaction. | Covered in v6 code, QA pending | `export_support_bundle` includes `failure_marker`, signer status/thumbprints, service diagnostics, and Windows network summaries with redaction; the dashboard has one-click export. The service now writes an `active-session.marker` at connect and clears it only through owned cleanup; a marker found at service startup is published as `previous_unclean_shutdown` in `TunnelStatus`, surfaced as an `unclean shutdown marker:` warning in health, and lands in the bundle via service diagnostics. | Verify marker behavior on the stand (service kill -> restart -> marker reported once) and on Win10/Win11. |
| Auto fallback: Protected -> Browsers -> Manual, honest limited protection. | Partial | Product modes exist and copy improved; protected failures no longer lie green. | Automatic fallback ladder is not complete; Browser fallback must show limited protection, not generic connected. |
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
- Final cleanup left service disconnected, WinINet disabled, and no xray or
  sing-box child processes.

## Do Not Claim Yet

- Do not claim "works on every Windows device" until the OS/network matrix is
  actually covered.
- Do not claim signed production readiness from local Codex artifacts; the
  local RC is unsigned.
- Do not claim IPv6 correctness. Current Windows behavior is intentionally
  IPv4-stable, while route tables can still show IPv6 coverage.
- Do not claim QUIC coverage; it is not tested by the current stand.
- Do not claim update safety until previous-version and broken-state upgrade
  tests are automated.

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
2. Run `Invoke-DoodleRayUpdatePathQa.ps1` for 5.4.3/5.4.4/5.4.5 per RC and add
   NRPT/routes/adapters injection variants plus active-VPN-during-update CDP
   automation.
3. Prove sleep/wake and network-change reassertion on the stand (code exists:
   power/netbind events -> suspect -> `repair_connected_runtime`).
4. IPv6 policy is explicit `degraded_disabled` until leak proof exists; either
   collect the proof or keep the degraded lane (no code gap, evidence gap).
5. Ship HTTP/3-capable curl or an owned QUIC reflector to QA stands so the new
   `quicProbe` can return `verified`; until then QUIC is explicitly unclaimed.
6. Run signed CI Authenticode validation before production tag.
7. Move proxy/browser mode lifecycle under service authority if v6.1 expands beyond protected mode.
