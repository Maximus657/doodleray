#[cfg(windows)]
mod windows_service_main {
    use serde_json::Value;
    use std::ffi::OsString;
    use std::fs::OpenOptions;
    use std::mem::{size_of, zeroed};
    use std::net::TcpStream;
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
        runtime_root, StartTunnelRequest, StopTunnelRequest, TunnelCommand, TunnelEngineKind,
        TunnelDiagnostics, TunnelResponse, TunnelState, TunnelStatus, TUNNEL_PIPE_NAME,
        TUNNEL_PROTOCOL_VERSION, TUNNEL_SERVICE_DISPLAY_NAME, TUNNEL_SERVICE_NAME,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::{PipeMode, ServerOptions};
    use windows_service::define_windows_service;
    use windows_service::service::{
        ServiceAccess, ServiceAction, ServiceActionType, ServiceControl, ServiceControlAccept,
        ServiceErrorControl, ServiceExitCode, ServiceFailureActions, ServiceFailureResetPeriod,
        ServiceInfo, ServiceSidType, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
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
    static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
    static OP_GENERATION: AtomicU64 = AtomicU64::new(0);
    const PIPE_WORKERS: usize = 4;
    const VPN_USERS_GROUP: &str = "DoodleRay VPN Users";

    define_windows_service!(ffi_service_main, service_main);

    struct TunnelRuntime {
        state: TunnelState,
        phase: Option<String>,
        active_op_id: Option<String>,
        error: Option<String>,
        timings_ms: Vec<(String, u64)>,
        xray: Option<Child>,
        singbox: Option<Child>,
        job: Option<JobHandle>,
    }

    impl Default for TunnelRuntime {
        fn default() -> Self {
            Self {
                state: TunnelState::Disconnected,
                phase: None,
                active_op_id: None,
                error: None,
                timings_ms: Vec::new(),
                xray: None,
                singbox: None,
                job: None,
            }
        }
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
        let status_handle = service_control_handler::register(
            TUNNEL_SERVICE_NAME,
            move |control_event| match control_event {
                ServiceControl::Stop | ServiceControl::Interrogate => {
                    if matches!(control_event, ServiceControl::Stop) {
                        stopped_for_handler.store(true, Ordering::SeqCst);
                    }
                    ServiceControlHandlerResult::NoError
                }
                _ => ServiceControlHandlerResult::NotImplemented,
            },
        )?;

        set_service_status(&status_handle, ServiceState::Running, 0)?;
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|_| windows_service::Error::Winapi(std::io::Error::last_os_error()))?;
        runtime.block_on(async move {
            if let Err(e) = secure_runtime_dirs() {
                eprintln!("failed to secure runtime dirs: {}", e);
                STOP_REQUESTED.store(true, Ordering::SeqCst);
                return;
            }
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
        handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint,
            wait_hint: Duration::from_secs(2),
            process_id: None,
        })
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
        let ok =
            unsafe { ConvertSidToStringSidW(sid.as_mut_ptr() as PSID, &mut sid_string_ptr) };
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
        let sid_string = unsafe {
            String::from_utf16_lossy(std::slice::from_raw_parts(sid_string_ptr, len))
        };
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
            log_service_event(&format!("pipe client validation warning: {}", message));
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
        if matches!(client_name.as_str(), "doodleray.exe" | "doodlerayservice.exe") {
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
                        message: format!(
                            "Unsupported tunnel protocol {}",
                            hello.protocol_version
                        ),
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
                    runtime.state = TunnelState::Connecting;
                    runtime.phase = Some("queued".into());
                    runtime.active_op_id = Some(request.op_id.clone());
                    runtime.error = None;
                    runtime.timings_ms.clear();
                }
                std::thread::spawn(move || {
                    if let Err(message) = start_tunnel(request, generation) {
                        if is_current_generation(generation) {
                            let _ = stop_owned_processes("failed_cleanup");
                            set_failed(&message);
                        }
                    }
                });
                TunnelResponse::Status(status_snapshot())
            }
            TunnelCommand::StopTunnel(request) => match stop_tunnel(request) {
                Ok(status) => TunnelResponse::Status(status),
                Err(message) => TunnelResponse::Error { message },
            },
            TunnelCommand::PrepareForUpdate => {
                log_service_event("PrepareForUpdate requested");
                OP_GENERATION.fetch_add(1, Ordering::SeqCst);
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

    fn status_snapshot() -> TunnelStatus {
        let runtime = state().lock().unwrap();
        TunnelStatus {
            protocol_version: TUNNEL_PROTOCOL_VERSION,
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            state: runtime.state.clone(),
            phase: runtime.phase.clone(),
            active_op_id: runtime.active_op_id.clone(),
            error: runtime.error.clone(),
            timings_ms: runtime.timings_ms.clone(),
        }
    }

    fn collect_diagnostics() -> TunnelDiagnostics {
        TunnelDiagnostics {
            status: status_snapshot(),
            log_tail: sanitized_log_tail(),
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
        lines.into_iter().rev().take(240).collect::<Vec<_>>().into_iter().rev().collect()
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
            runtime.state = TunnelState::Connecting;
            runtime.phase = Some("stopping_previous".into());
            runtime.active_op_id = Some(request.op_id.clone());
            runtime.error = None;
            runtime.timings_ms.clear();
        }
        stop_owned_processes("replace_tunnel")?;
        ensure_current_generation(generation)?;
        {
            let mut runtime = state().lock().unwrap();
            runtime.job = Some(create_kill_on_close_job()?);
            runtime.state = TunnelState::Connecting;
            runtime.phase = Some("starting_job".into());
            runtime.active_op_id = Some(request.op_id.clone());
        }

        let runtime_dir = runtime_root().join(sanitize_id(&request.op_id));
        std::fs::create_dir_all(&runtime_dir)
            .map_err(|e| format!("Failed to create runtime dir: {}", e))?;

        let singbox_config = with_tun_interface_name(request.singbox_config.clone());
        if matches!(request.engine_kind, TunnelEngineKind::XrayTun) {
            set_phase("starting_xray", started, generation)?;
            let xray_config = request
                .xray_config
                .as_ref()
                .ok_or("xray_config is required for xray_tun")?;
            let xray_config_path = runtime_dir.join("xray_config.json");
            write_json_file(&xray_config_path, xray_config)?;
            let xray_log_path = runtime_dir.join("xray.log");
            let child = spawn_engine(xray_exe_path()?, &["run", "-c"], &xray_config_path, &xray_log_path)?;
            assign_child_to_job(&child)?;
            state().lock().unwrap().xray = Some(child);
            wait_for_port(request.socks_port, Duration::from_secs(8), generation)?;
            set_phase("xray_ready", started, generation)?;
        }

        set_phase("starting_tun", started, generation)?;
        let singbox_config_path = runtime_dir.join("singbox_tun_config.json");
        write_json_file(&singbox_config_path, &singbox_config)?;
        let singbox_log_path = runtime_dir.join("singbox_tun.log");
        let singbox_exe = singbox_exe_path()?;
        check_singbox_config(&singbox_exe, &singbox_config_path, &runtime_dir)?;
        let child = spawn_engine(
            singbox_exe,
            &["run", "-c"],
            &singbox_config_path,
            &singbox_log_path,
        )?;
        assign_child_to_job(&child)?;
        state().lock().unwrap().singbox = Some(child);

        set_phase("waiting_adapter", started, generation)?;
        wait_for_adapter("DoodleRay Tunnel", Duration::from_secs(12), generation)?;
        ensure_singbox_alive(&singbox_log_path)?;
        set_phase("singbox_ready", started, generation)?;
        set_doodleray_interface_metric();
        set_phase("adapter_ready", started, generation)?;
        ensure_doodleray_route_preferred()?;
        set_phase("routes_ready", started, generation)?;
        ensure_current_generation(generation)?;

        let mut runtime = state().lock().unwrap();
        runtime.state = TunnelState::Connected;
        runtime.phase = Some("connected".into());
        runtime
            .timings_ms
            .push(("total_connect".into(), elapsed_ms(started)));
        log_service_event(&format!(
            "tunnel connected generation={} total_connect_ms={}",
            generation,
            elapsed_ms(started)
        ));
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

    fn stop_owned_processes(reason: &str) -> Result<(), String> {
        log_service_event(&format!("stop_owned_processes reason={}", reason));
        let (mut singbox, mut xray, job) = {
            let mut runtime = state().lock().unwrap();
            runtime.state = TunnelState::Disconnecting;
            runtime.phase = Some(reason.into());
            (runtime.singbox.take(), runtime.xray.take(), runtime.job.take())
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
        runtime.phase = None;
        runtime.active_op_id = None;
        runtime.error = None;
        Ok(())
    }

    fn set_failed(message: &str) {
        let mut runtime = state().lock().unwrap();
        runtime.state = TunnelState::Failed;
        runtime.phase = Some("failed".into());
        runtime.error = Some(redact(message));
    }

    fn clear_timings() {
        state().lock().unwrap().timings_ms.clear();
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
    ) -> Result<(), String> {
        let output = Command::new(exe)
            .args(["check", "-c"])
            .arg(config_path)
            .current_dir(runtime_dir)
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| format!("sing-box check failed to run: {}", e))?;
        if output.status.success() {
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

    fn ensure_singbox_alive(log_path: &Path) -> Result<(), String> {
        {
            let mut runtime = state().lock().unwrap();
            if let Some(child) = runtime.singbox.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    let log = std::fs::read_to_string(log_path).unwrap_or_default();
                    return Err(format!(
                        "sing-box exited with {}: {}",
                        status,
                        redact(&log)
                    ));
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

    fn wait_for_adapter(adapter_name: &str, timeout: Duration, generation: u64) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            ensure_current_generation(generation)?;
            let output = Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!(
                        "Get-NetAdapter -Name '{}' -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Name",
                        adapter_name.replace('\'', "''")
                    ),
                ])
                .creation_flags(0x08000000)
                .output();
            if let Ok(output) = output {
                let text = String::from_utf8_lossy(&output.stdout);
                if text.contains(adapter_name) {
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        Err("DoodleRay Tunnel adapter did not become ready".into())
    }

    fn set_doodleray_interface_metric() {
        let script = "Set-NetIPInterface -InterfaceAlias 'DoodleRay Tunnel' -AddressFamily IPv4 -InterfaceMetric 1 -ErrorAction SilentlyContinue; Set-NetIPInterface -InterfaceAlias 'DoodleRay Tunnel' -AddressFamily IPv6 -InterfaceMetric 1 -ErrorAction SilentlyContinue";
        match Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .creation_flags(0x08000000)
            .output()
        {
            Ok(output) if output.status.success() => {
                log_service_event("applied DoodleRay Tunnel interface metric");
            }
            Ok(output) => {
                log_service_event(&format!(
                    "failed to apply interface metric: {}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            Err(e) => log_service_event(&format!("failed to run metric command: {}", e)),
        }
    }

    fn ensure_doodleray_route_preferred() -> Result<(), String> {
        let script = r#"
$tunIface = Get-NetIPInterface -InterfaceAlias 'DoodleRay Tunnel' -AddressFamily IPv4 -ErrorAction SilentlyContinue
if (-not $tunIface) {
  Write-Output 'DoodleRay Tunnel IPv4 interface metric is missing'
  exit 2
}

$tunDefaultRoute = Get-NetRoute -DestinationPrefix '0.0.0.0/0' -InterfaceAlias 'DoodleRay Tunnel' -ErrorAction SilentlyContinue | Sort-Object RouteMetric | Select-Object -First 1
$tunSplitRoutes = @(
  Get-NetRoute -DestinationPrefix '0.0.0.0/1' -InterfaceAlias 'DoodleRay Tunnel' -ErrorAction SilentlyContinue | Sort-Object RouteMetric | Select-Object -First 1
  Get-NetRoute -DestinationPrefix '128.0.0.0/1' -InterfaceAlias 'DoodleRay Tunnel' -ErrorAction SilentlyContinue | Sort-Object RouteMetric | Select-Object -First 1
) | Where-Object { $_ }

$tunRoutes = @()
if ($tunDefaultRoute) {
  $tunRoutes += $tunDefaultRoute
} elseif ($tunSplitRoutes.Count -eq 2) {
  $tunRoutes += $tunSplitRoutes
} else {
  Write-Output 'DoodleRay Tunnel IPv4 default or split routes are missing'
  exit 2
}

$tunEffective = ($tunRoutes | ForEach-Object { [int]$_.RouteMetric + [int]$tunIface.InterfaceMetric } | Measure-Object -Maximum).Maximum
$bestOther = Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue |
  Where-Object { $_.InterfaceAlias -ne 'DoodleRay Tunnel' } |
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
if ($bestOther -and $tunEffective -ge [int]$bestOther.Effective) {
  Write-Output ("DoodleRay Tunnel route is not preferred: tun={0}, other={1}:{2}" -f $tunEffective, $bestOther.Alias, $bestOther.Effective)
  exit 3
}
$routeShape = if ($tunDefaultRoute) { 'default' } else { 'split' }
Write-Output ("DoodleRay Tunnel route preferred: shape={0}, tun={1}, best_other={2}" -f $routeShape, $tunEffective, $(if ($bestOther) { "$($bestOther.Alias):$($bestOther.Effective)" } else { 'none' }))
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

    fn with_tun_interface_name(mut config: Value) -> Value {
        if let Some(inbounds) = config.get_mut("inbounds").and_then(|v| v.as_array_mut()) {
            for inbound in inbounds {
                if inbound.get("type").and_then(|v| v.as_str()) == Some("tun") {
                    if let Some(obj) = inbound.as_object_mut() {
                        obj.insert("interface_name".into(), Value::String("DoodleRay Tunnel".into()));
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
        ensure_vpn_users_group().map_err(|e| {
            windows_service::Error::Winapi(std::io::Error::new(std::io::ErrorKind::Other, e))
        })?;
        let manager = ServiceManager::local_computer(
            None::<&str>,
            ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
        )?;
        repair_existing_service(&manager)?;
        let exe_path = std::env::current_exe().map_err(windows_service::Error::Winapi)?;
        let service_info = ServiceInfo {
            name: OsString::from(TUNNEL_SERVICE_NAME),
            display_name: OsString::from(TUNNEL_SERVICE_DISPLAY_NAME),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: exe_path,
            launch_arguments: vec![OsString::from("run-service")],
            dependencies: vec![],
            account_name: None,
            account_password: None,
        };
        let service = manager.create_service(
            &service_info,
            ServiceAccess::START | ServiceAccess::CHANGE_CONFIG | ServiceAccess::QUERY_STATUS,
        )?;
        service.set_config_service_sid_info(ServiceSidType::Unrestricted)?;
        service.update_failure_actions(ServiceFailureActions {
            reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(24 * 60 * 60)),
            reboot_msg: None,
            command: None,
            actions: Some(vec![
                ServiceAction {
                    action_type: ServiceActionType::Restart,
                    delay: Duration::from_secs(2),
                },
                ServiceAction {
                    action_type: ServiceActionType::Restart,
                    delay: Duration::from_secs(5),
                },
                ServiceAction {
                    action_type: ServiceActionType::None,
                    delay: Duration::from_secs(0),
                },
            ]),
        })?;
        service.set_failure_actions_on_non_crash_failures(true)?;
        let _ = service.start(&[] as &[&str]);
        wait_for_service_state(&service, ServiceState::Running, Duration::from_secs(10))?;
        secure_runtime_dirs().map_err(|e| {
            windows_service::Error::Winapi(std::io::Error::new(std::io::ErrorKind::Other, e))
        })?;
        Ok(())
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
                "/T",
                "/C",
                "/Q",
            ])
            .creation_flags(0x08000000)
            .status()
            .map_err(|e| format!("Failed to run icacls on {:?}: {}", path, e))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("icacls failed on {:?} with {}", path, status))
        }
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
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    windows_service_main::main_entry()
}

#[cfg(not(windows))]
fn main() {
    println!("This service is only supported on Windows.");
}
