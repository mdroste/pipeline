//! Browser download staging; profiles live outside the replaceable install.
use super::*;
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
