# DoodleRay PC v6 Claude Brief

Last updated: 2026-07-24.

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

- PC repo (this machine): `C:\Users\ilyae\Documents\DoodleRay PC`
- Current branch: `claude/windows-6.0.1-rc-hardening` (targets `main`)
- Stack: Tauri 2 + React + Rust.
- Windows-first VPN client, currently at v6.0.3.
- Runtime pieces: `DoodleRayTunnelService`, `sing-box`, `xray-core`, `wintun`.
- Two proxy engines exist in code (sing-box, Xray-core), but production
  traffic today is entirely Xray-core: VLESS+Reality+XHTTP and
  VLESS+Reality+WS. Don't assume sing-box changes affect real users unless
  you've checked `xray_engine_protocol`/`xray_engine_transport` in `lib.rs`.
- Do not test VPN on the user's local PC. Local machine is for build/check/test
  only. Real install/connect/update QA goes to the Play2Go Windows stand or
  clean Windows VMs. (In practice this session the user tested directly on
  their own machine anyway — respect it if asked, but don't default to it.)

## Local Build Environment — read before building, this bites silently

`npx tauri bundle` **does not compile Rust**. It only packages whatever
already exists at `src-tauri/target/release/<bin>.exe`. If that binary is
missing or stale, running `tauri bundle` alone either fails outright or
silently ships an old build. Use `npx tauri build` (compiles + bundles in one
documented, CLI-driven step), or explicitly `cargo build --release --bin
DoodleRay` before `tauri bundle`.

**Compile-time feature flags — these are `option_env!` in `lib.rs`, baked into
the binary permanently at compile time, not read at runtime. Forgetting them
does not error; it silently ships a broken build (e.g. account sign-in
disabled with no visible cause).** Every official CI workflow
(`.github/workflows/windows-v6-rc.yml`, `release.yml`,
`publish-downloads.yml`) sets these — any local build must match:

```powershell
$env:DOODLERAY_CLOSED_CONTROL_PLANE = "1"
$env:VITE_DOODLERAY_CLOSED_CONTROL_PLANE = "1"
$env:VITE_DOODLERAY_BUILD_CHANNEL = "direct"
$env:VITE_DOODLERAY_UPDATE_CHANNEL = "direct"
$env:VITE_DOODLERAY_DIAGNOSTICS_TELEMETRY = "0"
npx tauri build --bundles nsis --no-sign
```

If you only ever change frontend files, a stale `cargo build` cache can also
serve an old embedded frontend even though `dist/` on disk is current — Cargo
doesn't reliably treat `dist/` changes as a rebuild trigger for the asset
embed step. If a fresh-looking build still shows old UI, `cargo clean --release
-p doodleray --manifest-path .\src-tauri\Cargo.toml` before rebuilding to
rule it out.

## Secrets

Never print, paste, commit, or include raw secrets in support bundles:

- QA server access: `D:\DoodleRayPC\secrets\doodlevpn-server-access.md`
  (path is from an earlier machine layout — verify it still exists here
  before relying on it)
- Canonical DoodleVPN test subscription:
  `D:\DoodleRayPC\secrets\doodlevpn-test-subscription-url.txt` (same caveat)
- `backend_credits.md` in the repo root (if present) has held raw production
  credentials (Dokploy API token, server SSH password) in plaintext,
  untracked. Never `git add -A` blindly in this repo — check `git status`
  output for it and any similar file before staging.

Use secret files only from scripts or local commands. Evidence docs must stay
redacted.

## Read Only When Needed

Start with the smallest relevant set:

- `docs/optimization-research.md` - TUN connect-latency research/plan (added
  2026-07-24), covers the ~15s connect time problem and prioritized fixes.
- `D:\DoodleRayAPP\docs\vpn-practic-report.md` - source research.
- `D:\DoodleRayPC\docs\vpn-practic-coverage-matrix.md` - mapped coverage and gaps.
- `D:\DoodleRayPC\docs\windows-tun-release-qa-report.md` - current QA evidence.
- `D:\DoodleRayPC\docs\release-gate.md` - production gate.
- `D:\DoodleRayPC\docs\windows-pc-qa-play2go.md` - QA stand workflow.
- `D:\DoodleRayPC\scripts\windows-qa\` - QA automation.

The `D:\DoodleRayPC\...` paths above are from an earlier machine/session
layout — this machine's repo lives at `C:\Users\ilyae\Documents\DoodleRay PC`.
Verify a path resolves before trusting it; don't assume the drive letter.

Do not bulk-open the whole repo. Use `rg`/targeted reads.

## Product Truth

- Default mode: `Весь компьютер` / Protected (TUN via `DoodleRayTunnelService`
  owning the Wintun adapter).
- Protected means service-owned TUN + loopback HTTP/SOCKS compatibility proxy +
  structured health + auto-repair.
- The service is the runtime source of truth. UI only sends commands and renders
  the service snapshot.
- UI must not infer protected status from logs, regexes, stale ports, or ad hoc
  probes.
- "Closed control plane" (app-account login via 8-digit code, App API
  locations/connection-profile) is the **current default architecture** —
  `isClosedControlPlaneEnabled()` defaults to true client-side, and the
  Rust-side gate (`DOODLERAY_CLOSED_CONTROL_PLANE`) must be `1` at compile
  time for it to actually work (see Local Build Environment above). The old
  subscription-URL flow is legacy, gated behind an internal-qa-only flag.

## Current Implemented Direction

- `TunnelStatus` / `ConnectionHealthReport` carry structured runtime ports,
  generation/op id, engine, PIDs, adapter, route/DNS/proxy readiness, and
  fatal/degraded/warning checks.
- Health verdicts include protected, protected_degraded, limited, repairing,
  failed, cleanup_pending.
- TUN compatibility proxy failure should be degraded, not fatal, if core TUN is
  healthy.
- `webviewInstallMode` is configured for offline WebView2 installer.
- Support bundle and redaction work exists; Diagnostics panel's "Copy report"
  now carries the full report (support_summary + every check's technical
  detail, uncapped) — it used to silently truncate to 600 chars and omit
  check detail entirely, even though Copy is what almost every user presses
  instead of "Save full bundle."
- Repair (`repair_windows_runtime`) does real cleanup (stops stale
  processes, clears stale routes/DNS artifacts, reinstalls the service if
  unregistered) — but only when genuinely disconnected. While a tunnel is
  connected (even "degraded"), repair correctly withholds the destructive
  route/DNS cleanup script (it targets the "DoodleRay Tunnel" adapter by
  name, so running it on a live connection would tear down that connection's
  own active routes) and can only report status. The actual fix for
  "connected but degraded" is a clean reconnect, which the Repair button
  does not currently trigger — known gap, not yet wired in.
- QA scripts exist under `scripts/windows-qa`.
- Ping display/collection removed from the v6 dashboard client entirely
  (button, ms/dot display, auto-ping-on-load) — was unused dead weight for
  closed-control-plane servers since the only consumer (`autoSelectFastest`)
  is a legacy-subscription-only, currently-hidden Settings toggle.

Verify current code before assuming any item is complete.

## Known Gotchas (learned the hard way — read before repeating)

- **Zustand non-selective `useAppStore()` + a `useEffect` with an unstable
  dependency = infinite render loop.** `Dashboard.tsx` calls
  `useAppStore()` with no selector, so it re-renders on *any* store field
  changing. An effect that calls a store setter and depends on a value
  recreated every render (e.g. a `useCallback` whose own deps aren't
  perfectly stable) will re-fire every render, forever — process stays
  alive, window never finishes painting (looks like a total UI freeze/crash
  with no visible error). Fix pattern: register a stable, ref-backed wrapper
  in a mount-only (`[]`-deps) effect instead of depending on the unstable
  value directly (see `handleModeSelectRef` in `Dashboard.tsx`).
- **Never use `auth.openai.com`-style third-party domains as a hard
  connectivity gate.** OpenAI blocks huge ranges of VPN/datacenter exit IPs
  regardless of whether the tunnel's own DNS/routing is healthy. The Windows
  system resolver canary in `service.rs` used to `break` on the first
  failed target (openai first) and never try the second — now requires
  *all* targets to fail before reporting degraded.
- Country/location names from the closed-control-plane API are in whatever
  language was active when first fetched, and get **cached in `server.name`
  at fetch time** — not re-localized on a later language change. Anything
  that shows or searches location names must resolve them reactively via
  `localizedCountryName()` (`src/lib/ui-format.ts`), not trust the stored
  field directly. This bit both display (stale English names after
  switching to Russian) and search (typing "Каза" not matching a
  stale-English-cached "Kazakhstan").

## Known Remaining Blockers

Do not claim production readiness until all are closed with evidence:

1. Signed CI artifacts: app, service, sing-box, xray, installer/updater.
2. Full OS matrix: Play2Go Server 2022 plus clean Windows 10 22H2 and Windows 11
   23H2/24H2.
3. Upgrade tests from 5.9.1/6.0.1 to 6.0.2 as a real **in-place update**
   (installer run over a still-running prior version, not a full
   uninstall/reinstall cycle) — the storage/session migration code for this
   exists and predates this session (`cc5dbbb`, `01b87d9`), and the NSIS
   hooks unconditionally stop/delete/reinstall the service regardless of
   prior version, but the actual in-place-update path has not been
   exercised end-to-end on a real box.
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
12. TUN connect latency: currently averages ~15s in real-world testing
    against a 2-3s target (5s hard ceiling). Root-cause trace and prioritized
    fix plan are in `docs/optimization-research.md` — not yet implemented.
13. Repair doesn't meaningfully fix a "connected but degraded" state (see
    Current Implemented Direction above) — needs a safe reconnect-triggered
    path, not yet designed.

## Required Local Checks

Run from the repo root (`C:\Users\ilyae\Documents\DoodleRay PC` on this
machine):

```powershell
npm run build
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
cargo check --manifest-path .\src-tauri\Cargo.toml --bin DoodleRay
cargo check --manifest-path .\src-tauri\Cargo.toml --bin DoodleRayService --features windows-service
git diff --check
```

For local QA packaging, unsigned is allowed only as RC — see "Local Build
Environment" above for the required env vars, this is not optional for a
working build:

```powershell
npx tauri build --bundles nsis --no-sign
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
- Telegram, Discord, OpenAI, Claude probes are checked (individually — do not
  let any single one of these gate overall protected status; OpenAI in
  particular blocks VPN exit IPs independent of tunnel health, see Known
  Gotchas);
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
