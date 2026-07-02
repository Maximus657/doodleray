# Windows PC QA Play2Go Stand

This repo has a dedicated Play2Go Windows QA VPS for DoodleRay PC testing.
Private access details are stored only in `secrets/doodlevpn-server-access.md`
under `windows-pc-qa-play2go`; never copy host credentials into committed docs,
logs, evidence, screenshots, or support bundles.

The canonical DoodleVPN test subscription URL is stored in the ignored
`secrets/doodlevpn-test-subscription-url.txt` file. Use that subscription for
subscription import, proxy, protected/TUN, split-routing, and speed QA. Do not
copy the raw URL into committed docs, screenshots, logs, support bundles, or
release notes. The durable rule lives in `docs/qa-test-subscription.md`.

## Purpose

Use this stand before promoting Windows releases that touch installer,
updater, subscription import, proxy mode, protected/TUN mode, system proxy,
WebView2, service lifecycle, routing, DNS, diagnostics, or support bundle code.

## Remote Command Access

Use the checked-in SSH wrapper for scripted diagnostics and smoke tests:

```powershell
.\scripts\windows-qa\Invoke-Play2GoPowerShell.ps1 -Command "hostname"
.\scripts\windows-qa\Invoke-Play2GoPowerShell.ps1 -ScriptPath .\scripts\windows-qa\Some-Qa-Script.ps1
```

The wrapper reads private connection fields from the ignored
`secrets/doodlevpn-server-access.md` file. It requires `host`, `login_user`,
`login_password`, and either `ssh_hostkey` in that secret file or
`DOODLERAY_PLAY2GO_HOSTKEY` in the environment. Short commands are executed via
PowerShell `EncodedCommand`; larger script files are uploaded to
`C:\DoodleRayQA\codex-run` with `pscp` and then executed.

## v6 RC Harness

For v6 Windows protected-mode RCs, upload and gate the installer before manual
UI testing:

```powershell
.\scripts\windows-qa\Publish-DoodleRayQaInstaller.ps1 -LocalInstaller .\src-tauri\target\release\bundle\nsis\DoodleRay_5.9.0_x64-setup.exe
.\scripts\windows-qa\Invoke-DoodleRayV6QaGate.ps1 -InjectStaleWinInet
.\scripts\windows-qa\Invoke-Play2GoPowerShell.ps1 -ScriptPath .\scripts\windows-qa\Get-DoodleRayDeepQaSnapshot.ps1
```

The v6 gate verifies silent install, service registration, Authenticode status
for installed app/service/core binaries, service JSON status, and absence of
`xray api statsquery` orphans. Use `-UninstallAfter` only on disposable
installer-cleanup passes.

For the previous-version update path, run the dedicated update harness for each
supported source version:

```powershell
.\scripts\windows-qa\Invoke-DoodleRayUpdatePathQa.ps1 -FromVersion 5.4.3 -InjectStaleWinInet -InjectCorporatePac -AllowUnsignedLocalRc
.\scripts\windows-qa\Invoke-DoodleRayUpdatePathQa.ps1 -FromVersion 5.4.4 -AllowUnsignedLocalRc
.\scripts\windows-qa\Invoke-DoodleRayUpdatePathQa.ps1 -FromVersion 5.4.5 -AllowUnsignedLocalRc
```

The update harness downloads the public installer for the source version from
GitHub Releases onto the stand, installs it silently, optionally injects stale
loopback WinINet proxy state and a synthetic corporate `AutoConfigURL`, then
installs the RC over it. It fails if the updated service does not report the
expected RC version as JSON, if an `xray api statsquery` orphan appears, or if
the injected corporate PAC URL does not survive the update (DoodleRay may only
clean DoodleRay-owned loopback proxy state, never corporate config). It cleans
its own injected state afterwards. Active-VPN-during-update remains a manual
CDP/UI scenario on top of this harness.

For the v6 unclean-shutdown marker, run the crash-simulation harness:

```powershell
.\scripts\windows-qa\Test-DoodleRayUncleanShutdownMarker.ps1
```

It plants a synthetic `active-session.marker`, hard-kills the service process
(an SCM `Restart-Service` must not be used: a clean stop runs owned cleanup and
correctly clears the marker), waits for SCM failure-recovery to restart the
service, and asserts that the restarted service publishes
`previous_unclean_shutdown` in status JSON, consumes the marker file, and
reports nothing after a subsequent clean restart.

For the installed-app UI scenarios, run the CDP pass (drives the real
installed `DoodleRay.exe` through the `DoodleRayCodexCDP` scheduled task and
WebView2 CDP on port 9333; selectors are structure/ASCII-based, so the stand
UI language does not matter):

```powershell
.\scripts\windows-qa\Invoke-DoodleRayRc3UiCdpPass.ps1
.\scripts\windows-qa\Invoke-DoodleRayActiveUpdateQa.ps1 -AllowUnsignedLocalRc
```

The UI pass covers: launch from `C:\Program Files\DoodleRay`, version check,
subscription refresh, Whole Computer connect with service-verdict/WinINet/
runtime-port asserts, the mode-switch chain Proxy -> Whole -> Proxy -> Manual
-> Whole, UI kill + reattach with WinINet reassertion, service-owned core
crash honesty + cleanup, redacted support-bundle export, reconnect, and final
stand cleanliness. The active-update harness connects Whole Computer, runs the
RC installer over the active install, and asserts post-update cleanup, startup
repair of stale WinINet, and reconnect. Shared CDP helpers live in
`scripts/windows-qa/CdpQaHelpers.ps1`.

For a fresh stand or a full RC evidence pass, use the one-command runner
(bootstraps QA dirs, the CDP launcher, and the CDP scheduled task, then chains
all harnesses; see `docs/windows-vm-qa-matrix.md`):

```powershell
.\scripts\windows-qa\Invoke-DoodleRayFullStandQa.ps1 -LocalInstaller <setup.exe> -AllowUnsignedLocalRc
```

For local unsigned RC smoke testing only, add `-AllowUnsignedLocalRc`. Never use
that switch for production release approval; signed CI artifacts must pass the
default gate without it.

The gate intentionally does not print the canonical subscription URL or server
credentials. Subscription import/connect/speed evidence must use the ignored
secret file referenced below.

## Agent Tooling Checkpoint

Before a major Windows networking QA pass, refresh the agent tooling list and
use relevant skills/MCP tools where they are trustworthy. Start with:

- official Codex skills documentation and OpenAI Codex discussions;
- curated Codex ecosystem lists such as Awesome Codex CLI;
- broad Agent Skills indexes such as Awesome Agent Skills;
- QA-focused skill packs such as Playwright/E2E QA skills;
- Microsoft-maintained skills for Windows/Azure-adjacent workflows;
- Windows command/MCP tools only after reviewing their security scan,
  dependencies, and permissions.

Do not install random community skills directly into the release workflow.
Review the source, pin the commit/version, and document why the tool is trusted
before using it for production evidence.

The current vetted network tooling list is maintained in
`docs/windows-network-qa-tooling.md`.

## Mandatory QA Scenarios

Run these on the Play2Go stand for every Windows RC:

1. Clean install from the public installer or local RC installer.
2. Launch as normal user and confirm WebView2/app startup works.
3. Import a real DoodleVPN subscription through the app.
   Use the canonical test subscription from
   `secrets/doodlevpn-test-subscription-url.txt`; see
   `docs/qa-test-subscription.md`.
4. Refresh the subscription and confirm all expected profiles are present.
5. Connect in browser/proxy compatibility mode and verify HTTPS, WebSocket/SSE,
   Telegram Desktop, Discord/Electron, and AI sites where possible.
6. Switch to protected / whole-computer mode and verify tunnel health,
   DNS resolution, route coverage, local HTTP/SOCKS compatibility ports, and
   exit IP/country.
7. Verify default split routing: Russian test sites such as `2ip.ru` should use
   the expected direct route when default RU-direct rules apply.
8. Disconnect, reconnect, switch proxy -> protected -> proxy without reboot.
9. Reboot the server, launch DoodleRay, and verify no stale WinINet proxy,
   orphaned service state, or stuck runtime process prevents connect.
10. Run protected-mode crash recovery: kill only the service-owned
   `sing-box.exe` or `xray.exe`, verify UI leaves `CONNECTED`, WinINet is
   cleaned, service no longer reports stale runtime ports/routes, and reconnect
   succeeds without reboot.
11. Test update path from the previous public version to the RC.
12. Export support bundle and verify redaction before attaching evidence.
13. Uninstall and confirm DoodleRay-owned proxy/routes/service artifacts are
   cleaned without damaging unrelated Windows network settings.

## Measurement

Capture these numbers for each RC:

- app version and service version;
- install/update result;
- subscription import result and server count;
- selected profile name, protocol, and engine;
- proxy-mode ping/HTTP probe result;
- protected-mode health verdict and degraded reasons, if any;
- exit IP/country for proxy and protected modes;
- download/upload speed using the real DoodleVPN subscription;
- Telegram/Discord/browser/AI-site smoke result;
- reboot-free repair result;
- protected-mode crash/recovery result;
- support bundle redaction result.

## Runtime Prerequisite Notes

The stand is intentionally useful for catching clean-machine runtime gaps.
On 2026-07-01, the Rust test harness initially exited with `0xC0000135`
because the server did not have the Microsoft Visual C++ runtime available.
Installing the official x64 Visual C++ Redistributable made
`C:\Windows\System32\vcruntime140.dll` available and allowed the test harness
to start. Treat VC++ runtime presence as a release prerequisite to verify, the
same way WebView2 is verified.

## Current Evidence

2026-07-01:

- Remote command access was enabled for the Play2Go stand via WinRM/OpenSSH.
- `Invoke-Play2GoPowerShell.ps1` was verified in both inline command and
  uploaded script modes.
- A release-mode Rust test harness was copied to
  `C:\DoodleRayQA\tauri-lib-test-release`.
- `WebView2Loader.dll` was placed next to the harness, and the official x64
  Visual C++ Redistributable was installed on the stand for runtime support.
- Targeted server-side test passed:
  `tests::windows_subscription_fetch_uses_system_proxy_fallback`.
- Post-test WinINet state was clean:
  `ProxyEnable=0`, empty `ProxyServer`, empty `ProxyOverride`.
- NSA Cyber `HTTP-Connectivity-Tester` was downloaded as a zip on the stand and
  used for redacted HTTPS reachability checks. The real subscription endpoint
  smoke returned HTTP `200` with non-empty content through both
  `Invoke-WebRequest` and `curl.exe`.
- A short `netsh trace scenario=InternetClient capture=yes` was captured around
  the subscription fetch. The raw ETL stays on the server only because it can
  contain network identifiers.

## Safety Rules

- Keep provider console/VNC available before testing protected/TUN mode. RDP can
  disconnect if routing breaks.
- Create a provider snapshot or reinstall point before destructive tests.
- Do not test on the developer's everyday machine when a scenario can be run on
  this stand.
- Do not leave a connected tunnel, stale system proxy, or test subscription data
  behind after a QA run.
- Store raw logs and secrets only locally; commit only redacted summaries.
