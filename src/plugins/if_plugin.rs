use super::{Context, Plugin, SharedPlugin};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct IfConfig {
    #[serde(rename = "if")]
    cond: String,

    #[serde(default)]
    exec: Vec<String>,

    #[serde(default)]
    else_exec: Vec<String>,
}

pub struct IfPlugin {
    cond: SharedPlugin,
    exec: Vec<SharedPlugin>,
    else_exec: Vec<SharedPlugin>,
}

impl IfPlugin {
    pub fn new(
        config: Option<&serde_yaml::Value>,
        registry: &HashMap<String, SharedPlugin>,
    ) -> Result<Self> {
        let config: IfConfig = if let Some(c) = config {
            serde_yaml::from_value(c.clone())?
        } else {
            return Err(anyhow::anyhow!("IfPlugin requires config"));
        };

        let cond_tag = if config.cond.starts_with("provider:") {
            &config.cond["provider:".len()..]
        } else {
            &config.cond
        };

        let cond = registry
            .get(cond_tag)
            .ok_or_else(|| anyhow::anyhow!("Condition plugin not found: {}", cond_tag))?;
        if cond.as_condition().is_none() {
            return Err(anyhow::anyhow!("Plugin {} is not a Condition", cond_tag));
        }

        let mut exec = Vec::new();
        for tag in config.exec {
            let p = registry
                .get(&tag)
                .ok_or_else(|| anyhow::anyhow!("Plugin not found: {}", tag))?;
            exec.push(p.clone());
        }

        let mut else_exec = Vec::new();
        for tag in config.else_exec {
            let p = registry
                .get(&tag)
                .ok_or_else(|| anyhow::anyhow!("Plugin not found: {}", tag))?;
            else_exec.push(p.clone());
        }

        Ok(Self {
            cond: cond.clone(),
            exec,
            else_exec,
        })
    }
}

#[async_trait]
impl Plugin for IfPlugin {
    fn name(&self) -> &str {
        "if"
    }

    async fn next(&self, ctx: &mut Context) -> Result<()> {
        let condition_met = if let Some(c) = self.cond.as_condition() {
            c.check(ctx)
        } else {
            false
        };

        let plugins = if condition_met {
            &self.exec
        } else {
            &self.else_exec
        };

        for plugin in plugins {
            plugin.next(ctx).await?;
            if ctx.abort {
                break;
            }
        }

        Ok(())
    }
}
