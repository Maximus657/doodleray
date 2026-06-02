## 1. Bottom-line recommendation

DoodleRay should move Full Computer / TUN mode to a **one-time-installed Windows Service tunnel manager**:

```text
DoodleRay UI / Tauri app, non-elevated
  -> local authenticated IPC
DoodleRay Tunnel Service, privileged, installed once
  -> owns Wintun adapter, routes, DNS, WFP/kill switch
  -> owns sing-box.exe / xray.exe child processes
  -> reports structured progress/readiness back to UI
```

The current `ShellExecuteW("runas") -> .bat -> sing-box.exe` path explains both user-visible problems: UAC is expected whenever a non-elevated process requests administrative privileges, and Windows documents that UAC prompts when an administrative access token is required. `ShellExecute` also explicitly supports the `runas` verb for launching as administrator, which is the mechanism currently causing repeated prompts. ([Microsoft Learn][1])

This is the pattern mature Windows VPN clients usually converge on: **install privileged networking components once, then let a normal desktop UI command them through tightly scoped local IPC**. A scheduled task that launches the whole app elevated is a workaround, not the production control plane.

I would not ship repeated per-connect elevation for Full Computer mode except as a legacy fallback.

---

## 2. Critical review of the current design

The current design has several unsafe or fragile assumptions.

First, **process-name cleanup is dangerous**. `taskkill /IM sing-box.exe /F /T` can kill unrelated user or system processes. It also loses ownership semantics: DoodleRay does not know whether the `sing-box.exe` it sees is the one it started, whether it is stale, or whether another product is using it.

Second, **the `.bat` launcher and `%TEMP%` runtime files are not production-grade for privileged network control**. Even when the app writes the files correctly, temp locations, shell interpretation, quoting, log leakage, stale files, AV interference, and path confusion create avoidable risk. A privileged service should call `CreateProcess` directly with explicit executable paths, sanitized arguments, non-inheritable handles, and no shell.

Third, **polling `tasklist` is not a readiness signal**. “A process named sing-box is running” does not mean the TUN adapter exists, routes are installed, DNS is safe, kill-switch filters are active, or the local Xray SOCKS bridge is reachable.

Fourth, **the UI currently participates in too much tunnel lifecycle**. In Xray + TUN mode, starting Xray in the UI-side backend while sing-box TUN is started elevated creates split ownership. If the UI crashes or exits, the privileged TUN bridge can be left forwarding to a dead local port. In Full Computer mode, the privileged component should own the whole tunnel graph.

Fifth, **the scheduled task feature solves the wrong problem**. `/RL HIGHEST /SC ONLOGON` only makes the entire app elevated at logon. It does not provide a hardened, low-surface privileged tunnel manager. It also encourages running the Tauri/webview UI with elevated privileges, which is precisely what you should avoid.

Sixth, **the existing named-pipe service sketch is directionally right but not yet a product architecture**. A service with `StartTun <json>` and `StopTun` is not enough. It needs a state machine, authentication, command validation, resource ownership, job-object process control, update rules, crash reconciliation, and support for multi-user Windows sessions.

---

## 3. Service vs helper vs scheduled task

### Recommended: Windows Service

Use a dedicated service, for example:

```text
Service name:        DoodleRayTunnelService
Display name:        DoodleRay Tunnel Service
Binary path:         C:\Program Files\DoodleRay\doodleray-tunnel-service.exe
Runtime data:        C:\ProgramData\DoodleRay\
IPC pipe:            \\.\pipe\DoodleRay.TunnelService.v1
Startup type:        Automatic or Automatic Delayed Start
```

Windows services are managed by the Service Control Manager, which maintains installed services, starts them on boot or demand, tracks status, and transmits control requests. ([Microsoft Learn][2]) That gives DoodleRay a proper place for privileged networking lifecycle, recovery actions, event logging, service ACLs, and sleep/resume handling.

For fastest connect, I would use an **always-installed, normally-running idle service**, not a demand-start-only helper. A cold SCM start, driver load, adapter creation, and engine validation can easily dominate connection latency. An idle service can preflight binaries, pre-create/reuse the Wintun adapter, maintain a current state snapshot, and respond to the UI immediately.

### Not recommended as primary: scheduled task IPC

A scheduled task with highest privileges can reduce UAC prompts after the task is created, but it is a poor VPN control plane:

```text
UI -> scheduled task -> elevated helper
```

Problems:

* task actions and arguments are easy to misconfigure;
* multi-user semantics are awkward;
* service recovery, stop controls, status, and dependency handling are inferior to SCM;
* it tends to run in a user session rather than as a clean Session 0 service;
* it often leads to the whole app being elevated.

Keep scheduled task support only for **legacy “launch app elevated at login” compatibility**, not for Full Computer mode architecture.

### Privileged helper process

On Windows, the clean form of a privileged helper is a **Windows Service**. A standalone elevated helper launched by `runas` still produces UAC. A helper launched by a scheduled task is a workaround. A helper installed and managed by SCM is the mature pattern.

---

## 4. Service account choice

### Use LocalSystem for the MVP

For DoodleRay’s current architecture, the service should initially run as **LocalSystem**, then be aggressively hardened.

LocalSystem has extensive local privileges and includes the `NT AUTHORITY\SYSTEM` and `BUILTIN\Administrators` SIDs in its token. Microsoft explicitly warns that it has extensive privileges on the local computer. ([Microsoft Learn][3]) That is a security burden, but Full Computer mode needs to create/open Wintun, adjust adapter configuration, manipulate routes/DNS, and install WFP kill-switch rules. LocalSystem is the practical MVP choice.

### Why not LocalService initially?

LocalService has minimum privileges on the local computer and presents anonymous credentials on the network. ([Microsoft Learn][4]) That is attractive from least-privilege and network-identity perspectives, but it is unlikely to be sufficient for adapter creation, route/DNS manipulation, firewall/WFP policy, and driver-related work without a larger privilege-delegation design.

A future hardening step could split the architecture:

```text
SYSTEM broker service:
  - Wintun adapter
  - routes / DNS / WFP
  - process supervision

Lower-privilege engine worker:
  - xray outbound
  - possibly non-privileged proxy logic
```

But with external `sing-box.exe` currently owning TUN behavior, the TUN engine will probably need to run privileged unless DoodleRay takes over adapter/route management itself.

### Do not run the UI elevated

Do not rely on “Launch on Startup (Admin)” as the normal solution. The Tauri/webview UI, React state, renderer, update flow, and user-facing frontend are much larger attack surfaces than a small Rust service. The UI should be non-admin.

---

## 5. Installation, update, and removal model

### Installation

The installer should perform the one-time privileged setup:

```text
1. Install DoodleRay UI under Program Files.
2. Install doodleray-tunnel-service.exe under Program Files.
3. Install bundled sing-box.exe, xray.exe, wintun.dll under Program Files.
4. Create DoodleRayTunnelService.
5. Configure service SID and service ACLs.
6. Create ProgramData runtime/secrets/log directories with strict ACLs.
7. Optionally pre-create/reuse DoodleRay’s Wintun adapter.
8. Start the service.
```

Windows service creation can be done through the Windows Installer/MSI stack, WiX, NSIS custom action, or direct SCM APIs. The `sc.exe create` documentation describes service registration in the SCM database, but production installers should generally use a real installer framework rather than shelling out to `sc.exe`. ([Microsoft Learn][5])

Recommended directories:

```text
C:\Program Files\DoodleRay\
  DoodleRay.exe
  doodleray-tunnel-service.exe
  engines\
    sing-box.exe
    xray.exe
    wintun.dll

C:\ProgramData\DoodleRay\
  runtime\
  logs\
  state\
  secrets\
```

ACL principles:

```text
Program Files\DoodleRay
  SYSTEM: Full
  Administrators: Full
  Users: Read + Execute only

ProgramData\DoodleRay\runtime
  SYSTEM: Full
  Administrators: Full
  DoodleRay service SID: Full
  Users: no direct access, or read-only for redacted diagnostics only

ProgramData\DoodleRay\secrets
  SYSTEM/service SID only; administrators if product policy allows recovery
```

Use a **service SID** so the service can be granted access to its own objects without granting broad access to all LocalSystem services. Microsoft’s `SERVICE_SID_INFO` documentation notes that service SIDs let developers control access to objects a service uses, rather than relying only on LocalSystem. ([Microsoft Learn][6])

### Adapter provisioning

DoodleRay should stop creating/removing the tunnel adapter on every connection. Prefer a persistent adapter:

```text
Adapter name: DoodleRay Tunnel
Adapter type: Wintun
Stable GUID: product-generated fixed GUID
```

Wintun’s API model supports creating named adapters with a type and optional fixed GUID. ([GitHub][7]) Pre-creating or reusing a stable DoodleRay adapter removes a large source of connect latency and avoids PnP churn.

Do **not** remove arbitrary Wintun adapters. Only touch the adapter whose name, GUID, and ownership marker match DoodleRay.

### Updates

There are two acceptable models.

The safer first model is:

```text
UI downloads/verifies update
  -> user approves installer elevation once
  -> installer stops service
  -> installer replaces service + engines
  -> installer restarts service
```

This means no per-connect UAC, but updates may still require one UAC prompt. That is normal and defensible.

A later “silent privileged update” design is possible, but only if the service accepts **vendor-signed update packages**, verifies Authenticode identity, validates hashes, prevents downgrade/rollback unless explicitly allowed, and never executes a UI-supplied arbitrary path. Do not add this until the tunnel service IPC is mature.

Update compatibility rules:

```text
UI -> GetServiceInfo
service -> { service_version, protocol_version, engine_versions, capabilities }

If compatible:
  normal operation

If service too old:
  UI offers privileged repair/update

If UI too old:
  service rejects unsafe/new commands with typed error
```

### Removal

Uninstall must be idempotent:

```text
1. Stop active tunnel or leave persistent kill switch only if user explicitly requests.
2. Stop service.
3. Remove service registration.
4. Remove DoodleRay-owned WFP filters/provider/sublayer.
5. Remove DoodleRay-owned routes and DNS state.
6. Remove DoodleRay Wintun adapter only.
7. Remove Program Files binaries.
8. Remove ProgramData runtime.
9. Ask before removing secrets/profile store.
```

---

## 6. IPC design and security

Named pipes are a good fit for Rust/Tauri-to-service local IPC, but the current sketch needs hardening.

### Pipe creation

Use a versioned pipe name:

```text
\\.\pipe\DoodleRay.TunnelService.v1
```

Create it with:

```text
PIPE_ACCESS_DUPLEX
FILE_FLAG_OVERLAPPED
FILE_FLAG_FIRST_PIPE_INSTANCE
PIPE_TYPE_MESSAGE
PIPE_READMODE_MESSAGE
PIPE_REJECT_REMOTE_CLIENTS
```

Use `FILE_FLAG_FIRST_PIPE_INSTANCE` to reduce pipe-squatting ambiguity and `PIPE_REJECT_REMOTE_CLIENTS` to make it local-only. Named pipes support explicit security descriptors; Microsoft warns that a default named-pipe descriptor grants broad access, including read access to Everyone and anonymous users, so DoodleRay should never use the default descriptor for the control pipe. ([Microsoft Learn][8])

### Pipe ACL

Do not grant command access to `Everyone`.

A practical model:

```text
Allowed to connect/control:
  SYSTEM
  Administrators
  Local group: DoodleRay VPN Users

Allowed to read coarse status, optional:
  Authenticated Users or Interactive Users
```

Create a local group during install:

```text
DoodleRay VPN Users
```

Add the installing user by default for consumer installs. In enterprise installs, allow admins to manage membership.

Use two pipes if needed:

```text
\\.\pipe\DoodleRay.TunnelService.Control.v1
  strict: SYSTEM, Administrators, DoodleRay VPN Users

\\.\pipe\DoodleRay.TunnelService.Status.v1
  looser: authenticated local users, redacted status only
```

The control pipe should allow only read/write needed for the protocol, not generic all-access for normal users.

### Caller identity validation

Pipe ACL is necessary but not sufficient. On connection:

```text
1. Read ClientHello.
2. Call ImpersonateNamedPipeClient.
3. Open the impersonated thread token.
4. Extract TokenUser SID.
5. Check membership in DoodleRay VPN Users or Administrators.
6. Record logon session/session ID.
7. RevertToSelf immediately.
8. Bind the IPC session to that user SID.
```

`ImpersonateNamedPipeClient` exists specifically so a pipe server can impersonate the client after reading from the pipe, and the server should call `RevertToSelf` when finished. ([Microsoft Learn][9]) Use token checks as the primary authorization mechanism. `CheckTokenMembership` can determine whether a SID is enabled in a token. ([Microsoft Learn][10])

Use `GetNamedPipeClientProcessId` and `GetNamedPipeClientSessionId` only as defense-in-depth. Microsoft exposes these APIs for retrieving pipe client PID/session metadata. ([Microsoft Learn][11]) PID/path checks are useful for telemetry and anomaly detection, but they are not a substitute for token authorization.

Important: avoid service-to-client callbacks where the SYSTEM service connects to a pipe controlled by the UI. Named-pipe impersonation is a common local privilege escalation pattern when a privileged process is tricked into connecting to an attacker-controlled pipe. Keep the service as the pipe server and multiplex events over that server-owned connection.

### Command schema

Use a strict, versioned schema. JSON is acceptable for MVP if you use typed `serde` structs with `deny_unknown_fields`, size caps, and exhaustive enum validation. Protobuf/CBOR/MessagePack is better long-term.

Example:

```rust
ClientHello {
  protocol_version: u32,
  client_version: String,
  client_pid: u32,
  capabilities: Vec<String>,
  client_nonce: [u8; 32],
}

StartTunnel {
  op_id: Uuid,
  user_sid: String,          // service verifies against token, does not trust blindly
  profile_ref: ProfileRef,   // preferred over raw config
  route_mode: TunRouteMode,
  workshop_policy_ref: Option<PolicyRef>,
  dns_policy: DnsPolicy,
  kill_switch: KillSwitchPolicy,
  allow_lan: bool,
  request_created_ms: u64,
}

StopTunnel {
  op_id: Uuid,
  active_tunnel_id: Option<Uuid>,
  reason: StopReason,
}

CancelOperation {
  op_id: Uuid,
}

GetStatus {}

WatchStatus {
  since_seq: u64,
}
```

Service responses:

```rust
TunnelStatus {
  seq: u64,
  state: Disconnected | Connecting | Connected | Disconnecting | FailedClosed,
  phase: Option<Phase>,
  active_op_id: Option<Uuid>,
  active_tunnel_id: Option<Uuid>,
  owner_user_sid_hash: Option<String>,
  started_at: Option<SystemTime>,
  redacted_error: Option<ErrorInfo>,
  phase_timings_ms: Vec<PhaseTiming>,
}
```

### Replay and stale command handling

Use three IDs:

```text
op_id:        one UI operation, such as this connect attempt
tunnel_id:    one successful tunnel instance
generation:   monotonically increasing service state version
```

Rules:

```text
StartTunnel with stale generation -> reject
StopTunnel for non-active tunnel_id -> no-op or reject as stale
CancelOperation for completed op_id -> no-op
Duplicate op_id -> return existing operation status
Concurrent StartTunnel -> reject or convert to RestartTunnel explicitly
```

This preserves your frontend operation-id guard, but moves the authoritative version into the service.

### Config secrecy

Prefer this model:

```text
UI sends:
  profile_id
  route intent
  selected server/ref
  non-secret policy

Service stores:
  raw profile material
  generated xray/sing-box configs
  runtime credentials
```

The service should own a profile/secret store under `ProgramData`, encrypted using Windows facilities and ACLed to the service. If the UI must import or edit a profile, send the secret once over the authenticated local pipe and have the service store it. The UI should not keep emitting raw config on every connect.

If sing-box/xray require config files, write them under:

```text
C:\ProgramData\DoodleRay\runtime\<tunnel_id>\
```

not `%TEMP%`.

Runtime config files should have:

```text
SYSTEM/service SID: read/write
Administrators: optional read/write depending support policy
Users: no access
```

Never include raw profile material in:

```text
frontend logs
Tauri command errors
support bundles
analytics
crash reports
progress messages
IPC trace logs
```

---

## 7. Engine ownership: sing-box, xray, and Job Objects

### The service should own all processes needed for Full Computer mode

For Full Computer mode, do this:

```text
DoodleRayTunnelService
  -> xray.exe, if profile is Xray/XHTTP/raw Xray
  -> sing-box.exe, if TUN bridge or direct sing-box TUN
```

Do not let the UI own Xray while the service owns sing-box TUN. That creates split lifecycle and crash leakage.

For System Proxy mode, you have two options:

```text
Phase 1:
  UI/backend may continue to own pure proxy-only xray/sing-box, no privileged ops.

Phase 2 preferred:
  service owns all engine processes, including proxy-only, while UI only toggles Windows proxy settings.
```

The Phase 2 design makes mode switching faster and more reliable because Xray can be reused when switching from System Proxy to TUN bridge.

### Use a Windows Job Object

When the service starts an engine process:

```text
1. Validate executable path is under Program Files\DoodleRay\engines.
2. Verify file hash/signature.
3. Create a per-tunnel Job Object.
4. Set kill-on-job-close.
5. Create child process suspended.
6. Assign child to the job.
7. Resume child.
8. Keep process handle and creation time.
9. Redirect stdout/stderr to service-owned log pipes/files.
```

Windows Job Objects are designed to manage groups of processes as a unit, and a process can be associated with a job through `AssignProcessToJobObject`. ([Microsoft Learn][12]) Creating the process suspended before assigning it avoids a race where the child starts doing work before the job restrictions apply; Microsoft’s Raymond Chen has specifically called out this pattern. ([Microsoft for Developers][13])

Stop sequence:

```text
1. Mark service state Disconnecting.
2. Stop accepting new traffic / keep kill-switch policy active as required.
3. Ask engine to stop gracefully if supported.
4. Wait short bounded interval.
5. Terminate the job object if still alive.
6. Close job handle.
7. Remove routes/DNS/WFP according to policy.
8. Delete runtime files.
9. Mark Disconnected or FailedClosed.
```

This replaces:

```text
taskkill /IM sing-box.exe /F /T
```

with:

```text
Terminate only the job/process handles this service created.
```

### Should you link sing-box as a library?

Not for the first production migration.

Spawning a signed external `sing-box.exe` under a service-owned job object is pragmatic and incremental. Linking sing-box into a Rust service would be non-trivial because sing-box is Go-based, introduces build/ABI/licensing complexity, and expands the privileged service’s in-process attack surface.

A better long-term hardening direction is:

```text
SYSTEM service owns Wintun/routes/WFP
lower-privileged worker owns protocol parsing/outbound network
```

But that likely requires DoodleRay to own more of the TUN packet path instead of delegating everything to `sing-box.exe`.

---

## 8. Readiness signals: replace `tasklist` polling

The service should report readiness through a deterministic state machine, not by polling process names.

Recommended phases:

```text
ValidatingProfile
PreparingRuntime
StoppingPreviousTunnel
PreparingAdapter
StartingXray
WaitingForXrayProxy
StartingSingBox
WaitingForTunEngine
ApplyingRoutes
ApplyingDns
ApplyingKillSwitch
VerifyingTrafficSafety
Connected
```

Readiness checks should include:

```text
xray:
  process handle alive
  stdout/stderr has no fatal error
  local SOCKS/HTTP port accepts connection
  optional protocol-level probe succeeds

sing-box:
  process handle alive
  config validation succeeded before start
  stdout/stderr has no fatal error
  optional local control API responds on 127.0.0.1 with secret

TUN:
  DoodleRay adapter exists by GUID
  adapter is up or in expected state
  assigned addresses are present
  expected routes are present
  DNS configuration is present
  WFP kill-switch filters are present if required
```

`sing-box check -c config.json` should be run before launch when practical; sing-box documents a `check` command for configuration validation. ([Sing Box][14])

For sing-box TUN behavior, remember that non-privileged mode has limitations; sing-box documentation notes that when TUN runs non-privileged, addresses and MTU are not automatically configured. ([Sing Box][15]) This is one reason the privileged service needs to own or supervise adapter configuration.

If using sing-box’s experimental control API, bind it only to loopback with a high-entropy secret. Never expose it on LAN. The UI should not talk to sing-box directly; the service should.

---

## 9. Fast connect/disconnect design

To get competitor-like connect speed, remove avoidable cold work from the connect path.

### Do once at install or service start

```text
Install service
Install/copy signed engine binaries
Install/copy Wintun DLL
Pre-create DoodleRay Wintun adapter
Validate ProgramData ACLs
Validate engine signatures/hashes
Warm service state
```

### Do before user presses Connect, if possible

```text
Resolve selected profile metadata
Pre-render non-secret config template
Check port availability
Check adapter health
Check service/engine versions
```

### Do on Connect

```text
UI -> StartTunnel over pipe
service:
  acquire tunnel lock
  create op_id/tunnel_id
  build runtime config
  validate config
  apply preconnect kill-switch policy
  start xray if needed
  start sing-box
  apply/verify routes and DNS
  verify WFP filters
  publish Connected
```

### Do on Disconnect

```text
UI -> StopTunnel over pipe
service:
  mark Disconnecting
  preserve kill switch until safe
  stop engine job
  remove exact owned routes
  restore DNS using compare-and-swap
  remove dynamic/policy filters as appropriate
  keep adapter for reuse
  publish Disconnected
```

Do **not** delete the adapter on normal disconnect. Keeping a persistent adapter is one of the easiest latency wins.

---

## 10. Routes, DNS, and kill switch lifecycle

### Treat network changes as a transaction

The service should maintain a resource ledger:

```rust
OwnedTunnelResources {
  tunnel_id: Uuid,
  adapter_guid: Guid,
  adapter_luid: u64,
  routes_added: Vec<RouteKey>,
  dns_changes: Vec<DnsChangeRecord>,
  wfp_provider_guid: Guid,
  wfp_sublayer_guid: Guid,
  wfp_filter_ids: Vec<u64>,
  engine_processes: Vec<ProcessRecord>,
  runtime_dir: PathBuf,
  previous_state_snapshot_hash: String,
}
```

Write the ledger before and after each phase so crash recovery knows what was partially applied.

Use exact ownership markers:

```text
route destination/prefix + interface LUID + metric
DNS interface GUID + previous/applied values
WFP provider/sublayer GUIDs
adapter GUID/name
runtime tunnel_id
process handle/job object
```

### DNS cleanup must be compare-and-swap

Do not blindly restore DNS from an old snapshot if the user or another VPN changed DNS while DoodleRay was connected.

Use this rule:

```text
If current DNS == DoodleRay-applied DNS:
  restore previous DNS
Else:
  leave current DNS and log "external DNS modification detected"
```

Prefer per-interface DNS on the DoodleRay adapter over modifying physical adapters whenever possible.

### Route cleanup must remove exact routes only

Do not run broad route deletes. Remove only routes matching:

```text
destination/prefix
interface LUID/index
next hop
metric
tunnel_id ownership marker where available
```

Use IP Helper APIs for route inspection/modification rather than shelling out to `route.exe`. Microsoft’s IP Helper API is intended for retrieving and modifying local network configuration. ([Microsoft Learn][16])

### Kill switch

Use Windows Filtering Platform for kill switch and DNS leak prevention. WFP is Microsoft’s platform for filtering network data at layers in the Windows networking stack. ([Microsoft Learn][17])

There is a design tension:

```text
Dynamic WFP filters:
  + automatically cleaned up if service crashes
  - can fail open if service crashes

Persistent WFP filters:
  + can fail closed across service crash/reboot
  - can strand users offline if cleanup is buggy
```

Microsoft’s WFP best practices recommend dynamic sessions because objects added in a dynamic session are automatically deleted when the session ends. ([Microsoft Learn][18]) For ordinary “only while connected” filtering, use dynamic sessions.

For a true kill switch / “block traffic if VPN is not connected” product feature, use persistent filters deliberately, with:

```text
clear UI semantics: "Kill switch is ON"
service recovery actions
startup reconciliation
safe-mode removal path
uninstaller cleanup
support diagnostic for stuck filters
```

The service should install filters in a DoodleRay provider/sublayer with stable GUIDs so cleanup is exact.

### Failure ordering

Connect should be fail-closed:

```text
1. Install preconnect WFP policy.
2. Allow only required bootstrap traffic:
   - loopback
   - local IPC
   - DHCP/NDP as needed
   - VPN server endpoint/direct bootstrap path
   - LAN if user allowed LAN
3. Start engines.
4. Apply routes/DNS.
5. Verify.
6. Mark connected.
```

Disconnect should avoid transient leaks:

```text
1. Mark Disconnecting.
2. Keep kill-switch filters active while removing routes/DNS.
3. Stop engines.
4. Remove owned routes/DNS.
5. Remove kill-switch filters only if user policy permits direct traffic.
6. Mark Disconnected.
```

IPv6 must be explicit. Either route IPv6 through the tunnel or block it. Do not allow IPv6 to silently bypass an IPv4-only tunnel.

---

## 11. Crash, restart, app exit, sleep/resume

### UI crash or exit

The service should not depend on the UI to clean up. Decide product semantics:

```text
Option A:
  VPN remains connected if UI exits.
  Tray/app reconnects to service state later.

Option B:
  VPN disconnects when owning UI session exits.
  Service tracks owner session and stops tunnel.
```

For a VPN client, Option A is usually better. Full Computer mode is a machine-level state, not a renderer lifecycle state.

### Service crash

Use service failure actions so SCM restarts the service after crashes. Windows exposes service failure action configuration through `SERVICE_FAILURE_ACTIONS`. ([Microsoft Learn][19])

On service startup:

```text
1. Load last resource ledger.
2. Detect stale DoodleRay runtime dirs.
3. Ensure no owned engine processes survived.
4. Reconcile WFP filters:
   - if kill switch policy ON: ensure filters present
   - otherwise remove stale DoodleRay filters
5. Reconcile routes/DNS:
   - remove or restore exact owned state
6. Validate adapter health.
7. Publish recovered state.
```

If child engines are in a job object with kill-on-job-close, they should die when the service dies and the job handle closes. That prevents orphaned privileged `sing-box.exe` processes.

### Windows sleep/resume

Register a service control handler with `RegisterServiceCtrlHandlerEx`/`HandlerEx` and handle power events. Microsoft documents service control handlers and the extended handler function for service control requests. ([Microsoft Learn][20])

On suspend:

```text
mark network_suspended
pause aggressive health checks
keep kill-switch policy according to user setting
```

On resume:

```text
recheck adapter LUID/index
recheck default routes
reapply DNS if needed
reinstall WFP filters if missing
restart engines if sockets are dead
refresh endpoint resolution
publish Reconnecting or Connected
```

Also listen for network-change notifications. Wi-Fi changes, Ethernet arrival, VPN competing products, Hyper-V/WSL adapters, and captive portals can all mutate route metrics.

### Shutdown

Register for preshutdown if you need extra cleanup time. Microsoft’s preshutdown documentation describes SCM waiting while a service transitions through stop pending. ([Microsoft Learn][21])

But do not rely on shutdown cleanup for correctness. Startup reconciliation is more important.

---

## 12. What moves to the helper, what remains in UI

### Must move to service/helper

```text
Wintun adapter creation/open/configuration
route table mutation
DNS mutation
WFP kill switch and DNS leak filters
sing-box TUN process lifecycle
xray process lifecycle when used by TUN bridge
per-app TUN bridge lifecycle
full-device tunnel state machine
single-active-engine enforcement
runtime config generation for privileged engines
profile secret storage or at least profile secret handling
redacted engine log collection
crash recovery/reconciliation
sleep/resume/network-change handling
```

### Can remain in UI/Tauri process

```text
React UI
Zustand connection state rendering
button animations/progress display
profile selection/editing UI
subscription/account UX
Workshop rule editing UI
non-secret route intent construction
System Proxy toggle UI
IPC client
display of redacted diagnostics
```

### Maybe remain in UI temporarily

```text
pure System Proxy mode local engine
Windows user proxy setting toggle
```

But if System Proxy mode and TUN mode share the same Xray core, eventually move Xray ownership to the service too. That allows faster mode switching and cleaner crash behavior.

---

## 13. Workshop split tunneling caveats

Process-name routing is useful but not a security boundary.

sing-box supports `process_name` and `process_path` route rules on Windows, macOS, and Linux. ([Sing Box][22]) But process names are spoofable. For Workshop rules:

```text
Convenience routing:
  process_name is acceptable

Security-sensitive allow/block:
  prefer full process path
  prefer signed binary identity where available
  record publisher/hash for known apps
  warn when rule is name-only
```

Also remember Full Computer mode affects the entire machine. On multi-user Windows systems, a rule created by one user can affect processes in another session.

---

## 14. Instrumentation plan

Before rewriting everything, add phase timings now. You need p50/p95/p99 by phase, not one “connect took 18 seconds” number.

### Current path telemetry

Add redacted timings around:

```text
vpn_connect received
profile parsed
xray config generated
xray process spawn started
xray port ready
sing-box config generated
stop_tun cleanup started
stop_tun cleanup finished
temp dir/file writes
ShellExecuteW("runas") called
elevated launcher observed
sing-box process observed
first fatal/error log read
TUN ready inferred
connect success/failure
```

For UAC specifically, you cannot measure user decision time perfectly unless you use `ShellExecuteEx` with a process handle and a clear child process readiness contract, but you can still record:

```text
elevation_required: true/false
ShellExecute result/error
time until elevated child observable
time until readiness/failure
```

### Service path telemetry

For the production service, emit redacted structured events:

```text
ipc_connect_ms
service_cold_start_ms
auth_ms
queue_wait_ms
config_snapshot_ms
secret_decrypt_ms
config_render_ms
singbox_check_ms
previous_cleanup_ms
adapter_open_ms
adapter_create_ms
adapter_configure_ms
xray_spawn_ms
xray_ready_ms
singbox_spawn_ms
singbox_control_ready_ms
route_apply_ms
dns_apply_ms
wfp_apply_ms
readiness_verify_ms
total_connect_ms
disconnect_job_stop_ms
disconnect_route_cleanup_ms
disconnect_dns_restore_ms
disconnect_wfp_cleanup_ms
```

Never log:

```text
server credentials
raw profile JSON
access tokens
subscription URLs
private keys
proxy passwords
full generated configs
```

Use stable error categories:

```text
AccessDenied
ServiceUnavailable
PipeAuthFailed
EngineConfigInvalid
EngineSpawnFailed
WintunMissing
AdapterCreateFailed
RouteApplyFailed
DnsApplyFailed
WfpApplyFailed
ReadinessTimeout
AvQuarantineSuspected
```

---

## 15. Migration plan with milestones

### Milestone 0: Immediate telemetry and safety patches

Keep current behavior but make it measurable and safer.

Ship:

```text
redacted phase timings
operation-id propagation into Rust backend
replace tasklist readiness with process handle where possible
stop using .bat where possible
write runtime files under ProgramData if service/elevated context exists
avoid broad taskkill when you can identify owned process
add "Full Computer components not installed" UX state
```

Also add a hard warning internally: every `runas` path is legacy.

### Milestone 1: Service MVP

Goal: remove per-connect UAC for users who installed the service.

Implement:

```text
doodleray-tunnel-service.exe
SCM install/uninstall command
named pipe with restrictive ACL
StartTun / StopTun / GetStatus
single active tunnel lock
service-owned sing-box process
service-owned xray process for Xray+TUN
job object per tunnel
ProgramData runtime config
no .bat
no taskkill by image name
basic status phases
```

Wire production:

```text
vpn_connect:
  if service installed and healthy:
    use service
  else:
    show "Install Full Computer components" or use legacy runas fallback

vpn_disconnect:
  if service owns tunnel:
    StopTunnel over pipe
  else:
    legacy cleanup
```

Do not remove the legacy path until service telemetry is healthy.

### Milestone 2: Secure IPC hardening

Add:

```text
local group DoodleRay VPN Users
token-based caller authorization
message size limits
schema version negotiation
operation/tunnel generation checks
deny unknown fields
strict path canonicalization
no arbitrary executable paths
profile_ref instead of raw JSON where possible
redacted audit log
pipe fuzz tests
```

Split pipes if useful:

```text
Control pipe: strict
Status pipe: redacted/read-only
```

### Milestone 3: Lifecycle and cleanup hardening

Add:

```text
persistent DoodleRay Wintun adapter
resource ledger
startup reconciliation
sleep/resume handling
network-change handling
exact route cleanup
compare-and-swap DNS restore
WFP provider/sublayer ownership
dynamic WFP for ordinary connected state
persistent WFP only for explicit kill switch
service failure recovery
engine crash detection and restart policy
```

### Milestone 4: Installer/updater integration

Add:

```text
service installation in normal installer
engine binary signature/hash validation
locked Program Files ACLs
locked ProgramData ACLs
service SID
service recovery actions
upgrade protocol compatibility
rollback-safe update
uninstall cleanup
repair flow if service missing/broken
```

At this point, the legacy `ShellExecuteW("runas")` path should be hidden behind an emergency fallback or removed from default UX.

### Milestone 5: Full release test plan

Run the matrix below before making service mode default.

---

## 16. Windows test matrix

### OS and privilege

```text
Windows 10, supported x64 builds
Windows 11, supported x64 builds
Admin installing, standard user running
Standard user with admin credentials during install
UAC default
UAC maximum
UAC disabled / Admin Approval Mode differences
Built-in Administrator account
Domain-joined machine
Microsoft account user
Local account user
Fast User Switching
RDP session
Multiple users logged in
```

### Service lifecycle

```text
service not installed
service installed but stopped
service disabled by admin
service old version
service newer than UI
service crashes during connect
service crashes during disconnect
service killed while connected
SCM restarts service
machine reboot while connected
uninstall while connected
repair install
```

### Wintun and adapters

```text
no Wintun present
old Wintun present
DoodleRay adapter missing
DoodleRay adapter disabled
stale DoodleRay adapter
multiple Wintun adapters from other VPNs
OpenVPN/WireGuard installed
Hyper-V adapters
WSL adapters
IPv6 enabled
IPv6 disabled
no physical network
Wi-Fi to Ethernet switch
VPN competing route metrics
```

### Engines

```text
sing-box missing
sing-box quarantined by AV
sing-box invalid signature/hash
xray missing
xray port conflict
xray starts but SOCKS port never opens
sing-box config invalid
sing-box starts but TUN never becomes ready
engine crashes after Connected
engine produces huge logs
engine hangs on stop
```

### Network and kill switch

```text
kill switch off
kill switch on while disconnected
kill switch on during failed connect
DNS server unreachable
DNS changed externally while connected
route changed externally while connected
IPv6 leak tests
DNS leak tests
WebRTC/local network behavior
LAN allowed
LAN blocked
captive portal
sleep/resume
hibernate/resume
network disconnect/reconnect
```

### Security

```text
unauthorized user connects to pipe
authorized user starts tunnel
non-owner user tries to stop tunnel
admin stops tunnel
malformed IPC payload
oversized IPC payload
replayed StartTunnel
stale StopTunnel
path traversal in profile/rule names
reparse point/symlink attacks in ProgramData
user-writable engine path attempt
DLL planting attempt
fake sing-box.exe in PATH
pipe squatting before service start
service binary ACL verification
support bundle redaction
logs contain no secrets
```

### Updates

```text
UI updated, service old
service updated, UI old
engine updated only
failed update halfway
rollback after failed service replacement
update while connected
update while kill switch enabled
non-admin tries update
enterprise deployment/repair
```

### Performance

Measure cold and warm:

```text
service cold start connect
service already running connect
first connect after install
connect after adapter exists
connect after sleep/resume
rapid connect/disconnect loops
cancel during Connecting
disconnect during StartingXray
disconnect during ApplyingRoutes
```

Target metrics should be per phase, with p50/p95/p99 thresholds.

---

## 17. Common mistakes to avoid

Do not use `runas` for normal connect.

Do not launch `.bat` or `cmd.exe` for privileged tunnel startup.

Do not store privileged runtime config in `%TEMP%`.

Do not kill by image name.

Do not let the UI own half of a Full Computer tunnel.

Do not trust the frontend to enforce “one active engine.”

Do not expose sing-box/xray control APIs beyond `127.0.0.1`.

Do not grant pipe access to `Everyone`.

Do not use a pipe “shared secret” as the only auth mechanism.

Do not accept arbitrary executable paths or arbitrary command-line flags over IPC.

Do not log generated VPN configs.

Do not add persistent WFP kill-switch filters without a recovery and uninstall story.

Do not treat process-name split tunneling as a security guarantee.

Do not ignore IPv6.

Do not remove all Wintun adapters.

Do not blindly restore DNS if another app changed it.

Do not mark the UI “Connected” before routes, DNS, WFP filters, and engine health are verified.

Do not run the Tauri/webview app elevated just to avoid UAC.

---

## 18. Proposed target architecture

```text
┌──────────────────────────────────────────────────────────┐
│ DoodleRay UI / Tauri app                                 │
│ non-elevated                                             │
│                                                          │
│ - React/Zustand UX                                       │
│ - account/profile UI                                     │
│ - Workshop rule editor                                   │
│ - System Proxy UX                                        │
│ - IPC client                                             │
└───────────────────────────┬──────────────────────────────┘
                            │ authenticated named pipe
                            ▼
┌──────────────────────────────────────────────────────────┐
│ DoodleRay Tunnel Service                                 │
│ Windows Service, LocalSystem, service SID hardened        │
│                                                          │
│ - IPC auth and command validation                         │
│ - one active tunnel state machine                         │
│ - profile secret/runtime config handling                  │
│ - Wintun adapter ownership                                │
│ - routes / DNS / WFP kill switch                          │
│ - engine process supervision                              │
│ - job objects                                             │
│ - readiness verification                                  │
│ - crash/sleep/resume reconciliation                       │
└───────────────┬──────────────────────┬───────────────────┘
                │                      │
                ▼                      ▼
       ┌────────────────┐      ┌────────────────┐
       │ xray.exe        │      │ sing-box.exe    │
       │ low/no admin if │      │ privileged TUN  │
       │ possible        │      │ bridge/direct   │
       └────────────────┘      └────────────────┘
                │                      │
                └──────────┬───────────┘
                           ▼
                  Wintun / routes / DNS / WFP
```

The key product shift is this:

```text
Today:
  "Connect" means ask Windows for elevation and try to launch a privileged process.

Target:
  "Connect" means send a small authenticated command to an already-installed tunnel service.
```

That is what will remove repeated UAC, reduce latency, give you real progress states, and make cleanup safe enough for a production Full Computer VPN.

[1]: https://learn.microsoft.com/en-us/windows/security/application-security/application-control/user-account-control/?utm_source=chatgpt.com "User Account Control overview - Windows"
[2]: https://learn.microsoft.com/en-us/windows/win32/services/service-control-manager?utm_source=chatgpt.com "Service control manager - Win32 apps"
[3]: https://learn.microsoft.com/en-us/windows/win32/services/localsystem-account?utm_source=chatgpt.com "LocalSystem Account - Win32 apps"
[4]: https://learn.microsoft.com/en-us/windows/win32/services/localservice-account?utm_source=chatgpt.com "LocalService Account - Win32 apps"
[5]: https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/sc-create?utm_source=chatgpt.com "sc.exe create"
[6]: https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_sid_info?utm_source=chatgpt.com "SERVICE_SID_INFO (winsvc.h) - Win32 apps"
[7]: https://github.com/WireGuard/wintun?utm_source=chatgpt.com "Wintun Network Adapter"
[8]: https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights?utm_source=chatgpt.com "Named Pipe Security and Access Rights - Win32 apps"
[9]: https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-impersonatenamedpipeclient?utm_source=chatgpt.com "ImpersonateNamedPipeClient function (namedpipeapi.h)"
[10]: https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-checktokenmembership?utm_source=chatgpt.com "CheckTokenMembership function (securitybaseapi.h)"
[11]: https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid?utm_source=chatgpt.com "GetNamedPipeClientProcessId function (winbase.h)"
[12]: https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects?utm_source=chatgpt.com "Job Objects - Win32 apps"
[13]: https://devblogs.microsoft.com/oldnewthing/20230209-00/?p=107812&utm_source=chatgpt.com "A more direct and mistake-free way of creating a process in ..."
[14]: https://sing-box.sagernet.org/configuration/?utm_source=chatgpt.com "Introduction - sing-box"
[15]: https://sing-box.sagernet.org/configuration/inbound/tun/?utm_source=chatgpt.com "Tun - sing-box"
[16]: https://learn.microsoft.com/en-us/windows/win32/iphlp/ip-helper-start-page?utm_source=chatgpt.com "IP Helper - Win32 apps"
[17]: https://learn.microsoft.com/en-us/windows/win32/fwp/windows-filtering-platform-start-page?utm_source=chatgpt.com "Windows Filtering Platform - Win32 apps"
[18]: https://learn.microsoft.com/en-us/windows/win32/fwp/best-practices?utm_source=chatgpt.com "Best Practices (Windows Filtering Platform) - Win32 apps"
[19]: https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_failure_actionsa?utm_source=chatgpt.com "SERVICE_FAILURE_ACTIONSA (winsvc.h) - Win32 apps"
[20]: https://learn.microsoft.com/en-us/windows/win32/services/service-control-handler-function?utm_source=chatgpt.com "Service Control Handler Function - Win32 apps"
[21]: https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_preshutdown_info?utm_source=chatgpt.com "SERVICE_PRESHUTDOWN_IN..."
[22]: https://sing-box.sagernet.org/configuration/route/rule/?utm_source=chatgpt.com "Route Rule - sing-box - SagerNet"
