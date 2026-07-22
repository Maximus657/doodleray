# Certification Notes Draft — DoodleRay (for Partner Center "Notes for certification")

Paste-ready draft. Replace placeholders in ALL CAPS before submission.
Principle: disclose the service/TUN footprint fully; hide nothing.

---

DoodleRay is a Windows network client with three connection modes:
full-device tunnel, browser proxy, and manual local proxy.

What the installer does (perMachine NSIS, silent switch /S):

1. Installs the application to C:\Program Files\DoodleRay.
2. Installs and registers a demand-started Windows service,
   **DoodleRayTunnelService** (DoodleRayService.exe). Its process starts for a
   full-device connection and stops after disconnect; while connected it owns
   tunnel state so networking is repaired/cleaned even if the UI crashes.
3. Ships **wintun.dll** and creates a **virtual network adapter (TUN)** named
   "DoodleRay" — only while full-device mode is connected.
4. Bundles the WebView2 Evergreen offline installer (no network needed).
5. Bundles local proxy/tunnel engines: sing-box.exe and xray.exe. They listen
   only on loopback (default SOCKS 127.0.0.1:10808, HTTP 127.0.0.1:10809).

Runtime behavior:

- The app modifies system proxy (WinINet), routes, and DNS **only after an
  explicit user action** (pressing Connect in full-device or browser mode).
- Disconnect reverts every app-owned change (routes, DNS/NRPT, WinINet proxy,
  virtual adapter). Uninstall stops and removes the service and reverts
  app-owned system changes.
- Full-device mode may show a UAC prompt (administrator permission is required
  to manage the virtual adapter/routes).
- Browser proxy mode and manual proxy mode are available for scenarios where
  full-device routing is not required; some Windows apps do not honor proxy
  settings, and the UI states this limitation explicitly.
- Update policy for the Store build: the app checks a signed manifest and
  shows a "new version available" banner; installation is user-initiated
  (opens the Store listing/support page; no silent self-update).
- Diagnostics: a redacted report can be previewed and submitted by the user.
  Automatic serious-error reports are disabled by default and require an
  explicit, revocable opt-in.

Account requirement:

- After installation the app requires a DoodleVPN account activation code.
- Reviewer activation code: REVIEWER_ACTIVATION_CODE_PLACEHOLDER
- Steps to test: install silently (/S) → launch DoodleRay → enter the dedicated
  eight-digit reviewer code → press Connect (default "Whole computer" mode;
  accept UAC) → verify status shows Protected → Disconnect → uninstall from
  Apps & Features and verify no service/adapter/proxy leftovers.

---

Placeholders to fill before submission:
- REVIEWER_ACTIVATION_CODE_PLACEHOLDER (dedicated, revocable, long-lived code
  with enough device allowance; never a normal user or QA secret).
