# Isolated VPN QA for DoodleRay v6

The release Mac's active VPN is a protected dependency: no DoodleRay v6 test
may stop, replace, or reconfigure it. Real tunnel QA therefore runs in an
isolated guest. Static checks and loopback engine tests run on the host without
changing its routes, DNS, Network Extension preferences, or VPN state.

## Test layers

### 1. Host-safe build and engine checks

These checks are safe while another VPN is active:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --features app-store --lib
./scripts/macos/test-libxray-loopback.sh
./scripts/macos/verify-app-store-bundle.sh \
  "src-tauri/target/universal-apple-darwin/release/bundle/macos/DoodleRay VPN.app"
```

The libXray smoke test starts the exact framework embedded in the Packet Tunnel
extension, proxies a loopback HTTP request through an Xray SOCKS inbound, and
stops it. It owns only its child HTTP server and does not create a TUN device or
alter host routing.

### 2. Isolated Linux TUN and server compatibility

Use the disposable Colima VM with `/dev/net/tun`. A baseline test with the same
pinned Xray version is available now:

```bash
./scripts/vpn-qa/test-isolated-xray-tun.sh
```

It validates TUN creation, HTTPS, UDP DNS, route-loop avoidance, cleanup, and
the invariant that host route/DNS hashes do not change. Replace only the guest
outbound with a short-lived QA profile to validate real node authentication,
MTU, IPv4/IPv6, reconnects, and failure behavior. The Linux guest uses NAT
through the host's already-active VPN, while all default-route changes stay
inside the guest.

The QA profile must be injected through a secret environment/file mount, never
committed or printed. The runner must redact node addresses, UUIDs, keys, and
subscription tokens from artifacts. This layer does not replace Apple Network
Extension testing.

### 3. macOS Packet Tunnel acceptance in a Tart VM

Use an Apple-silicon macOS guest backed by Apple's Virtualization framework and
the default NAT attachment. Clone a clean baseline before every release:

```bash
export TART_HOME=/Volumes/QA-SSD/doodleray-tart
tart clone ghcr.io/cirruslabs/macos-tahoe-base:latest doodleray-v6-base
tart clone doodleray-v6-base doodleray-v6-run
tart run --dir=doodleray-build:"$PWD":ro doodleray-v6-run
```

Keep the app/build mount read-only. Use a development-signed QA build or
TestFlight build with the same host/extension bundle IDs, App Group, Packet
Tunnel entitlement, and libXray binary as the release flavor. A
production-profile `.app` cannot be side-loaded for this test: macOS accepts
its static signature but rejects the embedded Mac App Store profile outside
Apple's installation flow. For a development build, register the disposable VM
as a development Mac and use matching host and extension development profiles;
for a production build, install it through TestFlight.

Prefer `tart exec` through the Tart guest agent for test control and log
collection. That channel does not depend on guest SSH/network connectivity, so
a broken TUN route cannot strand the runner. Never run the acceptance matrix on
the host Mac.

The current release Mac has hardware virtualization and a valid Apple
Development identity, but only about 12 GB free. A ready Tart macOS image is
about 25 GB before writable VM data. Use an APFS external SSD with at least
60 GB free, or a dedicated remote Apple-silicon Mac with out-of-band console.
Do not delete user data to make room.

## macOS acceptance matrix

Every row must preserve an out-of-band control path and collect unified logs
from the host app and Packet Tunnel provider.

| Scenario | Pass condition |
|---|---|
| Fresh install and first connect | One VPN approval; status reaches `connected`; public egress changes |
| DNS and IPv4/IPv6 | DNS uses tunnel settings; no unintended resolver or IP-family leak |
| TCP, UDP, large transfer | HTTPS, UDP probe, and sustained transfer succeed without route loops |
| Disconnect | Guest routes/DNS restore; extension and Xray stop cleanly |
| 50 connect/disconnect cycles | No stuck `connecting`, leaked process, or stale VPN preference |
| Sleep/wake | Tunnel reconnects or reports an actionable disconnected state |
| Guest NIC detach/attach | Reassert/recovery completes without losing control channel |
| Server timeout/rejection | App fails closed and reports the real provider state |
| Extension crash | macOS and the app converge to disconnected/recoverable state |
| Upgrade over previous v6 build | Saved manager remains valid and the new extension is used |
| Uninstall/reinstall | No unusable stale manager or Keychain/session corruption |

Store screenshots should be captured from this guest or a separate QA Mac only
after the connected-state rows pass. A localhost mock is suitable for layout QA
but not release evidence.

## Evidence and release gate

Archive only redacted results:

- app/extension versions and architectures;
- code-signing and entitlement verification result;
- scenario pass/fail and duration;
- NEVPN status transitions and provider error categories;
- DNS/IPv4/IPv6/MTU outcomes;
- crash reports with credentials and endpoints removed.

The App Store build remains blocked until all macOS acceptance rows pass on an
isolated guest or second physical Mac. Host-safe unit/bundle checks alone do not
prove that the system Packet Tunnel works end to end.

## References

- [Apple: Virtualize macOS on a Mac](https://developer.apple.com/documentation/virtualization/virtualize-macos-on-a-mac)
- [Apple: NAT network device attachment](https://developer.apple.com/documentation/virtualization/vznatnetworkdeviceattachment)
- [Apple: Packet tunnel provider](https://developer.apple.com/documentation/networkextension/packet-tunnel-provider)
- [Tart quick start](https://tart.run/quick-start/)
- [Tart guest agent](https://tart.run/blog/2025/06/01/bridging-the-gaps-with-the-tart-guest-agent/)
- [Xray TUN inbound and external file descriptor](https://github.com/XTLS/Xray-core/blob/main/proxy/tun/README.md)
