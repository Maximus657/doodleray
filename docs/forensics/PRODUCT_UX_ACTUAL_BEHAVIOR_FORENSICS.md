# Product UX Actual Behavior Forensics

## 2026-08-05 — desktop update advisory placement

- The update advisory renders only when the existing signed updater reports a
  version or the authenticated control plane supplies a validated minimum
  version.
- It is a full-width Dashboard card directly beside the device-limit warning;
  it is no longer a fixed legacy-style notification.
- The advisory does not change VPN mode, connection state, device eligibility,
  or location selection. Its only action is the existing signed direct updater
  or the managed App Store updates page.
- Server input supplies only a dotted version. Localized copy and the update
  destination remain client-owned, so no raw endpoint, secret, or arbitrary
  server action is exposed in UI.

Verification: `npm test` and `npx tsc --noEmit` must pass. No app build or
physical render was produced for this change because the current release work
is intentionally paused.
