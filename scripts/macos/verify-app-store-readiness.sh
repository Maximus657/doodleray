#!/bin/bash

set -uo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT_DIR"

failures=0
PROFILE_DIR="$HOME/Library/MobileDevice/Provisioning Profiles"

pass() {
  printf 'PASS  %s\n' "$1"
}

fail() {
  printf 'FAIL  %s\n' "$1" >&2
  failures=$((failures + 1))
}

require_file() {
  if [ -f "$1" ]; then
    pass "$2"
  else
    fail "$2 (missing: $1)"
  fi
}

find_profile_by_name() {
  local expected_name="$1"
  local profile decoded name

  for profile in "$PROFILE_DIR"/*.provisionprofile; do
    [ -f "$profile" ] || continue
    decoded="$(mktemp "${TMPDIR:-/tmp}/doodleray-profile.XXXXXX")"
    if security cms -D -i "$profile" > "$decoded" 2>/dev/null; then
      name="$(/usr/libexec/PlistBuddy -c 'Print :Name' "$decoded" 2>/dev/null || true)"
      rm -f "$decoded"
      if [ "$name" = "$expected_name" ]; then
        printf '%s\n' "$profile"
        return 0
      fi
    else
      rm -f "$decoded"
    fi
  done
  return 1
}

printf 'DoodleRay macOS App Store readiness preflight\n\n'

package_version="$(node -p "require('./package.json').version" 2>/dev/null || true)"
tauri_version="$(node -e "process.stdout.write(require('./src-tauri/tauri.conf.json').version || '')" 2>/dev/null || true)"
cargo_version="$(awk -F ' *= *' '/^version *=/ { gsub(/\"/, "", $2); print $2; exit }' src-tauri/Cargo.toml)"
if [ -n "$package_version" ] && [ "$package_version" = "$tauri_version" ] && [ "$package_version" = "$cargo_version" ]; then
  pass "version is consistent across package.json, Tauri, and Cargo ($package_version)"
else
  fail "version mismatch (package=$package_version tauri=$tauri_version cargo=$cargo_version)"
fi

require_file "src-tauri/tauri.appstore.conf.json" "dedicated App Store Tauri config exists"
require_file "src-tauri/Info.appstore.plist" "App Store export-compliance Info.plist exists"
require_file "src-tauri/Entitlements.appstore.plist" "sandboxed host-app entitlements exist"
require_file "src-tauri/macos/project.yml" "XcodeGen Packet Tunnel project is source-controlled"
require_file "src-tauri/macos/PacketTunnelProvider/PacketTunnelProvider.swift" "Packet Tunnel Provider implementation exists"
require_file "src-tauri/macos/PacketTunnelProvider/Info.plist" "Packet Tunnel Provider Info.plist exists"
require_file "src-tauri/macos/PacketTunnelProvider/Entitlements.plist" "Packet Tunnel Provider entitlements exist"
require_file "scripts/macos/build-app-store.sh" "reproducible App Store build script exists"
require_file "scripts/macos/verify-app-store-bundle.sh" "signed-bundle verifier exists"
require_file "scripts/macos/package-app-store.sh" "signed App Store package script exists"
require_file "src-tauri/resources/PrivacyInfo.xcprivacy" "App Store privacy manifest exists"

if [ -f "src-tauri/resources/PrivacyInfo.xcprivacy" ] && \
   plutil -lint "src-tauri/resources/PrivacyInfo.xcprivacy" >/dev/null 2>&1 && \
   /usr/libexec/PlistBuddy -c 'Print :NSPrivacyTracking' "src-tauri/resources/PrivacyInfo.xcprivacy" 2>/dev/null | rg -q '^false$' && \
   rg -q 'NSPrivacyCollectedDataTypeDeviceID' "src-tauri/resources/PrivacyInfo.xcprivacy" && \
   rg -q 'NSPrivacyCollectedDataTypeOtherDiagnosticData' "src-tauri/resources/PrivacyInfo.xcprivacy"; then
  pass "privacy manifest is valid, tracking-free, and declares v6 collection"
else
  fail "privacy manifest must be valid and match the v6 data inventory"
fi

if [ -f "src-tauri/Entitlements.appstore.plist" ] && \
   /usr/libexec/PlistBuddy -c 'Print :com.apple.security.app-sandbox' src-tauri/Entitlements.appstore.plist 2>/dev/null | rg -q '^true$'; then
  pass "host app enables App Sandbox"
else
  fail "host app must enable com.apple.security.app-sandbox"
fi

if [ -f "src-tauri/Entitlements.appstore.plist" ] && \
   /usr/libexec/PlistBuddy -c 'Print :com.apple.developer.networking.networkextension' src-tauri/Entitlements.appstore.plist 2>/dev/null | rg -q 'packet-tunnel-provider'; then
  pass "host app declares packet-tunnel-provider entitlement"
else
  fail "host app must declare the approved packet-tunnel-provider entitlement"
fi

if rg -q 'NEPacketTunnelProvider' src-tauri/macos 2>/dev/null && \
   rg -q 'NEVPNManager|NETunnelProviderManager' src-tauri/macos src-tauri/src 2>/dev/null; then
  pass "Network Extension provider and manager integration are present"
else
  fail "NEPacketTunnelProvider plus NEVPNManager/NETunnelProviderManager integration are required"
fi

if rg -q 'vpn_connect_app_store' src-tauri/src/lib.rs && \
   rg -q 'app_store_tunnel::start' src-tauri/src/lib.rs && \
   rg -q '#\[cfg\(not\(all\(target_os = "macos", feature = "app-store"\)\)\)\]' src-tauri/src/lib.rs; then
  pass "App Store command routing uses Network Extension and compile-gates direct VPN paths"
else
  fail "App Store commands must use Network Extension while direct VPN paths remain compile-gated"
fi

if [ -f "src-tauri/macos/PacketTunnelProvider/Info.plist" ] && \
   /usr/libexec/PlistBuddy -c 'Print :NSExtension:NSExtensionPointIdentifier' src-tauri/macos/PacketTunnelProvider/Info.plist 2>/dev/null | rg -q '^com\.apple\.networkextension\.packet-tunnel$'; then
  pass "Packet Tunnel Info.plist declares the Network Extension point"
else
  fail "Packet Tunnel Info.plist is missing NSExtension metadata"
fi

if [ -f "src-tauri/macos/PacketTunnelProvider/Entitlements.plist" ] && \
   /usr/libexec/PlistBuddy -c 'Print :com.apple.security.app-sandbox' src-tauri/macos/PacketTunnelProvider/Entitlements.plist 2>/dev/null | rg -q '^true$' && \
   /usr/libexec/PlistBuddy -c 'Print :com.apple.developer.networking.networkextension' src-tauri/macos/PacketTunnelProvider/Entitlements.plist 2>/dev/null | rg -q 'packet-tunnel-provider'; then
  pass "Packet Tunnel source entitlements enable sandbox and packet tunnel"
else
  fail "Packet Tunnel source entitlements are incomplete"
fi

if rg -q 'CODE_SIGN_INJECT_BASE_ENTITLEMENTS: NO' src-tauri/macos/project.yml; then
  pass "distribution extension disables the debug get-task-allow injection"
else
  fail "distribution extension must set CODE_SIGN_INJECT_BASE_ENTITLEMENTS=NO"
fi

if [ -f "src-tauri/tauri.appstore.conf.json" ] && \
   node -e "const c=require('./src-tauri/tauri.appstore.conf.json'); process.exit(c.bundle?.createUpdaterArtifacts === false ? 0 : 1)"; then
  pass "App Store config disables updater artifacts"
else
  fail "App Store config must set bundle.createUpdaterArtifacts=false"
fi

if [ -f "src-tauri/Info.appstore.plist" ] && \
   plutil -lint "src-tauri/Info.appstore.plist" >/dev/null 2>&1 && \
   /usr/libexec/PlistBuddy -c 'Print :ITSAppUsesNonExemptEncryption' src-tauri/Info.appstore.plist 2>/dev/null | rg -q '^false$' && \
   node -e "const c=require('./src-tauri/tauri.appstore.conf.json'); process.exit(c.bundle?.macOS?.infoPlist === 'Info.appstore.plist' ? 0 : 1)"; then
  pass "Store bundle declares no non-exempt encryption per App Store Connect determination"
else
  fail "Store bundle must merge ITSAppUsesNonExemptEncryption=false"
fi

if rg -q "'app-store'" src/lib/update-channel.ts && \
   rg -q 'isUpdateManagedByStore' src/lib/app-updater.ts; then
  pass "frontend delegates App Store updates to Apple"
else
  fail "App Store update channel is not wired"
fi

if rg -q "VITE_DOODLERAY_DIAGNOSTICS_TELEMETRY === '1'" src/lib/build-policy.ts; then
  pass "diagnostics telemetry is opt-in and disabled by default"
else
  fail "diagnostics telemetry must be disabled by default"
fi

if rg -q 'export DOODLERAY_BUILD_CHANNEL="app-store"' scripts/macos/build-app-store.sh && \
   rg -q 'export DOODLERAY_CLOSED_CONTROL_PLANE="1"' scripts/macos/build-app-store.sh; then
  pass "Rust App Store channel and closed control plane are baked at compile time"
else
  fail "App Store build must compile Rust with the closed control plane and App Store channel"
fi

if rg -Fq 'MACOS_APP_STORE_SIGNING_IDENTITY_NAME:-Apple Distribution' scripts/macos/build-app-store.sh; then
  pass "release build defaults to Apple Distribution despite the explicit VM QA override"
else
  fail "release build must default to Apple Distribution signing"
fi

if [ "${DOODLERAY_APP_SUPPORTS_ACCOUNT_CREATION:-0}" = "1" ]; then
  if rg -q 'app_api_delete_account|delete_account' src src-tauri/src; then
    pass "account-creating build has an in-app deletion path"
  else
    fail "account-creating build is missing an in-app deletion path"
  fi
else
  pass "account deletion rule is not applicable (Store build has no account creation)"
fi

privacy_policy_url="${DOODLERAY_PRIVACY_POLICY_URL:-https://doodlevpn.online/privacy}"
privacy_policy_body=""
if [[ "$privacy_policy_url" == https://* ]]; then
  privacy_policy_body="$(curl -fsSL --max-time 15 "$privacy_policy_url" 2>/dev/null || true)"
fi
if printf '%s' "$privacy_policy_body" | rg -qi 'privacy|конфиден'; then
  pass "privacy policy URL is live HTTPS content"
else
  fail "privacy policy URL must be live HTTPS content"
fi

if printf '%s' "$privacy_policy_body" | rg -q 'DoodleRay VPN' && \
   printf '%s' "$privacy_policy_body" | rg -q 'Метаданные соединения' && \
   printf '%s' "$privacy_policy_body" | rg -q 'до 30 дней'; then
  pass "live privacy policy contains the audited v6 desktop data inventory"
else
  fail "live privacy policy is missing the audited v6 desktop data inventory"
fi

host_profile="${MACOS_APP_STORE_PROVISIONING_PROFILE:-}"
extension_profile="${PACKET_TUNNEL_PROVISIONING_PROFILE:-}"
[ -n "$host_profile" ] || host_profile="$(find_profile_by_name 'DoodleRay VPN macOS App Store Host' 2>/dev/null || true)"
[ -n "$extension_profile" ] || extension_profile="$(find_profile_by_name 'DoodleRay VPN macOS App Store Extension' 2>/dev/null || true)"

if [ -f "$host_profile" ]; then
  pass "host-app Mac App Store provisioning profile is present"
else
  fail "host-app Mac App Store provisioning profile is missing or unreadable"
fi

if [ -f "$extension_profile" ]; then
  pass "Packet Tunnel Provider provisioning profile is present"
else
  fail "Packet Tunnel Provider provisioning profile is missing or unreadable"
fi

if security find-identity -v -p codesigning 2>/dev/null | rg -q 'Apple Distribution:'; then
  pass "Apple Distribution signing identity is installed"
else
  fail "Apple Distribution signing identity is not installed"
fi

if security find-identity -v -p basic 2>/dev/null | rg -q 'Mac Installer Distribution:|3rd Party Mac Developer Installer:'; then
  pass "Mac App Store installer signing identity is installed"
else
  fail "Mac Installer Distribution signing identity is not installed"
fi

printf '\n'
if [ "$failures" -gt 0 ]; then
  printf 'BLOCKED: %d App Store readiness check(s) failed.\n' "$failures" >&2
  exit 1
fi

printf 'READY FOR ARCHIVE QA: all static App Store preflight checks passed.\n'
