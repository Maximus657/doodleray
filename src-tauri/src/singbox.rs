use lazy_static::lazy_static;
use libloading::{Library, Symbol};
use serde_json::Value;
use std::ffi::CString;
use std::os::raw::{c_char, c_int};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

lazy_static! {
    static ref SINGBOX_LIB: Mutex<Option<Library>> = Mutex::new(None);
    static ref SINGBOX_PROCESS: Mutex<Option<Child>> = Mutex::new(None);
}

pub fn start_singbox(config_json: &Value) -> Result<(), String> {
    if singbox_exe_path().is_some() {
        match start_singbox_process(config_json) {
            Ok(()) => return Ok(()),
            Err(process_error) => {
                if find_singbox_lib_path().is_none() {
                    return Err(process_error);
                }
            }
        }
    }

    start_singbox_lib(config_json)
}

fn start_singbox_lib(config_json: &Value) -> Result<(), String> {
    let mut lib_guard = SINGBOX_LIB.lock().unwrap();

    if lib_guard.is_none() {
        // Platform-specific library name
        #[cfg(windows)]
        let lib_name = "libsingbox.dll";
        #[cfg(target_os = "macos")]
        let lib_name = "libsingbox.dylib";
        #[cfg(target_os = "linux")]
        let lib_name = "libsingbox.so";

        let path_to_load =
            find_singbox_lib_path().ok_or_else(|| format!("{} is unavailable", lib_name))?;

        let lib = unsafe { Library::new(path_to_load) }
            .map_err(|e| format!("Failed to load {}: {}", lib_name, e))?;
        *lib_guard = Some(lib);
    }

    let lib = lib_guard.as_ref().unwrap();

    let config_str = config_json.to_string();
    let c_config = CString::new(config_str).map_err(|e| e.to_string())?;

    unsafe {
        let start_func: Symbol<unsafe extern "C" fn(*const c_char) -> c_int> = lib
            .get(b"StartSingBox")
            .map_err(|e| format!("Failed to find StartSingBox symbol: {}", e))?;

        let result = start_func(c_config.as_ptr());
        if result != 0 {
            return Err(format!("StartSingBox failed with code: {}", result));
        }
    }

    Ok(())
}

pub fn stop_singbox() -> Result<(), String> {
    if let Some(mut child) = SINGBOX_PROCESS.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }

    let lib_guard = SINGBOX_LIB.lock().unwrap();

    if let Some(lib) = lib_guard.as_ref() {
        unsafe {
            let stop_func: Symbol<unsafe extern "C" fn() -> c_int> = lib
                .get(b"StopSingBox")
                .map_err(|e| format!("Failed to find StopSingBox symbol: {}", e))?;

            let result = stop_func();
            if result != 0 {
                return Err(format!("StopSingBox failed with code: {}", result));
            }
        }
    }

    Ok(())
}

fn start_singbox_process(config_json: &Value) -> Result<(), String> {
    let _ = stop_singbox_process();
    let singbox_exe = singbox_exe_path()
        .ok_or_else(|| "sing-box executable is unavailable".to_string())?;

    let temp_dir = std::env::temp_dir().join("DoodleRay");
    create_private_dir(&temp_dir)?;
    let config_path = temp_dir.join("singbox_proxy_config.json");
    write_private_file(
        &config_path,
        &serde_json::to_string_pretty(config_json)
            .map_err(|e| format!("Failed to serialize sing-box config: {}", e))?,
    )?;

    let mut cmd = Command::new(&singbox_exe);
    cmd.arg("run")
        .arg("-c")
        .arg(&config_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start sing-box executable: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(500));
    if let Ok(Some(status)) = child.try_wait() {
        return Err(format!(
            "sing-box executable exited immediately with status {}",
            status
        ));
    }

    *SINGBOX_PROCESS.lock().unwrap() = Some(child);
    Ok(())
}

fn stop_singbox_process() -> Result<(), String> {
    if let Some(mut child) = SINGBOX_PROCESS.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(())
}

fn find_singbox_lib_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let lib_name = "libsingbox.dll";
    #[cfg(target_os = "macos")]
    let lib_name = "libsingbox.dylib";
    #[cfg(target_os = "linux")]
    let lib_name = "libsingbox.so";

    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let lib_path = exe_dir.join(lib_name);

    let mut candidates = vec![lib_path];

    #[cfg(target_os = "macos")]
    if let Some(resources_dir) = exe_dir.parent().map(|p| p.join("Resources")) {
        candidates.insert(0, resources_dir.join(lib_name));
    }

    candidates.push(std::env::current_dir().ok()?.join("singbox-core").join(lib_name));
    candidates.into_iter().find(|path| path.exists())
}

fn singbox_exe_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let exe_name = "sing-box.exe";
    #[cfg(not(windows))]
    let exe_name = "sing-box";

    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let mut candidates = vec![exe_dir.join(exe_name)];

    #[cfg(target_os = "macos")]
    if let Some(resources_dir) = exe_dir.parent().map(|p| p.join("Resources")) {
        candidates.insert(0, resources_dir.join(exe_name));
    }

    if let Some(parent) = exe_dir.parent() {
        candidates.push(parent.join(exe_name));
    }
    candidates.push(
        std::env::current_dir()
            .ok()?
            .join("singbox-core")
            .join(exe_name),
    );

    candidates.into_iter().find(|path| path.exists())
}

fn create_private_dir(path: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| format!("Failed to create temp dir: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

fn write_private_file(path: &std::path::Path, content: &str) -> Result<(), String> {
    use std::io::Write;

    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options
        .open(path)
        .map_err(|e| format!("Failed to write private file {:?}: {}", path, e))?;
    file.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write private file {:?}: {}", path, e))?;
    Ok(())
}
