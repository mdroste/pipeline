//! Networkless raw AppContainer + local TCP relay + inherited anonymous pipes.
//! Each pipe pair can reach only the parent's fixed, admitting HTTP/SOCKS proxy.
//! No named endpoint, destination RPC, DNS RPC, socket handle, or user token crosses.
use crate::staging::copy_tree;
use mxc_sdk::v1::{ContainerRequest, FilesystemPolicy};
use serde_json::json;
use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    os::windows::io::{AsRawHandle, FromRawHandle},
    path::Path,
    time::Duration,
};
use windows_sys::Win32::Foundation::{HANDLE, SetHandleInformation};

const CHANNELS: usize = 32;

pub fn parent(python: &Path, mode: &str) -> Result<(), String> {
    if ![
        "pipe-relay",
        "pipe-relay-failure",
        "pipe-relay-cancel",
        "pipe-relay-diagnostics",
    ]
    .contains(&mode)
    {
        return Err("Unknown relay experiment".into());
    }
    let token = crate::identity::token()?;
    if token["elevated"] != false || token["appcontainer"] != false {
        return Err("Use an unelevated ordinary terminal".into());
    }
    println!("{}", json!({"parent_token":token,"mode":"pipe-relay"}));
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    let _entered = rt.enter();
    let proxy = rt.block_on(crate::proxy::Proxy::start(vec!["example.com".into()]))?;
    let profile = crate::identity::Profile::create()?;
    if std::env::var("PIPELINE_PROBE_RESTRICTED_COPIES").as_deref() == Ok("1") {
        crate::filesystem::protect(&profile.folder, &profile.sid_text()?, "0x1301bf", "LW")?;
    }
    if std::env::var("PIPELINE_PROBE_BFS").as_deref() == Ok("1") {
        return Err(
            "BFS experiment removed: grants failed and upstream reports a 25H2 driver deadlock"
                .into(),
        );
    }
    let drive = if std::env::var("PIPELINE_PROBE_DRIVE_ALIAS").as_deref() == Ok("1") {
        Some(crate::drive::Drive::create(&profile.folder)?)
    } else {
        None
    };
    let root = drive.as_ref().map(|d| &d.root).unwrap_or(&profile.folder);
    let staged = if std::env::var("PIPELINE_PROBE_STAGED_FS").as_deref() == Ok("1") {
        Some(crate::filesystem::staged(
            &profile.name,
            &profile.sid_text()?,
        )?)
    } else {
        None
    };
    // All copied files live in new profile-owned storage; no original ACL edits.
    copy_tree(python.parent().ok_or("Python path")?, &root.join("python"))?;
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    copy_tree(&source.join("target/python"), &root.join("deps"))?;
    std::fs::copy(source.join("workload.py"), root.join("workload.py"))
        .map_err(|e| e.to_string())?;
    std::fs::copy(source.join("lifecycle.py"), root.join("lifecycle.py"))
        .map_err(|e| e.to_string())?;
    std::fs::copy(source.join("diagnostics.py"), root.join("diagnostics.py"))
        .map_err(|e| e.to_string())?;
    std::fs::copy(source.join("filesystem.py"), root.join("filesystem.py"))
        .map_err(|e| e.to_string())?;
    std::fs::copy(
        std::env::current_exe().map_err(|e| e.to_string())?,
        root.join("relay.exe"),
    )
    .map_err(|e| e.to_string())?;
    let tcp4 = crate::fixture("127.0.0.1:0");
    let tcp6 = crate::fixture("[::1]:0");
    let udp = std::net::UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let udp_port = udp.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut buf = [0; 64];
        while let Ok((n, peer)) = udp.recv_from(&mut buf) {
            let _ = udp.send_to(&buf[..n], peer);
        }
    });
    let mut extras = Vec::new();
    let mut child_ends = Vec::new();
    let mut workers = Vec::new();
    let mut pairs = Vec::new();
    for _ in 0..CHANNELS {
        let (from_child, child_writer) = std::io::pipe().map_err(|e| e.to_string())?;
        let (child_reader, to_child) = std::io::pipe().map_err(|e| e.to_string())?;
        extras.extend([child_reader.as_raw_handle(), child_writer.as_raw_handle()]);
        pairs.push((
            child_reader.as_raw_handle() as usize,
            child_writer.as_raw_handle() as usize,
        ));
        child_ends.push((child_reader, child_writer));
        let port = proxy.port;
        workers.push(std::thread::spawn(move || {
            crate::transport::host(from_child, to_child, port)
        }));
    }
    let exe = root.join("relay.exe");
    let copied_python = root.join("python").join(python.file_name().unwrap());
    let script = if mode == "pipe-relay-cancel" {
        "lifecycle.py"
    } else if mode == "pipe-relay-diagnostics" {
        "diagnostics.py"
    } else {
        "workload.py"
    };
    let mut request = ContainerRequest::new(format!(
        "\"{}\" pipe-child \"{}\" \"{}\"",
        exe.display(),
        copied_python.display(),
        root.join(script).display()
    ));
    request.filesystem = Some(FilesystemPolicy::default()); // Raw token relies on profile ACLs, not PSEC.
    if std::env::var("PIPELINE_PROBE_HYBRID").as_deref() == Ok("1") {
        request.filesystem = Some(FilesystemPolicy {
            readwrite_paths: vec![root.to_string_lossy().into_owned()],
            readonly_paths: vec![std::env::var("SystemRoot").map_err(|e| e.to_string())?],
            ..Default::default()
        });
    }
    request.working_directory = Some(root.to_string_lossy().into_owned());
    request.environment = Some(vec![
        (
            "PYTHONPATH".into(),
            root.join("deps").to_string_lossy().into_owned(),
        ),
        ("PYTHONDONTWRITEBYTECODE".into(), "1".into()),
        ("PROBE_REUSABLE".into(), "1".into()),
        ("PROBE_EXE".into(), exe.to_string_lossy().into_owned()),
        ("PROBE_TCP4".into(), tcp4.to_string()),
        ("PROBE_TCP6".into(), tcp6.to_string()),
        ("PROBE_UDP".into(), udp_port.to_string()),
        ("PROBE_ALTERNATE".into(), proxy.port.to_string()),
        (
            "PROBE_PIPE_PAIRS".into(),
            serde_json::to_string(&pairs).unwrap(),
        ),
        ("no_proxy".into(), String::new()),
        ("all_proxy".into(), String::new()),
        ("TEMP".into(), root.to_string_lossy().into_owned()),
        ("TMP".into(), root.to_string_lossy().into_owned()),
        ("USERPROFILE".into(), root.to_string_lossy().into_owned()),
        ("LOCALAPPDATA".into(), root.to_string_lossy().into_owned()),
        ("APPDATA".into(), root.to_string_lossy().into_owned()),
        ("HOME".into(), root.to_string_lossy().into_owned()),
    ]);
    let outside = source
        .join("target")
        .join(format!("relay-outside-{}.txt", std::process::id()));
    if drive.is_some() {
        request
            .environment
            .as_mut()
            .unwrap()
            .push(("PROBE_DRIVE_ALIAS".into(), "1".into()));
    }
    if std::env::var("PIPELINE_PROBE_TLS_DIAGNOSTICS").as_deref() == Ok("1") {
        request
            .environment
            .as_mut()
            .unwrap()
            .push(("PROBE_TLS_DIAGNOSTICS".into(), "1".into()));
    }
    if let Some(staged) = &staged {
        request.environment.as_mut().unwrap().push((
            "PROBE_FILESYSTEM".into(),
            staged.to_string_lossy().into_owned(),
        ));
    }
    if std::env::var("PIPELINE_PROBE_LPAC_WORKLOAD").as_deref() == Ok("1") {
        request.environment.as_mut().unwrap().extend([
            ("PROBE_LPAC_WORKLOAD".into(), "1".into()),
            ("PROBE_RELAY_ONLY".into(), "1".into()),
        ]);
    }
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&outside)
        .map_err(|e| e.to_string())?
        .write_all(b"outside-copy-canary")
        .map_err(|e| e.to_string())?;
    request.environment.as_mut().unwrap().push((
        "PROBE_OUTSIDE".into(),
        outside.to_string_lossy().into_owned(),
    ));
    let marker = root.join("fault.txt");
    if mode == "pipe-relay-failure" {
        request.environment.as_mut().unwrap().push((
            "PROBE_RELAY_FAULT".into(),
            marker.to_string_lossy().into_owned(),
        ));
    }
    let child = crate::native::Child::spawn_raw(&request, &profile.sid_text()?, &extras)?;
    println!(
        "{}",
        json!({"profile":profile.name,"pid":child.id(),"transport":"32 reusable bounded inherited pipe channels"})
    );
    drop(child_ends);
    if std::env::var("PIPELINE_PROBE_LPAC_WORKLOAD").as_deref() == Ok("1") {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        let port = loop {
            if let Ok(text) = std::fs::read_to_string(root.join("relay-port.txt"))
                && let Ok(port) = text.parse::<u16>()
            {
                break port;
            }
            if std::time::Instant::now() > deadline {
                return Err("relay marker deadline".into());
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        request.command = format!(
            "\"{}\" \"{}\"",
            copied_python.display(),
            root.join(script).display()
        );
        request
            .environment
            .as_mut()
            .unwrap()
            .retain(|(k, _)| !["PROBE_PIPE_PAIRS", "PROBE_RELAY_ONLY"].contains(&k.as_str()));
        let url = format!("http://127.0.0.1:{port}");
        request.environment.as_mut().unwrap().extend([
            ("http_proxy".into(), url.clone()),
            ("https_proxy".into(), url),
        ]);
        let result = crate::native::Child::spawn_lpac(&request, &profile.sid_text()?)?.capture()?;
        child.kill();
        let relay_result = child.capture()?;
        for worker in workers {
            let _ = worker.join();
        }
        let _ = std::fs::remove_file(outside);
        println!("{}", json!({"lpac_client":result,"relay":relay_result}));
        return if result["outcome"] == 0 {
            Ok(())
        } else {
            Err("LPAC client experiment failed".into())
        };
    }
    if mode == "pipe-relay-failure" {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        let port = loop {
            if let Ok(text) = std::fs::read_to_string(root.join("relay-port.txt"))
                && let Ok(port) = text.parse::<u16>()
            {
                break port;
            }
            if std::time::Instant::now() > deadline {
                return Err("relay port marker deadline".into());
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        let connected =
            TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(2));
        println!(
            "{}",
            json!({"case":mode,"host_to_relay":format!("{connected:?}")})
        );
        if connected.is_ok() {
            return Err("host unexpectedly reached isolated relay".into());
        }
        let mut peer = crate::identity::Profile::create()?;
        peer.launch_mode(&format!("raw-peer {port}"), false)?;
        let peer_result = peer.ready()?;
        println!(
            "{}",
            json!({"case":mode,"other_container_to_relay":peer_result})
        );
        if peer_result["connected"] != false {
            return Err("other AppContainer unexpectedly reached relay".into());
        }
    }
    let fault = if mode == "pipe-relay-failure" {
        Some(std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(60);
            while !marker.exists() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(25));
            }
            drop(proxy);
            let _ = std::fs::write(marker.with_extension("txt.ack"), "closed");
        }))
    } else {
        None
    };
    if mode == "pipe-relay-cancel" {
        let deadline = std::time::Instant::now() + Duration::from_secs(25);
        let ready = loop {
            if let Ok(data) = std::fs::read(root.join("lifecycle.json"))
                && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&data)
            {
                break value;
            }
            if std::time::Instant::now() > deadline {
                return Err("lifecycle marker deadline".into());
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        let mut watched = Vec::new();
        for key in ["workload", "descendant", "detached"] {
            let pid = ready[key].as_u64().ok_or("missing lifecycle pid")? as u32;
            unsafe {
                use windows_sys::Win32::System::Threading::*;
                let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
                if handle.is_null() {
                    return Err(
                        "Could not open lifecycle control process before cancellation".into(),
                    );
                }
                watched.push((key, pid, handle));
            }
        }
        child.kill();
        for (key, pid, handle) in watched {
            unsafe {
                use windows_sys::Win32::{Foundation::*, System::Threading::*};
                let stopped = WaitForSingleObject(handle, 3000) == WAIT_OBJECT_0;
                CloseHandle(handle);
                println!(
                    "{}",
                    json!({"case":mode,"process":key,"pid":pid,"stopped":stopped,"http_before_cancel":ready["http"]})
                );
                if !stopped {
                    return Err("native job leaked a descendant".into());
                }
            }
        }
    }
    let result = child.capture();
    // Kill-on-close job includes relay, workload, and all subprocesses. Its closure
    // releases pipe handles, which unblocks host pipe readers before proxy drop.
    for worker in workers {
        let _ = worker.join();
    }
    if let Some(fault) = fault {
        let _ = fault.join();
    }
    let _ = std::fs::remove_file(outside);
    let result = result?;
    println!("{}", json!({"case":mode,"native":result}));
    if result["outcome"] != 0 && mode == "pipe-relay-diagnostics" {
        return Err("diagnostic process did not exit successfully".into());
    }
    if !["pipe-relay-cancel", "pipe-relay-diagnostics"].contains(&mode) {
        validate(&result, mode)?;
    }
    Ok(())
}

fn validate(result: &serde_json::Value, mode: &str) -> Result<(), String> {
    let rows: Vec<serde_json::Value> = result["stdout"]
        .as_str()
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let test = |name: &str| {
        rows.iter()
            .find(|row| row["test"] == name)
            .cloned()
            .unwrap_or_default()
    };
    let mut failures = Vec::new();
    for (name, expected) in [
        ("powershell_invoke_webrequest", "200"),
        ("powershell_relative_files", "ok"),
    ] {
        let row = test(name);
        if row["result"]["code"] != 0
            || row["result"]["stdout"].as_str().map(str::trim) != Some(expected)
            || row["result"]["stderr"] != ""
        {
            failures.push(name);
        }
    }
    if test("powershell_python_subprocess")["result"]["stdout"]
        .as_str()
        .map(str::trim)
        != Some("200")
        || test("powershell_python_subprocess")["result"]["stderr"] != ""
    {
        failures.push("powershell_python_output");
    }
    if result["outcome"] != 0 {
        failures.push("workload_exit");
    }
    if std::env::var("PIPELINE_PROBE_STAGED_FS").as_deref() == Ok("1")
        && test("filesystem_subprocess")["result"]["code"] != 0
    {
        failures.push("filesystem_subprocess");
    }
    if !rows.iter().any(|row| {
        row["relay_token"]["capabilities"] == json!([])
            && row["relay_token"]["appcontainer"] == true
            && row["relay_token"]["elevated"] == false
    }) {
        failures.push("relay_token");
    }
    let child_token = serde_json::from_str::<serde_json::Value>(
        test("token_in_subprocess")["result"]["stdout"]
            .as_str()
            .unwrap_or_default(),
    )
    .unwrap_or_default();
    if std::env::var_os("PIPELINE_PROBE_RESTRICTED_SIDS").is_some() {
        let relay = rows
            .iter()
            .find_map(|r| r.get("relay_token"))
            .unwrap_or(&serde_json::Value::Null);
        if relay["restricted_sids"]
            .as_array()
            .is_none_or(|s| s.is_empty())
            || child_token["restricted_sids"] != relay["restricted_sids"]
        {
            failures.push("subprocess_restricted_token");
        }
    }
    if child_token["capabilities"] != json!([])
        || child_token["elevated"] != false
        || child_token["appcontainer"] != true
    {
        failures.push("subprocess_token");
    }
    for name in ["requests_http_example.com", "requests_https_example.com"] {
        if test(name)["result"]["status"] != 200 {
            failures.push(name);
        }
    }
    for name in [
        "python_subprocess_https",
        "powershell_python_subprocess",
        "curl_http://example.com",
        "curl_https://example.com",
    ] {
        if test(name)["result"]["code"] != 0 {
            failures.push(name);
        }
    }
    for name in [
        "dns_wire_allowed_domain",
        "dns_wire_disallowed_domain",
        "dns_wire_unique_disallowed_name",
    ] {
        if test(name)["result"]["status"] != 5 {
            failures.push(name);
        }
    }
    if test("subprocess_dns")["result"]["stdout"]
        .as_str()
        .map(str::trim)
        != Some("5")
    {
        failures.push("subprocess_dns");
    }
    if test("concurrent_requests")["result"] != json!([200, 200, 200, 200, 200, 200]) {
        failures.push("concurrent_requests");
    }
    if test("reused_channels")["result"] != json!(vec![200; 48]) {
        failures.push("reused_channels");
    }
    for name in [
        "raw_public_v4",
        "raw_host_v4",
        "raw_host_v6",
        "raw_host_mapped_v6",
        "udp_host_echo",
        "udp_public_dns",
        "outside_copy_read",
        "same_identity_alternate_port",
    ] {
        if !test(name)["error"].is_string() {
            failures.push(name);
        }
    }
    if test("requests_http_example.org")["result"]["status"] != 403 {
        failures.push("disallowed_http");
    }
    if !test("requests_https_example.org")["error"]
        .as_str()
        .is_some_and(|s| s.contains("403"))
    {
        failures.push("disallowed_https");
    }
    if mode == "pipe-relay-failure" {
        if test("relay_failure_ack")["result"] != true {
            failures.push("relay_failure_ack");
        }
        if !test("relay_failure_request")["error"].is_string() {
            failures.push("relay_failure_request");
        }
        if test("relay_failure_dns")["result"]["status"] != 5 {
            failures.push("relay_failure_dns");
        }
        if !test("relay_failure_direct")["error"].is_string() {
            failures.push("relay_failure_direct");
        }
    }
    println!("{}", json!({"case":mode,"assertion_failures":failures}));
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("relay checks failed: {failures:?}"))
    }
}

pub fn child(python: &str, script: &str) -> Result<(), String> {
    let token = crate::identity::token()?;
    if token["appcontainer"] != true || token["elevated"] != false {
        return Err("Expected unelevated AppContainer".into());
    }
    let pairs: Vec<(usize, usize)> =
        serde_json::from_str(&std::env::var("PROBE_PIPE_PAIRS").map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if pairs.len() != CHANNELS {
        return Err("Invalid inherited channel count".into());
    }
    let mut channels = Vec::new();
    for (reader, writer) in pairs {
        unsafe {
            // Relay retains IPC. Ordinary subprocesses inherit only their stdio.
            if SetHandleInformation(reader as HANDLE, 1, 0) == 0
                || SetHandleInformation(writer as HANDLE, 1, 0) == 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
            channels.push((
                std::fs::File::from_raw_handle(reader as HANDLE),
                std::fs::File::from_raw_handle(writer as HANDLE),
            ));
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::fs::write(
        "relay-port.txt",
        listener.local_addr().unwrap().port().to_string(),
    )
    .map_err(|e| e.to_string())?;
    println!("{}", json!({"relay_token":token,"relay_url":url}));
    let listener = std::sync::Arc::new(listener);
    for (downstream, upstream) in channels {
        let listener = listener.clone();
        std::thread::spawn(move || crate::transport::child(downstream, upstream, listener));
    }
    if std::env::var("PROBE_RELAY_ONLY").as_deref() == Ok("1") {
        loop {
            std::thread::park_timeout(Duration::from_secs(60));
        }
    }
    let status = std::process::Command::new(python)
        .arg(script)
        .env("http_proxy", &url)
        .env("https_proxy", &url)
        .env_remove("PROBE_PIPE_PAIRS")
        .stdin(std::process::Stdio::null())
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("workload {status}"));
    }
    Ok(())
}
