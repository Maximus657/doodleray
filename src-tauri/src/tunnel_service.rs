use serde::{Deserialize, Serialize};

pub const TUNNEL_SERVICE_NAME: &str = "DoodleRayTunnelService";
pub const TUNNEL_SERVICE_DISPLAY_NAME: &str = "DoodleRay Tunnel Service";
pub const TUNNEL_PIPE_NAME: &str = r"\\.\pipe\DoodleRay.TunnelService.v1";
pub const TUNNEL_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelEngineKind {
    XrayTun,
    SingboxTun,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelloRequest {
    pub protocol_version: u32,
    pub client_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartTunnelRequest {
    pub op_id: String,
    pub engine_kind: TunnelEngineKind,
    #[serde(default)]
    pub xray_config: Option<serde_json::Value>,
    pub singbox_config: serde_json::Value,
    pub socks_port: u16,
    pub http_port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_port: Option<u16>,
    pub redacted_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopTunnelRequest {
    pub op_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyCompatibilityReport {
    pub op_id: Option<String>,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairRuntimeRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op_id: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TunnelCommand {
    Hello(HelloRequest),
    GetStatus,
    GetDiagnostics,
    StartTunnel(StartTunnelRequest),
    StopTunnel(StopTunnelRequest),
    ReportProxyCompatibility(ProxyCompatibilityReport),
    RepairRuntime(RepairRuntimeRequest),
    PrepareForUpdate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
    Failed,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelEffectiveState {
    #[default]
    Idle,
    Preparing,
    Connecting,
    Protected,
    ProtectedDegraded,
    Limited,
    Suspect,
    Repairing,
    Failed,
    Disconnecting,
    CleanupPending,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelHealthVerdict {
    Protected,
    ProtectedDegraded,
    Limited,
    Repairing,
    #[default]
    Failed,
    CleanupPending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TunnelStatus {
    pub protocol_version: u32,
    pub service_version: String,
    pub state: TunnelState,
    #[serde(default)]
    pub effective_state: TunnelEffectiveState,
    #[serde(default)]
    pub health_verdict: TunnelHealthVerdict,
    pub phase: Option<String>,
    pub active_op_id: Option<String>,
    #[serde(default)]
    pub service_generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_generation: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_kind: Option<TunnelEngineKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_socks_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_http_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_api_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xray_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub singbox_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_ifindex: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_ready: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_ready: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_compat_state: Option<String>,
    #[serde(default)]
    pub fatal_checks: Vec<String>,
    #[serde(default)]
    pub degraded_checks: Vec<String>,
    #[serde(default)]
    pub warning_checks: Vec<String>,
    #[serde(default)]
    pub route_explanations: Vec<String>,
    #[serde(default)]
    pub endpoint_bypass_checks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_repair_action: Option<String>,
    #[serde(default)]
    pub network_event_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_unclean_shutdown: Option<String>,
    pub error: Option<String>,
    pub timings_ms: Vec<(String, u64)>,
    #[serde(default)]
    pub powershell_fallback_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub singbox_check_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xray_spawn_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xray_check_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_probe_backend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_probe_backend: Option<String>,
    #[serde(default)]
    pub native_probe_ms: Vec<(String, u64)>,
    #[serde(default)]
    pub fallback_probe_ms: Vec<(String, u64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TunnelDiagnostics {
    pub status: TunnelStatus,
    pub log_tail: Vec<String>,
    pub network_snapshot: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TunnelResponse {
    Status(TunnelStatus),
    Diagnostics(TunnelDiagnostics),
    Error { message: String },
}

#[cfg(windows)]
pub fn runtime_root() -> std::path::PathBuf {
    std::env::var_os("ProgramData")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\ProgramData"))
        .join("DoodleRay")
        .join("runtime")
}

#[cfg(windows)]
pub fn session_marker_path() -> std::path::PathBuf {
    runtime_root().join("active-session.marker")
}

/// Written when the service marks a tunnel Connected; removed by owned cleanup.
/// A marker found at service startup means the previous session ended without
/// running DoodleRay-owned cleanup (service crash, power loss, hard kill).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMarker {
    pub op_id: String,
    pub generation: u64,
    pub started_at_ms: u64,
}

impl SessionMarker {
    pub fn to_line(&self) -> String {
        format!(
            "op_id={};generation={};started_at_ms={}",
            self.op_id, self.generation, self.started_at_ms
        )
    }

    pub fn parse(line: &str) -> Option<Self> {
        let mut op_id = None;
        let mut generation = None;
        let mut started_at_ms = None;
        for part in line.trim().split(';') {
            let (key, value) = part.split_once('=')?;
            match key {
                "op_id" => op_id = Some(value.to_string()),
                "generation" => generation = value.parse().ok(),
                "started_at_ms" => started_at_ms = value.parse().ok(),
                _ => {}
            }
        }
        Some(Self {
            op_id: op_id?,
            generation: generation?,
            started_at_ms: started_at_ms?,
        })
    }

    /// Human-readable summary safe for status/support bundles: the op id is
    /// already sanitized by the service before it reaches the marker.
    pub fn summary(&self) -> String {
        format!(
            "previous session ended uncleanly: op_id={} generation={} started_at_ms={}",
            self.op_id, self.generation, self.started_at_ms
        )
    }
}

/// Human-readable summary of the sing-box route policy for diagnostics and
/// the support bundle: which classes of traffic go direct vs through the
/// tunnel. Pure so the mapping is unit-tested; unknown shapes degrade to
/// generic-but-honest lines instead of failing.
pub fn summarize_route_policy(singbox_config: &serde_json::Value) -> Vec<String> {
    let mut summary = Vec::new();
    let route = &singbox_config["route"];

    if let Some(final_tag) = route["final"].as_str() {
        summary.push(format!("route policy: default traffic -> {}", final_tag));
    }

    let rules = match route["rules"].as_array() {
        Some(rules) => rules,
        None => {
            summary.push("route policy: no explicit rules in config".into());
            return summary;
        }
    };

    let mut ru_direct = false;
    let mut private_direct = false;
    let mut endpoint_bypass_cidrs = 0usize;
    let mut dns_hijack = false;

    for rule in rules {
        let outbound = rule["outbound"].as_str().unwrap_or("");
        let action = rule["action"].as_str().unwrap_or("");
        if action == "hijack-dns" || outbound == "dns-out" {
            dns_hijack = true;
        }
        let direct = outbound == "direct" || action == "route-direct";
        if !direct {
            continue;
        }
        if rule["ip_is_private"].as_bool() == Some(true) {
            private_direct = true;
        }
        let mut matcher_text = String::new();
        for key in [
            "rule_set",
            "geosite",
            "geoip",
            "domain_suffix",
            "domain_keyword",
        ] {
            if let Some(value) = rule.get(key) {
                matcher_text.push_str(&value.to_string().to_lowercase());
            }
        }
        if matcher_text.contains("ru") || matcher_text.contains("russia") {
            ru_direct = true;
        }
        if let Some(cidrs) = rule["ip_cidr"].as_array() {
            endpoint_bypass_cidrs += cidrs.len();
        }
    }

    summary.push(if ru_direct {
        "route policy: RU sites/IPs (2ip class) stay direct (split routing)".into()
    } else {
        "route policy: no RU-direct split rule found in config".into()
    });
    summary.push(if private_direct {
        "route policy: private LAN ranges stay direct".into()
    } else {
        "route policy: no explicit private-LAN direct rule found".into()
    });
    if endpoint_bypass_cidrs > 0 {
        summary.push(format!(
            "route policy: {} direct ip_cidr entries (endpoint/custom bypass)",
            endpoint_bypass_cidrs
        ));
    }
    if dns_hijack {
        summary.push("route policy: DNS is hijacked into the tunnel resolver".into());
    }
    summary
}

#[cfg(test)]
mod route_policy_tests {
    use super::summarize_route_policy;

    #[test]
    fn summarizes_split_routing_config() {
        let config = serde_json::json!({
            "route": {
                "final": "proxy",
                "rules": [
                    { "action": "hijack-dns", "protocol": "dns" },
                    { "ip_is_private": true, "outbound": "direct" },
                    { "rule_set": ["geosite-category-ru", "geoip-ru"], "outbound": "direct" },
                    { "ip_cidr": ["203.0.113.7/32"], "outbound": "direct" }
                ]
            }
        });
        let summary = summarize_route_policy(&config);
        let text = summary.join("\n");
        assert!(text.contains("default traffic -> proxy"));
        assert!(text.contains("RU sites/IPs (2ip class) stay direct"));
        assert!(text.contains("private LAN ranges stay direct"));
        assert!(text.contains("1 direct ip_cidr entries"));
        assert!(text.contains("DNS is hijacked into the tunnel resolver"));
    }

    #[test]
    fn honest_about_missing_rules() {
        let summary = summarize_route_policy(&serde_json::json!({ "route": {} }));
        assert_eq!(
            summary,
            vec!["route policy: no explicit rules in config".to_string()]
        );
    }
}

/// TUN bring-up errors that are worth exactly one bounded DoodleRay-owned
/// repair retry (stop owned children, recreate the service-owned TUN) before
/// the user ever sees a failure. Cancellation and config/permission errors
/// are intentionally not repairable.
pub fn is_repairable_tun_bringup_error(error: &str) -> bool {
    [
        "DoodleRay Tunnel adapter is missing",
        "DoodleRay Tunnel adapter did not become ready",
        "DoodleRay Tunnel IPv4 readiness failed",
        "DoodleRay Tunnel IPv4 interface is not ready",
        "DoodleRay Tunnel IPv4 routes are missing",
        "DoodleRay Tunnel route is not preferred",
        "Windows system resolver canary failed after TUN route setup",
        "sing-box exited",
        "sing-box process is not running",
        "Cannot create a file when that file already exists",
        "open existing adapter: Element not found",
    ]
    .iter()
    .any(|needle| error.contains(needle))
}

/// Final, actionable message after the bounded repair could not bring the
/// tunnel adapter up. Kept in the shared lib so the wording is unit-tested.
pub fn format_tun_bringup_failure(
    error: &str,
    wintun_present: bool,
    last_phase: Option<&str>,
    attempts: u32,
) -> String {
    format!(
        "DoodleRay could not create the Windows tunnel adapter: {} (wintun.dll={}, last_phase={}, attempts={})",
        error,
        if wintun_present { "present" } else { "missing" },
        last_phase.unwrap_or("unknown"),
        attempts,
    )
}

#[cfg(test)]
mod tun_bringup_tests {
    use super::{format_tun_bringup_failure, is_repairable_tun_bringup_error};

    #[test]
    fn production_adapter_missing_error_is_repairable() {
        assert!(is_repairable_tun_bringup_error(
            "DoodleRay Tunnel IPv4 readiness failed: DoodleRay Tunnel adapter is missing"
        ));
        assert!(is_repairable_tun_bringup_error(
            "DoodleRay Tunnel adapter did not become ready"
        ));
        assert!(is_repairable_tun_bringup_error(
            "sing-box exited with exit code: 1: fatal start"
        ));
        assert!(is_repairable_tun_bringup_error(
            "Windows system resolver canary failed after TUN route setup: curl: (28) Resolving timed out"
        ));
        assert!(is_repairable_tun_bringup_error(
            "sing-box exited with exit status: 1: FATAL start service: start inbound/tun[tun-in]: configure tun interface: (create adapter: Cannot create a file when that file already exists. | open existing adapter: Element not found.)"
        ));
    }

    #[test]
    fn cancellation_and_config_errors_are_not_repairable() {
        assert!(!is_repairable_tun_bringup_error(
            "Tunnel start was cancelled"
        ));
        assert!(!is_repairable_tun_bringup_error(
            "xray_config is required for xray_tun"
        ));
        assert!(!is_repairable_tun_bringup_error(
            "Failed to create runtime dir: access denied"
        ));
    }

    #[test]
    fn final_failure_message_is_actionable() {
        let message = format_tun_bringup_failure(
            "DoodleRay Tunnel IPv4 readiness failed: DoodleRay Tunnel adapter is missing",
            false,
            Some("waiting_adapter"),
            2,
        );
        assert!(message.starts_with("DoodleRay could not create the Windows tunnel adapter:"));
        assert!(message.contains("wintun.dll=missing"));
        assert!(message.contains("last_phase=waiting_adapter"));
        assert!(message.contains("attempts=2"));
    }
}

#[cfg(test)]
mod session_marker_tests {
    use super::SessionMarker;

    #[test]
    fn session_marker_roundtrip() {
        let marker = SessionMarker {
            op_id: "connect-42".into(),
            generation: 7,
            started_at_ms: 1_751_400_000_000,
        };
        let parsed = SessionMarker::parse(&marker.to_line()).expect("marker should parse");
        assert_eq!(parsed, marker);
    }

    #[test]
    fn session_marker_rejects_garbage() {
        assert_eq!(SessionMarker::parse(""), None);
        assert_eq!(SessionMarker::parse("not-a-marker"), None);
        assert_eq!(
            SessionMarker::parse("op_id=x;generation=nan;started_at_ms=1"),
            None
        );
    }

    #[test]
    fn session_marker_parse_tolerates_trailing_newline_and_unknown_keys() {
        let parsed = SessionMarker::parse("op_id=a;generation=1;started_at_ms=2;future=key\n")
            .expect("marker should parse");
        assert_eq!(parsed.op_id, "a");
        assert_eq!(parsed.generation, 1);
        assert_eq!(parsed.started_at_ms, 2);
    }
}
