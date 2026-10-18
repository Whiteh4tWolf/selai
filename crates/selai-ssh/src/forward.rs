//! Bounded TCP forwarding. Every child connection is owned by the tunnel task.
use crate::{Session, SshError};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct ForwardCounters {
    pub connections: AtomicU64,
    pub sent: AtomicU64,
    pub received: AtomicU64,
    pub errors: AtomicU64,
}
pub struct ForwardSpec {
    pub mode: String,
    pub bind: String,
    pub port: u16,
    pub target: String,
    pub target_port: u16,
}
pub struct Forwarder {
    session: Arc<Session>,
    spec: ForwardSpec,
    listener: Option<TcpListener>,
    remote: Option<tokio::sync::mpsc::Receiver<russh::Channel<russh::client::Msg>>>,
}

impl Forwarder {
    pub async fn start(session: Arc<Session>, mut spec: ForwardSpec) -> Result<Self, SshError> {
        let (listener, remote) = if spec.mode == "remote" {
            let (port, rx) = session.remote_forward(&spec.bind, spec.port).await?;
            spec.port = port;
            (None, Some(rx))
        } else {
            let listener = TcpListener::bind((spec.bind.as_str(), spec.port)).await?;
            spec.port = listener.local_addr()?.port();
            (Some(listener), None)
        };
        Ok(Self {
            session,
            spec,
            listener,
            remote,
        })
    }
    pub fn port(&self) -> u16 {
        self.spec.port
    }
    pub async fn run(
        mut self,
        stop: CancellationToken,
        counts: Arc<ForwardCounters>,
    ) -> Result<(), SshError> {
        let mut children = JoinSet::new();
        let mut health = tokio::time::interval(Duration::from_secs(1));
        let result = loop {
            tokio::select! {
                biased;
                _=stop.cancelled()=>break Ok(()),
                _=health.tick()=>if self.session.is_closed(){break Err(SshError::ChannelClosed)},
                Some(_)=children.join_next(), if !children.is_empty()=>{},
                accepted=async {match self.listener.as_ref(){Some(listener)=>listener.accept().await,None=>std::future::pending().await}}, if self.listener.is_some()=>{
                    let (mut socket,_) = match accepted {Ok(pair)=>pair,Err(e)=>break Err(e.into())};
                    if children.len()>=64 {drop(socket);counts.errors.fetch_add(1,Ordering::Relaxed);continue;}
                    let session=self.session.clone();let stats=counts.clone();let target=self.spec.target.clone();let port=self.spec.target_port;let dynamic=self.spec.mode=="dynamic";
                    children.spawn(async move {
                        stats.connections.fetch_add(1,Ordering::Relaxed);
                        let result=async {
                            let (host,port)=if dynamic {tokio::time::timeout(Duration::from_secs(15),socks_request(&mut socket)).await.map_err(|_|SshError::ConnectTimeout)??} else {(target,port)};
                            let channel=tokio::time::timeout(Duration::from_secs(15),session.direct_tcpip(&host,port)).await.map_err(|_|SshError::ConnectTimeout)?;
                            match channel {
                                Ok(channel)=>{if dynamic {socket.write_all(&[5,0,0,1,0,0,0,0,0,0]).await?;} bridge(socket,channel,&stats).await},
                                Err(e)=>{if dynamic{let _=socket.write_all(&[5,5,0,1,0,0,0,0,0,0]).await;}Err(e)}
                            }
                        }.await;
                        if result.is_err(){stats.errors.fetch_add(1,Ordering::Relaxed);}stats.connections.fetch_sub(1,Ordering::Relaxed);
                    });
                },
                incoming=async {match self.remote.as_mut(){Some(remote)=>remote.recv().await,None=>std::future::pending().await}}, if self.remote.is_some()=>{
                    let Some(channel)=incoming else {break Err(SshError::ChannelClosed)};
                    if children.len()>=64 {drop(channel);counts.errors.fetch_add(1,Ordering::Relaxed);continue;}
                    let stats=counts.clone();let target=self.spec.target.clone();let port=self.spec.target_port;
                    children.spawn(async move {
                        stats.connections.fetch_add(1,Ordering::Relaxed);
                        let result=async {
                            let socket=tokio::time::timeout(Duration::from_secs(15),TcpStream::connect((target.as_str(),port))).await.map_err(|_|SshError::ConnectTimeout)??;
                            bridge(channel.into_stream(),socket,&stats).await
                        }.await;
                        if result.is_err(){stats.errors.fetch_add(1,Ordering::Relaxed);}stats.connections.fetch_sub(1,Ordering::Relaxed);
                    });
                }
            }
        };
        children.abort_all();
        while children.join_next().await.is_some() {}
        counts.connections.store(0, Ordering::Relaxed);
        if self.spec.mode == "remote" {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                self.session
                    .cancel_remote_forward(&self.spec.bind, self.spec.port),
            )
            .await;
        }
        let _ = tokio::time::timeout(Duration::from_secs(2), self.session.close()).await;
        result
    }
}

async fn pump<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    mut reader: R,
    mut writer: W,
    count: &AtomicU64,
) -> Result<(), SshError> {
    let mut buf = vec![0; 32768];
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            writer.shutdown().await?;
            return Ok(());
        }
        writer.write_all(&buf[..n]).await?;
        count.fetch_add(n as u64, Ordering::Relaxed);
    }
}
async fn bridge<A: AsyncRead + AsyncWrite + Unpin, B: AsyncRead + AsyncWrite + Unpin>(
    a: A,
    b: B,
    stats: &ForwardCounters,
) -> Result<(), SshError> {
    let (ar, aw) = tokio::io::split(a);
    let (br, bw) = tokio::io::split(b);
    tokio::try_join!(pump(ar, bw, &stats.sent), pump(br, aw, &stats.received))?;
    Ok(())
}

async fn socks_request(socket: &mut TcpStream) -> Result<(String, u16), SshError> {
    if socket.read_u8().await? != 5 {
        return Err(SshError::Protocol("SOCKS5 required".into()));
    }
    let len = socket.read_u8().await? as usize;
    let mut methods = vec![0; len];
    socket.read_exact(&mut methods).await?;
    if !methods.contains(&0) {
        socket.write_all(&[5, 255]).await?;
        return Err(SshError::Protocol("No supported SOCKS5 method".into()));
    }
    socket.write_all(&[5, 0]).await?;
    let mut header = [0; 4];
    socket.read_exact(&mut header).await?;
    if header[0] != 5 || header[1] != 1 || header[2] != 0 {
        socket.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
        return Err(SshError::Protocol("SOCKS5 TCP CONNECT required".into()));
    }
    let host = match header[3] {
        1 => {
            let mut ip = [0; 4];
            socket.read_exact(&mut ip).await?;
            std::net::Ipv4Addr::from(ip).to_string()
        }
        4 => {
            let mut ip = [0; 16];
            socket.read_exact(&mut ip).await?;
            std::net::Ipv6Addr::from(ip).to_string()
        }
        3 => {
            let len = socket.read_u8().await?;
            let mut name = vec![0; len as usize];
            socket.read_exact(&mut name).await?;
            String::from_utf8(name)
                .map_err(|_| SshError::Protocol("Invalid SOCKS5 destination".into()))?
        }
        _ => return Err(SshError::Protocol("Invalid SOCKS5 address".into())),
    };
    let port = socket.read_u16().await?;
    if port == 0 || host.is_empty() {
        return Err(SshError::Protocol("Invalid SOCKS5 destination".into()));
    }
    Ok((host, port))
}
