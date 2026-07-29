use std::path::Path;

pub const WINDOWS_RUNTIME_FILES: &[&str] = &[
    "DoodleRayService.exe",
    "sing-box.exe",
    "wintun.dll",
    "xray-core/xray.exe",
    "xray-core/geoip.dat",
    "xray-core/geosite.dat",
    "xray-core/wintun.dll",
];

pub fn should_use_check_config(
    root: &Path,
    profile: &str,
    has_tauri_config: bool,
    target_os: &str,
) -> bool {
    target_os == "windows"
        && profile == "debug"
        && !has_tauri_config
        && WINDOWS_RUNTIME_FILES
            .iter()
            .any(|path| !root.join(path).is_file())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    fn write_runtime_file(root: &Path, path: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, []).unwrap();
    }

    #[test]
    fn complete_windows_runtime_keeps_the_regular_tauri_config() {
        let root =
            std::env::temp_dir().join(format!("doodleray-build-config-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        assert!(super::should_use_check_config(
            &root, "debug", false, "windows"
        ));
        assert!(!super::should_use_check_config(
            &root, "release", false, "windows"
        ));
        assert!(!super::should_use_check_config(
            &root, "debug", true, "windows"
        ));
        assert!(!super::should_use_check_config(
            &root, "debug", false, "macos"
        ));
        for path in super::WINDOWS_RUNTIME_FILES {
            write_runtime_file(&root, path);
        }
        assert!(!super::should_use_check_config(
            &root, "debug", false, "windows"
        ));

        fs::remove_dir_all(root).unwrap();
    }
}
