# Windows VM QA Matrix

Date: 2026-07-02.

Production requires clean-OS evidence beyond the Play2Go Windows Server 2022
stand. This document is the exact plan: what to install, what to prepare once
per VM, and the single command that runs the full evidence pass.

## Verdict on Server 2022

The current Play2Go Server 2022 stand is enough for regression QA and harness
development, but NOT enough for the production claim. Consumer Windows differs
in ways that have already bitten this project: WebView2 preinstall state
(Win11 yes, Win10/Server no), VC++ runtime presence, SmartScreen/Defender
behavior on unsigned/newly-signed binaries, NCSI probing on consumer network
profiles, and non-admin default users.

## Required images

| Priority | Image | Why |
|---|---|---|
| P0 | Windows 10 22H2 x64, clean, local admin user | Largest legacy user base; no WebView2 preinstalled; oldest WinINet stack. |
| P0 | Windows 11 24H2 x64, clean, local admin user | Current consumer baseline; WebView2 preinstalled; newest network stack. |
| P1 | Windows 11 23H2 x64, clean | Previous consumer baseline if 24H2 and 23H2 diverge in QA. |
| P1 | Windows 10/11 with standard (non-admin) user | Non-admin first-run and connect/disconnect UX after admin install. |
| P2 | Windows Server 2025 | Only if the product footprint targets it. |

If the hoster can only reinstall the existing VPS: reinstall it as
Windows 10 22H2 x64 first (biggest coverage gap), run the full pass, then
reinstall as Windows 11 24H2 and repeat. Keep provider console access
available before protected/TUN tests: RDP can drop when routing breaks.

## One-time prep per fresh VM (manual, ~10 minutes)

1. Create/keep a local admin user; log it in once and leave the interactive
   session logged on (autologon or disconnected RDP session is fine — the CDP
   scheduled task needs an interactive session).
2. Install and start OpenSSH Server; allow it through the firewall.
3. Update `secrets/doodlevpn-server-access.md` with the new `host`,
   `login_user`, `login_password`, and the new `ssh_hostkey` fingerprint.
4. Install PuTTY is NOT needed on the VM (only locally).
5. Nothing else: WebView2 comes from the installer (offline mode), VC++
   presence is checked by the deep snapshot, and the QA scaffolding
   (C:\DoodleRayQA dirs, CDP launcher, CDP scheduled task) is bootstrapped by
   the runner below.
6. First run only: import the canonical test subscription once through the UI
   (from the ignored `secrets/doodlevpn-test-subscription-url.txt`). The
   UI pass refreshes it afterwards.

## One command per RC

From `D:\DoodleRayPC`:

```powershell
.\scripts\windows-qa\Invoke-DoodleRayFullStandQa.ps1 `
    -LocalInstaller .\src-tauri\target\release\bundle\nsis\DoodleRay_5.9.0_x64-setup.exe `
    -AllowUnsignedLocalRc
```

Stages (stops on first failure): bootstrap stand scaffolding, publish
installer, install gate with stale-WinINet injection, unclean-shutdown marker
crash simulation, update paths 5.4.3/5.4.4/5.4.5 (last one with stale WinINet
+ corporate PAC injection), active-VPN-during-update, full UI CDP pass
(subscription refresh, Whole Computer connect, mode-switch chain, UI crash
reattach, core crash honesty, support bundle redaction, final cleanup), and a
deep snapshot baseline. Evidence lands under `C:\DoodleRayQA\evidence` on the
stand; commit only redacted summaries.

`-AllowUnsignedLocalRc` is for local unsigned RCs only. The production gate is
the same command without that switch on a signed CI artifact.

## Claim policy

Do not mark Win10/Win11 as covered in `docs/vpn-practic-coverage-matrix.md`
until this pass has actually run on those images. Current coverage is
Windows Server 2022 only.
