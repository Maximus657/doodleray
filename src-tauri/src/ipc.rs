use std::io::{Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::fs::OpenOptions;

const PIPE_NAME: &str = r"\\.\pipe\DoodleRayServicePipe";

pub fn send_command_to_service(command: &str) -> Result<String, String> {
    let mut client = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(0)
        .open(PIPE_NAME)
        .map_err(|e| format!("Failed to connect to service pipe (is the service running?): {}", e))?;

    client.write_all(command.as_bytes())
        .map_err(|e| format!("Failed to write to pipe: {}", e))?;

    let mut buffer = [0; 1024];
    let response = match client.read(&mut buffer) {
        Ok(bytes_read) if bytes_read > 0 => {
            String::from_utf8_lossy(&buffer[..bytes_read]).to_string()
        }
        Ok(_) => return Err("Service closed connection immediately".to_string()),
        Err(e) => return Err(format!("Failed to read response: {}", e)),
    };

    if response.starts_with("ERROR:") {
        Err(response)
    } else {
        Ok(response)
    }
}
