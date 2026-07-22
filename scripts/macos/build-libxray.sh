#!/bin/bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
VERSIONS="$ROOT_DIR/runtime-versions.json"
TAG="$(plutil -extract libxray.tag raw -o - "$VERSIONS")"
COMMIT="$(plutil -extract libxray.commit raw -o - "$VERSIONS")"
GOMOBILE_VERSION="$(plutil -extract gomobile.version raw -o - "$VERSIONS")"
CACHE_DIR="${LIBXRAY_CACHE_DIR:-$ROOT_DIR/.build/libxray-$TAG}"
OUTPUT="$ROOT_DIR/src-tauri/macos/LibXray.xcframework"
ARTIFACT_DIR="$ROOT_DIR/.build/libxray-artifacts/$COMMIT"
ARTIFACT="$ARTIFACT_DIR/LibXray.xcframework"
PROVENANCE="$ARTIFACT/.doodleray-provenance"
EXPECTED_PROVENANCE="tag=$TAG
commit=$COMMIT
gomobile=$GOMOBILE_VERSION"

if [ ! -d "$CACHE_DIR/.git" ]; then
  git clone --branch "$TAG" --depth 1 https://github.com/XTLS/libXray.git "$CACHE_DIR"
fi

actual_commit="$(git -C "$CACHE_DIR" rev-parse HEAD)"
if [ "$actual_commit" != "$COMMIT" ]; then
  printf 'libXray source mismatch: got %s, expected %s\n' "$actual_commit" "$COMMIT" >&2
  exit 1
fi
if ! git -C "$CACHE_DIR" diff --quiet || ! git -C "$CACHE_DIR" diff --cached --quiet; then
  printf 'libXray source cache has local changes: %s\n' "$CACHE_DIR" >&2
  exit 1
fi

export PATH="$PATH:$(go env GOPATH)/bin"
go install "golang.org/x/mobile/cmd/gomobile@$GOMOBILE_VERSION"
gomobile init

if [ ! -f "$ARTIFACT/Info.plist" ] || [ ! -f "$PROVENANCE" ] || [ "$(cat "$PROVENANCE")" != "$EXPECTED_PROVENANCE" ]; then
  if [ -e "$ARTIFACT" ]; then
    find "$ARTIFACT" -depth -delete
  fi
  BUILD_DIR="$(mktemp -d "${TMPDIR:-/tmp}/doodleray-libxray.XXXXXX")"
  cleanup_build_dir() {
    case "$BUILD_DIR" in
      "${TMPDIR:-/tmp}"/doodleray-libxray.*)
        find "$BUILD_DIR" -depth -delete
        ;;
      *)
        printf 'Refusing to clean unexpected build path: %s\n' "$BUILD_DIR" >&2
        ;;
    esac
  }
  trap cleanup_build_dir EXIT

  git -C "$CACHE_DIR" archive "$COMMIT" | tar -x -C "$BUILD_DIR"
  (
    cd "$BUILD_DIR"
    # Go 1.26 requires gobind to be recorded as a tool dependency. Add it only
    # to this disposable source copy so the pinned upstream checkout stays clean.
    go get -tool "golang.org/x/mobile/cmd/gobind@$GOMOBILE_VERSION"
    go run download_geo/main.go
    gomobile bind -target macos -macosversion 12.0 -o LibXray.xcframework
  )

  test -f "$BUILD_DIR/LibXray.xcframework/Info.plist"
  mkdir -p "$ARTIFACT_DIR"
  ditto "$BUILD_DIR/LibXray.xcframework" "$ARTIFACT"
  printf '%s\n' "$EXPECTED_PROVENANCE" > "$PROVENANCE"
fi

if ! /usr/libexec/PlistBuddy -c 'Print :AvailableLibraries:0:SupportedPlatform' "$ARTIFACT/Info.plist" | rg -q '^macos$'; then
  printf 'libXray build did not produce a macOS slice\n' >&2
  exit 1
fi
[ "$(cat "$PROVENANCE")" = "$EXPECTED_PROVENANCE" ] || { printf 'libXray provenance mismatch\n' >&2; exit 1; }

if [ -e "$OUTPUT" ] && [ ! -L "$OUTPUT" ]; then
  preserved="$ROOT_DIR/.build/libxray-artifacts/preserved-$(date +%Y%m%d%H%M%S)"
  mkdir -p "$(dirname "$preserved")"
  mv "$OUTPUT" "$preserved"
fi
ln -sfn "../../.build/libxray-artifacts/$COMMIT/LibXray.xcframework" "$OUTPUT"
printf 'Built %s from %s at %s\n' "$TAG" "$COMMIT" "$OUTPUT"
