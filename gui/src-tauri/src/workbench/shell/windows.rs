//! Windows command networking requires a narrow elevated firewall guard because
//! ProcessContainer's host-loopback grant is broader than its egress rules.
//! The ordinary app and workload remain unelevated. One temporary copy of this
//! executable authenticates its parent, installs only per-AppContainer block
//! rules, and stops those identities before removing them. No service is installed.
//! The helper dispatches before Tauri, plugins, storage or desktop initialization.
mod firewall;
mod identity;
mod pipe;
#[cfg(test)]
mod tests;

use identity::Handle;
use pipe::Pipe;
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    System::Threading::*,
    UI::{Shell::*, WindowsAndMessaging::SW_HIDE},
};

const MODE: &str = "--pipeline-command-network-guard";
const STARTUP: Duration = Duration::from_secs(20);
const RPC: Duration = Duration::from_secs(5);
static BROKER: Mutex<Option<Arc<Mutex<Broker>>>> = Mutex::new(None);

#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", deny_unknown_fields)]
enum Request {
    Hello { version: u8 },
    Register { pid: u32, port: u16 },
    Check { id: String },
    Finish { id: String },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    result: Result<String, String>,
}
struct Broker {
    pipe: Pipe,
    process: Handle,
}
impl Broker {
    fn rpc(&self, request: Request) -> Result<String, String> {
        if !self.process.alive() {
            return Err("The Windows command firewall guard stopped".into());
        }
        self.pipe
            .write(&serde_json::to_vec(&request).map_err(|e| e.to_string())?)?;
        serde_json::from_slice::<Reply>(&self.pipe.receive(RPC)?)
            .map_err(|e| e.to_string())?
            .result
    }
    fn start() -> Result<Self, String> {
        let parent = identity::process(std::process::id(), false)?;
        let nonce = firewall::nonce()?;
        #[cfg(test)]
        if let Some(broker) = tests::fixture_broker(&parent, &nonce)? {
            return Ok(broker);
        }
        let args = format!(
            "{MODE} {} {} {nonce}",
            std::process::id(),
            identity::created(&parent)?
        );
        let executable = helper_executable()?;
        let executable = executable
            .to_str()
            .ok_or("Invalid Pipeline executable path")?;
        let process = launch(executable, &args)?;
        let pid = unsafe { GetProcessId(process.0) };
        let name = pipe_name(std::process::id(), &nonce);
        let deadline = Instant::now() + STARTUP;
        let pipe = loop {
            match Pipe::client(&name, pid) {
                Ok(pipe) => break pipe,
                Err(error) => {
                    if !process.alive() || Instant::now() >= deadline {
                        return Err(format!(
                            "Windows command firewall guard could not start: {error}"
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        };
        let broker = Self { pipe, process };
        broker.rpc(Request::Hello { version: 1 })?;
        Ok(broker)
    }
}
fn launch(executable: &str, args: &str) -> Result<Handle, String> {
    #[cfg(test)]
    if identity::elevated()? && std::env::var_os("PIPELINE_NATIVE_TEST_SHELL_EXECUTE").is_none() {
        use std::os::windows::io::IntoRawHandle;
        let child = std::process::Command::new(executable)
            .args(args.split_whitespace())
            .spawn()
            .map_err(|e| e.to_string())?;
        return Handle::new(child.into_raw_handle());
    }
    let file = identity::wide(executable);
    let parameters = identity::wide(args);
    let verb = identity::wide("runas");
    let mut launch = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: parameters.as_ptr(),
        nShow: SW_HIDE,
        ..Default::default()
    };
    if unsafe { ShellExecuteExW(&mut launch) } == 0 {
        return Err(format!("Windows command internet access requires approval for Pipeline's temporary firewall guard. {}", identity::error()));
    }
    Handle::new(launch.hProcess)
}

fn helper_executable() -> Result<std::path::PathBuf, String> {
    #[cfg(test)]
    if let Some(path) = std::env::var_os("PIPELINE_NATIVE_TEST_HELPER") {
        return Ok(path.into());
    }
    std::env::current_exe().map_err(|e| e.to_string())
}
fn pipe_name(parent: u32, nonce: &str) -> String {
    format!(r"\\.\pipe\Pipeline.CommandFirewall.{parent}.{nonce}")
}

pub(super) struct Guard {
    broker: Arc<Mutex<Broker>>,
    id: Option<String>,
    port: u16,
}
impl Guard {
    pub fn prepare(request: &mxc_sdk::v1::ContainerRequest) -> Result<Option<Self>, String> {
        use mxc_sdk::v1::{NetworkAction, NetworkProtocol};
        let Some(network) = &request.network else {
            return Err("Windows command network policy is missing".into());
        };
        let loopback = network.ingress.as_ref().and_then(|p| p.host_loopback);
        if loopback != Some(NetworkAction::Allow) {
            return Ok(None);
        }
        let port = network
            .egress
            .as_ref()
            .and_then(|p| p.allow.as_ref())
            .filter(|rules| rules.len() == 1)
            .and_then(|rules| rules[0].ports.as_ref())
            .filter(|ports| ports.len() == 1)
            .filter(|ports| ports[0].protocol == Some(NetworkProtocol::Tcp))
            .and_then(|ports| ports[0].port)
            .filter(|port| *port != 0)
            .ok_or("Windows command proxy endpoint is missing")?;
        let mut shared = BROKER.lock().unwrap_or_else(|e| e.into_inner());
        let alive = shared.as_ref().is_some_and(|broker| {
            broker
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .process
                .alive()
        });
        if !alive {
            *shared = Some(Arc::new(Mutex::new(Broker::start()?)));
        }
        Ok(Some(Self {
            broker: shared.as_ref().unwrap().clone(),
            id: None,
            port,
        }))
    }
    pub fn attach(&mut self, pid: u32) -> Result<(), String> {
        self.id = Some(self.broker.lock().unwrap_or_else(|e| e.into_inner()).rpc(
            Request::Register {
                pid,
                port: self.port,
            },
        )?);
        Ok(())
    }
    pub fn check(&self) -> Result<(), String> {
        let id = self
            .id
            .as_ref()
            .ok_or("Windows command firewall was not attached")?;
        self.broker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .rpc(Request::Check { id: id.clone() })
            .map(|_| ())
    }
    pub fn finish(&mut self) -> Result<(), String> {
        let Some(id) = self.id.take() else {
            return Ok(());
        };
        self.broker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .rpc(Request::Finish { id })
            .map(|_| ())
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

pub(super) fn run_if_requested() -> bool {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_none_or(|a| a != MODE) {
        return false;
    }
    let result = (|| {
        if args.len() != 5 {
            return Err("Invalid firewall guard arguments".into());
        }
        let parent = args[2]
            .to_str()
            .and_then(|v| v.parse::<u32>().ok())
            .ok_or("Invalid firewall parent")?;
        let created = args[3]
            .to_str()
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or("Invalid firewall parent timestamp")?;
        let nonce = args[4]
            .to_str()
            .filter(|v| v.len() == 48 && v.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or("Invalid firewall pipe name")?;
        serve(parent, created, nonce)
    })();
    if let Err(error) = result {
        eprintln!("Pipeline command firewall: {error}");
    }
    true
}
struct Active {
    rules: firewall::Rules,
    since: Instant,
}
fn serve(parent_pid: u32, created: u64, nonce: &str) -> Result<(), String> {
    if !identity::elevated()? {
        return Err("Firewall guard requires administrator approval".into());
    }
    identity::enable_cleanup_privilege()?;
    let parent = identity::process(parent_pid, false)?;
    if identity::created(&parent)? != created
        || !parent.alive()
        || identity::package(&parent)?.is_some()
    {
        return Err("Invalid firewall guard parent".into());
    }
    let user = identity::user(&parent)?;
    let pipe = Pipe::server(&pipe_name(parent_pid, nonce), &user)?;
    let deadline = Instant::now() + STARTUP;
    while !pipe.accept(parent_pid)? {
        if !parent.alive() || Instant::now() >= deadline {
            return Err("Firewall client did not connect".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let hello: Request = serde_json::from_slice(&pipe.receive(RPC)?).map_err(|e| e.to_string())?;
    if !matches!(hello, Request::Hello { version: 1 }) {
        return Err("Unsupported firewall guard protocol".into());
    }
    let engine = match firewall::Engine::open() {
        Ok(engine) => engine,
        Err(error) => {
            reply(&pipe, Err(error.clone()))?;
            return Err(error);
        }
    };
    reply(&pipe, Ok(String::new()))?;
    let mut active = std::collections::BTreeMap::<String, Active>::new();
    let result = (|| {
        while parent.alive() {
            if active
                .values()
                .any(|entry| entry.since.elapsed() > Duration::from_secs(150))
            {
                return Err("Command firewall lease expired".into());
            }
            let Some(bytes) = pipe.read()? else {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            };
            let request: Request = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            let result = match request {
                Request::Register { pid, port } => (|| {
                    if active.len() >= 64 || !identity::is_child(pid, parent_pid)? {
                        return Err("Invalid command firewall registration".into());
                    }
                    let child = identity::process(pid, false)?;
                    if identity::created(&child)? < created || !child.alive() {
                        return Err("Command exited before firewall setup".into());
                    }
                    let package =
                        identity::package(&child)?.ok_or("Command has no sandbox identity")?;
                    if active
                        .values()
                        .any(|entry| entry.rules.package() == package)
                    {
                        return Err("Sandbox identity is already registered".into());
                    }
                    let rules = engine.restrict(&child, port, &user)?;
                    let id = rules.id().to_owned();
                    active.insert(
                        id.clone(),
                        Active {
                            rules,
                            since: Instant::now(),
                        },
                    );
                    Ok(id)
                })(),
                Request::Check { id } => active
                    .contains_key(&id)
                    .then(String::new)
                    .ok_or("Command firewall lease is missing".into()),
                Request::Finish { id } => (|| {
                    let entry = active.get(&id).ok_or("Command firewall lease is missing")?;
                    engine.finish(&entry.rules)?;
                    active.remove(&id);
                    Ok(String::new())
                })(),
                Request::Hello { .. } => Err("Firewall guard is already connected".into()),
            };
            reply(&pipe, result)?;
        }
        Ok(())
    })();
    for entry in active.values() {
        let _ = engine.finish(&entry.rules);
    }
    result
}
fn reply(pipe: &Pipe, result: Result<String, String>) -> Result<(), String> {
    pipe.write(&serde_json::to_vec(&Reply { result }).map_err(|e| e.to_string())?)
}
