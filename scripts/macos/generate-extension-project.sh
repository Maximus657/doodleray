#!/bin/bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
MACOS_DIR="$ROOT_DIR/src-tauri/macos"

if ! command -v xcodegen >/dev/null 2>&1; then
  printf 'xcodegen is required (brew install xcodegen)\n' >&2
  exit 1
fi

test -d "$MACOS_DIR/LibXray.xcframework"
xcodegen generate --spec "$MACOS_DIR/project.yml" --project "$MACOS_DIR"
