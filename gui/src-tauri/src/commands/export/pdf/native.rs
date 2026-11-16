//! Isolated native renderer, entered before ordinary application setup. The
//! parent supervises this copy of the GUI executable with a process deadline.
//! No plugins, commands, accounts, stores, or frontend window are initialized.
//! The only page is a self-contained local report with a restrictive CSP; its
//! ephemeral webview denies navigation, popups, and downloads.
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{webview::NewWindowResponse, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tokio::sync::oneshot;

#[cfg(target_os = "linux")]
#[path = "native/linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "native/macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "native/windows.rs"]
mod platform;

pub(super) const ARGUMENT: &str = "--pipeline-native-pdf";
pub(super) const WIDTH: f64 = 612.;
pub(super) const HEIGHT: f64 = 792.;
const SIDE: f64 = 59.04;
const TOP: f64 = 51.84;
const BOTTOM: f64 = 50.4;
type Completion = Arc<Mutex<Option<oneshot::Sender<Result<(), String>>>>>;

fn complete(completion: &Completion, result: Result<(), String>) {
    if let Some(sender) = completion.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = sender.send(result);
    }
}

/// Apply the same typesetting while measuring and printing. Native APIs own
/// paper margins; PDF postprocessing supplies page numbers across all engines.
pub(super) fn document(html: &str) -> String {
    let html = html.replacen(
        "<head>",
        concat!(
            "<head><meta http-equiv=\"Content-Security-Policy\" content=\"",
            "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; ",
            "font-src data:; img-src data:; connect-src 'none'; object-src 'none'; ",
            "frame-src 'none'; base-uri 'none'; form-action 'none'\">"
        ),
        1,
    );
    let styles = concat!(
        "<style>",
        "@page { size: Letter; margin: 0.72in 0.82in 0.7in; @bottom-center { content: none; } }",
        "html,body { background: white; }",
        ".report-document { width: 100%; max-width: none; margin: 0; padding: 0; border: 0; box-shadow: none; }",
        "table { max-width: 100%; overflow-wrap: anywhere; }",
        "thead { display: table-header-group; } tr { break-inside: avoid; }",
        ".comment-header,h1,h2,h3 { page-break-after: avoid; page-break-inside: avoid; }",
        ".pipeline-display-equation,.katex-display,tr,pre,blockquote { page-break-inside: avoid; }",
        ".katex-mathml { display: none; }",
        "pre { white-space: pre-wrap; overflow-wrap: anywhere; }",
        "</style>"
    );
    html.replace(
        "</body></html>",
        &format!(
            // Hidden GTK windows can have a one-pixel viewport before they
            // are mapped. Measure at the actual printable column width even
            // then; printing still gets its dimensions from the paper setup.
            "{styles}<style media=\"screen\">html,body{{width:{}px}}</style><script>{}</script></body></html>",
            (WIDTH - 2. * SIDE) * 96. / 72.,
            include_str!("native/layout.js")
        ),
    )
}

/// Internal executable mode, never an IPC command. It may write only a new
/// PDF beside the bounded input file. Ordinary startup never enters this mode.
pub fn run_pdf_helper_if_requested() -> bool {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_none_or(|value| value != ARGUMENT) {
        return false;
    }
    let outcome = (|| {
        if args.len() != 3 {
            return Err("Expected a report input and PDF output".into());
        }
        let input = PathBuf::from(&args[1]);
        let output = PathBuf::from(&args[2]);
        validate_paths(&input, &output)?;
        watch_output_size(output.clone());
        run(input, output)
    })();
    if let Err(error) = outcome {
        eprintln!("Native PDF export: {error}");
        std::process::exit(1);
    }
    true
}

fn watch_output_size(output: PathBuf) {
    // Native printing can block the UI thread. This independent fuse stops a
    // faulty renderer while the parent still owns cleanup of its process tree
    // and staging directory. The parent's deadline covers stalled renderers.
    std::thread::spawn(move || loop {
        if std::fs::metadata(&output).is_ok_and(|file| file.len() > super::MAX_PDF) {
            eprintln!("Native PDF export exceeded the 64 MB limit. Save Markdown instead.");
            std::process::exit(1);
        }
        std::thread::sleep(Duration::from_millis(100));
    });
}

fn validate_paths(input: &Path, output: &Path) -> Result<(), String> {
    if !input.is_absolute()
        || input.file_name().is_none_or(|name| name != "report.html")
        || output.file_name().is_none_or(|name| name != "report.pdf")
        || input.parent() != output.parent()
        || std::fs::symlink_metadata(output).is_ok()
    {
        return Err("Invalid native PDF staging paths".into());
    }
    let file = crate::safety::open_regular_file(input).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
        return Err("Report HTML exceeds the rendering limit".into());
    }
    Ok(())
}

fn run(input: PathBuf, output: PathBuf) -> Result<(), String> {
    // The parent owns this directory, so deadline termination also cleans up
    // a profile whose renderer never reached normal shutdown.
    let profile_path = input.with_file_name("profile");
    std::fs::create_dir(&profile_path).map_err(|e| e.to_string())?;
    let url = tauri::Url::from_file_path(&input).map_err(|_| "Invalid report URL")?;
    let allowed = url.clone();
    let failure: Arc<Mutex<Option<String>>> = Arc::default();
    let result = failure.clone();
    let mut context = crate::app_context();
    context.config_mut().app.windows.clear();
    let (loaded, load_finished) = oneshot::channel();
    let loaded = Mutex::new(Some(loaded));
    let app = tauri::Builder::default()
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let window = WebviewWindowBuilder::new(app, "pdf-renderer", WebviewUrl::External(url))
                .title("PDF export")
                .inner_size((WIDTH - 2. * SIDE) * 96. / 72., HEIGHT * 96. / 72.)
                .visible(false)
                .focused(false)
                .incognito(true)
                .data_directory(profile_path)
                .on_navigation(move |target| target == &allowed)
                .on_new_window(|_, _| NewWindowResponse::Deny)
                .on_download(|_, _| false)
                .on_page_load(move |_, payload| {
                    if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                        if let Some(sender) =
                            loaded.lock().unwrap_or_else(|e| e.into_inner()).take()
                        {
                            let _ = sender.send(());
                        }
                    }
                })
                .build()?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let outcome = async {
                    tokio::time::timeout(Duration::from_secs(30), load_finished)
                        .await
                        .map_err(|_| "Report did not finish loading")?
                        .map_err(|_| "Report closed before loading")?;
                    ready(&window).await?;
                    print(&window, output).await
                }
                .await;
                let code = if let Err(error) = outcome {
                    *result.lock().unwrap_or_else(|e| e.into_inner()) = Some(error);
                    1
                } else {
                    0
                };
                handle.exit(code);
            });
            Ok(())
        })
        .build(context)
        .map_err(|e| e.to_string())?;
    let code = app.run_return(|_, _| {});
    let error = failure.lock().unwrap_or_else(|e| e.into_inner()).take();
    match (code, error) {
        (_, Some(error)) => Err(error),
        (0, None) => Ok(()),
        _ => Err("Native renderer closed before completing the export".into()),
    }
}

async fn ready(window: &WebviewWindow) -> Result<(), String> {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let (sender, receiver) = oneshot::channel();
            let sender = Mutex::new(Some(sender));
            window
                .eval_with_callback("window.pipelineNativePdfState || 'loading'", move |value| {
                    if let Some(sender) = sender.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = sender.send(value);
                    }
                })
                .map_err(|e| e.to_string())?;
            let value = receiver.await.map_err(|_| "Report layout check closed")?;
            let state = serde_json::from_str::<String>(&value).unwrap_or_default();
            if state == "ready" {
                return Ok(());
            }
            if let Some(error) = state.strip_prefix("error:") {
                return Err(error.to_owned());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| "Report fonts or equations did not finish rendering")?
}

async fn print(window: &WebviewWindow, output: PathBuf) -> Result<(), String> {
    let (sender, receiver) = oneshot::channel();
    let completion = Arc::new(Mutex::new(Some(sender)));
    window
        .with_webview(move |view| platform::print(view, output, completion))
        .map_err(|e| e.to_string())?;
    receiver
        .await
        .map_err(|_| "Native printing ended without a result")?
}

#[cfg(test)]
mod tests;
