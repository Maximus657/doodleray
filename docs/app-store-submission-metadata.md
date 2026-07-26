# DoodleRay VPN — App Store submission draft

Status: **core metadata, privacy labels, age rating, availability, and pricing
are saved in App Store Connect.** Do not put reviewer credentials or personal
contact details in Git.

## App information

| Field | Proposed value |
|---|---|
| Name | `DoodleRay VPN` |
| Version | Read from `release/release.json` for the submitted source SHA |
| Primary category | Utilities |
| Secondary category | Productivity |
| Privacy policy URL | `https://doodlevpn.online/privacy` |
| Support URL | `https://doodlevpn.online/support` |
| Marketing URL | `https://doodlevpn.online/` |
| License agreement | Use Apple's standard EULA unless the seller confirms that the DoodleVPN terms may be attached to this App Store listing |

### English (U.S.)

Subtitle (30-character limit):

> One-tap VPN for your Mac

Promotional text:

> Connect in one tap. DoodleRay VPN protects your whole Mac with Apple's system VPN, simple location selection, and unlimited traffic on every plan.

Description:

> DoodleRay VPN makes protecting your Mac simple. Sign in with your DoodleVPN code, choose a location, and connect in one tap. Your connection runs through the macOS system VPN powered by Apple's Network Extension framework.
>
> • Protect your whole Mac through the system VPN
> • Choose a country or let DoodleRay pick automatically
> • Enjoy unlimited traffic with every DoodleVPN plan
> • See clear connection states and local event history
> • Turn on automatic connection at app launch
> • Get help quickly if something does not connect
> • No ads or third-party analytics SDKs
>
> An active DoodleVPN subscription is required. Subscription and account management take place in the DoodleVPN account outside the app. DoodleRay VPN does not create accounts or sell subscriptions in the app.

Keywords (under 100 bytes):

> vpn,privacy,secure,network,tunnel,macos,connection,proxy

### Russian

Subtitle:

> VPN для Mac в одно касание

Promotional text:

> Подключайтесь в одно касание. DoodleRay VPN защищает весь Mac через системный VPN от Apple, предлагает простой выбор локации и безлимитный трафик.

Description:

> DoodleRay VPN делает защиту Mac простой. Войдите по коду DoodleVPN, выберите локацию и подключитесь в одно касание. Соединение работает через системный VPN macOS на базе Network Extension от Apple.
>
> • Защита всего Mac через системный VPN
> • Выбор страны или автоматической локации
> • Безлимитный трафик в каждом тарифе DoodleVPN
> • Понятные состояния и локальная история событий
> • Автоподключение при запуске приложения
> • Быстрая помощь, если что-то не подключается
> • Без рекламы и сторонних аналитических SDK
>
> Для работы нужна действующая подписка DoodleVPN. Подписка и аккаунт управляются в личном кабинете DoodleVPN вне приложения. В DoodleRay VPN нельзя создать аккаунт или купить подписку.

Keywords (76 UTF-8 bytes, saved):

> vpn,впн,приватность,сеть,туннель,macos,защита

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
| Device ID | Generated device ID/HWID and device public key | Yes | No | App Functionality |
| Product Interaction | Selected location and connect attempt | Yes | No | App Functionality |
| Other Diagnostic Data | Connection success/failure, latency, route/transport and redacted readiness result | Yes | No | App Functionality |
| Other Data Types | Device name/model, platform, OS/app/core versions, package/channel and client capabilities | Yes | No | App Functionality |

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

Saved questionnaire profile: no violence, sexual content, profanity,
drugs, gambling, loot boxes, contests, horror, unrestricted web browsing, or
in-app user-generated content. The app itself has no chat or social feed; its
Support action opens an external support channel. The resulting international
rating is 4+ (with Apple's regional equivalents); the app is not marked Made
for Kids.

## Encryption and export compliance

App Store Connect records that the app contains standard encryption algorithms
outside Apple's operating system and no proprietary encryption. France is not
in the distribution list. Apple's questionnaire therefore requires no export
documentation upload for this build, and the host bundle sets
`ITSAppUsesNonExemptEncryption` to `false`.

This clears the App Store Connect documentation gate only. The seller must
still confirm any external U.S. export-classification and annual
self-classification reporting obligations before release.

## Availability and price

The app is saved as free with the United States as the base storefront. It is
available in 173 markets. France is excluded pending any required French
encryption declaration; Mainland China is excluded pending VPN licensing.
Release is manual after App Review approval.

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
- Run the real Network Extension QA matrix, including the unproven transition
  from direct 5.9.1 (`com.doodlevpn.doodleray`) to the App Store bundle, and
  prepare accepted-size screenshots.
- Confirm any external export-reporting obligation.
- Complete content-rights and reviewer information, upload the exact current
  build, run TestFlight acceptance, and request App Review.

## Apple references

- [Platform version and review information](https://developer.apple.com/help/app-store-connect/reference/app-information/platform-version-information)
- [App privacy details](https://developer.apple.com/app-store/app-privacy-details/)
- [Privacy manifest files](https://developer.apple.com/documentation/bundleresources/privacy-manifest-files)
- [Mac screenshot specifications](https://developer.apple.com/help/app-store-connect/reference/app-information/screenshot-specifications/)
- [Age rating setup](https://developer.apple.com/help/app-store-connect/manage-app-information/set-an-app-age-rating)
- [Export compliance overview](https://developer.apple.com/help/app-store-connect/manage-app-information/overview-of-export-compliance)
