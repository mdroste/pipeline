//! Browser download staging; profiles live outside the replaceable install.
use super::*;

/// Starts the executable conversations use, not Playwright's separate headless
/// build, because that is the file a system policy would block. The outcome
/// goes to stdout so a refusal isn't reported as a download problem.
const START_CHECK: &str = "const {chromium}=require('playwright');chromium.launch({headless:true,chromiumSandbox:true,executablePath:chromium.executablePath()}).then(b=>b.close()).then(()=>process.stdout.write('ready'),e=>process.stdout.write(String(e&&e.message||e)))";

async fn confirm_start(command: std::process::Command) -> Result<(), String> {
    let outcome = super::super::tool_setup::run(command, "Browser start check").await?;
    if outcome == b"ready" {
        return Ok(());
    }
    let reason = String::from_utf8_lossy(&outcome);
    // Conversations start the browser with this same sandbox, so it can't run
    // here without one and failing setup is right. Only the cause differs.
    if reason.contains("No usable sandbox") {
        return Err("The browser was installed but could not start, because this Linux system does not allow the sandbox Pipeline runs it in. Allow unprivileged user namespaces for the browser, then retry.".into());
    }
    let reason: String = reason.chars().take(500).collect();
    Err(format!(
        "The browser was installed but could not start. Security software or a system policy, such as Windows Smart App Control, may be blocking it. {reason}"
    ))
}

pub(super) async fn install(root: PathBuf, runtime: &std::path::Path) -> Result<(), String> {
    let _progress = SetupProgress;
    *BROWSER_PHASE.lock().unwrap_or_else(|e| e.into_inner()) = Some("Downloading browser…".into());
    crate::agent_runtime::codex::session::private_directory(&root)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join("setup.lock"))
        .map_err(|e| e.to_string())?;
    fs2::FileExt::try_lock_exclusive(&lock)
        .map_err(|_| "Another Pipeline instance is setting up the browser. Retry shortly.")?;
    let stage = tempfile::Builder::new()
        .prefix("setup-")
        .tempdir_in(&root)
        .map_err(|e| e.to_string())?;
    let mut command = super::super::tool_setup::command_at(runtime);
    command
        .arg(runtime.to_path_buf().join("node_modules/playwright/cli.js"))
        .args(["install", "chromium"])
        .env("PLAYWRIGHT_BROWSERS_PATH", stage.path());
    super::super::tool_setup::run(command, "Browser download").await?;
    let mut verify = super::super::tool_setup::command_at(runtime);
    verify.args(["-e", "const p=require('playwright').chromium.executablePath(); if(!require('node:fs').existsSync(p)) process.exit(1); process.stdout.write(p)"])
            .env("PLAYWRIGHT_BROWSERS_PATH", stage.path());
    let result = super::super::tool_setup::run(verify, "Browser verification").await?;
    let path = PathBuf::from(String::from_utf8(result).map_err(|e| e.to_string())?);
    let relative = path
        .strip_prefix(stage.path())
        .map_err(|_| "Browser executable is outside its installation")?;
    *BROWSER_PHASE.lock().unwrap_or_else(|e| e.into_inner()) =
        Some("Checking that the browser starts…".into());
    let mut start = super::super::tool_setup::command_at(runtime);
    start
        .args(["-e", START_CHECK])
        .env("PLAYWRIGHT_BROWSERS_PATH", stage.path());
    confirm_start(start).await?;
    std::fs::write(
        stage.path().join("complete"),
        relative.to_string_lossy().as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    let target = browser_root(&root);
    if target.exists() {
        std::fs::rename(
            &target,
            root.join(format!(
                "incomplete-{}",
                chrono::Utc::now().timestamp_micros()
            )),
        )
        .map_err(|e| e.to_string())?;
    }
    std::fs::rename(stage.path(), target).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real check against a stand-in Playwright, so no browser is needed.
    /// Skipped where no Node is on the path.
    async fn start_check(launch: &str) -> Option<Result<(), String>> {
        let folder = tempfile::tempdir().unwrap();
        let module = folder.path().join("node_modules/playwright");
        std::fs::create_dir_all(&module).unwrap();
        std::fs::write(
            module.join("index.js"),
            format!("exports.chromium={{executablePath:()=>'browser',launch:{launch}}}"),
        )
        .unwrap();
        let node = std::process::Command::new("node").arg("--version").output();
        node.ok()?;
        let mut command = std::process::Command::new("node");
        command.current_dir(folder.path()).args(["-e", START_CHECK]);
        Some(confirm_start(command).await)
    }

    #[tokio::test]
    async fn a_blocked_browser_fails_setup_and_says_why() {
        let Some(outcome) = start_check("async()=>{throw new Error('spawn EPERM')}").await else {
            return;
        };
        let message = outcome.unwrap_err();
        assert!(message.contains("could not start"), "{message}");
        assert!(message.contains("spawn EPERM"), "{message}");
        assert!(!message.contains("internet"), "{message}");
    }

    #[tokio::test]
    async fn a_linux_system_without_the_sandbox_is_named_as_the_cause() {
        // Playwright puts the browser's command line ahead of its own error.
        let launch = format!(
            "async()=>{{throw new Error('{} FATAL: No usable sandbox!')}}",
            "--flag ".repeat(200)
        );
        let Some(outcome) = start_check(&launch).await else {
            return;
        };
        let message = outcome.unwrap_err();
        assert!(
            message.contains("unprivileged user namespaces"),
            "{message}"
        );
        assert!(!message.contains("Smart App Control"), "{message}");
    }

    #[test]
    fn the_start_check_uses_the_same_sandbox_as_conversations() {
        assert!(START_CHECK.contains("chromiumSandbox:true"));
        assert!(include_str!("driver.mjs").contains("chromiumSandbox: true"));
    }

    #[tokio::test]
    async fn a_browser_that_starts_passes_setup() {
        let Some(outcome) = start_check(
            "async(o)=>{if(!o.headless||!o.chromiumSandbox||o.executablePath!=='browser')throw Error('wrong options');return{close:async()=>{}}}",
        )
        .await
        else {
            return;
        };
        outcome.unwrap();
    }

    #[tokio::test]
    #[ignore = "Downloads managed browser into a disposable folder and starts headless Chromium; no model or sign-in"]
    async fn managed_browser_download_and_launch() {
        let (_runtime, runtime) = super::super::super::tool_setup::fixture_runtime().await;
        let browser = tempfile::tempdir().unwrap();
        install(browser.path().into(), &runtime).await.unwrap();
        assert!(executable(browser.path()).is_some());
        let mut command = super::super::super::tool_setup::command_at(&runtime);
        command.env("PLAYWRIGHT_BROWSERS_PATH", browser_root(browser.path())).args(["-e", "(async()=>{const b=await require('playwright').chromium.launch({headless:true,chromiumSandbox:true});try{const p=await b.newPage();await p.goto('data:text/html,<title>Pipeline fixture</title>');if(await p.title()!=='Pipeline fixture')throw Error('wrong title');console.log('ready')}finally{await b.close()}})().catch(e=>{console.error(e);process.exit(1)})"]);
        assert_eq!(
            super::super::super::tool_setup::run(command, "Browser probe")
                .await
                .unwrap(),
            b"ready\n"
        );
    }
}
