//! Real Windows checks supplement the shared runner suite. Failure probes keep
//! a raw SDK process alive deliberately so automatic host cleanup cannot hide a
//! gap in the firewall's lifetime.
use super::super::{policy, process, proxy};
use super::*;
use std::{
    io::Write,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};

pub(super) fn fixture_broker(parent: &Handle, nonce: &str) -> Result<Option<Broker>, String> {
    let Some(directory) = std::env::var_os("PIPELINE_NATIVE_TEST_BROKER_REQUEST") else {
        return Ok(None);
    };
    let directory = std::path::PathBuf::from(directory);
    let request = directory.join(format!("{nonce}.request"));
    let response = directory.join(format!("{nonce}.response"));
    std::fs::write(&request, serde_json::to_vec(&serde_json::json!({"parent":std::process::id(), "created":identity::created(parent)?, "nonce":nonce})).unwrap()).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + STARTUP;
    while !response.exists() {
        if Instant::now() >= deadline {
            return Err("Administrator test fixture did not launch the guard".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let response: serde_json::Value =
        serde_json::from_slice(&std::fs::read(response).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let pid = response["pid"]
        .as_u64()
        .ok_or("Fixture helper PID missing")? as u32;
    let raw = response["handle"]
        .as_u64()
        .ok_or("Fixture helper handle missing")? as usize;
    let process = Handle::new(raw as _)?;
    if unsafe { GetProcessId(process.0) } != pid {
        return Err("Fixture helper handle does not match PID".into());
    }
    // A standard user cannot open the elevated process handle. The authenticated
    // administrator-owned pipe still establishes the real helper identity.
    let name = pipe_name(std::process::id(), nonce);
    let pipe = loop {
        match Pipe::client(&name, pid) {
            Ok(pipe) => break pipe,
            Err(error) => {
                if Instant::now() >= deadline {
                    return Err(error);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    };
    let broker = Broker { pipe, process };
    broker.rpc(Request::Hello { version: 1 })?;
    Ok(Some(broker))
}

async fn ready(path: &std::path::Path) {
    tokio::time::timeout(Duration::from_secs(20), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[test]
#[ignore = "requires native Windows pipe security"]
fn pipe_waits_for_a_client_and_standard_users_cannot_forge_its_owner() {
    let parent = identity::process(std::process::id(), false).unwrap();
    let name = pipe_name(std::process::id(), &firewall::nonce().unwrap());
    let server = Pipe::server(&name, &identity::user(&parent).unwrap());
    if !identity::elevated().unwrap() {
        assert!(
            server.is_err(),
            "Standard account forged an administrator-owned guard pipe"
        );
        return;
    }
    let server = server.unwrap();
    assert!(!server.accept(std::process::id()).unwrap());
    let client = Pipe::client(&name, std::process::id()).unwrap();
    assert!(server.accept(std::process::id()).unwrap());
    assert!(server.read().unwrap().is_none());
    client.write(b"one bounded frame").unwrap();
    assert_eq!(server.receive(RPC).unwrap(), b"one bounded frame");
}

#[tokio::test]
#[ignore = "requires the real Windows sandbox and firewall guard"]
async fn incoming_ipv4_and_ipv6_connections_are_denied() {
    for ip in [
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
    ] {
        let reserve = tokio::net::TcpListener::bind(SocketAddr::new(ip, 0))
            .await
            .unwrap();
        let address = reserve.local_addr().unwrap();
        // A successful host-to-host connection is the positive control.
        assert!(tokio::net::TcpStream::connect(address).await.is_ok());
        drop(reserve);
        let root = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let proxy = proxy::Proxy::start(vec!["example.com".into()])
            .await
            .unwrap();
        let command = format!("$l=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Parse('{ip}'),{}); $l.Start(); Set-Content ready yes; while (-not (Test-Path done)) {{ Start-Sleep -Milliseconds 10 }}; $l.Stop()", address.port());
        let request =
            policy::request(root.path(), true, scratch.path(), &command, Some(&proxy)).unwrap();
        let task = tokio::spawn(process::run(request, scratch, Some(proxy)));
        ready(&root.path().join("ready")).await;
        let connected = matches!(
            tokio::time::timeout(
                Duration::from_millis(500),
                tokio::net::TcpStream::connect(address)
            )
            .await,
            Ok(Ok(_))
        );
        std::fs::write(root.path().join("done"), "done").unwrap();
        let result = task.await.unwrap().unwrap();
        assert!(!connected, "Incoming {address} was allowed: {result}");
    }
}

struct Child(mxc_sdk::v1::MxcProcess);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

#[tokio::test]
#[ignore = "requires an elevated Windows test harness to crash the guard"]
async fn guard_crash_retains_rules_until_reaping_stops_the_identity() {
    // An ordinary account runs the other tests; deliberately terminating an
    // elevated helper is reserved for the administrator fault-injection run.
    if !identity::elevated().unwrap() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let proxy = proxy::Proxy::start(vec!["example.com".into()])
        .await
        .unwrap();
    let unrelated = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let other = unrelated.local_addr().unwrap().port();
    assert!(
        tokio::net::TcpStream::connect(unrelated.local_addr().unwrap())
            .await
            .is_ok()
    );
    unrelated.accept().await.unwrap();
    let command = format!("Set-Content ready yes; while (-not (Test-Path proceed)) {{ Start-Sleep -Milliseconds 10 }}; $c=[Net.Sockets.TcpClient]::new(); try {{ $t=$c.ConnectAsync('127.0.0.1',{other}); if ($t.Wait(500) -and $c.Connected) {{ Set-Content escaped yes }} }} catch {{}} finally {{$c.Dispose()}}; Set-Content checked yes; Start-Sleep 30");
    let request =
        policy::request(root.path(), true, scratch.path(), &command, Some(&proxy)).unwrap();
    let mut guard = Guard::prepare(&request).unwrap().unwrap();
    let mut child = Child(mxc_sdk::v1::spawn(request.clone(), Default::default()).unwrap());
    guard.attach(child.0.id()).unwrap();
    child
        .0
        .take_stdin()
        .unwrap()
        .write_all(b"pipeline-command-start\n")
        .unwrap();
    ready(&root.path().join("ready")).await;
    {
        let broker = guard.broker.lock().unwrap();
        let helper = identity::process(unsafe { GetProcessId(broker.process.0) }, true).unwrap();
        assert_ne!(unsafe { TerminateProcess(helper.0, 125) }, 0);
        unsafe {
            WaitForSingleObject(helper.0, 5000);
        }
    }
    std::fs::write(root.path().join("proceed"), "go").unwrap();
    ready(&root.path().join("checked")).await;
    assert!(
        !root.path().join("escaped").exists(),
        "Guard crash removed network restrictions"
    );
    assert!(
        child.0.try_wait().unwrap().is_none(),
        "Fault probe exited before checking the retained rules"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), unrelated.accept())
            .await
            .is_err()
    );
    assert!(guard.check().is_err());
    // Starting a replacement guard must stop the abandoned identity before
    // deleting its rules. The raw SDK handle is still alive throughout.
    let replacement = Guard::prepare(&request).unwrap().unwrap();
    assert!(child.0.try_wait().unwrap().is_some());
    drop(replacement);
}

#[test]
#[ignore = "requires the real Windows firewall guard"]
fn broker_rejects_unsandboxed_and_unrelated_processes() {
    let broker = Broker::start().unwrap();
    assert!(broker
        .rpc(Request::Register {
            pid: std::process::id(),
            port: 12345
        })
        .is_err());
    let mut child = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep 20",
        ])
        .spawn()
        .unwrap();
    let result = broker.rpc(Request::Register {
        pid: child.id(),
        port: 12345,
    });
    let _ = child.kill();
    let _ = child.wait();
    assert!(result.is_err());
    assert!(broker
        .rpc(Request::Finish {
            id: "unregistered".into()
        })
        .is_err());
    let Broker { pipe, process } = broker;
    drop(pipe);
    assert_eq!(
        unsafe { WaitForSingleObject(process.0, 5000) },
        windows_sys::Win32::Foundation::WAIT_OBJECT_0,
        "Guard outlived its client connection"
    );
}
