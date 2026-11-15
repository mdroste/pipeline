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
    #[cfg(target_os = "linux")]
    let (request, _view) = {
        let mut request = request;
        let view = linux::View::prepare(&mut request).unwrap();
        (request, view)
    };
    let mut child = mxc_sdk::v1::spawn(request, Default::default()).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert!(!root.path().join("started.txt").exists());
    let mut stdin = child.take_stdin().unwrap();
    stdin.write_all(b"pipeline-command-start\n").unwrap();
    drop(stdin);
    child.wait().unwrap();
    assert!(root.path().join("started.txt").exists());
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
    #[cfg(windows)]
    {
        let mut keys = std::collections::HashSet::new();
        for (key, _) in request.environment.as_ref().unwrap() {
            assert!(
                keys.insert(key.to_ascii_lowercase()),
                "Duplicate Windows environment key: {key}"
            );
        }
    }
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
    // Keep the network assertions independent of missing-path mount behavior.
    // The empty-project regression below covers that separate startup case.
    std::fs::write(root.path().join(".env"), "PRIVATE NETWORK SENTINEL").unwrap();
    #[cfg(not(windows))]
    let curl = "curl";
    #[cfg(windows)]
    let curl = "curl.exe";
    #[cfg(not(windows))]
    let socks_proxy = "\"${HTTPS_PROXY/http:/socks5h:}\"";
    #[cfg(windows)]
    let socks_proxy = "($env:HTTPS_PROXY -replace '^http:', 'socks5h:')";
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
            format!("{curl} -f -sS --proxy {socks_proxy} --max-time 15 http://example.com"),
            true,
        ),
        (
            true,
            vec!["example.com".into()],
            format!("{curl} -f -sS --proxy {socks_proxy} --max-time 5 http://example.org"),
            false,
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
    // Prove the host listener accepts connections before checking isolation.
    let control = tokio::net::TcpStream::connect(address).await.unwrap();
    drop(listener.accept().await.unwrap());
    drop(control);
    #[cfg(not(target_os = "linux"))]
    let command = format!("{curl} -sS --noproxy '*' --max-time 2 http://{address}");
    // The sandbox's loopback is a separate interface on Linux. Probe the
    // slirp host gateway at a different port so this exercises the firewall,
    // rather than merely trying an empty sandbox-local loopback interface.
    #[cfg(target_os = "linux")]
    let command = format!(
        "{curl} -sS --noproxy '*' --max-time 2 \"${{HTTP_PROXY%:*}}:{}\"",
        address.port()
    );
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

async fn empty_project(writable: bool) {
    let root = tempfile::tempdir().unwrap();
    #[cfg(not(windows))]
    let command = "printf started";
    #[cfg(windows)]
    let command = "[Console]::Write('started')";
    let result = run(root.path(), writable, &[], false, command)
        .await
        .unwrap();
    assert_eq!(
        result["exitCode"],
        json!(0),
        "writable={writable}: {result}"
    );
    assert_eq!(result["stdout"], "started", "{result}");
    assert!(
        !root.path().join(".env").exists(),
        "The sandbox created a protected path in the host project"
    );
}

#[tokio::test]
#[ignore = "requires the real native OS sandbox outside the test runner sandbox"]
async fn native_empty_readonly_project_starts_without_creating_protected_paths() {
    empty_project(false).await;
}

#[tokio::test]
#[ignore = "requires the real native OS sandbox outside the test runner sandbox"]
async fn native_empty_writable_project_starts_without_creating_protected_paths() {
    empty_project(true).await;
}

#[cfg(windows)]
#[tokio::test]
#[ignore = "requires the real native Windows sandbox"]
async fn native_missing_secret_parent_stays_denied_when_created_after_launch() {
    let root = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing/credentials/secret.txt");
    let mut request = policy::request(
        root.path(), true, scratch.path(),
        "Set-Content ready.txt ready; while (-not (Test-Path continue.txt)) { Start-Sleep -Milliseconds 10 }; Get-Content missing/credentials/secret.txt; Set-Content missing/credentials/secret.txt changed; Set-Content allowed.txt ok",
        None,
    ).unwrap();
    let denied = policy::protected_path(missing.clone()).unwrap();
    assert_eq!(denied, root.path().canonicalize().unwrap().join("missing"));
    request
        .filesystem
        .as_mut()
        .unwrap()
        .denied_paths
        .push(denied.to_str().unwrap().to_owned());
    request.timeout_ms = Some(10_000);
    let probe = mxc_sdk::v1::probe(Some(&request)).unwrap();
    assert_eq!(probe.tier, Some("base-container"), "{probe:?}");
    assert!(probe.error.is_none(), "{probe:?}");
    let task = tokio::spawn(process::run(request, scratch, None));
    tokio::time::timeout(Duration::from_secs(8), async {
        while !root.path().join("ready.txt").exists() {
            assert!(!task.is_finished(), "Sandbox failed before readiness");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(!root.path().join("missing").exists());
    std::fs::create_dir_all(missing.parent().unwrap()).unwrap();
    std::fs::write(&missing, "LATE PRIVATE SENTINEL").unwrap();
    std::fs::write(root.path().join("continue.txt"), "go").unwrap();
    let result = task.await.unwrap().unwrap();
    assert!(
        !result.to_string().contains("LATE PRIVATE SENTINEL"),
        "{result}"
    );
    assert_eq!(
        std::fs::read_to_string(missing).unwrap(),
        "LATE PRIVATE SENTINEL"
    );
    assert!(root.path().join("allowed.txt").exists(), "{result}");
}

#[cfg(windows)]
#[tokio::test]
#[ignore = "requires the real native Windows sandbox"]
async fn native_windows_cmd_child_keeps_project_working_directory() {
    let root = tempfile::Builder::new()
        .prefix("pipeline José 日本語 ")
        .tempdir()
        .unwrap();
    std::fs::write(root.path().join("input.txt"), "project-input").unwrap();
    let result = run(
        root.path(), true, &[], false,
        "& (Join-Path $env:SystemRoot 'System32/cmd.exe') /d /c 'type input.txt & echo project-output> output.txt'",
    ).await.unwrap();
    assert_eq!(result["exitCode"], json!(0), "{result}");
    assert!(
        result["stdout"].as_str().unwrap().contains("project-input"),
        "{result}"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("output.txt"))
            .unwrap()
            .trim(),
        "project-output"
    );
}

#[cfg(windows)]
#[tokio::test]
#[ignore = "requires the real native Windows sandbox"]
async fn native_windows_preserves_native_and_powershell_exit_status() {
    let root = tempfile::tempdir().unwrap();
    for (command, expected) in [
        (
            "& (Join-Path $env:SystemRoot 'System32/cmd.exe') /d /c 'exit 7'",
            7,
        ),
        ("Get-Content missing.txt", 1),
        ("throw 'terminal failure'", 1),
        ("Get-Content missing.txt; [Console]::Write('recovered')", 0),
    ] {
        let result = run(root.path(), false, &[], false, command).await.unwrap();
        assert_eq!(result["exitCode"], json!(expected), "{command}: {result}");
    }
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

#[derive(Clone, Copy, Debug)]
enum CommandEnd {
    Completion,
    Cancellation,
    Timeout,
}

#[cfg(windows)]
fn windows_background_script(delay_seconds: u64) -> String {
    use base64::Engine;
    // This independent PowerShell child needs the same explicit module imports
    // as the command bootstrap before it can signal meaningful readiness.
    let script = format!("$ErrorActionPreference='Stop'; Import-Module Microsoft.PowerShell.Management,Microsoft.PowerShell.Utility -ErrorAction Stop; $root=(Get-Location).Path; [IO.File]::WriteAllText((Join-Path $root 'child.tmp'), 'ready'); [IO.File]::Move((Join-Path $root 'child.tmp'), (Join-Path $root 'child.ready')); Start-Sleep {delay_seconds}; [IO.File]::WriteAllText((Join-Path $root 'after-stop.txt'), 'escaped')");
    base64::engine::general_purpose::STANDARD.encode(
        script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}

#[cfg(windows)]
#[tokio::test]
#[ignore = "requires the real native Windows sandbox"]
async fn native_windows_waited_child_can_finish() {
    // Positive control: the exact child used by the cleanup tests can reach
    // its delayed write when the parent waits instead of ending the command.
    let root = tempfile::tempdir().unwrap();
    let encoded = windows_background_script(1);
    let command = format!("$child = Start-Process (Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe') -ArgumentList '-NoLogo -NoProfile -NonInteractive -EncodedCommand {encoded}' -WorkingDirectory (Get-Location) -NoNewWindow -PassThru -Wait; exit $child.ExitCode");
    let result = run(root.path(), true, &[], false, &command).await.unwrap();
    assert_eq!(result["exitCode"], json!(0), "{result}");
    assert!(root.path().join("child.ready").exists(), "{result}");
    assert_eq!(
        std::fs::read_to_string(root.path().join("after-stop.txt"))
            .unwrap()
            .trim(),
        "escaped"
    );
}

async fn background_cleanup(end: CommandEnd, network: bool) {
    let root = tempfile::Builder::new()
        .prefix("pipeline command probe ")
        .tempdir()
        .unwrap();
    std::fs::write(root.path().join(".env"), "PRIVATE LIFECYCLE SENTINEL").unwrap();
    let path = root.path().to_path_buf();
    let pause = !matches!(end, CommandEnd::Completion);
    #[cfg(not(windows))]
    let command = format!("(printf ready > child.tmp; mv child.tmp child.ready; sleep 10; printf escaped > after-stop.txt) & while [ ! -f child.ready ]; do sleep 0.01; done; printf foreground-ready; {}", if pause { "sleep 30" } else { ":" });
    #[cfg(windows)]
    let command = {
        let encoded = windows_background_script(10);
        format!("Start-Process (Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe') -ArgumentList '-NoLogo -NoProfile -NonInteractive -EncodedCommand {encoded}' -WorkingDirectory (Get-Location) -NoNewWindow; while (-not (Test-Path child.ready)) {{ Start-Sleep -Milliseconds 10 }}; [Console]::Write('foreground-ready'); {}", if pause { "Start-Sleep 30" } else { "" })
    };
    let task = tokio::spawn(async move {
        if matches!(end, CommandEnd::Timeout) {
            let scratch = tempfile::tempdir().unwrap();
            let proxy = if network {
                Some(
                    proxy::Proxy::start(vec!["example.com".into()])
                        .await
                        .unwrap(),
                )
            } else {
                None
            };
            let mut request =
                policy::request(&path, true, scratch.path(), &command, proxy.as_ref()).unwrap();
            request.timeout_ms = Some(5000);
            #[cfg(windows)]
            {
                let probe = mxc_sdk::v1::probe(Some(&request)).unwrap();
                assert_eq!(probe.tier, Some("base-container"), "{probe:?}");
                assert!(probe.error.is_none(), "{probe:?}");
            }
            process::run(request, scratch, proxy).await
        } else {
            run(&path, true, &["example.com".into()], network, &command).await
        }
    });
    tokio::time::timeout(Duration::from_secs(20), async {
        while !root.path().join("child.ready").exists() {
            assert!(
                !task.is_finished(),
                "Sandbox failed before the child signalled readiness"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    // Start the observation window after the child is known to be alive.
    // Windows container/PowerShell startup can itself take several seconds.
    let child_started = std::time::Instant::now();
    if matches!(end, CommandEnd::Cancellation) {
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    } else {
        let result = tokio::time::timeout(Duration::from_secs(20), task)
            .await
            .unwrap()
            .unwrap();
        if matches!(end, CommandEnd::Timeout) {
            assert!(result.unwrap_err().contains("timed out after 5 seconds"));
        } else {
            let result = result.unwrap();
            assert_eq!(result["exitCode"], json!(0), "{result}");
            assert!(
                result["stdout"]
                    .as_str()
                    .unwrap()
                    .contains("foreground-ready"),
                "{result}"
            );
        }
    }
    assert!(
        root.path().join("child.ready").exists(),
        "{end:?}: child never started"
    );
    tokio::time::sleep(Duration::from_secs(11).saturating_sub(child_started.elapsed())).await;
    assert!(
        !root.path().join("after-stop.txt").exists(),
        "{end:?}: child wrote after cleanup"
    );
}

#[tokio::test]
#[ignore = "requires the real native OS sandbox outside the test runner sandbox"]
async fn native_command_cancellation_stops_background_descendants() {
    background_cleanup(CommandEnd::Cancellation, false).await;
}

#[tokio::test]
#[ignore = "requires the real native OS sandbox outside the test runner sandbox"]
async fn native_shell_exit_reaps_background_descendants() {
    background_cleanup(CommandEnd::Completion, false).await;
}

#[tokio::test]
#[ignore = "requires the real native OS sandbox outside the test runner sandbox"]
async fn native_command_timeout_stops_background_descendants() {
    background_cleanup(CommandEnd::Timeout, false).await;
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

#[tokio::test]
#[ignore = "requires the real native OS sandbox outside the test runner sandbox"]
async fn native_commands_protect_secrets_and_git_metadata_inside_writable_roots() {
    let root = tempfile::Builder::new()
        .prefix("pipeline protected files ")
        .tempdir()
        .unwrap();
    std::fs::create_dir(root.path().join(".git")).unwrap();
    std::fs::write(root.path().join(".git/config"), "readable control file").unwrap();
    std::fs::write(root.path().join(".gitconfig"), "preserved git config").unwrap();
    std::fs::write(root.path().join(".env"), "PRIVATE ENV SENTINEL").unwrap();
    #[cfg(not(windows))]
    let command = "cat .git/config; cat .env; printf changed > .git/config; printf changed > .gitconfig; printf changed > .env; printf scratch-ok > \"$TMPDIR/scratch.txt\"; cat \"$TMPDIR/scratch.txt\"; printf project-ok > allowed.txt";
    #[cfg(windows)]
    let command = "Get-Content .git/config; Get-Content .env; Set-Content .git/config changed; Set-Content .gitconfig changed; Set-Content .env changed; Set-Content (Join-Path $env:TEMP 'scratch.txt') scratch-ok; Get-Content (Join-Path $env:TEMP 'scratch.txt'); Set-Content allowed.txt project-ok";
    let result = run(root.path(), true, &[], false, command).await.unwrap();
    assert!(
        result["stdout"]
            .as_str()
            .unwrap()
            .contains("readable control file"),
        "{result}"
    );
    assert!(
        result["stdout"].as_str().unwrap().contains("scratch-ok"),
        "{result}"
    );
    assert!(
        !result.to_string().contains("PRIVATE ENV SENTINEL"),
        "{result}"
    );
    for (name, expected) in [
        (".git/config", "readable control file"),
        (".gitconfig", "preserved git config"),
        (".env", "PRIVATE ENV SENTINEL"),
    ] {
        assert_eq!(
            std::fs::read_to_string(root.path().join(name)).unwrap(),
            expected,
            "{name}: {result}"
        );
    }
    assert!(std::fs::read_to_string(root.path().join("allowed.txt"))
        .unwrap()
        .contains("project-ok"));
}

#[tokio::test]
#[ignore = "requires the real native OS sandbox and network namespace prerequisites"]
async fn native_network_commands_stop_background_descendants_on_every_exit_path() {
    for end in [
        CommandEnd::Completion,
        CommandEnd::Cancellation,
        CommandEnd::Timeout,
    ] {
        background_cleanup(end, true).await;
    }
}
