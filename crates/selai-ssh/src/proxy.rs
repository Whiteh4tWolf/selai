//! Native HTTP CONNECT and SOCKS5 handshakes; the SSH host pin is checked after
//! the tunnel is established. No proxy credential is put in an error or URL.
use crate::{Endpoint, SshError};
use base64::{engine::general_purpose::STANDARD, Engine};
use selai_secrets::SecretValue;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

pub struct ProxyOptions {
    pub endpoint: Endpoint,
    pub kind: ProxyKind,
    pub username: Option<String>,
    pub password: Option<SecretValue>,
}
#[derive(Clone, Copy)]
pub enum ProxyKind {
    Http,
    Socks5,
}

fn fail(message: &str) -> SshError {
    SshError::Protocol(message.into())
}

pub async fn transport(
    endpoint: &Endpoint,
    proxy: Option<&ProxyOptions>,
) -> Result<TcpStream, SshError> {
    let Some(proxy) = proxy else {
        return TcpStream::connect(endpoint.socket_addr_pair())
            .await
            .map_err(|e| SshError::ConnectFailed(e.to_string()));
    };
    let mut stream = TcpStream::connect(proxy.endpoint.socket_addr_pair())
        .await
        .map_err(|_| fail("Cannot connect to proxy. Check its host and port."))?;
    match proxy.kind {
        ProxyKind::Http => {
            let address = if endpoint.host.contains(':') {
                format!("[{}]:{}", endpoint.host, endpoint.port)
            } else {
                format!("{}:{}", endpoint.host, endpoint.port)
            };
            let mut request = zeroize::Zeroizing::new(format!(
                "CONNECT {address} HTTP/1.1\r\nHost: {address}\r\n"
            ));
            if let Some(user) = &proxy.username {
                let password = proxy
                    .password
                    .as_ref()
                    .map(|p| p.expose_for_transport())
                    .unwrap_or_default();
                let mut auth = zeroize::Zeroizing::new(Vec::new());
                auth.extend_from_slice(user.as_bytes());
                auth.push(b':');
                auth.extend_from_slice(password);
                let encoded = zeroize::Zeroizing::new(STANDARD.encode(&auth));
                request.push_str("Proxy-Authorization: Basic ");
                request.push_str(&encoded);
                request.push_str("\r\n");
            }
            request.push_str("\r\n");
            stream.write_all(request.as_bytes()).await?;
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                if header.len() >= 16384 {
                    return Err(fail("Proxy response headers are too large."));
                }
                header.push(stream.read_u8().await?);
            }
            let line = std::str::from_utf8(&header)
                .map_err(|_| fail("Invalid HTTP proxy response."))?
                .lines()
                .next()
                .unwrap_or_default();
            let status = line.split_whitespace().nth(1).unwrap_or_default();
            if status != "200" {
                return Err(fail(if status == "407" {
                    "Proxy authentication failed."
                } else {
                    "HTTP proxy refused the CONNECT tunnel."
                }));
            }
        }
        ProxyKind::Socks5 => {
            let auth = proxy.username.is_some();
            stream
                .write_all(if auth { &[5, 1, 2] } else { &[5, 1, 0] })
                .await?;
            let mut reply = [0; 2];
            stream.read_exact(&mut reply).await?;
            if reply != [5, if auth { 2 } else { 0 }] {
                return Err(fail("SOCKS5 proxy rejected the authentication method."));
            }
            if let Some(user) = &proxy.username {
                let password = proxy
                    .password
                    .as_ref()
                    .map(|p| p.expose_for_transport())
                    .unwrap_or_default();
                if user.len() > 255 || password.len() > 255 {
                    return Err(fail("SOCKS5 credentials must be at most 255 bytes."));
                }
                let mut request = zeroize::Zeroizing::new(vec![1, user.len() as u8]);
                request.extend_from_slice(user.as_bytes());
                request.push(password.len() as u8);
                request.extend_from_slice(password);
                stream.write_all(&request).await?;
                stream.read_exact(&mut reply).await?;
                if reply != [1, 0] {
                    return Err(fail("Proxy authentication failed."));
                }
            }
            // Domain form delegates DNS to the proxy, including IP literals.
            if endpoint.host.len() > 255 {
                return Err(fail("Destination host is too long."));
            }
            let mut request = vec![5, 1, 0, 3, endpoint.host.len() as u8];
            request.extend_from_slice(endpoint.host.as_bytes());
            request.extend_from_slice(&endpoint.port.to_be_bytes());
            stream.write_all(&request).await?;
            let mut response = [0; 4];
            stream.read_exact(&mut response).await?;
            if response[0] != 5 || response[1] != 0 {
                return Err(fail("SOCKS5 proxy refused the connection."));
            }
            let bytes = match response[3] {
                1 => 4,
                4 => 16,
                3 => usize::from(stream.read_u8().await?),
                _ => return Err(fail("Invalid SOCKS5 proxy response.")),
            };
            let mut rest = vec![0; bytes + 2];
            stream.read_exact(&mut rest).await?;
        }
    }
    Ok(stream)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn proxy_refusals_never_echo_credentials() {
        for kind in [ProxyKind::Http, ProxyKind::Socks5] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind");
            let port = listener.local_addr().expect("address").port();
            let task = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.expect("accept");
                match kind {
                    ProxyKind::Http => {
                        let mut header = Vec::new();
                        while !header.ends_with(b"\r\n\r\n") {
                            header.push(socket.read_u8().await.expect("read"));
                        }
                        socket
                            .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n")
                            .await
                            .expect("response");
                    }
                    ProxyKind::Socks5 => {
                        let mut request = [0; 3];
                        socket.read_exact(&mut request).await.expect("greeting");
                        socket.write_all(&[5, 255]).await.expect("response");
                    }
                }
            });
            let options = ProxyOptions {
                endpoint: Endpoint::new("127.0.0.1", port),
                kind,
                username: Some("sentinel-user".into()),
                password: Some(SecretValue::new("sentinel-password")),
            };
            let error = transport(&Endpoint::new("destination.example", 22), Some(&options))
                .await
                .expect_err("refused");
            assert!(!error.to_string().contains("sentinel"));
            task.await.expect("server");
        }
    }
}
