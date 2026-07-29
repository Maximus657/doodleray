#!/bin/bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
MACOS_DIR="$ROOT_DIR/src-tauri/macos"
STAGING_DIR="$MACOS_DIR/profiles"
HOST_PROFILE_STAGED="$STAGING_DIR/DoodleRayHost.provisionprofile"
HOST_ENTITLEMENTS_STAGED="$STAGING_DIR/DoodleRayHost.entitlements"
EXTENSION_BUNDLE="$MACOS_DIR/DerivedData/Build/Products/Release/DoodleRayVPN.appex"
APP_BUNDLE="$ROOT_DIR/src-tauri/target/universal-apple-darwin/release/bundle/macos/DoodleRay VPN.app"
EXPECTED_TEAM_ID="${APPLE_TEAM_ID:-}"
read -r RELEASE_VERSION RELEASE_BUILD < <(
  node -e 'const release = JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8")); console.log(`${release.version} ${release.macBuild}`);' "$ROOT_DIR/release/release.json"
)
release_config="$(node -e 'const release = JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8")); process.stdout.write(JSON.stringify({version: release.version, bundle: {macOS: {bundleVersion: String(release.macBuild)}}}));' "$ROOT_DIR/release/release.json")"

node "$ROOT_DIR/scripts/release/check-release.mjs"
[[ "$RELEASE_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { printf 'Invalid App Store marketing version.\n' >&2; exit 1; }
[[ "$RELEASE_BUILD" =~ ^[1-9][0-9]*$ ]] || { printf 'Invalid App Store bundle version.\n' >&2; exit 1; }
[[ "$EXPECTED_TEAM_ID" =~ ^[A-Z0-9]{10}$ ]] || { printf 'APPLE_TEAM_ID is missing or invalid.\n' >&2; exit 1; }
[ -d "$MACOS_DIR/DoodleRayAppStoreExtensions.xcodeproj" ] || { printf 'Tracked Packet Tunnel Xcode project is missing.\n' >&2; exit 1; }

host_profile="${MACOS_APP_STORE_PROVISIONING_PROFILE:-}"
extension_profile="${PACKET_TUNNEL_PROVISIONING_PROFILE:-}"
[ -f "$host_profile" ] || { printf 'Host App Store provisioning profile is missing.\n' >&2; exit 1; }
[ -f "$extension_profile" ] || { printf 'Packet Tunnel provisioning profile is missing.\n' >&2; exit 1; }

mkdir -p "$STAGING_DIR"
security cms -D -i "$host_profile" > "$STAGING_DIR/host-profile.plist"
security cms -D -i "$extension_profile" > "$STAGING_DIR/extension-profile.plist"
team_id="$(/usr/libexec/PlistBuddy -c 'Print :TeamIdentifier:0' "$STAGING_DIR/host-profile.plist")"
extension_team_id="$(/usr/libexec/PlistBuddy -c 'Print :TeamIdentifier:0' "$STAGING_DIR/extension-profile.plist")"
host_application_id="$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:com.apple.application-identifier' "$STAGING_DIR/host-profile.plist")"
extension_application_id="$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:com.apple.application-identifier' "$STAGING_DIR/extension-profile.plist")"
host_profile_name="$(/usr/libexec/PlistBuddy -c 'Print :Name' "$STAGING_DIR/host-profile.plist")"
extension_profile_name="$(/usr/libexec/PlistBuddy -c 'Print :Name' "$STAGING_DIR/extension-profile.plist")"
identity="$(security find-identity -v -p codesigning | sed -n 's/.*"\([^"]*\)"/\1/p' | /usr/bin/grep -E "^Apple Distribution: .+ \\($EXPECTED_TEAM_ID\\)$" | head -n 1 || true)"

[ "$team_id" = "$EXPECTED_TEAM_ID" ] || { printf 'Host profile Team ID does not match APPLE_TEAM_ID.\n' >&2; exit 1; }
[ "$team_id" = "$extension_team_id" ] || { printf 'Host and extension profiles belong to different teams.\n' >&2; exit 1; }
[ "$host_application_id" = "$team_id.com.doodleray.doodleray" ] || { printf 'Host profile does not match the App Store bundle identifier.\n' >&2; exit 1; }
[ "$extension_application_id" = "$team_id.com.doodleray.doodleray.DoodleRayVPN" ] || { printf 'Extension profile does not match the Packet Tunnel bundle identifier.\n' >&2; exit 1; }
[ -n "$host_profile_name" ] || { printf 'Host provisioning profile name is missing.\n' >&2; exit 1; }
[ -n "$extension_profile_name" ] || { printf 'Extension provisioning profile name is missing.\n' >&2; exit 1; }
[ -n "$identity" ] || { printf 'Apple Distribution identity for APPLE_TEAM_ID is missing.\n' >&2; exit 1; }

cp "$host_profile" "$HOST_PROFILE_STAGED"
cp "$ROOT_DIR/src-tauri/Entitlements.appstore.plist" "$HOST_ENTITLEMENTS_STAGED"
/usr/libexec/PlistBuddy -c "Add :com.apple.application-identifier string $host_application_id" "$HOST_ENTITLEMENTS_STAGED"
/usr/libexec/PlistBuddy -c "Add :com.apple.developer.team-identifier string $team_id" "$HOST_ENTITLEMENTS_STAGED"
/usr/libexec/PlistBuddy -c 'Add :keychain-access-groups array' "$HOST_ENTITLEMENTS_STAGED"
/usr/libexec/PlistBuddy -c "Add :keychain-access-groups:0 string $host_application_id" "$HOST_ENTITLEMENTS_STAGED"

"$ROOT_DIR/scripts/macos/build-libxray.sh"
rustup target add x86_64-apple-darwin >/dev/null

if ! xcodebuild \
  -project "$MACOS_DIR/DoodleRayAppStoreExtensions.xcodeproj" \
  -scheme DoodleRayVPN \
  -destination 'generic/platform=macOS' \
  -configuration Release \
  -derivedDataPath "$MACOS_DIR/DerivedData" \
  ARCHS="arm64 x86_64" \
  ONLY_ACTIVE_ARCH=NO \
  MARKETING_VERSION="$RELEASE_VERSION" \
  CURRENT_PROJECT_VERSION="$RELEASE_BUILD" \
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
  --config "$release_config" \
  --ci > /tmp/doodleray-app-store-tauri-build.log 2>&1; then
  tail -n 200 /tmp/doodleray-app-store-tauri-build.log >&2
  exit 1
fi

/usr/bin/grep -a -q "$VITE_DOODLERAY_PRIVACY_POLICY_URL" "$ROOT_DIR/dist" || {
  printf 'App Store privacy-policy URL was not baked into the frontend.\n' >&2
  exit 1
}

"$ROOT_DIR/scripts/macos/verify-app-store-bundle.sh" "$APP_BUNDLE"
printf 'App Store bundle is ready for archive QA: %s\n' "$APP_BUNDLE"
