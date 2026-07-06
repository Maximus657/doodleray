#!/usr/bin/env bash
set -euo pipefail

DOMAIN="${DOMAIN:-doodleray.clickflare.click}"
ROOT="${ROOT:-/srv/doodleray-downloads}"
CADDY_SNIPPET="${CADDY_SNIPPET:-/etc/caddy/conf.d/doodleray-downloads.caddy}"
CADDYFILE="${CADDYFILE:-/etc/caddy/Caddyfile}"

if [[ "$(id -u)" != "0" ]]; then
  echo "Run as root on the downloads host." >&2
  exit 1
fi
if ! command -v caddy >/dev/null 2>&1; then
  echo "caddy is required. This host already uses Caddy for DoodleVPN; install/enable it before bootstrapping downloads." >&2
  exit 1
fi

install -d -m 0755 "$ROOT/public/releases/direct" "$ROOT/public/releases/store-win32"
install -d -m 0755 "$ROOT/public/channels/direct" "$ROOT/public/channels/store-win32"
install -d -m 0750 "$ROOT/staging"

cat > "$ROOT/public/index.html" <<EOF
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <title>DoodleRay Downloads</title>
    <meta name="robots" content="noindex">
  </head>
  <body>
    <h1>DoodleRay Downloads</h1>
    <p>Release artifacts are served from immutable versioned paths.</p>
  </body>
</html>
EOF

install -d -m 0755 "$(dirname "$CADDY_SNIPPET")"
cat > "$CADDY_SNIPPET" <<EOF
$DOMAIN {
  encode zstd gzip
  root * $ROOT/public

  @hidden path /.git* /staging* /.well-known/acme-challenge/../*
  respond @hidden 404

  @immutable path /releases/*
  header @immutable Cache-Control "public, max-age=31536000, immutable"

  @channels path /channels/*
  header @channels Cache-Control "no-cache, must-revalidate"

  header {
    X-Content-Type-Options "nosniff"
    Referrer-Policy "no-referrer"
    Permissions-Policy "interest-cohort=()"
  }

  file_server
}
EOF

if ! grep -q 'import /etc/caddy/conf.d/\*.caddy' "$CADDYFILE"; then
  cp "$CADDYFILE" "$CADDYFILE.doodleray-downloads.$(date -u +%Y%m%dT%H%M%SZ).bak"
  printf '\n# DoodleRay managed vhosts\nimport /etc/caddy/conf.d/*.caddy\n' >> "$CADDYFILE"
fi

caddy validate --config "$CADDYFILE"
systemctl reload caddy

echo "Downloads host ready:"
echo "  https://$DOMAIN/"
echo "  root: $ROOT/public"
echo "  release path: https://$DOMAIN/releases/<channel>/<version>/..."
echo "  channel manifest: https://$DOMAIN/channels/<channel>/latest.json"
