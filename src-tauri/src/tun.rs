use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

/// Check if the current process is running with elevated (admin) privileges
#[cfg(windows)]
pub fn is_elevated() -> bool {
    unsafe {
        #[link(name = "advapi32")]
        extern "system" {
            fn OpenProcessToken(
                ProcessHandle: *mut std::ffi::c_void,
                DesiredAccess: u32,
                TokenHandle: *mut *mut std::ffi::c_void,
            ) -> i32;
            fn GetTokenInformation(
                TokenHandle: *mut std::ffi::c_void,
                TokenInformationClass: u32,
                TokenInformation: *mut std::ffi::c_void,
                TokenInformationLength: u32,
                ReturnLength: *mut u32,
            ) -> i32;
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
        }

        let mut token: *mut std::ffi::c_void = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), 0x0008, &mut token) == 0 {
            return false;
        }

        let mut elevation: u32 = 0;
        let mut return_length: u32 = 0;
        let result = GetTokenInformation(
            token,
            20, // TokenElevation
            &mut elevation as *mut u32 as *mut std::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
            &mut return_length,
        );
        CloseHandle(token);

        result != 0 && elevation != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    unsafe {
        extern "C" {
            fn getuid() -> u32;
        }
        getuid() == 0
    }
}

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
        let resources_dir = exe_dir
            .parent()
            .map(|p| p.join("Resources"))
            .unwrap_or(exe_dir.clone());
        if resources_dir.exists() {
            return resources_dir;
        }
    }

    exe_dir
}

fn create_private_dir(path: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| format!("Failed to create temp dir: {}", e))?;
    #[cfg(unix)]
    {
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

fn write_private_file(
    path: &std::path::Path,
    content: &str,
    #[cfg_attr(not(unix), allow(unused_variables))] executable: bool,
) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    options.mode(if executable { 0o700 } else { 0o600 });

    let mut file = options
        .open(path)
        .map_err(|e| format!("Failed to write private file {:?}: {}", path, e))?;
    file.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write private file {:?}: {}", path, e))?;
    #[cfg(unix)]
    {
        let mode = if executable { 0o700 } else { 0o600 };
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
    Ok(())
}

#[cfg(unix)]
fn shell_quote(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`");
    format!("\"{}\"", escaped)
}

#[cfg(target_os = "macos")]
fn applescript_quote(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{}\"", escaped)
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
        return Err(format!(
            "{} not found at {:?} or {:?}",
            singbox_name,
            resource_dir.join(singbox_name),
            exe_dir.join(singbox_name)
        ));
    }

    // Write config/log/launcher to temp dir (exe_dir may be in Program Files = read-only)
    let temp_dir = std::env::temp_dir().join("DoodleRay");
    create_private_dir(&temp_dir)?;

    let config_path = temp_dir.join("tun_config.json");
    let config_str = serde_json::to_string_pretty(config_json)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    write_private_file(&config_path, &config_str, false)?;

    // Write a launcher script that captures sing-box output to a log
    let log_path = temp_dir.join("singbox_tun.log");
    let _ = write_private_file(&log_path, "", false);

    #[cfg(windows)]
    {
        let bat_path = temp_dir.join("launch_singbox.bat");
        let bat_content = format!(
            "@echo off\r\n\"{}\" run -c \"{}\" > \"{}\" 2>&1\r\n",
            singbox_exe.to_string_lossy(),
            config_path.to_string_lossy(),
            log_path.to_string_lossy(),
        );
        write_private_file(&bat_path, &bat_content, false)?;

        if is_elevated() {
            // Already running as admin — launch sing-box directly, no UAC prompt
            let bat_str = bat_path.to_string_lossy().to_string();
            Command::new("cmd")
                .args(["/c", &bat_str])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .spawn()
                .map_err(|e| format!("Failed to launch sing-box: {}", e))?;
        } else {
            // Not elevated — need UAC prompt via ShellExecuteW "runas"
            let bat_str: Vec<u16> = bat_path
                .to_string_lossy()
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
                    return Err(
                        "UAC was declined or ShellExecute failed. TUN requires admin privileges."
                            .into(),
                    );
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let sh_path = temp_dir.join("launch_singbox.sh");
        let sh_content = format!(
            "#!/bin/bash\nset -e\n{} run -c {} > {} 2>&1 < /dev/null &\n",
            shell_quote(&singbox_exe.to_string_lossy()),
            shell_quote(&config_path.to_string_lossy()),
            shell_quote(&log_path.to_string_lossy()),
        );
        write_private_file(&sh_path, &sh_content, true)?;

        // Make it executable
        Command::new("chmod")
            .args(["+x", &sh_path.to_string_lossy()])
            .output()
            .map_err(|e| format!("Failed to chmod: {}", e))?;

        // Launch with macOS authorization prompt and wait for the user decision.
        // The launcher backgrounds sing-box, so osascript exits after auth instead of
        // staying attached to the long-running VPN process.
        let shell_command = format!("bash {}", shell_quote(&sh_path.to_string_lossy()));
        let script = format!(
            "do shell script {} with administrator privileges",
            applescript_quote(&shell_command)
        );

        let output = Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| format!("Failed to launch with admin: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let message = if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                "administrator permission was cancelled or denied".into()
            };
            return Err(format!("TUN permission failed: {}", message));
        }
    }

    // Wait for sing-box to actually start (poll instead of hardcoded sleep)
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(300));
        if is_singbox_running() {
            return Ok(());
        }
    }

    // If not running after 3s, check log for errors
    for attempt in 0..5 {
        if is_singbox_running() {
            return Ok(());
        }
        // On later attempts, check the log file for errors
        if attempt >= 2 {
            if let Ok(log_content) = std::fs::read_to_string(&log_path) {
                let trimmed = log_content.trim();
                if !trimmed.is_empty()
                    && (trimmed.contains("FATAL")
                        || trimmed.contains("ERROR")
                        || trimmed.contains("panic"))
                {
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

pub fn is_singbox_running() -> bool {
    #[cfg(windows)]
    {
        if let Ok(output) = Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq sing-box.exe", "/NH"])
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
        if let Ok(output) = Command::new("pgrep").args(["-f", "sing-box"]).output() {
            return output.status.success();
        }
        false
    }
}

pub fn stop_tun() -> Result<(), String> {
    #[cfg(windows)]
    {
        // Production Windows TUN is owned by DoodleRayTunnelService. Do not kill by
        // image name here: that can hit unrelated sing-box users and can surface
        // taskkill/UAC dialogs during shutdown.
    }

    #[cfg(not(windows))]
    {
        // Kill via sudo (may prompt for password)
        let _ = Command::new("pkill").args(["-f", "sing-box"]).output();

        // If regular kill fails, try with sudo
        if is_singbox_running() {
            let _ = Command::new("osascript")
                .args([
                    "-e",
                    "do shell script \"pkill -f sing-box\" with administrator privileges",
                ])
                .output();
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    Ok(())
}

#[cfg(windows)]
pub fn stop_tun_for_update() -> Result<(), String> {
    // The updater path must never display UAC/taskkill windows. The service
    // receives PrepareForUpdate first and stops only service-owned children.
    Ok(())
}

#[cfg(not(windows))]
pub fn stop_tun_for_update() -> Result<(), String> {
    stop_tun()
}
