# Privacy Checklist — DoodleRay Store Submission

VPN-category listings get extra privacy scrutiny. Everything below must be
true in the shipped build and stated in the privacy policy URL.

## Data inventory (what the app actually touches)

| Data | Where | Leaves the device? |
|---|---|---|
| Direct-build subscription URL + server configs | OS Keychain / Credential Manager; legacy plaintext mirrors are migrated and deleted | Only to the user's own subscription host to fetch configs |
| App Store authentication and device registration | Sign-in code, generated device ID/HWID, device name, platform/OS/app/core versions, package/channel, and device public key; refresh credential stays in Keychain | Sent to the closed DoodleVPN app API for authentication, device limits, and secure requests |
| App Store location/profile lease | Selected country/location, client capabilities, and short-lived VPN profile lease | Sent to and received from the closed DoodleVPN app API |
| App Store connection result | Success/failure, latency, route/transport, target country, and redacted readiness result | Sent to the DoodleVPN app API; no traffic destinations or payload contents |
| Traffic contents | Tunneled through the selected DoodleVPN VPN server | Processed for VPN transport; not retained as browsing-content logs by the client |
| Connection logs (UI events) | In-memory / local, redacted by `src/lib/redaction.ts` | No |
| Support bundle | Direct builds only: local file, redacted (no raw secrets/subscription URLs) | Only if the user manually shares it; the current App Store UI does not expose the incomplete sandbox export |
| Diagnostics telemetry (launch/heartbeat/connect-error events via workshop-api) | Disabled by default; DoodleRay backend only in an explicitly flagged build | Only when `VITE_DOODLERAY_DIAGNOSTICS_TELEMETRY=1` |
| Update check | Direct builds may fetch a signed update manifest | App Store builds delegate updates to Apple and do not call the Tauri updater |

## Policy requirements

- [ ] Privacy policy URL is live, matches the above table, and names
      `DoodleRayTunnelService`, the TUN adapter, and proxy changes.
- [x] Automatic telemetry is disabled by default. Store/release CI must keep
      `VITE_DOODLERAY_DIAGNOSTICS_TELEMETRY=0`.
- [ ] If telemetry is enabled in any flavor, disclose every field, add explicit
      consent, retention/deletion details, and an opt-out before submission.
- [ ] No traffic-content logging claim is accurate: engines run with local
      logs only; support bundle redacts identifiers.
- [ ] Support bundle: document that it is generated locally, user-triggered,
      redacted (secrets/URLs masked), and never auto-uploaded.
- [x] Secure storage: subscription secrets go through `secure_store_*`; Tauri
      writes do not mirror values to `localStorage`, and old app-data mirrors
      are migrated one-way and deleted.
- [x] The App Store build does not create accounts; it only signs in to an
      existing DoodleVPN subscription with a code. Apple's in-app deletion rule
      is therefore not triggered by the current build. The policy still offers
      deletion by support. Re-open this item if registration is added.
- [x] No advertising SDKs or third-party analytics SDKs are present in the
      verified App Store bundle.
- [ ] Data deletion story: uninstall removes service and app-owned state;
      document what remains (e.g. `C:\ProgramData\DoodleRay` logs) and how to
      remove it.

## Store questionnaire answers (prepare in advance)

- Collects personal data? → Inventory authentication/device and subscription
  data precisely; telemetry is “No” only for builds where the flag stays off.
- Category-specific VPN declaration (Microsoft may require the VPN app
  declaration): affirm no sale of traffic data, no undisclosed logging.

## Red lines (auto-reject territory)

- Enabling heartbeat/error telemetry without consent and matching disclosure.
- Shipping the canonical QA subscription secret anywhere in the package.
- Auto-uploading diagnostics.
