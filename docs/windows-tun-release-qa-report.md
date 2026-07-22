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

## 2026-07-03: 5.9.0 RC2 - Bounded TUN Adapter Bring-Up Repair

Production error being closed:
`Full Computer components not installed or not ready: DoodleRay Tunnel IPv4 readiness failed: DoodleRay Tunnel adapter is missing`.
Root cause and fix are documented in `docs/solved-errors.md` (2026-07-03).

### Build Under Test

- Version: `5.9.0` (RC2 of the 5.9.0 line, local unsigned, QA-only).
- NSIS setup SHA-256:
  `9507B7B32E7483471B306FCB9209BC651BFC71F6D2033DEC3F46D8A063DA436D`.
- Bundled service SHA-256:
  `518B9D687C91F2CE5D3475F0682B834D56ABAD01E4792360D71BCF4D6407AB12`.
- Changes: service-side bounded two-attempt TUN bring-up
  (`bring_up_tun_runtime` + one DoodleRay-owned repair retry on repairable
  adapter/IPv4/engine-startup failures, status `repairing` in between),
  shared `is_repairable_tun_bringup_error`/`format_tun_bringup_failure` with
  unit tests, app-side wait-loop guard for the transient repair window plus a
  90s budget, and a runtime-files preflight
  (`wintun.dll`/`sing-box.exe`/`xray.exe`/`DoodleRayService.exe`) that fails
  with a reinstall message before TUN is attempted.

### Local Checks Passed

- `npm run build`: passed.
- `cargo test --manifest-path src-tauri\Cargo.toml --lib`: passed
  (`52 passed`, `3 ignored`; includes the new repairable-classification and
  actionable-failure-message tests).
- `cargo check` for `DoodleRay` and `DoodleRayService`: passed.
- `git diff --check`: passed.

### Stand Evidence Status

The Play2Go stand was mid-reinstall (back to Windows Server 2022) during this
pass: TCP 22 accepts but sshd closes the connection after the client banner,
and the WS-Management endpoints do not answer, so no remote QA could run yet.
Pending on the restored stand (harness already committed):

1. `Invoke-DoodleRayFullStandQa.ps1 -LocalInstaller <5.9.0 RC2 setup> -AllowUnsignedLocalRc`
   for the full regression pass.
2. `Test-DoodleRayTunBringupRepair.ps1` - the targeted proof: kills the
   service-owned sing-box during the Connecting window (reproduces the
   adapter-missing path), asserts the connect still ends
   protected/protected_degraded with the
   `TUN adapter repair retry ran after: ...` warning in structured health, or
   - only as fallback - a clean failure carrying the enriched
   `DoodleRay could not create the Windows tunnel adapter: ...` message, and
   a clean stand afterwards.

## 2026-07-03: 5.9.0 RC4 - Reliability P0 Pass (Server 2022, fresh stand)

### Build Under Test

- Version `5.9.0` RC3 setup SHA-256
  `A74A0C6B8B80E485479C2C57F1DBE5203DE98403E9EA93E98BA17A35DF39A1E8`
  (stand evidence below), superseded by RC4 with the idempotent service
  install fix (hashes in the final report / build log).
- Code added in this pass: honest automatic Protected->Browsers fallback with
  LIMITED messaging (en/ru/zh); child-generation rotation after a failed
  network/power reassert (stored start request, once per event burst);
  cleanup verification -> honest `cleanup_pending` when the owned adapter
  lingers after stop; `summarize_route_policy` config-derived route
  explanations (unit-tested) published into `route_explanations`;
  deterministic subscription profile ids + per-subscription dedupe
  (`stableServerId`, `finalizeSubscriptionServers`); QA-only loopback control
  surface (`DOODLERAY_QA_CONTROL=1`, 127.0.0.1:48765: status/connect/
  disconnect/switch-mode/refresh/import/export-bundle) consumed by the same
  UI handlers users click; idempotent `install_service` (adopt healthy
  registration, never delete+recreate it).

### Local Checks

`npm run build` passed; `cargo test --lib` 54 passed (route-policy tests
included); both `cargo check` passed; `git diff --check` clean; all QA
scripts parse-clean.

### Stand Evidence (fresh Server 2022 VM)

- Install gate with stale WinINet injection: passed on RC3.
- Canonical subscription imported through the new control surface
  (secret travelled local->pscp->stand temp file->loopback query only) and
  verified by a control-driven protected connect probe.
- **Bring-up crash repair proven twice** (`Test-DoodleRayTunBringupRepair`):
  service-owned sing-box killed in phase `waiting_adapter` during connect;
  the service ran the bounded repair and the connect still ended
  `connected`/`protected_degraded` with the
  `TUN adapter repair retry ran after: ...` warning
  (run 1: gen 9; run 2: gen 3, `total_connect_ms=37814`, service.log shows
  `tun bring-up attempt 1 failed; running DoodleRay-owned repair retry:
  DoodleRay Tunnel adapter did not become ready`).
- Stale-state repair (`Test-DoodleRayStaleStateRepair`): all steps passed -
  injected legacy DoodleRay-shaped WinINet (`127.0.0.1:10809` + game bypass)
  cleared by startup repair; injected DoodleRay-commented NRPT rule removed;
  a third-party NRPT control rule **survived** (owned-only repair proven).
- Service install ACL noise is gone: `DoodleRayService.exe install` on the
  stand now reports zero failed files (empty-DACL fix holds).
- Found and fixed on-stand: service registration churn/vanish under repeated
  repair installs (see `docs/solved-errors.md` 2026-07-03); QA teardown also
  hardened (`Stop-QaTunnelHard`: confirmed disconnect with service-restart
  fallback).
- CDP note: the fresh VM's WebView2 no longer exposes remote debugging via
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`; harnesses now run control-first
  (CDP is optional visual smoke), which was the point of the control surface.
- Final stand state after the pass: WinINet `ProxyEnable=0`, no ProxyServer/
  PAC, zero NRPT rules, zero engine processes, zero statsquery orphans, no
  DoodleRay adapter, no session marker.

## 2026-07-03: 5.9.0 RC5 - Protected Auto-Fallback E2E Closed

### Build Under Test

- Version `5.9.0`, local unsigned QA-only NSIS setup SHA-256
  `A75852B1BCB6FB46BA3196AF677231D8F677FD8DE4C0F635F3E65315657977C3`.
- Added in this pass:
  - shared frontend fallback path for both `vpn_connect` result failures and
    thrown/catch failures;
  - failed protected generation is explicitly disconnected before browser
    fallback starts;
  - QA-only frontend snapshot published into `/status` while
    `DOODLERAY_QA_CONTROL=1` is enabled (`subscriptions_count`,
    `servers_count`, mode, status, active-server presence, runtime ports; no
    URLs, UUIDs, keys, or raw configs);
  - QA teardown now treats app-owned browser proxy as active state even when
    the tunnel service is already `disconnected`.

### Local Checks Passed

- `npm run build`: passed.
- `cargo check --manifest-path src-tauri\Cargo.toml --bin DoodleRay`: passed.
- `cargo check --manifest-path src-tauri\Cargo.toml --bin DoodleRayService`:
  passed.
- `Import-DoodleRayQaSubscription.ps1`, `CdpQaHelpers.ps1`, and
  `Test-DoodleRayAutoFallback.ps1` parse-clean.

### Stand Evidence (Play2Go Server 2022)

- Install gate with stale WinINet injection: passed on the RC5 setup.
- Canonical subscription import: passed and verified directly through the new
  sanitized frontend snapshot (`subscriptions_count=1`, `servers_count=5`,
  active server present).
- `Test-DoodleRayAutoFallback.ps1`: passed end-to-end.
  - A real protected start was observed in `service.log`
    (`StartTunnel accepted`, `start_tunnel generation=21`).
  - The UI did not claim protected/green after the protected bring-up failure.
  - The UI automatically degraded to browser compatibility:
    `appConnected=true`, service `disconnected`, WinINet
    `127.0.0.1:59351`.
  - The loopback HTTP proxy fetched
    `https://captive.apple.com/hotspot-detect.html` with `HTTP_CODE=200`.
  - No TUN adapter was claimed during limited fallback.
  - Final cleanup was clean: service `disconnected`, WinINet `0`, engines `0`,
    session marker `false`, adapter `false`.
- `Test-DoodleRayStaleStateRepair.ps1`: passed again on this RC. Legacy
  DoodleRay-shaped WinINet was cleared; DoodleRay NRPT was removed; third-party
  NRPT survived.
- Final remote cleanliness check (run through `Invoke-Play2GoPowerShell.ps1`):
  Server 2022, service `5.9.0` disconnected/idle, WinINet disabled, no
  `xray`/`sing-box`, no DoodleRay adapter, no active-session marker, no
  DoodleRay NRPT.

### Superseded Harness Note

`Test-DoodleRayTunBringupRepair.ps1` is no longer a valid standalone pass/fail
gate after the honest auto-fallback work. The script watches only the service
for `connected` plus a retry warning; the new product behavior can catch an
early protected failure in the UI, disconnect the failed protected generation,
and continue in browser compatibility. In that case the old script reports
`service=disconnected` even though the user-facing fallback path is correct.
Keep the service-side repair unit tests and the historical RC4 evidence, and
replace this harness with a v2 that either disables UI fallback for the service
repair proof or explicitly asserts the new fallback behavior.

## 2026-07-03: 5.9.0 RC6 - Full Stand QA Complete

### Build Under Test

- Version `5.9.0`, local unsigned QA-only NSIS setup SHA-256
  `85A8B3A7A6AF5539FCBA68A38EF87C1CF864F568324C022BBF3898DF7DBCBA22`.
- Tauri build produced the NSIS setup; the final local command still exits on
  updater signing because `TAURI_SIGNING_PRIVATE_KEY` is intentionally absent
  outside CI. This remains a production gate, not a runtime QA failure.
- This RC includes the RC5 protected->browser fallback plus final hardening:
  TUN UI waits now use a 120s budget (longer than the service's bounded
  90s repair window), thrown timeout/error paths share the same honest limited
  fallback lane, Windows disconnect always asks the tunnel service to stop so
  a failed protected generation cannot remain sticky behind browser fallback,
  and stale service failure paths re-check generation before publishing
  `failed` after `failed_cleanup`.

### Full One-Command Stand Run

`Invoke-DoodleRayFullStandQa.ps1 -LocalInstaller <RC6 setup> -AllowUnsignedLocalRc`
completed on the Play2Go Server 2022 stand and reached
`=== FULL STAND QA COMPLETE ===`.

Stages passed in order:

1. `bootstrap-stand`
2. `publish-installer`
3. `install-gate`
4. `unclean-shutdown-marker`
5. `update-path-5.4.3`
6. `update-path-5.4.4`
7. `update-path-5.4.5-broken-state`
8. `import-subscription-before-active-update`
9. `active-vpn-during-update`
10. `import-subscription-before-ui-pass`
11. `rc-ui-cdp-pass`
12. `stale-state-repair`
13. `auto-fallback-protected-to-browsers`
14. `deep-snapshot`

Important evidence from the final pass:

- RC UI pass was control-first and detached from the SSH stream, so TUN route
  changes cannot falsely break the harness transport. It still exercises the
  same UI handlers: refresh, protected connect, mode chain
  Browsers -> Whole -> Browsers -> Manual -> Whole, UI-kill/reattach,
  service-owned core crash, support bundle export, reconnect, and cleanup.
- The previously-open `auto-fallback-protected-to-browsers` blocker is closed
  end-to-end inside the full runner. The harness forced protected bring-up to
  fail, observed a real `StartTunnel accepted`, verified that the UI did not
  claim protected, verified browser compatibility was active through WinINet
  loopback HTTP, fetched Apple's captive endpoint through that proxy with
  `HTTP_CODE=200`, verified no TUN adapter was claimed during limited fallback,
  and left the stand clean.
- Active-VPN-during-update stayed green in the same full run: RC installed over
  an active protected tunnel, service ended cleanly, startup repair cleared the
  orphaned WinINet state, reconnect succeeded, and the final state was clean.
- Update paths from `5.4.3`, `5.4.4`, and broken-state `5.4.5` passed in the
  same full run; the broken-state path includes stale WinINet/corporate-PAC
  preservation coverage.
- Stale-state repair passed: DoodleRay-shaped WinINet and DoodleRay NRPT were
  removed, third-party NRPT survived.
- Deep snapshot completed after the full matrix. WebSocket was OK, DNS/HTTPS
  probes were OK, 2ip/direct split showed the stand IP, and UDP/STUN remained
  a warning-style external probe failure rather than a protected claim.
- Separate final cleanliness check after the full run:
  service process `Running` with tunnel state `disconnected`, WinINet
  `ProxyEnable=0`, no ProxyServer/PAC, `xray`/`sing-box` count `0`,
  `statsquery` orphan count `0`, DoodleRay/Wintun adapter count `0`,
  DoodleRay NRPT count `0`, active-session marker `false`.

### Local Gates After The Full Stand Run

- `npm run build`: passed.
- `cargo check --manifest-path src-tauri\Cargo.toml --bin DoodleRay`: passed.
- `cargo check --manifest-path src-tauri\Cargo.toml --bin DoodleRayService`:
  passed.
- `cargo test --manifest-path src-tauri\Cargo.toml --lib`: passed
  (`54 passed`, `3 ignored` live-smoke tests).
- QA PowerShell parse checks passed for:
  `Invoke-DoodleRayRc3UiCdpPass.ps1`, `Publish-DoodleRayQaInstaller.ps1`,
  `Test-DoodleRayAutoFallback.ps1`, `Invoke-DoodleRayFullStandQa.ps1`,
  `Invoke-DoodleRayActiveUpdateQa.ps1`,
  `Test-DoodleRayTunBringupRepair.ps1`, and
  `Test-DoodleRayStaleStateRepair.ps1`.
- `git diff --check`: exit `0`; only Windows CRLF conversion warnings.

### Remaining Non-Deploy Blockers

These do not invalidate the Server 2022 RC evidence, but they still block any
honest "works for every Windows device" production claim:

1. Signed CI build with real updater key and Authenticode secrets, then run the
   same gates without `-AllowUnsignedLocalRc`.
2. Clean Windows 10 22H2 and Windows 11 24H2 VM evidence.
3. Real hardware sleep/wake and network-change proof.
4. IPv6 leak-proof evidence. Until then IPv6 must stay degraded/disabled when
   not proven.
5. Controlled QUIC/HTTP3 proof. Until then QUIC stays explicitly unclaimed.

## 2026-07-04 - Friend LAN dirty Windows 10 QA

This pass covered a real home desktop instead of the controlled Play2Go stand.
The target was Windows 10 Pro build 19045, running an existing public DoodleRay
5.4.5 install plus a dirty networking environment: Zapret `winws.exe`,
WinDivert, Outline, Happ, Hiddify, TAP/Wintun drivers, and an inactive stale
WinINet `ProxyServer=http://127.0.0.1:10809` with `ProxyEnable=0`.

Final artifact tested on that machine:
`DoodleRay_5.9.0_x64-setup.exe`
SHA-256 `372E1E27EA7C1BEDC91BDB5EC889C488381BA69E252E3D6401F446C9EE190470`
(unsigned, QA-only).

Evidence folders kept locally:

- `D:\DoodleRayPC\friend-lan-evidence-20260704-144534`
- `D:\DoodleRayPC\friend-auto-fallback-local-20260704-151024`
- `D:\DoodleRayPC\friend-crash-recovery-20260704-151458`
- `D:\DoodleRayPC\friend-crash-recovery-20260704-153443`
- `D:\DoodleRayPC\friend-lan-evidence-20260704-155232`
- `D:\DoodleRayPC\friend-lan-evidence-20260704-162403-ghostfix-final`
- `D:\DoodleRayPC\friend-crash-recovery-20260704-163555-ghostfix`

Results:

- Upgrade over the old 5.4.5 install succeeded on the dirty Windows 10 desktop.
- The real subscription imported through the QA control surface and produced
  the expected server list.
- A protected attempt connected once as `protected_degraded` and proved real
  data-plane coverage: Apple captive GET, Google 204, Telegram, Discord,
  OpenAI, Claude, WebSocket, SSE, UDP/STUN, DNS probes, and 2ip/direct split.
- On later attempts the same machine reproduced the user's class of problem:
  protected TUN could fail with `DoodleRay Tunnel IPv4 readiness failed:
  adapter is missing`. The 5.9 auto-fallback then moved to Browsers
  compatibility instead of leaving the user dead.
- Forced double `sing-box` kills during protected bring-up proved the fallback
  end-to-end: no protected claim, WinINet pointed at the service HTTP loopback,
  Apple's captive endpoint returned `HTTP_CODE=200`, no TUN adapter was claimed
  during limited fallback, and teardown was clean.
- Browser/proxy mode worked on the dirty desktop even when direct Telegram was
  blocked (`direct=000`, proxy Telegram `302`). This specifically covers the
  support pattern where proxy mode worked for a user while direct traffic did
  not.
- Manual mode did not mutate WinINet. The inactive legacy `ProxyServer` string
  remained inactive with `ProxyEnable=0`.
- Final cleanliness after the last pass: service `disconnected/idle`, WinINet
  `ProxyEnable=0`, no DoodleRay UI, no `xray.exe`/`sing-box.exe`, no
  `statsquery`, no DoodleRay TUN adapter, no DoodleRay NRPT, and no active
  session marker.
- Follow-up ghost-fix pass installed QA artifact
  `059C0D64F72E0752C889481CBC4B240E10C7450513D4FFC208AF3FE8E5ACA486` on the
  same dirty Windows 10 desktop. The service found and removed a stale Wintun
  PnP ghost (`sing-tun Tunnel`, `SWD\WINTUN\{...}`, `CM_PROB_PHANTOM`), then
  protected TUN connected instead of falling back:
  `service=connected`, `health_verdict=protected_degraded`,
  `adapter_alias=DoodleRay Tunnel`, `route_ready=true`, `dns_ready=true`,
  `proxy_compat_state=ready`, connect time `9044 ms`.
- The ghost-fix TUN deep snapshot passed real traffic probes: Apple captive
  `200`, Google `204`, Telegram `302`, Discord gateway `200`, OpenAI `308`,
  Claude reachable, SSE received bytes, WebSocket OK, UDP/STUN OK, and
  2ip/direct split stayed on the direct local prefix while tunnel traffic used
  the VPN exit prefix.
- After the ghost-fix cleanup, PnP and adapter inspection showed no Wintun
  ghosts and no DoodleRay adapters left behind.
- Follow-up crash-recovery pass after the Wintun ghost fix connected protected
  mode first (`mode=protected`, service `connected`,
  `health_verdict=protected_degraded`), killed the UI, verified service truth
  survived, reattached the UI, killed the service-owned core PID, and verified
  there was no fake-green (`front=disconnected`, `app=False`,
  service `disconnected`). Final cleanup was clean.

Findings fixed during the friend-LAN pass:

- DoodleRay-owned app-side `xray.exe` processes could survive a UI kill or
  fallback cleanup because the new UI process no longer had the original child
  handle. `vpn_disconnect` and `full_cleanup` now also terminate only
  DoodleRay-owned orphan engines from the install directory.
- A protected tunnel could stay visually green for the regular monitor window
  after the service-owned core died. The dashboard now has a fast protected
  fatal-health watchdog that asks the service for health and disconnects on
  fatal protected verdicts instead of waiting for the slow monitor loop.
- Stale Wintun PnP ghosts can block adapter creation while remaining invisible
  to `Get-NetAdapter`. The tunnel service now removes non-present stale
  `SWD\WINTUN\*` ghosts matching DoodleRay/sing-tun ownership heuristics during
  owned cleanup/replace, and it preserves the real `sing-box` fatal in logs.

Remaining friend-LAN caveats:

- This is a dirty Windows 10 real-user test, not a clean Win10/Win11 matrix.
  Clean OS evidence and signed CI evidence are still separate release blockers.

## 2026-07-05 - v6 Store Redesign Transport Sync

Branch: `codex/v6-store-redesign`.

Purpose: keep the new v6 design / Microsoft Store work while importing the
latest 5.9 protected-mode runtime improvements. No production release, tag, or
push was performed.

Synced from the 5.9 transport branch:

- Native Windows IP Helper readiness module (`windows_net.rs`) with adapter,
  IPv4 interface, route-canary, and event-counter probes.
- Service hot path that avoids blocking TUN creation on early xray readiness,
  records xray spawn / sing-box check timings, caches validated sing-box
  configs by hash, and runs IPv6 / QUIC policy warnings asynchronously.
- Safe warm reassert path for identical runtime requests, plus structured
  probe backend/fallback timings in `TunnelStatus` and support diagnostics.
- QA control `/repair-runtime` endpoint and `Invoke-DoodleRayConnectPerfQa.ps1`
  for local / LAN / stand connect-performance measurement.
- Local `.gitignore` guard for dirty-host evidence folders and downloaded
  Sysinternals tools so source commits do not absorb QA artifacts.

Local verification on the v6 worktree:

- `cargo fmt --check` passed.
- `cargo check --bin DoodleRayService` passed.
- `cargo check --bin DoodleRay` passed.
- `cargo test --lib` passed: 65 passed, 3 ignored.
- `npm run build` passed; Vite reported chunk/dynamic-import warnings only.
- `git diff --check` passed.
- `Invoke-DoodleRayConnectPerfQa.ps1` parsed successfully.

Not yet claimed:

- No signed CI / Store package was produced in this sync step.
- No fresh Win10 / Win11 clean-VM run was performed in this sync step.
- No live protected-mode perf run was performed after the v6 sync because the
  friend LAN host was released.

## 2026-07-06 - v6 Store RC Local Gate And Preview Smoke

Branch: `codex/v6-store-redesign`.

Artifact:

- `DoodleRay-store-win32-6.0.0-x64-setup.exe`
- SHA-256 `C4FE94A9D5A012682535B6AF50F0BCFE3C0C37D4C8603D72B02A6BDE1B9CDA2E`
- Unsigned, QA-only. Not Store-submittable and not production-ready.

Fixes in this pass:

- Diagnosis mapping no longer shows service warning/degraded bookkeeping as a
  failed user check when the verdict is otherwise `all_ok` or a non-actionable
  IPv6/QUIC note.
- App API secure session persistence was tightened: access tokens stay in
  memory only; the disk/keyring session stores refresh/device/subscription
  material only and migrates early-RC full-token entries by rewriting them.
- The v6 connect orb no longer renders decorative pulse/glow rings, removing
  the large connected-state color blooms and avoiding stuck ring animation on
  weak WebView2/GPU paths.
- Normal lifecycle events such as `Starting connection...`,
  `Connection active`, and `Disconnecting...` are displayed as neutral status
  events instead of warning/problem styling.
- Store/closed-control onboarding strings are localized in en/ru/zh and the
  web-preview fallback no longer exposes raw `invoke`/Tauri runtime errors.

Local verification:

- `cargo fmt --manifest-path src-tauri\Cargo.toml --check` passed.
- `cargo check --manifest-path src-tauri\Cargo.toml --bin DoodleRay` passed.
- `cargo check --manifest-path src-tauri\Cargo.toml --bin DoodleRayService`
  passed.
- `cargo test --manifest-path src-tauri\Cargo.toml --lib --quiet` passed:
  79 passed, 3 ignored.
- `npm run build -- --logLevel warn` passed; Vite reported chunk/dynamic-import
  warnings only.
- `scripts\build-store.ps1 -AllowUnsigned -OutDir dist-store-qa` passed and
  produced the artifact above.

Rendered preview smoke:

- In-app browser opened `http://127.0.0.1:4173/` against the built `dist`.
- Closed-control onboarding was visible with Russian strings:
  `Добро пожаловать!`, `Введите код входа DoodleVPN, чтобы загрузить локации.`,
  `Код DoodleVPN`, `Войти`.
- Legacy import text/link UI was absent in the store build.
- No raw `invoke`/Tauri runtime error was visible.
- Decorative pulse/glow ring nodes were absent (`rings=0`).
- Browser console returned no warnings/errors for the smoke page.

Blocked external QA:

- Play2Go host `31.77.147.47` was unreachable from this machine during this
  pass. SSH `:22`, RDP `:3389`, and WinRM `:5985` all timed out, so upload,
  clean install, real closed-login, protected connect, diagnostics, split
  routing, DNS/OpenAI, and cleanup QA could not be executed on the stand.

Not yet claimed:

- No signed CI / Authenticode release artifact.
- No Play2Go install/connect evidence for this exact RC due stand timeout.
- No fresh clean Win10 / Win11 matrix for this exact RC.
- No live closed-control backend login/connect proof for this exact RC.
