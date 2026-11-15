//! Seatbelt policy without implicit Unix-socket grants on writable paths.
//! MXC owns native launch; process_tree tracks the inherited sandbox for cleanup.
//! Every pathname is
//! quoted as data; deeper read-only grants override broader writable roots.
use mxc_sdk::v1::{configs::SeatbeltConfig, ContainerRequest, Containment};

pub(super) mod process_tree;
#[cfg(test)]
mod tests;

pub(super) fn confine(
    request: &mut ContainerRequest,
    proxy_port: Option<u16>,
) -> Result<(), String> {
    let fs = request
        .filesystem
        .as_ref()
        .ok_or("Missing command filesystem policy")?;
    let mut profile = String::from(
        "(version 1)\n(deny default)\n\
(allow process-exec process-fork)\n\
(allow signal (target same-sandbox))\n\
(allow sysctl-read)\n\
(allow file-read-metadata)\n\
(allow file-read-data (literal \"/\"))\n\
(allow mach-lookup (global-name \"com.apple.system.notification_center\")\n\
 (global-name \"com.apple.system.logger\")\n\
 (global-name \"com.apple.distributed_notifications@Uv3\")\n\
 (global-name \"com.apple.CoreServices.coreservicesd\")\n\
 (global-name \"com.apple.FSEvents\"))\n",
    );
    for path in [
        "/System",
        "/Library",
        "/private/var/db/timezone",
        "/private/var/db/dyld",
        "/private/var/select",
    ] {
        rule(&mut profile, "allow file-read*", "subpath", path);
    }
    if let Some(developer) = developer_tools() {
        rule(
            &mut profile,
            "allow file-read*",
            "subpath",
            developer.to_str().ok_or("Invalid developer tools path")?,
        );
    }
    for path in ["/dev/null", "/dev/zero", "/dev/random", "/dev/urandom"] {
        rule(
            &mut profile,
            "allow file-read* file-write*",
            "literal",
            path,
        );
    }
    let mut grants: Vec<_> = fs
        .readonly_paths
        .iter()
        .map(|p| (p, false))
        .chain(fs.readwrite_paths.iter().map(|p| (p, true)))
        .collect();
    grants.sort_by_key(|(path, writable)| {
        (std::path::Path::new(path).components().count(), !writable)
    });
    for (path, writable) in grants {
        // No network-bind or network-outbound permission accompanies a file grant.
        rule(&mut profile, "allow file-read*", "subpath", path);
        rule(
            &mut profile,
            if writable {
                "allow file-write*"
            } else {
                "deny file-write*"
            },
            "subpath",
            path,
        );
        if let Ok(canonical) = std::fs::canonicalize(path) {
            if canonical != std::path::Path::new(path) {
                let path = canonical.to_str().ok_or("Invalid sandbox path")?;
                rule(&mut profile, "allow file-read*", "subpath", path);
                rule(
                    &mut profile,
                    if writable {
                        "allow file-write*"
                    } else {
                        "deny file-write*"
                    },
                    "subpath",
                    path,
                );
            }
        }
    }
    for path in &fs.denied_paths {
        rule(&mut profile, "deny file-read* file-write*", "subpath", path);
    }
    if let Some(port) = proxy_port {
        profile.push_str(&format!(
            "(allow network-outbound (remote tcp \"localhost:{port}\"))\n"
        ));
    }
    let mut config = SeatbeltConfig::default();
    config.profile_override = Some(profile);
    config.nested_pty = false;
    request.containment = Containment::Seatbelt(config);
    Ok(())
}

fn rule(profile: &mut String, action: &str, filter: &str, path: &str) {
    // SBPL and JSON share string quoting for the path characters we admit.
    profile.push_str(&format!(
        "({action} ({filter} {}))\n",
        serde_json::to_string(path).unwrap()
    ));
}

fn developer_tools() -> Option<std::path::PathBuf> {
    use std::os::unix::fs::MetadataExt;
    // /usr/bin/python3 and git are xcrun shims. Trust only the system-selected
    // installation, never DEVELOPER_DIR or a link replaceable by a user.
    for name in [
        "/private/var/db/xcode_select_link",
        "/private/var/select/developer_dir",
    ] {
        let link = std::path::Path::new(name);
        let Ok(meta) = std::fs::symlink_metadata(link) else {
            continue;
        };
        if !meta.file_type().is_symlink() || meta.uid() != 0 {
            continue;
        }
        if !link.ancestors().skip(1).all(|parent| {
            std::fs::metadata(parent).is_ok_and(|m| m.uid() == 0 && m.mode() & 0o022 == 0)
        }) {
            continue;
        }
        let Ok(target) = link.canonicalize() else {
            continue;
        };
        if !target.is_dir() {
            continue;
        }
        let bundle = target
            .ancestors()
            .find(|p| p.extension().is_some_and(|ext| ext == "app"));
        return Some(bundle.unwrap_or(&target).to_path_buf());
    }
    // With no xcode-select override, xcrun chooses the standard Xcode bundle.
    let default = std::path::PathBuf::from("/Applications/Xcode.app");
    std::fs::symlink_metadata(&default)
        .ok()
        .filter(|m| {
            m.is_dir() && !m.file_type().is_symlink() && m.uid() == 0 && m.mode() & 0o002 == 0
        })
        .map(|_| default)
}
