#!/bin/bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
MACOS_DIR="$ROOT_DIR/src-tauri/macos"
APP_BUNDLE="${1:-$ROOT_DIR/src-tauri/target/universal-apple-darwin/release/bundle/macos/DoodleRay VPN.app}"
OUTPUT_DIR="${2:-$ROOT_DIR/dist-app-store}"
EXTENSION_BUNDLE="$APP_BUNDLE/Contents/PlugIns/DoodleRayVPN.appex"
HOST_EXECUTABLE="$APP_BUNDLE/Contents/MacOS/DoodleRay"
EXTENSION_DSYM="$MACOS_DIR/DerivedData/Build/Products/Release/DoodleRayVPN.appex.dSYM"
STAMP="$(date +%Y%m%d-%H%M%S)"
ARCHIVE="$OUTPUT_DIR/DoodleRay-VPN-6.0.0-$STAMP.xcarchive"
EXPORT_DIR="$OUTPUT_DIR/export-$STAMP"
EXPORT_OPTIONS="$OUTPUT_DIR/ExportOptions-$STAMP.plist"
UPLOAD_LOG="$OUTPUT_DIR/upload-$STAMP.log"
HOST_PROFILE_PLIST="$(mktemp "${TMPDIR:-/tmp}/doodleray-upload-host-profile.XXXXXX")"

cleanup() {
  rm -f "$HOST_PROFILE_PLIST"
}
trap cleanup EXIT

"$ROOT_DIR/scripts/macos/verify-app-store-bundle.sh" "$APP_BUNDLE"
[ -d "$EXTENSION_BUNDLE" ] || { printf 'Packet Tunnel extension is missing.\n' >&2; exit 1; }
[ -f "$HOST_EXECUTABLE" ] || { printf 'Host executable is missing.\n' >&2; exit 1; }
[ -d "$EXTENSION_DSYM" ] || { printf 'Packet Tunnel dSYM is missing; rebuild the App Store app first.\n' >&2; exit 1; }

security cms -D -i "$APP_BUNDLE/Contents/embedded.provisionprofile" > "$HOST_PROFILE_PLIST"
team_id="$(/usr/libexec/PlistBuddy -c 'Print :TeamIdentifier:0' "$HOST_PROFILE_PLIST")"
signing_sha="$(security find-identity -v -p codesigning | awk '/"Apple Distribution:/{print $2; exit}')"
installer_sha="$(security find-identity -v -p basic | awk '/"Mac Installer Distribution:|"3rd Party Mac Developer Installer:/{print $2; exit}')"

[ -n "$team_id" ] || { printf 'Team ID is missing from the embedded profile.\n' >&2; exit 1; }
[ -n "$signing_sha" ] || { printf 'Apple Distribution signing identity is missing.\n' >&2; exit 1; }
[ -n "$installer_sha" ] || { printf 'Mac Installer Distribution signing identity is missing.\n' >&2; exit 1; }

mkdir -p "$ARCHIVE/Products/Applications" "$ARCHIVE/dSYMs" "$EXPORT_DIR"
ditto "$APP_BUNDLE" "$ARCHIVE/Products/Applications/DoodleRay VPN.app"
xcrun dsymutil "$HOST_EXECUTABLE" -o "$ARCHIVE/dSYMs/DoodleRay VPN.app.dSYM"
ditto "$EXTENSION_DSYM" "$ARCHIVE/dSYMs/DoodleRayVPN.appex.dSYM"

plutil -create xml1 "$ARCHIVE/Info.plist"
plutil -insert ArchiveVersion -integer 2 "$ARCHIVE/Info.plist"
plutil -insert CreationDate -date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$ARCHIVE/Info.plist"
plutil -insert Name -string "DoodleRay VPN" "$ARCHIVE/Info.plist"
plutil -insert SchemeName -string "DoodleRay VPN" "$ARCHIVE/Info.plist"
plutil -insert ApplicationProperties -dictionary "$ARCHIVE/Info.plist"
plutil -insert ApplicationProperties.ApplicationPath -string "Applications/DoodleRay VPN.app" "$ARCHIVE/Info.plist"
plutil -insert ApplicationProperties.Architectures -json '["arm64","x86_64"]' "$ARCHIVE/Info.plist"
plutil -insert ApplicationProperties.CFBundleIdentifier -string "com.doodleray.doodleray" "$ARCHIVE/Info.plist"
plutil -insert ApplicationProperties.CFBundleShortVersionString -string "6.0.0" "$ARCHIVE/Info.plist"
plutil -insert ApplicationProperties.CFBundleVersion -string "60002" "$ARCHIVE/Info.plist"
plutil -insert ApplicationProperties.SigningIdentity -string "Apple Distribution" "$ARCHIVE/Info.plist"
plutil -insert ApplicationProperties.Team -string "$team_id" "$ARCHIVE/Info.plist"

plutil -create xml1 "$EXPORT_OPTIONS"
plutil -insert method -string app-store-connect "$EXPORT_OPTIONS"
plutil -insert destination -string upload "$EXPORT_OPTIONS"
plutil -insert signingStyle -string manual "$EXPORT_OPTIONS"
plutil -insert teamID -string "$team_id" "$EXPORT_OPTIONS"
plutil -insert signingCertificate -string "$signing_sha" "$EXPORT_OPTIONS"
plutil -insert installerSigningCertificate -string "$installer_sha" "$EXPORT_OPTIONS"
plutil -insert provisioningProfiles -json '{"com.doodleray.doodleray":"DoodleRay VPN macOS App Store Host","com.doodleray.doodleray.DoodleRayVPN":"DoodleRay VPN macOS App Store Extension"}' "$EXPORT_OPTIONS"
plutil -insert manageAppVersionAndBuildNumber -bool NO "$EXPORT_OPTIONS"
plutil -insert stripSwiftSymbols -bool YES "$EXPORT_OPTIONS"
plutil -insert uploadSymbols -bool YES "$EXPORT_OPTIONS"

if ! xcodebuild \
  -exportArchive \
  -archivePath "$ARCHIVE" \
  -exportPath "$EXPORT_DIR" \
  -exportOptionsPlist "$EXPORT_OPTIONS" \
  -allowProvisioningUpdates > "$UPLOAD_LOG" 2>&1; then
  tail -n 160 "$UPLOAD_LOG" >&2
  exit 1
fi

printf 'App Store Connect upload completed.\nArchive: %s\nLog: %s\n' "$ARCHIVE" "$UPLOAD_LOG"
