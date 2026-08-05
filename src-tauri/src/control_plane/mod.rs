use super::{
    default_system_proxy_mode, default_xray_api_port, ping_server_profile, record_connect_timing,
    redact_support_line, reset_connect_timings, system_proxy_fetch_client, vpn_connect_authorized,
    vpn_disconnect, ConnectRequest, ConnectResult, PingResult, RoutingRuleRequest,
    APP_PRODUCT_NAME, APP_ROUTING_ROOT_PUBLIC_KEY_BASE64,
};
#[cfg(windows)]
use super::{ensure_tunnel_service_running, ipc};
#[cfg(windows)]
use crate::storage::{
    app_api_dpapi_delete, app_api_dpapi_set, secure_store_chunk_count, secure_store_chunk_key,
    secure_store_entry,
};
use crate::storage::{
    app_api_native_secret_delete, app_api_native_secret_get, app_api_native_secret_set,
    secure_store_fallback_get, secure_store_keyring_get, APP_API_DEVICE_KEY,
    APP_API_PROFILE_CACHE_KEY, APP_API_SESSION_KEY, RENDERER_STATE_KEY,
};
#[cfg(not(windows))]
use crate::storage::{secure_store_native_delete, secure_store_native_set};
use crate::vpn::config::is_managed_xray_balancer_config;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use reqwest::Url;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::IpAddr;
#[cfg(all(target_os = "macos", feature = "app-store"))]
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(super) const APP_API_DEFAULT_BASE_URL: &str = "https://ddlvpn.lol/v1/mobile";
pub(super) const APP_API_CONNECTION_PROFILE_PATH: &str = "/connection-profile";
const APP_ROUTING_ROOT_KID: &str = "dogfood-20260513-ed25519";
const APP_ROUTING_ASSET_CANONICAL_RULE_VERSION: &str = "routing_asset.v1.lines";
pub(super) const APP_API_SESSION_TOMBSTONE: &str = "invalidated:v1";
const APP_API_PROFILE_CACHE_MAX_ENTRIES: usize = 8;

static APP_API_MEMORY_SESSION: Mutex<Option<AppApiTokenResponse>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppApiAntiJammerSummary {
    #[serde(default)]
    pub limit_bytes: u64,
    #[serde(default)]
    pub used_bytes: u64,
    #[serde(default)]
    pub remaining_bytes: u64,
    #[serde(default)]
    pub low_balance: bool,
    #[serde(default)]
    pub exhausted: bool,
    #[serde(default)]
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppApiSubscriptionSummary {
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub device_allowed: Option<bool>,
    #[serde(default)]
    pub remnawave_status: String,
    #[serde(default)]
    pub expires_at: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub user_uuid: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub anti_jammer: Option<AppApiAntiJammerSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppApiTokenResponse {
    pub access_token: String,
    pub access_expires_at: String,
    #[serde(default)]
    pub expires_in: i64,
    pub refresh_token: String,
    pub refresh_expires_at: String,
    pub device_id: String,
    pub subscription: AppApiSubscriptionSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppApiStoredSession {
    pub refresh_token: String,
    pub refresh_expires_at: String,
    pub device_id: String,
    pub subscription: AppApiSubscriptionSummary,
}

#[derive(Debug, Serialize)]
pub struct AppApiSessionStatus {
    pub logged_in: bool,
    pub device_id: Option<String>,
    pub access_expires_at: Option<String>,
    pub refresh_expires_at: Option<String>,
    pub subscription: Option<AppApiSubscriptionSummary>,
    pub api_base_url: String,
    pub closed_control_plane_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct AppApiDeviceState {
    pub(super) client_device_id: String,
    pub(super) hwid: String,
    pub(super) public_key: String,
    #[serde(default)]
    pub(super) public_key_jwk: serde_json::Value,
    #[serde(default)]
    pub(super) private_key_seed: String,
    #[serde(default)]
    pub(super) key_alg: String,
}

#[derive(Debug, Deserialize)]
pub struct AppApiExchangeCodeRequest {
    pub code: String,
}

#[derive(Debug, Deserialize)]
pub struct AppApiExchangeLegacySubscriptionRequest {
    pub subscription_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppApiLocation {
    pub id: String,
    pub country_code: String,
    pub title: String,
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub sort: i32,
    #[serde(default)]
    pub available_nodes_count: i32,
    #[serde(default)]
    pub healthy_nodes_count: Option<i32>,
    #[serde(default)]
    pub capacity_label: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AppApiLocationsResponse {
    #[serde(default)]
    pub locations: Vec<AppApiLocation>,
}

#[derive(Debug, Deserialize)]
pub struct AppApiDiagnosticsSubmission {
    #[serde(default)]
    manual: bool,
    #[serde(default)]
    events: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AppRoutingSignature {
    #[serde(default)]
    pub kid: String,
    #[serde(default)]
    pub alg: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AppRoutingAsset {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub etag: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub canonical_rule_version: String,
    #[serde(default)]
    pub signature: Option<AppRoutingSignature>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AppRoutingPolicy {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub direct_domains: Vec<String>,
    #[serde(default)]
    pub local_dns_domains: Vec<String>,
    #[serde(default)]
    pub direct_ip_ranges: Vec<String>,
    #[serde(default)]
    pub asset: Option<AppRoutingAsset>,
}

fn app_routing_asset_signing_bytes(asset: &AppRoutingAsset) -> Vec<u8> {
    format!(
        "doodleray-routing-asset-v1\n{}\n{}\n{}\n{}\n{}\n{}",
        asset.url,
        asset.sha256,
        asset.size_bytes,
        asset.etag,
        asset.version,
        asset.canonical_rule_version,
    )
    .into_bytes()
}

fn verify_app_routing_asset(asset: &AppRoutingAsset) -> Result<(), String> {
    let signature = asset
        .signature
        .as_ref()
        .ok_or_else(|| "DoodleVPN routing data is not signed.".to_string())?;
    if signature.kid != APP_ROUTING_ROOT_KID
        || signature.alg != "EdDSA"
        || asset.canonical_rule_version != APP_ROUTING_ASSET_CANONICAL_RULE_VERSION
    {
        return Err("DoodleVPN routing data has unsupported signature metadata.".into());
    }
    let public_key = URL_SAFE_NO_PAD
        .decode(APP_ROUTING_ROOT_PUBLIC_KEY_BASE64)
        .map_err(|_| "DoodleVPN routing verifier is invalid.".to_string())?;
    let public_key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| "DoodleVPN routing verifier is invalid.".to_string())?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| "DoodleVPN routing verifier is invalid.".to_string())?;
    let signature_bytes = URL_SAFE_NO_PAD
        .decode(&signature.value)
        .map_err(|_| "DoodleVPN routing signature is invalid.".to_string())?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| "DoodleVPN routing signature is invalid.".to_string())?;
    verifying_key
        .verify(&app_routing_asset_signing_bytes(asset), &signature)
        .map_err(|_| "DoodleVPN routing data signature verification failed.".to_string())
}

pub(super) fn validate_app_routing_policy(
    mut policy: AppRoutingPolicy,
) -> Result<AppRoutingPolicy, String> {
    if !matches!(policy.mode.as_str(), "full_tunnel" | "split") {
        return Err(
            "DoodleVPN returned an unsupported routing policy. Update the app and try again."
                .into(),
        );
    }
    if policy.version.len() > 128
        || policy.direct_domains.len() > 4096
        || policy.local_dns_domains.len() > 4096
        || policy.direct_ip_ranges.len() > 512
    {
        return Err("DoodleVPN returned an invalid routing policy.".into());
    }

    let valid_selector = |value: &String| {
        !value.is_empty()
            && value.len() <= 512
            && !value.chars().any(char::is_whitespace)
            && !value.chars().any(char::is_control)
    };
    if !policy.direct_domains.iter().all(valid_selector)
        || !policy.local_dns_domains.iter().all(valid_selector)
    {
        return Err("DoodleVPN returned an invalid routing domain selector.".into());
    }
    if !policy.direct_ip_ranges.iter().all(|value| {
        let Some((address, prefix)) = value.split_once('/') else {
            return false;
        };
        let Ok(address) = address.parse::<IpAddr>() else {
            return false;
        };
        let Ok(prefix) = prefix.parse::<u8>() else {
            return false;
        };
        prefix <= if address.is_ipv4() { 32 } else { 128 }
    }) {
        return Err("DoodleVPN returned an invalid routing IP range.".into());
    }

    for values in [
        &mut policy.direct_domains,
        &mut policy.local_dns_domains,
        &mut policy.direct_ip_ranges,
    ] {
        values.sort();
        values.dedup();
    }
    if policy.mode == "full_tunnel" {
        policy.direct_domains.clear();
        policy.local_dns_domains.clear();
        policy.direct_ip_ranges.clear();
    }

    if let Some(asset) = policy.asset.as_ref() {
        let valid_hash =
            asset.sha256.len() == 64 && asset.sha256.bytes().all(|byte| byte.is_ascii_hexdigit());
        if !asset.url.starts_with('/')
            || asset.url.starts_with("//")
            || asset.url.contains("..")
            || !valid_hash
            || asset.size_bytes == 0
            || asset.size_bytes > 64 * 1024 * 1024
        {
            return Err("DoodleVPN returned an invalid routing asset.".into());
        }
        verify_app_routing_asset(asset)?;
    }
    Ok(policy)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AppApiProfileLeaseResponse {
    #[serde(default)]
    pub(super) schema_version: i32,
    pub(super) profile_id: String,
    pub(super) lease_id: String,
    pub(super) expires_at: String,
    pub(super) location_id: String,
    #[serde(default)]
    pub(super) route_kind: String,
    #[serde(default)]
    pub(super) first_hop: String,
    #[serde(default)]
    pub(super) target_country_id: String,
    #[serde(default)]
    pub(super) entry_role: String,
    #[serde(default)]
    pub(super) routing_rules_version: String,
    #[serde(default)]
    pub(super) routing_policy: Option<AppRoutingPolicy>,
    #[serde(default)]
    pub(super) native_profile: serde_json::Value,
    #[serde(default)]
    pub(super) profile: Option<serde_json::Value>,
    #[serde(default)]
    pub(super) transport_capability: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AppApiProfileCache {
    #[serde(default)]
    app_version: String,
    device_id: String,
    user_uuid: Option<String>,
    entries: Vec<AppApiProfileLeaseResponse>,
}

impl AppApiProfileCache {
    fn for_session(session: &AppApiTokenResponse) -> Self {
        Self {
            app_version: env!("CARGO_PKG_VERSION").into(),
            device_id: session.device_id.clone(),
            user_uuid: session.subscription.user_uuid.clone(),
            entries: Vec::new(),
        }
    }

    fn scope_matches(&self, session: &AppApiTokenResponse) -> bool {
        self.app_version == env!("CARGO_PKG_VERSION")
            && self.device_id == session.device_id
            && self.user_uuid == session.subscription.user_uuid
    }

    fn profile(&self, location_id: &str, now: i64) -> Option<AppApiProfileLeaseResponse> {
        let location_id = location_id.trim().to_ascii_lowercase();
        self.entries
            .iter()
            .find(|entry| {
                entry.location_id.trim().eq_ignore_ascii_case(&location_id)
                    && app_api_profile_lease_is_fresh(entry, now)
            })
            .cloned()
    }

    fn location_ids(&self, now: i64) -> Vec<String> {
        self.entries
            .iter()
            .filter(|entry| app_api_profile_lease_is_fresh(entry, now))
            .map(|entry| entry.location_id.trim().to_ascii_lowercase())
            .collect()
    }

    fn insert(
        &mut self,
        session: &AppApiTokenResponse,
        lease: AppApiProfileLeaseResponse,
        now: i64,
    ) {
        if !app_api_profile_lease_is_fresh(&lease, now) {
            return;
        }
        if !self.scope_matches(session) {
            *self = Self::for_session(session);
        }
        self.entries.retain(|entry| {
            app_api_profile_lease_is_fresh(entry, now)
                && !entry
                    .location_id
                    .trim()
                    .eq_ignore_ascii_case(lease.location_id.trim())
        });
        self.entries.insert(0, lease);
        self.entries.truncate(APP_API_PROFILE_CACHE_MAX_ENTRIES);
    }
}

pub(super) fn app_api_profile_lease_is_fresh(lease: &AppApiProfileLeaseResponse, now: i64) -> bool {
    DateTime::parse_from_rfc3339(lease.expires_at.trim())
        .map(|expiry| expiry.timestamp() > now)
        .unwrap_or(false)
}

#[derive(Debug, Deserialize)]
pub struct AppConnectLocationRequest {
    pub location_id: String,
    #[serde(default)]
    pub fallback_location_ids: Vec<String>,
    #[serde(default = "default_app_proxy_mode")]
    pub proxy_mode: String,
    #[serde(default = "default_system_proxy_mode")]
    pub system_proxy_mode: String,
    #[serde(default = "default_app_socks_port")]
    pub socks_port: u16,
    #[serde(default = "default_app_http_port")]
    pub http_port: u16,
    #[serde(default = "default_app_network_stack")]
    pub network_stack: String,
    #[serde(default = "default_app_dns_mode")]
    pub dns_mode: String,
    #[serde(default = "default_app_strict_route")]
    pub strict_route: bool,
    #[serde(default)]
    pub kill_switch: bool,
    #[serde(default)]
    pub routing_rules: Vec<RoutingRuleRequest>,
}

pub(super) fn app_connection_location_ids(request: &AppConnectLocationRequest) -> Vec<String> {
    let mut ids = Vec::new();
    for location_id in std::iter::once(&request.location_id).chain(&request.fallback_location_ids) {
        let location_id = location_id.trim().to_ascii_lowercase();
        if !location_id.is_empty() && !ids.contains(&location_id) {
            ids.push(location_id);
        }
        if ids.len() == 3 {
            break;
        }
    }
    ids
}

#[derive(Debug)]
pub(super) struct AppApiHttpError {
    pub(super) status: u16,
    pub(super) message: String,
}

impl std::fmt::Display for AppApiHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "App API error {}: {}", self.status, self.message)
    }
}

pub(super) fn app_api_error_message(status: reqwest::StatusCode, text: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        for key in ["error", "message"] {
            if let Some(message) = value.get(key).and_then(serde_json::Value::as_str) {
                if !message.trim().is_empty() {
                    return message.trim().chars().take(500).collect();
                }
            }
        }
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        status.to_string()
    } else if trimmed.starts_with('<') || trimmed.to_ascii_lowercase().contains("<html") {
        "DoodleVPN API returned an incompatible response. Update the app and try again.".into()
    } else {
        trimmed.chars().take(500).collect()
    }
}

fn default_app_proxy_mode() -> String {
    "tun".into()
}

fn default_app_socks_port() -> u16 {
    10808
}

fn default_app_http_port() -> u16 {
    10809
}

fn default_app_network_stack() -> String {
    "system".into()
}

fn default_app_dns_mode() -> String {
    "fakeip".into()
}

fn default_app_strict_route() -> bool {
    true
}

fn closed_control_plane_enabled() -> bool {
    option_env!("DOODLERAY_CLOSED_CONTROL_PLANE") == Some("1")
}

fn ensure_closed_control_plane_enabled() -> Result<(), String> {
    if closed_control_plane_enabled() {
        Ok(())
    } else {
        Err("DoodleVPN account sign-in is not enabled in this build.".into())
    }
}

fn app_api_base_url() -> String {
    option_env!("DOODLERAY_APP_API_BASE_URL")
        .unwrap_or(APP_API_DEFAULT_BASE_URL)
        .trim_end_matches('/')
        .to_string()
}

pub(super) fn app_api_endpoint(path: &str) -> Result<Url, String> {
    let base = format!("{}/", app_api_base_url());
    let mut base_url = Url::parse(&base).map_err(|e| format!("Invalid App API base URL: {}", e))?;
    let path = path.trim_start_matches('/');
    if path.starts_with("v1/mobile/") {
        base_url.set_path(&format!("/{path}"));
        return Ok(base_url);
    }
    base_url
        .join(path)
        .map_err(|e| format!("Invalid App API endpoint path: {}", e))
}

fn app_api_http_client() -> Result<reqwest::Client, String> {
    let builder = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(20));
    #[cfg(all(target_os = "macos", feature = "app-store"))]
    let builder = builder.connect_timeout(Duration::from_secs(5));
    builder
        .build()
        .map_err(|e| format!("App API client init failed: {}", e))
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
fn app_api_fallback_http_client(url: &Url) -> Result<Option<reqwest::Client>, String> {
    const HOST: &str = "ddlvpn.lol";
    if url.host_str() != Some(HOST) {
        return Ok(None);
    }
    // Keep the hostname for TLS/SNI while moving only the blocked network path
    // to the independently hosted production origin.
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .resolve(
            HOST,
            SocketAddr::from((Ipv4Addr::new(87, 120, 166, 237), 443)),
        )
        .build()
        .map(Some)
        .map_err(|e| format!("App API fallback client init failed: {}", e))
}

#[cfg(all(test, target_os = "macos", feature = "app-store"))]
mod app_store_api_fallback_tests {
    use super::*;

    #[test]
    fn fallback_is_limited_to_the_production_api_hostname() {
        let production = app_api_endpoint("/healthz").expect("production URL");
        let unrelated = Url::parse("https://example.com/healthz").expect("unrelated URL");

        assert!(app_api_fallback_http_client(&production)
            .expect("fallback client")
            .is_some());
        assert!(app_api_fallback_http_client(&unrelated)
            .expect("no fallback client")
            .is_none());
    }
}

fn app_api_stored_session(session: &AppApiTokenResponse) -> AppApiStoredSession {
    AppApiStoredSession {
        refresh_token: session.refresh_token.clone(),
        refresh_expires_at: session.refresh_expires_at.clone(),
        device_id: session.device_id.clone(),
        subscription: session.subscription.clone(),
    }
}

fn app_api_session_from_stored(stored: AppApiStoredSession) -> AppApiTokenResponse {
    AppApiTokenResponse {
        access_token: String::new(),
        access_expires_at: String::new(),
        expires_in: 0,
        refresh_token: stored.refresh_token,
        refresh_expires_at: stored.refresh_expires_at,
        device_id: stored.device_id,
        subscription: stored.subscription,
    }
}

pub(super) fn app_api_encode_session_for_disk(
    session: &AppApiTokenResponse,
) -> Result<String, String> {
    serde_json::to_string(&app_api_stored_session(session))
        .map_err(|e| format!("App API session serialize failed: {}", e))
}

pub(super) fn app_api_decode_session_from_disk(
    encoded: &str,
) -> Result<AppApiTokenResponse, String> {
    if let Ok(stored) = serde_json::from_str::<AppApiStoredSession>(encoded) {
        return Ok(app_api_session_from_stored(stored));
    }

    // One-way migration for early v6 RCs that persisted the whole token
    // response. The loaded access token stays in memory for this process only;
    // app_api_load_session rewrites the disk entry without it.
    serde_json::from_str::<AppApiTokenResponse>(encoded)
        .map_err(|e| format!("Stored App API session is invalid: {}", e))
}

pub(super) fn app_api_decode_session_storage_value(
    encoded: &str,
) -> Result<Option<AppApiTokenResponse>, String> {
    if encoded == APP_API_SESSION_TOMBSTONE {
        return Ok(None);
    }
    app_api_decode_session_from_disk(encoded).map(Some)
}

fn app_api_store_session(session: &AppApiTokenResponse) -> Result<(), String> {
    let encoded = app_api_encode_session_for_disk(session)?;
    app_api_native_secret_set(APP_API_SESSION_KEY, &encoded)?;
    if let Ok(mut memory) = APP_API_MEMORY_SESSION.lock() {
        *memory = Some(session.clone());
    }
    Ok(())
}

fn app_api_load_profile_cache(session: &AppApiTokenResponse) -> Result<AppApiProfileCache, String> {
    let Some(encoded) = app_api_native_secret_get(APP_API_PROFILE_CACHE_KEY)? else {
        return Ok(AppApiProfileCache::for_session(session));
    };
    let cache: AppApiProfileCache = serde_json::from_str(&encoded)
        .map_err(|error| format!("Stored App API profile cache is invalid: {error}"))?;
    Ok(if cache.scope_matches(session) {
        cache
    } else {
        AppApiProfileCache::for_session(session)
    })
}

fn app_api_cached_profile(
    session: &AppApiTokenResponse,
    location_id: &str,
) -> Result<Option<AppApiProfileLeaseResponse>, String> {
    Ok(app_api_load_profile_cache(session)?.profile(location_id, Utc::now().timestamp()))
}

fn app_api_cached_profile_location_ids(
    session: &AppApiTokenResponse,
) -> Result<Vec<String>, String> {
    Ok(app_api_load_profile_cache(session)?.location_ids(Utc::now().timestamp()))
}

fn app_api_store_cached_profile(
    session: &AppApiTokenResponse,
    lease: &AppApiProfileLeaseResponse,
) -> Result<(), String> {
    let mut cache = app_api_load_profile_cache(session)?;
    cache.insert(session, lease.clone(), Utc::now().timestamp());
    let encoded = serde_json::to_string(&cache)
        .map_err(|error| format!("App API profile cache serialize failed: {error}"))?;
    app_api_native_secret_set(APP_API_PROFILE_CACHE_KEY, &encoded)
}

fn app_api_delete_profile_cache() -> Result<(), String> {
    app_api_native_secret_delete(APP_API_PROFILE_CACHE_KEY)
}

fn app_api_delete_session() -> Result<(), String> {
    if let Ok(mut memory) = APP_API_MEMORY_SESSION.lock() {
        *memory = None;
    }
    let session_result = app_api_native_secret_delete(APP_API_SESSION_KEY);
    let cache_result = app_api_delete_profile_cache();
    match (session_result, cache_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(session_error), Err(cache_error)) => Err(format!("{session_error}; {cache_error}")),
    }
}

fn app_api_invalidate_session_storage(
    delete: impl FnOnce() -> Result<(), String>,
    write_tombstone: impl FnOnce(&str) -> Result<(), String>,
) -> Result<(), String> {
    match delete() {
        Ok(()) => Ok(()),
        Err(delete_error) => write_tombstone(APP_API_SESSION_TOMBSTONE)
            .map_err(|tombstone_error| format!("{delete_error}; {tombstone_error}")),
    }
}

pub(super) fn app_api_invalidate_windows_session_storage(
    keyring_delete: impl FnOnce() -> Result<(), String>,
    keyring_tombstone: impl FnOnce(&str) -> Result<(), String>,
    dpapi_delete: impl FnOnce() -> Result<(), String>,
    dpapi_tombstone: impl FnOnce(&str) -> Result<(), String>,
) -> Result<(), String> {
    let results = [
        (
            "Credential Manager",
            app_api_invalidate_session_storage(keyring_delete, keyring_tombstone),
        ),
        (
            "DPAPI",
            app_api_invalidate_session_storage(dpapi_delete, dpapi_tombstone),
        ),
    ];
    let errors = results
        .into_iter()
        .filter_map(|(backend, result)| result.err().map(|error| format!("{backend}: {error}")))
        .collect::<Vec<_>>();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

#[cfg(windows)]
fn app_api_windows_keyring_get(key: &str) -> Result<Option<String>, String> {
    match secure_store_entry(key)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("Secure storage read failed: {error}")),
    }
}

#[cfg(windows)]
fn app_api_windows_keyring_delete(key: &str) -> Result<(), String> {
    match secure_store_entry(key)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(format!("Secure storage delete failed: {error}")),
    }
}

#[cfg(windows)]
fn app_api_windows_keyring_delete_session() -> Result<(), String> {
    if let Ok(Some(value)) = app_api_windows_keyring_get(APP_API_SESSION_KEY) {
        if let Some(count) = secure_store_chunk_count(&value) {
            for index in 0..count {
                let _ = app_api_windows_keyring_delete(&secure_store_chunk_key(
                    APP_API_SESSION_KEY,
                    index,
                ));
            }
        }
    }
    app_api_windows_keyring_delete(APP_API_SESSION_KEY)
}

#[cfg(windows)]
fn app_api_windows_keyring_tombstone_session(value: &str) -> Result<(), String> {
    if let Ok(Some(current)) = app_api_windows_keyring_get(APP_API_SESSION_KEY) {
        if let Some(count) = secure_store_chunk_count(&current) {
            for index in 0..count {
                let _ = app_api_windows_keyring_delete(&secure_store_chunk_key(
                    APP_API_SESSION_KEY,
                    index,
                ));
            }
        }
    }
    secure_store_entry(APP_API_SESSION_KEY)?
        .set_password(value)
        .map_err(|error| format!("Secure storage write failed: {error}"))
}

fn app_api_invalidate_session() -> Result<(), String> {
    if let Ok(mut memory) = APP_API_MEMORY_SESSION.lock() {
        *memory = None;
    }
    #[cfg(windows)]
    let result = app_api_invalidate_windows_session_storage(
        app_api_windows_keyring_delete_session,
        app_api_windows_keyring_tombstone_session,
        || app_api_dpapi_delete(APP_API_SESSION_KEY),
        |value| app_api_dpapi_set(APP_API_SESSION_KEY, value),
    );
    #[cfg(not(windows))]
    let result = app_api_invalidate_session_storage(
        || secure_store_native_delete(APP_API_SESSION_KEY),
        |value| secure_store_native_set(APP_API_SESSION_KEY, value),
    );
    let _ = app_api_delete_profile_cache();
    result
}

fn app_api_load_session() -> Result<Option<AppApiTokenResponse>, String> {
    if let Ok(memory) = APP_API_MEMORY_SESSION.lock() {
        if let Some(session) = memory.clone() {
            return Ok(Some(session));
        }
    }

    let Some(encoded) = app_api_native_secret_get(APP_API_SESSION_KEY)? else {
        return Ok(None);
    };
    let Some(session) = app_api_decode_session_storage_value(&encoded)? else {
        return Ok(None);
    };
    if !session.access_token.is_empty() {
        app_api_native_secret_set(
            APP_API_SESSION_KEY,
            &app_api_encode_session_for_disk(&session)?,
        )?;
    }
    if let Ok(mut memory) = APP_API_MEMORY_SESSION.lock() {
        *memory = Some(session.clone());
    }
    Ok(Some(session))
}

fn app_api_public_session(session: Option<AppApiTokenResponse>) -> AppApiSessionStatus {
    let access_expires_at = session.as_ref().and_then(|s| {
        (!s.access_expires_at.trim().is_empty()).then(|| s.access_expires_at.clone())
    });
    AppApiSessionStatus {
        logged_in: session.is_some(),
        device_id: session.as_ref().map(|s| s.device_id.clone()),
        access_expires_at,
        refresh_expires_at: session.as_ref().map(|s| s.refresh_expires_at.clone()),
        subscription: session.as_ref().map(|s| s.subscription.clone()),
        api_base_url: app_api_base_url(),
        closed_control_plane_enabled: closed_control_plane_enabled(),
    }
}

fn app_api_ed25519_jwk_from_seed(seed: &[u8; 32]) -> (String, serde_json::Value) {
    let signing_key = SigningKey::from_bytes(seed);
    let public_key = signing_key.verifying_key().to_bytes();
    let encoded_public = URL_SAFE_NO_PAD.encode(public_key);
    let jwk = serde_json::json!({
        "kty": "OKP",
        "crv": "Ed25519",
        "x": encoded_public,
    });
    (encoded_public, jwk)
}

pub(super) fn app_api_hwid_from_machine_seed(seed: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"doodleray-hwid-v1\n");
    hasher.update(seed.trim().to_ascii_lowercase().as_bytes());
    let digest = hasher.finalize();
    let prefix = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("pc-hwid-{prefix}")
}

#[cfg(windows)]
fn app_api_windows_machine_seed() -> Option<String> {
    use winreg::enums::{KEY_READ, KEY_WOW64_64KEY};

    let key = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(
            "SOFTWARE\\Microsoft\\Cryptography",
            KEY_READ | KEY_WOW64_64KEY,
        )
        .ok()?;
    let seed: String = key.get_value("MachineGuid").ok()?;
    let seed = seed.trim().to_string();
    (!seed.is_empty()).then_some(seed)
}

fn app_api_hwid_for_new_device() -> String {
    #[cfg(windows)]
    if let Some(seed) = app_api_windows_machine_seed() {
        return app_api_hwid_from_machine_seed(&seed);
    }

    format!("pc-hwid-{}", uuid::Uuid::new_v4())
}

pub(super) fn app_api_generate_device_state() -> Result<AppApiDeviceState, String> {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed)
        .map_err(|e| format!("App API device key generation failed: {}", e))?;
    let (public_key, public_key_jwk) = app_api_ed25519_jwk_from_seed(&seed);
    Ok(AppApiDeviceState {
        client_device_id: format!("pc-{}", uuid::Uuid::new_v4()),
        hwid: app_api_hwid_for_new_device(),
        public_key,
        public_key_jwk,
        private_key_seed: URL_SAFE_NO_PAD.encode(seed),
        key_alg: "Ed25519".into(),
    })
}

fn app_api_device_state_is_usable(device: &AppApiDeviceState) -> bool {
    !device.client_device_id.trim().is_empty()
        && !device.hwid.trim().is_empty()
        && device.key_alg == "Ed25519"
        && !device.public_key.trim().is_empty()
        && device.public_key_jwk.get("kty").and_then(|v| v.as_str()) == Some("OKP")
        && device.public_key_jwk.get("crv").and_then(|v| v.as_str()) == Some("Ed25519")
        && !device.private_key_seed.trim().is_empty()
}

fn app_api_signing_key(device: &AppApiDeviceState) -> Result<SigningKey, String> {
    let decoded = URL_SAFE_NO_PAD
        .decode(device.private_key_seed.trim())
        .map_err(|e| format!("Stored App API device key is invalid: {}", e))?;
    let seed: [u8; 32] = decoded
        .try_into()
        .map_err(|_| "Stored App API device key has invalid length".to_string())?;
    Ok(SigningKey::from_bytes(&seed))
}

pub(super) fn app_api_body_sha256(body: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body.unwrap_or("").as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

pub(super) fn app_api_device_proof(
    device: &AppApiDeviceState,
    method: &reqwest::Method,
    path: &str,
    body: Option<&str>,
) -> Result<String, String> {
    let signing_key = app_api_signing_key(device)?;
    let iat = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("System clock is not valid for App API proof: {}", e))?
        .as_secs();
    let jti = uuid::Uuid::new_v4().to_string();
    let normalized_path = format!("/{}", path.trim_start_matches('/'));
    let body_sha256 = app_api_body_sha256(body);
    let signing_input = format!(
        "DoodleVPN-PC-Proof-v1\n{}\n{}\n{}\n{}\n{}\n{}",
        method.as_str(),
        normalized_path,
        body_sha256,
        iat,
        jti,
        device.client_device_id
    );
    let signature = signing_key.sign(signing_input.as_bytes());
    let proof = serde_json::json!({
        "typ": "doodlevpn-device-proof-v1",
        "alg": "EdDSA",
        "device_id": device.client_device_id,
        "public_key_jwk": device.public_key_jwk,
        "htm": method.as_str(),
        "htu": normalized_path,
        "iat": iat,
        "jti": jti,
        "body_sha256": body_sha256,
        "sig": URL_SAFE_NO_PAD.encode(signature.to_bytes()),
    });
    Ok(URL_SAFE_NO_PAD.encode(proof.to_string()))
}

fn app_api_load_or_create_device() -> Result<AppApiDeviceState, String> {
    if let Some(encoded) = app_api_native_secret_get(APP_API_DEVICE_KEY)? {
        if let Ok(device) = serde_json::from_str::<AppApiDeviceState>(&encoded) {
            if app_api_device_state_is_usable(&device) {
                return Ok(device);
            }
        }
    }

    // Keep the private key below the React/Tauri renderer boundary. Windows
    // persists it with current-user DPAPI when Credential Manager is unavailable.
    let device = app_api_generate_device_state()?;
    let encoded = serde_json::to_string(&device)
        .map_err(|e| format!("App API device serialize failed: {}", e))?;
    app_api_native_secret_set(APP_API_DEVICE_KEY, &encoded)?;
    Ok(device)
}

fn app_api_build_request(
    client: &reqwest::Client,
    method: &reqwest::Method,
    url: &Url,
    bearer: Option<&str>,
    body_text: &Option<String>,
    device_headers: &Option<(String, String)>,
) -> reqwest::RequestBuilder {
    let mut request = client
        .request(method.clone(), url.clone())
        .header("Accept", "application/json")
        .header(
            "User-Agent",
            format!("DoodleRayPC/{}", env!("CARGO_PKG_VERSION")),
        );
    if let Some((device_id, proof)) = device_headers {
        request = request
            .header("X-Doodle-Device-ID", device_id)
            .header("X-Doodle-Device-Proof", proof);
    }
    if let Some(token) = bearer {
        request = request.bearer_auth(token);
    }
    if let Some(body) = body_text {
        request = request
            .header("Content-Type", "application/json")
            .body(body.clone());
    }
    request
}

async fn app_api_finish_response<T: DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, AppApiHttpError> {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(AppApiHttpError {
            status: status.as_u16(),
            message: app_api_error_message(status, &text),
        });
    }
    serde_json::from_str::<T>(&text).map_err(|e| AppApiHttpError {
        status: status.as_u16(),
        message: format!("App API JSON parse failed: {}", e),
    })
}

async fn app_api_send_json<T: DeserializeOwned>(
    method: reqwest::Method,
    path: &str,
    bearer: Option<&str>,
    body: Option<serde_json::Value>,
) -> Result<T, AppApiHttpError> {
    let url = app_api_endpoint(path).map_err(|message| AppApiHttpError { status: 0, message })?;
    let body_text = body.as_ref().map(|body| body.to_string());
    let device_headers = if closed_control_plane_enabled() {
        let device = app_api_load_or_create_device()
            .map_err(|message| AppApiHttpError { status: 0, message })?;
        let proof = app_api_device_proof(&device, &method, path, body_text.as_deref())
            .map_err(|message| AppApiHttpError { status: 0, message })?;
        Some((device.client_device_id, proof))
    } else {
        None
    };

    let direct_client =
        app_api_http_client().map_err(|message| AppApiHttpError { status: 0, message })?;
    let direct_request = app_api_build_request(
        &direct_client,
        &method,
        &url,
        bearer,
        &body_text,
        &device_headers,
    );
    let direct_error = match direct_request.send().await {
        Ok(response) => return app_api_finish_response(response).await,
        Err(e) => e.to_string(),
    };

    #[cfg(all(target_os = "macos", feature = "app-store"))]
    if let Ok(Some(fallback_client)) = app_api_fallback_http_client(&url) {
        let fallback_request = app_api_build_request(
            &fallback_client,
            &method,
            &url,
            bearer,
            &body_text,
            &device_headers,
        );
        if let Ok(response) = fallback_request.send().await {
            return app_api_finish_response(response).await;
        }
    }

    // Some Windows hosts only reach the internet through a configured system
    // proxy; .no_proxy() above is intentional (avoids looping through our own
    // VPN tunnel) but must not be the only path. Retry once through whatever
    // manual proxy Windows/macOS currently has configured, same fallback
    // already used for subscription fetches.
    if let Ok(Some(proxy_client)) = system_proxy_fetch_client(&url, Duration::from_secs(20)) {
        let proxy_request = app_api_build_request(
            &proxy_client,
            &method,
            &url,
            bearer,
            &body_text,
            &device_headers,
        );
        if let Ok(response) = proxy_request.send().await {
            return app_api_finish_response(response).await;
        }
    }

    Err(AppApiHttpError {
        status: 0,
        message: direct_error,
    })
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
async fn app_api_send_bytes(path: &str, bearer: &str) -> Result<Vec<u8>, AppApiHttpError> {
    let url = app_api_endpoint(path).map_err(|message| AppApiHttpError { status: 0, message })?;
    let method = reqwest::Method::GET;
    let device_headers = if closed_control_plane_enabled() {
        let device = app_api_load_or_create_device()
            .map_err(|message| AppApiHttpError { status: 0, message })?;
        let proof = app_api_device_proof(&device, &method, path, None)
            .map_err(|message| AppApiHttpError { status: 0, message })?;
        Some((device.client_device_id, proof))
    } else {
        None
    };
    let build_request = |client: &reqwest::Client| {
        let mut request = client
            .get(url.clone())
            .header("Accept", "application/octet-stream")
            .header(
                "User-Agent",
                format!("DoodleRayPC/{}", env!("CARGO_PKG_VERSION")),
            )
            .bearer_auth(bearer);
        if let Some((device_id, proof)) = &device_headers {
            request = request
                .header("X-Doodle-Device-ID", device_id)
                .header("X-Doodle-Device-Proof", proof);
        }
        request
    };
    let client = app_api_http_client().map_err(|message| AppApiHttpError { status: 0, message })?;
    let direct_error = match build_request(&client).send().await {
        Ok(response) => return app_api_finish_bytes_response(response).await,
        Err(error) => error.to_string(),
    };
    if let Ok(Some(fallback_client)) = app_api_fallback_http_client(&url) {
        if let Ok(response) = build_request(&fallback_client).send().await {
            return app_api_finish_bytes_response(response).await;
        }
    }
    if let Ok(Some(proxy_client)) = system_proxy_fetch_client(&url, Duration::from_secs(20)) {
        if let Ok(response) = build_request(&proxy_client).send().await {
            return app_api_finish_bytes_response(response).await;
        }
    }
    Err(AppApiHttpError {
        status: 0,
        message: direct_error,
    })
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
async fn app_api_finish_bytes_response(
    response: reqwest::Response,
) -> Result<Vec<u8>, AppApiHttpError> {
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(AppApiHttpError {
            status: status.as_u16(),
            message: app_api_error_message(status, &text),
        });
    }
    if response
        .content_length()
        .is_some_and(|size| size > 64 * 1024 * 1024)
    {
        return Err(AppApiHttpError {
            status: status.as_u16(),
            message: "routing asset is too large".into(),
        });
    }
    let bytes = response.bytes().await.map_err(|error| AppApiHttpError {
        status: status.as_u16(),
        message: error.to_string(),
    })?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(AppApiHttpError {
            status: status.as_u16(),
            message: "routing asset is too large".into(),
        });
    }
    Ok(bytes.to_vec())
}

pub(super) fn app_api_refresh_error_is_fatal(status: u16) -> bool {
    matches!(status, 401 | 403)
}

pub(super) fn app_api_refresh_error_after_invalidation(
    error: &AppApiHttpError,
    invalidation: Result<(), String>,
) -> String {
    if let Err(cleanup_error) = invalidation {
        eprintln!(
            "[warn] App API session invalidation failed: {}",
            redact_support_line(&cleanup_error)
        );
    }
    error.to_string()
}

async fn app_api_refresh_session() -> Result<AppApiTokenResponse, String> {
    let Some(session) = app_api_load_session()? else {
        return Err("DoodleVPN sign-in is required.".into());
    };
    let body = serde_json::json!({
        "refresh_token": session.refresh_token,
        "device_id": session.device_id,
    });
    let refreshed = match app_api_send_json::<AppApiTokenResponse>(
        reqwest::Method::POST,
        "/auth/refresh",
        None,
        Some(body),
    )
    .await
    {
        Ok(refreshed) => refreshed,
        Err(error) => {
            if app_api_refresh_error_is_fatal(error.status) {
                return Err(app_api_refresh_error_after_invalidation(
                    &error,
                    app_api_invalidate_session(),
                ));
            }
            return Err(error.to_string());
        }
    };
    app_api_store_session(&refreshed)?;
    Ok(refreshed)
}

async fn app_api_authorized_json<T: DeserializeOwned>(
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<T, String> {
    app_api_authorized_json_http(method, path, body)
        .await
        .map_err(|error| error.to_string())
}

async fn app_api_authorized_json_http<T: DeserializeOwned>(
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<T, AppApiHttpError> {
    let Some(session) =
        app_api_load_session().map_err(|message| AppApiHttpError { status: 0, message })?
    else {
        return Err(AppApiHttpError {
            status: 401,
            message: "DoodleVPN sign-in is required.".into(),
        });
    };
    if session.access_token.trim().is_empty() {
        let refreshed = app_api_refresh_session()
            .await
            .map_err(|message| AppApiHttpError { status: 0, message })?;
        return app_api_send_json::<T>(method, path, Some(&refreshed.access_token), body).await;
    }
    match app_api_send_json::<T>(
        method.clone(),
        path,
        Some(&session.access_token),
        body.clone(),
    )
    .await
    {
        Ok(value) => Ok(value),
        Err(err) if err.status == 401 => {
            let refreshed = app_api_refresh_session()
                .await
                .map_err(|message| AppApiHttpError { status: 0, message })?;
            app_api_send_json::<T>(method, path, Some(&refreshed.access_token), body).await
        }
        Err(err) => Err(err),
    }
}

#[cfg(all(target_os = "macos", feature = "app-store"))]
pub(super) async fn app_api_authorized_bytes(path: &str) -> Result<Vec<u8>, String> {
    let Some(session) = app_api_load_session()? else {
        return Err("DoodleVPN sign-in is required.".into());
    };
    let session = if session.access_token.trim().is_empty() {
        app_api_refresh_session().await?
    } else {
        session
    };
    match app_api_send_bytes(path, &session.access_token).await {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.status == 401 => {
            let refreshed = app_api_refresh_session().await?;
            app_api_send_bytes(path, &refreshed.access_token)
                .await
                .map_err(|error| error.to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

fn app_api_diagnostic_key_is_sensitive(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "token",
        "password",
        "secret",
        "private",
        "credential",
        "uuid",
        "address",
        "endpoint",
        "profile",
        "public_key",
        "short_id",
        "subscription",
        "packet",
        "dns_query",
        "destination_ip",
        "config",
        "domain",
        "host",
        "url",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn app_api_redact_diagnostic_text(value: &str) -> String {
    value
        .chars()
        .take(4_000)
        .collect::<String>()
        .split_whitespace()
        .map(|word| {
            let trimmed = word.trim_matches(|c: char| {
                !c.is_ascii_alphanumeric() && c != '.' && c != ':' && c != '-'
            });
            if trimmed.contains("://") {
                return "[url]".to_string();
            }
            if trimmed.parse::<IpAddr>().is_ok() {
                return "[ip]".to_string();
            }
            let uuid_like = trimmed.len() == 36
                && trimmed
                    .as_bytes()
                    .iter()
                    .filter(|byte| **byte == b'-')
                    .count()
                    == 4;
            if uuid_like {
                return "[id]".to_string();
            }
            let looks_like_domain = trimmed.rsplit_once('.').is_some_and(|(_, suffix)| {
                suffix.len() >= 2 && suffix.chars().all(|c| c.is_ascii_alphabetic())
            });
            if looks_like_domain {
                return "[domain]".to_string();
            }
            word.to_string()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn app_api_sanitize_diagnostic_value(
    value: serde_json::Value,
    depth: usize,
) -> serde_json::Value {
    if depth > 5 {
        return serde_json::Value::String("[truncated]".into());
    }
    match value {
        serde_json::Value::String(value) => {
            serde_json::Value::String(app_api_redact_diagnostic_text(&value))
        }
        serde_json::Value::Array(values) => serde_json::Value::Array(
            values
                .into_iter()
                .take(50)
                .map(|value| app_api_sanitize_diagnostic_value(value, depth + 1))
                .collect(),
        ),
        serde_json::Value::Object(values) => serde_json::Value::Object(
            values
                .into_iter()
                .filter(|(key, _)| !app_api_diagnostic_key_is_sensitive(key))
                .take(50)
                .map(|(key, value)| (key, app_api_sanitize_diagnostic_value(value, depth + 1)))
                .collect(),
        ),
        other => other,
    }
}

pub(super) fn app_api_profile_to_connect_request(
    profile: &serde_json::Value,
    request: &AppConnectLocationRequest,
) -> Result<ConnectRequest, String> {
    let profile_type = profile
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if profile_type == "xray" {
        return app_api_xray_profile_to_connect_request(profile, request);
    }
    if profile_type == "hysteria2" {
        return app_api_hysteria2_profile_to_connect_request(profile, request);
    }
    let security = profile
        .get("security")
        .and_then(|v| v.as_str())
        .unwrap_or("reality");
    if profile_type != "vless" || security != "reality" {
        return Err(format!(
            "Unsupported DoodleVPN profile type for PC runtime: type={} security={}",
            profile_type, security
        ));
    }

    let address = profile
        .get("connect_address")
        .or_else(|| profile.get("address"))
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| "DoodleVPN profile is missing connect address".to_string())?
        .to_string();
    let port = profile
        .get("port")
        .and_then(|v| v.as_u64())
        .filter(|port| *port > 0 && *port <= u16::MAX as u64)
        .unwrap_or(443) as u16;
    let uuid = profile
        .get("uuid")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| "DoodleVPN profile is missing user id".to_string())?
        .to_string();

    let transport = match profile.get("transport").and_then(|v| v.as_str()) {
        Some("reality_tcp") | Some("tcp") | None => "tcp".to_string(),
        Some(other) => other.to_string(),
    };

    Ok(ConnectRequest {
        server_address: address,
        server_port: port,
        protocol: "vless".into(),
        uuid: Some(uuid),
        password: None,
        transport,
        security: "reality".into(),
        sni: profile
            .get("server_name")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
        host: None,
        path: None,
        fingerprint: profile
            .get("fingerprint")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
        public_key: profile
            .get("public_key")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
        short_id: profile
            .get("short_id")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
        flow: profile
            .get("flow")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
        proxy_mode: request.proxy_mode.clone(),
        system_proxy_mode: request.system_proxy_mode.clone(),
        socks_port: request.socks_port,
        http_port: request.http_port,
        api_port: default_xray_api_port(),
        network_stack: request.network_stack.clone(),
        dns_mode: request.dns_mode.clone(),
        strict_route: request.strict_route,
        kill_switch: request.kill_switch,
        routing_rules: request.routing_rules.clone(),
        obfs_type: None,
        obfs_password: None,
        up_mbps: None,
        down_mbps: None,
        congestion_control: None,
        udp_relay_mode: None,
        alpn: None,
        private_key: None,
        peer_public_key: None,
        pre_shared_key: None,
        local_address: None,
        reserved: None,
        mtu: None,
        workers: None,
        encryption: None,
        raw_xray_config: None,
        routing_policy: None,
    })
}

fn app_api_hysteria2_profile_to_connect_request(
    profile: &serde_json::Value,
    request: &AppConnectLocationRequest,
) -> Result<ConnectRequest, String> {
    let security = profile
        .get("security")
        .and_then(|value| value.as_str())
        .unwrap_or("tls");
    if security != "tls" {
        return Err("DoodleVPN Hysteria2 profile must use TLS".into());
    }
    let address = profile
        .get("connect_address")
        .or_else(|| profile.get("address"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "DoodleVPN Hysteria2 profile is missing connect address".to_string())?
        .to_string();
    let port = profile
        .get("port")
        .and_then(|value| value.as_u64())
        .filter(|port| *port > 0 && *port <= u16::MAX as u64)
        .ok_or_else(|| "DoodleVPN Hysteria2 profile has an invalid port".to_string())?
        as u16;
    let password = profile
        .get("password")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty() && value.len() <= 4096)
        .ok_or_else(|| "DoodleVPN Hysteria2 profile is missing authentication".to_string())?
        .to_string();
    let server_name = profile
        .get("server_name")
        .or_else(|| profile.get("sni"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty() && value.len() <= 255)
        .ok_or_else(|| "DoodleVPN Hysteria2 profile is missing TLS server name".to_string())?
        .to_string();
    let obfs_type = profile
        .get("obfs_type")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    if obfs_type
        .as_deref()
        .is_some_and(|value| value != "salamander")
    {
        return Err("DoodleVPN Hysteria2 profile has unsupported obfuscation".into());
    }
    let obfs_password = profile
        .get("obfs_password")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    if obfs_type.is_some() && obfs_password.is_none() {
        return Err("DoodleVPN Hysteria2 profile is missing obfuscation authentication".into());
    }
    let alpn = match profile.get("alpn") {
        None => None,
        Some(value) => {
            let values = value
                .as_array()
                .filter(|values| !values.is_empty() && values.len() <= 8)
                .ok_or_else(|| "DoodleVPN Hysteria2 profile has invalid ALPN".to_string())?;
            let mut parsed = Vec::with_capacity(values.len());
            for value in values {
                let value = value
                    .as_str()
                    .filter(|value| !value.trim().is_empty() && value.len() <= 255)
                    .ok_or_else(|| "DoodleVPN Hysteria2 profile has invalid ALPN".to_string())?;
                parsed.push(value.to_string());
            }
            Some(parsed)
        }
    };
    let bandwidth = |name: &str| -> Result<Option<u32>, String> {
        match profile.get(name) {
            None => Ok(None),
            Some(value) => value
                .as_u64()
                .filter(|value| *value > 0 && *value <= 100_000)
                .map(|value| Some(value as u32))
                .ok_or_else(|| format!("DoodleVPN Hysteria2 profile has invalid {name}")),
        }
    };

    Ok(ConnectRequest {
        server_address: address,
        server_port: port,
        protocol: "hysteria2".into(),
        uuid: None,
        password: Some(password),
        transport: "udp".into(),
        security: "tls".into(),
        sni: Some(server_name),
        host: None,
        path: None,
        fingerprint: profile
            .get("fingerprint")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty() && value.len() <= 128)
            .map(str::to_string),
        public_key: None,
        short_id: None,
        flow: None,
        proxy_mode: request.proxy_mode.clone(),
        system_proxy_mode: request.system_proxy_mode.clone(),
        socks_port: request.socks_port,
        http_port: request.http_port,
        api_port: default_xray_api_port(),
        network_stack: request.network_stack.clone(),
        dns_mode: request.dns_mode.clone(),
        strict_route: request.strict_route,
        kill_switch: request.kill_switch,
        routing_rules: request.routing_rules.clone(),
        obfs_type,
        obfs_password,
        up_mbps: bandwidth("up_mbps")?,
        down_mbps: bandwidth("down_mbps")?,
        congestion_control: None,
        udp_relay_mode: None,
        alpn,
        private_key: None,
        peer_public_key: None,
        pre_shared_key: None,
        local_address: None,
        reserved: None,
        mtu: None,
        workers: None,
        encryption: None,
        raw_xray_config: None,
        routing_policy: None,
    })
}

fn app_api_xray_profile_to_connect_request(
    profile: &serde_json::Value,
    request: &AppConnectLocationRequest,
) -> Result<ConnectRequest, String> {
    let format = profile
        .get("format")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    if !matches!(format, "xray-outbound-v1" | "xray-balanced-v1") {
        return Err("Unsupported DoodleVPN Xray profile format".into());
    }
    let config = profile
        .get("config")
        .filter(|value| value.is_object())
        .ok_or_else(|| "DoodleVPN Xray profile is missing config".to_string())?
        .clone();
    let outbounds = config
        .get("outbounds")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "DoodleVPN Xray profile is missing outbounds".to_string())?;
    let balanced = format == "xray-balanced-v1";
    if balanced && !is_managed_xray_balancer_config(&config) {
        return Err("DoodleVPN Xray balancer profile is invalid".into());
    }
    let proxy = outbounds
        .iter()
        .find(|outbound| {
            outbound
                .get("tag")
                .and_then(|value| value.as_str())
                .is_some_and(|tag| {
                    (balanced && tag.starts_with("entry-")) || (!balanced && tag == "proxy")
                })
        })
        .ok_or_else(|| "DoodleVPN Xray profile is missing proxy outbound".to_string())?;
    if proxy.get("protocol").and_then(|value| value.as_str()) != Some("vless") {
        return Err("Unsupported DoodleVPN Xray outbound protocol".into());
    }
    let address = profile
        .get("connect_address")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "DoodleVPN Xray profile is missing connect address".to_string())?
        .to_string();
    let port = profile
        .get("port")
        .and_then(|value| value.as_u64())
        .filter(|value| *value > 0 && *value <= u16::MAX as u64)
        .ok_or_else(|| "DoodleVPN Xray profile has an invalid port".to_string())?
        as u16;

    Ok(ConnectRequest {
        server_address: address,
        server_port: port,
        protocol: "vless".into(),
        uuid: None,
        password: None,
        transport: "xhttp".into(),
        security: "tls".into(),
        sni: None,
        host: None,
        path: None,
        fingerprint: None,
        public_key: None,
        short_id: None,
        flow: None,
        proxy_mode: request.proxy_mode.clone(),
        system_proxy_mode: request.system_proxy_mode.clone(),
        socks_port: request.socks_port,
        http_port: request.http_port,
        api_port: default_xray_api_port(),
        network_stack: request.network_stack.clone(),
        dns_mode: request.dns_mode.clone(),
        strict_route: request.strict_route,
        kill_switch: request.kill_switch,
        routing_rules: request.routing_rules.clone(),
        obfs_type: None,
        obfs_password: None,
        up_mbps: None,
        down_mbps: None,
        congestion_control: None,
        udp_relay_mode: None,
        alpn: None,
        private_key: None,
        peer_public_key: None,
        pre_shared_key: None,
        local_address: None,
        reserved: None,
        mtu: None,
        workers: None,
        encryption: None,
        raw_xray_config: Some(config),
        routing_policy: None,
    })
}

fn app_api_connection_result_body(
    lease: &AppApiProfileLeaseResponse,
    session: &AppApiTokenResponse,
    success: bool,
    latency_ms: i64,
    message: &str,
) -> serde_json::Value {
    serde_json::json!({
        "profile_id": lease.profile_id,
        "device_id": session.device_id,
        "app_version": env!("CARGO_PKG_VERSION"),
        "core_version": app_api_core_version(),
        "success": success,
        "error_code": if success { "" } else { "pc_connect_failed" },
        "latency_ms": latency_ms.max(0),
        "transport": lease.route_kind,
        "last_error": if success { String::new() } else { redact_support_line(message) }
    })
}

fn app_api_platform() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(windows)]
    {
        "windows"
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        "desktop"
    }
}

pub(super) fn app_api_core_version() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos-v6"
    }
    #[cfg(windows)]
    {
        "pc-v6"
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        "desktop-v6"
    }
}

pub(super) fn app_api_client_capabilities() -> serde_json::Value {
    serde_json::json!({
        "windows": cfg!(windows),
        "macos": cfg!(target_os = "macos"),
        "tun": true,
        "network_extension": cfg!(all(target_os = "macos", feature = "app-store")),
        "xray_reality": true,
        "native_xray_xhttp": cfg!(windows) || cfg!(target_os = "macos"),
        "xray_balancer_v1": cfg!(windows),
        "native_hysteria2": cfg!(windows),
        "dns_hijack": true
    })
}

fn app_api_device_body(device: &AppApiDeviceState, computer_name: &str) -> serde_json::Value {
    serde_json::json!({
        "device_id": device.client_device_id,
        "platform": app_api_platform(),
        "model": computer_name,
        "app_version": env!("CARGO_PKG_VERSION"),
        "hwid": device.hwid,
        "public_key": device.public_key
    })
}

pub(super) fn app_api_exchange_code_body(
    code: &str,
    device: &AppApiDeviceState,
    computer_name: &str,
) -> serde_json::Value {
    serde_json::json!({
        "code": code,
        "device": app_api_device_body(device, computer_name)
    })
}

pub(super) fn legacy_subscription_token(subscription_url: &str) -> Result<String, String> {
    let parsed = Url::parse(subscription_url.trim())
        .map_err(|_| "Stored DoodleVPN subscription URL is invalid.".to_string())?;
    if parsed.scheme() != "https" {
        return Err("Stored DoodleVPN subscription must use HTTPS.".into());
    }
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    if !matches!(
        host.as_str(),
        "ddlvpn.lol"
            | "www.ddlvpn.lol"
            | "doodlevpn.online"
            | "www.doodlevpn.online"
            | "sub.brewsandrologistics.fun"
    ) {
        return Err("Stored subscription is not a DoodleVPN subscription.".into());
    }
    let segments = parsed
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if segments.len() != 2 || !matches!(segments[0], "s" | "sub") {
        return Err("Stored DoodleVPN subscription URL is not supported.".into());
    }
    let token = segments[1].trim();
    if token.len() < 8
        || token.len() > 256
        || !token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "-_.~=".contains(ch))
    {
        return Err("Stored DoodleVPN subscription token is invalid.".into());
    }
    Ok(token.to_string())
}

pub(super) fn legacy_subscription_urls_from_renderer_state(value: &str) -> Vec<String> {
    let Ok(root) = serde_json::from_str::<serde_json::Value>(value) else {
        return Vec::new();
    };
    let Some(state) = root.get("state") else {
        return Vec::new();
    };
    let preferred_id = state
        .get("activeServer")
        .and_then(|server| server.get("subscriptionId"))
        .and_then(serde_json::Value::as_str);
    let Some(subscriptions) = state
        .get("subscriptions")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };

    let mut candidates = subscriptions
        .iter()
        .filter_map(|subscription| {
            let url = subscription.get("url")?.as_str()?;
            let token = legacy_subscription_token(url).ok()?;
            let preferred = preferred_id.is_some_and(|preferred_id| {
                subscription.get("id").and_then(serde_json::Value::as_str) == Some(preferred_id)
            });
            Some((preferred, token, url.to_string()))
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(preferred, _, _)| !*preferred);

    let mut tokens = Vec::new();
    candidates
        .into_iter()
        .filter_map(|(_, token, url)| {
            if tokens.contains(&token) {
                None
            } else {
                tokens.push(token);
                Some(url)
            }
        })
        .collect()
}

async fn app_api_exchange_legacy_subscription_url(
    subscription_url: &str,
) -> Result<AppApiSessionStatus, String> {
    let token = legacy_subscription_token(subscription_url)?;
    let device = app_api_load_or_create_device()?;
    let computer_name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| APP_PRODUCT_NAME.into());
    let body = serde_json::json!({
        "subscription_token": token,
        "device": app_api_device_body(&device, &computer_name)
    });
    let session = app_api_send_json::<AppApiTokenResponse>(
        reqwest::Method::POST,
        "/auth/legacy-subscription/exchange",
        None,
        Some(body),
    )
    .await
    .map_err(|e| e.to_string())?;
    app_api_store_session(&session)?;
    Ok(app_api_public_session(Some(session)))
}

pub(super) fn legacy_auto_exchange_failure_message(error: &str, subscription_url: &str) -> String {
    let mut error = error.replace(subscription_url, "[redacted-url]");
    if let Ok(token) = legacy_subscription_token(subscription_url) {
        error = error.replace(&token, "[redacted-token]");
    }
    format!(
        "legacy subscription auto-restore failed: {}",
        redact_support_line(&error)
    )
}

#[tauri::command]
pub(super) async fn app_api_session_status(
    app: tauri::AppHandle,
) -> Result<AppApiSessionStatus, String> {
    if !closed_control_plane_enabled() {
        return Ok(app_api_public_session(None));
    }
    if let Some(session) = app_api_load_session()? {
        return Ok(app_api_public_session(Some(session)));
    }

    let mut legacy_urls = Vec::new();
    if let Ok(Some(value)) = secure_store_fallback_get(&app, RENDERER_STATE_KEY) {
        legacy_urls.extend(legacy_subscription_urls_from_renderer_state(&value));
    }
    if let Ok(Some(value)) = secure_store_keyring_get(RENDERER_STATE_KEY) {
        for url in legacy_subscription_urls_from_renderer_state(&value) {
            let token = legacy_subscription_token(&url).ok();
            if !legacy_urls
                .iter()
                .any(|known| legacy_subscription_token(known).ok() == token)
            {
                legacy_urls.push(url);
            }
        }
    }
    let mut last_failure = None;
    for subscription_url in legacy_urls {
        match app_api_exchange_legacy_subscription_url(&subscription_url).await {
            Ok(session) => return Ok(session),
            Err(error) => last_failure = Some((subscription_url, error)),
        }
    }
    if let Some((subscription_url, error)) = last_failure {
        eprintln!(
            "[warn] {}",
            legacy_auto_exchange_failure_message(&error, &subscription_url)
        );
    }
    Ok(app_api_public_session(None))
}

#[tauri::command]
pub(super) async fn app_api_exchange_code(
    request: AppApiExchangeCodeRequest,
) -> Result<AppApiSessionStatus, String> {
    ensure_closed_control_plane_enabled()?;
    let code = request.code.trim();
    if code.is_empty() {
        return Err("Enter the DoodleVPN sign-in code.".into());
    }
    let device = app_api_load_or_create_device()?;
    let computer_name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| APP_PRODUCT_NAME.into());
    let body = app_api_exchange_code_body(code, &device, &computer_name);
    let session = app_api_send_json::<AppApiTokenResponse>(
        reqwest::Method::POST,
        "/auth/code/exchange",
        None,
        Some(body),
    )
    .await
    .map_err(|e| e.to_string())?;
    app_api_store_session(&session)?;
    Ok(app_api_public_session(Some(session)))
}

#[tauri::command]
pub(super) async fn app_api_exchange_legacy_subscription(
    request: AppApiExchangeLegacySubscriptionRequest,
) -> Result<AppApiSessionStatus, String> {
    ensure_closed_control_plane_enabled()?;
    app_api_exchange_legacy_subscription_url(&request.subscription_url).await
}

#[tauri::command]
pub(super) async fn app_api_refresh() -> Result<AppApiSessionStatus, String> {
    ensure_closed_control_plane_enabled()?;
    let session = app_api_refresh_session().await?;
    Ok(app_api_public_session(Some(session)))
}

#[tauri::command]
pub(super) async fn app_api_logout() -> Result<(), String> {
    ensure_closed_control_plane_enabled()?;
    let _ =
        app_api_authorized_json::<serde_json::Value>(reqwest::Method::POST, "/device/logout", None)
            .await;
    app_api_delete_session()
}

#[tauri::command]
pub(super) async fn app_api_locations() -> Result<AppApiLocationsResponse, String> {
    ensure_closed_control_plane_enabled()?;
    app_api_authorized_json::<AppApiLocationsResponse>(reqwest::Method::GET, "/locations", None)
        .await
}

#[tauri::command]
pub(super) async fn app_api_subscription_status() -> Result<AppApiSubscriptionSummary, String> {
    ensure_closed_control_plane_enabled()?;
    app_api_authorized_json::<AppApiSubscriptionSummary>(
        reqwest::Method::GET,
        "/subscription/status",
        None,
    )
    .await
}

#[tauri::command]
pub(super) async fn app_api_submit_diagnostics(
    submission: AppApiDiagnosticsSubmission,
) -> Result<serde_json::Value, String> {
    ensure_closed_control_plane_enabled()?;
    if submission.events.is_empty() {
        return Err("Diagnostics report is empty.".into());
    }
    let session =
        app_api_load_session()?.ok_or_else(|| "DoodleVPN sign-in is required.".to_string())?;
    let events = submission
        .events
        .into_iter()
        .take(20)
        .map(|event| {
            let mut event = app_api_sanitize_diagnostic_value(event, 0);
            if let Some(object) = event.as_object_mut() {
                object.insert("manual".into(), submission.manual.into());
                object.insert("platform".into(), app_api_platform().into());
                object.insert("architecture".into(), std::env::consts::ARCH.into());
                object.insert("core_version".into(), app_api_core_version().into());
            }
            event
        })
        .collect::<Vec<_>>();
    let body = serde_json::json!({
        "session_id": format!(
            "diag_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ),
        "device_id": session.device_id,
        "app_version": env!("CARGO_PKG_VERSION"),
        "events": events,
    });
    if body.to_string().len() > 60 * 1024 {
        return Err("Diagnostics report is too large.".into());
    }
    app_api_authorized_json(reqwest::Method::POST, "/diagnostics", Some(body)).await
}

async fn app_api_connection_profile(
    session: &AppApiTokenResponse,
    location_id: &str,
    selection_mode: &str,
) -> Result<AppApiProfileLeaseResponse, AppApiHttpError> {
    let body = serde_json::json!({
        "location_id": location_id,
        "device_id": session.device_id,
        "network_class": "normal",
        "selection_mode": selection_mode,
        "app_version": env!("CARGO_PKG_VERSION"),
        "core_version": app_api_core_version(),
        "schema_version": 2,
        "routing_policy_version": "desktop-v2",
        "client_capabilities": app_api_client_capabilities()
    });
    let lease: AppApiProfileLeaseResponse = app_api_authorized_json_http(
        reqwest::Method::POST,
        APP_API_CONNECTION_PROFILE_PATH,
        Some(body),
    )
    .await?;
    if lease.schema_version != 2 {
        return Err(AppApiHttpError {
            status: 426,
            message: "DoodleVPN profile format requires an app update.".into(),
        });
    }
    if lease.routing_policy.is_none() {
        return Err(AppApiHttpError {
            status: 426,
            message:
                "DoodleVPN profile is missing its signed routing policy. Refresh and try again."
                    .into(),
        });
    }
    if !lease
        .location_id
        .trim()
        .eq_ignore_ascii_case(location_id.trim())
    {
        return Err(AppApiHttpError {
            status: 502,
            message: "DoodleVPN profile location does not match the request.".into(),
        });
    }
    if !app_api_profile_lease_is_fresh(&lease, Utc::now().timestamp()) {
        return Err(AppApiHttpError {
            status: 502,
            message: "DoodleVPN profile is already expired.".into(),
        });
    }
    Ok(lease)
}

fn app_api_default_profile_request(location_id: String) -> AppConnectLocationRequest {
    AppConnectLocationRequest {
        location_id,
        fallback_location_ids: Vec::new(),
        proxy_mode: default_app_proxy_mode(),
        system_proxy_mode: default_system_proxy_mode(),
        socks_port: default_app_socks_port(),
        http_port: default_app_http_port(),
        network_stack: default_app_network_stack(),
        dns_mode: default_app_dns_mode(),
        strict_route: false,
        kill_switch: false,
        routing_rules: Vec::new(),
    }
}

fn app_api_connect_request_from_lease(
    lease: &AppApiProfileLeaseResponse,
    request: &AppConnectLocationRequest,
) -> Result<ConnectRequest, String> {
    let mut connect_request = app_api_profile_to_connect_request(&lease.native_profile, request)?;
    connect_request.routing_policy = Some(app_api_validated_routing_policy(lease)?);
    Ok(connect_request)
}

async fn app_api_connect_with_lease(
    lease: &AppApiProfileLeaseResponse,
    request: &AppConnectLocationRequest,
    app: tauri::AppHandle,
) -> ConnectResult {
    let connect_request = match app_api_connect_request_from_lease(lease, request) {
        Ok(request) => request,
        Err(message) => {
            return ConnectResult {
                success: false,
                message,
                health: None,
            }
        }
    };
    vpn_connect_authorized(connect_request, app.clone()).await
}

async fn app_api_refresh_cached_location_profile(
    session: &AppApiTokenResponse,
    location_id: &str,
) -> Result<(), String> {
    let lease = app_api_connection_profile(session, location_id, "background")
        .await
        .map_err(|error| error.to_string())?;
    let request = app_api_default_profile_request(location_id.to_string());
    let connect_request = app_api_connect_request_from_lease(&lease, &request)?;
    let ping = ping_server_profile(
        connect_request,
        format!("app-location:{}", location_id.trim().to_ascii_lowercase()),
    )
    .await;
    if ping.ping_ms < 0 {
        return Err("DoodleVPN cached profile probe failed.".into());
    }
    app_api_store_cached_profile(session, &lease)
}

pub(super) fn app_api_profile_error_is_terminal(error: &AppApiHttpError) -> bool {
    matches!(error.status, 400 | 401 | 403 | 426 | 429)
}

pub(super) fn app_api_validated_routing_policy(
    lease: &AppApiProfileLeaseResponse,
) -> Result<AppRoutingPolicy, String> {
    lease
        .routing_policy
        .clone()
        .ok_or_else(|| "DoodleVPN profile is missing its signed routing policy.".to_string())
        .and_then(validate_app_routing_policy)
}

#[tauri::command]
pub(super) async fn app_connect_location(
    request: AppConnectLocationRequest,
    app: tauri::AppHandle,
) -> ConnectResult {
    if let Err(message) = ensure_closed_control_plane_enabled() {
        return ConnectResult {
            success: false,
            message,
            health: None,
        };
    }
    let session = match app_api_load_session() {
        Ok(Some(session)) => session,
        Ok(None) => {
            return ConnectResult {
                success: false,
                message: "DoodleVPN sign-in is required.".into(),
                health: None,
            };
        }
        Err(e) => {
            return ConnectResult {
                success: false,
                message: e,
                health: None,
            };
        }
    };
    if session.subscription.device_allowed == Some(false) {
        return ConnectResult {
            success: false,
            message: "DoodleVPN device limit reached. Remove an unused device, then refresh the account status.".into(),
            health: None,
        };
    }

    let location_ids = app_connection_location_ids(&request);

    reset_connect_timings();
    let connect_started = Instant::now();
    // The Windows service is stopped while disconnected, so a cold SCM start
    // plus the named-pipe handshake would otherwise run strictly after the
    // lease HTTP round trip. Both calls are idempotent and tunnel_service_start
    // repeats them, so this is purely additive: winning the race makes the
    // later start free, losing it changes nothing. Deliberate side effect: the
    // service may stay up after a failed lease fetch, which it already does
    // between connect attempts. Starting the service creates no tunnel —
    // StartTunnel is a separate command.
    #[cfg(windows)]
    if request.proxy_mode == "tun" {
        tauri::async_runtime::spawn_blocking(|| {
            let _ = ensure_tunnel_service_running();
            let _ = ipc::tunnel_service_hello(env!("CARGO_PKG_VERSION"));
        });
    }

    let selection_mode = if location_ids.len() > 1 {
        "auto"
    } else {
        "manual"
    };
    let mut last_failure = None;
    for location_id in location_ids {
        if let Ok(Some(cached_lease)) = app_api_cached_profile(&session, &location_id) {
            let bringup_started = Instant::now();
            let result = app_api_connect_with_lease(&cached_lease, &request, app.clone()).await;
            record_connect_timing("cache_bringup", bringup_started);
            if result.success {
                let refresh_session = session.clone();
                let refresh_location_id = location_id.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = app_api_refresh_cached_location_profile(
                        &refresh_session,
                        &refresh_location_id,
                    )
                    .await;
                });
                record_connect_timing("total_to_ui", connect_started);
                return result;
            }
        }

        let started = Instant::now();
        let lease = match app_api_connection_profile(&session, &location_id, selection_mode).await {
            Ok(lease) => lease,
            Err(error) => {
                let terminal = app_api_profile_error_is_terminal(&error);
                last_failure = Some(ConnectResult {
                    success: false,
                    message: format!("DoodleVPN connection profile failed: {error}"),
                    health: None,
                });
                if terminal {
                    break;
                }
                continue;
            }
        };
        record_connect_timing("lease_fetch", started);
        let bringup_started = Instant::now();
        let result = app_api_connect_with_lease(&lease, &request, app.clone()).await;
        record_connect_timing("bringup", bringup_started);
        record_connect_timing("total", connect_started);
        let result_body = app_api_connection_result_body(
            &lease,
            &session,
            result.success,
            started.elapsed().as_millis() as i64,
            &result.message,
        );
        // Telemetry only — the response was always discarded. Awaiting it here
        // charged every connect the full backend round trip *after* the tunnel
        // was already up, including the first DNS resolution through the
        // brand-new tunnel. Measured on the stand: the service reported
        // connected at 3.3s while the user's stopwatch read 21s, and the xray
        // log showed a matching ~12s gap with no traffic. Report in the
        // background so the UI is released as soon as the tunnel is ready.
        tauri::async_runtime::spawn(async move {
            let _ = app_api_authorized_json::<serde_json::Value>(
                reqwest::Method::POST,
                "/connection-result",
                Some(result_body),
            )
            .await;
        });
        if result.success {
            if let Err(error) = app_api_store_cached_profile(&session, &lease) {
                eprintln!(
                    "[warn] App API profile cache write failed: {}",
                    redact_support_line(&error)
                );
            }
            record_connect_timing("total_to_ui", connect_started);
            return result;
        }
        last_failure = Some(result);
    }

    last_failure.unwrap_or(ConnectResult {
        success: false,
        message: "No VPN location is available.".into(),
        health: None,
    })
}

#[tauri::command]
pub(super) async fn app_ping_location(
    location_id: String,
    server_id: String,
) -> Result<PingResult, String> {
    ensure_closed_control_plane_enabled()?;
    let session =
        app_api_load_session()?.ok_or_else(|| "DoodleVPN sign-in is required.".to_string())?;
    if session.subscription.device_allowed == Some(false) {
        return Err("DoodleVPN device limit reached.".into());
    }
    let lease = app_api_connection_profile(&session, &location_id, "probe")
        .await
        .map_err(|error| error.to_string())?;
    let request = app_api_default_profile_request(location_id);
    let connect_request = app_api_connect_request_from_lease(&lease, &request)?;
    let result = ping_server_profile(connect_request, server_id).await;
    if result.ping_ms >= 0 {
        app_api_store_cached_profile(&session, &lease)?;
    }
    Ok(result)
}

#[tauri::command]
pub(super) async fn app_api_refresh_cached_profiles() -> Result<(), String> {
    ensure_closed_control_plane_enabled()?;
    let session =
        app_api_load_session()?.ok_or_else(|| "DoodleVPN sign-in is required.".to_string())?;
    if session.subscription.device_allowed == Some(false) {
        return Err("DoodleVPN device limit reached.".into());
    }
    for location_id in app_api_cached_profile_location_ids(&session)? {
        let _ = app_api_refresh_cached_location_profile(&session, &location_id).await;
    }
    Ok(())
}

#[tauri::command]
pub(super) async fn app_disconnect(app: tauri::AppHandle) -> ConnectResult {
    vpn_disconnect(app).await
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    fn session(device_id: &str) -> AppApiTokenResponse {
        AppApiTokenResponse {
            access_token: String::new(),
            access_expires_at: String::new(),
            expires_in: 0,
            refresh_token: "refresh-redacted".into(),
            refresh_expires_at: "2030-01-01T00:00:00Z".into(),
            device_id: device_id.into(),
            subscription: AppApiSubscriptionSummary {
                user_uuid: Some("user-redacted".into()),
                ..Default::default()
            },
        }
    }

    fn lease(location_id: &str, expiry: i64) -> AppApiProfileLeaseResponse {
        AppApiProfileLeaseResponse {
            schema_version: 2,
            profile_id: "profile-redacted".into(),
            lease_id: "lease-redacted".into(),
            expires_at: DateTime::<Utc>::from_timestamp(expiry, 0)
                .expect("test timestamp")
                .to_rfc3339(),
            location_id: location_id.into(),
            route_kind: String::new(),
            first_hop: String::new(),
            target_country_id: location_id.into(),
            entry_role: String::new(),
            routing_rules_version: String::new(),
            routing_policy: None,
            native_profile: serde_json::json!({}),
            profile: None,
            transport_capability: None,
        }
    }

    #[test]
    fn profile_cache_is_session_scoped_and_expiry_bounded() {
        let now = 1_800_000_000;
        let owner = session("device-a");
        let mut cache = AppApiProfileCache::for_session(&owner);
        cache.insert(&owner, lease("de", now + 3600), now);
        cache.insert(&owner, lease("nl", now - 1), now);

        assert!(cache.profile("de", now).is_some());
        assert!(cache.profile("nl", now).is_none());
        assert!(!cache.scope_matches(&session("device-b")));
        cache.app_version = "older-release".into();
        assert!(!cache.scope_matches(&owner));
    }
}
