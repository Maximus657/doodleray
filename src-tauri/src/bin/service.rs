#[cfg(windows)]
use std::io::Error as IoError;
#[cfg(windows)]
#[cfg(windows)]
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

#[cfg(windows)]
const PIPE_NAME: &str = r"\\.\pipe\DoodleRayServicePipe";

#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("DoodleRay Service Starting...");

    // Create the named pipe server
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(PIPE_NAME)?;

    println!("Listening for commands on {}", PIPE_NAME);

    loop {
        // Wait for a client to connect
        server.connect().await?;
        println!("Client connected to pipe");

        // Process the connection in a separate task or just handle it sequentially for simplicity
        let mut connected_client = server;

        // Setup the next server instance to accept the next connection
        server = ServerOptions::new().create(PIPE_NAME)?;

        tokio::spawn(async move {
            if let Err(e) = handle_client(&mut connected_client).await {
                eprintln!("Error handling client: {}", e);
            }
        });
    }
}

#[cfg(windows)]
async fn handle_client(client: &mut NamedPipeServer) -> Result<(), IoError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut buffer = [0; 1024];
    match client.read(&mut buffer).await {
        Ok(0) => return Ok(()),
        Ok(n) => {
            let command_str = String::from_utf8_lossy(&buffer[..n]);
            println!("Received command: {}", command_str);

            let response = process_command(&command_str).await;
            client.write_all(response.as_bytes()).await?;
        }
        Err(e) => return Err(e),
    }

    Ok(())
}

#[cfg(windows)]
async fn process_command(cmd_str: &str) -> String {
    let cmd = cmd_str.trim();
    if cmd.starts_with("StartTun") {
        let config_json = cmd.trim_start_matches("StartTun").trim();
        if let Ok(parsed) = serde_json::from_str(config_json) {
            match tauri_app_lib::singbox::start_singbox(&parsed) {
                Ok(_) => "StartTun OK".to_string(),
                Err(e) => format!("ERROR: Failed to start TUN: {}", e),
            }
        } else {
            "ERROR: Invalid JSON configuration".to_string()
        }
    } else if cmd.starts_with("StopTun") {
        match tauri_app_lib::singbox::stop_singbox() {
            Ok(_) => "StopTun OK".to_string(),
            Err(e) => format!("ERROR: Failed to stop TUN: {}", e),
        }
    } else {
        format!("ERROR: Unknown command: {}", cmd)
    }
}

#[cfg(not(windows))]
fn main() {
    println!("This service is only supported on Windows.");
}
