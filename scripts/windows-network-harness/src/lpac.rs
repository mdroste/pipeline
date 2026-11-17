//! Observational capability matrix; an exit code of zero is not qualification.
use mxc_sdk::v1::{ContainerRequest, FilesystemPolicy};
use serde_json::json;
use std::{
    net::{TcpListener, TcpStream, UdpSocket},
    time::Duration,
};

pub fn matrix() -> Result<(), String> {
    let token = crate::identity::token()?;
    if token["elevated"] != false || token["appcontainer"] != false {
        return Err("Use a normal unelevated host".into());
    }
    println!("{}", json!({"parent_token":token}));
    let profile = crate::identity::Profile::create()?;
    if std::env::var("PIPELINE_PROBE_RESTRICTED_COPIES").as_deref() == Ok("1") {
        crate::filesystem::protect(&profile.folder, &profile.sid_text()?, "0x1301bf", "LW")?;
    }
    let root = crate::filesystem::staged(&profile.name, &profile.sid_text()?)?;
    let exe = profile.folder.join("lpac-probe.exe");
    std::fs::copy(std::env::current_exe().map_err(|e| e.to_string())?, &exe)
        .map_err(|e| e.to_string())?;
    let host = crate::fixture("127.0.0.1:0");
    let host6 = crate::fixture("[::1]:0");
    let cases = [
        ("raw-control", false, ""),
        ("lpac-empty", true, ""),
        ("lpac-registry", true, "registryRead"),
        ("lpac-web", true, "registryRead,lpacWebPlatform"),
        ("lpac-com", true, "registryRead,lpacCom"),
        ("lpac-services", true, "registryRead,lpacServicesManagement"),
        (
            "lpac-nonnetwork",
            true,
            "registryRead,lpacWebPlatform,lpacCom,lpacServicesManagement,lpacAppExperience,lpacIdentityServices,lpacCryptoServices,lpacEnterprisePolicyChangeNotifications",
        ),
        ("lpac-loopback", true, "registryRead,networkLoopback"),
        ("lpac-proximity", true, "registryRead,proximity"),
        ("lpac-internet", true, "registryRead,internetClient"),
    ];
    for (name, lpac, capabilities) in cases {
        if let Ok(filter) = std::env::var("PIPELINE_PROBE_MATRIX_FILTER")
            && !name.contains(&filter)
        {
            continue;
        }
        let mut request = ContainerRequest::new(format!("\"{}\" lpac-probe", exe.display()));
        if std::env::var("PIPELINE_PROBE_MATRIX_BOUNDARY").as_deref() == Ok("1") {
            request.command = format!("\"{}\" sdk-boundary-parent", exe.display());
        }
        request.filesystem = Some(FilesystemPolicy::default());
        request.working_directory = Some(profile.folder.to_string_lossy().into_owned());
        request.environment = Some(vec![
            ("PROBE_RAW_CAPABILITIES".into(), capabilities.into()),
            ("PROBE_HOST_TCP".into(), host.to_string()),
            ("PROBE_HOST_TCP6".into(), host6.to_string()),
            (
                "PROBE_FILESYSTEM".into(),
                root.to_string_lossy().into_owned(),
            ),
        ]);
        let child = if lpac {
            crate::native::Child::spawn_lpac(&request, &profile.sid_text()?)
        } else {
            crate::native::Child::spawn_raw(&request, &profile.sid_text()?, &[])
        };
        let result = child.and_then(|c| c.capture());
        println!(
            "{}",
            json!({"case":name,"capabilities":capabilities,"result":result})
        );
    }
    Ok(())
}

pub fn probe() -> Result<(), String> {
    println!("{}", json!({"token":crate::identity::token()?}));
    let self_result = (|| -> std::io::Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        println!("{}", json!({"bind":"ok"}));
        TcpStream::connect_timeout(&listener.local_addr()?, Duration::from_secs(2))?;
        Ok(())
    })();
    println!("{}", json!({"self":format!("{self_result:?}")}));
    for (name, address) in [
        (
            "host4",
            format!("127.0.0.1:{}", std::env::var("PROBE_HOST_TCP").unwrap()),
        ),
        (
            "host6",
            format!("[::1]:{}", std::env::var("PROBE_HOST_TCP6").unwrap()),
        ),
        ("public", "1.1.1.1:443".into()),
    ] {
        let result = TcpStream::connect_timeout(
            &address.parse().map_err(|e| format!("{e}"))?,
            Duration::from_secs(2),
        );
        println!(
            "{}",
            json!({"network":name,"result":format!("{:?}",result.map(|_|()))})
        );
    }
    println!(
        "{}",
        json!({"udp_bind":format!("{:?}",UdpSocket::bind("127.0.0.1:0").map(|_|()))})
    );
    let root = std::path::PathBuf::from(std::env::var("PROBE_FILESYSTEM").unwrap());
    for name in ["readonly", "work/.git", "work/.secrets", "outside"] {
        println!(
            "{}",
            json!({"read":name,"result":format!("{:?}",std::fs::read_to_string(root.join(name).join("canary.txt")))})
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
        println!("{}", json!({"dns":status}));
    }
    Ok(())
}
