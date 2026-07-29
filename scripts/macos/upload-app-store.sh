#!/bin/bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
MACOS_DIR="$ROOT_DIR/src-tauri/macos"
APP_BUNDLE="${1:-$ROOT_DIR/src-tauri/target/universal-apple-darwin/release/bundle/macos/DoodleRay VPN.app}"
OUTPUT_DIR="${2:-$ROOT_DIR/dist-app-store}"
EXTENSION_BUNDLE="$APP_BUNDLE/Contents/PlugIns/DoodleRayVPN.appex"
HOST_EXECUTABLE="$APP_BUNDLE/Contents/MacOS/DoodleRay"
HOST_INFO="$APP_BUNDLE/Contents/Info.plist"
EXTENSION_DSYM="$MACOS_DIR/DerivedData/Build/Products/Release/DoodleRayVPN.appex.dSYM"
STAMP="$(date +%Y%m%d-%H%M%S)"
EXPORT_DIR="$OUTPUT_DIR/export-$STAMP"
EXPORT_OPTIONS="$OUTPUT_DIR/ExportOptions-$STAMP.plist"
UPLOAD_LOG="$OUTPUT_DIR/upload-$STAMP.log"
HOST_PROFILE_PLIST="$(mktemp "${TMPDIR:-/tmp}/doodleray-upload-host-profile.XXXXXX")"
EXTENSION_PROFILE_PLIST="$(mktemp "${TMPDIR:-/tmp}/doodleray-upload-extension-profile.XXXXXX")"
EVIDENCE="$OUTPUT_DIR/upload-evidence-$STAMP.json"
API_KEY_PATH="${APP_STORE_CONNECT_API_KEY_PATH:-}"
API_KEY_ID="${APP_STORE_CONNECT_API_KEY_ID:-}"
API_ISSUER_ID="${APP_STORE_CONNECT_ISSUER_ID:-}"
EXPECTED_TEAM_ID="${APPLE_TEAM_ID:-}"
read -r RELEASE_VERSION RELEASE_BUILD < <(
  node -e 'const release = JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8")); console.log(`${release.version} ${release.macBuild}`);' "$ROOT_DIR/release/release.json"
)
ARCHIVE="$OUTPUT_DIR/DoodleRay-VPN-$RELEASE_VERSION-$STAMP.xcarchive"

node "$ROOT_DIR/scripts/release/check-release.mjs"

[[ "$RELEASE_BUILD" =~ ^[1-9][0-9]*$ ]] || {
  printf 'Invalid App Store bundle version: %s\n' "$RELEASE_BUILD" >&2
  exit 1
}
[ -f "$API_KEY_PATH" ] || { printf 'App Store Connect API private-key file is missing.\n' >&2; exit 1; }
[ -n "$API_KEY_ID" ] || { printf 'APP_STORE_CONNECT_API_KEY_ID is missing.\n' >&2; exit 1; }
[ -n "$API_ISSUER_ID" ] || { printf 'APP_STORE_CONNECT_ISSUER_ID is missing.\n' >&2; exit 1; }
[[ "$EXPECTED_TEAM_ID" =~ ^[A-Z0-9]{10}$ ]] || { printf 'APPLE_TEAM_ID is missing or invalid.\n' >&2; exit 1; }
[ -f "${MACOS_APP_STORE_PROVISIONING_PROFILE:-}" ] || { printf 'Installed host provisioning profile is missing.\n' >&2; exit 1; }
[ -f "${PACKET_TUNNEL_PROVISIONING_PROFILE:-}" ] || { printf 'Installed extension provisioning profile is missing.\n' >&2; exit 1; }

git -C "$ROOT_DIR" diff --quiet
git -C "$ROOT_DIR" diff --cached --quiet
git_commit="$(git -C "$ROOT_DIR" rev-parse HEAD)"

cleanup() {
  rm -f "$HOST_PROFILE_PLIST" "$EXTENSION_PROFILE_PLIST"
}
trap cleanup EXIT

"$ROOT_DIR/scripts/macos/verify-app-store-bundle.sh" "$APP_BUNDLE"
[ -d "$EXTENSION_BUNDLE" ] || { printf 'Packet Tunnel extension is missing.\n' >&2; exit 1; }
[ -f "$HOST_EXECUTABLE" ] || { printf 'Host executable is missing.\n' >&2; exit 1; }
[ -d "$EXTENSION_DSYM" ] || { printf 'Packet Tunnel dSYM is missing; rebuild the App Store app first.\n' >&2; exit 1; }
marketing_version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$HOST_INFO")"
build_version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$HOST_INFO")"
[ "$marketing_version" = "$RELEASE_VERSION" ] || { printf 'Host marketing version does not match release metadata.\n' >&2; exit 1; }
[ "$build_version" = "$RELEASE_BUILD" ] || { printf 'Host bundle version does not match release metadata.\n' >&2; exit 1; }

security cms -D -i "$APP_BUNDLE/Contents/embedded.provisionprofile" > "$HOST_PROFILE_PLIST"
security cms -D -i "$EXTENSION_BUNDLE/Contents/embedded.provisionprofile" > "$EXTENSION_PROFILE_PLIST"
team_id="$(/usr/libexec/PlistBuddy -c 'Print :TeamIdentifier:0' "$HOST_PROFILE_PLIST")"
extension_team_id="$(/usr/libexec/PlistBuddy -c 'Print :TeamIdentifier:0' "$EXTENSION_PROFILE_PLIST")"
host_profile_name="$(/usr/libexec/PlistBuddy -c 'Print :Name' "$HOST_PROFILE_PLIST")"
extension_profile_name="$(/usr/libexec/PlistBuddy -c 'Print :Name' "$EXTENSION_PROFILE_PLIST")"
signing_sha="$(security find-identity -v -p codesigning | awk -v team="$EXPECTED_TEAM_ID" '$0 ~ /"Apple Distribution:/ && $0 ~ "\\(" team "\\)" {print $2; exit}')"
installer_sha="$(security find-identity -v -p basic | awk -v team="$EXPECTED_TEAM_ID" '$0 ~ /"Mac Installer Distribution:|"3rd Party Mac Developer Installer:/ && $0 ~ "\\(" team "\\)" {print $2; exit}')"

[ "$team_id" = "$EXPECTED_TEAM_ID" ] || { printf 'Host profile Team ID does not match APPLE_TEAM_ID.\n' >&2; exit 1; }
[ "$extension_team_id" = "$EXPECTED_TEAM_ID" ] || { printf 'Extension profile Team ID does not match APPLE_TEAM_ID.\n' >&2; exit 1; }
[ -n "$host_profile_name" ] || { printf 'Host provisioning profile name is missing.\n' >&2; exit 1; }
[ -n "$extension_profile_name" ] || { printf 'Extension provisioning profile name is missing.\n' >&2; exit 1; }
[ -n "$signing_sha" ] || { printf 'Apple Distribution signing identity is missing.\n' >&2; exit 1; }
[ -n "$installer_sha" ] || { printf 'Mac Installer Distribution signing identity is missing.\n' >&2; exit 1; }

build_status="$(node "$ROOT_DIR/scripts/macos/check-app-store-build.mjs" --require-new-or-existing)"
if [ "$build_status" = exists ]; then
  printf 'App Store Connect already contains exact build com.doodleray.doodleray %s (%s); upload is a no-op. Apple does not expose artifact SHA for byte comparison.\n' "$RELEASE_VERSION" "$RELEASE_BUILD"
  exit 0
fi
[ "$build_status" = new ] || { printf 'Unexpected App Store build status.\n' >&2; exit 1; }

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
plutil -insert ApplicationProperties.CFBundleShortVersionString -string "$marketing_version" "$ARCHIVE/Info.plist"
plutil -insert ApplicationProperties.CFBundleVersion -string "$build_version" "$ARCHIVE/Info.plist"
plutil -insert ApplicationProperties.SigningIdentity -string "Apple Distribution" "$ARCHIVE/Info.plist"
plutil -insert ApplicationProperties.Team -string "$team_id" "$ARCHIVE/Info.plist"

plutil -create xml1 "$EXPORT_OPTIONS"
plutil -insert method -string app-store-connect "$EXPORT_OPTIONS"
plutil -insert destination -string upload "$EXPORT_OPTIONS"
plutil -insert signingStyle -string manual "$EXPORT_OPTIONS"
plutil -insert teamID -string "$team_id" "$EXPORT_OPTIONS"
plutil -insert signingCertificate -string "$signing_sha" "$EXPORT_OPTIONS"
plutil -insert installerSigningCertificate -string "$installer_sha" "$EXPORT_OPTIONS"
profiles_json="$(node -e 'process.stdout.write(JSON.stringify({[process.argv[1]]: process.argv[2], [process.argv[3]]: process.argv[4]}));' com.doodleray.doodleray "$host_profile_name" com.doodleray.doodleray.DoodleRayVPN "$extension_profile_name")"
plutil -insert provisioningProfiles -json "$profiles_json" "$EXPORT_OPTIONS"
plutil -insert manageAppVersionAndBuildNumber -bool NO "$EXPORT_OPTIONS"
plutil -insert stripSwiftSymbols -bool YES "$EXPORT_OPTIONS"
plutil -insert uploadSymbols -bool YES "$EXPORT_OPTIONS"

if ! xcodebuild \
  -exportArchive \
  -archivePath "$ARCHIVE" \
  -exportPath "$EXPORT_DIR" \
  -exportOptionsPlist "$EXPORT_OPTIONS" \
  -authenticationKeyPath "$API_KEY_PATH" \
  -authenticationKeyID "$API_KEY_ID" \
  -authenticationKeyIssuerID "$API_ISSUER_ID" > "$UPLOAD_LOG" 2>&1; then
  tail -n 160 "$UPLOAD_LOG" >&2
  exit 1
fi

python3 - "$EVIDENCE" "$git_commit" "$marketing_version" "$build_version" "$ARCHIVE" "$UPLOAD_LOG" <<'PY'
import json
import sys
from datetime import datetime, timezone

path, commit, version, build, archive, log = sys.argv[1:]
with open(path, "w", encoding="utf-8") as handle:
    json.dump({
        "uploaded_at": datetime.now(timezone.utc).isoformat(),
        "git_commit": commit,
        "marketing_version": version,
        "build_version": build,
        "archive": archive,
        "upload_log": log,
    }, handle, indent=2)
    handle.write("\n")
PY

printf 'App Store Connect upload completed.\nCommit: %s\nArchive: %s\nLog: %s\nEvidence: %s\n' "$git_commit" "$ARCHIVE" "$UPLOAD_LOG" "$EVIDENCE"
