pub mod singbox;
pub mod xray;
pub mod tun;

#[cfg(windows)]
pub mod sysproxy;
#[cfg(windows)]
pub mod ipc;

#[cfg(target_os = "macos")]
#[path = "sysproxy_macos.rs"]
pub mod sysproxy;

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::net::TcpStream;
use std::time::{Duration, Instant};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Manager,
};

// Global connection state
static CONNECTION_STATE: Mutex<bool> = Mutex::new(false);
// Track which engine is active: "singbox" or "xray"
static ACTIVE_ENGINE: Mutex<Option<String>> = Mutex::new(None);

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
    pub socks_port: u16,
    pub http_port: u16,
    pub network_stack: String, 
    pub dns_mode: String,
    pub strict_route: bool,
    #[serde(default)]
    pub routing_rules: Vec<RoutingRuleRequest>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RoutingRuleRequest {
    pub rule_type: String,   // "domain" or "exe"
    pub value: String,       // "youtube.com", "steam.exe"
    pub action: String,      // "proxy", "direct", "block"
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
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;
    
    let response = client
        .get(&url)
        .header("User-Agent", "DoodleRay/2.0")
        .send()
        .await
        .map_err(|e| format!("Fetch failed: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("HTTP {}: {}", response.status().as_u16(), response.status().as_str()));
    }
    
    response
        .text()
        .await
        .map_err(|e| format!("Failed to read body: {}", e))
}

/// Workshop API proxy — supports GET/POST with SSL bypass
#[tauri::command]
async fn workshop_api(url: String, method: String, body: Option<String>) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;
    
    let req = if method.to_uppercase() == "POST" {
        let mut r = client.post(&url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "DoodleRay/2.0");
        if let Some(b) = body {
            r = r.body(b);
        }
        r
    } else {
        client.get(&url).header("User-Agent", "DoodleRay/2.0")
    };
    
    let response = req.send().await
        .map_err(|e| format!("Request failed: {}", e))?;
    
    response.text().await
        .map_err(|e| format!("Failed to read body: {}", e))
}

/// Real TCP ping — measures connection time to server:port
/// Uses tokio spawn_blocking to avoid blocking the async runtime,
/// and performs a raw TCP connect that bypasses any local proxy/TUN.
#[tauri::command]
async fn ping_server(address: String, port: u16, server_id: String) -> PingResult {
    let sid = server_id.clone();
    let result = tokio::task::spawn_blocking(move || {
        // Resolve DNS first (not timed)
        let addr = format!("{}:{}", address, port);
        let sock_addr = match std::net::ToSocketAddrs::to_socket_addrs(&addr) {
            Ok(mut addrs) => match addrs.next() {
                Some(a) => a,
                None => return -1i32,
            },
            Err(_) => return -1i32,
        };
        
        // Multiple attempts, take the best (most accurate)
        let mut best = i32::MAX;
        for _ in 0..3 {
            let start = Instant::now();
            match TcpStream::connect_timeout(&sock_addr, Duration::from_secs(3)) {
                Ok(conn) => {
                    let ms = start.elapsed().as_millis() as i32;
                    drop(conn);
                    if ms < best { best = ms; }
                }
                Err(_) => {}
            }
        }
        if best == i32::MAX { -1 } else { best }
    }).await.unwrap_or(-1);
    
    PingResult { server_id: sid, ping_ms: result }
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
            serde_json::json!({
            "type": "vless",
            "tag": "proxy",
            "server": req.server_address,
            "server_port": req.server_port,
            "uuid": req.uuid.clone().unwrap_or_default(),
            "flow": flow_value,
            "tls": {
                "enabled": true,
                "server_name": req.sni.clone().unwrap_or(req.server_address.clone()),
                "utls": {
                    "enabled": true,
                    "fingerprint": req.fingerprint.clone().unwrap_or("chrome".into())
                },
                "reality": if req.security == "reality" {
                    serde_json::json!({
                        "enabled": true,
                        "public_key": req.public_key.clone().unwrap_or_default(),
                        "short_id": req.short_id.clone().unwrap_or_default()
                    })
                } else {
                    serde_json::json!(null)
                }
            },
            "transport": match req.transport.as_str() {
                "ws" => serde_json::json!({
                    "type": "ws",
                    "path": req.path.clone().unwrap_or("/".into()),
                    "headers": {
                        "Host": req.host.clone().unwrap_or(req.server_address.clone())
                    }
                }),
                "grpc" => serde_json::json!({
                    "type": "grpc",
                    "service_name": req.path.clone().unwrap_or_default()
                }),
                "xhttp" | "httpupgrade" => serde_json::json!({
                    "type": "http",
                    "path": req.path.clone().unwrap_or("/".into()),
                    "host": [req.host.clone().unwrap_or(req.server_address.clone())]
                }),
                "h2" | "http" => serde_json::json!({
                    "type": "http",
                    "path": req.path.clone().unwrap_or("/".into()),
                    "host": [req.host.clone().unwrap_or(req.server_address.clone())]
                }),
                _ => serde_json::json!(null)
            }
        })
        },
        "vmess" => serde_json::json!({
            "type": "vmess",
            "tag": "proxy",
            "server": req.server_address,
            "server_port": req.server_port,
            "uuid": req.uuid.clone().unwrap_or_default(),
            "security": "auto",
            "tls": {
                "enabled": req.security == "tls",
                "server_name": req.sni.clone().unwrap_or(req.server_address.clone()),
            },
            "transport": match req.transport.as_str() {
                "ws" => serde_json::json!({
                    "type": "ws",
                    "path": req.path.clone().unwrap_or("/".into()),
                }),
                "grpc" => serde_json::json!({
                    "type": "grpc",
                    "service_name": req.path.clone().unwrap_or_default()
                }),
                "xhttp" | "httpupgrade" => serde_json::json!({
                    "type": "http",
                    "path": req.path.clone().unwrap_or("/".into()),
                    "host": [req.host.clone().unwrap_or(req.server_address.clone())]
                }),
                "h2" | "http" => serde_json::json!({
                    "type": "http",
                    "path": req.path.clone().unwrap_or("/".into()),
                    "host": [req.host.clone().unwrap_or(req.server_address.clone())]
                }),
                _ => serde_json::json!(null)
            }
        }),
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
            "method": "aes-256-gcm"
        }),
        _ => serde_json::json!({
            "type": "direct",
            "tag": "proxy"
        })
    };

    // DNS config — sing-box 1.13+ format
    let dns = serde_json::json!({
        "servers": [
            {
                "tag": "dns-remote",
                "type": "udp",
                "server": "8.8.8.8",
                "detour": "proxy"
            },
            {
                "tag": "dns-direct",
                "type": "udp",
                "server": "8.8.4.4"
            }
        ],
        "final": "dns-remote",
        "strategy": "prefer_ipv4"
    });

    // Inbound config: TUN or SOCKS+HTTP
    let inbounds = if req.proxy_mode == "tun" {
        serde_json::json!([
            {
                "type": "tun",
                "tag": "tun-in",
                "address": ["172.19.0.1/30", "fdfe:dcba:9876::1/126"],
                "auto_route": true,
                "strict_route": req.strict_route,
                "stack": "system"
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

    serde_json::json!({
        "log": { "level": "info" },
        "dns": dns,
        "inbounds": inbounds,
        "outbounds": [
            outbound,
            { "type": "direct", "tag": "direct" }
        ],
        "route": {
            "auto_detect_interface": true,
            "default_domain_resolver": "dns-direct",
            "rules": [
                { "action": "sniff" },
                { "protocol": "dns", "action": "hijack-dns" }
            ]
        }
    })
}

/// Build the xray-core JSON config (for xhttp transport)
fn build_xray_config(req: &ConnectRequest) -> serde_json::Value {
    let flow_value = if req.transport == "tcp" || req.transport == "xhttp" || req.transport.is_empty() {
        req.flow.clone().unwrap_or_default()
    } else {
        String::new()
    };
    
    let outbound_settings = serde_json::json!({
        "vnext": [{
            "address": req.server_address,
            "port": req.server_port,
            "users": [{
                "id": req.uuid.clone().unwrap_or_default(),
                "encryption": "none",
                "flow": flow_value
            }]
        }]
    });

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
        })
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
    let mut final_rules = vec![
        serde_json::json!({
            "type": "field",
            "inboundTag": ["api"],
            "outboundTag": "api"
        })
    ];
    // DNS port 53 rule — so TUN mode DNS queries get resolved by xray instead of going to "direct"
    final_rules.insert(1, serde_json::json!({
        "type": "field",
        "port": "53",
        "outboundTag": "dns-out"
    }));
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
        "dns": {
            "servers": ["8.8.8.8", "8.8.4.4"]
        },
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
                "protocol": "vless",
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
    let use_xray = request.transport == "xhttp";
    let is_tun = request.proxy_mode == "tun";
    
    let debug_path = std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("doodleray_debug_config.json");
    
    // Stop any previous engines
    let _ = singbox::stop_singbox();
    let _ = xray::stop_xray();
    let _ = tun::stop_tun();
    let _ = sysproxy::unset_system_proxy();
    // Wait for ports to be released
    std::thread::sleep(std::time::Duration::from_millis(500));
    
    if use_xray && is_tun {
        // ═══ xhttp + TUN: xray-core (SOCKS5) + sing-box (TUN bridge) ═══
        let xray_config = build_xray_config(&request);
        let _ = std::fs::write(&debug_path, serde_json::to_string_pretty(&xray_config).unwrap_or_default());
        
        if let Err(e) = xray::start_xray(&xray_config) {
            return ConnectResult { success: false, message: format!("Failed to start xray-core: {}", e) };
        }
        
        // sing-box as TUN bridge → routes all traffic to xray's SOCKS5
        // sing-box v1.13+ config format
        let tun_bridge = serde_json::json!({
            "log": { "level": "info" },
            "dns": {
                "servers": [
                    {
                        "tag": "dns-remote",
                        "type": "udp",
                        "server": "8.8.8.8",
                        "detour": "proxy"
                    },
                    {
                        "tag": "dns-direct",
                        "type": "udp",
                        "server": "1.1.1.1"
                    }
                ],
                "final": "dns-remote",
                "strategy": "prefer_ipv4"
            },
            "inbounds": [{
                "type": "tun",
                "tag": "tun-in",
                "interface_name": "tun0",
                "address": ["172.19.0.1/30"],
                "auto_route": true,
                "strict_route": false,
                "stack": "mixed",
                "sniff": true,
                "sniff_override_destination": true
            }],
            "outbounds": [
                {
                    "type": "socks",
                    "tag": "proxy",
                    "server": "127.0.0.1",
                    "server_port": request.socks_port,
                    "udp_over_tcp": true
                },
                { "type": "direct", "tag": "direct" }
            ],
            "route": {
                "auto_detect_interface": true,
                "default_domain_resolver": "dns-direct",
                "rules": [
                    { "action": "sniff" },
                    { "protocol": "dns", "action": "hijack-dns" },
                    { "process_name": ["sing-box.exe", "xray.exe", "DoodleRay.exe", "node.exe"], "outbound": "direct" },
                    { "ip_is_private": true, "outbound": "direct" }
                ]
            }
        });
        
        match tun::start_tun_elevated(&tun_bridge) {
            Ok(_) => {
                let mut state = CONNECTION_STATE.lock().unwrap();
                *state = true;
                let mut engine = ACTIVE_ENGINE.lock().unwrap();
                *engine = Some("xray+tun".into());
                update_tray_connected(&app);
                ConnectResult {
                    success: true,
                    message: "TUN connected (xray-core + sing-box TUN bridge)".into(),
                }
            }
            Err(e) => {
                let _ = xray::stop_xray();
                ConnectResult {
                    success: false,
                    message: format!("TUN failed: {}", e),
                }
            }
        }
    } else if use_xray {
        // ═══ xhttp + System Proxy ═══
        // Sets Windows system proxy → browsers, Electron apps (Discord, Telegram Desktop) 
        // will use it. UDP (Discord voice) won't go through proxy.
        let xray_config = build_xray_config(&request);
        let _ = std::fs::write(&debug_path, serde_json::to_string_pretty(&xray_config).unwrap_or_default());
        
        match xray::start_xray(&xray_config) {
            Ok(_) => {
                let mut state = CONNECTION_STATE.lock().unwrap();
                *state = true;
                let mut engine = ACTIVE_ENGINE.lock().unwrap();
                *engine = Some("xray".into());
                if let Err(e) = sysproxy::set_system_proxy(request.http_port) {
                    return ConnectResult { success: false, message: format!("xray started but failed to set system proxy: {}", e) };
                }
                
                // If Workshop has exe-type rules, start per-app TUN bridge
                // so those apps get full TCP+UDP proxying (e.g. Discord voice)
                let proxy_exes: Vec<String> = request.routing_rules.iter()
                    .filter(|r| r.rule_type == "exe" && r.action == "proxy")
                    .map(|r| r.value.clone())
                    .collect();
                
                if !proxy_exes.is_empty() {
                    let exclude = vec!["sing-box.exe", "xray.exe", "DoodleRay.exe"];
                    let exclude_val: Vec<serde_json::Value> = exclude.iter()
                        .map(|s| serde_json::Value::String(s.to_string())).collect();
                    let proxy_val: Vec<serde_json::Value> = proxy_exes.iter()
                        .map(|s| serde_json::Value::String(s.to_string())).collect();
                    
                    let tun_bridge = serde_json::json!({
                        "log": { "level": "info" },
                        "dns": {
                            "servers": [
                                {
                                    "tag": "dns-direct",
                                    "type": "tcp",
                                    "server": "8.8.8.8"
                                }
                            ],
                            "strategy": "prefer_ipv4"
                        },
                        "inbounds": [{
                            "type": "tun",
                            "tag": "tun-in",
                            "address": ["172.19.0.1/30"],
                            "auto_route": true,
                            "strict_route": false,
                            "stack": "mixed"
                        }],
                        "outbounds": [
                            { "type": "direct", "tag": "direct" },
                            {
                                "type": "socks",
                                "tag": "proxy",
                                "server": "127.0.0.1",
                                "server_port": request.socks_port
                            }
                        ],
                        "route": {
                            "auto_detect_interface": true,
                            "default_domain_resolver": "dns-direct",
                            "rules": [
                                { "process_name": exclude_val, "outbound": "direct" },
                                { "process_name": proxy_val, "outbound": "proxy" }
                            ]
                        }
                    });
                    
                    if let Ok(_) = tun::start_tun_elevated(&tun_bridge) {
                        *engine = Some("xray+app-proxy".into());
                        update_tray_connected(&app);
                        return ConnectResult {
                            success: true,
                            message: format!("System Proxy + {} apps with UDP proxy", proxy_exes.len()),
                        };
                    }
                }
                
                update_tray_connected(&app);
                ConnectResult {
                    success: true,
                    message: format!("Connected via System Proxy. SOCKS5: 127.0.0.1:{}, HTTP: 127.0.0.1:{}", request.socks_port, request.http_port),
                }
            }
            Err(e) => ConnectResult { success: false, message: format!("Failed to start xray-core: {}", e) }
        }
    } else if is_tun {
        // ═══ Non-xhttp + TUN ═══
        let config = build_singbox_config(&request);
        let _ = std::fs::write(&debug_path, serde_json::to_string_pretty(&config).unwrap_or_default());
        
        match tun::start_tun_elevated(&config) {
            Ok(_) => {
                let mut state = CONNECTION_STATE.lock().unwrap();
                *state = true;
                let mut engine = ACTIVE_ENGINE.lock().unwrap();
                *engine = Some("singbox-tun".into());
                update_tray_connected(&app);
                ConnectResult {
                    success: true,
                    message: "TUN connected via sing-box".into(),
                }
            }
            Err(e) => ConnectResult {
                success: false,
                message: format!("TUN failed: {}", e),
            }
        }
    } else {
        // ═══ Non-xhttp + System Proxy ═══
        let config = build_singbox_config(&request);
        let _ = std::fs::write(&debug_path, serde_json::to_string_pretty(&config).unwrap_or_default());
        
        match singbox::start_singbox(&config) {
            Ok(_) => {
                let mut state = CONNECTION_STATE.lock().unwrap();
                *state = true;
                let mut engine = ACTIVE_ENGINE.lock().unwrap();
                *engine = Some("singbox".into());
                if let Err(e) = sysproxy::set_system_proxy(request.http_port) {
                    return ConnectResult { success: false, message: format!("sing-box started but failed to set system proxy: {}", e) };
                }
                update_tray_connected(&app);
                ConnectResult {
                    success: true,
                    message: format!("System proxy active. SOCKS5: 127.0.0.1:{}, HTTP: 127.0.0.1:{}", request.socks_port, request.http_port),
                }
            }
            Err(e) => ConnectResult { success: false, message: format!("Failed to start: {}", e) }
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

    // Stop both engines (whichever is active)
    let active = {
        let engine = ACTIVE_ENGINE.lock().unwrap();
        engine.clone()
    };
    
    match active.as_deref() {
        Some("xray") => {
            let _ = xray::stop_xray();
        }
        Some("xray+tun") => {
            let _ = tun::stop_tun();
            let _ = xray::stop_xray();
        }
        Some("singbox-tun") => {
            let _ = tun::stop_tun();
        }
        _ => {
            let _ = singbox::stop_singbox();
        }
    }
    
    // Unset Windows system proxy
    let _ = sysproxy::unset_system_proxy();
    
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

/// Check if we're running with Administrator privileges
#[tauri::command]
fn is_admin() -> bool {
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
        // TOKEN_QUERY = 0x0008
        if OpenProcessToken(GetCurrentProcess(), 0x0008, &mut token) == 0 {
            return false;
        }

        // TokenElevation = 20
        let mut elevation: u32 = 0;
        let mut return_length: u32 = 0;
        let result = GetTokenInformation(
            token,
            20, // TokenElevation
            &mut elevation as *mut u32 as *mut std::ffi::c_void,
            mem::size_of::<u32>() as u32,
            &mut return_length,
        );
        CloseHandle(token);
        
        result != 0 && elevation != 0
    }
}

/// Relaunch the app as Administrator (triggers UAC prompt)
#[tauri::command]
fn restart_as_admin() -> Result<(), String> {
    let exe_path = std::env::current_exe()
        .map_err(|e| format!("Failed to get exe path: {}", e))?;
    
    let exe_str: Vec<u16> = exe_path.to_string_lossy()
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
            1, // SW_SHOWNORMAL
        );
        
        if result as usize <= 32 {
            return Err("User declined UAC or ShellExecute failed".into());
        }
    }
    
    // Exit current (non-admin) process
    std::process::exit(0);
}

/// Returns new proxy log lines from xray-core since last call
#[tauri::command]
fn get_proxy_logs() -> Vec<String> {
    xray::get_new_logs()
}

/// Get real traffic stats from xray-core stats API (byte counters)
#[tauri::command]
async fn get_traffic_stats() -> serde_json::Value {
    let is_connected = {
        let state = CONNECTION_STATE.lock().unwrap();
        *state
    };
    if !is_connected {
        return serde_json::json!({ "download": 0, "upload": 0 });
    }

    let exe_dir = std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let xray_exe = exe_dir.join("xray-core").join("xray.exe");
    
    if !xray_exe.exists() {
        // Fallback to log-based estimation
        let logs = xray::get_recent_activity();
        return serde_json::json!({ "download": logs.0, "upload": logs.1 });
    }

    let mut dl: i64 = 0;
    let mut ul: i64 = 0;

    // Query xray stats API via CLI - reset counters each call to get delta
    if let Ok(output) = std::process::Command::new(&xray_exe)
        .args(&["api", "statsquery", "-s", "127.0.0.1:10813", "-reset"])
        .creation_flags(0x08000000)
        .output() 
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // xray outputs JSON: { "stat": [{ "name": "...", "value": "123" }, ...] }
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
            if let Some(stats) = json.get("stat").and_then(|s| s.as_array()) {
                for stat in stats {
                    let name = stat.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let value = stat.get("value")
                        .and_then(|v| v.as_str().map(|s| s.parse::<i64>().unwrap_or(0))
                            .or_else(|| v.as_i64()))
                        .unwrap_or(0);
                    
                    // Skip API inbound traffic
                    if name.contains("api") { continue; }
                    
                    if name.contains("downlink") {
                        dl += value;
                    } else if name.contains("uplink") {
                        ul += value;
                    }
                }
            }
        }
    }

    // If stats API didn't return data, fall back to log estimation
    if dl == 0 && ul == 0 {
        let logs = xray::get_recent_activity();
        dl = logs.0;
        ul = logs.1;
    }

    serde_json::json!({ "download": dl, "upload": ul })
}

/// Check what process is using a given port
#[tauri::command]
async fn check_port(port: u16) -> serde_json::Value {
    if let Ok(output) = std::process::Command::new("netstat")
        .args(&["-ano"])
        .creation_flags(0x08000000)
        .output() 
    {
        let text = String::from_utf8_lossy(&output.stdout);
        let port_str = format!(":{}", port);
        for line in text.lines() {
            if line.contains(&port_str) && line.contains("LISTENING") {
                // Extract PID (last column)
                if let Some(pid_str) = line.split_whitespace().last() {
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        // Get process name from PID
                        let mut proc_name = format!("PID {}", pid);
                        if let Ok(info) = std::process::Command::new("tasklist")
                            .args(&["/FI", &format!("PID eq {}", pid), "/FO", "CSV", "/NH"])
                            .creation_flags(0x08000000)
                            .output()
                        {
                            let info_text = String::from_utf8_lossy(&info.stdout);
                            if let Some(name) = info_text.split(',').next() {
                                proc_name = name.trim().trim_matches('"').to_string();
                            }
                        }
                        return serde_json::json!({
                            "busy": true,
                            "pid": pid,
                            "process": proc_name,
                            "message": format!("Port {} is used by {} (PID {})", port, proc_name, pid)
                        });
                    }
                }
            }
        }
    }
    serde_json::json!({ "busy": false, "message": format!("Port {} is free", port) })
}

/// Force kill process on a specific port
#[tauri::command]
async fn force_free_port(port: u16) -> String {
    if let Ok(output) = std::process::Command::new("netstat")
        .args(&["-ano"])
        .creation_flags(0x08000000)
        .output() 
    {
        let text = String::from_utf8_lossy(&output.stdout);
        let port_str = format!(":{}", port);
        for line in text.lines() {
            if line.contains(&port_str) && line.contains("LISTENING") {
                if let Some(pid_str) = line.split_whitespace().last() {
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        let _ = std::process::Command::new("taskkill")
                            .args(&["/PID", &pid.to_string(), "/F"])
                            .creation_flags(0x08000000)
                            .output();
                        return format!("Killed PID {} on port {}", pid, port);
                    }
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
    let _ = sysproxy::unset_system_proxy();
    app.exit(0);
}

// ═══════════════════════════════════════════════════════════
//  System Tray helpers
// ═══════════════════════════════════════════════════════════

fn update_tray_connected(app: &tauri::AppHandle) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some("DoodleRay VPN — Connected ✓"));
    }
}

fn update_tray_disconnected(app: &tauri::AppHandle) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some("DoodleRay VPN — Disconnected"));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]) // launch minimized by default if started via autostart
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
        ])
        .setup(|app| {
            // ── System Tray ──
            let show_item = MenuItemBuilder::with_id("show", "Show DoodleRay")
                .build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit")
                .build(app)?;
            
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
                            // Disconnect VPN before quitting
                            let is_connected = {
                                let state = CONNECTION_STATE.lock().unwrap();
                                *state
                            };
                            if is_connected {
                                let _ = singbox::stop_singbox();
                                let _ = ipc::send_command_to_service("StopTun");
                            }
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
