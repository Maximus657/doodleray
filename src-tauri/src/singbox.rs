use libloading::{Library, Symbol};
use std::ffi::CString;
use std::os::raw::{c_char, c_int};
use std::sync::Mutex;
use lazy_static::lazy_static;
use serde_json::Value;

lazy_static! {
    static ref SINGBOX_LIB: Mutex<Option<Library>> = Mutex::new(None);
}

pub fn start_singbox(config_json: &Value) -> Result<(), String> {
    let mut lib_guard = SINGBOX_LIB.lock().unwrap();
    
    if lib_guard.is_none() {
        // Platform-specific library name
        #[cfg(windows)]
        let lib_name = "libsingbox.dll";
        #[cfg(target_os = "macos")]
        let lib_name = "libsingbox.dylib";
        #[cfg(target_os = "linux")]
        let lib_name = "libsingbox.so";

        let lib_path = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join(lib_name);
            
        // Fallback for development (running via tauri dev)
        let fallback_path = std::env::current_dir()
            .unwrap()
            .join("singbox-core")
            .join(lib_name);

        let path_to_load = if lib_path.exists() {
            lib_path
        } else if fallback_path.exists() {
            fallback_path
        } else {
            return Err(format!("{} not found", lib_name));
        };

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
