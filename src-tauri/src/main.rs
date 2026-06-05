// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
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
