//! Experimental PSEC launch: same native API and serialized schema as MXC, but
//! no internetClient capability for an intra-container loopback egress rule.
//! Uses pinned SDK internals only in this harness; no registry SDK edits.
use mxc_sdk::{
    process_container_common::{ProcessSecurityEnvironment, ProcessSecurityEnvironmentApi},
    process_security_environment_spec::process_security_environment_layout::*,
    v1::ContainerRequest,
};
use std::{
    collections::BTreeMap,
    io::{PipeReader, PipeWriter, Read},
    os::windows::io::AsRawHandle,
    ptr::null_mut,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{
        Authorization::ConvertStringSidToSidW, DeriveCapabilitySidsFromName, SECURITY_CAPABILITIES,
        SID_AND_ATTRIBUTES,
    },
    System::{JobObjects::*, Threading::*},
};

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
#[track_caller]
fn checked(ok: i32) -> Result<(), String> {
    if ok == 0 {
        Err(format!(
            "{} [{}]",
            std::io::Error::last_os_error(),
            std::panic::Location::caller()
        ))
    } else {
        Ok(())
    }
}
pub struct Child {
    process: Handle,
    job: Handle,
    _environment: Option<ProcessSecurityEnvironment>,
    pub stdin: Option<PipeWriter>,
    pub stdout: Option<PipeReader>,
    pub stderr: Option<PipeReader>,
}
impl Child {
    pub fn spawn(request: &ContainerRequest, loopback: bool) -> Result<Self, String> {
        Self::spawn_inner(request, loopback, None, &[], false, false)
    }
    pub fn spawn_raw(
        request: &ContainerRequest,
        sid: &str,
        extra: &[HANDLE],
    ) -> Result<Self, String> {
        Self::spawn_inner(request, false, Some(sid), extra, false, false)
    }
    pub fn spawn_lpac(request: &ContainerRequest, sid: &str) -> Result<Self, String> {
        Self::spawn_inner(request, false, Some(sid), &[], true, false)
    }
    pub fn spawn_derived(request: &ContainerRequest, sid: &str) -> Result<Self, String> {
        Self::spawn_inner(request, false, Some(sid), &[], false, true)
    }
    fn spawn_inner(
        request: &ContainerRequest,
        loopback: bool,
        raw: Option<&str>,
        extra: &[HANDLE],
        lpac_requested: bool,
        nested: bool,
    ) -> Result<Self, String> {
        let mut egress = EndpointPolicyT::default();
        egress.default_action = FilterAction::deny;
        if std::env::var("PIPELINE_PROBE_EGRESS_ALLOW").as_deref() == Ok("1") {
            egress.default_action = FilterAction::allow;
        }
        if loopback {
            let mut subnet = IpSubnetT::default();
            subnet.address = Some("127.0.0.1".into());
            subnet.prefix_length = 32;
            let mut destination = DestinationRuleT::default();
            destination.subnet = Some(Box::new(subnet));
            let mut port = PortRuleT::default();
            port.protocol = IpProtocol::tcp;
            let mut rule = EndpointRuleT::default();
            rule.destinations = Some(vec![destination]);
            rule.ports = Some(vec![port]);
            egress.allow = Some(vec![rule]);
        }
        if std::env::var("PIPELINE_PROBE_FILTERED_LOOPBACK").as_deref() == Ok("1") {
            let url = request
                .environment
                .as_deref()
                .unwrap_or_default()
                .iter()
                .find(|(key, _)| key == "http_proxy")
                .ok_or("missing diagnostic proxy")?
                .1
                .clone();
            let proxy_port: u16 = url
                .rsplit(':')
                .next()
                .ok_or("proxy port")?
                .parse()
                .map_err(|_| "proxy port")?;
            fn rule(
                address: &str,
                prefix: u8,
                protocol: IpProtocol,
                start: u16,
                end: u16,
            ) -> EndpointRuleT {
                let mut subnet = IpSubnetT::default();
                subnet.address = Some(address.into());
                subnet.prefix_length = prefix;
                let mut destination = DestinationRuleT::default();
                destination.subnet = Some(Box::new(subnet));
                let mut port = PortRuleT::default();
                port.protocol = protocol;
                port.port = start;
                port.end_port = end;
                let mut rule = EndpointRuleT::default();
                rule.destinations = Some(vec![destination]);
                rule.ports = Some(vec![port]);
                rule
            }
            egress.allow = Some(vec![rule("127.0.0.1", 32, IpProtocol::tcp, proxy_port, 0)]);
            egress.deny = Some(vec![
                rule("0.0.0.0", 0, IpProtocol::tcp, 1, proxy_port - 1),
                rule("0.0.0.0", 0, IpProtocol::tcp, proxy_port + 1, 65535),
                rule("::", 0, IpProtocol::tcp, 0, 0),
                rule("0.0.0.0", 0, IpProtocol::udp, 0, 0),
                rule("::", 0, IpProtocol::udp, 0, 0),
            ]);
        }
        let mut network = NetworkPolicyT::default();
        network.egress = Some(Box::new(egress));
        if let Some(peer) = &request.container_name {
            network.allowed_appcontainer_peer = Some(peer.clone());
        }
        if std::env::var("PIPELINE_PROBE_INGRESS_ALLOW").as_deref() == Ok("1") {
            let mut ingress = IngressPolicyT::default();
            ingress.default_action = FilterAction::allow;
            network.ingress = Some(Box::new(ingress));
        }
        if let Some(url) = request
            .network
            .as_ref()
            .and_then(|n| n.runtime_config.as_ref())
            .and_then(|r| r.network_proxy.as_ref())
        {
            let mut proxy = ProxyInfoT::default();
            proxy.url = Some(url.clone());
            network.proxy = Some(Box::new(proxy));
            network.egress = None;
            network.allowed_appcontainer_peer = Some("MXC-Loopback".into());
        }
        // Like the pinned SDK's v1 mapping, omit the optional ingress table:
        // no network capabilities / peer exemption grants external ingress.
        let mut spec = ProcessSecurityEnvironmentT::default();
        spec.version.major = 1;
        spec.version.minor = 1;
        if std::env::var("PIPELINE_PROBE_LOOPBACK_CAP").as_deref() == Ok("1") {
            spec.capabilities = Some("networkLoopback".into());
        }
        if std::env::var("PIPELINE_PROBE_PSEC_INTERNET").as_deref() == Ok("1") {
            spec.capabilities = Some("internetClient,registryRead".into());
        }
        // Capabilities and allowed_appcontainer_peer intentionally stay absent.
        spec.network_policy = Some(Box::new(network));
        if std::env::var("PIPELINE_PROBE_CAP_ONLY").as_deref() == Ok("1") {
            spec.network_policy = None;
        }
        spec.ui_restrictions = 0xff; // Job UI restrictions; Win32k initialization allowed.
        let fs = request
            .filesystem
            .as_ref()
            .ok_or("missing filesystem policy")?;
        spec.fs_read_only = Some(fs.readonly_paths.clone());
        spec.fs_read_write = Some(fs.readwrite_paths.clone());
        if !fs.denied_paths.is_empty() {
            spec.fs_deny = Some(fs.denied_paths.clone());
        }
        if std::env::var("PIPELINE_PROBE_ENUMERATE_ANCESTORS").as_deref() == Ok("1") {
            let mut ancestors = std::collections::BTreeSet::new();
            let normalize = |s: &str| {
                s.replace('/', "\\")
                    .trim_end_matches('\\')
                    .to_ascii_lowercase()
            };
            let existing: std::collections::BTreeSet<_> = fs
                .readonly_paths
                .iter()
                .chain(&fs.readwrite_paths)
                .chain(&fs.denied_paths)
                .map(|s| normalize(s))
                .collect();
            for path in fs.readonly_paths.iter().chain(&fs.readwrite_paths) {
                for parent in std::path::Path::new(path).ancestors().skip(1) {
                    let text = parent.to_string_lossy();
                    if !existing.contains(&normalize(&text)) {
                        ancestors.insert(text.replace('/', "\\"));
                    }
                }
            }
            spec.fs_enumerate = Some(ancestors.into_iter().collect());
        }
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let packed = spec.pack(&mut builder);
        finish_process_security_environment_buffer(&mut builder, packed);
        let hybrid = raw.is_some() && std::env::var("PIPELINE_PROBE_HYBRID").as_deref() == Ok("1");
        let environment = if raw.is_none() || hybrid {
            Some(
                ProcessSecurityEnvironmentApi::load()
                    .map_err(|e| e.to_string())?
                    .create(builder.finished_data(), 0)
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        let (input, stdin) = std::io::pipe().map_err(|e| e.to_string())?;
        let (stdout, output) = std::io::pipe().map_err(|e| e.to_string())?;
        let (stderr, error) = std::io::pipe().map_err(|e| e.to_string())?;
        let mut handles = vec![
            input.as_raw_handle(),
            output.as_raw_handle(),
            error.as_raw_handle(),
        ];
        handles.extend_from_slice(extra);
        for &handle in &handles {
            unsafe {
                checked(SetHandleInformation(
                    handle,
                    HANDLE_FLAG_INHERIT,
                    HANDLE_FLAG_INHERIT,
                ))?;
            }
        }
        let mut sid = null_mut();
        if let Some(raw) = raw {
            unsafe {
                checked(ConvertStringSidToSidW(
                    crate::identity::wide(raw).as_ptr(),
                    &mut sid,
                ))?;
            }
        }
        let sid = LocalSid(sid);
        let mut cap_memory = Vec::new();
        let mut registry_caps = Vec::new();
        let mut raw_capability_names = Vec::new();
        let extra_raw_capabilities = std::env::var("PIPELINE_PROBE_RAW_CAPS").unwrap_or_default();
        if raw.is_some() {
            raw_capability_names
                .extend(extra_raw_capabilities.split(',').filter(|s| !s.is_empty()));
        }
        if raw.is_some() && std::env::var("PIPELINE_PROBE_REGISTRY_READ").as_deref() == Ok("1") {
            raw_capability_names.push("registryRead");
        }
        if raw.is_some() && std::env::var("PIPELINE_PROBE_RAW_AGENTIC").as_deref() == Ok("1") {
            raw_capability_names.push("AgenticAppContainer");
        }
        if raw.is_some() {
            for (key, value) in request.environment.as_deref().unwrap_or_default() {
                if key == "PROBE_RAW_CAPABILITIES" {
                    raw_capability_names.extend(value.split(',').filter(|s| !s.is_empty()));
                }
            }
        }
        for capability_name in raw_capability_names {
            let mut groups = null_mut();
            let mut group_count = 0;
            let mut capabilities = null_mut();
            let mut capability_count = 0;
            unsafe {
                checked(DeriveCapabilitySidsFromName(
                    crate::identity::wide(capability_name).as_ptr(),
                    &mut groups,
                    &mut group_count,
                    &mut capabilities,
                    &mut capability_count,
                ))?;
                for i in 0..group_count as usize {
                    cap_memory.push(LocalSid(*groups.add(i)));
                }
                for i in 0..capability_count as usize {
                    let sid = *capabilities.add(i);
                    registry_caps.push(SID_AND_ATTRIBUTES {
                        Sid: sid,
                        Attributes: 4,
                    });
                    cap_memory.push(LocalSid(sid));
                }
                cap_memory.push(LocalSid(groups.cast()));
                cap_memory.push(LocalSid(capabilities.cast()));
            }
        }
        let caps = SECURITY_CAPABILITIES {
            AppContainerSid: sid.0,
            Capabilities: if registry_caps.is_empty() {
                null_mut()
            } else {
                registry_caps.as_mut_ptr()
            },
            CapabilityCount: registry_caps.len() as u32,
            Reserved: 0,
        };
        let env_handle = environment
            .as_ref()
            .map(|e| e.raw().0)
            .unwrap_or(null_mut());
        let lpac = std::env::var("PIPELINE_PROBE_PSEC_LPAC").as_deref() == Ok("1")
            || (raw.is_some()
                && (lpac_requested || std::env::var("PIPELINE_PROBE_LPAC").as_deref() == Ok("1")));
        let attribute_count = 2 + u32::from(hybrid) + u32::from(lpac);
        let all_packages_policy = 1u32;
        let mut attr_size = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), attribute_count, 0, &mut attr_size);
        }
        let mut storage = vec![0usize; attr_size.div_ceil(size_of::<usize>())];
        let attrs = storage.as_mut_ptr().cast();
        unsafe {
            checked(InitializeProcThreadAttributeList(
                attrs,
                attribute_count,
                0,
                &mut attr_size,
            ))?;
        }
        let _attrs = Attributes(attrs);
        unsafe {
            if lpac {
                checked(UpdateProcThreadAttribute(
                    attrs,
                    0,
                    15 | 0x20000,
                    (&all_packages_policy as *const u32).cast(),
                    size_of_val(&all_packages_policy),
                    null_mut(),
                    null_mut(),
                ))?;
            }
            if raw.is_some() {
                checked(UpdateProcThreadAttribute(
                    attrs,
                    0,
                    PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                    (&caps as *const SECURITY_CAPABILITIES).cast(),
                    size_of_val(&caps),
                    null_mut(),
                    null_mut(),
                ))?;
            }
            if environment.is_some() {
                checked(UpdateProcThreadAttribute(
                    attrs,
                    0,
                    35 | 0x20000,
                    (&env_handle as *const HANDLE).cast(),
                    size_of_val(&env_handle),
                    null_mut(),
                    null_mut(),
                ))?;
            }
            checked(UpdateProcThreadAttribute(
                attrs,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                size_of_val(handles.as_slice()),
                null_mut(),
                null_mut(),
            ))?;
        }
        let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
        startup.StartupInfo.cb = size_of_val(&startup) as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = handles[0];
        startup.StartupInfo.hStdOutput = handles[1];
        startup.StartupInfo.hStdError = handles[2];
        startup.lpAttributeList = attrs;
        let mut env = BTreeMap::new();
        for key in [
            "SystemRoot",
            "WINDIR",
            "COMSPEC",
            "PATH",
            "PATHEXT",
            "TEMP",
            "TMP",
            "USERPROFILE",
            "LOCALAPPDATA",
            "APPDATA",
        ] {
            if let Ok(value) = std::env::var(key) {
                env.insert(key.to_uppercase(), value);
            }
        }
        for (key, value) in request.environment.as_deref().unwrap_or_default() {
            env.insert(key.to_uppercase(), value.clone());
        }
        let env: Vec<u16> = env
            .iter()
            .flat_map(|(k, v)| crate::identity::wide(&format!("{k}={v}")))
            .chain(Some(0))
            .collect();
        let mut command = crate::identity::wide(&request.command);
        let cwd = crate::identity::wide(request.working_directory.as_deref().ok_or("missing cwd")?);
        let restriction = request
            .environment
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|(k, _)| k == "PROBE_RESTRICTED_SIDS")
            .map(|(_, v)| v.clone())
            .or_else(|| std::env::var("PIPELINE_PROBE_RESTRICTED_SIDS").ok());
        let restricted = restriction
            .as_deref()
            .map(|s| crate::restricted::create(s, raw))
            .transpose()?;
        unsafe {
            let job = Handle(if nested {
                null_mut()
            } else {
                CreateJobObjectW(null_mut(), null_mut())
            });
            if !nested && job.0.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            if !nested {
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                checked(SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    size_of_val(&limits) as u32,
                ))?;
                let ui = JOBOBJECT_BASIC_UI_RESTRICTIONS {
                    UIRestrictionsClass: 0xff,
                };
                checked(SetInformationJobObject(
                    job.0,
                    JobObjectBasicUIRestrictions,
                    (&ui as *const JOBOBJECT_BASIC_UI_RESTRICTIONS).cast(),
                    size_of_val(&ui) as u32,
                ))?;
            }
            let mut info: PROCESS_INFORMATION = std::mem::zeroed();
            let flags = EXTENDED_STARTUPINFO_PRESENT
                | CREATE_NO_WINDOW
                | CREATE_UNICODE_ENVIRONMENT
                | CREATE_SUSPENDED;
            let created = if let Some(token) = &restricted {
                CreateProcessAsUserW(
                    token.0,
                    null_mut(),
                    command.as_mut_ptr(),
                    null_mut(),
                    null_mut(),
                    1,
                    flags,
                    env.as_ptr().cast(),
                    cwd.as_ptr(),
                    &startup.StartupInfo,
                    &mut info,
                )
            } else {
                CreateProcessW(
                    null_mut(),
                    command.as_mut_ptr(),
                    null_mut(),
                    null_mut(),
                    1,
                    flags,
                    env.as_ptr().cast(),
                    cwd.as_ptr(),
                    &startup.StartupInfo,
                    &mut info,
                )
            };
            checked(created)?;
            let process = Handle(info.hProcess);
            let thread = Handle(info.hThread);
            if !nested && let Err(error) = checked(AssignProcessToJobObject(job.0, process.0)) {
                TerminateProcess(process.0, 1);
                return Err(error);
            }
            let child = Self {
                process,
                job,
                _environment: environment,
                stdin: Some(stdin),
                stdout: Some(stdout),
                stderr: Some(stderr),
            };
            if ResumeThread(thread.0) == u32::MAX {
                return Err(std::io::Error::last_os_error().to_string());
            }
            Ok(child)
        }
    }
    pub fn id(&self) -> u32 {
        unsafe { GetProcessId(self.process.0) }
    }
    pub fn token(&self) -> Result<serde_json::Value, String> {
        crate::identity::process_token(self.process.0)
    }
    pub fn release_environment(&mut self) {
        drop(self._environment.take());
    }
    pub fn kill(&self) {
        unsafe {
            if self.job.0.is_null() {
                TerminateProcess(self.process.0, 1);
            } else {
                TerminateJobObject(self.job.0, 1);
            }
            WaitForSingleObject(self.process.0, 5000);
        }
    }
    pub fn try_wait(&self) -> Result<Option<u32>, String> {
        unsafe {
            if WaitForSingleObject(self.process.0, 0) == WAIT_TIMEOUT {
                return Ok(None);
            }
            let mut code = 0;
            checked(GetExitCodeProcess(self.process.0, &mut code))?;
            Ok(Some(code))
        }
    }
    pub fn capture(mut self) -> Result<serde_json::Value, String> {
        drop(self.stdin.take());
        let stdout = self.stdout.take().unwrap();
        let stderr = self.stderr.take().unwrap();
        let out = std::thread::spawn(move || bounded(stdout));
        let err = std::thread::spawn(move || bounded(stderr));
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        let status = loop {
            if let Some(code) = self.try_wait()? {
                break code;
            }
            if std::time::Instant::now() > deadline {
                self.kill();
                return Err("native deadline exceeded".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        self.kill();
        Ok(
            serde_json::json!({"outcome":status,"stdout":out.join().map_err(|_|"stdout thread")??,"stderr":err.join().map_err(|_|"stderr thread")??}),
        )
    }
}
struct LocalSid(*mut std::ffi::c_void);
impl Drop for LocalSid {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}
struct Attributes(LPPROC_THREAD_ATTRIBUTE_LIST);
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.0);
        }
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        self.kill();
    }
}
pub fn bounded(mut input: impl Read) -> Result<String, String> {
    let mut bytes = Vec::new();
    let mut buf = [0; 4096];
    loop {
        let n = input.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        let keep = n.min(65536usize.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&buf[..keep]);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
