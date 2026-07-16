#!/bin/bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
CONFIG="$ROOT_DIR/scripts/vpn-qa/xray-freedom-tun.json"
XRAY_IMAGE="ghcr.io/xtls/xray-core:26.7.11"
CURL_IMAGE="curlimages/curl:8.16.0"
DNS_IMAGE="busybox:1.37"
CONTAINER="doodleray-xray-tun-$$"
DOCKER_CONFIG="$(mktemp -d "${TMPDIR:-/tmp}/doodleray-docker-config.XXXXXX")"
export DOCKER_CONFIG
export DOCKER_HOST="unix://$HOME/.colima/default/docker.sock"

cleanup() {
  docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
  find "$DOCKER_CONFIG" -depth -delete
}
trap cleanup EXIT

if ! colima status >/dev/null 2>&1; then
  printf 'Colima is not running. Start the isolated VM with `colima start --vm-type vz`.\n' >&2
  exit 1
fi
if ! docker info >/dev/null 2>&1; then
  printf 'The isolated Docker daemon is unavailable.\n' >&2
  exit 1
fi

host_route_before="$(route -n get default | shasum -a 256 | awk '{print $1}')"
host_dns_before="$(scutil --dns | shasum -a 256 | awk '{print $1}')"

docker run --detach \
  --name "$CONTAINER" \
  --user 0 \
  --cap-add NET_ADMIN \
  --device /dev/net/tun:/dev/net/tun \
  --mount "type=bind,src=$CONFIG,dst=/etc/doodleray/config.json,readonly" \
  "$XRAY_IMAGE" run -c /etc/doodleray/config.json >/dev/null

ready=0
for _ in $(seq 1 30); do
  if ! docker inspect "$CONTAINER" --format '{{.State.Running}}' | rg -q '^true$'; then
    docker logs "$CONTAINER" >&2
    exit 1
  fi
  if docker logs "$CONTAINER" 2>&1 | rg -q 'Xray .* started'; then
    ready=1
    break
  fi
  sleep 0.25
done
if [ "$ready" -ne 1 ]; then
  docker logs "$CONTAINER" >&2
  printf 'Xray TUN did not become ready in the isolated namespace.\n' >&2
  exit 1
fi

http_code="$(docker run --rm --network "container:$CONTAINER" "$CURL_IMAGE" \
  --fail --silent --show-error --connect-timeout 8 --max-time 20 \
  --output /dev/null --write-out '%{http_code}' https://example.com/)"
if [ "$http_code" != "200" ]; then
  printf 'Unexpected HTTPS status through isolated TUN: %s\n' "$http_code" >&2
  exit 1
fi

docker run --rm --network "container:$CONTAINER" "$DNS_IMAGE" \
  nslookup example.com 1.1.1.1 >/dev/null

logs="$(docker logs "$CONTAINER" 2>&1)"
if printf '%s' "$logs" | rg -qi 'failed to|panic|fatal'; then
  printf '%s\n' "$logs" >&2
  exit 1
fi

host_route_after="$(route -n get default | shasum -a 256 | awk '{print $1}')"
host_dns_after="$(scutil --dns | shasum -a 256 | awk '{print $1}')"
if [ "$host_route_before" != "$host_route_after" ] || [ "$host_dns_before" != "$host_dns_after" ]; then
  printf 'Host route or DNS changed during isolated TUN smoke test.\n' >&2
  exit 1
fi

printf 'PASS  Xray TUN carried HTTPS and UDP DNS inside Colima; host route and DNS were unchanged.\n'
