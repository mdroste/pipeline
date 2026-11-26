"""Isolated hidden-console check: Ctrl+C reaches the engine and launcher waits."""
import pathlib, shutil, subprocess, sys, tempfile

launcher = pathlib.Path(sys.argv[1]).resolve()
host = r'''
#![windows_subsystem = "windows"]
use std::sync::atomic::{AtomicBool, Ordering};
static STOP: AtomicBool = AtomicBool::new(false);
#[link(name="kernel32")] unsafe extern "system" {
 fn AttachConsole(pid:u32)->i32;
 fn SetConsoleCtrlHandler(handler:Option<unsafe extern "system" fn(u32)->i32>,add:i32)->i32;
}
unsafe extern "system" fn control(event:u32)->i32 { if event==0 {STOP.store(true,Ordering::SeqCst);1} else {0} }
fn main() {
 unsafe { assert_ne!(AttachConsole(u32::MAX),0); assert_ne!(SetConsoleCtrlHandler(Some(control),1),0); }
 std::fs::write("ready", "ready").unwrap();
 while !STOP.load(Ordering::SeqCst) {std::thread::sleep(std::time::Duration::from_millis(10));}
 std::thread::sleep(std::time::Duration::from_millis(300));
 std::process::exit(130);
}
'''
driver = r'''
#[link(name="kernel32")] unsafe extern "system" {
 fn SetConsoleCtrlHandler(handler:Option<unsafe extern "system" fn(u32)->i32>,add:i32)->i32;
 fn GenerateConsoleCtrlEvent(event:u32,group:u32)->i32;
}
unsafe extern "system" fn control(_:u32)->i32 {1}
fn main() {
 unsafe { assert_ne!(SetConsoleCtrlHandler(Some(control),1),0); }
 let mut child=std::process::Command::new("./pipeline-cli.exe").spawn().unwrap();
 let start=std::time::Instant::now();
 while !std::path::Path::new("ready").is_file() {
  if start.elapsed().as_secs()>10 { child.kill().unwrap(); std::process::exit(2); }
  std::thread::sleep(std::time::Duration::from_millis(10));
 }
 let signal=std::time::Instant::now();
 unsafe { assert_ne!(GenerateConsoleCtrlEvent(0,0),0); }
 let status=child.wait().unwrap();
 let waited=signal.elapsed().as_millis();
 std::fs::write("result",format!("exit={:?}; waited={}ms",status.code(),waited)).unwrap();
 assert_eq!(status.code(),Some(130)); assert!(waited>=250);
}
'''
with tempfile.TemporaryDirectory(prefix="pipeline console qualification ") as temporary:
    directory = pathlib.Path(temporary)
    shutil.copyfile(launcher, directory / "pipeline-cli.exe")
    for name, code in [("pipeline-gui", host), ("control-driver", driver)]:
        source = directory / (name + ".rs")
        source.write_text(code)
        subprocess.run(["rustc", "--edition=2021", str(source), "-o", str(directory / (name + ".exe"))], check=True)
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    result = subprocess.run([str(directory / "control-driver.exe")], cwd=directory,
                            creationflags=subprocess.CREATE_NEW_CONSOLE, startupinfo=startup, timeout=15)
    assert result.returncode == 0, result.returncode
    print("Windows Ctrl+C passed:", (directory / "result").read_text())
