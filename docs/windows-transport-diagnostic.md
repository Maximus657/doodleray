# Windows Transport Diagnostic

Date: 2026-06-28

## Proven edges

- The PC client source is `Maximus657/doodleray`; the installed 5.3.1 binary
  references the same repository and bundles `DoodleRayService.exe`,
  `sing-box.exe`, `xray.exe`, and `wintun.dll`.
- The current persisted defaults are `proxyMode = "system-proxy"` and
  `systemProxyMode = "set"` in `src/stores/app-store.ts`. This makes the
  recommended dashboard card a WinINet proxy mode, not an all-app tunnel.
- `src/lib/connect-helpers.ts` only sends Workshop routing rules when
  `proxyMode === "tun"`, so proxy mode cannot honestly promise app routing.
- `src-tauri/src/lib.rs::vpn_connect` forced `system_proxy_mode` to
  `unchanged` whenever `proxy_mode == "tun"`, preventing a protected
  TUN-plus-compatibility-proxy default.
- `src-tauri/src/lib.rs::build_singbox_config` emitted only a TUN inbound for
  sing-box TUN mode, so Windows system proxy could not safely be enabled in
  sing-box TUN mode because no local HTTP proxy listener existed.
- The existing health monitor checks only service IPC for TUN mode or SOCKS
  liveness for proxy mode. That proves a process edge, not that apps like
  Telegram, browser WebView2 surfaces, DNS, routes, or WinINet are actually
  protected.

## Likely user impact

- Users who leave the recommended `browser-apps` card selected can see browsers
  work while Telegram and other desktop apps ignore the proxy and go direct.
- Users missing WebView2 can fail before any VPN code runs.
- Users can see `connected` after a runtime process starts even when local HTTP
  proxy, Windows proxy state, TUN route coverage, or DNS are not proven.

## Implementation direction

- Make the default product mode protected/all-apps: TUN first, system proxy as
  compatibility helper, strict route enabled by default.
- Keep compatibility proxy and manual proxy as advanced/fallback modes.
- Add a structured connection health report and gate UI green status on it.
- Keep support evidence redacted: no subscription URLs, UUIDs, endpoints,
  private keys, raw configs, or user identifiers.
