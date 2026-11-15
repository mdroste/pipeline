//! Translate Workspace permissions into a deny-by-default OS sandbox. The
//! subscription CLI policy remains in shell_policy.rs; API commands never use
//! CLI defaults or an unconfined fallback. Scratch is private to one command.
use super::proxy;
use mxc_sdk::v1::{
    ContainerRequest, FilesystemPolicy, NetworkAction, NetworkEgressPolicy, NetworkIngressPolicy,
    NetworkPeerPolicy, NetworkPolicy, NetworkPortPolicy, NetworkProtocol, NetworkRulePolicy,
    NetworkRuntimeConfig,
};
use std::path::{Path, PathBuf};

pub(super) const SITES_REQUIRED: &str = "This command did not run. API and local server conversations need a list of sites for command internet access. List them under Allowed command sites in Assistant settings, for example pypi.org, or turn off Internet access for commands.";

pub(super) fn domains(sites: &[String], network: bool) -> Result<Vec<String>, String> {
    if !network {
        return Ok(Vec::new());
    }
    if sites.is_empty()
        || sites.len() > 64
        || sites.iter().any(|site| {
            !proxy::valid_domain(site.strip_prefix("*.").unwrap_or(site))
                || *site != site.to_ascii_lowercase()
        })
    {
        return Err(SITES_REQUIRED.into());
    }
    Ok(sites.to_vec())
}

fn text(path: &Path) -> Result<String, String> {
    path.to_str()
        .filter(|p| !p.contains('\0'))
        .map(str::to_owned)
        .ok_or_else(|| "The command folder must have a valid Unicode path.".into())
}

pub(super) fn request(
    root: &Path,
    writable: bool,
    scratch: &Path,
    command: &str,
    proxy: Option<&proxy::Proxy>,
) -> Result<ContainerRequest, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let scratch = scratch.canonicalize().map_err(|e| e.to_string())?;
    if !root.is_dir() {
        return Err("The command folder is not a directory.".into());
    }
    let mut fs = FilesystemPolicy {
        readwrite_paths: vec![text(&scratch)?],
        ..Default::default()
    };
    if writable {
        fs.readwrite_paths.push(text(&root)?);
    } else {
        fs.readonly_paths.push(text(&root)?);
    }
    // Only system-wide tools are exposed. User profiles, caches, credentials,
    // other projects and mounted data are not implicit read grants.
    for path in system_roots() {
        if path.exists() {
            fs.readonly_paths.push(text(&path)?);
        }
    }
    // These repository control files remain readable, but cannot be rewritten
    // to arrange later execution by an unsandboxed Git process.
    for name in [".git", ".gitconfig"] {
        let path = root.join(name);
        if path.exists() {
            fs.readonly_paths
                .push(text(&path.canonicalize().map_err(|e| e.to_string())?)?);
        }
    }
    for path in protected_paths(&root) {
        fs.denied_paths
            .push(text(&path.canonicalize().unwrap_or(path))?);
    }
    let tmp = text(&scratch)?;
    let mut environment = vec![
        ("PATH".into(), crate::env::full_path()),
        ("LANG".into(), "en_US.UTF-8".into()),
        ("HOME".into(), tmp.clone()),
        ("TMPDIR".into(), tmp.clone()),
        ("TEMP".into(), tmp.clone()),
        ("TMP".into(), tmp.clone()),
        ("NO_PROXY".into(), String::new()),
        ("no_proxy".into(), String::new()),
    ];
    #[cfg(windows)]
    {
        environment.push(("SystemRoot".into(), text(&windows_root()?)?));
        environment.push(("LOCALAPPDATA".into(), tmp.clone()));
        environment.push(("APPDATA".into(), tmp.clone()));
        environment.push(("USERPROFILE".into(), tmp));
    }
    if let Some(proxy) = proxy {
        let socks = format!("socks5h://127.0.0.1:{}", proxy.port);
        environment.extend([
            ("ALL_PROXY".into(), socks.clone()),
            ("all_proxy".into(), socks),
        ]);
        for key in ["http_proxy", "https_proxy", "HTTP_PROXY", "HTTPS_PROXY"] {
            environment.push((key.into(), proxy.url()));
        }
    }
    #[cfg(target_os = "linux")]
    let command = {
        let (command, executable) = super::linux::command(command)?;
        fs.readonly_paths.push(text(&executable)?);
        // AppImage helpers load their bundled shared libraries beside usr/bin.
        // Grant only those library folders, not the extraction directory.
        if let Some(prefix) = executable.parent().and_then(Path::parent) {
            for name in ["lib", "lib64"] {
                let library = prefix.join(name);
                if library.is_dir() {
                    fs.readonly_paths.push(text(&library)?);
                }
            }
        }
        command
    };
    #[cfg(not(target_os = "linux"))]
    let command = shell_command(command, &scratch)?;
    #[cfg(not(windows))]
    let command = format!("/bin/bash --noprofile --norc -c '{}'", format!(
        "IFS= read -r pipeline_gate; [ \"$pipeline_gate\" = pipeline-command-start ] || exit 125;\n{command}"
    ).replace('\'', "'\\''"));
    let mut request = ContainerRequest::new(command);
    request.filesystem = Some(fs);
    request.working_directory = Some(text(&root)?);
    request.environment = Some(environment);
    request.inherit_default_environment = Some(false);
    request.timeout_ms = Some(120_000);
    request.network = Some(network_policy(proxy, cfg!(windows)));
    #[cfg(target_os = "macos")]
    super::macos::confine(&mut request, proxy.map(|proxy| proxy.port))?;
    Ok(request)
}

/// Windows uses direct endpoint policy, not MXC's proxy shortcut, because the
/// shortcut requires private-network server capability. The endpoint allowlist
/// grants only outbound TCP to our listener; other egress and private ingress
/// stay denied. Standard proxy variables supply HTTP/SOCKS client routing.
fn network_policy(proxy: Option<&proxy::Proxy>, windows: bool) -> NetworkPolicy {
    NetworkPolicy {
        egress: Some(NetworkEgressPolicy {
            default: Some(NetworkAction::Deny),
            allow: if windows {
                proxy.map(|proxy| {
                    vec![NetworkRulePolicy {
                        to: Some(vec![NetworkPeerPolicy::new("127.0.0.1/32")]),
                        ports: Some(vec![NetworkPortPolicy {
                            protocol: Some(NetworkProtocol::Tcp),
                            port: Some(proxy.port),
                            end_port: None,
                        }]),
                    }]
                })
            } else {
                None
            },
            deny: None,
        }),
        ingress: Some(NetworkIngressPolicy {
            default: Some(NetworkAction::Deny),
            host_loopback: Some(if windows && proxy.is_some() {
                NetworkAction::Allow
            } else {
                NetworkAction::Deny
            }),
        }),
        runtime_config: if windows {
            None
        } else {
            proxy.map(|proxy| NetworkRuntimeConfig {
                network_proxy: Some(proxy.url()),
            })
        },
    }
}

fn protected_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = dirs::home_dir() {
        for secret in [
            ".ssh",
            ".aws",
            ".azure",
            ".gnupg",
            ".netrc",
            ".config",
            ".docker",
            ".kube",
            ".claude",
            ".codex",
            "Library/Keychains",
            "AppData/Roaming/Microsoft/Credentials",
            "AppData/Local/Microsoft/Credentials",
        ] {
            paths.push(home.join(secret));
        }
    }
    for store in [crate::storage::local_root(), crate::storage::data_root()]
        .into_iter()
        .flatten()
    {
        let store = store.canonicalize().unwrap_or(store);
        // A conversation's private working folder can be inside this store;
        // the positive grant exposes only that folder, not its siblings.
        if !root.starts_with(&store) {
            paths.push(store);
        }
    }
    paths.push(root.join(".env"));
    paths
}

fn system_roots() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        ["SystemRoot", "ProgramFiles", "ProgramFiles(x86)"]
            .into_iter()
            .filter_map(std::env::var_os)
            .map(PathBuf::from)
            .collect()
    }
    #[cfg(not(windows))]
    {
        ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/opt", "/etc"]
            .map(PathBuf::from)
            .to_vec()
    }
}

#[cfg(windows)]
fn windows_root() -> Result<PathBuf, String> {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| "Windows system folder could not be located.".into())
}

#[cfg(not(target_os = "linux"))]
fn shell_command(command: &str, _scratch: &Path) -> Result<String, String> {
    #[cfg(windows)]
    {
        use base64::Engine;
        let shell = windows_root()?.join("System32/WindowsPowerShell/v1.0/powershell.exe");
        // Parsing the untrusted script happens only after the start signal.
        // A file also avoids Windows' 32K command-line limit after UTF-16/base64.
        // Windows PowerShell 5 needs a process-local .NET proxy as well as env
        // variables; it does not discover HTTP_PROXY for Invoke-WebRequest.
        let script = _scratch.join("command.ps1");
        std::fs::write(&script, command).map_err(|e| e.to_string())?;
        let gate = format!("if ([Console]::ReadLine() -ne 'pipeline-command-start') {{ exit 125 }}; [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); $OutputEncoding = [Console]::OutputEncoding; if ($env:HTTPS_PROXY) {{ [Net.WebRequest]::DefaultWebProxy = [Net.WebProxy]::new($env:HTTPS_PROXY) }}; & ([ScriptBlock]::Create([IO.File]::ReadAllText('{}')))", text(&script)?.replace('\'', "''"));
        let bytes: Vec<u8> = gate.encode_utf16().flat_map(u16::to_le_bytes).collect();
        Ok(format!(
            "\"{}\" -NoLogo -NoProfile -NonInteractive -EncodedCommand {}",
            shell.display(),
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    }
    #[cfg(not(windows))]
    {
        Ok(format!(
            "/bin/bash --noprofile --norc -c '{}'",
            command.replace('\'', "'\\''")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn windows_proxy_policy_does_not_grant_private_network_servers() {
        let proxy = proxy::Proxy::start(vec!["example.com".into()])
            .await
            .unwrap();
        let policy = network_policy(Some(&proxy), true);
        assert!(policy.runtime_config.is_none());
        assert_eq!(policy.ingress.unwrap().default, Some(NetworkAction::Deny));
        let egress = policy.egress.unwrap();
        assert_eq!(egress.default, Some(NetworkAction::Deny));
        let rules = egress.allow.unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].to.as_ref().unwrap()[0].cidr, "127.0.0.1/32");
        let ports = rules[0].ports.as_ref().unwrap();
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].protocol, Some(NetworkProtocol::Tcp));
        assert_eq!(ports[0].port, Some(proxy.port));
    }
}
