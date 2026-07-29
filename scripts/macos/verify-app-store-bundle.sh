#!/bin/bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
APP_BUNDLE="${1:-$ROOT_DIR/src-tauri/target/universal-apple-darwin/release/bundle/macos/DoodleRay VPN.app}"
EXTENSION_BUNDLE="$APP_BUNDLE/Contents/PlugIns/DoodleRayVPN.appex"
EXPECTED_TEAM_ID="${APPLE_TEAM_ID:-}"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/doodleray-bundle-verify.XXXXXX")"
HOST_ENTITLEMENTS="$WORK_DIR/host-entitlements.plist"
EXTENSION_ENTITLEMENTS="$WORK_DIR/extension-entitlements.plist"
HOST_PROFILE="$WORK_DIR/host-profile.plist"
EXTENSION_PROFILE="$WORK_DIR/extension-profile.plist"
SIGNING_DETAILS="$WORK_DIR/signing-details.txt"

trap 'rm -rf "$WORK_DIR"' EXIT

require_equal() {
  [ "$1" = "$2" ] || { printf 'FAIL  %s\n' "$3" >&2; exit 1; }
}

plist_value() {
  /usr/libexec/PlistBuddy -c "Print :$2" "$1"
}

require_array_exact() {
  local actual
  actual="$(plist_value "$1" "$2" | sed '1d;$d' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' | sed '/^$/d')"
  require_equal "$actual" "$3" "$4"
}

require_array_contains() {
  local actual
  actual="$(plist_value "$1" "$2" | sed '1d;$d' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' | sed '/^$/d')"
  [[ $'\n'"$actual"$'\n' == *$'\n'"$3"$'\n'* ]] || { printf 'FAIL  %s\n' "$4" >&2; exit 1; }
}

require_architecture() {
  lipo -archs "$1" | tr ' ' '\n' | /usr/bin/grep -Fxq "$2" || {
    printf 'FAIL  missing %s architecture in %s\n' "$2" "$1" >&2
    exit 1
  }
}

[[ "$EXPECTED_TEAM_ID" =~ ^[A-Z0-9]{10}$ ]] || { printf 'FAIL  APPLE_TEAM_ID is missing or invalid.\n' >&2; exit 1; }
[ -d "$APP_BUNDLE" ] || { printf 'FAIL  app bundle is missing: %s\n' "$APP_BUNDLE" >&2; exit 1; }
[ -d "$EXTENSION_BUNDLE" ] || { printf 'FAIL  Packet Tunnel extension is missing.\n' >&2; exit 1; }
[ -f "$APP_BUNDLE/Contents/embedded.provisionprofile" ] || { printf 'FAIL  host provisioning profile is missing.\n' >&2; exit 1; }
[ -f "$EXTENSION_BUNDLE/Contents/embedded.provisionprofile" ] || { printf 'FAIL  extension provisioning profile is missing.\n' >&2; exit 1; }

codesign --verify --strict --deep --verbose=2 "$APP_BUNDLE" >/dev/null 2>&1
codesign --verify --strict --verbose=2 "$EXTENSION_BUNDLE" >/dev/null 2>&1
for bundle in "$APP_BUNDLE" "$EXTENSION_BUNDLE"; do
  codesign -d --verbose=4 "$bundle" > "$SIGNING_DETAILS" 2>&1
  /usr/bin/grep -Fxq "TeamIdentifier=$EXPECTED_TEAM_ID" "$SIGNING_DETAILS" || { printf 'FAIL  signed Team ID mismatch.\n' >&2; exit 1; }
  /usr/bin/grep -q '^Authority=Apple Distribution:' "$SIGNING_DETAILS" || { printf 'FAIL  Apple Distribution signature is missing.\n' >&2; exit 1; }
done
codesign -d --entitlements :- "$APP_BUNDLE" > "$HOST_ENTITLEMENTS" 2>/dev/null
codesign -d --entitlements :- "$EXTENSION_BUNDLE" > "$EXTENSION_ENTITLEMENTS" 2>/dev/null
security cms -D -i "$APP_BUNDLE/Contents/embedded.provisionprofile" > "$HOST_PROFILE"
security cms -D -i "$EXTENSION_BUNDLE/Contents/embedded.provisionprofile" > "$EXTENSION_PROFILE"
plutil -lint "$HOST_ENTITLEMENTS" "$EXTENSION_ENTITLEMENTS" "$HOST_PROFILE" "$EXTENSION_PROFILE" >/dev/null

read -r expected_marketing_version expected_build_version < <(
  node -e 'const release = JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8")); console.log(`${release.version} ${release.macBuild}`);' "$ROOT_DIR/release/release.json"
)
host_info="$APP_BUNDLE/Contents/Info.plist"
extension_info="$EXTENSION_BUNDLE/Contents/Info.plist"
host_executable_name="$(plist_value "$host_info" CFBundleExecutable)"
host_executable="$APP_BUNDLE/Contents/MacOS/$host_executable_name"
extension_executable="$EXTENSION_BUNDLE/Contents/MacOS/DoodleRayVPN"

require_equal "$(plist_value "$host_info" CFBundleIdentifier)" com.doodleray.doodleray 'host bundle identifier'
require_equal "$(plist_value "$host_info" CFBundleShortVersionString)" "$expected_marketing_version" 'host marketing version'
require_equal "$(plist_value "$host_info" CFBundleVersion)" "$expected_build_version" 'host build number'
require_equal "$(plist_value "$host_info" ITSAppUsesNonExemptEncryption)" false 'export-compliance declaration'
require_equal "$(plist_value "$extension_info" CFBundleIdentifier)" com.doodleray.doodleray.DoodleRayVPN 'extension bundle identifier'
require_equal "$(plist_value "$extension_info" CFBundleDisplayName)" 'DoodleRay VPN' 'extension display name'
require_equal "$(plist_value "$extension_info" CFBundleShortVersionString)" "$expected_marketing_version" 'extension marketing version'
require_equal "$(plist_value "$extension_info" CFBundleVersion)" "$expected_build_version" 'extension build number'
require_equal "$(plist_value "$extension_info" NSExtension:NSExtensionPointIdentifier)" com.apple.networkextension.packet-tunnel 'Packet Tunnel extension point'
require_equal "$(plist_value "$extension_info" NSExtension:NSExtensionPrincipalClass)" DoodleRayVPN.PacketTunnelProvider 'Packet Tunnel principal class'

for entitlements in "$HOST_ENTITLEMENTS" "$EXTENSION_ENTITLEMENTS"; do
  require_equal "$(plist_value "$entitlements" com.apple.security.app-sandbox)" true 'App Sandbox entitlement'
  require_equal "$(plist_value "$entitlements" com.apple.security.network.client)" true 'network client entitlement'
  require_array_exact "$entitlements" com.apple.developer.networking.networkextension packet-tunnel-provider 'Network Extension entitlement'
  require_array_exact "$entitlements" com.apple.security.application-groups group.com.doodleray.doodleray 'App Group entitlement'
done

require_equal "$(plist_value "$HOST_ENTITLEMENTS" com.apple.developer.team-identifier)" "$EXPECTED_TEAM_ID" 'host Team ID entitlement'
require_equal "$(plist_value "$EXTENSION_ENTITLEMENTS" com.apple.developer.team-identifier)" "$EXPECTED_TEAM_ID" 'extension Team ID entitlement'
require_equal "$(plist_value "$HOST_ENTITLEMENTS" com.apple.application-identifier)" "$EXPECTED_TEAM_ID.com.doodleray.doodleray" 'host application identifier entitlement'
require_equal "$(plist_value "$EXTENSION_ENTITLEMENTS" com.apple.application-identifier)" "$EXPECTED_TEAM_ID.com.doodleray.doodleray.DoodleRayVPN" 'extension application identifier entitlement'
require_array_exact "$HOST_ENTITLEMENTS" keychain-access-groups "$EXPECTED_TEAM_ID.com.doodleray.doodleray" 'host Keychain group'

for profile in "$HOST_PROFILE" "$EXTENSION_PROFILE"; do
  require_equal "$(plist_value "$profile" TeamIdentifier:0)" "$EXPECTED_TEAM_ID" 'provisioning profile Team ID'
  require_array_contains "$profile" Entitlements:com.apple.developer.networking.networkextension packet-tunnel-provider 'profile Network Extension entitlement'
  require_array_contains "$profile" Entitlements:com.apple.security.application-groups group.com.doodleray.doodleray 'profile App Group entitlement'
  [ "$(plist_value "$profile" Entitlements:get-task-allow 2>/dev/null || true)" != true ] || {
    printf 'FAIL  distribution profile get-task-allow\n' >&2
    exit 1
  }
done
require_equal "$(plist_value "$HOST_PROFILE" Entitlements:com.apple.application-identifier)" "$EXPECTED_TEAM_ID.com.doodleray.doodleray" 'host profile application identifier'
require_equal "$(plist_value "$EXTENSION_PROFILE" Entitlements:com.apple.application-identifier)" "$EXPECTED_TEAM_ID.com.doodleray.doodleray.DoodleRayVPN" 'extension profile application identifier'
if [ -n "${MACOS_APP_STORE_HOST_PROFILE_NAME:-}" ]; then
  require_equal "$(plist_value "$HOST_PROFILE" Name)" "$MACOS_APP_STORE_HOST_PROFILE_NAME" 'host profile name'
fi
if [ -n "${MACOS_APP_STORE_EXTENSION_PROFILE_NAME:-}" ]; then
  require_equal "$(plist_value "$EXTENSION_PROFILE" Name)" "$MACOS_APP_STORE_EXTENSION_PROFILE_NAME" 'extension profile name'
fi

[ -f "$host_executable" ] || { printf 'FAIL  host executable is missing.\n' >&2; exit 1; }
[ -f "$extension_executable" ] || { printf 'FAIL  extension executable is missing.\n' >&2; exit 1; }
for executable in "$host_executable" "$extension_executable"; do
  require_architecture "$executable" arm64
  require_architecture "$executable" x86_64
done

if find "$APP_BUNDLE/Contents" -type f \( -name xray -o -name xray.exe -o -name sing-box -o -name sing-box.exe \) -print | /usr/bin/grep -q .; then
  printf 'FAIL  direct-distribution VPN engine executable is bundled.\n' >&2
  exit 1
fi
find "$APP_BUNDLE/Contents/Resources" -type f -name third-party-notices.txt -print | /usr/bin/grep -q . || { printf 'FAIL  third-party notices are missing.\n' >&2; exit 1; }
privacy_manifest="$APP_BUNDLE/Contents/Resources/PrivacyInfo.xcprivacy"
[ -f "$privacy_manifest" ] || { printf 'FAIL  PrivacyInfo.xcprivacy is missing.\n' >&2; exit 1; }
plutil -lint "$privacy_manifest" >/dev/null
require_equal "$(plist_value "$privacy_manifest" NSPrivacyTracking)" false 'privacy manifest tracking declaration'
for data_type in UserID DeviceID ProductInteraction OtherDiagnosticData OtherDataTypes; do
  /usr/bin/grep -q "NSPrivacyCollectedDataType$data_type" "$privacy_manifest" || { printf 'FAIL  privacy manifest data inventory is incomplete.\n' >&2; exit 1; }
done

printf 'PASS  DoodleRay VPN %s (%s) App Store bundle is universal, sandboxed, Apple-signed, exactly provisioned, and contains the Packet Tunnel extension.\n' "$expected_marketing_version" "$expected_build_version"
