# Partner Center Checklist — DoodleRay (Win32 EXE)

Status: prep on branch `codex/v6-store-redesign`. Submission blocked until every
box is checked with evidence. Do not submit unsigned or locally-built artifacts.

## Path decision

- **Win32 EXE submission via Partner Center (EXE/MSI app type), NOT MSIX-first.**
- Rationale: DoodleRay installs `DoodleRayTunnelService` (Windows NT service),
  ships `wintun.dll` (virtual TUN adapter), and mutates routes/DNS/WinINet on
  user action. MSIX containerization breaks or heavily complicates all of that
  (service install, driverless TUN, HKLM per-machine state, NSIS hooks).
  Win32 EXE listing keeps the proven NSIS perMachine installer byte-identical
  to the direct channel.
- Revisit MSIX only if Microsoft requires it for this category later; see
  `certification-notes.md` for the disclosure that makes Win32 viable.

## Account / identity

- [ ] Partner Center account with company verification completed.
- [ ] Publisher display name matches the Authenticode certificate subject.
- [ ] Product reservation: name "DoodleRay" (or fallback), category set
      honestly (Utilities & tools; it is a network/VPN client).

## Installer requirements (EXE path)

- [ ] Built by `scripts/build-store.ps1` (store-win32 flavor, signed CI).
- [ ] Silent install verified: `DoodleRay-store-win32-<ver>-x64-setup.exe /S`
      (`scripts/verify-store-installer.ps1 -Force` on the QA stand/VM).
- [ ] Offline: WebView2 `offlineInstaller` embedded (already configured);
      no other network fetch during install.
- [ ] Hosted at an **immutable, versioned HTTPS URL** (new URL per version,
      never overwrite in place; Store re-crawls the URL and hash must match).
- [ ] SHA256 recorded from `build-store.ps1` output and entered in Partner
      Center exactly.
- [ ] Silent switch declared in Partner Center: `/S`.
- [ ] Installer + every installed PE Authenticode-signed
      (`scripts/verify-signatures.ps1` gate green).

## Signing (all must be signed; gate: scripts/verify-signatures.ps1)

- [ ] DoodleRay.exe (app)
- [ ] DoodleRayService.exe (DoodleRayTunnelService)
- [ ] sing-box.exe (vendor signature or ours)
- [ ] xray.exe (vendor signature or ours)
- [ ] wintun.dll (both copies: src-tauri\ and xray-core\)
- [ ] NSIS installer/uninstaller
- [ ] Timestamped (RFC3161) so signatures outlive cert expiry.

## Update policy for Store builds

- Store does NOT auto-update Win32 EXE listings; existing users keep their
  installed build.
- Store flavor bakes channel `store-win32`; default policy:
  in-app self-update **disabled**, update banner opens the Store/support page
  (user-initiated). Critical-update banner UX is preserved.
- Optional signed in-app channel: publish `latest-store-win32.json` +
  updater artifacts from `build-store.ps1 -WithUpdaterArtifacts`, enable with
  `-EnableSelfUpdate`. Tauri updater verifies the minisign signature before
  install; PrepareForUpdate/startup-repair flow is unchanged.
- Never point Store builds at the direct `latest.json`.

## Listing

- [ ] Copy from `listing-draft.md` only (safe wording; no banned phrases).
- [ ] Screenshots of the real v6 UI (no mock data implying guarantees).
- [ ] Privacy policy URL live and accurate (see `privacy-checklist.md`).
- [ ] Support contact URL live.

## Certification

- [ ] Notes to certifiers pasted from `certification-notes.md`.
- [ ] Reviewer test account/subscription filled in (placeholders there).

## Final go/no-go

See `release-gate.md`. Any unchecked item ⇒ **no submission**.
