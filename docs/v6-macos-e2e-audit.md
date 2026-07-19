# DoodleRay VPN 6 — macOS App Store E2E audit

Updated: 2026-07-19

Branch: `codex/v6-macos-app-store`

Candidate: `6.0.0 (60003)`
Policy: credentials, subscription tokens, device identifiers, node addresses, routes, and egress addresses are never written to this report.

## Release verdict

**Ready for the final system-tunnel matrix; not ready to upload yet.** API, profile, UI, build, signing, and isolated transport checks pass. Upload remains blocked only until the signed QA candidate completes the real macOS Network Extension connect/disconnect matrix on this Mac.

## Fixed release blockers

| Severity | Finding | Resolution | Verification |
|---|---|---|---|
| P0 | Client called nonexistent `/v1/mobile/profile-leases` routes and received HTTP 404 | Moved connect and probe flows to canonical `/v1/mobile/connection-profile`; removed consume/revoke calls | Rust contract test and authenticated production profile retrieval pass |
| P0 | Closed App API could select nodes outside the user's subscription squad | Applied the subscription renderer's inbound/squad filtering to locations, countries, Reality candidates, and CDN candidates | Backend tests pass; production deployed; every country profile matches at least one current subscription node field-for-field |
| P0 | App Store login was lost after process restart | Moved macOS secure storage to the sandbox-compatible Data Protection Keychain with one-way legacy migration | Login survived repeated QA bundle restarts; subscription and device state remained available |
| P0 | Nested router VPN could cause the packet tunnel to route its own uplink back into itself | Resolve remote endpoints before installing tunnel routes and add exact IPv4/IPv6 uplink exclusions | All current endpoints resolve; signed extension build and unit tests pass; real NE test pending |
| P0 | A tunnel could report connected while carrying no usable traffic | Added an HTTPS post-connect verifier; a failure automatically stops the extension and restores disconnected state | Verifier uses the first-party health endpoint that passed through all current profile families; real NE test pending |
| P1 | First Network Extension preference save could exceed the bridge timeout | Increased preference operation timeout from 20 to 60 seconds | Signed QA setup no longer hits the earlier 20-second failure |
| P1 | App Store Xray config depended on unavailable external geodata and a sandbox-incompatible DNS outbound | Replaced private geodata with literal CIDRs; removed external geodata selectors and the local DNS outbound from the NE config | App Store config tests pass; all isolated profile tests resolve DNS and carry HTTPS |
| P1 | UI hid ping action/results and exposed protocol-oriented server text | Added visible `Пинг`, explicit result states, country-only labels, and `Автовыбор` | QA18 shows 8 user-facing entries and 6–8 ms results without protocol labels |
| P1 | Bypass and reserve were missing from the v6 location model | Sync closed-control-plane locations directly and prepend `Автовыбор` | QA18 shows auto + bypass + reserve + NL/DE/RU/KZ/US |
| P1 | Automatic ping runner cancelled itself after mutable ping updates | Depend on stable server identity instead of mutable ping values | All seven concrete locations complete in one UI run |

## Profile and transport evidence

- The current `ddlvpn.lol` subscription is the release source of truth; obsolete legacy subscription links are excluded from the audit.
- App API country candidates match the current subscription's authentication ID, endpoint and port, Reality key, short ID, SNI, and flow.
- Candidate multiplicity: Germany 4, Netherlands 2, United States 2, Kazakhstan 1, Russia 1. Bypass matches the current subscription; reserve is an app-only managed fallback.
- Local Xray SOCKS E2E, while the router/Wi-Fi VPN remained enabled: all 10 Reality nodes, bypass, and reserve returned HTTPS 200 from the first-party health endpoint.
- The 10 Reality nodes and bypass also returned Apple captive-portal HTTP 200. Reserve returned first-party health 200 but not captive-portal success, which is why release verification now uses the first-party health endpoint.
- A remote VPS could reach only a subset of nodes, while every node worked from the actual Mac network. This is treated as source-network filtering/routing evidence, not a client profile mismatch.

## Build and static verification

- `npm run build`: pass.
- `cargo test --features app-store`: 75 tests pass.
- `cargo clippy --all-targets --features app-store -- -D warnings`: pass.
- `cargo fmt --check`: pass.
- App Store readiness script: all gates pass.
- QA18 host and Packet Tunnel extension: `arm64` + `x86_64`.
- Host and extension are sandboxed, signed by the same Apple team, provisioned, and contain the required App Group and Packet Tunnel entitlements.
- QA18 version/build: `6.0.0 (60003)`.
- UI smoke: session persists, normal rounded macOS window, no white corners, 8 locations, visible ping action/results, no protocol labels.

## Final real Network Extension matrix

- [x] Login, kill app, relaunch, confirm session and device identity persist.
- [x] Retrieve every current location and complete profile-backed pings.
- [ ] Connect using `Автовыбор`; verify Packet Tunnel reaches `connected` and first-party health passes.
- [ ] Verify DNS over UDP and TCP, HTTPS traffic, large/MTU-sensitive transfer, and explicit IPv4/IPv6 behavior.
- [ ] Switch between two countries, bypass, and reserve without stale routing.
- [ ] Repeat connect/disconnect cycles and verify routes, DNS, and network state return to the captured baseline.
- [ ] Kill/relaunch while disconnected and verify no stale Network Extension state.
- [ ] Re-run full frontend/Rust/static/bundle gates after any E2E fix.
- [ ] Build with Apple Distribution, validate archive/export, then upload build 60003.
