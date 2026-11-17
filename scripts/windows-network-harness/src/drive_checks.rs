//! Local lifecycle checks; no public network or arbitrary command execution.
use serde_json::json;
use std::{
    io::{BufRead, BufReader, Write},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn client(profile: &Path) -> Result<(), String> {
    let drive = crate::drive::Drive::create(profile)?;
    println!("{}", drive.root.display());
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    // The outer check terminates this owned process, skipping Rust destructors.
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn run() -> Result<(), String> {
    let token = crate::identity::token()?;
    if token["appcontainer"] != false || token["elevated"] != false {
        return Err("Use the ordinary unelevated host".into());
    }
    let profiles = (0..4)
        .map(|_| crate::identity::Profile::create())
        .collect::<Result<Vec<_>, _>>()?;
    for (i, profile) in profiles.iter().enumerate() {
        std::fs::write(profile.folder.join("canary.txt"), i.to_string())
            .map_err(|e| e.to_string())?;
    }
    let drives = std::thread::scope(|scope| {
        let pending: Vec<_> = profiles
            .iter()
            .map(|p| {
                let folder = p.folder.clone();
                scope.spawn(move || crate::drive::Drive::create(&folder))
            })
            .collect();
        pending
            .into_iter()
            .map(|p| {
                p.join()
                    .map_err(|_| "allocation worker panicked".to_owned())?
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    let roots: Vec<_> = drives.iter().map(|d| d.root.clone()).collect();
    if roots.iter().collect::<std::collections::HashSet<_>>().len() != 4 {
        return Err("concurrent aliases collided".into());
    }
    for (i, drive) in drives.iter().enumerate() {
        if std::fs::read_to_string(drive.root.join("canary.txt")).map_err(|e| e.to_string())?
            != i.to_string()
        {
            return Err("wrong alias target".into());
        }
    }
    drop(drives);
    for root in &roots {
        if crate::drive::exists(root)? {
            return Err(format!("normal cleanup left {}", root.display()));
        }
    }
    println!(
        "{}",
        json!({"concurrent_aliases":roots,"targets_correct":true,"normal_cleanup":true})
    );

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut client = Command::new(&exe)
        .arg("drive-crash-client")
        .arg(&profiles[0].folder)
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut output = client.stdout.take().ok_or("client stdout")?;
    let (sender, receiver) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(&mut output)
            .read_line(&mut line)
            .map(|_| line);
        let _ = sender.send(result);
    });
    let ready = receiver.recv_timeout(Duration::from_secs(10));
    // Always terminate our fixture process, including readiness failures.
    let _ = client.kill();
    let _ = client.wait();
    drop(client.stdin.take());
    let _ = reader.join();
    let root = PathBuf::from(
        ready
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?
            .trim(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while crate::drive::exists(&root)? {
        if Instant::now() >= deadline {
            return Err("alias remains after host termination".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    println!(
        "{}",
        json!({"forced_host_termination":true,"alias_removed":root})
    );
    let result = Command::new(exe)
        .arg("drive-owner")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .creation_flags(0x08000000)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if result.status.success() {
        return Err("owner accepted a non-profile directory".into());
    }
    println!("{}", json!({"non_profile_target_rejected":true}));
    Ok(())
}
