#!/bin/bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
MACOS_DIR="$ROOT_DIR/src-tauri/macos"
PROFILE_DIR="$HOME/Library/MobileDevice/Provisioning Profiles"
STAGING_DIR="$MACOS_DIR/profiles"
HOST_PROFILE_NAME="DoodleRay VPN macOS App Store Host"
EXTENSION_PROFILE_NAME="DoodleRay VPN macOS App Store Extension"
HOST_PROFILE_STAGED="$STAGING_DIR/DoodleRayHost.provisionprofile"
HOST_ENTITLEMENTS_STAGED="$STAGING_DIR/DoodleRayHost.entitlements"
EXTENSION_BUNDLE="$MACOS_DIR/DerivedData/Build/Products/Release/DoodleRayVPN.appex"
APP_BUNDLE="$ROOT_DIR/src-tauri/target/universal-apple-darwin/release/bundle/macos/DoodleRay VPN.app"
SIGNING_IDENTITY_NAME="${MACOS_APP_STORE_SIGNING_IDENTITY_NAME:-Apple Distribution}"

find_profile_by_name() {
  local expected_name="$1"
  local profile decoded name

  for profile in "$PROFILE_DIR"/*.provisionprofile; do
    [ -f "$profile" ] || continue
    decoded="$(mktemp "${TMPDIR:-/tmp}/doodleray-profile.XXXXXX")"
    security cms -D -i "$profile" > "$decoded"
    name="$(/usr/libexec/PlistBuddy -c 'Print :Name' "$decoded")"
    rm -f "$decoded"
    if [ "$name" = "$expected_name" ]; then
      printf '%s\n' "$profile"
      return 0
    fi
  done
  return 1
}

host_profile="${MACOS_APP_STORE_PROVISIONING_PROFILE:-}"
extension_profile="${PACKET_TUNNEL_PROVISIONING_PROFILE:-}"
[ -n "$host_profile" ] || host_profile="$(find_profile_by_name "$HOST_PROFILE_NAME")"
[ -n "$extension_profile" ] || extension_profile="$(find_profile_by_name "$EXTENSION_PROFILE_NAME")"
[ -f "$host_profile" ] || { printf 'Host App Store provisioning profile is missing.\n' >&2; exit 1; }
[ -f "$extension_profile" ] || { printf 'Packet Tunnel provisioning profile is missing.\n' >&2; exit 1; }

mkdir -p "$STAGING_DIR"
security cms -D -i "$host_profile" > "$STAGING_DIR/host-profile.plist"
security cms -D -i "$extension_profile" > "$STAGING_DIR/extension-profile.plist"

team_id="$(/usr/libexec/PlistBuddy -c 'Print :TeamIdentifier:0' "$STAGING_DIR/host-profile.plist")"
extension_team_id="$(/usr/libexec/PlistBuddy -c 'Print :TeamIdentifier:0' "$STAGING_DIR/extension-profile.plist")"
host_application_id="$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:com.apple.application-identifier' "$STAGING_DIR/host-profile.plist")"
extension_application_id="$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:com.apple.application-identifier' "$STAGING_DIR/extension-profile.plist")"
extension_profile_name="$(/usr/libexec/PlistBuddy -c 'Print :Name' "$STAGING_DIR/extension-profile.plist")"
identity="$(security find-identity -v -p codesigning | sed -n 's/.*"\([^"]*\)"/\1/p' | awk -v prefix="$SIGNING_IDENTITY_NAME:" 'index($0, prefix) == 1 { print; exit }')"

[[ "$team_id" =~ ^[A-Z0-9]+$ ]] || { printf 'Invalid Team ID in host profile.\n' >&2; exit 1; }
[ "$team_id" = "$extension_team_id" ] || { printf 'Host and extension profiles belong to different teams.\n' >&2; exit 1; }
[ "$host_application_id" = "$team_id.com.doodleray.doodleray" ] || { printf 'Host profile does not match the App Store bundle identifier.\n' >&2; exit 1; }
[ "$extension_application_id" = "$team_id.com.doodleray.doodleray.DoodleRayVPN" ] || { printf 'Extension profile does not match the Packet Tunnel bundle identifier.\n' >&2; exit 1; }
[ -n "$identity" ] || { printf '%s signing identity is missing.\n' "$SIGNING_IDENTITY_NAME" >&2; exit 1; }

cp "$host_profile" "$HOST_PROFILE_STAGED"
cp "$ROOT_DIR/src-tauri/Entitlements.appstore.plist" "$HOST_ENTITLEMENTS_STAGED"
/usr/libexec/PlistBuddy -c "Add :com.apple.application-identifier string $host_application_id" "$HOST_ENTITLEMENTS_STAGED"
/usr/libexec/PlistBuddy -c "Add :com.apple.developer.team-identifier string $team_id" "$HOST_ENTITLEMENTS_STAGED"
/usr/libexec/PlistBuddy -c 'Add :keychain-access-groups array' "$HOST_ENTITLEMENTS_STAGED"
/usr/libexec/PlistBuddy -c "Add :keychain-access-groups:0 string $host_application_id" "$HOST_ENTITLEMENTS_STAGED"

if [ ! -d "$MACOS_DIR/LibXray.xcframework" ]; then
  "$ROOT_DIR/scripts/macos/build-libxray.sh"
fi
"$ROOT_DIR/scripts/macos/generate-extension-project.sh"
rustup target add x86_64-apple-darwin >/dev/null

if ! xcodebuild \
  -project "$MACOS_DIR/DoodleRayAppStoreExtensions.xcodeproj" \
  -scheme DoodleRayVPN \
  -destination 'generic/platform=macOS' \
  -configuration Release \
  -derivedDataPath "$MACOS_DIR/DerivedData" \
  ARCHS="arm64 x86_64" \
  ONLY_ACTIVE_ARCH=NO \
  DEVELOPMENT_TEAM="$team_id" \
  CODE_SIGN_STYLE=Manual \
  CODE_SIGN_IDENTITY="$identity" \
  PROVISIONING_PROFILE_SPECIFIER="$extension_profile_name" \
  build > /tmp/doodleray-app-store-extension-build.log 2>&1; then
  tail -n 160 /tmp/doodleray-app-store-extension-build.log >&2
  exit 1
fi

[ -d "$EXTENSION_BUNDLE" ] || { printf 'Signed Packet Tunnel bundle was not produced.\n' >&2; exit 1; }

export APPLE_SIGNING_IDENTITY="$identity"
export DOODLERAY_BUILD_CHANNEL="app-store"
export DOODLERAY_CLOSED_CONTROL_PLANE="1"
export VITE_DOODLERAY_BUILD_CHANNEL="app-store"
export VITE_DOODLERAY_UPDATE_CHANNEL="app-store"
export VITE_DOODLERAY_CLOSED_CONTROL_PLANE="1"
export VITE_DOODLERAY_DIAGNOSTICS_TELEMETRY="0"
export VITE_DOODLERAY_PRIVACY_POLICY_URL="${DOODLERAY_PRIVACY_POLICY_URL:-https://doodlevpn.online/privacy}"

cd "$ROOT_DIR"
if ! npm run tauri -- build \
  --target universal-apple-darwin \
  --features app-store \
  --bundles app \
  --config src-tauri/tauri.appstore.conf.json \
  --ci > /tmp/doodleray-app-store-tauri-build.log 2>&1; then
  tail -n 200 /tmp/doodleray-app-store-tauri-build.log >&2
  exit 1
fi

"$ROOT_DIR/scripts/macos/verify-app-store-bundle.sh" "$APP_BUNDLE"
printf 'App Store bundle is ready for archive QA: %s\n' "$APP_BUNDLE"
