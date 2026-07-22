# Microsoft Store Readiness — DoodleRay PC v6

Status: **RC / prep only. Store submission is BLOCKED.** This document tracks
what the v6 store-redesign branch (`codex/v6-store-redesign`) still needs before
a real Microsoft Store submission. It intentionally lists TODOs, not claims of
completion. Do not treat any unchecked item as done.

Last updated: 2026-07-03.

## Scope of the v6 branch

The v6 branch is a **frontend / UX redesign** (Claude Design dark-glass shell)
on top of the existing Tauri 2 + React runtime. It does **not** change the VPN
runtime, `DoodleRayTunnelService`, sing-box/xray/wintun, health, auto-repair,
update, or support-bundle logic. All connection functions remain wired to the
real backend commands.

Tauri was **kept** (not migrated to WinUI/WPF/Electron) to avoid risk to the VPN
backend, installer, updater, and QA matrix. See the branch summary for rationale.

## Packaging & identity blockers

- [ ] Decide Store packaging path: MSIX (Store-native) vs. keeping the NSIS
      per-machine installer as a Win32 "packaged desktop app". The current
      bundle target is `nsis` (per-machine). Store prefers MSIX.
- [ ] Per-machine install + a system service (`DoodleRayTunnelService`) is
      **incompatible with a pure MSIX sandbox**. Confirm whether the tunnel
      service can be delivered as a Store-approved Win32 packaged app, or a
      separate elevated installer, before committing to MSIX.
- [ ] Publisher identity / Partner Center account and `Identity Name`,
      `Publisher`, `PublisherDisplayName` reserved.
- [ ] Package family name + capabilities declaration (runFullTrust for the
      service/driver path).
- [ ] Signing: Store signs MSIX, but the bundled `DoodleRayService.exe`,
      `sing-box`, `xray`, `wintun`, and installer/updater still need valid
      Authenticode signatures from signed CI (see release gate). **Not done.**

## Window / chrome (v6)

- [x] Custom titlebar (undecorated window) with minimize / maximize / close and
      a drag region; Windows keeps native edge-resize on undecorated windows.
- [ ] Verify custom chrome on the Play2Go stand + clean Win10 22H2 / Win11
      23H2/24H2: drag, snap layouts, per-monitor DPI, maximize/restore, and that
      resize borders work. **Not verified on real hardware.**
- [ ] High-contrast mode + Windows accent color integration review.

## Accessibility & localization

- [x] ru / en / zh strings for all new v6 UI (no hardcoded English in
      components).
- [x] `prefers-reduced-motion` respected for orb/spin/pulse animations.
- [x] Focus-visible outlines on interactive controls (`.v6-focus`).
- [ ] Full keyboard-navigation pass (tab order, orb activation, list roving).
- [ ] Screen-reader pass (NVDA) for the connect orb + status announcements.
- [ ] Store age rating / content declarations questionnaire.

## Functional QA (must pass on the Windows matrix, not local dev)

- [ ] Connect / disconnect / cancel in all three modes (Protected, Browsers,
      Manual) drive the real backend and show honest verdicts (protected /
      protected_degraded / limited / failed).
- [ ] Honest fallback path (Protected → Browsers) surfaces `limited` messaging,
      never fake-green.
- [ ] Server selection / hot-switch, subscription refresh, add subscription /
      proxy link, support-bundle export, diagnostics drawer.
- [ ] Servers / Settings / Workshop pages fully functional inside the new shell.

## Store listing assets

- [ ] Screenshots of the v6 UI at Store-required resolutions.
- [ ] Store icons / tile assets (Square 44/71/150/310, wide 310x150).
- [ ] Description, privacy policy URL, support URL, EULA.

## Release rule

Per the project release gate: if any production gate item (signed CI artifacts,
full OS matrix, upgrade/broken-state update tests, PAC/NRPT restore, sleep/wake
recovery, IPv6/QUIC policy, support-bundle markers, honest fallback, scoped
stale-state repair) is missing, the final status is:

```
RC only, production blocked.
```

Store submission stays blocked until those are closed with evidence **and** the
Store-specific packaging/identity/signing items above are resolved.
