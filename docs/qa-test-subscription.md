# DoodleVPN QA Test Subscription

All Windows PC release-candidate QA must use the canonical DoodleVPN test
subscription stored in the ignored local file:

```text
secrets/doodlevpn-test-subscription-url.txt
```

Use this same subscription for:

- first-run subscription import;
- subscription refresh/profile-count checks;
- proxy/browser compatibility mode;
- protected / whole-computer TUN mode;
- split-routing checks, including Russian/direct sites such as `2ip.ru`;
- Telegram, Discord/Electron, WebView2, browser, and AI-site smoke tests;
- speed measurements and reconnect/update/reboot recovery checks.

Do not commit the raw subscription URL, provider response body, UUIDs,
endpoints, private keys, or screenshots/logs that expose them. Release notes
and QA reports should say "canonical DoodleVPN test subscription" and reference
this file path instead.

