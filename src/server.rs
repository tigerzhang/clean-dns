use crate::plugins::{Context, SharedPlugin};
use anyhow::Result;
use hickory_proto::op::Message;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tracing::{error, info};

pub struct Server {
    addr: SocketAddr,
    entry_plugin: SharedPlugin,
}

impl Server {
    pub fn new(addr: SocketAddr, entry_plugin: SharedPlugin) -> Self {
        Self { addr, entry_plugin }
    }

    pub async fn run(self) -> Result<()> {
        let socket = Arc::new(UdpSocket::bind(self.addr).await?);
        info!("Listening on {}", self.addr);

        loop {
            let mut buf = [0u8; 512];
            match socket.recv_from(&mut buf).await {
                Ok((size, src)) => {
                    let socket_clone = socket.clone();
                    let plugin = self.entry_plugin.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_request(socket_clone, &buf[..size], src, plugin).await {
                            error!("Failed to handle request: {}", e);
                        }
                    });
                }
                Err(e) => error!("Failed to receive UDP packet: {}", e),
            }
        }
    }

    async fn handle_request(
        socket: Arc<UdpSocket>,
        buf: &[u8],
        src: SocketAddr,
        plugin: SharedPlugin,
    ) -> Result<()> {
        let request = Message::from_vec(buf)?;
        let mut ctx = Context::new(src, request);

        plugin.next(&mut ctx).await?;

        if let Some(response) = ctx.response {
            let bytes = response.to_vec()?;
            socket.send_to(&bytes, src).await?;
        }

        Ok(())
    }
}
