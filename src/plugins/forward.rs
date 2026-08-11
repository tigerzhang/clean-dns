use super::{Context, Plugin};
use anyhow::{Context as AnyhowContext, Result};
use async_trait::async_trait;
use futures::future::{select_ok, BoxFuture};
use hickory_proto::op::Message;
use rand::seq::SliceRandom;
use reqwest::{Client, Url};
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    RwLock,
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UdpSocket;
use tokio_socks::tcp::Socks5Stream;
use tracing::{debug, warn};

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
    /// Extra attempts after the first failure (same upstream). Default 0.
    #[serde(default)]
    retries: u32,
    /// Delay between retries in milliseconds. Default 100.
    #[serde(default = "default_retry_interval_ms")]
    retry_interval_ms: u64,
    /// Rebuild the DoH client after this many consecutive failed requests.
    /// Default 2. Set to 0 to disable this recovery.
    #[serde(default = "default_reset_after_failures")]
    reset_after_failures: u32,
}

fn default_concurrent() -> u32 {
    1
}

fn default_retry_interval_ms() -> u64 {
    100
}

fn default_reset_after_failures() -> u32 {
    2
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
    /// Original socks5 host:port string (for rebuilding the DoH client proxy).
    socks5_str: Option<String>,
    /// Shared HTTP client for DoH. Replaced on DoH failure so idle pooled
    /// connections (and the underlying SOCKS tunnels) are dropped.
    client: RwLock<Client>,
    retries: u32,
    retry_interval: Duration,
    reset_after_failures: u32,
    consecutive_doh_failures: AtomicU32,
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

        // SOCKS5 applies to DoH via the reqwest client and to UDP via tokio-socks.
        let (socks5_addr, socks5_str) = if let Some(s) = config.socks5 {
            let addr = s.parse::<SocketAddr>().context("Invalid SOCKS5 address")?;
            (Some(addr), Some(s))
        } else {
            (None, None)
        };

        let client = Self::build_client(socks5_str.as_deref())?;

        Ok(Self {
            upstreams,
            concurrent: config.concurrent.max(1),
            socks5: socks5_addr,
            socks5_str,
            client: RwLock::new(client),
            retries: config.retries,
            retry_interval: Duration::from_millis(config.retry_interval_ms),
            reset_after_failures: config.reset_after_failures,
            consecutive_doh_failures: AtomicU32::new(0),
        })
    }

    /// Build a fresh reqwest Client. A new Client starts with an empty connection
    /// pool, so any prior SOCKS/TLS sockets held by the previous client are
    /// released once outstanding clones finish.
    fn build_client(socks5: Option<&str>) -> Result<Client> {
        let mut builder = Client::builder().timeout(Duration::from_secs(5));

        if let Some(s) = socks5 {
            // Note: applies to ALL DoH requests from this plugin instance.
            let proxy_url = format!("socks5://{}", s);
            let proxy = reqwest::Proxy::all(&proxy_url).context("Invalid SOCKS5 proxy URL")?;
            builder = builder.proxy(proxy);
        }

        builder.build().context("Failed to build HTTP client")
    }

    /// Replace the shared DoH client so the next request opens new connections
    /// instead of reusing a stale/broken pool entry through the proxy.
    fn rebuild_client(&self) -> Result<()> {
        let client = Self::build_client(self.socks5_str.as_deref())?;
        match self.client.write() {
            Ok(mut guard) => {
                *guard = client;
                debug!("Rebuilt DoH HTTP client (connection pool dropped)");
                Ok(())
            }
            Err(poisoned) => {
                // Prefer recovery over panicking: install the new client anyway.
                *poisoned.into_inner() = client;
                warn!("DoH client lock was poisoned; installed a fresh client");
                Ok(())
            }
        }
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

    /// Try the same upstream up to `1 + retries` times with a short delay between attempts.
    /// On DoH failure, rebuilds the HTTP client so retries do not reuse a dead pool/proxy socket.
    async fn exchange_with_retry(
        &self,
        upstream: Upstream,
        request_bytes: Vec<u8>,
    ) -> Result<Vec<u8>> {
        let attempts = self.retries.saturating_add(1);
        let mut last_err: Option<anyhow::Error> = None;

        for attempt in 1..=attempts {
            match self.exchange(upstream.clone(), request_bytes.clone()).await {
                Ok(bytes) => {
                    if matches!(upstream, Upstream::DoH(_)) {
                        self.consecutive_doh_failures.store(0, Ordering::Relaxed);
                    }
                    if attempt > 1 {
                        debug!(
                            "Upstream {:?} succeeded on attempt {}/{}",
                            upstream, attempt, attempts
                        );
                    }
                    return Ok(bytes);
                }
                Err(e) => {
                    if attempt < attempts {
                        // Drop pooled DoH/SOCKS connections before the next try.
                        if matches!(upstream, Upstream::DoH(_)) {
                            if let Err(rebuild_err) = self.rebuild_client() {
                                warn!(
                                    "Failed to rebuild DoH client after error: {:#}",
                                    rebuild_err
                                );
                            }
                        }
                        warn!(
                            "Upstream {:?} attempt {}/{} failed: {:#}. Retrying in {:?}",
                            upstream, attempt, attempts, e, self.retry_interval
                        );
                        if !self.retry_interval.is_zero() {
                            tokio::time::sleep(self.retry_interval).await;
                        }
                    }
                    last_err = Some(e);
                }
            }
        }

        if matches!(upstream, Upstream::DoH(_)) && self.reset_after_failures > 0 {
            let failures = self
                .consecutive_doh_failures
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1);
            if failures >= self.reset_after_failures {
                warn!(
                    "DoH failed {} consecutive times; rebuilding the DoH client",
                    failures
                );
                if let Err(rebuild_err) = self.rebuild_client() {
                    warn!(
                        "Failed to rebuild DoH client after consecutive failures: {:#}",
                        rebuild_err
                    );
                }
                self.consecutive_doh_failures.store(0, Ordering::Relaxed);
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("All retry attempts failed")))
    }

    async fn exchange_doh(&self, url: Url, request_bytes: Vec<u8>) -> Result<Vec<u8>> {
        // Clone is cheap (Client is Arc-backed) and lets us release the lock
        // before awaiting network I/O.
        let client = self
            .client
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();

        let response = client
            .post(url.clone())
            .header("content-type", "application/dns-message")
            .header("accept", "application/dns-message")
            .body(request_bytes)
            .send()
            .await
            .with_context(|| format!("DoH request failed ({url})"))?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "DoH server {} returned status: {}",
                url,
                response.status()
            ));
        }

        let bytes = response
            .bytes()
            .await
            .with_context(|| format!("DoH read body failed ({url})"))?;
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

        debug!(
            "Forwarding query to {:?} (retries={})",
            selected_upstreams, self.retries
        );

        let mut futures: Vec<BoxFuture<Result<Vec<u8>>>> = Vec::new();

        for upstream in selected_upstreams {
            let req_clone = request_bytes.clone();
            let f = Box::pin(self.exchange_with_retry(upstream, req_clone));
            futures.push(f);
        }

        match select_ok(futures).await {
            Ok((response_bytes, _)) => {
                let response = Message::from_vec(&response_bytes)?;
                ctx.response = Some(response);
                ctx.is_remote = self.socks5.is_some();
                debug!("Forwarded request success");
            }
            Err(e) => {
                // {:#} prints the full anyhow context chain (e.g. proxy refused, timeout).
                warn!("All upstreams failed: {:#}", e);
                return Err(e);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_upstream() {
        let u = Forward::parse_upstream("8.8.8.8:53").unwrap();
        if let Upstream::Udp(addr) = u {
            assert_eq!(addr.to_string(), "8.8.8.8:53");
        } else {
            panic!("Expected UDP");
        }

        let u = Forward::parse_upstream("https://dns.google/dns-query").unwrap();
        if let Upstream::DoH(url) = u {
            assert_eq!(url.as_str(), "https://dns.google/dns-query");
        } else {
            panic!("Expected DoH");
        }
    }

    #[test]
    fn test_config_parsing() {
        let yaml = r#"
            addr: "1.1.1.1:53"
            upstreams:
              - "8.8.8.8:53"
              - "https://1.1.1.1/dns-query"
        "#;
        let config: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let forward = Forward::new(Some(&config)).unwrap();
        assert_eq!(forward.upstreams.len(), 3); // 1 from addr, 2 from upstreams
        assert_eq!(forward.retries, 0);
        assert_eq!(forward.retry_interval, Duration::from_millis(100));
    }

    #[test]
    fn test_retry_config_parsing() {
        let yaml = r#"
            upstreams:
              - "8.8.8.8:53"
            retries: 2
            retry_interval_ms: 250
        "#;
        let config: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let forward = Forward::new(Some(&config)).unwrap();
        assert_eq!(forward.retries, 2);
        assert_eq!(forward.retry_interval, Duration::from_millis(250));
    }

    #[test]
    fn test_rebuild_client_preserves_socks5() {
        let yaml = r#"
            upstreams:
              - "https://8.8.8.8/dns-query"
            socks5: "127.0.0.1:1080"
            retries: 1
        "#;
        let config: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let forward = Forward::new(Some(&config)).unwrap();
        assert_eq!(forward.socks5_str.as_deref(), Some("127.0.0.1:1080"));
        assert!(forward.socks5.is_some());

        // Replace client; must succeed and leave socks5 config intact.
        forward.rebuild_client().unwrap();
        assert_eq!(forward.socks5_str.as_deref(), Some("127.0.0.1:1080"));
        // Client is still usable after rebuild.
        let _ = forward.client.read().unwrap().clone();
    }

    #[test]
    fn test_rebuild_client_without_proxy() {
        let yaml = r#"
            upstreams:
              - "https://dns.google/dns-query"
        "#;
        let config: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let forward = Forward::new(Some(&config)).unwrap();
        assert!(forward.socks5_str.is_none());
        forward.rebuild_client().unwrap();
    }
}
