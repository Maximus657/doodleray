use std::process::Command;
use std::path::PathBuf;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Get the directory where external resources (binaries) are located.
/// On Windows: next to the .exe
/// On macOS: Contents/Resources/ in the .app bundle, or next to the binary for dev
fn get_resource_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    
    #[cfg(target_os = "macos")]
    {
        // In a .app bundle: Contents/MacOS/DoodleRay → Contents/Resources/
        let resources_dir = exe_dir.parent()
            .map(|p| p.join("Resources"))
            .unwrap_or(exe_dir.clone());
        if resources_dir.exists() {
            return resources_dir;
        }
    }
    
    exe_dir
}

/// Start sing-box TUN as an elevated (admin/root) subprocess
pub fn start_tun_elevated(config_json: &serde_json::Value) -> Result<(), String> {
    let _ = stop_tun();

    let exe_dir = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    
    let resource_dir = get_resource_dir();

    #[cfg(windows)]
    let singbox_name = "sing-box.exe";
    #[cfg(not(windows))]
    let singbox_name = "sing-box";

    // Try resource dir first, then exe dir
    let singbox_exe = if resource_dir.join(singbox_name).exists() {
        resource_dir.join(singbox_name)
    } else {
        exe_dir.join(singbox_name)
    };
    if !singbox_exe.exists() {
        return Err(format!("{} not found at {:?} or {:?}", singbox_name, resource_dir.join(singbox_name), exe_dir.join(singbox_name)));
    }

    // Write config to file
    let config_path = exe_dir.join("tun_config.json");
    let config_str = serde_json::to_string_pretty(config_json)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    std::fs::write(&config_path, &config_str)
        .map_err(|e| format!("Failed to write config: {}", e))?;

    // Write a launcher script that captures sing-box output to a log
    let log_path = exe_dir.join("singbox_tun.log");
    let _ = std::fs::write(&log_path, "");

    #[cfg(windows)]
    {
        let bat_path = exe_dir.join("launch_singbox.bat");
        let bat_content = format!(
            "@echo off\r\n\"{}\" run -c \"{}\" > \"{}\" 2>&1\r\n",
            singbox_exe.to_string_lossy(),
            config_path.to_string_lossy(),
            log_path.to_string_lossy(),
        );
        std::fs::write(&bat_path, &bat_content)
            .map_err(|e| format!("Failed to write launcher bat: {}", e))?;

        // Launch elevated via ShellExecuteW "runas"
        let bat_str: Vec<u16> = bat_path.to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let verb: Vec<u16> = "runas\0".encode_utf16().collect();

        unsafe {
            #[link(name = "shell32")]
            extern "system" {
                fn ShellExecuteW(
                    hwnd: *mut std::ffi::c_void,
                    lpOperation: *const u16,
                    lpFile: *const u16,
                    lpParameters: *const u16,
                    lpDirectory: *const u16,
                    nShowCmd: i32,
                ) -> isize;
            }

            let result = ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                bat_str.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0, // SW_HIDE
            );

            if result as usize <= 32 {
                return Err("UAC was declined or ShellExecute failed. TUN requires admin privileges.".into());
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let sh_path = exe_dir.join("launch_singbox.sh");
        let sh_content = format!(
            "#!/bin/bash\n\"{}\" run -c \"{}\" > \"{}\" 2>&1\n",
            singbox_exe.to_string_lossy(),
            config_path.to_string_lossy(),
            log_path.to_string_lossy(),
        );
        std::fs::write(&sh_path, &sh_content)
            .map_err(|e| format!("Failed to write launcher script: {}", e))?;
        
        // Make it executable
        Command::new("chmod")
            .args(["+x", &sh_path.to_string_lossy()])
            .output()
            .map_err(|e| format!("Failed to chmod: {}", e))?;

        // Launch with sudo via osascript (shows macOS password prompt)
        let script = format!(
            "do shell script \"bash '{}'\" with administrator privileges",
            sh_path.to_string_lossy()
        );
        
        Command::new("osascript")
            .args(["-e", &script])
            .spawn()
            .map_err(|e| format!("Failed to launch with admin: {}", e))?;
    }

    // Give sing-box time to start TUN adapter
    std::thread::sleep(std::time::Duration::from_millis(3000));

    // Verify sing-box is actually running
    for attempt in 0..8 {
        if is_singbox_running() {
            return Ok(());
        }
        // On later attempts, check the log file for errors
        if attempt >= 2 {
            if let Ok(log_content) = std::fs::read_to_string(&log_path) {
                let trimmed = log_content.trim();
                if !trimmed.is_empty() && (trimmed.contains("FATAL") || trimmed.contains("ERROR") || trimmed.contains("panic")) {
                    return Err(format!("sing-box crashed: {}", trimmed));
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    // Read log for final error message
    let log_msg = std::fs::read_to_string(&log_path).unwrap_or_default();
    let log_trimmed = log_msg.trim();
    
    if !log_trimmed.is_empty() {
        Err(format!("sing-box failed to start: {}", log_trimmed))
    } else {
        Err("sing-box failed to start. TUN adapter may not have been created. Try running with admin/root privileges.".into())
    }
}

fn is_singbox_running() -> bool {
    #[cfg(windows)]
    {
        if let Ok(output) = Command::new("tasklist")
            .args(&["/FI", "IMAGENAME eq sing-box.exe", "/NH"])
            .creation_flags(0x08000000)
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            return text.contains("sing-box.exe");
        }
        false
    }
    #[cfg(not(windows))]
    {
        if let Ok(output) = Command::new("pgrep")
            .args(["-f", "sing-box"])
            .output()
        {
            return output.status.success();
        }
        false
    }
}

pub fn stop_tun() -> Result<(), String> {
    #[cfg(windows)]
    {
        let regular = Command::new("taskkill")
            .args(&["/IM", "sing-box.exe", "/F"])
            .creation_flags(0x08000000)
            .output();
        
        if let Ok(output) = &regular {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("Access") || stderr.contains("Отказано") || stderr.contains("denied") {
                let kill_cmd = "taskkill /IM sing-box.exe /F";
                let cmd_w: Vec<u16> = "cmd.exe\0".encode_utf16().collect();
                let args = format!("/c {}\0", kill_cmd);
                let args_w: Vec<u16> = args.encode_utf16().collect();
                let verb: Vec<u16> = "runas\0".encode_utf16().collect();
                
                unsafe {
                    #[link(name = "shell32")]
                    extern "system" {
                        fn ShellExecuteW(
                            hwnd: *mut std::ffi::c_void,
                            lpOperation: *const u16,
                            lpFile: *const u16,
                            lpParameters: *const u16,
                            lpDirectory: *const u16,
                            nShowCmd: i32,
                        ) -> isize;
                    }
                    
                    ShellExecuteW(
                        std::ptr::null_mut(),
                        verb.as_ptr(),
                        cmd_w.as_ptr(),
                        args_w.as_ptr(),
                        std::ptr::null(),
                        0,
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    }

    #[cfg(not(windows))]
    {
        // Kill via sudo (may prompt for password)
        let _ = Command::new("pkill")
            .args(["-f", "sing-box"])
            .output();
        
        // If regular kill fails, try with sudo
        if is_singbox_running() {
            let _ = Command::new("osascript")
                .args(["-e", "do shell script \"pkill -f sing-box\" with administrator privileges"])
                .output();
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    
    Ok(())
}
