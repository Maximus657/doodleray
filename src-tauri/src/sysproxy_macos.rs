/// macOS system proxy helper — sets/unsets the HTTP proxy via networksetup
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::process::Command;

fn shell_quote(value: &str) -> String {
    let escaped = value.replace('\'', "'\\''");
    format!("'{}'", escaped)
}

fn applescript_quote(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{}\"", escaped)
}

/// Get the active network service name (e.g., "Wi-Fi" or "Ethernet")
fn get_active_service() -> Result<String, String> {
    // Get the default route interface
    let output = Command::new("route")
        .args(["-n", "get", "default"])
        .output()
        .map_err(|e| format!("Failed to run route: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let interface = stdout
        .lines()
        .find(|l| l.contains("interface:"))
        .and_then(|l| l.split(':').nth(1))
        .map(|s| s.trim().to_string())
        .ok_or("Could not find default interface")?;

    // Map interface to network service name
    let output = Command::new("networksetup")
        .args(["-listallhardwareports"])
        .output()
        .map_err(|e| format!("Failed to run networksetup: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    for i in 0..lines.len() {
        if lines[i].contains("Device:") && lines[i].contains(&interface) {
            // Service name is on the previous line
            if i > 0 {
                if let Some(name) = lines[i - 1].strip_prefix("Hardware Port: ") {
                    return Ok(name.to_string());
                }
            }
        }
    }

    // Fallback to Wi-Fi
    Ok("Wi-Fi".to_string())
}

fn command_error(action: &str, output: std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stderr.is_empty() {
        format!("{}: {}", action, stderr)
    } else if !stdout.is_empty() {
        format!("{}: {}", action, stdout)
    } else {
        format!("{} failed with status {}", action, output.status)
    }
}

fn run_networksetup(action: &str, args: &[String]) -> Result<(), String> {
    let output = Command::new("networksetup")
        .args(args)
        .output()
        .map_err(|e| format!("{}: failed to run networksetup: {}", action, e))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(action, output))
    }
}

fn write_admin_script(script: &str) -> Result<std::path::PathBuf, String> {
    let temp_dir = std::env::temp_dir().join("DoodleRay");
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Failed to create temp dir for proxy helper: {}", e))?;
    let path = temp_dir.join("macos_proxy_admin.sh");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o700)
        .open(&path)
        .map_err(|e| format!("Failed to write proxy helper: {}", e))?;
    file.write_all(script.as_bytes())
        .map_err(|e| format!("Failed to write proxy helper: {}", e))?;
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700));
    Ok(path)
}

fn run_admin_script(action: &str, script: &str) -> Result<(), String> {
    let script_path = write_admin_script(script)?;
    let shell_command = format!("bash {}", shell_quote(&script_path.to_string_lossy()));
    let applescript = format!(
        "do shell script {} with administrator privileges",
        applescript_quote(&shell_command)
    );

    let output = Command::new("osascript")
        .args(["-e", &applescript])
        .output()
        .map_err(|e| {
            format!(
                "{}: failed to request administrator permission: {}",
                action, e
            )
        })?;

    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(action, output))
    }
}

fn networksetup_script(commands: &[Vec<String>]) -> String {
    let mut script = String::from("#!/bin/bash\nset -e\n");
    for args in commands {
        script.push_str("/usr/sbin/networksetup");
        for arg in args {
            script.push(' ');
            script.push_str(&shell_quote(arg));
        }
        script.push('\n');
    }
    script
}

fn run_networksetup_batch(action: &str, commands: Vec<Vec<String>>) -> Result<(), String> {
    let mut first_error: Option<String> = None;
    for args in &commands {
        if let Err(e) = run_networksetup(action, args) {
            first_error = Some(e);
            break;
        }
    }

    if first_error.is_none() {
        return Ok(());
    }

    let script = networksetup_script(&commands);
    run_admin_script(action, &script).map_err(|admin_err| {
        let first = first_error.unwrap_or_else(|| "networksetup failed".to_string());
        format!("{}; admin retry failed: {}", first, admin_err)
    })
}

pub fn set_system_proxy(http_port: u16) -> Result<(), String> {
    let socks_port = http_port - 1;
    let service = get_active_service()?;
    let http_port = http_port.to_string();
    let socks_port = socks_port.to_string();

    run_networksetup_batch(
        "set macOS system proxy",
        vec![
            vec![
                "-setwebproxy".into(),
                service.clone(),
                "127.0.0.1".into(),
                http_port.clone(),
            ],
            vec![
                "-setsecurewebproxy".into(),
                service.clone(),
                "127.0.0.1".into(),
                http_port,
            ],
            vec![
                "-setsocksfirewallproxy".into(),
                service.clone(),
                "127.0.0.1".into(),
                socks_port,
            ],
            vec!["-setwebproxystate".into(), service.clone(), "on".into()],
            vec![
                "-setsecurewebproxystate".into(),
                service.clone(),
                "on".into(),
            ],
            vec!["-setsocksfirewallproxystate".into(), service, "on".into()],
        ],
    )
}

pub fn unset_system_proxy() -> Result<(), String> {
    let service = get_active_service()?;
    run_networksetup_batch(
        "unset macOS system proxy",
        vec![
            vec!["-setwebproxystate".into(), service.clone(), "off".into()],
            vec![
                "-setsecurewebproxystate".into(),
                service.clone(),
                "off".into(),
            ],
            vec!["-setsocksfirewallproxystate".into(), service, "off".into()],
        ],
    )
}

pub fn current_manual_http_proxy_for_url(_scheme: &str) -> Result<Option<String>, String> {
    Ok(None)
}
