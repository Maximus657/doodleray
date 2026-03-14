/// macOS system proxy helper — sets/unsets the HTTP proxy via networksetup
use std::process::Command;

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

pub fn set_system_proxy(http_port: u16) -> Result<(), String> {
    let socks_port = http_port - 1;
    let service = get_active_service()?;
    
    // Set HTTP proxy
    Command::new("networksetup")
        .args(["-setwebproxy", &service, "127.0.0.1", &http_port.to_string()])
        .output()
        .map_err(|e| format!("Failed to set HTTP proxy: {}", e))?;
    
    // Set HTTPS proxy
    Command::new("networksetup")
        .args(["-setsecurewebproxy", &service, "127.0.0.1", &http_port.to_string()])
        .output()
        .map_err(|e| format!("Failed to set HTTPS proxy: {}", e))?;
    
    // Set SOCKS proxy
    Command::new("networksetup")
        .args(["-setsocksfirewallproxy", &service, "127.0.0.1", &socks_port.to_string()])
        .output()
        .map_err(|e| format!("Failed to set SOCKS proxy: {}", e))?;
    
    // Enable all proxies
    Command::new("networksetup")
        .args(["-setwebproxystate", &service, "on"])
        .output()
        .map_err(|_| "Failed to enable HTTP proxy".to_string())?;
    
    Command::new("networksetup")
        .args(["-setsecurewebproxystate", &service, "on"])
        .output()
        .map_err(|_| "Failed to enable HTTPS proxy".to_string())?;
    
    Command::new("networksetup")
        .args(["-setsocksfirewallproxystate", &service, "on"])
        .output()
        .map_err(|_| "Failed to enable SOCKS proxy".to_string())?;
    
    Ok(())
}

pub fn unset_system_proxy() -> Result<(), String> {
    let service = get_active_service()?;
    
    Command::new("networksetup")
        .args(["-setwebproxystate", &service, "off"])
        .output()
        .map_err(|_| "Failed to disable HTTP proxy".to_string())?;
    
    Command::new("networksetup")
        .args(["-setsecurewebproxystate", &service, "off"])
        .output()
        .map_err(|_| "Failed to disable HTTPS proxy".to_string())?;
    
    Command::new("networksetup")
        .args(["-setsocksfirewallproxystate", &service, "off"])
        .output()
        .map_err(|_| "Failed to disable SOCKS proxy".to_string())?;
    
    Ok(())
}
