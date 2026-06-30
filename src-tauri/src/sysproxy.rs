//! Windows WinINet proxy ownership for DoodleRay.
//!
//! The app-owned proxy path is intentionally narrow:
//! - write only `127.0.0.1:<http_port>` into `ProxyServer`;
//! - snapshot the user's previous WinINet registry values before applying;
//! - restore that snapshot only while the current proxy still looks DoodleRay-owned;
//! - repair old DoodleRay stale values without clearing unrelated user/corporate proxy settings.

use serde::{Deserialize, Serialize};
use std::ffi::c_void;
use std::fs;
#[cfg(test)]
use std::net::TcpListener;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
#[cfg(not(test))]
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use winreg::enums::*;
use winreg::{RegKey, RegValue};

const INTERNET_SETTINGS: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
const CONNECTIONS_SETTINGS: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections";
const MARKER_SETTINGS: &str = r"Software\DoodleRay\SystemProxy";
const STATE_SCHEMA_VERSION: u32 = 1;
const DEFAULT_PROXY_HOST: &str = "127.0.0.1";
const DEFAULT_PROXY_PORT: u16 = 10809;
const MUTEX_NAME: &str = "Local\\DoodleRay.SystemProxy.v1";

type WinHandle = *mut c_void;

#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(
        lpMutexAttributes: *const c_void,
        bInitialOwner: i32,
        lpName: *const u16,
    ) -> WinHandle;
    fn WaitForSingleObject(hHandle: WinHandle, dwMilliseconds: u32) -> u32;
    fn ReleaseMutex(hMutex: WinHandle) -> i32;
    fn CloseHandle(hObject: WinHandle) -> i32;
    fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> WinHandle;
    fn GetExitCodeProcess(hProcess: WinHandle, lpExitCode: *mut u32) -> i32;
}

const INFINITE: u32 = 0xFFFF_FFFF;
const WAIT_OBJECT_0: u32 = 0x0000_0000;
const WAIT_ABANDONED: u32 = 0x0000_0080;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const STILL_ACTIVE: u32 = 259;

#[derive(Debug)]
struct SystemProxyMutex {
    handle: WinHandle,
    locked: bool,
}

impl SystemProxyMutex {
    fn acquire() -> Result<Self, String> {
        let mut name = MUTEX_NAME.encode_utf16().collect::<Vec<_>>();
        name.push(0);

        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Err("Failed to create system proxy mutex".into());
        }

        let wait = unsafe { WaitForSingleObject(handle, INFINITE) };
        if wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED {
            unsafe {
                CloseHandle(handle);
            }
            return Err(format!("Failed to acquire system proxy mutex: {}", wait));
        }

        Ok(Self {
            handle,
            locked: true,
        })
    }
}

impl Drop for SystemProxyMutex {
    fn drop(&mut self) {
        unsafe {
            if self.locked {
                let _ = ReleaseMutex(self.handle);
            }
            let _ = CloseHandle(self.handle);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyOutcome {
    pub proxy_server: String,
    pub proxy_override: String,
    pub owner_token: String,
    pub state_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RestoreOutcome {
    Restored,
    NoState,
    SkippedChangedByOtherApp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryOutcome {
    Noop,
    ActiveOwnerAlive,
    RestoredOrphaned,
    RepairedLegacy,
    SkippedChangedByOtherApp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepairOutcome {
    Noop,
    RestoredSnapshot,
    CleanedLegacyValues,
    SkippedChangedByOtherApp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StaleProxyState {
    None,
    ActiveManaged,
    OrphanedManaged,
    LegacyDisabledValues,
    LegacyEnabledValues,
    ChangedByOtherApp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProxyOwnership {
    NotDoodleRay,
    CurrentDoodleRay,
    LegacyDoodleRay,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StringSnapshot {
    present: bool,
    value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DwordSnapshot {
    present: bool,
    value: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BinarySnapshot {
    present: bool,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PreviousProxyState {
    proxy_enable: DwordSnapshot,
    proxy_server: StringSnapshot,
    proxy_override: StringSnapshot,
    auto_config_url: StringSnapshot,
    auto_detect: DwordSnapshot,
    proxy_http_1_1: DwordSnapshot,
    default_connection_settings: BinarySnapshot,
    saved_legacy_settings: BinarySnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppliedProxyState {
    proxy_server: String,
    proxy_override: String,
    http_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProxyStateFile {
    schema_version: u32,
    owner_token: String,
    owner_pid: u32,
    captured_at_unix_ms: u128,
    app_version: String,
    previous: PreviousProxyState,
    applied: AppliedProxyState,
}

#[derive(Debug, Clone)]
struct MarkerState {
    owner_token: Option<String>,
    proxy_server: Option<String>,
}

#[derive(Debug, Clone)]
struct CurrentProxyValues {
    proxy_enable: Option<u32>,
    proxy_server: Option<String>,
    proxy_override: Option<String>,
    marker: Option<MarkerState>,
}

pub fn apply_doodleray_proxy(http_port: u16, app_version: &str) -> Result<ApplyOutcome, String> {
    if !loopback_port_ready(http_port) {
        return Err(format!(
            "HTTP proxy port 127.0.0.1:{} is not ready",
            http_port
        ));
    }

    let _guard = SystemProxyMutex::acquire()?;
    let existing_state = load_state_file()?;
    let current = read_current_proxy_values()?;
    let use_existing_previous = existing_state
        .as_ref()
        .map(|state| {
            classify_current_proxy(&current, Some(state)) == ProxyOwnership::CurrentDoodleRay
        })
        .unwrap_or(false);
    let previous = if use_existing_previous {
        existing_state
            .as_ref()
            .map(|state| state.previous.clone())
            .ok_or("Missing existing DoodleRay proxy state")?
    } else {
        capture_previous_proxy_state()?
    };

    let proxy_server = format!("{}:{}", DEFAULT_PROXY_HOST, http_port);
    let proxy_override = local_proxy_bypass();
    let owner_token = Uuid::new_v4().to_string();
    let state_path = state_file_path();
    let state = ProxyStateFile {
        schema_version: STATE_SCHEMA_VERSION,
        owner_token: owner_token.clone(),
        owner_pid: std::process::id(),
        captured_at_unix_ms: now_unix_ms(),
        app_version: app_version.to_string(),
        previous,
        applied: AppliedProxyState {
            proxy_server: proxy_server.clone(),
            proxy_override: proxy_override.clone(),
            http_port,
        },
    };

    write_state_file(&state)?;
    apply_proxy_registry_values(&state)?;
    write_marker(&state, &state_path)?;
    notify_proxy_change();
    reassert_applied_proxy_flags(&state)?;
    spawn_proxy_guardian(&state, &state_path);

    Ok(ApplyOutcome {
        proxy_server,
        proxy_override,
        owner_token,
        state_path: state_path.to_string_lossy().to_string(),
    })
}

pub fn restore_previous_proxy_state() -> Result<RestoreOutcome, String> {
    let _guard = SystemProxyMutex::acquire()?;
    restore_previous_proxy_state_locked()
}

pub fn recover_orphaned_proxy_on_startup() -> Result<RecoveryOutcome, String> {
    let _guard = SystemProxyMutex::acquire()?;
    recover_orphaned_proxy_on_startup_locked()
}

pub fn detect_stale_doodleray_proxy() -> Result<StaleProxyState, String> {
    let _guard = SystemProxyMutex::acquire()?;
    detect_stale_doodleray_proxy_locked()
}

pub fn repair_stale_doodleray_proxy_only() -> Result<RepairOutcome, String> {
    let _guard = SystemProxyMutex::acquire()?;
    repair_stale_doodleray_proxy_only_locked()
}

pub fn run_proxy_guardian_from_args(args: &[String]) -> i32 {
    if args.len() != 3 {
        eprintln!("Usage: --proxy-guardian <state-path> <owner-pid> <owner-token>");
        return 2;
    }

    let state_path = PathBuf::from(&args[0]);
    let owner_pid = match args[1].parse::<u32>() {
        Ok(pid) => pid,
        Err(err) => {
            eprintln!("Invalid proxy guardian owner pid: {}", err);
            return 2;
        }
    };
    let owner_token = args[2].clone();

    run_proxy_guardian(&state_path, owner_pid, &owner_token)
}

pub fn state_file_path_for_ui() -> String {
    state_file_path().to_string_lossy().to_string()
}

fn restore_previous_proxy_state_locked() -> Result<RestoreOutcome, String> {
    let Some(state) = load_state_file()? else {
        clear_marker_key();
        return Ok(RestoreOutcome::NoState);
    };

    let current = read_current_proxy_values()?;
    if classify_current_proxy(&current, Some(&state)) == ProxyOwnership::NotDoodleRay {
        remove_state_file();
        clear_marker_key();
        notify_proxy_change();
        return Ok(RestoreOutcome::SkippedChangedByOtherApp);
    }

    restore_snapshot(&state.previous)?;
    remove_state_file();
    clear_marker_key();
    notify_proxy_change();
    restore_snapshot(&state.previous)?;
    Ok(RestoreOutcome::Restored)
}

fn recover_orphaned_proxy_on_startup_locked() -> Result<RecoveryOutcome, String> {
    if let Some(state) = load_state_file()? {
        let current = read_current_proxy_values()?;
        let ownership = classify_current_proxy(&current, Some(&state));
        if ownership == ProxyOwnership::NotDoodleRay {
            remove_state_file();
            clear_marker_key();
            return Ok(RecoveryOutcome::SkippedChangedByOtherApp);
        }

        let owner_alive = process_is_alive(state.owner_pid);
        let port_alive = loopback_port_ready(state.applied.http_port);
        if !owner_alive || !port_alive {
            restore_snapshot(&state.previous)?;
            remove_state_file();
            clear_marker_key();
            notify_proxy_change();
            restore_snapshot(&state.previous)?;
            return Ok(RecoveryOutcome::RestoredOrphaned);
        }

        return Ok(RecoveryOutcome::ActiveOwnerAlive);
    }

    match detect_stale_doodleray_proxy_locked()? {
        StaleProxyState::LegacyDisabledValues | StaleProxyState::LegacyEnabledValues => {
            match repair_stale_doodleray_proxy_only_locked()? {
                RepairOutcome::Noop | RepairOutcome::SkippedChangedByOtherApp => {
                    Ok(RecoveryOutcome::Noop)
                }
                RepairOutcome::RestoredSnapshot | RepairOutcome::CleanedLegacyValues => {
                    Ok(RecoveryOutcome::RepairedLegacy)
                }
            }
        }
        _ => Ok(RecoveryOutcome::Noop),
    }
}

fn detect_stale_doodleray_proxy_locked() -> Result<StaleProxyState, String> {
    let state = load_state_file()?;
    let current = read_current_proxy_values()?;
    let ownership = classify_current_proxy(&current, state.as_ref());
    let proxy_enabled = current.proxy_enable.unwrap_or(0) != 0;

    if let Some(state) = state {
        if ownership == ProxyOwnership::NotDoodleRay {
            return Ok(StaleProxyState::ChangedByOtherApp);
        }
        if process_is_alive(state.owner_pid) && loopback_port_ready(state.applied.http_port) {
            return Ok(StaleProxyState::ActiveManaged);
        }
        return Ok(StaleProxyState::OrphanedManaged);
    }

    match ownership {
        ProxyOwnership::LegacyDoodleRay if proxy_enabled => {
            Ok(StaleProxyState::LegacyEnabledValues)
        }
        ProxyOwnership::LegacyDoodleRay => Ok(StaleProxyState::LegacyDisabledValues),
        ProxyOwnership::CurrentDoodleRay if proxy_enabled => {
            Ok(StaleProxyState::LegacyEnabledValues)
        }
        ProxyOwnership::CurrentDoodleRay => Ok(StaleProxyState::LegacyDisabledValues),
        ProxyOwnership::NotDoodleRay => Ok(StaleProxyState::None),
    }
}

fn repair_stale_doodleray_proxy_only_locked() -> Result<RepairOutcome, String> {
    if let Some(state) = load_state_file()? {
        let current = read_current_proxy_values()?;
        if classify_current_proxy(&current, Some(&state)) == ProxyOwnership::NotDoodleRay {
            remove_state_file();
            clear_marker_key();
            return Ok(RepairOutcome::SkippedChangedByOtherApp);
        }

        restore_snapshot(&state.previous)?;
        remove_state_file();
        clear_marker_key();
        notify_proxy_change();
        restore_snapshot(&state.previous)?;
        return Ok(RepairOutcome::RestoredSnapshot);
    }

    let current = read_current_proxy_values()?;
    if classify_current_proxy(&current, None) == ProxyOwnership::NotDoodleRay {
        clear_marker_key();
        return Ok(RepairOutcome::Noop);
    }

    let settings = internet_settings_key(true)?;
    set_raw_dword(&settings, "ProxyEnable", 0)
        .map_err(|err| format!("Failed to disable stale DoodleRay proxy: {}", err))?;
    delete_value_if_present(&settings, "ProxyServer");
    delete_value_if_present(&settings, "ProxyOverride");
    clear_marker_key();
    notify_proxy_change();
    Ok(RepairOutcome::CleanedLegacyValues)
}

fn apply_proxy_registry_values(state: &ProxyStateFile) -> Result<(), String> {
    let settings = internet_settings_key(true)?;
    settings
        .set_value("ProxyEnable", &1u32)
        .map_err(|err| format!("Failed to set ProxyEnable: {}", err))?;
    settings
        .set_value("ProxyServer", &state.applied.proxy_server)
        .map_err(|err| format!("Failed to set ProxyServer: {}", err))?;
    settings
        .set_value("ProxyOverride", &state.applied.proxy_override)
        .map_err(|err| format!("Failed to set ProxyOverride: {}", err))?;
    delete_value_if_present(&settings, "AutoConfigURL");
    set_raw_dword(&settings, "AutoDetect", 0)
        .map_err(|err| format!("Failed to disable AutoDetect: {}", err))?;
    settings
        .set_value("ProxyHttp1.1", &1u32)
        .map_err(|err| format!("Failed to set ProxyHttp1.1: {}", err))?;
    Ok(())
}

fn reassert_applied_proxy_flags(state: &ProxyStateFile) -> Result<(), String> {
    let settings = internet_settings_key(true)?;
    settings
        .set_value("ProxyEnable", &1u32)
        .map_err(|err| format!("Failed to reassert ProxyEnable: {}", err))?;
    settings
        .set_value("ProxyServer", &state.applied.proxy_server)
        .map_err(|err| format!("Failed to reassert ProxyServer: {}", err))?;
    settings
        .set_value("ProxyOverride", &state.applied.proxy_override)
        .map_err(|err| format!("Failed to reassert ProxyOverride: {}", err))?;
    delete_value_if_present(&settings, "AutoConfigURL");
    set_raw_dword(&settings, "AutoDetect", 0)
        .map_err(|err| format!("Failed to reassert AutoDetect: {}", err))?;
    settings
        .set_value("ProxyHttp1.1", &1u32)
        .map_err(|err| format!("Failed to reassert ProxyHttp1.1: {}", err))?;
    Ok(())
}

fn capture_previous_proxy_state() -> Result<PreviousProxyState, String> {
    let settings = internet_settings_key(true)?;
    let connections = connections_settings_key(true)?;

    Ok(PreviousProxyState {
        proxy_enable: snapshot_dword(&settings, "ProxyEnable"),
        proxy_server: snapshot_string(&settings, "ProxyServer"),
        proxy_override: snapshot_string(&settings, "ProxyOverride"),
        auto_config_url: snapshot_string(&settings, "AutoConfigURL"),
        auto_detect: snapshot_dword(&settings, "AutoDetect"),
        proxy_http_1_1: snapshot_dword(&settings, "ProxyHttp1.1"),
        default_connection_settings: snapshot_binary(&connections, "DefaultConnectionSettings"),
        saved_legacy_settings: snapshot_binary(&connections, "SavedLegacySettings"),
    })
}

fn restore_snapshot(previous: &PreviousProxyState) -> Result<(), String> {
    let settings = internet_settings_key(true)?;
    let connections = connections_settings_key(true)?;

    restore_dword(&settings, "ProxyEnable", &previous.proxy_enable)?;
    restore_string(&settings, "ProxyServer", &previous.proxy_server)?;
    restore_string(&settings, "ProxyOverride", &previous.proxy_override)?;
    restore_string(&settings, "AutoConfigURL", &previous.auto_config_url)?;
    restore_dword(&settings, "AutoDetect", &previous.auto_detect)?;
    restore_dword(&settings, "ProxyHttp1.1", &previous.proxy_http_1_1)?;
    restore_binary(
        &connections,
        "DefaultConnectionSettings",
        &previous.default_connection_settings,
    )?;
    restore_binary(
        &connections,
        "SavedLegacySettings",
        &previous.saved_legacy_settings,
    )?;

    Ok(())
}

fn snapshot_string(key: &RegKey, name: &str) -> StringSnapshot {
    match key.get_value::<String, _>(name) {
        Ok(value) => StringSnapshot {
            present: true,
            value,
        },
        Err(_) => StringSnapshot {
            present: false,
            value: String::new(),
        },
    }
}

fn snapshot_dword(key: &RegKey, name: &str) -> DwordSnapshot {
    match key.get_value::<u32, _>(name) {
        Ok(value) => DwordSnapshot {
            present: true,
            value,
        },
        Err(_) => DwordSnapshot {
            present: false,
            value: 0,
        },
    }
}

fn snapshot_binary(key: &RegKey, name: &str) -> BinarySnapshot {
    match key.get_raw_value(name) {
        Ok(value) if value.vtype == REG_BINARY => BinarySnapshot {
            present: true,
            bytes: value.bytes,
        },
        Ok(value) => BinarySnapshot {
            present: true,
            bytes: value.bytes,
        },
        Err(_) => BinarySnapshot {
            present: false,
            bytes: Vec::new(),
        },
    }
}

fn restore_string(key: &RegKey, name: &str, snapshot: &StringSnapshot) -> Result<(), String> {
    if snapshot.present {
        key.set_value(name, &snapshot.value)
            .map_err(|err| format!("Failed to restore {}: {}", name, err))
    } else {
        delete_value_if_present(key, name);
        Ok(())
    }
}

fn restore_dword(key: &RegKey, name: &str, snapshot: &DwordSnapshot) -> Result<(), String> {
    if snapshot.present {
        set_raw_dword(key, name, snapshot.value)
            .map_err(|err| format!("Failed to restore {}: {}", name, err))
    } else {
        delete_value_if_present(key, name);
        Ok(())
    }
}

fn set_raw_dword(key: &RegKey, name: &str, value: u32) -> std::io::Result<()> {
    key.set_raw_value(
        name,
        &RegValue {
            vtype: REG_DWORD,
            bytes: value.to_le_bytes().to_vec(),
        },
    )
}

fn restore_binary(key: &RegKey, name: &str, snapshot: &BinarySnapshot) -> Result<(), String> {
    if snapshot.present {
        key.set_raw_value(
            name,
            &RegValue {
                vtype: REG_BINARY,
                bytes: snapshot.bytes.clone(),
            },
        )
        .map_err(|err| format!("Failed to restore {}: {}", name, err))
    } else {
        delete_value_if_present(key, name);
        Ok(())
    }
}

fn read_current_proxy_values() -> Result<CurrentProxyValues, String> {
    let settings = internet_settings_key(true)?;
    Ok(CurrentProxyValues {
        proxy_enable: settings.get_value::<u32, _>("ProxyEnable").ok(),
        proxy_server: settings.get_value::<String, _>("ProxyServer").ok(),
        proxy_override: settings.get_value::<String, _>("ProxyOverride").ok(),
        marker: read_marker(),
    })
}

fn classify_current_proxy(
    current: &CurrentProxyValues,
    state: Option<&ProxyStateFile>,
) -> ProxyOwnership {
    let has_state_match = state
        .map(|state| {
            current
                .proxy_server
                .as_deref()
                .map(|server| server.eq_ignore_ascii_case(&state.applied.proxy_server))
                .unwrap_or(false)
        })
        .unwrap_or(false);
    let has_marker = current
        .marker
        .as_ref()
        .map(|marker| {
            let marker_server_matches = marker
                .proxy_server
                .as_deref()
                .zip(current.proxy_server.as_deref())
                .map(|(marker_server, current_server)| {
                    marker_server.eq_ignore_ascii_case(current_server)
                })
                .unwrap_or(false);
            let marker_token_matches = state
                .and_then(|state| {
                    marker
                        .owner_token
                        .as_ref()
                        .map(|token| token == &state.owner_token)
                })
                .unwrap_or_else(|| marker.owner_token.is_some());
            marker_server_matches || marker_token_matches
        })
        .unwrap_or(false);

    classify_proxy_ownership(
        current.proxy_server.as_deref(),
        current.proxy_override.as_deref(),
        has_marker,
        has_state_match,
    )
}

fn classify_proxy_ownership(
    proxy_server: Option<&str>,
    proxy_override: Option<&str>,
    has_marker: bool,
    has_state_match: bool,
) -> ProxyOwnership {
    let Some(server) = proxy_server
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return ProxyOwnership::NotDoodleRay;
    };

    if has_state_match {
        return ProxyOwnership::CurrentDoodleRay;
    }

    if is_simple_doodleray_proxy(server) && has_marker {
        return ProxyOwnership::CurrentDoodleRay;
    }

    let legacy_context = has_marker || proxy_override.map(has_legacy_game_bypass).unwrap_or(false);
    if looks_like_legacy_doodleray_proxy(server) && legacy_context {
        return ProxyOwnership::LegacyDoodleRay;
    }

    ProxyOwnership::NotDoodleRay
}

fn is_simple_doodleray_proxy(server: &str) -> bool {
    server.eq_ignore_ascii_case(&format!("{}:{}", DEFAULT_PROXY_HOST, DEFAULT_PROXY_PORT))
}

fn looks_like_legacy_doodleray_proxy(server: &str) -> bool {
    let normalized = server.to_ascii_lowercase().replace(' ', "");
    let has_loopback = normalized.contains("127.0.0.1");
    let has_doodleray_ports = normalized.contains(":10809") || normalized.contains(":10808");
    let has_protocol_map = normalized.contains("http=")
        || normalized.contains("https=")
        || normalized.contains("socks=");

    has_loopback && has_doodleray_ports && has_protocol_map
}

fn has_legacy_game_bypass(proxy_override: &str) -> bool {
    let normalized = proxy_override.to_ascii_lowercase();
    normalized.contains("*.riotgames.com")
        || normalized.contains("*.leagueoflegends.com")
        || normalized.contains("*.steampowered.com")
        || normalized.contains("*.epicgames.com")
        || normalized.contains("*.battle.net")
        || normalized.contains("*.roblox.com")
}

fn local_proxy_bypass() -> String {
    let mut entries = vec![
        "<local>".to_string(),
        "localhost".to_string(),
        "127.*".to_string(),
        "[::1]".to_string(),
        "10.*".to_string(),
    ];
    for n in 16..=31 {
        entries.push(format!("172.{}.*", n));
    }
    entries.push("192.168.*".to_string());
    entries.push("*.local".to_string());
    entries.join(";")
}

fn internet_settings_key(write: bool) -> Result<RegKey, String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if write {
        hkcu.create_subkey(INTERNET_SETTINGS)
            .map(|(key, _)| key)
            .map_err(|err| format!("Failed to open Internet Settings registry key: {}", err))
    } else {
        hkcu.open_subkey(INTERNET_SETTINGS)
            .map_err(|err| format!("Failed to open Internet Settings registry key: {}", err))
    }
}

fn connections_settings_key(write: bool) -> Result<RegKey, String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if write {
        hkcu.create_subkey(CONNECTIONS_SETTINGS)
            .map(|(key, _)| key)
            .map_err(|err| format!("Failed to open Connections registry key: {}", err))
    } else {
        hkcu.open_subkey(CONNECTIONS_SETTINGS)
            .map_err(|err| format!("Failed to open Connections registry key: {}", err))
    }
}

fn read_marker() -> Option<MarkerState> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey(MARKER_SETTINGS).ok()?;
    Some(MarkerState {
        owner_token: key.get_value::<String, _>("OwnerToken").ok(),
        proxy_server: key.get_value::<String, _>("ProxyServer").ok(),
    })
}

fn write_marker(state: &ProxyStateFile, state_path: &Path) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(MARKER_SETTINGS)
        .map_err(|err| format!("Failed to create DoodleRay proxy marker: {}", err))?;
    key.set_value("OwnerToken", &state.owner_token)
        .map_err(|err| format!("Failed to write proxy marker token: {}", err))?;
    key.set_value("OwnerPid", &state.owner_pid)
        .map_err(|err| format!("Failed to write proxy marker pid: {}", err))?;
    key.set_value("ProxyServer", &state.applied.proxy_server)
        .map_err(|err| format!("Failed to write proxy marker server: {}", err))?;
    key.set_value("StatePath", &state_path.to_string_lossy().to_string())
        .map_err(|err| format!("Failed to write proxy marker state path: {}", err))?;
    key.set_value("AppVersion", &state.app_version)
        .map_err(|err| format!("Failed to write proxy marker version: {}", err))?;
    Ok(())
}

fn clear_marker_key() {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey_with_flags(MARKER_SETTINGS, KEY_WRITE) {
        for name in [
            "OwnerToken",
            "OwnerPid",
            "ProxyServer",
            "StatePath",
            "AppVersion",
        ] {
            let _ = key.delete_value(name);
        }
    }
    let _ = hkcu.delete_subkey(MARKER_SETTINGS);
}

fn delete_value_if_present(key: &RegKey, name: &str) {
    let _ = key.delete_value(name);
}

fn state_file_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir());
    base.join("DoodleRay")
        .join("system-proxy")
        .join("state.json")
}

fn write_state_file(state: &ProxyStateFile) -> Result<(), String> {
    let path = state_file_path();
    let dir = path
        .parent()
        .ok_or_else(|| "Invalid DoodleRay proxy state path".to_string())?;
    fs::create_dir_all(dir)
        .map_err(|err| format!("Failed to create proxy state directory: {}", err))?;
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(state)
        .map_err(|err| format!("Failed to serialize proxy state: {}", err))?;
    fs::write(&tmp, text).map_err(|err| format!("Failed to write proxy state: {}", err))?;
    fs::rename(&tmp, &path).map_err(|err| format!("Failed to commit proxy state: {}", err))?;
    Ok(())
}

fn load_state_file() -> Result<Option<ProxyStateFile>, String> {
    let path = state_file_path();
    if !path.exists() {
        return Ok(None);
    }
    let text =
        fs::read_to_string(&path).map_err(|err| format!("Failed to read proxy state: {}", err))?;
    let state = serde_json::from_str::<ProxyStateFile>(&text)
        .map_err(|err| format!("Failed to parse proxy state: {}", err))?;
    if state.schema_version != STATE_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported DoodleRay proxy state schema: {}",
            state.schema_version
        ));
    }
    Ok(Some(state))
}

fn remove_state_file() {
    let _ = fs::remove_file(state_file_path());
}

fn loopback_port_ready_once(port: u16) -> bool {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok()
}

fn loopback_port_ready(port: u16) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if loopback_port_ready_once(port) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn process_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut exit_code = 0u32;
        let ok = GetExitCodeProcess(handle, &mut exit_code as *mut u32) != 0;
        let _ = CloseHandle(handle);
        ok && exit_code == STILL_ACTIVE
    }
}

fn spawn_proxy_guardian(state: &ProxyStateFile, state_path: &Path) {
    #[cfg(test)]
    {
        let _ = (state, state_path);
    }

    #[cfg(not(test))]
    {
        if std::env::var_os("DOODLERAY_DISABLE_PROXY_GUARDIAN").is_some() {
            return;
        }

        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let mut command = Command::new(exe);
        command
            .arg("--proxy-guardian")
            .arg(state_path)
            .arg(state.owner_pid.to_string())
            .arg(&state.owner_token);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let _ = command.spawn();
    }
}

fn run_proxy_guardian(state_path: &Path, owner_pid: u32, owner_token: &str) -> i32 {
    for _ in 0..3600 {
        std::thread::sleep(Duration::from_secs(1));

        if !state_path.exists() {
            return 0;
        }

        let _guard = match SystemProxyMutex::acquire() {
            Ok(guard) => guard,
            Err(err) => {
                eprintln!("Proxy guardian failed to acquire mutex: {}", err);
                continue;
            }
        };

        let state = match load_state_file() {
            Ok(Some(state)) => state,
            Ok(None) => return 0,
            Err(err) => {
                eprintln!("Proxy guardian failed to load state: {}", err);
                continue;
            }
        };

        if state.owner_pid != owner_pid || state.owner_token != owner_token {
            return 0;
        }

        let current = match read_current_proxy_values() {
            Ok(current) => current,
            Err(err) => {
                eprintln!("Proxy guardian failed to read proxy values: {}", err);
                continue;
            }
        };

        if classify_current_proxy(&current, Some(&state)) == ProxyOwnership::NotDoodleRay {
            remove_state_file();
            clear_marker_key();
            return 0;
        }

        if !process_is_alive(owner_pid) || !loopback_port_ready(state.applied.http_port) {
            if let Err(err) = restore_snapshot(&state.previous) {
                eprintln!("Proxy guardian restore failed: {}", err);
                continue;
            }
            remove_state_file();
            clear_marker_key();
            notify_proxy_change();
            if let Err(err) = restore_snapshot(&state.previous) {
                eprintln!("Proxy guardian post-notify restore failed: {}", err);
                continue;
            }
            return 0;
        }
    }

    0
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn notify_proxy_change() {
    unsafe {
        #[link(name = "wininet")]
        extern "system" {
            fn InternetSetOptionW(
                hInternet: *mut c_void,
                dwOption: u32,
                lpBuffer: *mut c_void,
                dwBufferLength: u32,
            ) -> i32;
        }
        const INTERNET_OPTION_SETTINGS_CHANGED: u32 = 39;
        const INTERNET_OPTION_REFRESH: u32 = 37;
        const INTERNET_OPTION_PROXY_SETTINGS_CHANGED: u32 = 95;

        let _ = InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_PROXY_SETTINGS_CHANGED,
            std::ptr::null_mut(),
            0,
        );
        let _ = InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_SETTINGS_CHANGED,
            std::ptr::null_mut(),
            0,
        );
        let _ = InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_REFRESH,
            std::ptr::null_mut(),
            0,
        );
    }
}

// Backward-compatible wrappers retained for non-updated call sites/tests.
pub fn set_system_proxy(http_port: u16) -> Result<(), String> {
    apply_doodleray_proxy(http_port, env!("CARGO_PKG_VERSION")).map(|_| ())
}

pub fn unset_system_proxy() -> Result<(), String> {
    restore_previous_proxy_state().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_loopback_proxy_requires_marker_or_state() {
        assert_eq!(
            classify_proxy_ownership(Some("127.0.0.1:10809"), None, false, false),
            ProxyOwnership::NotDoodleRay
        );
        assert_eq!(
            classify_proxy_ownership(Some("127.0.0.1:10809"), None, true, false),
            ProxyOwnership::CurrentDoodleRay
        );
        assert_eq!(
            classify_proxy_ownership(Some("127.0.0.1:10809"), None, false, true),
            ProxyOwnership::CurrentDoodleRay
        );
    }

    #[test]
    fn legacy_protocol_map_detected_with_legacy_bypass() {
        let legacy_server = "http=127.0.0.1:10809;https=127.0.0.1:10809;socks=127.0.0.1:10808";
        let legacy_bypass = "localhost;127.*;10.*;*.riotgames.com;*.leagueoflegends.com";
        assert_eq!(
            classify_proxy_ownership(Some(legacy_server), Some(legacy_bypass), false, false),
            ProxyOwnership::LegacyDoodleRay
        );
    }

    #[test]
    fn old_socks_port_detected_with_marker() {
        assert_eq!(
            classify_proxy_ownership(Some("socks=127.0.0.1:10809"), None, true, false),
            ProxyOwnership::LegacyDoodleRay
        );
    }

    #[test]
    fn arbitrary_loopback_proxy_is_not_owned() {
        assert_eq!(
            classify_proxy_ownership(Some("127.0.0.1:7777"), None, false, false),
            ProxyOwnership::NotDoodleRay
        );
        assert_eq!(
            classify_proxy_ownership(Some("127.0.0.1:10808"), None, false, false),
            ProxyOwnership::NotDoodleRay
        );
    }

    #[test]
    fn local_bypass_does_not_include_game_domains() {
        let bypass = local_proxy_bypass();
        assert!(bypass.contains("<local>"));
        assert!(bypass.contains("172.31.*"));
        assert!(!bypass.contains("riotgames"));
        assert!(!bypass.contains("pubg"));
    }

    #[test]
    fn loopback_port_ready_waits_for_delayed_listener() {
        let reserve = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = reserve.local_addr().unwrap().port();
        drop(reserve);

        let listener = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
            let _ = listener.accept();
        });

        assert!(loopback_port_ready(port));
        let _ = listener.join();
    }
}
