//! Translate Workspace permissions into a deny-by-default OS sandbox. The
//! subscription CLI policy remains in shell_policy.rs; API commands never use
//! CLI defaults or an unconfined fallback. Scratch is private to one command.
use super::proxy;
use mxc_sdk::v1::{
    ContainerRequest, FilesystemPolicy, NetworkAction, NetworkEgressPolicy, NetworkIngressPolicy,
    NetworkPolicy,
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
        fs.denied_paths.push(text(&protected_path(path)?)?);
    }
    let tmp = command_directory(&scratch)?;
    let mut environment = vec![
        ("PATH".into(), crate::env::full_path()),
        ("LANG".into(), "en_US.UTF-8".into()),
        ("HOME".into(), tmp.clone()),
        ("TMPDIR".into(), tmp.clone()),
        ("TEMP".into(), tmp.clone()),
        ("TMP".into(), tmp.clone()),
        ("no_proxy".into(), String::new()),
    ];
    #[cfg(not(windows))]
    environment.push(("NO_PROXY".into(), String::new()));
    #[cfg(target_os = "macos")]
    if proxy.is_some() {
        super::macos::tls::configure(&scratch, &mut fs, &mut environment)?;
    }
    #[cfg(windows)]
    {
        let windows = windows_root()?;
        environment.push(("SystemRoot".into(), text(&windows)?));
        environment.push(("WINDIR".into(), text(&windows)?));
        environment.push(("COMSPEC".into(), text(&windows.join("System32/cmd.exe"))?));
        // PowerShell otherwise treats even explicitly named .exe files as
        // documents and can silently skip native commands in noninteractive mode.
        environment.push(("PATHEXT".into(), ".COM;.EXE;.BAT;.CMD".into()));
        environment.push(("LOCALAPPDATA".into(), tmp.clone()));
        environment.push(("APPDATA".into(), tmp.clone()));
        environment.push(("USERPROFILE".into(), tmp));
    }
    if let Some(proxy) = proxy {
        let socks = format!("socks5h://127.0.0.1:{}", proxy.port);
        environment.push(("all_proxy".into(), socks.clone()));
        for key in ["http_proxy", "https_proxy"] {
            environment.push((key.into(), proxy.url()));
        }
        // Windows environment names are case-insensitive; duplicates break
        // Start-Process when it builds the child's environment dictionary.
        #[cfg(not(windows))]
        environment.extend([
            ("ALL_PROXY".into(), socks),
            ("HTTP_PROXY".into(), proxy.url()),
            ("HTTPS_PROXY".into(), proxy.url()),
        ]);
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
    request.working_directory = Some(command_directory(&root)?);
    request.environment = Some(environment);
    request.inherit_default_environment = Some(false);
    request.timeout_ms = Some(120_000);
    request.network = Some(network_policy(proxy));
    #[cfg(windows)]
    {
        // PowerShell imports USER32 even for console commands. Permit Win32k
        // initialization while keeping the default isolated UI resources,
        // clipboard denial and input-injection denial.
        request.ui = Some(mxc_sdk::v1::UiPolicy {
            disable: false,
            ..Default::default()
        });
    }
    #[cfg(target_os = "macos")]
    super::macos::confine(&mut request, proxy.map(|proxy| proxy.port))?;
    Ok(request)
}

fn command_directory(root: &Path) -> Result<String, String> {
    let path = text(root)?;
    #[cfg(windows)]
    {
        // CMD treats local \\?\ drive paths as UNC; PowerShell's path provider
        // cannot Join-Path them either. Use ordinary Win32 spelling when it
        // fits MAX_PATH; retain extended paths for long-path-aware programs.
        use std::path::{Component, Prefix};
        let ordinary = match root.components().next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::VerbatimDisk(_) => Some(path[4..].to_owned()),
                Prefix::VerbatimUNC(_, _) => Some(format!(r"\\{}", &path[8..])),
                _ => None,
            },
            _ => None,
        };
        if let Some(ordinary) = ordinary.filter(|p| p.encode_utf16().count() < 260) {
            return Ok(ordinary);
        }
    }
    Ok(path)
}

pub(super) fn protected_path(path: PathBuf) -> Result<PathBuf, String> {
    #[cfg(not(windows))]
    {
        Ok(path.canonicalize().unwrap_or(path))
    }
    #[cfg(windows)]
    {
        // PSEC accepts an absent leaf but cannot resolve a missing parent.
        // Deny the first missing ancestor instead: the intended secret stays
        // covered if it appears later, without creating host placeholders.
        let mut denied = path;
        loop {
            match denied.canonicalize() {
                Ok(canonical) => return Ok(canonical),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
            let parent = denied.parent().ok_or("Invalid protected command path")?;
            match parent.canonicalize() {
                Ok(canonical) => {
                    return Ok(
                        canonical.join(denied.file_name().ok_or("Invalid protected command path")?)
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    denied = parent.to_path_buf();
                }
                Err(error) => return Err(error.to_string()),
            }
        }
    }
}

/// Unix proxy modes keep direct egress and ingress denied. Windows requests
/// admit only offline commands until strict proxy isolation is available.
#[cfg(not(windows))]
fn network_policy(proxy: Option<&proxy::Proxy>) -> NetworkPolicy {
    NetworkPolicy {
        egress: Some(NetworkEgressPolicy {
            default: Some(NetworkAction::Deny),
            allow: None,
            deny: None,
        }),
        ingress: Some(NetworkIngressPolicy {
            default: Some(NetworkAction::Deny),
            host_loopback: Some(NetworkAction::Deny),
        }),
        runtime_config: proxy.map(|proxy| mxc_sdk::v1::NetworkRuntimeConfig {
            network_proxy: Some(proxy.url()),
        }),
    }
}

#[cfg(windows)]
fn network_policy(proxy: Option<&proxy::Proxy>) -> NetworkPolicy {
    use mxc_sdk::v1::{NetworkPeerPolicy, NetworkPortPolicy, NetworkProtocol, NetworkRulePolicy};
    // Windows broadens host loopback beyond this native endpoint rule. The
    // privileged guard must attach per-identity WFP restrictions before the
    // command gate opens. Runtime proxy mode would grant inbound capability.
    NetworkPolicy {
        egress: Some(NetworkEgressPolicy {
            default: Some(NetworkAction::Deny),
            deny: None,
            allow: proxy.map(|p| {
                vec![NetworkRulePolicy {
                    to: Some(vec![NetworkPeerPolicy::new("127.0.0.1/32")]),
                    ports: Some(vec![NetworkPortPolicy {
                        protocol: Some(NetworkProtocol::Tcp),
                        port: Some(p.port),
                        end_port: None,
                    }]),
                }]
            }),
        }),
        ingress: Some(NetworkIngressPolicy {
            default: Some(NetworkAction::Deny),
            host_loopback: Some(if proxy.is_some() {
                NetworkAction::Allow
            } else {
                NetworkAction::Deny
            }),
        }),
        runtime_config: None,
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
        // A scriptblock invocation otherwise masks a final nonterminating
        // cmdlet error or native exit code. Inspect success inside its scope,
        // before the invocation itself can reset PowerShell's $? flag.
        let body = format!(
            "{command}\nif (-not $?) {{ if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}; exit 1 }}"
        );
        std::fs::write(&script, body).map_err(|e| e.to_string())?;
        // A fresh private profile has no module-analysis cache. Import the
        // standard command modules directly to avoid scanning every installed
        // module on the first Get-Content, Start-Process or Invoke-WebRequest.
        let gate = format!("if ([Console]::ReadLine() -ne 'pipeline-command-start') {{ exit 125 }}; [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); $OutputEncoding = [Console]::OutputEncoding; Import-Module Microsoft.PowerShell.Management,Microsoft.PowerShell.Utility -ErrorAction Stop; if ($env:HTTPS_PROXY) {{ [Net.WebRequest]::DefaultWebProxy = [Net.WebProxy]::new($env:HTTPS_PROXY) }}; & ([ScriptBlock]::Create([IO.File]::ReadAllText('{}'))); if (-not $?) {{ exit 1 }}", text(&script)?.replace('\'', "''"));
        let bytes: Vec<u8> = gate.encode_utf16().flat_map(u16::to_le_bytes).collect();
        Ok(format!(
            "\"{}\" -NoLogo -NoProfile -NonInteractive -OutputFormat Text -EncodedCommand {}",
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

    #[cfg(not(windows))]
    #[tokio::test]
    async fn proxy_policy_keeps_direct_network_and_ingress_denied() {
        let proxy = proxy::Proxy::start(vec!["example.com".into()])
            .await
            .unwrap();
        let policy = network_policy(Some(&proxy));
        assert_eq!(
            policy.runtime_config.unwrap().network_proxy,
            Some(proxy.url())
        );
        let ingress = policy.ingress.unwrap();
        assert_eq!(ingress.default, Some(NetworkAction::Deny));
        assert_eq!(ingress.host_loopback, Some(NetworkAction::Deny));
        let egress = policy.egress.unwrap();
        assert_eq!(egress.default, Some(NetworkAction::Deny));
        assert!(egress.allow.is_none());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_proxy_policy_uses_only_the_guarded_endpoint() {
        let proxy = proxy::Proxy::start(vec!["example.com".into()])
            .await
            .unwrap();
        let policy = network_policy(Some(&proxy));
        assert!(policy.runtime_config.is_none());
        let ingress = policy.ingress.unwrap();
        assert_eq!(ingress.default, Some(NetworkAction::Deny));
        assert_eq!(ingress.host_loopback, Some(NetworkAction::Allow));
        let egress = policy.egress.unwrap();
        assert_eq!(egress.default, Some(NetworkAction::Deny));
        let allow = egress.allow.unwrap();
        assert_eq!(allow.len(), 1);
        assert_eq!(allow[0].ports.as_ref().unwrap()[0].port, Some(proxy.port));
        let offline = network_policy(None);
        assert!(offline.egress.unwrap().allow.is_none());
        assert_eq!(
            offline.ingress.unwrap().host_loopback,
            Some(NetworkAction::Deny)
        );
    }
}
