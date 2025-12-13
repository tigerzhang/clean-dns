use super::{Context, Plugin, SharedPlugin};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use tracing::warn;

#[derive(Deserialize)]
struct FallbackConfig {
    primary: String,
    secondary: String,
}

pub struct FallbackPlugin {
    primary: SharedPlugin,
    secondary: SharedPlugin,
}

impl FallbackPlugin {
    pub fn new(
        config: Option<&serde_yaml::Value>,
        registry: &HashMap<String, SharedPlugin>,
    ) -> Result<Self> {
        let config: FallbackConfig = if let Some(c) = config {
            serde_yaml::from_value(c.clone())?
        } else {
            return Err(anyhow::anyhow!("FallbackPlugin requires config"));
        };

        let primary = registry
            .get(&config.primary)
            .ok_or_else(|| anyhow::anyhow!("Primary plugin not found: {}", config.primary))?
            .clone();

        let secondary = registry
            .get(&config.secondary)
            .ok_or_else(|| anyhow::anyhow!("Secondary plugin not found: {}", config.secondary))?
            .clone();

        Ok(Self { primary, secondary })
    }
}

#[async_trait]
impl Plugin for FallbackPlugin {
    fn name(&self) -> &str {
        "fallback"
    }

    async fn next(&self, ctx: &mut Context) -> Result<()> {
        if let Err(e) = self.primary.next(ctx).await {
            warn!(
                "Primary plugin {} failed: {}. Switching to secondary.",
                self.primary.name(),
                e
            );
            self.secondary.next(ctx).await
        } else {
            Ok(())
        }
    }
}