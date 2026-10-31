//! Credential-bearing API clients. Redirects and implicit retries are disabled;
//! callers own retry decisions and request deadlines. Loopback bypasses proxies.
use std::sync::LazyLock;
pub(crate) fn client(
    no_proxy: bool,
    timeout: Option<std::time::Duration>,
) -> Result<reqwest::Client, String> {
    let builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(std::time::Duration::from_secs(15));
    let builder = if no_proxy {
        builder.no_proxy()
    } else {
        builder
    };
    let builder = if let Some(timeout) = timeout {
        builder.timeout(timeout)
    } else {
        builder
    };
    builder
        .build()
        .map_err(|_| "Could not set up the model connection.".to_string())
}
fn build_client(no_proxy: bool) -> reqwest::Client {
    client(no_proxy, None).expect("API client initialization failed")
}
/// Shared HTTP client for all direct API calls.
/// `reqwest::Client` wraps an `Arc` internally, so cloning is cheap.
/// Reusing a single client enables TCP/TLS connection pooling across
/// pipeline steps that hit the same API host.
pub static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| build_client(false));
/// Custom/local endpoints must never redirect requests carrying prompts or
/// bearer tokens to a different origin.
pub static LOCAL_HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| build_client(true));
/// HTTPS custom endpoints may use an enterprise proxy, but still cannot
/// redirect a request containing prompts or credentials to another origin.
pub static CUSTOM_HTTPS_HTTP_CLIENT: LazyLock<reqwest::Client> =
    LazyLock::new(|| build_client(false));

pub fn custom_endpoint_client(base_url: &str) -> &'static reqwest::Client {
    if base_url.trim_start().starts_with("https://") {
        &CUSTOM_HTTPS_HTTP_CLIENT
    } else {
        &LOCAL_HTTP_CLIENT
    }
}
