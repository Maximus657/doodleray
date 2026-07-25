# TUN Connect Latency Phase 0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship one Windows build that both surfaces a per-phase connect-time breakdown in the Diagnostics "Copy report" and applies four latency fixes that provably cannot change TUN bring-up semantics.

**Architecture:** Four independent changes. Three are in the app/lib crate (`src-tauri/src/lib.rs`, `src-tauri/src/ipc.rs`), one is in the separate Windows service binary (`src-tauri/src/service.rs`). Timing data already collected by the service is routed into the existing `ConnectionHealthReport` → `copy_text` path rather than through a new IPC call; app-side phases the service cannot see are recorded in a process-global buffer and appended to the same report.

**Tech Stack:** Rust 2021, Tauri 2, `serde`/`serde_json`, Windows service via `windows-service` crate, React + TypeScript frontend (untouched by this plan).

## Global Constraints

- Target repo: `C:\Users\ilyae\Documents\DoodleRay PC`, branch `claude/windows-6.0.1-rc-hardening`.
- **Never change** TUN bring-up gate ordering or conditions (`src-tauri/src/service.rs:1551-1585`), config generation, or routing policy. The connect flow has a history of regressions.
- **Never** run `git add -A` in this repo. `backend_credits.md` is untracked and holds plaintext production credentials. Stage explicit paths only.
- `src-tauri/src/service.rs` is a **separate binary target** (`[[bin]] name = "DoodleRayService"`, `required-features = ["windows-service"]`), not part of the lib. `cargo test --lib` does **not** compile or test it. Tests for service.rs code run via `cargo test --bin DoodleRayService --features windows-service`.
- `src-tauri/src/ipc.rs` **is** part of the lib (`#[cfg(windows)] pub mod ipc;`), so its tests run under `cargo test --lib`.
- Mandatory verification checklist before any packaging, run from repo root:

```powershell
npm run build
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
cargo check --manifest-path .\src-tauri\Cargo.toml --bin DoodleRay
cargo check --manifest-path .\src-tauri\Cargo.toml --bin DoodleRayService --features windows-service
git diff --check
```

- Packaging is **only** via `npx tauri build` with all five env vars. `cargo build` directly, or `npx tauri bundle` without a preceding build, silently ships a broken or stale binary:

```powershell
$env:DOODLERAY_CLOSED_CONTROL_PLANE = "1"
$env:VITE_DOODLERAY_CLOSED_CONTROL_PLANE = "1"
$env:VITE_DOODLERAY_BUILD_CHANNEL = "direct"
$env:VITE_DOODLERAY_UPDATE_CHANNEL = "direct"
$env:VITE_DOODLERAY_DIAGNOSTICS_TELEMETRY = "0"
npx tauri build --bundles nsis --no-sign
```

- Claude **cannot** verify connect behaviour. There is no VPN on the dev machine; the user installs the produced installer on a separate Windows stand and tests there. Every task's acceptance is therefore local compile + unit tests only, plus the manual stand protocol in Task 6.
- Spec: `docs/superpowers/specs/2026-07-24-tun-connect-latency-phase0-design.md`.

---

### Task 1: Short exponential backoff for the named-pipe retry

The service reports `Running` to SCM before its named-pipe server is listening, so `send_tunnel_command` retries. It currently sleeps a flat 200ms 14 times. The pipe is normally up within tens of milliseconds, so the first retries should be cheap while the tail keeps the original budget.

**Files:**
- Modify: `src-tauri/src/ipc.rs:53-78`
- Test: `src-tauri/src/ipc.rs` (new `#[cfg(test)] mod tests` at end of file)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `fn pipe_retry_delay_ms(attempt: u32) -> u64` (private to `ipc`).

- [ ] **Step 1: Write the failing test**

Append to the end of `src-tauri/src/ipc.rs`:

```rust
#[cfg(all(test, windows))]
mod tests {
    use super::pipe_retry_delay_ms;

    #[test]
    fn pipe_retry_backoff_starts_short_and_settles_at_200ms() {
        let delays: Vec<u64> = (0..15).map(pipe_retry_delay_ms).collect();
        assert_eq!(&delays[..4], &[25, 50, 100, 200]);
        assert!(
            delays[4..].iter().all(|delay| *delay == 200),
            "tail must stay at 200ms: {delays:?}"
        );
        // Must not spend more wall clock than the previous flat 14 x 200ms.
        let total: u64 = delays[..14].iter().sum();
        assert!(total <= 14 * 200, "backoff budget grew to {total}ms");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml --lib pipe_retry_backoff`
Expected: FAIL — compile error, `cannot find function 'pipe_retry_delay_ms' in module 'super'`.

- [ ] **Step 3: Add the backoff function and use it**

In `src-tauri/src/ipc.rs`, insert immediately above `pub fn send_tunnel_command` (currently line 53-54):

```rust
/// Retry backoff for the SCM-running-before-pipe-listening race. The pipe is
/// usually accepting within tens of milliseconds, so the early retries are
/// cheap; the tail keeps the original budget for a genuinely slow start.
#[cfg(windows)]
fn pipe_retry_delay_ms(attempt: u32) -> u64 {
    match attempt {
        0 => 25,
        1 => 50,
        2 => 100,
        _ => 200,
    }
}
```

Then replace the loop body inside `send_tunnel_command` (currently lines 65-77):

```rust
    let mut last_error = String::new();
    for attempt in 0..15u32 {
        match send_tunnel_payload_with_timeout(payload.clone(), Duration::from_secs(6)) {
            Ok(response) => return Ok(response),
            Err(error) => {
                last_error = error;
                if attempt < 14 {
                    std::thread::sleep(Duration::from_millis(pipe_retry_delay_ms(attempt)));
                }
            }
        }
    }
    Err(last_error)
```

Leave the existing explanatory comment above the loop unchanged.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml --lib pipe_retry_backoff`
Expected: PASS — `test ipc::tests::pipe_retry_backoff_starts_short_and_settles_at_200ms ... ok`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ipc.rs
git commit -m "perf: short exponential backoff for tunnel pipe retries"
```

---

### Task 2: Cache `xray -test` validation by config hash

`check_xray_config` spawns a validation subprocess on every connect. `check_singbox_config` right above it already caches by config hash; mirror that. Also add an `xray_check_ms` counter so the cost is visible in the report built in Task 3.

**Files:**
- Modify: `src-tauri/src/service.rs:60` (static), `:98-104` (runtime fields), `:142-147` (defaults), `:1009-1014` (status build), `:1515` (call site), `:2037-2052` (the function), `:1850-1856` (setters)
- Modify: `src-tauri/src/tunnel_service.rs` (add `xray_check_ms` to `TunnelStatus`)
- Modify: `src-tauri/src/lib.rs:7330-7336` (existing test constructs a full `TunnelStatus`)
- Test: `src-tauri/src/service.rs` (new `#[cfg(test)] mod tests` inside `mod windows_service_main`)

**Interfaces:**
- Consumes: existing `hash_json_value(&Value) -> Result<u64, String>` (`service.rs:2058`), `elapsed_ms(Instant) -> u64` (`service.rs:1912`), `log_service_event(&str)`, `redact(&str)`.
- Produces: `fn check_xray_config(exe: &Path, config_path: &Path, config: &Value) -> Result<(), String>` (signature gains a third parameter); `fn xray_validation_cache() -> &'static Mutex<Option<u64>>`; `TunnelStatus.xray_check_ms: Option<u64>` consumed by Task 3.

- [ ] **Step 1: Write the failing test**

Add at the end of `mod windows_service_main` in `src-tauri/src/service.rs`, immediately before the closing brace of that module:

```rust
    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn xray_config_check_cache_hit_skips_the_subprocess() {
            let config = serde_json::json!({ "outbounds": [{ "tag": "proxy" }] });
            let hash = hash_json_value(&config).expect("hash config");
            *xray_validation_cache().lock().unwrap() = Some(hash);

            // Paths that cannot possibly execute: only a cache hit returns Ok.
            let missing_exe = Path::new(r"Z:\doodleray-nonexistent\xray.exe");
            let config_path = Path::new(r"Z:\doodleray-nonexistent\xray_config.json");
            assert!(check_xray_config(missing_exe, config_path, &config).is_ok());

            // A config that was never validated must still attempt the spawn.
            let other = serde_json::json!({ "outbounds": [{ "tag": "direct" }] });
            let error = check_xray_config(missing_exe, config_path, &other)
                .expect_err("uncached config must attempt to spawn xray");
            assert!(
                error.contains("failed to run"),
                "expected a spawn failure, got: {error}"
            );
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml --bin DoodleRayService --features windows-service xray_config_check`
Expected: FAIL — compile errors: `cannot find function 'xray_validation_cache'`, and `check_xray_config` takes 2 arguments but 3 were supplied.

- [ ] **Step 3: Add the cache static and accessor**

In `src-tauri/src/service.rs`, immediately after the `SINGBOX_VALIDATION_CACHE` static (line 60-61), add:

```rust
    static XRAY_VALIDATION_CACHE: OnceLock<Mutex<Option<u64>>> = OnceLock::new();
```

Immediately after `fn singbox_validation_cache()` (line 2054-2056), add:

```rust
    fn xray_validation_cache() -> &'static Mutex<Option<u64>> {
        XRAY_VALIDATION_CACHE.get_or_init(|| Mutex::new(None))
    }
```

- [ ] **Step 4: Add the `xray_check_ms` runtime field, default, setter, and status wiring**

In `struct TunnelRuntime` (line 99), immediately after `singbox_check_ms: Option<u64>,`:

```rust
        xray_check_ms: Option<u64>,
```

In `impl Default for TunnelRuntime` (line 142), immediately after `singbox_check_ms: None,`:

```rust
                xray_check_ms: None,
```

In the status builder (line 1009), immediately after `singbox_check_ms: runtime.singbox_check_ms,`:

```rust
            xray_check_ms: runtime.xray_check_ms,
```

Immediately after `fn set_singbox_check_ms` (line 1850-1852), add:

```rust
    fn set_xray_check_ms(value: u64) {
        state().lock().unwrap().xray_check_ms = Some(value);
    }
```

In `src-tauri/src/tunnel_service.rs`, in `pub struct TunnelStatus`, immediately after the `singbox_check_ms` field, add:

```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xray_check_ms: Option<u64>,
```

- [ ] **Step 5: Rewrite `check_xray_config` with the cache**

Replace `src-tauri/src/service.rs:2037-2052` entirely:

```rust
    fn check_xray_config(exe: &Path, config_path: &Path, config: &Value) -> Result<(), String> {
        let config_hash = hash_json_value(config)?;
        if *xray_validation_cache().lock().unwrap() == Some(config_hash) {
            set_xray_check_ms(0);
            log_service_event(
                "xray config check skipped: effective config hash was already validated",
            );
            return Ok(());
        }

        let started = Instant::now();
        let output = Command::new(exe)
            .args(["run", "-test", "-c"])
            .arg(config_path)
            .creation_flags(0x08000000)
            .output()
            .map_err(|error| format!("xray config check failed to run: {error}"))?;
        set_xray_check_ms(elapsed_ms(started));
        if output.status.success() {
            *xray_validation_cache().lock().unwrap() = Some(config_hash);
            return Ok(());
        }
        Err(format!(
            "xray config check failed: {}{}",
            redact(&String::from_utf8_lossy(&output.stdout)),
            redact(&String::from_utf8_lossy(&output.stderr))
        ))
    }
```

- [ ] **Step 6: Update the call site**

In `src-tauri/src/service.rs:1515`, change:

```rust
            check_xray_config(&xray_exe, &xray_config_path)?;
```

to:

```rust
            check_xray_config(&xray_exe, &xray_config_path, xray_config)?;
```

`xray_config` is the `&Value` already bound at line 1507-1510. Do not move or reorder any surrounding line.

- [ ] **Step 7: Fix the existing lib test that builds a full `TunnelStatus`**

In `src-tauri/src/lib.rs:7331`, immediately after `singbox_check_ms: Some(10),`, add:

```rust
            xray_check_ms: Some(5),
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml --bin DoodleRayService --features windows-service xray_config_check`
Expected: PASS — `test windows_service_main::tests::xray_config_check_cache_hit_skips_the_subprocess ... ok`

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml --lib`
Expected: PASS — all existing tests still green, including the `attach_tunnel_status_to_health` test at lib.rs:7339.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/service.rs src-tauri/src/tunnel_service.rs src-tauri/src/lib.rs
git commit -m "perf: cache xray -test validation by effective config hash"
```

---

### Task 3: Carry service phase timings into the Diagnostics copy report

The service already records `timings_ms` per phase plus the sub-phase counters, but none of it reaches the UI: `ConnectionHealthReport` does not carry it, and the `tunnel_service_diagnostics` command is registered yet never invoked from the frontend. Route it through `attach_tunnel_status_to_health`, which already receives the status snapshot, so no extra IPC round trip is added.

**Files:**
- Modify: `src-tauri/src/lib.rs:5156-5187` (`ConnectionHealthReport` fields)
- Modify: `src-tauri/src/lib.rs:12752-12789` (`attach_tunnel_status_to_health`)
- Modify: `src-tauri/src/lib.rs:13280-13301` (`copy_text` in `build_network_diagnosis`)
- Modify: `src-tauri/src/lib.rs:6374-6398` (`diag_health` test helper — it lists every field explicitly)
- Test: `src-tauri/src/lib.rs` (new test in the existing `#[cfg(test)] mod` that holds `diag_health`)

**Interfaces:**
- Consumes: `TunnelStatus.xray_check_ms` from Task 2; existing `TunnelStatus.timings_ms: Vec<(String, u64)>`, `.xray_spawn_ms`, `.singbox_check_ms`, `.powershell_fallback_count: u32`.
- Produces: `ConnectionHealthReport.service_timings_ms: Vec<(String, u64)>`, `.xray_spawn_ms: Option<u64>`, `.xray_check_ms: Option<u64>`, `.singbox_check_ms: Option<u64>`, `.powershell_fallback_count: u32`; a `connect_timings:` line inside `NetworkDiagnosisReport.copy_text`.

- [ ] **Step 1: Write the failing test**

Add to the same `#[cfg(test)] mod` that contains `diag_health` in `src-tauri/src/lib.rs`, after the `diagnosis_copy_text_has_no_secrets` test:

```rust
    #[test]
    fn diagnosis_copy_text_carries_service_connect_timings() {
        let mut health = diag_health("protected", vec![], vec![]);
        health.service_timings_ms = vec![
            ("waiting_adapter".into(), 4200),
            ("routes_ready".into(), 9100),
        ];
        health.xray_check_ms = Some(310);
        health.singbox_check_ms = Some(0);
        health.xray_spawn_ms = Some(45);
        health.powershell_fallback_count = 2;

        let report = build_network_diagnosis(&health, "tun", None, false);

        assert!(report.copy_text.contains("waiting_adapter=4200ms"));
        assert!(report.copy_text.contains("routes_ready=9100ms"));
        assert!(report.copy_text.contains("xray_check=310ms"));
        assert!(report.copy_text.contains("singbox_check=0ms"));
        assert!(report.copy_text.contains("xray_spawn=45ms"));
        assert!(report.copy_text.contains("powershell_fallbacks=2"));
    }

    #[test]
    fn diagnosis_copy_text_without_timings_says_none() {
        let health = diag_health("protected", vec![], vec![]);
        let report = build_network_diagnosis(&health, "tun", None, false);
        assert!(report.copy_text.contains("service_phases: none"));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml --lib diagnosis_copy_text_carries`
Expected: FAIL — compile error, `no field 'service_timings_ms' on type 'ConnectionHealthReport'`.

- [ ] **Step 3: Add the fields to `ConnectionHealthReport`**

In `src-tauri/src/lib.rs`, inside `pub struct ConnectionHealthReport`, immediately before the final `pub checks: Vec<ConnectionHealthCheck>,` line (currently line 5186):

```rust
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub service_timings_ms: Vec<(String, u64)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xray_spawn_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xray_check_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub singbox_check_ms: Option<u64>,
    #[serde(default)]
    pub powershell_fallback_count: u32,
```

- [ ] **Step 4: Update the `diag_health` test helper**

In `src-tauri/src/lib.rs:6395`, immediately after `endpoint_bypass_checks: Vec::new(),`:

```rust
            service_timings_ms: Vec::new(),
            xray_spawn_ms: None,
            xray_check_ms: None,
            singbox_check_ms: None,
            powershell_fallback_count: 0,
```

- [ ] **Step 5: Compile to find every other construction site**

Run: `cargo check --manifest-path .\src-tauri\Cargo.toml --bin DoodleRay`
Expected: errors of the form `missing fields 'service_timings_ms', ... in initializer of 'ConnectionHealthReport'`. Add the same five lines (all defaults: `Vec::new()`, `None`, `None`, `None`, `0`) to each reported construction site until this command passes. Do not change any other value at those sites.

- [ ] **Step 6: Populate the fields from the service status**

In `src-tauri/src/lib.rs`, inside `attach_tunnel_status_to_health`, immediately before the final line `health.verdict = service_health_verdict_to_report(&status.health_verdict).into();`:

```rust
    health.service_timings_ms = status.timings_ms.clone();
    health.xray_spawn_ms = status.xray_spawn_ms;
    health.xray_check_ms = status.xray_check_ms;
    health.singbox_check_ms = status.singbox_check_ms;
    health.powershell_fallback_count = status.powershell_fallback_count;
```

- [ ] **Step 7: Append the timings block to `copy_text`**

In `src-tauri/src/lib.rs`, immediately after the `checks_detail` binding (currently ending line 13284) and before the `let copy_text = format!(` line, insert:

```rust
    // Connect-time phase breakdown. The service already records these; they
    // were previously unreachable from the UI because nothing invoked the
    // tunnel_service_diagnostics command. powershell_fallback_count matters
    // because a native probe falling back to spawning PowerShell costs
    // seconds and is otherwise indistinguishable from a slow network.
    let service_phases = if health.service_timings_ms.is_empty() {
        "none".to_string()
    } else {
        health
            .service_timings_ms
            .iter()
            .map(|(phase, ms)| format!("{phase}={ms}ms"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let optional_ms = |value: Option<u64>| {
        value
            .map(|ms| format!("{ms}ms"))
            .unwrap_or_else(|| "-".into())
    };
    let timings_block = format!(
        "connect_timings: xray_check={} singbox_check={} xray_spawn={} powershell_fallbacks={}\nservice_phases: {}",
        optional_ms(health.xray_check_ms),
        optional_ms(health.singbox_check_ms),
        optional_ms(health.xray_spawn_ms),
        health.powershell_fallback_count,
        service_phases,
    );
```

Then change the `copy_text` format call to include it. Replace the format string and add the argument, keeping every existing argument in its current order:

```rust
    let copy_text = format!(
        "DoodleRay v{} | {}\nmode={} verdict={} gen={} cause={} repairable={} repair_tried={}\nfailed_checks: {}\n{}\n{}\n{}",
        env!("CARGO_PKG_VERSION"),
        windows_build_short(),
        proxy_mode,
        health.verdict,
        health
            .service_generation
            .map(|g| g.to_string())
            .unwrap_or_else(|| "-".into()),
        cause,
        can_auto_repair,
        repair_attempted,
        if failed_ids.is_empty() { "none".into() } else { failed_ids.join(", ") },
        timings_block,
        support_summary,
        checks_detail,
    );
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml --lib`
Expected: PASS for the two new tests and for the pre-existing `diagnosis_copy_text_has_no_secrets` (phase names and integers contain no secrets, so redaction is unaffected).

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: surface service connect phase timings in the diagnostics report"
```

---

### Task 4: Record app-side connect phases the service cannot see

The service's `timings_ms` starts inside bring-up, so it excludes the lease HTTP fetch, the cold SCM service start, and the pipe handshake. Record those app-side in a process-global buffer and append them to the same report.

**Files:**
- Modify: `src-tauri/src/lib.rs` (new statics + helpers, near the other `OnceLock` statics)
- Modify: `src-tauri/src/lib.rs:4959` (reset + lease timing in `app_connect_location`)
- Modify: `src-tauri/src/lib.rs:11726-11745` (`tunnel_service_start` phases)
- Modify: `src-tauri/src/lib.rs` `copy_text` block from Task 3
- Test: `src-tauri/src/lib.rs` (same `#[cfg(test)] mod`)

**Interfaces:**
- Consumes: `timings_block` from Task 3.
- Produces: `fn reset_connect_timings()`, `fn record_connect_timing(phase: &str, started: std::time::Instant)`, `fn connect_timings_snapshot() -> Vec<(String, u64)>`.

- [ ] **Step 1: Write the failing test**

Add to the same `#[cfg(test)] mod` in `src-tauri/src/lib.rs`:

```rust
    #[test]
    fn app_connect_timings_reset_and_record_in_order() {
        reset_connect_timings();
        let started = std::time::Instant::now();
        record_connect_timing("lease_fetch", started);
        record_connect_timing("service_start", started);

        let snapshot = connect_timings_snapshot();
        let phases: Vec<&str> = snapshot.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(phases, vec!["lease_fetch", "service_start"]);

        reset_connect_timings();
        assert!(connect_timings_snapshot().is_empty());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml --lib app_connect_timings_reset`
Expected: FAIL — `cannot find function 'reset_connect_timings' in this scope`.

- [ ] **Step 3: Add the buffer and helpers**

In `src-tauri/src/lib.rs`, immediately after the `ConnectionHealthReport` struct definition (after line 5187), add:

```rust
/// Connect phases measured in the app process. The service's own timings_ms
/// only starts once bring-up begins, so the lease fetch, the cold SCM service
/// start and the pipe handshake are invisible to it. Reset at the start of
/// each connect attempt; read back by the diagnostics report.
static LAST_CONNECT_TIMINGS: std::sync::OnceLock<std::sync::Mutex<Vec<(String, u64)>>> =
    std::sync::OnceLock::new();

fn last_connect_timings() -> &'static std::sync::Mutex<Vec<(String, u64)>> {
    LAST_CONNECT_TIMINGS.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

fn reset_connect_timings() {
    last_connect_timings().lock().unwrap().clear();
}

fn record_connect_timing(phase: &str, started: std::time::Instant) {
    let ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    last_connect_timings()
        .lock()
        .unwrap()
        .push((phase.to_string(), ms));
}

fn connect_timings_snapshot() -> Vec<(String, u64)> {
    last_connect_timings().lock().unwrap().clone()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml --lib app_connect_timings_reset`
Expected: PASS.

- [ ] **Step 5: Record the lease fetch and total in `app_connect_location`**

In `src-tauri/src/lib.rs`, immediately after the existing line 4959, which reads `let location_ids = app_connection_location_ids(&request);`, add:

```rust
    reset_connect_timings();
    let connect_started = std::time::Instant::now();
```

Inside the `for location_id in location_ids` loop, the lease is fetched at line 4969 as `let lease = match app_api_connection_profile(...).await {`. Immediately after that `match` statement's closing `};` (line 4983), add:

```rust
        record_connect_timing("lease_fetch", started);
```

`started` is the `Instant` already bound at the top of the loop body (line 4968).

Immediately before `let result = vpn_connect(connect_request, app.clone()).await;` (line 5007), add:

```rust
        let bringup_started = std::time::Instant::now();
```

Immediately after that same line, add:

```rust
        record_connect_timing("bringup", bringup_started);
        record_connect_timing("total", connect_started);
```

These are inside the fallback-location loop on purpose: if the first location
fails and a second is tried, the report shows one `bringup`/`total` pair per
attempt, which is exactly the signal needed to spot a wasted failed attempt.
Do not hoist them out of the loop.

- [ ] **Step 6: Record the service start and hello phases**

In `src-tauri/src/lib.rs`, in `tunnel_service_start`, replace lines 11726-11727:

```rust
    ensure_tunnel_service_running()?;
    let _ = ipc::tunnel_service_hello(env!("CARGO_PKG_VERSION"))?;
```

with:

```rust
    let service_start_started = std::time::Instant::now();
    ensure_tunnel_service_running()?;
    record_connect_timing("service_start", service_start_started);
    let hello_started = std::time::Instant::now();
    let _ = ipc::tunnel_service_hello(env!("CARGO_PKG_VERSION"))?;
    record_connect_timing("hello", hello_started);
```

- [ ] **Step 7: Append the app phases to the copy report**

In `src-tauri/src/lib.rs`, in the `timings_block` added in Task 3, extend it to include the app-side phases. Replace the `timings_block` binding with:

```rust
    let app_phases = {
        let snapshot = connect_timings_snapshot();
        if snapshot.is_empty() {
            "none".to_string()
        } else {
            snapshot
                .iter()
                .map(|(phase, ms)| format!("{phase}={ms}ms"))
                .collect::<Vec<_>>()
                .join(" ")
        }
    };
    let timings_block = format!(
        "connect_timings: xray_check={} singbox_check={} xray_spawn={} powershell_fallbacks={}\napp_phases: {}\nservice_phases: {}",
        optional_ms(health.xray_check_ms),
        optional_ms(health.singbox_check_ms),
        optional_ms(health.xray_spawn_ms),
        health.powershell_fallback_count,
        app_phases,
        service_phases,
    );
```

- [ ] **Step 8: Add a test that app phases reach the report**

Add to the same `#[cfg(test)] mod`:

```rust
    #[test]
    fn diagnosis_copy_text_carries_app_connect_phases() {
        reset_connect_timings();
        let started = std::time::Instant::now();
        record_connect_timing("lease_fetch", started);

        let health = diag_health("protected", vec![], vec![]);
        let report = build_network_diagnosis(&health, "tun", None, false);

        assert!(report.copy_text.contains("app_phases: lease_fetch="));
        reset_connect_timings();
    }
```

- [ ] **Step 9: Run the full lib test suite**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml --lib`
Expected: PASS. If `diagnosis_copy_text_without_timings_says_none` from Task 3 now fails because another test left timings behind, add `reset_connect_timings();` as its first line — the buffer is process-global and tests share it.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: record app-side connect phases in the diagnostics report"
```

---

### Task 5: Warm the tunnel service in parallel with the lease fetch

The service is stopped while disconnected, so `ensure_tunnel_service_running()` performs a cold SCM start — and today that runs strictly after the lease HTTP round trip has already finished. Start it concurrently. Both calls are idempotent and `tunnel_service_start` repeats them, so the warm-up is purely additive.

**Files:**
- Modify: `src-tauri/src/lib.rs:4959-4966` (before the location loop in `app_connect_location`)

**Interfaces:**
- Consumes: `reset_connect_timings()` from Task 4; existing `ensure_tunnel_service_running()` (`lib.rs:246`) and `ipc::tunnel_service_hello` (`ipc.rs:151`).
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Add the warm-up**

In `src-tauri/src/lib.rs`, immediately after the `reset_connect_timings();` / `let connect_started = ...;` lines added in Task 4 Step 5, and **before** `let selection_mode = ...`, add:

```rust
    // The Windows service is stopped while disconnected, so a cold SCM start
    // plus the named-pipe handshake would otherwise run strictly after the
    // lease HTTP round trip. Both calls are idempotent and tunnel_service_start
    // repeats them, so this is purely additive: winning the race makes the
    // later start free, losing it changes nothing. Deliberate side effect: the
    // service may stay up after a failed lease fetch, which it already does
    // between connect attempts. Starting the service creates no tunnel —
    // StartTunnel is a separate command.
    #[cfg(windows)]
    if request.proxy_mode == "tun" {
        tauri::async_runtime::spawn_blocking(|| {
            let _ = ensure_tunnel_service_running();
            let _ = ipc::tunnel_service_hello(env!("CARGO_PKG_VERSION"));
        });
    }
```

- [ ] **Step 2: Verify it compiles on both binaries**

Run: `cargo check --manifest-path .\src-tauri\Cargo.toml --bin DoodleRay`
Expected: PASS, no warnings about unused results.

Run: `cargo check --manifest-path .\src-tauri\Cargo.toml --bin DoodleRayService --features windows-service`
Expected: PASS.

- [ ] **Step 3: Confirm no gate semantics changed**

Run: `git diff HEAD~4 -- src-tauri/src/service.rs`
Expected: the only service.rs changes are the validation cache, the `xray_check_ms` field/setter, and the call-site argument. Confirm by inspection that lines 1551-1585 (`waiting_adapter` → `adapter_ready` → `xray_ready` → `singbox_ready` → `dual_stack_ready` → `routes_ready` → `dns_ready`) are untouched. If anything there differs, revert that hunk.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "perf: warm the tunnel service in parallel with the lease fetch"
```

---

### Task 6: Full verification and stand build

**Files:**
- No source changes. Produces `src-tauri/target/release/bundle/nsis/DoodleRay_6.0.2_x64-setup.exe`.

**Interfaces:**
- Consumes: all of Tasks 1-5.
- Produces: an unsigned RC installer for the Windows stand.

- [ ] **Step 1: Run the mandatory checklist**

Run each, from the repo root, and confirm all pass before continuing:

```powershell
npm run build
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
cargo check --manifest-path .\src-tauri\Cargo.toml --bin DoodleRay
cargo check --manifest-path .\src-tauri\Cargo.toml --bin DoodleRayService --features windows-service
git diff --check
```

Expected: `npm run build` succeeds; lib tests all green; both `cargo check` clean; `git diff --check` prints nothing.

- [ ] **Step 2: Run the service binary tests**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml --bin DoodleRayService --features windows-service`
Expected: PASS, including `xray_config_check_cache_hit_skips_the_subprocess`.

- [ ] **Step 3: Confirm nothing secret is staged**

Run: `git status --short`
Expected: `backend_credits.md` still shows as untracked (`??`) and appears in no commit from this plan. If it is staged, unstage it immediately with `git restore --staged backend_credits.md`.

- [ ] **Step 4: Build the installer**

```powershell
$env:DOODLERAY_CLOSED_CONTROL_PLANE = "1"
$env:VITE_DOODLERAY_CLOSED_CONTROL_PLANE = "1"
$env:VITE_DOODLERAY_BUILD_CHANNEL = "direct"
$env:VITE_DOODLERAY_UPDATE_CHANNEL = "direct"
$env:VITE_DOODLERAY_DIAGNOSTICS_TELEMETRY = "0"
npx tauri build --bundles nsis --no-sign
```

Expected: an installer at `src-tauri/target/release/bundle/nsis/DoodleRay_6.0.2_x64-setup.exe`. If a previously-cached frontend is suspected (fresh build still showing old UI), run `cargo clean --release -p doodleray --manifest-path .\src-tauri\Cargo.toml` and rebuild.

- [ ] **Step 5: Hand the stand protocol to the user**

Claude cannot verify connect behaviour — there is no VPN on the dev machine. Give the user these instructions verbatim:

1. On the Windows stand, uninstall the current DoodleRay through Windows Settings (full uninstall, not just closing the app).
2. Install the new `DoodleRay_6.0.2_x64-setup.exe`.
3. Connect in protected mode (`Весь компьютер`). Note roughly how long it takes.
4. Open Diagnostics, press **Копировать отчёт**, paste the result back.
5. Disconnect, reconnect, and copy the report a second time. The second connect should show `xray_check=0ms` (validation cache hit) — that difference is the quickest confirmation the build is the new one.
6. Repeat once more so there are three samples.

- [ ] **Step 6: Record the outcome**

Once the reports arrive, read the `connect_timings`, `app_phases` and `service_phases` lines. The largest single phase determines Phase 1 from the spec's candidate list: gate parallelisation, PowerShell fallback elimination (if `powershell_fallbacks` is non-zero), or `waitForConnectionHealth` quantisation. Do not start Phase 1 before these numbers exist.

---

## Notes on what this plan deliberately does not do

- No change to gate ordering, readiness conditions, config generation, or routing policy.
- No frontend changes. The report reaches the user through the existing "Копировать отчёт" button, which already copies `copy_text` in full.
- No balancer-related changes. `constrain_xray_config_to_managed_policy` is untouched.
- No persistent Wintun adapter reuse, no XHTTP mode pinning, no QUIC experiments — see the spec's out-of-scope section.
