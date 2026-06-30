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
    pub redacted_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopTunnelRequest {
    pub op_id: String,
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
#[serde(deny_unknown_fields)]
pub struct TunnelStatus {
    pub protocol_version: u32,
    pub service_version: String,
    pub state: TunnelState,
    pub phase: Option<String>,
    pub active_op_id: Option<String>,
    #[serde(default)]
    pub service_generation: u64,
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
