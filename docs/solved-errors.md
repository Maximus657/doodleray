# Solved Errors

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
