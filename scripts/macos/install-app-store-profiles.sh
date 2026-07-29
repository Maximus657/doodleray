#!/bin/bash

set -euo pipefail

ENV_FILE="${1:-}"
PROFILE_DIR="$HOME/Library/MobileDevice/Provisioning Profiles"
STAGING_DIR="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/doodleray-app-store-profiles"
EXPECTED_TEAM="${APPLE_TEAM_ID:-}"

[ -n "$ENV_FILE" ] || { printf 'GITHUB_ENV output path is required.\n' >&2; exit 1; }
[[ "$EXPECTED_TEAM" =~ ^[A-Z0-9]{10}$ ]] || { printf 'APPLE_TEAM_ID is missing or invalid.\n' >&2; exit 1; }
for name in MACOS_APP_STORE_HOST_PROFILE_BASE64 MACOS_APP_STORE_EXTENSION_PROFILE_BASE64; do
  [ -n "${!name:-}" ] || { printf 'Required Apple secret is missing: %s\n' "$name" >&2; exit 1; }
done

umask 077
mkdir -p "$PROFILE_DIR" "$STAGING_DIR"

install_profile() {
  local encoded_name="$1"
  local label="$2"
  local expected_bundle_id="$3"
  local path_env="$4"
  local name_env="$5"
  local profile="$STAGING_DIR/$label.provisionprofile"
  local decoded="$STAGING_DIR/$label.plist"
  local name uuid team application_id installed

  printf '%s' "${!encoded_name}" | base64 -D > "$profile"
  security cms -D -i "$profile" > "$decoded"
  name="$(/usr/libexec/PlistBuddy -c 'Print :Name' "$decoded")"
  uuid="$(/usr/libexec/PlistBuddy -c 'Print :UUID' "$decoded")"
  team="$(/usr/libexec/PlistBuddy -c 'Print :TeamIdentifier:0' "$decoded")"
  application_id="$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:com.apple.application-identifier' "$decoded")"

  [ -n "$name" ] && [[ "$name" != *$'\n'* ]] || { printf '%s profile name is invalid.\n' "$label" >&2; exit 1; }
  [[ "$uuid" =~ ^[A-Fa-f0-9-]+$ ]] || { printf '%s profile UUID is invalid.\n' "$label" >&2; exit 1; }
  [ "$team" = "$EXPECTED_TEAM" ] || { printf '%s profile Team ID does not match APPLE_TEAM_ID.\n' "$label" >&2; exit 1; }
  [ "$application_id" = "$EXPECTED_TEAM.$expected_bundle_id" ] || { printf '%s profile bundle identifier is invalid.\n' "$label" >&2; exit 1; }
  /usr/libexec/PlistBuddy -c 'Print :Entitlements:com.apple.security.application-groups' "$decoded" | /usr/bin/grep -Fq 'group.com.doodleray.doodleray' || {
    printf '%s profile does not authorize the App Group.\n' "$label" >&2
    exit 1
  }
  /usr/libexec/PlistBuddy -c 'Print :Entitlements:com.apple.developer.networking.networkextension' "$decoded" | /usr/bin/grep -Fq 'packet-tunnel-provider' || {
    printf '%s profile does not authorize Packet Tunnel Provider.\n' "$label" >&2
    exit 1
  }
  [ "$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:get-task-allow' "$decoded" 2>/dev/null || true)" != true ] || {
    printf '%s profile is not a distribution profile.\n' "$label" >&2
    exit 1
  }
  ! /usr/libexec/PlistBuddy -c 'Print :ProvisionedDevices:0' "$decoded" >/dev/null 2>&1 || {
    printf '%s profile is device-scoped, not Mac App Store distribution.\n' "$label" >&2
    exit 1
  }

  installed="$PROFILE_DIR/$uuid.provisionprofile"
  cp "$profile" "$installed"
  printf '%s=%s\n%s=%s\n' "$path_env" "$installed" "$name_env" "$name" >> "$ENV_FILE"
  printf 'Installed and verified %s profile: %s\n' "$label" "$name"
}

install_profile MACOS_APP_STORE_HOST_PROFILE_BASE64 host com.doodleray.doodleray \
  MACOS_APP_STORE_PROVISIONING_PROFILE MACOS_APP_STORE_HOST_PROFILE_NAME
install_profile MACOS_APP_STORE_EXTENSION_PROFILE_BASE64 extension com.doodleray.doodleray.DoodleRayVPN \
  PACKET_TUNNEL_PROVISIONING_PROFILE MACOS_APP_STORE_EXTENSION_PROFILE_NAME
