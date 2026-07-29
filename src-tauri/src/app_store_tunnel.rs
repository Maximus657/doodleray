use serde::Deserialize;
use std::ffi::c_void;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use tokio::sync::oneshot;

#[derive(Debug, Deserialize)]
pub struct TunnelResponse {
    pub success: bool,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct AutostartResponse {
    success: bool,
    supported: bool,
    enabled: bool,
    message: String,
}

type DoodleRayNECompletion = extern "C" fn(*mut c_void, *mut c_char);

extern "C" {
    fn doodleray_ne_start_async(
        config_json: *const c_char,
        context: *mut c_void,
        completion: DoodleRayNECompletion,
    );
    fn doodleray_ne_stop_async(context: *mut c_void, completion: DoodleRayNECompletion);
    fn doodleray_ne_status_async(context: *mut c_void, completion: DoodleRayNECompletion);
    fn doodleray_ne_stop_cached();
    fn doodleray_app_group_container_path() -> *mut c_char;
    fn doodleray_autostart_status() -> *mut c_char;
    fn doodleray_autostart_set_enabled(enabled: i32) -> *mut c_char;
    fn doodleray_ne_free(value: *mut c_char);
}

pub fn app_group_container_path() -> Result<PathBuf, String> {
    let value = unsafe { doodleray_app_group_container_path() };
    if value.is_null() {
        return Err("DoodleRay App Group container is unavailable".into());
    }
    let path = unsafe { CStr::from_ptr(value) }
        .to_str()
        .map(PathBuf::from)
        .map_err(|_| "DoodleRay App Group path is invalid UTF-8".to_string());
    unsafe { doodleray_ne_free(value) };
    path
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

fn decode_autostart_response(value: *mut c_char) -> Result<AutostartResponse, String> {
    if value.is_null() {
        return Err("Login item returned no response".into());
    }
    let text = unsafe { CStr::from_ptr(value) }
        .to_str()
        .map(ToOwned::to_owned)
        .map_err(|_| "Login item returned invalid UTF-8".to_string());
    unsafe { doodleray_ne_free(value) };
    serde_json::from_str(&text?).map_err(|_| "Login item returned invalid JSON".into())
}

fn resolve_autostart_response(
    response: AutostartResponse,
    requested: Option<bool>,
) -> Result<bool, String> {
    if !response.supported
        || !response.success
        || requested.is_some_and(|enabled| enabled != response.enabled)
    {
        return Err(if response.message.is_empty() {
            "Could not change Launch at startup".into()
        } else {
            response.message
        });
    }
    Ok(response.enabled)
}

pub fn autostart_enabled() -> Result<bool, String> {
    resolve_autostart_response(
        unsafe { decode_autostart_response(doodleray_autostart_status()) }?,
        None,
    )
}

pub fn set_autostart_enabled(enabled: bool) -> Result<bool, String> {
    resolve_autostart_response(
        unsafe { decode_autostart_response(doodleray_autostart_set_enabled(i32::from(enabled))) }?,
        Some(enabled),
    )
}

extern "C" fn complete_async_response(context: *mut c_void, value: *mut c_char) {
    let sender = unsafe { Box::from_raw(context.cast::<oneshot::Sender<usize>>()) };
    if sender.send(value as usize).is_err() && !value.is_null() {
        unsafe { doodleray_ne_free(value) };
    }
}

async fn run_async<F>(operation: &'static str, register: F) -> Result<TunnelResponse, String>
where
    F: FnOnce(*mut c_void, DoodleRayNECompletion) + Send,
{
    let (sender, receiver) = oneshot::channel::<usize>();
    let context = Box::into_raw(Box::new(sender)) as usize;
    register(context as *mut c_void, complete_async_response);
    let value = receiver
        .await
        .map_err(|_| format!("Network Extension {operation} callback was dropped"))?;
    decode_response(value as *mut c_char)
}

pub async fn start(config: serde_json::Value) -> Result<TunnelResponse, String> {
    let encoded = serde_json::to_string(&config)
        .map_err(|_| "Could not encode the VPN configuration".to_string())?;
    let encoded = CString::new(encoded)
        .map_err(|_| "VPN configuration contains an invalid character".to_string())?;
    run_async("start", move |context, completion| unsafe {
        doodleray_ne_start_async(encoded.as_ptr(), context, completion)
    })
    .await
}

pub async fn stop() -> Result<TunnelResponse, String> {
    run_async("stop", |context, completion| unsafe {
        doodleray_ne_stop_async(context, completion)
    })
    .await
}

pub async fn status() -> Result<TunnelResponse, String> {
    run_async("status", |context, completion| unsafe {
        doodleray_ne_status_async(context, completion)
    })
    .await
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
    use super::{
        is_active_status, is_connected_status, is_stopped_status, resolve_autostart_response,
        AutostartResponse,
    };

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

    #[test]
    fn autostart_requires_a_supported_confirmed_login_item() {
        let enabled = AutostartResponse {
            success: true,
            supported: true,
            enabled: true,
            message: String::new(),
        };
        assert!(resolve_autostart_response(enabled, Some(true)).unwrap());

        let pending_approval = AutostartResponse {
            success: true,
            supported: true,
            enabled: false,
            message: "Allow DoodleRay in System Settings".into(),
        };
        assert_eq!(
            resolve_autostart_response(pending_approval, Some(true)).unwrap_err(),
            "Allow DoodleRay in System Settings"
        );
    }
}
