# DoodleRay PC v6 Claude Brief

Last updated: 2026-07-02.

This file is the compact operating brief for Claude Code on DoodleRay PC. Keep
context small: read this first, then only open the referenced files needed for
the current task.

## Mandatory Ponytail Mode

Use DietrichGebert/ponytail for all coding and review work:

- Repository: https://github.com/DietrichGebert/ponytail
- Claude Code install, if not already installed:

```text
/plugin marketplace add DietrichGebert/ponytail
/plugin install ponytail@ponytail
```

If the plugin is unavailable, emulate its ladder manually before every edit:

1. Does this need to exist? If no, skip it.
2. Is the solution already in this codebase? Reuse it.
3. Can stdlib/native platform/installed dependency do it? Use that.
4. Prefer the smallest safe change that fixes the real flow.
5. Never cut validation, cleanup, security, redaction, accessibility, or tests.

Be lazy about code volume, never lazy about reading the touched flow.

## Repo And Scope

- PC repo: `D:\DoodleRayPC`
- Branch: `codex/windows-one-click-vpn`
- Stack: Tauri 2 + React + Rust.
- Windows-first VPN client.
- Runtime pieces: `DoodleRayTunnelService`, `sing-box`, `xray-core`, `wintun`.
- Do not test VPN on the user's local PC. Local machine is for build/check/test
  only. Real install/connect/update QA goes to the Play2Go Windows stand or
  clean Windows VMs.

## Secrets

Never print, paste, commit, or include raw secrets in support bundles:

- QA server access: `D:\DoodleRayPC\secrets\doodlevpn-server-access.md`
- Canonical DoodleVPN test subscription:
  `D:\DoodleRayPC\secrets\doodlevpn-test-subscription-url.txt`

Use these files only from scripts or local commands. Evidence docs must stay
redacted.

## Read Only When Needed

Start with the smallest relevant set:

- `D:\DoodleRayAPP\docs\vpn-practic-report.md` - source research.
- `D:\DoodleRayPC\docs\vpn-practic-coverage-matrix.md` - mapped coverage and gaps.
- `D:\DoodleRayPC\docs\windows-tun-release-qa-report.md` - current QA evidence.
- `D:\DoodleRayPC\docs\release-gate.md` - production gate.
- `D:\DoodleRayPC\docs\windows-pc-qa-play2go.md` - QA stand workflow.
- `D:\DoodleRayPC\scripts\windows-qa\` - QA automation.

Do not bulk-open the whole repo. Use `rg`/targeted reads.

## Product Truth

The target is v6.0.0:

- Default mode: `Весь компьютер` / Protected.
- Protected means service-owned TUN + loopback HTTP/SOCKS compatibility proxy +
  structured health + auto-repair.
- The service is the runtime source of truth. UI only sends commands and renders
  the service snapshot.
- UI must not infer protected status from logs, regexes, stale ports, or ad hoc
  probes.

## Current Implemented Direction

The v6 work already started:

- `TunnelStatus` / `ConnectionHealthReport` carry structured runtime ports,
  generation/op id, engine, PIDs, adapter, route/DNS/proxy readiness, and
  fatal/degraded/warning checks.
- Health verdicts include protected, protected_degraded, limited, repairing,
  failed, cleanup_pending.
- TUN compatibility proxy failure should be degraded, not fatal, if core TUN is
  healthy.
- `webviewInstallMode` is configured for offline WebView2 installer.
- Support bundle and redaction work exists but still needs hardening.
- QA scripts exist under `scripts/windows-qa`.

Verify current code before assuming any item is complete.

## Known Remaining Blockers

Do not claim production readiness until all are closed with evidence:

1. Signed CI artifacts: app, service, sing-box, xray, installer/updater.
2. Full OS matrix: Play2Go Server 2022 plus clean Windows 10 22H2 and Windows 11
   23H2/24H2.
3. Upgrade tests from 5.4.3, 5.4.4, 5.4.5 to v6.
4. Broken-state update tests: stale WinINet, PAC/autodetect, NRPT, routes,
   adapters, orphan processes, active VPN during update.
5. Corporate PAC/autodetect exact snapshot and restore.
6. Sleep/wake/network-change service recovery without reboot.
7. IPv6 leak policy: either prove full IPv6 protection or mark it degraded/disabled.
8. QUIC/HTTP3 probe or explicit "not verified" status.
9. Support bundle failure marker, previous unclean shutdown marker, signer
   status/thumbprints, service snapshot, logs, route/DNS/proxy summaries.
10. Honest fallback: Protected -> Browsers -> Manual may happen only with
    `limited` messaging, never fake-green full protection.
11. Stale-state repair must only touch DoodleRay-owned objects and must not
    destroy other VPN/proxy software.

## Required Local Checks

Run from `D:\DoodleRayPC`:

```powershell
npm run build
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
cargo check --manifest-path .\src-tauri\Cargo.toml --bin DoodleRay
cargo check --manifest-path .\src-tauri\Cargo.toml --bin DoodleRayService
git diff --check
```

For local QA packaging, unsigned is allowed only as RC:

```powershell
npx tauri bundle --bundles nsis --no-sign
```

Production must come from signed CI, not a local unsigned RC.

## Play2Go QA Rules

Use the QA scripts, do not hand-mutate the server unless debugging the scripts:

- Upload installer.
- Silent install/update/uninstall.
- Import or refresh canonical subscription from the ignored secret file.
- Connect/disconnect modes.
- Collect health/deep snapshot.
- Inject stale state.
- Verify repair.
- Export redacted evidence.
- Cleanup at the end: WinINet proxy off, no stale ProxyServer, no DoodleRay NRPT
  leftovers, no stale DoodleRay routes/adapters when disconnected, no orphan
  `xray.exe`/`sing-box.exe`/`xray api statsquery`.

Never leave the server dirty.

## Minimum Protected QA Assertions

For `Весь компьютер`:

- service verdict is `protected` or honest `protected_degraded`;
- structured SOCKS/HTTP/API ports are present;
- local listeners accept connections;
- TUN adapter alias/ifIndex exists;
- IPv4 route coverage is correct;
- DNS path is expected and no known leak is open;
- Apple captive GET uses `https://captive.apple.com/hotspot-detect.html`;
- HTTPS, WebSocket, SSE, UDP/STUN probes pass or are honestly degraded;
- Telegram, Discord, OpenAI, Claude probes are checked;
- RU/2ip split-direct behavior matches product rules;
- endpoint bypass route is verified;
- no orphan helper/core processes remain after disconnect/failure.

## Reference Projects To Use

Use these as architecture/test references, not as code to copy blindly:

- WireGuard for Windows, WireGuardNT, Wintun.
- sing-box official docs/releases.
- Tailscale and NetBird for service truth, DNS/NRPT repair, and bugreport/debug
  bundle patterns.
- v2rayN, sing-box-windows, Clash Verge Rev for Windows desktop/proxy UX.
- Microsoft Windows networking docs plus Tauri/WebView2 docs.
- Useful secondary references: mihomo, Xray-core, Hiddify, Rover, FlClash,
  singbox-launcher, sing-box-drover, ZeroTier, OpenVPN, strongSwan.

Respect licenses. Do not copy GPL/custom-license code into this project unless
the project license decision explicitly allows it.

## Release Rule

If any production gate item is missing, final status is:

```text
RC only, production blocked.
```

List exact blockers and evidence. Do not promise "works for everyone" without
signed artifacts and the full Windows QA matrix.
