#!/usr/bin/env bash
set -euo pipefail

DOMAIN="${DOMAIN:-doodleray.clickflare.click}"
ROOT="${ROOT:-/srv/doodleray-downloads}"
UPSTREAM="${UPSTREAM:-}"
WEB_SERVER="${WEB_SERVER:-auto}"
CADDY_SNIPPET="${CADDY_SNIPPET:-/etc/caddy/conf.d/doodleray-downloads.caddy}"
CADDYFILE="${CADDYFILE:-/etc/caddy/Caddyfile}"
NGINX_SITE="${NGINX_SITE:-/etc/nginx/sites-available/doodleray-clickflare-click}"
NGINX_ENABLED="${NGINX_ENABLED:-/etc/nginx/sites-enabled/doodleray-clickflare-click}"

if [[ "$(id -u)" != "0" ]]; then
  echo "Run as root on the downloads host." >&2
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

write_caddy() {
  install -d -m 0755 "$(dirname "$CADDY_SNIPPET")"
  if [[ -n "$UPSTREAM" ]]; then
    cat > "$CADDY_SNIPPET" <<EOF
$DOMAIN {
  encode zstd gzip

  reverse_proxy $UPSTREAM

  header {
    X-Content-Type-Options "nosniff"
    Referrer-Policy "no-referrer"
    Permissions-Policy "interest-cohort=()"
  }
}
EOF
  else
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
  fi

  if ! grep -q 'import /etc/caddy/conf.d/\*.caddy' "$CADDYFILE"; then
    cp "$CADDYFILE" "$CADDYFILE.doodleray-downloads.$(date -u +%Y%m%dT%H%M%SZ).bak"
    printf '\n# DoodleRay managed vhosts\nimport /etc/caddy/conf.d/*.caddy\n' >> "$CADDYFILE"
  fi

  caddy validate --config "$CADDYFILE"
  systemctl reload caddy
}

write_nginx() {
  install -d -m 0755 "$(dirname "$NGINX_SITE")" "$(dirname "$NGINX_ENABLED")"
  if [[ -e "$NGINX_SITE" ]]; then
    cp "$NGINX_SITE" "$NGINX_SITE.doodleray-downloads.$(date -u +%Y%m%dT%H%M%SZ).bak"
  fi

  if [[ -n "$UPSTREAM" ]]; then
    cat > "$NGINX_SITE" <<EOF
server {
    listen 80;
    listen [::]:80;
    server_name $DOMAIN;

    client_max_body_size 0;

    location / {
        proxy_http_version 1.1;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
        proxy_read_timeout 300s;
        proxy_send_timeout 300s;
        proxy_buffering off;
        proxy_pass $UPSTREAM;
    }
}
EOF
  else
    cat > "$NGINX_SITE" <<EOF
server {
    listen 80;
    listen [::]:80;
    server_name $DOMAIN;

    root $ROOT/public;
    index index.html;

    client_max_body_size 0;
    add_header X-Content-Type-Options "nosniff" always;
    add_header Referrer-Policy "no-referrer" always;
    add_header Permissions-Policy "interest-cohort=()" always;

    location ~ (^|/)\\.git {
        return 404;
    }

    location /releases/ {
        add_header Cache-Control "public, max-age=31536000, immutable" always;
        try_files \$uri =404;
    }

    location /channels/ {
        add_header Cache-Control "no-cache, must-revalidate" always;
        try_files \$uri =404;
    }

    location / {
        try_files \$uri \$uri/ =404;
    }
}
EOF
  fi

  ln -sfn "$NGINX_SITE" "$NGINX_ENABLED"
  nginx -t
  systemctl reload nginx

  if command -v certbot >/dev/null 2>&1; then
    certbot --nginx -d "$DOMAIN" --non-interactive --agree-tos \
      -m "${LETSENCRYPT_EMAIL:-doodlerayhelp@hotmail.com}" --redirect --keep-until-expiring || {
        echo "certbot failed; HTTP route is still active." >&2
      }
    nginx -t
    systemctl reload nginx
  else
    echo "certbot not found; HTTP route is active but HTTPS was not configured." >&2
  fi
}

if [[ "$WEB_SERVER" == "auto" ]]; then
  if command -v nginx >/dev/null 2>&1 && [[ -d /etc/nginx ]]; then
    WEB_SERVER="nginx"
  elif command -v caddy >/dev/null 2>&1 && [[ -f "$CADDYFILE" ]]; then
    WEB_SERVER="caddy"
  else
    echo "Neither nginx nor caddy was found." >&2
    exit 1
  fi
fi

case "$WEB_SERVER" in
  nginx) write_nginx ;;
  caddy) write_caddy ;;
  *)
    echo "Unsupported WEB_SERVER=$WEB_SERVER. Use auto, nginx, or caddy." >&2
    exit 1
    ;;
esac

echo "Downloads host ready:"
echo "  https://$DOMAIN/"
echo "  root: $ROOT/public"
if [[ -n "$UPSTREAM" ]]; then
  echo "  upstream: $UPSTREAM"
fi
echo "  release path: https://$DOMAIN/releases/<channel>/<version>/..."
echo "  channel manifest: https://$DOMAIN/channels/<channel>/latest.json"
