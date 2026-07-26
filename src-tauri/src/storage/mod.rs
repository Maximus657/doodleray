use super::legacy_subscription_token;
#[cfg(windows)]
use super::{write_private_file, APP_IDENTIFIER};
#[cfg(windows)]
use std::path::PathBuf;
use tauri::Manager;

const SECURE_STORE_SERVICE: &str = "DoodleRay";
const SECURE_STORE_CHUNK_BYTES: usize = 1800;
const SECURE_STORE_CHUNK_PREFIX: &str = "chunked:v1:";
pub(crate) const RENDERER_STATE_KEY: &str = "doodleray-storage";
pub(crate) const APP_API_SESSION_KEY: &str = "app-api-session-v1";
pub(crate) const APP_API_DEVICE_KEY: &str = "app-api-device-v1";

fn validate_secure_store_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key.len() > 60 {
        return Err("Invalid secure storage key length".into());
    }
    if !key
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err("Secure storage key contains unsupported characters".into());
    }
    Ok(())
}

fn is_reserved_secure_store_key(key: &str) -> bool {
    [APP_API_SESSION_KEY, APP_API_DEVICE_KEY]
        .iter()
        .any(|reserved| key == *reserved || key.starts_with(&format!("{}.chunk.", reserved)))
}

fn validate_renderer_secure_store_key(key: &str) -> Result<(), String> {
    validate_secure_store_key(key)?;
    if is_reserved_secure_store_key(key) {
        return Err(
            "This secure storage key is reserved for native DoodleVPN account state.".into(),
        );
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn secure_store_entry(key: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SECURE_STORE_SERVICE, key)
        .map_err(|e| format!("Secure storage unavailable: {}", e))
}

#[cfg(target_os = "macos")]
fn secure_store_macos_options(key: &str) -> security_framework::passwords::PasswordOptions {
    let mut options = security_framework::passwords::PasswordOptions::new_generic_password(
        SECURE_STORE_SERVICE,
        key,
    );
    options.use_protected_keychain();
    options.set_access_synchronized(Some(false));
    options
}

#[cfg(target_os = "macos")]
fn secure_store_native_get(key: &str) -> Result<Option<String>, String> {
    use security_framework::passwords::generic_password;

    match generic_password(secure_store_macos_options(key)) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| "Secure storage entry is not valid UTF-8".to_string()),
        Err(error) if error.code() == -25300 => {
            // One-way migration from the legacy SecKeychain backend used by
            // early macOS builds. App Store builds use the sandbox-compatible
            // Data Protection Keychain above.
            let legacy = keyring::Entry::new(SECURE_STORE_SERVICE, key)
                .map_err(|e| format!("Secure storage unavailable: {}", e))?;
            match legacy.get_password() {
                Ok(value) => {
                    secure_store_native_set(key, &value)?;
                    let _ = legacy.delete_credential();
                    Ok(Some(value))
                }
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(format!("Legacy secure storage read failed: {}", e)),
            }
        }
        Err(error) => Err(format!("Secure storage read failed: {}", error)),
    }
}

#[cfg(not(target_os = "macos"))]
fn secure_store_native_get(key: &str) -> Result<Option<String>, String> {
    match secure_store_entry(key)?.get_password() {
        Ok(value) => return Ok(Some(value)),
        Err(keyring::Error::NoEntry) => {}
        Err(e) => {
            #[cfg(not(windows))]
            return Err(format!("Secure storage read failed: {}", e));
            #[cfg(windows)]
            eprintln!(
                "[warn] secure storage keyring read failed, trying Windows DPAPI: {}",
                e
            );
        }
    }
    // Credential Manager is not reliably available in every Windows session
    // (service accounts, locked-down/Server Core hosts). Fall back to a
    // per-key DPAPI file, same pattern already used for the App API session.
    // A missing/corrupt/inaccessible DPAPI file must degrade to "no data"
    // (same as a clean keyring miss), never a hard error — this value backs
    // login/device state and account data, and callers treat Err very
    // differently from Ok(None) (e.g. failing auth outright instead of
    // starting fresh).
    #[cfg(windows)]
    {
        match app_api_dpapi_get(key) {
            Ok(value) => {
                if let Some(value) = &value {
                    let _ = secure_store_native_set(key, value);
                }
                Ok(value)
            }
            Err(error) => {
                eprintln!("[warn] DPAPI fallback read failed for {}: {}", key, error);
                Ok(None)
            }
        }
    }
    #[cfg(not(windows))]
    Ok(None)
}

#[cfg(target_os = "macos")]
pub(crate) fn secure_store_native_set(key: &str, value: &str) -> Result<(), String> {
    security_framework::passwords::set_generic_password_options(
        value.as_bytes(),
        secure_store_macos_options(key),
    )
    .map_err(|e| format!("Secure storage write failed: {}", e))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn secure_store_native_set(key: &str, value: &str) -> Result<(), String> {
    let keyring_result = secure_store_entry(key)?
        .set_password(value)
        .map_err(|e| format!("Secure storage write failed: {}", e));
    #[cfg(windows)]
    {
        let dpapi_result = app_api_dpapi_set(key, value);
        return match (keyring_result, dpapi_result) {
            (Ok(()), Ok(())) | (Err(_), Ok(())) | (Ok(()), Err(_)) => Ok(()),
            (Err(keyring_error), Err(dpapi_error)) => {
                Err(format!("{}; {}", keyring_error, dpapi_error))
            }
        };
    }
    #[cfg(not(windows))]
    keyring_result
}

#[cfg(target_os = "macos")]
pub(crate) fn secure_store_native_delete(key: &str) -> Result<(), String> {
    use security_framework::passwords::delete_generic_password_options;

    let modern_result = match delete_generic_password_options(secure_store_macos_options(key)) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == -25300 => Ok(()),
        Err(error) => Err(format!("Secure storage delete failed: {}", error)),
    };
    if let Ok(legacy) = keyring::Entry::new(SECURE_STORE_SERVICE, key) {
        let _ = legacy.delete_credential();
    }
    modern_result
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn secure_store_native_delete(key: &str) -> Result<(), String> {
    let keyring_result = match secure_store_entry(key)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Secure storage delete failed: {}", e)),
    };
    #[cfg(windows)]
    {
        let dpapi_result = app_api_dpapi_delete(key);
        return match (keyring_result, dpapi_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(keyring_error), Ok(())) => Err(keyring_error),
            (Ok(()), Err(dpapi_error)) => Err(dpapi_error),
            (Err(keyring_error), Err(dpapi_error)) => {
                Err(format!("{}; {}", keyring_error, dpapi_error))
            }
        };
    }
    #[cfg(not(windows))]
    keyring_result
}

pub(crate) fn secure_store_chunk_key(key: &str, index: usize) -> String {
    format!("{}.chunk.{}", key, index)
}

pub(crate) fn secure_store_chunk_count(value: &str) -> Option<usize> {
    value
        .strip_prefix(SECURE_STORE_CHUNK_PREFIX)
        .and_then(|raw| raw.parse::<usize>().ok())
}

fn secure_store_chunks(value: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for ch in value.chars() {
        if !current.is_empty() && current.len() + ch.len_utf8() > SECURE_STORE_CHUNK_BYTES {
            chunks.push(current);
            current = String::new();
        }
        current.push(ch);
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn delete_secure_store_entry(key: &str) -> Result<(), String> {
    secure_store_native_delete(key)
}

fn delete_secure_store_chunks(key: &str, manifest: &str) {
    let Some(count) = secure_store_chunk_count(manifest) else {
        return;
    };

    for index in 0..count {
        let _ = delete_secure_store_entry(&secure_store_chunk_key(key, index));
    }
}

fn secure_store_fallback_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Secure storage fallback path unavailable: {}", e))?
        .join("secure-storage");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Secure storage fallback init failed: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    Ok(dir)
}

fn secure_store_fallback_path(
    app: &tauri::AppHandle,
    key: &str,
) -> Result<std::path::PathBuf, String> {
    Ok(secure_store_fallback_dir(app)?.join(format!("{}.store", key)))
}

pub(crate) fn secure_store_fallback_get(
    app: &tauri::AppHandle,
    key: &str,
) -> Result<Option<String>, String> {
    let path = secure_store_fallback_path(app, key)?;
    match std::fs::read_to_string(&path) {
        Ok(value) => Ok(Some(value)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Secure storage fallback read failed: {}", e)),
    }
}

fn secure_store_fallback_delete(app: &tauri::AppHandle, key: &str) -> Result<(), String> {
    let path = secure_store_fallback_path(app, key)?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Secure storage fallback delete failed: {}", e)),
    }
}

fn legacy_renderer_state_has_doodle_subscription(value: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(value)
        .ok()
        .and_then(|root| root.get("state")?.get("subscriptions")?.as_array().cloned())
        .is_some_and(|subscriptions| {
            subscriptions.iter().any(|subscription| {
                subscription
                    .get("url")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|url| legacy_subscription_token(url).is_ok())
            })
        })
}

pub(crate) fn secure_store_keyring_get(key: &str) -> Result<Option<String>, String> {
    match secure_store_native_get(key)? {
        Some(value) => {
            let Some(count) = secure_store_chunk_count(&value) else {
                return Ok(Some(value));
            };

            let mut restored = String::new();
            for index in 0..count {
                let chunk = secure_store_native_get(&secure_store_chunk_key(key, index))?
                    .ok_or_else(|| "Secure storage chunk is missing".to_string())?;
                restored.push_str(&chunk);
            }
            Ok(Some(restored))
        }
        None => Ok(None),
    }
}

fn secure_store_keyring_set(key: &str, value: &str) -> Result<(), String> {
    if let Some(old_value) = secure_store_native_get(key)? {
        delete_secure_store_chunks(key, &old_value);
    }

    if value.len() > SECURE_STORE_CHUNK_BYTES {
        let chunks = secure_store_chunks(value);
        for (index, chunk) in chunks.iter().enumerate() {
            secure_store_native_set(&secure_store_chunk_key(key, index), chunk)
                .map_err(|e| format!("Secure storage chunk write failed: {}", e))?;
        }

        return secure_store_native_set(
            key,
            &format!("{}{}", SECURE_STORE_CHUNK_PREFIX, chunks.len()),
        );
    }

    secure_store_native_set(key, value)
}

fn secure_store_keyring_delete(key: &str) -> Result<(), String> {
    if let Some(value) = secure_store_native_get(key)? {
        delete_secure_store_chunks(key, &value);
    }
    delete_secure_store_entry(key)
}

#[cfg(windows)]
fn app_api_dpapi_path(key: &str) -> Result<PathBuf, String> {
    validate_secure_store_key(key)?;
    let app_data = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "Windows app-data path is unavailable".to_string())?;
    let dir = app_data.join(APP_IDENTIFIER).join("secure-storage-native");
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("Native secure storage init failed: {}", error))?;
    Ok(dir.join(format!("{}.dpapi", key)))
}

#[cfg(windows)]
fn app_api_dpapi_transform(key: &str, value: &[u8], protect: bool) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Foundation::{GetLastError, LocalFree};
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let entropy = format!("{}:{}:v1", APP_IDENTIFIER, key).into_bytes();
    let input = CRYPT_INTEGER_BLOB {
        cbData: value.len() as u32,
        pbData: value.as_ptr() as *mut u8,
    };
    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: entropy.len() as u32,
        pbData: entropy.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        if protect {
            CryptProtectData(
                &input,
                std::ptr::null(),
                &entropy_blob,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                &entropy_blob,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 {
        return Err(format!("Windows DPAPI operation failed: {}", unsafe {
            GetLastError()
        }));
    }
    let result =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData as _);
    }
    Ok(result)
}

#[cfg(windows)]
fn app_api_dpapi_get(key: &str) -> Result<Option<String>, String> {
    let path = app_api_dpapi_path(key)?;
    let encrypted = match std::fs::read(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Native secure storage read failed: {}", error)),
    };
    let plaintext = app_api_dpapi_transform(key, &encrypted, false)?;
    String::from_utf8(plaintext)
        .map(Some)
        .map_err(|_| "Native secure storage value is not valid UTF-8".to_string())
}

#[cfg(windows)]
pub(crate) fn app_api_dpapi_set(key: &str, value: &str) -> Result<(), String> {
    let path = app_api_dpapi_path(key)?;
    let encrypted = app_api_dpapi_transform(key, value.as_bytes(), true)?;
    write_private_file(&path, &encrypted)
        .map_err(|error| format!("Native secure storage write failed: {}", error))
}

#[cfg(windows)]
pub(crate) fn app_api_dpapi_delete(key: &str) -> Result<(), String> {
    let path = app_api_dpapi_path(key)?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Native secure storage delete failed: {}", error)),
    }
}

pub(crate) fn app_api_native_secret_get(key: &str) -> Result<Option<String>, String> {
    match secure_store_keyring_get(key) {
        Ok(Some(value)) => return Ok(Some(value)),
        Ok(None) => {}
        Err(keyring_error) => {
            #[cfg(not(windows))]
            return Err(keyring_error);
            #[cfg(windows)]
            eprintln!(
                "[warn] keyring read failed, trying Windows DPAPI: {}",
                keyring_error
            );
        }
    }
    #[cfg(windows)]
    {
        match app_api_dpapi_get(key) {
            Ok(value) => {
                if let Some(value) = &value {
                    let _ = secure_store_keyring_set(key, value);
                }
                Ok(value)
            }
            Err(error) => {
                eprintln!("[warn] DPAPI fallback read failed for {}: {}", key, error);
                Ok(None)
            }
        }
    }
    #[cfg(not(windows))]
    Ok(None)
}

pub(crate) fn app_api_native_secret_set(key: &str, value: &str) -> Result<(), String> {
    let keyring_result = secure_store_keyring_set(key, value);
    #[cfg(windows)]
    {
        let dpapi_result = app_api_dpapi_set(key, value);
        match (keyring_result, dpapi_result) {
            (Ok(()), Ok(())) | (Err(_), Ok(())) | (Ok(()), Err(_)) => Ok(()),
            (Err(keyring_error), Err(dpapi_error)) => {
                Err(format!("{}; {}", keyring_error, dpapi_error))
            }
        }
    }
    #[cfg(not(windows))]
    keyring_result
}

pub(crate) fn app_api_native_secret_delete(key: &str) -> Result<(), String> {
    let keyring_result = secure_store_keyring_delete(key);
    #[cfg(windows)]
    {
        let dpapi_result = app_api_dpapi_delete(key);
        match (keyring_result, dpapi_result) {
            (Ok(()), Ok(())) | (Err(_), Ok(())) => Ok(()),
            (Ok(()), Err(dpapi_error)) => Err(dpapi_error),
            (Err(keyring_error), Err(dpapi_error)) => {
                Err(format!("{}; {}", keyring_error, dpapi_error))
            }
        }
    }
    #[cfg(not(windows))]
    keyring_result
}

#[tauri::command(async)]
pub(crate) fn secure_store_get(
    app: tauri::AppHandle,
    key: String,
) -> Result<Option<String>, String> {
    validate_renderer_secure_store_key(&key)?;

    // 5.x wrote a plaintext app-data fallback before updating Credential
    // Manager. An interrupted/RC upgrade can therefore leave a newer empty
    // credential beside the still-valid 5.x state. Consume that fallback once
    // when it contains a real DoodleVPN subscription, then make keyring
    // canonical. This preserves the no-code 5.x -> 6.x account migration.
    if key == RENDERER_STATE_KEY {
        if let Some(legacy_value) = secure_store_fallback_get(&app, &key)? {
            if legacy_renderer_state_has_doodle_subscription(&legacy_value) {
                secure_store_keyring_set(&key, &legacy_value).map_err(|error| {
                    format!("Legacy renderer state migration failed: {}", error)
                })?;
                secure_store_fallback_delete(&app, &key)?;
                return Ok(Some(legacy_value));
            }
        }
    }

    match secure_store_keyring_get(&key) {
        Ok(Some(value)) => {
            // Remove the legacy plaintext mirror once Keychain/Credential
            // Manager is confirmed readable.
            if let Err(fallback_error) = secure_store_fallback_delete(&app, &key) {
                eprintln!(
                    "[warn] legacy secure storage fallback cleanup failed: {}",
                    fallback_error
                );
            }
            Ok(Some(value))
        }
        Ok(None) => {
            // One-way migration for builds that mirrored the renderer state
            // into app data as plaintext. Never create or refresh this file.
            let Some(legacy_value) = secure_store_fallback_get(&app, &key)? else {
                return Ok(None);
            };
            secure_store_keyring_set(&key, &legacy_value)
                .map_err(|error| format!("Legacy secure storage migration failed: {}", error))?;
            secure_store_fallback_delete(&app, &key)?;
            Ok(Some(legacy_value))
        }
        Err(keyring_error) => Err(keyring_error),
    }
}

#[tauri::command(async)]
pub(crate) fn secure_store_set(
    app: tauri::AppHandle,
    key: String,
    value: String,
) -> Result<(), String> {
    validate_renderer_secure_store_key(&key)?;
    secure_store_keyring_set(&key, &value)?;
    if let Err(fallback_error) = secure_store_fallback_delete(&app, &key) {
        eprintln!(
            "[warn] legacy secure storage fallback cleanup failed: {}",
            fallback_error
        );
    }
    Ok(())
}

#[tauri::command(async)]
pub(crate) fn secure_store_delete(app: tauri::AppHandle, key: String) -> Result<(), String> {
    validate_renderer_secure_store_key(&key)?;
    let keyring_result = secure_store_keyring_delete(&key);
    let fallback_result = secure_store_fallback_delete(&app, &key);

    match (keyring_result, fallback_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(keyring_error), Ok(())) => Err(keyring_error),
        (Ok(()), Err(fallback_error)) => Err(fallback_error),
        (Err(keyring_error), Err(fallback_error)) => {
            Err(format!("{}; {}", keyring_error, fallback_error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn legacy_renderer_state_requires_a_doodlevpn_subscription() {
        let doodle = json!({
            "state": {
                "subscriptions": [
                    { "url": "https://ddlvpn.lol/s/oldDesktopToken123" },
                    { "url": "https://example.com/sub/external-token" }
                ]
            },
            "version": 0
        });
        let external = json!({
            "state": {
                "subscriptions": [
                    { "url": "https://example.com/sub/external-token" }
                ]
            },
            "version": 0
        });

        assert!(legacy_renderer_state_has_doodle_subscription(
            &doodle.to_string()
        ));
        assert!(!legacy_renderer_state_has_doodle_subscription(
            &external.to_string()
        ));
        assert!(!legacy_renderer_state_has_doodle_subscription("not-json"));
    }

    #[test]
    fn renderer_secure_store_cannot_access_app_api_reserved_keys() {
        assert!(validate_renderer_secure_store_key(APP_API_SESSION_KEY).is_err());
        assert!(
            validate_renderer_secure_store_key(&format!("{}.chunk.0", APP_API_SESSION_KEY))
                .is_err()
        );
        assert!(validate_renderer_secure_store_key(APP_API_DEVICE_KEY).is_err());
        assert!(validate_renderer_secure_store_key("ui-preference-theme").is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn windows_dpapi_native_secret_roundtrip() {
        let plaintext = b"refresh-session-test";
        let encrypted =
            app_api_dpapi_transform("qa-session", plaintext, true).expect("DPAPI encryption");
        assert_ne!(encrypted, plaintext);
        let decrypted =
            app_api_dpapi_transform("qa-session", &encrypted, false).expect("DPAPI decryption");
        assert_eq!(decrypted, plaintext);
    }
}
