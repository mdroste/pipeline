//! Native security-environment lifetime probe. No production code or policy change.
use mxc_sdk::v1::{ContainerRequest, FilesystemPolicy};
use serde_json::json;
use std::{
    io::Write,
    path::Path,
    time::{Duration, Instant},
};

pub fn lifetime(python: &Path) -> Result<(), String> {
    let token = crate::identity::token()?;
    if token["elevated"] != false || token["appcontainer"] != false {
        return Err("Use a normal unelevated host".into());
    }
    println!("{}", json!({"parent_token":token}));
    let profile = crate::identity::Profile::create()?;
    let root = crate::filesystem::staged(&profile.name, &profile.sid_text()?)?;
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut request = ContainerRequest::new(format!(
        "\"{}\" \"{}\"",
        python.display(),
        source.join("psec_lifetime.py").display()
    ));
    request.filesystem = Some(FilesystemPolicy {
        readonly_paths: vec![
            python
                .parent()
                .ok_or("python parent")?
                .to_string_lossy()
                .into_owned(),
            source
                .join("psec_lifetime.py")
                .to_string_lossy()
                .into_owned(),
            root.join("readonly").to_string_lossy().into_owned(),
            root.join("work/.git").to_string_lossy().into_owned(),
            "C:\\Windows".into(),
        ],
        readwrite_paths: vec![root.join("work").to_string_lossy().into_owned()],
        denied_paths: vec![root.join("work/.secrets").to_string_lossy().into_owned()],
        ..Default::default()
    });
    request.working_directory = Some(root.join("work").to_string_lossy().into_owned());
    if std::env::var("PIPELINE_PROBE_DERIVED").as_deref() == Ok("1") {
        request.command = format!(
            "\"{}\" psec-child \"{}\" \"{}\"",
            exe.display(),
            python.display(),
            source.join("psec_lifetime.py").display()
        );
    }
    request
        .filesystem
        .as_mut()
        .unwrap()
        .readonly_paths
        .push(exe.to_string_lossy().into_owned());
    if std::env::var("PIPELINE_PROBE_DENY_AMBIENT").as_deref() == Ok("1") {
        request
            .filesystem
            .as_mut()
            .unwrap()
            .denied_paths
            .push(root.join("outside").to_string_lossy().into_owned());
    }
    request.environment = Some(vec![
        (
            "PROBE_FILESYSTEM".into(),
            root.to_string_lossy().into_owned(),
        ),
        (
            "PROBE_HOST_TCP".into(),
            crate::fixture("127.0.0.1:0").to_string(),
        ),
        ("PROBE_EXE".into(), exe.to_string_lossy().into_owned()),
    ]);
    if std::env::var("PIPELINE_PROBE_CHILD_SID").as_deref() == Ok("1") {
        request
            .environment
            .as_mut()
            .unwrap()
            .push(("PROBE_CHILD_SID".into(), "1".into()));
    }
    if std::env::var("PIPELINE_PROBE_CHILD_PROFILE").as_deref() == Ok("1") {
        request
            .environment
            .as_mut()
            .unwrap()
            .push(("PROBE_CHILD_PROFILE".into(), "1".into()));
    }
    if std::env::var("PIPELINE_PROBE_CHILD_AGENTIC").as_deref() == Ok("1") {
        request
            .environment
            .as_mut()
            .unwrap()
            .push(("PIPELINE_PROBE_RAW_AGENTIC".into(), "1".into()));
    }
    let mut child = crate::native::Child::spawn(
        &request,
        std::env::var("PIPELINE_PROBE_ALLOW_SELF").as_deref() == Ok("1"),
    )?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while !root.join("work/ready.txt").exists() {
        if child.try_wait()?.is_some() {
            return Err(format!("early child exit: {}", child.capture()?));
        }
        if Instant::now() > deadline {
            child.kill();
            return Err(format!("ready deadline: {}", child.capture()?));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    if std::env::var("PIPELINE_PROBE_PSEC_RAW_PEER").as_deref() == Ok("1") {
        let token = child.token()?;
        let sid = token["sid"].as_str().ok_or("PSEC SID")?;
        let dir = root.join("raw-peer");
        std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
        crate::filesystem::protect(&dir, sid, "0x1200a9", "ME")?;
        let peer_exe = dir.join("peer.exe");
        std::fs::copy(&exe, &peer_exe).map_err(|e| e.to_string())?;
        let mut peer = ContainerRequest::new(format!("\"{}\" lpac-probe", peer_exe.display()));
        peer.filesystem = Some(FilesystemPolicy::default());
        peer.working_directory = Some(dir.to_string_lossy().into_owned());
        peer.environment = Some(vec![
            (
                "PROBE_FILESYSTEM".into(),
                root.to_string_lossy().into_owned(),
            ),
            (
                "PROBE_HOST_TCP".into(),
                crate::fixture("127.0.0.1:0").to_string(),
            ),
            (
                "PROBE_HOST_TCP6".into(),
                crate::fixture("[::1]:0").to_string(),
            ),
        ]);
        let result = crate::native::Child::spawn_raw(&peer, sid, &[]).and_then(|p| p.capture());
        println!("{}", json!({"host_launched_same_sid_peer":result}));
    }
    child.release_environment();
    child
        .stdin
        .as_mut()
        .ok_or("stdin")?
        .write_all(b"go\n")
        .map_err(|e| e.to_string())?;
    println!("{}", json!({"psec_lifetime":child.capture()?}));
    Ok(())
}

pub fn derived(python: &str, script: &str) -> Result<(), String> {
    let token = crate::identity::token()?;
    println!("{}", json!({"psec_parent_token":token}));
    let mut request = ContainerRequest::new(format!("\"{python}\" \"{script}\""));
    request.filesystem = Some(FilesystemPolicy::default());
    request.working_directory = Some(
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned(),
    );
    request.environment = Some(std::env::vars().collect());
    let sid = token["sid"].as_str().ok_or("PSEC SID")?;
    let profile = if std::env::var("PROBE_CHILD_PROFILE").as_deref() == Ok("1") {
        Some(crate::identity::Profile::create()?)
    } else {
        None
    };
    let sid = if let Some(profile) = &profile {
        let exe = profile.folder.join("inner.exe");
        std::fs::copy(std::env::current_exe().map_err(|e| e.to_string())?, &exe)
            .map_err(|e| e.to_string())?;
        request.command = format!("\"{}\" psec-inner", exe.display());
        request.working_directory = Some(profile.folder.to_string_lossy().into_owned());
        profile.sid_text()?
    } else if std::env::var("PROBE_CHILD_SID").as_deref() == Ok("1") {
        crate::identity::restricted_sid(sid)?
    } else {
        sid.to_owned()
    };
    println!("{}", json!({"derived_sid":sid}));
    let mut child = crate::native::Child::spawn_derived(&request, &sid)?;
    let mut stdin = child.stdin.take().ok_or("stdin")?;
    let forward = std::thread::spawn(move || std::io::copy(&mut std::io::stdin(), &mut stdin));
    let result = child.capture()?;
    print!("{}", result["stdout"].as_str().unwrap_or_default());
    eprint!("{}", result["stderr"].as_str().unwrap_or_default());
    if result["outcome"] != 0 {
        return Err(format!("derived child: {}", result["outcome"]));
    }
    let _ = forward.join();
    Ok(())
}

pub fn inner() -> Result<(), String> {
    println!("{}", json!({"inner_token":crate::identity::token()?}));
    let root =
        std::path::PathBuf::from(std::env::var("PROBE_FILESYSTEM").map_err(|e| e.to_string())?);
    for phase in ["before", "after"] {
        let self_result = (|| -> std::io::Result<()> {
            let server = std::net::TcpListener::bind("127.0.0.1:0")?;
            println!("{}", json!({"phase":phase,"bind":"ok"}));
            std::net::TcpStream::connect_timeout(&server.local_addr()?, Duration::from_secs(2))?;
            Ok(())
        })();
        println!(
            "{}",
            json!({"phase":phase,"self_connect":format!("{self_result:?}")})
        );
        for name in ["readonly", "work/.git", "work/.secrets", "outside"] {
            let read = std::fs::read_to_string(root.join(name).join("canary.txt"));
            println!(
                "{}",
                json!({"phase":phase,"path":name,"read":format!("{read:?}")})
            );
        }
        #[link(name = "dnsapi")]
        unsafe extern "system" {
            fn DnsQuery_W(
                name: *const u16,
                kind: u16,
                options: u32,
                servers: *const std::ffi::c_void,
                records: *mut *mut std::ffi::c_void,
                reserved: *mut std::ffi::c_void,
            ) -> i32;
            fn DnsRecordListFree(records: *mut std::ffi::c_void, kind: i32);
        }
        unsafe {
            let mut records = std::ptr::null_mut();
            let status = DnsQuery_W(
                crate::identity::wide("example.org").as_ptr(),
                1,
                0x148,
                std::ptr::null(),
                &mut records,
                std::ptr::null_mut(),
            );
            if !records.is_null() {
                DnsRecordListFree(records, 1);
            }
            println!("{}", json!({"phase":phase,"dns":status}));
        }
        if phase == "before" {
            std::fs::write(root.join("work/ready.txt"), "ready").map_err(|e| e.to_string())?;
            let mut line = String::new();
            std::io::stdin()
                .read_line(&mut line)
                .map_err(|e| e.to_string())?;
            if line.trim() != "go" {
                return Err("missing gate".into());
            }
        }
    }
    Ok(())
}
