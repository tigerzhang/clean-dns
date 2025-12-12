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
