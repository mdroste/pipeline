//! Builds the main window from its config entry (`"create": false`) so its
//! webview can refuse navigation away from the app and popup windows.
//!
//! Without these guards a webview's own commands (a context-menu "Open Link",
//! "Open link in new window", or `window.open`) could replace the app with an
//! external page or open a bare browser window inside Pipeline. External links
//! open in the system browser through the shell plugin instead.

use tauri::{webview::NewWindowResponse, App, Url, WebviewWindowBuilder};

pub const LABEL: &str = "main";

pub fn create(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == LABEL)
        .cloned()
        .ok_or("the main window is missing from the Tauri config")?;
    let dev_url = tauri::is_dev()
        .then(|| app.config().build.dev_url.clone())
        .flatten();
    WebviewWindowBuilder::from_config(app.handle(), &config)?
        .on_navigation(move |url| allows_navigation(url, dev_url.as_ref()))
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .build()?;
    Ok(())
}

/// The app's own pages, its `pipeline-file` previews (WebKit also asks about
/// frame navigations), and the blank and blob documents frames start from.
fn allows_navigation(url: &Url, dev_url: Option<&Url>) -> bool {
    let host = url.host_str();
    let app_host = matches!(host, Some("tauri.localhost" | "pipeline-file.localhost"));
    match url.scheme() {
        "tauri" => host == Some("localhost"),
        "pipeline-file" | "about" | "blob" => true,
        "http" | "https" => app_host || dev_url.is_some_and(|dev| dev.origin() == url.origin()),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(url: &str, dev: Option<&str>) -> bool {
        let dev = dev.map(|dev| Url::parse(dev).unwrap());
        allows_navigation(&Url::parse(url).unwrap(), dev.as_ref())
    }

    #[test]
    fn app_pages_and_previews_stay_in_the_window() {
        assert!(allowed("tauri://localhost/index.html#/home", None));
        assert!(allowed("http://tauri.localhost/#/projects", None));
        assert!(allowed("https://tauri.localhost/", None));
        assert!(allowed("pipeline-file://localhost/abc123", None));
        assert!(allowed("http://pipeline-file.localhost/abc123", None));
        assert!(allowed("about:blank", None));
        assert!(allowed("about:srcdoc", None));
        assert!(allowed("blob:http://tauri.localhost/1234", None));
    }

    #[test]
    fn external_and_lookalike_pages_are_refused() {
        assert!(!allowed("https://github.com/mdroste/pipeline/issues", None));
        assert!(!allowed("https://tauri.localhost.example.com/", None));
        assert!(!allowed("tauri://example.com/", None));
        assert!(!allowed("file:///C:/Users/report.html", None));
        assert!(!allowed("mailto:someone@example.org", None));
        assert!(!allowed("data:text/html,<p>x</p>", None));
    }

    #[test]
    fn the_dev_server_is_allowed_only_on_its_own_origin() {
        let dev = Some("http://localhost:1420");
        assert!(allowed("http://localhost:1420/#/home", dev));
        assert!(!allowed("http://localhost:1420/#/home", None));
        assert!(!allowed("http://localhost:8080/", dev));
    }
}
