//! Isolated qualification config plus the production permission profiles.
use super::*;

fn toml_basic_string(value: &str) -> String {
    let mut encoded = String::from("\"");
    for character in value.chars() {
        match character {
            '\\' => encoded.push_str("\\\\"),
            '"' => encoded.push_str("\\\""),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\t' => encoded.push_str("\\t"),
            value if value.is_control() => {
                encoded.push_str(&format!("\\u{:04x}", value as u32));
            }
            value => encoded.push(value),
        }
    }
    encoded.push('"');
    encoded
}

pub(super) fn write_probe_config(
    codex_home: &Path,
    fixture_root: &Path,
    launcher: &Path,
) -> Result<(), String> {
    let fixture_root = fixture_root
        .to_str()
        .ok_or("Workbench probe fixture path is not valid UTF-8")?;
    let launcher = launcher
        .to_str()
        .ok_or("Codex launcher path is not valid UTF-8")?;
    let canonical_launcher = std::fs::canonicalize(launcher)
        .map_err(|error| format!("Failed to resolve Codex launcher: {error}"))?;
    let canonical_launcher = canonical_launcher
        .to_str()
        .ok_or("Resolved Codex launcher path is not valid UTF-8")?;
    let launcher_permissions = if launcher == canonical_launcher {
        format!("{} = \"read\"\n", toml_basic_string(launcher))
    } else {
        format!(
            "{} = \"read\"\n{} = \"read\"\n",
            toml_basic_string(launcher),
            toml_basic_string(canonical_launcher),
        )
    };
    let config = format!(
        "cli_auth_credentials_store = \"file\"\n\
default_permissions = \"{PROBE_PERMISSION_PROFILE}\"\n\
\n\
[permissions.{PROBE_PERMISSION_PROFILE}]\n\
description = \"Pipeline Workbench read-only qualification profile\"\n\
\n\
[permissions.{PROBE_PERMISSION_PROFILE}.workspace_roots]\n\
{} = true\n\
\n\
[permissions.{PROBE_PERMISSION_PROFILE}.filesystem]\n\
\":minimal\" = \"read\"\n\
{}\
\n\
[permissions.{PROBE_PERMISSION_PROFILE}.filesystem.\":workspace_roots\"]\n\
\".\" = \"read\"\n\
\n\
[permissions.{PROBE_PERMISSION_PROFILE}.network]\n\
enabled = false\n",
        toml_basic_string(fixture_root),
        launcher_permissions,
    );
    let profiles = codex_home.join("production-profiles");
    super::super::process::write_runtime_config(&profiles, Path::new(launcher))?;
    let production =
        std::fs::read_to_string(profiles.join("config.toml")).map_err(|e| e.to_string())?;
    let (_, permissions) = production
        .split_once("\n[permissions.")
        .ok_or("Production permission profiles missing")?;
    let config = format!("{config}\n[permissions.{permissions}");
    let path = codex_home.join("config.toml");
    std::fs::write(&path, config)
        .map_err(|error| format!("Failed to write Workbench probe config: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("Failed to secure Workbench probe config: {error}"))?;
    }
    Ok(())
}
