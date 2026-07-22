fn main() {
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
