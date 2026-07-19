#!/bin/bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
APP_BUNDLE="${1:-$ROOT_DIR/src-tauri/target/universal-apple-darwin/release/bundle/macos/DoodleRay VPN.app}"
EXTENSION_BUNDLE="$APP_BUNDLE/Contents/PlugIns/DoodleRayVPN.appex"
HOST_ENTITLEMENTS="$(mktemp "${TMPDIR:-/tmp}/doodleray-host-entitlements.XXXXXX")"
EXTENSION_ENTITLEMENTS="$(mktemp "${TMPDIR:-/tmp}/doodleray-extension-entitlements.XXXXXX")"

cleanup() {
  rm -f "$HOST_ENTITLEMENTS" "$EXTENSION_ENTITLEMENTS"
}
trap cleanup EXIT

require_equal() {
  local actual="$1"
  local expected="$2"
  local label="$3"
  [ "$actual" = "$expected" ] || { printf 'FAIL  %s\n' "$label" >&2; exit 1; }
}

require_architecture() {
  local executable="$1"
  local architecture="$2"
  lipo -archs "$executable" | tr ' ' '\n' | rg -qx "$architecture" || {
    printf 'FAIL  missing %s architecture in %s\n' "$architecture" "$executable" >&2
    exit 1
  }
}

[ -d "$APP_BUNDLE" ] || { printf 'FAIL  app bundle is missing: %s\n' "$APP_BUNDLE" >&2; exit 1; }
[ -d "$EXTENSION_BUNDLE" ] || { printf 'FAIL  Packet Tunnel extension is missing.\n' >&2; exit 1; }

codesign --verify --strict --deep --verbose=2 "$APP_BUNDLE" >/dev/null 2>&1
codesign --verify --strict --verbose=2 "$EXTENSION_BUNDLE" >/dev/null 2>&1
codesign -d --entitlements :- "$APP_BUNDLE" > "$HOST_ENTITLEMENTS" 2>/dev/null
codesign -d --entitlements :- "$EXTENSION_BUNDLE" > "$EXTENSION_ENTITLEMENTS" 2>/dev/null
plutil -lint "$HOST_ENTITLEMENTS" "$EXTENSION_ENTITLEMENTS" >/dev/null

host_info="$APP_BUNDLE/Contents/Info.plist"
extension_info="$EXTENSION_BUNDLE/Contents/Info.plist"
expected_marketing_version="$(node -p "require('$ROOT_DIR/package.json').version")"
expected_build_version="$(node -p "require('$ROOT_DIR/src-tauri/tauri.appstore.conf.json').bundle.macOS.bundleVersion")"
host_executable_name="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$host_info")"
host_executable="$APP_BUNDLE/Contents/MacOS/$host_executable_name"
extension_executable="$EXTENSION_BUNDLE/Contents/MacOS/DoodleRayVPN"

require_equal "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$host_info")" "com.doodleray.doodleray" "host bundle identifier"
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$host_info")" "$expected_marketing_version" "host marketing version"
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$host_info")" "$expected_build_version" "host build number"
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :ITSAppUsesNonExemptEncryption' "$host_info")" "false" "export-compliance declaration"
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$extension_info")" "com.doodleray.doodleray.DoodleRayVPN" "extension bundle identifier"
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleDisplayName' "$extension_info")" "DoodleRay VPN" "extension display name"
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$extension_info")" "$expected_marketing_version" "extension marketing version"
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$extension_info")" "$expected_build_version" "extension build number"
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :NSExtension:NSExtensionPointIdentifier' "$extension_info")" "com.apple.networkextension.packet-tunnel" "Packet Tunnel extension point"
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :NSExtension:NSExtensionPrincipalClass' "$extension_info")" "DoodleRayVPN.PacketTunnelProvider" "Packet Tunnel principal class"

require_equal "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.app-sandbox' "$HOST_ENTITLEMENTS")" "true" "host App Sandbox entitlement"
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.network.client' "$HOST_ENTITLEMENTS")" "true" "host network client entitlement"
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.developer.networking.networkextension:0' "$HOST_ENTITLEMENTS")" "packet-tunnel-provider" "host Network Extension entitlement"
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.application-groups:0' "$HOST_ENTITLEMENTS")" "group.com.doodleray.doodleray" "host App Group entitlement"
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.app-sandbox' "$EXTENSION_ENTITLEMENTS")" "true" "extension App Sandbox entitlement"
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.network.client' "$EXTENSION_ENTITLEMENTS")" "true" "extension network client entitlement"
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.developer.networking.networkextension:0' "$EXTENSION_ENTITLEMENTS")" "packet-tunnel-provider" "extension Network Extension entitlement"
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.application-groups:0' "$EXTENSION_ENTITLEMENTS")" "group.com.doodleray.doodleray" "extension App Group entitlement"

if host_get_task_allow="$(/usr/libexec/PlistBuddy -c 'Print :get-task-allow' "$HOST_ENTITLEMENTS" 2>/dev/null)"; then
  require_equal "$host_get_task_allow" "false" "host distribution build disables get-task-allow"
fi
if extension_get_task_allow="$(/usr/libexec/PlistBuddy -c 'Print :get-task-allow' "$EXTENSION_ENTITLEMENTS" 2>/dev/null)"; then
  require_equal "$extension_get_task_allow" "false" "extension distribution build disables get-task-allow"
fi

host_team="$(/usr/libexec/PlistBuddy -c 'Print :com.apple.developer.team-identifier' "$HOST_ENTITLEMENTS")"
extension_team="$(/usr/libexec/PlistBuddy -c 'Print :com.apple.developer.team-identifier' "$EXTENSION_ENTITLEMENTS")"
host_application_id="$(/usr/libexec/PlistBuddy -c 'Print :com.apple.application-identifier' "$HOST_ENTITLEMENTS")"
extension_application_id="$(/usr/libexec/PlistBuddy -c 'Print :com.apple.application-identifier' "$EXTENSION_ENTITLEMENTS")"
[ -n "$host_team" ] || { printf 'FAIL  host Team ID entitlement is missing.\n' >&2; exit 1; }
require_equal "$host_team" "$extension_team" "host and extension signing teams"
require_equal "$host_application_id" "$host_team.com.doodleray.doodleray" "host application identifier entitlement"
require_equal "$extension_application_id" "$host_team.com.doodleray.doodleray.DoodleRayVPN" "extension application identifier entitlement"
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :keychain-access-groups:0' "$HOST_ENTITLEMENTS")" "$host_application_id" "least-privilege host Keychain group"

[ -f "$APP_BUNDLE/Contents/embedded.provisionprofile" ] || { printf 'FAIL  host provisioning profile is missing.\n' >&2; exit 1; }
[ -f "$EXTENSION_BUNDLE/Contents/embedded.provisionprofile" ] || { printf 'FAIL  extension provisioning profile is missing.\n' >&2; exit 1; }
[ -f "$host_executable" ] || { printf 'FAIL  host executable is missing.\n' >&2; exit 1; }
[ -f "$extension_executable" ] || { printf 'FAIL  extension executable is missing.\n' >&2; exit 1; }
require_architecture "$host_executable" arm64
require_architecture "$host_executable" x86_64
require_architecture "$extension_executable" arm64
require_architecture "$extension_executable" x86_64

if find "$APP_BUNDLE/Contents" -type f \( -name xray -o -name xray.exe -o -name sing-box -o -name sing-box.exe \) -print | rg -q .; then
  printf 'FAIL  direct-distribution VPN engine executable is bundled.\n' >&2
  exit 1
fi

find "$APP_BUNDLE/Contents/Resources" -type f -name third-party-notices.txt -print | rg -q . || {
  printf 'FAIL  third-party notices are missing.\n' >&2
  exit 1
}
privacy_manifest="$APP_BUNDLE/Contents/Resources/PrivacyInfo.xcprivacy"
[ -f "$privacy_manifest" ] || {
  printf 'FAIL  PrivacyInfo.xcprivacy is missing from Contents/Resources.\n' >&2
  exit 1
}
plutil -lint "$privacy_manifest" >/dev/null
require_equal "$(/usr/libexec/PlistBuddy -c 'Print :NSPrivacyTracking' "$privacy_manifest")" "false" "privacy manifest tracking declaration"
rg -q 'NSPrivacyCollectedDataTypeUserID' "$privacy_manifest" || {
  printf 'FAIL  privacy manifest is missing User ID collection.\n' >&2
  exit 1
}
rg -q 'NSPrivacyCollectedDataTypeDeviceID' "$privacy_manifest" || {
  printf 'FAIL  privacy manifest is missing Device ID collection.\n' >&2
  exit 1
}
rg -q 'NSPrivacyCollectedDataTypeOtherDiagnosticData' "$privacy_manifest" || {
  printf 'FAIL  privacy manifest is missing connection diagnostic collection.\n' >&2
  exit 1
}
printf 'PASS  DoodleRay VPN %s (%s) App Store bundle is universal, sandboxed, signed, provisioned, and contains the Packet Tunnel extension.\n' "$expected_marketing_version" "$expected_build_version"
