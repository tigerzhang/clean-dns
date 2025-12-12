use super::{Context, Plugin, SharedPlugin};
use anyhow::{Context as AnyhowContext, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use tracing::debug;

#[derive(Deserialize)]
struct SequenceConfig {
    exec: Vec<String>,
}

pub struct Sequence {
    plugins: Vec<SharedPlugin>,
}

impl Sequence {
    pub fn new(
        config: Option<&serde_yaml::Value>,
        registry: &HashMap<String, SharedPlugin>,
    ) -> Result<Self> {
        let config: SequenceConfig = if let Some(c) = config {
            serde_yaml::from_value(c.clone())?
        } else {
            SequenceConfig { exec: vec![] }
        };

        let mut plugins = Vec::new();
        for tag in config.exec {
            let p = registry
                .get(&tag)
                .ok_or_else(|| anyhow::anyhow!("Plugin not found: {}", tag))?;
            plugins.push(p.clone());
        }

        Ok(Self { plugins })
    }
}

#[async_trait]
impl Plugin for Sequence {
    fn name(&self) -> &str {
        "sequence"
    }

    async fn next(&self, ctx: &mut Context) -> Result<()> {
        for plugin in &self.plugins {
            if ctx.abort {
                debug!("Sequence aborted");
                break;
            }
            plugin
                .next(ctx)
                .await
                .with_context(|| format!("Plugin {} failed", plugin.name()))?;
        }
        Ok(())
    }
}
