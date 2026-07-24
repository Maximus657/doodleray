#[cfg(windows)]
use std::fs::{File, OpenOptions};
#[cfg(windows)]
use std::io::{Read, Write};
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(windows)]
use std::sync::mpsc;
#[cfg(windows)]
use std::time::{Duration, Instant};

#[cfg(windows)]
use crate::tunnel_service::{
    TunnelCommand, TunnelResponse, TUNNEL_PIPE_NAME, TUNNEL_PROTOCOL_VERSION,
};

#[cfg(windows)]
const ERROR_PIPE_BUSY: i32 = 231;

#[cfg(windows)]
fn wait_for_pipe_slot(timeout: Duration) -> bool {
    use windows_sys::Win32::System::Pipes::WaitNamedPipeW;
    let wide: Vec<u16> = TUNNEL_PIPE_NAME
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let timeout_ms = timeout.as_millis().min(u32::MAX as u128) as u32;
    unsafe { WaitNamedPipeW(wide.as_ptr(), timeout_ms) != 0 }
}

#[cfg(windows)]
fn open_tunnel_pipe() -> std::io::Result<File> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(0)
            .open(TUNNEL_PIPE_NAME)
        {
            Ok(file) => return Ok(file),
            Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() || !wait_for_pipe_slot(remaining) {
                    return Err(error);
                }
            }
            Err(error) => return Err(error),
        }
    }
}

/// Retry backoff for the SCM-running-before-pipe-listening race. The pipe is
/// usually accepting within tens of milliseconds, so the early retries are
/// cheap; the tail keeps the original budget for a genuinely slow start.
#[cfg(windows)]
fn pipe_retry_delay_ms(attempt: u32) -> u64 {
    match attempt {
        0 => 25,
        1 => 50,
        2 => 100,
        _ => 200,
    }
}

#[cfg(windows)]
pub fn send_tunnel_command(command: &TunnelCommand) -> Result<TunnelResponse, String> {
    let payload = serde_json::to_vec(command).map_err(|e| format!("IPC encode failed: {}", e))?;
    if payload.len() > 4 * 1024 * 1024 {
        return Err("IPC payload is too large".into());
    }

    // A just-(re)started service reports Running to SCM before its pipe
    // server is actually listening, so the very first connect attempt after
    // a start can transiently fail with "file not found" even though the
    // service is healthy a moment later. Retry generously; a genuinely dead
    // service still fails the same way, just a couple seconds later.
    let mut last_error = String::new();
    for attempt in 0..15u32 {
        match send_tunnel_payload_with_timeout(payload.clone(), Duration::from_secs(6)) {
            Ok(response) => return Ok(response),
            Err(error) => {
                last_error = error;
                if attempt < 14 {
                    std::thread::sleep(Duration::from_millis(pipe_retry_delay_ms(attempt)));
                }
            }
        }
    }
    Err(last_error)
}

#[cfg(windows)]
fn send_tunnel_payload_with_timeout(
    payload: Vec<u8>,
    timeout: Duration,
) -> Result<TunnelResponse, String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(send_tunnel_payload(&payload));
    });
    match rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(format!(
            "Tunnel service IPC timed out after {}s",
            timeout.as_secs()
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err("Tunnel service IPC worker stopped unexpectedly".into())
        }
    }
}

#[cfg(windows)]
fn send_tunnel_payload(payload: &[u8]) -> Result<TunnelResponse, String> {
    let mut client = open_tunnel_pipe().map_err(|e| {
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
        return Err(format!(
            "Invalid tunnel service response length: {}",
            response_len
        ));
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
pub fn send_tunnel_command(
    _: &crate::tunnel_service::TunnelCommand,
) -> Result<crate::tunnel_service::TunnelResponse, String> {
    Err("Tunnel service is only available on Windows".into())
}

#[cfg(all(test, windows))]
mod tests {
    use super::pipe_retry_delay_ms;

    #[test]
    fn pipe_retry_backoff_starts_short_and_settles_at_200ms() {
        let delays: Vec<u64> = (0..15).map(pipe_retry_delay_ms).collect();
        assert_eq!(&delays[..4], &[25, 50, 100, 200]);
        assert!(
            delays[4..].iter().all(|delay| *delay == 200),
            "tail must stay at 200ms: {delays:?}"
        );
        // Must not spend more wall clock than the previous flat 14 x 200ms.
        let total: u64 = delays[..14].iter().sum();
        assert!(total <= 14 * 200, "backoff budget grew to {total}ms");
    }
}
