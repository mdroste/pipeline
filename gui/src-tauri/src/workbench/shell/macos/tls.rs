//! File-based system trust for TLS clients without granting trust-service IPC.
use mxc_sdk::v1::FilesystemPolicy;
use std::{path::Path, process::Command, sync::OnceLock};

const ROOT_KEYCHAIN: &str = "/System/Library/Keychains/SystemRootCertificates.keychain";

fn system_roots() -> Result<&'static [u8], String> {
    static ROOTS: OnceLock<Result<Vec<u8>, String>> = OnceLock::new();
    ROOTS
        .get_or_init(|| {
            // Export public system roots only, before launching the command.
            // /etc/ssl/cert.pem can lag behind the OS's actual root store.
            let output = Command::new("/usr/bin/security")
                .args(["find-certificate", "-a", "-p", ROOT_KEYCHAIN])
                .output()
                .map_err(|error| format!("Cannot read macOS system TLS roots: {error}"))?;
            if !output.status.success()
                || !output
                    .stdout
                    .windows(b"-----BEGIN CERTIFICATE-----".len())
                    .any(|bytes| bytes == b"-----BEGIN CERTIFICATE-----")
            {
                return Err("Cannot export macOS system TLS roots for this command.".into());
            }
            Ok(output.stdout)
        })
        .as_deref()
        .map_err(Clone::clone)
}

pub(crate) fn configure(
    scratch: &Path,
    filesystem: &mut FilesystemPolicy,
    environment: &mut Vec<(String, String)>,
) -> Result<(), String> {
    let bundle = scratch.join("system-tls-roots.pem");
    std::fs::write(&bundle, system_roots()?).map_err(|error| error.to_string())?;
    let bundle = bundle
        .to_str()
        .ok_or("Invalid TLS certificate bundle path")?
        .to_owned();
    filesystem.readonly_paths.push(bundle.clone());
    for key in ["CURL_CA_BUNDLE", "SSL_CERT_FILE"] {
        environment.push((key.into(), bundle.clone()));
    }
    // Apple's MultiSSL curl otherwise uses SecureTransport, which needs
    // trust-service IPC even with a CA file. Its OpenSSL backend verifies
    // certificates entirely from this bundle, keeping Seatbelt unchanged.
    environment.push(("CURL_SSL_BACKEND".into(), "openssl".into()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_tls_uses_readonly_public_system_roots() {
        let scratch = tempfile::tempdir().unwrap();
        let mut filesystem = FilesystemPolicy::default();
        let mut environment = Vec::new();
        configure(scratch.path(), &mut filesystem, &mut environment).unwrap();
        let value = |key: &str| &environment.iter().find(|(name, _)| name == key).unwrap().1;
        assert_eq!(value("CURL_SSL_BACKEND"), "openssl");
        assert_eq!(value("CURL_CA_BUNDLE"), value("SSL_CERT_FILE"));
        assert!(filesystem.readonly_paths.contains(value("CURL_CA_BUNDLE")));
        assert_eq!(
            std::fs::read(value("CURL_CA_BUNDLE")).unwrap(),
            system_roots().unwrap()
        );
    }
}
