# macOS App Store audit — 2026-07-29

Scope: current source candidate, release manifest `6.0.2 (60018)`, Apple
Developer and App Store Connect state inspected on 2026-07-29. The signed
candidate was uploaded to App Store Connect; no tester group was changed, no
App Review submission was made, and no release was triggered during this audit.

## Verdict

The source and signed bundle are ready for controlled external QA. They are
not ready for App Review submission yet because the uploaded build still needs
processing and the real Network Extension path has not been exercised on a
safe QA Mac. Windows is isolated: this branch is a draft PR and no release
workflow or Windows update channel was invoked.

## Verified

| Area | Evidence | Result |
|---|---|---|
| Source identity | `release/release.json`, Cargo, Tauri and npm all declare `6.0.2`; `macBuild` is `60018`. | Pass |
| Store architecture | `verify-app-store-readiness.sh --full` passed a locally signed universal host and Packet Tunnel extension. | Pass |
| Bundle contract | Both bundles are sandboxed, Apple-signed, precisely provisioned, use the App Group, and the extension has `packet-tunnel-provider`. | Pass |
| Store restrictions | The Store build has no updater artifacts, no direct tunnel executables, no administrator repair path, and diagnostics use Network Extension health. | Pass |
| Apple identifiers | Host `com.doodleray.doodleray` and extension `com.doodleray.doodleray.DoodleRayVPN` exist. Both have App Groups and Network Extensions enabled. | Pass |
| Signing material | Apple Distribution, Mac Installer Distribution, and both App Store profiles are valid through July 2027. | Pass |
| Store metadata | English name, subtitle, category, standard EULA, age rating, privacy URL, support URL, review information, privacy labels, availability, and manual release policy are populated. | Pass |
| Privacy declaration | The five collected-data categories in App Store Connect match the privacy manifest and authenticated control-plane behavior. Store diagnostics telemetry is disabled at build time. | Pass |
| Availability | The app is public in 173 storefronts and intentionally unavailable in Mainland China and France. The free-app agreement is active; the paid-app agreement is not needed while the app stays free and has no IAP. | Pass |
| CI isolation | The pushed draft-PR commit passed both Windows CI and macOS App Store CI. No release or updater publication ran. | Pass |

## Blocking work before App Review

| Priority | Gap | Why it matters | Required action |
|---|---|---|---|
| P0 | Awaiting build processing | The existing TestFlight `60017` build has marketing version `6.0.0`; it cannot be attached to the manual-release `6.0.2` draft. The verified `6.0.2 (60018)` candidate was uploaded, while the version record retains its manual-release policy and metadata. | Wait until `60018` is valid, then attach it. Do not submit it for review. |
| P0 | No safe runtime evidence for `60018` | Static verification cannot prove sign-in, Packet Tunnel permission, routing, DNS, reconnect, or upgrade behavior. The current Mac is intentionally unsuitable because it is already behind a VPN. | Run the QA matrix on an isolated Mac or VM with a dedicated reviewer account. Keep the reviewer service available for Apple. |
| P1 | Current TestFlight build is not available to a private QA audience | The existing internal group has two testers, so assigning a build to it could notify someone other than the intended QA user. No group was changed. | After processing, create a private internal group and add only the confirmed internal QA Apple ID; do not use external testers until the internal matrix passes. |
| P1 | Screenshots are minimal | The listing contains one accepted Mac screenshot. It satisfies the minimum, but it is not an ideal review/marketing set and does not demonstrate the current release. | Capture 4–5 redacted screenshots from `60018`: sign-in disclosure, location selection, connected state, settings, and support/history. Never show a real code, endpoint, account, or profile. |
| P1 | Accessibility label is empty | App Store Connect has no accessibility nutrition-label declaration. It is optional, but claiming support without testing would be inaccurate. | Validate VoiceOver, keyboard navigation, contrast, dark appearance, text scaling, and reduced motion on the QA Mac; then record only the verified features. |
| P1 | Automated upload credentials | App Store Connect API access is approved. A `Developer`-role team key and all ten required GitHub Actions secrets are configured; no release workflow was run to test them. | Exercise the production workflow only with an explicitly approved release candidate, then verify its signed upload evidence. |

## Review-risk controls

- The app is a free companion to an existing DoodleVPN service. It must keep
  the current model: no purchase flow, no in-app purchase CTA, and no link that
  directs a user to buy a subscription. Apple permits a free companion to a
  paid web-based tool when it has no purchase flow or external purchase CTA;
  the review notes must explain that narrow model.
- The reviewer account must stay active and have a device slot for the whole
  review window. The account details belong only in App Store Connect, never in
  source control or screenshots.
- `ITSAppUsesNonExemptEncryption=false` is embedded in the Store host. Do not
  alter availability or upload export paperwork without an explicit legal/
  export-classification decision; China and France remain excluded.
- The direct 5.9.1 application uses a different bundle ID. Its migration path
  is not proven by signing or source inspection and must be exercised before a
  Store release is approved.

## Safe sequence

1. The existing Store draft is now a manual-release `6.0.2` record with
   verified metadata preserved; do not send it for review.
2. Wait for the uploaded `60018` build to process, attach it, and assign it
   only to its private internal group.
3. Complete the clean-install and direct-5.9.1 migration matrix on a safe Mac;
   collect redacted screenshots and real accessibility evidence.
4. App Store Connect API access and CI secrets are configured; do not invoke
   the production workflow without explicit approval for its release target.
5. Re-run the full signed-bundle gate for the exact release SHA and review the
   final metadata. Only then request an explicit go-ahead to submit to App
   Review; App Store release remains manual after approval.

## References

- [App Review Guidelines — completeness and reviewer access](https://developer.apple.com/app-store/review/guidelines/)
- [App Review Guidelines — free stand-alone companion apps](https://developer.apple.com/app-store/review/guidelines/)
- [Mac screenshot specifications](https://developer.apple.com/help/app-store-connect/reference/app-information/screenshot-specifications/)
- [App privacy details](https://developer.apple.com/app-store/app-privacy-details/)
