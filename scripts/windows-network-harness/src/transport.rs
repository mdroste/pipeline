//! Reusable bounded pipe channels to one fixed proxy. Frames carry bytes only;
//! neither endpoint accepts a destination, token, handle, or filesystem request.
use std::{
    io::{self, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::Arc,
    time::Duration,
};
const OPEN: u8 = 0;
const DATA: u8 = 1;
const FIN: u8 = 2;
const MAX_FRAME: usize = 16 * 1024;

fn send(pipe: &mut impl Write, kind: u8, data: &[u8]) -> io::Result<()> {
    pipe.write_all(&[kind])?;
    pipe.write_all(&(data.len() as u32).to_le_bytes())?;
    pipe.write_all(data)
}
fn receive(pipe: &mut impl Read) -> io::Result<(u8, Vec<u8>)> {
    let mut header = [0; 5];
    pipe.read_exact(&mut header)?;
    let len = u32::from_le_bytes(header[1..].try_into().unwrap()) as usize;
    if len > MAX_FRAME || !matches!((header[0], len), (OPEN | FIN, 0) | (DATA, 1..=MAX_FRAME)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid bounded relay frame",
        ));
    }
    let mut data = vec![0; len];
    pipe.read_exact(&mut data)?;
    Ok((header[0], data))
}

fn bridge<R: Read, W: Write + Send>(
    input: &mut R,
    output: &mut W,
    mut socket: TcpStream,
) -> io::Result<()> {
    socket.set_read_timeout(Some(Duration::from_secs(15)))?;
    socket.set_write_timeout(Some(Duration::from_secs(15)))?;
    let mut response = socket.try_clone()?;
    std::thread::scope(|scope| {
        let outgoing = scope.spawn(move || {
            let mut buffer = [0; MAX_FRAME];
            let mut shutdown = Shutdown::Read;
            let outcome = loop {
                match response.read(&mut buffer) {
                    Ok(0) => break Ok(()),
                    Ok(n) => {
                        if let Err(error) = send(output, DATA, &buffer[..n]) {
                            break Err(error);
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => {
                        shutdown = Shutdown::Both;
                        break Ok(()); // Network errors close this byte stream.
                    }
                }
            };
            let fin = send(output, FIN, &[]);
            // A clean read EOF is a half-close. The peer may still be waiting
            // for a response, so leave the socket's write direction available.
            let _ = response.shutdown(shutdown);
            outcome.and(fin)
        });
        let incoming = loop {
            match receive(input) {
                Ok((FIN, _)) => break Ok(()),
                Ok((DATA, data)) => {
                    if socket.write_all(&data).is_err() {
                        let _ = socket.shutdown(Shutdown::Both);
                    }
                }
                Ok(_) => {
                    break Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "unexpected OPEN",
                    ));
                }
                Err(error) => break Err(error),
            }
        };
        let _ = socket.shutdown(if incoming.is_ok() {
            Shutdown::Write
        } else {
            Shutdown::Both
        });
        let outgoing = outgoing
            .join()
            .map_err(|_| io::Error::other("relay worker panicked"))?;
        incoming.and(outgoing)
    })
}

pub fn host(mut input: impl Read, mut output: impl Write + Send, port: u16) {
    while let Ok((OPEN, _)) = receive(&mut input) {
        match TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(3)) {
            Ok(socket) => {
                if bridge(&mut input, &mut output, socket).is_err() {
                    break;
                }
            }
            Err(_) => {
                if send(&mut output, FIN, &[]).is_err() {
                    break;
                }
                loop {
                    match receive(&mut input) {
                        Ok((DATA, _)) => {}
                        Ok((FIN, _)) => break,
                        _ => return,
                    }
                }
            }
        }
    }
}

pub fn child(mut input: impl Read, mut output: impl Write + Send, listener: Arc<TcpListener>) {
    while let Ok((socket, _)) = listener.accept() {
        if send(&mut output, OPEN, &[]).is_err() || bridge(&mut input, &mut output, socket).is_err()
        {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_oversized_and_invalid_frames_before_allocation() {
        for frame in [
            [DATA, 1, 0, 1, 0],
            [OPEN, 1, 0, 0, 0],
            [7, 0, 0, 0, 0],
            [DATA, 0, 0, 0, 0],
        ] {
            assert_eq!(
                receive(&mut frame.as_slice()).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
    }

    #[test]
    fn reuses_one_channel_and_preserves_client_half_close() {
        // Entirely local: the echo server responds only AFTER receiving EOF.
        // This catches prematurely closing the reverse direction on FIN, and
        // exercises many more uses than the workload's channel-pool size.
        const CONNECTIONS: usize = 96;
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let echo = std::thread::spawn(move || {
            for _ in 0..CONNECTIONS {
                let (mut socket, _) = server.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = Vec::new();
                socket.read_to_end(&mut bytes).unwrap();
                socket.write_all(&bytes).unwrap();
            }
        });
        let (up_reader, mut up_writer) = std::io::pipe().unwrap();
        let (mut down_reader, down_writer) = std::io::pipe().unwrap();
        let host = std::thread::spawn(move || super::host(up_reader, down_writer, port));
        for i in 0..CONNECTIONS {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let (socket, _) = listener.accept().unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let payload = vec![(i % 251) as u8; MAX_FRAME + i];
            std::thread::scope(|scope| {
                let relay = scope.spawn(|| {
                    send(&mut up_writer, OPEN, &[]).unwrap();
                    bridge(&mut down_reader, &mut up_writer, socket).unwrap();
                });
                client.write_all(&payload).unwrap();
                client.shutdown(Shutdown::Write).unwrap();
                let mut actual = Vec::new();
                client.read_to_end(&mut actual).unwrap();
                relay.join().unwrap();
                assert_eq!(actual.len(), payload.len(), "connection {i} truncated");
                assert_eq!(actual, payload);
            });
        }
        drop(up_writer);
        host.join().unwrap();
        echo.join().unwrap();
    }
}
