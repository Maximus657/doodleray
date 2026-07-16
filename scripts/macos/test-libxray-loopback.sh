#!/bin/bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
FRAMEWORK_DIR="$ROOT_DIR/src-tauri/macos/LibXray.xcframework/macos-arm64_x86_64"
SOURCE="$ROOT_DIR/scripts/macos/libxray-loopback-smoke.swift"

if [ ! -f "$FRAMEWORK_DIR/LibXray.framework/LibXray" ]; then
  printf 'LibXray framework is missing; run scripts/macos/build-libxray.sh first.\n' >&2
  exit 1
fi

unused_port() {
  /usr/bin/python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()'
}

SOCKS_PORT="$(unused_port)"
TARGET_PORT="$(unused_port)"
while [ "$TARGET_PORT" = "$SOCKS_PORT" ]; do
  TARGET_PORT="$(unused_port)"
done

BUILD_DIR="$(mktemp -d "${TMPDIR:-/tmp}/doodleray-libxray-smoke.XXXXXX")"
cleanup() {
  find "$BUILD_DIR" -depth -delete
}
trap cleanup EXIT

xcrun swiftc \
  -F "$FRAMEWORK_DIR" \
  -framework LibXray \
  -lresolv \
  "$SOURCE" \
  -o "$BUILD_DIR/libxray-loopback-smoke"

DYLD_FRAMEWORK_PATH="$FRAMEWORK_DIR" \
  "$BUILD_DIR/libxray-loopback-smoke" "$SOCKS_PORT" "$TARGET_PORT"
