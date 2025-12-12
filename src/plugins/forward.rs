use super::{Context, Plugin};
use anyhow::{Context as AnyhowContext, Result};
use async_trait::async_trait;
use futures::future::{select_ok, BoxFuture};
use hickory_proto::op::Message;
use rand::seq::SliceRandom;
use reqwest::{Client, Url};
use serde::Deserialize;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UdpSocket;
use tokio_socks::tcp::Socks5Stream;
use tracing::{debug, info, warn};

#[derive(Deserialize)]
struct ForwardConfig {
    #[serde(default)]
    addr: Option<String>,
    #[serde(default)]
    upstreams: Option<Vec<String>>,
    #[serde(default = "default_concurrent")]
    concurrent: u32,
    #[serde(default)]
    socks5: Option<String>,
}

fn default_concurrent() -> u32 {
    1
}

#[derive(Clone, Debug)]
enum Upstream {
    Udp(SocketAddr),
    DoH(Url),
}

pub struct Forward {
    upstreams: Vec<Upstream>,
    concurrent: u32,
    socks5: Option<SocketAddr>,
    client: Client, // Shared HTTP client for DoH
}

impl Forward {
    pub fn new(config: Option<&serde_yaml::Value>) -> Result<Self> {
        let config: ForwardConfig = if let Some(c) = config {
            serde_yaml::from_value(c.clone())?
        } else {
            return Err(anyhow::anyhow!("Forward plugin requires config"));
        };

        let mut upstreams = Vec::new();

        if let Some(addr) = config.addr {
            upstreams.push(Self::parse_upstream(&addr)?);
        }

        if let Some(list) = config.upstreams {
            for u in list {
                upstreams.push(Self::parse_upstream(&u)?);
            }
        }

        if upstreams.is_empty() {
            return Err(anyhow::anyhow!(
                "Forward plugin requires at least one upstream"
            ));
        }

        // Build REQWEST client
        let mut builder = Client::builder().timeout(Duration::from_secs(5));

        // SOCKS5 for DoH?
        // reqwest supports proxy.
        // If socks5 is configured, we apply it to the reqwest client.
        // Note: This applies to ALL DoH requests from this plugin instance.
        let socks5_addr = if let Some(s) = config.socks5 {
            let addr = s.parse::<SocketAddr>().context("Invalid SOCKS5 address")?;
            let proxy_url = format!("socks5://{}", s);
            let proxy = reqwest::Proxy::all(&proxy_url).context("Invalid SOCKS5 proxy URL")?;
            builder = builder.proxy(proxy);
            Some(addr)
        } else {
            None
        };

        let client = builder.build().context("Failed to build HTTP client")?;

        Ok(Self {
            upstreams,
            concurrent: config.concurrent.max(1),
            socks5: socks5_addr,
            client,
        })
    }

    fn parse_upstream(s: &str) -> Result<Upstream> {
        if s.starts_with("https://") {
            let url = Url::parse(s).context("Invalid DoH URL")?;
            Ok(Upstream::DoH(url))
        } else {
            let addr = s.parse().context("Invalid UDP upstream address")?;
            Ok(Upstream::Udp(addr))
        }
    }

    async fn exchange(&self, upstream: Upstream, request_bytes: Vec<u8>) -> Result<Vec<u8>> {
        match upstream {
            Upstream::Udp(addr) => self.exchange_udp(addr, request_bytes).await,
            Upstream::DoH(url) => self.exchange_doh(url, request_bytes).await,
        }
    }

    async fn exchange_doh(&self, url: Url, request_bytes: Vec<u8>) -> Result<Vec<u8>> {
        // Send POST request
        let response = self
            .client
            .post(url)
            .header("content-type", "application/dns-message")
            .header("accept", "application/dns-message")
            .body(request_bytes)
            .send()
            .await
            .context("DoH request failed")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "DoH server returned status: {}",
                response.status()
            ));
        }

        let bytes = response.bytes().await.context("DoH read body failed")?;
        Ok(bytes.to_vec())
    }

    async fn exchange_udp(&self, upstream: SocketAddr, request_bytes: Vec<u8>) -> Result<Vec<u8>> {
        let result = tokio::time::timeout(Duration::from_secs(5), async {
            if let Some(proxy_addr) = self.socks5 {
                // TCP via SOCKS5
                let mut stream = Socks5Stream::connect(proxy_addr, upstream)
                    .await
                    .context("SOCKS5 connect failed")?;

                let len = (request_bytes.len() as u16).to_be_bytes();
                stream
                    .write_all(&len)
                    .await
                    .context("SOCKS5 write len failed")?;
                stream
                    .write_all(&request_bytes)
                    .await
                    .context("SOCKS5 write body failed")?;

                let mut len_buf = [0u8; 2];
                stream
                    .read_exact(&mut len_buf)
                    .await
                    .context("SOCKS5 read len failed")?;
                let len = u16::from_be_bytes(len_buf) as usize;

                let mut buf = vec![0u8; len];
                stream
                    .read_exact(&mut buf)
                    .await
                    .context("SOCKS5 read body failed")?;
                Ok::<Vec<u8>, anyhow::Error>(buf)
            } else {
                // UDP direct
                let socket = UdpSocket::bind("0.0.0.0:0")
                    .await
                    .context("UDP bind failed")?;
                socket
                    .connect(upstream)
                    .await
                    .context("UDP connect failed")?;
                socket
                    .send(&request_bytes)
                    .await
                    .context("UDP send failed")?;
                let mut buf = [0u8; 4096];
                let (len, _) = socket
                    .recv_from(&mut buf)
                    .await
                    .context("UDP recv failed")?;
                Ok::<Vec<u8>, anyhow::Error>(buf[..len].to_vec())
            }
        })
        .await
        .context("UDP/SOCKS5 exchange timeout")??;

        Ok(result)
    }
}

#[async_trait]
impl Plugin for Forward {
    fn name(&self) -> &str {
        "forward"
    }

    async fn next(&self, ctx: &mut Context) -> Result<()> {
        if ctx.response.is_some() {
            return Ok(());
        }

        let request_bytes = ctx.request.to_vec()?;

        let mut selected_upstreams = self.upstreams.clone();
        if self.concurrent > 1 && self.upstreams.len() > 1 {
            let mut rng = rand::thread_rng();
            selected_upstreams.shuffle(&mut rng);
            selected_upstreams.truncate(self.concurrent as usize);
        } else if self.upstreams.len() > 1 {
            let mut rng = rand::thread_rng();
            if let Some(picked) = selected_upstreams.choose(&mut rng) {
                selected_upstreams = vec![picked.clone()];
            }
        }

        debug!("Forwarding query to {:?}", selected_upstreams);

        let mut futures: Vec<BoxFuture<Result<Vec<u8>>>> = Vec::new();

        for upstream in selected_upstreams {
            let req_clone = request_bytes.clone();
            let f = Box::pin(self.exchange(upstream, req_clone));
            futures.push(f);
        }

        match select_ok(futures).await {
            Ok((response_bytes, _)) => {
                let response = Message::from_vec(&response_bytes)?;
                ctx.response = Some(response);
                info!("Forwarded request success");
            }
            Err(e) => {
                warn!("All upstreams failed: {}", e);
                return Err(e);
            }
        }

        Ok(())
    }
}
