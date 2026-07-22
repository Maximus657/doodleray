use serde::Deserialize;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

#[derive(Debug, Deserialize)]
pub struct TunnelResponse {
    pub success: bool,
    pub status: String,
    pub message: String,
}

extern "C" {
    fn doodleray_ne_start(config_json: *const c_char) -> *mut c_char;
    fn doodleray_ne_stop() -> *mut c_char;
    fn doodleray_ne_status() -> *mut c_char;
    fn doodleray_ne_stop_cached();
    fn doodleray_ne_free(value: *mut c_char);
}

fn decode_response(value: *mut c_char) -> Result<TunnelResponse, String> {
    if value.is_null() {
        return Err("Network Extension returned no response".into());
    }

    let text = unsafe { CStr::from_ptr(value) }
        .to_str()
        .map(ToOwned::to_owned)
        .map_err(|_| "Network Extension returned invalid UTF-8".to_string());
    unsafe { doodleray_ne_free(value) };
    serde_json::from_str(&text?).map_err(|_| "Network Extension returned invalid JSON".into())
}

fn start_blocking(config: &serde_json::Value) -> Result<TunnelResponse, String> {
    let encoded = serde_json::to_string(config)
        .map_err(|_| "Could not encode the VPN configuration".to_string())?;
    let encoded = CString::new(encoded)
        .map_err(|_| "VPN configuration contains an invalid character".to_string())?;
    decode_response(unsafe { doodleray_ne_start(encoded.as_ptr()) })
}

fn stop_blocking() -> Result<TunnelResponse, String> {
    decode_response(unsafe { doodleray_ne_stop() })
}

fn status_blocking() -> Result<TunnelResponse, String> {
    decode_response(unsafe { doodleray_ne_status() })
}

async fn run_blocking<F>(operation: &'static str, task: F) -> Result<TunnelResponse, String>
where
    F: FnOnce() -> Result<TunnelResponse, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| format!("Network Extension {operation} task failed: {error}"))?
}

pub async fn start(config: serde_json::Value) -> Result<TunnelResponse, String> {
    run_blocking("start", move || start_blocking(&config)).await
}

pub async fn stop() -> Result<TunnelResponse, String> {
    run_blocking("stop", stop_blocking).await
}

pub async fn status() -> Result<TunnelResponse, String> {
    run_blocking("status", status_blocking).await
}

pub fn stop_cached() {
    unsafe { doodleray_ne_stop_cached() };
}

pub fn is_active_status(status: &str) -> bool {
    matches!(status, "connecting" | "connected" | "reasserting")
}

pub fn is_connected_status(status: &str) -> bool {
    status == "connected"
}

pub fn is_stopped_status(status: &str) -> bool {
    matches!(status, "disconnected" | "invalid")
}

#[cfg(test)]
mod tests {
    use super::{is_active_status, is_connected_status, is_stopped_status};

    #[test]
    fn only_live_network_extension_states_are_active() {
        for status in ["connecting", "connected", "reasserting"] {
            assert!(is_active_status(status));
        }
        for status in ["invalid", "disconnected", "disconnecting", "unknown"] {
            assert!(!is_active_status(status));
        }
        assert!(is_connected_status("connected"));
        for status in ["connecting", "reasserting", "disconnected", "invalid"] {
            assert!(!is_connected_status(status));
        }
        for status in ["disconnected", "invalid"] {
            assert!(is_stopped_status(status));
        }
        for status in ["connecting", "connected", "reasserting", "disconnecting"] {
            assert!(!is_stopped_status(status));
        }
    }
}
