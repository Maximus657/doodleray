# Solved Errors

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
