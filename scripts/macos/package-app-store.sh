#!/bin/bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
APP_BUNDLE="${1:-$ROOT_DIR/src-tauri/target/universal-apple-darwin/release/bundle/macos/DoodleRay VPN.app}"
OUTPUT_DIR="${2:-$ROOT_DIR/dist-app-store}"
OUTPUT_PKG="$OUTPUT_DIR/DoodleRay-VPN-6.0.0-macOS.pkg"

"$ROOT_DIR/scripts/macos/verify-app-store-bundle.sh" "$APP_BUNDLE"

installer_identity="$(security find-identity -v -p basic 2>/dev/null | sed -n 's/.*"\(Mac Installer Distribution:.*\)"/\1/p' | head -n 1)"
if [ -z "$installer_identity" ]; then
  installer_identity="$(security find-identity -v -p basic 2>/dev/null | sed -n 's/.*"\(3rd Party Mac Developer Installer:.*\)"/\1/p' | head -n 1)"
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
