# Privacy Checklist — DoodleRay Store Submission

VPN-category listings get extra privacy scrutiny. Everything below must be
true in the shipped build and stated in the privacy policy URL.

## Data inventory (what the app actually touches)

| Data | Where | Leaves the device? |
|---|---|---|
| Subscription URL + server configs | Encrypted secure store (Windows) with local fallback | Only to the user's own subscription host to fetch configs |
| Traffic contents | Tunneled through user-chosen servers | Never to DoodleRay infrastructure |
| Connection logs (UI events) | In-memory / local, redacted by `src/lib/redaction.ts` | No |
| Support bundle | Local file, redacted (no raw secrets/subscription URLs) | Only if the user manually shares it |
| Telemetry (launch/heartbeat/connect-error events via workshop-api) | DoodleRay backend | Yes — MUST be disclosed |
| Crash/version info (update check) | Update manifest fetch | Version + platform only |

## Policy requirements

- [ ] Privacy policy URL is live, matches the above table, and names
      `DoodleRayTunnelService`, the TUN adapter, and proxy changes.
- [ ] Telemetry (launch, heartbeat, connection-error reporting in
      `src/lib/workshop-api.ts`) is disclosed with data fields listed; decide
      and document opt-out or removal for Store builds before submission.
- [ ] No traffic-content logging claim is accurate: engines run with local
      logs only; support bundle redacts identifiers.
- [ ] Support bundle: document that it is generated locally, user-triggered,
      redacted (secrets/URLs masked), and never auto-uploaded.
- [ ] Secure storage: subscription secrets go through `secure_store_*`
      commands; localStorage fallback documented as device-local.
- [ ] No advertising SDKs, no third-party analytics SDKs in the package.
- [ ] Data deletion story: uninstall removes service and app-owned state;
      document what remains (e.g. `C:\ProgramData\DoodleRay` logs) and how to
      remove it.

## Store questionnaire answers (prepare in advance)

- Collects personal data? → Yes (subscription endpoint chosen by user;
  telemetry events) — disclose precisely.
- Category-specific VPN declaration (Microsoft may require the VPN app
  declaration): affirm no sale of traffic data, no undisclosed logging.

## Red lines (auto-reject territory)

- Claiming "no logs" while heartbeat/error telemetry is enabled — either
  disclose or strip telemetry from the Store flavor.
- Shipping the canonical QA subscription secret anywhere in the package.
- Auto-uploading diagnostics.
