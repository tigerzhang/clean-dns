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

#[cfg(test)]
mod tests {
    use super::super::Condition;
    use super::*; // Condition is in parent module (plugins/mod.rs) re-exported or accessible via super?
                  // In src/plugins/if_plugin.rs, super refers to src/plugins/mod.rs
                  // Check line 1: use super::{Context, Plugin, SharedPlugin};
                  // So usually `use super::Condition` works if it's there.

    use std::sync::{Arc, Mutex};

    struct MockCondition {
        response: bool,
    }

    impl Condition for MockCondition {
        fn check(&self, _ctx: &Context) -> bool {
            self.response
        }
    }

    #[async_trait]
    impl Plugin for MockCondition {
        fn name(&self) -> &str {
            "mock_cond"
        }
        async fn next(&self, _ctx: &mut Context) -> Result<()> {
            Ok(())
        }
        fn as_condition(&self) -> Option<&dyn Condition> {
            Some(self)
        }
    }

    struct MockExec {
        executed: Arc<Mutex<bool>>,
    }

    #[async_trait]
    impl Plugin for MockExec {
        fn name(&self) -> &str {
            "mock_exec"
        }
        async fn next(&self, _ctx: &mut Context) -> Result<()> {
            *self.executed.lock().unwrap() = true;
            Ok(())
        }
    }

    fn make_ctx() -> Context {
        use crate::statistics::Statistics;
        use hickory_proto::op::Message;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        use std::sync::{Arc, RwLock};

        Context::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1234),
            Message::new(),
            Arc::new(RwLock::new(Statistics::new())),
        )
    }

    #[tokio::test]
    async fn test_if_plugin_true() {
        let cond: SharedPlugin = Arc::new(MockCondition { response: true });
        let exec_flag = Arc::new(Mutex::new(false));
        let exec: SharedPlugin = Arc::new(MockExec {
            executed: exec_flag.clone(),
        });

        let mut registry = HashMap::new();
        registry.insert("cond".to_string(), cond);
        registry.insert("exec".to_string(), exec);

        let yaml = r#"
            if: cond
            exec:
              - exec
        "#;
        let config: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let plugin = IfPlugin::new(Some(&config), &registry).unwrap();

        let mut ctx = make_ctx();
        plugin.next(&mut ctx).await.unwrap();

        assert!(*exec_flag.lock().unwrap());
    }

    #[tokio::test]
    async fn test_if_plugin_false() {
        let cond: SharedPlugin = Arc::new(MockCondition { response: false });
        let exec_flag = Arc::new(Mutex::new(false));
        let exec: SharedPlugin = Arc::new(MockExec {
            executed: exec_flag.clone(),
        });

        // Else branch
        let else_flag = Arc::new(Mutex::new(false));
        let else_exec: SharedPlugin = Arc::new(MockExec {
            executed: else_flag.clone(),
        });

        let mut registry = HashMap::new();
        registry.insert("cond".to_string(), cond);
        registry.insert("exec".to_string(), exec);
        registry.insert("else_exec".to_string(), else_exec);

        let yaml = r#"
            if: cond
            exec:
              - exec
            else_exec:
              - else_exec
        "#;
        let config: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let plugin = IfPlugin::new(Some(&config), &registry).unwrap();

        let mut ctx = make_ctx();
        plugin.next(&mut ctx).await.unwrap();

        assert!(!*exec_flag.lock().unwrap());
        assert!(*else_flag.lock().unwrap());
    }
}
