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

## 2026-08-05 — Windows service traffic counters

- The Dashboard throughput cards now query the runtime that owns the live
  tunnel: Xray service modes use the configured local Xray `StatsService`,
  while sing-box service modes use their configured local Clash API.
- The UI receives only measured counter deltas. If an owning runtime is
  unavailable, the existing zero value remains; no log-derived or estimated
  speed is displayed as real traffic.
- Focused coverage locks the engine routing so an Xray service cannot regress
  to the prior hard-coded zero branch. Physical Windows throughput evidence
  remains required before a release claim.

## 2026-08-05 — Windows kill-switch truth

- The former desktop toggle did not install a persistent Windows Filtering
  Platform (WFP) policy. It only requested sing-box `strict_route`; Windows
  TUN already forces that route/DNS hardening for every protected session.
- `strict_route` is not a kill switch: when the TUN/engine dies, its routes
  disappear and Windows can use the ordinary direct route. The old control
  therefore advertised protection that did not exist during an unexpected
  tunnel loss.
- The unavailable toggle is removed from both desktop settings surfaces. A
  deliberate Disconnect continues to restore normal network access; a real
  kill switch must block only unexpected loss and requires service-owned WFP
  filters, recovery, uninstaller cleanup, and physical Windows proof before
  it may be exposed again.
