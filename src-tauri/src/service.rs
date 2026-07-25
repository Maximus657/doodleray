// Kept outside src/bin so Tauri's desktop bundler cannot mistake this
// separately built Windows service for a companion macOS application binary.
#[cfg(windows)]
mod windows_service_main {
    use serde_json::Value;
    use std::collections::hash_map::DefaultHasher;
    use std::ffi::OsString;
    use std::fs::OpenOptions;
    use std::hash::{Hash, Hasher};
    use std::mem::{size_of, zeroed};
    use std::net::{Ipv4Addr, Ipv6Addr, TcpStream};
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    };
    use std::time::{Duration, Instant};
    use tauri_app_lib::tunnel_service::{
        runtime_root, session_marker_path, SessionMarker, StartTunnelRequest, StopTunnelRequest,
        TunnelCommand, TunnelDiagnostics, TunnelEffectiveState, TunnelEngineKind,
        TunnelHealthVerdict, TunnelResponse, TunnelState, TunnelStatus, TUNNEL_PIPE_NAME,
        TUNNEL_PROTOCOL_VERSION, TUNNEL_SERVICE_DISPLAY_NAME, TUNNEL_SERVICE_NAME,
    };
    use tauri_app_lib::windows_net;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::{PipeMode, ServerOptions};
    use windows_service::define_windows_service;
    use windows_service::service::{
        PowerEventParam, ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl,
        ServiceExitCode, ServiceInfo, ServiceSidType, ServiceStartType, ServiceState,
        ServiceStatus, ServiceType,
    };
    use windows_service::service_control_handler::{
        self, ServiceControlHandlerResult, ServiceStatusHandle,
    };
    use windows_service::service_dispatcher;
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        LookupAccountNameW, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, SID_NAME_USE,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Pipes::GetNamedPipeClientProcessId;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    static STATE: OnceLock<Mutex<TunnelRuntime>> = OnceLock::new();
    static SINGBOX_VALIDATION_CACHE: OnceLock<Mutex<Option<SingboxValidationCache>>> =
        OnceLock::new();
    static XRAY_VALIDATION_CACHE: OnceLock<Mutex<Option<u64>>> = OnceLock::new();
    static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
    static OP_GENERATION: AtomicU64 = AtomicU64::new(0);
    const PIPE_WORKERS: usize = 16;
    const VPN_USERS_GROUP: &str = "DoodleRay VPN Users";

    define_windows_service!(ffi_service_main, service_main);

    struct TunnelRuntime {
        state: TunnelState,
        effective_state: TunnelEffectiveState,
        health_verdict: TunnelHealthVerdict,
        phase: Option<String>,
        active_op_id: Option<String>,
        service_generation: u64,
        previous_generation: Option<u64>,
        engine_kind: Option<TunnelEngineKind>,
        runtime_socks_port: Option<u16>,
        runtime_http_port: Option<u16>,
        runtime_api_port: Option<u16>,
        adapter_alias: Option<String>,
        adapter_ifindex: Option<u32>,
        route_ready: Option<bool>,
        dns_ready: Option<bool>,
        proxy_compat_state: Option<String>,
        fatal_checks: Vec<String>,
        degraded_checks: Vec<String>,
        warning_checks: Vec<String>,
        route_explanations: Vec<String>,
        endpoint_bypass_checks: Vec<String>,
        last_repair_action: Option<String>,
        network_event_seq: u64,
        previous_unclean_shutdown: Option<String>,
        ghost_scan_done_since_start: bool,
        last_start_request: Option<StartTunnelRequest>,
        last_rotation_seq: u64,
        error: Option<String>,
        timings_ms: Vec<(String, u64)>,
        powershell_fallback_count: u32,
        singbox_check_ms: Option<u64>,
        xray_spawn_ms: Option<u64>,
        xray_check_ms: Option<u64>,
        adapter_probe_backend: Option<String>,
        route_probe_backend: Option<String>,
        native_probe_ms: Vec<(String, u64)>,
        fallback_probe_ms: Vec<(String, u64)>,
        xray: Option<Child>,
        singbox: Option<Child>,
        job: Option<JobHandle>,
    }

    impl Default for TunnelRuntime {
        fn default() -> Self {
            Self {
                state: TunnelState::Disconnected,
                effective_state: TunnelEffectiveState::Idle,
                health_verdict: TunnelHealthVerdict::Failed,
                phase: None,
                active_op_id: None,
                service_generation: 0,
                previous_generation: None,
                engine_kind: None,
                runtime_socks_port: None,
                runtime_http_port: None,
                runtime_api_port: None,
                adapter_alias: None,
                adapter_ifindex: None,
                route_ready: None,
                dns_ready: None,
                proxy_compat_state: None,
                fatal_checks: Vec::new(),
                degraded_checks: Vec::new(),
                warning_checks: Vec::new(),
                route_explanations: Vec::new(),
                endpoint_bypass_checks: Vec::new(),
                last_repair_action: None,
                network_event_seq: 0,
                previous_unclean_shutdown: None,
                ghost_scan_done_since_start: false,
                last_start_request: None,
                last_rotation_seq: 0,
                error: None,
                timings_ms: Vec::new(),
                powershell_fallback_count: 0,
                singbox_check_ms: None,
                xray_spawn_ms: None,
                xray_check_ms: None,
                adapter_probe_backend: None,
                route_probe_backend: None,
                native_probe_ms: Vec::new(),
                fallback_probe_ms: Vec::new(),
                xray: None,
                singbox: None,
                job: None,
            }
        }
    }

    #[derive(Clone, Copy)]
    struct SingboxValidationCache {
        config_hash: u64,
    }

    struct JobHandle(HANDLE);

    unsafe impl Send for JobHandle {}

    impl JobHandle {
        fn raw(&self) -> HANDLE {
            self.0
        }
    }

    impl Drop for JobHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }

    struct PipeSecurityAttributes {
        descriptor: PSECURITY_DESCRIPTOR,
        attrs: SECURITY_ATTRIBUTES,
    }

    unsafe impl Send for PipeSecurityAttributes {}

    impl PipeSecurityAttributes {
        fn as_mut_ptr(&mut self) -> *mut std::ffi::c_void {
            &mut self.attrs as *mut _ as *mut std::ffi::c_void
        }
    }

    impl Drop for PipeSecurityAttributes {
        fn drop(&mut self) {
            if !self.descriptor.is_null() {
                unsafe {
                    LocalFree(self.descriptor);
                }
            }
        }
    }

    fn state() -> &'static Mutex<TunnelRuntime> {
        STATE.get_or_init(|| Mutex::new(TunnelRuntime::default()))
    }

    pub fn main_entry() -> Result<(), Box<dyn std::error::Error>> {
        let mut args = std::env::args().skip(1);
        match args.next().as_deref() {
            Some("run-service") => {
                service_dispatcher::start(TUNNEL_SERVICE_NAME, ffi_service_main)?;
            }
            Some("install") => install_service()?,
            Some("uninstall") => uninstall_service()?,
            Some("start") => start_service()?,
            Some("stop") => stop_service()?,
            Some("status") => print_service_status()?,
            Some("diagnostics") => print_service_diagnostics()?,
            Some("prepare-update") => prepare_service_update()?,
            _ => {
                eprintln!(
                    "Usage: DoodleRayService.exe <run-service|install|uninstall|start|stop|status|diagnostics|prepare-update>"
                );
            }
        }
        Ok(())
    }

    fn service_main(_arguments: Vec<OsString>) {
        if let Err(e) = run_service() {
            eprintln!("DoodleRay tunnel service failed: {}", e);
        }
    }

    fn run_service() -> windows_service::Result<()> {
        STOP_REQUESTED.store(false, Ordering::SeqCst);
        let stopped = Arc::new(AtomicBool::new(false));
        let stopped_for_handler = stopped.clone();
        let status_handle =
            service_control_handler::register(TUNNEL_SERVICE_NAME, move |control_event| {
                match control_event {
                    ServiceControl::Stop | ServiceControl::Interrogate => {
                        if matches!(control_event, ServiceControl::Stop) {
                            stopped_for_handler.store(true, Ordering::SeqCst);
                        }
                        ServiceControlHandlerResult::NoError
                    }
                    ServiceControl::ParamChange
                    | ServiceControl::NetBindAdd
                    | ServiceControl::NetBindDisable
                    | ServiceControl::NetBindEnable
                    | ServiceControl::NetBindRemove => {
                        mark_network_event_suspect("windows_network_change");
                        schedule_runtime_reassert("windows_network_change");
                        ServiceControlHandlerResult::NoError
                    }
                    ServiceControl::PowerEvent(event) => {
                        let reason = match event {
                            PowerEventParam::ResumeAutomatic
                            | PowerEventParam::ResumeSuspend
                            | PowerEventParam::ResumeCritical => "windows_power_resume",
                            PowerEventParam::Suspend => "windows_power_suspend",
                            _ => "windows_power_event",
                        };
                        mark_network_event_suspect(reason);
                        if !matches!(
                            event,
                            PowerEventParam::Suspend | PowerEventParam::QuerySuspend
                        ) {
                            schedule_runtime_reassert(reason);
                        }
                        ServiceControlHandlerResult::NoError
                    }
                    _ => ServiceControlHandlerResult::NotImplemented,
                }
            })?;

        set_service_status(&status_handle, ServiceState::Running, 0)?;
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|_| windows_service::Error::Winapi(std::io::Error::last_os_error()))?;
        runtime.block_on(async move {
            if let Err(e) = secure_runtime_dirs() {
                eprintln!("failed to secure runtime dirs: {}", e);
                STOP_REQUESTED.store(true, Ordering::SeqCst);
                return;
            }
            detect_previous_unclean_shutdown();
            let mut workers = Vec::with_capacity(PIPE_WORKERS);
            for _ in 0..PIPE_WORKERS {
                let worker_stopped = stopped.clone();
                workers.push(tokio::spawn(async move {
                    while !worker_stopped.load(Ordering::SeqCst)
                        && !STOP_REQUESTED.load(Ordering::SeqCst)
                    {
                        if let Err(e) = serve_once().await {
                            eprintln!("pipe serve error: {}", e);
                            tokio::time::sleep(Duration::from_millis(250)).await;
                        }
                    }
                }));
            }
            while !stopped.load(Ordering::SeqCst) && !STOP_REQUESTED.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let _ = set_service_status(&status_handle, ServiceState::StopPending, 1);
            for worker in workers {
                worker.abort();
            }
            let _ = stop_owned_processes("service_stop");
        });
        set_service_status(&status_handle, ServiceState::Stopped, 0)?;
        Ok(())
    }

    fn set_service_status(
        handle: &ServiceStatusHandle,
        state: ServiceState,
        checkpoint: u32,
    ) -> windows_service::Result<()> {
        let controls_accepted = if state == ServiceState::Running {
            ServiceControlAccept::STOP
                | ServiceControlAccept::POWER_EVENT
                | ServiceControlAccept::PARAM_CHANGE
                | ServiceControlAccept::NETBIND_CHANGE
        } else {
            ServiceControlAccept::empty()
        };
        handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint,
            wait_hint: if state == ServiceState::StopPending {
                Duration::from_secs(10)
            } else {
                Duration::from_secs(2)
            },
            process_id: None,
        })
    }

    fn mark_network_event_suspect(reason: &str) {
        if let Ok(mut runtime) = state().lock() {
            runtime.network_event_seq = runtime.network_event_seq.saturating_add(1);
            runtime.last_repair_action = Some(reason.into());
            let check = format!(
                "Windows event observed while service was running: {}",
                reason
            );
            if !runtime
                .warning_checks
                .iter()
                .any(|existing| existing == &check)
            {
                runtime.warning_checks.push(check);
            }
            if matches!(runtime.state, TunnelState::Connected) {
                runtime.effective_state = TunnelEffectiveState::Suspect;
                runtime.health_verdict = TunnelHealthVerdict::Repairing;
                runtime.phase = Some(reason.into());
            }
        }
        log_service_event(&format!("network/power event marked suspect: {}", reason));
    }

    fn detect_previous_unclean_shutdown() {
        let marker_path = session_marker_path();
        let raw = match std::fs::read_to_string(&marker_path) {
            Ok(raw) => raw,
            Err(_) => return,
        };
        let detail = SessionMarker::parse(&raw)
            .map(|marker| marker.summary())
            .unwrap_or_else(|| "previous session ended uncleanly: marker unreadable".into());
        let _ = std::fs::remove_file(&marker_path);
        if let Ok(mut runtime) = state().lock() {
            runtime.previous_unclean_shutdown = Some(detail.clone());
        }
        log_service_event(&format!("unclean shutdown marker consumed: {}", detail));
    }

    fn write_session_marker(op_id: &str, generation: u64) {
        let marker = SessionMarker {
            op_id: sanitize_id(op_id),
            generation,
            started_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis().min(u64::MAX as u128) as u64)
                .unwrap_or(0),
        };
        if let Err(error) = std::fs::write(session_marker_path(), marker.to_line()) {
            log_service_event(&format!("failed to write session marker: {}", error));
        }
    }

    fn clear_session_marker() {
        let marker_path = session_marker_path();
        if marker_path.exists() {
            if let Err(error) = std::fs::remove_file(&marker_path) {
                log_service_event(&format!("failed to clear session marker: {}", error));
            }
        }
    }

    fn schedule_runtime_reassert(reason: &str) {
        let reason = reason.to_string();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1500));
            match repair_connected_runtime(&reason, None) {
                Ok(status) => log_service_event(&format!(
                    "runtime reassert finished reason={} state={:?} effective={:?} verdict={:?}",
                    reason, status.state, status.effective_state, status.health_verdict
                )),
                Err(error) => {
                    log_service_event(&format!(
                        "runtime reassert failed reason={} error={}",
                        reason,
                        redact(&error)
                    ));
                    rotate_children_after_failed_reassert(&reason);
                }
            }
        });
    }

    /// In-place reassert could not fix the connected runtime (typically the
    /// children are bound to a dead interface after sleep/wake). Rotate the
    /// service-owned child generation once per network event burst using the
    /// stored start request, instead of leaving the user to reboot.
    fn rotate_children_after_failed_reassert(reason: &str) {
        let (request, generation) = {
            let mut runtime = state().lock().unwrap();
            if !matches!(runtime.state, TunnelState::Connected) {
                return;
            }
            if runtime.last_rotation_seq >= runtime.network_event_seq {
                return;
            }
            let Some(request) = runtime.last_start_request.clone() else {
                return;
            };
            runtime.last_rotation_seq = runtime.network_event_seq;
            (request, OP_GENERATION.load(Ordering::SeqCst))
        };
        log_service_event(&format!(
            "rotating child generation after failed reassert: {}",
            reason
        ));
        let rotated_note = format!(
            "child generation rotated after Windows network/power event: {}",
            reason
        );
        match start_tunnel(request, generation) {
            Ok(()) => {
                if let Ok(mut runtime) = state().lock() {
                    runtime.warning_checks.push(rotated_note);
                }
                log_service_event("child generation rotation succeeded");
            }
            Err(message) => {
                if is_current_generation(generation) {
                    let _ = stop_owned_processes("failed_cleanup");
                    if is_current_generation(generation) {
                        set_failed(&message);
                    } else {
                        log_service_event(
                            "ignored stale rotate failure after newer tunnel operation",
                        );
                    }
                }
            }
        }
    }

    fn pipe_security_attributes() -> Result<PipeSecurityAttributes, String> {
        let group_sid = account_sid_sddl(VPN_USERS_GROUP)?;
        // BU access keeps the app usable immediately after install. The service still validates
        // the connected client executable path before accepting any command.
        let sddl = format!(
            "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;BU)(A;;GRGW;;;{})",
            group_sid
        );
        let sddl_w: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl_w.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 || descriptor.is_null() {
            return Err(format!(
                "ConvertStringSecurityDescriptorToSecurityDescriptorW failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        Ok(PipeSecurityAttributes {
            descriptor,
            attrs: SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor,
                bInheritHandle: 0,
            },
        })
    }

    fn account_sid_sddl(account: &str) -> Result<String, String> {
        let account_w: Vec<u16> = account.encode_utf16().chain(std::iter::once(0)).collect();
        let mut sid_len = 0u32;
        let mut domain_len = 0u32;
        let mut sid_type: SID_NAME_USE = 0;
        unsafe {
            LookupAccountNameW(
                std::ptr::null(),
                account_w.as_ptr(),
                std::ptr::null_mut(),
                &mut sid_len,
                std::ptr::null_mut(),
                &mut domain_len,
                &mut sid_type,
            );
        }
        if sid_len == 0 {
            return Err(format!("Failed to resolve SID size for {}", account));
        }

        let mut sid = vec![0u8; sid_len as usize];
        let mut domain = vec![0u16; domain_len.saturating_add(1) as usize];
        let ok = unsafe {
            LookupAccountNameW(
                std::ptr::null(),
                account_w.as_ptr(),
                sid.as_mut_ptr() as PSID,
                &mut sid_len,
                domain.as_mut_ptr(),
                &mut domain_len,
                &mut sid_type,
            )
        };
        if ok == 0 {
            return Err(format!(
                "LookupAccountNameW({}) failed: {}",
                account,
                std::io::Error::last_os_error()
            ));
        }

        let mut sid_string_ptr: *mut u16 = std::ptr::null_mut();
        let ok = unsafe { ConvertSidToStringSidW(sid.as_mut_ptr() as PSID, &mut sid_string_ptr) };
        if ok == 0 || sid_string_ptr.is_null() {
            return Err(format!(
                "ConvertSidToStringSidW({}) failed: {}",
                account,
                std::io::Error::last_os_error()
            ));
        }
        let mut len = 0usize;
        unsafe {
            while *sid_string_ptr.add(len) != 0 {
                len += 1;
            }
        }
        let sid_string =
            unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(sid_string_ptr, len)) };
        unsafe {
            LocalFree(sid_string_ptr as _);
        }
        Ok(sid_string)
    }

    async fn serve_once() -> Result<(), String> {
        let mut pipe_attrs = pipe_security_attributes()?;
        let mut pipe = unsafe {
            ServerOptions::new()
                .first_pipe_instance(false)
                .pipe_mode(PipeMode::Byte)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(TUNNEL_PIPE_NAME, pipe_attrs.as_mut_ptr())
                .map_err(|e| format!("create pipe: {}", e))?
        };

        match tokio::time::timeout(Duration::from_millis(500), pipe.connect()).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(format!("pipe connect: {}", e)),
            Err(_) => return Ok(()),
        }
        if let Err(message) = validate_pipe_client(pipe.as_raw_handle() as HANDLE) {
            log_service_event(&format!("pipe client validation rejected: {}", message));
            let response = TunnelResponse::Error {
                message: "Tunnel service rejected this client".into(),
            };
            let payload =
                serde_json::to_vec(&response).map_err(|e| format!("encode response: {}", e))?;
            let _ = write_ipc_frame(&mut pipe, &payload).await;
            return Err("pipe client validation failed".into());
        }
        let buffer = read_ipc_frame(&mut pipe).await?;
        let response = match std::panic::catch_unwind(|| {
            match serde_json::from_slice::<TunnelCommand>(&buffer) {
                Ok(command) => handle_command(command),
                Err(e) => TunnelResponse::Error {
                    message: format!("Invalid tunnel command: {}", e),
                },
            }
        }) {
            Ok(response) => response,
            Err(_) => {
                log_service_event("command handler panicked");
                TunnelResponse::Error {
                    message: "Tunnel service command handler failed internally".into(),
                }
            }
        };
        let payload =
            serde_json::to_vec(&response).map_err(|e| format!("encode response: {}", e))?;
        write_ipc_frame(&mut pipe, &payload).await?;
        Ok(())
    }

    async fn read_ipc_frame(
        pipe: &mut tokio::net::windows::named_pipe::NamedPipeServer,
    ) -> Result<Vec<u8>, String> {
        let mut len_buf = [0u8; 4];
        match tokio::time::timeout(Duration::from_secs(5), pipe.read_exact(&mut len_buf)).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                let message = format!("pipe read frame length failed: {}", e);
                log_service_event(&message);
                return Err(message);
            }
            Err(_) => {
                log_service_event("pipe read frame length timeout");
                return Err("pipe read frame length timeout".into());
            }
        }
        let len = u32::from_le_bytes(len_buf) as usize;
        if len == 0 || len > 4 * 1024 * 1024 {
            let message = format!("invalid pipe frame length: {}", len);
            log_service_event(&message);
            return Err(message);
        }
        let mut buffer = vec![0; len];
        match tokio::time::timeout(Duration::from_secs(10), pipe.read_exact(&mut buffer)).await {
            Ok(Ok(_)) => Ok(buffer),
            Ok(Err(e)) => {
                let message = format!("pipe read frame payload failed: {}", e);
                log_service_event(&message);
                Err(message)
            }
            Err(_) => {
                log_service_event("pipe read frame payload timeout");
                Err("pipe read frame payload timeout".into())
            }
        }
    }

    async fn write_ipc_frame(
        pipe: &mut tokio::net::windows::named_pipe::NamedPipeServer,
        payload: &[u8],
    ) -> Result<(), String> {
        if payload.len() > 4 * 1024 * 1024 {
            return Err("pipe response payload is too large".into());
        }
        pipe.write_all(&(payload.len() as u32).to_le_bytes())
            .await
            .map_err(|e| format!("pipe write frame length: {}", e))?;
        pipe.write_all(payload)
            .await
            .map_err(|e| format!("pipe write frame payload: {}", e))?;
        pipe.flush()
            .await
            .map_err(|e| format!("pipe flush: {}", e))?;
        Ok(())
    }

    fn log_service_event(message: &str) {
        let root = runtime_root()
            .parent()
            .map(|path| path.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData\DoodleRay"));
        let _ = std::fs::create_dir_all(&root);
        let path = root.join("service.log");
        let timestamp = chrono_like_timestamp();
        let line = format!("{} {}\n", timestamp, message);
        let _ = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, line.as_bytes()));
    }

    fn chrono_like_timestamp() -> String {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| format!("unix_ms={}", duration.as_millis()))
            .unwrap_or_else(|_| "unix_ms=0".into())
    }

    fn validate_pipe_client(pipe: HANDLE) -> Result<(), String> {
        let mut pid = 0u32;
        let ok = unsafe { GetNamedPipeClientProcessId(pipe, &mut pid) };
        if ok == 0 || pid == 0 {
            return Err(format!(
                "GetNamedPipeClientProcessId failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let client_path = process_image_path(pid)?;
        let service_dir = exe_dir()?.canonicalize().map_err(|e| e.to_string())?;
        let client_dir = client_path
            .parent()
            .ok_or("Pipe client path has no parent directory")?
            .canonicalize()
            .map_err(|e| format!("canonicalize client dir: {}", e))?;
        if client_dir != service_dir {
            return Err(format!(
                "Rejected tunnel service client outside install dir: {}",
                client_path.display()
            ));
        }

        let client_name = client_path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(
            client_name.as_str(),
            "doodleray.exe" | "doodlerayservice.exe"
        ) {
            Ok(())
        } else {
            Err(format!(
                "Rejected unexpected tunnel service client executable: {}",
                client_path.display()
            ))
        }
    }

    fn process_image_path(pid: u32) -> Result<PathBuf, String> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return Err(format!(
                "OpenProcess({}) failed: {}",
                pid,
                std::io::Error::last_os_error()
            ));
        }

        let mut buffer = vec![0u16; 32768];
        let mut len = buffer.len() as u32;
        let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut len) };
        unsafe {
            CloseHandle(handle);
        }
        if ok == 0 {
            return Err(format!(
                "QueryFullProcessImageNameW({}) failed: {}",
                pid,
                std::io::Error::last_os_error()
            ));
        }
        buffer.truncate(len as usize);
        Ok(PathBuf::from(String::from_utf16_lossy(&buffer)))
    }

    fn handle_command(command: TunnelCommand) -> TunnelResponse {
        match command {
            TunnelCommand::Hello(hello) => {
                if hello.protocol_version != TUNNEL_PROTOCOL_VERSION {
                    return TunnelResponse::Error {
                        message: format!("Unsupported tunnel protocol {}", hello.protocol_version),
                    };
                }
                TunnelResponse::Status(status_snapshot())
            }
            TunnelCommand::GetStatus => TunnelResponse::Status(status_snapshot()),
            TunnelCommand::GetDiagnostics => TunnelResponse::Diagnostics(collect_diagnostics()),
            TunnelCommand::StartTunnel(request) => {
                log_service_event(&format!(
                    "StartTunnel accepted op_id={} engine={:?} label={}",
                    sanitize_id(&request.op_id),
                    request.engine_kind,
                    request.redacted_label
                ));
                if let Some(response) = try_warm_reassert_start(&request) {
                    return response;
                }
                let busy = {
                    let runtime = state().lock().unwrap();
                    matches!(
                        runtime.state,
                        TunnelState::Connecting | TunnelState::Disconnecting
                    )
                };
                if busy {
                    return TunnelResponse::Status(status_snapshot());
                }
                let generation = OP_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
                {
                    let mut runtime = state().lock().unwrap();
                    runtime.previous_generation = Some(runtime.service_generation);
                    runtime.state = TunnelState::Connecting;
                    runtime.effective_state = TunnelEffectiveState::Preparing;
                    runtime.health_verdict = TunnelHealthVerdict::Repairing;
                    runtime.phase = Some("queued".into());
                    runtime.active_op_id = Some(request.op_id.clone());
                    runtime.service_generation = generation;
                    runtime.engine_kind = Some(request.engine_kind.clone());
                    runtime.runtime_socks_port = Some(request.socks_port);
                    runtime.runtime_http_port = Some(request.http_port);
                    runtime.runtime_api_port = None;
                    runtime.adapter_alias = None;
                    runtime.adapter_ifindex = None;
                    runtime.route_ready = None;
                    runtime.dns_ready = None;
                    runtime.proxy_compat_state = Some("pending".into());
                    runtime.fatal_checks.clear();
                    runtime.degraded_checks.clear();
                    runtime.warning_checks.clear();
                    runtime.route_explanations = vec!["route readiness pending".into()];
                    runtime.endpoint_bypass_checks =
                        vec!["endpoint bypass readiness pending".into()];
                    runtime.last_repair_action = None;
                    runtime.error = None;
                    runtime.timings_ms.clear();
                    // Kept for child-generation rotation after a failed
                    // network/power reassert; configs are service-local only.
                    runtime.last_start_request = Some(request.clone());
                }
                std::thread::spawn(move || {
                    if let Err(message) = start_tunnel(request, generation) {
                        if is_current_generation(generation) {
                            let _ = stop_owned_processes("failed_cleanup");
                            if is_current_generation(generation) {
                                set_failed(&message);
                            } else {
                                log_service_event(
                                    "ignored stale start failure after newer tunnel operation",
                                );
                            }
                        }
                    }
                });
                TunnelResponse::Status(status_snapshot())
            }
            TunnelCommand::StopTunnel(request) => match stop_tunnel(request) {
                Ok(status) => {
                    std::thread::spawn(|| {
                        std::thread::sleep(Duration::from_millis(150));
                        STOP_REQUESTED.store(true, Ordering::SeqCst);
                    });
                    TunnelResponse::Status(status)
                }
                Err(message) => TunnelResponse::Error { message },
            },
            TunnelCommand::ReportProxyCompatibility(report) => {
                let mut runtime = state().lock().unwrap();
                if let Some(op_id) = report.op_id.as_deref() {
                    if runtime.active_op_id.as_deref() != Some(op_id) {
                        drop(runtime);
                        return TunnelResponse::Status(status_snapshot());
                    }
                }
                let detail = redact(&report.detail);
                if report.ok {
                    if detail.contains("direct app exclusions") {
                        runtime.proxy_compat_state =
                            Some("disabled_for_direct_app_exclusions".into());
                    } else {
                        runtime.proxy_compat_state = Some("ready".into());
                    }
                    runtime
                        .degraded_checks
                        .retain(|check| !check.contains("Windows proxy compatibility"));
                    runtime.route_explanations.push(detail);
                } else {
                    runtime.proxy_compat_state = Some("degraded".into());
                    if !runtime
                        .degraded_checks
                        .iter()
                        .any(|check| check.contains("Windows proxy compatibility"))
                    {
                        runtime.degraded_checks.push(detail);
                    }
                }
                drop(runtime);
                TunnelResponse::Status(status_snapshot())
            }
            TunnelCommand::RepairRuntime(request) => {
                match repair_connected_runtime(&request.reason, request.op_id.as_deref()) {
                    Ok(status) => TunnelResponse::Status(status),
                    Err(message) => TunnelResponse::Error { message },
                }
            }
            TunnelCommand::PrepareForUpdate => {
                log_service_event("PrepareForUpdate requested");
                OP_GENERATION.fetch_add(1, Ordering::SeqCst);
                {
                    let mut runtime = state().lock().unwrap();
                    runtime.effective_state = TunnelEffectiveState::Repairing;
                    runtime.health_verdict = TunnelHealthVerdict::Repairing;
                    runtime.last_repair_action = Some("prepare_for_update".into());
                }
                match stop_owned_processes("prepare_update") {
                    Ok(_) => {
                        clear_timings();
                        STOP_REQUESTED.store(true, Ordering::SeqCst);
                        TunnelResponse::Status(status_snapshot())
                    }
                    Err(message) => TunnelResponse::Error { message },
                }
            }
        }
    }

    fn try_warm_reassert_start(request: &StartTunnelRequest) -> Option<TunnelResponse> {
        let generation = {
            let mut runtime = state().lock().unwrap();
            refresh_connected_process_state(&mut runtime);
            refresh_runtime_verdict(&mut runtime);
            if !matches!(runtime.state, TunnelState::Connected) {
                return None;
            }
            let previous = runtime.last_start_request.as_ref()?;
            if !start_requests_share_runtime(previous, request) {
                return None;
            }
            runtime.active_op_id = Some(request.op_id.clone());
            runtime.last_start_request = Some(request.clone());
            runtime.phase = Some("warm_reassert_queued".into());
            runtime.last_repair_action = Some("warm_reassert_start".into());
            runtime.route_ready = None;
            runtime.dns_ready = None;
            runtime.route_explanations.push(
                "warm reconnect reused the existing protected runtime; reasserting routes, DNS, and local listeners"
                    .into(),
            );
            runtime.service_generation
        };

        log_service_event(&format!(
            "warm reassert accepted op_id={} generation={}",
            sanitize_id(&request.op_id),
            generation
        ));
        write_session_marker(&request.op_id, generation);
        Some(
            match repair_connected_runtime("warm_reassert_start", None) {
                Ok(status) => TunnelResponse::Status(status),
                Err(message) => TunnelResponse::Error { message },
            },
        )
    }

    fn start_requests_share_runtime(
        previous: &StartTunnelRequest,
        next: &StartTunnelRequest,
    ) -> bool {
        previous.engine_kind == next.engine_kind
            && previous.socks_port == next.socks_port
            && previous.http_port == next.http_port
            && previous.api_port == next.api_port
            && previous.xray_config == next.xray_config
            && previous.singbox_config == next.singbox_config
    }

    fn status_snapshot() -> TunnelStatus {
        let mut runtime = state().lock().unwrap();
        refresh_connected_process_state(&mut runtime);
        refresh_runtime_verdict(&mut runtime);
        TunnelStatus {
            protocol_version: TUNNEL_PROTOCOL_VERSION,
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            state: runtime.state.clone(),
            effective_state: runtime.effective_state.clone(),
            health_verdict: runtime.health_verdict.clone(),
            phase: runtime.phase.clone(),
            active_op_id: runtime.active_op_id.clone(),
            service_generation: runtime.service_generation,
            previous_generation: runtime.previous_generation,
            engine_kind: runtime.engine_kind.clone(),
            runtime_socks_port: runtime.runtime_socks_port,
            runtime_http_port: runtime.runtime_http_port,
            runtime_api_port: runtime.runtime_api_port,
            xray_pid: runtime.xray.as_ref().map(|child| child.id()),
            singbox_pid: runtime.singbox.as_ref().map(|child| child.id()),
            adapter_alias: runtime.adapter_alias.clone(),
            adapter_ifindex: runtime.adapter_ifindex,
            route_ready: runtime.route_ready,
            dns_ready: runtime.dns_ready,
            proxy_compat_state: runtime.proxy_compat_state.clone(),
            fatal_checks: runtime.fatal_checks.clone(),
            degraded_checks: runtime.degraded_checks.clone(),
            warning_checks: runtime.warning_checks.clone(),
            route_explanations: runtime.route_explanations.clone(),
            endpoint_bypass_checks: runtime.endpoint_bypass_checks.clone(),
            last_repair_action: runtime.last_repair_action.clone(),
            network_event_seq: runtime.network_event_seq,
            previous_unclean_shutdown: runtime.previous_unclean_shutdown.clone(),
            error: runtime.error.clone(),
            timings_ms: runtime.timings_ms.clone(),
            powershell_fallback_count: runtime.powershell_fallback_count,
            singbox_check_ms: runtime.singbox_check_ms,
            xray_spawn_ms: runtime.xray_spawn_ms,
            xray_check_ms: runtime.xray_check_ms,
            adapter_probe_backend: runtime.adapter_probe_backend.clone(),
            route_probe_backend: runtime.route_probe_backend.clone(),
            native_probe_ms: runtime.native_probe_ms.clone(),
            fallback_probe_ms: runtime.fallback_probe_ms.clone(),
        }
    }

    fn refresh_runtime_verdict(runtime: &mut TunnelRuntime) {
        match runtime.state {
            TunnelState::Connected => {
                if !runtime.fatal_checks.is_empty() {
                    runtime.effective_state = TunnelEffectiveState::Failed;
                    runtime.health_verdict = TunnelHealthVerdict::Failed;
                    return;
                }

                let missing_core = runtime.runtime_socks_port.is_none()
                    || runtime.adapter_alias.is_none()
                    || runtime.adapter_ifindex.is_none()
                    || runtime.route_ready != Some(true)
                    || runtime.dns_ready != Some(true);
                if missing_core {
                    runtime.effective_state = TunnelEffectiveState::Failed;
                    runtime.health_verdict = TunnelHealthVerdict::Failed;
                    if !runtime
                        .fatal_checks
                        .iter()
                        .any(|check| check == "service core readiness incomplete")
                    {
                        runtime
                            .fatal_checks
                            .push("service core readiness incomplete".into());
                    }
                    return;
                }

                let was_suspect = matches!(
                    runtime.effective_state,
                    TunnelEffectiveState::Suspect | TunnelEffectiveState::Repairing
                );
                if was_suspect
                    && !runtime.degraded_checks.iter().any(|check| {
                        check == "recent Windows network/power event requires route reassertion"
                    })
                {
                    runtime.degraded_checks.push(
                        "recent Windows network/power event requires route reassertion".into(),
                    );
                }

                let degraded = !runtime.degraded_checks.is_empty()
                    || runtime
                        .proxy_compat_state
                        .as_deref()
                        .is_some_and(|state| matches!(state, "pending" | "failed" | "degraded"));
                if degraded {
                    runtime.effective_state = TunnelEffectiveState::ProtectedDegraded;
                    runtime.health_verdict = TunnelHealthVerdict::ProtectedDegraded;
                } else {
                    runtime.effective_state = TunnelEffectiveState::Protected;
                    runtime.health_verdict = TunnelHealthVerdict::Protected;
                }
            }
            TunnelState::Connecting => {
                if !matches!(
                    runtime.effective_state,
                    TunnelEffectiveState::Preparing | TunnelEffectiveState::Connecting
                ) {
                    runtime.effective_state = TunnelEffectiveState::Connecting;
                }
                runtime.health_verdict = TunnelHealthVerdict::Repairing;
            }
            TunnelState::Disconnecting => {
                runtime.effective_state = TunnelEffectiveState::Disconnecting;
                runtime.health_verdict = TunnelHealthVerdict::CleanupPending;
            }
            TunnelState::Failed => {
                runtime.effective_state = TunnelEffectiveState::Failed;
                runtime.health_verdict = TunnelHealthVerdict::Failed;
            }
            TunnelState::Disconnected => {
                runtime.effective_state = TunnelEffectiveState::Idle;
                // cleanup_pending set by stop verification stays visible until
                // the next start clears runtime checks.
                if !matches!(runtime.health_verdict, TunnelHealthVerdict::CleanupPending) {
                    runtime.health_verdict = TunnelHealthVerdict::Failed;
                }
            }
        }
    }

    fn refresh_connected_process_state(runtime: &mut TunnelRuntime) {
        if !matches!(runtime.state, TunnelState::Connected) {
            return;
        }

        let mut failure = None;
        if let Some(child) = runtime.xray.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    failure = Some(format!("xray exited unexpectedly with {}", status));
                }
                Err(e) => {
                    failure = Some(format!("xray status check failed: {}", e));
                }
                Ok(None) => {}
            }
        }
        if failure.is_none() {
            if let Some(child) = runtime.singbox.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        failure = Some(format!("sing-box exited unexpectedly with {}", status));
                    }
                    Err(e) => {
                        failure = Some(format!("sing-box status check failed: {}", e));
                    }
                    Ok(None) => {}
                }
            }
        }

        if let Some(message) = failure {
            mark_runtime_failed(runtime, &message);
        }
    }

    fn mark_runtime_failed(runtime: &mut TunnelRuntime, message: &str) {
        let redacted = redact(message);

        let mut singbox = runtime.singbox.take();
        let mut xray = runtime.xray.take();
        let job = runtime.job.take();
        drop(job);

        if let Some(mut child) = singbox.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(mut child) = xray.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        runtime.state = TunnelState::Failed;
        runtime.effective_state = TunnelEffectiveState::Failed;
        runtime.health_verdict = TunnelHealthVerdict::Failed;
        runtime.phase = Some("failed".into());
        runtime.runtime_socks_port = None;
        runtime.runtime_http_port = None;
        runtime.runtime_api_port = None;
        runtime.adapter_alias = None;
        runtime.adapter_ifindex = None;
        runtime.route_ready = None;
        runtime.dns_ready = None;
        runtime.proxy_compat_state = Some("failed".into());
        runtime.last_repair_action = Some("failed_cleanup".into());
        runtime.error = Some(redacted.clone());
        if !runtime.fatal_checks.iter().any(|check| check == &redacted) {
            runtime.fatal_checks.push(redacted);
        }
    }

    fn collect_diagnostics() -> TunnelDiagnostics {
        TunnelDiagnostics {
            status: status_snapshot(),
            log_tail: sanitized_log_tail(),
            network_snapshot: sanitized_network_snapshot(),
        }
    }

    fn sanitized_network_snapshot() -> Vec<String> {
        let script = r#"
Write-Output '--- adapters'
Get-NetAdapter -ErrorAction SilentlyContinue |
  Where-Object { $_.Name -match 'DoodleRay|tun|happ|Wintun|sing' -or $_.InterfaceDescription -match 'DoodleRay|tun|happ|Wintun|sing' } |
  Select-Object Name, InterfaceDescription, Status, ifIndex |
  Format-Table -AutoSize | Out-String -Width 220
Write-Output '--- ipv4 interfaces'
Get-NetIPInterface -AddressFamily IPv4 -ErrorAction SilentlyContinue |
  Sort-Object InterfaceMetric |
  Select-Object ifIndex, InterfaceAlias, InterfaceMetric, ConnectionState, NlMtu |
  Format-Table -AutoSize | Out-String -Width 220
Write-Output '--- ipv4 default/split routes'
Get-NetRoute -AddressFamily IPv4 -ErrorAction SilentlyContinue |
  Where-Object { $_.DestinationPrefix -in @('0.0.0.0/0','0.0.0.0/1','128.0.0.0/1') -or $_.InterfaceAlias -match 'DoodleRay|happ|tun' } |
  Sort-Object InterfaceAlias, DestinationPrefix, RouteMetric |
  Select-Object InterfaceAlias, DestinationPrefix, NextHop, RouteMetric, Protocol |
  Format-Table -AutoSize | Out-String -Width 220
Write-Output '--- owned/competing processes'
Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
  Where-Object { $_.Name -in @('DoodleRay.exe','DoodleRayService.exe','xray.exe','sing-box.exe','Happ.exe') -or $_.ExecutablePath -match 'DoodleRay|Happ|sing-box|xray' } |
  Select-Object ProcessId, Name, ExecutablePath |
  Format-Table -AutoSize | Out-String -Width 260
"#;
        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .creation_flags(0x08000000)
            .output();

        match output {
            Ok(output) => {
                let mut text = String::new();
                text.push_str(&String::from_utf8_lossy(&output.stdout));
                text.push_str(&String::from_utf8_lossy(&output.stderr));
                text.lines()
                    .map(redact_sensitive_line)
                    .filter(|line| !line.trim().is_empty())
                    .take(220)
                    .collect()
            }
            Err(e) => vec![format!("network snapshot failed: {}", e)],
        }
    }

    fn sanitized_log_tail() -> Vec<String> {
        let root = runtime_root();
        let mut files = Vec::new();
        collect_log_files(&root, &mut files, 0);
        if let Some(parent) = root.parent() {
            collect_log_files(parent, &mut files, 0);
        }
        files.sort();
        files.dedup();
        files.sort_by_key(|path| {
            std::fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .ok()
        });
        files.reverse();

        let mut lines = Vec::new();
        for path in files.into_iter().take(6) {
            let label = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("log");
            lines.push(format!("--- {}", label));
            if let Ok(text) = std::fs::read_to_string(&path) {
                let mut tail: Vec<String> = text
                    .lines()
                    .rev()
                    .take(40)
                    .map(redact_sensitive_line)
                    .collect();
                tail.reverse();
                lines.extend(tail);
            }
        }
        lines
            .into_iter()
            .rev()
            .take(240)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    fn collect_log_files(dir: &Path, files: &mut Vec<PathBuf>, depth: usize) {
        if depth > 3 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_log_files(&path, files, depth + 1);
            } else if path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("log"))
                .unwrap_or(false)
            {
                files.push(path);
            }
        }
    }

    fn redact_sensitive_line(line: &str) -> String {
        let mut redacted = redact(line);
        redacted = redact_url_like(&redacted);
        redacted = redact_uuid_like(&redacted);
        redacted
    }

    fn redact_url_like(value: &str) -> String {
        value
            .split_whitespace()
            .map(|part| {
                if part.starts_with("http://") || part.starts_with("https://") {
                    "[redacted-url]"
                } else {
                    part
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn redact_uuid_like(value: &str) -> String {
        value
            .split(|c: char| !c.is_ascii_hexdigit() && c != '-')
            .fold(value.to_string(), |acc, token| {
                if token.len() == 36
                    && token.chars().filter(|c| *c == '-').count() == 4
                    && token.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
                {
                    acc.replace(token, "[redacted-uuid]")
                } else {
                    acc
                }
            })
    }

    fn start_tunnel(request: StartTunnelRequest, generation: u64) -> Result<(), String> {
        let started = Instant::now();
        log_service_event(&format!(
            "start_tunnel generation={} engine={:?} op_id={}",
            generation,
            request.engine_kind,
            sanitize_id(&request.op_id)
        ));
        ensure_current_generation(generation)?;
        {
            let mut runtime = state().lock().unwrap();
            runtime.previous_generation = Some(runtime.service_generation);
            runtime.state = TunnelState::Connecting;
            runtime.effective_state = TunnelEffectiveState::Preparing;
            runtime.health_verdict = TunnelHealthVerdict::Repairing;
            runtime.phase = Some("stopping_previous".into());
            runtime.active_op_id = Some(request.op_id.clone());
            runtime.service_generation = generation;
            runtime.engine_kind = Some(request.engine_kind.clone());
            runtime.runtime_socks_port = Some(request.socks_port);
            runtime.runtime_http_port = Some(request.http_port);
            runtime.runtime_api_port = request.api_port;
            runtime.adapter_alias = None;
            runtime.adapter_ifindex = None;
            runtime.route_ready = None;
            runtime.dns_ready = None;
            runtime.proxy_compat_state = Some("pending".into());
            runtime.fatal_checks.clear();
            runtime.degraded_checks.clear();
            runtime.warning_checks.clear();
            runtime.route_explanations = vec!["route readiness pending".into()];
            runtime.endpoint_bypass_checks = vec!["endpoint bypass readiness pending".into()];
            runtime.last_repair_action = Some("replace_tunnel".into());
            runtime.error = None;
            runtime.timings_ms.clear();
            runtime.powershell_fallback_count = 0;
            runtime.singbox_check_ms = None;
            runtime.xray_spawn_ms = None;
            runtime.adapter_probe_backend = None;
            runtime.route_probe_backend = None;
            runtime.native_probe_ms.clear();
            runtime.fallback_probe_ms.clear();
        }
        stop_owned_processes("replace_tunnel")?;
        ensure_current_generation(generation)?;
        start_network_watchers_for_connect();
        write_session_marker(&request.op_id, generation);

        let runtime_dir = runtime_root().join(sanitize_id(&request.op_id));
        std::fs::create_dir_all(&runtime_dir)
            .map_err(|e| format!("Failed to create runtime dir: {}", e))?;
        let singbox_config = with_tun_interface_name(request.singbox_config.clone());

        // Bounded bring-up: one normal attempt, and on a repairable adapter/
        // IPv4/engine startup failure exactly one DoodleRay-owned repair retry
        // (stop only our children, recreate the service-owned TUN). The user
        // only sees an error after the second attempt, and it is actionable.
        let mut attempt: u32 = 1;
        loop {
            match bring_up_tun_runtime(&request, &runtime_dir, &singbox_config, started, generation)
            {
                Ok(()) => break,
                Err(error)
                    if attempt == 1
                        && tauri_app_lib::tunnel_service::is_repairable_tun_bringup_error(
                            &error,
                        ) =>
                {
                    log_service_event(&format!(
                        "tun bring-up attempt 1 failed; running DoodleRay-owned repair retry: {}",
                        redact(&error)
                    ));
                    {
                        let mut runtime = state().lock().unwrap();
                        runtime.effective_state = TunnelEffectiveState::Repairing;
                        runtime.health_verdict = TunnelHealthVerdict::Repairing;
                        runtime.phase = Some("tun_adapter_repair".into());
                    }
                    stop_owned_processes("tun_adapter_repair")?;
                    ensure_current_generation(generation)?;
                    write_session_marker(&request.op_id, generation);
                    {
                        let mut runtime = state().lock().unwrap();
                        runtime.state = TunnelState::Connecting;
                        runtime.effective_state = TunnelEffectiveState::Repairing;
                        runtime.health_verdict = TunnelHealthVerdict::Repairing;
                        runtime.phase = Some("tun_adapter_repair_retry".into());
                        runtime.active_op_id = Some(request.op_id.clone());
                        runtime.service_generation = generation;
                        runtime.last_repair_action = Some("tun_adapter_repair".into());
                        runtime.warning_checks.push(format!(
                            "TUN adapter repair retry ran after: {}",
                            redact(&error)
                        ));
                    }
                    attempt = 2;
                }
                Err(error) => {
                    let wintun_present = singbox_exe_path()
                        .ok()
                        .and_then(|exe| exe.parent().map(|dir| dir.join("wintun.dll").exists()))
                        .unwrap_or(false);
                    let last_phase = state().lock().unwrap().phase.clone();
                    return Err(tauri_app_lib::tunnel_service::format_tun_bringup_failure(
                        &error,
                        wintun_present,
                        last_phase.as_deref(),
                        attempt,
                    ));
                }
            }
        }

        let mut runtime = state().lock().unwrap();
        runtime.state = TunnelState::Connected;
        runtime.effective_state = if runtime.degraded_checks.is_empty() {
            TunnelEffectiveState::Protected
        } else {
            TunnelEffectiveState::ProtectedDegraded
        };
        runtime.health_verdict = if runtime.degraded_checks.is_empty() {
            TunnelHealthVerdict::Protected
        } else {
            TunnelHealthVerdict::ProtectedDegraded
        };
        runtime.phase = Some("connected".into());
        runtime.proxy_compat_state = Some("core_connected".into());
        runtime.route_explanations.push(
            "default protected route canaries preferred DoodleRay Tunnel before marking protected"
                .into(),
        );
        runtime
            .route_explanations
            .extend(tauri_app_lib::tunnel_service::summarize_route_policy(
                &singbox_config,
            ));
        runtime.endpoint_bypass_checks.push(
            "control-plane endpoint bypass must remain direct after connect and network changes"
                .into(),
        );
        runtime
            .timings_ms
            .push(("total_connect".into(), elapsed_ms(started)));
        drop(runtime);
        log_service_event(&format!(
            "tunnel connected generation={} total_connect_ms={}",
            generation,
            elapsed_ms(started)
        ));
        Ok(())
    }

    fn bring_up_tun_runtime(
        request: &StartTunnelRequest,
        runtime_dir: &Path,
        singbox_config: &Value,
        started: Instant,
        generation: u64,
    ) -> Result<(), String> {
        {
            let mut runtime = state().lock().unwrap();
            runtime.engine_kind = Some(request.engine_kind.clone());
            runtime.runtime_socks_port = Some(request.socks_port);
            runtime.runtime_http_port = Some(request.http_port);
            runtime.runtime_api_port = request.api_port;
        }
        {
            let mut runtime = state().lock().unwrap();
            runtime.job = Some(create_kill_on_close_job()?);
            runtime.state = TunnelState::Connecting;
            runtime.effective_state = TunnelEffectiveState::Connecting;
            runtime.health_verdict = TunnelHealthVerdict::Repairing;
            runtime.phase = Some("starting_job".into());
            runtime.active_op_id = Some(request.op_id.clone());
            runtime.service_generation = generation;
        }

        let xray_log_path = if matches!(request.engine_kind, TunnelEngineKind::XrayTun) {
            set_phase("starting_xray", started, generation)?;
            let xray_config = request
                .xray_config
                .as_ref()
                .ok_or("xray_config is required for xray_tun")?;
            let xray_config_path = runtime_dir.join("xray_config.json");
            write_json_file(&xray_config_path, xray_config)?;
            let xray_log_path = runtime_dir.join("xray.log");
            let xray_exe = xray_exe_path()?;
            check_xray_config(&xray_exe, &xray_config_path, xray_config)?;
            let spawn_started = Instant::now();
            let child = spawn_engine(xray_exe, &["run", "-c"], &xray_config_path, &xray_log_path)?;
            set_xray_spawn_ms(elapsed_ms(spawn_started));
            assign_child_to_job(&child)?;
            state().lock().unwrap().xray = Some(child);
            // Do not serialize xray readiness before Wintun bring-up. The sing-box
            // bridge can create the TUN adapter while xray is still binding its
            // loopback ports; we still require xray readiness before declaring the
            // runtime usable below.
            set_phase("xray_spawned", started, generation)?;
            Some(xray_log_path)
        } else {
            None
        };

        set_phase("starting_tun", started, generation)?;
        let singbox_config_path = runtime_dir.join("singbox_tun_config.json");
        write_json_file(&singbox_config_path, singbox_config)?;
        let singbox_log_path = runtime_dir.join("singbox_tun.log");
        let singbox_exe = singbox_exe_path()?;
        check_singbox_config(
            &singbox_exe,
            &singbox_config_path,
            runtime_dir,
            singbox_config,
        )?;
        let child = spawn_engine(
            singbox_exe,
            &["run", "-c"],
            &singbox_config_path,
            &singbox_log_path,
        )?;
        assign_child_to_job(&child)?;
        state().lock().unwrap().singbox = Some(child);

        set_phase("waiting_adapter", started, generation)?;
        let adapter_snapshot =
            match wait_for_adapter("DoodleRay Tunnel", Duration::from_secs(15), generation) {
                Ok(snapshot) => snapshot,
                Err(adapter_error) => {
                    if let Some(path) = xray_log_path.as_deref() {
                        ensure_xray_alive(path)?;
                    }
                    ensure_singbox_alive(&singbox_log_path)?;
                    return Err(adapter_error);
                }
            };
        mark_adapter_ready(adapter_snapshot);
        set_phase("adapter_ready", started, generation)?;
        ensure_singbox_alive(&singbox_log_path)?;
        if let Some(path) = xray_log_path.as_deref() {
            ensure_xray_alive(path)?;
            wait_for_port(request.socks_port, Duration::from_secs(8), generation)?;
            if request.http_port != request.socks_port {
                wait_for_port(request.http_port, Duration::from_secs(8), generation)?;
            }
            set_phase("xray_ready", started, generation)?;
        } else if matches!(request.engine_kind, TunnelEngineKind::SingboxTun) {
            wait_for_port(request.socks_port, Duration::from_secs(8), generation)?;
            if request.http_port != request.socks_port {
                wait_for_port(request.http_port, Duration::from_secs(8), generation)?;
            }
        }
        set_phase("singbox_ready", started, generation)?;
        wait_for_doodleray_dual_stack_interface(Duration::from_secs(20), generation)?;
        set_phase("dual_stack_ready", started, generation)?;
        apply_doodleray_dns_client_policy(generation);
        wait_for_doodleray_route_preferred(Duration::from_secs(20), generation)?;
        mark_route_ready();
        set_phase("routes_ready", started, generation)?;
        mark_dns_policy_ready();
        set_phase("dns_ready", started, generation)?;
        // The resolver canary no longer gates bring-up (see
        // spawn_nonfatal_policy_checks): it only proves two specific
        // third-party domains are reachable, so it now runs in the
        // background instead of blocking connect for up to 20s.
        spawn_nonfatal_policy_checks(generation);
        if let Some(path) = xray_log_path.as_deref() {
            ensure_xray_alive(path)?;
        }
        ensure_singbox_alive(&singbox_log_path)?;
        wait_for_port(request.socks_port, Duration::from_secs(5), generation)?;
        if request.http_port != request.socks_port {
            wait_for_port(request.http_port, Duration::from_secs(5), generation)?;
        }
        set_phase("local_proxy_ready", started, generation)?;
        ensure_current_generation(generation)?;
        Ok(())
    }

    fn stop_tunnel(request: StopTunnelRequest) -> Result<TunnelStatus, String> {
        log_service_event(&format!(
            "StopTunnel requested op_id={} reason={}",
            sanitize_id(&request.op_id),
            request.reason
        ));
        OP_GENERATION.fetch_add(1, Ordering::SeqCst);
        stop_owned_processes("stop_tunnel")?;
        clear_timings();
        Ok(status_snapshot())
    }

    fn repair_connected_runtime(
        reason: &str,
        expected_op_id: Option<&str>,
    ) -> Result<TunnelStatus, String> {
        let started = Instant::now();
        let (generation, socks_port, http_port, op_id) = {
            let mut runtime = state().lock().unwrap();
            if !matches!(runtime.state, TunnelState::Connected) {
                return Ok(status_snapshot());
            }
            if let Some(expected) = expected_op_id {
                if runtime.active_op_id.as_deref() != Some(expected) {
                    return Ok(status_snapshot());
                }
            }
            let socks_port = runtime
                .runtime_socks_port
                .ok_or("active runtime has no SOCKS port")?;
            let http_port = runtime.runtime_http_port.unwrap_or(socks_port);
            runtime.effective_state = TunnelEffectiveState::Repairing;
            runtime.health_verdict = TunnelHealthVerdict::Repairing;
            runtime.phase = Some(format!("repairing:{}", reason));
            runtime.last_repair_action = Some(reason.into());
            runtime.route_ready = None;
            runtime.dns_ready = None;
            runtime.degraded_checks.retain(|check| {
                check != "recent Windows network/power event requires route reassertion"
            });
            runtime
                .route_explanations
                .push(format!("runtime reassert started: {}", reason));
            (
                runtime.service_generation,
                socks_port,
                http_port,
                runtime.active_op_id.clone(),
            )
        };

        ensure_current_generation(generation)?;
        refresh_adapter_snapshot_required()?;
        wait_for_doodleray_dual_stack_interface(Duration::from_secs(8), generation)?;
        apply_doodleray_dns_client_policy(generation);
        wait_for_doodleray_route_preferred(Duration::from_secs(8), generation)?;
        mark_route_ready();
        mark_dns_policy_ready();
        spawn_nonfatal_policy_checks(generation);
        wait_for_port(socks_port, Duration::from_secs(5), generation)?;
        if http_port != socks_port {
            match wait_for_port(http_port, Duration::from_secs(5), generation) {
                Ok(()) => {
                    let mut runtime = state().lock().unwrap();
                    if !matches!(
                        runtime.proxy_compat_state.as_deref(),
                        Some("degraded") | Some("disabled_for_direct_app_exclusions")
                    ) {
                        runtime.proxy_compat_state = Some("ready".into());
                    }
                }
                Err(error) => {
                    let mut runtime = state().lock().unwrap();
                    runtime.proxy_compat_state = Some("degraded".into());
                    let detail = format!(
                        "Windows proxy compatibility HTTP listener was not ready during repair: {}",
                        error
                    );
                    if !runtime.degraded_checks.iter().any(|check| check == &detail) {
                        runtime.degraded_checks.push(detail);
                    }
                }
            }
        }
        ensure_current_generation(generation)?;

        {
            let mut runtime = state().lock().unwrap();
            if runtime.active_op_id != op_id {
                return Ok(status_snapshot());
            }
            runtime.state = TunnelState::Connected;
            runtime.phase = Some("connected".into());
            runtime
                .timings_ms
                .push((format!("repair:{}", reason), elapsed_ms(started)));
            runtime
                .route_explanations
                .push(format!("runtime reassert completed: {}", reason));
        }
        Ok(status_snapshot())
    }

    fn stop_owned_processes(reason: &str) -> Result<(), String> {
        log_service_event(&format!("stop_owned_processes reason={}", reason));
        let (mut singbox, mut xray, job) = {
            let mut runtime = state().lock().unwrap();
            runtime.state = TunnelState::Disconnecting;
            runtime.effective_state = TunnelEffectiveState::Disconnecting;
            runtime.health_verdict = TunnelHealthVerdict::CleanupPending;
            runtime.phase = Some(reason.into());
            runtime.last_repair_action = Some(reason.into());
            (
                runtime.singbox.take(),
                runtime.xray.take(),
                runtime.job.take(),
            )
        };

        drop(job);
        if let Some(mut child) = singbox.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(mut child) = xray.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let mut runtime = state().lock().unwrap();
        runtime.state = TunnelState::Disconnected;
        runtime.effective_state = TunnelEffectiveState::Idle;
        runtime.health_verdict = TunnelHealthVerdict::Failed;
        runtime.phase = None;
        runtime.active_op_id = None;
        runtime.engine_kind = None;
        runtime.runtime_socks_port = None;
        runtime.runtime_http_port = None;
        runtime.runtime_api_port = None;
        runtime.adapter_alias = None;
        runtime.adapter_ifindex = None;
        runtime.route_ready = None;
        runtime.dns_ready = None;
        runtime.proxy_compat_state = None;
        runtime.fatal_checks.clear();
        runtime.degraded_checks.clear();
        runtime.warning_checks.clear();
        runtime.route_explanations.clear();
        runtime.endpoint_bypass_checks.clear();
        runtime.error = None;
        drop(runtime);
        clear_session_marker();

        // Verify DoodleRay-owned cleanup actually finished: the wintun adapter
        // must disappear with its owning child. If it lingers, report an
        // honest cleanup_pending instead of a silent idle. We never touch
        // non-DoodleRay adapters here.
        let mut adapter_still_present = doodleray_adapter_snapshot().is_some();
        if should_wait_for_adapter_cleanup(reason) {
            let deadline = Instant::now() + Duration::from_secs(3);
            while adapter_still_present && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(100));
                adapter_still_present = doodleray_adapter_snapshot().is_some();
            }
        }
        if adapter_still_present {
            let mut runtime = state().lock().unwrap();
            runtime.health_verdict = TunnelHealthVerdict::CleanupPending;
            runtime.warning_checks.push(
                "DoodleRay Tunnel adapter is still present after owned cleanup; cleanup pending"
                    .into(),
            );
            log_service_event("cleanup pending: DoodleRay Tunnel adapter still present after stop");
        }
        let already_scanned = state().lock().unwrap().ghost_scan_done_since_start;
        if should_scan_wintun_ghosts(reason, adapter_still_present, already_scanned) {
            for action in repair_stale_wintun_ghost_devices(reason) {
                log_service_event(&action);
                if action.contains("removed=") && !action.contains("removed=0") {
                    let mut runtime = state().lock().unwrap();
                    runtime.warning_checks.push(action);
                }
            }
            state().lock().unwrap().ghost_scan_done_since_start = true;
        } else {
            log_service_event(&format!(
                "stale Wintun ghost repair ({}) skipped: already verified clean this service run",
                reason
            ));
        }
        Ok(())
    }

    /// Whether to spend up to 3s proving the Wintun adapter disappeared after
    /// stopping our children. That proof exists so a tunnel that comes to rest
    /// "disconnected" reports an honest cleanup_pending instead of a silent
    /// idle — so it is only worth paying for when we are actually coming to
    /// rest. On `replace_tunnel` we are mid-restart and about to bring a new
    /// tunnel up immediately; bring-up has its own adapter-readiness wait, and
    /// the state never settles as "disconnected" for a user to see. Measured on
    /// the stand: the adapter never vanished inside the window anyway, so a
    /// country switch burned ~2.5s here on top of ~3s on the preceding stop.
    fn should_wait_for_adapter_cleanup(reason: &str) -> bool {
        reason != "replace_tunnel"
    }

    /// Whether to pay for the PowerShell Get-PnpDevice ghost-device scan on
    /// this stop. Real-world timing showed this scan costs ~3s per connect
    /// even when it finds nothing (seen=0 removed=0), because bring-up always
    /// stops any prior runtime first. A stale Wintun ghost can only appear
    /// from a crash this service process didn't witness (checked once, right
    /// after start) or from this stop's own cleanup not finishing cleanly
    /// (adapter_still_present, checked fresh every time) — so skipping is
    /// safe once one full scan has proven the machine clean for this service
    /// run and neither signal is currently active.
    ///
    /// `replace_tunnel` never scans. It runs at the start of every connect,
    /// right after killing sing-box, when the adapter has essentially always
    /// not vanished yet — so `adapter_still_present` is true for an entirely
    /// benign reason and would force a rescan on every single connect (~2.4s
    /// measured on the stand, every time reporting seen=0). A ghost that
    /// genuinely blocks adapter creation still gets cleaned: bring-up attempt 1
    /// fails, and the retry path calls stop_owned_processes("tun_adapter_repair"),
    /// which does scan.
    fn should_scan_wintun_ghosts(
        reason: &str,
        adapter_still_present: bool,
        already_scanned: bool,
    ) -> bool {
        if reason == "replace_tunnel" {
            return false;
        }
        adapter_still_present || !already_scanned
    }

    fn set_failed(message: &str) {
        let mut runtime = state().lock().unwrap();
        mark_runtime_failed(&mut runtime, message);
    }

    fn clear_timings() {
        let mut runtime = state().lock().unwrap();
        runtime.timings_ms.clear();
        runtime.powershell_fallback_count = 0;
        runtime.singbox_check_ms = None;
        runtime.xray_spawn_ms = None;
        runtime.adapter_probe_backend = None;
        runtime.route_probe_backend = None;
        runtime.native_probe_ms.clear();
        runtime.fallback_probe_ms.clear();
    }

    fn is_current_generation(generation: u64) -> bool {
        OP_GENERATION.load(Ordering::SeqCst) == generation
    }

    fn ensure_current_generation(generation: u64) -> Result<(), String> {
        if is_current_generation(generation) {
            Ok(())
        } else {
            Err("Tunnel start was cancelled".into())
        }
    }

    fn set_phase(phase: &str, started: Instant, generation: u64) -> Result<(), String> {
        ensure_current_generation(generation)?;
        let mut runtime = state().lock().unwrap();
        runtime.phase = Some(phase.into());
        runtime.timings_ms.push((phase.into(), elapsed_ms(started)));
        Ok(())
    }

    fn record_native_probe(label: &str, started: Instant) {
        state()
            .lock()
            .unwrap()
            .native_probe_ms
            .push((label.into(), elapsed_ms(started)));
    }

    fn record_fallback_probe(label: &str, started: Instant) {
        let mut runtime = state().lock().unwrap();
        runtime.powershell_fallback_count = runtime.powershell_fallback_count.saturating_add(1);
        runtime
            .fallback_probe_ms
            .push((label.into(), elapsed_ms(started)));
    }

    fn set_adapter_probe_backend(backend: &str) {
        state().lock().unwrap().adapter_probe_backend = Some(backend.into());
    }

    fn set_route_probe_backend(backend: &str) {
        state().lock().unwrap().route_probe_backend = Some(backend.into());
    }

    fn set_singbox_check_ms(value: u64) {
        state().lock().unwrap().singbox_check_ms = Some(value);
    }

    fn set_xray_spawn_ms(value: u64) {
        state().lock().unwrap().xray_spawn_ms = Some(value);
    }

    fn set_xray_check_ms(value: u64) {
        state().lock().unwrap().xray_check_ms = Some(value);
    }

    fn start_network_watchers_for_connect() {
        match windows_net::ensure_network_watchers() {
            Ok(detail) => {
                let mut runtime = state().lock().unwrap();
                let note = format!("native network change watchers ready: {detail}");
                if !runtime.route_explanations.iter().any(|item| item == &note) {
                    runtime.route_explanations.push(note);
                }
            }
            Err(error) => {
                let mut runtime = state().lock().unwrap();
                let note = format!(
                    "native network change watchers unavailable; using timed native polling: {}",
                    error
                );
                if !runtime.warning_checks.iter().any(|item| item == &note) {
                    runtime.warning_checks.push(note);
                }
            }
        }
    }

    fn wait_for_adapter_event_or_delay(
        cursor: windows_net::NetworkEventCursors,
        max_delay: Duration,
        generation: u64,
    ) -> Result<(), String> {
        let deadline = Instant::now() + max_delay;
        while Instant::now() < deadline {
            ensure_current_generation(generation)?;
            if cursor.adapter_changed_since() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        Ok(())
    }

    fn wait_for_route_event_or_delay(
        cursor: windows_net::NetworkEventCursors,
        max_delay: Duration,
        generation: u64,
    ) -> Result<(), String> {
        let deadline = Instant::now() + max_delay;
        while Instant::now() < deadline {
            ensure_current_generation(generation)?;
            if cursor.route_changed_since() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        Ok(())
    }

    fn elapsed_ms(started: Instant) -> u64 {
        started.elapsed().as_millis().min(u64::MAX as u128) as u64
    }

    fn write_json_file(path: &Path, value: &Value) -> Result<(), String> {
        let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| format!("write {:?}: {}", path, e))
    }

    fn create_kill_on_close_job() -> Result<JobHandle, String> {
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(format!(
                "CreateJobObject failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        let ok = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &mut info as *mut _ as *mut _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok == 0 {
            let error = std::io::Error::last_os_error();
            unsafe {
                CloseHandle(job);
            }
            return Err(format!("SetInformationJobObject failed: {}", error));
        }

        Ok(JobHandle(job))
    }

    fn assign_child_to_job(child: &Child) -> Result<(), String> {
        let runtime = state().lock().unwrap();
        let job = runtime
            .job
            .as_ref()
            .ok_or("Tunnel job object is not initialized")?;
        let process = child.as_raw_handle() as HANDLE;
        let ok = unsafe { AssignProcessToJobObject(job.raw(), process) };
        if ok == 0 {
            return Err(format!(
                "AssignProcessToJobObject failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    fn spawn_engine(
        exe: PathBuf,
        prefix_args: &[&str],
        config_path: &Path,
        log_path: &Path,
    ) -> Result<Child, String> {
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .map_err(|e| format!("open log {:?}: {}", log_path, e))?;
        let log_err = log
            .try_clone()
            .map_err(|e| format!("clone log {:?}: {}", log_path, e))?;
        let mut cmd = Command::new(exe);
        for arg in prefix_args {
            cmd.arg(arg);
        }
        cmd.arg(config_path)
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_err))
            .creation_flags(0x08000000);
        cmd.spawn().map_err(|e| format!("spawn engine: {}", e))
    }

    fn check_singbox_config(
        exe: &Path,
        config_path: &Path,
        runtime_dir: &Path,
        config: &Value,
    ) -> Result<(), String> {
        let config_hash = hash_json_value(config)?;
        let cache_hit = singbox_validation_cache()
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|cache| cache.config_hash == config_hash);
        if cache_hit {
            set_singbox_check_ms(0);
            log_service_event(
                "sing-box config check skipped: effective config hash was already validated",
            );
            return Ok(());
        }

        let started = Instant::now();
        let output = Command::new(exe)
            .args(["check", "-c"])
            .arg(config_path)
            .current_dir(runtime_dir)
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| format!("sing-box check failed to run: {}", e))?;
        set_singbox_check_ms(elapsed_ms(started));
        if output.status.success() {
            *singbox_validation_cache().lock().unwrap() =
                Some(SingboxValidationCache { config_hash });
            return Ok(());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "sing-box config check failed: {}{}",
            redact(&stdout),
            redact(&stderr)
        ))
    }

    fn check_xray_config(exe: &Path, config_path: &Path, config: &Value) -> Result<(), String> {
        let config_hash = hash_json_value(config)?;
        if *xray_validation_cache().lock().unwrap() == Some(config_hash) {
            set_xray_check_ms(0);
            log_service_event(
                "xray config check skipped: effective config hash was already validated",
            );
            return Ok(());
        }

        let started = Instant::now();
        let output = Command::new(exe)
            .args(["run", "-test", "-c"])
            .arg(config_path)
            .creation_flags(0x08000000)
            .output()
            .map_err(|error| format!("xray config check failed to run: {error}"))?;
        set_xray_check_ms(elapsed_ms(started));
        if output.status.success() {
            *xray_validation_cache().lock().unwrap() = Some(config_hash);
            return Ok(());
        }
        Err(format!(
            "xray config check failed: {}{}",
            redact(&String::from_utf8_lossy(&output.stdout)),
            redact(&String::from_utf8_lossy(&output.stderr))
        ))
    }

    fn singbox_validation_cache() -> &'static Mutex<Option<SingboxValidationCache>> {
        SINGBOX_VALIDATION_CACHE.get_or_init(|| Mutex::new(None))
    }

    fn xray_validation_cache() -> &'static Mutex<Option<u64>> {
        XRAY_VALIDATION_CACHE.get_or_init(|| Mutex::new(None))
    }

    fn hash_json_value(value: &Value) -> Result<u64, String> {
        let bytes = serde_json::to_vec(value).map_err(|e| format!("hash config: {}", e))?;
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        Ok(hasher.finish())
    }

    fn wait_for_port(port: u16, timeout: Duration, generation: u64) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            ensure_current_generation(generation)?;
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(75));
        }
        Err(format!("Local port {} did not become ready", port))
    }

    fn repair_stale_wintun_ghost_devices(reason: &str) -> Vec<String> {
        let script = r#"
$ErrorActionPreference = 'Continue'
$summary = New-Object System.Collections.Generic.List[string]
$seen = 0
$removed = 0
$failed = 0
$targets = @()

$devices = @(Get-PnpDevice -Class Net -ErrorAction SilentlyContinue | Where-Object {
  $id = [string]$_.InstanceId
  $friendly = [string]$_.FriendlyName
  $isWintun = $id -like 'SWD\WINTUN\*'
  $isKnownSingTun = $friendly -in @('DoodleRay Tunnel', 'sing-tun Tunnel')
  $isStale = ($_.Status -ne 'OK') -or ([int]$_.Problem -eq 45)
  $hasVisibleNetAdapter = [bool](Get-NetAdapter -IncludeHidden -ErrorAction SilentlyContinue | Where-Object {
    $_.Name -eq $friendly -or $_.InterfaceDescription -eq $friendly
  } | Select-Object -First 1)
  $isWintun -and $isStale -and ($isKnownSingTun -or -not $hasVisibleNetAdapter)
})

foreach ($device in $devices) {
  $seen += 1
  $targets += ("{0}|{1}|problem={2}|status={3}" -f $device.FriendlyName, $device.InstanceId, $device.Problem, $device.Status)
  $id = [string]$device.InstanceId
  try {
    $pnputil = Join-Path $env:WINDIR 'System32\pnputil.exe'
    $help = (& $pnputil /? 2>&1 | Out-String)
    if ($help -match '/remove-device') {
      $out = (& $pnputil /remove-device "$id" 2>&1 | Out-String)
      if ($LASTEXITCODE -eq 0 -or $out -match 'removed|success|успеш') {
        $removed += 1
      } else {
        $failed += 1
        $summary.Add(("remove_failed={0}: {1}" -f $device.FriendlyName, ($out -replace '\s+', ' ').Trim()))
      }
    } elseif (Get-Command Disable-PnpDevice -ErrorAction SilentlyContinue) {
      Disable-PnpDevice -InstanceId $id -Confirm:$false -ErrorAction Stop
      $removed += 1
    } else {
      $failed += 1
      $summary.Add(("remove_unavailable={0}" -f $device.FriendlyName))
    }
  } catch {
    $failed += 1
    $summary.Add(("remove_exception={0}: {1}" -f $device.FriendlyName, $_.Exception.Message))
  }
}

if ($removed -gt 0) {
  try {
    & (Join-Path $env:WINDIR 'System32\pnputil.exe') /scan-devices /async | Out-Null
  } catch {}
  Start-Sleep -Milliseconds 750
}

$summary.Insert(0, ("stale_wintun_ghosts_seen={0} removed={1} failed={2} targets={3}" -f $seen, $removed, $failed, ($targets -join '; ')))
$summary -join "`n"
"#;

        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .creation_flags(0x08000000)
            .output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let combined = format!("{}{}", stdout, stderr);
                combined
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(|line| format!("stale Wintun ghost repair ({}) {}", reason, redact(line)))
                    .collect()
            }
            Err(error) => vec![format!(
                "stale Wintun ghost repair ({}) failed to run: {}",
                reason, error
            )],
        }
    }

    fn ensure_singbox_alive(log_path: &Path) -> Result<(), String> {
        {
            let mut runtime = state().lock().unwrap();
            if let Some(child) = runtime.singbox.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    let log = std::fs::read_to_string(log_path).unwrap_or_default();
                    return Err(format!("sing-box exited with {}: {}", status, redact(&log)));
                }
            } else {
                return Err("sing-box process is not running".to_string());
            }
        }

        let log = std::fs::read_to_string(log_path).unwrap_or_default();
        let lower = log.to_lowercase();
        if lower.contains("fatal") || lower.contains("panic") {
            return Err(format!("sing-box failed: {}", redact(&log)));
        }
        Ok(())
    }

    fn ensure_xray_alive(log_path: &Path) -> Result<(), String> {
        let mut runtime = state().lock().unwrap();
        if let Some(child) = runtime.xray.as_mut() {
            if let Ok(Some(status)) = child.try_wait() {
                let log = std::fs::read_to_string(log_path).unwrap_or_default();
                return Err(format!("xray exited with {}: {}", status, redact(&log)));
            }
        } else {
            return Err("xray process is not running".to_string());
        }
        Ok(())
    }

    fn wait_for_adapter(
        adapter_name: &str,
        timeout: Duration,
        generation: u64,
    ) -> Result<(String, Option<u32>), String> {
        let deadline = Instant::now() + timeout;
        let native_started = Instant::now();
        let mut last_native_error: Option<String> = None;
        while Instant::now() < deadline {
            ensure_current_generation(generation)?;
            let cursor = windows_net::network_event_cursors();
            match windows_net::find_adapter_by_alias(adapter_name) {
                Ok(snapshot) => {
                    record_native_probe("adapter_snapshot", native_started);
                    set_adapter_probe_backend("native_iphelper_evented");
                    return Ok((snapshot.alias, Some(snapshot.ifindex)));
                }
                Err(error) => {
                    last_native_error = Some(error.to_string());
                    if !error.is_not_found() {
                        break;
                    }
                }
            }
            wait_for_adapter_event_or_delay(cursor, Duration::from_millis(125), generation)?;
        }

        let fallback_started = Instant::now();
        if let Some(snapshot) = wait_for_adapter_powershell_once(adapter_name) {
            record_fallback_probe("adapter_snapshot", fallback_started);
            set_adapter_probe_backend("powershell_fallback");
            return Ok(snapshot);
        }
        Err(format!(
            "DoodleRay Tunnel adapter did not become ready{}",
            last_native_error
                .as_deref()
                .map(|error| format!(": native probe: {}", error))
                .unwrap_or_default()
        ))
    }

    fn wait_for_adapter_powershell_once(adapter_name: &str) -> Option<(String, Option<u32>)> {
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Get-NetAdapter -Name '{}' -ErrorAction SilentlyContinue | Select-Object -First 1 | ForEach-Object {{ \"$($_.Name)|$($_.ifIndex)\" }}",
                    adapter_name.replace('\'', "''")
                ),
            ])
            .creation_flags(0x08000000)
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        parse_adapter_snapshot_line(&text)
    }

    fn parse_adapter_snapshot_line(text: &str) -> Option<(String, Option<u32>)> {
        let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
        let mut parts = line.splitn(2, '|');
        let alias = parts.next()?.trim().to_string();
        let ifindex = parts.next().and_then(|value| value.trim().parse().ok());
        if alias.is_empty() {
            None
        } else {
            Some((alias, ifindex))
        }
    }

    fn mark_adapter_ready(snapshot: (String, Option<u32>)) {
        let (alias, ifindex) = snapshot;
        let mut runtime = state().lock().unwrap();
        runtime.adapter_alias = Some(alias);
        runtime.adapter_ifindex = ifindex;
        runtime
            .route_explanations
            .push("DoodleRay Tunnel adapter is visible to Windows".into());
    }

    fn refresh_adapter_snapshot_required() -> Result<(), String> {
        let (alias, ifindex) = doodleray_adapter_snapshot()
            .ok_or("DoodleRay Tunnel adapter is missing during runtime repair")?;
        if ifindex.is_none() {
            return Err("DoodleRay Tunnel adapter has no ifIndex during runtime repair".into());
        }
        let mut runtime = state().lock().unwrap();
        runtime.adapter_alias = Some(alias);
        runtime.adapter_ifindex = ifindex;
        runtime
            .route_explanations
            .push("DoodleRay Tunnel adapter refreshed during runtime repair".into());
        Ok(())
    }

    fn mark_route_ready() {
        let mut runtime = state().lock().unwrap();
        runtime.route_ready = Some(true);
        runtime
            .route_explanations
            .push("Windows route preference probe selected the DoodleRay Tunnel interface".into());
        runtime.endpoint_bypass_checks.push(
            "service verified protected route preference after the engine became ready".into(),
        );
    }

    fn apply_doodleray_dns_client_policy(generation: u64) {
        if !is_current_generation(generation) {
            return;
        }
        let native_started = Instant::now();
        let result =
            windows_net::apply_dns_client_policy("DoodleRay Tunnel", &["1.1.1.1", "8.8.8.8"]);
        let outcome = match result {
            Ok(detail) => {
                record_native_probe("dns_policy", native_started);
                set_adapter_probe_backend("native_iphelper");
                Ok(detail)
            }
            Err(native_error) => {
                let fallback_started = Instant::now();
                match apply_doodleray_dns_client_policy_powershell() {
                    Ok(detail) => {
                        record_fallback_probe("dns_policy", fallback_started);
                        set_adapter_probe_backend("powershell_fallback");
                        Ok(detail)
                    }
                    Err(fallback_error) => Err(format!(
                        "native: {native_error}; powershell fallback: {fallback_error}"
                    )),
                }
            }
        };

        let mut runtime = state().lock().unwrap();
        match outcome {
            Ok(detail) => {
                let detail = format!(
                    "DoodleRay Tunnel DNS client policy pinned to IPv4 resolvers ({detail})"
                );
                log_service_event(&detail);
                if !runtime
                    .route_explanations
                    .iter()
                    .any(|existing| existing == &detail)
                {
                    runtime.route_explanations.push(detail);
                }
            }
            Err(error) => {
                let detail = format!(
                    "Windows DNS client policy could not be pinned to the DoodleRay Tunnel adapter: {}",
                    error
                );
                log_service_event(&detail);
                let detail = redact(&detail);
                if !runtime
                    .degraded_checks
                    .iter()
                    .any(|existing| existing == &detail)
                {
                    runtime.degraded_checks.push(detail);
                }
            }
        }
    }

    /// Fallback for apply_doodleray_dns_client_policy when the native
    /// SetInterfaceDnsSettings call fails. Only sets DNS servers and disables
    /// registration — the interface metric is handled separately by
    /// apply_doodleray_interface_metric, so it is not duplicated here.
    fn apply_doodleray_dns_client_policy_powershell() -> Result<String, String> {
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                r#"
$ErrorActionPreference = 'Stop'
$adapter = Get-NetAdapter -Name 'DoodleRay Tunnel' -ErrorAction Stop | Select-Object -First 1
Set-DnsClientServerAddress -InterfaceIndex $adapter.ifIndex -ServerAddresses @('1.1.1.1','8.8.8.8') -ErrorAction Stop
Set-DnsClient -InterfaceIndex $adapter.ifIndex -RegisterThisConnectionsAddress $false -ErrorAction SilentlyContinue
$after = @(Get-DnsClientServerAddress -InterfaceIndex $adapter.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue |
  ForEach-Object { $_.ServerAddresses } |
  Where-Object { $_ })
"adapter_dns_ipv4=" + ($after -join ',') + "; registration_enabled=false"
"#,
            ])
            .creation_flags(0x08000000)
            .output()
            .map_err(|error| format!("failed to run: {error}"))?;
        if output.status.success() {
            return Ok(redact(String::from_utf8_lossy(&output.stdout).trim()));
        }
        Err(redact(&format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )))
    }

    fn mark_dns_policy_ready() {
        let mut runtime = state().lock().unwrap();
        runtime.dns_ready = Some(true);
        runtime.route_explanations.push(
            "protected DNS policy is owned by sing-box DNS hijack with remote DoH over the proxy detour"
                .into(),
        );
    }

    fn windows_system_resolver_canary() -> Result<String, String> {
        // auth.openai.com is a known-bad choice as a hard gate: OpenAI
        // actively blocks huge ranges of VPN/datacenter exit IPs regardless
        // of whether the tunnel's own DNS/routing is healthy, so a block on
        // this one domain is not evidence of a real local problem. Try both
        // targets — never stop at the first failure — and only report the
        // canary as failed when *neither* target is reachable, matching
        // this function's own intent ("this exit simply not routing to that
        // one domain" must not fail the whole check).
        let targets = [
            ("auth.openai.com", "https://auth.openai.com"),
            ("api.ipify.org", "https://api.ipify.org"),
        ];
        let mut ok = Vec::new();
        let mut errors = Vec::new();
        for (host, url) in targets {
            let output = Command::new("curl.exe")
                .args([
                    "-4",
                    "--connect-timeout",
                    "4",
                    "--max-time",
                    "8",
                    "--noproxy",
                    "*",
                    "--silent",
                    "--show-error",
                    "--output",
                    "NUL",
                    "--write-out",
                    "%{http_code}",
                    url,
                ])
                .creation_flags(0x08000000)
                .output()
                .map_err(|error| format!("failed to run curl.exe resolver canary: {}", error))?;

            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            if output.status.success() {
                ok.push(format!("{}={}", host, combined.trim()));
                continue;
            }

            let lower = combined.to_lowercase();
            let dns_like = lower.contains("could not resolve host")
                || lower.contains("resolving timed out")
                || lower.contains("getaddrinfo")
                || lower.contains("enotfound")
                || lower.contains("name or service not known");
            errors.push(if dns_like {
                format!(
                    "Windows system resolver canary failed after TUN route setup for {}: {}",
                    host,
                    redact(combined.trim())
                )
            } else {
                format!(
                    "Windows HTTPS canary failed after TUN route setup for {}: {}",
                    host,
                    redact(combined.trim())
                )
            });
        }

        if !ok.is_empty() {
            Ok(format!(
                "Windows system resolver canary passed through TUN ({})",
                ok.join(", ")
            ))
        } else {
            Err(errors.into_iter().next().unwrap_or_else(|| {
                "Windows system resolver canary failed after TUN route setup".into()
            }))
        }
    }

    fn wait_for_windows_system_resolver_ready(
        timeout: Duration,
        generation: u64,
    ) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let mut last_error = "Windows system resolver canary did not run".to_string();
        while Instant::now() < deadline {
            ensure_current_generation(generation)?;
            match windows_system_resolver_canary() {
                Ok(message) => {
                    log_service_event(&format!("DNS readiness ok: {}", message));
                    let mut runtime = state().lock().unwrap();
                    let detail =
                        "Windows system resolver canaries passed through TUN (auth.openai.com, api.ipify.org)";
                    if !runtime.route_explanations.iter().any(|item| item == detail) {
                        runtime.route_explanations.push(detail.into());
                    }
                    return Ok(());
                }
                Err(error) => {
                    last_error = error;
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
        Err(last_error)
    }

    // The resolver canary only proves two specific third-party domains are
    // reachable through the tunnel. The adapter, dual-stack interface and
    // routes are already confirmed healthy by the time this runs, so a
    // canary miss (a slow network, or this exit simply not routing to that
    // one domain) must not block or tear down an otherwise-working TUN
    // connection — it just downgrades to a degraded_checks entry, from the
    // background, same as the QUIC policy check below.
    fn record_windows_system_resolver_canary(generation: u64) {
        if let Err(error) =
            wait_for_windows_system_resolver_ready(Duration::from_secs(20), generation)
        {
            if !is_current_generation(generation) {
                return;
            }
            log_service_event(&format!(
                "resolver canary degraded, tunnel stays active: {}",
                error
            ));
            let mut runtime = state().lock().unwrap();
            let detail = format!(
                "Windows system resolver canary did not pass through TUN (tunnel stays active): {}",
                error
            );
            if !runtime
                .degraded_checks
                .iter()
                .any(|existing| existing == &detail)
            {
                runtime.degraded_checks.push(detail);
            }
        }
    }

    fn spawn_nonfatal_policy_checks(generation: u64) {
        std::thread::spawn(move || {
            if !is_current_generation(generation) {
                return;
            }
            mark_quic_policy_status();
            record_windows_system_resolver_canary(generation);
        });
    }

    fn mark_quic_policy_status() {
        let mut runtime = state().lock().unwrap();
        let check = "QUIC/HTTP3 is not verified by a controlled probe in this build; no QUIC claim";
        if !runtime
            .warning_checks
            .iter()
            .any(|existing| existing == check)
        {
            runtime.warning_checks.push(check.into());
        }
    }

    fn doodleray_adapter_snapshot() -> Option<(String, Option<u32>)> {
        if let Ok(snapshot) = windows_net::find_adapter_by_alias("DoodleRay Tunnel") {
            return Some((snapshot.alias, Some(snapshot.ifindex)));
        }

        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-NetAdapter -Name 'DoodleRay Tunnel' -ErrorAction SilentlyContinue | Select-Object -First 1 | ForEach-Object { \"$($_.Name)|$($_.ifIndex)\" }",
            ])
            .creation_flags(0x08000000)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let line = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())?
            .to_string();
        let mut parts = line.splitn(2, '|');
        let alias = parts.next()?.trim().to_string();
        let ifindex = parts.next().and_then(|value| value.trim().parse().ok());
        if alias.is_empty() {
            None
        } else {
            Some((alias, ifindex))
        }
    }

    fn apply_doodleray_interface_metric() -> Result<String, String> {
        let native_started = Instant::now();
        match windows_net::apply_interface_metric("DoodleRay Tunnel", 50) {
            Ok(message) => {
                record_native_probe("dual_stack_metric", native_started);
                set_adapter_probe_backend("native_iphelper");
                Ok(message)
            }
            Err(error) => Err(error.to_string()),
        }
    }

    fn apply_doodleray_interface_metric_powershell() -> Result<String, String> {
        let script = r#"
$ErrorActionPreference = 'Stop'
$adapter = Get-NetAdapter -Name 'DoodleRay Tunnel' -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $adapter) {
  Write-Output 'DoodleRay Tunnel adapter is missing'
  exit 2
}

$binding = Get-NetAdapterBinding -Name $adapter.Name -ComponentID 'ms_tcpip' -ErrorAction SilentlyContinue
if ($binding -and -not $binding.Enabled) {
  Enable-NetAdapterBinding -Name $adapter.Name -ComponentID 'ms_tcpip' -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 150
}

$tunIface = Get-NetIPInterface -InterfaceIndex $adapter.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue
if (-not $tunIface) {
  $binding = Get-NetAdapterBinding -Name $adapter.Name -ComponentID 'ms_tcpip' -ErrorAction SilentlyContinue
  $bindingState = if ($binding) { $binding.Enabled } else { 'unknown' }
  Write-Output ("DoodleRay Tunnel IPv4 interface is not ready: ifIndex={0}, adapterStatus={1}, ipv4Binding={2}" -f $adapter.ifIndex, $adapter.Status, $bindingState)
  exit 2
}
$tunIfaceV6 = Get-NetIPInterface -InterfaceIndex $adapter.ifIndex -AddressFamily IPv6 -ErrorAction SilentlyContinue
if (-not $tunIfaceV6) {
  Write-Output ("DoodleRay Tunnel IPv6 interface is not ready: ifIndex={0}, adapterStatus={1}" -f $adapter.ifIndex, $adapter.Status)
  exit 2
}

$targetMetric = 50
Set-NetIPInterface -InterfaceIndex $adapter.ifIndex -AddressFamily IPv4 -AutomaticMetric Disabled -InterfaceMetric $targetMetric -ErrorAction Stop
Set-NetIPInterface -InterfaceIndex $adapter.ifIndex -AddressFamily IPv6 -AutomaticMetric Disabled -InterfaceMetric $targetMetric -ErrorAction Stop

$tunIface = Get-NetIPInterface -InterfaceIndex $adapter.ifIndex -AddressFamily IPv4 -ErrorAction Stop
$tunIfaceV6 = Get-NetIPInterface -InterfaceIndex $adapter.ifIndex -AddressFamily IPv6 -ErrorAction Stop
if ([int]$tunIface.InterfaceMetric -ne $targetMetric -or [int]$tunIfaceV6.InterfaceMetric -ne $targetMetric) {
  Write-Output ("DoodleRay Tunnel dual-stack metric was not applied: ifIndex={0}, ipv4={1}, ipv6={2}" -f $adapter.ifIndex, $tunIface.InterfaceMetric, $tunIfaceV6.InterfaceMetric)
  exit 3
}

Write-Output ("DoodleRay Tunnel dual-stack ready: ifIndex={0}, ipv4_metric={1}, ipv6_metric={2}, state={3}, mtu={4}" -f $adapter.ifIndex, $tunIface.InterfaceMetric, $tunIfaceV6.InterfaceMetric, $tunIface.ConnectionState, $tunIface.NlMtu)
"#;
        match Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .creation_flags(0x08000000)
            .output()
        {
            Ok(output) if output.status.success() => {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            }
            Ok(output) => Err(format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )),
            Err(e) => Err(format!("failed to run metric command: {}", e)),
        }
    }

    fn wait_for_doodleray_dual_stack_interface(
        timeout: Duration,
        generation: u64,
    ) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let mut last_error =
            "DoodleRay Tunnel dual-stack interface did not become ready".to_string();
        while Instant::now() < deadline {
            ensure_current_generation(generation)?;
            let cursor = windows_net::network_event_cursors();
            match apply_doodleray_interface_metric() {
                Ok(message) => {
                    log_service_event(&format!(
                        "applied DoodleRay Tunnel interface metric: {}",
                        message
                    ));
                    return Ok(());
                }
                Err(message) => last_error = message,
            }
            wait_for_adapter_event_or_delay(cursor, Duration::from_millis(175), generation)?;
        }
        let fallback_started = Instant::now();
        if let Ok(message) = apply_doodleray_interface_metric_powershell() {
            record_fallback_probe("dual_stack_metric_final", fallback_started);
            set_adapter_probe_backend("powershell_fallback");
            log_service_event(&format!(
                "applied DoodleRay Tunnel interface metric via final fallback: {}",
                message
            ));
            return Ok(());
        }
        Err(format!(
            "DoodleRay Tunnel dual-stack readiness failed: {}",
            last_error
        ))
    }

    fn ensure_doodleray_route_preferred_native() -> Result<String, String> {
        let ipv4_canaries = [
            Ipv4Addr::new(104, 26, 13, 205),
            Ipv4Addr::new(142, 251, 20, 113),
            Ipv4Addr::new(162, 159, 136, 232),
        ];
        let ipv6_canaries = [
            Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1111),
            Ipv6Addr::new(0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888),
        ];
        let started = Instant::now();
        match windows_net::route_canaries_prefer_adapter(
            "DoodleRay Tunnel",
            &ipv4_canaries,
            &ipv6_canaries,
        ) {
            Ok(message) => {
                record_native_probe("route_canaries", started);
                set_route_probe_backend("native_getbestroute2");
                Ok(message)
            }
            Err(error) => Err(error.to_string()),
        }
    }

    fn ensure_doodleray_route_preferred_powershell() -> Result<(), String> {
        let script = r#"
$adapter = Get-NetAdapter -Name 'DoodleRay Tunnel' -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $adapter) {
  Write-Output 'DoodleRay Tunnel adapter is missing'
  exit 2
}

$tunIface = Get-NetIPInterface -InterfaceIndex $adapter.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue
if (-not $tunIface) {
  $binding = Get-NetAdapterBinding -Name $adapter.Name -ComponentID 'ms_tcpip' -ErrorAction SilentlyContinue
  $bindingState = if ($binding) { $binding.Enabled } else { 'unknown' }
  Write-Output ("DoodleRay Tunnel IPv4 interface is not ready: ifIndex={0}, adapterStatus={1}, ipv4Binding={2}" -f $adapter.ifIndex, $adapter.Status, $bindingState)
  exit 2
}
$tunIfaceV6 = Get-NetIPInterface -InterfaceIndex $adapter.ifIndex -AddressFamily IPv6 -ErrorAction SilentlyContinue
if (-not $tunIfaceV6) {
  Write-Output ("DoodleRay Tunnel IPv6 interface is not ready: ifIndex={0}, adapterStatus={1}" -f $adapter.ifIndex, $adapter.Status)
  exit 2
}

$tunDefaultRoute = Get-NetRoute -DestinationPrefix '0.0.0.0/0' -InterfaceIndex $adapter.ifIndex -ErrorAction SilentlyContinue | Sort-Object RouteMetric | Select-Object -First 1
$tunSplitRoutes = @(
  Get-NetRoute -DestinationPrefix '0.0.0.0/1' -InterfaceIndex $adapter.ifIndex -ErrorAction SilentlyContinue | Sort-Object RouteMetric | Select-Object -First 1
  Get-NetRoute -DestinationPrefix '128.0.0.0/1' -InterfaceIndex $adapter.ifIndex -ErrorAction SilentlyContinue | Sort-Object RouteMetric | Select-Object -First 1
) | Where-Object { $_ }

$tunRoutes = @()
$routeShape = 'none'
if ($tunDefaultRoute) {
  $tunRoutes += $tunDefaultRoute
  $routeShape = 'default'
} elseif ($tunSplitRoutes.Count -eq 2) {
  $tunRoutes += $tunSplitRoutes
  $routeShape = 'split'
} else {
  $tunCustomRoutes = Get-NetRoute -InterfaceIndex $adapter.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue |
    Where-Object {
      $_.DestinationPrefix -notin @('172.30.255.0/30', '255.255.255.255/32') -and
      $_.DestinationPrefix -notlike '224.*' -and
      $_.DestinationPrefix -notlike '239.*'
    } |
    Sort-Object RouteMetric
  if ($tunCustomRoutes.Count -ge 4) {
    $tunRoutes += $tunCustomRoutes
    $routeShape = 'custom'
  } else {
    Write-Output ("DoodleRay Tunnel IPv4 routes are missing: default=0 split={0} custom={1}" -f $tunSplitRoutes.Count, $tunCustomRoutes.Count)
    exit 2
  }
}

$tunEffective = ($tunRoutes | ForEach-Object { [int]$_.RouteMetric + [int]$tunIface.InterfaceMetric } | Measure-Object -Maximum).Maximum
$bestOther = Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue |
  Where-Object { $_.InterfaceIndex -ne $adapter.ifIndex } |
  ForEach-Object {
    $iface = Get-NetIPInterface -InterfaceIndex $_.InterfaceIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue
    if ($iface) {
      [pscustomobject]@{
        Alias = $_.InterfaceAlias
        Effective = ([int]$_.RouteMetric + [int]$iface.InterfaceMetric)
      }
    }
  } |
  Sort-Object Effective |
  Select-Object -First 1
if ($routeShape -eq 'default' -and $bestOther -and $tunEffective -ge [int]$bestOther.Effective) {
  Write-Output ("DoodleRay Tunnel route is not preferred: tun={0}, other={1}:{2}" -f $tunEffective, $bestOther.Alias, $bestOther.Effective)
  exit 3
}

$tunV6DefaultRoute = Get-NetRoute -AddressFamily IPv6 -DestinationPrefix '::/0' -InterfaceIndex $adapter.ifIndex -ErrorAction SilentlyContinue | Sort-Object RouteMetric | Select-Object -First 1
$tunV6SplitRoutes = @(
  Get-NetRoute -AddressFamily IPv6 -DestinationPrefix '::/1' -InterfaceIndex $adapter.ifIndex -ErrorAction SilentlyContinue | Sort-Object RouteMetric | Select-Object -First 1
  Get-NetRoute -AddressFamily IPv6 -DestinationPrefix '8000::/1' -InterfaceIndex $adapter.ifIndex -ErrorAction SilentlyContinue | Sort-Object RouteMetric | Select-Object -First 1
) | Where-Object { $_ }
$routeShapeV6 = if ($tunV6DefaultRoute) { 'default' } elseif ($tunV6SplitRoutes.Count -eq 2) { 'split' } else { 'none' }
if ($routeShapeV6 -eq 'none') {
  Write-Output ("DoodleRay Tunnel IPv6 routes are missing: default=0 split={0}" -f $tunV6SplitRoutes.Count)
  exit 2
}

$routeCanaries = @(
  '104.26.13.205',
  '142.251.20.113',
  '162.159.136.232',
  '2606:4700:4700::1111',
  '2001:4860:4860::8888'
)
$bypassedCanaries = @()
foreach ($ip in $routeCanaries) {
  $matches = @(Find-NetRoute -RemoteIPAddress $ip -ErrorAction SilentlyContinue |
    Where-Object { [int]$_.InterfaceIndex -eq [int]$adapter.ifIndex })
  if ($matches.Count -eq 0) {
    $best = Find-NetRoute -RemoteIPAddress $ip -ErrorAction SilentlyContinue |
      Select-Object -First 1
    $via = if ($best) { "$($best.InterfaceAlias):$($best.InterfaceIndex)" } else { 'none' }
    $bypassedCanaries += "$ip via $via"
  }
}
if ($bypassedCanaries.Count -gt 0) {
  Write-Output ("DoodleRay Tunnel is not selected for protected route canaries: {0}" -f ($bypassedCanaries -join '; '))
  exit 3
}

Write-Output ("DoodleRay Tunnel dual-stack route preferred: ipv4_shape={0}, ipv6_shape={1}, tun={2}, best_other={3}, canaries=ok" -f $routeShape, $routeShapeV6, $tunEffective, $(if ($bestOther) { "$($bestOther.Alias):$($bestOther.Effective)" } else { 'none' }))
"#;
        match Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .creation_flags(0x08000000)
            .output()
        {
            Ok(output) if output.status.success() => {
                log_service_event(&format!(
                    "route readiness ok: {}",
                    String::from_utf8_lossy(&output.stdout).trim()
                ));
                Ok(())
            }
            Ok(output) => {
                let message = format!(
                    "route readiness failed: {}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                log_service_event(&message);
                Err(message)
            }
            Err(e) => {
                let message = format!("failed to run route readiness command: {}", e);
                log_service_event(&message);
                Err(message)
            }
        }
    }

    fn wait_for_doodleray_route_preferred(
        timeout: Duration,
        generation: u64,
    ) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let mut last_error = "DoodleRay Tunnel route did not become ready".to_string();
        while Instant::now() < deadline {
            ensure_current_generation(generation)?;
            let cursor = windows_net::network_event_cursors();
            match ensure_doodleray_route_preferred_native() {
                Ok(message) => {
                    log_service_event(&format!("route readiness ok: {}", message));
                    return Ok(());
                }
                Err(message) => last_error = message,
            }
            wait_for_route_event_or_delay(cursor, Duration::from_millis(175), generation)?;
        }
        let fallback_started = Instant::now();
        match ensure_doodleray_route_preferred_powershell() {
            Ok(()) => {
                record_fallback_probe("route_canaries_final", fallback_started);
                set_route_probe_backend("powershell_fallback");
                Ok(())
            }
            Err(fallback_error) => Err(format!("{}; fallback: {}", last_error, fallback_error)),
        }
    }

    fn with_tun_interface_name(mut config: Value) -> Value {
        if let Some(inbounds) = config.get_mut("inbounds").and_then(|v| v.as_array_mut()) {
            for inbound in inbounds {
                if inbound.get("type").and_then(|v| v.as_str()) == Some("tun") {
                    if let Some(obj) = inbound.as_object_mut() {
                        obj.insert(
                            "interface_name".into(),
                            Value::String("DoodleRay Tunnel".into()),
                        );
                    }
                }
            }
        }
        config
    }

    fn exe_dir() -> Result<PathBuf, String> {
        std::env::current_exe()
            .map_err(|e| e.to_string())?
            .parent()
            .map(|p| p.to_path_buf())
            .ok_or("Failed to resolve service executable directory".into())
    }

    fn singbox_exe_path() -> Result<PathBuf, String> {
        let dir = exe_dir()?;
        let path = dir.join("sing-box.exe");
        if path.exists() {
            return Ok(path);
        }
        Err(format!("sing-box executable not found at {:?}", path))
    }

    fn xray_exe_path() -> Result<PathBuf, String> {
        let path = exe_dir()?.join("xray-core").join("xray.exe");
        if path.exists() {
            Ok(path)
        } else {
            Err(format!("xray.exe not found at {:?}", path))
        }
    }

    fn sanitize_id(value: &str) -> String {
        value
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .take(80)
            .collect::<String>()
    }

    fn redact(value: &str) -> String {
        value.lines().take(12).collect::<Vec<_>>().join("\n")
    }

    fn install_service() -> windows_service::Result<()> {
        ensure_vpn_users_group()
            .map_err(|e| windows_service::Error::Winapi(std::io::Error::other(e)))?;
        let manager = ServiceManager::local_computer(
            None::<&str>,
            ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
        )?;
        let exe_path = std::env::current_exe().map_err(windows_service::Error::Winapi)?;
        let service_info = ServiceInfo {
            name: OsString::from(TUNNEL_SERVICE_NAME),
            display_name: OsString::from(TUNNEL_SERVICE_DISPLAY_NAME),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::OnDemand,
            error_control: ServiceErrorControl::Normal,
            executable_path: exe_path.clone(),
            launch_arguments: vec![OsString::from("run-service")],
            dependencies: vec![],
            account_name: None,
            account_password: None,
        };

        // Idempotent path first: if a registration already exists and points
        // at this exact binary, adopt it instead of delete+recreate. Deleting
        // a live registration only marks it delete-pending while any handle
        // is open (SCM tooling, QA scripts), and the service then silently
        // vanishes on its next stop. App startup repair may call install on
        // every transient IPC failure, so this path must never be destructive.
        if let Ok(service) = manager.open_service(
            TUNNEL_SERVICE_NAME,
            ServiceAccess::QUERY_CONFIG
                | ServiceAccess::CHANGE_CONFIG
                | ServiceAccess::STOP
                | ServiceAccess::QUERY_STATUS,
        ) {
            let matches_current_exe = service
                .query_config()
                .map(|config| {
                    config
                        .executable_path
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .contains(exe_path.to_string_lossy().to_ascii_lowercase().as_str())
                })
                .unwrap_or(false);
            if matches_current_exe {
                service.change_config(&service_info)?;
                configure_service(&service)?;
                if service
                    .query_status()
                    .map(|status| status.current_state != ServiceState::Stopped)
                    .unwrap_or(false)
                {
                    let _ = service.stop();
                    wait_for_service_state(
                        &service,
                        ServiceState::Stopped,
                        Duration::from_secs(10),
                    )?;
                }
                secure_runtime_dirs()
                    .map_err(|e| windows_service::Error::Winapi(std::io::Error::other(e)))?;
                return Ok(());
            }
        }

        repair_existing_service(&manager)?;
        let service = manager.create_service(
            &service_info,
            ServiceAccess::CHANGE_CONFIG | ServiceAccess::QUERY_STATUS,
        )?;
        configure_service(&service)?;
        secure_runtime_dirs()
            .map_err(|e| windows_service::Error::Winapi(std::io::Error::other(e)))?;
        Ok(())
    }

    fn configure_service(
        service: &windows_service::service::Service,
    ) -> windows_service::Result<()> {
        service.set_config_service_sid_info(ServiceSidType::Unrestricted)?;
        configure_service_failure_actions()
            .map_err(|e| windows_service::Error::Winapi(std::io::Error::other(e)))?;
        grant_builtin_users_start()
            .map_err(|e| windows_service::Error::Winapi(std::io::Error::other(e)))?;
        Ok(())
    }

    fn configure_service_failure_actions() -> Result<(), String> {
        let failure = Command::new("sc.exe")
            .args([
                "failure",
                TUNNEL_SERVICE_NAME,
                "reset=",
                "86400",
                "actions=",
                "restart/2000/restart/5000/\"\"/0",
            ])
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| format!("Failed to configure tunnel service recovery: {e}"))?;
        if !failure.status.success() {
            return Err(format!(
                "Failed to configure tunnel service recovery: {}{}",
                String::from_utf8_lossy(&failure.stdout),
                String::from_utf8_lossy(&failure.stderr)
            ));
        }

        let flag = Command::new("sc.exe")
            .args(["failureflag", TUNNEL_SERVICE_NAME, "0"])
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| format!("Failed to configure tunnel service recovery scope: {e}"))?;
        if flag.status.success() {
            Ok(())
        } else {
            Err(format!(
                "Failed to configure tunnel service recovery scope: {}{}",
                String::from_utf8_lossy(&flag.stdout),
                String::from_utf8_lossy(&flag.stderr)
            ))
        }
    }

    fn grant_builtin_users_start() -> Result<(), String> {
        // The service is demand-started. Standard users may only start it;
        // pipe ACL + executable-path validation still gate every command.
        let sddl = "D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLOCRRC;;;IU)(A;;CCLCSWLOCRRC;;;SU)(A;;RP;;;BU)";
        let output = Command::new("sc.exe")
            .args(["sdset", TUNNEL_SERVICE_NAME, sddl])
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| format!("Failed to configure tunnel service start access: {}", e))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "Failed to configure tunnel service start access: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    fn secure_runtime_dirs() -> Result<(), String> {
        let runtime = runtime_root();
        let root = runtime
            .parent()
            .ok_or("Failed to resolve ProgramData DoodleRay directory")?
            .to_path_buf();
        std::fs::create_dir_all(&runtime)
            .map_err(|e| format!("Failed to create runtime directory: {}", e))?;
        secure_directory_acl(&root)?;
        secure_directory_acl(&runtime)?;
        Ok(())
    }

    fn secure_directory_acl(path: &Path) -> Result<(), String> {
        // Harden the directory object only. `/T` must not be used with
        // `/inheritance:r` + container-inherit grants: on files it strips the
        // inherited ACEs while the (OI)(CI) grants carry no effective file
        // access, leaving an empty DACL that even LocalSystem cannot read
        // (unreadable service.log/session marker, "Access is denied" install
        // noise on reused machines).
        let status = Command::new("icacls")
            .arg(path)
            .args([
                "/inheritance:r",
                "/grant:r",
                "*S-1-5-18:(OI)(CI)F",
                "*S-1-5-32-544:(OI)(CI)F",
                "/remove:g",
                "*S-1-5-11",
                "*S-1-5-32-545",
                "/C",
                "/Q",
            ])
            .creation_flags(0x08000000)
            .status()
            .map_err(|e| format!("Failed to run icacls on {:?}: {}", path, e))?;
        if !status.success() {
            return Err(format!("icacls failed on {:?} with {}", path, status));
        }

        // Re-derive children from the hardened directory so files created
        // before this fix (or by other principals) become SYSTEM/Admins-only
        // through inheritance instead of keeping stale or empty DACLs.
        let has_children = std::fs::read_dir(path)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false);
        if has_children {
            let reset = Command::new("icacls")
                .arg(path.join("*"))
                .args(["/reset", "/T", "/C", "/Q"])
                .creation_flags(0x08000000)
                .status()
                .map_err(|e| format!("Failed to run icacls reset on {:?}: {}", path, e))?;
            if !reset.success() {
                log_service_event(&format!(
                    "icacls child ACL reset reported failures on {:?} with {}",
                    path, reset
                ));
            }
        }
        Ok(())
    }

    fn ensure_vpn_users_group() -> Result<(), String> {
        let _ = Command::new("net")
            .args(["localgroup", VPN_USERS_GROUP, "/add"])
            .creation_flags(0x08000000)
            .output();

        if let Some(user) = installing_user_name() {
            let _ = Command::new("net")
                .args(["localgroup", VPN_USERS_GROUP, &user, "/add"])
                .creation_flags(0x08000000)
                .output();
        }

        account_sid_sddl(VPN_USERS_GROUP).map(|_| ())
    }

    fn installing_user_name() -> Option<String> {
        let username = std::env::var("USERNAME").ok()?;
        if username.is_empty() || username.eq_ignore_ascii_case("SYSTEM") {
            return None;
        }
        match std::env::var("USERDOMAIN") {
            Ok(domain) if !domain.is_empty() => Some(format!("{}\\{}", domain, username)),
            _ => Some(username),
        }
    }

    fn repair_existing_service(manager: &ServiceManager) -> windows_service::Result<()> {
        let service = match manager.open_service(
            TUNNEL_SERVICE_NAME,
            ServiceAccess::STOP | ServiceAccess::DELETE | ServiceAccess::QUERY_STATUS,
        ) {
            Ok(service) => service,
            Err(_) => return Ok(()),
        };

        if let Ok(status) = service.query_status() {
            if status.current_state != ServiceState::Stopped {
                let _ = service.stop();
                for _ in 0..30 {
                    std::thread::sleep(Duration::from_millis(250));
                    if let Ok(status) = service.query_status() {
                        if status.current_state == ServiceState::Stopped {
                            break;
                        }
                    }
                }
            }
        }
        service.delete()?;
        std::thread::sleep(Duration::from_millis(250));
        Ok(())
    }

    fn wait_for_service_state(
        service: &windows_service::service::Service,
        expected: ServiceState,
        timeout: Duration,
    ) -> windows_service::Result<()> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(status) = service.query_status() {
                if status.current_state == expected {
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        Err(windows_service::Error::Winapi(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "Tunnel service did not reach expected state",
        )))
    }

    fn uninstall_service() -> windows_service::Result<()> {
        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
        let service = manager.open_service(
            TUNNEL_SERVICE_NAME,
            ServiceAccess::STOP | ServiceAccess::DELETE | ServiceAccess::QUERY_STATUS,
        )?;
        let _ = service.stop();
        service.delete()?;
        Ok(())
    }

    fn start_service() -> windows_service::Result<()> {
        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
        let service = manager.open_service(TUNNEL_SERVICE_NAME, ServiceAccess::START)?;
        service.start(&[] as &[&str])
    }

    fn stop_service() -> windows_service::Result<()> {
        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
        let service = manager.open_service(TUNNEL_SERVICE_NAME, ServiceAccess::STOP)?;
        service.stop().map(|_| ())
    }

    fn print_service_status() -> Result<(), Box<dyn std::error::Error>> {
        let response = tauri_app_lib::ipc::send_tunnel_command(
            &tauri_app_lib::tunnel_service::TunnelCommand::GetStatus,
        )
        .map_err(|e| format!("status IPC failed: {}", e))?;
        println!("{}", serde_json::to_string_pretty(&response)?);
        Ok(())
    }

    fn print_service_diagnostics() -> Result<(), Box<dyn std::error::Error>> {
        let response = tauri_app_lib::ipc::send_tunnel_command(
            &tauri_app_lib::tunnel_service::TunnelCommand::GetDiagnostics,
        )
        .map_err(|e| format!("diagnostics IPC failed: {}", e))?;
        println!("{}", serde_json::to_string_pretty(&response)?);
        Ok(())
    }

    fn prepare_service_update() -> Result<(), Box<dyn std::error::Error>> {
        let response = tauri_app_lib::ipc::send_tunnel_command(
            &tauri_app_lib::tunnel_service::TunnelCommand::PrepareForUpdate,
        )
        .map_err(|e| format!("prepare-update IPC failed: {}", e))?;
        println!("{}", serde_json::to_string_pretty(&response)?);
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn xray_config_check_cache_hit_skips_the_subprocess() {
            let config = serde_json::json!({ "outbounds": [{ "tag": "proxy" }] });
            let hash = hash_json_value(&config).expect("hash config");
            *xray_validation_cache().lock().unwrap() = Some(hash);

            // Paths that cannot possibly execute: only a cache hit returns Ok.
            let missing_exe = Path::new(r"Z:\doodleray-nonexistent\xray.exe");
            let config_path = Path::new(r"Z:\doodleray-nonexistent\xray_config.json");
            assert!(check_xray_config(missing_exe, config_path, &config).is_ok());

            // A config that was never validated must still attempt the spawn.
            let other = serde_json::json!({ "outbounds": [{ "tag": "direct" }] });
            let error = check_xray_config(missing_exe, config_path, &other)
                .expect_err("uncached config must attempt to spawn xray");
            assert!(
                error.contains("failed to run"),
                "expected a spawn failure, got: {error}"
            );
        }

        #[test]
        fn adapter_cleanup_wait_is_skipped_only_when_replacing_the_tunnel() {
            // Mid-restart: bring-up waits for the adapter itself, and the state
            // never rests as "disconnected", so the proof buys nothing.
            assert!(!should_wait_for_adapter_cleanup("replace_tunnel"));
            // Coming to rest: cleanup_pending must stay honest.
            assert!(should_wait_for_adapter_cleanup("stop_tunnel"));
            assert!(should_wait_for_adapter_cleanup("service_stop"));
            assert!(should_wait_for_adapter_cleanup("failed_cleanup"));
        }

        #[test]
        fn wintun_ghost_scan_runs_once_per_service_run_when_clean() {
            // First stop this service run: always scan, clean or not.
            assert!(should_scan_wintun_ghosts("stop_tunnel", false, false));
            assert!(should_scan_wintun_ghosts("stop_tunnel", true, false));
            // After one clean scan, skip further scans while nothing is wrong.
            assert!(!should_scan_wintun_ghosts("stop_tunnel", false, true));
            // Fresh evidence of trouble always forces a rescan, regardless of
            // whether this service run already scanned once before.
            assert!(should_scan_wintun_ghosts("stop_tunnel", true, true));
        }

        #[test]
        fn wintun_ghost_scan_never_runs_on_the_connect_hot_path() {
            // replace_tunnel runs on every connect with the adapter still
            // lingering for benign reasons; the bring-up retry path
            // (tun_adapter_repair) is what cleans a real ghost.
            assert!(!should_scan_wintun_ghosts("replace_tunnel", true, false));
            assert!(!should_scan_wintun_ghosts("replace_tunnel", true, true));
            assert!(should_scan_wintun_ghosts("tun_adapter_repair", true, true));
        }
    }
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    windows_service_main::main_entry()
}

#[cfg(not(windows))]
fn main() {
    println!("This service is only supported on Windows.");
}
