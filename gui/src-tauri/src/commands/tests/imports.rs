//! Imports regression coverage.

use super::*;

#[test]
fn chunked_import_limit_is_enforced_before_appending() {
    let mut buffer = vec![0u8; 8];
    assert!(append_limited(&mut buffer, &[1, 2], 10).is_ok());
    assert_eq!(buffer.len(), 10);
    assert!(append_limited(&mut buffer, &[3], 10).is_err());
    assert_eq!(buffer.len(), 10);
}

#[test]
fn profile_url_import_rejects_non_public_destinations() {
    for address in [
        "127.0.0.1",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.0.1",
        "169.254.1.1",
        "100.64.0.1",
        "192.0.2.1",
        "198.18.0.1",
        "198.51.100.1",
        "203.0.113.1",
        "::1",
        "fe80::1",
        "fc00::1",
        "2001:db8::1",
        "::ffff:127.0.0.1",
    ] {
        let address = address.parse().unwrap();
        assert!(!import_ip_is_public(address), "{address}");
    }
    for address in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
        let address = address.parse().unwrap();
        assert!(import_ip_is_public(address), "{address}");
    }
}

#[test]
fn profile_url_import_rejects_credentials_and_localhost() {
    for url in [
        "file:///tmp/profile.json",
        "http://example.com/profile.json",
        "http://localhost/profile.json",
        "http://worker.localhost/profile.json",
        "https://user:secret@example.com/profile.json",
    ] {
        let parsed = reqwest::Url::parse(url).unwrap();
        assert!(validate_import_url_shape(parsed).is_err(), "{url}");
    }
    let normalized = validate_import_url_shape(
        reqwest::Url::parse("https://example.com./profile.json#fragment").unwrap(),
    )
    .unwrap();
    assert_eq!(normalized.host_str(), Some("example.com"));
    assert!(normalized.fragment().is_none());
}
