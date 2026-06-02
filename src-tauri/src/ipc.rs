#[cfg(windows)]
use std::fs::OpenOptions;
#[cfg(windows)]
use std::io::{Read, Write};
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

#[cfg(windows)]
use crate::tunnel_service::{
    TunnelCommand, TunnelResponse, TUNNEL_PIPE_NAME, TUNNEL_PROTOCOL_VERSION,
};

#[cfg(windows)]
pub fn send_tunnel_command(command: &TunnelCommand) -> Result<TunnelResponse, String> {
    let payload = serde_json::to_vec(command).map_err(|e| format!("IPC encode failed: {}", e))?;
    if payload.len() > 4 * 1024 * 1024 {
        return Err("IPC payload is too large".into());
    }

    let mut last_error = String::new();
    for attempt in 0..3 {
        match send_tunnel_payload(&payload) {
            Ok(response) => return Ok(response),
            Err(error) => {
                last_error = error;
                if attempt < 2 {
                    std::thread::sleep(std::time::Duration::from_millis(120));
                }
            }
        }
    }
    Err(last_error)
}

#[cfg(windows)]
fn send_tunnel_payload(payload: &[u8]) -> Result<TunnelResponse, String> {
    let mut client = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(0)
        .open(TUNNEL_PIPE_NAME)
        .map_err(|e| {
            format!(
                "Failed to connect to tunnel service pipe (is the service running?): {}",
                e
            )
        })?;

    let len = (payload.len() as u32).to_le_bytes();
    client
        .write_all(&len)
        .map_err(|e| format!("Failed to write pipe frame length: {}", e))?;
    client
        .write_all(payload)
        .map_err(|e| format!("Failed to write to pipe: {}", e))?;
    client
        .flush()
        .map_err(|e| format!("Failed to flush pipe command: {}", e))?;

    let mut len_buf = [0u8; 4];
    client
        .read_exact(&mut len_buf)
        .map_err(|e| format!("Failed to read response length: {}", e))?;
    let response_len = u32::from_le_bytes(len_buf) as usize;
    if response_len == 0 || response_len > 4 * 1024 * 1024 {
        return Err(format!("Invalid tunnel service response length: {}", response_len));
    }
    let mut response = vec![0; response_len];
    client
        .read_exact(&mut response)
        .map_err(|e| format!("Failed to read response payload: {}", e))?;

    let decoded: TunnelResponse =
        serde_json::from_slice(&response).map_err(|e| format!("IPC decode failed: {}", e))?;
    if let TunnelResponse::Error { message } = &decoded {
        return Err(message.clone());
    }
    Ok(decoded)
}

#[cfg(windows)]
pub fn tunnel_service_status() -> Result<TunnelResponse, String> {
    send_tunnel_command(&TunnelCommand::GetStatus)
}

#[cfg(windows)]
pub fn tunnel_service_hello(client_version: &str) -> Result<TunnelResponse, String> {
    send_tunnel_command(&TunnelCommand::Hello(crate::tunnel_service::HelloRequest {
        protocol_version: TUNNEL_PROTOCOL_VERSION,
        client_version: client_version.to_string(),
    }))
}

#[cfg(not(windows))]
pub fn send_tunnel_command(_: &crate::tunnel_service::TunnelCommand) -> Result<crate::tunnel_service::TunnelResponse, String> {
    Err("Tunnel service is only available on Windows".into())
}
