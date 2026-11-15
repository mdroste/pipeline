//! WebView2's PDF API writes to private staging without a printer or dialog.
use super::*;
use std::os::windows::ffi::OsStrExt;
use webview2_com::{Microsoft::Web::WebView2::Win32::*, PrintToPdfCompletedHandler};
use windows::core::{Interface, PCWSTR};

pub(super) fn print(view: tauri::webview::PlatformWebview, path: PathBuf, done: Completion) {
    // COM objects are used only on the webview's UI thread. WebView2 retains
    // the completion handler until the asynchronous print finishes.
    let result = unsafe { start(view, path, done.clone()) };
    if let Err(error) = result {
        complete(&done, Err(format!("Windows PDF export failed: {error}")));
    }
}

unsafe fn start(
    view: tauri::webview::PlatformWebview,
    path: PathBuf,
    done: Completion,
) -> windows::core::Result<()> {
    let browser: ICoreWebView2_7 = view.controller().CoreWebView2()?.cast()?;
    let environment: ICoreWebView2Environment6 = view.environment().cast()?;
    let settings = environment.CreatePrintSettings()?;
    settings.SetPageWidth(WIDTH / 72.)?;
    settings.SetPageHeight(HEIGHT / 72.)?;
    settings.SetMarginTop(TOP / 72.)?;
    settings.SetMarginBottom(BOTTOM / 72.)?;
    settings.SetMarginLeft(SIDE / 72.)?;
    settings.SetMarginRight(SIDE / 72.)?;
    settings.SetScaleFactor(1.)?;
    settings.SetShouldPrintBackgrounds(true)?;
    settings.SetShouldPrintHeaderAndFooter(false)?;
    settings.SetShouldPrintSelectionOnly(false)?;
    let handler = PrintToPdfCompletedHandler::create(Box::new(move |result, success| {
        complete(
            &done,
            result.map_err(|e| e.to_string()).and_then(|_| {
                if success {
                    Ok(())
                } else {
                    Err("Windows could not finish printing the report to PDF".into())
                }
            }),
        );
        Ok(())
    }));
    let filename: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    browser.PrintToPdf(PCWSTR(filename.as_ptr()), &settings, &handler)
}
