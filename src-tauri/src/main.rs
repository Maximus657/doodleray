// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(message) = tauri_app_lib::runtime_guard::message(
        option_env!("DOODLERAY_RUNTIME_RESOURCES_SUPPRESSED") == Some("1"),
    ) {
        eprintln!("{message}");
        std::process::exit(1);
    }

    #[cfg(windows)]
    {
        let args = std::env::args().collect::<Vec<_>>();
        if args.get(1).map(String::as_str) == Some("--proxy-guardian") {
            let code = tauri_app_lib::sysproxy::run_proxy_guardian_from_args(&args[2..]);
            std::process::exit(code);
        }
    }

    tauri_app_lib::run()
}
