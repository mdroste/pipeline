use super::*;
use std::time::Duration;

#[test]
#[ignore = "requires the real native OS sandbox outside the test runner sandbox"]
fn native_workload_waits_for_parent_start_signal() {
    use std::io::Write;
    let root = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    #[cfg(not(windows))]
    let command = "echo ran > started.txt";
    #[cfg(windows)]
    let command = "Set-Content started.txt ran";
    let request = policy::request(root.path(), true, scratch.path(), command, None).unwrap();
    let mut child = mxc_sdk::v1::spawn(request, Default::default()).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert!(!root.path().join("started.txt").exists());
    let mut stdin = child.take_stdin().unwrap();
    stdin.write_all(b"pipeline-command-start\n").unwrap();
    drop(stdin);
    child.wait().unwrap();
    assert!(root.path().join("started.txt").exists());
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "internal child entry point for native Linux sandbox probes"]
fn linux_command_guard_entry() {
    let command = std::env::var("PIPELINE_TEST_COMMAND").expect("Internal command is missing");
    super::linux::execute(&command);
}

#[test]
fn domain_policy_is_checked_before_launch() {
    assert!(policy::domains(&[], true)
        .unwrap_err()
        .contains("Allowed command sites"));
    for bad in [
        "*",
        "example.org:443",
        "127.0.0.1",
        "a..org",
        "*.org",
        "Example.org",
        "example.org/path",
    ] {
        assert!(policy::domains(&[bad.into()], true).is_err(), "{bad}");
    }
    assert!(policy::domains(&["pypi.org".into(), "*.pythonhosted.org".into()], true).is_ok());
    assert!(policy::domains(&["*".into()], false).unwrap().is_empty());
}

#[test]
fn native_policy_uses_only_supplied_environment_and_separate_scratch() {
    let root = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let request = policy::request(root.path(), false, scratch.path(), "echo ok", None).unwrap();
    assert_eq!(request.inherit_default_environment, Some(false));
    let fs = request.filesystem.unwrap();
    assert!(!fs
        .readwrite_paths
        .contains(&root.path().canonicalize().unwrap().display().to_string()));
    assert!(fs
        .readwrite_paths
        .contains(&scratch.path().canonicalize().unwrap().display().to_string()));
    assert_eq!(request.timeout_ms, Some(120_000));
    assert!(request
        .environment
        .unwrap()
        .iter()
        .all(|(key, _)| !key.ends_with("API_KEY")));
}

#[cfg(not(windows))]
fn quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}
#[cfg(windows)]
fn quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "''"))
}

#[tokio::test]
#[ignore = "requires the real native OS sandbox outside the test runner sandbox"]
async fn native_commands_confine_files_and_keep_output_bounded() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("input.txt"), "project input").unwrap();
    std::fs::write(outside.path().join("secret.txt"), "PRIVATE SENTINEL").unwrap();
    #[cfg(not(windows))]
    let command = format!(
        "cat input.txt; printf kept > output.txt; cat {}; printf escaped > {}",
        quote(&outside.path().join("secret.txt")),
        quote(&outside.path().join("escaped.txt"))
    );
    #[cfg(windows)]
    let command = format!("Get-Content input.txt; Set-Content output.txt kept; Get-Content {}; Set-Content {} escaped", quote(&outside.path().join("secret.txt")), quote(&outside.path().join("escaped.txt")));
    let result = run(root.path(), true, &[], false, &command).await.unwrap();
    assert!(
        result["stdout"].as_str().unwrap().contains("project input"),
        "{result}"
    );
    assert!(!result.to_string().contains("PRIVATE SENTINEL"), "{result}");
    assert!(root.path().join("output.txt").exists(), "{result}");
    assert!(!outside.path().join("escaped.txt").exists(), "{result}");
    #[cfg(not(windows))]
    let command = "printf changed > input.txt";
    #[cfg(windows)]
    let command = "Set-Content input.txt changed";
    let result = run(root.path(), false, &[], false, command).await.unwrap();
    assert_ne!(result["exitCode"], json!(0), "{result}");
    assert_eq!(
        std::fs::read_to_string(root.path().join("input.txt")).unwrap(),
        "project input"
    );
    #[cfg(not(windows))]
    let flood = "head -c 200000 /dev/zero; head -c 200000 /dev/zero >&2";
    #[cfg(windows)]
    let flood = "[Console]::Out.Write('x' * 200000); [Console]::Error.Write('x' * 200000)";
    let result = run(root.path(), false, &[], false, flood).await.unwrap();
    assert_eq!(result["stdout"].as_str().unwrap().len(), 65536);
    assert_eq!(result["stderr"].as_str().unwrap().len(), 65536);
    assert_eq!(result["truncated"], true);
    #[cfg(not(windows))]
    let unicode = "printf 'José 日本語 αβ'";
    #[cfg(windows)]
    let unicode = "[Console]::Write('José 日本語 αβ')";
    let result = run(root.path(), false, &[], false, unicode).await.unwrap();
    assert_eq!(result["stdout"], "José 日本語 αβ");
}

#[tokio::test]
#[ignore = "requires the real native OS sandbox and public internet"]
async fn native_commands_enforce_domain_filter_and_direct_network_denial() {
    let root = tempfile::tempdir().unwrap();
    #[cfg(not(windows))]
    let curl = "curl";
    #[cfg(windows)]
    let curl = "curl.exe";
    for (network, sites, command, succeeds) in [
        #[cfg(windows)]
        (
            true,
            vec!["example.com".into()],
            "(Invoke-WebRequest -UseBasicParsing -TimeoutSec 15 https://example.com).Content"
                .to_owned(),
            true,
        ),
        (
            true,
            vec!["example.com".into()],
            format!("{curl} -f -L -sS --max-time 15 http://example.com"),
            true,
        ),
        (
            true,
            vec!["example.com".into()],
            format!("{curl} -f -sS --max-time 15 https://example.com"),
            true,
        ),
        (
            true,
            vec!["example.com".into()],
            format!("{curl} -f -sS --max-time 5 https://example.org"),
            false,
        ),
        (
            true,
            vec!["example.com".into()],
            format!("{curl} -f -sS --noproxy '*' --max-time 5 https://example.com"),
            false,
        ),
        (
            false,
            vec![],
            format!("{curl} -f -sS --max-time 5 https://example.com"),
            false,
        ),
    ] {
        let result = run(root.path(), false, &sites, network, &command)
            .await
            .unwrap();
        assert_eq!(
            result["stdout"]
                .as_str()
                .unwrap()
                .contains("Example Domain"),
            succeeds,
            "{command}: {result}"
        );
    }
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let command = format!("{curl} -sS --noproxy '*' --max-time 2 http://{address}");
    let result = run(root.path(), false, &["example.com".into()], true, &command)
        .await
        .unwrap();
    assert!(!result["stderr"].as_str().unwrap().is_empty(), "{result}");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires public internet"]
async fn native_proxy_socks5_forwards_an_allowed_public_site() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let proxy = proxy::Proxy::start(vec!["example.com".into()])
        .await
        .unwrap();
    let mut client = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, proxy.port))
        .await
        .unwrap();
    client.write_all(&[5, 1, 0]).await.unwrap();
    let mut reply = [0; 2];
    client.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply, [5, 0]);
    let mut connect = vec![5, 1, 0, 3, 11];
    connect.extend_from_slice(b"example.com");
    connect.extend_from_slice(&80u16.to_be_bytes());
    client.write_all(&connect).await.unwrap();
    let mut reply = [0; 10];
    client.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply[1], 0);
    client
        .write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut body = String::new();
    tokio::time::timeout(Duration::from_secs(15), client.read_to_string(&mut body))
        .await
        .unwrap()
        .unwrap();
    assert!(body.contains("Example Domain"), "{body}");
}

#[tokio::test]
#[ignore = "requires the real native OS sandbox outside the test runner sandbox"]
async fn native_command_cancellation_stops_background_descendants() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_path_buf();
    #[cfg(not(windows))]
    let command = "(sleep 3; echo escaped > after-cancel.txt) & echo ready > started.txt; sleep 30";
    #[cfg(windows)]
    let command = "Start-Process powershell -ArgumentList '-NoProfile -Command Start-Sleep 3; Set-Content after-cancel.txt escaped' -WorkingDirectory (Get-Location); Set-Content started.txt ready; Start-Sleep 30";
    let task = tokio::spawn(async move { run(&path, true, &[], false, command).await });
    tokio::time::timeout(Duration::from_secs(20), async {
        while !root.path().join("started.txt").exists() {
            assert!(
                !task.is_finished(),
                "Sandbox command failed before signalling readiness"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert!(!root.path().join("after-cancel.txt").exists());
}

#[tokio::test]
#[ignore = "requires the real native OS sandbox outside the test runner sandbox"]
async fn native_shell_exit_reaps_background_descendants() {
    let root = tempfile::tempdir().unwrap();
    #[cfg(not(windows))]
    let command = "(sleep 3; echo escaped > after-exit.txt) & echo done";
    #[cfg(windows)]
    let command = "Start-Process powershell -ArgumentList '-NoProfile -Command Start-Sleep 3; Set-Content after-exit.txt escaped' -WorkingDirectory (Get-Location); Write-Output done";
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        run(root.path(), true, &[], false, command),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        result["stdout"].as_str().unwrap().contains("done"),
        "{result}"
    );
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert!(!root.path().join("after-exit.txt").exists());
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "requires the real native OS sandbox and Python 3"]
async fn native_commands_cannot_use_project_unix_sockets_as_a_network_bypass() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("host.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let script = format!("import socket; print('socket-probe-started', flush=True); s=socket.socket(socket.AF_UNIX); s.connect({}); s.send(b'escape')", serde_json::to_string(&socket).unwrap());
    let command = format!("/usr/bin/python3 -c '{}'", script.replace('\'', "'\\''"));
    let result = run(root.path(), true, &[], false, &command).await.unwrap();
    assert!(
        result["stdout"]
            .as_str()
            .unwrap()
            .contains("socket-probe-started"),
        "{result}"
    );
    assert_ne!(result["exitCode"], json!(0), "{result}");
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[cfg(windows)]
#[tokio::test]
async fn unsupported_windows_refuses_commands_before_writing_files() {
    if unavailable_reason().is_none() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    assert!(run(
        root.path(),
        true,
        &[],
        false,
        "Set-Content escaped.txt unsafe"
    )
    .await
    .is_err());
    assert!(!root.path().join("escaped.txt").exists());
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "requires the real native OS sandbox and Python 3"]
async fn native_setsid_children_cannot_outlive_the_command() {
    let root = tempfile::tempdir().unwrap();
    let script = "import os,time\nif os.fork() == 0:\n try:\n  os.setsid()\n except PermissionError:\n  os._exit(0)\n open('detached.txt','w').write(str(os.getpid()))\n time.sleep(4)\n open('after-exit.txt','w').write('escaped')\n os._exit(0)\ntime.sleep(0.3)\nprint('done',flush=True)\n";
    let command = format!("/usr/bin/python3 -c '{}'", script.replace('\'', "'\\''"));
    let result = run(root.path(), true, &[], false, &command).await.unwrap();
    assert!(
        result["stdout"].as_str().unwrap().contains("done"),
        "{result}"
    );
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(
        !root.path().join("after-exit.txt").exists(),
        "A detached process survived command completion"
    );
}
