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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // Mock Plugin
    struct MockPlugin {
        name: String,
        call_count: Arc<Mutex<usize>>,
    }

    #[async_trait]
    impl Plugin for MockPlugin {
        fn name(&self) -> &str {
            &self.name
        }
        async fn next(&self, _ctx: &mut Context) -> Result<()> {
            let mut c = self.call_count.lock().unwrap();
            *c += 1;
            Ok(())
        }
    }

    fn make_mock(name: &str) -> (SharedPlugin, Arc<Mutex<usize>>) {
        let counter = Arc::new(Mutex::new(0));
        let p = MockPlugin {
            name: name.to_string(),
            call_count: counter.clone(),
        };
        (Arc::new(p), counter)
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
    async fn test_sequence_execution() {
        let (p1, c1) = make_mock("p1");
        let (p2, c2) = make_mock("p2");

        let mut registry = HashMap::new();
        registry.insert("p1".to_string(), p1);
        registry.insert("p2".to_string(), p2);

        let yaml = r#"
            exec:
              - p1
              - p2
        "#;
        let config: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();

        let sequence = Sequence::new(Some(&config), &registry).unwrap();

        let mut ctx = make_ctx();
        sequence.next(&mut ctx).await.unwrap();

        assert_eq!(*c1.lock().unwrap(), 1);
        assert_eq!(*c2.lock().unwrap(), 1);
    }
}
