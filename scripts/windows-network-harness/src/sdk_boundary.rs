//! Reproduce child filesystem boundaries through the unchanged pinned SDK.
//! Uses only disposable canaries, never user secrets or an elevated token.
use mxc_sdk::v1::{self, configs::*, *};
use serde_json::json;
use std::{path::PathBuf, process::Command};

pub fn host() -> Result<(), String> {
    let token = crate::identity::token()?;
    if token["elevated"] != false || token["appcontainer"] != false {
        return Err("Use a normal unelevated host".into());
    }
    if std::env::var("PIPELINE_PROBE_AMBIENT_ACL").as_deref() != Ok("1") {
        return Err("Set PIPELINE_PROBE_AMBIENT_ACL=1 for the synthetic outside canary".into());
    }
    println!(
        "{}",
        json!({"parent_token":token,"sdk":"mxc-sdk 1.0.1 unmodified"})
    );
    let profile = crate::identity::Profile::create()?;
    let root = crate::filesystem::staged(&profile.name, &profile.sid_text()?)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut request = ContainerRequest::new(format!("\"{}\" sdk-boundary-parent", exe.display()));
    request.filesystem = Some(FilesystemPolicy {
        readonly_paths: vec![
            exe.to_string_lossy().into_owned(),
            "C:\\Windows".into(),
            root.join("readonly").to_string_lossy().into_owned(),
            root.join("work/.git").to_string_lossy().into_owned(),
        ],
        readwrite_paths: vec![root.join("work").to_string_lossy().into_owned()],
        denied_paths: vec![
            root.join("outside").to_string_lossy().into_owned(),
            root.join("work/.secrets").to_string_lossy().into_owned(),
        ],
        ..Default::default()
    });
    let mut config = ProcessContainerConfig::default();
    let mut filesystem = ProcessContainerFilesystem::default();
    let mut ancestors = std::collections::BTreeSet::new();
    for path in [&exe, &root] {
        for parent in path.ancestors().skip(1) {
            ancestors.insert(parent.to_string_lossy().into_owned());
        }
    }
    filesystem.enumerate_paths = ancestors.into_iter().collect();
    config.filesystem = Some(filesystem);
    request.containment = Containment::ProcessContainer(config);
    request.network = Some(NetworkPolicy {
        egress: Some(NetworkEgressPolicy {
            default: Some(NetworkAction::Deny),
            ..Default::default()
        }),
        ingress: Some(NetworkIngressPolicy {
            default: Some(NetworkAction::Deny),
            host_loopback: Some(NetworkAction::Deny),
        }),
        ..Default::default()
    });
    request.ui = Some(UiPolicy {
        disable: false,
        ..Default::default()
    });
    request.timeout_ms = Some(30000);
    request.working_directory = Some(root.join("work").to_string_lossy().into_owned());
    request.environment = Some(vec![(
        "PROBE_FILESYSTEM".into(),
        root.to_string_lossy().into_owned(),
    )]);
    request.inherit_default_environment = Some(true);
    let probe = v1::probe(Some(&request)).map_err(|e| e.to_string())?;
    println!("{}", json!({"probe":format!("{probe:?}")}));
    if probe.tier != Some("base-container")
        || probe.error.is_some()
        || probe.needs_dacl_augmentation != Some(false)
    {
        return Err("Native base-container without host ACL changes required".into());
    }
    let result = v1::run(request, RunOptions::default()).map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&result.stdout);
    println!(
        "{}",
        json!({"sdk_boundary":{"outcome":format!("{:?}",result.outcome),"warnings":result.warnings,"stdout":stdout,"stderr":String::from_utf8_lossy(&result.stderr)}})
    );
    // This is an observational experiment. Print the exact parent/child rows;
    // do not turn a readable forbidden canary into a successful isolation claim.
    Ok(())
}

pub fn check(phase: &str) -> Result<(), String> {
    let root = PathBuf::from(std::env::var("PROBE_FILESYSTEM").map_err(|e| e.to_string())?);
    println!(
        "{}",
        json!({"phase":phase,"token":crate::identity::token()?})
    );
    for name in ["readonly", "work", "work/.git", "work/.secrets", "outside"] {
        let read = std::fs::read_to_string(root.join(name).join("canary.txt"));
        println!(
            "{}",
            json!({"phase":phase,"path":name,"read_ok":read.is_ok(),"read":format!("{read:?}")})
        );
    }
    Ok(())
}

pub fn parent() -> Result<(), String> {
    check("sdk-parent")?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let normal = Command::new(&exe)
        .args(["sdk-boundary-check", "ordinary-child"])
        .output()
        .map_err(|e| e.to_string())?;
    println!(
        "{}",
        json!({"ordinary_child":{"code":normal.status.code(),"stdout":String::from_utf8_lossy(&normal.stdout),"stderr":String::from_utf8_lossy(&normal.stderr)}})
    );
    let profile = crate::identity::Profile::create()?;
    let inner = profile.folder.join("inner.exe");
    std::fs::copy(&exe, &inner).map_err(|e| e.to_string())?;
    let mut request = ContainerRequest::new(format!(
        "\"{}\" sdk-boundary-check restricted-profile-child",
        inner.display()
    ));
    request.filesystem = Some(FilesystemPolicy::default());
    request.working_directory = Some(profile.folder.to_string_lossy().into_owned());
    request.environment = Some(
        std::env::vars()
            .filter(|(k, _)| !k.starts_with("PIPELINE_PROBE_"))
            .collect(),
    );
    let result = crate::native::Child::spawn_derived(&request, &profile.sid_text()?)?.capture()?;
    println!("{}", json!({"restricted_profile_child":result}));
    Ok(())
}
