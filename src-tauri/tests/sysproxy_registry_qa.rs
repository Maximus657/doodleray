#![cfg(windows)]

use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use tauri_app_lib::sysproxy;
use winreg::enums::*;
use winreg::{RegKey, RegValue};

const INTERNET_SETTINGS: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
const CONNECTIONS_SETTINGS: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections";
const MARKER_SETTINGS: &str = r"Software\DoodleRay\SystemProxy";
const HTTP_PORT: u16 = 10809;

const INTERNET_VALUES: &[&str] = &[
    "ProxyEnable",
    "ProxyServer",
    "ProxyOverride",
    "AutoConfigURL",
    "AutoDetect",
    "ProxyHttp1.1",
];
const CONNECTION_VALUES: &[&str] = &["DefaultConnectionSettings", "SavedLegacySettings"];

struct KeyBackup {
    existed: bool,
    values: BTreeMap<String, RegValue>,
}

struct RegistryQaGuard {
    internet: KeyBackup,
    connections: KeyBackup,
    marker: KeyBackup,
    state_path: PathBuf,
    state_file: Option<Vec<u8>>,
}

impl RegistryQaGuard {
    fn capture() -> Self {
        let state_path = PathBuf::from(sysproxy::state_file_path_for_ui());
        Self {
            internet: backup_selected_values(INTERNET_SETTINGS, INTERNET_VALUES),
            connections: backup_selected_values(CONNECTIONS_SETTINGS, CONNECTION_VALUES),
            marker: backup_all_values(MARKER_SETTINGS),
            state_file: fs::read(&state_path).ok(),
            state_path,
        }
    }
}

impl Drop for RegistryQaGuard {
    fn drop(&mut self) {
        restore_selected_values(INTERNET_SETTINGS, INTERNET_VALUES, &self.internet);
        restore_selected_values(CONNECTIONS_SETTINGS, CONNECTION_VALUES, &self.connections);
        restore_whole_key(MARKER_SETTINGS, &self.marker);
        match &self.state_file {
            Some(bytes) => {
                if let Some(parent) = self.state_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(&self.state_path, bytes);
            }
            None => {
                let _ = fs::remove_file(&self.state_path);
            }
        }
    }
}

#[test]
#[ignore = "mutates HKCU Windows proxy settings; run only for manual QA"]
fn windows_system_proxy_registry_lifecycle_qa() {
    let _guard = RegistryQaGuard::capture();
    std::env::set_var("DOODLERAY_DISABLE_PROXY_GUARDIAN", "1");
    let _listener = TcpListener::bind(("127.0.0.1", HTTP_PORT)).ok();

    scenario_empty_previous_proxy_roundtrip();
    scenario_existing_corporate_proxy_roundtrip();
    scenario_legacy_disabled_proxy_repair();
    scenario_user_change_is_not_overwritten();
    scenario_guardian_restores_orphaned_proxy();
}

fn scenario_empty_previous_proxy_roundtrip() {
    reset_test_values();

    let outcome = sysproxy::apply_doodleray_proxy(HTTP_PORT, "qa").expect("apply empty previous");
    assert_eq!(outcome.proxy_server, "127.0.0.1:10809");
    assert_eq!(
        read_string(INTERNET_SETTINGS, "ProxyServer"),
        Some("127.0.0.1:10809".into())
    );
    assert_eq!(read_dword(INTERNET_SETTINGS, "ProxyEnable"), Some(1));
    let bypass = read_string(INTERNET_SETTINGS, "ProxyOverride").unwrap_or_default();
    assert!(bypass.contains("<local>"));
    assert!(bypass.contains("172.31.*"));
    assert!(!bypass.contains("riotgames"));
    assert!(!bypass.contains("steampowered"));
    assert!(read_string(INTERNET_SETTINGS, "AutoConfigURL").is_none());

    let restored = sysproxy::restore_previous_proxy_state().expect("restore empty previous");
    assert_eq!(format!("{:?}", restored), "Restored");
    assert!(read_string(INTERNET_SETTINGS, "ProxyServer").is_none());
    assert!(read_string(INTERNET_SETTINGS, "ProxyOverride").is_none());
    assert!(matches!(
        read_dword(INTERNET_SETTINGS, "ProxyEnable"),
        None | Some(0)
    ));
}

fn scenario_existing_corporate_proxy_roundtrip() {
    reset_test_values();
    let internet = create_key(INTERNET_SETTINGS);
    internet.set_value("ProxyEnable", &1u32).unwrap();
    internet
        .set_value("ProxyServer", &"corp.proxy.local:8080")
        .unwrap();
    internet
        .set_value("ProxyOverride", &"intranet.local;*.corp")
        .unwrap();
    internet
        .set_value("AutoConfigURL", &"https://pac.corp/proxy.pac")
        .unwrap();
    internet.set_value("AutoDetect", &1u32).unwrap();
    internet.set_value("ProxyHttp1.1", &0u32).unwrap();
    let connections = create_key(CONNECTIONS_SETTINGS);
    connections
        .set_raw_value(
            "DefaultConnectionSettings",
            &RegValue {
                vtype: REG_BINARY,
                bytes: vec![70, 71, 72, 73],
            },
        )
        .unwrap();
    connections
        .set_raw_value(
            "SavedLegacySettings",
            &RegValue {
                vtype: REG_BINARY,
                bytes: vec![80, 81, 82],
            },
        )
        .unwrap();
    let before = snapshot_all_test_values();

    sysproxy::apply_doodleray_proxy(HTTP_PORT, "qa").expect("apply corporate previous");
    assert_eq!(
        read_string(INTERNET_SETTINGS, "ProxyServer"),
        Some("127.0.0.1:10809".into())
    );
    assert_eq!(read_dword(INTERNET_SETTINGS, "AutoDetect"), Some(0));
    assert!(read_string(INTERNET_SETTINGS, "AutoConfigURL").is_none());

    sysproxy::restore_previous_proxy_state().expect("restore corporate previous");
    assert_eq!(snapshot_all_test_values(), before);
}

fn scenario_legacy_disabled_proxy_repair() {
    reset_test_values();
    let internet = create_key(INTERNET_SETTINGS);
    internet.set_value("ProxyEnable", &0u32).unwrap();
    internet
        .set_value(
            "ProxyServer",
            &"http=127.0.0.1:10809;https=127.0.0.1:10809;socks=127.0.0.1:10808",
        )
        .unwrap();
    internet
        .set_value(
            "ProxyOverride",
            &"localhost;127.*;10.*;*.riotgames.com;*.leagueoflegends.com",
        )
        .unwrap();

    let stale = sysproxy::detect_stale_doodleray_proxy().expect("detect legacy stale");
    assert_eq!(format!("{:?}", stale), "LegacyDisabledValues");
    let repaired = sysproxy::repair_stale_doodleray_proxy_only().expect("repair legacy stale");
    assert_eq!(format!("{:?}", repaired), "CleanedLegacyValues");
    assert_eq!(read_dword(INTERNET_SETTINGS, "ProxyEnable"), Some(0));
    assert!(read_string(INTERNET_SETTINGS, "ProxyServer").is_none());
    assert!(read_string(INTERNET_SETTINGS, "ProxyOverride").is_none());
}

fn scenario_user_change_is_not_overwritten() {
    reset_test_values();
    let internet = create_key(INTERNET_SETTINGS);
    internet.set_value("ProxyEnable", &1u32).unwrap();
    internet
        .set_value("ProxyServer", &"original.proxy.local:8080")
        .unwrap();

    sysproxy::apply_doodleray_proxy(HTTP_PORT, "qa").expect("apply before user change");
    internet
        .set_value("ProxyServer", &"another.client.local:9090")
        .unwrap();
    let outcome = sysproxy::restore_previous_proxy_state().expect("restore after user change");
    assert_eq!(format!("{:?}", outcome), "SkippedChangedByOtherApp");
    assert_eq!(
        read_string(INTERNET_SETTINGS, "ProxyServer"),
        Some("another.client.local:9090".into())
    );
}

fn scenario_guardian_restores_orphaned_proxy() {
    reset_test_values();
    let internet = create_key(INTERNET_SETTINGS);
    internet.set_value("ProxyEnable", &1u32).unwrap();
    internet
        .set_value("ProxyServer", &"guardian.previous.local:8080")
        .unwrap();

    let outcome = sysproxy::apply_doodleray_proxy(HTTP_PORT, "qa").expect("apply before guardian");
    let state_path = PathBuf::from(outcome.state_path);
    let mut state: Value =
        serde_json::from_slice(&fs::read(&state_path).expect("read guardian state")).unwrap();
    let dead_pid = 4_294_000_000u64;
    state["owner_pid"] = Value::from(dead_pid);
    fs::write(&state_path, serde_json::to_vec_pretty(&state).unwrap()).unwrap();

    let exe = doodleray_exe_path();
    let mut child = Command::new(exe)
        .arg("--proxy-guardian")
        .arg(&state_path)
        .arg(dead_pid.to_string())
        .arg(&outcome.owner_token)
        .spawn()
        .expect("spawn DoodleRay proxy guardian");

    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("poll guardian") {
            assert!(status.success(), "guardian exited with {status:?}");
            break;
        }
        if started.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            panic!("guardian did not restore and exit in time");
        }
        thread::sleep(Duration::from_millis(250));
    }

    assert_eq!(
        read_string(INTERNET_SETTINGS, "ProxyServer"),
        Some("guardian.previous.local:8080".into())
    );
    assert!(fs::read(&state_path).is_err());
}

fn reset_test_values() {
    restore_selected_values(
        INTERNET_SETTINGS,
        INTERNET_VALUES,
        &KeyBackup {
            existed: true,
            values: BTreeMap::new(),
        },
    );
    restore_selected_values(
        CONNECTIONS_SETTINGS,
        CONNECTION_VALUES,
        &KeyBackup {
            existed: true,
            values: BTreeMap::new(),
        },
    );
    restore_whole_key(
        MARKER_SETTINGS,
        &KeyBackup {
            existed: false,
            values: BTreeMap::new(),
        },
    );
    let _ = fs::remove_file(sysproxy::state_file_path_for_ui());
}

fn snapshot_all_test_values() -> BTreeMap<String, Option<Vec<u8>>> {
    let mut values = BTreeMap::new();
    for name in INTERNET_VALUES {
        values.insert(
            format!("internet:{name}"),
            raw_value(INTERNET_SETTINGS, name).map(|value| value.bytes),
        );
    }
    for name in CONNECTION_VALUES {
        values.insert(
            format!("connections:{name}"),
            raw_value(CONNECTIONS_SETTINGS, name).map(|value| value.bytes),
        );
    }
    values
}

fn backup_selected_values(path: &str, names: &[&str]) -> KeyBackup {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey_with_flags(path, KEY_READ) else {
        return KeyBackup {
            existed: false,
            values: BTreeMap::new(),
        };
    };
    let mut values = BTreeMap::new();
    for name in names {
        if let Ok(value) = key.get_raw_value(name) {
            values.insert((*name).to_string(), value);
        }
    }
    KeyBackup {
        existed: true,
        values,
    }
}

fn backup_all_values(path: &str) -> KeyBackup {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey_with_flags(path, KEY_READ) else {
        return KeyBackup {
            existed: false,
            values: BTreeMap::new(),
        };
    };
    let values = key
        .enum_values()
        .filter_map(Result::ok)
        .collect::<BTreeMap<_, _>>();
    KeyBackup {
        existed: true,
        values,
    }
}

fn restore_selected_values(path: &str, names: &[&str], backup: &KeyBackup) {
    let key = create_key(path);
    for name in names {
        let _ = key.delete_value(name);
    }
    for (name, value) in &backup.values {
        let _ = key.set_raw_value(name, value);
    }
}

fn restore_whole_key(path: &str, backup: &KeyBackup) {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey(path);
    if !backup.existed {
        return;
    }
    let (key, _) = hkcu.create_subkey(path).unwrap();
    for (name, value) in &backup.values {
        let _ = key.set_raw_value(name, value);
    }
}

fn create_key(path: &str) -> RegKey {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.create_subkey(path).unwrap().0
}

fn raw_value(path: &str, name: &str) -> Option<RegValue> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(path, KEY_READ)
        .ok()?
        .get_raw_value(name)
        .ok()
}

fn read_string(path: &str, name: &str) -> Option<String> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(path, KEY_READ)
        .ok()?
        .get_value(name)
        .ok()
}

fn read_dword(path: &str, name: &str) -> Option<u32> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(path, KEY_READ)
        .ok()?
        .get_value(name)
        .ok()
}

fn doodleray_exe_path() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_DoodleRay") {
        return PathBuf::from(path);
    }
    if let Ok(path) = std::env::var("DOODLERAY_EXE") {
        return PathBuf::from(path);
    }
    let test_exe = std::env::current_exe().unwrap();
    let debug_dir = test_exe
        .parent()
        .and_then(Path::parent)
        .expect("target debug directory");
    debug_dir.join("DoodleRay.exe")
}
