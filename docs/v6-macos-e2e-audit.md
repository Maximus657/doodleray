# DoodleRay VPN 6 — macOS App Store E2E audit

Updated: 2026-07-19

Branch: `codex/v6-macos-app-store`

Candidate: `6.0.0 (60006)`
Policy: credentials, subscription tokens, device identifiers, node addresses, routes, and egress addresses are never written to this report.

## Release verdict

**Build 60006 is uploaded for TestFlight processing; App Review remains blocked by a clean cold-start retest and the real system-tunnel matrix.** API, profile, UI, build, signing, archive/export, and isolated transport checks pass.

## Fixed release blockers

| Severity | Finding | Resolution | Verification |
|---|---|---|---|
| P0 | Client called nonexistent `/v1/mobile/profile-leases` routes and received HTTP 404 | Moved connect and probe flows to canonical `/v1/mobile/connection-profile`; removed consume/revoke calls | Rust contract test and authenticated production profile retrieval pass |
| P0 | Closed App API could select nodes outside the user's subscription squad | Applied the subscription renderer's inbound/squad filtering to locations, countries, Reality candidates, and CDN candidates | Backend tests pass; production deployed; every country profile matches at least one current subscription node field-for-field |
| P0 | App Store login was lost after process restart | Moved macOS secure storage to the sandbox-compatible Data Protection Keychain with one-way legacy migration | Login survived repeated QA bundle restarts; subscription and device state remained available |
| P0 | TestFlight 60003 could beachball on a white window for about two minutes during first launch | Marked renderer secure-storage commands and startup session lookup as asynchronous Tauri commands so Keychain work cannot block the main thread | App Store compile, clippy, and Rust tests pass; clean-device TestFlight retest is pending on 60006 |
| P0 | TestFlight 60003 froze its timer and entire UI one second after a successful tunnel connection | Moved all blocking status/health commands off the UI thread, cached the loaded Network Extension manager, and prevented overlapping watchdog checks | Signed universal 60005 compiles the host bridge and extension; clean-device TestFlight retest is pending on 60006 |
| P0 | `Автовыбор` could stop after choosing one reachable endpoint whose VPN traffic check failed | Send ping-ranked country fallbacks, wait for Network Extension to fully stop between attempts, and try at most three distinct locations | Production telemetry confirmed the failed auto attempt was followed by a successful Germany tunnel; fallback and stopped-state tests pass |
| P0 | Nested router VPN could cause the packet tunnel to route its own uplink back into itself | Resolve remote endpoints before installing tunnel routes and add exact IPv4/IPv6 uplink exclusions | All current endpoints resolve; signed extension build and unit tests pass; real NE test pending |
| P0 | A tunnel could report connected while carrying no usable traffic | Added an HTTPS post-connect verifier; a failure automatically stops the extension and restores disconnected state | Verifier uses the first-party health endpoint that passed through all current profile families; real NE test pending |
| P1 | First Network Extension preference save could exceed the bridge timeout | Increased preference operation timeout from 20 to 60 seconds | Signed QA setup no longer hits the earlier 20-second failure |
| P1 | App Store Xray config depended on unavailable external geodata and a sandbox-incompatible DNS outbound | Replaced private geodata with literal CIDRs; removed external geodata selectors and the local DNS outbound from the NE config | App Store config tests pass; all isolated profile tests resolve DNS and carry HTTPS |
| P1 | UI hid ping action/results and exposed protocol-oriented server text | Added visible `Пинг`, explicit result states, country-only labels, and `Автовыбор` | QA18 shows 8 user-facing entries and 6–8 ms results without protocol labels |
| P1 | Bypass and reserve were missing from the v6 location model | Sync closed-control-plane locations directly and prepend `Автовыбор` | QA18 shows auto + bypass + reserve + NL/DE/RU/KZ/US |
| P1 | Automatic ping runner cancelled itself after mutable ping updates | Depend on stable server identity instead of mutable ping values | All seven concrete locations complete in one UI run |
| P2 | Native macOS traffic lights left excessive empty space before the DoodleRay brand | Reduced the native-only title-bar inset while retaining traffic-light clearance | Frontend production build passes; visual TestFlight confirmation pending |

## Profile and transport evidence

- The current `ddlvpn.lol` subscription is the release source of truth; obsolete legacy subscription links are excluded from the audit.
- App API country candidates match the current subscription's authentication ID, endpoint and port, Reality key, short ID, SNI, and flow.
- Candidate multiplicity: Germany 4, Netherlands 2, United States 2, Kazakhstan 1, Russia 1. Bypass matches the current subscription; reserve is an app-only managed fallback.
- Local Xray SOCKS E2E, while the router/Wi-Fi VPN remained enabled: all 10 Reality nodes, bypass, and reserve returned HTTPS 200 from the first-party health endpoint.
- The 10 Reality nodes and bypass also returned Apple captive-portal HTTP 200. Reserve returned first-party health 200 but not captive-portal success, which is why release verification now uses the first-party health endpoint.
- A remote VPS could reach only a subset of nodes, while every node worked from the actual Mac network. This is treated as source-network filtering/routing evidence, not a client profile mismatch.

## Build and static verification

- `npm run build`: pass.
- `cargo test --features app-store`: 76 tests pass.
- `cargo clippy --all-targets --features app-store -- -D warnings`: pass.
- `cargo fmt --check`: pass.
- App Store readiness script: all gates pass.
- QA18 host and Packet Tunnel extension: `arm64` + `x86_64`.
- Host and extension are sandboxed, signed by the same Apple team, provisioned, and contain the required App Group and Packet Tunnel entitlements.
- Clean TestFlight build `6.0.0 (60003)` reproduced the cold-start stall, post-connect UI freeze, and one failed auto-selected tunnel; explicit Germany carried real VPN traffic.
- UI smoke: session persists, normal rounded macOS window, no white corners, 8 locations, visible ping action/results, no protocol labels.
- Apple Distribution build, archive/export, and App Store Connect upload: pass for 60006; 60005 was intentionally not distributed.
- Internal TestFlight group `Mac QA` exists with two internal testers and build 60003; build 60006 will replace it after Apple processing completes.

## Final real Network Extension matrix

- [x] Login, kill app, relaunch, confirm session and device identity persist.
- [x] Retrieve every current location and complete profile-backed pings.
- [ ] Install 60006 on the clean TestFlight Mac and verify a responsive first frame without a beachball.
- [ ] Connect using `Автовыбор`; verify Packet Tunnel reaches `connected` and first-party health passes.
- [ ] Keep the connected app open for at least two minutes; verify the timer advances and every control remains responsive.
- [ ] Verify DNS over UDP and TCP, HTTPS traffic, large/MTU-sensitive transfer, and explicit IPv4/IPv6 behavior.
- [ ] Switch between two countries, bypass, and reserve without stale routing.
- [ ] Repeat connect/disconnect cycles and verify routes, DNS, and network state return to the captured baseline.
- [ ] Kill/relaunch while disconnected and verify no stale Network Extension state.
- [ ] Re-run full frontend/Rust/static/bundle gates after any E2E fix.
- [x] Build with Apple Distribution, validate archive/export, then upload build 60006.
