fn main() {
    if std::env::var_os("TAURI_CONFIG").is_none()
        && std::env::var("PROFILE").as_deref() == Ok("debug")
    {
        std::env::set_var("TAURI_CONFIG", include_str!("tauri.test.conf.json"));
    }

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" && std::env::var_os("CARGO_FEATURE_APP_STORE").is_some() {
        println!("cargo:rerun-if-changed=macos/HostBridge/NetworkExtensionBridge.h");
        println!("cargo:rerun-if-changed=macos/HostBridge/NetworkExtensionBridge.m");
        cc::Build::new()
            .file("macos/HostBridge/NetworkExtensionBridge.m")
            .flag("-fobjc-arc")
            .compile("doodleray_network_extension_bridge");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=NetworkExtension");
    }

    tauri_build::build()
}
