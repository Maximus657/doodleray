## Жесткий архитектурный вердикт

Для Windows 10/11 в DoodleRay должен остаться **один production-путь для массового пользователя**:

```text
Proxy Mode / Browser & apps proxy:
  Windows per-user WinINet / Internet Settings
  ProxyEnable  = 1
  ProxyServer  = 127.0.0.1:10809
  ProxyOverride = минимальный локальный bypass
  AutoDetect / AutoConfigURL временно отключаются только на время подключения
  SOCKS в системный прокси Windows не пишется

TUN / Full Device:
  системный proxy Windows не трогается вообще
```

То есть **не чинить текущую строку до `socks=127.0.0.1:10808`, а убрать `http=...;https=...;socks=...` из обычного Windows-пути полностью**. DoodleRay уже имеет HTTP proxy на `127.0.0.1:10809`; именно его и надо ставить как простой Windows proxy. SOCKS `127.0.0.1:10808` оставить только для advanced/manual use: “настройте конкретное приложение вручную”.

Причина: protocol-specific строка допустима на уровне части WinINet/Chromium-совместимых клиентов, но Windows 11 Settings UI официально предлагает пользователю модель “Proxy IP address” + “Port”, а не per-protocol editor. Поэтому строка вида `http=127.0.0.1:10809;https=...;socks=...` превращается в UX-ловушку: формально не обязательно invalid, но в Settings выглядит как мусор в поле IP. Microsoft Support для Windows 10/11 описывает manual proxy именно как имя/IP proxy-сервера и порт, плюс exception list через `;`; NetworkProxy CSP также описывает static proxy address как `<server>[:<port>]`. ([Microsoft Support][1])

---

## Что является фактом, а что инженерным выводом

**Факт:** Windows/WinINet поддерживает явную настройку proxy-сервера, bypass list и auto-config через WinINet per-connection options; `INTERNET_PER_CONN_PROXY_SERVER`, `INTERNET_PER_CONN_PROXY_BYPASS`, `INTERNET_PER_CONN_AUTOCONFIG_URL`, flags `PROXY_TYPE_PROXY`, `PROXY_TYPE_AUTO_PROXY_URL`, `PROXY_TYPE_AUTO_DETECT` — документированные опции. После изменения надо уведомлять WinINet: refresh/settings/proxy-settings-changed. ([Microsoft Learn][2])

**Факт:** protocol-specific proxy mapping существует как практика: WinINet-документация описывает формат `<protocol>=...` для отдельных протоколов, Edge/Chromium/Electron поддерживают proxy rules вида `<scheme>=<uri>[:port][;...]`, а Chromium прямо описывает manual proxy map. ([Microsoft Learn][3])

**Факт:** Edge использует системные network settings по умолчанию; Tauri на Windows использует WebView2, основанный на Microsoft Edge/Chromium; Firefox может работать в режиме “Use system proxy settings”. Поэтому простой Windows proxy реально покрывает основной browser/webview-класс приложений. ([Microsoft Learn][4])

**Инженерный вывод:** protocol-specific строка — плохой default для DoodleRay на Windows 11. Я не нашел официального Microsoft-документа, где было бы написано “Windows 11 Settings некрасиво отображает `http=...;https=...;socks=...`”. Но официальная Windows 11 Settings-модель — single address + port, а ваша пользовательская жалоба ровно подтверждает этот UX-провал. Это надо считать production bug, даже если WinINet/Chromium часть такой строки принимает.

---

## Почему именно `ProxyServer = 127.0.0.1:10809`

HTTP proxy достаточно для массового Windows system proxy path. Для HTTPS браузеры используют HTTP proxy через `CONNECT`; Chromium также описывает HTTP proxy как proxy, который может использоваться для `http://`, `https://`, `ws://`, `wss://`. ([Chromium GooglSource][5])

SOCKS в системном Windows proxy DoodleRay не нужен по умолчанию. SOCKS полезен как manual endpoint для приложений, где пользователь сам выбирает SOCKS5. Но Windows Settings UI не является хорошей SOCKS-first моделью, а `socks=` в `ProxyServer` создает ровно ту проблему, которую видит пользователь: непонятная multi-protocol строка в поле адреса.

Итоговый write-path:

```text
ProxyEnable   = 1
ProxyServer   = "127.0.0.1:10809"
ProxyOverride = "<local>;localhost;127.*;[::1];10.*;172.16.*;...;172.31.*;192.168.*;*.local"
```

Bypass list должен быть коротким и инфраструктурным. Уберите из `ProxyOverride` скрытые game domains. Windows proxy bypass означает “идти DIRECT, вне DoodleRay”; это split tunneling, а не системная proxy-гигиена. Если продукту нужны game bypass rules, они должны быть в routing engine DoodleRay и видимы пользователю.

---

## Почему не альтернативы

| Вариант                                      | Решение                                                                                                                                                                                                                                                                    |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `127.0.0.1:10809`                            | **Выбрать.** Совместимо с Windows Settings UI, просто объясняется, достаточно для HTTP/HTTPS/WebSocket browser traffic.                                                                                                                                                    |
| `http=127.0.0.1:10809;https=127.0.0.1:10809` | Валидно как per-protocol mapping, но всё еще плохо для Windows 11 Settings UI. Не использовать как default.                                                                                                                                                                |
| `http=...;https=...;socks=...`               | **Запретить в обычном пути.** Это текущий UX-баг. SOCKS оставить только для ручных настроек приложений.                                                                                                                                                                    |
| PAC file                                     | Не default. PAC полезен для сложного per-domain routing, но добавляет local PAC server, stale AutoConfigURL, конфликт с corporate PAC и еще один crash failure mode.                                                                                                       |
| `netsh winhttp set proxy`                    | Не трогать автоматически. WinHTTP — отдельная подсистема для WinHTTP clients/services; `netsh` может затронуть services и Microsoft прямо предупреждает, что это влияет на приложения/services с WinHTTP default proxy и плохо для roaming laptops. ([Microsoft Learn][6]) |
| “Вообще не трогать system proxy”             | Для TUN — да. Для Proxy Mode — нет, иначе обычный пользователь не получит browser/app routing без ручной настройки.                                                                                                                                                        |

---

## WinINet vs WinHTTP: что трогать

DoodleRay как desktop app должен управлять **per-user WinINet / Windows Internet Settings**, не WinHTTP. WinHTTP нужен службам и system-context компонентам; Microsoft отдельно описывает сценарий, где WinHTTP clients могут импортировать/читать WinINet settings, а службы требуют особой осторожности с user registry hive. ([Microsoft Learn][7])

`netsh winhttp` не должен быть частью consumer flow. Даже если современные `netsh winhttp set advproxy` умеют user/machine scope, это всё равно другой proxy-plane, не Windows Settings UI и не основной browser/WebView path. Для “весь компьютер, включая приложения, игнорирующие system proxy” у вас уже есть правильный механизм — TUN. ([Microsoft Learn][8])

---

## Что делать с AutoDetect и PAC

Перед apply надо сохранять и временно убирать:

```text
AutoDetect
AutoConfigURL
WinINet flags: PROXY_TYPE_AUTO_DETECT / PROXY_TYPE_AUTO_PROXY_URL
```

Причина: если у пользователя/корпорации уже включен PAC/WPAD, explicit proxy DoodleRay может не стать фактическим маршрутом для части клиентов. NetworkProxy CSP описывает порядок применения как auto-detect → setup script → proxy server → direct, поэтому в managed Proxy Mode DoodleRay должен временно заменить auto/PAC на explicit proxy и восстановить всё обратно на disconnect. ([Microsoft Learn][9])

Но UX должен честно предупреждать:

> “На этом компьютере уже настроен proxy/PAC вашей организации. В режиме Browser & apps DoodleRay временно заменит эти настройки и восстановит их после отключения. Чтобы не менять Windows proxy, используйте Whole computer / TUN.”

Если proxy settings заблокированы policy — не бороться с policy, не писать HKLM, не просить странные admin-действия. Показать: “Windows proxy is managed by your organization. Use Whole computer mode.”

---

## Cleanup: текущий `unset_system_proxy()` неправильный

Текущий disconnect:

```rust
key.set_value("ProxyEnable", &0u32)?;
```

недостаточен. Он оставляет stale `ProxyServer` и `ProxyOverride`, из-за чего Windows Settings потом показывает пользователю старую multi-protocol строку. Правильный disconnect — это не “выключить proxy”, а **restore previous state**.

Правило:

1. Если перед DoodleRay у пользователя не было `ProxyServer` — после disconnect удалить `ProxyServer`.
2. Если был корпоративный/manual `ProxyServer` — восстановить ровно его.
3. Если был `AutoConfigURL`/PAC — восстановить ровно его.
4. Если пользователь/другой VPN поменял proxy while connected — не перетирать чужую настройку, кроме случая, когда активный `ProxyServer` всё еще указывает на DoodleRay и иначе после остановки локального proxy сломается интернет.
5. Если старый DoodleRay оставил `ProxyEnable=0` + `ProxyServer=http=...;https=...;socks=...` — migration должна удалить stale DoodleRay values, если они точно принадлежат DoodleRay.

---

## Какие значения сохранять

Минимальный production snapshot:

```text
HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings:
  ProxyEnable
  ProxyServer
  ProxyOverride
  AutoConfigURL
  AutoDetect
  ProxyHttp1.1        // сохранить, но не менять, если не нужно

HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections:
  DefaultConnectionSettings
  SavedLegacySettings

WinINet semantic snapshot:
  INTERNET_PER_CONN_FLAGS / FLAGS_UI
  INTERNET_PER_CONN_PROXY_SERVER
  INTERNET_PER_CONN_PROXY_BYPASS
  INTERNET_PER_CONN_AUTOCONFIG_URL
  INTERNET_PER_CONN_AUTODISCOVERY_FLAGS
```

`DefaultConnectionSettings` и `SavedLegacySettings` не надо парсить. Их надо сохранять byte-for-byte и восстанавливать только если текущая proxy-конфигурация всё еще owned by DoodleRay. Исторически даже Firefox пришел к тому, что простое чтение `ProxyEnable/ProxyServer/ProxyOverride` недостаточно; для корректного “system proxy” на Windows надо использовать WinINet API, а не вручную полагаться только на registry text values. ([Bugzilla][10])

---

## Rust-план изменений

### 1. Заменить API

Сейчас:

```rust
set_system_proxy(http_port)
unset_system_proxy()
```

Должно стать:

```rust
capture_previous_proxy_state()
apply_doodleray_proxy()
restore_previous_proxy_state()
clear_only_if_owned_by_doodleray()
detect_stale_doodleray_proxy()
recover_orphaned_proxy_on_startup()
```

`unset_system_proxy()` как публичная операция должен исчезнуть или стать thin wrapper вокруг `restore_previous_proxy_state()`.

---

### 2. Структуры состояния

```rust
const INTERNET_SETTINGS: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

const CONNECTIONS_SUBKEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections";

const DOODLERAY_MARKER_KEY: &str =
    r"Software\DoodleRay\SystemProxy";

const DOODLERAY_HTTP_PORT: u16 = 10809;
const DOODLERAY_SOCKS_PORT: u16 = 10808;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProxyStateFile {
    pub schema_version: u32,
    pub owner_token: String,
    pub owner_pid: u32,
    pub captured_at_unix_ms: i64,
    pub app_version: String,
    pub previous: PreviousProxyState,
    pub applied: AppliedProxyState,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PreviousProxyState {
    pub internet_settings: Vec<RegValueSnapshot>,
    pub connections: Vec<RegValueSnapshot>,
    pub wininet: Option<WinInetSnapshot>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppliedProxyState {
    pub proxy_server: String,   // always "127.0.0.1:10809"
    pub proxy_override: String,
    pub disabled_auto_detect: bool,
    pub cleared_auto_config_url: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RegValueSnapshot {
    pub name: String,
    pub present: bool,
    pub value: Option<RegData>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum RegData {
    Dword(u32),
    Sz(String),
    Binary(Vec<u8>),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WinInetSnapshot {
    pub flags: Option<u32>,
    pub flags_ui: Option<u32>,
    pub proxy_server: Option<String>,
    pub proxy_bypass: Option<String>,
    pub auto_config_url: Option<String>,
    pub autodiscovery_flags: Option<u32>,
}
```

---

### 3. `capture_previous_proxy_state()`

```rust
pub fn capture_previous_proxy_state(app_version: &str) -> Result<ProxyStateFile, String> {
    acquire_system_proxy_mutex()?;

    // Important: do not capture DoodleRay-over-DoodleRay.
    // If previous active state file exists, recover/restore first or continue same session.
    recover_orphaned_proxy_on_startup()?;

    ensure_proxy_settings_not_policy_locked()?;

    let owner_token = uuid::Uuid::new_v4().to_string();
    let owner_pid = std::process::id();

    let internet_values = snapshot_reg_values(
        HKEY_CURRENT_USER,
        INTERNET_SETTINGS,
        &[
            ("ProxyEnable", RegKind::Dword),
            ("ProxyServer", RegKind::Sz),
            ("ProxyOverride", RegKind::Sz),
            ("AutoConfigURL", RegKind::Sz),
            ("AutoDetect", RegKind::Dword),
            ("ProxyHttp1.1", RegKind::Dword),
        ],
    )?;

    let connection_values = snapshot_reg_values(
        HKEY_CURRENT_USER,
        CONNECTIONS_SUBKEY,
        &[
            ("DefaultConnectionSettings", RegKind::Binary),
            ("SavedLegacySettings", RegKind::Binary),
        ],
    ).unwrap_or_default();

    let wininet = query_wininet_default_connection().ok();

    let state = ProxyStateFile {
        schema_version: 1,
        owner_token,
        owner_pid,
        captured_at_unix_ms: now_unix_ms(),
        app_version: app_version.to_string(),
        previous: PreviousProxyState {
            internet_settings: internet_values,
            connections: connection_values,
            wininet,
        },
        applied: AppliedProxyState {
            proxy_server: "127.0.0.1:10809".to_string(),
            proxy_override: default_proxy_override(),
            disabled_auto_detect: true,
            cleared_auto_config_url: true,
        },
    };

    // Atomic write BEFORE touching Windows proxy.
    atomic_write_json(proxy_state_path(), &state)?;

    Ok(state)
}
```

---

### 4. `apply_doodleray_proxy()`

```rust
pub fn apply_doodleray_proxy(app_version: &str, http_port: u16) -> Result<(), String> {
    if http_port != DOODLERAY_HTTP_PORT {
        return Err("unexpected Windows system proxy HTTP port".into());
    }

    // Never point Windows at a dead local proxy.
    ensure_loopback_port_accepting("127.0.0.1", http_port)?;

    let state = capture_previous_proxy_state(app_version)?;

    let proxy_server = format!("127.0.0.1:{http_port}");
    let proxy_override = default_proxy_override();

    // Preferred semantic path: WinINet per-connection API.
    // Set explicit proxy only; do not include AUTO_DETECT or AUTO_PROXY_URL while connected.
    set_wininet_default_connection_manual_proxy(
        &proxy_server,
        &proxy_override,
        /* disable_auto_detect */ true,
        /* clear_auto_config_url */ true,
    )?;

    // Registry normalization for Windows Settings UI and legacy readers.
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(INTERNET_SETTINGS).map_err(to_string)?;

    key.set_value("ProxyEnable", &1u32).map_err(to_string)?;
    key.set_value("ProxyServer", &proxy_server).map_err(to_string)?;
    key.set_value("ProxyOverride", &proxy_override).map_err(to_string)?;

    // Temporarily disable auto proxy mechanisms; restore from snapshot later.
    key.set_value("AutoDetect", &0u32).ok();
    key.delete_value("AutoConfigURL").ok();

    write_doodleray_marker(&state)?;

    notify_proxy_change();

    Ok(())
}
```

`default_proxy_override()`:

```rust
fn default_proxy_override() -> String {
    [
        "<local>",
        "localhost",
        "127.*",
        "[::1]",
        "10.*",
        "172.16.*", "172.17.*", "172.18.*", "172.19.*",
        "172.20.*", "172.21.*", "172.22.*", "172.23.*",
        "172.24.*", "172.25.*", "172.26.*", "172.27.*",
        "172.28.*", "172.29.*", "172.30.*", "172.31.*",
        "192.168.*",
        "*.local",
    ].join(";")
}
```

WinINet bypass list использует `;`, а `<local>` означает bypass для hostnames без точки; WinINet также по умолчанию bypass-ит loopback/local addresses, но явные значения полезны для UI-предсказуемости. ([Microsoft Learn][3])

---

### 5. `restore_previous_proxy_state()`

```rust
pub enum RestoreOutcome {
    Restored,
    SkippedBecauseChangedByUserOrOtherApp,
    CleanedDisabledStaleDoodleRayProxy,
    NoState,
}

pub fn restore_previous_proxy_state() -> Result<RestoreOutcome, String> {
    acquire_system_proxy_mutex()?;

    let Some(state) = load_proxy_state_file()? else {
        return clear_only_if_owned_by_doodleray();
    };

    let current = read_current_proxy_state()?;

    // If another proxy client replaced DoodleRay proxy, do not overwrite it.
    if !current_critical_proxy_is_doodleray(&current, &state.applied) {
        remove_doodleray_marker_if_matches(&state.owner_token).ok();
        archive_state_file_for_diagnostics().ok();
        notify_proxy_change();
        return Ok(RestoreOutcome::SkippedBecauseChangedByUserOrOtherApp);
    }

    // Restore semantic WinINet state first.
    if let Some(snapshot) = &state.previous.wininet {
        restore_wininet_default_connection(snapshot).ok();
    }

    // Restore exact registry values after semantic restore.
    restore_reg_values(
        HKEY_CURRENT_USER,
        INTERNET_SETTINGS,
        &state.previous.internet_settings,
    )?;

    restore_reg_values(
        HKEY_CURRENT_USER,
        CONNECTIONS_SUBKEY,
        &state.previous.connections,
    ).ok();

    remove_doodleray_marker_if_matches(&state.owner_token).ok();
    delete_proxy_state_file().ok();

    notify_proxy_change();

    Ok(RestoreOutcome::Restored)
}
```

Ключевой момент: restore разрешен, только если критическое поле `ProxyServer` всё еще указывает на DoodleRay. Если другой VPN/proxy-клиент уже поставил свой proxy, DoodleRay не имеет права возвращать старое состояние поверх него.

---

### 6. `detect_stale_doodleray_proxy()`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleDoodleRayProxy {
    None,
    DisabledStaleValues,
    EnabledButLocalProxyDead,
    EnabledAndProbablyCurrentDoodleRay,
}

pub fn detect_stale_doodleray_proxy() -> Result<StaleDoodleRayProxy, String> {
    let current = read_current_proxy_state()?;

    let Some(proxy_server) = current.proxy_server.as_deref() else {
        return Ok(StaleDoodleRayProxy::None);
    };

    if !proxy_server_looks_like_doodleray(proxy_server) {
        return Ok(StaleDoodleRayProxy::None);
    }

    let has_marker = read_doodleray_marker().is_ok();
    let has_legacy_multi = proxy_server.contains("http=")
        || proxy_server.contains("https=")
        || proxy_server.contains("socks=");

    let has_old_bypass = current.proxy_override
        .as_deref()
        .map(looks_like_old_doodleray_bypass)
        .unwrap_or(false);

    // Avoid deleting Clash/Fiddler/other localhost proxy configs.
    if !has_marker && !has_legacy_multi && !has_old_bypass {
        return Ok(StaleDoodleRayProxy::None);
    }

    if current.proxy_enable == Some(0) {
        return Ok(StaleDoodleRayProxy::DisabledStaleValues);
    }

    if current.proxy_enable == Some(1)
        && !is_loopback_port_accepting("127.0.0.1", DOODLERAY_HTTP_PORT)
    {
        return Ok(StaleDoodleRayProxy::EnabledButLocalProxyDead);
    }

    Ok(StaleDoodleRayProxy::EnabledAndProbablyCurrentDoodleRay)
}

fn proxy_server_looks_like_doodleray(value: &str) -> bool {
    // Accept both new and old broken formats:
    //   127.0.0.1:10809
    //   http=127.0.0.1:10809;https=127.0.0.1:10809;socks=127.0.0.1:10808
    //   old buggy socks=127.0.0.1:10809
    //
    // Do NOT match arbitrary 127.0.0.1:* to avoid stealing other proxy clients.
    let normalized = value.to_ascii_lowercase().replace(' ', "");

    let known_ports = [":10809", ":10808"];
    let loopback_hosts = ["127.0.0.1", "localhost", "[::1]"];

    let contains_known_loopback = loopback_hosts.iter().any(|h| normalized.contains(h))
        && known_ports.iter().any(|p| normalized.contains(p));

    let contains_only_known_schemes =
        normalized.split(';').all(|part| {
            part.starts_with("http=")
                || part.starts_with("https=")
                || part.starts_with("socks=")
                || part.starts_with("127.0.0.1:")
                || part.starts_with("localhost:")
                || part.starts_with("[::1]:")
        });

    contains_known_loopback && contains_only_known_schemes
}
```

---

### 7. `clear_only_if_owned_by_doodleray()`

```rust
pub fn clear_only_if_owned_by_doodleray() -> Result<RestoreOutcome, String> {
    match detect_stale_doodleray_proxy()? {
        StaleDoodleRayProxy::None => Ok(RestoreOutcome::NoState),

        StaleDoodleRayProxy::DisabledStaleValues => {
            // Safe migration case:
            // ProxyEnable = 0, but old DoodleRay ProxyServer/Override pollute Windows Settings UI.
            delete_doodleray_proxy_server_if_owned()?;
            delete_doodleray_proxy_override_if_owned()?;
            remove_doodleray_marker().ok();
            notify_proxy_change();
            Ok(RestoreOutcome::CleanedDisabledStaleDoodleRayProxy)
        }

        StaleDoodleRayProxy::EnabledButLocalProxyDead => {
            // Safety-first: active Windows proxy points to dead local DoodleRay port.
            // If snapshot exists, restore it. If no snapshot, disable only DoodleRay-owned values.
            if load_proxy_state_file()?.is_some() {
                return restore_previous_proxy_state();
            }

            let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
            let (key, _) = hkcu.create_subkey(INTERNET_SETTINGS).map_err(to_string)?;

            key.set_value("ProxyEnable", &0u32).map_err(to_string)?;
            delete_doodleray_proxy_server_if_owned()?;
            delete_doodleray_proxy_override_if_owned()?;

            remove_doodleray_marker().ok();
            notify_proxy_change();

            Ok(RestoreOutcome::CleanedDisabledStaleDoodleRayProxy)
        }

        StaleDoodleRayProxy::EnabledAndProbablyCurrentDoodleRay => {
            // Do not clear an active, working current session.
            Ok(RestoreOutcome::NoState)
        }
    }
}
```

---

### 8. `notify_proxy_change()`

```rust
pub fn notify_proxy_change() {
    unsafe {
        // Keep existing calls.
        InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_SETTINGS_CHANGED,
            std::ptr::null_mut(),
            0,
        );

        // Add explicit proxy-settings-changed notification.
        InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_PROXY_SETTINGS_CHANGED,
            std::ptr::null_mut(),
            0,
        );

        InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_REFRESH,
            std::ptr::null_mut(),
            0,
        );
    }
}
```

---

## Crash-safe restore strategy

Нельзя честно гарантировать cleanup после hard crash, если единственный owner proxy-состояния — тот же процесс, который упал. Поэтому production-архитектура должна быть такой:

1. **Proxy state owner — Rust networking backend/service, не React UI.**
2. Если backend может падать вместе с UI, добавить маленький per-user guardian process:

   * получает `owner_pid`, `owner_token`, path к snapshot;
   * мониторит heartbeat/named pipe;
   * проверяет, что `127.0.0.1:10809` жив;
   * если owner умер или port умер, а Windows proxy still DoodleRay-owned — вызывает restore.
3. На startup DoodleRay всегда вызывает `recover_orphaned_proxy_on_startup()`.
4. Перед update/restart: restore first, then update, then reconnect if auto-connect enabled.
5. Перед apply: сначала поднять local HTTP inbound, потом менять Windows proxy. Никогда наоборот.
6. Использовать per-user named mutex, чтобы две копии DoodleRay не могли одновременно capture/apply/restore.

Это та же логика, которую Microsoft Dev Proxy явно учитывает: system proxy registration удобен, но может конфликтовать с corporate proxy, другими proxy instances и background/system traffic; у них даже documented option позволяет отключать automatic system proxy registration. ([Microsoft Learn][11])

---

## Migration для уже сломанных пользователей

На первом запуске новой версии:

1. Прочитать `ProxyEnable`, `ProxyServer`, `ProxyOverride`.
2. Если `ProxyServer` равен одному из legacy DoodleRay formats:

   ```text
   http=127.0.0.1:10809;https=127.0.0.1:10809;socks=127.0.0.1:10808
   http=127.0.0.1:10809;https=127.0.0.1:10809;socks=127.0.0.1:10809
   127.0.0.1:10809
   ```

   и есть DoodleRay marker / legacy bypass list / известные ports — считать owned.
3. Если `ProxyEnable=0`, удалить только owned `ProxyServer`/`ProxyOverride`. Это чинит Windows 11 Settings UI без изменения реального интернет-состояния.
4. Если `ProxyEnable=1`, но `127.0.0.1:10809` не слушает — восстановить snapshot, если есть; если snapshot нет, отключить proxy и удалить owned values.
5. Если `ProxyEnable=1` и local proxy жив — при следующем connect переписать active system proxy в новый simple format `127.0.0.1:10809`.

Никаких broad rules вроде “любой `127.0.0.1:*` принадлежит DoodleRay” — нельзя. Это может быть Fiddler, Clash, mitmproxy, corporate agent или другой VPN/proxy client.

---

## Новый UI/UX

Уберите `set / unchanged / clear` из обычного UI. Это implementation terms, не пользовательские режимы.

Главный выбор:

```text
Whole computer (recommended)
Routes most apps through DoodleRay using Full Device / TUN.
Does not change Windows proxy settings.

Browser & proxy-aware apps
Turns on Windows proxy while connected.
Works with browsers and apps that follow Windows proxy.
Some apps and games may ignore it.

Manual proxy
Starts local proxies only.
HTTP: 127.0.0.1:10809
SOCKS5: 127.0.0.1:10808
Windows settings are not changed.
```

Default:

```text
Если TUN доступен: Whole computer / TUN = recommended default.
Если пользователь выбирает Proxy Mode: Manage Windows proxy while connected = ON.
В TUN mode: system proxy всегда unchanged.
```

Advanced:

```text
[ ] Leave Windows proxy unchanged in Browser & apps mode
    For users who configure apps manually.

[Repair Windows proxy]
    Shows only when stale DoodleRay proxy is detected.

Read-only diagnostics:
    Windows proxy: 127.0.0.1:10809, managed by DoodleRay
    Previous proxy: saved, will be restored on disconnect
```

Status messages:

```text
Connected:
  Windows proxy is managed by DoodleRay: 127.0.0.1:10809

Disconnected:
  Windows proxy restored

Changed by another app:
  Windows proxy was changed while DoodleRay was connected.
  DoodleRay left the new setting unchanged.

Corporate/PAC detected:
  Existing Windows proxy/PAC will be temporarily replaced in Browser & apps mode.
  Use Whole computer mode to avoid changing Windows proxy.
```

`clear` как пользовательская настройка удалить. Максимум — support action: “Repair old DoodleRay proxy settings”, который работает только через ownership detection.

---

## Edge cases и политика поведения

| Edge case                                       | Поведение                                                                                                                                                                             |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| App crash while proxy enabled                   | Guardian/service restores if current proxy is DoodleRay-owned. Startup recovery repeats restore if needed.                                                                            |
| Proxy process dies, UI жив                      | Немедленно restore; не оставлять Windows pointing to dead `127.0.0.1:10809`.                                                                                                          |
| User edits Windows proxy while connected        | Если `ProxyServer` заменен не-DoodleRay значением — DoodleRay не overwrites. Если `ProxyServer` всё еще DoodleRay — DoodleRay restores/clears on disconnect to avoid broken internet. |
| Another VPN/proxy client changes proxy          | Не восстанавливать поверх него. Снять marker, показать warning.                                                                                                                       |
| Corporate proxy/PAC before DoodleRay            | Capture exact state; temporarily replace only in Proxy Mode; restore exact state. Recommend TUN.                                                                                      |
| Proxy settings locked by policy                 | Не писать. Disable Browser & apps system proxy management; recommend TUN/manual.                                                                                                      |
| System proxy enabled before DoodleRay           | Capture and restore exactly.                                                                                                                                                          |
| Stale old DoodleRay string with `ProxyEnable=0` | Silent migration cleanup: delete owned stale values.                                                                                                                                  |
| Old bug `socks=127.0.0.1:10809`                 | Detect as legacy DoodleRay-owned pattern.                                                                                                                                             |
| TUN mode                                        | No registry writes except optional safe stale repair on startup.                                                                                                                      |

---

## Тест-план

### Registry/API tests

1. `ProxyServer` absent before connect → after disconnect absent again.
2. `ProxyEnable=0`, but user had `ProxyServer=corp:8080` hidden → restore exactly.
3. Existing corporate proxy:

   ```text
   ProxyEnable=1
   ProxyServer=corp.example.com:8080
   ProxyOverride=<local>;*.corp
   ```

   Apply DoodleRay → restore exact values.
4. Existing PAC:

   ```text
   AutoDetect=1
   AutoConfigURL=http://pac.corp/proxy.pac
   ```

   Apply disables auto/PAC temporarily → restore exact PAC.
5. New apply writes exactly:

   ```text
   ProxyServer=127.0.0.1:10809
   ```

   and never writes `http=`, `https=`, `socks=`.
6. `DefaultConnectionSettings`/`SavedLegacySettings` are captured/restored byte-for-byte under ownership.
7. `notify_proxy_change()` is called after apply, restore, migration cleanup.
8. Policy-locked settings return controlled error and do not partially write.

### Windows 10/11 manual UI verification

1. Windows 11 Settings → Proxy shows:

   ```text
   Proxy IP address: 127.0.0.1
   Port: 10809
   ```

   not multi-protocol text.
2. Disconnect clears fields if previous state was empty.
3. Existing corporate/PAC settings return exactly.
4. Legacy stale:

   ```text
   ProxyEnable=0
   ProxyServer=http=127.0.0.1:10809;https=...;socks=...
   ```

   is cleaned on upgrade.

### Browser/app tests

1. Edge, Chrome, WebView2/Tauri webview traffic uses DoodleRay in Proxy Mode.
2. Firefox with “Use system proxy settings” uses DoodleRay; Firefox with manual/no proxy does not.
3. Electron app in default/system mode uses DoodleRay; Electron app with custom proxy does not.
4. UWP/Store apps: test, but do not promise universal coverage. Microsoft documents automatic/transparent proxy support for some UWP networking APIs, but app behavior varies by API and app configuration. ([Microsoft Learn][12])
5. Apps that ignore Windows proxy must be routed only by TUN.

### Crash/recovery tests

1. Kill local proxy child → restore.
2. Kill UI only while backend lives → no restore unless connection stops.
3. Kill backend hard → guardian restores.
4. Reboot with stale state file → startup recovery restores/cleans.
5. Crash between snapshot write and registry apply → no harmful restore.
6. Crash after registry apply before marker write → state file still allows recovery.

### Race/ownership tests

1. User changes `ProxyServer` to another proxy while connected → DoodleRay does not overwrite on disconnect.
2. User changes only bypass list while `ProxyServer` remains DoodleRay → DoodleRay removes/restores DoodleRay proxy on disconnect to avoid dead local proxy.
3. Another VPN/proxy app starts after DoodleRay → DoodleRay does not restore over it.
4. Two DoodleRay instances → named mutex prevents double capture/apply.

### TUN no-regression

1. TUN connect/disconnect does not touch:

   ```text
   ProxyEnable
   ProxyServer
   ProxyOverride
   AutoConfigURL
   AutoDetect
   Connections\DefaultConnectionSettings
   ```
2. TUN can still trigger safe startup repair only for clearly stale DoodleRay-owned values.

---

## Главные риски и снижение

**Риск:** direct registry writes не полностью синхронизируются с WinINet/Windows UI.
**Снижение:** использовать WinINet `INTERNET_OPTION_PER_CONNECTION_OPTION` как semantic path, registry — только для snapshot/normalization/repair. Microsoft документирует per-connection options именно для set/query proxy options. ([Microsoft Learn][2])

**Риск:** сломать corporate proxy/PAC.
**Снижение:** capture exact previous state, restore only if current critical proxy is still DoodleRay-owned, detect policy lock, recommend TUN.

**Риск:** оставить Windows pointing to dead `127.0.0.1:10809`.
**Снижение:** apply only after local HTTP proxy is listening; guardian/service restores on crash; startup recovery cleans stale owned values.

**Риск:** переписать proxy другого клиента.
**Снижение:** ownership token + exact `ProxyServer` match + legacy DoodleRay pattern detection. Никогда не считать любой loopback proxy “нашим”.

**Риск:** пользователи не понимают proxy behavior.
**Снижение:** UI должен говорить режимами продукта: “Whole computer” vs “Browser & proxy-aware apps” vs “Manual proxy”, а не `set/unchanged/clear`.

---

## Финальное решение в одну строку

**DoodleRay на Windows должен управлять только per-user WinINet system proxy в simple HTTP формате `127.0.0.1:10809`, только в Proxy Mode, только while connected, с обязательным snapshot/restore/ownership/guardian; SOCKS, PAC и WinHTTP не использовать в consumer default.**

[1]: https://support.microsoft.com/en-us/windows/use-a-proxy-server-in-windows-03096c53-0554-4ffe-b6ab-8b1deee8dae1 "Use a proxy server in Windows - Microsoft Support"
[2]: https://learn.microsoft.com/en-us/windows/win32/api/wininet/ns-wininet-internet_per_conn_optiona "https://learn.microsoft.com/en-us/windows/win32/api/wininet/ns-wininet-internet_per_conn_optiona"
[3]: https://learn.microsoft.com/en-us/windows/win32/wininet/enabling-internet-functionality "Enabling Internet Functionality - Win32 apps | Microsoft Learn"
[4]: https://learn.microsoft.com/en-us/deployedge/edge-learnmore-cmdline-options-proxy-settings "Microsoft Edge proxy settings | Microsoft Learn"
[5]: https://chromium.googlesource.com/chromium/src/%2B/HEAD/net/docs/proxy.md "Proxy support in Chrome"
[6]: https://learn.microsoft.com/en-us/purview/device-onboarding-configure-proxy "https://learn.microsoft.com/en-us/purview/device-onboarding-configure-proxy"
[7]: https://learn.microsoft.com/en-us/windows/win32/winhttp/setting-wininet-proxy-configurations-in-winhttp "Setting WinINet Proxy Configurations in WinHTTP - Win32 apps | Microsoft Learn"
[8]: https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/netsh-winhttp "netsh winhttp | Microsoft Learn"
[9]: https://learn.microsoft.com/en-us/windows/client-management/mdm/networkproxy-csp "https://learn.microsoft.com/en-us/windows/client-management/mdm/networkproxy-csp"
[10]: https://bugzilla.mozilla.org/show_bug.cgi?id=563169 "https://bugzilla.mozilla.org/show_bug.cgi?id=563169"
[11]: https://learn.microsoft.com/en-us/microsoft-cloud/dev/dev-proxy/how-to/use-system-proxy-option "https://learn.microsoft.com/en-us/microsoft-cloud/dev/dev-proxy/how-to/use-system-proxy-option"
[12]: https://learn.microsoft.com/en-us/uwp/api/windows.networking.connectivity.proxyconfiguration.proxyuris?view=winrt-28000 "https://learn.microsoft.com/en-us/uwp/api/windows.networking.connectivity.proxyconfiguration.proxyuris?view=winrt-28000"
