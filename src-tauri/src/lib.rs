pub mod singbox;
pub mod tun;
pub mod xray;

#[cfg(windows)]
pub mod ipc;
#[cfg(windows)]
pub mod sysproxy;

#[cfg(target_os = "macos")]
#[path = "sysproxy_macos.rs"]
pub mod sysproxy;

use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::sync::Mutex;
use std::time::{Duration, Instant};
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
// sing-box clash API traffic tracking (previous totals for delta calculation)
static SB_PREV_DOWN: Mutex<i64> = Mutex::new(0);
static SB_PREV_UP: Mutex<i64> = Mutex::new(0);
// sing-box seen connection IDs (to only log new connections)
use std::collections::HashSet;
static SB_SEEN_CONNS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

// Connection debug log buffer — shown in UI via get_proxy_logs
static CONNECT_LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

const WORKSHOP_API_HOST: &str = "doodleraydb-doodleray-ic3y6k-c7350f-94-241-172-101.traefik.me";
const APP_MANAGED_PORTS: &[u16] = &[10808, 10809, 10813];
const SECURE_STORE_SERVICE: &str = "DoodleRay";
const SECURE_STORE_CHUNK_BYTES: usize = 1800;
const SECURE_STORE_CHUNK_PREFIX: &str = "chunked:v1:";

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

fn validate_http_url(raw_url: &str) -> Result<Url, String> {
    let parsed = Url::parse(raw_url).map_err(|e| format!("Invalid URL: {}", e))?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err("Only http:// and https:// URLs are allowed".into()),
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
    } else {
        let port = parsed.port_or_known_default().unwrap_or(443);
        if let Ok(addrs) = (host, port).to_socket_addrs() {
            for addr in addrs {
                if !is_public_ip(addr.ip()) {
                    return Err(
                        "Subscription host resolves to a private, loopback, or link-local address"
                            .into(),
                    );
                }
            }
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

fn validate_workshop_api_url(raw_url: &str) -> Result<Url, String> {
    let parsed = validate_http_url(raw_url)?;
    if parsed.scheme() != "https" {
        return Err("Workshop API must use HTTPS".into());
    }
    if parsed.host_str() != Some(WORKSHOP_API_HOST) {
        return Err("Workshop API host is not allowed".into());
    }
    if !parsed.path().starts_with("/api/") && parsed.path() != "/api" {
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

fn default_system_proxy_mode() -> String {
    "set".into()
}

fn safe_system_proxy_mode(mode: &str) -> &str {
    match mode {
        "set" | "clear" | "unchanged" => mode,
        _ => "set",
    }
}

fn clear_system_proxy_if_managed(force: bool) {
    let should_clear = force
        || SYSTEM_PROXY_MANAGED
            .lock()
            .map(|managed| *managed)
            .unwrap_or(false);
    if !should_clear {
        return;
    }

    let _ = sysproxy::unset_system_proxy();
    if let Ok(mut managed) = SYSTEM_PROXY_MANAGED.lock() {
        *managed = false;
    }
}

fn apply_system_proxy_mode(mode: &str, http_port: u16) -> Result<&'static str, String> {
    match safe_system_proxy_mode(mode) {
        "set" => {
            sysproxy::set_system_proxy(http_port)?;
            if let Ok(mut managed) = SYSTEM_PROXY_MANAGED.lock() {
                *managed = true;
            }
            Ok("set")
        }
        "clear" => {
            clear_system_proxy_if_managed(true);
            Ok("cleared")
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
            "Connected with local proxy only; system proxy cleared. SOCKS5: 127.0.0.1:{}, HTTP: 127.0.0.1:{}",
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

fn singbox_dns_config(mode: &str) -> serde_json::Value {
    match mode {
        "realip" => serde_json::json!({
            "servers": [
                {
                    "tag": "dns-remote",
                    "type": "udp",
                    "server": "1.1.1.1",
                    "detour": "proxy"
                },
                {
                    "tag": "dns-direct",
                    "type": "udp",
                    "server": "9.9.9.9"
                }
            ],
            "final": "dns-remote",
            "strategy": "prefer_ipv4"
        }),
        _ => serde_json::json!({
            "servers": [
                {
                    "tag": "dns-remote",
                    "type": "udp",
                    "server": "1.1.1.1",
                    "detour": "proxy"
                },
                {
                    "tag": "dns-direct",
                    "type": "udp",
                    "server": "9.9.9.9"
                },
                {
                    "tag": "dns-fakeip",
                    "type": "fakeip",
                    "inet4_range": "198.18.0.0/15",
                    "inet6_range": "fc00::/18"
                }
            ],
            "rules": [
                { "query_type": ["A", "AAAA"], "server": "dns-fakeip" }
            ],
            "final": "dns-remote",
            "strategy": "prefer_ipv4",
            "independent_cache": true
        }),
    }
}

fn xray_dns_servers(mode: &str) -> serde_json::Value {
    match mode {
        "fakeip" => serde_json::json!({
            "servers": ["1.1.1.1", "9.9.9.9"]
        }),
        _ => serde_json::json!({
            "servers": ["1.1.1.1", "9.9.9.9"]
        }),
    }
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

fn secure_store_entry(key: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SECURE_STORE_SERVICE, key)
        .map_err(|e| format!("Secure storage unavailable: {}", e))
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
    let entry = secure_store_entry(key)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Secure storage delete failed: {}", e)),
    }
}

fn delete_secure_store_chunks(key: &str, manifest: &str) {
    let Some(count) = secure_store_chunk_count(manifest) else {
        return;
    };

    for index in 0..count {
        let _ = delete_secure_store_entry(&secure_store_chunk_key(key, index));
    }
}

#[tauri::command]
fn secure_store_get(key: String) -> Result<Option<String>, String> {
    validate_secure_store_key(&key)?;
    let entry = secure_store_entry(&key)?;
    match entry.get_password() {
        Ok(value) => {
            let Some(count) = secure_store_chunk_count(&value) else {
                return Ok(Some(value));
            };

            let mut restored = String::new();
            for index in 0..count {
                let chunk_entry = secure_store_entry(&secure_store_chunk_key(&key, index))?;
                let chunk = chunk_entry
                    .get_password()
                    .map_err(|e| format!("Secure storage chunk read failed: {}", e))?;
                restored.push_str(&chunk);
            }
            Ok(Some(restored))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Secure storage read failed: {}", e)),
    }
}

#[tauri::command]
fn secure_store_set(key: String, value: String) -> Result<(), String> {
    validate_secure_store_key(&key)?;
    let entry = secure_store_entry(&key)?;
    if let Ok(old_value) = entry.get_password() {
        delete_secure_store_chunks(&key, &old_value);
    }

    if value.len() > SECURE_STORE_CHUNK_BYTES {
        let chunks = secure_store_chunks(&value);
        for (index, chunk) in chunks.iter().enumerate() {
            let chunk_entry = secure_store_entry(&secure_store_chunk_key(&key, index))?;
            chunk_entry
                .set_password(chunk)
                .map_err(|e| format!("Secure storage chunk write failed: {}", e))?;
        }

        return entry
            .set_password(&format!("{}{}", SECURE_STORE_CHUNK_PREFIX, chunks.len()))
            .map_err(|e| format!("Secure storage write failed: {}", e));
    }

    entry
        .set_password(&value)
        .map_err(|e| format!("Secure storage write failed: {}", e))
}

#[tauri::command]
fn secure_store_delete(key: String) -> Result<(), String> {
    validate_secure_store_key(&key)?;
    if let Ok(value) = secure_store_entry(&key)?.get_password() {
        delete_secure_store_chunks(&key, &value);
    }
    delete_secure_store_entry(&key)
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
}

#[derive(Debug, Serialize)]
pub struct PingResult {
    pub server_id: String,
    pub ping_ms: i32, // -1 = failed/timeout
}

/// Fetch a URL from Rust side — bypasses CORS restrictions in WebView
#[tauri::command]
async fn fetch_url(url: String) -> Result<String, String> {
    let parsed_url = validate_http_url(&url)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let response = client
        .get(parsed_url)
        .header("User-Agent", "DoodleRay/2.0")
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "Fetch failed: request timed out".to_string()
            } else if e.is_connect() {
                format!("Fetch failed: connection error ({})", e)
            } else {
                format!("Fetch failed: {}", e)
            }
        })?;

    if !response.status().is_success() {
        return Err(format!(
            "HTTP {}: {}",
            response.status().as_u16(),
            response.status().as_str()
        ));
    }

    response
        .text()
        .await
        .map_err(|e| format!("Failed to read body: {}", e))
}

/// Workshop API proxy — supports GET/POST for the pinned production API.
#[tauri::command]
async fn workshop_api(url: String, method: String, body: Option<String>) -> Result<String, String> {
    let parsed_url = validate_workshop_api_url(&url)?;
    // Extract host from URL for DNS pinning (crucial for TUN mode where DNS may fail)
    let mut builder = reqwest::Client::builder()
        .no_proxy() // IMPORTANT: bypass system proxy so API calls don't loop through VPN
        .timeout(Duration::from_secs(15));

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

    response
        .text()
        .await
        .map_err(|e| format!("Failed to read body: {}", e))
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

    // DNS config — sing-box 1.13+ format
    let dns = singbox_dns_config(&req.dns_mode);
    let network_stack = safe_network_stack(&req.network_stack);

    // Inbound config: TUN or SOCKS+HTTP
    let inbounds = if req.proxy_mode == "tun" {
        serde_json::json!([
            {
                "type": "tun",
                "tag": "tun-in",
                "address": ["172.19.0.1/30", "fdfe:dcba:9876::1/126"],
                "mtu": 1492,
                "auto_route": true,
                "strict_route": req.strict_route,
                "stack": network_stack,
                "sniff": true,
                "sniff_override_destination": false
            }
        ])
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
    let mut proxy_processes = Vec::new();

    let mut direct_domains = Vec::new();
    let mut direct_domain_suffixes = Vec::new();
    let mut direct_processes = Vec::new();

    let mut block_domains = Vec::new();
    let mut block_domain_suffixes = Vec::new();
    let mut block_processes = Vec::new();

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
        } else if rule.rule_type == "exe" {
            let val = rule.value.to_lowercase();
            match rule.action.as_str() {
                "proxy" => proxy_processes.push(val),
                "direct" => direct_processes.push(val),
                "block" => block_processes.push(val),
                _ => {}
            }
        }
    }

    let mut custom_rules = Vec::new();

    if !proxy_domains.is_empty() || !proxy_domain_suffixes.is_empty() || !proxy_processes.is_empty()
    {
        let mut r = serde_json::json!({ "outbound": "proxy" });
        if !proxy_domains.is_empty() {
            r["domain"] = proxy_domains.clone().into();
        }
        if !proxy_domain_suffixes.is_empty() {
            r["domain_suffix"] = proxy_domain_suffixes.clone().into();
        }
        if !proxy_processes.is_empty() {
            r["process_name"] = proxy_processes.clone().into();
        }
        custom_rules.push(r);
    }

    if !direct_domains.is_empty()
        || !direct_domain_suffixes.is_empty()
        || !direct_processes.is_empty()
    {
        let mut r = serde_json::json!({ "outbound": "direct" });
        if !direct_domains.is_empty() {
            r["domain"] = direct_domains.clone().into();
        }
        if !direct_domain_suffixes.is_empty() {
            r["domain_suffix"] = direct_domain_suffixes.clone().into();
        }
        if !direct_processes.is_empty() {
            r["process_name"] = direct_processes.clone().into();
        }
        custom_rules.push(r);
    }

    if !block_domains.is_empty() || !block_domain_suffixes.is_empty() || !block_processes.is_empty()
    {
        let mut r = serde_json::json!({ "outbound": "block" });
        if !block_domains.is_empty() {
            r["domain"] = block_domains.clone().into();
        }
        if !block_domain_suffixes.is_empty() {
            r["domain_suffix"] = block_domain_suffixes.clone().into();
        }
        if !block_processes.is_empty() {
            r["process_name"] = block_processes.clone().into();
        }
        custom_rules.push(r);
    }

    let mut rules = vec![
        serde_json::json!({ "action": "sniff" }),
        serde_json::json!({ "protocol": "dns", "action": "hijack-dns" }),
    ];

    // TUN mode: private IPs (LAN, localhost) must go direct — they're unreachable via VPN server.
    // NOTE: sing-box's own outbound to the VPN server is already protected from TUN loop
    // by `auto_detect_interface: true` in route config — no process_name exclusion needed.
    if req.proxy_mode == "tun" {
        rules.push(serde_json::json!({
            "ip_is_private": true,
            "outbound": "direct"
        }));
    }

    rules.extend(custom_rules);

    // Kill Switch: if enabled + TUN mode, block all traffic that doesn't match proxy/direct rules
    let final_outbound = if req.kill_switch && req.proxy_mode == "tun" {
        "block"
    } else {
        "proxy"
    };

    // Kill Switch in TUN mode: force strict_route regardless of user setting
    let effective_strict_route = if req.kill_switch && req.proxy_mode == "tun" {
        true
    } else {
        req.strict_route
    };

    // Update inbounds strict_route if TUN mode
    let effective_inbounds = if req.proxy_mode == "tun" {
        serde_json::json!([
            {
                "type": "tun",
                "tag": "tun-in",
                "address": ["172.19.0.1/30", "fdfe:dcba:9876::1/126"],
                "mtu": 1492,
                "auto_route": true,
                "strict_route": effective_strict_route,
                "stack": network_stack,
                "sniff": true,
                "sniff_override_destination": false
            }
        ])
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
            "settings": { "udp": true },
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
            "port": 10813,
            "listen": "127.0.0.1",
            "protocol": "dokodemo-door",
            "settings": { "address": "127.0.0.1" }
        }
    ]);
    config["inbounds"] = inbounds;

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

    config
}

/// Build the xray-core JSON config (for xhttp transport)
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

    let stream_settings = match req.transport.as_str() {
        "xhttp" => serde_json::json!({
            "network": "xhttp",
            "security": req.security,
            "xhttpSettings": {
                "path": req.path.clone().unwrap_or("/xhttp".into())
            },
            "realitySettings": if req.security == "reality" {
                serde_json::json!({
                    "serverName": req.sni.clone().unwrap_or(req.server_address.clone()),
                    "publicKey": req.public_key.clone().unwrap_or_default(),
                    "shortId": req.short_id.clone().unwrap_or_default(),
                    "fingerprint": req.fingerprint.clone().unwrap_or("chrome".into())
                })
            } else {
                serde_json::json!(null)
            },
            "tlsSettings": if req.security == "tls" {
                serde_json::json!({
                    "serverName": req.sni.clone().unwrap_or(req.server_address.clone()),
                    "fingerprint": req.fingerprint.clone().unwrap_or("chrome".into())
                })
            } else {
                serde_json::json!(null)
            }
        }),
        "ws" => serde_json::json!({
            "network": "ws",
            "security": req.security,
            "wsSettings": {
                "path": req.path.clone().unwrap_or("/".into()),
                "headers": {
                    "Host": req.host.clone().unwrap_or(req.server_address.clone())
                }
            }
        }),
        _ => serde_json::json!({
            "network": "tcp",
            "security": req.security
        }),
    };

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

    // Default: private IPs go direct
    routing_rules.push(serde_json::json!({
        "type": "field",
        "ip": ["geoip:private"],
        "outboundTag": "direct"
    }));
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

    serde_json::json!({
        "log": { "loglevel": "info" },
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
        "dns": xray_dns_servers(&req.dns_mode),
        "inbounds": [
            {
                "tag": "socks-in",
                "port": req.socks_port,
                "listen": "127.0.0.1",
                "protocol": "socks",
                "settings": { "udp": true },
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
                "port": 10813,
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
            }
        ],
        "routing": {
            "domainStrategy": "AsIs",
            "rules": final_rules
        }
    })
}

#[tauri::command]
async fn vpn_connect(request: ConnectRequest, app: tauri::AppHandle) -> ConnectResult {
    // Clear previous connect logs
    if let Ok(mut logs) = CONNECT_LOG.lock() {
        logs.clear();
    }

    let has_raw_config = request.raw_xray_config.is_some();
    let use_xray = request.transport == "xhttp" || has_raw_config;
    let is_tun = request.proxy_mode == "tun";

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
            "TUN config: stack={}, dns={}, mtu=1492, sniff=true, strict_route={}",
            safe_network_stack(&request.network_stack),
            request.dns_mode,
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
            let _ = tun::stop_tun();
            let _ = xray::stop_xray();
        }
        Some("xray+app-proxy") => {
            let _ = xray::stop_xray();
        }
        Some("singbox-tun") => {
            let _ = tun::stop_tun();
        }
        Some("singbox+app-proxy") => {}
        Some("singbox") => {}
        _ => {
            let _ = xray::stop_xray();
            let _ = tun::stop_tun();
        }
    }
    clear_system_proxy_if_managed(safe_system_proxy_mode(&request.system_proxy_mode) == "clear");
    reset_sb_traffic();
    vpn_log("previous engine stopped, ports freed");

    // Forcefully release local ports to prevent "Only one usage of each socket address is normally permitted"
    // caused by zombie processes (or double React Strict Mode invocations) locking the ports.
    let _ = force_free_managed_port(request.socks_port).await;
    let _ = force_free_managed_port(request.http_port).await;
    let _ = force_free_managed_port(10813).await;

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
                let _ = force_free_managed_port(10813).await;
                std::thread::sleep(std::time::Duration::from_millis(1000));
                start_result = xray::start_xray(&xray_config);
            }
        }

        if let Err(e) = start_result {
            vpn_log(&format!("FATAL: xray-core failed to start: {}", e));
            return ConnectResult {
                success: false,
                message: format!("Failed to start xray-core: {}", e),
            };
        }
        vpn_log("xray-core started OK");

        // sing-box as TUN bridge → routes all traffic to xray's SOCKS5
        vpn_log("building TUN bridge config (sing-box -> xray SOCKS5)");
        let tun_bridge = serde_json::json!({
            "log": { "level": "info" },
            "dns": singbox_dns_config(&request.dns_mode),
            "inbounds": [{
                "type": "tun",
                "tag": "tun-in",
                "interface_name": "tun0",
                "address": ["172.19.0.1/30"],
                "mtu": 1492,
                "auto_route": true,
                "strict_route": false,
                "stack": safe_network_stack(&request.network_stack),
                "sniff": true,
                "sniff_override_destination": false
            }],
            "outbounds": [
                {
                    "type": "socks",
                    "tag": "proxy",
                    "server": "127.0.0.1",
                    "server_port": request.socks_port
                },
                { "type": "direct", "tag": "direct" }
            ],
            "route": {
                "auto_detect_interface": true,
                "default_domain_resolver": "dns-direct",
                "rules": [
                    { "action": "sniff" },
                    { "protocol": "dns", "action": "hijack-dns" },
                    { "process_name": ["sing-box.exe", "xray.exe", "DoodleRay.exe", "node.exe", "adb.exe", "svchost.exe", "lsass.exe", "csrss.exe", "System"], "outbound": "direct" },
                    { "ip_is_private": true, "outbound": "direct" }
                ]
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
                let mut state = CONNECTION_STATE.lock().unwrap();
                *state = true;
                let mut engine = ACTIVE_ENGINE.lock().unwrap();
                *engine = Some("xray+tun".into());
                update_tray_connected(&app, &request.server_address);
                ConnectResult {
                    success: true,
                    message: "TUN connected (xray-core + sing-box TUN bridge)".into(),
                }
            }
            Err(e) => {
                vpn_log(&format!("FATAL: TUN bridge failed: {}", e));
                let _ = xray::stop_xray();
                ConnectResult {
                    success: false,
                    message: format!("TUN failed: {}", e),
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
                let _ = force_free_managed_port(10813).await;
                std::thread::sleep(std::time::Duration::from_millis(1000));
                start_result = xray::start_xray(&xray_config);
            }
        }

        match start_result {
            Ok(_) => {
                vpn_log("xray-core started OK, waiting for port ready...");
                wait_for_port_ready(request.socks_port);
                vpn_log("xray port ready");
                let mut state = CONNECTION_STATE.lock().unwrap();
                *state = true;
                let mut engine = ACTIVE_ENGINE.lock().unwrap();
                *engine = Some("xray".into());
                let proxy_action =
                    match apply_system_proxy_mode(&request.system_proxy_mode, request.http_port) {
                        Ok(action) => action,
                        Err(e) => {
                            vpn_log(&format!("FATAL: system proxy failed: {}", e));
                            return ConnectResult {
                                success: false,
                                message: format!(
                                    "xray started but failed to apply system proxy mode: {}",
                                    e
                                ),
                            };
                        }
                    };
                vpn_log(&format!("system proxy mode applied: {}", proxy_action));

                // Per-app TUN bridge: route specific apps via process_name matching
                // Activated when user adds ANY exe rules in Workshop (proxy, direct, or block)
                // sing-box TUN captures all traffic and routes by process name,
                // while xray handles the actual proxy connection via SOCKS5
                let proxy_exes: Vec<String> = request
                    .routing_rules
                    .iter()
                    .filter(|r| r.rule_type == "exe" && r.action == "proxy")
                    .map(|r| r.value.to_lowercase())
                    .collect();
                let direct_exes: Vec<String> = request
                    .routing_rules
                    .iter()
                    .filter(|r| r.rule_type == "exe" && r.action == "direct")
                    .map(|r| r.value.to_lowercase())
                    .collect();
                let block_exes: Vec<String> = request
                    .routing_rules
                    .iter()
                    .filter(|r| r.rule_type == "exe" && r.action == "block")
                    .map(|r| r.value.to_lowercase())
                    .collect();
                let has_exe_rules =
                    !proxy_exes.is_empty() || !direct_exes.is_empty() || !block_exes.is_empty();

                if has_exe_rules {
                    vpn_log(&format!(
                        "per-app TUN bridge: proxy={:?} direct={:?} block={:?}",
                        proxy_exes, direct_exes, block_exes
                    ));
                    let exclude = vec![
                        "sing-box.exe",
                        "xray.exe",
                        "DoodleRay.exe",
                        "adb.exe",
                        "svchost.exe",
                        "lsass.exe",
                        "csrss.exe",
                        "System",
                    ];

                    let mut tun_rules: Vec<serde_json::Value> = Vec::new();

                    let exclude_val: Vec<serde_json::Value> = exclude
                        .iter()
                        .map(|s| serde_json::Value::String(s.to_string()))
                        .collect();
                    tun_rules.push(
                        serde_json::json!({ "process_name": exclude_val, "outbound": "direct" }),
                    );

                    // 2. Blocked apps
                    if !block_exes.is_empty() {
                        let block_val: Vec<serde_json::Value> = block_exes
                            .iter()
                            .map(|s| serde_json::Value::String(s.to_string()))
                            .collect();
                        tun_rules.push(
                            serde_json::json!({ "process_name": block_val, "outbound": "block" }),
                        );
                    }

                    // 3. Direct apps — bypass VPN entirely (games, etc.)
                    if !direct_exes.is_empty() {
                        let direct_val: Vec<serde_json::Value> = direct_exes
                            .iter()
                            .map(|s| serde_json::Value::String(s.to_string()))
                            .collect();
                        tun_rules.push(
                            serde_json::json!({ "process_name": direct_val, "outbound": "direct" }),
                        );
                    }

                    // 4. Proxy apps — route through xray SOCKS5
                    if !proxy_exes.is_empty() {
                        let proxy_val: Vec<serde_json::Value> = proxy_exes
                            .iter()
                            .map(|s| serde_json::Value::String(s.to_string()))
                            .collect();
                        tun_rules.push(
                            serde_json::json!({ "process_name": proxy_val, "outbound": "proxy" }),
                        );
                    }

                    // 5. Private IPs always go direct
                    tun_rules
                        .push(serde_json::json!({ "ip_is_private": true, "outbound": "direct" }));

                    let tun_bridge = serde_json::json!({
                        "log": { "level": "info" },
                        "dns": singbox_dns_config(&request.dns_mode),
                        "inbounds": [{
                            "type": "tun",
                            "tag": "tun-in",
                            "address": ["172.19.0.1/30"],
                            "mtu": 1492,
                            "auto_route": true,
                            "strict_route": false,
                            "stack": safe_network_stack(&request.network_stack),
                            "sniff": true,
                            "sniff_override_destination": false
                        }],
                        "outbounds": [
                            { "type": "direct", "tag": "direct" },
                            {
                                "type": "socks",
                                "tag": "proxy",
                                "server": "127.0.0.1",
                                "server_port": request.socks_port
                            },
                            { "type": "block", "tag": "block" }
                        ],
                        "route": {
                            "auto_detect_interface": true,
                            "default_domain_resolver": "dns-direct",
                            "rules": tun_rules
                        }
                    });

                    // Hot-switch: if TUN bridge is already running from previous session, reuse it
                    // (it routes to localhost SOCKS port which didn't change)
                    if tun::is_singbox_running() {
                        *engine = Some("xray+app-proxy".into());
                        update_tray_connected(&app, &request.server_address);
                        let total = proxy_exes.len() + direct_exes.len() + block_exes.len();
                        return ConnectResult {
                            success: true,
                            message: format!(
                                "Server switched (TUN bridge preserved, {} app rules active)",
                                total
                            ),
                        };
                    }

                    if let Ok(_) = tun::start_tun_elevated(&tun_bridge) {
                        *engine = Some("xray+app-proxy".into());
                        update_tray_connected(&app, &request.server_address);
                        let total = proxy_exes.len() + direct_exes.len() + block_exes.len();
                        return ConnectResult {
                            success: true,
                            message: format!("System Proxy + TUN app routing ({} rules: {} proxy, {} direct, {} block)", 
                                total, proxy_exes.len(), direct_exes.len(), block_exes.len()),
                        };
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
                }
            }
            Err(e) => {
                vpn_log(&format!("FATAL: xray-core failed: {}", e));
                ConnectResult {
                    success: false,
                    message: format!("Failed to start xray-core: {}", e),
                }
            }
        }
    } else if is_tun {
        // ═══ Non-xhttp + TUN ═══
        vpn_log("mode: sing-box TUN (direct, no xray)");
        let config = build_singbox_config(&request);
        write_debug_config(&debug_path, &config);

        vpn_log("starting sing-box TUN (elevated)...");
        match tun::start_tun_elevated(&config) {
            Ok(_) => {
                vpn_log("sing-box TUN started OK");
                let mut state = CONNECTION_STATE.lock().unwrap();
                *state = true;
                let mut engine = ACTIVE_ENGINE.lock().unwrap();
                *engine = Some("singbox-tun".into());
                update_tray_connected(&app, &request.server_address);
                ConnectResult {
                    success: true,
                    message: "TUN connected via sing-box".into(),
                }
            }
            Err(e) => {
                vpn_log(&format!("FATAL: sing-box TUN failed: {}", e));
                ConnectResult {
                    success: false,
                    message: format!("TUN failed: {}", e),
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
                wait_for_port_ready(request.socks_port);
                vpn_log("port ready");
                let mut state = CONNECTION_STATE.lock().unwrap();
                *state = true;
                let mut engine = ACTIVE_ENGINE.lock().unwrap();
                *engine = Some("singbox".into());
                let proxy_action =
                    match apply_system_proxy_mode(&request.system_proxy_mode, request.http_port) {
                        Ok(action) => action,
                        Err(e) => {
                            vpn_log(&format!("FATAL: system proxy failed: {}", e));
                            return ConnectResult {
                                success: false,
                                message: format!(
                                    "sing-box started but failed to apply system proxy mode: {}",
                                    e
                                ),
                            };
                        }
                    };
                vpn_log(&format!("system proxy mode applied: {}", proxy_action));

                // Per-app TUN bridge: route specific apps via process_name matching
                // Activated when user adds ANY exe rules in Workshop (proxy, direct, or block)
                // sing-box TUN captures all traffic and routes by process name
                let proxy_exes: Vec<String> = request
                    .routing_rules
                    .iter()
                    .filter(|r| r.rule_type == "exe" && r.action == "proxy")
                    .map(|r| r.value.to_lowercase())
                    .collect();
                let direct_exes: Vec<String> = request
                    .routing_rules
                    .iter()
                    .filter(|r| r.rule_type == "exe" && r.action == "direct")
                    .map(|r| r.value.to_lowercase())
                    .collect();
                let block_exes: Vec<String> = request
                    .routing_rules
                    .iter()
                    .filter(|r| r.rule_type == "exe" && r.action == "block")
                    .map(|r| r.value.to_lowercase())
                    .collect();
                let has_exe_rules =
                    !proxy_exes.is_empty() || !direct_exes.is_empty() || !block_exes.is_empty();

                if has_exe_rules {
                    let exclude = vec![
                        "sing-box.exe",
                        "DoodleRay.exe",
                        "adb.exe",
                        "svchost.exe",
                        "lsass.exe",
                        "csrss.exe",
                        "System",
                    ];

                    // Build routing rules for the TUN bridge
                    let mut tun_rules: Vec<serde_json::Value> = Vec::new();

                    // 1. System processes always bypass TUN
                    let exclude_val: Vec<serde_json::Value> = exclude
                        .iter()
                        .map(|s| serde_json::Value::String(s.to_string()))
                        .collect();
                    tun_rules.push(
                        serde_json::json!({ "process_name": exclude_val, "outbound": "direct" }),
                    );

                    // 2. Blocked apps
                    if !block_exes.is_empty() {
                        let block_val: Vec<serde_json::Value> = block_exes
                            .iter()
                            .map(|s| serde_json::Value::String(s.to_string()))
                            .collect();
                        tun_rules.push(
                            serde_json::json!({ "process_name": block_val, "outbound": "block" }),
                        );
                    }

                    // 3. Direct apps — bypass VPN entirely (games, etc.)
                    if !direct_exes.is_empty() {
                        let direct_val: Vec<serde_json::Value> = direct_exes
                            .iter()
                            .map(|s| serde_json::Value::String(s.to_string()))
                            .collect();
                        tun_rules.push(
                            serde_json::json!({ "process_name": direct_val, "outbound": "direct" }),
                        );
                    }

                    // 4. Proxy apps — route through SOCKS5
                    if !proxy_exes.is_empty() {
                        let proxy_val: Vec<serde_json::Value> = proxy_exes
                            .iter()
                            .map(|s| serde_json::Value::String(s.to_string()))
                            .collect();
                        tun_rules.push(
                            serde_json::json!({ "process_name": proxy_val, "outbound": "proxy" }),
                        );
                    }

                    // 5. Private IPs always go direct
                    tun_rules
                        .push(serde_json::json!({ "ip_is_private": true, "outbound": "direct" }));

                    let tun_bridge = serde_json::json!({
                        "log": { "level": "info" },
                        "dns": singbox_dns_config(&request.dns_mode),
                        "inbounds": [{
                            "type": "tun",
                            "tag": "tun-in",
                            "address": ["172.19.0.1/30"],
                            "mtu": 1492,
                            "auto_route": true,
                            "strict_route": false,
                            "stack": safe_network_stack(&request.network_stack),
                            "sniff": true,
                            "sniff_override_destination": false
                        }],
                        "outbounds": [
                            { "type": "direct", "tag": "direct" },
                            {
                                "type": "socks",
                                "tag": "proxy",
                                "server": "127.0.0.1",
                                "server_port": request.socks_port
                            },
                            { "type": "block", "tag": "block" }
                        ],
                        "route": {
                            "auto_detect_interface": true,
                            "default_domain_resolver": "dns-direct",
                            "rules": tun_rules
                        }
                    });

                    // Hot-switch: if TUN bridge is already running from previous session, reuse it
                    if tun::is_singbox_running() {
                        *engine = Some("singbox+app-proxy".into());
                        update_tray_connected(&app, &request.server_address);
                        let total = proxy_exes.len() + direct_exes.len() + block_exes.len();
                        return ConnectResult {
                            success: true,
                            message: format!(
                                "Server switched (TUN bridge preserved, {} app rules active)",
                                total
                            ),
                        };
                    }

                    if let Ok(_) = tun::start_tun_elevated(&tun_bridge) {
                        *engine = Some("singbox+app-proxy".into());
                        update_tray_connected(&app, &request.server_address);
                        let total = proxy_exes.len() + direct_exes.len() + block_exes.len();
                        return ConnectResult {
                            success: true,
                            message: format!("System Proxy + TUN app routing ({} rules: {} proxy, {} direct, {} block)", 
                                total, proxy_exes.len(), direct_exes.len(), block_exes.len()),
                        };
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
                }
            }
            Err(e) => ConnectResult {
                success: false,
                message: format!("Failed to start: {}", e),
            },
        }
    }
}

#[tauri::command]
async fn vpn_disconnect(app: tauri::AppHandle) -> ConnectResult {
    let is_connected = {
        let state = CONNECTION_STATE.lock().unwrap();
        *state
    };

    if !is_connected {
        return ConnectResult {
            success: true,
            message: "Already disconnected".into(),
        };
    }

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
            | Some("singbox+app-proxy")
            | Some("xray+tun")
            | Some("xray+app-proxy")
            | None
    );

    if had_tun {
        let _ = tun::stop_tun();
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

    clear_system_proxy_if_managed(false);

    let mut state = CONNECTION_STATE.lock().unwrap();
    *state = false;
    let mut engine = ACTIVE_ENGINE.lock().unwrap();
    *engine = None;
    update_tray_disconnected(&app);
    ConnectResult {
        success: true,
        message: "Disconnected".into(),
    }
}

#[tauri::command]
fn vpn_status() -> bool {
    let state = CONNECTION_STATE.lock().unwrap();
    *state
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

                            if !apps.contains_key(&name) {
                                apps.insert(name, exe_name);
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
                if let Ok(entries) = std::fs::read_dir(&local_dir) {
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
                            if !apps.values().any(|v| v.to_lowercase() == lower) {
                                apps.entry(display).or_insert(fname);
                            }
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
                                if !apps.values().any(|v| v.to_lowercase() == lower) {
                                    apps.entry(pkg_name.to_string()).or_insert(fname);
                                }
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

            let mut cmd = std::process::Command::new(&xray_exe);
            cmd.args(&["api", "statsquery", "-s", "127.0.0.1:10813", "-reset"]);
            #[cfg(windows)]
            cmd.creation_flags(0x08000000);
            if let Ok(output) = cmd.output() {
                let stdout = String::from_utf8_lossy(&output.stdout);
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
        cmd.args(&["-ano"]);
        cmd.creation_flags(0x08000000);
        if let Ok(output) = cmd.output() {
            let text = String::from_utf8_lossy(&output.stdout);
            let port_str = format!(":{}", port);
            for line in text.lines() {
                if line.contains(&port_str) && line.contains("LISTENING") {
                    if let Some(pid_str) = line.split_whitespace().last() {
                        if let Ok(pid) = pid_str.parse::<u32>() {
                            let mut proc_name = format!("PID {}", pid);
                            let mut info_cmd = std::process::Command::new("tasklist");
                            info_cmd.args(&[
                                "/FI",
                                &format!("PID eq {}", pid),
                                "/FO",
                                "CSV",
                                "/NH",
                            ]);
                            info_cmd.creation_flags(0x08000000);
                            if let Ok(info) = info_cmd.output() {
                                let info_text = String::from_utf8_lossy(&info.stdout);
                                if let Some(name) = info_text.split(',').next() {
                                    proc_name = name.trim().trim_matches('"').to_string();
                                }
                            }
                            return serde_json::json!({
                                "busy": true, "pid": pid, "process": proc_name,
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
            .args(&["-i", &format!(":{}", port), "-t"])
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
        cmd.args(&["-ano"]);
        cmd.creation_flags(0x08000000);
        if let Ok(output) = cmd.output() {
            let text = String::from_utf8_lossy(&output.stdout);
            let port_str = format!(":{}", port);
            for line in text.lines() {
                if line.contains(&port_str) && line.contains("LISTENING") {
                    if let Some(pid_str) = line.split_whitespace().last() {
                        if let Ok(pid) = pid_str.parse::<u32>() {
                            let mut kill = std::process::Command::new("taskkill");
                            kill.args(&["/PID", &pid.to_string(), "/F"]);
                            kill.creation_flags(0x08000000);
                            let _ = kill.output();
                            return format!("Killed PID {} on port {}", pid, port);
                        }
                    }
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(output) = std::process::Command::new("lsof")
            .args(&["-i", &format!(":{}", port), "-t"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some(pid_str) = text.lines().next() {
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    let _ = std::process::Command::new("kill")
                        .args(&["-9", &pid.to_string()])
                        .output();
                    return format!("Killed PID {} on port {}", pid, port);
                }
            }
        }
    }
    format!("Port {} is already free", port)
}

/// Fully quit the application (disconnect VPN, unset proxy, exit)
#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    let _ = singbox::stop_singbox();
    let _ = xray::stop_xray();
    let _ = tun::stop_tun();
    clear_system_proxy_if_managed(false);
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
fn wait_for_port_ready(port: u16) {
    use std::net::SocketAddr;
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    for _ in 0..20 {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    eprintln!("[warn] Port {} did not become ready in 2s", port);
}

/// Check connection health by testing if SOCKS port is alive
#[tauri::command]
fn check_connection_health(socks_port: u16) -> bool {
    use std::net::SocketAddr;
    let addr: SocketAddr = format!("127.0.0.1:{}", socks_port).parse().unwrap();
    TcpStream::connect_timeout(&addr, Duration::from_millis(2000)).is_ok()
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
                .args(&[
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
                .args(&["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
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
        for value_result in key.enum_values() {
            if let Ok((name, _)) = value_result {
                if name.to_lowercase() == dir_lower {
                    return true;
                }
            }
        }
    }

    // Method 2: Fallback to PowerShell (may need admin to list ExclusionPath)
    let mut cmd = std::process::Command::new("powershell");
    cmd.creation_flags(0x08000000);
    let output = cmd
        .args(&[
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
                    .args(&[
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
                    .args(&["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
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
                    .args(&["/Delete", "/TN", "DoodleRay_SilentStart", "/F"])
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
                    .args(&["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
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
    cmd.args(&["/Query", "/TN", "DoodleRay_SilentStart"]);
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
    let _ = singbox::stop_singbox();
    let _ = xray::stop_xray();
    let _ = tun::stop_tun();
    clear_system_proxy_if_managed(false);

    // Reset connection state
    if let Ok(mut state) = CONNECTION_STATE.lock() {
        *state = false;
    }
    if let Ok(mut engine) = ACTIVE_ENGINE.lock() {
        *engine = None;
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ── Startup cleanup ──
    // If previous session crashed, clean up orphaned processes and stale proxy
    // This runs BEFORE the UI loads, so the user never sees broken internet
    let _ = tun::stop_tun(); // Kill any orphaned sing-box.exe
    let _ = sysproxy::unset_system_proxy(); // Clear stale system proxy
    if let Ok(mut managed) = SYSTEM_PROXY_MANAGED.lock() {
        *managed = false;
    }

    // Ctrl+C handler (for dev mode)
    let _ = ctrlc::set_handler(move || {
        full_cleanup();
        std::process::exit(0);
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]), // launch minimized by default if started via autostart
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_http::init())
        .invoke_handler(tauri::generate_handler![
            vpn_connect,
            vpn_disconnect,
            vpn_status,
            ping_server,
            fetch_url,
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
            add_defender_exclusion,
            check_defender_exclusion,
            secure_store_get,
            secure_store_set,
            secure_store_delete,
        ])
        .setup(|app| {
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
        .run(|_app_handle, event| {
            // Catch ALL exit paths — OS shutdown, task manager kill, etc.
            if let tauri::RunEvent::Exit = event {
                full_cleanup();
            }
        });
}
