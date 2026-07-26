mod build_config;

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    for path in build_config::WINDOWS_RUNTIME_FILES {
        println!("cargo:rerun-if-changed={path}");
    }

    let runtime_resources_suppressed = build_config::should_use_check_config(
        std::path::Path::new("."),
        &std::env::var("PROFILE").unwrap_or_default(),
        std::env::var_os("TAURI_CONFIG").is_some(),
        &target_os,
    );
    println!(
        "cargo:rustc-env=DOODLERAY_RUNTIME_RESOURCES_SUPPRESSED={}",
        u8::from(runtime_resources_suppressed)
    );

    if runtime_resources_suppressed {
        std::env::set_var("TAURI_CONFIG", include_str!("tauri.test.conf.json"));
    }

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
