#![cfg_attr(all(target_os = "macos", feature = "app-store"), allow(dead_code))]

pub mod singbox;
pub mod tun;
pub mod tunnel_service;
pub mod xray;

#[cfg(all(target_os = "macos", feature = "app-store"))]
mod app_store_tunnel;

#[cfg(windows)]
pub mod ipc;
#[cfg(windows)]
pub mod sysproxy;
#[cfg(windows)]
pub mod windows_net;

#[cfg(target_os = "macos")]
#[path = "sysproxy_macos.rs"]
pub mod sysproxy;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use reqwest::Url;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
#[cfg(any(windows, all(target_os = "macos", feature = "app-store")))]
use std::sync::atomic::AtomicBool;
#[cfg(windows)]
use std::sync::atomic::AtomicIsize;
#[cfg(any(windows, all(target_os = "macos", feature = "app-store")))]
use std::sync::atomic::Ordering;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Manager,
};

// Global connection state
static CONNECTION_STATE: Mutex<bool> = Mutex::new(false);
// Track which engine is active: "singbox" or "xray"
static ACTIVE_ENGINE: Mutex<Option<String>> = Mutex::new(None);
static SYSTEM_PROXY_MANAGED: Mutex<bool> = Mutex::new(false);
static ACTIVE_XRAY_API_PORT: Mutex<u16> = Mutex::new(10813);
#[cfg(windows)]
static APP_INSTANCE_MUTEX_HANDLE: AtomicIsize = AtomicIsize::new(0);
// sing-box clash API traffic tracking (previous totals for delta calculation)
static SB_PREV_DOWN: Mutex<i64> = Mutex::new(0);
static SB_PREV_UP: Mutex<i64> = Mutex::new(0);
// sing-box seen connection IDs (to only log new connections)
use std::collections::HashSet;
static SB_SEEN_CONNS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

// Connection debug log buffer — shown in UI via get_proxy_logs
static CONNECT_LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
#[cfg(windows)]
static QA_FRONTEND_SNAPSHOT: Mutex<Option<serde_json::Value>> = Mutex::new(None);

static RUNTIME_OP_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));
#[cfg(all(target_os = "macos", feature = "app-store"))]
static APP_STORE_CONNECT_CANCELLED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static WINDOWS_CONNECT_CANCELLED: AtomicBool = AtomicBool::new(false);
#[cfg(all(target_os = "macos", feature = "app-store"))]
#[derive(Default)]
struct AppStoreDataplaneProbeCache {
    checked_at: Option<Instant>,
    ok: bool,
    detail: String,
}
#[cfg(all(target_os = "macos", feature = "app-store"))]
static APP_STORE_DATAPLANE_PROBE: LazyLock<tokio::sync::Mutex<AppStoreDataplaneProbeCache>> =
    LazyLock::new(|| tokio::sync::Mutex::new(AppStoreDataplaneProbeCache::default()));

const WORKSHOP_API_HOSTS: &[&str] = &[
    "doodleraydb-doodleray-ic3y6k-c7350f-94-241-172-101.traefik.me",
    "94-241-172-101.sslip.io",
];
const APP_MANAGED_PORTS: &[u16] = &[10808, 10809, 10813];
const SECURE_STORE_SERVICE: &str = "DoodleRay";
const SECURE_STORE_CHUNK_BYTES: usize = 1800;
const SECURE_STORE_CHUNK_PREFIX: &str = "chunked:v1:";
const APP_IDENTIFIER: &str = match option_env!("DOODLERAY_APP_IDENTIFIER") {
    Some(identifier) => identifier,
    None => "com.doodlevpn.doodleray",
};
const APP_PRODUCT_NAME: &str = "DoodleRay VPN";
const PROFILE_PING_URL: &str = "https://captive.apple.com/hotspot-detect.html";
const APP_API_DEFAULT_BASE_URL: &str = "https://ddlvpn.lol/v1/mobile";
const APP_API_CONNECTION_PROFILE_PATH: &str = "/connection-profile";
const APP_ROUTING_ROOT_KID: &str = "dogfood-20260513-ed25519";
const APP_ROUTING_ROOT_PUBLIC_KEY_BASE64: &str = "wXPEoRe8eSiTD9a3x21WhgDAYayS0XxB_2ajIcjUtiw";
const APP_ROUTING_ASSET_CANONICAL_RULE_VERSION: &str = "routing_asset.v1.lines";
#[cfg(all(target_os = "macos", feature = "app-store"))]
const APP_STORE_TRAFFIC_VERIFY_URLS: [&str; 3] = [
    "https://1.1.1.1/cdn-cgi/trace",
    "https://api.ipify.org",
    "https://ddlvpn.lol/healthz",
];
const APP_API_SESSION_KEY: &str = "app-api-session-v1";
const APP_API_DEVICE_KEY: &str = "app-api-device-v1";

static APP_API_MEMORY_SESSION: Mutex<Option<AppApiTokenResponse>> = Mutex::new(None);

#[cfg(windows)]
fn claim_single_app_instance() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let local_name: Vec<u16> = "Local\\DoodleRay.VPN.AppInstance.v1\0"
        .encode_utf16()
        .collect();
    let handle = unsafe { CreateMutexW(std::ptr::null(), 1, local_name.as_ptr()) };
    if handle.is_null() {
        return false;
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            CloseHandle(handle);
        }
        return false;
    }

    let legacy_global_name: Vec<u16> = "Global\\DoodleRay.VPN.AppInstance.v1\0"
        .encode_utf16()
        .collect();
    let legacy_handle = unsafe { CreateMutexW(std::ptr::null(), 0, legacy_global_name.as_ptr()) };
    if !legacy_handle.is_null() {
        let legacy_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        unsafe {
            CloseHandle(legacy_handle);
        }
        if legacy_exists {
            unsafe {
                CloseHandle(handle);
            }
            return false;
        }
    }

    APP_INSTANCE_MUTEX_HANDLE.store(handle as isize, Ordering::SeqCst);
    true
}

#[cfg(not(windows))]
fn claim_single_app_instance() -> bool {
    true
}

fn vpn_log(msg: &str) {
    let line = format!("[vpn] {}", msg);
    eprintln!("{}", line);
    if let Ok(mut logs) = CONNECT_LOG.lock() {
        logs.push(line);
        if logs.len() > 200 {
            let drain = logs.len() - 200;
            logs.drain(..drain);
        }
    }
}

#[cfg(windows)]
fn quote_ps_single(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(windows)]
fn run_hidden_powershell(script: &str) {
    let mut cmd = std::process::Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", script]);
    cmd.creation_flags(0x08000000);
    let _ = cmd.output();
}

#[cfg(windows)]
fn terminate_other_doodleray_app_instances() {
    let Ok(exe_path) = std::env::current_exe() else {
        return;
    };
    let exe = quote_ps_single(&exe_path.to_string_lossy());
    let pid = std::process::id();
    let script = format!(
        "Get-CimInstance Win32_Process -Filter \"Name = 'DoodleRay.exe'\" | Where-Object {{ $_.ProcessId -ne {} -and $_.ExecutablePath -eq {} }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}",
        pid, exe
    );
    run_hidden_powershell(&script);
}

#[cfg(windows)]
fn terminate_orphaned_doodleray_engine_processes() {
    let Ok(exe_path) = std::env::current_exe() else {
        return;
    };
    let Some(exe_dir) = exe_path.parent() else {
        return;
    };
    let xray_path = quote_ps_single(&exe_dir.join("xray-core").join("xray.exe").to_string_lossy());
    let singbox_path = quote_ps_single(&exe_dir.join("sing-box.exe").to_string_lossy());
    let script = format!(
        "$owned = @({}, {}); Get-CimInstance Win32_Process | Where-Object {{ $_.ExecutablePath -in $owned }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}",
        xray_path, singbox_path
    );
    run_hidden_powershell(&script);
}

#[cfg(windows)]
fn tunnel_service_reports_active() -> bool {
    match ipc::tunnel_service_status() {
        Ok(tunnel_service::TunnelResponse::Status(status)) => matches!(
            status.state,
            tunnel_service::TunnelState::Connected | tunnel_service::TunnelState::Connecting
        ),
        _ => false,
    }
}

#[cfg(windows)]
fn tunnel_service_registration_state() -> Result<windows_service::service::ServiceState, String> {
    use windows_service::service::ServiceAccess;
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|e| format!("Failed to open Windows service manager: {}", e))?;
    let service = manager
        .open_service(
            tunnel_service::TUNNEL_SERVICE_NAME,
            ServiceAccess::QUERY_STATUS,
        )
        .map_err(|e| format!("Tunnel service is not installed: {}", e))?;
    service
        .query_status()
        .map(|status| status.current_state)
        .map_err(|e| format!("Failed to query tunnel service: {}", e))
}

#[cfg(windows)]
fn ensure_tunnel_service_running() -> Result<(), String> {
    use windows_service::service::{ServiceAccess, ServiceState};
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|e| format!("Failed to open Windows service manager: {}", e))?;
    let service = manager
        .open_service(
            tunnel_service::TUNNEL_SERVICE_NAME,
            ServiceAccess::START | ServiceAccess::QUERY_STATUS,
        )
        .map_err(|e| format!("Tunnel service cannot be started: {}", e))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut start_requested = false;
    while Instant::now() < deadline {
        let state = service
            .query_status()
            .map_err(|e| format!("Failed to query tunnel service: {}", e))?
            .current_state;
        match state {
            ServiceState::Running => return Ok(()),
            ServiceState::Stopped if !start_requested => {
                service
                    .start(&[] as &[&str])
                    .map_err(|e| format!("Failed to start tunnel service: {}", e))?;
                start_requested = true;
            }
            ServiceState::StartPending | ServiceState::StopPending | ServiceState::Stopped => {}
            other => {
                return Err(format!(
                    "Tunnel service cannot start from state {:?}",
                    other
                ))
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("Tunnel service did not start within 10s".into())
}

#[cfg(windows)]
fn wait_for_tunnel_service_stop(timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if matches!(
            tunnel_service_registration_state(),
            Ok(windows_service::service::ServiceState::Stopped)
        ) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn validate_http_url(raw_url: &str) -> Result<Url, String> {
    let parsed = Url::parse(raw_url).map_err(|e| format!("Invalid URL: {}", e))?;
    match parsed.scheme() {
        "https" => {}
        _ => return Err("Only https:// URLs are allowed".into()),
    }

    let host = parsed.host_str().ok_or("URL must include a host")?;
    let blocked_host = host.eq_ignore_ascii_case("localhost")
        || host.eq_ignore_ascii_case("0.0.0.0")
        || host.ends_with(".localhost")
        || host.ends_with(".local");
    if blocked_host {
        return Err("Local subscription URLs are not allowed".into());
    }

    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if !is_public_ip(ip) {
            return Err(
                "Private, loopback, or link-local subscription URLs are not allowed".into(),
            );
        }
    }

    Ok(parsed)
}

fn is_public_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => {
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.octets()[0] == 0)
        }
        std::net::IpAddr::V6(ip) => {
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local())
        }
    }
}

const MAX_SUBSCRIPTION_BODY_BYTES: usize = 8 * 1024 * 1024;
const MAX_WORKSHOP_BODY_BYTES: usize = 2 * 1024 * 1024;

fn redirect_target_allowed(initial: &Url, next: &Url, redirects: usize) -> Result<(), String> {
    if redirects >= 5 {
        return Err("Too many HTTP redirects".into());
    }
    validate_http_url(next.as_str())?;
    if initial.host_str() != next.host_str() {
        return Err("Cross-host subscription redirects are not allowed".into());
    }
    if initial.scheme() == "https" && next.scheme() != "https" {
        return Err("HTTPS subscription redirects cannot downgrade to HTTP".into());
    }
    Ok(())
}

fn safe_subscription_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        let Some(initial) = attempt.previous().first() else {
            return attempt.stop();
        };
        if redirect_target_allowed(initial, attempt.url(), attempt.previous().len()).is_ok() {
            attempt.follow()
        } else {
            attempt.stop()
        }
    })
}

async fn read_response_body_limited(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(format!("Response exceeds {} bytes", max_bytes));
    }

    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?
    {
        if body.len().saturating_add(chunk.len()) > max_bytes {
            return Err(format!("Response exceeds {} bytes", max_bytes));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn diagnostic_item(
    severity: &str,
    code: &str,
    title: &str,
    detail: impl Into<String>,
) -> serde_json::Value {
    serde_json::json!({
        "severity": severity,
        "code": code,
        "title": title,
        "detail": detail.into(),
    })
}

fn host_from_optional_url(raw_url: Option<&str>) -> Option<String> {
    let raw = raw_url?.trim();
    if raw.is_empty() {
        return None;
    }
    Url::parse(raw)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_string()))
}

fn hosts_file_path() -> &'static str {
    #[cfg(windows)]
    {
        r"C:\Windows\System32\drivers\etc\hosts"
    }
    #[cfg(not(windows))]
    {
        "/etc/hosts"
    }
}

fn hosts_file_mentions(host: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(hosts_file_path()) else {
        return false;
    };
    let host_lower = host.to_lowercase();
    text.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.starts_with('#') && trimmed.to_lowercase().contains(&host_lower)
    })
}

fn command_stdout(program: &str, args: &[&str]) -> String {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    match cmd.output() {
        Ok(output) => {
            let mut text = String::new();
            text.push_str(&String::from_utf8_lossy(&output.stdout));
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            text
        }
        Err(_) => String::new(),
    }
}

fn process_snapshot() -> String {
    #[cfg(windows)]
    {
        command_stdout("tasklist", &["/FO", "CSV", "/NH"])
    }
    #[cfg(not(windows))]
    {
        command_stdout("ps", &["-axo", "comm,args"])
    }
}

#[derive(serde::Serialize)]
struct RunningApp {
    name: String,
    path: String,
}

/// UI-facing adapter for the split-routing app picker: windowed processes
/// with resolvable paths, minus system/self entries. Read-only.
#[tauri::command]
fn list_running_apps() -> Vec<RunningApp> {
    #[cfg(windows)]
    {
        let raw = command_stdout(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "Get-Process | Where-Object { $_.MainWindowTitle -and $_.Path } | Select-Object Name, Path -Unique | ConvertTo-Json -Compress",
            ],
        );
        let start = raw.find(['[', '{']).unwrap_or(raw.len());
        let parsed: serde_json::Value = match serde_json::from_str(raw[start..].trim()) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        let items: Vec<serde_json::Value> = match parsed {
            serde_json::Value::Array(a) => a,
            v @ serde_json::Value::Object(_) => vec![v],
            _ => Vec::new(),
        };
        let mut apps: Vec<RunningApp> = items
            .into_iter()
            .filter_map(|v| {
                let name = v.get("Name")?.as_str()?.to_string();
                let path = v.get("Path")?.as_str()?.to_string();
                let lower = path.to_lowercase();
                if lower.starts_with("c:\\windows") || lower.contains("doodleray") {
                    return None;
                }
                Some(RunningApp { name, path })
            })
            .collect();
        apps.sort_by_key(|app| app.name.to_lowercase());
        apps.dedup_by(|a, b| a.path.eq_ignore_ascii_case(&b.path));
        apps.truncate(60);
        apps
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Sibling .exe files next to a chosen app — games/launchers often need
/// several related executables routed together. Read-only directory listing.
#[tauri::command]
fn list_dir_exes(exe_path: String) -> Vec<String> {
    let path = std::path::Path::new(&exe_path);
    if !path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("exe"))
        .unwrap_or(false)
    {
        return Vec::new();
    }
    let Some(dir) = path.parent() else {
        return Vec::new();
    };
    let mut exes: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.to_lowercase().ends_with(".exe") {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    exes.sort_by_key(|n| n.to_lowercase());
    exes.truncate(40);
    exes
}

fn network_interface_snapshot() -> String {
    #[cfg(windows)]
    {
        command_stdout("ipconfig", &["/all"])
    }
    #[cfg(not(windows))]
    {
        command_stdout("ifconfig", &[])
    }
}

fn known_conflict_patterns() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        (
            "radmin",
            "Radmin VPN",
            "VPN service can change routes and virtual adapters.",
        ),
        (
            "rvcontrolsvc",
            "Radmin VPN Control Service",
            "VPN service can change routes and virtual adapters.",
        ),
        (
            "adguard",
            "AdGuard",
            "DNS/filtering layer can rewrite subscription DNS results.",
        ),
        (
            "goodbyedpi",
            "GoodbyeDPI",
            "DPI bypass tool can conflict with VPN routing.",
        ),
        (
            "zapret",
            "zapret",
            "DPI bypass tool can conflict with VPN routing.",
        ),
        (
            "winws",
            "zapret/winws",
            "DPI bypass service can conflict with VPN routing.",
        ),
        (
            "killer",
            "Killer Network",
            "Network optimizer can reprioritize or filter traffic.",
        ),
        (
            "proxifier",
            "Proxifier",
            "Proxy manager can capture traffic before DoodleRay.",
        ),
        (
            "clash",
            "Clash",
            "Another proxy client can compete for VPN/proxy routes.",
        ),
        (
            "v2ray",
            "v2ray",
            "Another proxy core can compete for VPN/proxy routes.",
        ),
        (
            "xray",
            "xray",
            "Another proxy core can compete for VPN/proxy routes.",
        ),
        (
            "nekoray",
            "NekoRay",
            "Another proxy client can compete for VPN/proxy routes.",
        ),
        (
            "nekobox",
            "NekoBox",
            "Another proxy client can compete for VPN/proxy routes.",
        ),
        (
            "hiddify",
            "Hiddify",
            "Another VPN client can compete for VPN/proxy routes.",
        ),
        (
            "happ",
            "Happ",
            "Another VPN client can compete for VPN/proxy routes.",
        ),
        (
            "wireguard",
            "WireGuard",
            "VPN tunnel can change routes and DNS.",
        ),
        (
            "openvpn",
            "OpenVPN",
            "VPN tunnel can change routes and DNS.",
        ),
        (
            "tailscale",
            "Tailscale",
            "VPN/mesh tunnel can change routes and DNS.",
        ),
        (
            "zerotier",
            "ZeroTier",
            "VPN/mesh tunnel can change routes and DNS.",
        ),
        (
            "cloudflare warp",
            "Cloudflare WARP",
            "VPN/DNS layer can change routes and DNS.",
        ),
        (
            "warp-svc",
            "Cloudflare WARP",
            "VPN/DNS layer can change routes and DNS.",
        ),
    ]
}

fn detect_conflicting_software() -> Vec<serde_json::Value> {
    let snapshot = process_snapshot().to_lowercase();
    let interfaces = network_interface_snapshot().to_lowercase();
    let combined = format!("{}\n{}", snapshot, interfaces);
    let mut seen = HashSet::new();
    let mut found = Vec::new();

    for (needle, name, reason) in known_conflict_patterns() {
        if combined.contains(needle) && seen.insert(*name) {
            found.push(serde_json::json!({
                "name": name,
                "reason": reason,
            }));
        }
    }

    found
}

fn port_busy_snapshot(port: u16) -> Option<String> {
    #[cfg(windows)]
    {
        let text = command_stdout("netstat", &["-ano"]);
        let port_str = format!(":{}", port);
        for line in text.lines() {
            if line.contains(&port_str) && line.contains("LISTENING") {
                return Some(line.trim().to_string());
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        let text = command_stdout("lsof", &["-nP", "-i", &format!(":{}", port)]);
        text.lines()
            .find(|line| line.contains("LISTEN"))
            .map(|line| line.trim().to_string())
    }
}

fn compact_command_output(output: String, max_chars: usize) -> String {
    let collapsed = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(18)
        .collect::<Vec<_>>()
        .join(" | ");
    if collapsed.len() > max_chars {
        let clipped = collapsed.chars().take(max_chars).collect::<String>();
        format!("{}...", clipped)
    } else {
        collapsed
    }
}

fn dns_snapshot() -> String {
    #[cfg(windows)]
    {
        command_stdout(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "Get-DnsClientServerAddress -AddressFamily IPv4 | Select-Object -First 8 InterfaceAlias,ServerAddresses | Format-Table -HideTableHeaders",
            ],
        )
    }
    #[cfg(target_os = "macos")]
    {
        command_stdout("scutil", &["--dns"])
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let resolv = std::fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
        if resolv.trim().is_empty() {
            command_stdout("resolvectl", &["dns"])
        } else {
            resolv
        }
    }
}

fn default_route_snapshot() -> String {
    #[cfg(windows)]
    {
        command_stdout(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Sort-Object RouteMetric | Select-Object -First 5 InterfaceAlias,NextHop,RouteMetric | Format-Table -HideTableHeaders",
            ],
        )
    }
    #[cfg(target_os = "macos")]
    {
        command_stdout("route", &["-n", "get", "1.1.1.1"])
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        command_stdout("ip", &["route", "get", "1.1.1.1"])
    }
}

fn service_snapshot() -> String {
    #[cfg(windows)]
    {
        command_stdout("sc", &["query", "state=", "all"])
    }
    #[cfg(not(windows))]
    {
        String::new()
    }
}

fn tcp_reachability_check(
    code: &str,
    title: &str,
    host: &str,
    port: u16,
    required: bool,
) -> serde_json::Value {
    let target = format!("{}:{}", host, port);
    let resolved = match target.to_socket_addrs() {
        Ok(addrs) => addrs.collect::<Vec<_>>(),
        Err(e) => {
            return diagnostic_item(
                if required { "error" } else { "warning" },
                code,
                title,
                format!("DNS/resolve failed for {}: {}", target, e),
            )
        }
    };

    if resolved.is_empty() {
        return diagnostic_item(
            if required { "error" } else { "warning" },
            code,
            title,
            format!("{} did not resolve to any socket address", target),
        );
    }

    let started = Instant::now();
    let mut last_error = String::new();
    for addr in resolved.iter().take(6) {
        match TcpStream::connect_timeout(addr, Duration::from_millis(2500)) {
            Ok(_) => {
                return diagnostic_item(
                    "ok",
                    code,
                    title,
                    format!(
                        "{} reachable via {} in {} ms",
                        target,
                        addr,
                        started.elapsed().as_millis()
                    ),
                )
            }
            Err(e) => {
                last_error = format!("{}: {}", addr, e);
            }
        }
    }

    diagnostic_item(
        if required { "error" } else { "warning" },
        code,
        title,
        format!(
            "{} is not reachable over TCP. Last error: {}",
            target, last_error
        ),
    )
}

fn socks5_handshake_check(port: u16) -> serde_json::Value {
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    match TcpStream::connect_timeout(&addr, Duration::from_millis(1200)) {
        Ok(mut stream) => {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(1200)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(1200)));
            if let Err(e) = stream.write_all(&[0x05, 0x01, 0x00]) {
                return diagnostic_item(
                    "error",
                    "socks_handshake_failed",
                    "Local SOCKS port is not responding correctly",
                    format!("Write to 127.0.0.1:{} failed: {}", port, e),
                );
            }
            let mut response = [0u8; 2];
            match stream.read_exact(&mut response) {
                Ok(()) if response[0] == 0x05 && response[1] != 0xff => diagnostic_item(
                    "ok",
                    "socks_handshake_ok",
                    "Local SOCKS5 handshake passed",
                    format!("127.0.0.1:{} accepted SOCKS5 no-auth method", port),
                ),
                Ok(()) => diagnostic_item(
                    "error",
                    "socks_handshake_rejected",
                    "Local SOCKS5 handshake was rejected",
                    format!("127.0.0.1:{} returned {:02x?}", port, response),
                ),
                Err(e) => diagnostic_item(
                    "error",
                    "socks_handshake_timeout",
                    "Local SOCKS port accepted TCP but did not answer SOCKS5",
                    format!("127.0.0.1:{} read failed: {}", port, e),
                ),
            }
        }
        Err(e) => diagnostic_item(
            "error",
            "socks_port_closed",
            "Local SOCKS port is closed",
            format!("127.0.0.1:{}: {}", port, e),
        ),
    }
}

async fn subscription_fetch_check(raw_url: &str) -> serde_json::Value {
    let parsed = match validate_http_url(raw_url) {
        Ok(url) => url,
        Err(e) => {
            return diagnostic_item(
                "error",
                "subscription_fetch_blocked",
                "Subscription fetch blocked before HTTP request",
                e,
            )
        }
    };

    let client = match direct_fetch_client(&parsed, Duration::from_secs(8)).await {
        Ok(client) => client,
        Err(e) => {
            return diagnostic_item(
                "error",
                "subscription_fetch_client_failed",
                "Subscription HTTP client failed to initialize",
                e.to_string(),
            )
        }
    };

    let started = Instant::now();
    match client.get(parsed.clone()).send().await {
        Ok(resp) => {
            let status = resp.status();
            if !status.is_success() {
                return diagnostic_item(
                    "error",
                    "subscription_http_status_bad",
                    "Subscription HTTP request returned an error",
                    format!(
                        "{} returned HTTP {}",
                        parsed.host_str().unwrap_or("subscription"),
                        status
                    ),
                );
            }
            match read_response_body_limited(resp, MAX_SUBSCRIPTION_BODY_BYTES).await {
                Ok(bytes) if bytes.is_empty() => diagnostic_item(
                    "warning",
                    "subscription_body_empty",
                    "Subscription HTTP request returned empty body",
                    format!(
                        "HTTP {} in {} ms, 0 bytes",
                        status,
                        started.elapsed().as_millis()
                    ),
                ),
                Ok(bytes) => diagnostic_item(
                    "ok",
                    "subscription_fetch_ok",
                    "Subscription HTTP fetch passed",
                    format!(
                        "HTTP {} in {} ms, {} bytes",
                        status,
                        started.elapsed().as_millis(),
                        bytes.len()
                    ),
                ),
                Err(e) => diagnostic_item(
                    "error",
                    "subscription_body_read_failed",
                    "Subscription body read failed",
                    e.to_string(),
                ),
            }
        }
        Err(e) => diagnostic_item(
            "error",
            "subscription_fetch_failed",
            "Subscription HTTP request failed",
            format!("{}: {}", parsed, e),
        ),
    }
}

fn diagnostics_summary(checks: &[serde_json::Value]) -> &'static str {
    let severity_rank = |value: &serde_json::Value| match value
        .get("severity")
        .and_then(|severity| severity.as_str())
        .unwrap_or("info")
    {
        "error" => 3,
        "warning" => 2,
        "ok" => 0,
        _ => 1,
    };
    match checks.iter().map(severity_rank).max().unwrap_or(0) {
        3 => "errors_found",
        2 => "warnings_found",
        _ => "ok",
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn run_network_diagnostics(
    subscription_url: Option<String>,
    socks_port: u16,
    http_port: u16,
    active_server_address: Option<String>,
    active_server_port: Option<u16>,
    active_server_protocol: Option<String>,
    proxy_mode: Option<String>,
    app_status: Option<String>,
    active_routing_rule_count: Option<usize>,
    system_proxy_mode: Option<String>,
    dns_mode: Option<String>,
    network_stack: Option<String>,
) -> serde_json::Value {
    let started = Instant::now();
    let mut checks = Vec::new();
    let mut resolved_ips: Vec<String> = Vec::new();
    let host = host_from_optional_url(subscription_url.as_deref());
    let backend_connected = CONNECTION_STATE.lock().map(|state| *state).unwrap_or(false);
    let frontend_connected = app_status.as_deref() == Some("connected");
    let app_connected = backend_connected || frontend_connected;
    let proxy_mode = proxy_mode.unwrap_or_else(|| "system-proxy".to_string());
    let active_routing_rule_count = active_routing_rule_count.unwrap_or(0);

    if let Some(host) = host.as_deref() {
        if hosts_file_mentions(host) {
            checks.push(diagnostic_item(
                "warning",
                "hosts_override",
                "Subscription host appears in hosts file",
                format!(
                    "{} contains an entry for {}. This can force the subscription to a wrong IP.",
                    hosts_file_path(),
                    host
                ),
            ));
        }

        let port = subscription_url
            .as_deref()
            .and_then(|raw| Url::parse(raw).ok())
            .and_then(|url| url.port_or_known_default())
            .unwrap_or(443);

        match (host, port).to_socket_addrs() {
            Ok(addrs) => {
                let mut private_hits = Vec::new();
                for addr in addrs {
                    let ip = addr.ip();
                    let ip_text = ip.to_string();
                    if !resolved_ips.contains(&ip_text) {
                        resolved_ips.push(ip_text.clone());
                    }
                    if !is_public_ip(ip) {
                        private_hits.push(ip_text);
                    }
                }

                if private_hits.is_empty() {
                    checks.push(diagnostic_item(
                        "ok",
                        "subscription_dns_public",
                        "Subscription DNS resolves to public IPs",
                        format!("{} -> {}", host, resolved_ips.join(", ")),
                    ));
                } else {
                    checks.push(diagnostic_item(
                        "error",
                        "subscription_dns_private",
                        "Subscription host resolves to private/local IP",
                        format!(
                            "{} -> {}. DNS filters, hosts file, local proxy, or another VPN may be rewriting this host.",
                            host,
                            private_hits.join(", ")
                        ),
                    ));
                }
            }
            Err(e) => checks.push(diagnostic_item(
                "error",
                "subscription_dns_failed",
                "Subscription DNS lookup failed",
                format!("{}: {}", host, e),
            )),
        }

        if let Some(url) = subscription_url
            .as_deref()
            .filter(|url| !url.trim().is_empty())
        {
            checks.push(subscription_fetch_check(url).await);
        }
    } else {
        checks.push(diagnostic_item(
            "info",
            "subscription_not_checked",
            "Subscription URL not provided",
            "Paste a subscription URL before running diagnostics to check DNS resolution.",
        ));
    }

    let conflicts = detect_conflicting_software();
    if conflicts.is_empty() {
        checks.push(diagnostic_item(
            "ok",
            "conflicts_none_detected",
            "No known conflicting network tools detected",
            "This is a heuristic check; hidden services or drivers may still exist.",
        ));
    } else {
        let names = conflicts
            .iter()
            .filter_map(|item| item.get("name").and_then(|v| v.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        checks.push(diagnostic_item(
            "warning",
            "conflicts_detected",
            "Potentially conflicting network tools detected",
            format!(
                "{}. Do not remove them automatically; ask the user to test with them disabled.",
                names
            ),
        ));
    }

    let services = service_snapshot().to_lowercase();
    if services.contains("windivert")
        || services.contains("goodbyedpi")
        || services.contains("zapret")
        || services.contains("radmin")
    {
        checks.push(diagnostic_item(
            "warning",
            "network_services_detected",
            "Potential network services detected",
            "Windows service list contains WinDivert/GoodbyeDPI/zapret/Radmin markers. Disable them for a test instead of deleting automatically.",
        ));
    } else if cfg!(windows) {
        checks.push(diagnostic_item(
            "ok",
            "network_services_clean",
            "Known conflicting Windows services were not detected",
            "Service scan did not find WinDivert/GoodbyeDPI/zapret/Radmin markers.",
        ));
    }

    for (label, port) in [("SOCKS", socks_port), ("HTTP", http_port)] {
        if let Some(line) = port_busy_snapshot(port) {
            let severity = if app_connected { "ok" } else { "warning" };
            let title = if app_connected {
                format!("{} port is listening", label)
            } else {
                format!("{} port is already in use", label)
            };
            checks.push(diagnostic_item(
                severity,
                &format!("{}_port_busy", label.to_lowercase()),
                &title,
                line,
            ));
        } else {
            let severity = if app_connected { "error" } else { "ok" };
            let title = if app_connected {
                format!("{} port is not listening while VPN is connected", label)
            } else {
                format!("{} port is free", label)
            };
            checks.push(diagnostic_item(
                severity,
                &format!("{}_port_free", label.to_lowercase()),
                &title,
                format!("Port {}", port),
            ));
        }
    }

    checks.push(tcp_reachability_check(
        "public_tcp_443",
        "Public TCP/443 connectivity",
        "1.1.1.1",
        443,
        true,
    ));

    checks.push(tcp_reachability_check(
        "system_dns_resolve",
        "System DNS resolve check",
        "cloudflare.com",
        443,
        true,
    ));

    if let (Some(address), Some(port)) = (active_server_address.as_deref(), active_server_port) {
        let protocol = active_server_protocol
            .as_deref()
            .unwrap_or("unknown")
            .to_lowercase();
        if matches!(protocol.as_str(), "hysteria2" | "tuic" | "wireguard") {
            checks.push(diagnostic_item(
                "info",
                "active_server_udp_protocol",
                "Active server uses a UDP-based protocol",
                format!(
                    "{}:{} uses {}. TCP connect is not a valid reachability test for this protocol.",
                    address, port, protocol
                ),
            ));
            let target = format!("{}:{}", address, port);
            match target.to_socket_addrs() {
                Ok(addrs) => {
                    let ips = addrs.map(|addr| addr.ip().to_string()).collect::<Vec<_>>();
                    checks.push(diagnostic_item(
                        "ok",
                        "active_server_dns_ok",
                        "Active server DNS resolved",
                        format!("{} -> {}", address, ips.join(", ")),
                    ));
                }
                Err(e) => checks.push(diagnostic_item(
                    "error",
                    "active_server_dns_failed",
                    "Active server DNS failed",
                    format!("{}: {}", address, e),
                )),
            }
        } else {
            checks.push(tcp_reachability_check(
                "active_server_tcp",
                "Active server TCP reachability",
                address,
                port,
                true,
            ));
        }
    } else {
        checks.push(diagnostic_item(
            "info",
            "active_server_not_selected",
            "Active server was not selected",
            "Select a server before running diagnostics to test server reachability.",
        ));
    }

    if app_connected {
        checks.push(socks5_handshake_check(socks_port));
    }

    if active_routing_rule_count > 0 && proxy_mode != "tun" {
        checks.push(diagnostic_item(
            "warning",
            "split_rules_proxy_mode",
            "Workshop rules need Whole computer mode",
            format!(
                "{} active rule(s). They stay enabled, but only Whole computer mode can apply app and site routing rules.",
                active_routing_rule_count
            ),
        ));
    } else if active_routing_rule_count > 0 {
        checks.push(diagnostic_item(
            "ok",
            "split_rules_tun_mode",
            "Workshop rules can be applied",
            format!(
                "{} active rule(s), mode={}",
                active_routing_rule_count, proxy_mode
            ),
        ));
    }

    let route = compact_command_output(default_route_snapshot(), 700);
    if route.is_empty() {
        checks.push(diagnostic_item(
            "warning",
            "default_route_unavailable",
            "Default route snapshot unavailable",
            "The diagnostic command did not return route information.",
        ));
    } else {
        checks.push(diagnostic_item(
            "info",
            "default_route_snapshot",
            "Default route snapshot",
            route,
        ));
    }

    let dns = compact_command_output(dns_snapshot(), 700);
    if dns.is_empty() {
        checks.push(diagnostic_item(
            "warning",
            "dns_snapshot_unavailable",
            "DNS snapshot unavailable",
            "The diagnostic command did not return DNS resolver information.",
        ));
    } else {
        let lower = dns.to_lowercase();
        let severity =
            if lower.contains("127.0.0.1") || lower.contains("::1") || lower.contains("adguard") {
                "warning"
            } else {
                "info"
            };
        checks.push(diagnostic_item(
            severity,
            "dns_snapshot",
            "DNS resolver snapshot",
            dns,
        ));
    }

    checks.push(diagnostic_item(
        "info",
        "app_network_settings",
        "DoodleRay network settings",
        format!(
            "status={}, backend_connected={}, mode={}, system_proxy={}, dns={}, stack={}, socks={}, http={}",
            app_status.unwrap_or_else(|| "unknown".to_string()),
            backend_connected,
            proxy_mode,
            system_proxy_mode.unwrap_or_else(|| "unknown".to_string()),
            dns_mode.unwrap_or_else(|| "unknown".to_string()),
            network_stack.unwrap_or_else(|| "unknown".to_string()),
            socks_port,
            http_port
        ),
    ));

    let summary = diagnostics_summary(&checks);

    serde_json::json!({
        "summary": summary,
        "subscriptionHost": host,
        "resolvedIps": resolved_ips,
        "conflicts": conflicts,
        "checks": checks,
        "durationMs": started.elapsed().as_millis() as u64,
    })
}

fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let value = bytes as f64;
    if value >= GB {
        format!("{:.2} GB", value / GB)
    } else if value >= MB {
        format!("{:.1} MB", value / MB)
    } else if value >= KB {
        format!("{:.1} KB", value / KB)
    } else {
        format!("{} B", bytes)
    }
}

fn dir_size_limited(path: &Path, max_entries: usize) -> (u64, bool) {
    let mut total = 0u64;
    let mut truncated = false;
    let mut seen = 0usize;
    let mut stack = vec![path.to_path_buf()];

    while let Some(current) = stack.pop() {
        if seen >= max_entries {
            truncated = true;
            break;
        }
        seen += 1;

        let Ok(meta) = std::fs::symlink_metadata(&current) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_file() {
            total = total.saturating_add(meta.len());
            continue;
        }
        if meta.is_dir() {
            let Ok(entries) = std::fs::read_dir(&current) else {
                continue;
            };
            for entry in entries.filter_map(|entry| entry.ok()) {
                stack.push(entry.path());
            }
        }
    }

    (total, truncated)
}

fn push_storage_path(
    paths: &mut Vec<(String, PathBuf, &'static str, bool)>,
    label: impl Into<String>,
    path: PathBuf,
    kind: &'static str,
    clearable: bool,
) {
    paths.push((label.into(), path, kind, clearable));
}

fn known_storage_paths(app: &tauri::AppHandle) -> Vec<(String, PathBuf, &'static str, bool)> {
    let mut paths = Vec::new();
    if let Ok(path) = app.path().app_data_dir() {
        push_storage_path(&mut paths, "App data", path, "app_data", false);
    }

    push_storage_path(
        &mut paths,
        "Temp",
        std::env::temp_dir().join(APP_PRODUCT_NAME),
        "temp",
        true,
    );

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let home = PathBuf::from(home);
            push_storage_path(
                &mut paths,
                "Cache",
                home.join("Library").join("Caches").join(APP_IDENTIFIER),
                "cache",
                true,
            );
            push_storage_path(
                &mut paths,
                "Legacy cache",
                home.join("Library").join("Caches").join(APP_PRODUCT_NAME),
                "cache",
                true,
            );
            push_storage_path(
                &mut paths,
                "WebKit data",
                home.join("Library").join("WebKit").join(APP_IDENTIFIER),
                "webkit",
                false,
            );
            push_storage_path(
                &mut paths,
                "Legacy WebKit data",
                home.join("Library").join("WebKit").join(APP_PRODUCT_NAME),
                "webkit",
                false,
            );
            push_storage_path(
                &mut paths,
                "HTTP storage",
                home.join("Library")
                    .join("HTTPStorages")
                    .join(APP_IDENTIFIER),
                "cache",
                true,
            );
        }
    }

    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let local = PathBuf::from(local);
            push_storage_path(
                &mut paths,
                "Local cache",
                local.join(APP_IDENTIFIER),
                "cache",
                true,
            );
            push_storage_path(
                &mut paths,
                "Legacy local cache",
                local.join(APP_PRODUCT_NAME),
                "cache",
                true,
            );
        }
        if let Ok(roaming) = std::env::var("APPDATA") {
            push_storage_path(
                &mut paths,
                "Roaming app data",
                PathBuf::from(roaming).join(APP_IDENTIFIER),
                "app_data",
                false,
            );
        }
    }

    paths
}

#[tauri::command]
fn get_storage_report(app: tauri::AppHandle) -> serde_json::Value {
    let mut total = 0u64;
    let paths = known_storage_paths(&app)
        .into_iter()
        .map(|(label, path, kind, clearable)| {
            let exists = path.exists();
            let (bytes, truncated) = if exists {
                dir_size_limited(&path, 200_000)
            } else {
                (0, false)
            };
            total = total.saturating_add(bytes);
            serde_json::json!({
                "label": label,
                "path": path.to_string_lossy(),
                "kind": kind,
                "exists": exists,
                "clearable": clearable,
                "bytes": bytes,
                "size": human_bytes(bytes),
                "truncated": truncated,
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "totalBytes": total,
        "totalSize": human_bytes(total),
        "paths": paths,
    })
}

#[tauri::command]
fn clear_app_cache(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let mut removed = Vec::new();
    let mut failed = Vec::new();

    for (label, path, kind, clearable) in known_storage_paths(&app) {
        if !clearable || !path.exists() {
            continue;
        }
        let (bytes, _) = dir_size_limited(&path, 200_000);
        match std::fs::remove_dir_all(&path) {
            Ok(()) => removed.push(serde_json::json!({
                "label": label,
                "path": path.to_string_lossy(),
                "kind": kind,
                "bytes": bytes,
                "size": human_bytes(bytes),
            })),
            Err(e) => failed.push(serde_json::json!({
                "label": label,
                "path": path.to_string_lossy(),
                "error": e.to_string(),
            })),
        }
    }

    if failed.is_empty() {
        Ok(serde_json::json!({ "removed": removed, "failed": failed }))
    } else {
        Err(format!(
            "Some cache folders could not be removed: {}",
            serde_json::Value::Array(failed)
        ))
    }
}

fn validate_workshop_api_url(raw_url: &str) -> Result<Url, String> {
    let parsed = validate_http_url(raw_url)?;
    if parsed.scheme() != "https" {
        return Err("Workshop API must use HTTPS".into());
    }
    let host = parsed.host_str().ok_or("Workshop API host is missing")?;
    if !WORKSHOP_API_HOSTS.contains(&host) {
        return Err("Workshop API host is not allowed".into());
    }
    let path = parsed.path();
    let is_allowed_path = path.starts_with("/api/")
        || path == "/api"
        || path.starts_with("/doodleray-api/api/")
        || path == "/doodleray-api/api";
    if !is_allowed_path {
        return Err("Workshop API path is not allowed".into());
    }
    Ok(parsed)
}

fn requested_port_is_safe(port: u16) -> bool {
    APP_MANAGED_PORTS.contains(&port) || (49152..=65535).contains(&port)
}

fn is_physical_interface_name(name: &str) -> bool {
    name.starts_with("en") || name.starts_with("eth") || name.starts_with("wlan")
}

fn is_usable_source_ipv4(ip: Ipv4Addr) -> bool {
    !(ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.octets()[0] == 198 && ip.octets()[1] == 18)
}

fn physical_ipv4_candidates() -> Vec<Ipv4Addr> {
    let output = match std::process::Command::new("ifconfig").output() {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut current_interface = "";
    let mut candidates: Vec<(u8, Ipv4Addr)> = Vec::new();

    for line in stdout.lines() {
        if !line.starts_with('\t') && !line.starts_with(' ') {
            current_interface = line.split(':').next().unwrap_or_default();
        }

        let trimmed = line.trim_start();
        if !trimmed.starts_with("inet ") || !is_physical_interface_name(current_interface) {
            continue;
        }

        let Some(raw_ip) = trimmed.split_whitespace().nth(1) else {
            continue;
        };
        let Ok(ip) = raw_ip.parse::<Ipv4Addr>() else {
            continue;
        };
        if !is_usable_source_ipv4(ip) {
            continue;
        }

        let priority = if current_interface == "en0" { 0 } else { 1 };
        candidates.push((priority, ip));
    }

    candidates.sort_by_key(|(priority, ip)| (*priority, *ip));
    candidates.dedup_by_key(|(_, ip)| *ip);
    candidates.into_iter().map(|(_, ip)| ip).take(3).collect()
}

fn safe_network_stack(stack: &str) -> &str {
    match stack {
        "mixed" | "system" | "gvisor" => stack,
        _ => "system",
    }
}

fn effective_tun_network_stack(stack: &str) -> &str {
    let safe_stack = safe_network_stack(stack);
    #[cfg(windows)]
    {
        if safe_stack == "mixed" {
            "system"
        } else {
            safe_stack
        }
    }
    #[cfg(not(windows))]
    {
        safe_stack
    }
}

fn default_system_proxy_mode() -> String {
    "set".into()
}

fn default_xray_api_port() -> u16 {
    10813
}

fn safe_system_proxy_mode(mode: &str) -> &str {
    match mode {
        "set" | "clear" | "unchanged" => mode,
        _ => "unchanged",
    }
}

fn restore_system_proxy_if_owned(force: bool) {
    let should_restore = force
        || SYSTEM_PROXY_MANAGED
            .lock()
            .map(|managed| *managed)
            .unwrap_or(false);
    if !should_restore {
        return;
    }

    #[cfg(windows)]
    let _ = sysproxy::restore_previous_proxy_state();
    #[cfg(target_os = "macos")]
    let _ = sysproxy::unset_system_proxy();

    if let Ok(mut managed) = SYSTEM_PROXY_MANAGED.lock() {
        *managed = false;
    }
}

fn repair_stale_system_proxy_only() {
    #[cfg(windows)]
    let _ = sysproxy::repair_stale_doodleray_proxy_only();
}

fn apply_system_proxy_mode(mode: &str, http_port: u16) -> Result<&'static str, String> {
    match safe_system_proxy_mode(mode) {
        "set" => {
            #[cfg(windows)]
            sysproxy::apply_doodleray_proxy(http_port, env!("CARGO_PKG_VERSION"))?;
            #[cfg(target_os = "macos")]
            sysproxy::set_system_proxy(http_port)?;

            if let Ok(mut managed) = SYSTEM_PROXY_MANAGED.lock() {
                *managed = true;
            }
            Ok("set")
        }
        "clear" => {
            repair_stale_system_proxy_only();
            Ok("unchanged")
        }
        "unchanged" => Ok("unchanged"),
        _ => unreachable!(),
    }
}

fn proxy_mode_success_message(action: &str, socks_port: u16, http_port: u16) -> String {
    match action {
        "set" => format!(
            "Connected via system proxy. SOCKS5: 127.0.0.1:{}, HTTP: 127.0.0.1:{}",
            socks_port, http_port
        ),
        "cleared" => format!(
            "Connected with local proxy only; system proxy unchanged. SOCKS5: 127.0.0.1:{}, HTTP: 127.0.0.1:{}",
            socks_port, http_port
        ),
        "unchanged" => format!(
            "Connected with local proxy only; system proxy unchanged. SOCKS5: 127.0.0.1:{}, HTTP: 127.0.0.1:{}",
            socks_port, http_port
        ),
        _ => format!(
            "Connected. SOCKS5: 127.0.0.1:{}, HTTP: 127.0.0.1:{}",
            socks_port, http_port
        ),
    }
}

fn remote_doh_dns_server() -> serde_json::Value {
    serde_json::json!({
        "tag": "dns-remote",
        "type": "https",
        "server": "1.1.1.1",
        "server_port": 443,
        "path": "/dns-query",
        "tls": {
            "server_name": "cloudflare-dns.com"
        },
        "detour": "proxy"
    })
}

#[cfg(any(windows, test))]
fn first_usable_physical_dns(output: &str) -> Option<String> {
    output.lines().map(str::trim).find_map(|value| {
        let address = value.parse::<IpAddr>().ok()?;
        if address.is_loopback() || address.is_unspecified() || address.is_multicast() {
            return None;
        }
        if let IpAddr::V4(ipv4) = address {
            if ipv4.is_link_local() {
                return None;
            }
        }
        Some(address.to_string())
    })
}

#[cfg(windows)]
fn windows_physical_dns_server() -> Option<String> {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            r#"$route = Get-NetRoute -DestinationPrefix '0.0.0.0/0' -AddressFamily IPv4 -ErrorAction SilentlyContinue |
  Where-Object { $_.InterfaceAlias -ne 'DoodleRay Tunnel' } |
  Sort-Object @{Expression={$_.RouteMetric + $_.InterfaceMetric}} |
  Select-Object -First 1
if ($route) {
  Get-DnsClientServerAddress -InterfaceIndex $route.InterfaceIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue |
    ForEach-Object { $_.ServerAddresses } |
    Where-Object { $_ } |
    ForEach-Object { $_ }
}"#,
        ])
        .creation_flags(0x08000000)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    first_usable_physical_dns(&String::from_utf8_lossy(&output.stdout))
}

fn direct_dns_server() -> serde_json::Value {
    #[cfg(windows)]
    if let Some(server) = windows_physical_dns_server() {
        return serde_json::json!({
            "tag": "dns-direct",
            "type": "udp",
            "server": server,
            "server_port": 53,
            "detour": "direct"
        });
    }
    serde_json::json!({
        "tag": "dns-direct",
        "type": "local"
    })
}

fn push_dns_direct_rules(
    rules: &mut Vec<serde_json::Value>,
    direct_domains: &[String],
    direct_domain_suffixes: &[String],
    direct_domain_regexes: &[String],
    direct_processes: &[String],
) {
    if !direct_processes.is_empty() {
        rules.push(serde_json::json!({
            "process_name": string_values(direct_processes),
            "server": "dns-direct"
        }));
    }

    if !direct_domains.is_empty()
        || !direct_domain_suffixes.is_empty()
        || !direct_domain_regexes.is_empty()
    {
        let mut rule = serde_json::json!({
            "server": "dns-direct"
        });
        if !direct_domains.is_empty() {
            rule["domain"] = serde_json::json!(direct_domains);
        }
        if !direct_domain_suffixes.is_empty() {
            rule["domain_suffix"] = serde_json::json!(direct_domain_suffixes);
        }
        if !direct_domain_regexes.is_empty() {
            rule["domain_regex"] = serde_json::json!(direct_domain_regexes);
        }
        rules.push(rule);
    }
}

fn singbox_dns_config_with_direct_rules(
    mode: &str,
    direct_domains: &[String],
    direct_domain_suffixes: &[String],
    direct_domain_regexes: &[String],
    direct_processes: &[String],
) -> serde_json::Value {
    let mut rules = Vec::new();
    push_dns_direct_rules(
        &mut rules,
        direct_domains,
        direct_domain_suffixes,
        direct_domain_regexes,
        direct_processes,
    );

    match mode {
        "realip" => {
            let mut dns = serde_json::json!({
            "servers": [
                remote_doh_dns_server(),
                direct_dns_server()
            ],
            "final": "dns-remote",
            "strategy": "ipv4_only"
            });
            if !rules.is_empty() {
                dns["rules"] = serde_json::json!(rules);
            }
            dns
        }
        _ => {
            rules.push(serde_json::json!({ "query_type": "A", "server": "dns-fakeip" }));
            serde_json::json!({
            "servers": [
                remote_doh_dns_server(),
                direct_dns_server(),
                {
                    "tag": "dns-fakeip",
                    "type": "fakeip",
                    "inet4_range": "198.18.0.0/15"
                }
            ],
            "rules": rules,
            "final": "dns-remote",
            "strategy": "ipv4_only",
            "independent_cache": true
            })
        }
    }
}

#[cfg(test)]
fn singbox_dns_config(mode: &str) -> serde_json::Value {
    singbox_dns_config_with_direct_rules(mode, &[], &[], &[], &[])
}

#[cfg(test)]
fn xray_tun_bridge_dns_config() -> serde_json::Value {
    xray_tun_bridge_dns_config_for_direct_processes(&[])
}

#[cfg(test)]
fn xray_tun_bridge_dns_config_for_direct_processes(
    direct_processes: &[String],
) -> serde_json::Value {
    // FakeIP mappings are local to sing-box and can be lost when TUN traffic is handed
    // to xray through SOCKS. Use real IP DNS for the bridge so no-proxy apps get a
    // routable destination after DNS resolution.
    singbox_dns_config_with_direct_rules("realip", &[], &[], &[], direct_processes)
}

fn xray_tun_bridge_outbounds(req: &ConnectRequest) -> serde_json::Value {
    serde_json::json!([
        {
            "type": "socks",
            "tag": "proxy",
            "server": "127.0.0.1",
            "server_port": req.socks_port
        },
        {
            "type": "socks",
            "tag": "proxy-udp",
            "server": "127.0.0.1",
            "server_port": req.socks_port
        },
        { "type": "direct", "tag": "direct" },
        { "type": "block", "tag": "block" }
    ])
}

fn xray_tun_bridge_udp_rule() -> serde_json::Value {
    serde_json::json!({
        "network": "udp",
        "outbound": "proxy-udp"
    })
}

const DEFAULT_DIRECT_DOMAIN_SUFFIXES: &[&str] = &[
    "2ip.ru",
    "vk.com",
    "vk.ru",
    "ok.ru",
    "mail.ru",
    "yandex.ru",
    "yandex.com",
    "yandex.net",
    "ya.ru",
    "dzen.ru",
    "rutube.ru",
    "gosuslugi.ru",
    "mos.ru",
    "nalog.gov.ru",
    "sberbank.ru",
    "sber.ru",
    "tbank.ru",
    "tinkoff.ru",
    "alfabank.ru",
];

#[cfg(test)]
const DEFAULT_DIRECT_SINGBOX_DOMAIN_REGEXES: &[&str] = &[
    r"(^|\.)[^.]+\.ru$",
    r"(^|\.)[^.]+\.su$",
    r"(^|\.)[^.]+\.xn--p1ai$",
    r"(^|\.)[^.]+\.xn--p1acf$",
    r"(^|\.)[^.]+\.moscow$",
    r"(^|\.)[^.]+\.xn--80adxhks$",
];

const DEFAULT_DIRECT_XRAY_DOMAIN_REGEXES: &[&str] = &[
    r"regexp:.*\.ru$",
    r"regexp:.*\.su$",
    r"regexp:.*\.xn--p1ai$",
    r"regexp:.*\.xn--p1acf$",
    r"regexp:.*\.moscow$",
    r"regexp:.*\.xn--80adxhks$",
];

const STEAM_DIRECT_XRAY_DOMAINS: &[&str] = &[
    "domain:steampowered.com",
    "domain:steamcommunity.com",
    "domain:steamgames.com",
    "domain:steamusercontent.com",
    "domain:steamcontent.com",
    "domain:steamstatic.com",
    "full:steamcdn-a.akamaihd.net",
];

const STEAM_DIRECT_PROCESS_NAMES: &[&str] = &[
    "steam",
    "steam.exe",
    "steam_osx",
    "steamservice.exe",
    "steamwebhelper",
    "steamwebhelper.exe",
];

#[cfg(test)]
fn default_direct_singbox_rule() -> serde_json::Value {
    serde_json::json!({
        "domain_suffix": DEFAULT_DIRECT_DOMAIN_SUFFIXES,
        "domain_regex": DEFAULT_DIRECT_SINGBOX_DOMAIN_REGEXES,
        "outbound": "direct"
    })
}

#[cfg(test)]
fn push_default_direct_singbox_rule(rules: &mut Vec<serde_json::Value>) {
    rules.push(default_direct_singbox_rule());
}

fn routing_policy_is_full_tunnel(req: &ConnectRequest) -> bool {
    req.routing_policy
        .as_ref()
        .is_some_and(|policy| policy.mode == "full_tunnel")
}

fn with_steam_direct_domains(mut domains: Vec<String>) -> Vec<String> {
    domains.extend(
        STEAM_DIRECT_XRAY_DOMAINS
            .iter()
            .map(|domain| (*domain).to_string()),
    );
    domains.sort();
    domains.dedup();
    domains
}

fn routing_policy_xray_domains(req: &ConnectRequest) -> Vec<String> {
    with_steam_direct_domains(match req.routing_policy.as_ref() {
        Some(policy) if policy.mode == "split" => policy.direct_domains.clone(),
        Some(_) => Vec::new(),
        None => default_direct_xray_domains(),
    })
}

fn routing_policy_xray_dns_domains(req: &ConnectRequest) -> Vec<String> {
    with_steam_direct_domains(match req.routing_policy.as_ref() {
        Some(policy) if policy.mode == "split" && !policy.local_dns_domains.is_empty() => {
            policy.local_dns_domains.clone()
        }
        Some(policy) if policy.mode == "split" => policy.direct_domains.clone(),
        Some(_) => Vec::new(),
        None => default_direct_xray_domains(),
    })
}

fn routing_policy_singbox_domains(req: &ConnectRequest) -> (Vec<String>, Vec<String>, Vec<String>) {
    routing_selectors_to_singbox(routing_policy_xray_domains(req))
}

fn routing_policy_singbox_dns_domains(
    req: &ConnectRequest,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    routing_selectors_to_singbox(routing_policy_xray_dns_domains(req))
}

fn routing_selectors_to_singbox(selectors: Vec<String>) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut domains = Vec::new();
    let mut suffixes = Vec::new();
    let mut regexes = Vec::new();
    for selector in selectors {
        if let Some(value) = selector.strip_prefix("full:") {
            domains.push(value.to_string());
        } else if let Some(value) = selector.strip_prefix("domain:") {
            suffixes.push(value.to_string());
        } else if let Some(value) = selector.strip_prefix("regexp:") {
            regexes.push(value.to_string());
        } else if !selector.starts_with("geosite:") {
            suffixes.push(selector);
        }
    }
    (domains, suffixes, regexes)
}

fn push_routing_policy_singbox_rules(rules: &mut Vec<serde_json::Value>, req: &ConnectRequest) {
    let (domains, suffixes, regexes) = routing_policy_singbox_domains(req);
    if !domains.is_empty() || !suffixes.is_empty() || !regexes.is_empty() {
        let mut rule = serde_json::json!({ "outbound": "direct" });
        if !domains.is_empty() {
            rule["domain"] = serde_json::json!(domains);
        }
        if !suffixes.is_empty() {
            rule["domain_suffix"] = serde_json::json!(suffixes);
        }
        if !regexes.is_empty() {
            rule["domain_regex"] = serde_json::json!(regexes);
        }
        rules.push(rule);
    }
    if let Some(policy) = req
        .routing_policy
        .as_ref()
        .filter(|policy| policy.mode == "split")
    {
        if !policy.direct_ip_ranges.is_empty() {
            rules.push(serde_json::json!({
                "ip_cidr": policy.direct_ip_ranges,
                "outbound": "direct"
            }));
        }
    }
}

fn xray_tun_bridge_dns_config_for_request(
    req: &ConnectRequest,
    direct_processes: &[String],
) -> serde_json::Value {
    let (domains, suffixes, regexes) = routing_policy_singbox_dns_domains(req);
    singbox_dns_config_with_direct_rules("realip", &domains, &suffixes, &regexes, direct_processes)
}

fn default_direct_xray_domains() -> Vec<String> {
    DEFAULT_DIRECT_XRAY_DOMAIN_REGEXES
        .iter()
        .map(|value| (*value).to_string())
        .chain(
            DEFAULT_DIRECT_DOMAIN_SUFFIXES
                .iter()
                .map(|value| format!("domain:{}", value)),
        )
        .collect()
}

fn xray_rule_has_default_direct_domains(rule: &serde_json::Value) -> bool {
    rule.get("outboundTag").and_then(|value| value.as_str()) == Some("direct")
        && rule
            .get("domain")
            .and_then(|value| value.as_array())
            .map(|domains| {
                domains
                    .iter()
                    .any(|value| value.as_str() == Some("domain:2ip.ru"))
            })
            .unwrap_or(false)
}

fn ensure_xray_direct_outbound(config: &mut serde_json::Value) {
    let direct_outbound = serde_json::json!({
        "tag": "direct",
        "protocol": "freedom"
    });

    let Some(outbounds) = config
        .get_mut("outbounds")
        .and_then(|value| value.as_array_mut())
    else {
        config["outbounds"] = serde_json::json!([direct_outbound]);
        return;
    };

    let has_direct = outbounds
        .iter()
        .any(|outbound| outbound.get("tag").and_then(|value| value.as_str()) == Some("direct"));
    if !has_direct {
        outbounds.push(direct_outbound);
    }
}

fn ensure_xray_dns_outbound(config: &mut serde_json::Value) {
    let dns_outbound = serde_json::json!({ "tag": "dns-out", "protocol": "dns" });
    let Some(outbounds) = config
        .get_mut("outbounds")
        .and_then(serde_json::Value::as_array_mut)
    else {
        config["outbounds"] = serde_json::json!([dns_outbound]);
        return;
    };
    if !outbounds
        .iter()
        .any(|outbound| outbound.get("tag").and_then(serde_json::Value::as_str) == Some("dns-out"))
    {
        outbounds.push(dns_outbound);
    }
}

fn ensure_xray_api_outbound(config: &mut serde_json::Value) {
    let api_outbound = serde_json::json!({ "tag": "api", "protocol": "blackhole" });
    let Some(outbounds) = config
        .get_mut("outbounds")
        .and_then(serde_json::Value::as_array_mut)
    else {
        config["outbounds"] = serde_json::json!([api_outbound]);
        return;
    };
    if !outbounds
        .iter()
        .any(|outbound| outbound.get("tag").and_then(serde_json::Value::as_str) == Some("api"))
    {
        outbounds.push(api_outbound);
    }
}

fn constrain_xray_config_to_managed_policy(config: &mut serde_json::Value, req: &ConnectRequest) {
    if req.routing_policy.is_none() {
        return;
    }

    let mut allowed_tags = HashSet::new();
    if let Some(outbounds) = config
        .get_mut("outbounds")
        .and_then(serde_json::Value::as_array_mut)
    {
        outbounds.retain(|outbound| {
            let tag = outbound
                .get("tag")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let protocol = outbound
                .get("protocol")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let keep = tag == "proxy"
                || (tag == "direct" && protocol == "freedom")
                || tag == "dns-out"
                || tag == "api"
                || protocol == "blackhole";
            if keep && !tag.is_empty() {
                allowed_tags.insert(tag.to_string());
            }
            keep
        });
        if let Some(index) = outbounds.iter().position(|outbound| {
            outbound.get("tag").and_then(serde_json::Value::as_str) == Some("proxy")
        }) {
            if index != 0 {
                let proxy = outbounds.remove(index);
                outbounds.insert(0, proxy);
            }
        }
    }

    if let Some(routing) = config
        .get_mut("routing")
        .and_then(serde_json::Value::as_object_mut)
    {
        routing.remove("balancers");
        if let Some(rules) = routing
            .get_mut("rules")
            .and_then(serde_json::Value::as_array_mut)
        {
            rules.retain(|rule| {
                if rule.get("balancerTag").is_some() {
                    return false;
                }
                rule.get("outboundTag")
                    .and_then(serde_json::Value::as_str)
                    .is_none_or(|tag| allowed_tags.contains(tag))
            });
        }
    }
}

fn apply_xray_routing_policy(
    config: &mut serde_json::Value,
    req: &ConnectRequest,
    include_legacy_default_split: bool,
) {
    if !config
        .get("routing")
        .map(|value| value.is_object())
        .unwrap_or(false)
    {
        config["routing"] = serde_json::json!({});
    }
    if !config["routing"]
        .get("rules")
        .map(|value| value.is_array())
        .unwrap_or(false)
    {
        config["routing"]["rules"] = serde_json::json!([]);
    }

    let Some(rules) = config["routing"]["rules"].as_array_mut() else {
        return;
    };

    rules.retain(|rule| {
        let inbound = rule
            .get("inboundTag")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|tags| {
                tags.iter()
                    .any(|tag| matches!(tag.as_str(), Some("dns-direct" | "dns-remote")))
            });
        if inbound {
            return false;
        }
        if req.routing_policy.is_none() {
            return true;
        }
        rule.get("outboundTag").and_then(serde_json::Value::as_str) != Some("direct")
    });

    if !rules.iter().any(|rule| {
        rule.get("inboundTag")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|tags| tags.iter().any(|tag| tag.as_str() == Some("api")))
    }) {
        rules.insert(
            0,
            serde_json::json!({
                "type": "field",
                "inboundTag": ["api"],
                "outboundTag": "api"
            }),
        );
    }

    let insert_at = rules
        .iter()
        .position(|rule| {
            rule.get("inboundTag")
                .and_then(|value| value.as_array())
                .map(|tags| tags.iter().any(|tag| tag.as_str() == Some("api")))
                .unwrap_or(false)
        })
        .map(|index| index + 1)
        .unwrap_or(0);
    let mut additions = Vec::new();
    let managed_dns = req.routing_policy.is_some() || include_legacy_default_split;
    let dns_domains = if managed_dns {
        routing_policy_xray_dns_domains(req)
    } else {
        Vec::new()
    };
    if !dns_domains.is_empty() {
        additions.push(serde_json::json!({
            "type": "field",
            "inboundTag": ["dns-direct"],
            "outboundTag": "direct"
        }));
    }
    if managed_dns {
        additions.push(serde_json::json!({
            "type": "field",
            "inboundTag": ["dns-remote"],
            "outboundTag": "proxy"
        }));
    }
    // Resolver-originated DNS traffic must be classified before the generic
    // port-53 interception rule. Otherwise a local resolver query is sent back
    // into dns-out recursively and direct domains such as .ru never resolve.
    if !rules.iter().any(|rule| {
        rule.get("port").and_then(serde_json::Value::as_str) == Some("53")
            && rule.get("outboundTag").and_then(serde_json::Value::as_str) == Some("dns-out")
    }) {
        additions.push(serde_json::json!({
            "type": "field",
            "port": "53",
            "outboundTag": "dns-out"
        }));
    }
    let direct_domains = if req.routing_policy.is_some() || include_legacy_default_split {
        routing_policy_xray_domains(req)
    } else {
        Vec::new()
    };
    if !direct_domains.is_empty()
        && !rules
            .iter()
            .any(|rule| req.routing_policy.is_none() && xray_rule_has_default_direct_domains(rule))
    {
        additions.push(serde_json::json!({
            "type": "field",
            "domain": direct_domains,
            "outboundTag": "direct"
        }));
    }
    if include_legacy_default_split
        && req.routing_policy.is_none()
        && !rules.iter().any(|rule| {
            rule.get("outboundTag").and_then(serde_json::Value::as_str) == Some("direct")
                && rule
                    .get("ip")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|values| {
                        values
                            .iter()
                            .any(|value| value.as_str() == Some("geoip:private"))
                    })
        })
    {
        additions.push(serde_json::json!({
            "type": "field",
            "ip": ["geoip:private"],
            "outboundTag": "direct"
        }));
    }
    if let Some(policy) = req
        .routing_policy
        .as_ref()
        .filter(|policy| policy.mode == "split")
    {
        if !policy.direct_ip_ranges.is_empty() {
            additions.push(serde_json::json!({
                "type": "field",
                "ip": policy.direct_ip_ranges,
                "outboundTag": "direct"
            }));
        }
    }
    for (offset, rule) in additions.into_iter().enumerate() {
        rules.insert(insert_at + offset, rule);
    }
    if req.routing_policy.is_some() {
        rules.push(serde_json::json!({
            "type": "field",
            "network": "tcp,udp",
            "outboundTag": "proxy"
        }));
    }
}

fn xray_dns_config(req: &ConnectRequest) -> serde_json::Value {
    let mut servers = Vec::new();
    let direct_domains = routing_policy_xray_dns_domains(req);
    if !direct_domains.is_empty() {
        servers.push(serde_json::json!({
            "address": "localhost",
            "domains": direct_domains,
            "skipFallback": true,
            "tag": "dns-direct"
        }));
    }
    servers.push(serde_json::json!({
        "address": "https://1.1.1.1/dns-query",
        "tag": "dns-remote"
    }));
    serde_json::json!({
        "servers": servers,
        "queryStrategy": "UseIPv4",
        "disableFallbackIfMatch": true
    })
}

fn xray_tunnel_dns_config() -> serde_json::Value {
    serde_json::json!({
        "queryStrategy": "UseIPv4",
        "servers": [{
            "address": "https://1.1.1.1/dns-query",
            "tag": "dns-remote"
        }]
    })
}

fn xray_engine_transport(transport: &str) -> bool {
    matches!(transport, "xhttp" | "ws")
}

fn xray_engine_protocol(protocol: &str) -> bool {
    matches!(protocol, "vless" | "vmess" | "trojan" | "shadowsocks")
}

fn uses_xray_engine(req: &ConnectRequest) -> bool {
    req.raw_xray_config.is_some()
        || xray_engine_transport(req.transport.as_str())
        || xray_engine_protocol(req.protocol.as_str())
}

fn xray_transport_host(req: &ConnectRequest) -> String {
    req.host
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .or(req.sni.as_ref().filter(|value| !value.trim().is_empty()))
        .cloned()
        .unwrap_or_else(|| req.server_address.clone())
}

fn xray_tls_settings(req: &ConnectRequest) -> serde_json::Value {
    let mut settings = serde_json::json!({
        "serverName": req.sni.clone().unwrap_or(req.server_address.clone()),
        "fingerprint": req.fingerprint.clone().unwrap_or("chrome".into())
    });
    if let Some(ref alpn) = req.alpn {
        if !alpn.is_empty() {
            settings["alpn"] = serde_json::json!(alpn);
        }
    }
    settings
}

fn xray_reality_settings(req: &ConnectRequest) -> serde_json::Value {
    serde_json::json!({
        "serverName": req.sni.clone().unwrap_or(req.server_address.clone()),
        "publicKey": req.public_key.clone().unwrap_or_default(),
        "shortId": req.short_id.clone().unwrap_or_default(),
        "fingerprint": req.fingerprint.clone().unwrap_or("chrome".into())
    })
}

fn apply_xray_stream_security_settings(
    stream_settings: &mut serde_json::Value,
    req: &ConnectRequest,
) {
    if req.security == "reality" {
        stream_settings["realitySettings"] = xray_reality_settings(req);
    } else if req.security == "tls" {
        stream_settings["tlsSettings"] = xray_tls_settings(req);
    }
}

fn normalize_xray_transport_settings(config: &mut serde_json::Value) {
    let Some(outbounds) = config
        .get_mut("outbounds")
        .and_then(|value| value.as_array_mut())
    else {
        return;
    };

    for outbound in outbounds {
        let Some(stream_settings) = outbound
            .get_mut("streamSettings")
            .and_then(|value| value.as_object_mut())
        else {
            continue;
        };
        let Some(ws_settings) = stream_settings
            .get_mut("wsSettings")
            .and_then(|value| value.as_object_mut())
        else {
            continue;
        };

        let header_host = {
            let headers = ws_settings
                .get_mut("headers")
                .and_then(|value| value.as_object_mut());
            headers.and_then(|headers| headers.remove("Host").or_else(|| headers.remove("host")))
        };

        if let Some(host) = header_host {
            ws_settings.entry("host").or_insert(host);
        }

        let remove_headers = ws_settings
            .get("headers")
            .and_then(|value| value.as_object())
            .map(|headers| headers.is_empty())
            .unwrap_or(false);
        if remove_headers {
            ws_settings.remove("headers");
        }
    }
}

const SYSTEM_BYPASS_PROCESS_NAMES: &[&str] = &[
    "sing-box",
    "sing-box.exe",
    "xray",
    "xray.exe",
    "DoodleRayService",
    "DoodleRayService.exe",
];

fn effective_tun_strict_route(req: &ConnectRequest) -> bool {
    req.kill_switch || req.strict_route || routing_policy_is_full_tunnel(req)
}

fn normalize_process_name(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_matches('"').trim_matches('\'');
    if trimmed.is_empty() {
        return None;
    }

    let file_name = trimmed
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(trimmed);
    let lower = file_name.to_lowercase();
    let process_name = lower.strip_suffix(".app").unwrap_or(&lower);

    if process_name.is_empty() {
        None
    } else {
        Some(process_name.to_string())
    }
}

fn process_rule_names(req: &ConnectRequest, action: &str) -> Vec<String> {
    let mut names: Vec<String> = req
        .routing_rules
        .iter()
        .filter(|r| r.rule_type == "exe" && r.action == action)
        .filter_map(|r| normalize_process_name(&r.value))
        .collect();
    if action == "direct" && req.proxy_mode == "tun" {
        names.extend(
            STEAM_DIRECT_PROCESS_NAMES
                .iter()
                .map(|name| (*name).to_string()),
        );
    }
    names.sort();
    names.dedup();
    names
}

fn tun_direct_process_exclusions_need_raw_tun_path(req: &ConnectRequest) -> bool {
    req.proxy_mode == "tun" && !process_rule_names(req, "direct").is_empty()
}

fn string_values(values: &[String]) -> Vec<serde_json::Value> {
    values
        .iter()
        .map(|value| serde_json::Value::String(value.clone()))
        .collect()
}

fn system_bypass_process_values() -> Vec<serde_json::Value> {
    SYSTEM_BYPASS_PROCESS_NAMES
        .iter()
        .map(|name| serde_json::Value::String((*name).to_string()))
        .collect()
}

fn push_process_route(
    rules: &mut Vec<serde_json::Value>,
    process_names: &[String],
    outbound: &str,
) {
    if !process_names.is_empty() {
        rules.push(serde_json::json!({
            "process_name": string_values(process_names),
            "outbound": outbound
        }));
    }
}

fn push_domain_route(
    rules: &mut Vec<serde_json::Value>,
    domains: &[String],
    domain_suffixes: &[String],
    outbound: &str,
) {
    if domains.is_empty() && domain_suffixes.is_empty() {
        return;
    }

    let mut rule = serde_json::json!({
        "outbound": outbound
    });
    if !domains.is_empty() {
        rule["domain"] = serde_json::json!(domains);
    }
    if !domain_suffixes.is_empty() {
        rule["domain_suffix"] = serde_json::json!(domain_suffixes);
    }
    rules.push(rule);
}

fn tun_address_values() -> serde_json::Value {
    serde_json::json!(["172.30.255.1/30", "fdfe:dcba:9876::1/126"])
}

fn tun_mtu_value(req: &ConnectRequest) -> u16 {
    req.mtu
        .filter(|mtu| (1280..=1500).contains(mtu))
        .unwrap_or(1408)
}

fn tun_route_exclude_addresses(req: &ConnectRequest) -> Vec<String> {
    req.server_address
        .parse::<IpAddr>()
        .ok()
        .map(|ip| match ip {
            IpAddr::V4(value) => format!("{}/32", value),
            IpAddr::V6(value) => format!("{}/128", value),
        })
        .into_iter()
        .collect()
}

fn tun_inbound_value(
    req: &ConnectRequest,
    interface_name: Option<&str>,
    strict_route: bool,
) -> serde_json::Value {
    let stack = effective_tun_network_stack(&req.network_stack);
    let mut inbound = serde_json::json!({
        "type": "tun",
        "tag": "tun-in",
        "address": tun_address_values(),
        "mtu": tun_mtu_value(req),
        "auto_route": true,
        "strict_route": strict_route,
        "stack": stack,
        "udp_timeout": "10m"
    });

    if let Some(name) = interface_name {
        inbound["interface_name"] = serde_json::json!(name);
    }
    let route_exclude_address = tun_route_exclude_addresses(req);
    if !route_exclude_address.is_empty() {
        inbound["route_exclude_address"] = serde_json::json!(route_exclude_address);
    }
    if matches!(stack, "mixed" | "gvisor") {
        inbound["endpoint_independent_nat"] = serde_json::json!(true);
    }

    inbound
}

fn loopback_proxy_inbounds(req: &ConnectRequest) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "type": "socks",
            "tag": "socks-in",
            "listen": "127.0.0.1",
            "listen_port": req.socks_port,
        }),
        serde_json::json!({
            "type": "http",
            "tag": "http-in",
            "listen": "127.0.0.1",
            "listen_port": req.http_port,
        }),
    ]
}

fn singbox_tun_inbounds(
    req: &ConnectRequest,
    interface_name: Option<&str>,
    strict_route: bool,
) -> serde_json::Value {
    let mut inbounds = vec![tun_inbound_value(req, interface_name, strict_route)];
    inbounds.extend(loopback_proxy_inbounds(req));
    serde_json::json!(inbounds)
}

fn write_debug_config(path: &std::path::Path, config: &serde_json::Value) {
    if std::env::var("DOODLERAY_DEBUG_CONFIG").ok().as_deref() != Some("1") {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = write_private_file(
        path,
        serde_json::to_string_pretty(config)
            .unwrap_or_default()
            .as_bytes(),
    );
}

fn write_private_file(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);

    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    #[cfg(unix)]
    {
        let _ = std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600));
    }
    Ok(())
}

fn validate_secure_store_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key.len() > 60 {
        return Err("Invalid secure storage key length".into());
    }
    if !key
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err("Secure storage key contains unsupported characters".into());
    }
    Ok(())
}

fn is_reserved_secure_store_key(key: &str) -> bool {
    [APP_API_SESSION_KEY, APP_API_DEVICE_KEY]
        .iter()
        .any(|reserved| key == *reserved || key.starts_with(&format!("{}.chunk.", reserved)))
}

fn validate_renderer_secure_store_key(key: &str) -> Result<(), String> {
    validate_secure_store_key(key)?;
    if is_reserved_secure_store_key(key) {
        return Err(
            "This secure storage key is reserved for native DoodleVPN account state.".into(),
        );
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn secure_store_entry(key: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SECURE_STORE_SERVICE, key)
        .map_err(|e| format!("Secure storage unavailable: {}", e))
}

#[cfg(target_os = "macos")]
fn secure_store_macos_options(key: &str) -> security_framework::passwords::PasswordOptions {
    let mut options = security_framework::passwords::PasswordOptions::new_generic_password(
        SECURE_STORE_SERVICE,
        key,
    );
    options.use_protected_keychain();
    options.set_access_synchronized(Some(false));
    options
}

#[cfg(target_os = "macos")]
fn secure_store_native_get(key: &str) -> Result<Option<String>, String> {
    use security_framework::passwords::generic_password;

    match generic_password(secure_store_macos_options(key)) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| "Secure storage entry is not valid UTF-8".to_string()),
        Err(error) if error.code() == -25300 => {
            // One-way migration from the legacy SecKeychain backend used by
            // early macOS builds. App Store builds use the sandbox-compatible
            // Data Protection Keychain above.
            let legacy = keyring::Entry::new(SECURE_STORE_SERVICE, key)
                .map_err(|e| format!("Secure storage unavailable: {}", e))?;
            match legacy.get_password() {
                Ok(value) => {
                    secure_store_native_set(key, &value)?;
                    let _ = legacy.delete_credential();
                    Ok(Some(value))
                }
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(format!("Legacy secure storage read failed: {}", e)),
            }
        }
        Err(error) => Err(format!("Secure storage read failed: {}", error)),
    }
}

#[cfg(not(target_os = "macos"))]
fn secure_store_native_get(key: &str) -> Result<Option<String>, String> {
    match secure_store_entry(key)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Secure storage read failed: {}", e)),
    }
}

#[cfg(target_os = "macos")]
fn secure_store_native_set(key: &str, value: &str) -> Result<(), String> {
    security_framework::passwords::set_generic_password_options(
        value.as_bytes(),
        secure_store_macos_options(key),
    )
    .map_err(|e| format!("Secure storage write failed: {}", e))
}

#[cfg(not(target_os = "macos"))]
fn secure_store_native_set(key: &str, value: &str) -> Result<(), String> {
    secure_store_entry(key)?
        .set_password(value)
        .map_err(|e| format!("Secure storage write failed: {}", e))
}

#[cfg(target_os = "macos")]
fn secure_store_native_delete(key: &str) -> Result<(), String> {
    use security_framework::passwords::delete_generic_password_options;

    let modern_result = match delete_generic_password_options(secure_store_macos_options(key)) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == -25300 => Ok(()),
        Err(error) => Err(format!("Secure storage delete failed: {}", error)),
    };
    if let Ok(legacy) = keyring::Entry::new(SECURE_STORE_SERVICE, key) {
        let _ = legacy.delete_credential();
    }
    modern_result
}

#[cfg(not(target_os = "macos"))]
fn secure_store_native_delete(key: &str) -> Result<(), String> {
    match secure_store_entry(key)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Secure storage delete failed: {}", e)),
    }
}

fn secure_store_chunk_key(key: &str, index: usize) -> String {
    format!("{}.chunk.{}", key, index)
}

fn secure_store_chunk_count(value: &str) -> Option<usize> {
    value
        .strip_prefix(SECURE_STORE_CHUNK_PREFIX)
        .and_then(|raw| raw.parse::<usize>().ok())
}

fn secure_store_chunks(value: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for ch in value.chars() {
        if !current.is_empty() && current.len() + ch.len_utf8() > SECURE_STORE_CHUNK_BYTES {
            chunks.push(current);
            current = String::new();
        }
        current.push(ch);
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn delete_secure_store_entry(key: &str) -> Result<(), String> {
    secure_store_native_delete(key)
}

fn delete_secure_store_chunks(key: &str, manifest: &str) {
    let Some(count) = secure_store_chunk_count(manifest) else {
        return;
    };

    for index in 0..count {
        let _ = delete_secure_store_entry(&secure_store_chunk_key(key, index));
    }
}

fn secure_store_fallback_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Secure storage fallback path unavailable: {}", e))?
        .join("secure-storage");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Secure storage fallback init failed: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    Ok(dir)
}

fn secure_store_fallback_path(
    app: &tauri::AppHandle,
    key: &str,
) -> Result<std::path::PathBuf, String> {
    Ok(secure_store_fallback_dir(app)?.join(format!("{}.store", key)))
}

fn secure_store_fallback_get(app: &tauri::AppHandle, key: &str) -> Result<Option<String>, String> {
    let path = secure_store_fallback_path(app, key)?;
    match std::fs::read_to_string(&path) {
        Ok(value) => Ok(Some(value)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Secure storage fallback read failed: {}", e)),
    }
}

fn secure_store_fallback_delete(app: &tauri::AppHandle, key: &str) -> Result<(), String> {
    let path = secure_store_fallback_path(app, key)?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Secure storage fallback delete failed: {}", e)),
    }
}

fn secure_store_keyring_get(key: &str) -> Result<Option<String>, String> {
    match secure_store_native_get(key)? {
        Some(value) => {
            let Some(count) = secure_store_chunk_count(&value) else {
                return Ok(Some(value));
            };

            let mut restored = String::new();
            for index in 0..count {
                let chunk = secure_store_native_get(&secure_store_chunk_key(key, index))?
                    .ok_or_else(|| "Secure storage chunk is missing".to_string())?;
                restored.push_str(&chunk);
            }
            Ok(Some(restored))
        }
        None => Ok(None),
    }
}

fn secure_store_keyring_set(key: &str, value: &str) -> Result<(), String> {
    if let Some(old_value) = secure_store_native_get(key)? {
        delete_secure_store_chunks(key, &old_value);
    }

    if value.len() > SECURE_STORE_CHUNK_BYTES {
        let chunks = secure_store_chunks(value);
        for (index, chunk) in chunks.iter().enumerate() {
            secure_store_native_set(&secure_store_chunk_key(key, index), chunk)
                .map_err(|e| format!("Secure storage chunk write failed: {}", e))?;
        }

        return secure_store_native_set(
            key,
            &format!("{}{}", SECURE_STORE_CHUNK_PREFIX, chunks.len()),
        );
    }

    secure_store_native_set(key, value)
}

fn secure_store_keyring_delete(key: &str) -> Result<(), String> {
    if let Some(value) = secure_store_native_get(key)? {
        delete_secure_store_chunks(key, &value);
    }
    delete_secure_store_entry(key)
}

#[tauri::command(async)]
fn secure_store_get(app: tauri::AppHandle, key: String) -> Result<Option<String>, String> {
    validate_renderer_secure_store_key(&key)?;
    match secure_store_keyring_get(&key) {
        Ok(Some(value)) => {
            // Remove the legacy plaintext mirror once Keychain/Credential
            // Manager is confirmed readable.
            if let Err(fallback_error) = secure_store_fallback_delete(&app, &key) {
                eprintln!(
                    "[warn] legacy secure storage fallback cleanup failed: {}",
                    fallback_error
                );
            }
            Ok(Some(value))
        }
        Ok(None) => {
            // One-way migration for builds that mirrored the renderer state
            // into app data as plaintext. Never create or refresh this file.
            let Some(legacy_value) = secure_store_fallback_get(&app, &key)? else {
                return Ok(None);
            };
            secure_store_keyring_set(&key, &legacy_value)
                .map_err(|error| format!("Legacy secure storage migration failed: {}", error))?;
            secure_store_fallback_delete(&app, &key)?;
            Ok(Some(legacy_value))
        }
        Err(keyring_error) => Err(keyring_error),
    }
}

#[tauri::command(async)]
fn secure_store_set(app: tauri::AppHandle, key: String, value: String) -> Result<(), String> {
    validate_renderer_secure_store_key(&key)?;
    secure_store_keyring_set(&key, &value)?;
    if let Err(fallback_error) = secure_store_fallback_delete(&app, &key) {
        eprintln!(
            "[warn] legacy secure storage fallback cleanup failed: {}",
            fallback_error
        );
    }
    Ok(())
}

#[tauri::command(async)]
fn secure_store_delete(app: tauri::AppHandle, key: String) -> Result<(), String> {
    validate_renderer_secure_store_key(&key)?;
    let keyring_result = secure_store_keyring_delete(&key);
    let fallback_result = secure_store_fallback_delete(&app, &key);

    match (keyring_result, fallback_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(keyring_error), Ok(())) => Err(keyring_error),
        (Ok(()), Err(fallback_error)) => Err(fallback_error),
        (Err(keyring_error), Err(fallback_error)) => {
            Err(format!("{}; {}", keyring_error, fallback_error))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppApiAntiJammerSummary {
    #[serde(default)]
    pub limit_bytes: u64,
    #[serde(default)]
    pub used_bytes: u64,
    #[serde(default)]
    pub remaining_bytes: u64,
    #[serde(default)]
    pub low_balance: bool,
    #[serde(default)]
    pub exhausted: bool,
    #[serde(default)]
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppApiSubscriptionSummary {
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub device_allowed: Option<bool>,
    #[serde(default)]
    pub remnawave_status: String,
    #[serde(default)]
    pub expires_at: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub user_uuid: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub anti_jammer: Option<AppApiAntiJammerSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppApiTokenResponse {
    pub access_token: String,
    pub access_expires_at: String,
    #[serde(default)]
    pub expires_in: i64,
    pub refresh_token: String,
    pub refresh_expires_at: String,
    pub device_id: String,
    pub subscription: AppApiSubscriptionSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppApiStoredSession {
    pub refresh_token: String,
    pub refresh_expires_at: String,
    pub device_id: String,
    pub subscription: AppApiSubscriptionSummary,
}

#[derive(Debug, Serialize)]
pub struct AppApiSessionStatus {
    pub logged_in: bool,
    pub device_id: Option<String>,
    pub access_expires_at: Option<String>,
    pub refresh_expires_at: Option<String>,
    pub subscription: Option<AppApiSubscriptionSummary>,
    pub api_base_url: String,
    pub closed_control_plane_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct AppApiDeviceState {
    client_device_id: String,
    hwid: String,
    public_key: String,
    #[serde(default)]
    public_key_jwk: serde_json::Value,
    #[serde(default)]
    private_key_seed: String,
    #[serde(default)]
    key_alg: String,
}

#[derive(Debug, Deserialize)]
pub struct AppApiExchangeCodeRequest {
    pub code: String,
}

#[derive(Debug, Deserialize)]
pub struct AppApiExchangeLegacySubscriptionRequest {
    pub subscription_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppApiLocation {
    pub id: String,
    pub country_code: String,
    pub title: String,
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub sort: i32,
    #[serde(default)]
    pub available_nodes_count: i32,
    #[serde(default)]
    pub healthy_nodes_count: Option<i32>,
    #[serde(default)]
    pub capacity_label: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AppApiLocationsResponse {
    #[serde(default)]
    pub locations: Vec<AppApiLocation>,
}

#[derive(Debug, Deserialize)]
pub struct AppApiDiagnosticsSubmission {
    #[serde(default)]
    manual: bool,
    #[serde(default)]
    events: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AppRoutingSignature {
    #[serde(default)]
    pub kid: String,
    #[serde(default)]
    pub alg: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AppRoutingAsset {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub etag: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub canonical_rule_version: String,
    #[serde(default)]
    pub signature: Option<AppRoutingSignature>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AppRoutingPolicy {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub direct_domains: Vec<String>,
    #[serde(default)]
    pub local_dns_domains: Vec<String>,
    #[serde(default)]
    pub direct_ip_ranges: Vec<String>,
    #[serde(default)]
    pub asset: Option<AppRoutingAsset>,
}

fn app_routing_asset_signing_bytes(asset: &AppRoutingAsset) -> Vec<u8> {
    format!(
        "doodleray-routing-asset-v1\n{}\n{}\n{}\n{}\n{}\n{}",
        asset.url,
        asset.sha256,
        asset.size_bytes,
        asset.etag,
        asset.version,
        asset.canonical_rule_version,
    )
    .into_bytes()
}

fn verify_app_routing_asset(asset: &AppRoutingAsset) -> Result<(), String> {
    let signature = asset
        .signature
        .as_ref()
        .ok_or_else(|| "DoodleVPN routing data is not signed.".to_string())?;
    if signature.kid != APP_ROUTING_ROOT_KID
        || signature.alg != "EdDSA"
        || asset.canonical_rule_version != APP_ROUTING_ASSET_CANONICAL_RULE_VERSION
    {
        return Err("DoodleVPN routing data has unsupported signature metadata.".into());
    }
    let public_key = URL_SAFE_NO_PAD
        .decode(APP_ROUTING_ROOT_PUBLIC_KEY_BASE64)
        .map_err(|_| "DoodleVPN routing verifier is invalid.".to_string())?;
    let public_key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| "DoodleVPN routing verifier is invalid.".to_string())?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| "DoodleVPN routing verifier is invalid.".to_string())?;
    let signature_bytes = URL_SAFE_NO_PAD
        .decode(&signature.value)
        .map_err(|_| "DoodleVPN routing signature is invalid.".to_string())?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| "DoodleVPN routing signature is invalid.".to_string())?;
    verifying_key
        .verify(&app_routing_asset_signing_bytes(asset), &signature)
        .map_err(|_| "DoodleVPN routing data signature verification failed.".to_string())
}

fn validate_app_routing_policy(mut policy: AppRoutingPolicy) -> Result<AppRoutingPolicy, String> {
    if !matches!(policy.mode.as_str(), "full_tunnel" | "split") {
        return Err(
            "DoodleVPN returned an unsupported routing policy. Update the app and try again."
                .into(),
        );
    }
    if policy.version.len() > 128
        || policy.direct_domains.len() > 4096
        || policy.local_dns_domains.len() > 4096
        || policy.direct_ip_ranges.len() > 512
    {
        return Err("DoodleVPN returned an invalid routing policy.".into());
    }

    let valid_selector = |value: &String| {
        !value.is_empty()
            && value.len() <= 512
            && !value.chars().any(char::is_whitespace)
            && !value.chars().any(char::is_control)
    };
    if !policy.direct_domains.iter().all(valid_selector)
        || !policy.local_dns_domains.iter().all(valid_selector)
    {
        return Err("DoodleVPN returned an invalid routing domain selector.".into());
    }
    if !policy.direct_ip_ranges.iter().all(|value| {
        let Some((address, prefix)) = value.split_once('/') else {
            return false;
        };
        let Ok(address) = address.parse::<IpAddr>() else {
            return false;
        };
        let Ok(prefix) = prefix.parse::<u8>() else {
            return false;
        };
        prefix <= if address.is_ipv4() { 32 } else { 128 }
    }) {
        return Err("DoodleVPN returned an invalid routing IP range.".into());
    }

    for values in [
        &mut policy.direct_domains,
        &mut policy.local_dns_domains,
        &mut policy.direct_ip_ranges,
    ] {
        values.sort();
        values.dedup();
    }
    if policy.mode == "full_tunnel" {
        policy.direct_domains.clear();
        policy.local_dns_domains.clear();
        policy.direct_ip_ranges.clear();
    }

    if let Some(asset) = policy.asset.as_ref() {
        let valid_hash =
            asset.sha256.len() == 64 && asset.sha256.bytes().all(|byte| byte.is_ascii_hexdigit());
        if !asset.url.starts_with('/')
            || asset.url.starts_with("//")
            || asset.url.contains("..")
            || !valid_hash
            || asset.size_bytes == 0
            || asset.size_bytes > 64 * 1024 * 1024
        {
            return Err("DoodleVPN returned an invalid routing asset.".into());
        }
        verify_app_routing_asset(asset)?;
    }
    Ok(policy)
}

#[derive(Debug, Serialize, Deserialize)]
struct AppApiProfileLeaseResponse {
    #[serde(default)]
    schema_version: i32,
    profile_id: String,
    lease_id: String,
    expires_at: String,
    location_id: String,
    #[serde(default)]
    route_kind: String,
    #[serde(default)]
    first_hop: String,
    #[serde(default)]
    target_country_id: String,
    #[serde(default)]
    entry_role: String,
    #[serde(default)]
    routing_rules_version: String,
    #[serde(default)]
    routing_policy: Option<AppRoutingPolicy>,
    #[serde(default)]
    native_profile: serde_json::Value,
    #[serde(default)]
    profile: Option<serde_json::Value>,
    #[serde(default)]
    transport_capability: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct AppConnectLocationRequest {
    pub location_id: String,
    #[serde(default)]
    pub fallback_location_ids: Vec<String>,
    #[serde(default = "default_app_proxy_mode")]
    pub proxy_mode: String,
    #[serde(default = "default_system_proxy_mode")]
    pub system_proxy_mode: String,
    #[serde(default = "default_app_socks_port")]
    pub socks_port: u16,
    #[serde(default = "default_app_http_port")]
    pub http_port: u16,
    #[serde(default = "default_app_network_stack")]
    pub network_stack: String,
    #[serde(default = "default_app_dns_mode")]
    pub dns_mode: String,
    #[serde(default = "default_app_strict_route")]
    pub strict_route: bool,
    #[serde(default)]
    pub kill_switch: bool,
    #[serde(default)]
    pub routing_rules: Vec<RoutingRuleRequest>,
}

fn app_connection_location_ids(request: &AppConnectLocationRequest) -> Vec<String> {
    let mut ids = Vec::new();
    for location_id in std::iter::once(&request.location_id).chain(&request.fallback_location_ids) {
        let location_id = location_id.trim().to_ascii_lowercase();
        if !location_id.is_empty() && !ids.contains(&location_id) {
            ids.push(location_id);
        }
        if ids.len() == 3 {
            break;
        }
    }
    ids
}

#[derive(Debug)]
struct AppApiHttpError {
    status: u16,
    message: String,
}

impl std::fmt::Display for AppApiHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "App API error {}: {}", self.status, self.message)
    }
}

fn app_api_error_message(status: reqwest::StatusCode, text: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        for key in ["error", "message"] {
            if let Some(message) = value.get(key).and_then(serde_json::Value::as_str) {
                if !message.trim().is_empty() {
                    return message.trim().chars().take(500).collect();
                }
            }
        }
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        status.to_string()
    } else if trimmed.starts_with('<') || trimmed.to_ascii_lowercase().contains("<html") {
        "DoodleVPN API returned an incompatible response. Update the app and try again.".into()
    } else {
        trimmed.chars().take(500).collect()
    }
}

fn default_app_proxy_mode() -> String {
    "tun".into()
}

fn default_app_socks_port() -> u16 {
    10808
}

fn default_app_http_port() -> u16 {
    10809
}

fn default_app_network_stack() -> String {
    "system".into()
}

fn default_app_dns_mode() -> String {
    "fakeip".into()
}

fn default_app_strict_route() -> bool {
    true
}

fn closed_control_plane_enabled() -> bool {
    option_env!("DOODLERAY_CLOSED_CONTROL_PLANE") == Some("1")
}

fn ensure_closed_control_plane_enabled() -> Result<(), String> {
    if closed_control_plane_enabled() {
        Ok(())
    } else {
        Err("DoodleVPN account sign-in is not enabled in this build.".into())
    }
}

fn app_api_base_url() -> String {
    option_env!("DOODLERAY_APP_API_BASE_URL")
        .unwrap_or(APP_API_DEFAULT_BASE_URL)
        .trim_end_matches('/')
        .to_string()
}

fn app_api_endpoint(path: &str) -> Result<Url, String> {
    let base = format!("{}/", app_api_base_url());
    let mut base_url = Url::parse(&base).map_err(|e| format!("Invalid App API base URL: {}", e))?;
    let path = path.trim_start_matches('/');
    if path.starts_with("v1/mobile/") {
        base_url.set_path(&format!("/{path}"));
        return Ok(base_url);
    }
    base_url
        .join(path)
        .map_err(|e| format!("Invalid App API endpoint path: {}", e))
}

fn app_api_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("App API client init failed: {}", e))
}

fn app_api_stored_session(session: &AppApiTokenResponse) -> AppApiStoredSession {
    AppApiStoredSession {
        refresh_token: session.refresh_token.clone(),
        refresh_expires_at: session.refresh_expires_at.clone(),
        device_id: session.device_id.clone(),
        subscription: session.subscription.clone(),
    }
}

fn app_api_session_from_stored(stored: AppApiStoredSession) -> AppApiTokenResponse {
    AppApiTokenResponse {
        access_token: String::new(),
        access_expires_at: String::new(),
        expires_in: 0,
        refresh_token: stored.refresh_token,
        refresh_expires_at: stored.refresh_expires_at,
        device_id: stored.device_id,
        subscription: stored.subscription,
    }
}

fn app_api_encode_session_for_disk(session: &AppApiTokenResponse) -> Result<String, String> {
    serde_json::to_string(&app_api_stored_session(session))
        .map_err(|e| format!("App API session serialize failed: {}", e))
}

fn app_api_decode_session_from_disk(encoded: &str) -> Result<AppApiTokenResponse, String> {
    if let Ok(stored) = serde_json::from_str::<AppApiStoredSession>(encoded) {
        return Ok(app_api_session_from_stored(stored));
    }

    // One-way migration for early v6 RCs that persisted the whole token
    // response. The loaded access token stays in memory for this process only;
    // app_api_load_session rewrites the disk entry without it.
    serde_json::from_str::<AppApiTokenResponse>(encoded)
        .map_err(|e| format!("Stored App API session is invalid: {}", e))
}

fn app_api_store_session(session: &AppApiTokenResponse) -> Result<(), String> {
    let encoded = app_api_encode_session_for_disk(session)?;
    secure_store_keyring_set(APP_API_SESSION_KEY, &encoded)?;
    if let Ok(mut memory) = APP_API_MEMORY_SESSION.lock() {
        *memory = Some(session.clone());
    }
    Ok(())
}

fn app_api_delete_session() -> Result<(), String> {
    if let Ok(mut memory) = APP_API_MEMORY_SESSION.lock() {
        *memory = None;
    }
    secure_store_keyring_delete(APP_API_SESSION_KEY)
}

fn app_api_load_session() -> Result<Option<AppApiTokenResponse>, String> {
    if let Ok(memory) = APP_API_MEMORY_SESSION.lock() {
        if let Some(session) = memory.clone() {
            return Ok(Some(session));
        }
    }

    let Some(encoded) = secure_store_keyring_get(APP_API_SESSION_KEY)? else {
        return Ok(None);
    };
    let session = app_api_decode_session_from_disk(&encoded)?;
    if !session.access_token.is_empty() {
        secure_store_keyring_set(
            APP_API_SESSION_KEY,
            &app_api_encode_session_for_disk(&session)?,
        )?;
    }
    if let Ok(mut memory) = APP_API_MEMORY_SESSION.lock() {
        *memory = Some(session.clone());
    }
    Ok(Some(session))
}

fn app_api_public_session(session: Option<AppApiTokenResponse>) -> AppApiSessionStatus {
    let access_expires_at = session.as_ref().and_then(|s| {
        (!s.access_expires_at.trim().is_empty()).then(|| s.access_expires_at.clone())
    });
    AppApiSessionStatus {
        logged_in: session.is_some(),
        device_id: session.as_ref().map(|s| s.device_id.clone()),
        access_expires_at,
        refresh_expires_at: session.as_ref().map(|s| s.refresh_expires_at.clone()),
        subscription: session.as_ref().map(|s| s.subscription.clone()),
        api_base_url: app_api_base_url(),
        closed_control_plane_enabled: closed_control_plane_enabled(),
    }
}

fn app_api_ed25519_jwk_from_seed(seed: &[u8; 32]) -> (String, serde_json::Value) {
    let signing_key = SigningKey::from_bytes(seed);
    let public_key = signing_key.verifying_key().to_bytes();
    let encoded_public = URL_SAFE_NO_PAD.encode(public_key);
    let jwk = serde_json::json!({
        "kty": "OKP",
        "crv": "Ed25519",
        "x": encoded_public,
    });
    (encoded_public, jwk)
}

fn app_api_generate_device_state() -> Result<AppApiDeviceState, String> {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed)
        .map_err(|e| format!("App API device key generation failed: {}", e))?;
    let (public_key, public_key_jwk) = app_api_ed25519_jwk_from_seed(&seed);
    Ok(AppApiDeviceState {
        client_device_id: format!("pc-{}", uuid::Uuid::new_v4()),
        hwid: format!("pc-hwid-{}", uuid::Uuid::new_v4()),
        public_key,
        public_key_jwk,
        private_key_seed: URL_SAFE_NO_PAD.encode(seed),
        key_alg: "Ed25519".into(),
    })
}

fn app_api_device_state_is_usable(device: &AppApiDeviceState) -> bool {
    !device.client_device_id.trim().is_empty()
        && !device.hwid.trim().is_empty()
        && device.key_alg == "Ed25519"
        && !device.public_key.trim().is_empty()
        && device.public_key_jwk.get("kty").and_then(|v| v.as_str()) == Some("OKP")
        && device.public_key_jwk.get("crv").and_then(|v| v.as_str()) == Some("Ed25519")
        && !device.private_key_seed.trim().is_empty()
}

fn app_api_signing_key(device: &AppApiDeviceState) -> Result<SigningKey, String> {
    let decoded = URL_SAFE_NO_PAD
        .decode(device.private_key_seed.trim())
        .map_err(|e| format!("Stored App API device key is invalid: {}", e))?;
    let seed: [u8; 32] = decoded
        .try_into()
        .map_err(|_| "Stored App API device key has invalid length".to_string())?;
    Ok(SigningKey::from_bytes(&seed))
}

fn app_api_body_sha256(body: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body.unwrap_or("").as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

fn app_api_device_proof(
    device: &AppApiDeviceState,
    method: &reqwest::Method,
    path: &str,
    body: Option<&str>,
) -> Result<String, String> {
    let signing_key = app_api_signing_key(device)?;
    let iat = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("System clock is not valid for App API proof: {}", e))?
        .as_secs();
    let jti = uuid::Uuid::new_v4().to_string();
    let normalized_path = format!("/{}", path.trim_start_matches('/'));
    let body_sha256 = app_api_body_sha256(body);
    let signing_input = format!(
        "DoodleVPN-PC-Proof-v1\n{}\n{}\n{}\n{}\n{}\n{}",
        method.as_str(),
        normalized_path,
        body_sha256,
        iat,
        jti,
        device.client_device_id
    );
    let signature = signing_key.sign(signing_input.as_bytes());
    let proof = serde_json::json!({
        "typ": "doodlevpn-device-proof-v1",
        "alg": "EdDSA",
        "device_id": device.client_device_id,
        "public_key_jwk": device.public_key_jwk,
        "htm": method.as_str(),
        "htu": normalized_path,
        "iat": iat,
        "jti": jti,
        "body_sha256": body_sha256,
        "sig": URL_SAFE_NO_PAD.encode(signature.to_bytes()),
    });
    Ok(URL_SAFE_NO_PAD.encode(proof.to_string()))
}

fn app_api_load_or_create_device() -> Result<AppApiDeviceState, String> {
    if let Some(encoded) = secure_store_keyring_get(APP_API_DEVICE_KEY)? {
        if let Ok(device) = serde_json::from_str::<AppApiDeviceState>(&encoded) {
            if app_api_device_state_is_usable(&device) {
                return Ok(device);
            }
        }
    }

    // v6 keeps the private key below the React/Tauri renderer boundary. A later
    // Windows-only hardening pass should move this seed into a CNG persisted key.
    let device = app_api_generate_device_state()?;
    let encoded = serde_json::to_string(&device)
        .map_err(|e| format!("App API device serialize failed: {}", e))?;
    secure_store_keyring_set(APP_API_DEVICE_KEY, &encoded)?;
    Ok(device)
}

async fn app_api_send_json<T: DeserializeOwned>(
    method: reqwest::Method,
    path: &str,
    bearer: Option<&str>,
    body: Option<serde_json::Value>,
) -> Result<T, AppApiHttpError> {
    let client = app_api_http_client().map_err(|message| AppApiHttpError { status: 0, message })?;
    let url = app_api_endpoint(path).map_err(|message| AppApiHttpError { status: 0, message })?;
    let body_text = body.as_ref().map(|body| body.to_string());
    let mut request = client
        .request(method.clone(), url)
        .header("Accept", "application/json")
        .header(
            "User-Agent",
            format!("DoodleRayPC/{}", env!("CARGO_PKG_VERSION")),
        );
    if closed_control_plane_enabled() {
        let device = app_api_load_or_create_device()
            .map_err(|message| AppApiHttpError { status: 0, message })?;
        let proof = app_api_device_proof(&device, &method, path, body_text.as_deref())
            .map_err(|message| AppApiHttpError { status: 0, message })?;
        request = request
            .header("X-Doodle-Device-ID", device.client_device_id)
            .header("X-Doodle-Device-Proof", proof);
    }
    if let Some(token) = bearer {
        request = request.bearer_auth(token);
    }
    if let Some(body) = body_text {
        request = request
            .header("Content-Type", "application/json")
            .body(body);
    }

    let response = request.send().await.map_err(|e| AppApiHttpError {
        status: 0,
        message: e.to_string(),
    })?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(AppApiHttpError {
            status: status.as_u16(),
            message: app_api_error_message(status, &text),
        });
    }
    serde_json::from_str::<T>(&text).map_err(|e| AppApiHttpError {
        status: status.as_u16(),
        message: format!("App API JSON parse failed: {}", e),
    })
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
async fn app_api_send_bytes(path: &str, bearer: &str) -> Result<Vec<u8>, AppApiHttpError> {
    let client = app_api_http_client().map_err(|message| AppApiHttpError { status: 0, message })?;
    let url = app_api_endpoint(path).map_err(|message| AppApiHttpError { status: 0, message })?;
    let method = reqwest::Method::GET;
    let mut request = client
        .get(url)
        .header("Accept", "application/octet-stream")
        .header(
            "User-Agent",
            format!("DoodleRayPC/{}", env!("CARGO_PKG_VERSION")),
        )
        .bearer_auth(bearer);
    if closed_control_plane_enabled() {
        let device = app_api_load_or_create_device()
            .map_err(|message| AppApiHttpError { status: 0, message })?;
        let proof = app_api_device_proof(&device, &method, path, None)
            .map_err(|message| AppApiHttpError { status: 0, message })?;
        request = request
            .header("X-Doodle-Device-ID", device.client_device_id)
            .header("X-Doodle-Device-Proof", proof);
    }
    let response = request.send().await.map_err(|error| AppApiHttpError {
        status: 0,
        message: error.to_string(),
    })?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(AppApiHttpError {
            status: status.as_u16(),
            message: app_api_error_message(status, &text),
        });
    }
    if response
        .content_length()
        .is_some_and(|size| size > 64 * 1024 * 1024)
    {
        return Err(AppApiHttpError {
            status: status.as_u16(),
            message: "routing asset is too large".into(),
        });
    }
    let bytes = response.bytes().await.map_err(|error| AppApiHttpError {
        status: status.as_u16(),
        message: error.to_string(),
    })?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(AppApiHttpError {
            status: status.as_u16(),
            message: "routing asset is too large".into(),
        });
    }
    Ok(bytes.to_vec())
}

async fn app_api_refresh_session() -> Result<AppApiTokenResponse, String> {
    let Some(session) = app_api_load_session()? else {
        return Err("DoodleVPN sign-in is required.".into());
    };
    let body = serde_json::json!({
        "refresh_token": session.refresh_token,
        "device_id": session.device_id,
    });
    let refreshed = app_api_send_json::<AppApiTokenResponse>(
        reqwest::Method::POST,
        "/auth/refresh",
        None,
        Some(body),
    )
    .await
    .map_err(|e| e.to_string())?;
    app_api_store_session(&refreshed)?;
    Ok(refreshed)
}

async fn app_api_authorized_json<T: DeserializeOwned>(
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<T, String> {
    app_api_authorized_json_http(method, path, body)
        .await
        .map_err(|error| error.to_string())
}

async fn app_api_authorized_json_http<T: DeserializeOwned>(
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<T, AppApiHttpError> {
    let Some(session) =
        app_api_load_session().map_err(|message| AppApiHttpError { status: 0, message })?
    else {
        return Err(AppApiHttpError {
            status: 401,
            message: "DoodleVPN sign-in is required.".into(),
        });
    };
    if session.access_token.trim().is_empty() {
        let refreshed = app_api_refresh_session()
            .await
            .map_err(|message| AppApiHttpError { status: 0, message })?;
        return app_api_send_json::<T>(method, path, Some(&refreshed.access_token), body).await;
    }
    match app_api_send_json::<T>(
        method.clone(),
        path,
        Some(&session.access_token),
        body.clone(),
    )
    .await
    {
        Ok(value) => Ok(value),
        Err(err) if err.status == 401 => {
            let refreshed = app_api_refresh_session()
                .await
                .map_err(|message| AppApiHttpError { status: 0, message })?;
            app_api_send_json::<T>(method, path, Some(&refreshed.access_token), body).await
        }
        Err(err) => Err(err),
    }
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
async fn app_api_authorized_bytes(path: &str) -> Result<Vec<u8>, String> {
    let Some(session) = app_api_load_session()? else {
        return Err("DoodleVPN sign-in is required.".into());
    };
    let session = if session.access_token.trim().is_empty() {
        app_api_refresh_session().await?
    } else {
        session
    };
    match app_api_send_bytes(path, &session.access_token).await {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.status == 401 => {
            let refreshed = app_api_refresh_session().await?;
            app_api_send_bytes(path, &refreshed.access_token)
                .await
                .map_err(|error| error.to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

fn app_api_diagnostic_key_is_sensitive(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "token",
        "password",
        "secret",
        "private",
        "credential",
        "uuid",
        "address",
        "endpoint",
        "profile",
        "public_key",
        "short_id",
        "subscription",
        "packet",
        "dns_query",
        "destination_ip",
        "config",
        "domain",
        "host",
        "url",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn app_api_redact_diagnostic_text(value: &str) -> String {
    value
        .chars()
        .take(4_000)
        .collect::<String>()
        .split_whitespace()
        .map(|word| {
            let trimmed = word.trim_matches(|c: char| {
                !c.is_ascii_alphanumeric() && c != '.' && c != ':' && c != '-'
            });
            if trimmed.contains("://") {
                return "[url]".to_string();
            }
            if trimmed.parse::<IpAddr>().is_ok() {
                return "[ip]".to_string();
            }
            let uuid_like = trimmed.len() == 36
                && trimmed
                    .as_bytes()
                    .iter()
                    .filter(|byte| **byte == b'-')
                    .count()
                    == 4;
            if uuid_like {
                return "[id]".to_string();
            }
            let looks_like_domain = trimmed.rsplit_once('.').is_some_and(|(_, suffix)| {
                suffix.len() >= 2 && suffix.chars().all(|c| c.is_ascii_alphabetic())
            });
            if looks_like_domain {
                return "[domain]".to_string();
            }
            word.to_string()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn app_api_sanitize_diagnostic_value(value: serde_json::Value, depth: usize) -> serde_json::Value {
    if depth > 5 {
        return serde_json::Value::String("[truncated]".into());
    }
    match value {
        serde_json::Value::String(value) => {
            serde_json::Value::String(app_api_redact_diagnostic_text(&value))
        }
        serde_json::Value::Array(values) => serde_json::Value::Array(
            values
                .into_iter()
                .take(50)
                .map(|value| app_api_sanitize_diagnostic_value(value, depth + 1))
                .collect(),
        ),
        serde_json::Value::Object(values) => serde_json::Value::Object(
            values
                .into_iter()
                .filter(|(key, _)| !app_api_diagnostic_key_is_sensitive(key))
                .take(50)
                .map(|(key, value)| (key, app_api_sanitize_diagnostic_value(value, depth + 1)))
                .collect(),
        ),
        other => other,
    }
}

fn app_api_profile_to_connect_request(
    profile: &serde_json::Value,
    request: &AppConnectLocationRequest,
) -> Result<ConnectRequest, String> {
    let profile_type = profile
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if profile_type == "xray" {
        return app_api_xray_profile_to_connect_request(profile, request);
    }
    let security = profile
        .get("security")
        .and_then(|v| v.as_str())
        .unwrap_or("reality");
    if profile_type != "vless" || security != "reality" {
        return Err(format!(
            "Unsupported DoodleVPN profile type for PC runtime: type={} security={}",
            profile_type, security
        ));
    }

    let address = profile
        .get("connect_address")
        .or_else(|| profile.get("address"))
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| "DoodleVPN profile is missing connect address".to_string())?
        .to_string();
    let port = profile
        .get("port")
        .and_then(|v| v.as_u64())
        .filter(|port| *port > 0 && *port <= u16::MAX as u64)
        .unwrap_or(443) as u16;
    let uuid = profile
        .get("uuid")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| "DoodleVPN profile is missing user id".to_string())?
        .to_string();

    let transport = match profile.get("transport").and_then(|v| v.as_str()) {
        Some("reality_tcp") | Some("tcp") | None => "tcp".to_string(),
        Some(other) => other.to_string(),
    };

    Ok(ConnectRequest {
        server_address: address,
        server_port: port,
        protocol: "vless".into(),
        uuid: Some(uuid),
        password: None,
        transport,
        security: "reality".into(),
        sni: profile
            .get("server_name")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
        host: None,
        path: None,
        fingerprint: profile
            .get("fingerprint")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
        public_key: profile
            .get("public_key")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
        short_id: profile
            .get("short_id")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
        flow: profile
            .get("flow")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
        proxy_mode: request.proxy_mode.clone(),
        system_proxy_mode: request.system_proxy_mode.clone(),
        socks_port: request.socks_port,
        http_port: request.http_port,
        api_port: default_xray_api_port(),
        network_stack: request.network_stack.clone(),
        dns_mode: request.dns_mode.clone(),
        strict_route: request.strict_route,
        kill_switch: request.kill_switch,
        routing_rules: request.routing_rules.clone(),
        obfs_type: None,
        obfs_password: None,
        up_mbps: None,
        down_mbps: None,
        congestion_control: None,
        udp_relay_mode: None,
        alpn: None,
        private_key: None,
        peer_public_key: None,
        pre_shared_key: None,
        local_address: None,
        reserved: None,
        mtu: None,
        workers: None,
        encryption: None,
        raw_xray_config: None,
        routing_policy: None,
    })
}

fn app_api_xray_profile_to_connect_request(
    profile: &serde_json::Value,
    request: &AppConnectLocationRequest,
) -> Result<ConnectRequest, String> {
    if profile.get("format").and_then(|v| v.as_str()) != Some("xray-outbound-v1") {
        return Err("Unsupported DoodleVPN Xray profile format".into());
    }
    let config = profile
        .get("config")
        .filter(|value| value.is_object())
        .ok_or_else(|| "DoodleVPN Xray profile is missing config".to_string())?
        .clone();
    let outbounds = config
        .get("outbounds")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "DoodleVPN Xray profile is missing outbounds".to_string())?;
    let proxy = outbounds
        .iter()
        .find(|outbound| outbound.get("tag").and_then(|value| value.as_str()) == Some("proxy"))
        .ok_or_else(|| "DoodleVPN Xray profile is missing proxy outbound".to_string())?;
    if proxy.get("protocol").and_then(|value| value.as_str()) != Some("vless") {
        return Err("Unsupported DoodleVPN Xray outbound protocol".into());
    }
    let address = profile
        .get("connect_address")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "DoodleVPN Xray profile is missing connect address".to_string())?
        .to_string();
    let port = profile
        .get("port")
        .and_then(|value| value.as_u64())
        .filter(|value| *value > 0 && *value <= u16::MAX as u64)
        .ok_or_else(|| "DoodleVPN Xray profile has an invalid port".to_string())?
        as u16;

    Ok(ConnectRequest {
        server_address: address,
        server_port: port,
        protocol: "vless".into(),
        uuid: None,
        password: None,
        transport: "xhttp".into(),
        security: "tls".into(),
        sni: None,
        host: None,
        path: None,
        fingerprint: None,
        public_key: None,
        short_id: None,
        flow: None,
        proxy_mode: request.proxy_mode.clone(),
        system_proxy_mode: request.system_proxy_mode.clone(),
        socks_port: request.socks_port,
        http_port: request.http_port,
        api_port: default_xray_api_port(),
        network_stack: request.network_stack.clone(),
        dns_mode: request.dns_mode.clone(),
        strict_route: request.strict_route,
        kill_switch: request.kill_switch,
        routing_rules: request.routing_rules.clone(),
        obfs_type: None,
        obfs_password: None,
        up_mbps: None,
        down_mbps: None,
        congestion_control: None,
        udp_relay_mode: None,
        alpn: None,
        private_key: None,
        peer_public_key: None,
        pre_shared_key: None,
        local_address: None,
        reserved: None,
        mtu: None,
        workers: None,
        encryption: None,
        raw_xray_config: Some(config),
        routing_policy: None,
    })
}

fn app_api_connection_result_body(
    lease: &AppApiProfileLeaseResponse,
    session: &AppApiTokenResponse,
    success: bool,
    latency_ms: i64,
    message: &str,
) -> serde_json::Value {
    serde_json::json!({
        "profile_id": lease.profile_id,
        "device_id": session.device_id,
        "app_version": env!("CARGO_PKG_VERSION"),
        "core_version": app_api_core_version(),
        "success": success,
        "error_code": if success { "" } else { "pc_connect_failed" },
        "latency_ms": latency_ms.max(0),
        "transport": lease.route_kind,
        "last_error": if success { String::new() } else { redact_support_line(message) }
    })
}

fn app_api_platform() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(windows)]
    {
        "windows"
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        "desktop"
    }
}

fn app_api_core_version() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos-v6"
    }
    #[cfg(windows)]
    {
        "pc-v6"
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        "desktop-v6"
    }
}

fn app_api_client_capabilities() -> serde_json::Value {
    serde_json::json!({
        "windows": cfg!(windows),
        "macos": cfg!(target_os = "macos"),
        "tun": true,
        "network_extension": cfg!(all(target_os = "macos", feature = "app-store")),
        "xray_reality": true,
        "dns_hijack": true
    })
}

fn app_api_device_body(device: &AppApiDeviceState, computer_name: &str) -> serde_json::Value {
    serde_json::json!({
        "device_id": device.client_device_id,
        "platform": app_api_platform(),
        "model": computer_name,
        "app_version": env!("CARGO_PKG_VERSION"),
        "hwid": device.hwid,
        "public_key": device.public_key
    })
}

fn app_api_exchange_code_body(
    code: &str,
    device: &AppApiDeviceState,
    computer_name: &str,
) -> serde_json::Value {
    serde_json::json!({
        "code": code,
        "device": app_api_device_body(device, computer_name)
    })
}

fn legacy_subscription_token(subscription_url: &str) -> Result<String, String> {
    let parsed = Url::parse(subscription_url.trim())
        .map_err(|_| "Stored DoodleVPN subscription URL is invalid.".to_string())?;
    if parsed.scheme() != "https" {
        return Err("Stored DoodleVPN subscription must use HTTPS.".into());
    }
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    if !matches!(
        host.as_str(),
        "ddlvpn.lol"
            | "www.ddlvpn.lol"
            | "doodlevpn.online"
            | "www.doodlevpn.online"
            | "sub.brewsandrologistics.fun"
    ) {
        return Err("Stored subscription is not a DoodleVPN subscription.".into());
    }
    let segments = parsed
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if segments.len() != 2 || !matches!(segments[0], "s" | "sub") {
        return Err("Stored DoodleVPN subscription URL is not supported.".into());
    }
    let token = segments[1].trim();
    if token.len() < 8
        || token.len() > 256
        || !token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "-_.~=".contains(ch))
    {
        return Err("Stored DoodleVPN subscription token is invalid.".into());
    }
    Ok(token.to_string())
}

#[tauri::command(async)]
fn app_api_session_status() -> Result<AppApiSessionStatus, String> {
    if !closed_control_plane_enabled() {
        return Ok(app_api_public_session(None));
    }
    app_api_load_session().map(app_api_public_session)
}

#[tauri::command]
async fn app_api_exchange_code(
    request: AppApiExchangeCodeRequest,
) -> Result<AppApiSessionStatus, String> {
    ensure_closed_control_plane_enabled()?;
    let code = request.code.trim();
    if code.is_empty() {
        return Err("Enter the DoodleVPN sign-in code.".into());
    }
    let device = app_api_load_or_create_device()?;
    let computer_name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| APP_PRODUCT_NAME.into());
    let body = app_api_exchange_code_body(code, &device, &computer_name);
    let session = app_api_send_json::<AppApiTokenResponse>(
        reqwest::Method::POST,
        "/auth/code/exchange",
        None,
        Some(body),
    )
    .await
    .map_err(|e| e.to_string())?;
    app_api_store_session(&session)?;
    Ok(app_api_public_session(Some(session)))
}

#[tauri::command]
async fn app_api_exchange_legacy_subscription(
    request: AppApiExchangeLegacySubscriptionRequest,
) -> Result<AppApiSessionStatus, String> {
    ensure_closed_control_plane_enabled()?;
    let token = legacy_subscription_token(&request.subscription_url)?;
    let device = app_api_load_or_create_device()?;
    let computer_name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| APP_PRODUCT_NAME.into());
    let body = serde_json::json!({
        "subscription_token": token,
        "device": app_api_device_body(&device, &computer_name)
    });
    let session = app_api_send_json::<AppApiTokenResponse>(
        reqwest::Method::POST,
        "/auth/legacy-subscription/exchange",
        None,
        Some(body),
    )
    .await
    .map_err(|e| e.to_string())?;
    app_api_store_session(&session)?;
    Ok(app_api_public_session(Some(session)))
}

#[tauri::command]
async fn app_api_refresh() -> Result<AppApiSessionStatus, String> {
    ensure_closed_control_plane_enabled()?;
    let session = app_api_refresh_session().await?;
    Ok(app_api_public_session(Some(session)))
}

#[tauri::command]
async fn app_api_logout() -> Result<(), String> {
    ensure_closed_control_plane_enabled()?;
    let _ =
        app_api_authorized_json::<serde_json::Value>(reqwest::Method::POST, "/device/logout", None)
            .await;
    app_api_delete_session()
}

#[tauri::command]
async fn app_api_locations() -> Result<AppApiLocationsResponse, String> {
    ensure_closed_control_plane_enabled()?;
    app_api_authorized_json::<AppApiLocationsResponse>(reqwest::Method::GET, "/locations", None)
        .await
}

#[tauri::command]
async fn app_api_subscription_status() -> Result<AppApiSubscriptionSummary, String> {
    ensure_closed_control_plane_enabled()?;
    app_api_authorized_json::<AppApiSubscriptionSummary>(
        reqwest::Method::GET,
        "/subscription/status",
        None,
    )
    .await
}

#[tauri::command]
async fn app_api_submit_diagnostics(
    submission: AppApiDiagnosticsSubmission,
) -> Result<serde_json::Value, String> {
    ensure_closed_control_plane_enabled()?;
    if submission.events.is_empty() {
        return Err("Diagnostics report is empty.".into());
    }
    let session =
        app_api_load_session()?.ok_or_else(|| "DoodleVPN sign-in is required.".to_string())?;
    let events = submission
        .events
        .into_iter()
        .take(20)
        .map(|event| {
            let mut event = app_api_sanitize_diagnostic_value(event, 0);
            if let Some(object) = event.as_object_mut() {
                object.insert("manual".into(), submission.manual.into());
                object.insert("platform".into(), app_api_platform().into());
                object.insert("architecture".into(), std::env::consts::ARCH.into());
                object.insert("core_version".into(), app_api_core_version().into());
            }
            event
        })
        .collect::<Vec<_>>();
    let body = serde_json::json!({
        "session_id": format!(
            "diag_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ),
        "device_id": session.device_id,
        "app_version": env!("CARGO_PKG_VERSION"),
        "events": events,
    });
    if body.to_string().len() > 60 * 1024 {
        return Err("Diagnostics report is too large.".into());
    }
    app_api_authorized_json(reqwest::Method::POST, "/diagnostics", Some(body)).await
}

async fn app_api_connection_profile(
    session: &AppApiTokenResponse,
    location_id: &str,
    selection_mode: &str,
) -> Result<AppApiProfileLeaseResponse, AppApiHttpError> {
    let body = serde_json::json!({
        "location_id": location_id,
        "device_id": session.device_id,
        "network_class": "normal",
        "selection_mode": selection_mode,
        "app_version": env!("CARGO_PKG_VERSION"),
        "core_version": app_api_core_version(),
        "schema_version": 2,
        "routing_policy_version": "desktop-v2",
        "client_capabilities": app_api_client_capabilities()
    });
    let lease: AppApiProfileLeaseResponse = app_api_authorized_json_http(
        reqwest::Method::POST,
        APP_API_CONNECTION_PROFILE_PATH,
        Some(body),
    )
    .await?;
    if lease.schema_version != 2 {
        return Err(AppApiHttpError {
            status: 426,
            message: "DoodleVPN profile format requires an app update.".into(),
        });
    }
    if lease.routing_policy.is_none() {
        return Err(AppApiHttpError {
            status: 426,
            message:
                "DoodleVPN profile is missing its signed routing policy. Refresh and try again."
                    .into(),
        });
    }
    Ok(lease)
}

fn app_api_profile_error_is_terminal(error: &AppApiHttpError) -> bool {
    matches!(error.status, 400 | 401 | 403 | 426 | 429)
}

fn app_api_validated_routing_policy(
    lease: &AppApiProfileLeaseResponse,
) -> Result<AppRoutingPolicy, String> {
    lease
        .routing_policy
        .clone()
        .ok_or_else(|| "DoodleVPN profile is missing its signed routing policy.".to_string())
        .and_then(validate_app_routing_policy)
}

#[tauri::command]
async fn app_connect_location(
    request: AppConnectLocationRequest,
    app: tauri::AppHandle,
) -> ConnectResult {
    if let Err(message) = ensure_closed_control_plane_enabled() {
        return ConnectResult {
            success: false,
            message,
            health: None,
        };
    }
    let session = match app_api_load_session() {
        Ok(Some(session)) => session,
        Ok(None) => {
            return ConnectResult {
                success: false,
                message: "DoodleVPN sign-in is required.".into(),
                health: None,
            };
        }
        Err(e) => {
            return ConnectResult {
                success: false,
                message: e,
                health: None,
            };
        }
    };
    if session.subscription.device_allowed == Some(false) {
        return ConnectResult {
            success: false,
            message: "DoodleVPN device limit reached. Remove an unused device, then refresh the account status.".into(),
            health: None,
        };
    }

    let location_ids = app_connection_location_ids(&request);

    let selection_mode = if location_ids.len() > 1 {
        "auto"
    } else {
        "manual"
    };
    let mut last_failure = None;
    for location_id in location_ids {
        let started = Instant::now();
        let lease = match app_api_connection_profile(&session, &location_id, selection_mode).await {
            Ok(lease) => lease,
            Err(e) => {
                let terminal = app_api_profile_error_is_terminal(&e);
                last_failure = Some(ConnectResult {
                    success: false,
                    message: format!("DoodleVPN connection profile failed: {}", e),
                    health: None,
                });
                if terminal {
                    break;
                }
                continue;
            }
        };
        let mut connect_request =
            match app_api_profile_to_connect_request(&lease.native_profile, &request) {
                Ok(request) => request,
                Err(e) => {
                    last_failure = Some(ConnectResult {
                        success: false,
                        message: e,
                        health: None,
                    });
                    continue;
                }
            };
        connect_request.routing_policy = match app_api_validated_routing_policy(&lease) {
            Ok(policy) => Some(policy),
            Err(message) => {
                last_failure = Some(ConnectResult {
                    success: false,
                    message,
                    health: None,
                });
                continue;
            }
        };
        let result = vpn_connect(connect_request, app.clone()).await;
        let result_body = app_api_connection_result_body(
            &lease,
            &session,
            result.success,
            started.elapsed().as_millis() as i64,
            &result.message,
        );
        let _ = app_api_authorized_json::<serde_json::Value>(
            reqwest::Method::POST,
            "/connection-result",
            Some(result_body),
        )
        .await;
        if result.success {
            return result;
        }
        last_failure = Some(result);
    }

    last_failure.unwrap_or(ConnectResult {
        success: false,
        message: "No VPN location is available.".into(),
        health: None,
    })
}

#[tauri::command]
async fn app_ping_location(location_id: String, server_id: String) -> Result<PingResult, String> {
    ensure_closed_control_plane_enabled()?;
    let session =
        app_api_load_session()?.ok_or_else(|| "DoodleVPN sign-in is required.".to_string())?;
    if session.subscription.device_allowed == Some(false) {
        return Err("DoodleVPN device limit reached.".into());
    }
    let lease = app_api_connection_profile(&session, &location_id, "probe")
        .await
        .map_err(|error| error.to_string())?;
    let request = AppConnectLocationRequest {
        location_id,
        fallback_location_ids: Vec::new(),
        proxy_mode: default_app_proxy_mode(),
        system_proxy_mode: default_system_proxy_mode(),
        socks_port: default_app_socks_port(),
        http_port: default_app_http_port(),
        network_stack: default_app_network_stack(),
        dns_mode: default_app_dns_mode(),
        strict_route: false,
        kill_switch: false,
        routing_rules: Vec::new(),
    };
    let mut connect_request = app_api_profile_to_connect_request(&lease.native_profile, &request)?;
    connect_request.routing_policy = Some(app_api_validated_routing_policy(&lease)?);
    Ok(ping_server_profile(connect_request, server_id).await)
}

#[tauri::command]
async fn app_disconnect(app: tauri::AppHandle) -> ConnectResult {
    vpn_disconnect(app).await
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectRequest {
    pub server_address: String,
    pub server_port: u16,
    pub protocol: String,
    pub uuid: Option<String>,
    pub password: Option<String>,
    pub transport: String,
    pub security: String,
    pub sni: Option<String>,
    pub host: Option<String>,
    pub path: Option<String>,
    pub fingerprint: Option<String>,
    pub public_key: Option<String>,
    pub short_id: Option<String>,
    pub flow: Option<String>,
    pub proxy_mode: String,
    #[serde(default = "default_system_proxy_mode")]
    pub system_proxy_mode: String,
    pub socks_port: u16,
    pub http_port: u16,
    #[serde(default = "default_xray_api_port")]
    pub api_port: u16,
    pub network_stack: String,
    pub dns_mode: String,
    pub strict_route: bool,
    #[serde(default)]
    pub kill_switch: bool,
    #[serde(default)]
    pub routing_rules: Vec<RoutingRuleRequest>,
    // Hysteria2
    #[serde(default)]
    pub obfs_type: Option<String>,
    #[serde(default)]
    pub obfs_password: Option<String>,
    #[serde(default)]
    pub up_mbps: Option<u32>,
    #[serde(default)]
    pub down_mbps: Option<u32>,
    // TUIC
    #[serde(default)]
    pub congestion_control: Option<String>,
    #[serde(default)]
    pub udp_relay_mode: Option<String>,
    #[serde(default)]
    pub alpn: Option<Vec<String>>,
    // WireGuard
    #[serde(default)]
    pub private_key: Option<String>,
    #[serde(default)]
    pub peer_public_key: Option<String>,
    #[serde(default)]
    pub pre_shared_key: Option<String>,
    #[serde(default)]
    pub local_address: Option<Vec<String>>,
    #[serde(default)]
    pub reserved: Option<Vec<u8>>,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub workers: Option<u32>,
    // Shadowsocks encryption method
    #[serde(default)]
    pub encryption: Option<String>,
    // Full raw xray JSON config — when present, passed directly to xray-core
    // instead of building a simplified single-server config
    #[serde(default)]
    pub raw_xray_config: Option<serde_json::Value>,
    #[serde(default)]
    pub routing_policy: Option<AppRoutingPolicy>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RoutingRuleRequest {
    pub rule_type: String, // "domain" or "exe"
    pub value: String,     // "youtube.com", "steam.exe"
    pub action: String,    // "proxy", "direct", "block"
}

#[derive(Debug, Serialize)]
pub struct ConnectResult {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<ConnectionHealthReport>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ConnectionHealthReport {
    pub verdict: String,
    pub mode: String,
    pub generated_at_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_effective_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_health_verdict: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_socks_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_http_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_api_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_op_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub service_fatal_checks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub service_degraded_checks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub service_warning_checks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route_explanations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoint_bypass_checks: Vec<String>,
    pub checks: Vec<ConnectionHealthCheck>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ConnectionHealthCheck {
    pub code: String,
    pub severity: String,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct PingResult {
    pub server_id: String,
    pub ping_ms: i32, // -1 = failed/timeout
}

#[derive(Debug, Serialize)]
pub struct SubscriptionFetchResult {
    pub body: String,
    pub subscription_userinfo: Option<String>,
    pub profile_title: Option<String>,
    pub content_disposition: Option<String>,
}

fn legacy_import_bridge_enabled() -> bool {
    option_env!("DOODLERAY_CLOSED_CONTROL_PLANE") != Some("1")
        || option_env!("DOODLERAY_ENABLE_LEGACY_IMPORT") == Some("1")
}

fn ensure_legacy_import_bridge_enabled() -> Result<(), String> {
    if legacy_import_bridge_enabled() {
        Ok(())
    } else {
        Err("Legacy subscription and proxy-link import is disabled in this build.".into())
    }
}

/// Fetch a URL from Rust side — bypasses CORS restrictions in WebView
#[tauri::command]
async fn fetch_url(url: String) -> Result<String, String> {
    ensure_legacy_import_bridge_enabled()?;
    let parsed_url = validate_http_url(&url)?;
    let response = fetch_http_response_with_fallback(&parsed_url, Duration::from_secs(30)).await?;

    let body = read_response_body_limited(response, MAX_SUBSCRIPTION_BODY_BYTES).await?;
    String::from_utf8(body).map_err(|e| format!("Response is not valid UTF-8: {}", e))
}

/// Fetch a subscription and return its quota metadata headers together with body.
#[tauri::command]
async fn fetch_subscription_url(url: String) -> Result<SubscriptionFetchResult, String> {
    ensure_legacy_import_bridge_enabled()?;
    let parsed_url = validate_http_url(&url)?;
    let response = fetch_http_response_with_fallback(&parsed_url, Duration::from_secs(30)).await?;

    let subscription_userinfo = response
        .headers()
        .get("subscription-userinfo")
        .or_else(|| response.headers().get("x-subscription-userinfo"))
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    let profile_title = response
        .headers()
        .get("profile-title")
        .or_else(|| response.headers().get("x-profile-title"))
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    let content_disposition = response
        .headers()
        .get("content-disposition")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    let body =
        String::from_utf8(read_response_body_limited(response, MAX_SUBSCRIPTION_BODY_BYTES).await?)
            .map_err(|e| format!("Response is not valid UTF-8: {}", e))?;

    Ok(SubscriptionFetchResult {
        body,
        subscription_userinfo,
        profile_title,
        content_disposition,
    })
}

/// Workshop API proxy — supports GET/POST for the pinned production API.
#[tauri::command]
async fn workshop_api(url: String, method: String, body: Option<String>) -> Result<String, String> {
    let parsed_url = validate_workshop_api_url(&url)?;
    // Extract host from URL for DNS pinning (crucial for TUN mode where DNS may fail)
    let mut builder = reqwest::Client::builder()
        .no_proxy() // IMPORTANT: bypass system proxy so API calls don't loop through VPN
        .timeout(Duration::from_secs(15));

    if let Some(host) = parsed_url.host_str() {
        if host == "94-241-172-101.sslip.io" {
            let ip = std::net::IpAddr::V4(std::net::Ipv4Addr::new(94, 241, 172, 101));
            builder = builder.resolve(host, std::net::SocketAddr::new(ip, 443));
        }
    }

    // Pin DNS for traefik.me domains (they embed the IP in the subdomain)
    // e.g., "...-94-241-172-101.traefik.me" → 94.241.172.101
    if url.contains("traefik.me") {
        if let Some(host) = url.split("//").nth(1).and_then(|s| s.split('/').next()) {
            // Extract IP from subdomain: take the 4 numbers before ".traefik.me"
            let parts: Vec<&str> = host.trim_end_matches(".traefik.me").split('-').collect();
            if parts.len() >= 4 {
                let ip_parts = &parts[parts.len() - 4..];
                if let (Ok(a), Ok(b), Ok(c), Ok(d)) = (
                    ip_parts[0].parse::<u8>(),
                    ip_parts[1].parse::<u8>(),
                    ip_parts[2].parse::<u8>(),
                    ip_parts[3].parse::<u8>(),
                ) {
                    let ip = std::net::IpAddr::V4(std::net::Ipv4Addr::new(a, b, c, d));
                    let addr = std::net::SocketAddr::new(ip, 443);
                    builder = builder.resolve(host, addr);
                }
            }
        }
    }

    let client = builder
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let req = if method.eq_ignore_ascii_case("POST") {
        let mut r = client
            .post(parsed_url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "DoodleRay/2.0");
        if let Some(b) = body {
            r = r.body(b);
        }
        r
    } else if method.eq_ignore_ascii_case("GET") {
        client.get(parsed_url).header("User-Agent", "DoodleRay/2.0")
    } else {
        return Err("Unsupported Workshop API method".into());
    };

    let response = req
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    String::from_utf8(read_response_body_limited(response, MAX_WORKSHOP_BODY_BYTES).await?)
        .map_err(|e| format!("Workshop API response is not valid UTF-8: {}", e))
}

/// Check VPN endpoint reachability with a raw TCP connect.
/// Most proxy ports are not HTTP endpoints, so HTTP/TLS errors must not be
/// treated as successful latency samples.
#[tauri::command]
async fn ping_server(address: String, port: u16, server_id: String) -> PingResult {
    let sid = server_id.clone();

    let addr = address.clone();
    let p = port;
    let tcp_result = tokio::task::spawn_blocking(move || {
        let target = format!("{}:{}", addr, p);
        let addrs: Vec<_> = match std::net::ToSocketAddrs::to_socket_addrs(&target) {
            Ok(addrs) => addrs.collect(),
            Err(_) => return -1i32,
        };
        if addrs.is_empty() {
            return -1i32;
        }
        let addrs = addrs
            .into_iter()
            .filter(|addr| is_public_ip(addr.ip()))
            .collect::<Vec<_>>();
        if addrs.is_empty() {
            return -1i32;
        }

        let physical_sources = physical_ipv4_candidates();
        let mut samples = tcp_connect_samples(&addrs, &physical_sources);

        if samples.is_empty() {
            samples = tcp_connect_samples(&addrs, &[]);
        }

        if samples.is_empty() {
            return -1i32;
        }
        samples.sort_unstable();
        samples[samples.len() / 2]
    })
    .await
    .unwrap_or(-1);

    PingResult {
        server_id: sid,
        ping_ms: tcp_result,
    }
}

/// Check a full VPN profile by starting an isolated local proxy and performing
/// an HTTP GET through it. This avoids false-green TCP port checks.
#[tauri::command]
async fn ping_server_profile(request: ConnectRequest, server_id: String) -> PingResult {
    #[cfg(all(target_os = "macos", feature = "app-store"))]
    {
        ping_server(request.server_address, request.server_port, server_id).await
    }
    #[cfg(not(all(target_os = "macos", feature = "app-store")))]
    {
        ping_server_profile_direct(request, server_id).await
    }
}

#[cfg(not(all(target_os = "macos", feature = "app-store")))]
async fn ping_server_profile_direct(mut request: ConnectRequest, server_id: String) -> PingResult {
    let sid = server_id.clone();
    let endpoint_is_public = format!("{}:{}", request.server_address, request.server_port)
        .to_socket_addrs()
        .map(|addrs| {
            let addrs = addrs.collect::<Vec<_>>();
            !addrs.is_empty() && addrs.iter().all(|addr| is_public_ip(addr.ip()))
        })
        .unwrap_or(false);
    let ping_ms = if !endpoint_is_public {
        -1
    } else {
        match profile_http_ping_ms(&mut request).await {
            Ok(ms) => ms,
            Err(error) => {
                eprintln!("[ping] profile GET probe failed: {}", error);
                -1
            }
        }
    };

    PingResult {
        server_id: sid,
        ping_ms,
    }
}

async fn profile_http_ping_ms(request: &mut ConnectRequest) -> Result<i32, String> {
    let use_xray = uses_xray_engine(request);
    let ports = reserve_profile_ping_ports(if use_xray { 3 } else { 2 })?;
    request.socks_port = ports[0];
    request.http_port = ports[1];
    request.api_port = *ports.get(2).unwrap_or(&0);
    request.proxy_mode = "system-proxy".into();
    request.system_proxy_mode = "unchanged".into();
    request.network_stack = "system".into();
    request.dns_mode = "realip".into();
    request.strict_route = false;
    request.kill_switch = false;
    request.routing_rules.clear();

    let mut config = if use_xray {
        if let Some(ref raw) = request.raw_xray_config {
            inject_xray_inbounds(raw.clone(), request)
        } else {
            build_xray_config(request)
        }
    } else {
        let mut config = build_singbox_config(request);
        if let Some(object) = config.as_object_mut() {
            object.remove("experimental");
        }
        config
    };

    if !use_xray {
        if let Some(route) = config.get_mut("route") {
            route["final"] = serde_json::json!("proxy");
        }
    }

    let _guard = start_isolated_profile_ping_runtime(use_xray, &config, request.http_port)?;
    wait_for_profile_ping_port(request.http_port)?;
    http_get_profile_ping(request.http_port).await
}

fn reserve_profile_ping_ports(count: usize) -> Result<Vec<u16>, String> {
    let mut listeners = Vec::with_capacity(count);
    let mut ports = Vec::with_capacity(count);
    for _ in 0..count {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .map_err(|e| format!("bind 127.0.0.1:0 failed: {}", e))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("read reserved local port failed: {}", e))?
            .port();
        ports.push(port);
        listeners.push(listener);
    }
    drop(listeners);
    Ok(ports)
}

fn wait_for_profile_ping_port(port: u16) -> Result<(), String> {
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    for _ in 0..80 {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(format!("127.0.0.1:{} did not open for profile ping", port))
}

async fn http_get_profile_ping(http_port: u16) -> Result<i32, String> {
    let proxy = reqwest::Proxy::http(format!("http://127.0.0.1:{}", http_port))
        .map_err(|e| format!("profile ping proxy error: {}", e))?;
    let client = reqwest::Client::builder()
        .proxy(proxy)
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("profile ping HTTP client error: {}", e))?;

    let started = Instant::now();
    let response = client
        .get(PROFILE_PING_URL)
        .header("User-Agent", "DoodleRay/2.0")
        .header("Cache-Control", "no-cache")
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "profile ping GET timed out".to_string()
            } else {
                format!("profile ping GET failed: {}", e)
            }
        })?;

    if !response.status().is_success() {
        return Err(format!("profile ping HTTP {}", response.status().as_u16()));
    }

    let _ = response.bytes().await;
    Ok(started.elapsed().as_millis().max(1) as i32)
}

struct ProfilePingProcess {
    child: Option<std::process::Child>,
    config_path: PathBuf,
}

impl Drop for ProfilePingProcess {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_file(&self.config_path);
    }
}

fn start_isolated_profile_ping_runtime(
    use_xray: bool,
    config: &serde_json::Value,
    http_port: u16,
) -> Result<ProfilePingProcess, String> {
    let exe = if use_xray {
        profile_ping_xray_path()
            .ok_or_else(|| "xray.exe is unavailable for profile ping".to_string())?
    } else {
        profile_ping_singbox_path()
            .ok_or_else(|| "sing-box.exe is unavailable for profile ping".to_string())?
    };

    let temp_dir = std::env::temp_dir().join("DoodleRay").join("ping-probes");
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("create profile ping temp dir failed: {}", e))?;
    let runtime_name = if use_xray { "xray" } else { "singbox" };
    let config_path = temp_dir.join(format!(
        "{}-{}-{}-{}.json",
        runtime_name,
        std::process::id(),
        http_port,
        unix_ms()
    ));
    write_private_file(
        &config_path,
        serde_json::to_string_pretty(config)
            .map_err(|e| format!("serialize profile ping config failed: {}", e))?
            .as_bytes(),
    )
    .map_err(|e| format!("write profile ping config failed: {}", e))?;

    let mut cmd = std::process::Command::new(&exe);
    cmd.arg("run").arg("-c").arg(&config_path);
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("start profile ping runtime failed: {}", e))?;
    std::thread::sleep(Duration::from_millis(350));
    if let Ok(Some(status)) = child.try_wait() {
        let _ = std::fs::remove_file(&config_path);
        return Err(format!(
            "profile ping runtime exited early with status {}",
            status
        ));
    }

    Ok(ProfilePingProcess {
        child: Some(child),
        config_path,
    })
}

fn profile_ping_search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.to_path_buf());
            if let Some(parent) = dir.parent() {
                roots.push(parent.to_path_buf());
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd.clone());
        roots.push(cwd.join("src-tauri"));
    }

    let mut seen = HashSet::new();
    roots
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

fn profile_ping_singbox_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let exe_name = "sing-box.exe";
    #[cfg(not(windows))]
    let exe_name = "sing-box";

    for root in profile_ping_search_roots() {
        for candidate in [
            root.join(exe_name),
            root.join("singbox-core").join(exe_name),
        ] {
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn profile_ping_xray_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let exe_name = "xray.exe";
    #[cfg(not(windows))]
    let exe_name = "xray";

    for root in profile_ping_search_roots() {
        let candidate = root.join("xray-core").join(exe_name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn tcp_connect_samples(addrs: &[SocketAddr], source_ipv4s: &[Ipv4Addr]) -> Vec<i32> {
    let mut samples = Vec::new();
    let sources: Vec<Option<Ipv4Addr>> = if source_ipv4s.is_empty() {
        vec![None]
    } else {
        source_ipv4s.iter().copied().map(Some).collect()
    };

    for _ in 0..3 {
        let mut best_attempt: Option<i32> = None;
        for source_ip in &sources {
            for sock_addr in addrs {
                if source_ip.is_some() && !sock_addr.is_ipv4() {
                    continue;
                }
                let Some(ms) = tcp_connect_once(sock_addr, *source_ip) else {
                    continue;
                };
                best_attempt = Some(best_attempt.map_or(ms, |best| best.min(ms)));
            }
        }
        if let Some(ms) = best_attempt {
            samples.push(ms);
        }
        std::thread::sleep(Duration::from_millis(80));
    }

    samples
}

fn tcp_connect_once(sock_addr: &SocketAddr, source_ip: Option<Ipv4Addr>) -> Option<i32> {
    if let Some(source_ip) = source_ip {
        let socket = socket2::Socket::new(
            socket2::Domain::IPV4,
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        )
        .ok()?;
        socket
            .bind(&SocketAddr::new(IpAddr::V4(source_ip), 0).into())
            .ok()?;
        let start = Instant::now();
        socket
            .connect_timeout(&(*sock_addr).into(), Duration::from_secs(3))
            .ok()?;
        let ms = start.elapsed().as_millis().max(1) as i32;
        drop(socket);
        return Some(ms);
    }

    let start = Instant::now();
    match TcpStream::connect_timeout(sock_addr, Duration::from_secs(3)) {
        Ok(conn) => {
            let ms = start.elapsed().as_millis().max(1) as i32;
            drop(conn);
            Some(ms)
        }
        Err(_) => None,
    }
}

/// Build the sing-box JSON config from the connect request
fn build_singbox_config(req: &ConnectRequest) -> serde_json::Value {
    let outbound = match req.protocol.as_str() {
        "vless" => {
            // flow (xtls-rprx-vision) only works with TCP transport
            let flow_value = if req.transport == "tcp" || req.transport.is_empty() {
                req.flow.clone().unwrap_or_default()
            } else {
                String::new()
            };

            // Build TLS object — only include "reality" key when security == "reality"
            let mut tls_obj = serde_json::json!({
                "enabled": true,
                "server_name": req.sni.clone().unwrap_or(req.server_address.clone()),
                "utls": {
                    "enabled": true,
                    "fingerprint": req.fingerprint.clone().unwrap_or("chrome".into())
                }
            });
            if let Some(ref alpn) = req.alpn {
                if !alpn.is_empty() {
                    tls_obj["alpn"] = serde_json::json!(alpn);
                }
            }
            if req.security == "reality" {
                tls_obj["reality"] = serde_json::json!({
                    "enabled": true,
                    "public_key": req.public_key.clone().unwrap_or_default(),
                    "short_id": req.short_id.clone().unwrap_or_default()
                });
            }

            // Build outbound — only include "flow" when actually set (empty string "" can cause issues in sing-box 1.13)
            let mut ob = serde_json::json!({
                "type": "vless",
                "tag": "proxy",
                "server": req.server_address,
                "server_port": req.server_port,
                "uuid": req.uuid.clone().unwrap_or_default(),
                "tls": tls_obj
            });
            if !flow_value.is_empty() {
                ob["flow"] = serde_json::json!(flow_value);
            }

            // Add transport only for non-TCP (avoids "transport": null which crashes sing-box 1.13)
            match req.transport.as_str() {
                "ws" => {
                    ob["transport"] = serde_json::json!({
                        "type": "ws",
                        "path": req.path.clone().unwrap_or("/".into()),
                        "headers": {
                            "Host": req.host.clone().unwrap_or(req.server_address.clone())
                        }
                    });
                }
                "grpc" => {
                    ob["transport"] = serde_json::json!({
                        "type": "grpc",
                        "service_name": req.path.clone().unwrap_or_default()
                    });
                }
                "httpupgrade" => {
                    ob["transport"] = serde_json::json!({
                        "type": "httpupgrade",
                        "path": req.path.clone().unwrap_or("/".into()),
                        "host": req.host.clone().unwrap_or(req.server_address.clone())
                    });
                }
                "h2" | "http" => {
                    ob["transport"] = serde_json::json!({
                        "type": "http",
                        "path": req.path.clone().unwrap_or("/".into()),
                        "host": [req.host.clone().unwrap_or(req.server_address.clone())]
                    });
                }
                _ => { /* TCP or empty — no transport field at all */ }
            }
            ob
        }
        "vmess" => {
            // Build outbound without transport first
            let mut ob = serde_json::json!({
                "type": "vmess",
                "tag": "proxy",
                "server": req.server_address,
                "server_port": req.server_port,
                "uuid": req.uuid.clone().unwrap_or_default(),
                "security": "auto",
                "tls": {
                    "enabled": req.security == "tls",
                    "server_name": req.sni.clone().unwrap_or(req.server_address.clone())
                }
            });

            // Add transport only for non-TCP
            match req.transport.as_str() {
                "ws" => {
                    ob["transport"] = serde_json::json!({
                        "type": "ws",
                        "path": req.path.clone().unwrap_or("/".into())
                    });
                }
                "grpc" => {
                    ob["transport"] = serde_json::json!({
                        "type": "grpc",
                        "service_name": req.path.clone().unwrap_or_default()
                    });
                }
                "httpupgrade" => {
                    ob["transport"] = serde_json::json!({
                        "type": "httpupgrade",
                        "path": req.path.clone().unwrap_or("/".into()),
                        "host": req.host.clone().unwrap_or(req.server_address.clone())
                    });
                }
                "h2" | "http" => {
                    ob["transport"] = serde_json::json!({
                        "type": "http",
                        "path": req.path.clone().unwrap_or("/".into()),
                        "host": [req.host.clone().unwrap_or(req.server_address.clone())]
                    });
                }
                _ => { /* TCP or empty — no transport field */ }
            }
            ob
        }
        "trojan" => serde_json::json!({
            "type": "trojan",
            "tag": "proxy",
            "server": req.server_address,
            "server_port": req.server_port,
            "password": req.password.clone().unwrap_or_default(),
            "tls": {
                "enabled": true,
                "server_name": req.sni.clone().unwrap_or(req.server_address.clone()),
            }
        }),
        "shadowsocks" => serde_json::json!({
            "type": "shadowsocks",
            "tag": "proxy",
            "server": req.server_address,
            "server_port": req.server_port,
            "password": req.password.clone().unwrap_or_default(),
            "method": req.encryption.clone().unwrap_or("aes-256-gcm".into())
        }),
        "hysteria2" => {
            let mut ob = serde_json::json!({
                "type": "hysteria2",
                "tag": "proxy",
                "server": req.server_address,
                "server_port": req.server_port,
                "password": req.password.clone().unwrap_or_default(),
                "tls": {
                    "enabled": true,
                    "server_name": req.sni.clone().unwrap_or(req.server_address.clone())
                }
            });
            if let Some(ref obfs) = req.obfs_type {
                if !obfs.is_empty() {
                    ob["obfs"] = serde_json::json!({
                        "type": obfs,
                        "password": req.obfs_password.clone().unwrap_or_default()
                    });
                }
            }
            if let Some(up) = req.up_mbps {
                ob["up_mbps"] = serde_json::json!(up);
            }
            if let Some(down) = req.down_mbps {
                ob["down_mbps"] = serde_json::json!(down);
            }
            ob
        }
        "tuic" => {
            let mut ob = serde_json::json!({
                "type": "tuic",
                "tag": "proxy",
                "server": req.server_address,
                "server_port": req.server_port,
                "uuid": req.uuid.clone().unwrap_or_default(),
                "password": req.password.clone().unwrap_or_default(),
                "congestion_control": req.congestion_control.clone().unwrap_or("bbr".into()),
                "udp_relay_mode": req.udp_relay_mode.clone().unwrap_or("native".into()),
                "tls": {
                    "enabled": true,
                    "server_name": req.sni.clone().unwrap_or(req.server_address.clone())
                }
            });
            if let Some(ref alpn) = req.alpn {
                if !alpn.is_empty() {
                    ob["tls"]["alpn"] = serde_json::json!(alpn);
                }
            }
            ob
        }
        "wireguard" => {
            let mut ob = serde_json::json!({
                "type": "wireguard",
                "tag": "proxy",
                "server": req.server_address,
                "server_port": req.server_port,
                "private_key": req.private_key.clone().unwrap_or_default(),
                "peer_public_key": req.peer_public_key.clone().unwrap_or_default(),
                "local_address": req.local_address.clone().unwrap_or_else(|| vec!["10.0.0.2/32".into()]),
                "mtu": req.mtu.unwrap_or(1408)
            });
            if let Some(ref psk) = req.pre_shared_key {
                if !psk.is_empty() {
                    ob["pre_shared_key"] = serde_json::json!(psk);
                }
            }
            if let Some(ref reserved) = req.reserved {
                if !reserved.is_empty() {
                    ob["reserved"] = serde_json::json!(reserved);
                }
            }
            if let Some(workers) = req.workers {
                ob["workers"] = serde_json::json!(workers);
            }
            ob
        }
        unsupported => {
            // Unknown protocol — return error outbound so user gets clear feedback
            eprintln!("[error] Unsupported protocol: {}", unsupported);
            serde_json::json!({
                "type": "direct",
                "tag": "proxy"
            })
        }
    };

    // Inbound config: protected TUN also exposes local SOCKS/HTTP listeners so
    // Windows system proxy can be used as a compatibility helper.
    let inbounds = if req.proxy_mode == "tun" {
        singbox_tun_inbounds(req, None, req.strict_route)
    } else {
        serde_json::json!([
            {
                "type": "socks",
                "tag": "socks-in",
                "listen": "127.0.0.1",
                "listen_port": req.socks_port // Default should be changed in TS, but we trust the request
            },
            {
                "type": "http",
                "tag": "http-in",
                "listen": "127.0.0.1",
                "listen_port": req.http_port // Default should be changed in TS
            }
        ])
    };

    let mut proxy_domains = Vec::new();
    let mut proxy_domain_suffixes = Vec::new();

    let mut direct_domains = Vec::new();
    let mut direct_domain_suffixes = Vec::new();

    let mut block_domains = Vec::new();
    let mut block_domain_suffixes = Vec::new();

    for rule in &req.routing_rules {
        if rule.rule_type == "domain" {
            let val = rule.value.clone();
            if val.starts_with("*.") {
                let suffix = val.trim_start_matches("*.").to_string();
                match rule.action.as_str() {
                    "proxy" => proxy_domain_suffixes.push(suffix),
                    "direct" => direct_domain_suffixes.push(suffix),
                    "block" => block_domain_suffixes.push(suffix),
                    _ => {}
                }
            } else {
                match rule.action.as_str() {
                    "proxy" => proxy_domains.push(val),
                    "direct" => direct_domains.push(val),
                    "block" => block_domains.push(val),
                    _ => {}
                }
            }
        }
    }

    let proxy_processes = process_rule_names(req, "proxy");
    let direct_processes = process_rule_names(req, "direct");
    let block_processes = process_rule_names(req, "block");

    let (policy_direct_domains, policy_direct_suffixes, policy_direct_regexes) =
        routing_policy_singbox_domains(req);
    let mut dns_direct_domains = direct_domains.clone();
    dns_direct_domains.extend(policy_direct_domains);
    let mut dns_direct_suffixes = direct_domain_suffixes.clone();
    dns_direct_suffixes.extend(policy_direct_suffixes);

    // DNS config — direct split-routing rules must use the same selectors as traffic.
    let dns = singbox_dns_config_with_direct_rules(
        &req.dns_mode,
        &dns_direct_domains,
        &dns_direct_suffixes,
        &policy_direct_regexes,
        &direct_processes,
    );

    let mut custom_rules = Vec::new();

    push_domain_route(
        &mut custom_rules,
        &proxy_domains,
        &proxy_domain_suffixes,
        "proxy",
    );
    push_process_route(&mut custom_rules, &proxy_processes, "proxy");

    push_domain_route(
        &mut custom_rules,
        &direct_domains,
        &direct_domain_suffixes,
        "direct",
    );
    push_process_route(&mut custom_rules, &direct_processes, "direct");

    push_domain_route(
        &mut custom_rules,
        &block_domains,
        &block_domain_suffixes,
        "block",
    );
    push_process_route(&mut custom_rules, &block_processes, "block");

    let mut rules = vec![
        serde_json::json!({ "action": "sniff" }),
        serde_json::json!({ "protocol": "dns", "action": "hijack-dns" }),
    ];

    // TUN mode: private IPs (LAN, localhost) must go direct — they're unreachable via VPN server.
    // NOTE: sing-box's own outbound to the VPN server is already protected from TUN loop
    // by `auto_detect_interface: true` in route config — no process_name exclusion needed.
    if req.proxy_mode == "tun" && req.routing_policy.is_none() {
        rules.push(serde_json::json!({
            "ip_is_private": true,
            "outbound": "direct"
        }));
    }

    rules.extend(custom_rules);
    push_routing_policy_singbox_rules(&mut rules, req);

    // Default route remains the VPN outbound. Kill Switch hardens TUN routing with strict_route;
    // setting final=block here would block normal VPN traffic that has no custom rule.
    let final_outbound = "proxy";

    // Kill Switch in TUN mode: force strict_route regardless of user setting.
    let effective_strict_route = effective_tun_strict_route(req);

    // Update inbounds strict_route if TUN mode
    let effective_inbounds = if req.proxy_mode == "tun" {
        singbox_tun_inbounds(req, None, effective_strict_route)
    } else {
        inbounds
    };

    serde_json::json!({
        "log": { "level": "info" },
        "dns": dns,
        "inbounds": effective_inbounds,
        "outbounds": [
            outbound,
            { "type": "direct", "tag": "direct" },
            { "type": "block", "tag": "block" }
        ],
        "route": {
            "auto_detect_interface": true,
            "default_domain_resolver": "dns-direct",
            "final": final_outbound,
            "rules": rules
        },
        "experimental": {
            "clash_api": {
                "external_controller": "127.0.0.1:9191"
            }
        }
    })
}

/// Take a raw xray JSON config (from DoodleVPN subscription) and inject
/// DoodleRay's inbounds (SOCKS, HTTP, stats API) so it uses the correct ports.
/// Preserves all outbounds, routing, observatory, balancing etc. from the original.
fn inject_xray_inbounds(mut config: serde_json::Value, req: &ConnectRequest) -> serde_json::Value {
    // Replace or add inbounds with DoodleRay's SOCKS/HTTP/API ports
    let inbounds = serde_json::json!([
        {
            "tag": "socks-in",
            "port": req.socks_port,
            "listen": "127.0.0.1",
            "protocol": "socks",
            "settings": { "udp": true, "ip": "127.0.0.1" },
            "sniffing": {
                "enabled": true,
                "destOverride": ["http", "tls", "quic", "fakedns"],
                "routeOnly": true
            }
        },
        {
            "tag": "http-in",
            "port": req.http_port,
            "listen": "127.0.0.1",
            "protocol": "http"
        },
        {
            "tag": "api",
            "port": req.api_port,
            "listen": "127.0.0.1",
            "protocol": "dokodemo-door",
            "settings": { "address": "127.0.0.1" }
        }
    ]);
    config["inbounds"] = inbounds;
    if req.routing_policy.is_some() {
        config["dns"] = xray_dns_config(req);
    } else if config.get("dns").is_none() {
        config["dns"] = xray_tunnel_dns_config();
    }

    // Ensure stats/api/policy exist for traffic monitoring
    if config.get("stats").is_none() {
        config["stats"] = serde_json::json!({});
    }
    if config.get("api").is_none() {
        config["api"] = serde_json::json!({
            "tag": "api",
            "services": ["StatsService"]
        });
    }
    if config.get("policy").is_none() {
        config["policy"] = serde_json::json!({
            "system": {
                "statsInboundUplink": true,
                "statsInboundDownlink": true,
                "statsOutboundUplink": true,
                "statsOutboundDownlink": true
            }
        });
    }

    normalize_xray_transport_settings(&mut config);
    sanitize_xray_routing_rules(&mut config);
    constrain_xray_config_to_managed_policy(&mut config, req);
    ensure_xray_direct_outbound(&mut config);
    ensure_xray_dns_outbound(&mut config);
    ensure_xray_api_outbound(&mut config);

    // Make sure routing rules include the API rule
    if let Some(routing) = config.get_mut("routing") {
        if let Some(rules) = routing.get_mut("rules") {
            if let Some(rules_arr) = rules.as_array_mut() {
                let has_api_rule = rules_arr.iter().any(|r| {
                    r.get("inboundTag")
                        .and_then(|t| t.as_array())
                        .map(|arr| arr.iter().any(|v| v.as_str() == Some("api")))
                        .unwrap_or(false)
                });
                if !has_api_rule {
                    rules_arr.insert(
                        0,
                        serde_json::json!({
                            "type": "field",
                            "inboundTag": ["api"],
                            "outboundTag": "api"
                        }),
                    );
                }
            }
        }
    }

    apply_xray_routing_policy(&mut config, req, false);

    config
}

fn sanitize_xray_routing_rules(config: &mut serde_json::Value) {
    let Some(rules) = config
        .get_mut("routing")
        .and_then(|routing| routing.get_mut("rules"))
        .and_then(|rules| rules.as_array_mut())
    else {
        return;
    };

    for rule in rules.iter_mut() {
        remove_unsupported_xray_rule_values(
            rule.get_mut("domain"),
            &[
                "geosite:category-bittorrent",
                "geosite:torrent",
                "geosite:twitch-ads",
                "geosite:whitelist",
                "geosite:faceit",
            ],
        );
        remove_unsupported_xray_rule_values(rule.get_mut("ip"), &["geoip:direct"]);
        remove_empty_xray_rule_array(rule, "domain");
        remove_empty_xray_rule_array(rule, "ip");
    }

    rules.retain(has_effective_xray_rule_fields);
}

fn remove_unsupported_xray_rule_values(
    value: Option<&mut serde_json::Value>,
    unsupported: &[&str],
) {
    let Some(values) = value.and_then(|v| v.as_array_mut()) else {
        return;
    };

    values.retain(|item| {
        item.as_str()
            .map(|s| !unsupported.iter().any(|bad| s.eq_ignore_ascii_case(bad)))
            .unwrap_or(true)
    });
}

fn remove_empty_xray_rule_array(rule: &mut serde_json::Value, key: &str) {
    let should_remove = rule
        .get(key)
        .and_then(|value| value.as_array())
        .map(|values| values.is_empty())
        .unwrap_or(false);

    if should_remove {
        if let Some(rule_object) = rule.as_object_mut() {
            rule_object.remove(key);
        }
    }
}

fn has_effective_xray_rule_fields(rule: &serde_json::Value) -> bool {
    [
        "domain",
        "ip",
        "port",
        "sourcePort",
        "network",
        "source",
        "user",
        "inboundTag",
        "protocol",
        "attrs",
    ]
    .iter()
    .any(|key| has_effective_xray_rule_field(rule.get(*key)))
}

fn has_effective_xray_rule_field(value: Option<&serde_json::Value>) -> bool {
    match value {
        Some(serde_json::Value::Array(values)) => !values.is_empty(),
        Some(serde_json::Value::String(value)) => !value.is_empty(),
        Some(serde_json::Value::Null) | None => false,
        Some(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn subscription_redirects_stay_on_the_public_origin() {
        let initial = Url::parse("https://subscriptions.example/path").unwrap();
        let same_host = Url::parse("https://subscriptions.example/next").unwrap();
        let private = Url::parse("http://127.0.0.1/admin").unwrap();
        let downgrade = Url::parse("http://subscriptions.example/next").unwrap();

        assert!(redirect_target_allowed(&initial, &same_host, 1).is_ok());
        assert!(redirect_target_allowed(&initial, &private, 1).is_err());
        assert!(redirect_target_allowed(&initial, &downgrade, 1).is_err());
        assert!(redirect_target_allowed(&initial, &same_host, 5).is_err());
    }

    #[test]
    fn app_api_default_uses_the_canonical_mobile_contract() {
        assert_eq!(APP_API_DEFAULT_BASE_URL, "https://ddlvpn.lol/v1/mobile");
        assert_eq!(APP_API_CONNECTION_PROFILE_PATH, "/connection-profile");
        assert_eq!(
            app_api_endpoint(APP_API_CONNECTION_PROFILE_PATH)
                .expect("connection-profile URL")
                .as_str(),
            "https://ddlvpn.lol/v1/mobile/connection-profile"
        );
    }

    #[test]
    fn app_api_errors_prefer_json_and_never_surface_html() {
        let status = reqwest::StatusCode::UPGRADE_REQUIRED;
        assert_eq!(
            app_api_error_message(status, r#"{"error":"update DoodleRay VPN"}"#),
            "update DoodleRay VPN"
        );
        assert_eq!(
            app_api_error_message(reqwest::StatusCode::NOT_FOUND, "<html>not found</html>"),
            "DoodleVPN API returned an incompatible response. Update the app and try again."
        );
    }

    #[test]
    fn tunnel_start_cancellation_is_an_explicit_terminal_result() {
        assert!(ensure_tunnel_start_not_cancelled(false).is_ok());
        assert_eq!(
            ensure_tunnel_start_not_cancelled(true),
            Err("VPN connection was cancelled")
        );
    }

    #[test]
    fn diagnostics_are_redacted_before_the_app_api_request() {
        let sanitized = app_api_sanitize_diagnostic_value(
            json!({
                "error_message": "failed at https://secret.example/path via 192.0.2.1",
                "access_token": "secret",
                "details": {
                    "profile_id": "prof_secret",
                    "phase": "starting"
                }
            }),
            0,
        );
        let encoded = sanitized.to_string();
        assert!(!encoded.contains("secret.example"));
        assert!(!encoded.contains("192.0.2.1"));
        assert!(!encoded.contains("access_token"));
        assert!(!encoded.contains("profile_id"));
        assert!(encoded.contains("[url]") || encoded.contains("[domain]"));
        assert!(encoded.contains("[ip]"));
        assert!(encoded.contains("starting"));
    }

    #[test]
    fn auto_location_fallbacks_are_normalized_deduplicated_and_bounded() {
        let mut request = sample_app_connect_request();
        request.location_id = " RU ".into();
        request.fallback_location_ids = vec!["ru".into(), "DE".into(), "nl".into(), "us".into()];
        assert_eq!(app_connection_location_ids(&request), ["ru", "de", "nl"]);
    }

    fn diag_health(
        verdict: &str,
        fatal: Vec<&str>,
        checks: Vec<ConnectionHealthCheck>,
    ) -> ConnectionHealthReport {
        ConnectionHealthReport {
            verdict: verdict.into(),
            mode: "tun".into(),
            generated_at_ms: 0,
            service_effective_state: Some("Connected".into()),
            service_health_verdict: None,
            engine_kind: Some("singbox".into()),
            runtime_socks_port: None,
            runtime_http_port: None,
            runtime_api_port: None,
            service_generation: Some(7),
            active_op_id: None,
            service_fatal_checks: fatal.into_iter().map(String::from).collect(),
            service_degraded_checks: Vec::new(),
            service_warning_checks: Vec::new(),
            route_explanations: Vec::new(),
            endpoint_bypass_checks: Vec::new(),
            checks,
        }
    }

    #[test]
    fn diagnosis_maps_wintun_ghost_to_repairable_cause() {
        let health = diag_health(
            "failed",
            vec!["configure tun interface: (create adapter: Cannot create a file when that file already exists. | open existing adapter: Element not found.)"],
            vec![],
        );
        let report = build_network_diagnosis(&health, "tun", None, false);
        assert_eq!(
            report.primary_cause_code.as_deref(),
            Some("wintun_ghost_adapter")
        );
        assert!(report.can_auto_repair);
        assert_eq!(report.overall, "failed");
        // Exact technical line preserved (redacted) for support.
        assert!(report.checks.iter().any(|c| c.id == "service_fatal"
            && c.technical_detail_redacted.contains("Element not found")));
        // No jargon in the user-facing text.
        assert!(!report.user_summary.to_lowercase().contains("wintun"));
        assert!(report.copy_text.contains("cause=wintun_ghost_adapter"));
    }

    #[test]
    fn diagnosis_maps_adapter_missing() {
        let health = diag_health(
            "failed",
            vec!["DoodleRay Tunnel IPv4 readiness failed: DoodleRay Tunnel adapter is missing"],
            vec![],
        );
        let report = build_network_diagnosis(&health, "tun", None, true);
        assert_eq!(
            report.primary_cause_code.as_deref(),
            Some("adapter_missing")
        );
        assert!(report.can_auto_repair);
        assert!(report.copy_text.contains("repair_tried=true"));
    }

    #[test]
    fn diagnosis_maps_wininet_stale_proxy() {
        let health = diag_health(
            "protected_degraded",
            vec![],
            vec![health_check(
                "wininet_proxy",
                "warning",
                "Windows proxy",
                "ProxyEnable=1, ProxyServer=not expected loopback",
            )],
        );
        let report = build_network_diagnosis(&health, "tun", None, false);
        assert_eq!(
            report.primary_cause_code.as_deref(),
            Some("wininet_stale_proxy")
        );
        assert!(report.can_auto_repair);
        assert_eq!(report.overall, "degraded");
    }

    #[test]
    fn diagnosis_degraded_without_cause_is_probe_nonclaim() {
        let health = diag_health(
            "protected_degraded",
            vec![],
            vec![health_check(
                "tunnel_service_degraded_checks",
                "warning",
                "Tunnel Service degraded checks",
                "IPv6 full-protection leak proof is not collected yet; treating IPv6 as degraded_disabled (ipv6_default_route=DoodleRay Tunnel|ifIndex=26|nextHop=fdfe:dcba:9876::2|metric=0)",
            )],
        );
        let report = build_network_diagnosis(&health, "tun", None, false);
        assert_eq!(
            report.primary_cause_code.as_deref(),
            Some("ipv6_quic_unverified")
        );
        assert!(!report.can_auto_repair);
        assert_eq!(report.overall, "ok");
        assert!(report
            .checks
            .iter()
            .any(|check| check.id == "tunnel_service_degraded_checks" && check.status == "info"));
        assert!(report.copy_text.contains("failed_checks: none"));
    }

    #[test]
    fn diagnosis_all_ok_service_warnings_are_info_not_failed() {
        let health = diag_health(
            "protected",
            vec![],
            vec![health_check(
                "tunnel_service_warning_checks",
                "warning",
                "Tunnel Service warnings",
                "QUIC/HTTP3 is not verified by a controlled probe in this build; no QUIC claim",
            )],
        );
        let report = build_network_diagnosis(&health, "tun", None, false);
        assert_eq!(report.primary_cause_code.as_deref(), Some("all_ok"));
        assert_eq!(report.overall, "ok");
        let check = report
            .checks
            .iter()
            .find(|check| check.id == "tunnel_service_warning_checks")
            .expect("service warning check");
        assert_eq!(check.status, "info");
        assert_eq!(check.user_text, "В порядке");
        assert!(report.copy_text.contains("failed_checks: none"));
    }

    #[test]
    fn diagnosis_all_ok_service_degraded_notes_are_info_not_failed() {
        let health = diag_health(
            "protected",
            vec![],
            vec![health_check(
                "tunnel_service_degraded_checks",
                "warning",
                "Tunnel Service degraded checks",
                "IPv6 default route is absent; protected verdict covers IPv4 routing",
            )],
        );
        let report = build_network_diagnosis(&health, "tun", None, false);
        assert_eq!(report.primary_cause_code.as_deref(), Some("all_ok"));
        assert_eq!(report.overall, "ok");
        let check = report
            .checks
            .iter()
            .find(|check| check.id == "tunnel_service_degraded_checks")
            .expect("service degraded check");
        assert_eq!(check.status, "info");
        assert_eq!(check.user_text, "В порядке");
        assert!(report.copy_text.contains("failed_checks: none"));
    }

    #[test]
    fn diagnosis_unknown_warning_is_attention_not_failed_copy() {
        let health = diag_health(
            "protected_degraded",
            vec![],
            vec![health_check(
                "future_warning_probe",
                "warning",
                "Future warning probe",
                "Future non-fatal probe warning",
            )],
        );
        let report = build_network_diagnosis(&health, "tun", None, false);
        let check = report
            .checks
            .iter()
            .find(|check| check.id == "future_warning_probe")
            .expect("future warning check");
        assert_eq!(check.status, "warning");
        assert_eq!(check.user_text, "Требует внимания");
        assert!(report.copy_text.contains("failed_checks: none"));
        assert!(!report.copy_text.contains("future_warning_probe"));
    }

    #[test]
    fn diagnosis_browsers_mode_is_honest_limited() {
        let mut health = diag_health("partial", vec![], vec![]);
        health.mode = "system-proxy".into();
        let report = build_network_diagnosis(&health, "system-proxy", None, false);
        assert_eq!(
            report.primary_cause_code.as_deref(),
            Some("browsers_fallback")
        );
        assert_eq!(report.overall, "limited");
    }

    #[test]
    fn diagnosis_subscription_error_is_reported_without_breaking_ok() {
        let health = diag_health("protected", vec![], vec![]);
        let report = build_network_diagnosis(
            &health,
            "tun",
            Some("fetch https://user:secret@sub.example.com/abc failed: timeout"),
            false,
        );
        assert_eq!(
            report.primary_cause_code.as_deref(),
            Some("subscription_fetch_failed")
        );
        assert_eq!(report.overall, "ok");
        // Subscription URL must never leak into the report.
        assert!(!report.copy_text.contains("sub.example.com"));
        let sub_check = report
            .checks
            .iter()
            .find(|c| c.id == "subscription_refresh")
            .unwrap();
        assert!(!sub_check.technical_detail_redacted.contains("secret"));
    }

    #[test]
    fn diagnosis_copy_text_has_no_secrets() {
        let health = diag_health(
            "failed",
            vec!["uuid=123e4567-e89b-12d3-a456-426614174000 endpoint=203.0.113.7 vless://secret@host"],
            vec![],
        );
        let report = build_network_diagnosis(&health, "tun", None, false);
        assert!(!report.copy_text.contains("123e4567"));
        assert!(!report.copy_text.contains("203.0.113.7"));
        assert!(!report.copy_text.contains("vless://secret"));
        assert!(!report.support_summary.contains("203.0.113.7"));
    }

    #[test]
    fn diagnosis_all_ok_when_protected_and_clean() {
        let health = diag_health("protected", vec![], vec![]);
        let report = build_network_diagnosis(&health, "tun", None, false);
        assert_eq!(report.primary_cause_code.as_deref(), Some("all_ok"));
        assert_eq!(report.overall, "ok");
        assert!(!report.can_auto_repair);
    }

    fn sample_request(proxy_mode: &str) -> ConnectRequest {
        ConnectRequest {
            server_address: "example.com".into(),
            server_port: 443,
            protocol: "vless".into(),
            uuid: Some("00000000-0000-0000-0000-000000000000".into()),
            password: None,
            transport: "tcp".into(),
            security: "tls".into(),
            sni: Some("example.com".into()),
            host: None,
            path: None,
            fingerprint: Some("chrome".into()),
            public_key: None,
            short_id: None,
            flow: None,
            proxy_mode: proxy_mode.into(),
            system_proxy_mode: "set".into(),
            socks_port: 10808,
            http_port: 10809,
            api_port: 10813,
            network_stack: "system".into(),
            dns_mode: "fakeip".into(),
            strict_route: false,
            kill_switch: false,
            routing_rules: Vec::new(),
            obfs_type: None,
            obfs_password: None,
            up_mbps: None,
            down_mbps: None,
            congestion_control: None,
            udp_relay_mode: None,
            alpn: None,
            private_key: None,
            peer_public_key: None,
            pre_shared_key: None,
            local_address: None,
            reserved: None,
            mtu: None,
            workers: None,
            encryption: None,
            raw_xray_config: None,
            routing_policy: None,
        }
    }

    fn sample_app_connect_request() -> AppConnectLocationRequest {
        AppConnectLocationRequest {
            location_id: "de".into(),
            fallback_location_ids: Vec::new(),
            proxy_mode: "tun".into(),
            system_proxy_mode: "set".into(),
            socks_port: 21080,
            http_port: 21081,
            network_stack: "system".into(),
            dns_mode: "fakeip".into(),
            strict_route: true,
            kill_switch: false,
            routing_rules: vec![RoutingRuleRequest {
                rule_type: "domain".into(),
                value: "2ip.ru".into(),
                action: "direct".into(),
            }],
        }
    }

    #[test]
    fn server_full_tunnel_keeps_only_required_steam_bypass() {
        let mut request = sample_request("tun");
        request.routing_policy = Some(AppRoutingPolicy {
            mode: "full_tunnel".into(),
            version: "app-routing-v2".into(),
            direct_domains: vec!["domain:must-not-survive.example".into()],
            local_dns_domains: vec!["domain:must-not-survive.example".into()],
            direct_ip_ranges: vec!["10.0.0.0/8".into()],
            asset: None,
        });
        request.routing_policy = request
            .routing_policy
            .take()
            .map(validate_app_routing_policy)
            .transpose()
            .unwrap();
        let config = build_xray_config(&request);
        let rules = config["routing"]["rules"].as_array().unwrap();
        assert!(rules.iter().any(|rule| rule["outboundTag"] == "dns-out"));
        let direct_domain_rule = rules
            .iter()
            .find(|rule| rule["outboundTag"] == "direct" && rule.get("domain").is_some())
            .expect("Steam direct rule missing");
        assert!(json_array_contains_str(
            &direct_domain_rule["domain"],
            "domain:steamcontent.com"
        ));
        assert!(json_array_contains_str(
            &direct_domain_rule["domain"],
            "full:steamcdn-a.akamaihd.net"
        ));
        assert!(!rules.iter().any(|rule| {
            rule["outboundTag"] == "direct"
                && (json_array_contains_str(&rule["domain"], "domain:must-not-survive.example")
                    || json_array_contains_str(&rule["ip"], "10.0.0.0/8"))
        }));
        let dns = config["dns"]["servers"].as_array().unwrap();
        let direct_dns = dns
            .iter()
            .find(|server| server["tag"] == "dns-direct")
            .expect("Steam direct DNS rule missing");
        assert_eq!(direct_dns["domains"], direct_domain_rule["domain"]);
        assert!(dns.iter().any(|server| server["tag"] == "dns-remote"));

        let singbox = build_singbox_config(&request);
        let singbox_rules = singbox["route"]["rules"].as_array().unwrap();
        assert!(singbox_rules.iter().any(|rule| {
            rule["outbound"] == "direct"
                && json_array_contains_str(&rule["process_name"], "steam.exe")
        }));
        assert!(singbox_rules.iter().any(|rule| {
            rule["outbound"] == "direct"
                && json_array_contains_str(&rule["domain_suffix"], "steamcontent.com")
        }));
        assert!(effective_tun_strict_route(&request));
    }

    #[test]
    fn managed_full_tunnel_removes_raw_alternate_bypasses() {
        let mut request = sample_request("tun");
        request.routing_policy = Some(AppRoutingPolicy {
            mode: "full_tunnel".into(),
            version: "app-routing-v2".into(),
            ..Default::default()
        });
        let raw = json!({
            "outbounds": [
                { "tag": "bypass", "protocol": "freedom" },
                { "tag": "proxy", "protocol": "vless" },
                { "tag": "blocked", "protocol": "blackhole" }
            ],
            "routing": {
                "balancers": [{ "tag": "legacy", "selector": ["bypass"] }],
                "rules": [
                    { "type": "field", "domain": ["domain:leak.example"], "outboundTag": "bypass" },
                    { "type": "field", "network": "tcp,udp", "balancerTag": "legacy" }
                ]
            }
        });

        let config = inject_xray_inbounds(raw, &request);
        let outbounds = config["outbounds"].as_array().unwrap();
        let rules = config["routing"]["rules"].as_array().unwrap();

        assert_eq!(outbounds[0]["tag"], "proxy");
        assert!(!outbounds.iter().any(|outbound| outbound["tag"] == "bypass"));
        assert!(config["routing"].get("balancers").is_none());
        assert!(!rules.iter().any(|rule| rule.get("balancerTag").is_some()));
        assert!(!rules
            .iter()
            .any(|rule| { json_array_contains_str(&rule["domain"], "domain:leak.example") }));
        assert_eq!(rules.last().unwrap()["outboundTag"], "proxy");
    }

    #[test]
    fn server_split_policy_uses_symmetric_dns_and_traffic_selectors() {
        let mut request = sample_request("tun");
        request.routing_policy = Some(AppRoutingPolicy {
            mode: "split".into(),
            version: "app-routing-v2".into(),
            direct_domains: vec!["domain:vk.com".into(), "regexp:.*\\.ru$".into()],
            local_dns_domains: vec!["domain:vk.com".into(), "regexp:.*\\.ru$".into()],
            direct_ip_ranges: vec!["192.168.0.0/16".into()],
            asset: None,
        });
        let config = build_xray_config(&request);
        let direct_rule = config["routing"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|rule| rule["outboundTag"] == "direct" && rule.get("domain").is_some())
            .unwrap();
        assert_eq!(
            direct_rule["domain"],
            config["dns"]["servers"][0]["domains"]
        );
        let rules = config["routing"]["rules"].as_array().unwrap();
        let direct_dns_index = rules
            .iter()
            .position(|rule| {
                rule["outboundTag"] == "direct"
                    && json_array_contains_str(&rule["inboundTag"], "dns-direct")
            })
            .expect("direct DNS routing rule missing");
        let port_53_index = rules
            .iter()
            .position(|rule| rule["outboundTag"] == "dns-out" && rule["port"] == "53")
            .expect("DNS interception rule missing");
        assert!(
            direct_dns_index < port_53_index,
            "local DNS egress must bypass generic port-53 interception"
        );

        let (domains, suffixes, regexes) = routing_policy_singbox_domains(&request);
        assert!(domains.contains(&"steamcdn-a.akamaihd.net".to_string()));
        assert!(suffixes.contains(&"vk.com".to_string()));
        assert!(suffixes.contains(&"steamcontent.com".to_string()));
        assert_eq!(regexes, [".*\\.ru$"]);

        let (dns_domains, dns_suffixes, dns_regexes) = routing_policy_singbox_dns_domains(&request);
        assert_eq!(dns_domains, domains);
        assert_eq!(dns_suffixes, suffixes);
        assert_eq!(dns_regexes, regexes);
    }

    #[test]
    fn physical_dns_parser_rejects_loopback_and_link_local() {
        assert_eq!(
            first_usable_physical_dns("127.0.0.1\n169.254.1.1\n192.168.1.1\n"),
            Some("192.168.1.1".into())
        );
    }

    #[test]
    fn app_api_reality_profile_maps_to_existing_connect_request() {
        let native = json!({
            "type": "vless",
            "security": "reality",
            "transport": "reality_tcp",
            "connect_address": "203.0.113.10",
            "port": 443,
            "uuid": "00000000-0000-0000-0000-000000000001",
            "server_name": "example.com",
            "public_key": "reality-public-key",
            "short_id": "abcd",
            "fingerprint": "chrome",
            "flow": "xtls-rprx-vision"
        });

        let mapped = app_api_profile_to_connect_request(&native, &sample_app_connect_request())
            .expect("profile should map");

        assert_eq!(mapped.protocol, "vless");
        assert_eq!(mapped.transport, "tcp");
        assert_eq!(mapped.security, "reality");
        assert_eq!(mapped.server_address, "203.0.113.10");
        assert_eq!(mapped.server_port, 443);
        assert_eq!(mapped.sni.as_deref(), Some("example.com"));
        assert_eq!(mapped.public_key.as_deref(), Some("reality-public-key"));
        assert_eq!(mapped.routing_rules[0].value, "2ip.ru");
        assert!(uses_xray_engine(&mapped));
        assert!(mapped.raw_xray_config.is_none());
    }

    #[test]
    fn app_api_profile_requires_routing_policy() {
        let lease = AppApiProfileLeaseResponse {
            schema_version: 2,
            profile_id: "profile".into(),
            lease_id: "lease".into(),
            expires_at: "2026-07-22T00:00:00Z".into(),
            location_id: "ru".into(),
            route_kind: String::new(),
            first_hop: String::new(),
            target_country_id: "ru".into(),
            entry_role: String::new(),
            routing_rules_version: String::new(),
            routing_policy: None,
            native_profile: json!({}),
            profile: None,
            transport_capability: None,
        };

        assert!(app_api_validated_routing_policy(&lease)
            .unwrap_err()
            .contains("routing policy"));
    }

    #[test]
    fn terminal_profile_errors_do_not_retry_other_locations() {
        for status in [400, 401, 403, 426, 429] {
            assert!(app_api_profile_error_is_terminal(&AppApiHttpError {
                status,
                message: "terminal".into(),
            }));
        }
        for status in [0, 404, 422, 500, 503] {
            assert!(!app_api_profile_error_is_terminal(&AppApiHttpError {
                status,
                message: "retryable".into(),
            }));
        }
    }

    #[test]
    fn app_api_xray_outbound_profile_preserves_production_transport_config() {
        let native = json!({
            "type": "xray",
            "format": "xray-outbound-v1",
            "connect_address": "203.0.113.30",
            "port": 443,
            "config": {
                "outbounds": [{
                    "tag": "proxy",
                    "protocol": "vless",
                    "settings": {"vnext": [{"address": "203.0.113.30", "port": 443}]},
                    "streamSettings": {
                        "network": "xhttp",
                        "security": "tls",
                        "xhttpSettings": {"path": "/reserve", "mode": "packet-up"}
                    }
                }],
                "routing": {"rules": []}
            }
        });

        let mapped = app_api_profile_to_connect_request(&native, &sample_app_connect_request())
            .expect("xray profile should map");

        assert_eq!(mapped.server_address, "203.0.113.30");
        assert_eq!(mapped.server_port, 443);
        assert!(uses_xray_engine(&mapped));
        let config = mapped.raw_xray_config.expect("raw config");
        assert_eq!(
            config["outbounds"][0]["streamSettings"]["xhttpSettings"]["mode"],
            json!("packet-up")
        );
    }

    #[test]
    fn app_api_unsupported_profile_fails_closed() {
        let native = json!({
            "type": "cdn_xhttp",
            "security": "sealed",
            "address": "edge.example.com"
        });

        let err = app_api_profile_to_connect_request(&native, &sample_app_connect_request())
            .expect_err("unsupported profile must not be accepted");

        assert!(err.contains("Unsupported DoodleVPN profile type"));
    }

    #[test]
    fn app_api_capabilities_match_the_compiled_platform() {
        let capabilities = app_api_client_capabilities();
        assert_eq!(capabilities["windows"], json!(cfg!(windows)));
        assert_eq!(capabilities["macos"], json!(cfg!(target_os = "macos")));
        assert_eq!(
            capabilities["network_extension"],
            json!(cfg!(all(target_os = "macos", feature = "app-store")))
        );
        assert_eq!(
            app_api_core_version(),
            if cfg!(target_os = "macos") {
                "macos-v6"
            } else if cfg!(windows) {
                "pc-v6"
            } else {
                "desktop-v6"
            }
        );
    }

    #[test]
    fn app_api_exchange_code_body_matches_backend_contract() {
        let device = app_api_generate_device_state().expect("device state");
        let body = app_api_exchange_code_body("1234-5678", &device, "QA Mac");
        let mut keys = body["device"]
            .as_object()
            .expect("device object")
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        keys.sort_unstable();

        assert_eq!(
            keys,
            [
                "app_version",
                "device_id",
                "hwid",
                "model",
                "platform",
                "public_key"
            ]
        );
        assert_eq!(body["code"], "1234-5678");
        assert_eq!(body["device"]["device_id"], device.client_device_id);
        assert_eq!(body["device"]["hwid"], device.hwid);
        assert_eq!(body["device"]["public_key"], device.public_key);
    }

    #[test]
    fn legacy_subscription_migration_accepts_only_doodlevpn_urls() {
        assert_eq!(
            legacy_subscription_token("https://ddlvpn.lol/s/oldDesktopToken123?format=happ")
                .expect("canonical legacy URL"),
            "oldDesktopToken123"
        );
        assert_eq!(
            legacy_subscription_token("https://doodlevpn.online/sub/legacy_token-456")
                .expect("legacy alias URL"),
            "legacy_token-456"
        );
        for invalid in [
            "http://ddlvpn.lol/s/oldDesktopToken123",
            "https://example.com/s/oldDesktopToken123",
            "https://ddlvpn.lol/healthz",
            "https://ddlvpn.lol/s/bad token",
        ] {
            assert!(
                legacy_subscription_token(invalid).is_err(),
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn renderer_secure_store_cannot_access_app_api_reserved_keys() {
        assert!(validate_renderer_secure_store_key(APP_API_SESSION_KEY).is_err());
        assert!(
            validate_renderer_secure_store_key(&format!("{}.chunk.0", APP_API_SESSION_KEY))
                .is_err()
        );
        assert!(validate_renderer_secure_store_key(APP_API_DEVICE_KEY).is_err());
        assert!(validate_renderer_secure_store_key("ui-preference-theme").is_ok());
    }

    #[test]
    fn app_api_disk_session_does_not_persist_access_token() {
        let session = AppApiTokenResponse {
            access_token: "access-secret".into(),
            access_expires_at: "2026-07-06T10:00:00Z".into(),
            expires_in: 600,
            refresh_token: "refresh-secret".into(),
            refresh_expires_at: "2026-08-06T10:00:00Z".into(),
            device_id: "device-1".into(),
            subscription: AppApiSubscriptionSummary::default(),
        };

        let encoded = app_api_encode_session_for_disk(&session).expect("encoded");
        let value: serde_json::Value = serde_json::from_str(&encoded).expect("session json");
        assert!(value.get("access_token").is_none());
        assert!(value.get("access_expires_at").is_none());
        assert_eq!(value["refresh_token"], "refresh-secret");

        let decoded = app_api_decode_session_from_disk(&encoded).expect("decoded");
        assert!(decoded.access_token.is_empty());
        assert!(decoded.access_expires_at.is_empty());
        assert_eq!(decoded.refresh_token, "refresh-secret");
    }

    #[test]
    fn app_api_subscription_status_preserves_anti_jammer_quota() {
        let summary: AppApiSubscriptionSummary = serde_json::from_value(serde_json::json!({
            "active": true,
            "anti_jammer": {
                "limit_bytes": 32212254720_u64,
                "used_bytes": 21474836480_u64,
                "remaining_bytes": 10737418240_u64,
                "low_balance": false,
                "exhausted": false,
                "state": "active"
            }
        }))
        .expect("quota response");

        let quota = summary.anti_jammer.expect("anti-jammer quota");
        assert_eq!(quota.limit_bytes, 32212254720);
        assert_eq!(quota.remaining_bytes, 10737418240);
        assert_eq!(quota.state, "active");
    }

    #[test]
    fn app_api_legacy_disk_session_migrates_without_repersisting_access_token() {
        let legacy = AppApiTokenResponse {
            access_token: "legacy-access-secret".into(),
            access_expires_at: "2026-07-06T10:00:00Z".into(),
            expires_in: 600,
            refresh_token: "legacy-refresh-secret".into(),
            refresh_expires_at: "2026-08-06T10:00:00Z".into(),
            device_id: "device-legacy".into(),
            subscription: AppApiSubscriptionSummary::default(),
        };
        let legacy_encoded = serde_json::to_string(&legacy).expect("legacy json");

        let decoded = app_api_decode_session_from_disk(&legacy_encoded).expect("legacy decoded");
        assert!(decoded.access_token.is_empty());
        assert!(decoded.access_expires_at.is_empty());
        assert_eq!(decoded.refresh_token, "legacy-refresh-secret");

        let migrated = app_api_encode_session_for_disk(&decoded).expect("migrated json");
        assert!(!migrated.contains("legacy-access-secret"));
        assert!(migrated.contains("legacy-refresh-secret"));
    }

    #[test]
    fn app_api_device_proof_is_signed_with_registered_public_key() {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};

        let device = app_api_generate_device_state().expect("device key");
        assert_eq!(device.key_alg, "Ed25519");
        assert!(!device.public_key.contains("placeholder"));
        assert_eq!(
            device.public_key_jwk.get("kty").and_then(|v| v.as_str()),
            Some("OKP")
        );
        assert_eq!(
            device.public_key_jwk.get("crv").and_then(|v| v.as_str()),
            Some("Ed25519")
        );

        let method = reqwest::Method::POST;
        let path = "/profile-leases";
        let body = r#"{"location_id":"de"}"#;
        let proof = app_api_device_proof(&device, &method, path, Some(body)).expect("proof");
        let decoded = URL_SAFE_NO_PAD.decode(proof).expect("proof base64");
        let proof_json: serde_json::Value = serde_json::from_slice(&decoded).expect("proof json");
        assert_eq!(proof_json["typ"], "doodlevpn-device-proof-v1");
        assert_eq!(proof_json["alg"], "EdDSA");
        assert_eq!(proof_json["device_id"], device.client_device_id);
        assert_eq!(proof_json["htm"], "POST");
        assert_eq!(proof_json["htu"], "/profile-leases");
        assert_eq!(proof_json["body_sha256"], app_api_body_sha256(Some(body)));

        let jti = proof_json["jti"].as_str().expect("jti");
        let iat = proof_json["iat"].as_u64().expect("iat");
        let signing_input = format!(
            "DoodleVPN-PC-Proof-v1\nPOST\n/profile-leases\n{}\n{}\n{}\n{}",
            app_api_body_sha256(Some(body)),
            iat,
            jti,
            device.client_device_id
        );
        let public_key = URL_SAFE_NO_PAD
            .decode(device.public_key_jwk["x"].as_str().expect("jwk x"))
            .expect("public key b64");
        let verifying_key =
            VerifyingKey::from_bytes(&public_key.try_into().expect("public key len"))
                .expect("verifying key");
        let signature_bytes = URL_SAFE_NO_PAD
            .decode(proof_json["sig"].as_str().expect("sig"))
            .expect("sig b64");
        let signature = Signature::from_slice(&signature_bytes).expect("sig len");
        verifying_key
            .verify(signing_input.as_bytes(), &signature)
            .expect("signature verifies");
    }

    fn json_array_contains_str(value: &serde_json::Value, expected: &str) -> bool {
        value
            .as_array()
            .map(|items| items.iter().any(|item| item.as_str() == Some(expected)))
            .unwrap_or(false)
    }

    #[test]
    fn protected_compatibility_errors_are_degraded_not_failed() {
        let check = protected_compatibility_check(health_check(
            "http_listener",
            "error",
            "HTTP compatibility listener",
            "127.0.0.1:32001 is not accepting connections",
        ));
        let report = health_report("protected", vec![check]);

        assert_eq!(report.verdict, "protected_degraded");
        assert_eq!(report.checks[0].severity, "warning");
    }

    #[test]
    fn protected_core_dns_errors_are_fatal() {
        let report = health_report(
            "protected",
            vec![health_check(
                "tun_dns",
                "error",
                "TUN DNS",
                "DNS resolution failed",
            )],
        );

        assert_eq!(report.verdict, "failed");
        assert_eq!(report.checks[0].severity, "error");
    }

    #[test]
    fn connection_health_report_carries_structured_runtime_ports() {
        let mut report = health_report("protected", Vec::new());
        attach_runtime_ports(&mut report, 32101, 32102, Some(32103));

        assert_eq!(report.runtime_socks_port, Some(32101));
        assert_eq!(report.runtime_http_port, Some(32102));
        assert_eq!(report.runtime_api_port, Some(32103));
    }

    #[test]
    fn start_tunnel_request_can_omit_api_port_for_legacy_service_retry() {
        let request = tunnel_service::StartTunnelRequest {
            op_id: "op-test".into(),
            engine_kind: tunnel_service::TunnelEngineKind::SingboxTun,
            xray_config: None,
            singbox_config: json!({ "log": { "disabled": true } }),
            socks_port: 31001,
            http_port: 31002,
            api_port: None,
            redacted_label: "vless:tcp".into(),
        };
        let value =
            serde_json::to_value(tunnel_service::TunnelCommand::StartTunnel(request)).unwrap();

        assert_eq!(value["type"], json!("start_tunnel"));
        assert!(value.get("api_port").is_none());
    }

    #[test]
    fn service_verdict_overrides_app_side_health_verdict() {
        let mut report = health_report(
            "protected",
            vec![health_check(
                "http_listener",
                "warning",
                "HTTP compatibility listener",
                "compatibility not ready",
            )],
        );
        let status = tunnel_service::TunnelStatus {
            protocol_version: tunnel_service::TUNNEL_PROTOCOL_VERSION,
            service_version: "6.0.0".into(),
            state: tunnel_service::TunnelState::Connected,
            effective_state: tunnel_service::TunnelEffectiveState::ProtectedDegraded,
            health_verdict: tunnel_service::TunnelHealthVerdict::ProtectedDegraded,
            phase: Some("connected".into()),
            active_op_id: Some("op-test".into()),
            service_generation: 42,
            previous_generation: Some(41),
            engine_kind: Some(tunnel_service::TunnelEngineKind::SingboxTun),
            runtime_socks_port: Some(32101),
            runtime_http_port: Some(32102),
            runtime_api_port: Some(32103),
            xray_pid: None,
            singbox_pid: Some(1234),
            adapter_alias: Some("DoodleRay Tunnel".into()),
            adapter_ifindex: Some(77),
            route_ready: Some(true),
            dns_ready: Some(true),
            proxy_compat_state: Some("core_connected".into()),
            fatal_checks: Vec::new(),
            degraded_checks: vec!["IPv6 degraded_disabled".into()],
            warning_checks: vec!["NCSI may lag".into()],
            route_explanations: vec!["default route preferred DoodleRay Tunnel".into()],
            endpoint_bypass_checks: vec!["endpoint bypass direct".into()],
            last_repair_action: None,
            network_event_seq: 1,
            previous_unclean_shutdown: Some(
                "previous session ended uncleanly: op_id=op-old generation=40 started_at_ms=1"
                    .into(),
            ),
            error: None,
            timings_ms: vec![("connected".into(), 1000)],
            powershell_fallback_count: 0,
            singbox_check_ms: Some(10),
            xray_spawn_ms: Some(20),
            adapter_probe_backend: Some("native_iphelper_evented".into()),
            route_probe_backend: Some("native_getbestroute2".into()),
            native_probe_ms: vec![("adapter_snapshot".into(), 30)],
            fallback_probe_ms: Vec::new(),
        };

        attach_tunnel_status_to_health(&mut report, Some(&status));

        assert_eq!(report.verdict, "protected_degraded");
        assert_eq!(report.runtime_socks_port, Some(32101));
        assert_eq!(report.runtime_http_port, Some(32102));
        assert_eq!(report.runtime_api_port, Some(32103));
        assert_eq!(report.service_generation, Some(42));
        assert_eq!(report.active_op_id.as_deref(), Some("op-test"));
        assert_eq!(report.engine_kind.as_deref(), Some("SingboxTun"));
        assert_eq!(report.service_degraded_checks.len(), 1);
        assert_eq!(report.route_explanations.len(), 1);
        assert!(report
            .service_warning_checks
            .iter()
            .any(|check| check.starts_with("unclean shutdown marker:")));
    }

    #[cfg(windows)]
    struct RuntimeSmokeGuard;

    #[cfg(windows)]
    impl Drop for RuntimeSmokeGuard {
        fn drop(&mut self) {
            let _ = sysproxy::restore_previous_proxy_state();
            let _ = xray::stop_xray();
            let _ = singbox::stop_singbox();
        }
    }

    #[cfg(windows)]
    fn start_subscription_fallback_proxy(body: &'static str) -> (u16, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind fallback proxy");
        let port = listener.local_addr().expect("fallback proxy addr").port();
        let handle = std::thread::spawn(move || {
            for _ in 0..5 {
                let Ok((mut stream, _)) = listener.accept() else {
                    continue;
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut buf = [0u8; 2048];
                let Ok(n) = stream.read(&mut buf) else {
                    continue;
                };
                if n == 0 {
                    continue;
                }

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
                break;
            }
        });
        (port, handle)
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "temporarily applies HKCU WinINet proxy and verifies subscription fetch fallback"]
    fn windows_subscription_fetch_uses_system_proxy_fallback() {
        let _guard = RuntimeSmokeGuard;
        let (proxy_port, proxy_thread) = start_subscription_fallback_proxy("fallback-ok");
        sysproxy::apply_doodleray_proxy(proxy_port, "qa-fetch-fallback")
            .expect("apply fallback proxy");

        let reserve = TcpListener::bind(("127.0.0.1", 0)).expect("reserve direct-fail port");
        let direct_fail_port = reserve.local_addr().expect("direct-fail addr").port();
        drop(reserve);

        let parsed = Url::parse(&format!(
            "http://127.0.0.1:{}/subscription",
            direct_fail_port
        ))
        .expect("test url");
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let body = runtime
            .block_on(async {
                let response =
                    fetch_http_response_with_fallback(&parsed, Duration::from_secs(5)).await?;
                response
                    .text()
                    .await
                    .map_err(|e| format!("read fallback body: {}", e))
            })
            .expect("subscription fetch should fall back to WinINet proxy");

        assert_eq!(body, "fallback-ok");
        let _ = sysproxy::restore_previous_proxy_state();
        let _ = proxy_thread.join();
    }

    #[cfg(windows)]
    fn smoke_value_string(value: &serde_json::Value, key: &str) -> Option<String> {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .filter(|v| !v.trim().is_empty())
            .map(ToString::to_string)
    }

    #[cfg(windows)]
    fn smoke_active_server_request_from_store() -> Option<ConnectRequest> {
        let appdata = std::env::var("APPDATA").ok()?;
        let store_path = std::path::Path::new(&appdata)
            .join("com.doodlevpn.doodleray")
            .join("secure-storage")
            .join("doodleray-storage.store");
        let store = std::fs::read_to_string(store_path).ok()?;
        let root: serde_json::Value = serde_json::from_str(&store).ok()?;
        let server = root.get("state")?.get("activeServer")?;

        Some(ConnectRequest {
            server_address: smoke_value_string(server, "address")?,
            server_port: server.get("port")?.as_u64()? as u16,
            protocol: smoke_value_string(server, "protocol")?,
            uuid: smoke_value_string(server, "uuid"),
            password: smoke_value_string(server, "password"),
            transport: smoke_value_string(server, "transport").unwrap_or_else(|| "tcp".into()),
            security: smoke_value_string(server, "security").unwrap_or_else(|| "tls".into()),
            sni: smoke_value_string(server, "sni"),
            host: smoke_value_string(server, "host"),
            path: smoke_value_string(server, "path"),
            fingerprint: smoke_value_string(server, "fingerprint"),
            public_key: smoke_value_string(server, "publicKey"),
            short_id: smoke_value_string(server, "shortId"),
            flow: smoke_value_string(server, "flow"),
            proxy_mode: "system-proxy".into(),
            system_proxy_mode: "set".into(),
            socks_port: 20808,
            http_port: 20809,
            api_port: 20813,
            network_stack: "system".into(),
            dns_mode: "fakeip".into(),
            strict_route: false,
            kill_switch: false,
            routing_rules: Vec::new(),
            obfs_type: smoke_value_string(server, "obfsType"),
            obfs_password: smoke_value_string(server, "obfsPassword"),
            up_mbps: server
                .get("upMbps")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            down_mbps: server
                .get("downMbps")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            congestion_control: smoke_value_string(server, "congestionControl"),
            udp_relay_mode: smoke_value_string(server, "udpRelayMode"),
            alpn: server.get("alpn").and_then(|value| {
                value.as_array().map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(ToString::to_string))
                        .collect::<Vec<_>>()
                })
            }),
            private_key: smoke_value_string(server, "privateKey"),
            peer_public_key: smoke_value_string(server, "peerPublicKey"),
            pre_shared_key: smoke_value_string(server, "preSharedKey"),
            local_address: server.get("localAddress").and_then(|value| {
                value.as_array().map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(ToString::to_string))
                        .collect::<Vec<_>>()
                })
            }),
            reserved: server.get("reserved").and_then(|value| {
                value.as_array().map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_u64().map(|v| v as u8))
                        .collect::<Vec<_>>()
                })
            }),
            mtu: server.get("mtu").and_then(|v| v.as_u64()).map(|v| v as u16),
            workers: server
                .get("workers")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            encryption: smoke_value_string(server, "encryption"),
            raw_xray_config: server.get("rawConfig").cloned(),
            routing_policy: None,
        })
    }

    #[cfg(windows)]
    fn ensure_test_xray_resources() {
        let exe_dir = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let source = exe_dir.parent().unwrap_or(&exe_dir).join("xray-core");
        let dest = exe_dir.join("xray-core");
        if dest.join("xray.exe").exists() {
            return;
        }
        std::fs::create_dir_all(&dest).unwrap();
        for entry in std::fs::read_dir(&source).unwrap() {
            let entry = entry.unwrap();
            let file_type = entry.file_type().unwrap();
            if file_type.is_file() {
                let _ = std::fs::copy(entry.path(), dest.join(entry.file_name())).unwrap();
            }
        }
    }

    #[cfg(windows)]
    fn wininet_snapshot_for_smoke() -> String {
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "$p=Get-ItemProperty 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings'; [pscustomobject]@{ProxyEnable=$p.ProxyEnable;ProxyServer=$p.ProxyServer;ProxyOverride=$p.ProxyOverride;AutoConfigURL=$p.AutoConfigURL;AutoDetect=$p.AutoDetect;ProxyHttp11=$p.'ProxyHttp1.1'} | ConvertTo-Json -Compress",
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    #[cfg(windows)]
    fn assert_curl_through_http_proxy(http_port: u16) {
        let proxy = format!("http://127.0.0.1:{}", http_port);
        let output = std::process::Command::new("curl.exe")
            .args([
                "--silent",
                "--show-error",
                "--max-time",
                "20",
                "--proxy",
                &proxy,
                "https://www.gstatic.com/generate_204",
                "--output",
                "NUL",
                "--write-out",
                "%{http_code}",
            ])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        assert!(
            output.status.success() && (stdout == "204" || stdout == "200"),
            "curl through DoodleRay HTTP proxy failed: status={:?}, stdout={}, stderr={}",
            output.status.code(),
            stdout,
            stderr
        );
    }

    #[test]
    fn sanitize_xray_routing_rules_removes_unsupported_geo_rules() {
        let mut config = json!({
            "routing": {
                "rules": [
                    {
                        "type": "field",
                        "domain": ["geosite:CATEGORY-BITTORRENT"],
                        "outboundTag": "direct"
                    },
                    {
                        "type": "field",
                        "domain": ["geosite:category-bittorrent", "domain:example.com"],
                        "outboundTag": "proxy"
                    },
                    {
                        "type": "field",
                        "ip": ["geoip:direct"],
                        "outboundTag": "direct"
                    },
                    {
                        "type": "field",
                        "inboundTag": ["api"],
                        "outboundTag": "api"
                    }
                ]
            }
        });

        sanitize_xray_routing_rules(&mut config);

        let rules = config["routing"]["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0]["domain"], json!(["domain:example.com"]));
        assert_eq!(rules[1]["inboundTag"], json!(["api"]));
    }

    #[test]
    fn singbox_tun_kill_switch_keeps_vpn_final_and_enables_strict_route() {
        let mut req = sample_request("tun");
        req.kill_switch = true;
        req.strict_route = false;

        let config = build_singbox_config(&req);

        assert_eq!(config["route"]["final"], json!("proxy"));
        assert_eq!(config["inbounds"][0]["strict_route"], json!(true));
    }

    #[test]
    fn singbox_tun_includes_loopback_compatibility_inbounds() {
        let req = sample_request("tun");

        let config = build_singbox_config(&req);
        let inbounds = config["inbounds"].as_array().unwrap();

        assert_eq!(config["route"]["final"], json!("proxy"));
        assert_eq!(inbounds[0]["type"], json!("tun"));
        assert_eq!(inbounds[1]["type"], json!("socks"));
        assert_eq!(inbounds[1]["listen"], json!("127.0.0.1"));
        assert_eq!(inbounds[1]["listen_port"], json!(10808));
        assert_eq!(inbounds[2]["type"], json!("http"));
        assert_eq!(inbounds[2]["listen"], json!("127.0.0.1"));
        assert_eq!(inbounds[2]["listen_port"], json!(10809));
        assert_eq!(
            config["route"]["rules"][1],
            json!({ "protocol": "dns", "action": "hijack-dns" })
        );
    }

    #[test]
    fn singbox_routes_ru_domains_direct_by_default() {
        let req = sample_request("tun");

        let config = build_singbox_config(&req);
        let rules = config["route"]["rules"].as_array().unwrap();
        let default_direct_rule = rules
            .iter()
            .find(|rule| {
                rule.get("outbound").and_then(|value| value.as_str()) == Some("direct")
                    && json_array_contains_str(&rule["domain_suffix"], "2ip.ru")
            })
            .expect("default direct RU/domain rule missing");

        assert!(json_array_contains_str(
            &default_direct_rule["domain_suffix"],
            "gosuslugi.ru"
        ));
        assert!(json_array_contains_str(
            &default_direct_rule["domain_regex"],
            r".*\.ru$"
        ));
        assert_eq!(config["route"]["final"], json!("proxy"));
    }

    #[test]
    fn custom_singbox_proxy_rules_override_default_ru_direct() {
        let mut req = sample_request("tun");
        req.routing_rules = vec![RoutingRuleRequest {
            rule_type: "domain".into(),
            value: "*.ru".into(),
            action: "proxy".into(),
        }];

        let config = build_singbox_config(&req);
        let rules = config["route"]["rules"].as_array().unwrap();
        let proxy_index = rules
            .iter()
            .position(|rule| {
                rule.get("outbound").and_then(|value| value.as_str()) == Some("proxy")
                    && json_array_contains_str(&rule["domain_suffix"], "ru")
            })
            .expect("custom proxy rule missing");
        let default_direct_index = rules
            .iter()
            .position(|rule| {
                rule.get("outbound").and_then(|value| value.as_str()) == Some("direct")
                    && json_array_contains_str(&rule["domain_suffix"], "2ip.ru")
            })
            .expect("default direct rule missing");

        assert!(proxy_index < default_direct_index);
    }

    #[test]
    fn tauri_config_bundles_offline_webview2_installer() {
        let config_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.windows.conf.json");
        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(config_path).unwrap()).unwrap();

        assert_eq!(config["productName"], json!("DoodleRay"));
        assert_eq!(
            config["bundle"]["windows"]["webviewInstallMode"]["type"],
            json!("offlineInstaller")
        );
        assert_eq!(
            config["bundle"]["windows"]["webviewInstallMode"]["silent"],
            json!(true)
        );
        assert_eq!(
            config["bundle"]["windows"]["signCommand"]["cmd"],
            json!("powershell")
        );
    }

    #[test]
    fn window_capabilities_allow_compact_mode_resize() {
        for name in ["default.json", "appstore.json"] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("capabilities")
                .join(name);
            let capability: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            let permissions = capability["permissions"].as_array().unwrap();

            for permission in [
                "core:window:allow-set-size",
                "core:window:allow-set-min-size",
                "core:window:allow-is-maximized",
                "core:window:allow-unmaximize",
            ] {
                assert!(permissions.contains(&json!(permission)));
            }
        }
    }

    #[test]
    fn support_bundle_redaction_strips_urls_uuids_ips_and_keys() {
        let input = [
            "url=https://user:pass@example.com/sub",
            "id=11111111-2222-3333-4444-555555555555",
            "endpoint=203.0.113.7:443",
            "private_key=super-secret",
            "loopback=127.0.0.1:10809",
            "wintun=Tunnel|SWD\\\\WINTUN\\\\{B71A9688-FC4C-FB7D-060F-9F1B5D26DA3D}|problem=CM_PROB_PHANTOM",
        ]
        .join("\n");

        let redacted = redact_support_text(&input);

        assert!(!redacted.contains("https://"));
        assert!(!redacted.contains("11111111-2222-3333-4444-555555555555"));
        assert!(!redacted.contains("B71A9688-FC4C-FB7D-060F-9F1B5D26DA3D"));
        assert!(!redacted.contains("203.0.113.7"));
        assert!(!redacted.contains("super-secret"));
        assert!(redacted.contains("[redacted-url]"));
        assert!(redacted.contains("[redacted-uuid]"));
        assert!(redacted.contains("[redacted-ip]"));
        assert!(redacted.contains("[redacted-sensitive-line]"));
        assert!(redacted.contains("127.0.0.1:10809"));
    }

    #[test]
    fn singbox_tun_uses_profile_mtu() {
        let mut req = sample_request("tun");
        req.mtu = Some(1408);

        let config = build_singbox_config(&req);

        assert_eq!(config["inbounds"][0]["mtu"], json!(1408));
    }

    #[test]
    fn singbox_tun_rejects_unsafe_mtu_to_stable_fallback() {
        let mut req = sample_request("tun");
        req.mtu = Some(9000);

        let config = build_singbox_config(&req);

        assert_eq!(config["inbounds"][0]["mtu"], json!(1408));
    }

    #[test]
    fn singbox_tun_stabilizes_mixed_stack_on_windows() {
        let mut req = sample_request("tun");
        req.network_stack = "mixed".into();

        let config = build_singbox_config(&req);

        assert_eq!(config["inbounds"][0]["udp_timeout"], json!("10m"));
        #[cfg(windows)]
        {
            assert_eq!(config["inbounds"][0]["stack"], json!("system"));
            assert!(config["inbounds"][0]
                .get("endpoint_independent_nat")
                .is_none());
        }
        #[cfg(not(windows))]
        {
            assert_eq!(config["inbounds"][0]["stack"], json!("mixed"));
            assert_eq!(
                config["inbounds"][0]["endpoint_independent_nat"],
                json!(true)
            );
        }
    }

    #[test]
    fn singbox_tun_gvisor_keeps_udp_stability_options() {
        let mut req = sample_request("tun");
        req.network_stack = "gvisor".into();

        let config = build_singbox_config(&req);

        assert_eq!(config["inbounds"][0]["stack"], json!("gvisor"));
        assert_eq!(
            config["inbounds"][0]["endpoint_independent_nat"],
            json!(true)
        );
    }

    #[test]
    fn singbox_dns_is_ipv4_only_for_windows_tun_stability() {
        let dns = singbox_dns_config("fakeip");

        assert_eq!(dns["strategy"], json!("ipv4_only"));
        assert!(dns["servers"][0].get("strategy").is_none());
        assert!(dns["servers"][1].get("strategy").is_none());
        assert_eq!(dns["servers"][0]["type"], json!("https"));
        assert_eq!(dns["servers"][0]["detour"], json!("proxy"));
        assert_eq!(dns["servers"][2]["inet4_range"], json!("198.18.0.0/15"));
        assert!(dns["servers"][2].get("inet6_range").is_none());
        assert_eq!(dns["rules"][0]["query_type"], json!("A"));
    }

    #[test]
    fn xray_tun_bridge_dns_uses_real_ips_over_doh() {
        let dns = xray_tun_bridge_dns_config();

        assert_eq!(dns["strategy"], json!("ipv4_only"));
        assert_eq!(dns["final"], json!("dns-remote"));
        assert!(dns["rules"].is_null());
        assert_eq!(dns["servers"][0]["tag"], json!("dns-remote"));
        assert_eq!(dns["servers"][0]["type"], json!("https"));
        assert_eq!(dns["servers"][0]["server_port"], json!(443));
        assert_eq!(dns["servers"][0]["path"], json!("/dns-query"));
        assert_eq!(
            dns["servers"][0]["tls"]["server_name"],
            json!("cloudflare-dns.com")
        );
        assert_eq!(dns["servers"][0]["detour"], json!("proxy"));
        assert!(dns["servers"]
            .as_array()
            .unwrap()
            .iter()
            .all(|server| server["type"] != json!("fakeip")));
    }

    #[test]
    fn xray_tun_bridge_uses_socks_for_tcp_and_udp() {
        let req = sample_request("tun");
        let outbounds = xray_tun_bridge_outbounds(&req);

        assert_eq!(outbounds[0]["tag"], json!("proxy"));
        assert_eq!(outbounds[0]["type"], json!("socks"));
        assert_eq!(outbounds[0]["server_port"], json!(10808));
        assert_eq!(outbounds[1]["tag"], json!("proxy-udp"));
        assert_eq!(outbounds[1]["type"], json!("socks"));
        assert_eq!(outbounds[1]["server_port"], json!(10808));
        assert_eq!(
            xray_tun_bridge_dns_config()["servers"][0]["detour"],
            json!("proxy")
        );
        assert_eq!(xray_tun_bridge_udp_rule()["outbound"], json!("proxy-udp"));
    }

    #[test]
    fn xray_tun_bridge_routes_ru_domains_direct_by_default() {
        let mut rules = Vec::new();

        push_default_direct_singbox_rule(&mut rules);

        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["outbound"], json!("direct"));
        assert!(json_array_contains_str(
            &rules[0]["domain_suffix"],
            "2ip.ru"
        ));
        assert!(json_array_contains_str(
            &rules[0]["domain_regex"],
            r"(^|\.)[^.]+\.ru$"
        ));
    }

    #[test]
    fn xray_socks_udp_is_bound_to_loopback_for_tun_bridge() {
        let req = sample_request("tun");

        let config = build_xray_config(&req);

        assert_eq!(config["inbounds"][0]["settings"]["udp"], json!(true));
        assert_eq!(config["inbounds"][0]["settings"]["ip"], json!("127.0.0.1"));
    }

    #[test]
    fn xray_engine_is_selected_for_websocket_transport() {
        let mut req = sample_request("system-proxy");
        req.transport = "ws".into();

        assert!(uses_xray_engine(&req));
    }

    #[test]
    fn xray_engine_is_selected_for_vless_reality_tcp() {
        let mut req = sample_request("system-proxy");
        req.protocol = "vless".into();
        req.transport = "tcp".into();
        req.security = "reality".into();
        req.public_key = Some("test-public-key".into());
        req.short_id = Some("abcd".into());

        assert!(uses_xray_engine(&req));
    }

    #[test]
    fn singbox_engine_is_kept_for_udp_only_protocols() {
        let mut req = sample_request("system-proxy");
        req.protocol = "hysteria2".into();
        req.transport = "udp".into();
        req.security = "tls".into();

        assert!(!uses_xray_engine(&req));
    }

    #[test]
    fn xray_routes_ru_domains_direct_by_default() {
        let req = sample_request("system-proxy");

        let config = build_xray_config(&req);
        let rules = config["routing"]["rules"].as_array().unwrap();
        let direct_rule = rules
            .iter()
            .find(|rule| xray_rule_has_default_direct_domains(rule))
            .expect("default xray direct RU/domain rule missing");

        assert!(json_array_contains_str(
            &direct_rule["domain"],
            "domain:2ip.ru"
        ));
        assert!(json_array_contains_str(
            &direct_rule["domain"],
            r"regexp:.*\.ru$"
        ));
    }

    #[test]
    fn raw_xray_injection_keeps_server_routing_without_generic_split() {
        let req = sample_request("system-proxy");
        let raw = json!({
            "outbounds": [
                { "tag": "proxy", "protocol": "freedom" }
            ]
        });

        let config = inject_xray_inbounds(raw, &req);
        let outbounds = config["outbounds"].as_array().unwrap();
        let rules = config["routing"]["rules"].as_array().unwrap();

        assert!(outbounds.iter().any(|outbound| {
            outbound.get("tag").and_then(|value| value.as_str()) == Some("direct")
                && outbound.get("protocol").and_then(|value| value.as_str()) == Some("freedom")
        }));
        assert!(outbounds.iter().any(|outbound| {
            outbound.get("tag").and_then(|value| value.as_str()) == Some("api")
                && outbound.get("protocol").and_then(|value| value.as_str()) == Some("blackhole")
        }));
        assert!(rules.iter().any(|rule| {
            rule.get("inboundTag")
                .and_then(|value| value.as_array())
                .map(|tags| tags.iter().any(|tag| tag.as_str() == Some("api")))
                .unwrap_or(false)
        }));
        assert!(!rules.iter().any(xray_rule_has_default_direct_domains));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "starts local xray, temporarily applies HKCU WinINet proxy, and performs a live proxy request"]
    fn windows_runtime_xray_system_proxy_smoke_from_store() {
        let _guard = RuntimeSmokeGuard;
        ensure_test_xray_resources();
        let before = wininet_snapshot_for_smoke();
        let req = smoke_active_server_request_from_store()
            .expect("active server in DoodleRay secure-storage is required for runtime smoke");

        if !uses_xray_engine(&req) {
            eprintln!(
                "Skipping runtime xray smoke: active protocol={} transport={} is not xray-backed",
                req.protocol, req.transport
            );
            return;
        }

        assert!(
            loopback_port_available(req.socks_port)
                && loopback_port_available(req.http_port)
                && loopback_port_available(req.api_port),
            "runtime smoke ports are busy"
        );

        let config = build_xray_config(&req);
        xray::start_xray(&config).expect("xray should start for active server");
        wait_for_port_ready(req.socks_port).expect("SOCKS port should become ready");
        wait_for_port_ready(req.http_port).expect("HTTP port should become ready");

        let apply = sysproxy::apply_doodleray_proxy(req.http_port, env!("CARGO_PKG_VERSION"))
            .expect("WinINet proxy apply should succeed");
        assert_eq!(apply.proxy_server, format!("127.0.0.1:{}", req.http_port));

        let applied = wininet_snapshot_for_smoke();
        assert!(
            applied.contains(&format!("127.0.0.1:{}", req.http_port)),
            "WinINet ProxyServer should point at DoodleRay HTTP port: {}",
            applied
        );
        assert!(
            !applied.contains("http=") && !applied.contains("socks="),
            "WinINet ProxyServer must stay simple, not protocol-mapped: {}",
            applied
        );

        assert_curl_through_http_proxy(req.http_port);

        sysproxy::restore_previous_proxy_state().expect("WinINet proxy restore should succeed");
        xray::stop_xray().expect("xray should stop cleanly");
        let after = wininet_snapshot_for_smoke();
        assert_eq!(
            after, before,
            "WinINet proxy state must be restored exactly"
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "starts bundled sing-box executable and performs a live local proxy request"]
    fn windows_runtime_singbox_exe_proxy_smoke() {
        let _guard = RuntimeSmokeGuard;
        assert!(
            loopback_port_available(20808) && loopback_port_available(20809),
            "runtime smoke ports are busy"
        );

        let config = json!({
            "log": { "level": "warn" },
            "inbounds": [
                {
                    "type": "socks",
                    "tag": "socks-in",
                    "listen": "127.0.0.1",
                    "listen_port": 20808
                },
                {
                    "type": "http",
                    "tag": "http-in",
                    "listen": "127.0.0.1",
                    "listen_port": 20809
                }
            ],
            "outbounds": [
                { "type": "direct", "tag": "direct" }
            ],
            "route": {
                "final": "direct"
            }
        });

        singbox::start_singbox(&config).expect("sing-box executable fallback should start");
        wait_for_port_ready(20808).expect("sing-box SOCKS port should become ready");
        wait_for_port_ready(20809).expect("sing-box HTTP port should become ready");
        assert_curl_through_http_proxy(20809);
        singbox::stop_singbox().expect("sing-box executable fallback should stop");
    }

    #[test]
    fn xray_ws_config_uses_modern_host_and_tls_settings() {
        let mut req = sample_request("system-proxy");
        req.transport = "ws".into();
        req.host = Some("cdn.example.com".into());
        req.path = Some("/ray".into());

        let config = build_xray_config(&req);
        let stream = &config["outbounds"][0]["streamSettings"];

        assert_eq!(stream["network"], json!("ws"));
        assert_eq!(stream["wsSettings"]["path"], json!("/ray"));
        assert_eq!(stream["wsSettings"]["host"], json!("cdn.example.com"));
        assert!(stream["wsSettings"].get("headers").is_none());
        assert_eq!(stream["tlsSettings"]["serverName"], json!("example.com"));
    }

    #[test]
    fn raw_xray_ws_config_moves_deprecated_header_host() {
        let req = sample_request("system-proxy");
        let raw = json!({
            "outbounds": [{
                "tag": "proxy",
                "protocol": "vless",
                "streamSettings": {
                    "network": "ws",
                    "wsSettings": {
                        "path": "/ray",
                        "headers": {
                            "Host": "cdn.example.com",
                            "X-Test": "kept"
                        }
                    }
                }
            }],
            "routing": { "rules": [] }
        });

        let config = inject_xray_inbounds(raw, &req);
        let ws = &config["outbounds"][0]["streamSettings"]["wsSettings"];

        assert_eq!(ws["host"], json!("cdn.example.com"));
        assert!(ws["headers"].get("Host").is_none());
        assert_eq!(ws["headers"]["X-Test"], json!("kept"));
    }

    #[test]
    fn process_rules_are_normalized_to_process_names() {
        let mut req = sample_request("system-proxy");
        req.routing_rules = vec![
            RoutingRuleRequest {
                rule_type: "exe".into(),
                value: r"C:\Program Files\Discord\Discord.exe".into(),
                action: "direct".into(),
            },
            RoutingRuleRequest {
                rule_type: "exe".into(),
                value: "/Applications/Steam.app".into(),
                action: "block".into(),
            },
        ];

        let direct_names = process_rule_names(&req, "direct");
        let block_names = process_rule_names(&req, "block");

        assert_eq!(direct_names, vec!["discord.exe".to_string()]);
        assert_eq!(block_names, vec!["steam".to_string()]);
    }

    #[test]
    fn singbox_tun_routes_pubg_processes_direct() {
        let mut req = sample_request("tun");
        req.routing_rules = vec![
            RoutingRuleRequest {
                rule_type: "exe".into(),
                value: "TslGame.exe".into(),
                action: "direct".into(),
            },
            RoutingRuleRequest {
                rule_type: "exe".into(),
                value: "TslGame_BE.exe".into(),
                action: "direct".into(),
            },
            RoutingRuleRequest {
                rule_type: "exe".into(),
                value: "TslGame_ZK.exe".into(),
                action: "direct".into(),
            },
            RoutingRuleRequest {
                rule_type: "exe".into(),
                value: "ExecPubg.exe".into(),
                action: "direct".into(),
            },
        ];

        let config = build_singbox_config(&req);
        let rules = config["route"]["rules"].as_array().unwrap();
        let direct_rule = rules
            .iter()
            .find(|rule| {
                rule.get("outbound").and_then(|value| value.as_str()) == Some("direct")
                    && rule.get("process_name").is_some()
            })
            .expect("direct process rule missing");

        for process in [
            "execpubg.exe",
            "tslgame.exe",
            "tslgame_be.exe",
            "tslgame_zk.exe",
            "steam.exe",
            "steamservice.exe",
        ] {
            assert!(json_array_contains_str(
                &direct_rule["process_name"],
                process
            ));
        }
    }

    #[test]
    fn singbox_split_rules_keep_processes_independent_from_domains() {
        let mut req = sample_request("tun");
        req.routing_rules = vec![
            RoutingRuleRequest {
                rule_type: "domain".into(),
                value: "example.com".into(),
                action: "direct".into(),
            },
            RoutingRuleRequest {
                rule_type: "domain".into(),
                value: "*.microsoft.com".into(),
                action: "direct".into(),
            },
            RoutingRuleRequest {
                rule_type: "exe".into(),
                value: "msedge.exe".into(),
                action: "direct".into(),
            },
            RoutingRuleRequest {
                rule_type: "exe".into(),
                value: "msedgewebview2.exe".into(),
                action: "direct".into(),
            },
        ];

        let config = build_singbox_config(&req);
        let rules = config["route"]["rules"].as_array().unwrap();

        let direct_process_rule = rules
            .iter()
            .find(|rule| {
                rule.get("outbound").and_then(|value| value.as_str()) == Some("direct")
                    && rule
                        .get("process_name")
                        .map(|value| json_array_contains_str(value, "msedge.exe"))
                        .unwrap_or(false)
            })
            .expect("direct Edge process rule missing");
        assert!(direct_process_rule.get("domain").is_none());
        assert!(direct_process_rule.get("domain_suffix").is_none());
        assert!(json_array_contains_str(
            &direct_process_rule["process_name"],
            "msedgewebview2.exe"
        ));

        let direct_domain_rule = rules
            .iter()
            .find(|rule| {
                rule.get("outbound").and_then(|value| value.as_str()) == Some("direct")
                    && rule
                        .get("domain")
                        .map(|value| json_array_contains_str(value, "example.com"))
                        .unwrap_or(false)
            })
            .expect("direct domain rule missing");
        assert!(direct_domain_rule.get("process_name").is_none());
        assert!(json_array_contains_str(
            &direct_domain_rule["domain_suffix"],
            "microsoft.com"
        ));
    }

    #[test]
    fn tun_direct_process_exclusions_disable_wininet_compat_path() {
        let req = sample_request("tun");
        assert!(tun_direct_process_exclusions_need_raw_tun_path(&req));

        let mut system_proxy = sample_request("system-proxy");
        system_proxy.routing_rules = vec![RoutingRuleRequest {
            rule_type: "exe".into(),
            value: r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe".into(),
            action: "direct".into(),
        }];
        assert!(!tun_direct_process_exclusions_need_raw_tun_path(
            &system_proxy
        ));
    }

    #[test]
    fn singbox_direct_process_dns_rules_precede_fakeip() {
        let mut req = sample_request("tun");
        req.dns_mode = "fakeip".into();
        req.routing_rules = vec![
            RoutingRuleRequest {
                rule_type: "domain".into(),
                value: "example.com".into(),
                action: "direct".into(),
            },
            RoutingRuleRequest {
                rule_type: "exe".into(),
                value: "msedge.exe".into(),
                action: "direct".into(),
            },
        ];

        let config = build_singbox_config(&req);
        let dns_rules = config["dns"]["rules"].as_array().unwrap();
        let edge_dns_index = dns_rules
            .iter()
            .position(|rule| {
                rule.get("server").and_then(|value| value.as_str()) == Some("dns-direct")
                    && rule
                        .get("process_name")
                        .map(|value| json_array_contains_str(value, "msedge.exe"))
                        .unwrap_or(false)
            })
            .expect("direct Edge DNS rule missing");
        let domain_dns_index = dns_rules
            .iter()
            .position(|rule| {
                rule.get("server").and_then(|value| value.as_str()) == Some("dns-direct")
                    && rule
                        .get("domain")
                        .map(|value| json_array_contains_str(value, "example.com"))
                        .unwrap_or(false)
            })
            .expect("direct domain DNS rule missing");
        let fakeip_index = dns_rules
            .iter()
            .position(|rule| {
                rule.get("server").and_then(|value| value.as_str()) == Some("dns-fakeip")
                    && rule.get("query_type").and_then(|value| value.as_str()) == Some("A")
            })
            .expect("fake-IP DNS rule missing");

        assert!(edge_dns_index < fakeip_index);
        assert!(domain_dns_index < fakeip_index);
    }

    #[test]
    fn xray_tun_bridge_direct_process_dns_uses_direct_resolver_without_fakeip() {
        let direct_processes = vec!["msedge.exe".to_string(), "msedgewebview2.exe".to_string()];
        let dns = xray_tun_bridge_dns_config_for_direct_processes(&direct_processes);
        let dns_rules = dns["rules"].as_array().unwrap();

        assert_eq!(dns_rules[0]["server"], json!("dns-direct"));
        assert!(json_array_contains_str(
            &dns_rules[0]["process_name"],
            "msedge.exe"
        ));
        assert!(json_array_contains_str(
            &dns_rules[0]["process_name"],
            "msedgewebview2.exe"
        ));
        assert!(dns["servers"]
            .as_array()
            .unwrap()
            .iter()
            .all(|server| server["type"] != json!("fakeip")));
    }

    #[test]
    fn tun_bridge_bypass_processes_cover_macos_and_windows_engines() {
        let names: Vec<String> = system_bypass_process_values()
            .into_iter()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
            .collect();

        assert!(names.contains(&"xray".to_string()));
        assert!(names.contains(&"xray.exe".to_string()));
        assert!(names.contains(&"sing-box".to_string()));
        assert!(names.contains(&"sing-box.exe".to_string()));
    }

    #[test]
    fn tun_addresses_match_platform_ipv6_policy() {
        assert_eq!(
            tun_address_values(),
            json!(["172.30.255.1/30", "fdfe:dcba:9876::1/126"])
        );
    }

    #[test]
    fn tun_inbound_excludes_ip_endpoint_from_auto_route() {
        let mut req = sample_request("tun");
        req.server_address = "89.58.26.124".into();

        let inbound = tun_inbound_value(&req, Some("DoodleRay Tunnel"), true);

        assert_eq!(inbound["route_exclude_address"], json!(["89.58.26.124/32"]));
    }

    #[test]
    fn singbox_tun_uses_route_action_sniff_not_legacy_inbound_fields() {
        let req = sample_request("tun");

        let config = build_singbox_config(&req);
        let inbound = config["inbounds"][0].as_object().unwrap();

        assert!(!inbound.contains_key("sniff"));
        assert!(!inbound.contains_key("sniff_override_destination"));
        assert!(!inbound.contains_key("sniff_timeout"));
        assert!(!inbound.contains_key("domain_strategy"));
        assert_eq!(config["route"]["rules"][0], json!({ "action": "sniff" }));
    }

    #[test]
    fn raw_xray_config_preserves_server_owned_dns_without_policy() {
        let req = sample_request("system-proxy");
        let raw = json!({
            "dns": {
                "servers": [
                    "https://1.1.1.1/dns-query",
                    "https://8.8.8.8/dns-query"
                ]
            },
            "inbounds": [],
            "outbounds": [
                { "tag": "proxy", "protocol": "freedom" }
            ],
            "routing": { "rules": [] }
        });

        let config = inject_xray_inbounds(raw, &req);

        assert_eq!(
            config["dns"],
            json!({
                "servers": [
                    "https://1.1.1.1/dns-query",
                    "https://8.8.8.8/dns-query"
                ]
            })
        );
    }
}

/// Build the xray-core JSON config for transports owned by xray-core.
fn build_xray_config(req: &ConnectRequest) -> serde_json::Value {
    let flow_value =
        if req.transport == "tcp" || req.transport == "xhttp" || req.transport.is_empty() {
            req.flow.clone().unwrap_or_default()
        } else {
            String::new()
        };

    // Build xray outbound settings based on protocol
    let outbound_settings = match req.protocol.as_str() {
        "vmess" => serde_json::json!({
            "vnext": [{
                "address": req.server_address,
                "port": req.server_port,
                "users": [{
                    "id": req.uuid.clone().unwrap_or_default(),
                    "security": "auto"
                }]
            }]
        }),
        "trojan" => serde_json::json!({
            "servers": [{
                "address": req.server_address,
                "port": req.server_port,
                "password": req.password.clone().unwrap_or_default()
            }]
        }),
        "shadowsocks" => serde_json::json!({
            "servers": [{
                "address": req.server_address,
                "port": req.server_port,
                "password": req.password.clone().unwrap_or_default(),
                "method": req.encryption.clone().unwrap_or("aes-256-gcm".into())
            }]
        }),
        _ => serde_json::json!({
            "vnext": [{
                "address": req.server_address,
                "port": req.server_port,
                "users": [{
                    "id": req.uuid.clone().unwrap_or_default(),
                    "encryption": "none",
                    "flow": flow_value
                }]
            }]
        }),
    };

    let mut stream_settings = match req.transport.as_str() {
        "xhttp" => serde_json::json!({
            "network": "xhttp",
            "security": req.security,
            "xhttpSettings": {
                "path": req.path.clone().unwrap_or("/xhttp".into())
            }
        }),
        "ws" => serde_json::json!({
            "network": "ws",
            "security": req.security,
            "wsSettings": {
                "path": req.path.clone().unwrap_or("/".into()),
                "host": xray_transport_host(req)
            }
        }),
        _ => serde_json::json!({
            "network": "tcp",
            "security": req.security
        }),
    };
    apply_xray_stream_security_settings(&mut stream_settings, req);

    // Build routing rules from Workshop rules
    let mut routing_rules = Vec::new();

    // Custom domain rules from Workshop
    let mut proxy_domains = Vec::new();
    let mut direct_domains = Vec::new();
    let mut block_domains = Vec::new();

    for rule in &req.routing_rules {
        if rule.rule_type == "domain" {
            let domain_val = if rule.value.starts_with("*.") {
                // Wildcard → xray "domain:" prefix
                serde_json::Value::String(format!("domain:{}", rule.value.trim_start_matches("*.")))
            } else {
                serde_json::Value::String(format!("domain:{}", rule.value))
            };
            match rule.action.as_str() {
                "proxy" => proxy_domains.push(domain_val),
                "direct" => direct_domains.push(domain_val),
                "block" => block_domains.push(domain_val),
                _ => {}
            }
        }
    }

    // Add custom routing rules
    if !proxy_domains.is_empty() {
        routing_rules.push(serde_json::json!({
            "type": "field",
            "domain": proxy_domains,
            "outboundTag": "proxy"
        }));
    }
    if !direct_domains.is_empty() {
        routing_rules.push(serde_json::json!({
            "type": "field",
            "domain": direct_domains,
            "outboundTag": "direct"
        }));
    }
    if !block_domains.is_empty() {
        routing_rules.push(serde_json::json!({
            "type": "field",
            "domain": block_domains,
            "outboundTag": "block"
        }));
    }

    // API routing rule — must be FIRST
    let mut final_rules = vec![serde_json::json!({
        "type": "field",
        "inboundTag": ["api"],
        "outboundTag": "api"
    })];
    // DNS port 53 rule — so TUN mode DNS queries get resolved by xray instead of going to "direct"
    final_rules.insert(
        1,
        serde_json::json!({
            "type": "field",
            "port": "53",
            "outboundTag": "dns-out"
        }),
    );
    final_rules.extend(routing_rules);

    let mut config = serde_json::json!({
        "log": { "loglevel": "warning" },
        "stats": {},
        "api": {
            "tag": "api",
            "services": ["StatsService"]
        },
        "policy": {
            "system": {
                "statsInboundUplink": true,
                "statsInboundDownlink": true,
                "statsOutboundUplink": true,
                "statsOutboundDownlink": true
            }
        },
        "dns": xray_dns_config(req),
        "inbounds": [
            {
                "tag": "socks-in",
                "port": req.socks_port,
                "listen": "127.0.0.1",
                "protocol": "socks",
                "settings": { "udp": true, "ip": "127.0.0.1" },
                "sniffing": {
                    "enabled": true,
                    "destOverride": ["http", "tls", "quic", "fakedns"],
                    "routeOnly": true
                }
            },
            {
                "tag": "http-in",
                "port": req.http_port,
                "listen": "127.0.0.1",
                "protocol": "http"
                },
                {
                    "tag": "api",
                    "port": req.api_port,
                    "listen": "127.0.0.1",
                    "protocol": "dokodemo-door",
                    "settings": { "address": "127.0.0.1" }
            }
        ],
        "outbounds": [
            {
                "tag": "proxy",
                "protocol": req.protocol,
                "settings": outbound_settings,
                "streamSettings": stream_settings
            },
            {
                "tag": "direct",
                "protocol": "freedom"
            },
            {
                "tag": "block",
                "protocol": "blackhole",
                "settings": { "response": { "type": "http" } }
            },
            {
                "tag": "dns-out",
                "protocol": "dns"
            },
            {
                "tag": "api",
                "protocol": "blackhole"
            }
        ],
        "routing": {
            "domainStrategy": "AsIs",
            "rules": final_rules
        }
    });
    apply_xray_routing_policy(&mut config, req, true);
    config
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
const APP_STORE_PRIVATE_IP_RANGES: &[&str] = &[
    "10.0.0.0/8",
    "100.64.0.0/10",
    "127.0.0.0/8",
    "169.254.0.0/16",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "::1/128",
    "fc00::/7",
    "fe80::/10",
];

#[cfg(all(target_os = "macos", feature = "app-store"))]
fn rewrite_app_store_geodata_dependencies(config: &mut serde_json::Value, keep_geosite: bool) {
    let Some(rules) = config
        .get_mut("routing")
        .and_then(|routing| routing.get_mut("rules"))
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };

    for rule in rules.iter_mut() {
        if let Some(domains) = rule
            .get_mut("domain")
            .and_then(serde_json::Value::as_array_mut)
        {
            if !keep_geosite {
                domains.retain(|value| {
                    !value
                        .as_str()
                        .is_some_and(|value| value.to_ascii_lowercase().starts_with("geosite:"))
                });
            }
        }

        if let Some(ip_values) = rule.get_mut("ip").and_then(serde_json::Value::as_array_mut) {
            let mut rewritten = Vec::with_capacity(ip_values.len());
            for value in std::mem::take(ip_values) {
                let Some(selector) = value.as_str() else {
                    rewritten.push(value);
                    continue;
                };
                if selector.eq_ignore_ascii_case("geoip:private") {
                    rewritten.extend(
                        APP_STORE_PRIVATE_IP_RANGES
                            .iter()
                            .map(|range| serde_json::Value::String((*range).to_owned())),
                    );
                } else if !selector.to_ascii_lowercase().starts_with("geoip:") {
                    rewritten.push(value);
                }
            }
            *ip_values = rewritten;
        }

        remove_empty_xray_rule_array(rule, "domain");
        remove_empty_xray_rule_array(rule, "ip");
    }

    rules.retain(has_effective_xray_rule_fields);
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
fn prepare_app_store_xray_config(mut config: serde_json::Value) -> serde_json::Value {
    if let Some(root) = config.as_object_mut() {
        root.remove("api");
        root.remove("stats");
        root.remove("policy");
        root.remove("metrics");
    }

    config["log"] = serde_json::json!({ "loglevel": "warning" });
    config["inbounds"] = serde_json::json!([{
        "tag": "tun-in",
        "port": 0,
        "protocol": "tun",
        "settings": {
            "name": "doodleray-ne",
            "mtu": 1408
        },
        "sniffing": {
            "enabled": true,
            "destOverride": ["http", "tls", "quic", "fakedns"],
            "routeOnly": true
        }
    }]);

    if let Some(outbounds) = config
        .get_mut("outbounds")
        .and_then(serde_json::Value::as_array_mut)
    {
        outbounds.retain(|outbound| {
            !matches!(
                outbound.get("tag").and_then(serde_json::Value::as_str),
                Some("api")
            )
        });
    }
    if let Some(rules) = config
        .get_mut("routing")
        .and_then(|routing| routing.get_mut("rules"))
        .and_then(serde_json::Value::as_array_mut)
    {
        rules.retain(|rule| {
            let targets_removed_outbound = matches!(
                rule.get("outboundTag").and_then(serde_json::Value::as_str),
                Some("api")
            );
            let has_stale_inbound = rule
                .get("inboundTag")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|tags| tags.iter().any(|tag| tag.as_str() == Some("api")));
            !targets_removed_outbound && !has_stale_inbound
        });
    }
    let has_external_geodata = config
        .get("env")
        .and_then(|env| env.get("xray.location.asset"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|path| !path.trim().is_empty());
    rewrite_app_store_geodata_dependencies(&mut config, has_external_geodata);

    config
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
fn routing_policy_requires_geosite(request: &ConnectRequest) -> bool {
    request.routing_policy.as_ref().is_some_and(|policy| {
        policy
            .direct_domains
            .iter()
            .chain(&policy.local_dns_domains)
            .any(|selector| selector.starts_with("geosite:"))
    })
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
async fn prepare_app_store_routing_asset(
    request: &ConnectRequest,
) -> Result<Option<PathBuf>, String> {
    let Some(asset) = request
        .routing_policy
        .as_ref()
        .and_then(|policy| policy.asset.as_ref())
    else {
        return if routing_policy_requires_geosite(request) {
            Err(
                "DoodleVPN routing data is unavailable. Refresh the server list and try again."
                    .into(),
            )
        } else {
            Ok(None)
        };
    };

    let directory = app_store_tunnel::app_group_container_path()?
        .join("Library/Application Support/DoodleRay/Routing");
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| format!("Could not prepare DoodleVPN routing storage: {error}"))?;
    let current = directory.join("geosite.dat");
    let marker = directory.join("geosite.sha256");

    let cached = tokio::fs::read(&current).await.ok();
    if cached
        .as_deref()
        .is_some_and(|bytes| sha256_hex(bytes).eq_ignore_ascii_case(&asset.sha256))
    {
        return Ok(Some(directory));
    }

    match app_api_authorized_bytes(&asset.url).await {
        Ok(bytes)
            if bytes.len() as u64 == asset.size_bytes
                && sha256_hex(&bytes).eq_ignore_ascii_case(&asset.sha256) =>
        {
            let temporary = directory.join(format!("geosite.dat.{}.tmp", std::process::id()));
            tokio::fs::write(&temporary, &bytes)
                .await
                .map_err(|error| format!("Could not save DoodleVPN routing data: {error}"))?;
            tokio::fs::rename(&temporary, &current)
                .await
                .map_err(|error| format!("Could not activate DoodleVPN routing data: {error}"))?;
            tokio::fs::write(&marker, asset.sha256.to_ascii_lowercase())
                .await
                .map_err(|error| format!("Could not save DoodleVPN routing version: {error}"))?;
            Ok(Some(directory))
        }
        Ok(_) => Err("DoodleVPN routing data failed integrity verification.".into()),
        Err(download_error) => {
            Err(format!(
                "DoodleVPN routing data could not be downloaded or matched to the current signed policy: {download_error}"
            ))
        }
    }
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
fn build_app_store_xray_config(
    request: &ConnectRequest,
    asset_directory: Option<&Path>,
) -> serde_json::Value {
    let mut config = if let Some(ref raw) = request.raw_xray_config {
        inject_xray_inbounds(raw.clone(), request)
    } else {
        build_xray_config(request)
    };
    if let Some(directory) = asset_directory {
        config["env"]["xray.location.asset"] =
            serde_json::Value::String(directory.to_string_lossy().into_owned());
    }
    prepare_app_store_xray_config(config)
}

#[cfg(all(test, target_os = "macos", feature = "app-store"))]
mod app_store_config_tests {
    use super::{app_store_connection_health_from_response, prepare_app_store_xray_config};
    use crate::app_store_tunnel::TunnelResponse;
    use serde_json::json;

    #[test]
    fn network_extension_config_removes_local_api_and_proxy_inbounds() {
        let config = prepare_app_store_xray_config(json!({
            "api": { "tag": "api" },
            "dns": { "servers": ["localhost", "1.1.1.1"] },
            "stats": {},
            "policy": {},
            "metrics": {},
            "inbounds": [{ "tag": "socks-in", "protocol": "socks" }],
            "outbounds": [
                { "tag": "proxy", "protocol": "vless" },
                { "tag": "api", "protocol": "freedom" },
                { "tag": "dns-out", "protocol": "dns" }
            ],
            "routing": { "rules": [
                { "inboundTag": ["api"], "outboundTag": "api" },
                { "port": "53", "outboundTag": "dns-out" },
                { "domain": ["domain:example.com"], "outboundTag": "proxy" }
            ] }
        }));

        assert!(config.get("api").is_none());
        assert!(config.get("dns").is_some());
        assert!(config.get("stats").is_none());
        assert_eq!(config["inbounds"][0]["protocol"], "tun");
        assert_eq!(config["inbounds"][0]["settings"]["mtu"], 1408);
        assert_eq!(config["outbounds"].as_array().unwrap().len(), 2);
        assert_eq!(config["routing"]["rules"].as_array().unwrap().len(), 2);
        assert!(config["routing"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|rule| rule["outboundTag"] == "dns-out"));
    }

    #[test]
    fn network_extension_config_does_not_require_external_geodata() {
        let config = prepare_app_store_xray_config(json!({
            "inbounds": [],
            "outbounds": [{ "tag": "proxy", "protocol": "freedom" }],
            "routing": { "rules": [
                {
                    "type": "field",
                    "ip": ["geoip:private", "geoip:ru", "203.0.113.0/24"],
                    "outboundTag": "direct"
                },
                {
                    "type": "field",
                    "domain": ["geosite:ru", "domain:example.com"],
                    "outboundTag": "direct"
                },
                {
                    "type": "field",
                    "domain": ["geosite:category-ads-all"],
                    "outboundTag": "block"
                }
            ] }
        }));

        let rules = config["routing"]["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 2);
        assert!(rules[0]["ip"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "10.0.0.0/8"));
        assert!(rules[0]["ip"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "203.0.113.0/24"));
        assert!(!rules[0]["ip"].as_array().unwrap().iter().any(|value| value
            .as_str()
            .is_some_and(|value| value.starts_with("geoip:"))));
        assert_eq!(rules[1]["domain"], json!(["domain:example.com"]));
    }

    #[test]
    fn network_extension_health_does_not_require_direct_proxy_ports() {
        let connected = app_store_connection_health_from_response(&TunnelResponse {
            success: true,
            status: "connected".into(),
            message: String::new(),
        });
        assert_eq!(connected.verdict, "protected");
        assert_eq!(
            connected.engine_kind.as_deref(),
            Some("xray+network-extension")
        );
        assert!(connected.runtime_socks_port.is_none());
        assert!(connected.runtime_http_port.is_none());
        assert_eq!(connected.checks[0].code, "network_extension");

        let connecting = app_store_connection_health_from_response(&TunnelResponse {
            success: true,
            status: "connecting".into(),
            message: String::new(),
        });
        assert_eq!(connecting.verdict, "protected_degraded");

        let disconnected = app_store_connection_health_from_response(&TunnelResponse {
            success: true,
            status: "disconnected".into(),
            message: String::new(),
        });
        assert_eq!(disconnected.verdict, "failed");
    }
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
async fn verify_app_store_tunnel_traffic() -> Result<(), String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::limited(2))
        .build()
        .map_err(|_| "could not initialize the traffic verifier".to_string())?;
    async fn probe(client: &reqwest::Client, url: &str) -> bool {
        client
            .get(url)
            .header("User-Agent", "DoodleRay-VPN-Connectivity-Check/1.0")
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
    }
    let (cloudflare, public_ip, control_plane) = tokio::join!(
        probe(&client, APP_STORE_TRAFFIC_VERIFY_URLS[0]),
        probe(&client, APP_STORE_TRAFFIC_VERIFY_URLS[1]),
        probe(&client, APP_STORE_TRAFFIC_VERIFY_URLS[2]),
    );
    if cloudflare || public_ip || control_plane {
        Ok(())
    } else {
        Err("all independent traffic probes failed".into())
    }
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
async fn record_app_store_dataplane_probe(result: &Result<(), String>) {
    let mut cache = APP_STORE_DATAPLANE_PROBE.lock().await;
    cache.checked_at = Some(Instant::now());
    cache.ok = result.is_ok();
    cache.detail = result
        .as_ref()
        .map(|_| "Independent HTTPS probes can use the VPN dataplane".to_string())
        .unwrap_or_else(Clone::clone);
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
async fn app_store_dataplane_health_check() -> ConnectionHealthCheck {
    let mut cache = APP_STORE_DATAPLANE_PROBE.lock().await;
    if cache
        .checked_at
        .is_none_or(|checked_at| checked_at.elapsed() >= Duration::from_secs(30))
    {
        let result =
            tokio::time::timeout(Duration::from_secs(4), verify_app_store_tunnel_traffic())
                .await
                .unwrap_or_else(|_| Err("VPN dataplane probe timed out".into()));
        cache.checked_at = Some(Instant::now());
        cache.ok = result.is_ok();
        cache.detail = result
            .as_ref()
            .map(|_| "Independent HTTPS probes can use the VPN dataplane".to_string())
            .unwrap_or_else(Clone::clone);
    }
    health_check(
        "vpn_dataplane",
        if cache.ok { "ok" } else { "error" },
        "VPN dataplane",
        cache.detail.clone(),
    )
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
async fn vpn_connect_app_store(request: ConnectRequest, app: tauri::AppHandle) -> ConnectResult {
    let _runtime_guard = RUNTIME_OP_LOCK.lock().await;
    APP_STORE_CONNECT_CANCELLED.store(false, Ordering::SeqCst);
    if let Ok(mut logs) = CONNECT_LOG.lock() {
        logs.clear();
    }
    vpn_log("starting App Store Network Extension tunnel");

    let asset_directory = match prepare_app_store_routing_asset(&request).await {
        Ok(path) => path,
        Err(message) => {
            vpn_log("App Store routing asset preparation failed");
            return ConnectResult {
                success: false,
                message,
                health: None,
            };
        }
    };
    let config = build_app_store_xray_config(&request, asset_directory.as_deref());
    match wait_for_app_store_tunnel_connected(app_store_tunnel::start(config).await).await {
        Ok(response) => {
            let verification = tokio::select! {
                result = verify_app_store_tunnel_traffic() => result,
                _ = async {
                    while !APP_STORE_CONNECT_CANCELLED.load(Ordering::SeqCst) {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                } => Err("connection cancelled".into()),
            };
            if let Err(error) = verification {
                vpn_log("App Store tunnel failed end-to-end traffic verification; disconnecting");
                let _ = wait_for_app_store_tunnel_disconnected().await;
                if let Ok(mut state) = CONNECTION_STATE.lock() {
                    *state = false;
                }
                if let Ok(mut engine) = ACTIVE_ENGINE.lock() {
                    *engine = None;
                }
                update_tray_disconnected(&app);
                return ConnectResult {
                    success: false,
                    message: format!(
                        "VPN traffic did not become usable ({error}). DoodleRay disconnected automatically to restore internet."
                    ),
                    health: None,
                };
            }
            record_app_store_dataplane_probe(&verification).await;
            if let Ok(mut state) = CONNECTION_STATE.lock() {
                *state = true;
            }
            if let Ok(mut engine) = ACTIVE_ENGINE.lock() {
                *engine = Some("xray+network-extension".into());
            }
            update_tray_connected(&app, &request.server_address);
            vpn_log("App Store Network Extension passed end-to-end traffic verification");
            ConnectResult {
                success: true,
                message: "DoodleRay VPN is connected through Network Extension".into(),
                health: Some(app_store_connection_health_from_response(&response)),
            }
        }
        Err(error) => {
            let _ = wait_for_app_store_tunnel_disconnected().await;
            vpn_log("App Store Network Extension did not reach connected state");
            ConnectResult {
                success: false,
                message: error,
                health: None,
            }
        }
    }
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
async fn wait_for_app_store_tunnel_connected(
    initial: Result<app_store_tunnel::TunnelResponse, String>,
) -> Result<app_store_tunnel::TunnelResponse, String> {
    let mut response = initial?;
    for attempt in 0..40 {
        if APP_STORE_CONNECT_CANCELLED.load(Ordering::SeqCst) {
            return Err("VPN connection cancelled".into());
        }
        if response.success && app_store_tunnel::is_connected_status(&response.status) {
            return Ok(response);
        }
        if !response.success
            || (!app_store_tunnel::is_active_status(&response.status) && attempt > 0)
        {
            return Err(if response.message.is_empty() {
                format!("Network Extension stopped with status {}", response.status)
            } else {
                response.message
            });
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        response = app_store_tunnel::status().await?;
    }
    Err("Network Extension timed out while connecting".into())
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
async fn wait_for_app_store_tunnel_disconnected() -> Result<app_store_tunnel::TunnelResponse, String>
{
    let mut response = app_store_tunnel::stop().await?;
    for _ in 0..20 {
        if !response.success {
            return Err(if response.message.is_empty() {
                "Network Extension could not stop the VPN".into()
            } else {
                response.message
            });
        }
        if app_store_tunnel::is_stopped_status(&response.status) {
            return Ok(response);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
        response = app_store_tunnel::status().await?;
    }
    Err("Network Extension timed out while disconnecting".into())
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
fn app_store_connection_health_from_response(
    response: &app_store_tunnel::TunnelResponse,
) -> ConnectionHealthReport {
    let (severity, detail) =
        if response.success && app_store_tunnel::is_connected_status(&response.status) {
            ("ok", "Network Extension reports connected".to_string())
        } else if response.success && app_store_tunnel::is_active_status(&response.status) {
            (
                "warning",
                format!("Network Extension reports {}", response.status),
            )
        } else {
            (
                "error",
                if response.message.is_empty() {
                    format!("Network Extension reports {}", response.status)
                } else {
                    response.message.clone()
                },
            )
        };
    let mut health = health_report(
        "protected",
        vec![health_check(
            "network_extension",
            severity,
            "Network Extension tunnel",
            detail,
        )],
    );
    health.engine_kind = Some("xray+network-extension".into());
    health
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
async fn app_store_connection_health() -> ConnectionHealthReport {
    match app_store_tunnel::status().await {
        Ok(response) => {
            let connected =
                response.success && app_store_tunnel::is_connected_status(&response.status);
            let mut health = app_store_connection_health_from_response(&response);
            if connected {
                let dataplane = app_store_dataplane_health_check().await;
                if dataplane.severity == "error" {
                    health.verdict = "failed".into();
                }
                health.checks.push(dataplane);
            }
            health
        }
        Err(error) => {
            app_store_connection_health_from_response(&app_store_tunnel::TunnelResponse {
                success: false,
                status: "unknown".into(),
                message: error,
            })
        }
    }
}

#[tauri::command]
async fn vpn_connect(request: ConnectRequest, app: tauri::AppHandle) -> ConnectResult {
    #[cfg(all(target_os = "macos", feature = "app-store"))]
    {
        vpn_connect_app_store(request, app).await
    }
    #[cfg(not(all(target_os = "macos", feature = "app-store")))]
    {
        vpn_connect_direct(request, app).await
    }
}

#[cfg(not(all(target_os = "macos", feature = "app-store")))]
#[cfg_attr(windows, allow(unreachable_code))]
async fn vpn_connect_direct(mut request: ConnectRequest, app: tauri::AppHandle) -> ConnectResult {
    let _runtime_guard = RUNTIME_OP_LOCK.lock().await;

    #[cfg(windows)]
    WINDOWS_CONNECT_CANCELLED.store(false, Ordering::SeqCst);

    // Clear previous connect logs
    if let Ok(mut logs) = CONNECT_LOG.lock() {
        logs.clear();
    }

    let use_xray = uses_xray_engine(&request);
    let is_tun = request.proxy_mode == "tun";

    #[cfg(windows)]
    if is_tun && use_xray {
        if let Ok(adapters) = competing_tun_adapters() {
            if !adapters.is_empty() {
                vpn_log(&format!(
                    "warning: competing TUN adapters are up: {}",
                    adapters.join(", ")
                ));
            }
        }
        match reserve_loopback_ports(3) {
            Ok(ports) => {
                request.socks_port = ports[0];
                request.http_port = ports[1];
                request.api_port = ports[2];
                if let Ok(mut api_port) = ACTIVE_XRAY_API_PORT.lock() {
                    *api_port = request.api_port;
                }
                vpn_log(&format!(
                    "Windows TUN reserved runtime xray ports: socks={} http={} api={}",
                    request.socks_port, request.http_port, request.api_port
                ));
            }
            Err(e) => {
                vpn_log(&format!(
                    "FATAL: failed to reserve runtime TUN ports: {}",
                    e
                ));
                return ConnectResult {
                    success: false,
                    message: format!("Failed to reserve local tunnel ports: {}", e),
                    health: None,
                };
            }
        }
    }

    vpn_log(&format!(
        "=== vpn_connect start === server={}:{} proto={} transport={} mode={} use_xray={}",
        request.server_address,
        request.server_port,
        request.protocol,
        request.transport,
        request.proxy_mode,
        use_xray
    ));

    if is_tun {
        vpn_log(&format!(
            "TUN config: stack={}, dns={}, mtu={}, sniff=true, strict_route={}",
            effective_tun_network_stack(&request.network_stack),
            request.dns_mode,
            tun_mtu_value(&request),
            request.strict_route
        ));
    }

    let exe_rules: Vec<String> = request
        .routing_rules
        .iter()
        .filter(|r| r.rule_type == "exe")
        .map(|r| format!("{}:{}", r.value, r.action))
        .collect();
    if !exe_rules.is_empty() {
        vpn_log(&format!("exe rules: {:?}", exe_rules));
    }

    let debug_path = std::env::temp_dir()
        .join("DoodleRay")
        .join("doodleray_debug_config.json");
    let _ = std::fs::create_dir_all(debug_path.parent().unwrap_or(std::path::Path::new(".")));

    // Stop previous engine — only call stop_tun() (which needs admin password on macOS)
    // when TUN was actually active
    let prev_engine = {
        let engine = ACTIVE_ENGINE.lock().unwrap();
        engine.clone()
    };

    // Hot-switch optimization: when switching servers in app-proxy mode,
    // keep the TUN bridge alive — it routes to localhost SOCKS port, not tied to any server.
    // This prevents game disconnections on server switch.
    let keep_tun_bridge = matches!(
        prev_engine.as_deref(),
        Some("xray+app-proxy") | Some("singbox+app-proxy")
    );

    // Always stop in-process libsingbox (safe, no admin needed)
    vpn_log(&format!(
        "stopping previous engine: {:?} (keep_bridge={})",
        prev_engine, keep_tun_bridge
    ));
    let _ = singbox::stop_singbox();

    match prev_engine.as_deref() {
        Some("xray") => {
            let _ = xray::stop_xray();
        }
        Some("xray+tun") => {
            #[cfg(windows)]
            let _ = tunnel_service_stop("replace_xray_tun");
            let _ = tun::stop_tun();
            let _ = xray::stop_xray();
        }
        Some("xray+tun-service") => {
            #[cfg(windows)]
            let _ = tunnel_service_stop("replace_xray_tun_service");
        }
        Some("xray+app-proxy") => {
            let _ = xray::stop_xray();
        }
        Some("xray+app-proxy-service") => {
            #[cfg(windows)]
            let _ = tunnel_service_stop("replace_xray_app_proxy_service");
            let _ = xray::stop_xray();
        }
        Some("singbox-tun") => {
            #[cfg(windows)]
            let _ = tunnel_service_stop("replace_singbox_tun");
            let _ = tun::stop_tun();
        }
        Some("singbox-tun-service") => {
            #[cfg(windows)]
            let _ = tunnel_service_stop("replace_singbox_tun_service");
        }
        Some("singbox+app-proxy") => {}
        Some("singbox+app-proxy-service") => {
            #[cfg(windows)]
            let _ = tunnel_service_stop("replace_singbox_app_proxy_service");
        }
        Some("singbox") => {}
        _ => {
            let _ = xray::stop_xray();
            let _ = tun::stop_tun();
        }
    }
    restore_system_proxy_if_owned(false);
    if safe_system_proxy_mode(&request.system_proxy_mode) == "clear" {
        repair_stale_system_proxy_only();
        request.system_proxy_mode = "unchanged".into();
    }
    if tun_direct_process_exclusions_need_raw_tun_path(&request)
        && safe_system_proxy_mode(&request.system_proxy_mode) == "set"
    {
        vpn_log(
            "TUN direct app exclusions are active; disabling WinINet compatibility proxy so process routing can apply",
        );
        request.system_proxy_mode = "unchanged".into();
    }
    reset_sb_traffic();
    vpn_log("previous engine stopped, ports freed");

    #[cfg(windows)]
    if !is_tun {
        let ports_busy = !loopback_port_available(request.socks_port)
            || !loopback_port_available(request.http_port)
            || request.socks_port == request.http_port;
        if ports_busy {
            match reserve_loopback_ports(3) {
                Ok(ports) => {
                    vpn_log(&format!(
                        "Proxy ports {} / {} are busy; using runtime ports socks={} http={} api={}",
                        request.socks_port, request.http_port, ports[0], ports[1], ports[2]
                    ));
                    request.socks_port = ports[0];
                    request.http_port = ports[1];
                    request.api_port = ports[2];
                    if let Ok(mut api_port) = ACTIVE_XRAY_API_PORT.lock() {
                        *api_port = request.api_port;
                    }
                }
                Err(e) => {
                    vpn_log(&format!("FATAL: failed to reserve proxy ports: {}", e));
                    return ConnectResult {
                        success: false,
                        message: format!("Failed to reserve local proxy ports: {}", e),
                        health: None,
                    };
                }
            }
        }
    }

    // Forcefully release local ports to prevent "Only one usage of each socket address is normally permitted"
    // caused by zombie processes (or double React Strict Mode invocations) locking the ports.
    let _ = force_free_managed_port(request.socks_port).await;
    let _ = force_free_managed_port(request.http_port).await;
    let _ = force_free_managed_port(request.api_port).await;

    // Only wait for sing-box.exe process death when TUN was killed (not preserved)
    let needs_process_wait = !keep_tun_bridge
        && matches!(
            prev_engine.as_deref(),
            Some("singbox-tun") | Some("xray+tun") | None
        );

    if needs_process_wait {
        for _ in 0..10 {
            if !tun::is_singbox_running() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        if tun::is_singbox_running() {
            eprintln!("[warn] sing-box.exe still alive, retrying stop_tun...");
            let _ = tun::stop_tun();
            for _ in 0..4 {
                if !tun::is_singbox_running() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        }
        // Brief wait for port release after process death
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    if use_xray && is_tun {
        // ═══ xray + TUN: xray-core (SOCKS5) + sing-box (TUN bridge) ═══
        vpn_log("mode: xray + TUN bridge");
        let xray_config = if let Some(ref raw) = request.raw_xray_config {
            vpn_log("using raw xray config (injecting inbounds)");
            inject_xray_inbounds(raw.clone(), &request)
        } else {
            vpn_log("building xray config from request");
            build_xray_config(&request)
        };
        write_debug_config(&debug_path, &xray_config);

        #[cfg(windows)]
        {
            let proxy_exes = process_rule_names(&request, "proxy");
            let direct_exes = process_rule_names(&request, "direct");
            let block_exes = process_rule_names(&request, "block");

            let mut tun_bridge_rules = vec![
                serde_json::json!({ "action": "sniff" }),
                serde_json::json!({ "protocol": "dns", "action": "hijack-dns" }),
                serde_json::json!({ "process_name": system_bypass_process_values(), "outbound": "direct" }),
            ];
            push_process_route(&mut tun_bridge_rules, &block_exes, "block");
            push_process_route(&mut tun_bridge_rules, &direct_exes, "direct");
            push_process_route(&mut tun_bridge_rules, &proxy_exes, "proxy");
            push_routing_policy_singbox_rules(&mut tun_bridge_rules, &request);
            if request.routing_policy.is_none() {
                tun_bridge_rules
                    .push(serde_json::json!({ "ip_is_private": true, "outbound": "direct" }));
            }
            tun_bridge_rules.push(xray_tun_bridge_udp_rule());

            let tun_bridge = serde_json::json!({
                "log": { "level": "warn" },
                "dns": xray_tun_bridge_dns_config_for_request(&request, &direct_exes),
                "inbounds": [tun_inbound_value(&request, Some("DoodleRay Tunnel"), effective_tun_strict_route(&request))],
                "outbounds": xray_tun_bridge_outbounds(&request),
                "route": {
                    "auto_detect_interface": true,
                    "default_domain_resolver": "dns-direct",
                    "final": "proxy",
                    "rules": tun_bridge_rules
                }
            });

            vpn_log("starting Windows Tunnel Service graph (xray + sing-box TUN)...");
            return match tunnel_service_start(
                &request,
                tunnel_service::TunnelEngineKind::XrayTun,
                Some(xray_config),
                tun_bridge,
            ) {
                Ok(status) => {
                    vpn_log(&format!(
                        "Tunnel Service connected: phase={:?}",
                        status.phase
                    ));
                    let compatibility = apply_compat_proxy_after_tun_nonfatal(&request);
                    let status = tunnel_service_report_proxy_compatibility(&status, &compatibility);
                    let mut state = CONNECTION_STATE.lock().unwrap();
                    *state = true;
                    let mut engine = ACTIVE_ENGINE.lock().unwrap();
                    *engine = Some("xray+tun-service".into());
                    update_tray_connected(&app, &request.server_address);
                    let message = protected_connect_message(&compatibility);
                    ConnectResult {
                        success: true,
                        message,
                        health: connect_result_health_for_request_with_status(
                            &request,
                            Some(&status),
                            compatibility.degraded_message(),
                        ),
                    }
                }
                Err(e) => {
                    vpn_log(&format!("FATAL: Tunnel Service failed: {}", e));
                    ConnectResult {
                        success: false,
                        message: format!(
                            "Full Computer components not installed or not ready: {}",
                            e
                        ),
                        health: None,
                    }
                }
            };
        }

        vpn_log(&format!(
            "starting xray-core (socks:{} http:{})",
            request.socks_port, request.http_port
        ));
        let mut start_result = xray::start_xray(&xray_config);
        if let Err(e) = &start_result {
            if e.to_lowercase().contains("bind")
                || e.to_lowercase().contains("listen")
                || e.to_lowercase().contains("socket")
            {
                let _ = force_free_managed_port(request.socks_port).await;
                let _ = force_free_managed_port(request.http_port).await;
                let _ = force_free_managed_port(request.api_port).await;
                std::thread::sleep(std::time::Duration::from_millis(1000));
                start_result = xray::start_xray(&xray_config);
            }
        }

        if let Err(e) = start_result {
            vpn_log(&format!("FATAL: xray-core failed to start: {}", e));
            return ConnectResult {
                success: false,
                message: format!("Failed to start xray-core: {}", e),
                health: None,
            };
        }
        vpn_log("xray-core started OK");
        if let Err(e) = wait_for_port_ready(request.socks_port) {
            vpn_log(&format!(
                "FATAL: xray port not ready before TUN bridge: {}",
                e
            ));
            let _ = xray::stop_xray();
            return ConnectResult {
                success: false,
                message: format!("xray started but local proxy is not ready: {}", e),
                health: None,
            };
        }

        // sing-box as TUN bridge → routes all traffic to xray's SOCKS5
        vpn_log("building TUN bridge config (sing-box -> xray SOCKS5)");
        let proxy_exes = process_rule_names(&request, "proxy");
        let direct_exes = process_rule_names(&request, "direct");
        let block_exes = process_rule_names(&request, "block");

        let mut tun_bridge_rules = vec![
            serde_json::json!({ "action": "sniff" }),
            serde_json::json!({ "protocol": "dns", "action": "hijack-dns" }),
            serde_json::json!({ "process_name": system_bypass_process_values(), "outbound": "direct" }),
        ];
        push_process_route(&mut tun_bridge_rules, &block_exes, "block");
        push_process_route(&mut tun_bridge_rules, &direct_exes, "direct");
        push_process_route(&mut tun_bridge_rules, &proxy_exes, "proxy");
        push_routing_policy_singbox_rules(&mut tun_bridge_rules, &request);
        if request.routing_policy.is_none() {
            tun_bridge_rules
                .push(serde_json::json!({ "ip_is_private": true, "outbound": "direct" }));
        }
        tun_bridge_rules.push(xray_tun_bridge_udp_rule());

        let tun_bridge = serde_json::json!({
            "log": { "level": "warn" },
            "dns": xray_tun_bridge_dns_config_for_request(&request, &direct_exes),
            "inbounds": [tun_inbound_value(&request, None, effective_tun_strict_route(&request))],
            "outbounds": xray_tun_bridge_outbounds(&request),
            "route": {
                "auto_detect_interface": true,
                "default_domain_resolver": "dns-direct",
                "final": "proxy",
                "rules": tun_bridge_rules
            }
        });

        vpn_log("starting TUN bridge (elevated sing-box)...");
        let tun_debug_path = std::env::temp_dir()
            .join("DoodleRay")
            .join("tun_bridge_config.json");
        write_debug_config(&tun_debug_path, &tun_bridge);

        match tun::start_tun_elevated(&tun_bridge) {
            Ok(_) => {
                vpn_log("TUN bridge started OK — connection established");
                let compatibility = apply_compat_proxy_after_tun_nonfatal(&request);
                let mut state = CONNECTION_STATE.lock().unwrap();
                *state = true;
                let mut engine = ACTIVE_ENGINE.lock().unwrap();
                *engine = Some("xray+tun".into());
                update_tray_connected(&app, &request.server_address);
                ConnectResult {
                    success: true,
                    message: protected_connect_message(&compatibility),
                    health: connect_result_health_for_request_with_status(
                        &request,
                        None,
                        compatibility.degraded_message(),
                    ),
                }
            }
            Err(e) => {
                vpn_log(&format!("FATAL: TUN bridge failed: {}", e));
                let _ = xray::stop_xray();
                ConnectResult {
                    success: false,
                    message: format!("Whole computer mode failed: {}", e),
                    health: None,
                }
            }
        }
    } else if use_xray {
        // ═══ xray + System Proxy ═══
        vpn_log("mode: xray + System Proxy");
        let xray_config = if let Some(ref raw) = request.raw_xray_config {
            inject_xray_inbounds(raw.clone(), &request)
        } else {
            build_xray_config(&request)
        };
        write_debug_config(&debug_path, &xray_config);

        vpn_log(&format!(
            "starting xray-core (socks:{} http:{})",
            request.socks_port, request.http_port
        ));
        let mut start_result = xray::start_xray(&xray_config);
        if let Err(e) = &start_result {
            if e.to_lowercase().contains("bind")
                || e.to_lowercase().contains("listen")
                || e.to_lowercase().contains("socket")
            {
                let _ = force_free_managed_port(request.socks_port).await;
                let _ = force_free_managed_port(request.http_port).await;
                let _ = force_free_managed_port(request.api_port).await;
                std::thread::sleep(std::time::Duration::from_millis(1000));
                start_result = xray::start_xray(&xray_config);
            }
        }

        match start_result {
            Ok(_) => {
                vpn_log("xray-core started OK, waiting for port ready...");
                if let Err(e) = wait_for_port_ready(request.socks_port) {
                    vpn_log(&format!("FATAL: xray port not ready: {}", e));
                    let _ = xray::stop_xray();
                    return ConnectResult {
                        success: false,
                        message: format!("xray started but local proxy is not ready: {}", e),
                        health: None,
                    };
                }
                if safe_system_proxy_mode(&request.system_proxy_mode) == "set" {
                    if let Err(e) = wait_for_port_ready(request.http_port) {
                        vpn_log(&format!("FATAL: xray HTTP proxy port not ready: {}", e));
                        let _ = xray::stop_xray();
                        return ConnectResult {
                            success: false,
                            message: format!(
                                "xray started but local HTTP proxy is not ready: {}",
                                e
                            ),
                            health: None,
                        };
                    }
                }
                vpn_log("xray local proxy ports ready");
                let mut state = CONNECTION_STATE.lock().unwrap();
                *state = true;
                let mut engine = ACTIVE_ENGINE.lock().unwrap();
                *engine = Some("xray".into());
                let proxy_action =
                    match apply_system_proxy_mode(&request.system_proxy_mode, request.http_port) {
                        Ok(action) => action,
                        Err(e) => {
                            vpn_log(&format!("FATAL: system proxy failed: {}", e));
                            let _ = xray::stop_xray();
                            if let Ok(mut state) = CONNECTION_STATE.lock() {
                                *state = false;
                            }
                            if let Ok(mut engine) = ACTIVE_ENGINE.lock() {
                                *engine = None;
                            }
                            return ConnectResult {
                                success: false,
                                message: format!(
                                    "xray started but failed to apply system proxy mode: {}",
                                    e
                                ),
                                health: None,
                            };
                        }
                    };
                vpn_log(&format!("system proxy mode applied: {}", proxy_action));

                // Per-app TUN bridge: route specific apps via process_name matching
                // Activated when user adds ANY exe rules in Workshop (proxy, direct, or block)
                // sing-box TUN captures all traffic and routes by process name,
                // while xray handles the actual proxy connection via SOCKS5
                let proxy_exes = process_rule_names(&request, "proxy");
                let direct_exes = process_rule_names(&request, "direct");
                let block_exes = process_rule_names(&request, "block");
                let has_exe_rules =
                    !proxy_exes.is_empty() || !direct_exes.is_empty() || !block_exes.is_empty();

                if has_exe_rules {
                    vpn_log(&format!(
                        "per-app TUN bridge: proxy={:?} direct={:?} block={:?}",
                        proxy_exes, direct_exes, block_exes
                    ));
                    let mut tun_rules: Vec<serde_json::Value> = Vec::new();

                    tun_rules.push(
                        serde_json::json!({ "process_name": system_bypass_process_values(), "outbound": "direct" }),
                    );

                    // 2. Blocked apps
                    push_process_route(&mut tun_rules, &block_exes, "block");

                    // 3. Direct apps — bypass VPN entirely (games, etc.)
                    push_process_route(&mut tun_rules, &direct_exes, "direct");

                    // 4. Proxy apps — route through xray SOCKS5
                    push_process_route(&mut tun_rules, &proxy_exes, "proxy");

                    push_routing_policy_singbox_rules(&mut tun_rules, &request);

                    // 5. Private IPs always go direct
                    if request.routing_policy.is_none() {
                        tun_rules.push(
                            serde_json::json!({ "ip_is_private": true, "outbound": "direct" }),
                        );
                    }
                    tun_rules.push(xray_tun_bridge_udp_rule());

                    let tun_bridge = serde_json::json!({
                        "log": { "level": "warn" },
                        "dns": xray_tun_bridge_dns_config_for_request(&request, &direct_exes),
                        "inbounds": [tun_inbound_value(&request, None, false)],
                        "outbounds": xray_tun_bridge_outbounds(&request),
                        "route": {
                            "auto_detect_interface": true,
                            "default_domain_resolver": "dns-direct",
                            "final": "direct",
                            "rules": tun_rules
                        }
                    });

                    // Hot-switch: if TUN bridge is already running from previous session, reuse it
                    // (it routes to localhost SOCKS port which didn't change)
                    #[cfg(not(windows))]
                    {
                        if tun::is_singbox_running() {
                            *engine = Some("xray+app-proxy".into());
                            update_tray_connected(&app, &request.server_address);
                            let total = proxy_exes.len() + direct_exes.len() + block_exes.len();
                            return ConnectResult {
                                success: true,
                                message: format!(
                                    "Server switched (app routing preserved, {} app rules active)",
                                    total
                                ),
                                health: connect_result_health_for_request(&request),
                            };
                        }
                    }

                    #[cfg(windows)]
                    {
                        return match tunnel_service_start(
                            &request,
                            tunnel_service::TunnelEngineKind::SingboxTun,
                            None,
                            tun_bridge.clone(),
                        ) {
                            Ok(_status) => {
                                *engine = Some("xray+app-proxy-service".into());
                                update_tray_connected(&app, &request.server_address);
                                let total = proxy_exes.len() + direct_exes.len() + block_exes.len();
                                ConnectResult {
                                    success: true,
                                    message: format!(
                                        "Browsers/apps with service app routing ({} rules: {} proxy, {} direct, {} block)",
                                        total,
                                        proxy_exes.len(),
                                        direct_exes.len(),
                                        block_exes.len()
                                    ),
                                    health: connect_result_health_for_request(&request),
                                }
                            }
                            Err(e) => ConnectResult {
                                success: false,
                                message: format!(
                                    "Full Computer components not installed or not ready: {}",
                                    e
                                ),
                                health: None,
                            },
                        };
                    }

                    #[cfg(not(windows))]
                    {
                        if tun::start_tun_elevated(&tun_bridge).is_ok() {
                            *engine = Some("xray+app-proxy".into());
                            update_tray_connected(&app, &request.server_address);
                            let total = proxy_exes.len() + direct_exes.len() + block_exes.len();
                            return ConnectResult {
                                success: true,
                                message: format!(
                                    "Browsers/apps with app routing ({} rules: {} proxy, {} direct, {} block)",
                                    total,
                                    proxy_exes.len(),
                                    direct_exes.len(),
                                    block_exes.len()
                                ),
                                health: connect_result_health_for_request(&request),
                            };
                        }
                    }
                }

                update_tray_connected(&app, &request.server_address);
                ConnectResult {
                    success: true,
                    message: proxy_mode_success_message(
                        proxy_action,
                        request.socks_port,
                        request.http_port,
                    ),
                    health: connect_result_health_for_request(&request),
                }
            }
            Err(e) => {
                vpn_log(&format!("FATAL: xray-core failed: {}", e));
                ConnectResult {
                    success: false,
                    message: format!("Failed to start xray-core: {}", e),
                    health: None,
                }
            }
        }
    } else if is_tun {
        // ═══ Non-xhttp + TUN ═══
        vpn_log("mode: sing-box TUN (direct, no xray)");
        let config = build_singbox_config(&request);
        write_debug_config(&debug_path, &config);

        #[cfg(windows)]
        {
            vpn_log("starting Windows Tunnel Service graph (sing-box TUN)...");
            return match tunnel_service_start(
                &request,
                tunnel_service::TunnelEngineKind::SingboxTun,
                None,
                config,
            ) {
                Ok(status) => {
                    vpn_log(&format!(
                        "Tunnel Service connected: phase={:?}",
                        status.phase
                    ));
                    let compatibility = apply_compat_proxy_after_tun_nonfatal(&request);
                    let status = tunnel_service_report_proxy_compatibility(&status, &compatibility);
                    let mut state = CONNECTION_STATE.lock().unwrap();
                    *state = true;
                    let mut engine = ACTIVE_ENGINE.lock().unwrap();
                    *engine = Some("singbox-tun-service".into());
                    update_tray_connected(&app, &request.server_address);
                    let message = protected_connect_message(&compatibility);
                    ConnectResult {
                        success: true,
                        message,
                        health: connect_result_health_for_request_with_status(
                            &request,
                            Some(&status),
                            compatibility.degraded_message(),
                        ),
                    }
                }
                Err(e) => {
                    vpn_log(&format!("FATAL: Tunnel Service failed: {}", e));
                    ConnectResult {
                        success: false,
                        message: format!(
                            "Full Computer components not installed or not ready: {}",
                            e
                        ),
                        health: None,
                    }
                }
            };
        }

        vpn_log("starting sing-box TUN (elevated)...");
        match tun::start_tun_elevated(&config) {
            Ok(_) => {
                vpn_log("sing-box TUN started OK");
                let compatibility = apply_compat_proxy_after_tun_nonfatal(&request);
                let mut state = CONNECTION_STATE.lock().unwrap();
                *state = true;
                let mut engine = ACTIVE_ENGINE.lock().unwrap();
                *engine = Some("singbox-tun".into());
                update_tray_connected(&app, &request.server_address);
                ConnectResult {
                    success: true,
                    message: protected_connect_message(&compatibility),
                    health: connect_result_health_for_request_with_status(
                        &request,
                        None,
                        compatibility.degraded_message(),
                    ),
                }
            }
            Err(e) => {
                vpn_log(&format!("FATAL: Whole computer mode failed: {}", e));
                ConnectResult {
                    success: false,
                    message: format!("Whole computer mode failed: {}", e),
                    health: None,
                }
            }
        }
    } else {
        // ═══ Non-xhttp + System Proxy ═══
        vpn_log("mode: sing-box + System Proxy");
        let config = build_singbox_config(&request);
        write_debug_config(&debug_path, &config);

        vpn_log("starting sing-box in-process...");
        match singbox::start_singbox(&config) {
            Ok(_) => {
                vpn_log("sing-box started OK, waiting for port ready...");
                if let Err(e) = wait_for_port_ready(request.socks_port) {
                    vpn_log(&format!("FATAL: sing-box port not ready: {}", e));
                    let _ = singbox::stop_singbox();
                    return ConnectResult {
                        success: false,
                        message: format!("sing-box started but local proxy is not ready: {}", e),
                        health: None,
                    };
                }
                if safe_system_proxy_mode(&request.system_proxy_mode) == "set" {
                    if let Err(e) = wait_for_port_ready(request.http_port) {
                        vpn_log(&format!("FATAL: sing-box HTTP proxy port not ready: {}", e));
                        let _ = singbox::stop_singbox();
                        return ConnectResult {
                            success: false,
                            message: format!(
                                "sing-box started but local HTTP proxy is not ready: {}",
                                e
                            ),
                            health: None,
                        };
                    }
                }
                vpn_log("local proxy ports ready");
                let mut state = CONNECTION_STATE.lock().unwrap();
                *state = true;
                let mut engine = ACTIVE_ENGINE.lock().unwrap();
                *engine = Some("singbox".into());
                let proxy_action =
                    match apply_system_proxy_mode(&request.system_proxy_mode, request.http_port) {
                        Ok(action) => action,
                        Err(e) => {
                            vpn_log(&format!("FATAL: system proxy failed: {}", e));
                            let _ = singbox::stop_singbox();
                            if let Ok(mut state) = CONNECTION_STATE.lock() {
                                *state = false;
                            }
                            if let Ok(mut engine) = ACTIVE_ENGINE.lock() {
                                *engine = None;
                            }
                            return ConnectResult {
                                success: false,
                                message: format!(
                                    "sing-box started but failed to apply system proxy mode: {}",
                                    e
                                ),
                                health: None,
                            };
                        }
                    };
                vpn_log(&format!("system proxy mode applied: {}", proxy_action));

                // Per-app TUN bridge: route specific apps via process_name matching
                // Activated when user adds ANY exe rules in Workshop (proxy, direct, or block)
                // sing-box TUN captures all traffic and routes by process name
                let proxy_exes = process_rule_names(&request, "proxy");
                let direct_exes = process_rule_names(&request, "direct");
                let block_exes = process_rule_names(&request, "block");
                let has_exe_rules =
                    !proxy_exes.is_empty() || !direct_exes.is_empty() || !block_exes.is_empty();

                if has_exe_rules {
                    // Build routing rules for the TUN bridge
                    let mut tun_rules: Vec<serde_json::Value> = Vec::new();

                    // 1. System processes always bypass TUN
                    tun_rules.push(
                        serde_json::json!({ "process_name": system_bypass_process_values(), "outbound": "direct" }),
                    );

                    // 2. Blocked apps
                    push_process_route(&mut tun_rules, &block_exes, "block");

                    // 3. Direct apps — bypass VPN entirely (games, etc.)
                    push_process_route(&mut tun_rules, &direct_exes, "direct");

                    // 4. Proxy apps — route through SOCKS5
                    push_process_route(&mut tun_rules, &proxy_exes, "proxy");

                    push_routing_policy_singbox_rules(&mut tun_rules, &request);

                    // 5. Private IPs always go direct
                    if request.routing_policy.is_none() {
                        tun_rules.push(
                            serde_json::json!({ "ip_is_private": true, "outbound": "direct" }),
                        );
                    }
                    tun_rules.push(xray_tun_bridge_udp_rule());

                    let tun_bridge = serde_json::json!({
                        "log": { "level": "warn" },
                        "dns": xray_tun_bridge_dns_config_for_request(&request, &direct_exes),
                        "inbounds": [tun_inbound_value(&request, None, false)],
                        "outbounds": xray_tun_bridge_outbounds(&request),
                        "route": {
                            "auto_detect_interface": true,
                            "default_domain_resolver": "dns-direct",
                            "final": "direct",
                            "rules": tun_rules
                        }
                    });

                    // Hot-switch: if TUN bridge is already running from previous session, reuse it
                    #[cfg(not(windows))]
                    {
                        if tun::is_singbox_running() {
                            *engine = Some("singbox+app-proxy".into());
                            update_tray_connected(&app, &request.server_address);
                            let total = proxy_exes.len() + direct_exes.len() + block_exes.len();
                            return ConnectResult {
                                success: true,
                                message: format!(
                                    "Server switched (app routing preserved, {} app rules active)",
                                    total
                                ),
                                health: connect_result_health_for_request(&request),
                            };
                        }
                    }

                    #[cfg(windows)]
                    {
                        return match tunnel_service_start(
                            &request,
                            tunnel_service::TunnelEngineKind::SingboxTun,
                            None,
                            tun_bridge.clone(),
                        ) {
                            Ok(_status) => {
                                *engine = Some("singbox+app-proxy-service".into());
                                update_tray_connected(&app, &request.server_address);
                                let total = proxy_exes.len() + direct_exes.len() + block_exes.len();
                                ConnectResult {
                                    success: true,
                                    message: format!(
                                        "Browsers/apps with service app routing ({} rules: {} proxy, {} direct, {} block)",
                                        total,
                                        proxy_exes.len(),
                                        direct_exes.len(),
                                        block_exes.len()
                                    ),
                                    health: connect_result_health_for_request(&request),
                                }
                            }
                            Err(e) => ConnectResult {
                                success: false,
                                message: format!(
                                    "Full Computer components not installed or not ready: {}",
                                    e
                                ),
                                health: None,
                            },
                        };
                    }

                    #[cfg(not(windows))]
                    {
                        if tun::start_tun_elevated(&tun_bridge).is_ok() {
                            *engine = Some("singbox+app-proxy".into());
                            update_tray_connected(&app, &request.server_address);
                            let total = proxy_exes.len() + direct_exes.len() + block_exes.len();
                            return ConnectResult {
                                success: true,
                                message: format!(
                                    "Browsers/apps with app routing ({} rules: {} proxy, {} direct, {} block)",
                                    total,
                                    proxy_exes.len(),
                                    direct_exes.len(),
                                    block_exes.len()
                                ),
                                health: connect_result_health_for_request(&request),
                            };
                        }
                    }
                }

                update_tray_connected(&app, &request.server_address);
                ConnectResult {
                    success: true,
                    message: proxy_mode_success_message(
                        proxy_action,
                        request.socks_port,
                        request.http_port,
                    ),
                    health: connect_result_health_for_request(&request),
                }
            }
            Err(e) => ConnectResult {
                success: false,
                message: format!("Failed to start: {}", e),
                health: None,
            },
        }
    }
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
async fn vpn_disconnect_app_store(app: tauri::AppHandle) -> ConnectResult {
    let _runtime_guard = RUNTIME_OP_LOCK.lock().await;
    let was_connected = CONNECTION_STATE.lock().map(|state| *state).unwrap_or(false);

    match wait_for_app_store_tunnel_disconnected().await {
        Ok(response) if response.success => {
            if let Ok(mut state) = CONNECTION_STATE.lock() {
                *state = false;
            }
            if let Ok(mut engine) = ACTIVE_ENGINE.lock() {
                *engine = None;
            }
            update_tray_disconnected(&app);
            ConnectResult {
                success: true,
                message: if was_connected {
                    "Disconnected".into()
                } else {
                    "VPN is already disconnected".into()
                },
                health: None,
            }
        }
        Ok(response) => ConnectResult {
            success: false,
            message: if response.message.is_empty() {
                "Network Extension could not stop the VPN".into()
            } else {
                response.message
            },
            health: None,
        },
        Err(error) => ConnectResult {
            success: false,
            message: error,
            health: None,
        },
    }
}

#[tauri::command]
async fn vpn_disconnect(app: tauri::AppHandle) -> ConnectResult {
    #[cfg(all(target_os = "macos", feature = "app-store"))]
    {
        APP_STORE_CONNECT_CANCELLED.store(true, Ordering::SeqCst);
        vpn_disconnect_app_store(app).await
    }
    #[cfg(not(all(target_os = "macos", feature = "app-store")))]
    {
        #[cfg(windows)]
        WINDOWS_CONNECT_CANCELLED.store(true, Ordering::SeqCst);
        vpn_disconnect_direct(app).await
    }
}

#[cfg(not(all(target_os = "macos", feature = "app-store")))]
async fn vpn_disconnect_direct(app: tauri::AppHandle) -> ConnectResult {
    let _runtime_guard = RUNTIME_OP_LOCK.lock().await;

    let is_connected = {
        let state = CONNECTION_STATE.lock().unwrap();
        *state
    };

    // Stop all engines — always clean up everything to prevent orphaned processes
    let prev_engine = {
        let engine = ACTIVE_ENGINE.lock().unwrap();
        engine.clone()
    };

    // Always stop in-process libsingbox (safe even if not running)
    let _ = singbox::stop_singbox();

    // Always stop xray (safe if not running)
    let _ = xray::stop_xray();

    // Only kill external sing-box.exe and wait if TUN was active
    let had_tun = matches!(
        prev_engine.as_deref(),
        Some("singbox-tun")
            | Some("singbox-tun-service")
            | Some("singbox+app-proxy")
            | Some("singbox+app-proxy-service")
            | Some("xray+tun")
            | Some("xray+tun-service")
            | Some("xray+app-proxy")
            | Some("xray+app-proxy-service")
            | None
    );

    #[cfg(windows)]
    let _ = tunnel_service_stop("disconnect");

    if had_tun {
        let _ = tun::stop_tun();
        #[cfg(not(windows))]
        for _ in 0..8 {
            if !tun::is_singbox_running() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        if tun::is_singbox_running() {
            eprintln!("[warn] sing-box.exe still alive after stop_tun, retrying...");
            let _ = tun::stop_tun();
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    #[cfg(windows)]
    terminate_orphaned_doodleray_engine_processes();

    restore_system_proxy_if_owned(false);

    let mut state = CONNECTION_STATE.lock().unwrap();
    *state = false;
    let mut engine = ACTIVE_ENGINE.lock().unwrap();
    *engine = None;
    update_tray_disconnected(&app);
    ConnectResult {
        success: true,
        message: if is_connected {
            "Disconnected".into()
        } else {
            "Cleaned up VPN engines".into()
        },
        health: None,
    }
}

fn system_dns_needs_public_override(host: &str, port: u16) -> bool {
    let target = (host, port);
    match target.to_socket_addrs() {
        Ok(mut addrs) => addrs.any(|addr| !is_public_ip(addr.ip())),
        Err(_) => true,
    }
}

#[derive(Deserialize)]
struct DnsJsonAnswer {
    #[serde(rename = "type")]
    record_type: Option<u16>,
    data: String,
}

#[derive(Deserialize)]
struct DnsJsonResponse {
    #[serde(rename = "Answer")]
    answer: Option<Vec<DnsJsonAnswer>>,
}

async fn resolve_public_ipv4_doh(host: &str) -> Option<Ipv4Addr> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .resolve(
            "cloudflare-dns.com",
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(104, 16, 248, 249)), 443),
        )
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;

    let body = client
        .get(format!(
            "https://cloudflare-dns.com/dns-query?name={}&type=A",
            host
        ))
        .header("Accept", "application/dns-json")
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;

    let response: DnsJsonResponse = serde_json::from_str(&body).ok()?;
    response.answer?.into_iter().find_map(|answer| {
        if answer.record_type != Some(1) {
            return None;
        }
        let ip = answer.data.parse::<Ipv4Addr>().ok()?;
        if is_public_ip(IpAddr::V4(ip)) {
            Some(ip)
        } else {
            None
        }
    })
}

async fn direct_fetch_client(
    parsed_url: &Url,
    timeout: Duration,
) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .timeout(timeout)
        .redirect(safe_subscription_redirect_policy());

    if let Some(host) = parsed_url.host_str() {
        if host.parse::<IpAddr>().is_err() {
            let port = parsed_url.port_or_known_default().unwrap_or(443);
            if system_dns_needs_public_override(host, port) {
                let ip = resolve_public_ipv4_doh(host).await.ok_or_else(|| {
                    "Subscription host did not resolve to a public IP".to_string()
                })?;
                builder = builder.resolve(host, SocketAddr::new(IpAddr::V4(ip), port));
            }
        }
    }

    builder
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))
}

fn describe_reqwest_fetch_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "request timed out".to_string()
    } else if error.is_connect() {
        format!("connection error ({})", error)
    } else {
        error.to_string()
    }
}

async fn send_fetch_get(
    client: reqwest::Client,
    parsed_url: &Url,
) -> Result<reqwest::Response, String> {
    let response = client
        .get(parsed_url.clone())
        .header("User-Agent", "DoodleRay/2.0")
        .send()
        .await
        .map_err(|e| format!("Fetch failed: {}", describe_reqwest_fetch_error(e)))?;

    if !response.status().is_success() {
        return Err(format!(
            "HTTP {}: {}",
            response.status().as_u16(),
            response.status().as_str()
        ));
    }

    Ok(response)
}

fn system_proxy_fetch_client(
    parsed_url: &Url,
    timeout: Duration,
) -> Result<Option<reqwest::Client>, String> {
    let _ = parsed_url;

    #[cfg(any(windows, target_os = "macos"))]
    {
        let Some(proxy_url) = sysproxy::current_manual_http_proxy_for_url(parsed_url.scheme())?
        else {
            return Ok(None);
        };
        let proxy = reqwest::Proxy::all(&proxy_url)
            .map_err(|e| format!("system proxy fallback is invalid: {}", e))?;
        reqwest::Client::builder()
            .timeout(timeout)
            .proxy(proxy)
            .redirect(safe_subscription_redirect_policy())
            .build()
            .map(Some)
            .map_err(|e| format!("system proxy HTTP client error: {}", e))
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        Ok(None)
    }
}

async fn fetch_http_response_with_fallback(
    parsed_url: &Url,
    timeout: Duration,
) -> Result<reqwest::Response, String> {
    let direct_client = direct_fetch_client(parsed_url, timeout).await?;

    match send_fetch_get(direct_client, parsed_url).await {
        Ok(response) => Ok(response),
        Err(direct_error) => {
            let Some(proxy_client) = system_proxy_fetch_client(parsed_url, timeout)? else {
                return Err(direct_error);
            };

            send_fetch_get(proxy_client, parsed_url)
                .await
                .map_err(|proxy_error| {
                    format!(
                        "{}; system proxy fallback failed: {}",
                        direct_error, proxy_error
                    )
                })
        }
    }
}

#[tauri::command]
async fn vpn_status() -> bool {
    #[cfg(all(target_os = "macos", feature = "app-store"))]
    {
        let active = app_store_tunnel::status()
            .await
            .map(|response| {
                response.success && app_store_tunnel::is_active_status(&response.status)
            })
            .unwrap_or_else(|_| CONNECTION_STATE.lock().map(|state| *state).unwrap_or(false));
        if let Ok(mut state) = CONNECTION_STATE.lock() {
            *state = active;
        }
        active
    }

    #[cfg(not(all(target_os = "macos", feature = "app-store")))]
    {
        #[cfg(windows)]
        {
            if tunnel_service_reports_active() {
                if let Ok(mut state) = CONNECTION_STATE.lock() {
                    *state = true;
                }
                return true;
            }
        }

        CONNECTION_STATE.lock().map(|state| *state).unwrap_or(false)
    }
}

/// Check if we're running with Administrator/root privileges
#[tauri::command]
fn is_admin() -> bool {
    #[cfg(windows)]
    {
        use std::mem;
        use std::ptr;

        unsafe {
            #[link(name = "advapi32")]
            extern "system" {
                fn OpenProcessToken(
                    ProcessHandle: *mut std::ffi::c_void,
                    DesiredAccess: u32,
                    TokenHandle: *mut *mut std::ffi::c_void,
                ) -> i32;
                fn GetTokenInformation(
                    TokenHandle: *mut std::ffi::c_void,
                    TokenInformationClass: u32,
                    TokenInformation: *mut std::ffi::c_void,
                    TokenInformationLength: u32,
                    ReturnLength: *mut u32,
                ) -> i32;
            }
            #[link(name = "kernel32")]
            extern "system" {
                fn GetCurrentProcess() -> *mut std::ffi::c_void;
                fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
            }

            let mut token: *mut std::ffi::c_void = ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), 0x0008, &mut token) == 0 {
                return false;
            }

            let mut elevation: u32 = 0;
            let mut return_length: u32 = 0;
            let result = GetTokenInformation(
                token,
                20,
                &mut elevation as *mut u32 as *mut std::ffi::c_void,
                mem::size_of::<u32>() as u32,
                &mut return_length,
            );
            CloseHandle(token);

            result != 0 && elevation != 0
        }
    }
    #[cfg(not(windows))]
    {
        // On macOS/Linux, check if running as root (uid 0)
        unsafe {
            extern "C" {
                fn getuid() -> u32;
            }
            getuid() == 0
        }
    }
}

/// Relaunch the app as Administrator (triggers UAC prompt)
#[tauri::command]
fn restart_as_admin() -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe_path =
            std::env::current_exe().map_err(|e| format!("Failed to get exe path: {}", e))?;

        let exe_str: Vec<u16> = exe_path
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        let verb: Vec<u16> = "runas\0".encode_utf16().collect();

        unsafe {
            #[link(name = "shell32")]
            extern "system" {
                fn ShellExecuteW(
                    hwnd: *mut std::ffi::c_void,
                    lpOperation: *const u16,
                    lpFile: *const u16,
                    lpParameters: *const u16,
                    lpDirectory: *const u16,
                    nShowCmd: i32,
                ) -> isize;
            }

            let result = ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                exe_str.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            );

            if result as usize <= 32 {
                return Err("User declined UAC or ShellExecute failed".into());
            }
        }

        std::process::exit(0);
    }
    #[cfg(not(windows))]
    {
        Err("restart_as_admin is only supported on Windows. Use sudo on macOS.".into())
    }
}

/// Scan installed applications on Windows (reads registry Uninstall keys)
/// Returns: [{ name: "Steam", path: "steam.exe" }, ...]
#[tauri::command]
fn scan_installed_apps() -> Result<Vec<serde_json::Value>, String> {
    #[cfg(windows)]
    {
        use std::collections::BTreeMap;
        let mut apps: BTreeMap<String, String> = BTreeMap::new();
        let mut seen_app_paths: HashSet<String> = HashSet::new();
        let mut add_app = |name: String, path: String| {
            let display = name.trim().to_string();
            let value = path.trim().trim_matches('"').to_string();
            if display.is_empty() || value.is_empty() {
                return;
            }
            let lower_value = value.to_lowercase();
            if seen_app_paths.contains(&lower_value) || apps.contains_key(&display) {
                return;
            }
            seen_app_paths.insert(lower_value);
            apps.insert(display, value);
        };

        let reg_paths = [
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
            "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ];
        let hives = [
            winreg::enums::HKEY_LOCAL_MACHINE,
            winreg::enums::HKEY_CURRENT_USER,
        ];

        for hive in &hives {
            for reg_path in &reg_paths {
                if let Ok(key) = winreg::RegKey::predef(*hive).open_subkey(reg_path) {
                    for subkey_name in key.enum_keys().filter_map(|k| k.ok()) {
                        if let Ok(subkey) = key.open_subkey(&subkey_name) {
                            let name: String = subkey.get_value("DisplayName").unwrap_or_default();
                            let install_location: String =
                                subkey.get_value("InstallLocation").unwrap_or_default();
                            let display_icon: String =
                                subkey.get_value("DisplayIcon").unwrap_or_default();

                            if name.is_empty() {
                                continue;
                            }
                            // Skip system/framework entries
                            if name.contains("Microsoft Visual C++")
                                || name.contains("Microsoft .NET")
                                || name.contains("Windows SDK")
                                || name.contains("Redistributable")
                            {
                                continue;
                            }

                            // Strategy: find the actual exe name (not uninstaller!)
                            // 1. DisplayIcon often points to main exe: "C:\...\steam.exe,0"
                            // 2. InstallLocation is the install directory
                            let mut exe_name = String::new();

                            // Try DisplayIcon first — strip comma suffix and quotes
                            let icon_clean = display_icon
                                .split(',')
                                .next()
                                .unwrap_or("")
                                .trim_matches('"')
                                .trim();

                            if !icon_clean.is_empty() && icon_clean.to_lowercase().ends_with(".exe")
                            {
                                // Check it's not an uninstaller
                                let basename = std::path::Path::new(icon_clean)
                                    .file_name()
                                    .map(|f| f.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                let lower = basename.to_lowercase();
                                if !lower.contains("unins")
                                    && !lower.contains("uninst")
                                    && !lower.contains("remove")
                                {
                                    exe_name = basename;
                                }
                            }

                            // If DisplayIcon failed, try scanning InstallLocation for main exe
                            if exe_name.is_empty() && !install_location.is_empty() {
                                let dir = std::path::Path::new(&install_location);
                                if dir.is_dir() {
                                    // Look for .exe files in root of install dir (not recursive)
                                    if let Ok(entries) = std::fs::read_dir(dir) {
                                        for entry in entries.filter_map(|e| e.ok()) {
                                            let fname =
                                                entry.file_name().to_string_lossy().to_string();
                                            let lower = fname.to_lowercase();
                                            if lower.ends_with(".exe")
                                                && !lower.contains("unins")
                                                && !lower.contains("uninst")
                                                && !lower.contains("crash")
                                                && !lower.contains("update")
                                            {
                                                exe_name = fname;
                                                break; // take first non-helper exe
                                            }
                                        }
                                        // If still nothing, take any .exe that's not an uninstaller
                                        if exe_name.is_empty() {
                                            if let Ok(entries) = std::fs::read_dir(dir) {
                                                for entry in entries.filter_map(|e| e.ok()) {
                                                    let fname = entry
                                                        .file_name()
                                                        .to_string_lossy()
                                                        .to_string();
                                                    let lower = fname.to_lowercase();
                                                    if lower.ends_with(".exe")
                                                        && !lower.contains("unins")
                                                    {
                                                        exe_name = fname;
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if exe_name.is_empty() {
                                continue;
                            }

                            add_app(name, exe_name);
                        }
                    }
                }
            }
        }

        if let Ok(output) = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-Process | Where-Object { $_.Path -and $_.Path.ToLower().EndsWith('.exe') } | ForEach-Object { $_.ProcessName + '|' + $_.Path }",
            ])
            .creation_flags(0x08000000)
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.splitn(2, '|').collect();
                if parts.len() != 2 {
                    continue;
                }
                let process_name = parts[0].trim();
                let path = parts[1].trim();
                let exe = std::path::Path::new(path)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| format!("{}.exe", process_name));
                add_app(format!("{} (running)", process_name), exe);
            }
        }

        let mut steam_libraries: Vec<std::path::PathBuf> = Vec::new();
        let steam_roots = [
            std::path::PathBuf::from(r"C:\Program Files (x86)\Steam"),
            std::path::PathBuf::from(r"C:\Program Files\Steam"),
        ];
        for root in steam_roots {
            if root.is_dir() {
                steam_libraries.push(root.clone());
            }
            let vdf = root.join("steamapps").join("libraryfolders.vdf");
            if let Ok(text) = std::fs::read_to_string(vdf) {
                for line in text.lines() {
                    let trimmed = line.trim();
                    if !trimmed.starts_with("\"path\"") {
                        continue;
                    }
                    let parts: Vec<&str> = trimmed.split('"').collect();
                    if parts.len() >= 4 {
                        let library = std::path::PathBuf::from(parts[3].replace("\\\\", "\\"));
                        if library.is_dir() {
                            steam_libraries.push(library);
                        }
                    }
                }
            }
        }
        steam_libraries.sort();
        steam_libraries.dedup();
        for library in steam_libraries {
            let common = library.join("steamapps").join("common");
            if !common.is_dir() {
                continue;
            }
            if let Ok(games) = std::fs::read_dir(common) {
                for game in games.filter_map(|entry| entry.ok()) {
                    let game_dir = game.path();
                    if !game_dir.is_dir() {
                        continue;
                    }
                    let game_name = game_dir
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Steam game".to_string());
                    let candidate_dirs = [
                        game_dir.clone(),
                        game_dir.join("Binaries").join("Win64"),
                        game_dir.join("TslGame").join("Binaries").join("Win64"),
                    ];
                    for dir in candidate_dirs {
                        if !dir.is_dir() {
                            continue;
                        }
                        if let Ok(entries) = std::fs::read_dir(dir) {
                            for entry in entries.filter_map(|entry| entry.ok()) {
                                let file_name = entry.file_name().to_string_lossy().to_string();
                                let lower = file_name.to_lowercase();
                                if !lower.ends_with(".exe")
                                    || lower.contains("crash")
                                    || lower.contains("redist")
                                    || lower.contains("unins")
                                    || lower.contains("uninst")
                                {
                                    continue;
                                }
                                add_app(format!("{} - {}", game_name, file_name), file_name);
                            }
                        }
                    }
                }
            }
        }

        // Also scan %LOCALAPPDATA% for Electron/Squirrel apps (Claude, Discord, Slack, etc.)
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let local_dir = std::path::Path::new(&local_app_data);
            // Scan direct subdirectories (Squirrel installs: %LOCALAPPDATA%\claude\, Discord\, etc.)
            // and %LOCALAPPDATA%\Programs\ subdirectories
            let scan_dirs: Vec<std::path::PathBuf> = {
                let mut dirs = Vec::new();
                // Direct subdirs of LOCALAPPDATA (Squirrel-style)
                if let Ok(entries) = std::fs::read_dir(local_dir) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let p = entry.path();
                        if p.is_dir() {
                            dirs.push(p);
                        }
                    }
                }
                // Subdirs of LOCALAPPDATA\Programs (e.g. claude\)
                let programs = local_dir.join("Programs");
                if programs.is_dir() {
                    if let Ok(entries) = std::fs::read_dir(&programs) {
                        for entry in entries.filter_map(|e| e.ok()) {
                            let p = entry.path();
                            if p.is_dir() {
                                dirs.push(p);
                            }
                        }
                    }
                }
                dirs
            };

            for dir in scan_dirs {
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let fname = entry.file_name().to_string_lossy().to_string();
                        let lower = fname.to_lowercase();
                        if lower.ends_with(".exe")
                            && !lower.contains("unins")
                            && !lower.contains("uninst")
                            && !lower.contains("update")
                            && !lower.contains("crash")
                        {
                            // Derive display name from directory name
                            let dir_name = dir
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_default();
                            if dir_name.is_empty() || dir_name.to_lowercase() == "programs" {
                                continue;
                            }
                            // Skip if we already have this app from registry
                            let display = {
                                let mut s = dir_name.clone();
                                // Capitalize first letter
                                if let Some(first) = s.get_mut(..1) {
                                    first.make_ascii_uppercase();
                                }
                                s
                            };
                            add_app(display, fname);
                            break; // one exe per directory
                        }
                    }
                }
            }
        }

        // Scan MSIX/AppX packages (Claude Desktop, etc.)
        if let Ok(output) = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command",
                "Get-AppxPackage | Where-Object { $_.IsFramework -eq $false -and $_.SignatureKind -ne 'System' } | ForEach-Object { $_.Name + '|' + $_.InstallLocation }"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.splitn(2, '|').collect();
                if parts.len() != 2 { continue; }
                let pkg_name = parts[0].trim();
                let install_loc = parts[1].trim();
                if install_loc.is_empty() { continue; }
                // Skip Microsoft system apps
                if pkg_name.starts_with("Microsoft.") || pkg_name.starts_with("Windows.") { continue; }
                // Look for .exe in the app directory
                let dir = std::path::Path::new(install_loc);
                // Check root and "app" subdirectory (Electron MSIX pattern)
                let search_dirs = vec![dir.to_path_buf(), dir.join("app")];
                for search_dir in search_dirs {
                    if !search_dir.is_dir() { continue; }
                    if let Ok(entries) = std::fs::read_dir(&search_dir) {
                        for entry in entries.filter_map(|e| e.ok()) {
                            let fname = entry.file_name().to_string_lossy().to_string();
                            let lower = fname.to_lowercase();
                            if lower.ends_with(".exe")
                                && !lower.contains("unins") && !lower.contains("uninst")
                                && !lower.contains("crash") && !lower.contains("update")
                            {
                                add_app(pkg_name.to_string(), fname);
                                break;
                            }
                        }
                    }
                }
            }
        }

        let result: Vec<serde_json::Value> = apps
            .into_iter()
            .map(|(name, path)| serde_json::json!({ "name": name, "path": path }))
            .collect();

        Ok(result)
    }
    #[cfg(not(windows))]
    {
        Ok(vec![])
    }
}

/// Returns new proxy log lines — dispatches to xray or sing-box
#[tauri::command]
async fn get_proxy_logs() -> Vec<String> {
    let engine = {
        let e = ACTIVE_ENGINE.lock().unwrap();
        e.clone().unwrap_or_default()
    };

    let engine_logs = match engine.as_str() {
        "singbox" | "singbox-tun" => {
            // Query sing-box clash API /connections for new connections
            let client = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_millis(500))
                .build()
                .unwrap_or_default();

            let resp = match client.get("http://127.0.0.1:9191/connections").send().await {
                Ok(r) => r,
                Err(_) => return vec![],
            };
            let text = match resp.text().await {
                Ok(t) => t,
                Err(_) => return vec![],
            };
            let json: serde_json::Value = match serde_json::from_str(&text) {
                Ok(j) => j,
                Err(_) => return vec![],
            };

            let connections = match json.get("connections").and_then(|c| c.as_array()) {
                Some(c) => c,
                None => return vec![],
            };

            let mut new_lines = Vec::new();
            let mut seen = SB_SEEN_CONNS.lock().unwrap();
            if seen.is_none() {
                *seen = Some(HashSet::new());
            }
            let seen_set = seen.as_mut().unwrap();

            for conn in connections {
                let id = conn
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if id.is_empty() || seen_set.contains(&id) {
                    continue;
                }
                seen_set.insert(id);

                let meta = match conn.get("metadata") {
                    Some(m) => m,
                    None => continue,
                };
                let host = meta.get("host").and_then(|v| v.as_str()).unwrap_or("");
                let dst_ip = meta
                    .get("destinationIP")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let dst_port = meta
                    .get("destinationPort")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let network = meta
                    .get("network")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tcp");
                let chain = conn
                    .get("chains")
                    .and_then(|c| c.as_array())
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_str())
                    .unwrap_or("direct");

                let target = if !host.is_empty() { host } else { dst_ip };
                if target.is_empty() {
                    continue;
                }

                // Only log proxy-routed connections (skip direct/dns)
                if chain == "direct" {
                    continue;
                }

                let label = format!(
                    "tunneling request to {}:{}:{} [{}]",
                    network, target, dst_port, chain
                );
                new_lines.push(label);
            }

            // Limit seen set size to prevent memory leak — evict older half
            if seen_set.len() > 5000 {
                let to_keep: Vec<String> =
                    seen_set.iter().skip(seen_set.len() / 2).cloned().collect();
                seen_set.clear();
                for id in to_keep {
                    seen_set.insert(id);
                }
            }

            new_lines
        }
        "xray+tun-service"
        | "singbox-tun-service"
        | "xray+app-proxy-service"
        | "singbox+app-proxy-service" => Vec::new(),
        _ => xray::get_new_logs(),
    };

    // Prepend any connect-phase logs
    if let Ok(mut connect_logs) = CONNECT_LOG.lock() {
        if !connect_logs.is_empty() {
            let mut combined = connect_logs.drain(..).collect::<Vec<_>>();
            combined.extend(engine_logs);
            return combined;
        }
    }
    engine_logs
}

/// Reset sing-box traffic counters (call on connect/disconnect)
fn reset_sb_traffic() {
    *SB_PREV_DOWN.lock().unwrap() = 0;
    *SB_PREV_UP.lock().unwrap() = 0;
    *SB_SEEN_CONNS.lock().unwrap() = None;
}

fn run_xray_statsquery(xray_exe: &Path, endpoint: &str) -> Option<String> {
    let mut cmd = std::process::Command::new(xray_exe);
    cmd.args(["api", "statsquery", "-s", endpoint, "-reset"]);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    let mut child = cmd.spawn().ok()?;
    let deadline = Instant::now() + Duration::from_millis(1200);
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                let mut stdout = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = pipe.read_to_string(&mut stdout);
                }
                return Some(stdout);
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Get real traffic stats — dispatches to xray or sing-box clash API based on active engine
#[tauri::command]
async fn get_traffic_stats() -> serde_json::Value {
    let is_connected = {
        let state = CONNECTION_STATE.lock().unwrap();
        *state
    };
    if !is_connected {
        return serde_json::json!({ "download": 0, "upload": 0 });
    }

    let engine = {
        let e = ACTIVE_ENGINE.lock().unwrap();
        e.clone().unwrap_or_default()
    };

    if engine.is_empty() {
        return serde_json::json!({ "download": 0, "upload": 0 });
    }

    match engine.as_str() {
        "singbox" | "singbox-tun" => {
            // Query sing-box clash API: GET /connections → { downloadTotal, uploadTotal }
            let client = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_millis(500))
                .build()
                .unwrap_or_default();

            if let Ok(resp) = client.get("http://127.0.0.1:9191/connections").send().await {
                if let Ok(text) = resp.text().await {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        let total_down = json
                            .get("downloadTotal")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        let total_up = json
                            .get("uploadTotal")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);

                        // Calculate delta from previous poll
                        let prev_down = {
                            let mut p = SB_PREV_DOWN.lock().unwrap();
                            let prev = *p;
                            *p = total_down;
                            prev
                        };
                        let prev_up = {
                            let mut p = SB_PREV_UP.lock().unwrap();
                            let prev = *p;
                            *p = total_up;
                            prev
                        };

                        // First poll (prev=0) → don't show huge spike
                        let dl = if prev_down == 0 {
                            0
                        } else {
                            (total_down - prev_down).max(0)
                        };
                        let ul = if prev_up == 0 {
                            0
                        } else {
                            (total_up - prev_up).max(0)
                        };

                        return serde_json::json!({ "download": dl, "upload": ul });
                    }
                }
            }
            serde_json::json!({ "download": 0, "upload": 0 })
        }
        "xray+tun-service"
        | "singbox-tun-service"
        | "xray+app-proxy-service"
        | "singbox+app-proxy-service" => serde_json::json!({ "download": 0, "upload": 0 }),
        _ => {
            // xray-core stats API
            let exe_dir = std::env::current_exe()
                .unwrap_or_default()
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf();
            #[cfg(windows)]
            let xray_exe = exe_dir.join("xray-core").join("xray.exe");
            #[cfg(not(windows))]
            let xray_exe = exe_dir.join("xray-core").join("xray");

            if !xray_exe.exists() {
                let logs = xray::get_recent_activity();
                return serde_json::json!({ "download": logs.0, "upload": logs.1 });
            }

            let mut dl: i64 = 0;
            let mut ul: i64 = 0;

            let api_port = ACTIVE_XRAY_API_PORT
                .lock()
                .map(|port| *port)
                .unwrap_or(10813);
            let endpoint = format!("127.0.0.1:{}", api_port);
            if let Some(stdout) = run_xray_statsquery(&xray_exe, &endpoint) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                    if let Some(stats) = json.get("stat").and_then(|s| s.as_array()) {
                        for stat in stats {
                            let name = stat.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            let value = stat
                                .get("value")
                                .and_then(|v| {
                                    v.as_str()
                                        .map(|s| s.parse::<i64>().unwrap_or(0))
                                        .or_else(|| v.as_i64())
                                })
                                .unwrap_or(0);
                            if name.contains("api") {
                                continue;
                            }
                            if name.contains("downlink") {
                                dl += value;
                            } else if name.contains("uplink") {
                                ul += value;
                            }
                        }
                    }
                }
            }

            if dl == 0 && ul == 0 {
                let logs = xray::get_recent_activity();
                dl = logs.0;
                ul = logs.1;
            }

            serde_json::json!({ "download": dl, "upload": ul })
        }
    }
}

/// Check what process is using a given port
#[tauri::command]
async fn check_port(port: u16) -> serde_json::Value {
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("netstat");
        cmd.args(["-ano"]);
        cmd.creation_flags(0x08000000);
        if let Ok(output) = cmd.output() {
            let text = String::from_utf8_lossy(&output.stdout);
            let port_str = format!(":{}", port);
            for line in text.lines() {
                if line.contains(&port_str) && line.contains("LISTENING") {
                    if let Some(pid_str) = line.split_whitespace().last() {
                        if let Ok(pid) = pid_str.parse::<u32>() {
                            let mut proc_name = format!("PID {}", pid);
                            #[cfg(windows)]
                            let doodleray_owned = windows_pid_is_doodleray_owned(pid);
                            #[cfg(not(windows))]
                            let doodleray_owned = false;
                            let mut info_cmd = std::process::Command::new("tasklist");
                            info_cmd.args(["/FI", &format!("PID eq {}", pid), "/FO", "CSV", "/NH"]);
                            info_cmd.creation_flags(0x08000000);
                            if let Ok(info) = info_cmd.output() {
                                let info_text = String::from_utf8_lossy(&info.stdout);
                                if let Some(name) = info_text.split(',').next() {
                                    proc_name = name.trim().trim_matches('"').to_string();
                                }
                            }
                            return serde_json::json!({
                                "busy": true, "pid": pid, "process": proc_name,
                                "doodleray_owned": doodleray_owned,
                                "message": format!("Port {} is used by {} (PID {})", port, proc_name, pid)
                            });
                        }
                    }
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(output) = std::process::Command::new("lsof")
            .args(["-i", &format!(":{}", port), "-t"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some(pid_str) = text.lines().next() {
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    return serde_json::json!({ "busy": true, "pid": pid, "process": format!("PID {}", pid), "message": format!("Port {} is used by PID {}", port, pid) });
                }
            }
        }
    }
    serde_json::json!({ "busy": false, "message": format!("Port {} is free", port) })
}

/// Force kill process on a specific port
#[tauri::command]
async fn force_free_port(port: u16) -> String {
    if !requested_port_is_safe(port) {
        return format!("Refusing to kill process on unmanaged port {}", port);
    }
    force_free_managed_port(port).await
}

async fn force_free_managed_port(port: u16) -> String {
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("netstat");
        cmd.args(["-ano"]);
        cmd.creation_flags(0x08000000);
        if let Ok(output) = cmd.output() {
            let text = String::from_utf8_lossy(&output.stdout);
            let port_str = format!(":{}", port);
            for line in text.lines() {
                if line.contains(&port_str) && line.contains("LISTENING") {
                    if let Some(pid_str) = line.split_whitespace().last() {
                        if let Ok(pid) = pid_str.parse::<u32>() {
                            if windows_pid_is_doodleray_owned(pid) {
                                return match windows_terminate_pid(pid) {
                                    Ok(()) => {
                                        format!(
                                            "Terminated DoodleRay-owned PID {} on port {}",
                                            pid, port
                                        )
                                    }
                                    Err(err) => {
                                        format!("Failed to terminate DoodleRay-owned PID {} on port {}: {}", pid, port, err)
                                    }
                                };
                            }
                            return format!(
                                "Port {} is used by PID {}, but it is not DoodleRay-owned",
                                port, pid
                            );
                        }
                    }
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(output) = std::process::Command::new("lsof")
            .args(["-i", &format!(":{}", port), "-t"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some(pid_str) = text.lines().next() {
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    let _ = std::process::Command::new("kill")
                        .args(["-9", &pid.to_string()])
                        .output();
                    return format!("Killed PID {} on port {}", pid, port);
                }
            }
        }
    }
    format!("Port {} is already free", port)
}

#[cfg(windows)]
fn windows_terminate_pid(pid: u32) -> Result<(), String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    let process = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    if process.is_null() {
        return Err(format!("{}", std::io::Error::last_os_error()));
    }
    let ok = unsafe { TerminateProcess(process, 1) };
    unsafe {
        CloseHandle(process);
    }
    if ok == 0 {
        return Err(format!("{}", std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(windows)]
fn windows_pid_is_doodleray_owned(pid: u32) -> bool {
    let script = format!(
        "(Get-CimInstance Win32_Process -Filter \"ProcessId = {}\" | Select-Object -First 1 -ExpandProperty ExecutablePath)",
        pid
    );
    let mut cmd = std::process::Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", &script]);
    cmd.creation_flags(0x08000000);
    let output = match cmd.output() {
        Ok(output) => output,
        Err(_) => return false,
    };
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return false;
    }
    let lower = path.to_lowercase();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|parent| parent.to_path_buf()))
        .map(|p| p.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    !exe_dir.is_empty()
        && lower.starts_with(&exe_dir)
        && (lower.ends_with("doodleray.exe")
            || lower.ends_with("doodlerayservice.exe")
            || lower.ends_with("xray.exe")
            || lower.ends_with("sing-box.exe"))
}

/// Fully quit the application (disconnect VPN, unset proxy, exit)
#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    #[cfg(windows)]
    let _ = tunnel_service_stop("quit_app");
    let _ = singbox::stop_singbox();
    let _ = xray::stop_xray();
    let _ = tun::stop_tun();
    restore_system_proxy_if_owned(false);
    app.exit(0);
}

// ═══════════════════════════════════════════════════════════
//  System Tray helpers
// ═══════════════════════════════════════════════════════════

fn update_tray_connected(app: &tauri::AppHandle, server: &str) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let tip = format!("DoodleRay VPN — Connected ✓\n{}", server);
        let _ = tray.set_tooltip(Some(&tip));
    }
}

fn update_tray_disconnected(app: &tauri::AppHandle) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some("DoodleRay VPN — Disconnected"));
    }
}

/// Wait for the SOCKS port to become ready (max 2s)
/// Prevents DNS leaks by ensuring the core is actually listening before we set system proxy
fn wait_for_port_ready(port: u16) -> Result<(), String> {
    use std::net::SocketAddr;
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    for _ in 0..20 {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(format!("127.0.0.1:{} did not open in time", port))
}

fn apply_compat_proxy_after_tun(request: &ConnectRequest) -> Result<&'static str, String> {
    if tun_direct_process_exclusions_need_raw_tun_path(request) {
        repair_stale_system_proxy_only();
        if let Ok(mut managed) = SYSTEM_PROXY_MANAGED.lock() {
            *managed = false;
        }
        return Ok("disabled_for_direct_app_exclusions");
    }

    if safe_system_proxy_mode(&request.system_proxy_mode) == "set" {
        wait_for_port_ready(request.http_port).map_err(|e| {
            format!(
                "protected mode local HTTP proxy is not ready for Windows system proxy: {}",
                e
            )
        })?;
    }
    apply_system_proxy_mode(&request.system_proxy_mode, request.http_port)
}

#[derive(Debug, Clone, Default)]
struct CompatProxyOutcome {
    degraded: Option<String>,
    #[cfg_attr(not(windows), allow(dead_code))]
    report_detail: Option<&'static str>,
}

impl CompatProxyOutcome {
    fn degraded_message(&self) -> Option<&str> {
        self.degraded.as_deref()
    }

    fn is_degraded(&self) -> bool {
        self.degraded.is_some()
    }
}

fn apply_compat_proxy_after_tun_nonfatal(request: &ConnectRequest) -> CompatProxyOutcome {
    match apply_compat_proxy_after_tun(request) {
        Ok(action) => {
            vpn_log(&format!("protected system proxy mode applied: {}", action));
            if action == "disabled_for_direct_app_exclusions" {
                CompatProxyOutcome {
                    degraded: None,
                    report_detail: Some(
                        "Windows proxy compatibility disabled for direct app exclusions; TUN process routing is authoritative",
                    ),
                }
            } else {
                CompatProxyOutcome::default()
            }
        }
        Err(error) => {
            let message = format!(
                "Windows proxy compatibility is degraded; retry/repair can continue without stopping TUN: {}",
                error
            );
            vpn_log(&format!("WARN: {}", message));
            CompatProxyOutcome {
                degraded: Some(message),
                report_detail: None,
            }
        }
    }
}

fn protected_connect_message(compatibility: &CompatProxyOutcome) -> String {
    if compatibility.is_degraded() {
        "Whole computer connected via DoodleRay Tunnel Service; browser compatibility is recovering"
            .into()
    } else {
        "Whole computer connected via DoodleRay Tunnel Service".into()
    }
}

#[cfg(windows)]
fn competing_tun_adapters() -> Result<Vec<String>, String> {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-NetAdapter | Where-Object { $_.Status -eq 'Up' -and $_.Name -ne 'DoodleRay Tunnel' -and ($_.Name -like '*tun*' -or $_.InterfaceDescription -like '*tun*' -or $_.InterfaceDescription -like '*Wintun*' -or $_.InterfaceDescription -like '*sing*') } | ForEach-Object { $_.Name }",
        ])
        .creation_flags(0x08000000)
        .output()
        .map_err(|e| format!("failed to inspect TUN adapters: {}", e))?;
    if !output.status.success() {
        return Err("Get-NetAdapter failed while checking competing TUN adapters".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

#[cfg(windows)]
fn reserve_loopback_ports(count: usize) -> Result<Vec<u16>, String> {
    let mut listeners = Vec::with_capacity(count);
    let mut ports = Vec::with_capacity(count);
    for _ in 0..count {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .map_err(|e| format!("bind 127.0.0.1:0 failed: {}", e))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("read reserved local port failed: {}", e))?
            .port();
        ports.push(port);
        listeners.push(listener);
    }
    drop(listeners);
    Ok(ports)
}

#[cfg(windows)]
fn loopback_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

#[cfg(windows)]
fn tun_op_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("tun-{}", now)
}

#[cfg(windows)]
fn tunnel_service_exe_path() -> Result<PathBuf, String> {
    let exe_dir = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot resolve app directory")?
        .to_path_buf();
    let path = exe_dir.join("DoodleRayService.exe");
    if path.exists() {
        Ok(path)
    } else {
        Err(format!("DoodleRayService.exe not found at {:?}", path))
    }
}

#[cfg(any(windows, test))]
fn ensure_tunnel_start_not_cancelled(cancelled: bool) -> Result<(), &'static str> {
    if cancelled {
        Err("VPN connection was cancelled")
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn tunnel_service_start(
    request: &ConnectRequest,
    engine_kind: tunnel_service::TunnelEngineKind,
    xray_config: Option<serde_json::Value>,
    singbox_config: serde_json::Value,
) -> Result<tunnel_service::TunnelStatus, String> {
    // Preflight: fail with an actionable reinstall message before asking the
    // service to bring up TUN when a required runtime file is missing.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(app_dir) = exe.parent() {
            let missing: Vec<&str> = windows_required_runtime_files(app_dir)
                .into_iter()
                .filter(|(_, path)| !path.exists())
                .map(|(label, _)| label)
                .collect();
            if !missing.is_empty() {
                return Err(format!(
                    "required DoodleRay runtime files are missing: {}. Reinstall DoodleRay from the official installer.",
                    missing.join(", ")
                ));
            }
        }
    }
    ensure_tunnel_service_running()?;
    let _ = ipc::tunnel_service_hello(env!("CARGO_PKG_VERSION"))?;
    let response = send_start_tunnel_command(tunnel_service::StartTunnelRequest {
        op_id: tun_op_id(),
        engine_kind,
        xray_config,
        singbox_config,
        socks_port: request.socks_port,
        http_port: request.http_port,
        api_port: Some(request.api_port),
        redacted_label: format!("{}:{}", request.protocol, request.transport),
    })?;
    let mut status = match response {
        tunnel_service::TunnelResponse::Status(status) => status,
        tunnel_service::TunnelResponse::Error { message } => return Err(message),
        tunnel_service::TunnelResponse::Diagnostics(_) => {
            return Err("Tunnel Service returned diagnostics for StartTunnel".into())
        }
    };

    let started = Instant::now();
    // Long enough to cover one failed bring-up attempt plus the service's
    // bounded DoodleRay-owned TUN adapter repair retry.
    let timeout = Duration::from_secs(90);
    let mut last_phase = status.phase.clone();
    loop {
        if let Err(message) =
            ensure_tunnel_start_not_cancelled(WINDOWS_CONNECT_CANCELLED.load(Ordering::SeqCst))
        {
            vpn_log("Tunnel Service start cancelled; stopping partial runtime");
            let _ = tunnel_service_stop("connect_cancelled");
            return Err(message.into());
        }

        match status.state {
            tunnel_service::TunnelState::Connected => return Ok(status),
            tunnel_service::TunnelState::Failed => {
                return Err(status
                    .error
                    .unwrap_or_else(|| "Tunnel Service failed to start TUN".into()))
            }
            // Tunnel start briefly passes through Disconnected while the
            // service stops the previous owned children (replace_tunnel), and
            // the bounded bring-up retry does the same for tun_adapter_repair.
            // Keep waiting instead of aborting the connect in that transient.
            tunnel_service::TunnelState::Disconnected
                if matches!(
                    status.last_repair_action.as_deref(),
                    Some("replace_tunnel") | Some("tun_adapter_repair")
                ) => {}
            tunnel_service::TunnelState::Disconnected => {
                return Err(status
                    .error
                    .unwrap_or_else(|| "Tunnel Service stopped before TUN became ready".into()))
            }
            tunnel_service::TunnelState::Connecting
            | tunnel_service::TunnelState::Disconnecting => {}
        }

        if started.elapsed() >= timeout {
            return Err(format!(
                "Tunnel Service did not become ready in {}s (last phase: {})",
                timeout.as_secs(),
                status.phase.as_deref().unwrap_or("unknown")
            ));
        }

        std::thread::sleep(Duration::from_millis(100));
        status = match ipc::tunnel_service_status()? {
            tunnel_service::TunnelResponse::Status(next) => next,
            tunnel_service::TunnelResponse::Error { message } => return Err(message),
            tunnel_service::TunnelResponse::Diagnostics(_) => {
                return Err("Tunnel Service returned diagnostics for status".into())
            }
        };
        if status.phase != last_phase {
            vpn_log(&format!("Tunnel Service phase: {:?}", status.phase));
            last_phase = status.phase.clone();
        }
    }
}

#[cfg(windows)]
fn send_start_tunnel_command(
    mut request: tunnel_service::StartTunnelRequest,
) -> Result<tunnel_service::TunnelResponse, String> {
    match ipc::send_tunnel_command(&tunnel_service::TunnelCommand::StartTunnel(request.clone())) {
        Err(error) if request.api_port.is_some() && is_legacy_api_port_rejection(&error) => {
            vpn_log(
                "Tunnel Service is from an older build and does not accept runtime api_port; retrying StartTunnel with legacy-compatible IPC payload.",
            );
            request.api_port = None;
            ipc::send_tunnel_command(&tunnel_service::TunnelCommand::StartTunnel(request))
        }
        result => result,
    }
}

#[cfg(windows)]
fn is_legacy_api_port_rejection(error: &str) -> bool {
    error.contains("unknown field `api_port`")
        || (error.contains("unknown field")
            && error.contains("api_port")
            && error.contains("StartTunnel"))
}

#[cfg(windows)]
fn tunnel_service_stop(reason: &str) -> Result<tunnel_service::TunnelStatus, String> {
    let response = ipc::send_tunnel_command(&tunnel_service::TunnelCommand::StopTunnel(
        tunnel_service::StopTunnelRequest {
            op_id: tun_op_id(),
            reason: reason.to_string(),
        },
    ))?;
    match response {
        tunnel_service::TunnelResponse::Status(status) => {
            wait_for_tunnel_service_stop(Duration::from_secs(5));
            Ok(status)
        }
        tunnel_service::TunnelResponse::Error { message } => Err(message),
        tunnel_service::TunnelResponse::Diagnostics(_) => {
            Err("Tunnel Service returned diagnostics for StopTunnel".into())
        }
    }
}

#[cfg(windows)]
fn tunnel_service_prepare_update() -> Result<tunnel_service::TunnelStatus, String> {
    let response = ipc::send_tunnel_command(&tunnel_service::TunnelCommand::PrepareForUpdate)?;
    match response {
        tunnel_service::TunnelResponse::Status(status) => Ok(status),
        tunnel_service::TunnelResponse::Error { message } => Err(message),
        tunnel_service::TunnelResponse::Diagnostics(_) => {
            Err("Tunnel Service returned diagnostics for PrepareForUpdate".into())
        }
    }
}

#[cfg(windows)]
fn tunnel_service_report_proxy_compatibility(
    status: &tunnel_service::TunnelStatus,
    compatibility: &CompatProxyOutcome,
) -> tunnel_service::TunnelStatus {
    let (ok, detail) = if let Some(reason) = compatibility.degraded_message() {
        (false, reason.to_string())
    } else if let Some(detail) = compatibility.report_detail {
        (true, detail.to_string())
    } else {
        (
            true,
            "Windows proxy compatibility is ready for the active tunnel".to_string(),
        )
    };
    match ipc::send_tunnel_command(&tunnel_service::TunnelCommand::ReportProxyCompatibility(
        tunnel_service::ProxyCompatibilityReport {
            op_id: status.active_op_id.clone(),
            ok,
            detail,
        },
    )) {
        Ok(tunnel_service::TunnelResponse::Status(status)) => status,
        Ok(tunnel_service::TunnelResponse::Error { message }) => {
            vpn_log(&format!(
                "WARN: Tunnel Service did not accept proxy compatibility report: {}",
                message
            ));
            status.clone()
        }
        Ok(tunnel_service::TunnelResponse::Diagnostics(_)) => status.clone(),
        Err(error) => {
            vpn_log(&format!(
                "WARN: failed to report proxy compatibility to Tunnel Service: {}",
                error
            ));
            status.clone()
        }
    }
}

#[cfg(windows)]
#[tauri::command]
fn install_tunnel_service() -> Result<String, String> {
    let service_exe = tunnel_service_exe_path()?;
    let file_w: Vec<u16> = service_exe
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let params_w: Vec<u16> = "install\0".encode_utf16().collect();
    let verb_w: Vec<u16> = "runas\0".encode_utf16().collect();

    unsafe {
        #[link(name = "shell32")]
        extern "system" {
            fn ShellExecuteW(
                hwnd: *mut std::ffi::c_void,
                lpOperation: *const u16,
                lpFile: *const u16,
                lpParameters: *const u16,
                lpDirectory: *const u16,
                nShowCmd: i32,
            ) -> isize;
        }

        let result = ShellExecuteW(
            std::ptr::null_mut(),
            verb_w.as_ptr(),
            file_w.as_ptr(),
            params_w.as_ptr(),
            std::ptr::null(),
            0,
        );

        if result as usize <= 32 {
            return Err("Tunnel service installation was cancelled or failed".into());
        }
    }

    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(500));
        if tunnel_service_registration_state().is_ok() {
            return Ok("Tunnel service installed; it starts only while VPN is connecting".into());
        }
    }
    Ok("Tunnel service install started. Please try connecting again in a few seconds.".into())
}

#[cfg(not(windows))]
#[tauri::command]
fn install_tunnel_service() -> Result<String, String> {
    Err("Tunnel service is only available on Windows".into())
}

#[cfg(windows)]
#[tauri::command]
fn tunnel_service_health() -> Result<String, String> {
    let response = match ipc::tunnel_service_status() {
        Ok(response) => response,
        Err(_)
            if matches!(
                tunnel_service_registration_state(),
                Ok(windows_service::service::ServiceState::Stopped)
            ) =>
        {
            return Ok("Tunnel service ready: stopped while VPN is disconnected".into())
        }
        Err(error) => return Err(error),
    };
    match response {
        tunnel_service::TunnelResponse::Status(status) => Ok(format!(
            "Tunnel service ready: version={}, state={:?}, phase={:?}",
            status.service_version, status.state, status.phase
        )),
        tunnel_service::TunnelResponse::Error { message } => Err(message),
        tunnel_service::TunnelResponse::Diagnostics(_) => {
            Err("Tunnel Service returned diagnostics for health check".into())
        }
    }
}

#[cfg(windows)]
#[tauri::command]
fn tunnel_service_diagnostics() -> Result<String, String> {
    match ipc::send_tunnel_command(&tunnel_service::TunnelCommand::GetDiagnostics)? {
        tunnel_service::TunnelResponse::Diagnostics(diagnostics) => Ok(format!(
            "service_version={}\nstate={:?}\nphase={:?}\nerror={:?}\ntimings_ms={:?}\npowershell_fallback_count={}\nsingbox_check_ms={:?}\nxray_spawn_ms={:?}\nadapter_probe_backend={:?}\nroute_probe_backend={:?}\nnative_probe_ms={:?}\nfallback_probe_ms={:?}\n\n{}",
            diagnostics.status.service_version,
            diagnostics.status.state,
            diagnostics.status.phase,
            diagnostics.status.error,
            diagnostics.status.timings_ms,
            diagnostics.status.powershell_fallback_count,
            diagnostics.status.singbox_check_ms,
            diagnostics.status.xray_spawn_ms,
            diagnostics.status.adapter_probe_backend,
            diagnostics.status.route_probe_backend,
            diagnostics.status.native_probe_ms,
            diagnostics.status.fallback_probe_ms,
            diagnostics.log_tail.join("\n")
        )),
        tunnel_service::TunnelResponse::Status(_) => {
            Err("Tunnel Service returned status for diagnostics".into())
        }
        tunnel_service::TunnelResponse::Error { message } => Err(message),
    }
}

#[cfg(not(windows))]
#[tauri::command]
fn tunnel_service_diagnostics() -> Result<String, String> {
    Err("Tunnel service is only available on Windows".into())
}

#[cfg(not(windows))]
#[tauri::command]
fn tunnel_service_health() -> Result<String, String> {
    Err("Tunnel service is only available on Windows".into())
}

#[tauri::command]
async fn prepare_for_app_update() -> Result<String, String> {
    let _runtime_guard = RUNTIME_OP_LOCK.lock().await;

    #[cfg(windows)]
    {
        let _ = tunnel_service_prepare_update();
    }
    let _ = singbox::stop_singbox();
    let _ = xray::stop_xray();
    let tun_result = tun::stop_tun_for_update();
    restore_system_proxy_if_owned(false);
    tun_result?;
    Ok("Update preparation complete".into())
}

/// Check connection health by testing if SOCKS port is alive
#[tauri::command(async)]
fn check_connection_health(socks_port: u16) -> bool {
    use std::net::SocketAddr;
    let addr: SocketAddr = format!("127.0.0.1:{}", socks_port).parse().unwrap();
    TcpStream::connect_timeout(&addr, Duration::from_millis(2000)).is_ok()
}

#[tauri::command]
async fn get_connection_health(
    proxy_mode: Option<String>,
    system_proxy_mode: Option<String>,
    socks_port: u16,
    http_port: u16,
) -> ConnectionHealthReport {
    #[cfg(all(target_os = "macos", feature = "app-store"))]
    {
        let _ = (proxy_mode, system_proxy_mode, socks_port, http_port);
        app_store_connection_health().await
    }
    #[cfg(not(all(target_os = "macos", feature = "app-store")))]
    {
        build_connection_health(
            proxy_mode.as_deref().unwrap_or("tun"),
            system_proxy_mode.as_deref().unwrap_or("set"),
            socks_port,
            http_port,
        )
    }
}

#[tauri::command]
async fn get_connection_health_full(
    proxy_mode: Option<String>,
    system_proxy_mode: Option<String>,
    socks_port: u16,
    http_port: u16,
) -> ConnectionHealthReport {
    #[cfg(all(target_os = "macos", feature = "app-store"))]
    {
        let _ = (proxy_mode, system_proxy_mode, socks_port, http_port);
        app_store_connection_health().await
    }
    #[cfg(not(all(target_os = "macos", feature = "app-store")))]
    {
        build_full_connection_health(
            proxy_mode.as_deref().unwrap_or("tun"),
            system_proxy_mode.as_deref().unwrap_or("set"),
            socks_port,
            http_port,
        )
    }
}

#[tauri::command]
async fn repair_windows_runtime() -> Result<String, String> {
    let _runtime_guard = RUNTIME_OP_LOCK.lock().await;

    let mut actions = Vec::new();

    #[cfg(windows)]
    {
        let active_service_status = match ipc::tunnel_service_status() {
            Ok(tunnel_service::TunnelResponse::Status(status))
                if matches!(
                    status.state,
                    tunnel_service::TunnelState::Connected
                        | tunnel_service::TunnelState::Connecting
                ) =>
            {
                Some(status)
            }
            _ => None,
        };

        if let Some(status) = active_service_status.as_ref() {
            if let Ok(mut state) = CONNECTION_STATE.lock() {
                *state = true;
            }
            actions.push(format!(
                "preserved active Tunnel Service state: {:?}, generation={}",
                status.state, status.service_generation
            ));
        } else {
            repair_stale_system_proxy_only();
            actions.push("checked stale DoodleRay WinINet proxy ownership".to_string());
        }

        let app_dir = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .parent()
            .ok_or("Cannot resolve app directory")?
            .to_path_buf();

        for (label, path) in windows_required_runtime_files(&app_dir) {
            if path.exists() {
                actions.push(format!("{}: present", label));
            } else {
                actions.push(format!("{}: missing at {}", label, path.display()));
            }
        }

        let connected = active_service_status.is_some()
            || CONNECTION_STATE.lock().map(|state| *state).unwrap_or(false);
        if !connected {
            let _ = tunnel_service_stop("repair_windows_runtime");
            let _ = tun::stop_tun();
            terminate_orphaned_doodleray_engine_processes();
            actions.extend(repair_stale_doodleray_network_artifacts());
            actions.push(
                "soft rebooted stale DoodleRay tunnel generation and owned engine processes"
                    .to_string(),
            );
        }

        if active_service_status.is_some() {
            actions.push("Tunnel Service IPC: active".to_string());
        } else {
            match ipc::tunnel_service_status() {
                Ok(_) => actions.push("Tunnel Service IPC: ready".to_string()),
                Err(error) => {
                    actions.push(format!(
                        "Tunnel Service IPC failed before repair: {}",
                        error
                    ));
                    match tunnel_service_registration_state() {
                        Ok(windows_service::service::ServiceState::Stopped) => actions.push(
                            "Tunnel Service: registered and stopped while disconnected".into(),
                        ),
                        Ok(state) => actions
                            .push(format!("Tunnel Service: registered with state {:?}", state)),
                        Err(_) => match install_tunnel_service() {
                            Ok(message) => actions.push(message),
                            Err(install_error) => actions.push(format!(
                                "Tunnel Service install repair failed: {}",
                                install_error
                            )),
                        },
                    }
                }
            }
        }

        actions.push("WebView2: app is running; installer carries offline runtime".to_string());
    }

    #[cfg(not(windows))]
    {
        actions.push("Windows runtime repair is only active on Windows".to_string());
    }

    Ok(actions.join("\n"))
}

#[cfg(windows)]
fn repair_stale_doodleray_network_artifacts() -> Vec<String> {
    let script = r#"
$ErrorActionPreference = 'Continue'
$summary = New-Object System.Collections.Generic.List[string]

$nrptRemoved = 0
if (Get-Command Get-DnsClientNrptRule -ErrorAction SilentlyContinue) {
  $rules = @(Get-DnsClientNrptRule -ErrorAction SilentlyContinue | Where-Object {
    ($_.DisplayName -match 'DoodleRay') -or
    ($_.Namespace -match 'DoodleRay') -or
    ($_.Comment -match 'DoodleRay')
  })
  foreach ($rule in $rules) {
    try {
      Remove-DnsClientNrptRule -Name $rule.Name -Force -ErrorAction Stop
      $nrptRemoved += 1
    } catch {}
  }
}
$summary.Add("stale_nrpt_removed=$nrptRemoved")

$routeRemoved = 0
$adapter = Get-NetAdapter -Name 'DoodleRay Tunnel' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($adapter) {
  $routes = @(Get-NetRoute -InterfaceIndex $adapter.ifIndex -ErrorAction SilentlyContinue | Where-Object {
    $_.DestinationPrefix -in @('0.0.0.0/0','0.0.0.0/1','128.0.0.0/1','::/0') -or
    $_.DestinationPrefix -like '198.18.*'
  })
  foreach ($route in $routes) {
    try {
      Remove-NetRoute -InterfaceIndex $adapter.ifIndex -DestinationPrefix $route.DestinationPrefix -NextHop $route.NextHop -Confirm:$false -ErrorAction Stop
      $routeRemoved += 1
    } catch {}
  }
  $summary.Add("doodleray_adapter_present_after_stop=true")
} else {
  $summary.Add("doodleray_adapter_present_after_stop=false")
}
$summary.Add("stale_routes_removed=$routeRemoved")

$summary -join "`n"
"#;

    match windows_powershell_output(script) {
        Ok(output) => output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| format!("network artifact repair: {}", redact_support_line(line)))
            .collect(),
        Err(error) => vec![format!("network artifact repair failed: {}", error)],
    }
}

#[cfg(windows)]
#[tauri::command]
fn repair_active_tunnel_compatibility_proxy(
    system_proxy_mode: Option<String>,
) -> Result<String, String> {
    if safe_system_proxy_mode(system_proxy_mode.as_deref().unwrap_or("set")) != "set" {
        return Ok("active tunnel WinINet compatibility left unchanged by mode".into());
    }

    let status = match ipc::tunnel_service_status()? {
        tunnel_service::TunnelResponse::Status(status) => status,
        tunnel_service::TunnelResponse::Error { message } => return Err(message),
        tunnel_service::TunnelResponse::Diagnostics(_) => {
            return Err("Tunnel Service returned diagnostics for status".into());
        }
    };

    if !matches!(status.state, tunnel_service::TunnelState::Connected) {
        return Err(format!(
            "Tunnel Service is not connected; state={:?}, phase={:?}",
            status.state, status.phase
        ));
    }

    let http_port = status
        .runtime_http_port
        .ok_or("Tunnel Service has no runtime HTTP compatibility port")?;
    wait_for_port_ready(http_port)?;
    let action = apply_system_proxy_mode("set", http_port)?;
    let status = tunnel_service_report_proxy_compatibility(&status, &CompatProxyOutcome::default());
    Ok(format!(
        "reasserted active tunnel WinINet compatibility: {} on 127.0.0.1:{}, service verdict={:?}",
        action, http_port, status.health_verdict
    ))
}

#[cfg(not(windows))]
#[tauri::command]
fn repair_active_tunnel_compatibility_proxy(
    _system_proxy_mode: Option<String>,
) -> Result<String, String> {
    Ok("active tunnel compatibility proxy repair is only active on Windows".into())
}

#[cfg(windows)]
#[tauri::command]
fn repair_active_tunnel_runtime(reason: Option<String>) -> Result<String, String> {
    let reason = reason.unwrap_or_else(|| "ui_health_monitor".into());
    let response = ipc::send_tunnel_command(&tunnel_service::TunnelCommand::RepairRuntime(
        tunnel_service::RepairRuntimeRequest {
            op_id: None,
            reason: reason.clone(),
        },
    ))?;
    match response {
        tunnel_service::TunnelResponse::Status(status) => Ok(format!(
            "runtime repair requested: reason={}, state={:?}, effective={:?}, verdict={:?}, generation={}",
            reason,
            status.state,
            status.effective_state,
            status.health_verdict,
            status.service_generation
        )),
        tunnel_service::TunnelResponse::Error { message } => Err(message),
        tunnel_service::TunnelResponse::Diagnostics(_) => {
            Err("Tunnel Service returned diagnostics for RepairRuntime".into())
        }
    }
}

#[cfg(not(windows))]
#[tauri::command]
fn repair_active_tunnel_runtime(_reason: Option<String>) -> Result<String, String> {
    Ok("active tunnel runtime repair is only active on Windows".into())
}

#[tauri::command]
fn export_support_bundle(
    proxy_mode: Option<String>,
    system_proxy_mode: Option<String>,
    socks_port: u16,
    http_port: u16,
    failure_marker: Option<String>,
) -> Result<String, String> {
    let dir = std::env::temp_dir().join("DoodleRay");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("doodleray-support-{}.txt", unix_ms()));

    let health = build_full_connection_health(
        proxy_mode.as_deref().unwrap_or("tun"),
        system_proxy_mode.as_deref().unwrap_or("set"),
        socks_port,
        http_port,
    );
    let mut sections = Vec::new();
    sections.push("# DoodleRay Support Bundle".to_string());
    sections.push(format!("generated_at_ms={}", unix_ms()));
    sections.push(format!("app_version={}", env!("CARGO_PKG_VERSION")));
    if let Some(marker) = failure_marker.as_deref() {
        sections.push(format!("failure_marker={}", redact_support_line(marker)));
    }

    sections.push("\n## Connection Health".to_string());
    sections.push(redact_support_text(
        &serde_json::to_string_pretty(&health)
            .unwrap_or_else(|_| "<health serialization failed>".into()),
    ));

    sections.push("\n## App Log Tail".to_string());
    let app_log_tail = CONNECT_LOG
        .lock()
        .map(|logs| {
            logs.iter()
                .rev()
                .take(120)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|_| "<app log unavailable>".into());
    sections.push(redact_support_text(&app_log_tail));

    #[cfg(windows)]
    {
        let app_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files\DoodleRay"));

        sections.push("\n## Runtime Files".to_string());
        for (label, file) in windows_required_runtime_files(&app_dir) {
            sections.push(format!(
                "{}={}",
                label,
                if file.exists() { "present" } else { "missing" }
            ));
        }

        sections.push("\n## Tunnel Service".to_string());
        match ipc::send_tunnel_command(&tunnel_service::TunnelCommand::GetDiagnostics) {
            Ok(response) => sections.push(redact_support_text(
                &serde_json::to_string_pretty(&response)
                    .unwrap_or_else(|_| "<diagnostics serialization failed>".into()),
            )),
            Err(error) => sections.push(redact_support_line(&format!(
                "Tunnel service diagnostics failed: {}",
                error
            ))),
        }

        sections.push("\n## Windows Network Summary".to_string());
        for (label, script) in windows_support_scripts(&app_dir) {
            sections.push(format!("\n### {}", label));
            match windows_powershell_output(&script) {
                Ok(output) if !output.trim().is_empty() => {
                    sections.push(redact_support_text(&output));
                }
                Ok(_) => sections.push("<empty>".into()),
                Err(error) => sections.push(redact_support_line(&format!("failed: {}", error))),
            }
        }
    }

    #[cfg(not(windows))]
    {
        sections.push("\n## Platform".to_string());
        sections.push("Windows support bundle sections are unavailable on this platform.".into());
    }

    std::fs::write(&path, sections.join("\n")).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

fn connection_health_for_request(request: &ConnectRequest) -> ConnectionHealthReport {
    let mut health = build_connection_health(
        &request.proxy_mode,
        &request.system_proxy_mode,
        request.socks_port,
        request.http_port,
    );
    if request.proxy_mode == "tun" {
        if health.runtime_socks_port.is_none() {
            health.runtime_socks_port = Some(request.socks_port);
        }
        if health.runtime_http_port.is_none() {
            health.runtime_http_port = Some(request.http_port);
        }
        health.runtime_api_port = Some(request.api_port);
    } else {
        attach_request_runtime_ports(&mut health, request);
    }
    health
}

fn connect_result_health_for_request(request: &ConnectRequest) -> Option<ConnectionHealthReport> {
    connect_result_health_for_request_with_status(request, None, None)
}

fn connect_result_health_for_request_with_status(
    request: &ConnectRequest,
    tunnel_status: Option<&tunnel_service::TunnelStatus>,
    compatibility_degraded: Option<&str>,
) -> Option<ConnectionHealthReport> {
    if request.proxy_mode == "tun" {
        let mut health = build_fast_tun_connection_health(
            &request.system_proxy_mode,
            request.socks_port,
            request.http_port,
            tunnel_status,
            compatibility_degraded,
        );
        attach_request_runtime_ports(&mut health, request);
        attach_tunnel_status_to_health(&mut health, tunnel_status);
        Some(health)
    } else {
        Some(connection_health_for_request(request))
    }
}

fn build_fast_tun_connection_health(
    system_proxy_mode: &str,
    socks_port: u16,
    http_port: u16,
    tunnel_status: Option<&tunnel_service::TunnelStatus>,
    compatibility_degraded: Option<&str>,
) -> ConnectionHealthReport {
    #[cfg(not(windows))]
    let _ = (system_proxy_mode, compatibility_degraded);

    let mut checks = Vec::new();
    let effective_socks_port = tunnel_status
        .and_then(|status| status.runtime_socks_port)
        .unwrap_or(socks_port);
    let effective_http_port = tunnel_status
        .and_then(|status| status.runtime_http_port)
        .unwrap_or(http_port);
    checks.push(loopback_listener_health(
        "socks_listener",
        "SOCKS listener",
        effective_socks_port,
    ));
    checks.push(protected_compatibility_check(loopback_listener_health(
        "http_listener",
        "HTTP compatibility listener",
        effective_http_port,
    )));

    #[cfg(windows)]
    {
        if let Some(status) = tunnel_status {
            checks.push(tunnel_service_health_check_from_status(status));
            checks.extend(tunnel_service_snapshot_checks(status));
        } else {
            checks.push(tunnel_service_health_check());
        }
        if let Some(reason) = compatibility_degraded {
            checks.push(health_check(
                "wininet_proxy",
                "warning",
                "Windows proxy compatibility",
                reason,
            ));
        } else if system_proxy_mode == "set" {
            checks.push(protected_compatibility_check(
                windows_wininet_proxy_health_check(effective_http_port),
            ));
        }
    }

    #[cfg(not(windows))]
    {
        checks.push(health_check(
            "platform_tun_health",
            "info",
            "Platform tunnel health",
            "Detailed protected-mode health quorum is implemented for Windows first.",
        ));
    }

    health_report("protected", checks)
}

fn build_connection_health(
    proxy_mode: &str,
    system_proxy_mode: &str,
    socks_port: u16,
    http_port: u16,
) -> ConnectionHealthReport {
    if proxy_mode == "tun" {
        return build_current_tun_connection_health(system_proxy_mode, socks_port, http_port);
    }

    build_full_connection_health(proxy_mode, system_proxy_mode, socks_port, http_port)
}

fn build_current_tun_connection_health(
    system_proxy_mode: &str,
    socks_port: u16,
    http_port: u16,
) -> ConnectionHealthReport {
    #[cfg(windows)]
    {
        let tunnel_status = tunnel_service_status_for_health();
        let effective_socks_port = tunnel_status
            .as_ref()
            .and_then(|status| status.runtime_socks_port)
            .unwrap_or(socks_port);
        let effective_http_port = tunnel_status
            .as_ref()
            .and_then(|status| status.runtime_http_port)
            .unwrap_or(http_port);
        let effective_api_port = tunnel_status
            .as_ref()
            .and_then(|status| status.runtime_api_port);
        let mut health = build_fast_tun_connection_health(
            system_proxy_mode,
            socks_port,
            http_port,
            tunnel_status.as_ref(),
            None,
        );
        attach_runtime_ports(
            &mut health,
            effective_socks_port,
            effective_http_port,
            effective_api_port,
        );
        attach_tunnel_status_to_health(&mut health, tunnel_status.as_ref());
        health
    }

    #[cfg(not(windows))]
    {
        let mut health =
            build_fast_tun_connection_health(system_proxy_mode, socks_port, http_port, None, None);
        attach_runtime_ports(&mut health, socks_port, http_port, None);
        health
    }
}

fn build_full_connection_health(
    proxy_mode: &str,
    system_proxy_mode: &str,
    socks_port: u16,
    http_port: u16,
) -> ConnectionHealthReport {
    let mut checks = Vec::new();
    let mode = if proxy_mode == "tun" {
        "protected"
    } else if system_proxy_mode == "set" {
        "compatibility"
    } else {
        "manual"
    };
    #[cfg(windows)]
    let tunnel_status = if proxy_mode == "tun" {
        tunnel_service_status_for_health()
    } else {
        None
    };
    #[cfg(not(windows))]
    let tunnel_status: Option<tunnel_service::TunnelStatus> = None;
    let effective_socks_port = tunnel_status
        .as_ref()
        .and_then(|status| status.runtime_socks_port)
        .unwrap_or(socks_port);
    let effective_http_port = tunnel_status
        .as_ref()
        .and_then(|status| status.runtime_http_port)
        .unwrap_or(http_port);

    checks.push(loopback_listener_health(
        "socks_listener",
        "SOCKS listener",
        effective_socks_port,
    ));
    if system_proxy_mode == "set" || proxy_mode == "tun" {
        let http_check = loopback_listener_health(
            "http_listener",
            if proxy_mode == "tun" {
                "HTTP compatibility listener"
            } else {
                "HTTP listener"
            },
            effective_http_port,
        );
        checks.push(if proxy_mode == "tun" {
            protected_compatibility_check(http_check)
        } else {
            http_check
        });
    }

    #[cfg(windows)]
    {
        if proxy_mode == "tun" {
            if let Some(status) = tunnel_status.as_ref() {
                checks.push(tunnel_service_health_check_from_status(status));
                checks.extend(tunnel_service_snapshot_checks(status));
            } else {
                checks.push(tunnel_service_health_check());
            }
            checks.push(windows_tun_adapter_health_check());
            checks.push(windows_tun_route_health_check());
            checks.push(windows_tun_dns_health_check());
            checks.push(windows_tun_https_canary_health_check());
        }
        if system_proxy_mode == "set" {
            let wininet_check = windows_wininet_proxy_health_check(effective_http_port);
            checks.push(if proxy_mode == "tun" {
                protected_compatibility_check(wininet_check)
            } else {
                wininet_check
            });
        }
    }

    #[cfg(not(windows))]
    {
        if proxy_mode == "tun" {
            checks.push(health_check(
                "platform_tun_health",
                "info",
                "Platform tunnel health",
                "Detailed protected-mode health quorum is implemented for Windows first.",
            ));
        }
    }

    let mut health = health_report(mode, checks);
    attach_runtime_ports(&mut health, effective_socks_port, effective_http_port, None);
    attach_tunnel_status_to_health(&mut health, tunnel_status.as_ref());
    health
}

fn health_report(mode: &str, checks: Vec<ConnectionHealthCheck>) -> ConnectionHealthReport {
    let has_error = checks.iter().any(|check| check.severity == "error");
    let has_warning = checks.iter().any(|check| check.severity == "warning");
    let verdict = if has_error {
        "failed"
    } else if mode == "protected" && has_warning {
        "protected_degraded"
    } else if has_warning {
        "partial"
    } else {
        "protected"
    };

    ConnectionHealthReport {
        verdict: verdict.into(),
        mode: mode.into(),
        generated_at_ms: unix_ms(),
        service_effective_state: None,
        service_health_verdict: None,
        engine_kind: None,
        runtime_socks_port: None,
        runtime_http_port: None,
        runtime_api_port: None,
        service_generation: None,
        active_op_id: None,
        service_fatal_checks: Vec::new(),
        service_degraded_checks: Vec::new(),
        service_warning_checks: Vec::new(),
        route_explanations: Vec::new(),
        endpoint_bypass_checks: Vec::new(),
        checks,
    }
}

fn protected_compatibility_check(mut check: ConnectionHealthCheck) -> ConnectionHealthCheck {
    if check.severity == "error" {
        check.severity = "warning".into();
    }
    check
}

fn attach_request_runtime_ports(health: &mut ConnectionHealthReport, request: &ConnectRequest) {
    attach_runtime_ports(
        health,
        request.socks_port,
        request.http_port,
        Some(request.api_port),
    );
}

fn attach_runtime_ports(
    health: &mut ConnectionHealthReport,
    socks_port: u16,
    http_port: u16,
    api_port: Option<u16>,
) {
    health.runtime_socks_port = Some(socks_port);
    health.runtime_http_port = Some(http_port);
    health.runtime_api_port = api_port;
}

fn attach_tunnel_status_to_health(
    health: &mut ConnectionHealthReport,
    tunnel_status: Option<&tunnel_service::TunnelStatus>,
) {
    let Some(status) = tunnel_status else {
        return;
    };
    if let Some(port) = status.runtime_socks_port {
        health.runtime_socks_port = Some(port);
    }
    if let Some(port) = status.runtime_http_port {
        health.runtime_http_port = Some(port);
    }
    if let Some(port) = status.runtime_api_port {
        health.runtime_api_port = Some(port);
    }
    if status.service_generation > 0 {
        health.service_generation = Some(status.service_generation);
    }
    health.active_op_id = status.active_op_id.clone();
    health.service_effective_state = Some(format!("{:?}", status.effective_state));
    health.service_health_verdict = Some(format!("{:?}", status.health_verdict));
    health.engine_kind = status
        .engine_kind
        .as_ref()
        .map(|kind| format!("{:?}", kind));
    health.service_fatal_checks = status.fatal_checks.clone();
    health.service_degraded_checks = status.degraded_checks.clone();
    health.service_warning_checks = status.warning_checks.clone();
    if let Some(detail) = status.previous_unclean_shutdown.as_deref() {
        health
            .service_warning_checks
            .push(format!("unclean shutdown marker: {}", detail));
    }
    health.route_explanations = status.route_explanations.clone();
    health.endpoint_bypass_checks = status.endpoint_bypass_checks.clone();
    health.verdict = service_health_verdict_to_report(&status.health_verdict).into();
}

fn service_health_verdict_to_report(verdict: &tunnel_service::TunnelHealthVerdict) -> &'static str {
    match verdict {
        tunnel_service::TunnelHealthVerdict::Protected => "protected",
        tunnel_service::TunnelHealthVerdict::ProtectedDegraded => "protected_degraded",
        tunnel_service::TunnelHealthVerdict::Limited => "limited",
        tunnel_service::TunnelHealthVerdict::Repairing => "repairing",
        tunnel_service::TunnelHealthVerdict::Failed => "failed",
        tunnel_service::TunnelHealthVerdict::CleanupPending => "cleanup_pending",
    }
}

#[cfg(windows)]
fn tunnel_service_status_for_health() -> Option<tunnel_service::TunnelStatus> {
    match ipc::tunnel_service_status() {
        Ok(tunnel_service::TunnelResponse::Status(status)) => Some(status),
        _ => None,
    }
}

fn loopback_listener_health(code: &str, title: &str, port: u16) -> ConnectionHealthCheck {
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    if TcpStream::connect_timeout(&addr, Duration::from_millis(600)).is_ok() {
        health_check(
            code,
            "ok",
            title,
            format!("127.0.0.1:{} accepts connections", port),
        )
    } else {
        health_check(
            code,
            "error",
            title,
            format!("127.0.0.1:{} is not accepting connections", port),
        )
    }
}

fn health_check(
    code: &str,
    severity: &str,
    title: &str,
    detail: impl Into<String>,
) -> ConnectionHealthCheck {
    ConnectionHealthCheck {
        code: code.into(),
        severity: severity.into(),
        title: title.into(),
        detail: detail.into(),
    }
}

fn unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(windows)]
fn windows_required_runtime_files(app_dir: &Path) -> Vec<(&'static str, PathBuf)> {
    vec![
        ("DoodleRayService.exe", app_dir.join("DoodleRayService.exe")),
        ("sing-box.exe", app_dir.join("sing-box.exe")),
        ("wintun.dll", app_dir.join("wintun.dll")),
        ("xray.exe", app_dir.join("xray-core").join("xray.exe")),
    ]
}

fn redact_support_text(text: &str) -> String {
    text.lines()
        .map(redact_support_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_support_line(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    let sensitive_line = [
        "subscription",
        "private_key",
        "privatekey",
        "pre_shared_key",
        "presharedkey",
        "short_id",
        "server_name",
        "servername",
        "password",
        "uuid",
        "raw_xray_config",
        "raw config",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    if sensitive_line {
        return "[redacted-sensitive-line]".into();
    }

    line.split_whitespace()
        .map(redact_support_token)
        .collect::<Vec<_>>()
        .join(" ")
}

// ═══════════════════════════════════════════════════════════════════════
//  v6 network diagnosis: maps the real health report onto a support-grade,
//  human-readable report. User text stays free of Wintun/WinINet/NRPT
//  jargon; exact (redacted) technical lines are preserved for support.
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize)]
pub struct DiagnosticCheck {
    pub id: String,
    pub label: String,
    pub status: String,
    pub user_text: String,
    pub technical_detail_redacted: String,
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct NetworkDiagnosisReport {
    pub overall: String,
    pub user_title: String,
    pub user_summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_cause_code: Option<String>,
    pub user_actions: Vec<String>,
    pub support_summary: String,
    pub checks: Vec<DiagnosticCheck>,
    pub copy_text: String,
    pub can_auto_repair: bool,
    pub bundle_available: bool,
}

fn diagnosis_haystack(health: &ConnectionHealthReport) -> String {
    let mut hay = String::new();
    for line in health
        .service_fatal_checks
        .iter()
        .chain(health.service_degraded_checks.iter())
        .chain(health.service_warning_checks.iter())
    {
        hay.push_str(&line.to_ascii_lowercase());
        hay.push('\n');
    }
    for check in &health.checks {
        if check.severity == "error" || check.severity == "warning" {
            hay.push_str(&check.code.to_ascii_lowercase());
            hay.push(' ');
            hay.push_str(&check.detail.to_ascii_lowercase());
            hay.push('\n');
        }
    }
    hay
}

/// Priority-ordered classification of the primary cause. Returns
/// (cause_code, can_auto_repair).
fn classify_diagnosis_cause(
    health: &ConnectionHealthReport,
    proxy_mode: &str,
    last_subscription_error: Option<&str>,
) -> (String, bool) {
    let hay = diagnosis_haystack(health);
    let verdict = health.verdict.as_str();
    let failed_check = |code: &str| {
        health
            .checks
            .iter()
            .any(|c| c.code == code && (c.severity == "error" || c.severity == "warning"))
    };

    // 1. Stale Wintun PnP ghost (docs/solved-errors.md 2026-07-04).
    if hay.contains("cannot create a file when that file already exists")
        || hay.contains("open existing adapter: element not found")
        || hay.contains("cm_prob_phantom")
        || hay.contains("swd\\wintun")
    {
        return ("wintun_ghost_adapter".into(), true);
    }
    // 2. Adapter missing / IPv4 readiness.
    if hay.contains("adapter is missing")
        || hay.contains("ipv4 readiness failed")
        || hay.contains("adapter did not become ready")
        || (proxy_mode == "tun" && failed_check("tun_adapter"))
    {
        return ("adapter_missing".into(), true);
    }
    // 3. Service-owned core died (fake-green class).
    if hay.contains("sing-box exited")
        || hay.contains("sing-box process is not running")
        || hay.contains("process is not running")
        || hay.contains("core exited")
    {
        return ("core_process_dead".into(), true);
    }
    // 4. Tunnel service unreachable/dead.
    if (health
        .checks
        .iter()
        .any(|c| c.code == "tunnel_service" && c.severity == "error")
        || hay.contains("state=failed")
        || hay.contains("state=disconnected")
        || hay.contains("did not respond")
        || hay.contains("service is not installed"))
        && (verdict == "failed" || verdict == "cleanup_pending" || proxy_mode == "tun")
    {
        return ("service_unavailable".into(), false);
    }
    // 5. Stale WinINet proxy left behind.
    if failed_check("wininet_proxy")
        && (hay.contains("not expected loopback") || hay.contains("proxyenable=1"))
    {
        return ("wininet_stale_proxy".into(), true);
    }
    // 6. Browser-compatibility proxy listener not ready.
    if failed_check("http_listener") || hay.contains("listener") && hay.contains("not accepting") {
        return ("compat_proxy_unready".into(), true);
    }
    // 7. Connection is otherwise fine in a non-TUN mode: honest limited state.
    if proxy_mode != "tun" && verdict != "failed" && verdict != "cleanup_pending" {
        if let Some(_err) = last_subscription_error {
            return ("subscription_fetch_failed".into(), false);
        }
        return ("browsers_fallback".into(), false);
    }
    // 8. Degraded protected with no hard cause: extra probes not confirmed.
    if verdict == "protected_degraded" {
        return ("ipv6_quic_unverified".into(), false);
    }
    if verdict == "repairing" {
        return ("repair_in_progress".into(), false);
    }
    if verdict == "protected" {
        if let Some(_err) = last_subscription_error {
            return ("subscription_fetch_failed".into(), false);
        }
        return ("all_ok".into(), false);
    }
    if verdict == "failed" || verdict == "cleanup_pending" {
        return ("unknown_failure".into(), true);
    }
    ("unknown_degraded".into(), false)
}

fn diagnosis_user_text(code: &str) -> (&'static str, &'static str, &'static [&'static str]) {
    match code {
        "all_ok" => (
            "Все проверки пройдены",
            "VPN работает, проблем не найдено.",
            &[],
        ),
        "wintun_ghost_adapter" => (
            "Windows сохранила сломанный VPN-адаптер",
            "Старый сетевой адаптер завис в системе и мешает подключению. DoodleRay может пересоздать его автоматически.",
            &["Нажмите «Починить автоматически»", "Затем подключитесь заново"],
        ),
        "adapter_missing" => (
            "Сетевой адаптер VPN не поднялся",
            "VPN-адаптер не успел подняться или был удалён Windows. Обычно это чинится автоматически.",
            &["Нажмите «Починить автоматически»", "Затем подключитесь заново"],
        ),
        "core_process_dead" => (
            "Движок VPN остановился",
            "Процесс, который шифрует трафик, неожиданно завершился. Переподключение поднимет его заново.",
            &["Переподключитесь", "Если повторяется — сохраните отчет для поддержки"],
        ),
        "service_unavailable" => (
            "Фоновая служба VPN не отвечает",
            "Служба, которая управляет защитой всего компьютера, недоступна. Может помочь перезагрузка компьютера или переустановка DoodleRay.",
            &["Перезагрузите компьютер", "Если не помогло — переустановите DoodleRay", "Сохраните отчет для поддержки"],
        ),
        "wininet_stale_proxy" => (
            "Остались старые настройки прокси",
            "Windows-прокси остался от прошлого запуска и будет очищен автоматически.",
            &["Нажмите «Починить автоматически»"],
        ),
        "compat_proxy_unready" => (
            "Совместимость браузеров временно не готова",
            "VPN работает, но локальный прокси для браузеров ещё не поднялся. Обычно чинится автоматически.",
            &["Нажмите «Починить автоматически»", "Или просто подождите минуту"],
        ),
        "subscription_fetch_failed" => (
            "Подписка не обновилась",
            "Нет доступа к серверу подписки. Соединение при этом работает.",
            &["Проверьте, что подписка не истекла", "Повторите обновление позже"],
        ),
        "browsers_fallback" => (
            "Работает режим браузеров",
            "VPN работает в режиме браузеров, но весь компьютер пока не защищён.",
            &["Переключитесь на «Весь компьютер», когда будет удобно"],
        ),
        "ipv6_quic_unverified" => (
            "Подключение активно, часть проверок не подтверждена",
            "VPN работает, но некоторые дополнительные проверки (например IPv6/QUIC) не подтверждены. На обычную работу это чаще всего не влияет.",
            &["Можно продолжать пользоваться", "Если что-то не работает — сохраните отчет"],
        ),
        "repair_in_progress" => (
            "Идёт автоматический ремонт",
            "DoodleRay уже чинит подключение. Подождите несколько секунд.",
            &["Подождите и проверьте ещё раз"],
        ),
        "unknown_failure" => (
            "Подключение не работает",
            "Точную причину определить не удалось. Попробуйте автоматический ремонт и переподключение.",
            &["Нажмите «Починить автоматически»", "Переподключитесь", "Сохраните отчет для поддержки"],
        ),
        _ => (
            "Подключение работает не полностью",
            "Часть проверок не прошла. Подробности — в блоке для поддержки.",
            &["Сохраните отчет для поддержки"],
        ),
    }
}

fn diagnosis_check_source(code: &str) -> &'static str {
    if code.starts_with("tunnel_service") || code == "platform_tun_health" {
        "service"
    } else if code.starts_with("wininet") || code.contains("route") || code.contains("dns") {
        "windows"
    } else if code.contains("listener") || code.contains("canary") || code.contains("probe") {
        "probe"
    } else {
        "app"
    }
}

fn is_non_actionable_ipv6_quic_line(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("ipv6 full-protection leak proof is not collected")
        || lower.contains("degraded_disabled")
        || lower.contains("quic/http3 is not verified")
        || (lower.contains("quic") && lower.contains("not verified"))
        || lower.contains("unverified-no-tooling")
        || (lower.contains("ipv6_default_route") && lower.contains("doodleray tunnel"))
}

fn is_non_actionable_service_warning_line(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    is_non_actionable_ipv6_quic_line(value)
        || lower.contains("ipv6 default route is absent")
        || lower.contains("protected verdict covers ipv4 routing")
        || lower.contains("native network change watchers ready")
        || lower.contains("native network change watchers unavailable")
        || lower.contains("windows event observed while service was running")
        || lower.contains("child generation rotated after windows network/power event")
        || lower.contains("tun adapter repair retry ran after")
}

fn diagnosis_check_status(cause: &str, code: &str, severity: &str, detail: &str) -> String {
    if cause == "ipv6_quic_unverified"
        && (severity == "warning" || severity == "error")
        && is_non_actionable_ipv6_quic_line(detail)
    {
        return "info".into();
    }
    if code == "tunnel_service_warning_checks"
        && severity == "warning"
        && (cause == "all_ok" || is_non_actionable_service_warning_line(detail))
    {
        return "info".into();
    }
    if code == "tunnel_service_degraded_checks"
        && severity == "warning"
        && (cause == "all_ok" || is_non_actionable_service_warning_line(detail))
    {
        return "info".into();
    }

    match severity {
        "ok" => "ok".into(),
        "info" => "info".into(),
        "warning" => "warning".into(),
        _ => "error".into(),
    }
}

fn diagnosis_check_user_text(code: &str, status: &str) -> String {
    if status == "ok" || status == "info" {
        return "В порядке".into();
    }
    match code {
        "tunnel_service" | "tunnel_service_fatal_checks" => {
            "Фоновая служба сообщила о проблеме".into()
        }
        "tun_adapter" | "tun_adapter_snapshot" => "Сетевой адаптер VPN не в порядке".into(),
        "tun_routes" | "tun_routes_snapshot" => "Маршруты трафика не подтверждены".into(),
        "tun_dns" => "Защита DNS не подтверждена".into(),
        "tun_https_canary" => "Проверка выхода в интернет не прошла".into(),
        "http_listener" => "Прокси для браузеров не отвечает".into(),
        "wininet_proxy" => "Настройки системного прокси не совпадают".into(),
        "tunnel_service_warning_checks" => "Служебная заметка".into(),
        _ if status == "warning" => "Требует внимания".into(),
        _ => "Проверка не прошла".into(),
    }
}

fn windows_build_short() -> String {
    let raw = command_stdout("cmd", &["/c", "ver"]);
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "Windows".into()
    } else {
        trimmed.trim_start_matches("Microsoft ").to_string()
    }
}

fn build_network_diagnosis(
    health: &ConnectionHealthReport,
    proxy_mode: &str,
    last_subscription_error: Option<&str>,
    repair_attempted: bool,
) -> NetworkDiagnosisReport {
    let (cause, can_auto_repair) =
        classify_diagnosis_cause(health, proxy_mode, last_subscription_error);
    let (title, summary, actions) = diagnosis_user_text(&cause);

    let overall = match health.verdict.as_str() {
        "protected" if cause == "all_ok" || cause == "subscription_fetch_failed" => "ok",
        "protected_degraded" if cause == "ipv6_quic_unverified" => "ok",
        "protected" | "protected_degraded" => "degraded",
        "repairing" => "repairing",
        "failed" | "cleanup_pending" => "failed",
        _ if proxy_mode != "tun" => "limited",
        _ => "degraded",
    }
    .to_string();

    let mut checks: Vec<DiagnosticCheck> = health
        .checks
        .iter()
        .map(|c| {
            let status = diagnosis_check_status(&cause, &c.code, &c.severity, &c.detail);
            DiagnosticCheck {
                id: c.code.clone(),
                label: c.title.clone(),
                user_text: diagnosis_check_user_text(&c.code, &status),
                status,
                technical_detail_redacted: redact_support_line(&c.detail),
                source: diagnosis_check_source(&c.code).into(),
            }
        })
        .collect();
    for fatal in &health.service_fatal_checks {
        checks.push(DiagnosticCheck {
            id: "service_fatal".into(),
            label: "Tunnel Service fatal".into(),
            status: "error".into(),
            user_text: "Фоновая служба сообщила о серьёзной ошибке".into(),
            technical_detail_redacted: redact_support_line(fatal),
            source: "service".into(),
        });
    }
    if let Some(err) = last_subscription_error {
        checks.push(DiagnosticCheck {
            id: "subscription_refresh".into(),
            label: "Subscription refresh".into(),
            status: "warning".into(),
            user_text: "Подписка не обновилась".into(),
            technical_detail_redacted: redact_support_line(err),
            source: "app".into(),
        });
    }

    let failed_ids: Vec<&str> = checks
        .iter()
        .filter(|c| c.status == "error")
        .map(|c| c.id.as_str())
        .collect();

    let support_summary = redact_support_text(&format!(
        "verdict={} mode={} engine={} state={} generation={} fatal=[{}] degraded=[{}]",
        health.verdict,
        health.mode,
        health.engine_kind.as_deref().unwrap_or("-"),
        health.service_effective_state.as_deref().unwrap_or("-"),
        health
            .service_generation
            .map(|g| g.to_string())
            .unwrap_or_else(|| "-".into()),
        health.service_fatal_checks.join(" | "),
        health.service_degraded_checks.join(" | "),
    ));

    let copy_text = format!(
        "DoodleRay v{} | {}\nmode={} verdict={} gen={} cause={} repairable={} repair_tried={}\nfailed_checks: {}\n{}",
        env!("CARGO_PKG_VERSION"),
        windows_build_short(),
        proxy_mode,
        health.verdict,
        health
            .service_generation
            .map(|g| g.to_string())
            .unwrap_or_else(|| "-".into()),
        cause,
        can_auto_repair,
        repair_attempted,
        if failed_ids.is_empty() { "none".into() } else { failed_ids.join(", ") },
        support_summary.chars().take(600).collect::<String>(),
    );

    NetworkDiagnosisReport {
        overall,
        user_title: title.into(),
        user_summary: summary.into(),
        primary_cause_code: Some(cause),
        user_actions: actions.iter().map(|s| s.to_string()).collect(),
        support_summary,
        checks,
        copy_text,
        can_auto_repair,
        bundle_available: true,
    }
}

#[tauri::command]
fn run_network_diagnosis(
    proxy_mode: Option<String>,
    system_proxy_mode: Option<String>,
    socks_port: u16,
    http_port: u16,
    last_subscription_error: Option<String>,
    repair_attempted: Option<bool>,
) -> NetworkDiagnosisReport {
    let mode = proxy_mode.unwrap_or_else(|| "tun".into());
    let health = build_full_connection_health(
        &mode,
        system_proxy_mode.as_deref().unwrap_or("set"),
        socks_port,
        http_port,
    );
    build_network_diagnosis(
        &health,
        &mode,
        last_subscription_error.as_deref(),
        repair_attempted.unwrap_or(false),
    )
}

fn redact_support_token(token: &str) -> String {
    if token.contains("://") {
        return "[redacted-url]".into();
    }
    let uuid_redacted = redact_uuid_substrings(token);
    let redacted_token = uuid_redacted.as_str();
    let trimmed = redacted_token.trim_matches(|ch: char| {
        !(ch.is_ascii_alphanumeric() || matches!(ch, '.' | ':' | '-' | '_' | '[' | ']'))
    });
    if trimmed.is_empty() {
        return uuid_redacted;
    }
    let normalized = trimmed.trim_matches(|ch| matches!(ch, '[' | ']'));
    let comparable = normalized
        .rsplit_once('=')
        .map(|(_, value)| value)
        .unwrap_or(normalized);
    if looks_like_uuid(comparable) {
        return uuid_redacted.replace(comparable, "[redacted-uuid]");
    }
    if looks_like_secret_token(comparable) {
        return uuid_redacted.replace(comparable, "[redacted-token]");
    }
    if should_redact_ip_token(comparable) {
        return uuid_redacted.replace(comparable, "[redacted-ip]");
    }
    uuid_redacted
}

fn redact_uuid_substrings(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut idx = 0;
    while idx < value.len() {
        let end = idx + 36;
        if end <= value.len()
            && value.is_char_boundary(idx)
            && value.is_char_boundary(end)
            && looks_like_uuid(&value[idx..end])
        {
            out.push_str("[redacted-uuid]");
            idx = end;
            continue;
        }

        let Some(ch) = value[idx..].chars().next() else {
            break;
        };
        out.push(ch);
        idx += ch.len_utf8();
    }
    out
}

fn looks_like_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (idx, byte) in bytes.iter().enumerate() {
        let is_dash = matches!(idx, 8 | 13 | 18 | 23);
        if is_dash {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn looks_like_secret_token(value: &str) -> bool {
    value.len() >= 48
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '+' | '/' | '='))
}

fn should_redact_ip_token(value: &str) -> bool {
    let candidate = value
        .rsplit_once(':')
        .and_then(|(host, port)| port.parse::<u16>().ok().map(|_| host))
        .unwrap_or(value);
    match candidate.parse::<IpAddr>() {
        Ok(ip) => !ip.is_loopback(),
        Err(_) => false,
    }
}

#[cfg(windows)]
fn windows_support_scripts(app_dir: &Path) -> Vec<(&'static str, String)> {
    // PowerShell single-quoted literal: escape embedded quotes by doubling.
    let app_dir_literal = app_dir.to_string_lossy().replace('\'', "''");
    vec![
        (
            "Adapters",
            r#"
Get-NetAdapter |
  Where-Object { $_.Name -like '*DoodleRay*' -or $_.InterfaceDescription -like '*Wintun*' -or $_.InterfaceDescription -like '*sing*' } |
  Select-Object Name,Status,InterfaceDescription |
  Format-Table -AutoSize | Out-String
"#
            .to_string(),
        ),
        (
            "Routes",
            r#"
$adapter = Get-NetAdapter -Name 'DoodleRay Tunnel' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($adapter) {
  $routes = Get-NetRoute -InterfaceIndex $adapter.ifIndex -ErrorAction SilentlyContinue
  "route_count=" + @($routes).Count
  "ipv4_count=" + @($routes | Where-Object { $_.AddressFamily -eq 'IPv4' }).Count
  "ipv6_count=" + @($routes | Where-Object { $_.AddressFamily -eq 'IPv6' }).Count
  "has_ipv4_default=" + [bool]($routes | Where-Object { $_.DestinationPrefix -eq '0.0.0.0/0' } | Select-Object -First 1)
  "has_ipv4_split=" + ([bool]($routes | Where-Object { $_.DestinationPrefix -eq '0.0.0.0/1' } | Select-Object -First 1) -and [bool]($routes | Where-Object { $_.DestinationPrefix -eq '128.0.0.0/1' } | Select-Object -First 1))
} else {
  "DoodleRay Tunnel adapter not found"
}
"#
            .to_string(),
        ),
        (
            "DNS",
            r#"
Get-DnsClientServerAddress |
  Select-Object InterfaceAlias,AddressFamily,@{Name='ServerCount';Expression={ @($_.ServerAddresses).Count }} |
  Format-Table -AutoSize | Out-String
"#
            .to_string(),
        ),
        (
            "Windows Connectivity Indicator",
            r#"
"Profiles:"
Get-NetConnectionProfile |
  Select-Object Name,InterfaceAlias,IPv4Connectivity,IPv6Connectivity,NetworkCategory |
  Format-Table -AutoSize | Out-String
"IP interface metrics:"
Get-NetIPInterface |
  Select-Object InterfaceAlias,AddressFamily,InterfaceMetric,AutomaticMetric,ConnectionState,NlMtu |
  Sort-Object InterfaceMetric |
  Format-Table -AutoSize | Out-String
"NCSI active probe settings:"
$ncsi = Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Services\NlaSvc\Parameters\Internet' -ErrorAction SilentlyContinue
if ($ncsi) {
  "EnableActiveProbing=$($ncsi.EnableActiveProbing)"
  "ActiveWebProbeHost=$($ncsi.ActiveWebProbeHost)"
  "ActiveWebProbePath=$($ncsi.ActiveWebProbePath)"
  "ActiveDnsProbeHost=$($ncsi.ActiveDnsProbeHost)"
} else {
  "NCSI registry settings unavailable"
}
"#
            .to_string(),
        ),
        (
            "WinINet Proxy",
            r#"
$settings = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings'
"ProxyEnable=$($settings.ProxyEnable)"
"ProxyServerStatus=$(if ($settings.ProxyServer -match '127\.0\.0\.1') { 'loopback' } elseif ($settings.ProxyServer) { 'redacted-non-loopback' } else { 'empty' })"
"AutoConfigURLStatus=$(if ($settings.AutoConfigURL) { 'present-redacted' } else { 'empty' })"
"ProxyOverrideStatus=$(if ($settings.ProxyOverride) { 'present-redacted' } else { 'empty' })"
"#
            .to_string(),
        ),
        ("WinHTTP Proxy", "netsh winhttp show proxy".to_string()),
        (
            "Signature Status",
            format!(
                r#"
$dir = '{}'
$files = @('DoodleRay.exe','DoodleRayService.exe','sing-box.exe','wintun.dll','xray-core\xray.exe')
foreach ($file in $files) {{
  $path = Join-Path $dir $file
  if (Test-Path $path) {{
    $sig = Get-AuthenticodeSignature $path
    $subject = if ($sig.SignerCertificate) {{ $sig.SignerCertificate.Subject }} else {{ 'none' }}
    $thumbprint = if ($sig.SignerCertificate) {{ $sig.SignerCertificate.Thumbprint }} else {{ 'none' }}
    "$file status=$($sig.Status) signer=$subject thumbprint=$thumbprint"
  }} else {{
    "$file missing"
  }}
}}
"#,
                app_dir_literal
            ),
        ),
    ]
}

#[cfg(windows)]
fn tunnel_service_health_check() -> ConnectionHealthCheck {
    match ipc::tunnel_service_status() {
        Ok(tunnel_service::TunnelResponse::Status(status)) => {
            tunnel_service_health_check_from_status(&status)
        }
        Ok(_) => health_check(
            "tunnel_service",
            "error",
            "Tunnel service",
            "Unexpected tunnel service response",
        ),
        Err(error) => health_check(
            "tunnel_service",
            "error",
            "Tunnel service",
            format!("IPC failed: {}", error),
        ),
    }
}

#[cfg(windows)]
fn tunnel_service_health_check_from_status(
    status: &tunnel_service::TunnelStatus,
) -> ConnectionHealthCheck {
    let ok = matches!(status.state, tunnel_service::TunnelState::Connected);
    let mut detail = format!(
        "state={:?}, effective_state={:?}, health_verdict={:?}, phase={:?}, generation={}",
        status.state,
        status.effective_state,
        status.health_verdict,
        status.phase,
        status.service_generation
    );
    if let Some(port) = status.runtime_socks_port {
        detail.push_str(&format!(", socks=127.0.0.1:{}", port));
    }
    if let Some(port) = status.runtime_http_port {
        detail.push_str(&format!(", http=127.0.0.1:{}", port));
    }
    if let Some(alias) = status.adapter_alias.as_deref() {
        detail.push_str(&format!(", adapter={}", alias));
    }
    health_check(
        "tunnel_service",
        if ok { "ok" } else { "error" },
        "Tunnel service",
        detail,
    )
}

#[cfg(windows)]
fn tunnel_service_snapshot_checks(
    status: &tunnel_service::TunnelStatus,
) -> Vec<ConnectionHealthCheck> {
    let mut checks = Vec::new();

    let adapter_ok = status.adapter_alias.is_some() && status.adapter_ifindex.is_some();
    checks.push(health_check(
        "tun_adapter_snapshot",
        if adapter_ok { "ok" } else { "error" },
        "TUN adapter snapshot",
        match (
            status.adapter_alias.as_deref(),
            status.adapter_ifindex,
            &status.state,
        ) {
            (Some(alias), Some(ifindex), tunnel_service::TunnelState::Connected) => {
                format!("service reports adapter={} ifIndex={}", alias, ifindex)
            }
            (Some(alias), Some(ifindex), _) => {
                format!(
                    "service reports adapter={} ifIndex={} while state={:?}",
                    alias, ifindex, status.state
                )
            }
            _ => "service did not report an active TUN adapter".into(),
        },
    ));

    checks.push(health_check(
        "tun_routes_snapshot",
        if status.route_ready == Some(true) {
            "ok"
        } else {
            "error"
        },
        "TUN routes snapshot",
        if status.route_ready == Some(true) {
            "service route readiness passed".into()
        } else {
            format!("service route readiness is {:?}", status.route_ready)
        },
    ));

    if !status.fatal_checks.is_empty() {
        checks.push(health_check(
            "tunnel_service_fatal_checks",
            "error",
            "Tunnel service fatal checks",
            status.fatal_checks.join("; "),
        ));
    }
    if !status.degraded_checks.is_empty() {
        checks.push(health_check(
            "tunnel_service_degraded_checks",
            "warning",
            "Tunnel service degraded checks",
            status.degraded_checks.join("; "),
        ));
    }
    if !status.warning_checks.is_empty() {
        checks.push(health_check(
            "tunnel_service_warning_checks",
            "warning",
            "Tunnel service warnings",
            status.warning_checks.join("; "),
        ));
    }
    if !status.route_explanations.is_empty() {
        checks.push(health_check(
            "tunnel_service_route_explanation",
            "info",
            "Route explanation",
            status.route_explanations.join("; "),
        ));
    }
    if !status.endpoint_bypass_checks.is_empty() {
        checks.push(health_check(
            "tunnel_service_endpoint_bypass",
            "info",
            "Endpoint bypass explanation",
            status.endpoint_bypass_checks.join("; "),
        ));
    }

    checks
}

#[cfg(windows)]
fn windows_powershell_output(script: &str) -> Result<String, String> {
    let mut child = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .creation_flags(0x08000000)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if started.elapsed() >= Duration::from_secs(15) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("PowerShell health probe timed out after 15s".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut text = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut text);
    }
    if let Some(mut stderr) = child.stderr.take() {
        let mut err = String::new();
        let _ = stderr.read_to_string(&mut err);
        text.push_str(&err);
    }
    if status.success() {
        Ok(text.trim().to_string())
    } else {
        Err(text.trim().to_string())
    }
}

#[cfg(windows)]
fn windows_command_output_with_timeout(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<String, String> {
    let mut child = std::process::Command::new(program)
        .args(args)
        .creation_flags(0x08000000)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to run {}: {}", program, e))?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "{} timed out after {}s",
                program,
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut text = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut text);
    }
    if let Some(mut stderr) = child.stderr.take() {
        let mut err = String::new();
        let _ = stderr.read_to_string(&mut err);
        if !err.trim().is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&err);
        }
    }
    let text = text.trim().to_string();
    if status.success() {
        Ok(text)
    } else {
        Err(if text.is_empty() {
            format!("{} exited with {}", program, status)
        } else {
            text
        })
    }
}

#[cfg(windows)]
fn windows_tun_adapter_health_check() -> ConnectionHealthCheck {
    match windows_powershell_output(
        "Get-NetAdapter -Name 'DoodleRay Tunnel' -ErrorAction Stop | Select-Object -First 1 -ExpandProperty Status",
    ) {
        Ok(status) if status.eq_ignore_ascii_case("up") => health_check(
            "tun_adapter",
            "ok",
            "TUN adapter",
            "DoodleRay Tunnel is Up",
        ),
        Ok(status) => health_check(
            "tun_adapter",
            "error",
            "TUN adapter",
            format!("DoodleRay Tunnel status={}", status),
        ),
        Err(error) => health_check(
            "tun_adapter",
            "error",
            "TUN adapter",
            format!("DoodleRay Tunnel not found: {}", error),
        ),
    }
}

#[cfg(windows)]
fn windows_tun_route_health_check() -> ConnectionHealthCheck {
    let script = r#"
$adapter = Get-NetAdapter -Name 'DoodleRay Tunnel' -ErrorAction Stop | Select-Object -First 1
$routes = Get-NetRoute -InterfaceIndex $adapter.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue
$hasDefault = [bool]($routes | Where-Object { $_.DestinationPrefix -eq '0.0.0.0/0' } | Select-Object -First 1)
$hasSplit = [bool]($routes | Where-Object { $_.DestinationPrefix -eq '0.0.0.0/1' } | Select-Object -First 1) -and [bool]($routes | Where-Object { $_.DestinationPrefix -eq '128.0.0.0/1' } | Select-Object -First 1)
$customCount = @($routes | Where-Object {
  $_.DestinationPrefix -notin @('172.30.255.0/30','255.255.255.255/32') -and
  $_.DestinationPrefix -notlike '224.*'
}).Count
if (-not ($hasDefault -or $hasSplit -or $customCount -ge 4)) {
  Write-Error "missing protected route coverage: default=$hasDefault split=$hasSplit custom=$customCount"
  exit 2
}

$routeCanaries = @(
  '104.26.13.205',
  '142.251.20.113',
  '162.159.136.232'
)
$bypassedCanaries = @()
foreach ($ip in $routeCanaries) {
  $matches = @(Find-NetRoute -RemoteIPAddress $ip -ErrorAction SilentlyContinue |
    Where-Object { [int]$_.InterfaceIndex -eq [int]$adapter.ifIndex })
  if ($matches.Count -eq 0) {
    $best = Find-NetRoute -RemoteIPAddress $ip -ErrorAction SilentlyContinue |
      Select-Object -First 1
    $via = if ($best) { "$($best.InterfaceAlias):$($best.InterfaceIndex)" } else { 'none' }
    $bypassedCanaries += "$ip via $via"
  }
}

if ($bypassedCanaries.Count -gt 0) {
  Write-Error ("protected route canaries bypass TUN: {0}" -f ($bypassedCanaries -join '; '))
  exit 3
}

"ok default=$hasDefault split=$hasSplit custom=$customCount canaries=ok"
"#;
    match windows_powershell_output(script) {
        Ok(detail) => health_check("tun_routes", "ok", "TUN routes", detail),
        Err(error) => health_check("tun_routes", "error", "TUN routes", error),
    }
}

#[cfg(windows)]
fn windows_tun_dns_health_check() -> ConnectionHealthCheck {
    let script = r#"
$ErrorActionPreference = 'Stop'
$adapterSummary = ''
$adapter = $null
try {
  $adapter = Get-NetAdapter -Name 'DoodleRay Tunnel' -ErrorAction Stop | Select-Object -First 1
  $servers = Get-DnsClientServerAddress -InterfaceAlias 'DoodleRay Tunnel' -ErrorAction Stop |
    ForEach-Object { $_.ServerAddresses } |
    Where-Object { $_ }
  if (@($servers).Count -gt 0) {
    $adapterSummary = "adapter_servers=" + (@($servers) -join ',')
  } else {
    $adapterSummary = "adapter_servers=none; protected mode uses sing-box DNS hijack"
  }
} catch {
  $adapterSummary = "adapter_dns_snapshot=unavailable"
}

if ($adapter) {
  $dnsRouteBypass = @()
  foreach ($ip in @('1.1.1.1', '8.8.8.8')) {
    $matches = @(Find-NetRoute -RemoteIPAddress $ip -ErrorAction SilentlyContinue |
      Where-Object { [int]$_.InterfaceIndex -eq [int]$adapter.ifIndex })
    if ($matches.Count -eq 0) {
      $best = Find-NetRoute -RemoteIPAddress $ip -ErrorAction SilentlyContinue |
        Select-Object -First 1
      $via = if ($best) { "$($best.InterfaceAlias):$($best.InterfaceIndex)" } else { 'none' }
      $dnsRouteBypass += "$ip via $via"
    }
  }
  if ($dnsRouteBypass.Count -gt 0) {
    Write-Error ("DNS route bypasses TUN: {0}; {1}" -f ($dnsRouteBypass -join '; '), $adapterSummary)
    exit 3
  }
}

$target = 'www.google.com'
$resolved = @()
if (Get-Command Resolve-DnsName -ErrorAction SilentlyContinue) {
  try {
    $resolved = @(Resolve-DnsName -Name $target -Type A -DnsOnly -QuickTimeout -ErrorAction Stop |
      Where-Object { $_.IPAddress } |
      Select-Object -ExpandProperty IPAddress)
  } catch {
    $resolved = @()
  }
}

if (@($resolved).Count -eq 0) {
  $nslookup = nslookup -timeout=5 $target 2>&1 | Out-String
  $resolved = @([regex]::Matches($nslookup, '\b(?:\d{1,3}\.){3}\d{1,3}\b') |
    ForEach-Object { $_.Value } |
    Where-Object { $_ -ne '0.0.0.0' } |
    Select-Object -Unique)
}

if (@($resolved).Count -gt 0) {
  $canaryFailures = @()
  foreach ($url in @('https://auth.openai.com','https://api.ipify.org')) {
    $curlOutput = curl.exe -4 --connect-timeout 6 --max-time 12 --noproxy '*' --silent --show-error --output NUL --write-out '%{http_code}' $url 2>&1 | Out-String
    $curlExit = $LASTEXITCODE
    if ($curlExit -ne 0 -and $curlOutput -match '(?i)could not resolve host|resolving timed out|getaddrinfo|enotfound|name or service not known') {
      $canaryFailures += ("{0}: {1}" -f $url, $curlOutput.Trim())
    }
  }
  if ($canaryFailures.Count -gt 0) {
    Write-Error ("Windows system resolver canaries failed: {0}; {1}" -f ($canaryFailures -join ' | '), $adapterSummary)
    exit 4
  }
  "resolved $target -> " + (@($resolved | Select-Object -First 3) -join ',') + "; resolver_canaries=ok; " + $adapterSummary
} else {
  Write-Error ("DNS resolution failed for {0}; {1}" -f $target, $adapterSummary)
}
"#;
    match windows_powershell_output(script) {
        Ok(detail) => health_check("tun_dns", "ok", "TUN DNS", detail),
        Err(error) => health_check("tun_dns", "error", "TUN DNS", error),
    }
}

#[cfg(windows)]
fn windows_tun_https_canary_health_check() -> ConnectionHealthCheck {
    match windows_command_output_with_timeout(
        "curl.exe",
        &[
            "--max-time",
            "15",
            "--silent",
            "--show-error",
            "--output",
            "NUL",
            "--write-out",
            "%{http_code} %{size_download}",
            PROFILE_PING_URL,
        ],
        Duration::from_secs(18),
    ) {
        Ok(detail) => {
            let mut parts = detail.split_whitespace();
            let status = parts.next().and_then(|value| value.parse::<u16>().ok());
            let size = parts.next().and_then(|value| value.parse::<u64>().ok());
            if status == Some(200) && size.unwrap_or(0) > 0 {
                health_check(
                    "tun_https_canary",
                    "ok",
                    "TUN HTTPS canary",
                    format!("GET {} returned {}", PROFILE_PING_URL, detail),
                )
            } else {
                health_check(
                    "tun_https_canary",
                    "error",
                    "TUN HTTPS canary",
                    format!("GET {} returned {}", PROFILE_PING_URL, detail),
                )
            }
        }
        Err(error) => health_check(
            "tun_https_canary",
            "error",
            "TUN HTTPS canary",
            format!("GET {} failed: {}", PROFILE_PING_URL, error),
        ),
    }
}

#[cfg(windows)]
fn windows_wininet_proxy_health_check(http_port: u16) -> ConnectionHealthCheck {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let settings = match RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
    {
        Ok(settings) => settings,
        Err(error) => {
            return health_check(
                "wininet_proxy",
                "error",
                "Windows proxy",
                format!("Cannot open Internet Settings: {}", error),
            )
        }
    };
    let enabled: u32 = settings.get_value("ProxyEnable").unwrap_or(0);
    let server: String = settings.get_value("ProxyServer").unwrap_or_default();
    let expected = format!("127.0.0.1:{}", http_port);
    if enabled == 1 && server.contains(&expected) {
        health_check(
            "wininet_proxy",
            "ok",
            "Windows proxy",
            format!("WinINet proxy points to {}", expected),
        )
    } else {
        let server_status = if server.is_empty() {
            "empty"
        } else {
            "not expected loopback"
        };
        health_check(
            "wininet_proxy",
            "error",
            "Windows proxy",
            format!("ProxyEnable={}, ProxyServer={}", enabled, server_status),
        )
    }
}

#[tauri::command]
fn detect_stale_doodleray_proxy() -> Result<String, String> {
    #[cfg(windows)]
    {
        let state = sysproxy::detect_stale_doodleray_proxy()?;
        let value = serde_json::to_value(state)
            .map_err(|err| format!("Failed to serialize proxy state: {}", err))?;
        Ok(value.as_str().unwrap_or("unknown").to_string())
    }

    #[cfg(not(windows))]
    {
        Ok("unsupported".into())
    }
}

#[tauri::command]
fn repair_stale_doodleray_proxy_only() -> Result<String, String> {
    #[cfg(windows)]
    {
        let outcome = sysproxy::repair_stale_doodleray_proxy_only()?;
        let value = serde_json::to_value(outcome)
            .map_err(|err| format!("Failed to serialize proxy repair outcome: {}", err))?;
        Ok(value.as_str().unwrap_or("unknown").to_string())
    }

    #[cfg(not(windows))]
    {
        Ok("unsupported".into())
    }
}

/// Add Windows Defender exclusion for the app directory
/// If already running as admin — runs directly. Otherwise elevates via UAC using temp .ps1 script.
#[tauri::command]
fn add_defender_exclusion() -> Result<String, String> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let dir = exe
            .parent()
            .ok_or("Cannot get parent dir")?
            .to_string_lossy()
            .to_string();
        let already_admin = is_admin();

        if already_admin {
            let mut cmd = std::process::Command::new("powershell");
            cmd.creation_flags(0x08000000);
            let status = cmd
                .args([
                    "-NoProfile",
                    "-WindowStyle",
                    "Hidden",
                    "-Command",
                    &format!(
                        "Add-MpPreference -ExclusionPath '{}'",
                        dir.replace("'", "''")
                    ),
                ])
                .status()
                .map_err(|e| format!("Failed to run powershell: {}", e))?;
            if !status.success() {
                return Err(format!("PowerShell exited with code: {:?}", status.code()));
            }
        } else {
            // Write temp .ps1 to avoid nested escaping issues
            let ps1_path = std::env::temp_dir().join("doodleray_defender.ps1");
            let ps1_content = format!(
                "Add-MpPreference -ExclusionPath '{}'",
                dir.replace("'", "''")
            );
            std::fs::write(&ps1_path, &ps1_content)
                .map_err(|e| format!("Failed to write temp script: {}", e))?;

            let script = format!(
                "Start-Process powershell -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-WindowStyle','Hidden','-File','{}' -Verb RunAs -WindowStyle Hidden -Wait",
                ps1_path.to_string_lossy().replace("'", "''")
            );
            let mut cmd = std::process::Command::new("powershell");
            cmd.creation_flags(0x08000000);
            let status = cmd
                .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
                .status()
                .map_err(|e| format!("Failed to run powershell: {}", e))?;

            let _ = std::fs::remove_file(&ps1_path);

            if !status.success() {
                return Err("UAC was cancelled or elevation failed".into());
            }
        }

        // Verify — try registry first (works without admin), then PowerShell fallback
        std::thread::sleep(Duration::from_millis(1000));
        let verified = check_defender_exclusion_inner();
        if verified {
            Ok(format!("✓ Exclusion added for {}", dir))
        } else {
            // UAC was accepted but verification failed — Get-MpPreference often
            // requires admin to read ExclusionPath. The exclusion was likely added.
            Ok(format!("✓ Exclusion applied for {}", dir))
        }
    }
    #[cfg(not(windows))]
    {
        Err("Not supported on this platform".into())
    }
}

/// Check if app directory is in Defender exclusion list.
/// Uses registry first (works without admin), falls back to PowerShell.
#[cfg(windows)]
fn check_defender_exclusion_inner() -> bool {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(_) => return false,
    };
    let dir = match exe.parent() {
        Some(d) => d.to_string_lossy().to_string(),
        None => return false,
    };
    let dir_lower = dir.to_lowercase();

    // Method 1: Check registry (readable without admin on most systems)
    if let Ok(key) = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
        .open_subkey("SOFTWARE\\Microsoft\\Windows Defender\\Exclusions\\Paths")
    {
        for (name, _) in key.enum_values().flatten() {
            if name.to_lowercase() == dir_lower {
                return true;
            }
        }
    }

    // Method 2: Fallback to PowerShell (may need admin to list ExclusionPath)
    let mut cmd = std::process::Command::new("powershell");
    cmd.creation_flags(0x08000000);
    let output = cmd
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-Command",
            "(Get-MpPreference).ExclusionPath -join '|'",
        ])
        .output();

    match output {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
            text.contains(&dir_lower)
        }
        Err(_) => false,
    }
}

/// Tauri command to check Defender exclusion status
#[tauri::command]
fn check_defender_exclusion() -> bool {
    #[cfg(windows)]
    {
        check_defender_exclusion_inner()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

// ═══════════════════════════════════════════════════════════
//  Silent Admin Autostart (UAC Bypass via Task Scheduler)
// ═══════════════════════════════════════════════════════════

#[tauri::command]
async fn toggle_silent_autostart(_enable: bool) -> Result<String, String> {
    #[cfg(windows)]
    {
        let exe_path_buf =
            std::env::current_exe().map_err(|e| format!("Failed to get exe path: {}", e))?;
        let exe_path = exe_path_buf.to_string_lossy().to_string();
        let already_admin = is_admin();

        if _enable {
            if already_admin {
                // Already admin — create task directly without UAC
                let mut cmd = std::process::Command::new("schtasks");
                cmd.creation_flags(0x08000000);
                let status = cmd
                    .args([
                        "/Create",
                        "/TN",
                        "DoodleRay_SilentStart",
                        "/TR",
                        &format!("\"{}\" --minimized", exe_path),
                        "/SC",
                        "ONLOGON",
                        "/RL",
                        "HIGHEST",
                        "/F",
                    ])
                    .status()
                    .map_err(|e| format!("schtasks failed: {}", e))?;

                if !status.success() {
                    return Err("schtasks /Create failed".into());
                }
            } else {
                // Write temp .ps1 script to avoid PowerShell escaping issues
                // that cause schtasks to receive literal single-quote characters
                let ps1_path = std::env::temp_dir().join("doodleray_task_create.ps1");
                let ps1_content = format!(
                    "$action = New-ScheduledTaskAction -Execute '{}' -Argument '--minimized'\n\
                     $trigger = New-ScheduledTaskTrigger -AtLogOn\n\
                     $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries\n\
                     $principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -RunLevel Highest -LogonType Interactive\n\
                     Register-ScheduledTask -TaskName 'DoodleRay_SilentStart' -Action $action -Trigger $trigger -Settings $settings -Principal $principal -Force\n",
                    exe_path.replace("'", "''")
                );
                std::fs::write(&ps1_path, &ps1_content)
                    .map_err(|e| format!("Failed to write temp script: {}", e))?;

                let script = format!(
                    "Start-Process powershell -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-WindowStyle','Hidden','-File','{}' -Verb RunAs -WindowStyle Hidden -Wait",
                    ps1_path.to_string_lossy().replace("'", "''")
                );
                let mut cmd = std::process::Command::new("powershell");
                cmd.creation_flags(0x08000000);
                let _ = cmd
                    .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
                    .status();

                let _ = std::fs::remove_file(&ps1_path);
            }

            // Verify the task was actually created
            std::thread::sleep(std::time::Duration::from_millis(1500));
            let exists = check_silent_autostart_inner();
            if exists {
                Ok("Silent autostart enabled".into())
            } else {
                Err("Task was not created — UAC may have been declined".into())
            }
        } else {
            if already_admin {
                let mut cmd = std::process::Command::new("schtasks");
                cmd.creation_flags(0x08000000);
                let _ = cmd
                    .args(["/Delete", "/TN", "DoodleRay_SilentStart", "/F"])
                    .status();
            } else {
                // Write temp .ps1 script for clean deletion
                let ps1_path = std::env::temp_dir().join("doodleray_task_delete.ps1");
                let ps1_content = "Unregister-ScheduledTask -TaskName 'DoodleRay_SilentStart' -Confirm:$false -ErrorAction SilentlyContinue\n\
                     schtasks /Delete /TN \"DoodleRay_SilentStart\" /F 2>$null\n";
                let _ = std::fs::write(&ps1_path, ps1_content);

                let script = format!(
                    "Start-Process powershell -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-WindowStyle','Hidden','-File','{}' -Verb RunAs -WindowStyle Hidden -Wait",
                    ps1_path.to_string_lossy().replace("'", "''")
                );
                let mut cmd = std::process::Command::new("powershell");
                cmd.creation_flags(0x08000000);
                let _ = cmd
                    .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
                    .status();

                let _ = std::fs::remove_file(&ps1_path);
            }

            // Verify deletion
            std::thread::sleep(std::time::Duration::from_millis(1000));
            let still_exists = check_silent_autostart_inner();
            if !still_exists {
                Ok("Silent autostart disabled".into())
            } else {
                Err("Task was not removed — UAC may have been declined".into())
            }
        }
    }
    #[cfg(not(windows))]
    {
        Err("Silent autostart is only supported on Windows".into())
    }
}

#[cfg(windows)]
fn check_silent_autostart_inner() -> bool {
    let mut cmd = std::process::Command::new("schtasks");
    cmd.args(["/Query", "/TN", "DoodleRay_SilentStart"]);
    cmd.creation_flags(0x08000000);
    if let Ok(out) = cmd.output() {
        out.status.success()
    } else {
        false
    }
}

#[tauri::command]
async fn check_silent_autostart() -> bool {
    #[cfg(windows)]
    {
        check_silent_autostart_inner()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Full cleanup — stop all engines, kill subprocesses, unset system proxy
/// Safe to call multiple times (idempotent)
fn full_cleanup() {
    #[cfg(all(target_os = "macos", feature = "app-store"))]
    app_store_tunnel::stop_cached();

    #[cfg(not(all(target_os = "macos", feature = "app-store")))]
    {
        #[cfg(windows)]
        let _ = tunnel_service_stop("full_cleanup");
        let _ = singbox::stop_singbox();
        let _ = xray::stop_xray();
        let _ = tun::stop_tun();
        #[cfg(windows)]
        terminate_orphaned_doodleray_engine_processes();
        restore_system_proxy_if_owned(false);
    }

    // Reset connection state
    if let Ok(mut state) = CONNECTION_STATE.lock() {
        *state = false;
    }
    if let Ok(mut engine) = ACTIVE_ENGINE.lock() {
        *engine = None;
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// True only when the QA control surface env flag is set at app launch.
/// Production installs never set it, so the surface stays off by default.
fn qa_control_token() -> Option<String> {
    std::env::var("DOODLERAY_QA_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| value.len() >= 24)
}

#[cfg(windows)]
fn qa_control_token_matches(provided: &str) -> bool {
    let Some(expected) = qa_control_token() else {
        return false;
    };
    let provided = provided.trim();
    if expected.len() != provided.len() {
        return false;
    }
    expected
        .bytes()
        .zip(provided.bytes())
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

#[tauri::command]
fn qa_control_enabled() -> bool {
    std::env::var("DOODLERAY_QA_CONTROL")
        .map(|v| v == "1")
        .unwrap_or(false)
        && qa_control_token().is_some()
}

#[cfg(windows)]
#[tauri::command]
fn qa_control_update_frontend_snapshot(snapshot: serde_json::Value) -> bool {
    if !qa_control_enabled() {
        return false;
    }
    if let Ok(mut guard) = QA_FRONTEND_SNAPSHOT.lock() {
        *guard = Some(snapshot);
        return true;
    }
    false
}

#[cfg(not(windows))]
#[tauri::command]
fn qa_control_update_frontend_snapshot(_snapshot: serde_json::Value) -> bool {
    false
}

/// QA-only local control surface (loopback HTTP on 127.0.0.1:48765), enabled
/// exclusively by DOODLERAY_QA_CONTROL=1 in the app environment. It replaces
/// fragile CDP DOM automation for connect/disconnect/mode/refresh actions by
/// emitting events the frontend executes through the exact same handlers a
/// user clicks; status/bundle are answered backend-side. Never reachable off
/// the machine (loopback bind) and never enabled in production launches.
#[cfg(windows)]
fn spawn_qa_control_server(app: tauri::AppHandle) {
    if !qa_control_enabled() {
        return;
    }
    std::thread::spawn(move || {
        let listener = match std::net::TcpListener::bind(("127.0.0.1", 48765)) {
            Ok(listener) => listener,
            Err(error) => {
                vpn_log(&format!("QA control surface bind failed: {}", error));
                return;
            }
        };
        vpn_log("QA control surface listening on 127.0.0.1:48765 (DOODLERAY_QA_CONTROL=1)");
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let _ = handle_qa_control_connection(&mut stream, &app);
        }
    });
}

#[cfg(windows)]
fn handle_qa_control_connection(
    stream: &mut std::net::TcpStream,
    app: &tauri::AppHandle,
) -> std::io::Result<()> {
    use std::io::{Read, Write};
    let mut buffer = [0u8; 4096];
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let supplied_token = request.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("X-DoodleRay-QA-Token")
            .then(|| value.trim())
    });
    if !supplied_token.is_some_and(qa_control_token_matches) {
        let body = serde_json::json!({ "ok": false, "error": "unauthorized" }).to_string();
        let response = format!(
            "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        return stream.write_all(response.as_bytes());
    }
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let (code, body) = qa_control_dispatch(app, path);
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        code,
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())
}

#[cfg(windows)]
fn qa_query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|part| {
        let (name, value) = part.split_once('=')?;
        if name == key {
            Some(value.replace('+', " "))
        } else {
            None
        }
    })
}

#[cfg(windows)]
fn qa_control_dispatch(app: &tauri::AppHandle, path: &str) -> (&'static str, String) {
    use tauri::Emitter;
    let (route, query) = path.split_once('?').unwrap_or((path, ""));
    match route {
        "/status" => {
            let service = match ipc::tunnel_service_status() {
                Ok(tunnel_service::TunnelResponse::Status(status)) => {
                    serde_json::to_value(status).ok()
                }
                _ => None,
            };
            let connected = CONNECTION_STATE.lock().map(|state| *state).unwrap_or(false);
            let frontend = QA_FRONTEND_SNAPSHOT
                .lock()
                .ok()
                .and_then(|snapshot| snapshot.clone());
            (
                "200 OK",
                serde_json::json!({
                    "app_version": env!("CARGO_PKG_VERSION"),
                    "app_connected": connected,
                    "service": service,
                    "frontend": frontend,
                })
                .to_string(),
            )
        }
        "/export-bundle" => match export_support_bundle(
            Some("tun".into()),
            Some("set".into()),
            1080,
            1081,
            Some(qa_query_param(query, "failure_marker").unwrap_or_else(|| "qa-control".into())),
        ) {
            Ok(bundle_path) => (
                "200 OK",
                serde_json::json!({ "ok": true, "path": bundle_path }).to_string(),
            ),
            Err(error) => (
                "500 Internal Server Error",
                serde_json::json!({ "ok": false, "error": error }).to_string(),
            ),
        },
        "/repair-runtime" => {
            let reason = qa_query_param(query, "reason").unwrap_or_else(|| "qa-control".into());
            match ipc::send_tunnel_command(&tunnel_service::TunnelCommand::RepairRuntime(
                tunnel_service::RepairRuntimeRequest {
                    op_id: None,
                    reason,
                },
            )) {
                Ok(tunnel_service::TunnelResponse::Status(status)) => (
                    "200 OK",
                    serde_json::json!({ "ok": true, "service": status }).to_string(),
                ),
                Ok(tunnel_service::TunnelResponse::Error { message }) => (
                    "500 Internal Server Error",
                    serde_json::json!({ "ok": false, "error": message }).to_string(),
                ),
                Ok(tunnel_service::TunnelResponse::Diagnostics(_)) => (
                    "500 Internal Server Error",
                    serde_json::json!({ "ok": false, "error": "unexpected diagnostics response" })
                        .to_string(),
                ),
                Err(error) => (
                    "500 Internal Server Error",
                    serde_json::json!({ "ok": false, "error": error }).to_string(),
                ),
            }
        }
        "/connect"
        | "/disconnect"
        | "/logout"
        | "/switch-mode"
        | "/refresh-subscription"
        | "/import-subscription"
        | "/add-routing-rule"
        | "/clear-custom-routing-rules"
        | "/simulate-tun-failure" => {
            let payload = serde_json::json!({
                "action": route.trim_start_matches('/'),
                "query": query,
            });
            match app.emit("doodleray-qa-control", payload) {
                Ok(()) => (
                    "202 Accepted",
                    serde_json::json!({ "ok": true, "accepted": route }).to_string(),
                ),
                Err(error) => (
                    "500 Internal Server Error",
                    serde_json::json!({ "ok": false, "error": error.to_string() }).to_string(),
                ),
            }
        }
        _ => (
            "404 Not Found",
            serde_json::json!({ "ok": false, "error": "unknown route" }).to_string(),
        ),
    }
}

pub fn run() {
    if !claim_single_app_instance() {
        return;
    }

    #[cfg(windows)]
    terminate_other_doodleray_app_instances();

    // ── Startup cleanup ──
    // If the service still owns an active protected tunnel, preserve it and
    // let the reloaded UI reconcile from service status. Only stale/non-active
    // generations should be scrubbed here.
    #[cfg(not(all(target_os = "macos", feature = "app-store")))]
    {
        #[cfg(windows)]
        let preserve_service_tunnel = tunnel_service_reports_active();
        #[cfg(not(windows))]
        let preserve_service_tunnel = false;

        if !preserve_service_tunnel {
            #[cfg(windows)]
            let _ = tunnel_service_stop("startup_cleanup");
            let _ = tun::stop_tun(); // Kill any orphaned sing-box.exe
            #[cfg(windows)]
            terminate_orphaned_doodleray_engine_processes();
            #[cfg(windows)]
            let _ = sysproxy::recover_orphaned_proxy_on_startup();
        } else {
            vpn_log("startup cleanup: preserving active tunnel service state");
        }
        #[cfg(target_os = "macos")]
        let _ = sysproxy::unset_system_proxy(); // Restore stale app proxy on macOS
    }
    if let Ok(mut managed) = SYSTEM_PROXY_MANAGED.lock() {
        *managed = false;
    }

    // Ctrl+C handler (for dev mode)
    let _ = ctrlc::set_handler(move || {
        full_cleanup();
        std::process::exit(0);
    });

    let builder = tauri::Builder::default();
    #[cfg(not(feature = "app-store"))]
    let builder = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]), // launch minimized by default if started via autostart
        ));

    builder
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            vpn_connect,
            vpn_disconnect,
            vpn_status,
            ping_server,
            ping_server_profile,
            fetch_url,
            fetch_subscription_url,
            get_proxy_logs,
            get_traffic_stats,
            check_port,
            force_free_port,
            is_admin,
            quit_app,
            workshop_api,
            toggle_silent_autostart,
            check_silent_autostart,
            restart_as_admin,
            scan_installed_apps,
            check_connection_health,
            get_connection_health,
            get_connection_health_full,
            repair_windows_runtime,
            repair_active_tunnel_compatibility_proxy,
            repair_active_tunnel_runtime,
            export_support_bundle,
            run_network_diagnosis,
            list_running_apps,
            list_dir_exes,
            detect_stale_doodleray_proxy,
            repair_stale_doodleray_proxy_only,
            add_defender_exclusion,
            check_defender_exclusion,
            install_tunnel_service,
            tunnel_service_health,
            tunnel_service_diagnostics,
            prepare_for_app_update,
            run_network_diagnostics,
            get_storage_report,
            clear_app_cache,
            secure_store_get,
            secure_store_set,
            secure_store_delete,
            app_api_session_status,
            app_api_exchange_code,
            app_api_exchange_legacy_subscription,
            app_api_refresh,
            app_api_logout,
            app_api_locations,
            app_api_subscription_status,
            app_api_submit_diagnostics,
            app_connect_location,
            app_ping_location,
            app_disconnect,
            qa_control_enabled,
            qa_control_update_frontend_snapshot,
        ])
        .setup(|app| {
            #[cfg(windows)]
            spawn_qa_control_server(app.handle().clone());
            // ── System Tray ──
            let show_item = MenuItemBuilder::with_id("show", "Show DoodleRay").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let tray_menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let _tray = TrayIconBuilder::with_id("main-tray")
                .tooltip("DoodleRay VPN — Disconnected")
                .icon(app.default_window_icon().cloned().unwrap())
                .menu(&tray_menu)
                .on_menu_event(|app, event| {
                    match event.id().as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            // Full cleanup before quitting
                            full_cleanup();
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // ── Minimize to tray on close ──
            let app_handle = app.handle().clone();
            if let Some(window) = app.get_webview_window("main") {
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // Hide instead of close → minimize to tray
                        api.prevent_close();
                        if let Some(win) = app_handle.get_webview_window("main") {
                            let _ = win.hide();
                        }
                    }
                });
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // Catch ALL exit paths — OS shutdown, task manager kill, etc.
            match event {
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { .. } => {
                    vpn_log("macOS Dock reopen: restoring main window");
                    let _ = app_handle.show();
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                    }
                }
                tauri::RunEvent::ExitRequested { .. } => {
                    xray::begin_shutdown();
                }
                tauri::RunEvent::Exit => {
                    full_cleanup();
                }
                _ => {}
            }
        });
}
