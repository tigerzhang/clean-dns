use super::{Context, Plugin};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use tokio::time::{sleep, Duration};

#[derive(Deserialize)]
struct DelayConfig {
    #[serde(default)]
    ms: u64,
}

pub struct DelayPlugin {
    ms: u64,
}

impl DelayPlugin {
    pub fn new(config: Option<&serde_yaml::Value>) -> Result<Self> {
        let config: DelayConfig = if let Some(c) = config {
            serde_yaml::from_value(c.clone())?
        } else {
            DelayConfig { ms: 0 }
        };

        Ok(Self { ms: config.ms })
    }
}

#[async_trait]
impl Plugin for DelayPlugin {
    fn name(&self) -> &str {
        "delay"
    }

    async fn next(&self, _ctx: &mut Context) -> Result<()> {
        if self.ms > 0 {
            sleep(Duration::from_millis(self.ms)).await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, RwLock};
    use std::time::Instant;

    fn make_ctx() -> Context {
        use crate::statistics::Statistics;
        use hickory_proto::op::Message;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        Context::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1234),
            Message::new(),
            Arc::new(RwLock::new(Statistics::new())),
        )
    }

    #[tokio::test]
    async fn test_delay_plugin() {
        let yaml = r#"
            ms: 50
        "#;
        let config: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let plugin = DelayPlugin::new(Some(&config)).unwrap();

        let start = Instant::now();
        let mut ctx = make_ctx();
        plugin.next(&mut ctx).await.unwrap();

        // Assert at least 50ms passed (lenient check)
        assert!(start.elapsed() >= Duration::from_millis(40));
    }
}
