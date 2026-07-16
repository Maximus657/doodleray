# DoodleRay VPN — App Store submission draft

Status: **draft ready for App Store Connect entry after the legal and privacy
publication gates are resolved.** Do not put reviewer credentials or personal
contact details in Git.

## App information

| Field | Proposed value |
|---|---|
| Name | `DoodleRay VPN` |
| Version | `6.0.0` |
| Primary category | Utilities |
| Secondary category | Productivity |
| Privacy policy URL | `https://doodlevpn.online/privacy` — blocked until the audited v6 revision is deployed |
| Support URL | `https://doodlevpn.online/support` |
| Marketing URL | `https://doodlevpn.online/` |
| License agreement | Use Apple's standard EULA unless the seller confirms that the DoodleVPN terms may be attached to this App Store listing |

### English (U.S.)

Subtitle (30-character limit):

> VPN tunnel for your Mac

Promotional text:

> Connect your Mac to DoodleVPN locations through Apple's system VPN framework, with a simple location picker and clear connection states.

Description:

> DoodleRay VPN is a macOS client for an existing DoodleVPN subscription. Sign in with a DoodleVPN code, choose a location, and connect your Mac through a system VPN tunnel powered by Apple's Network Extension framework.
>
> • Full-device VPN through the macOS system VPN
> • Country-based locations and automatic selection
> • Clear connection states and a local event history
> • Optional auto-connect when the app starts
> • No advertising or third-party analytics SDKs
> • Updates delivered through the Mac App Store
>
> An active DoodleVPN subscription is required. Subscriptions are purchased and managed outside this app. DoodleRay VPN does not create accounts or sell subscriptions in the app.

Keywords (under 100 bytes):

> vpn,privacy,secure,network,tunnel,macos,connection,proxy

### Russian

Subtitle:

> VPN-туннель для вашего Mac

Promotional text:

> Подключайте Mac к локациям DoodleVPN через системный VPN от Apple — с простым выбором страны и понятными состояниями соединения.

Description:

> DoodleRay VPN — клиент macOS для действующей подписки DoodleVPN. Войдите по коду DoodleVPN, выберите локацию и подключите Mac через системный VPN-туннель Network Extension.
>
> • VPN для всего устройства через системный механизм macOS
> • Локации по странам и автоматический выбор
> • Понятные состояния подключения и локальная история событий
> • Автоподключение при запуске приложения
> • Без рекламы и сторонних аналитических SDK
> • Обновления через Mac App Store
>
> Для работы нужна действующая подписка DoodleVPN. Подписка оформляется и управляется вне приложения. В DoodleRay VPN нельзя создать аккаунт или купить подписку.

Russian keywords must be byte-counted in App Store Connect before saving; its
limit is bytes, not characters.

## App Review information

Mark **Sign-in required**. The app uses one eight-digit DoodleVPN code instead
of a username/password pair. Put the reusable reviewer code only in App Store
Connect review information.

Proposed review notes:

> DoodleRay VPN is a macOS Network Extension VPN client for an existing DoodleVPN subscription. It does not create accounts and has no in-app purchases.
>
> Sign-in: enter the dedicated eight-digit review code supplied below and click Sign in. The code remains valid for the review window and is attached to an active reviewer subscription.
>
> Test steps:
> 1. Sign in with the review code.
> 2. Select any available location.
> 3. Click Connect.
> 4. On the first connection, approve the standard macOS request to add the DoodleRay VPN configuration.
> 5. Confirm that macOS reports the VPN as connected, then click Disconnect.
> 6. Open Settings to verify auto-connect, language selection, and App Store-managed updates.
>
> The Mac App Store build uses a sandboxed NEPacketTunnelProvider extension with the VPN engine linked into the extension. It does not request administrator privileges, modify system proxy settings, or launch child VPN executables.

Before submission, verify the reviewer code twice from clean installations,
including refresh, locations, connect, disconnect, relaunch, and sign-out. Keep
the reviewer subscription and device allowance active for the whole review
window, then revoke the reusable code after review.

## App privacy answers

Answer **Yes, data is collected**. The Store binary has no ads, tracking, or
third-party analytics. The following answers match `PrivacyInfo.xcprivacy` and
the current v6 API requests:

| Apple data type | What v6 sends | Linked to user | Tracking | Purpose |
|---|---|---:|---:|---|
| User ID | The account/subscription identity resolved from the sign-in code | Yes | No | App Functionality |
| Device ID | Generated device ID/HWID and device public key | Yes | No | App Functionality, security and fraud prevention |
| Product Interaction | Selected location and connect attempt | Yes | No | App Functionality |
| Other Diagnostic Data | Connection success/failure, latency, route/transport and redacted readiness result | Yes | No | App Functionality and service reliability |
| Other Data Types | Device name/model, platform, OS/app/core versions, package/channel and client capabilities | Yes | No | App Functionality and compatibility |

Do not select advertising, physical location, browsing history, search history,
contacts, health, financial information, or traffic contents. The selected VPN
country is a service location, not the user's physical location. Source IP
addresses may be processed by the API edge for rate limiting and security; the
final App Store answers and published policy must describe that consistently.

Automatic renderer diagnostics/analytics are disabled in this build. This does
not make the correct answer “no data collected”: the minimal authenticated
connection result above is still sent automatically. Support chat/email is
opened outside the app, and the Store UI does not auto-upload a support bundle.

## Age rating

Expected questionnaire profile: no violence, sexual content, profanity,
drugs, gambling, loot boxes, contests, horror, unrestricted web browsing, or
in-app user-generated content. The app itself has no chat or social feed; its
Support action opens an external support channel. Do not select Made for Kids.
Use the calculated rating unless the confirmed service terms require a higher
minimum age, in which case apply the matching override.

## Encryption and export compliance

Do not guess the `ITSAppUsesNonExemptEncryption` answer. The app contains
standard VPN and TLS cryptography through Network Extension and the linked
Xray/Reality implementation. Complete Apple's encryption questionnaire for the
actual countries of distribution and obtain the resulting exemption or
documentation determination before adding any Info.plist compliance key. A
VPN-specific export classification may need legal review and, depending on the
determination, periodic reporting outside App Store Connect.

## Screenshots

Provide one to ten non-transparent PNG/JPEG screenshots, all at one accepted
Mac 16:10 size: 1280×800, 1440×900, 2560×1600, or 2880×1800. Recommended set:

1. Sign-in screen with the pre-use VPN data disclosure; never show a real code.
2. Location list and disconnected connect screen.
3. Connected state using the dedicated reviewer account on an isolated QA Mac.
4. App Store settings showing auto-connect, language, and Store updates.
5. Local event history and support choices.

Do not use localhost mock data in final screenshots. Capture the connected
state only on a clean QA Mac where changing the VPN is safe, and redact account,
endpoint, profile, and reviewer information.

## Remaining submission gates

- Confirm the legal relationship between the App Store seller and the operator
  named in the policy/terms.
- Deploy and verify the audited v6 privacy policy.
- Complete export-compliance determination.
- Install the Mac Installer Distribution identity and build the signed `.pkg`.
- Run the real Network Extension QA matrix and prepare accepted-size screenshots.
- Enter/publish metadata, privacy and age-rating answers only after those facts
  are final; then upload to TestFlight before requesting App Review.

## Apple references

- [Platform version and review information](https://developer.apple.com/help/app-store-connect/reference/app-information/platform-version-information)
- [App privacy details](https://developer.apple.com/app-store/app-privacy-details/)
- [Privacy manifest files](https://developer.apple.com/documentation/bundleresources/privacy-manifest-files)
- [Mac screenshot specifications](https://developer.apple.com/help/app-store-connect/reference/app-information/screenshot-specifications/)
- [Age rating setup](https://developer.apple.com/help/app-store-connect/manage-app-information/set-an-app-age-rating)
- [Export compliance overview](https://developer.apple.com/help/app-store-connect/manage-app-information/overview-of-export-compliance)
