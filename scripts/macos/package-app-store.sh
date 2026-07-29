#!/bin/bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
APP_BUNDLE="${1:-$ROOT_DIR/src-tauri/target/universal-apple-darwin/release/bundle/macos/DoodleRay VPN.app}"
OUTPUT_DIR="${2:-$ROOT_DIR/dist-app-store}"
read -r RELEASE_VERSION RELEASE_BUILD < <(
  node -e 'const release = JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8")); console.log(`${release.version} ${release.macBuild}`);' "$ROOT_DIR/release/release.json"
)
OUTPUT_PKG="$OUTPUT_DIR/DoodleRay-VPN-$RELEASE_VERSION-macOS.pkg"

node "$ROOT_DIR/scripts/release/check-release.mjs"

[[ "$RELEASE_BUILD" =~ ^[1-9][0-9]*$ ]] || {
  printf 'Invalid App Store bundle version: %s\n' "$RELEASE_BUILD" >&2
  exit 1
}
[[ "${APPLE_TEAM_ID:-}" =~ ^[A-Z0-9]{10}$ ]] || {
  printf 'APPLE_TEAM_ID is missing or invalid.\n' >&2
  exit 1
}

"$ROOT_DIR/scripts/macos/verify-app-store-bundle.sh" "$APP_BUNDLE"

installer_identity="$(security find-identity -v -p basic 2>/dev/null | sed -n 's/.*"\(Mac Installer Distribution:.*\)"/\1/p' | /usr/bin/grep -E "\\(${APPLE_TEAM_ID}\\)$" | head -n 1 || true)"
if [ -z "$installer_identity" ]; then
  installer_identity="$(security find-identity -v -p basic 2>/dev/null | sed -n 's/.*"\(3rd Party Mac Developer Installer:.*\)"/\1/p' | /usr/bin/grep -E "\\(${APPLE_TEAM_ID}\\)$" | head -n 1 || true)"
fi
[ -n "$installer_identity" ] || {
  printf 'Mac Installer Distribution signing identity is missing.\n' >&2
  exit 1
}

mkdir -p "$OUTPUT_DIR"
xcrun productbuild \
  --sign "$installer_identity" \
  --component "$APP_BUNDLE" /Applications \
  "$OUTPUT_PKG"
pkgutil --check-signature "$OUTPUT_PKG"
printf 'Signed App Store package: %s\n' "$OUTPUT_PKG"
