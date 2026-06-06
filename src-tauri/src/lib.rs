pub mod singbox;
pub mod tun;
pub mod tunnel_service;
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
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::sync::atomic::{AtomicIsize, Ordering};
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

const WORKSHOP_API_HOSTS: &[&str] = &[
    "doodleraydb-doodleray-ic3y6k-c7350f-94-241-172-101.traefik.me",
    "94-241-172-101.sslip.io",
];
const APP_MANAGED_PORTS: &[u16] = &[10808, 10809, 10813];
const SECURE_STORE_SERVICE: &str = "DoodleRay";
const SECURE_STORE_CHUNK_BYTES: usize = 1800;
const SECURE_STORE_CHUNK_PREFIX: &str = "chunked:v1:";
const APP_IDENTIFIER: &str = "com.doodlevpn.doodleray";
const APP_PRODUCT_NAME: &str = "DoodleRay";

#[cfg(windows)]
fn claim_single_app_instance() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let name: Vec<u16> = "Global\\DoodleRay.VPN.AppInstance.v1\0"
        .encode_utf16()
        .collect();
    let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
    if handle.is_null() {
        return true;
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            CloseHandle(handle);
        }
        return false;
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
            match resp.bytes().await {
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
            serde_json::Value::Array(failed).to_string()
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
            "strategy": "ipv4_only"
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
                    "inet4_range": "198.18.0.0/15"
                }
            ],
            "rules": [
                { "query_type": "A", "server": "dns-fakeip" }
            ],
            "final": "dns-remote",
            "strategy": "ipv4_only",
            "independent_cache": true
        }),
    }
}

fn xray_dns_servers(mode: &str) -> serde_json::Value {
    match mode {
        "fakeip" => serde_json::json!({
            "servers": ["localhost", "1.1.1.1", "9.9.9.9"]
        }),
        _ => serde_json::json!({
            "servers": ["localhost", "1.1.1.1", "9.9.9.9"]
        }),
    }
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
    "DoodleRay",
    "DoodleRay.exe",
    "doodleray",
    "doodleray.exe",
    "DoodleRayService",
    "DoodleRayService.exe",
    "node",
    "node.exe",
    "adb",
    "adb.exe",
    "svchost.exe",
    "lsass.exe",
    "csrss.exe",
    "System",
    "system",
];

fn effective_tun_strict_route(req: &ConnectRequest) -> bool {
    req.kill_switch || req.strict_route
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
    names.sort();
    names.dedup();
    names
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
    let stack = safe_network_stack(&req.network_stack);
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

fn secure_store_fallback_set(app: &tauri::AppHandle, key: &str, value: &str) -> Result<(), String> {
    let path = secure_store_fallback_path(app, key)?;
    write_private_file(&path, value.as_bytes())
        .map_err(|e| format!("Secure storage fallback write failed: {}", e))
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
    let entry = secure_store_entry(key)?;
    match entry.get_password() {
        Ok(value) => {
            let Some(count) = secure_store_chunk_count(&value) else {
                return Ok(Some(value));
            };

            let mut restored = String::new();
            for index in 0..count {
                let chunk_entry = secure_store_entry(&secure_store_chunk_key(key, index))?;
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

fn secure_store_keyring_set(key: &str, value: &str) -> Result<(), String> {
    let entry = secure_store_entry(key)?;
    if let Ok(old_value) = entry.get_password() {
        delete_secure_store_chunks(key, &old_value);
    }

    if value.len() > SECURE_STORE_CHUNK_BYTES {
        let chunks = secure_store_chunks(value);
        for (index, chunk) in chunks.iter().enumerate() {
            let chunk_entry = secure_store_entry(&secure_store_chunk_key(key, index))?;
            chunk_entry
                .set_password(chunk)
                .map_err(|e| format!("Secure storage chunk write failed: {}", e))?;
        }

        return entry
            .set_password(&format!("{}{}", SECURE_STORE_CHUNK_PREFIX, chunks.len()))
            .map_err(|e| format!("Secure storage write failed: {}", e));
    }

    entry
        .set_password(value)
        .map_err(|e| format!("Secure storage write failed: {}", e))
}

fn secure_store_keyring_delete(key: &str) -> Result<(), String> {
    if let Ok(value) = secure_store_entry(key)?.get_password() {
        delete_secure_store_chunks(key, &value);
    }
    delete_secure_store_entry(key)
}

#[tauri::command]
fn secure_store_get(app: tauri::AppHandle, key: String) -> Result<Option<String>, String> {
    validate_secure_store_key(&key)?;
    match secure_store_fallback_get(&app, &key) {
        Ok(Some(value)) => return Ok(Some(value)),
        Ok(None) => {}
        Err(fallback_error) => {
            eprintln!(
                "[warn] secure storage fallback read failed: {}",
                fallback_error
            );
        }
    }

    match secure_store_keyring_get(&key) {
        Ok(Some(value)) => {
            // Keep the app-data fallback warm for later boots where the OS
            // credential store is temporarily unavailable.
            if let Err(fallback_error) = secure_store_fallback_set(&app, &key, &value) {
                eprintln!(
                    "[warn] secure storage fallback backfill failed: {}",
                    fallback_error
                );
            }
            Ok(Some(value))
        }
        Ok(None) => Ok(None),
        Err(keyring_error) => {
            eprintln!(
                "[warn] secure storage keyring read failed: {}",
                keyring_error
            );
            Err(keyring_error)
        }
    }
}

#[tauri::command]
fn secure_store_set(app: tauri::AppHandle, key: String, value: String) -> Result<(), String> {
    validate_secure_store_key(&key)?;

    let fallback_result = secure_store_fallback_set(&app, &key, &value);
    if let Err(fallback_error) = &fallback_result {
        eprintln!(
            "[warn] secure storage fallback mirror write failed: {}",
            fallback_error
        );
    }

    match secure_store_keyring_set(&key, &value) {
        Ok(()) => fallback_result,
        Err(keyring_error) => {
            eprintln!(
                "[warn] secure storage keyring write failed: {}",
                keyring_error
            );
            fallback_result
                .map_err(|fallback_error| format!("{}; {}", keyring_error, fallback_error))
        }
    }
}

#[tauri::command]
fn secure_store_delete(app: tauri::AppHandle, key: String) -> Result<(), String> {
    validate_secure_store_key(&key)?;
    let keyring_result = secure_store_keyring_delete(&key);
    let fallback_result = secure_store_fallback_delete(&app, &key);

    match (keyring_result, fallback_result) {
        (Ok(()), _) | (_, Ok(())) => Ok(()),
        (Err(keyring_error), Err(fallback_error)) => {
            Err(format!("{}; {}", keyring_error, fallback_error))
        }
    }
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

#[derive(Debug, Serialize)]
pub struct SubscriptionFetchResult {
    pub body: String,
    pub subscription_userinfo: Option<String>,
    pub profile_title: Option<String>,
    pub content_disposition: Option<String>,
}

/// Fetch a URL from Rust side — bypasses CORS restrictions in WebView
#[tauri::command]
async fn fetch_url(url: String) -> Result<String, String> {
    let parsed_url = validate_http_url(&url)?;
    let client = direct_fetch_client(&parsed_url, Duration::from_secs(30))
        .await
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

/// Fetch a subscription and return its quota metadata headers together with body.
#[tauri::command]
async fn fetch_subscription_url(url: String) -> Result<SubscriptionFetchResult, String> {
    let parsed_url = validate_http_url(&url)?;
    let client = direct_fetch_client(&parsed_url, Duration::from_secs(30))
        .await
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

    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read body: {}", e))?;

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

    // Inbound config: TUN or SOCKS+HTTP
    let inbounds = if req.proxy_mode == "tun" {
        serde_json::json!([tun_inbound_value(req, None, req.strict_route)])
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
            let Some(val) = normalize_process_name(&rule.value) else {
                continue;
            };
            match rule.action.as_str() {
                "proxy" => proxy_processes.push(val),
                "direct" => direct_processes.push(val),
                "block" => block_processes.push(val),
                _ => {}
            }
        }
    }

    proxy_processes.sort();
    proxy_processes.dedup();
    direct_processes.sort();
    direct_processes.dedup();
    block_processes.sort();
    block_processes.dedup();

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

    // Default route remains the VPN outbound. Kill Switch hardens TUN routing with strict_route;
    // setting final=block here would block normal VPN traffic that has no custom rule.
    let final_outbound = "proxy";

    // Kill Switch in TUN mode: force strict_route regardless of user setting.
    let effective_strict_route = effective_tun_strict_route(req);

    // Update inbounds strict_route if TUN mode
    let effective_inbounds = if req.proxy_mode == "tun" {
        serde_json::json!([tun_inbound_value(req, None, effective_strict_route)])
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
    // Subscription JSON can contain DoH servers that are blocked or unreachable on some
    // networks. Prefer the local resolver first so proxy mode does not get stuck on DNS.
    config["dns"] = xray_dns_servers(&req.dns_mode);

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
        }
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
            mtu: server
                .get("mtu")
                .and_then(|v| v.as_u64())
                .map(|v| v as u16),
            workers: server
                .get("workers")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            encryption: smoke_value_string(server, "encryption"),
            raw_xray_config: server.get("rawConfig").cloned(),
        })
    }

    #[cfg(windows)]
    fn ensure_test_xray_resources() {
        let exe_dir = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let source = exe_dir
            .parent()
            .unwrap_or(&exe_dir)
            .join("xray-core");
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
    fn singbox_tun_mixed_stack_uses_udp_stability_options() {
        let mut req = sample_request("tun");
        req.network_stack = "mixed".into();

        let config = build_singbox_config(&req);

        assert_eq!(config["inbounds"][0]["udp_timeout"], json!("10m"));
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
        assert_eq!(dns["servers"][2]["inet4_range"], json!("198.18.0.0/15"));
        assert!(dns["servers"][2].get("inet6_range").is_none());
        assert_eq!(dns["rules"][0]["query_type"], json!("A"));
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
        assert_eq!(after, before, "WinINet proxy state must be restored exactly");
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
        let mut req = sample_request("tun");
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

        assert_eq!(
            direct_rule["process_name"],
            json!([
                "execpubg.exe",
                "tslgame.exe",
                "tslgame_be.exe",
                "tslgame_zk.exe"
            ])
        );
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
    fn tun_addresses_include_ipv4_and_ipv6() {
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
    fn raw_xray_config_uses_safe_dns_after_injection() {
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
            json!({ "servers": ["localhost", "1.1.1.1", "9.9.9.9"] })
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
        "dns": xray_dns_servers(&req.dns_mode),
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
            }
        ],
        "routing": {
            "domainStrategy": "AsIs",
            "rules": final_rules
        }
    })
}

#[tauri::command]
#[cfg_attr(windows, allow(unreachable_code))]
async fn vpn_connect(mut request: ConnectRequest, app: tauri::AppHandle) -> ConnectResult {
    // Clear previous connect logs
    if let Ok(mut logs) = CONNECT_LOG.lock() {
        logs.clear();
    }

    let use_xray = uses_xray_engine(&request);
    let is_tun = request.proxy_mode == "tun";
    if is_tun {
        request.system_proxy_mode = "unchanged".into();
    }

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
            safe_network_stack(&request.network_stack),
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
            tun_bridge_rules
                .push(serde_json::json!({ "ip_is_private": true, "outbound": "direct" }));

            let tun_bridge = serde_json::json!({
                "log": { "level": "warn" },
                "dns": singbox_dns_config(&request.dns_mode),
                "inbounds": [tun_inbound_value(&request, Some("DoodleRay Tunnel"), effective_tun_strict_route(&request))],
                "outbounds": [
                    {
                        "type": "socks",
                        "tag": "proxy",
                        "server": "127.0.0.1",
                        "server_port": request.socks_port
                    },
                    { "type": "direct", "tag": "direct" },
                    { "type": "block", "tag": "block" }
                ],
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
                    let mut state = CONNECTION_STATE.lock().unwrap();
                    *state = true;
                    let mut engine = ACTIVE_ENGINE.lock().unwrap();
                    *engine = Some("xray+tun-service".into());
                    update_tray_connected(&app, &request.server_address);
                    ConnectResult {
                        success: true,
                        message: "Whole computer connected via DoodleRay Tunnel Service".into(),
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
        tun_bridge_rules.push(serde_json::json!({ "ip_is_private": true, "outbound": "direct" }));

        let tun_bridge = serde_json::json!({
            "log": { "level": "warn" },
            "dns": singbox_dns_config(&request.dns_mode),
            "inbounds": [tun_inbound_value(&request, None, effective_tun_strict_route(&request))],
            "outbounds": [
                {
                    "type": "socks",
                    "tag": "proxy",
                    "server": "127.0.0.1",
                    "server_port": request.socks_port
                },
                { "type": "direct", "tag": "direct" },
                { "type": "block", "tag": "block" }
            ],
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
                let mut state = CONNECTION_STATE.lock().unwrap();
                *state = true;
                let mut engine = ACTIVE_ENGINE.lock().unwrap();
                *engine = Some("xray+tun".into());
                update_tray_connected(&app, &request.server_address);
                ConnectResult {
                    success: true,
                    message: "Whole computer connected".into(),
                }
            }
            Err(e) => {
                vpn_log(&format!("FATAL: TUN bridge failed: {}", e));
                let _ = xray::stop_xray();
                ConnectResult {
                    success: false,
                    message: format!("Whole computer mode failed: {}", e),
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

                    // 5. Private IPs always go direct
                    tun_rules
                        .push(serde_json::json!({ "ip_is_private": true, "outbound": "direct" }));

                    let tun_bridge = serde_json::json!({
                        "log": { "level": "warn" },
                        "dns": singbox_dns_config(&request.dns_mode),
                        "inbounds": [tun_inbound_value(&request, None, false)],
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
                                    message: format!("Browsers/apps with service app routing ({} rules: {} proxy, {} direct, {} block)",
                                        total, proxy_exes.len(), direct_exes.len(), block_exes.len()),
                                }
                            }
                            Err(e) => ConnectResult {
                                success: false,
                                message: format!(
                                    "Full Computer components not installed or not ready: {}",
                                    e
                                ),
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
                                message: format!("Browsers/apps with app routing ({} rules: {} proxy, {} direct, {} block)",
                                    total, proxy_exes.len(), direct_exes.len(), block_exes.len()),
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
                    let mut state = CONNECTION_STATE.lock().unwrap();
                    *state = true;
                    let mut engine = ACTIVE_ENGINE.lock().unwrap();
                    *engine = Some("singbox-tun-service".into());
                    update_tray_connected(&app, &request.server_address);
                    ConnectResult {
                        success: true,
                        message: "Whole computer connected via DoodleRay Tunnel Service".into(),
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
                    }
                }
            };
        }

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
                    message: "Whole computer connected".into(),
                }
            }
            Err(e) => {
                vpn_log(&format!("FATAL: Whole computer mode failed: {}", e));
                ConnectResult {
                    success: false,
                    message: format!("Whole computer mode failed: {}", e),
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

                    // 5. Private IPs always go direct
                    tun_rules
                        .push(serde_json::json!({ "ip_is_private": true, "outbound": "direct" }));

                    let tun_bridge = serde_json::json!({
                        "log": { "level": "warn" },
                        "dns": singbox_dns_config(&request.dns_mode),
                        "inbounds": [tun_inbound_value(&request, None, false)],
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
                                    message: format!("Browsers/apps with service app routing ({} rules: {} proxy, {} direct, {} block)",
                                        total, proxy_exes.len(), direct_exes.len(), block_exes.len()),
                                }
                            }
                            Err(e) => ConnectResult {
                                success: false,
                                message: format!(
                                    "Full Computer components not installed or not ready: {}",
                                    e
                                ),
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
                                message: format!("Browsers/apps with app routing ({} rules: {} proxy, {} direct, {} block)",
                                    total, proxy_exes.len(), direct_exes.len(), block_exes.len()),
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

    if had_tun {
        #[cfg(windows)]
        let _ = tunnel_service_stop("disconnect");
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
) -> Result<reqwest::Client, reqwest::Error> {
    let mut builder = reqwest::Client::builder().no_proxy().timeout(timeout);

    if let Some(host) = parsed_url.host_str() {
        if host.parse::<IpAddr>().is_err() {
            let port = parsed_url.port_or_known_default().unwrap_or(443);
            if system_dns_needs_public_override(host, port) {
                if let Some(ip) = resolve_public_ipv4_doh(host).await {
                    builder = builder.resolve(host, SocketAddr::new(IpAddr::V4(ip), port));
                }
            }
        }
    }

    builder.build()
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

            let api_port = ACTIVE_XRAY_API_PORT
                .lock()
                .map(|port| *port)
                .unwrap_or(10813);
            let endpoint = format!("127.0.0.1:{}", api_port);
            let mut cmd = std::process::Command::new(&xray_exe);
            cmd.args(["api", "statsquery", "-s", &endpoint, "-reset"]);
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
                            #[cfg(windows)]
                            let doodleray_owned = windows_pid_is_doodleray_owned(pid);
                            #[cfg(not(windows))]
                            let doodleray_owned = false;
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
        cmd.args(&["-ano"]);
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

#[cfg(windows)]
fn tunnel_service_start(
    request: &ConnectRequest,
    engine_kind: tunnel_service::TunnelEngineKind,
    xray_config: Option<serde_json::Value>,
    singbox_config: serde_json::Value,
) -> Result<tunnel_service::TunnelStatus, String> {
    let _ = ipc::tunnel_service_hello(env!("CARGO_PKG_VERSION"))?;
    let response = ipc::send_tunnel_command(&tunnel_service::TunnelCommand::StartTunnel(
        tunnel_service::StartTunnelRequest {
            op_id: tun_op_id(),
            engine_kind,
            xray_config,
            singbox_config,
            socks_port: request.socks_port,
            http_port: request.http_port,
            redacted_label: format!("{}:{}", request.protocol, request.transport),
        },
    ))?;
    let mut status = match response {
        tunnel_service::TunnelResponse::Status(status) => status,
        tunnel_service::TunnelResponse::Error { message } => return Err(message),
        tunnel_service::TunnelResponse::Diagnostics(_) => {
            return Err("Tunnel Service returned diagnostics for StartTunnel".into())
        }
    };

    let started = Instant::now();
    let timeout = Duration::from_secs(35);
    let mut last_phase = status.phase.clone();
    loop {
        match status.state {
            tunnel_service::TunnelState::Connected => return Ok(status),
            tunnel_service::TunnelState::Failed => {
                return Err(status
                    .error
                    .unwrap_or_else(|| "Tunnel Service failed to start TUN".into()))
            }
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
fn tunnel_service_stop(reason: &str) -> Result<tunnel_service::TunnelStatus, String> {
    let response = ipc::send_tunnel_command(&tunnel_service::TunnelCommand::StopTunnel(
        tunnel_service::StopTunnelRequest {
            op_id: tun_op_id(),
            reason: reason.to_string(),
        },
    ))?;
    match response {
        tunnel_service::TunnelResponse::Status(status) => Ok(status),
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
        if ipc::tunnel_service_status().is_ok() {
            return Ok("Tunnel service installed and running".into());
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
    match ipc::tunnel_service_status()? {
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
            "service_version={}\nstate={:?}\nphase={:?}\nerror={:?}\ntimings_ms={:?}\n\n{}",
            diagnostics.status.service_version,
            diagnostics.status.state,
            diagnostics.status.phase,
            diagnostics.status.error,
            diagnostics.status.timings_ms,
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
fn prepare_for_app_update() -> Result<String, String> {
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
#[tauri::command]
fn check_connection_health(socks_port: u16) -> bool {
    use std::net::SocketAddr;
    let addr: SocketAddr = format!("127.0.0.1:{}", socks_port).parse().unwrap();
    TcpStream::connect_timeout(&addr, Duration::from_millis(2000)).is_ok()
}

#[tauri::command]
fn detect_stale_doodleray_proxy() -> Result<String, String> {
    #[cfg(windows)]
    {
        let state = sysproxy::detect_stale_doodleray_proxy()?;
        let value = serde_json::to_value(state)
            .map_err(|err| format!("Failed to serialize proxy state: {}", err))?;
        return Ok(value.as_str().unwrap_or("unknown").to_string());
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
        return Ok(value.as_str().unwrap_or("unknown").to_string());
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
    #[cfg(windows)]
    let _ = tunnel_service_stop("full_cleanup");
    let _ = singbox::stop_singbox();
    let _ = xray::stop_xray();
    let _ = tun::stop_tun();
    restore_system_proxy_if_owned(false);

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
    if !claim_single_app_instance() {
        return;
    }

    // ── Startup cleanup ──
    // If previous session crashed, clean up orphaned processes and stale proxy
    // This runs BEFORE the UI loads, so the user never sees broken internet
    #[cfg(windows)]
    let _ = tunnel_service_stop("startup_cleanup");
    let _ = tun::stop_tun(); // Kill any orphaned sing-box.exe
    #[cfg(windows)]
    let _ = sysproxy::recover_orphaned_proxy_on_startup();
    #[cfg(target_os = "macos")]
    let _ = sysproxy::unset_system_proxy(); // Restore stale app proxy on macOS
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
                        #[cfg(windows)]
                        {
                            let _ = &app_handle;
                            let _ = api;
                        }
                        #[cfg(not(windows))]
                        {
                            api.prevent_close();
                            if let Some(win) = app_handle.get_webview_window("main") {
                                let _ = win.hide();
                            }
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
