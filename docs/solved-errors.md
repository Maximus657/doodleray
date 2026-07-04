# Solved Errors

## 2026-07-03 - Windows protected fallback - app timeout and stale service failure could mask a valid recovery

- Symptom/command: the RC5 targeted fallback test passed, but the full
  `Invoke-DoodleRayFullStandQa.ps1` matrix could still fail or hang around the
  UI protected-connect stages. The user-facing risk was worse: a slow
  service-side TUN repair could exceed the UI's shorter timeout, and an older
  failed protected generation could publish `failed` after a newer disconnect
  or browser-fallback generation had already started.
- Root cause: the service had a bounded 90s protected bring-up/repair window,
  while the dashboard still used a 45s connect timeout in some mode-switch and
  fallback paths. Separately, `start_tunnel` error cleanup called
  `stop_owned_processes("failed_cleanup")` and then wrote a failed status
  without re-checking that the generation was still current. Browser fallback
  also disconnected the app-side proxy engine but could leave the service's
  failed protected snapshot sticky behind it.
- Fix: TUN connect and reconnect paths now use a 120s UI budget and share the
  same honest limited fallback path for both returned failures and thrown
  timeout/error failures. On Windows, `vpn_disconnect` always asks
  `DoodleRayTunnelService` to stop even for proxy-only mode, which clears any
  stale protected generation before/after fallback. Service failure paths now
  re-check `is_current_generation(generation)` after cleanup and log stale
  failure results instead of overwriting newer runtime truth.
- Verification: full Play2Go Server 2022 run
  `Invoke-DoodleRayFullStandQa.ps1 -AllowUnsignedLocalRc` reached
  `FULL STAND QA COMPLETE` on setup
  `85A8B3A7A6AF5539FCBA68A38EF87C1CF864F568324C022BBF3898DF7DBCBA22`;
  the included `auto-fallback-protected-to-browsers` stage forced protected
  failure, verified browser proxy fallback with Apple captive `HTTP_CODE=200`,
  verified no TUN claim, and left the stand clean. Local gates after the run:
  `npm run build`, both Rust binary `cargo check`s, `cargo test --lib`
  (`54 passed`, `3 ignored`), QA PowerShell parse checks, and
  `git diff --check`.

## 2026-07-03 - Windows QA harness - network tests broke their own SSH control stream

- Symptom/command: the first full stand run after RC5 passed earlier stages but
  failed around the UI pass when the SSH/plink stream was interrupted while
  the tested app changed routes/proxies during TUN scenarios. Another false
  blocker appeared on some runner shells where `Get-FileHash` was unavailable.
- Root cause: long-running remote UI tests were executed inline through the
  same control channel that the VPN test intentionally perturbs. The publish
  stage also assumed the Microsoft.PowerShell.Utility module was available for
  hashing on every stand session.
- Fix: `Invoke-DoodleRayRc3UiCdpPass.ps1` now uploads a remote script and runs
  it as a detached scheduled task in the interactive admin session, then polls
  a summary JSON. The pass is QA-control-first and CDP-optional, so WebView2
  remote-debugging availability is no longer required for the core gates.
  `Publish-DoodleRayQaInstaller.ps1` now has a .NET SHA-256 fallback.
- Verification: the final one-command full stand run completed all stages,
  including `rc-ui-cdp-pass`, `auto-fallback-protected-to-browsers`, and
  `deep-snapshot`, on the same Play2Go Server 2022 stand.

## 2026-07-03 - Windows protected mode - TUN failure did not always degrade to Browsers

- Symptom/command: the new auto-fallback code existed, but the end-to-end QA
  blocker `Protected -> Browsers` was still not proven. When protected bring-up
  failed through the thrown/catch path instead of a normal `{ success: false }`
  `vpn_connect` result, the UI could run generic cleanup/reporting without
  trying the limited browser compatibility fallback.
- Root cause: fallback logic lived in the result-failure branch only. The catch
  branch exported/cleaned the failed protected attempt but did not share the
  same TUN-failure classifier. The QA harness also treated
  `DoodleRayTunnelService=disconnected` as fully clean, which is false after
  browser fallback because the app-owned xray HTTP proxy and WinINet can be
  active while the TUN service is idle.
- Fix: moved the TUN limited-fallback classifier into a shared
  `attemptLimitedBrowsersFallback` path used by both result failures and thrown
  failures. The fallback first disconnects the failed protected generation,
  then starts Browsers compatibility with explicit LIMITED messaging. The QA
  control surface now publishes a sanitized frontend snapshot so harnesses can
  verify subscription/server counts without reading secrets or relying on CDP.
  `Stop-QaTunnelHard` now checks app connectivity, frontend state, and loopback
  WinINet before declaring cleanup complete.
- Verification: `Test-DoodleRayAutoFallback.ps1` on Play2Go Server 2022 passed:
  protected start observed in service log, no protected claim, browser fallback
  WinINet `127.0.0.1:59351`, Apple captive GET `HTTP_CODE=200`, no TUN adapter
  during fallback, and final cleanup `service=disconnected winInet=0 engines=0
  marker=False adapter=False`. Local checks: `npm run build`,
  `cargo check --manifest-path src-tauri\Cargo.toml --bin DoodleRay`,
  `cargo check --manifest-path src-tauri\Cargo.toml --bin DoodleRayService`,
  QA script parser checks.

## 2026-07-03 - Windows service registration vanished after repeated repair installs

- Symptom/command: during stand QA the `DoodleRayTunnelService` registration
  disappeared from SCM twice (`Get-Service` returned nothing, status IPC
  failed) after sequences of app startup repair and QA service restarts.
  System event log showed repeated `7045 service installed` / `7036 stopped`
  pairs with the registration gone after the final stop.
- Root cause: `install_service` always ran `repair_existing_service`, which
  unconditionally stops and **deletes** any existing registration before
  recreating it. The app's startup repair calls `install_tunnel_service` on
  any transient IPC failure, so healthy registrations were being
  delete+recreated repeatedly; if any SCM handle is open during `delete()`
  (QA tooling, event viewers), Windows only marks the service
  delete-pending and it silently vanishes at its next stop.
- Fix: `install_service` is now idempotent: if a registration exists and its
  binary path matches the current `DoodleRayService.exe`, it is adopted
  (SID/config refreshed, started if needed) without delete/recreate. The
  destructive repair path remains only for registrations pointing at a
  different/broken binary.
- Verification: `cargo test --lib`, `cargo check` both bins; on the stand a
  double `DoodleRayService.exe install` run keeps a single healthy Running
  registration with no new 7045 churn.

## 2026-07-03 - Windows protected mode - adapter-missing error reached the user without repair

- Symptom/command: production users saw
  `Full Computer components not installed or not ready: DoodleRay Tunnel IPv4 readiness failed: DoodleRay Tunnel adapter is missing`
  on Whole Computer connect.
- Root cause: in the service `start_tunnel` bring-up, `wait_for_adapter` can
  pass and then the wintun adapter disappears (typically the service-owned
  sing-box dies right after startup, or the adapter fails to bind IPv4).
  `wait_for_doodleray_ipv4_interface` then polls `apply_doodleray_interface_metric`
  for 20s, keeps getting `DoodleRay Tunnel adapter is missing` (its exit-2
  branch), and the raw error propagated through `tunnel_service_start` to the
  UI with no repair attempt.
- Fix: bounded two-attempt bring-up in the service. The engine spawn/adapter/
  IPv4/route phase is extracted into `bring_up_tun_runtime`; a first-attempt
  failure classified by `tunnel_service::is_repairable_tun_bringup_error`
  (adapter missing/not ready, IPv4 readiness, route readiness, sing-box died
  at startup - never cancellation or config errors) triggers exactly one
  DoodleRay-owned repair: `stop_owned_processes("tun_adapter_repair")` (kills
  only service-owned children; the wintun adapter dies with its owner), session
  marker rewrite, status stays `repairing`, then a full second bring-up. Only a
  second failure surfaces, now actionable via
  `tunnel_service::format_tun_bringup_failure` ("DoodleRay could not create
  the Windows tunnel adapter: ... wintun.dll=..., last_phase=..., attempts=N").
  The app-side wait loop got a matching guard (brief `Disconnected` with
  `last_repair_action=tun_adapter_repair` is not terminal) and a 90s budget;
  `tunnel_service_start` also gained a preflight that fails with a reinstall
  message when `wintun.dll`/`sing-box.exe`/`xray.exe`/`DoodleRayService.exe`
  are missing before TUN is even attempted. A successful retry leaves a
  `TUN adapter repair retry ran after: ...` warning in structured health, so
  support bundles show the self-repair.
- Verification: `cargo test --manifest-path src-tauri/Cargo.toml --lib`
  (52 passed; new classifier/formatter tests), `cargo check` both bins,
  `npm run build`, `git diff --check`. Stand evidence tracked in
  `docs/windows-tun-release-qa-report.md` (bring-up crash injection during the
  Connecting window).

## 2026-07-02 - Windows service runtime dir hardening bricked its own files (empty DACL)

- Symptom/command: on the Play2Go stand, `Get-Content C:\ProgramData\DoodleRay\service.log` returned `Access is denied` even for an elevated admin; a SYSTEM scheduled-task probe showed `icacls` listing the file with an empty DACL and `type` failing as SYSTEM; the new v6 `active-session.marker` planted for unclean-shutdown QA was never consumed by `detect_previous_unclean_shutdown` (silent `read_to_string` failure), while marker deletion still worked (via directory `DELETE_CHILD`); NSIS install hooks printed `Access is denied` noise for old runtime files on reused machines.
- Root cause: `secure_directory_acl` ran `icacls <dir> /inheritance:r /grant:r "*S-1-5-18:(OI)(CI)F" "*S-1-5-32-544:(OI)(CI)F" ... /T`. With `/T` the command also processed every file in the tree: `/inheritance:r` stripped the files' inherited ACEs while the container-inherit `(OI)(CI)` grants carry no effective access on file objects, leaving files with an empty DACL that even LocalSystem could not read. Every service start re-bricked the tree.
- Fix: the hardening now applies `/inheritance:r`/`/grant:r`/`/remove:g` to the directory object only (no `/T`), then runs `icacls <dir>\* /reset /T /C /Q` so existing children re-derive SYSTEM/Administrators-only ACLs from the hardened directory through normal inheritance. Newly created files inherit the `(OI)(CI)` grants directly.
- Verification: `cargo test --manifest-path src-tauri/Cargo.toml --lib` (49 passed), `cargo check` for both bins, and on the Play2Go stand after installing the fixed RC: `service.log` readable by admin and SYSTEM, and the unclean-shutdown marker QA (`Test-DoodleRayUncleanShutdownMarker.ps1`: plant marker, hard-kill service, SCM recovery) publishes `previous_unclean_shutdown` and consumes the marker.

## 2026-07-01 - Windows subscription import - direct-only fetch ignored working system proxy

- Symptom/command: installed `5.4.5` showed `Failed to fetch subscription: Fetch failed: connection error` for the canonical DoodleVPN test subscription from `secrets/doodlevpn-test-subscription-url.txt` before the user could connect any VPN profile.
- Root cause: `fetch_subscription_url` and legacy `fetch_url` used `direct_fetch_client(...).no_proxy()` only. That is correct for avoiding stale DoodleRay proxy loops, but too strict for first-run subscription import: if the subscription host is blocked or only reachable through an already-working system proxy/another VPN client, DoodleRay failed before it had any server list.
- Fix: subscription URL fetching now tries the direct hardened client first, then falls back to the current WinINet manual HTTP/HTTPS proxy when it is enabled and not a stale DoodleRay-owned proxy. The WinINet parser supports simple `host:port` and protocol-mapped `http=...;https=...` formats, and skips SOCKS-only entries because this reqwest build does not enable SOCKS.
- Verification: `cargo test --manifest-path src-tauri/Cargo.toml --lib`, `cargo check --manifest-path src-tauri/Cargo.toml --bin DoodleRay`, `cargo check --manifest-path src-tauri/Cargo.toml --bin DoodleRayService`, `npm run build`, `git diff --check`, and `cargo test --manifest-path src-tauri/Cargo.toml --all-targets`. A release-mode Rust test harness was also run on the dedicated Play2Go Windows QA stand: `tests::windows_subscription_fetch_uses_system_proxy_fallback` passed and left WinINet clean (`ProxyEnable=0`, empty `ProxyServer`, empty `ProxyOverride`). NSA Cyber `HTTP-Connectivity-Tester` verified redacted HTTPS reachability for the subscription host and Apple GET ping endpoint; the real subscription endpoint returned HTTP `200` with non-empty content via both `Invoke-WebRequest` and `curl.exe`; a short `netsh trace scenario=InternetClient` capture around the fetch was saved on the server only. The stand exposed a clean-machine prerequisite gap too: the harness required `WebView2Loader.dll` plus Microsoft Visual C++ runtime availability before it could start.

## 2026-06-30 - Windows xray protected mode - connected state with broken DNS resolution

- Symptom/command: user screenshot showed DoodleRay connected in protected mode while the browser failed to resolve `www.google.com` (`server IP address could not be found` / name resolution failure). Phone keys worked, Happ keys worked, and the issue looked like PC protected-mode DNS rather than subscription failure.
- Root cause: xray+TUN bridge used sing-box HTTP outbound as the default `proxy` dataplane and SOCKS only for the explicit UDP rule. The bridge DNS server used UDP DNS with `detour=proxy`, which could send DNS over the HTTP outbound instead of the xray SOCKS inbound that supports UDP. Health also marked TUN DNS as OK from adapter/config presence and did not perform a real domain resolution probe.
- Fix: xray+TUN bridge now uses xray SOCKS inbound for the default `proxy` dataplane; HTTP inbound remains only for WinINet/manual browser compatibility. Windows TUN DNS health now performs an actual `www.google.com` A-record resolution probe with `Resolve-DnsName`/`nslookup` fallback, so broken DNS makes protected health fail instead of showing a clean connected state.
- Verification: `cargo test --manifest-path src-tauri/Cargo.toml --lib`, `cargo check --manifest-path src-tauri/Cargo.toml --bin DoodleRayService`, `cargo check --manifest-path src-tauri/Cargo.toml --bin DoodleRay`, `npm run build`, and `git diff --check`.

## 2026-06-30 - Windows protected mode - TUN core was treated as failed when only browser compatibility failed

- Symptom/command: protected mode could connect the Tunnel Service core, then report failures such as `HTTP listener: 127.0.0.1:<port> is not accepting connections` or `Windows proxy compatibility failed`, while proxy mode still worked and a reboot could make TUN work again.
- Root cause: the app treated WinINet/HTTP compatibility as fatal for protected mode and stopped the service-owned tunnel. The UI also relied on message/health text parsing and could keep checking stale local proxy ports after the service had selected runtime ports.
- Fix: protected-mode compatibility failures now return `success=true` with `health.verdict=protected_degraded`; TUN core stays up. `TunnelStatus` and `ConnectionHealthReport` now carry structured runtime ports/generation/op-id, and the UI stores actual runtime ports for TUN after connect/reload/health monitor. `repair_windows_runtime` also soft-reboots stale DoodleRay-owned service/engine state only when no connection is active.
- Verification: `cargo test --manifest-path src-tauri/Cargo.toml --lib`, `cargo check --manifest-path src-tauri/Cargo.toml --bin DoodleRayService`, `cargo check --manifest-path src-tauri/Cargo.toml --bin DoodleRay`, `npm run build`, `git diff --check`, and local NSIS RC bundle creation at `src-tauri/target/release/bundle/nsis/DoodleRay_5.4.5_x64-setup.exe`.

## 2026-06-29 - Windows protected mode - transient loopback proxy readiness false negative

- Symptom/command: on installed 5.4.4, protected mode failed with `Protected mode started but Windows proxy compatibility failed: HTTP proxy port 127.0.0.1:4491 is not ready`.
- Root cause: service diagnostics showed `DoodleRayService.exe` 5.4.4 and xray accepted protected-mode `http-in` traffic during the same 23:07 connection attempt, while sing-box was dialing the same `127.0.0.1:4491` bridge. The failure came from the app-side WinINet proxy apply path, where `apply_doodleray_proxy()` performed a second single-shot 250ms loopback readiness check after the service and app had already waited for the port. Under immediate TUN traffic load this could return a false negative and tear down an otherwise-started protected mode.
- Fix: loopback readiness in Windows sysproxy now retries for up to 3 seconds before declaring the HTTP compatibility proxy unavailable. xray stats collection also has a bounded timeout so `xray.exe api statsquery` cannot accumulate as stuck helper processes.
- Verification: read-only diagnostics confirmed installed app/service version 5.4.4 and xray `http-in` traffic at 23:07; local verification commands were `cargo check --manifest-path src-tauri/Cargo.toml --lib`, `cargo test --manifest-path src-tauri/Cargo.toml --lib sysproxy::tests::loopback_port_ready_waits_for_delayed_listener`, `cargo test --manifest-path src-tauri/Cargo.toml --lib`, `cargo check --manifest-path src-tauri/Cargo.toml --bin DoodleRayService`, and `git diff --check`.

## 2026-06-29 - Windows protected mode - duplicate app instances killed tunnel engines

- Symptom/command: after updating to 5.4.3, protected mode still failed with `HTTP listener: 127.0.0.1:<port> is not accepting connections`, while proxy mode worked.
- Root cause: diagnostics showed service version 5.4.3 but multiple `DoodleRay.exe` and multiple DoodleRay-owned `xray.exe` processes. The app's single-instance guard used a `Global\` mutex that can fail for normal users; on failure it allowed startup. A second app instance could then run startup cleanup or reconnect logic and stop the service/xray that the first protected-mode attempt was using.
- Fix: single-instance guard now uses a per-session `Local\` mutex and fails closed if it cannot claim it. After a successful claim, startup cleanup terminates only duplicate `DoodleRay.exe` and orphaned DoodleRay-owned `xray.exe`/`sing-box.exe` from the current install directory, leaving other VPN clients alone. The tunnel service also rechecks local SOCKS/HTTP ports after route readiness before reporting `Connected`.
- Verification: diagnostics confirmed duplicate DoodleRay/xray processes and the failed `127.0.0.1:<port>` bridge dials; local verification commands were `npm run build`, `cargo check --manifest-path src-tauri/Cargo.toml --lib`, `cargo check --manifest-path src-tauri/Cargo.toml --bin DoodleRayService`, and `cargo test --manifest-path src-tauri/Cargo.toml --lib`.

## 2026-06-29 - Windows protected mode - sing-box mixed stack panic killed local proxy

- Symptom/command: protected mode repeatedly reported unstable health and then failed with `SOCKS listener`/`HTTP listener` not accepting connections, while another VPN client could connect.
- Root cause: service diagnostics showed bundled `sing-box` crashed inside `sing-tun` mixed stack with `panic: runtime error: slice bounds out of range [:16] with capacity 8` in `Mixed.processIPv6`. The app defaulted Windows TUN to `networkStack: mixed`, and persisted old settings could keep using it after update.
- Fix: Windows TUN now coerces `mixed` to the stable `system` stack in backend config generation; frontend defaults/migration also normalize stored `mixed` to `system`, and the Settings UI no longer offers `Mixed`. The tunnel service now rechecks xray/sing-box liveness before reporting `Connected` and marks a connected tunnel failed if a managed engine exits.
- Verification: service diagnostics captured the redacted panic and failed loopback proxy dials; local verification commands for the fix were `npm run build`, `cargo check --manifest-path src-tauri/Cargo.toml --lib`, `cargo check --manifest-path src-tauri/Cargo.toml --bin DoodleRayService`, and `cargo test --manifest-path src-tauri/Cargo.toml --lib`.

## 2026-06-29 - Windows protected mode - default RU split routing missing

- Symptom/command: in protected/TUN mode, opening `2ip.ru` showed the VPN exit country/IP instead of the user's direct Russian connection.
- Root cause: default RU/direct routing was not part of the generated runtime configs; Workshop rules were empty unless the user explicitly applied a preset, and xray TUN bridge paths only applied process rules.
- Fix: added backend default direct rules for `.ru`, `.su`, `.рф`/punycode, Moscow TLDs, `2ip.ru`, and common Russian service domains across sing-box TUN, xray configs, raw xray injection, and xray-to-sing-box TUN bridge configs. User custom proxy/block/direct rules still take priority over the default list.
- Verification: `cargo test --manifest-path src-tauri/Cargo.toml --lib` passed with split-routing coverage for sing-box, xray, raw xray injection, and TUN bridge defaults.

## 2026-06-29 - App updater - WebView2 V8 out-of-memory on 5.3.1 to 5.4.0 update

- Symptom/command: in-app update from installed `DoodleRay.exe` 5.3.1 showed the WebView2 error page `Out of Memory`; Crashpad dump contained `v8-oom-last-few-messages` with old-space around 2112 MB.
- Root cause: the generated `latest.json` exposed `windows-x86_64-nsis` as the full Windows setup `.exe` instead of the small NSIS updater zip, and the 5.3.1 UI wrote updater progress state through persisted storage for every download chunk.
- Fix: release workflow now patches `latest.json` after all platform builds so `windows-x86_64-nsis` points at the NSIS updater zip; app update progress is throttled, and persisted storage writes are skipped when the serialized persisted state did not change.
- Verification: Crashpad evidence confirmed renderer V8 OOM; local store was only about 68 KB, ruling out user data bloat. `latest.json` patch logic requires `windows-x86_64` to point at `.nsis.zip` before replacing the NSIS target.

## 2026-06-28 - Release 5.4.0 - local updater signing key absent

- Symptom/command: `npm run tauri build` emitted `DoodleRay_5.4.0_x64-setup.exe`, then exited with `A public key has been found, but no private key`.
- Root cause: local release QA machine does not have `TAURI_SIGNING_PRIVATE_KEY`; updater artifact signing is provided by GitHub Actions secrets during tagged release builds.
- Fix: confirmed the Windows release workflow exports `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, and treated the local command exit as a machine-secret limitation only.
- Verification: local NSIS installer exists at `src-tauri/target/release/bundle/nsis/DoodleRay_5.4.0_x64-setup.exe`; SHA-256 `F61DD8543A7B8B1B81EB53FD48E0DD5178ED63C1D5C97119F86F7F80B7DC04CE`.

## 2026-06-28 - Windows installer QA - Tauri updater signing key missing locally

- Symptom/command: `npm run tauri build` produced the NSIS installer, then exited with `A public key has been found, but no private key`.
- Root cause: local QA environment does not have `TAURI_SIGNING_PRIVATE_KEY`; the release workflow is expected to provide updater signing secrets.
- Fix: treated the final updater-artifact signing failure as a local QA caveat only after confirming `DoodleRay_5.3.1_x64-setup.exe` was emitted successfully.
- Verification: rebuilt installer and installed it in Windows Sandbox; app launched, WebView2 was present, service reached `connected`, WinINet proxy was applied, and TUN routes were active.

## 2026-06-28 - Windows update QA - public release installer on clean Sandbox

- Symptom/command: public `v5.3.1` Windows installer returned `exit=2` on a completely clean Windows Sandbox without WebView2.
- Root cause: the old public installer depends on WebView2 being available or installable online; clean Sandbox did not have the prerequisite.
- Fix: validated the real upgrade path by preinstalling WebView2/runtime, uninstalling DoodleRay, installing the old public release, then installing the new local build over it.
- Verification: old release installed after WebView2 preseed, new installer updated binaries, stale WinINet proxy was replaced with a new loopback proxy, and tunnel service reported `connected`.

## 2026-06-28 - Real subscription QA - Node ESM Windows path import

- Symptom/command: temporary Node parser runner failed with `ERR_UNSUPPORTED_ESM_URL_SCHEME` when importing a bundled `.mjs` from an absolute `C:\...` path.
- Root cause: Node ESM import on Windows requires a `file://` URL for absolute paths.
- Fix: converted the generated bundle path with `pathToFileURL(...).href` before dynamic import.
- Verification: parser runner loaded current subscription code and produced a redacted summary with 5 supported VLESS servers.

## 2026-06-28 - Real subscription QA - UTF-8 state mojibake in Sandbox harness

- Symptom/command: seeded subscription names rendered as mojibake in the Sandbox UI after writing `doodleray-storage.store`.
- Root cause: Windows PowerShell read the UTF-8 JSON state without an explicit encoding and decoded it as the legacy ANSI code page.
- Fix: changed the Sandbox harness to read `subscription-state.json` with `[System.Text.Encoding]::UTF8` before writing secure-storage candidates.
- Verification: subsequent state generation and harness runs preserved the redacted parsed summary and avoided corrupting the UTF-8 source state.

## 2026-06-28 - Real subscription QA - Sandbox curl Schannel revocation false negative

- Symptom/command: `curl.exe` HTTPS probes through the local proxy failed with `CRYPT_E_REVOCATION_OFFLINE` even while the tunnel service reported `connected`.
- Root cause: Windows Sandbox Schannel could not reach certificate revocation endpoints reliably during HTTPS probe validation.
- Fix: added `--ssl-no-revoke` to Sandbox curl probes so the QA checks measure VPN connectivity instead of CRL availability.
- Verification: proxy probes returned `http_code=200` for `msftconnecttest`, `204` for `gstatic`, and `200` for `api.telegram.org` through the real subscription.

## 2026-06-28 - Desktop UI QA - release executable locked by running app

- Symptom/command: `cargo build --release --bin DoodleRay` failed to remove `target\release\DoodleRay.exe` with Windows `os error 5`.
- Root cause: a real DoodleRay desktop process was already running from that release path, so Windows locked the executable file.
- Fix: rebuilt the fresh QA executable with `cargo build --release --bin DoodleRay --target-dir target\codex-live` and copied required runtime files into that live target directory.
- Verification: alternate release build completed successfully and `target\codex-live\release\DoodleRay.exe`, `sing-box.exe`, and `xray-core\xray.exe` exist.

## 2026-07-04 - Friend LAN QA - app-side xray orphan after fallback or UI kill

- Symptom/command: on a real dirty Windows 10 desktop, the crash-recovery QA
  pass ended with service disconnected and WinINet disabled, but two
  DoodleRay-owned `xray.exe` processes from `C:\Program Files\DoodleRay` were
  still alive.
- Root cause: app-side proxy/fallback xray processes were tracked by an
  in-process child handle. After UI kill/relaunch or fallback transitions, the
  new UI process no longer had that handle, so normal `xray::stop_xray()`
  could not clean the old DoodleRay-owned children.
- Fix: `vpn_disconnect` and `full_cleanup` now run the owner-aware orphan
  engine cleanup after the normal stop path. The cleanup only terminates
  `xray.exe`/`sing-box.exe` whose executable path belongs to the DoodleRay
  install directory, leaving other VPN clients alone.
- Verification: before the fix, `friend-crash-recovery-20260704-151458`
  captured two orphan `xray.exe` processes. After the fix, the final friend LAN
  pass `friend-lan-evidence-20260704-155232` and the final runtime snapshot
  showed `engines=[]`, `statsquery=[]`, service `disconnected/idle`, WinINet
  `ProxyEnable=0`, no adapter, and no marker.

## 2026-07-04 - Friend LAN QA - protected fake-green after service core crash

- Symptom/command: on the same dirty Windows 10 desktop, killing the
  service-owned core while protected mode was connected left the UI visually
  connected for the slow monitor window even though service health already
  reported `failed` with a fatal `sing-box exited unexpectedly` check.
- Root cause: the frontend normal health monitor was too slow for fatal
  protected-mode transitions. The service already knew the runtime was failed,
  but the dashboard could keep showing `connected` until the next periodic
  monitor cycle.
- Fix: the dashboard now starts a fast protected-mode fatal-health watchdog
  while `proxyMode='tun'` and UI status is connected. It asks
  `get_connection_health` shortly after connect and every few seconds
  thereafter; fatal protected verdicts trigger `vpn_disconnect`, clear the
  connected UI state, and show an error instead of fake-green.
- Verification: the failure was captured in
  `friend-crash-recovery-20260704-153443` (`app_connected=true` while service
  state/effective state/health were failed). After the stale-Wintun ghost fix,
  `friend-crash-recovery-20260704-163555-ghostfix` re-ran the same dirty Win10
  machine in protected mode, killed the service-owned core, and verified the UI
  did not stay fake-green (`front=disconnected`, `app=False`, service
  `disconnected`). The watchdog change passed `npm run build`, `cargo check`,
  and the final friend LAN pass.

## 2026-07-04 - Windows TUN - stale Wintun PnP ghost blocked adapter creation

- Symptom/command: on the dirty Windows 10 LAN desktop, protected mode
  repeatedly failed before the TUN adapter became ready and fell back to
  Browsers compatibility. The user-facing class was
  `DoodleRay Tunnel IPv4 readiness failed: DoodleRay Tunnel adapter is
  missing`.
- Root cause: the support bundle contained the real `sing-box` fatal:
  `configure tun interface: (create adapter: Cannot create a file when that
  file already exists. | open existing adapter: Element not found.)`.
  Read-only PnP inspection then found a non-present Wintun network device:
  `FriendlyName=sing-tun Tunnel`, `InstanceId=SWD\WINTUN\{...}`,
  `Problem=CM_PROB_PHANTOM`, `Status=Unknown`. `Get-NetAdapter` could not see
  it, but Wintun still hit the stale device/name state during adapter creation.
- Fix: the tunnel service now runs a bounded stale-Wintun ghost repair during
  service-owned cleanup/replace. It removes only stale `SWD\WINTUN\*` PnP
  devices that are non-OK/non-present and match DoodleRay/sing-tun ownership
  heuristics, using `pnputil /remove-device` when available. The service also
  checks `sing-box` liveness immediately when waiting for the adapter fails, so
  future bundles preserve the real fatal instead of only saying adapter missing.
  The exact `Cannot create a file ... Element not found` fatal is now classified
  as repairable by unit tests.
- Verification: after installing QA artifact
  `DoodleRay_5.9.0_x64-setup.exe` SHA-256
  `059C0D64F72E0752C889481CBC4B240E10C7450513D4FFC208AF3FE8E5ACA486`,
  the service log showed
  `stale_wintun_ghosts_seen=1 removed=1 failed=0 targets=sing-tun Tunnel|SWD\WINTUN\{...}|problem=CM_PROB_PHANTOM|status=Unknown`,
  then protected TUN connected in `9044 ms` with `adapter=DoodleRay Tunnel`,
  `health_verdict=protected_degraded`, `route_ready=true`, and
  `dns_ready=true`. Final inspection showed no Wintun PnP ghosts and no
  DoodleRay adapters after cleanup.
