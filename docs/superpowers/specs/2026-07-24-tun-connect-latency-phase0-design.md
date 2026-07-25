# TUN Connect Latency — Phase 0 Design

Date: 2026-07-24.
Branch: `claude/windows-6.0.1-rc-hardening`.
Status: approved, ready for implementation planning.

## Problem

TUN-mode connect takes ~10-15s in real-world use. Target is 2-3s, with 5s as
the hard ceiling. System-proxy mode is already fast, so this is specific to the
protected/TUN bring-up path.

`docs/optimization-research.md` contains a 13-item deep-research plan for this.
That document was written **without repository access** (it says so itself) and
its architectural model of the client is substantially wrong. This spec records
what was verified against the actual code, and scopes the first implementation
phase against those findings rather than against the research document's
guesses.

## Verification: research document vs actual code

### Already implemented (research doc did not know)

| Research item | Reality |
|---|---|
| #2 Replace polling with Windows notifications (claimed 0.2-0.6s) | Already done. `windows_net::network_event_cursors()` drives native IP Helper events. The 125/175/75ms intervals are fallback ticks between events, not naive sleep loops. |
| #3 Remove external-IP probe from the route gate (claimed 0.3-1.5s, "one of the largest") | Already done. The gate is `ensure_doodleray_route_preferred_native` → `windows_net::route_canaries_prefer_adapter`, i.e. `GetBestRoute2` against hardcoded IP literals — pure OS routing-table decision, no HTTP. The HTTP canary was already moved off the connect path into `spawn_nonfatal_policy_checks` (`service.rs:1588-1592` documents exactly this). |
| #13 Field-phase instrumentation | Already exists: `timings_ms` per phase (`service.rs:1818`), plus `xray_spawn_ms`, `singbox_check_ms`, `native_probe_ms`, `fallback_probe_ms`, `powershell_fallback_count`, `adapter_probe_backend`, `route_probe_backend`. A QA harness also exists at `scripts/windows-qa/Invoke-DoodleRayConnectPerfQa.ps1`. |
| #5 Cache `xray -test` by config hash | Half done. `check_singbox_config` (`service.rs:1994`) already caches by `hash_json_value`. `check_xray_config` (`service.rs:2037`) does not. |

### Mis-modelled by the research document

**Two engines, not one.** The doc assumes xray owns the TUN. Actually xray is
spawned first for the loopback SOCKS/HTTP proxy, and then **sing-box** is
spawned separately and owns the Wintun adapter (`service.rs:1505-1549`).

Consequence: research item **#8 (persistent Wintun adapter reuse)** is not
implementable as described. DoodleRay calls no Wintun API anywhere in
`src-tauri/src`; adapter lifecycle is entirely inside sing-box. Achieving reuse
would require patching sing-box or moving TUN ownership into our own code.

**UI health polling (#4).** Steady-state polling is 30s (`Dashboard.tsx:813`),
not 1.5s. The 1.5s figure is `waitForConnectionHealth`
(`connection-health.ts:118`, 8 attempts x 1500ms in TUN mode). But in TUN mode
`get_connection_health` takes the cheap branch —
`build_current_tun_connection_health` just reads the service snapshot over IPC —
and the verdict flips to `protected`/`protected_degraded` as soon as service
state is `Connected` (`service.rs:1061-1072`). Both verdicts satisfy
`isHealthAcceptable`, so the loop normally breaks on the first or second
attempt. The claimed 0.75-1.5s average win is not there.

### Confirmed as real

- **#6 Lease fetch is strictly serial.** `app_api_connect` awaits
  `app_api_connection_profile` fully before calling `vpn_connect`
  (`lib.rs:4969`, `lib.rs:5007`).
- **#7 SCM-running vs pipe-ready race.** `ipc.rs:66` — exactly `15 x 200ms`
  fixed retries, with the comment explaining the race.
- **#1 Post-spawn gates are serial:** `waiting_adapter` (15s budget) → ports
  (8s) → `dual_stack` (20s) → `route` (20s), at `service.rs:1551-1585`.
- **Cold service start on every connect.** The service is stopped while
  disconnected (`lib.rs:11973`), and `ensure_tunnel_service_running()`
  (`lib.rs:246`) performs a cold SCM start with 100ms polling and a 10s budget
  — all of it *after* the lease fetch has already completed.

### Balancers — correction to the stated constraint

The requirement was "do not break the balancers". Verified: 
`constrain_xray_config_to_managed_policy` (`lib.rs:2209`) executes
`routing.remove("balancers")` and drops every rule carrying a `balancerTag`,
collapsing outbounds to a single `proxy` tag — whenever `routing_policy` is
`Some(...)`. The managed closed-control-plane path **always** sets it
(`lib.rs:4996`).

So on the production managed path, server-supplied balancers are already
discarded today, before xray ever sees them. They survive only on the manual
subscription-import path, where `subscription.ts:112` expands them into
separate selectable entries in the server list.

This is out of scope for latency work, but it is a real finding worth its own
investigation: it may explain why a client like Happ, where the balancer stays
live, behaves differently from ours, which connects to one fixed outbound from
the lease.

## Scope decision

Chosen approach: **measure and fix safe items in a single build.**

Rationale: the build/install cycle is expensive — testing happens by installing
the produced installer on a separate Windows stand, not on the developer
machine (a VPN on the dev machine breaks the working session). Spending a whole
install cycle on measurement alone is wasteful, but blind-fixing the fragile
connect path is worse. Phase 0 therefore ships measurement plus only those
fixes that provably cannot change bring-up semantics.

## Phase 0 changes

### 1. Surface connect phase timings in the Diagnostics "Copy report"

The data already exists in `TunnelStatus` (`tunnel_service.rs:167`) but never
reaches the UI: `ConnectionHealthReport` (`lib.rs:5156`) does not carry it, and
the `tunnel_service_diagnostics` command (`lib.rs:11991`) is registered but
never invoked from the frontend.

Route it through the path already being walked — `build_current_tun_connection_health`
already calls `tunnel_service_status_for_health()`, so no extra IPC round trip
is needed.

- Add to `ConnectionHealthReport`: `timings_ms`, `xray_spawn_ms`,
  `singbox_check_ms`, `powershell_fallback_count`.
- Populate them in `build_current_tun_connection_health` from the status
  snapshot it already holds.
- Append a compact block to `copy_text` in `build_network_diagnosis`
  (`lib.rs:13285`).

`powershell_fallback_count` is included deliberately: when native adapter/route
probes fail, the code falls back to `wait_for_adapter_powershell_once` and
`ensure_doodleray_route_preferred_powershell`, which spawn PowerShell
processes and can cost seconds. On a Windows Server stand this is plausible,
and without this counter it is indistinguishable from "slow network".

Service-side `timings_ms` is measured from the start of bring-up *inside* the
service, so it excludes lease fetch, service start, and the pipe handshake.
Record those app-side in a `LAST_CONNECT_TIMINGS` static: `lease_fetch`,
`service_start`, `hello`, `bringup`, `total`. Both blocks go into the report.

No secrets are involved — phase names and integers only — so the existing
redaction tests (`diagnosis_copy_text_has_no_secrets`) are unaffected. A test
asserting timings appear in `copy_text` is added.

### 2. Cache `xray -test` by config hash

`check_xray_config` (`service.rs:2037`) spawns a validation subprocess on every
connect. Mirror the existing, proven pattern from `check_singbox_config`
(`service.rs:1994`): an `XRAY_VALIDATION_CACHE` keyed on `hash_json_value` of
the effective config, plus `set_xray_check_ms` for visibility.

Cache invalidation: the cache lives in service memory, so it clears on service
restart and on any config change. An xray binary upgrade always arrives with a
reinstall, which stops the service — so no separate binary-version key is
needed.

### 3. Warm the service in parallel with the lease fetch

Today `app_api_connect` runs strictly in sequence: await profile →
`vpn_connect` → `tunnel_service_start` → `ensure_tunnel_service_running()`
(cold SCM start, 100ms polling) → `tunnel_service_hello` (through the pipe
retry).

Start a `spawn_blocking` task performing `ensure_tunnel_service_running()` +
`tunnel_service_hello()` **once, before the location loop**, and do not await
it. `tunnel_service_start` will invoke both again, and both are idempotent —
`ensure_tunnel_service_running` queries state first and returns immediately on
`Running`.

The warm-up is therefore purely additive: if it wins the race the start is
free; if it does not, behaviour is exactly today's.

Deliberate side effect: the service comes up slightly earlier and stays up if
the lease fetch fails. It already stays up between connect attempts today, and
no tunnel is created by service start alone — `StartTunnel` is a separate
command.

Applies to the managed path only. The manual subscription path has no lease
fetch, so there is nothing to overlap with.

### 4. Short exponential backoff for the pipe retry

`ipc.rs:66` retries `15 x 200ms` at a fixed interval. Replace with
`25, 50, 100, 200, 200...`, preserving the tail budget while making the common
first retry 8x cheaper. This is the shared path for all IPC commands, so the
gain spreads across the whole connect rather than just the hello.

## Safety invariants

None of the four changes alters gate ordering, readiness conditions, config
generation, or routing policy. The balancer-stripping code is not touched.
Failure semantics are unchanged: nothing that currently fails closed starts
failing open.

## Verification

Mandatory local checklist from `CLAUDE.md` before any build:

```powershell
npm run build
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
cargo check --manifest-path .\src-tauri\Cargo.toml --bin DoodleRay
cargo check --manifest-path .\src-tauri\Cargo.toml --bin DoodleRayService --features windows-service
git diff --check
```

New tests, one each for the non-trivial logic:

- xray validation cache: a cache hit does not spawn the subprocess.
- pipe backoff: the delay sequence is the intended one.
- diagnosis: `copy_text` contains the timings block and still leaks no secrets.

Packaging must use the full env-var set from `CLAUDE.md` — omitting them
silently ships a broken build:

```powershell
$env:DOODLERAY_CLOSED_CONTROL_PLANE = "1"
$env:VITE_DOODLERAY_CLOSED_CONTROL_PLANE = "1"
$env:VITE_DOODLERAY_BUILD_CHANNEL = "direct"
$env:VITE_DOODLERAY_UPDATE_CHANNEL = "direct"
$env:VITE_DOODLERAY_DIAGNOSTICS_TELEMETRY = "0"
npx tauri build --bundles nsis --no-sign
```

## Measurement protocol

On the Windows stand: uninstall the previous version through Windows Settings,
install the Phase 0 build, connect in protected mode 2-3 times, then open
Diagnostics and press "Copy report" after each connect. The report now carries
the full phase breakdown; paste it back for Phase 1 planning.

## Phase 1 candidates (to be selected by the numbers)

Not decided in advance. Known candidates, in the order the evidence is most
likely to justify:

1. Parallelise the post-spawn gate chain `adapter → ports → dual_stack → route`
   (`service.rs:1551-1585`) into concurrent latches. This is the genuine
   remaining orchestration win, and also the change most likely to regress the
   fragile connect path — hence evidence first.
2. Eliminate PowerShell fallbacks if `powershell_fallback_count` shows they are
   firing on the stand.
3. Shorten `waitForConnectionHealth` quantisation if the app-side block shows
   meaningful time there.

## Explicitly out of scope

- **#8 persistent Wintun adapter reuse** — not implementable without patching
  sing-box; DoodleRay calls no Wintun API.
- **#10 XHTTP mode pinning, #11 QUIC/H3 A/B** — negligible effect on cold
  connect, and the managed path carries no balancers anyway.
- **#12 speculative pre-warm of routes/data plane** — deliberately avoided;
  would blur the definition of "disconnected".
- **Balancer stripping on the managed path** — real finding, separate task.
