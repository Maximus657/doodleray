# Legacy device migration decision

This note records the reviewed disposition of commit `ca14e27`; the commit was not cherry-picked.

## Accepted semantics

- An already persisted `AppApiDeviceState` remains authoritative and is never automatically rotated.
- Only a newly generated Windows device attempts to derive its `hwid` from the 64-bit registry view of `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid`. The trimmed, ASCII-case-folded value is hashed with SHA-256 after `doodleray-hwid-v1\n`; the API receives `pc-hwid-` plus the first 16 digest bytes as lowercase hex. The raw registry value is neither transmitted nor logged.
- If that Windows registry read fails, device creation retains the existing random UUID `hwid` fallback. `client_device_id` and the Ed25519 keypair remain random in every case.
- Refresh failures delete the stored session only when HTTP 401 or 403 rejects it. Network failures and all other statuses, including 5xx, retain the session for retry.
- Legacy automatic exchange continues through every candidate and emits only the final failure after existing support redaction, with the known legacy URL and token explicitly removed first.

## Rejected semantics

- No macOS `IOPlatformUUID`, MAC address, Linux machine ID, or other non-Windows permanent hardware identifier is read. Non-Windows builds keep the persisted random UUID behavior. This avoids deriving device data to uniquely identify a user or device, which is incompatible with Apple App Store privacy requirements.
- The frontend closed-control-plane default was not changed. It remains enabled unless explicitly configured as `0`; Rust remains compile-time fail-closed, and official builds must explicitly enable both flags.
- API field names, updater behavior, VPN behavior, application identifiers, and existing persisted device/session formats were not changed.
