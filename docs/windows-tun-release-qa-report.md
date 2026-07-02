# Windows Full Computer / TUN Release QA Report

Date: 2026-06-02

## Scope

- Windows Full Computer / TUN service path.
- Installer-owned `DoodleRayTunnelService`.
- Connect/disconnect/update/shutdown cleanup.
- Privileged IPC/security review for the current working-tree patch.
- Real subscription QA must use the canonical DoodleVPN test subscription from
  `docs/qa-test-subscription.md`, with the raw URL kept only in the ignored
  `secrets/doodlevpn-test-subscription-url.txt` file.

## Automated Checks

- `cargo build --release --manifest-path src-tauri\Cargo.toml`: passed.
- `npm run build`: passed.
- `cargo test --manifest-path src-tauri\Cargo.toml`: passed, 6/6 tests.
- `npm run tauri build -- --bundles nsis`: produced the NSIS setup exe.
- Local NSIS command exit status: non-zero only because `TAURI_SIGNING_PRIVATE_KEY` is not present for updater artifact signing.

## Generated Installer

- Path: `src-tauri\target\release\bundle\nsis\DoodleRay_5.1.4_x64-setup.exe`
- Built after Windows service, IPC, runtime ACL, and cleanup fixes.

## Security Scan

- Codex Security diff scan completed for the Windows TUN/service working-tree patch.
- Final markdown report: `%TEMP%\codex-security-scans\DoodleRay PC\localpatch_20260602150515\report.md`
- Final HTML report: `%TEMP%\codex-security-scans\DoodleRay PC\localpatch_20260602150515\report.html`
- Surviving reportable findings: none.

## Fixed During QA

- Runtime config secret exposure risk:
  - `C:\ProgramData\DoodleRay` and `runtime` now get explicit ACL lockdown.
  - ACL inheritance is removed.
  - SYSTEM and Administrators get full control.
  - Authenticated Users and normal Users grants are removed.
- Blind process kill risk:
  - Managed-port cleanup no longer kills arbitrary PIDs.
  - Windows cleanup verifies that the PID executable is DoodleRay-owned and under the current app directory before invoking `taskkill`.
- Unreachable legacy cleanup noise:
  - Windows `stop_tun()` and `stop_tun_for_update()` no longer contain unreachable `taskkill`/`runas` cleanup bodies.

## Current Installed-Machine Status

- The current shell is non-admin.
- The new NSIS installer was not applied locally because it requires UAC.
- The currently installed service still shows old SCM settings:
  - `SERVICE_SID_TYPE: NONE`
  - failure recovery reset period `0`
- This proves the local installed service has not yet been upgraded by the new installer.

## Release Blocker

Before production release, one UAC-approved installed-app test is still required:

1. Run `DoodleRay_5.1.4_x64-setup.exe`.
2. Verify `DoodleRayTunnelService` is auto-installed and running as LocalSystem.
3. Verify `sc qsidtype DoodleRayTunnelService` is not `NONE`.
4. Verify `sc qfailure DoodleRayTunnelService` contains restart recovery actions.
5. Verify `C:\ProgramData\DoodleRay` and `C:\ProgramData\DoodleRay\runtime` are not readable by normal Users/Authenticated Users.
6. Start DoodleRay from Start Menu or Program Files, not `target/release`.
7. Verify the app does not open `127.0.0.1`.
8. Verify the main Dashboard does not show `Install Tunnel Service`.
9. Run five Full Computer connect/disconnect cycles with zero per-connect UAC prompts.
10. Verify no visible `taskkill`/OK dialogs on quit/shutdown/update preparation.

## Release Decision

Code and automated QA are ready for the final installed-app gate.

Do not publish to production until the UAC-approved installed-app test above passes on Windows.

## 2026-07-01 Protected Mode RC Evidence

Test target: Play2Go Windows Server QA stand from `docs/windows-pc-qa-play2go.md`.
Raw server credentials and the DoodleVPN test subscription URL are intentionally
kept only in ignored files under `secrets/`.

### Build Under Test

- Version: `5.4.6`.
- Packaged app hash: `6C8CB798DDB9EBD0D480B5114B61F545E45EA36FEC6CFC01A4D228963CC7D0CF`.
- Service hash: `CF69A5AFB31899A7210C6046C2D232F3CA6D784132DC9189AD2E7ABF2EABF309`.
- NSIS setup hash: `FF74640AAA0FFD7E54FD5D6739C7EBB7219B65DA626FEF75EBE77CA258EFA476`.
- Local `npm run tauri build -- --bundles nsis` produced the setup exe and
  updater zip, then exited non-zero only because `TAURI_SIGNING_PRIVATE_KEY`
  is not available in the local Codex environment.

### Checks Passed

- `npm run build`: passed.
- `cargo check --manifest-path src-tauri\Cargo.toml --bin DoodleRay`: passed.
- `cargo check --manifest-path src-tauri\Cargo.toml --bin DoodleRayService`: passed.
- `cargo test --manifest-path src-tauri\Cargo.toml --lib`: passed
  (`45 passed`, `3 ignored`).
- Packaged app launched from `C:\Program Files\DoodleRay\DoodleRay.exe`.
- Full Computer / TUN connected on the QA server with service status:
  `state=connected`, runtime SOCKS/HTTP ports present, adapter alias present,
  `route_ready=true`, no fatal/degraded service checks.
- Fast UI health returned `protected` in 306 ms, then 275 ms on a repeat check.
- Full diagnostic health returned `protected` in 26 s after DNS probe timeout
  hardening.
- UI stayed connected with no new `Protected-mode health quorum is unstable`
  entry after multiple heartbeat intervals.
- External QA snapshot passed:
  - WinINet proxy pointed at the runtime HTTP port.
  - Loopback SOCKS/HTTP/API listeners were present.
  - DNS resolved through the TUN path.
  - Apple captive GET returned `200`.
  - Telegram HTTP probe returned success.
  - Discord gateway probe returned success.
  - Direct 2ip check stayed on the QA server address family, confirming the
    tested split-routing/direct behavior.
- Disconnect cleanup passed: WinINet disabled, service disconnected, and
  `xray.exe`/`sing-box.exe` were not left running.
- Browser compatibility/proxy mode also connected and passed Apple/Telegram
  HTTP probes through the local HTTP proxy, then cleaned up WinINet on
  disconnect.

### Bugs Found And Fixed In This Pass

- `get_connection_health` was doing full PowerShell/DNS/curl diagnostics for
  TUN while the UI called it every 30 s. The full path can take 25-60 s, so
  health checks could overlap and create false `unstable` UI events while the
  data plane was healthy.
- The UI heartbeat now uses a fast, service-authoritative TUN health path.
  Full route/DNS/HTTPS probes are kept behind `get_connection_health_full` and
  support diagnostics.
- The UI health monitor now has an in-flight guard and timeout, so one slow
  health request cannot stack another one on top of it.
- Full DNS diagnostics now use bounded DNS commands
  (`Resolve-DnsName -QuickTimeout` and `nslookup -timeout=5`) to avoid false
  DNS timeouts in support/release diagnostics.

### QA Lessons

- Do not deploy `cargo build --release --bin DoodleRay` as a Tauri app QA
  artifact. That binary can behave like a dev build and attempt to load
  `127.0.0.1:1420`. Use `npm run tauri build -- --bundles nsis` and test the
  packaged `target\release\DoodleRay.exe` or, preferably, the NSIS-installed
  app from `C:\Program Files\DoodleRay`.
- A single SSH/PowerShell snapshot timed out while TUN was connected, but
  service status, CDP UI, fast health, and a repeated snapshot succeeded. Treat
  remote-server VPN QA timeouts carefully: they may be QA-transport noise, not
  app data-plane failure.

### Remaining Non-Blocking Debt

- `DoodleRayService.exe install` on the reused QA server still prints
  `Access is denied` for old runtime log/config files under
  `C:\ProgramData\DoodleRay\runtime`. The NSIS hook runs service install as
  best-effort and the service starts, but the ACL cleanup noise should be
  hardened before it becomes a support concern.

## 2026-07-02 Protected Mode Crash-Recovery Evidence

Test target: Play2Go Windows Server QA stand from `docs/windows-pc-qa-play2go.md`.
The canonical DoodleVPN test subscription was already imported on the stand;
the raw URL remains only in ignored `secrets/`.

### Build Under Test

- Version: `5.4.6`.
- NSIS setup hash:
  `8C87698662468849A5469CA4AD07A11E7612CEDA57A8CD1CB03499DA316741AF`.
- Installed service hash:
  `D4FE60A7941FCB900E177B0E9F10C1D4121B0C447FA8B018A1BB1D4CE0FB7212`.
- Local `npm run tauri build -- --bundles nsis` produced the setup exe and
  updater zip, then exited non-zero only because `TAURI_SIGNING_PRIVATE_KEY`
  is unavailable in the local Codex environment.

### Checks Passed

- `npm run build`: passed.
- `cargo check --manifest-path src-tauri\Cargo.toml --bin DoodleRay`: passed.
- `cargo check --manifest-path src-tauri\Cargo.toml --bin DoodleRayService`:
  passed.
- `cargo test --manifest-path src-tauri\Cargo.toml --lib`: passed
  (`45 passed`, `3 ignored`).
- NSIS silent install on the Play2Go stand exited `0`.
- Packaged app launched from `C:\Program Files\DoodleRay\DoodleRay.exe` in an
  interactive Windows session with WebView2 CDP attached for UI QA.
- Full Computer / TUN connected and fast health returned `protected` with
  structured runtime SOCKS/HTTP ports from service state.
- Full health returned `protected`: service connected, adapter snapshot ok,
  route snapshot ok, DNS ok, Apple captive GET ok, WinINet proxy ok.
- Deep QA snapshot passed WebView2, VC++ runtime, service recovery settings,
  WinINet, WinHTTP, NCSI, route, DNS, WebSocket, SSE, UDP/STUN, Telegram,
  Discord, OpenAI, Claude, and split-route probes.
- Controlled fault injection killed the service-owned `sing-box.exe`.
- The UI no longer kept a green connected state. It logged:
  `Whole computer mode stopped: ... Tunnel service: state=Failed ...`,
  switched back to `CONNECT`, and ran best-effort cleanup.
- After the crash cleanup, service status was `disconnected`, WinINet proxy was
  disabled, and `xray.exe`/`sing-box.exe` were not left running.
- Reconnect after the fault injection succeeded without reboot; full health
  again returned `protected`.
- Final cleanup left service `disconnected`, WinINet disabled, and no engine
  child processes running.

### Bugs Found And Fixed In This Pass

- The UI monitor treated fatal protected-mode health the same as soft unstable
  health, so a killed `sing-box.exe` could leave the interface showing
  `CONNECTED` while the data plane was gone.
- The service status snapshot kept stale runtime ports, adapter alias, and
  route readiness after a child process failure.
- Fatal protected health is now handled separately: the UI logs an error,
  switches to disconnected, and calls `vpn_disconnect` for cleanup.
- Service runtime failure now clears runtime ports, adapter info, route/DNS
  readiness, proxy compatibility state, and child handles before reporting
  status.

### Remaining Release Blockers

- Production release must sign `DoodleRay.exe`, `DoodleRayService.exe`,
  `sing-box.exe`, `xray.exe`, and the installer. The local Codex build remains
  unsigned except for WireGuard's signed `wintun.dll`.
- IPv6 remains a product gap: the Windows TUN route table advertises IPv6
  coverage, while Windows DNS stability currently uses IPv4-only behavior.
  Before claiming universal IPv6 support, either implement full IPv6 DNS/egress
  or mark IPv6 unavailable/degraded explicitly.

## 2026-07-02 Protected Mode Runtime-Truth RC Evidence

Test target: the dedicated Play2Go Windows Server 2022 QA stand. The canonical
DoodleVPN test subscription was already present; raw subscription credentials
remain only in ignored `secrets/`.

### Build Under Test

- Version: `5.4.6`.
- NSIS setup path on the QA stand:
  `C:\DoodleRayQA\DoodleRay_5.4.6_x64-setup-codex-runtime-truth.exe`.
- NSIS setup SHA-256:
  `4D51122720E1E44600D3DCE359E4965D2E77F69DA4401A340ADDAAF1F6B9EBE8`.
- Installed app SHA-256:
  `05F3802238FF3646A820CAF02B0B512B9B7D47D856B58ED229D4002A2584FFDF`.
- Installed service SHA-256:
  `0B88B8D5416460B763E63CD5142AAB4C2141CA1002789B9B7098053A1B89169B`.
- Local `npm run tauri build -- --bundles nsis` produced the NSIS installer
  and updater zip, then exited non-zero only because the local Codex environment
  does not have `TAURI_SIGNING_PRIVATE_KEY`.

### Checks Passed

- `npm run build`: passed.
- `cargo check --manifest-path src-tauri\Cargo.toml --bin DoodleRay`: passed.
- `cargo check --manifest-path src-tauri\Cargo.toml --bin DoodleRayService`:
  passed.
- `cargo test --manifest-path src-tauri\Cargo.toml --lib`: passed
  (`45 passed`, `3 ignored`).
- `git diff --check`: passed.
- Upload to Play2Go preserved the setup hash.
- NSIS silent install exited `0`; service version was `5.4.6` and started in
  `disconnected` state.
- UI launched from `C:\Program Files\DoodleRay\DoodleRay.exe` through WebView2
  CDP; the installed app showed `v5.4.6`.
- First `Whole Computer` connect succeeded without reboot:
  service state `connected`, generation `3`, runtime ports
  `SOCKS=50665`, `HTTP=50666`, `API=50667`, `route_ready=true`,
  and empty fatal/degraded checks.
- Fast and full health both returned `protected`. Full health verified service
  status, adapter, route snapshot, DNS, Apple captive GET
  `https://captive.apple.com/hotspot-detect.html`, and WinINet proxy.
- Deep QA snapshot passed WebView2, VC++ runtime, service recovery settings,
  signatures inventory, WinINet, WinHTTP, NCSI, route, DNS, WebSocket, SSE,
  UDP/STUN, Telegram, Discord, OpenAI, Claude, and split-route probes. `2ip`
  observed the Play2Go server prefix for the direct-route probe.
- No long-lived `xray.exe api statsquery` helper process was present
  (`statsqueryCount=0`).
- UI crash/reload test:
  - killed the main `DoodleRay.exe`;
  - service-owned `xray.exe` and `sing-box.exe` stayed alive;
  - proxy guardian cleared WinINet while the UI was dead;
  - restarted UI through the CDP scheduled task;
  - UI reasserted WinINet to the live service HTTP port `127.0.0.1:50666`;
  - service remained `connected`, generation `3`;
  - UI logged `Browser compatibility repaired after UI reload`,
    `VPN is still active (reconnected after UI reload)`, and
    `Startup repair: preserved active Tunnel Service state`;
  - fast and full health again returned `protected`;
  - `statsqueryCount` stayed `0`.
- Disconnect cleanup after the reload test left service `disconnected`,
  WinINet disabled, no `ProxyServer`, no `xray.exe`, no `sing-box.exe`, and
  `statsqueryCount=0`.
- Second clean reconnect after cleanup succeeded without reboot:
  service generation `5`, runtime ports `SOCKS=59807`, `HTTP=59808`,
  `API=59809`, health `protected`, and `statsqueryCount=0`.
- Final disconnect left the QA stand clean: service `disconnected`, WinINet
  disabled, and no engine child processes.

### Bugs Found And Fixed In This Pass

- The service IPC `StartTunnelRequest` did not carry `api_port`, so the service
  could not publish the xray API port as structured runtime truth after UI
  reload. It now carries and reports `runtime_api_port`.
- UI reload reasserted WinINet compatibility only when the first health check
  already contained a WinINet warning. It now reasserts active protected-mode
  compatibility whenever `Whole Computer` plus system compatibility proxy is
  active.
- `get_traffic_stats` could fall through to `xray api statsquery` when the app
  backend had connected state but no active engine string after service reattach.
  Empty engine now returns zero stats instead of spawning a stale helper.
- System-proxy mode could leave a started in-process core and connected backend
  state if applying WinINet failed. The error path now stops the core and resets
  backend connection state.

### Still Not Proven By This Stand

- This evidence is Windows Server 2022 only. It does not replace clean
  Windows 10/11 VM coverage, standard-user coverage, corporate PAC/autodetect,
  sleep/wake, network-change, captive/LTE, or overlapping-LAN testing.
- The local RC is unsigned except for WireGuard's `wintun.dll`. Production
  release must be built by CI with signing secrets and must validate
  Authenticode signatures for the app, service, cores, installer, and updater.
- QUIC/HTTP3 remains unverified on this stand because the system `curl` lacks
  HTTP/3 support.
- IPv6 is not a universal-support claim yet; DNS stability is intentionally
  IPv4-stable while route tables can expose IPv6 state.

## 2026-07-02: v6.0.0 Service-Authoritative Install Smoke

- Local unsigned NSIS RC rebuilt after the service-authoritative runtime
  changes.
- RC uploaded to the Play2Go stand as
  `C:\DoodleRayQA\artifacts\DoodleRay_6.0.0_x64-setup-codex-v6.exe`.
- Installer SHA-256:
  `059341E2FFF1635E7E1ABC74B58B3F9CA545C7406D1E07836A6CB83DB97C6AAC`.
- `Invoke-DoodleRayV6QaGate.ps1 -InjectStaleWinInet -AllowUnsignedLocalRc`
  passed. `-AllowUnsignedLocalRc` was used only because this was a local RC,
  not a production signed CI artifact.
- Installed service reported version `6.0.0`, state `disconnected`,
  effective state `idle`, health verdict `failed` while idle.
- Final server cleanup verified `WinINet ProxyEnable=0`, no proxy server value,
  and `xray api statsquery` orphan count `0`.
- Deep baseline snapshot after install showed WebView2 present, VC++ x64
  runtime present, service recovery configured, NRPT count `0`, WinHTTP direct,
  WebSocket probe open, and UDP/STUN probe ok.
- Signatures for `DoodleRay.exe`, `DoodleRayService.exe`, `sing-box.exe`, and
  `xray.exe` were `NotSigned` because this was a local RC. Production remains
  blocked until signed CI artifacts pass the default v6 gate without
  `-AllowUnsignedLocalRc`.

## 2026-07-02: v6.0.0 RC3 Runtime-Honesty + ACL Fix + Update-Path Pass

### Build Under Test

- Version: `6.0.0` (RC3, local unsigned, QA-only).
- Changes since the previous v6 smoke:
  - service-side unclean-shutdown session marker (`active-session.marker`
    under the locked runtime dir, published as `previous_unclean_shutdown` in
    `TunnelStatus` and as an `unclean shutdown marker:` warning in health);
  - explicit structured QUIC non-claim warning on every protected
    connect/repair;
  - dedup guards for IPv6/network-event checks so long-lived sessions cannot
    grow unbounded status arrays;
  - runtime-dir ACL hardening fix: directory-only `/inheritance:r` +
    `(OI)(CI)` grants plus child `/reset /T`, replacing the old `/T` variant
    that left every runtime file with an empty DACL unreadable even by
    LocalSystem (see `docs/solved-errors.md` 2026-07-02 entry);
  - `quicProbe` verdict in the deep QA snapshot;
  - new `Invoke-DoodleRayUpdatePathQa.ps1` and
    `Test-DoodleRayUncleanShutdownMarker.ps1` harnesses.
- NSIS setup SHA-256:
  `FA662AD97FA2D534A2760A21A1ACF9BCFE763AA0AA69E01E5965696CE9E4C740`.
- Bundled service binary SHA-256:
  `F663D95DCF5B3861AF588088892AC266ED65F1BC494CE6C164FEC311EC20F845`.
- An intermediate RC2 (setup `3CF52B1F...`, without the ACL fix) was used only
  to discover the empty-DACL bug on the stand and was superseded by RC3.
- Local `npm run tauri build -- --bundles nsis` produced the setup exe and
  updater zip, then exited non-zero only because `TAURI_SIGNING_PRIVATE_KEY`
  is unavailable locally (acceptable for a QA-only RC per the release gate).

### Local Checks Passed

- `npm run build`: passed.
- `cargo test --manifest-path src-tauri\Cargo.toml --lib`: passed
  (`49 passed`, `3 ignored`; includes new session-marker roundtrip/garbage
  tests and the unclean-shutdown health propagation assertion).
- `cargo check` for `DoodleRay` and `DoodleRayService`: passed.
- `git diff --check`: passed.
- PowerShell parser validation for the modified/new QA scripts: passed.

### Play2Go Stand Evidence (Windows Server 2022, build 20348)

- RC3 upload preserved SHA-256 `FA662AD9...`.
- `Invoke-DoodleRayV6QaGate.ps1 -InjectStaleWinInet -AllowUnsignedLocalRc`
  passed: silent install exit 0, service registered and running, service
  status JSON parsed, no `xray api statsquery` orphans. Signatures reported
  `NotSigned` as expected for a local RC (production requires the signed gate
  without the local-RC switch).
- Unclean-shutdown marker QA passed on RC3
  (`Test-DoodleRayUncleanShutdownMarker.ps1`):
  - synthetic `active-session.marker` planted, service hard-killed with
    `Stop-Process -Force`;
  - SCM failure-recovery restarted the service automatically;
  - the restarted service published
    `previous_unclean_shutdown="previous session ended uncleanly: op_id=... generation=99 ..."`
    and consumed the marker file;
  - a subsequent clean `Restart-Service` published no marker (clean SCM stop
    runs owned cleanup and clears the marker by design).
- The same run initially failed on RC2 and exposed the empty-DACL runtime
  hardening bug; after the RC3 fix, `C:\ProgramData\DoodleRay\service.log`
  is readable again by SYSTEM and elevated admins, and the log contains
  `unclean shutdown marker consumed`.
- Deep QA snapshot on RC3: WebView2 present, VC++ x64 runtime present,
  WinINet clean (`proxyEnable=0`, empty server/override), and the new
  `quicProbe` honestly reported
  `unverified-no-tooling: system curl does not support HTTP/3; QUIC coverage is not claimed`.
- Update-path harness passed for all three supported source versions
  (`Invoke-DoodleRayUpdatePathQa.ps1 ... -AllowUnsignedLocalRc`):
  - `5.4.3 -> 6.0.0`: public installer downloaded from GitHub Releases on the
    stand, silent install, RC installed over it, updated service reported
    `6.0.0` as JSON, no statsquery orphans;
  - `5.4.4 -> 6.0.0`: same result;
  - `5.4.5 -> 6.0.0` with `-InjectStaleWinInet -InjectCorporatePac`: same
    result, and the synthetic corporate `AutoConfigURL` survived the update
    untouched (the harness fails otherwise), then the harness cleaned its own
    injected state.
  - Evidence JSON: `C:\DoodleRayQA\evidence\update-{before,old-installed,after}-5.4.{3,4,5}.json`.
- Final stand cleanliness verified after the full pass: WinINet
  `ProxyEnable=0` with no `ProxyServer`/`AutoConfigURL`, service `6.0.0`
  `disconnected` with no `previous_unclean_shutdown`, zero `xray`/`sing-box`
  processes, zero `statsquery` orphans, no `DoodleRay Tunnel` adapter, zero
  DoodleRay NRPT rules, and no leftover session marker file.

### Not Covered By This Scripted Pass

- UI connect/disconnect, subscription import, protected-mode data-plane
  probes, mode switches, and crash-recovery UI behavior were not re-run in
  this pass; the 2026-07-02 runtime-truth evidence above covers them for the
  earlier v6 RC. The RC3 changes are service-side and additive, but a CDP/UI
  pass on RC3 is still required before any release candidate is promoted.
- Active-VPN-during-update remains a manual CDP/UI scenario.
- Windows 10 22H2 and Windows 11 23H2/24H2 clean-VM coverage still does not
  exist; this pass is Windows Server 2022 only.

## 2026-07-02: 5.9.0 RC (ex-6.0.0 line) Automated UI + Active-Update Pass

Product decision: this release line ships as `5.9.0` (not 6.0.0 yet). The
service-authoritative protected-mode architecture is unchanged; only the
version was renumbered.

### Build Under Test

- Version: `5.9.0` (local unsigned RC, QA-only).
- Changes since RC3 (`FA662AD9...`): support-bundle `Signature Status` section
  fixed to resolve the app directory from Rust instead of
  `(Get-Process -Id $PID).Path` (which pointed at powershell.exe and produced
  an empty/missing section); version renumbered 6.0.0 -> 5.9.0; new committed
  harnesses `Invoke-DoodleRayRc3UiCdpPass.ps1`,
  `Invoke-DoodleRayActiveUpdateQa.ps1`, `CdpQaHelpers.ps1`,
  `Invoke-DoodleRayFullStandQa.ps1`.
- NSIS setup SHA-256:
  `B1CE0BD465E3714B43E6DD12AFE85295E9594CB6235E617457BE3E4F0C1211CA`.
- Bundled service SHA-256:
  `21E60BE97C026B2CD3A54403C7448EECF5CC86B60A87278FC6CA6E3DF5308B8E`.
- Local checks: `npm run build` passed, `cargo test --lib` 49 passed, both
  `cargo check` passed; `npm run tauri build -- --bundles nsis` produced the
  setup and failed only at updater signing (no local key), as allowed for RC.

### RC3 UI CDP Pass Evidence (precursor run on setup `FA662AD9...`)

The first fully automated UI pass ran against RC3 via the new CDP harness
(WebView2 CDP port 9333, structure-based selectors): 19 of 20 steps passed on
the Play2Go stand, including: installed-app launch, v6.0.0 version display,
subscription refresh, Whole Computer connect
(`verdict=protected_degraded`, structured ports, WinINet at the service HTTP
port, QUIC non-claim warning visible in health), full mode-switch chain
Proxy -> Whole -> Proxy -> Manual -> Whole (manual mode left WinINet
untouched), UI kill with service survival (same generation) and reattach with
WinINet reassertion, service-owned sing-box crash with honest un-green UI and
full cleanup, reconnect (new generation), and clean final state. The single
failure was the support bundle's empty `Signature Status` section - root
cause above, fixed in 5.9.0.

### Active-VPN-During-Update Evidence (5.9.0, setup `B1CE0BD4...`)

`Invoke-DoodleRayActiveUpdateQa.ps1 -AllowUnsignedLocalRc` passed 11/11 steps
on the Play2Go stand:

- protected tunnel genuinely active before update
  (`state=connected verdict=protected_degraded gen=15 winInet=1`);
- silent RC install over the active install exited `0` (NSIS pre-install SCM
  stop ran DoodleRay-owned cleanup);
- post-update service reported `5.9.0`, `disconnected`, zero engine children,
  zero statsquery orphans, no leftover session marker, and no false
  `previous_unclean_shutdown` flag (the SCM stop is a clean shutdown);
- the killed UI left a stale loopback WinINet proxy (recorded honestly), and
  the app relaunch's startup repair cleared it (`proxyEnable=0`);
- reconnect on the updated build returned `protected_degraded`
  (`gen=3, socks=51091, http=51092`);
- final state clean: service disconnected, WinINet off, no adapter/NRPT/
  engines/marker.

### Full UI CDP Pass Evidence (5.9.0, setup `B1CE0BD4...`)

`Invoke-DoodleRayRc3UiCdpPass.ps1` passed 20/20 steps on the Play2Go stand,
re-proving everything from the RC3 pass on the renumbered build, now with the
UI/service version consistency check (`ui=v5.9.0 service=5.9.0`), Whole
Computer connect (`verdict=protected_degraded gen=9 socks=54081 http=54082
api=54083`), and the fixed support bundle: failure marker, Tunnel Service
snapshot, signer status/thumbprint lines, network summaries, no unredacted
URLs, no unredacted UUIDs (24367 bytes). Final state clean. Evidence JSON on
the stand: `C:\DoodleRayQA\evidence\rc3-ui\rc3-ui-pass-summary.json`,
`C:\DoodleRayQA\evidence\active-update\active-update-summary.json`.

### Remaining Blockers (unchanged in kind)

1. Signed CI artifacts + the default signed gate without
   `-AllowUnsignedLocalRc`.
2. Clean Windows 10 22H2 / Windows 11 23H2/24H2 VM evidence
   (`docs/windows-vm-qa-matrix.md` has the exact plan and the one-command
   runner `Invoke-DoodleRayFullStandQa.ps1`).
3. Data-plane breadth on consumer networks (sleep/wake on real hardware,
   IPv6 leak proof, QUIC probe still `unverified-no-tooling`).
