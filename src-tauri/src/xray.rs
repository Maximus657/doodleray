use std::io::BufRead;
use std::process::{Child, Command};
use std::sync::Mutex;
use std::path::PathBuf;
use lazy_static::lazy_static;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

lazy_static! {
    static ref XRAY_PROCESS: Mutex<Option<Child>> = Mutex::new(None);
    static ref XRAY_LOGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static ref LOG_CURSOR: Mutex<usize> = Mutex::new(0);
    static ref ACTIVITY_CURSOR: Mutex<usize> = Mutex::new(0);
}

/// Get the directory where xray-core resources are located.
fn get_xray_resource_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    
    #[cfg(target_os = "macos")]
    {
        // In a .app bundle: Contents/MacOS/ → Contents/Resources/
        let resources_dir = exe_dir.parent()
            .map(|p| p.join("Resources"))
            .unwrap_or(exe_dir.clone());
        let xray_in_resources = resources_dir.join("xray-core");
        if xray_in_resources.exists() {
            return resources_dir;
        }
    }
    
    exe_dir
}

pub fn start_xray(config_json: &serde_json::Value) -> Result<(), String> {
    let _ = stop_xray();

    {
        let mut logs = XRAY_LOGS.lock().unwrap();
        logs.clear();
        let mut cursor = LOG_CURSOR.lock().unwrap();
        *cursor = 0;
        let mut acursor = ACTIVITY_CURSOR.lock().unwrap();
        *acursor = 0;
    }

    let exe_dir = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    
    let resource_dir = get_xray_resource_dir();

    #[cfg(windows)]
    let xray_name = "xray.exe";
    #[cfg(not(windows))]
    let xray_name = "xray";

    // Try resource dir first (for macOS .app bundle), then exe dir
    let xray_exe = if resource_dir.join("xray-core").join(xray_name).exists() {
        resource_dir.join("xray-core").join(xray_name)
    } else {
        exe_dir.join("xray-core").join(xray_name)
    };
    if !xray_exe.exists() {
        return Err(format!("xray not found at {:?}", xray_exe));
    }

    let temp_dir = std::env::temp_dir().join("DoodleRay");
    let _ = std::fs::create_dir_all(&temp_dir);
    let config_path = temp_dir.join("xray_config.json");
    let config_str = serde_json::to_string_pretty(config_json)
        .map_err(|e| format!("Failed to serialize xray config: {}", e))?;
    std::fs::write(&config_path, &config_str)
        .map_err(|e| format!("Failed to write xray config: {}", e))?;

    let mut cmd = Command::new(&xray_exe);
    cmd.arg("run")
        .arg("-c")
        .arg(&config_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    
    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    
    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to start xray: {}", e))?;

    // Capture stdout (xray writes access logs here)
    if let Some(stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines().flatten() {
                if line.contains("api -> api") || line.contains("api]") 
                    || line.contains("172.19.0.2:53") 
                    || line.contains("fdfe:dcba")
                    || line.contains("dokodemo") 
                    || line.contains("The feature VLESS") 
                    || line.contains("An established connection was aborted by the software")
                    || line.contains("dns-out")
                    || line.contains("cannot find the pending request") {
                    continue;
                }
                let mut logs = XRAY_LOGS.lock().unwrap();
                if logs.len() > 1000 {
                    logs.drain(0..500);
                    // Adjust cursors so they don't point past the end
                    if let Ok(mut c) = LOG_CURSOR.lock() {
                        *c = c.saturating_sub(500).min(logs.len());
                    }
                    if let Ok(mut c) = ACTIVITY_CURSOR.lock() {
                        *c = c.saturating_sub(500).min(logs.len());
                    }
                }
                logs.push(line);
            }
        });
    }

    // Capture stderr (xray writes errors/warnings here)
    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(stderr);
            for line in reader.lines().flatten() {
                if line.contains("api -> api") || line.contains("api]") 
                    || line.contains("172.19.0.2:53") 
                    || line.contains("dokodemo") 
                    || line.contains("The feature VLESS")
                    || line.contains("An established connection was aborted by the software") {
                    continue;
                }
                let mut logs = XRAY_LOGS.lock().unwrap();
                if logs.len() > 1000 {
                    logs.drain(0..500);
                    if let Ok(mut c) = LOG_CURSOR.lock() {
                        *c = c.saturating_sub(500).min(logs.len());
                    }
                    if let Ok(mut c) = ACTIVITY_CURSOR.lock() {
                        *c = c.saturating_sub(500).min(logs.len());
                    }
                }
                logs.push(line);
            }
        });
    }

    {
        let mut proc = XRAY_PROCESS.lock().unwrap();
        *proc = Some(child);
    }

    std::thread::sleep(std::time::Duration::from_millis(300));
    Ok(())
}

pub fn stop_xray() -> Result<(), String> {
    let mut proc = XRAY_PROCESS.lock().unwrap();
    if let Some(ref mut child) = *proc {
        let _ = child.kill();
        let _ = child.wait();
    }
    *proc = None;
    
    // Also force-kill any orphaned xray process (e.g. after crash)
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("taskkill");
        cmd.args(&["/IM", "xray.exe", "/F"]);
        cmd.creation_flags(0x08000000);
        let _ = cmd.output();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("pkill").args(["-f", "xray"]).output();
    }
    
    // Brief pause to let ports release
    std::thread::sleep(std::time::Duration::from_millis(300));
    Ok(())
}

pub fn get_new_logs() -> Vec<String> {
    let logs = XRAY_LOGS.lock().unwrap();
    let mut cursor = LOG_CURSOR.lock().unwrap();
    // Clamp cursor to valid range
    let start = (*cursor).min(logs.len());
    let new_lines: Vec<String> = logs[start..].to_vec();
    *cursor = logs.len();
    new_lines
}

/// Estimate traffic by counting proxy connections in recent logs
pub fn get_recent_activity() -> (i64, i64) {
    let logs = XRAY_LOGS.lock().unwrap();
    let mut activity_cursor = ACTIVITY_CURSOR.lock().unwrap();
    
    // Clamp cursor to valid range
    let start = (*activity_cursor).min(logs.len());
    
    let mut dl: i64 = 0;
    let mut ul: i64 = 0;
    
    for line in logs[start..].iter() {
        // Skip API/dokodemo lines
        if line.contains("api-in") || line.contains("dokodemo") || line.contains("api]") {
            continue;
        }
        // "tunneling request" = outgoing proxy connection established
        if line.contains("tunneling request") {
            dl += 15000; // ~15KB download per connection avg
            ul += 3000;  // ~3KB upload per connection avg
        }
        // "accepted" with "proxy" = incoming traffic accepted to proxy
        if line.contains("accepted") && line.contains("proxy") {
            dl += 5000;
            ul += 1500;
        }
    }
    
    *activity_cursor = logs.len();
    (dl, ul)
}
