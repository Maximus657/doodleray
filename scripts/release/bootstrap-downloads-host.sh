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
WINDOWS_EXE_REDIRECT="${WINDOWS_EXE_REDIRECT:-}"

if [[ "$(id -u)" != "0" ]]; then
  echo "Run as root on the downloads host." >&2
  exit 1
fi

install -d -m 0755 "$ROOT/public/assets"
install -d -m 0755 "$ROOT/public/releases/direct" "$ROOT/public/releases/store-win32"
install -d -m 0755 "$ROOT/public/channels/direct" "$ROOT/public/channels/store-win32"
install -d -m 0755 "$ROOT/public/download/windows"
install -d -m 0750 "$ROOT/staging"

cat > "$ROOT/public/index.html" <<EOF
<!doctype html>
<html lang="ru">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>DoodleRay Downloads</title>
    <meta name="description" content="Скачать DoodleRay VPN для Windows с официального хоста загрузок.">
    <style>
      :root {
        color-scheme: dark;
        --bg: #17090f;
        --panel: rgba(255,255,255,.075);
        --border: rgba(255,255,255,.14);
        --text: #fff7f2;
        --muted: rgba(255,247,242,.68);
        --accent: #ff7a2f;
      }
      * { box-sizing: border-box; }
      body {
        margin: 0;
        min-height: 100vh;
        display: grid;
        place-items: center;
        padding: 32px;
        font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        background:
          radial-gradient(circle at 25% 10%, rgba(255,122,47,.24), transparent 34%),
          radial-gradient(circle at 82% 80%, rgba(255,59,114,.18), transparent 38%),
          var(--bg);
        color: var(--text);
      }
      main {
        width: min(720px, 100%);
        padding: 42px;
        border: 1px solid var(--border);
        border-radius: 28px;
        background: linear-gradient(145deg, rgba(255,255,255,.11), rgba(255,255,255,.04));
        box-shadow: 0 30px 90px rgba(0,0,0,.36);
      }
      .brand {
        display: flex;
        align-items: center;
        gap: 14px;
        margin-bottom: 28px;
        font-weight: 800;
        font-size: 24px;
        letter-spacing: -.02em;
      }
      .mark {
        width: 48px;
        height: 48px;
        display: grid;
        place-items: center;
        border-radius: 14px;
        background: linear-gradient(135deg, #ffb000, #ff6724);
        color: #210b05;
        font-weight: 900;
        overflow: hidden;
        box-shadow: 0 12px 30px rgba(255, 122, 47, .24);
      }
      .mark img {
        display: block;
        width: 100%;
        height: 100%;
        object-fit: cover;
      }
      .mark span { display: none; }
      .mark--fallback span { display: block; }
      h1 {
        margin: 0 0 12px;
        font-size: clamp(34px, 7vw, 58px);
        line-height: .95;
        letter-spacing: -.05em;
      }
      p {
        margin: 0;
        color: var(--muted);
        font-size: 18px;
        line-height: 1.55;
      }
      .actions {
        display: flex;
        flex-wrap: wrap;
        gap: 12px;
        margin-top: 32px;
      }
      a.button {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        min-height: 54px;
        padding: 0 22px;
        border-radius: 16px;
        color: #190904;
        background: linear-gradient(135deg, #ff9d45, var(--accent));
        text-decoration: none;
        font-weight: 800;
      }
      a.secondary {
        color: var(--text);
        background: var(--panel);
        border: 1px solid var(--border);
      }
      .note {
        margin-top: 22px;
        font-size: 14px;
      }
      .release-history {
        margin-top: 34px;
        padding-top: 28px;
        border-top: 1px solid var(--border);
      }
      .section-title {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 16px;
        margin-bottom: 16px;
        color: var(--text);
        font-size: 18px;
        font-weight: 900;
      }
      .release-item {
        padding: 18px;
        border: 1px solid rgba(255,122,47,.30);
        border-radius: 18px;
        background: rgba(255,122,47,.08);
      }
      .release-item h3 {
        margin: 0 0 6px;
        color: var(--text);
        font-size: 18px;
      }
      .release-item p {
        margin: 0;
        font-size: 15px;
      }
    </style>
  </head>
  <body>
    <main>
      <div class="brand"><div class="mark"><img src="/assets/doodleray-logo.png" alt="" onerror="this.remove();this.parentElement.classList.add('mark--fallback');"><span>DR</span></div><span>DoodleRay VPN</span></div>
      <h1>Скачать для Windows</h1>
      <p>Официальный хост загрузок DoodleRay. Публичная кнопка скачивания включается только для версии, которая готова для пользователей.</p>
      <div class="actions">
        <a class="button" href="/download/windows/">Скачать DoodleRay для Windows</a>
        <a class="button secondary" href="#versions">Что изменилось</a>
      </div>
      <p class="note">Если скачивание пока недоступно, значит новая публичная версия ещё не опубликована.</p>
      <section id="versions" class="release-history">
        <div class="section-title"><span>История версий</span></div>
        <article class="release-item">
          <h3>Скоро появится после публикации релиза</h3>
          <p>Каждый публичный релиз DoodleRay будет публиковаться с коротким и понятным списком изменений.</p>
        </article>
      </section>
    </main>
  </body>
</html>
EOF

cat > "$ROOT/public/download/windows/index.html" <<EOF
<!doctype html>
<html lang="ru">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Скачать DoodleRay для Windows</title>
    <style>
      body {
        min-height: 100vh;
        margin: 0;
        display: grid;
        place-items: center;
        font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        background: #17090f;
        color: #fff7f2;
      }
      main {
        width: min(560px, calc(100vw - 40px));
        padding: 32px;
        border: 1px solid rgba(255,255,255,.14);
        border-radius: 22px;
        background: rgba(255,255,255,.075);
      }
      a { color: #ff9d45; font-weight: 800; }
    </style>
  </head>
  <body>
    <main>
      <h1>Скачивание готовится</h1>
      <p>Публичная ссылка для Windows ещё не подключена на этом хосте. Попробуйте позже или используйте текущий публичный релиз.</p>
    </main>
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
    download_redirect_block=""
    if [[ -n "$WINDOWS_EXE_REDIRECT" ]]; then
      download_redirect_block=$(cat <<EOF
    location = /download/windows/latest.exe {
        return 302 $WINDOWS_EXE_REDIRECT;
    }

EOF
)
    fi

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

$download_redirect_block
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
