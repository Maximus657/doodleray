# Solved Errors

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
