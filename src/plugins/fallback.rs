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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct MockPlugin {
        fail: bool,
        called: Arc<Mutex<bool>>,
    }

    #[async_trait]
    impl Plugin for MockPlugin {
        fn name(&self) -> &str {
            "mock"
        }
        async fn next(&self, _ctx: &mut Context) -> Result<()> {
            *self.called.lock().unwrap() = true;
            if self.fail {
                Err(anyhow::anyhow!("Mock fail"))
            } else {
                Ok(())
            }
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
    async fn test_fallback_success() {
        let p1_called = Arc::new(Mutex::new(false));
        let p1 = Arc::new(MockPlugin {
            fail: false,
            called: p1_called.clone(),
        });
        let p2_called = Arc::new(Mutex::new(false));
        let p2 = Arc::new(MockPlugin {
            fail: false,
            called: p2_called.clone(),
        });

        let mut registry: HashMap<String, SharedPlugin> = HashMap::new();
        registry.insert("p1".to_string(), p1);
        registry.insert("p2".to_string(), p2);

        let yaml = r#"
            primary: p1
            secondary: p2
        "#;
        let config: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let plugin = FallbackPlugin::new(Some(&config), &registry).unwrap();

        let mut ctx = make_ctx();
        plugin.next(&mut ctx).await.unwrap();

        assert!(*p1_called.lock().unwrap());
        assert!(!*p2_called.lock().unwrap());
    }

    #[tokio::test]
    async fn test_fallback_failure() {
        let p1_called = Arc::new(Mutex::new(false));
        let p1 = Arc::new(MockPlugin {
            fail: true,
            called: p1_called.clone(),
        });
        let p2_called = Arc::new(Mutex::new(false));
        let p2 = Arc::new(MockPlugin {
            fail: false,
            called: p2_called.clone(),
        });

        let mut registry: HashMap<String, SharedPlugin> = HashMap::new();
        registry.insert("p1".to_string(), p1);
        registry.insert("p2".to_string(), p2);

        let yaml = r#"
            primary: p1
            secondary: p2
        "#;
        let config: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let plugin = FallbackPlugin::new(Some(&config), &registry).unwrap();

        let mut ctx = make_ctx();
        plugin.next(&mut ctx).await.unwrap();

        assert!(*p1_called.lock().unwrap());
        assert!(*p2_called.lock().unwrap());
    }
}
