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

## 2026-08-05 — direct domain exception in Windows Xray-TUN

- A custom `direct` domain now goes through both the TUN bridge's `direct`
  route and its physical `dns-direct` resolver. The UI's «Напрямую» label is
  therefore backed by the runtime graph rather than only a stored rule.
- The route is rebuilt on every reconnect and sent to the Windows-owned Tunnel
  Service; no location, VPN eligibility, or proxy-success state is inferred
  from this rule.
- Static coverage asserts exact and wildcard direct domains in both the TUN
  route and its DNS rule. Physical Windows egress proof remains required
  before claiming production runtime validation.
