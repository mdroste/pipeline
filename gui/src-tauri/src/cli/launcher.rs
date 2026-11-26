//! Dependency-free compatibility command. The signed desktop executable owns
//! the engine, resources and version. Keep this file usable by plain rustc.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MARKER: &str = "pipeline-cli-launcher-v1";
const LOCATOR: &str = "pipeline-cli-app";

fn application(executable: &Path) -> Result<PathBuf, String> {
    let directory = executable.parent().ok_or("Missing command directory")?;
    // Bundled commands and symlinks to them preserve their original location.
    let sibling = directory.join(if cfg!(windows) {
        "pipeline-gui.exe"
    } else {
        "pipeline-gui"
    });
    if sibling.is_file() {
        return Ok(sibling);
    }
    let locator = directory.join(LOCATOR);
    let metadata = std::fs::symlink_metadata(&locator).map_err(|_| "Pipeline was moved or removed. Open Settings → Advanced & About → Command-line tools and choose Repair.")?;
    if !metadata.is_file() || metadata.len() > 32_768 {
        return Err(
            "Invalid Pipeline command location; repair Command-line tools in Settings.".into(),
        );
    }
    let contents = std::fs::read_to_string(locator).map_err(|e| e.to_string())?;
    let app = PathBuf::from(contents);
    if !app.is_absolute() || !app.is_file() {
        return Err("Pipeline was moved or removed. Repair Command-line tools in Settings.".into());
    }
    Ok(app)
}

pub fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    // Used by packaging and the app before installing a launcher. Does not
    // start the GUI or depend on its location.
    if args.as_slice() == [std::ffi::OsString::from("--pipeline-launcher-info")] {
        println!("{MARKER}");
        return;
    }
    let result = std::env::current_exe()
        .map_err(|e| e.to_string())
        .and_then(|path| application(&path));
    let app = match result {
        Ok(app) => app,
        Err(error) => {
            eprintln!("pipeline-cli: {error}");
            std::process::exit(1);
        }
    };
    let mut command = Command::new(app);
    command
        .arg("--pipeline-cli")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        eprintln!("pipeline-cli: {error}");
        std::process::exit(1);
    }
    #[cfg(windows)]
    {
        // Ctrl+C belongs to the engine. Do not let the waiting launcher exit
        // early or set the inheritable CTRL_C_EVENT-ignore process flag.
        unsafe extern "system" fn control(event: u32) -> i32 {
            i32::from(event <= 1)
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn SetConsoleCtrlHandler(
                handler: Option<unsafe extern "system" fn(u32) -> i32>,
                add: i32,
            ) -> i32;
        }
        unsafe {
            SetConsoleCtrlHandler(Some(control), 1);
        }
        match command.status() {
            Ok(status) => std::process::exit(status.code().unwrap_or(1)),
            Err(error) => {
                eprintln!("pipeline-cli: {error}");
                std::process::exit(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_app_is_an_actionable_error() {
        let error = application(Path::new("/missing/pipeline-cli")).unwrap_err();
        assert!(error.contains("Repair"));
    }
}
