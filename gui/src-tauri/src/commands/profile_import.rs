//! Bounded local and remote profile imports.

use super::*;

/// Maximum file size for imported configs (10 MB). Pipeline configs are
/// small JSON; anything larger is almost certainly the wrong file.
pub(super) const MAX_IMPORT_SIZE: u64 = 10_000_000;
pub(super) const MAX_IMPORT_REDIRECTS: usize = 5;
pub(super) const MAX_RUN_CONTEXT_SIZE: u64 = 64 * 1024 * 1024;

/// Read a file for import, rejecting files above the size limit.
pub(super) fn read_import_file(path: &str) -> Result<String, String> {
    let file = crate::safety::open_regular_file(std::path::Path::new(path))
        .map_err(|e| format!("Failed to read {path}: {e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_IMPORT_SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read {path}: {e}"))?;
    if bytes.len() as u64 > MAX_IMPORT_SIZE {
        return Err(format!(
            "File is too large. Import files should be under {} MB.",
            MAX_IMPORT_SIZE / 1_000_000,
        ));
    }
    String::from_utf8(bytes).map_err(|e| format!("Import file is not valid UTF-8: {e}"))
}

pub(super) fn append_limited(
    buffer: &mut Vec<u8>,
    chunk: &[u8],
    limit: usize,
) -> Result<(), String> {
    if chunk.len() > limit.saturating_sub(buffer.len()) {
        return Err("The fetched file is too large to be a profile.".to_string());
    }
    buffer.extend_from_slice(chunk);
    Ok(())
}

pub(super) async fn read_response_limited(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err("The fetched file is too large to be a profile.".to_string());
    }
    let mut bytes =
        Vec::with_capacity(response.content_length().unwrap_or(0).min(limit as u64) as usize);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Fetch failed: {e}"))?
    {
        append_limited(&mut bytes, &chunk, limit)?;
    }
    Ok(bytes)
}

pub(super) fn import_ip_is_public(address: std::net::IpAddr) -> bool {
    match address {
        std::net::IpAddr::V4(address) => {
            let [first, second, third, _] = address.octets();
            !(address.is_unspecified()
                || address.is_loopback()
                || address.is_private()
                || address.is_link_local()
                || address.is_multicast()
                || address.is_broadcast()
                || first == 0
                || (first == 100 && (64..=127).contains(&second))
                || (first == 192 && second == 0 && third == 0)
                || (first == 192 && second == 0 && third == 2)
                || (first == 192 && second == 88 && third == 99)
                || (first == 198 && (second == 18 || second == 19))
                || (first == 198 && second == 51 && third == 100)
                || (first == 203 && second == 0 && third == 113)
                || first >= 240)
        }
        std::net::IpAddr::V6(address) => {
            if let Some(mapped) = address.to_ipv4_mapped() {
                return import_ip_is_public(std::net::IpAddr::V4(mapped));
            }
            let segments = address.segments();
            // Global IPv6 unicast is 2000::/3. Documentation and 6to4
            // addresses are excluded as well; neither is needed for profile
            // sharing and both complicate destination validation.
            segments[0] & 0xe000 == 0x2000
                && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
                && segments[0] != 0x2002
        }
    }
}

pub(super) fn validate_import_url_shape(mut url: reqwest::Url) -> Result<reqwest::Url, String> {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
    {
        return Err("Profile URL must use HTTPS and cannot contain credentials".to_string());
    }
    let original_host = url.host_str().unwrap_or_default().to_string();
    let host = original_host.trim_end_matches('.').to_string();
    if host.eq_ignore_ascii_case("localhost")
        || host
            .to_ascii_lowercase()
            .strip_suffix(".localhost")
            .is_some()
    {
        return Err("Profile URLs cannot target localhost or private networks".to_string());
    }
    if host != original_host {
        url.set_host(Some(&host))
            .map_err(|_| "Profile URL contains an invalid host".to_string())?;
    }
    url.set_fragment(None);
    Ok(url)
}

pub(super) async fn public_import_addresses(
    url: &reqwest::Url,
) -> Result<(String, Vec<std::net::SocketAddr>, bool), String> {
    let host = url
        .host_str()
        .ok_or("Profile URL is missing a host")?
        .trim_end_matches('.')
        .to_string();
    let port = url
        .port_or_known_default()
        .ok_or("Profile URL has no usable port")?;
    let literal = host.parse::<std::net::IpAddr>().ok();
    let mut addresses = if let Some(address) = literal {
        vec![std::net::SocketAddr::new(address, port)]
    } else {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::net::lookup_host((host.as_str(), port)),
        )
        .await
        .map_err(|_| "Profile URL DNS lookup timed out".to_string())?
        .map_err(|error| format!("Profile URL DNS lookup failed: {error}"))?
        .collect::<Vec<_>>()
    };
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty() {
        return Err("Profile URL host did not resolve to an address".to_string());
    }
    if addresses
        .iter()
        .any(|address| !import_ip_is_public(address.ip()))
    {
        return Err("Profile URLs cannot target localhost or private networks".to_string());
    }
    Ok((host, addresses, literal.is_none()))
}

pub(super) async fn fetch_public_profile_url(url: &str) -> Result<reqwest::Response, String> {
    let mut current = validate_import_url_shape(
        reqwest::Url::parse(url).map_err(|error| format!("Invalid profile URL: {error}"))?,
    )?;
    for redirect_count in 0..=MAX_IMPORT_REDIRECTS {
        let (host, addresses, pin_dns) = public_import_addresses(&current).await?;
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(15));
        if pin_dns {
            builder = builder.resolve_to_addrs(&host, &addresses);
        }
        let response = builder
            .build()
            .map_err(|error| format!("HTTP client error: {error}"))?
            .get(current.clone())
            .header("User-Agent", "pipeline")
            .send()
            .await
            .map_err(|error| format!("Fetch failed: {error}"))?;
        if !response.status().is_redirection() {
            return Ok(response);
        }
        if redirect_count == MAX_IMPORT_REDIRECTS {
            return Err(format!(
                "Profile URL exceeded the {MAX_IMPORT_REDIRECTS}-redirect safety limit"
            ));
        }
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .ok_or("Profile URL redirect omitted the Location header")?
            .to_str()
            .map_err(|_| "Profile URL redirect Location is not valid text")?;
        let next = validate_import_url_shape(
            current
                .join(location)
                .map_err(|error| format!("Invalid profile URL redirect: {error}"))?,
        )?;
        if current.scheme() == "https" && next.scheme() != "https" {
            return Err("Profile URL redirects cannot downgrade HTTPS to HTTP".to_string());
        }
        current = next;
    }
    unreachable!("redirect loop always returns or advances within its fixed bound")
}
