#!/bin/bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
MODE="${1:---static}"
APP_BUNDLE="${2:-$ROOT_DIR/src-tauri/target/universal-apple-darwin/release/bundle/macos/DoodleRay VPN.app}"

cd "$ROOT_DIR"
node --test scripts/macos/app-store-static.test.mjs scripts/macos/check-app-store-build.test.mjs
printf 'STATIC PASS: App Store source contracts are valid.\n'

if [ "$MODE" != "--full" ]; then
  [ "$MODE" = "--static" ] || { printf 'Usage: %s [--static|--full [app-bundle]]\n' "$0" >&2; exit 2; }
  printf 'MACOS RELEASE BLOCKED: static proof cannot replace signing, bundle, or TestFlight evidence; run --full on macOS.\n'
  exit 0
fi

block() {
  printf 'MACOS RELEASE BLOCKED: %s\n' "$1" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || block "$1 is missing"
}

[ "$(uname -s)" = Darwin ] || block 'full verification requires macOS'
for command in node security codesign xcodebuild xcrun plutil lipo; do
  require_command "$command"
done
[ -x /usr/libexec/PlistBuddy ] || block 'PlistBuddy is missing'
[[ "${APPLE_TEAM_ID:-}" =~ ^[A-Z0-9]{10}$ ]] || block 'APPLE_TEAM_ID is missing or invalid'
[ -f "${MACOS_APP_STORE_PROVISIONING_PROFILE:-}" ] || block 'host provisioning profile is missing'
[ -f "${PACKET_TUNNEL_PROVISIONING_PROFILE:-}" ] || block 'extension provisioning profile is missing'
[ -d "$APP_BUNDLE" ] || block "signed app bundle is missing: $APP_BUNDLE"
security find-identity -v -p codesigning 2>/dev/null | /usr/bin/grep -Eq "Apple Distribution: .+ \\($APPLE_TEAM_ID\\)" || block 'Apple Distribution identity is missing'
security find-identity -v -p basic 2>/dev/null | /usr/bin/grep -Eq "(Mac Installer Distribution|3rd Party Mac Developer Installer): .+ \\($APPLE_TEAM_ID\\)" || block 'Mac Installer Distribution identity is missing'

"$ROOT_DIR/scripts/macos/verify-app-store-bundle.sh" "$APP_BUNDLE"
printf 'MACOS RELEASE PASS: signed bundle, profiles, identities, and platform tools passed local verification.\n'
