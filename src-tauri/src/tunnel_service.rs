use serde::{Deserialize, Serialize};

pub const TUNNEL_SERVICE_NAME: &str = "DoodleRayTunnelService";
pub const TUNNEL_SERVICE_DISPLAY_NAME: &str = "DoodleRay Tunnel Service";
pub const TUNNEL_PIPE_NAME: &str = r"\\.\pipe\DoodleRay.TunnelService.v1";
pub const TUNNEL_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelEffectiveState {
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

impl Default for TunnelEffectiveState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelHealthVerdict {
    Protected,
    ProtectedDegraded,
    Limited,
    Repairing,
    Failed,
    CleanupPending,
}

impl Default for TunnelHealthVerdict {
    fn default() -> Self {
        Self::Failed
    }
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
        assert_eq!(SessionMarker::parse("op_id=x;generation=nan;started_at_ms=1"), None);
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
