use crate::plugins::{Context, SharedPlugin};
use anyhow::Result;
use hickory_proto::op::Message;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tracing::{error, info};

use crate::statistics::Statistics;
use std::sync::RwLock;

pub struct Server {
    addr: SocketAddr,
    entry_plugin: SharedPlugin,
    statistics: Arc<RwLock<Statistics>>,
}

impl Server {
    pub fn new(
        addr: SocketAddr,
        entry_plugin: SharedPlugin,
        statistics: Arc<RwLock<Statistics>>,
    ) -> Self {
        Self {
            addr,
            entry_plugin,
            statistics,
        }
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
                    let stats = self.statistics.clone();
                    tokio::spawn(async move {
                        if let Err(e) =
                            Self::handle_request(socket_clone, &buf[..size], src, plugin, stats)
                                .await
                        {
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
        stats: Arc<RwLock<Statistics>>,
    ) -> Result<()> {
        let request = Message::from_vec(buf)?;

        // Record request and keep domain for later
        let domain = if let Some(query) = request.query() {
            let d = query.name().to_string();
            {
                let mut s = stats.write().unwrap();
                s.record_request(d.clone());
            }
            Some(d)
        } else {
            None
        };

        let mut ctx = Context::new(src, request, stats.clone());

        plugin.next(&mut ctx).await?;

        if let Some(response) = ctx.response {
            // Record resolved IPs
            if let Some(d) = &domain {
                for answer in response.answers() {
                    if let Some(rdata) = answer.data() {
                        match rdata {
                            hickory_proto::rr::RData::A(ipv4) => {
                                let mut s = stats.write().unwrap();
                                s.record_resolved_ip(
                                    d,
                                    std::net::IpAddr::V4(ipv4.0),
                                    ctx.is_remote,
                                );
                            }
                            hickory_proto::rr::RData::AAAA(ipv6) => {
                                let mut s = stats.write().unwrap();
                                s.record_resolved_ip(
                                    d,
                                    std::net::IpAddr::V6(ipv6.0),
                                    ctx.is_remote,
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }

            let bytes = response.to_vec()?;
            socket.send_to(&bytes, src).await?;
        }

        Ok(())
    }
}
