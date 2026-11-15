//! Per-command HTTP CONNECT / HTTP / SOCKS5 gateway. Domain admission happens
//! before DNS; every resolved address must be public, and connects use those
//! exact addresses. HTTP requests are parsed independently (including on reused
//! connections), so redirects, Host headers and pipelining cannot bypass policy.
//! Dropping the owner closes listeners, tunnels and pending DNS/connect work.
use bytes::Bytes;
use http_body_util::{combinators::UnsyncBoxBody, BodyExt, Empty, Full};
use hyper::{body::Incoming, header, service::service_fn, Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use std::{convert::Infallible, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Semaphore,
};
use tokio_util::sync::CancellationToken;

type Body = UnsyncBoxBody<Bytes, hyper::Error>;
const IO_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) struct Proxy {
    pub port: u16,
    stop: CancellationToken,
    listener: tokio::task::JoinHandle<()>,
}
impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop.cancel();
        self.listener.abort();
    }
}

impl Proxy {
    pub async fn start(domains: Vec<String>) -> Result<Self, String> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let stop = CancellationToken::new();
        let cancel = stop.clone();
        let domains = Arc::new(domains);
        let slots = Arc::new(Semaphore::new(64));
        let listener = tokio::spawn(async move {
            loop {
                let accepted = tokio::select! {
                    _ = cancel.cancelled() => break,
                    result = listener.accept() => result,
                };
                let Ok((stream, _)) = accepted else { break };
                let Ok(slot) = slots.clone().try_acquire_owned() else {
                    continue;
                };
                let domains = domains.clone();
                let stop = cancel.clone();
                tokio::spawn(async move {
                    let slot = Arc::new(slot);
                    tokio::select! {
                        _ = stop.cancelled() => {},
                        _ = serve(stream, domains, stop.clone(), slot) => {},
                    }
                });
            }
        });
        Ok(Self {
            port,
            stop,
            listener,
        })
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

pub(super) fn valid_domain(host: &str) -> bool {
    host.len() <= 253
        && host.contains('.')
        && host.parse::<std::net::IpAddr>().is_err()
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

fn allowed(host: &str, domains: &[String]) -> bool {
    let host = host.to_ascii_lowercase();
    valid_domain(&host)
        && domains.iter().any(|domain| {
            if let Some(suffix) = domain.strip_prefix("*.") {
                host.strip_suffix(suffix)
                    .is_some_and(|prefix| prefix.ends_with('.') && prefix.len() > 1)
            } else {
                host == *domain
            }
        })
}

fn public_addresses(addresses: &[SocketAddr]) -> bool {
    !addresses.is_empty()
        && addresses
            .iter()
            .all(|a| crate::public_web::public_ip(a.ip()))
}

async fn connect(host: &str, port: u16, domains: &[String]) -> Result<TcpStream, ()> {
    if port == 0 || !allowed(host, domains) {
        return Err(());
    }
    tokio::time::timeout(IO_TIMEOUT, async {
        let addresses: Vec<_> = tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| ())?
            .take(65)
            .collect();
        if addresses.len() > 64 || !public_addresses(&addresses) {
            return Err(());
        }
        for address in addresses {
            if let Ok(stream) = TcpStream::connect(address).await {
                return Ok(stream);
            }
        }
        Err(())
    })
    .await
    .map_err(|_| ())?
}

async fn serve(
    mut stream: TcpStream,
    domains: Arc<Vec<String>>,
    stop: CancellationToken,
    slot: Arc<tokio::sync::OwnedSemaphorePermit>,
) -> Result<(), ()> {
    let mut first = [0];
    let n = tokio::time::timeout(IO_TIMEOUT, stream.peek(&mut first))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?;
    if n == 0 {
        return Ok(());
    }
    if first[0] == 5 {
        return socks(&mut stream, &domains).await;
    }
    hyper::server::conn::http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(IO_TIMEOUT)
        .max_buf_size(16 * 1024)
        .serve_connection(
            TokioIo::new(stream),
            service_fn(move |request| http(request, domains.clone(), stop.clone(), slot.clone())),
        )
        .with_upgrades()
        .await
        .map_err(|_| ())
}

fn response(status: StatusCode, text: &'static str) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(
            Full::new(Bytes::from_static(text.as_bytes()))
                .map_err(|never| match never {})
                .boxed_unsync(),
        )
        .unwrap()
}

async fn http(
    mut request: Request<Incoming>,
    domains: Arc<Vec<String>>,
    stop: CancellationToken,
    slot: Arc<tokio::sync::OwnedSemaphorePermit>,
) -> Result<Response<Body>, Infallible> {
    let denied = || {
        response(
            StatusCode::FORBIDDEN,
            "Command site is not allowed or could not be reached.\n",
        )
    };
    let uri = request.uri();
    let is_tunnel = request.method() == Method::CONNECT;
    if !is_tunnel && uri.scheme_str() != Some("http") {
        return Ok(denied());
    }
    let Some(authority) = uri.authority() else {
        return Ok(denied());
    };
    if authority.as_str().contains('@') {
        return Ok(denied());
    }
    let host = authority.host().to_owned();
    let port = authority
        .port_u16()
        .unwrap_or(if is_tunnel { 443 } else { 80 });
    // An invalid explicit port must not silently become port 80/443.
    if authority.as_str().contains(':') && authority.port_u16().is_none() {
        return Ok(denied());
    }
    let Ok(mut remote) = connect(&host, port, &domains).await else {
        return Ok(denied());
    };
    if is_tunnel {
        if uri.path_and_query().is_some() {
            return Ok(denied());
        }
        tokio::spawn(async move {
            let _slot = slot;
            tokio::select! {
                _ = stop.cancelled() => {},
                _ = async {
                    if let Ok(upgraded) = hyper::upgrade::on(&mut request).await {
                        let _ = tokio::io::copy_bidirectional(&mut TokioIo::new(upgraded), &mut remote).await;
                    }
                } => {},
            }
        });
        return Ok(Response::new(
            Empty::new().map_err(|never| match never {}).boxed_unsync(),
        ));
    }
    let authority = authority.as_str().to_owned();
    let path = uri.path_and_query().map_or("/", |p| p.as_str()).to_owned();
    *request.uri_mut() = path.parse().unwrap();
    strip_hop_headers(request.headers_mut());
    // Only the URL's admitted host is forwarded, never the caller's Host.
    request
        .headers_mut()
        .insert(header::HOST, authority.parse().unwrap());
    request.headers_mut().insert(
        header::CONNECTION,
        header::HeaderValue::from_static("close"),
    );
    let Ok((mut sender, connection)) =
        hyper::client::conn::http1::handshake(TokioIo::new(remote)).await
    else {
        return Ok(denied());
    };
    tokio::spawn(async move {
        let _slot = slot;
        tokio::select! { _ = stop.cancelled() => {}, _ = connection => {} }
    });
    match sender.send_request(request).await {
        Ok(mut result) => {
            strip_hop_headers(result.headers_mut());
            Ok(result.map(BodyExt::boxed_unsync))
        }
        Err(_) => Ok(response(
            StatusCode::BAD_GATEWAY,
            "Command site did not return a valid response.\n",
        )),
    }
}

fn strip_hop_headers(headers: &mut hyper::HeaderMap) {
    let extra: Vec<String> = headers
        .get_all(header::CONNECTION)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(|s| s.trim().to_owned())
        .collect();
    for name in extra {
        headers.remove(name);
    }
    for name in [
        "connection",
        "proxy-connection",
        "proxy-authorization",
        "proxy-authenticate",
        "keep-alive",
        "upgrade",
        "te",
        "trailer",
        "transfer-encoding",
    ] {
        headers.remove(name);
    }
}

async fn socks(stream: &mut TcpStream, domains: &[String]) -> Result<(), ()> {
    let mut remote = tokio::time::timeout(IO_TIMEOUT, async {
        if stream.read_u8().await.map_err(|_| ())? != 5 {
            return Err(());
        }
        let n = stream.read_u8().await.map_err(|_| ())?;
        let mut methods = vec![0; n as usize];
        stream.read_exact(&mut methods).await.map_err(|_| ())?;
        if !methods.contains(&0) {
            let _ = stream.write_all(&[5, 255]).await;
            return Err(());
        }
        stream.write_all(&[5, 0]).await.map_err(|_| ())?;
        let mut header = [0; 4];
        stream.read_exact(&mut header).await.map_err(|_| ())?;
        // Remote DNS only. IP literals, BIND and UDP cannot bypass admission.
        if header != [5, 1, 0, 3] {
            return Err(());
        }
        let len = stream.read_u8().await.map_err(|_| ())?;
        let mut name = vec![0; len as usize];
        stream.read_exact(&mut name).await.map_err(|_| ())?;
        let port = stream.read_u16().await.map_err(|_| ())?;
        let host = std::str::from_utf8(&name).map_err(|_| ())?;
        connect(host, port, domains).await
    })
    .await
    .map_err(|_| ())??;
    stream
        .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
        .await
        .map_err(|_| ())?;
    tokio::io::copy_bidirectional(stream, &mut remote)
        .await
        .map_err(|_| ())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_requires_domain_boundaries_and_rejects_ip_literals() {
        let sites = vec!["example.com".into(), "*.example.org".into()];
        for host in [
            "example.com",
            "EXAMPLE.COM",
            "a.example.org",
            "a.b.example.org",
        ] {
            assert!(allowed(host, &sites), "{host}");
        }
        for host in [
            "example.org",
            "badexample.com",
            "example.com.evil.org",
            "127.0.0.1",
            "[::1]",
            "2130706433",
            "example.com.",
            "a..example.org",
        ] {
            assert!(!allowed(host, &sites), "{host}");
        }
    }

    #[test]
    fn any_private_dns_answer_rejects_the_entire_destination() {
        let public = "93.184.215.14:443".parse().unwrap();
        assert!(public_addresses(&[public]));
        assert!(!public_addresses(&[]));
        for address in [
            "127.0.0.1:443",
            "10.0.0.1:443",
            "169.254.169.254:80",
            "[::ffff:127.0.0.1]:443",
            "[::1]:80",
        ] {
            assert!(!public_addresses(&[public, address.parse().unwrap()]));
        }
    }

    #[tokio::test]
    async fn blocked_http_connect_and_socks_never_reach_a_local_service() {
        let target = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let proxy = Proxy::start(vec!["localhost.test".into()]).await.unwrap();
        for request in [
            format!("GET http://127.0.0.1:{}/ HTTP/1.1\r\nHost: localhost.test\r\nConnection: close\r\n\r\n", target.local_addr().unwrap().port()),
            "CONNECT forbidden.example:443 HTTP/1.1\r\nHost: localhost.test\r\nConnection: close\r\n\r\n".into(),
            "GET http://forbidden.example/ HTTP/1.1\r\nHost: localhost.test\r\nConnection: close\r\n\r\n".into(),
        ] {
            let mut client = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, proxy.port)).await.unwrap();
            client.write_all(request.as_bytes()).await.unwrap();
            let mut result = String::new();
            client.read_to_string(&mut result).await.unwrap();
            assert!(result.starts_with("HTTP/1.1 403"), "{result}");
        }
        let mut client = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, proxy.port))
            .await
            .unwrap();
        client.write_all(&[5, 1, 0]).await.unwrap();
        let mut reply = [0; 2];
        client.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply, [5, 0]);
        client
            .write_all(&[5, 1, 0, 1, 127, 0, 0, 1, 0, 80])
            .await
            .unwrap();
        match client.read(&mut reply).await {
            Ok(0) => {}
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
            other => panic!("Forbidden SOCKS request remained open: {other:?}"),
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(100), target.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn dropping_proxy_closes_open_clients_and_listener() {
        let proxy = Proxy::start(vec![]).await.unwrap();
        let port = proxy.port;
        let mut client = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        client.write_all(&[5, 1, 0]).await.unwrap();
        let mut reply = [0; 2];
        client.read_exact(&mut reply).await.unwrap();
        drop(proxy);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), client.read(&mut reply))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        assert!(TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .is_err());
    }
}
