# Windows v6.0.1 RC evidence — 2026-07-23

## Scope

Redacted native QA evidence for the direct Windows channel. No credentials, subscription URL, endpoint, IP address, token, or user identifier is recorded here.

## Passed local checks

- `cargo fmt --check`
- service-focused regression tests for retained TUN route/DNS cleanup and the absent-adapter fast path
- Rust library tests: 101 passed, 3 intentionally ignored
- production-configured Windows NSIS RC built with both closed-control-plane flags enabled

## Passed QA-stand checks

- An earlier packaged RC passed its install gate: clean service status JSON, no `xray api statsquery` orphan, and injected stale WinINet state cleanup.
- Migration path from public `5.9.1` with the canonical DoodleVPN test subscription restored the v6 closed-API session automatically. The device was allowed, locations loaded, and no sign-in code was required.
- Reinstalling the RC preserved that authenticated session and still required no code entry.
- Protected-mode split-routing/DNS validation passed twice after a normal disconnect: direct-process exclusions remained direct, protected IPv4/IPv6 route canaries selected `DoodleRay Tunnel`, and DNS resolution succeeded.
- The exact final `DoodleRayService.exe` was staged on the QA stand and its clean connection reached protected route and DNS readiness in 4.23 seconds, with native route probes and no PowerShell fallback.
- Final normal Disconnect stopped the on-demand service and left no DoodleRay adapter, adapter routes, or adapter DNS servers.

## Production gate remaining

This is an unsigned local RC smoke run only. The final local NSIS package was built, but the public release remains blocked until GitHub Actions produces and verifies Authenticode-signed Windows artifacts and updater signatures with the required CI secrets.
