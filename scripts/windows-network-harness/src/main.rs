//! Native networking experiment, deliberately separate from the Tauri app.
mod drive;
mod drive_checks;
mod filesystem;
mod identity;
mod lpac;
mod native;
mod psec;
mod relay;
mod restricted;
mod sdk_boundary;
mod staging;
mod transport;
#[rustfmt::skip]
#[path = "../../../gui/src-tauri/src/workbench/shell/proxy.rs"]
mod proxy;
mod public_web {
    use std::net::IpAddr;
    include!(concat!(env!("OUT_DIR"), "/public_ip.rs"));
}
use mxc_sdk::v1::{self, configs::*, *};
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{TcpListener, UdpSocket},
    path::PathBuf,
    time::Duration,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("HARNESS_ERROR: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("drive-checks") => return drive_checks::run(),
        Some("drive-crash-client") => {
            return drive_checks::client(std::path::Path::new(args.get(2).ok_or("profile path")?));
        }
        Some("drive-owner") => {
            return drive::owner(std::path::Path::new(args.get(2).ok_or("profile path")?));
        }
        Some("sdk-boundary") => return sdk_boundary::host(),
        Some("sdk-boundary-parent") => return sdk_boundary::parent(),
        Some("sdk-boundary-check") => return sdk_boundary::check(args.get(2).ok_or("phase")?),
        _ => {}
    }
    if args.get(1).map(String::as_str) == Some("lpac-matrix") {
        return lpac::matrix();
    }
    if args.get(1).map(String::as_str) == Some("lpac-probe") {
        return lpac::probe();
    }
    if args.get(1).map(String::as_str) == Some("psec-inner") {
        return psec::inner();
    }
    if args.get(1).map(String::as_str) == Some("psec-child") {
        return psec::derived(args.get(2).ok_or("python")?, args.get(3).ok_or("script")?);
    }
    if args.get(2).map(String::as_str) == Some("psec-lifetime") {
        return psec::lifetime(std::path::Path::new(&args[1]));
    }
    if args.get(1).map(String::as_str) == Some("raw-peer") {
        let port = args
            .get(2)
            .ok_or("port")?
            .parse::<u16>()
            .map_err(|e| e.to_string())?;
        let connected = std::net::TcpStream::connect_timeout(
            &([127, 0, 0, 1], port).into(),
            Duration::from_secs(2),
        );
        std::fs::write(args.get(3).ok_or("output")?,json!({"token":identity::token()?,"connected":connected.is_ok(),"result":format!("{connected:?}")}).to_string()).map_err(|e|e.to_string())?;
        std::thread::sleep(Duration::from_secs(10));
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("pipe-child") {
        return relay::child(args.get(2).ok_or("python")?, args.get(3).ok_or("script")?);
    }
    if args.get(2).is_some_and(|s| s.starts_with("pipe-relay")) {
        return relay::parent(std::path::Path::new(&args[1]), &args[2]);
    }
    if args.get(1).map(String::as_str) == Some("identity") {
        println!("{}", identity::token()?);
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("raw-selftest") {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let connection = std::net::TcpStream::connect_timeout(
            &listener.local_addr().unwrap(),
            Duration::from_secs(2),
        );
        let result = json!({"token":identity::token()?,"self_connect":format!("{:?}",connection.map(|_|"connected"))});
        std::fs::write(&args[2], result.to_string()).map_err(|e| e.to_string())?;
        std::thread::sleep(Duration::from_secs(10));
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("raw-appcontainer") {
        let mut profile = identity::Profile::create()?;
        profile.launch_mode("raw-selftest", false)?;
        println!("{}", profile.ready()?);
        return Ok(());
    }
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    let _entered = rt.enter();
    if args.get(1).map(String::as_str) == Some("proxy") {
        let result = rt.block_on(proxy::Proxy::start(vec!["example.com".into()]));
        let alternate = rt.block_on(proxy::Proxy::start(vec!["example.com".into()]))?;
        let ready = match &result {
            Ok(proxy) => {
                json!({"port":proxy.port,"alternate_port":alternate.port,"token":identity::token()?})
            }
            Err(error) => json!({"error":error}),
        };
        std::fs::write(&args[2], ready.to_string()).map_err(|e| e.to_string())?;
        let _proxy = result?;
        // The parent can test reverse connections from the authorized proxy
        // identity. Only loopback ports supplied in this profile's own storage.
        let folder = PathBuf::from(&args[2]).parent().unwrap().to_owned();
        let deadline = std::time::Instant::now() + Duration::from_secs(240);
        let mut last = String::new();
        while std::time::Instant::now() < deadline {
            if let Ok(data) = std::fs::read_to_string(folder.join("connect.json"))
                && data != last
            {
                last = data.clone();
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&data)
                    && let Some(port) = value["port"].as_u64().and_then(|n| u16::try_from(n).ok())
                {
                    let result = std::net::TcpStream::connect_timeout(
                        &([127, 0, 0, 1], port).into(),
                        Duration::from_secs(2),
                    )
                    .map(|_| "connected".to_owned())
                    .unwrap_or_else(|e| e.to_string());
                    std::fs::write(
                        folder.join("connect-result.json"),
                        json!({"case":value["case"],"proxy_to_workload":result}).to_string(),
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        return Ok(());
    }
    let token = identity::token()?;
    println!("{}", json!({"parent_token":token}));
    if token["elevated"] != false || token["appcontainer"] != false {
        return Err(
            "Run from an ordinary unelevated terminal, outside any parent AppContainer".into(),
        );
    }
    let python =
        PathBuf::from(args.get(1).ok_or(
            "Usage: harness.exe <absolute-python.exe> [identity|host|offline|baseline|all]",
        )?);
    let mode = args.get(2).map(String::as_str).unwrap_or("identity");
    if ![
        "identity",
        "host",
        "offline",
        "baseline",
        "all",
        "native-loopback",
    ]
    .contains(&mode)
    {
        return Err("Unknown mode".into());
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    println!("{}", json!({"probe":format!("{:?}",v1::probe(None))}));
    let tcp4 = fixture("127.0.0.1:0");
    let tcp6 = fixture("[::1]:0");
    let udp = UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let udp_port = udp.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut buffer = [0u8; 64];
        while let Ok((n, peer)) = udp.recv_from(&mut buffer) {
            let _ = udp.send_to(&buffer[..n], peer);
        }
    });
    let host_proxy = rt.block_on(proxy::Proxy::start(vec!["example.com".into()]))?;
    let mut profile = identity::Profile::create()?;
    profile.launch()?;
    let ready = profile.ready()?;
    if ready["token"]["appcontainer"] != true || ready["token"]["elevated"] != false {
        return Err("Proxy token did not match the experiment".into());
    }
    println!(
        "{}",
        json!({"proxy_profile":profile.name,"proxy_ready":ready})
    );
    let port = ready["port"].as_u64().ok_or("Proxy did not bind")? as u16;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let common = vec![
        (
            "PYTHONPATH".into(),
            root.join("target/python").to_string_lossy().into_owned(),
        ),
        ("PYTHONDONTWRITEBYTECODE".into(), "1".into()),
        ("PROBE_TCP4".into(), tcp4.to_string()),
        ("PROBE_TCP6".into(), tcp6.to_string()),
        ("PROBE_UDP".into(), udp_port.to_string()),
        ("PROBE_EXE".into(), exe.to_string_lossy().into_owned()),
        (
            "PROBE_ALTERNATE".into(),
            ready["alternate_port"].to_string(),
        ),
        ("no_proxy".into(), String::new()),
        ("all_proxy".into(), String::new()),
    ];
    if mode == "baseline" || mode == "all" {
        let output = std::process::Command::new(&python)
            .arg(root.join("workload.py"))
            .envs(common.clone())
            .env("http_proxy", host_proxy.url())
            .env("https_proxy", host_proxy.url())
            .output()
            .map_err(|e| e.to_string())?;
        println!(
            "{}",
            json!({"case":"host_baseline","status":output.status.code(),"stdout":String::from_utf8_lossy(&output.stdout),"stderr":String::from_utf8_lossy(&output.stderr)})
        );
    }
    for case in [
        "identity_strict_ingress",
        "identity",
        "identity_wrong_peer",
        "host",
        "offline",
        "native-loopback",
    ] {
        if mode != "all" && case != mode && !(mode == "identity" && case.starts_with("identity")) {
            continue;
        }
        let identity_mode = case.starts_with("identity");
        let url = if identity_mode && case != "identity_wrong_peer" {
            format!("http://127.0.0.1:{port}")
        } else {
            host_proxy.url()
        };
        let run_folder = root
            .join("target")
            .join(format!("run-{}-{case}", std::process::id()));
        std::fs::create_dir_all(&run_folder).map_err(|e| e.to_string())?;
        let mut request = ContainerRequest::new(format!(
            "\"{}\" \"{}\"",
            python.display(),
            root.join("workload.py").display()
        ));
        request.filesystem = Some(FilesystemPolicy {
            readonly_paths: vec![
                "C:\\Windows".into(),
                "C:\\Program Files".into(),
                python.parent().unwrap().to_string_lossy().into_owned(),
                root.to_string_lossy().into_owned(),
            ],
            readwrite_paths: vec![run_folder.to_string_lossy().into_owned()],
            ..Default::default()
        });
        request.working_directory = Some(root.to_string_lossy().into_owned());
        request.timeout_ms = Some(120_000);
        request.environment = Some(common.clone());
        request.environment.as_mut().unwrap().push((
            "PROBE_RUN".into(),
            run_folder.to_string_lossy().into_owned(),
        ));
        request.inherit_default_environment = Some(true);
        request.ui = Some(UiPolicy {
            disable: false,
            ..Default::default()
        });
        request.network = Some(NetworkPolicy {
            egress: Some(NetworkEgressPolicy {
                default: Some(NetworkAction::Deny),
                ..Default::default()
            }),
            ingress: Some(NetworkIngressPolicy {
                default: Some(
                    if case == "offline"
                        || case == "native-loopback"
                        || case == "identity_strict_ingress"
                    {
                        NetworkAction::Deny
                    } else {
                        NetworkAction::Allow
                    },
                ),
                host_loopback: Some(if case == "host" {
                    NetworkAction::Allow
                } else {
                    NetworkAction::Deny
                }),
            }),
            runtime_config: if case == "offline" || case == "native-loopback" {
                None
            } else {
                Some(NetworkRuntimeConfig {
                    network_proxy: Some(url),
                })
            },
        });
        if identity_mode {
            let mut network = ProcessContainerNetwork::default();
            network.allowed_proxy_peer = Some(profile.name.clone());
            let mut config = ProcessContainerConfig::default();
            config.network = Some(network);
            request.containment = Containment::ProcessContainer(config);
        }
        if case == "native-loopback" {
            if std::env::var("PIPELINE_PROBE_FILTERED_LOOPBACK").as_deref() == Ok("1") {
                request.environment.as_mut().unwrap().extend([
                    ("http_proxy".into(), host_proxy.url()),
                    ("https_proxy".into(), host_proxy.url()),
                ]);
            }
            request
                .environment
                .as_mut()
                .unwrap()
                .retain(|(key, _)| key != "PROBE_RUN");
            if std::env::var("PIPELINE_PROBE_NATIVE_PEER").as_deref() == Ok("1") {
                request.container_name = Some(profile.name.clone());
                let peer_url = format!("http://127.0.0.1:{port}");
                request.environment.as_mut().unwrap().extend([
                    ("http_proxy".into(), peer_url.clone()),
                    ("https_proxy".into(), peer_url),
                ]);
            }
            if std::env::var("PIPELINE_PROBE_NATIVE_PROXY").as_deref() == Ok("1") {
                request.network.as_mut().unwrap().runtime_config = Some(NetworkRuntimeConfig {
                    network_proxy: Some(host_proxy.url()),
                });
                request.environment.as_mut().unwrap().extend([
                    ("http_proxy".into(), host_proxy.url()),
                    ("https_proxy".into(), host_proxy.url()),
                ]);
            }
            let result = native::Child::spawn(&request, true)?.capture()?;
            println!("{}", json!({"case":case,"native":result}));
            continue;
        }
        let probe = v1::probe(Some(&request));
        println!(
            "{}",
            json!({"case":case,"request_probe":format!("{probe:?}")})
        );
        match probe {
            Ok(p)
                if p.tier == Some("base-container")
                    && p.error.is_none()
                    && p.needs_dacl_augmentation == Some(false) => {}
            _ => {
                println!(
                    "{}",
                    json!({"case":case,"skipped":"request not enforceable by native base-container"})
                );
                continue;
            }
        }
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = done.clone();
        let proxy_folder = profile.folder.clone();
        let observer = std::thread::spawn(move || {
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                if let Ok(text) = std::fs::read_to_string(run_folder.join("listener.txt"))
                    && let Ok(port) = text.parse::<u16>()
                {
                    let result = std::net::TcpStream::connect_timeout(
                        &([127, 0, 0, 1], port).into(),
                        Duration::from_secs(2),
                    )
                    .map(|_| "connected".to_owned())
                    .unwrap_or_else(|e| e.to_string());
                    println!("{}", json!({"case":case,"host_to_workload":result}));
                    let _ = std::fs::write(
                        proxy_folder.join("connect.json"),
                        json!({"case":case,"port":port}).to_string(),
                    );
                    let deadline = std::time::Instant::now() + Duration::from_secs(4);
                    while std::time::Instant::now() < deadline {
                        if let Ok(text) =
                            std::fs::read_to_string(proxy_folder.join("connect-result.json"))
                            && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
                            && value["case"] == case
                        {
                            println!("{value}");
                            let _ = std::fs::write(run_folder.join("observer.done"), "done");
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(25));
                    }
                    let _ = std::fs::write(run_folder.join("observer.done"), "timeout");
                    return;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        });
        match v1::run(request, RunOptions::default()) {
            Ok(result) => println!(
                "{}",
                json!({"case":case,"outcome":format!("{:?}",result.outcome),"warnings":result.warnings,"stdout":String::from_utf8_lossy(&result.stdout),"stderr":String::from_utf8_lossy(&result.stderr)})
            ),
            Err(error) => println!("{}", json!({"case":case,"error":error.to_string()})),
        }
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        observer.join().map_err(|_| "observer panicked")?;
    }
    Ok(())
}
fn fixture(address: &str) -> u16 {
    let listener = TcpListener::bind(address).expect("loopback fixture");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = [0; 1024];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nfixture",
            );
        }
    });
    port
}
