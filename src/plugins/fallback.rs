use super::{Context, Plugin, SharedPlugin};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use tracing::warn;

/// One tag, or a list of tags. `secondary: forward_local` and
/// `secondary: [forward_local, forward_system]` are both valid.
#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    fn into_vec(self) -> Vec<String> {
        match self {
            OneOrMany::One(tag) => vec![tag],
            OneOrMany::Many(tags) => tags,
        }
    }
}

#[derive(Deserialize)]
struct FallbackConfig {
    /// Ordered plugins. The first one that produces a response wins.
    /// Use this to fall through local forwarders to the system resolver.
    #[serde(default)]
    exec: Vec<String>,
    #[serde(default)]
    primary: Option<String>,
    #[serde(default)]
    secondary: Option<OneOrMany>,
}

struct FallbackStep {
    tag: String,
    plugin: SharedPlugin,
}

pub struct FallbackPlugin {
    steps: Vec<FallbackStep>,
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

        let tags = if !config.exec.is_empty() {
            if config.primary.is_some() || config.secondary.is_some() {
                return Err(anyhow::anyhow!(
                    "fallback `exec` cannot be combined with `primary`/`secondary`"
                ));
            }
            config.exec
        } else {
            let primary = config
                .primary
                .ok_or_else(|| anyhow::anyhow!("FallbackPlugin requires `exec` or `primary`"))?;
            let mut secondary = config
                .secondary
                .ok_or_else(|| anyhow::anyhow!("FallbackPlugin requires `secondary`"))?
                .into_vec();
            secondary.retain(|tag| !tag.is_empty());
            if secondary.is_empty() {
                return Err(anyhow::anyhow!(
                    "FallbackPlugin requires at least one secondary plugin"
                ));
            }
            let mut tags = Vec::with_capacity(1 + secondary.len());
            tags.push(primary);
            tags.append(&mut secondary);
            tags
        };

        let mut steps = Vec::with_capacity(tags.len());
        for tag in tags {
            let plugin = registry
                .get(&tag)
                .ok_or_else(|| anyhow::anyhow!("Fallback plugin not found: {}", tag))?
                .clone();
            steps.push(FallbackStep { tag, plugin });
        }

        Ok(Self { steps })
    }
}

#[async_trait]
impl Plugin for FallbackPlugin {
    fn name(&self) -> &str {
        "fallback"
    }

    async fn next(&self, ctx: &mut Context) -> Result<()> {
        // Already answered or aborted by an earlier stage — nothing to do.
        if ctx.response.is_some() || ctx.abort {
            return Ok(());
        }

        let mut last_err: Option<anyhow::Error> = None;

        for (i, step) in self.steps.iter().enumerate() {
            if ctx.response.is_some() || ctx.abort {
                return Ok(());
            }

            let more = i + 1 < self.steps.len();
            match step.plugin.next(ctx).await {
                Ok(()) if ctx.response.is_some() || ctx.abort => return Ok(()),
                Ok(()) => {
                    if more {
                        warn!(
                            "Plugin {} ({}) produced no response. Trying the next forwarder.",
                            step.tag,
                            step.plugin.name()
                        );
                    } else {
                        warn!(
                            "Plugin {} ({}) produced no response.",
                            step.tag,
                            step.plugin.name()
                        );
                    }
                }
                Err(e) => {
                    // A response alongside the error is still an answer.
                    if ctx.response.is_some() || ctx.abort {
                        return Ok(());
                    }
                    if more {
                        warn!(
                            "Plugin {} ({}) failed: {:#}. Trying the next forwarder.",
                            step.tag,
                            step.plugin.name(),
                            e
                        );
                    } else {
                        warn!(
                            "Plugin {} ({}) failed: {:#}.",
                            step.tag,
                            step.plugin.name(),
                            e
                        );
                    }
                    last_err = Some(e);
                }
            }
        }

        if ctx.response.is_some() || ctx.abort {
            return Ok(());
        }

        // Every forwarder missed. Surface the last hard failure so the server
        // answers SERVFAIL instead of dropping the query.
        if let Some(e) = last_err {
            return Err(e.context("all fallback forwarders failed"));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct MockPlugin {
        fail: bool,
        /// When true, sets a dummy response so fallback treats this as a real answer.
        produce_response: bool,
        called: Arc<Mutex<bool>>,
    }

    #[async_trait]
    impl Plugin for MockPlugin {
        fn name(&self) -> &str {
            "mock"
        }
        async fn next(&self, ctx: &mut Context) -> Result<()> {
            *self.called.lock().unwrap() = true;
            if self.fail {
                Err(anyhow::anyhow!("Mock fail"))
            } else {
                if self.produce_response {
                    ctx.response = Some(hickory_proto::op::Message::new());
                }
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
            produce_response: true,
            called: p1_called.clone(),
        });
        let p2_called = Arc::new(Mutex::new(false));
        let p2 = Arc::new(MockPlugin {
            fail: false,
            produce_response: true,
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
        assert!(ctx.response.is_some());
    }

    #[tokio::test]
    async fn test_fallback_failure() {
        let p1_called = Arc::new(Mutex::new(false));
        let p1 = Arc::new(MockPlugin {
            fail: true,
            produce_response: false,
            called: p1_called.clone(),
        });
        let p2_called = Arc::new(Mutex::new(false));
        let p2 = Arc::new(MockPlugin {
            fail: false,
            produce_response: true,
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
        assert!(ctx.response.is_some());
    }

    #[tokio::test]
    async fn test_fallback_no_response() {
        // Primary returns Ok but no answer → secondary must run.
        let p1_called = Arc::new(Mutex::new(false));
        let p1 = Arc::new(MockPlugin {
            fail: false,
            produce_response: false,
            called: p1_called.clone(),
        });
        let p2_called = Arc::new(Mutex::new(false));
        let p2 = Arc::new(MockPlugin {
            fail: false,
            produce_response: true,
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
        assert!(ctx.response.is_some());
    }

    fn insert_mock(
        registry: &mut HashMap<String, SharedPlugin>,
        tag: &str,
        fail: bool,
        produce_response: bool,
    ) -> Arc<Mutex<bool>> {
        let called = Arc::new(Mutex::new(false));
        registry.insert(
            tag.to_string(),
            Arc::new(MockPlugin {
                fail,
                produce_response,
                called: called.clone(),
            }),
        );
        called
    }

    #[tokio::test]
    async fn test_fallback_exec_chain_skips_after_success() {
        let mut registry = HashMap::new();
        let proxy = insert_mock(&mut registry, "forward_proxy", true, false);
        let local = insert_mock(&mut registry, "forward_local", false, true);
        let system = insert_mock(&mut registry, "forward_system", false, true);

        let yaml = r#"
            exec:
              - forward_proxy
              - forward_local
              - forward_system
        "#;
        let config: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let plugin = FallbackPlugin::new(Some(&config), &registry).unwrap();

        let mut ctx = make_ctx();
        plugin.next(&mut ctx).await.unwrap();

        assert!(*proxy.lock().unwrap());
        assert!(*local.lock().unwrap());
        assert!(!*system.lock().unwrap());
        assert!(ctx.response.is_some());
    }

    #[tokio::test]
    async fn test_fallback_exec_reaches_system_after_forwarders_fail() {
        let mut registry = HashMap::new();
        let proxy = insert_mock(&mut registry, "forward_proxy", true, false);
        let local = insert_mock(&mut registry, "forward_local", false, false);
        let system = insert_mock(&mut registry, "forward_system", false, true);

        let yaml = r#"
            primary: forward_proxy
            secondary:
              - forward_local
              - forward_system
        "#;
        let config: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let plugin = FallbackPlugin::new(Some(&config), &registry).unwrap();

        let mut ctx = make_ctx();
        plugin.next(&mut ctx).await.unwrap();

        assert!(*proxy.lock().unwrap());
        assert!(*local.lock().unwrap());
        assert!(*system.lock().unwrap());
        assert!(ctx.response.is_some());
    }

    #[tokio::test]
    async fn test_fallback_all_failed_returns_error() {
        let mut registry = HashMap::new();
        let _proxy = insert_mock(&mut registry, "forward_proxy", true, false);
        let _local = insert_mock(&mut registry, "forward_local", true, false);
        // System-style soft failure: Ok, but no answer.
        let _system = insert_mock(&mut registry, "forward_system", false, false);

        let yaml = r#"
            exec:
              - forward_proxy
              - forward_local
              - forward_system
        "#;
        let config: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let plugin = FallbackPlugin::new(Some(&config), &registry).unwrap();

        let mut ctx = make_ctx();
        let err = plugin.next(&mut ctx).await.unwrap_err();
        let rendered = format!("{:#}", err);
        assert!(rendered.contains("all fallback forwarders failed"));
        assert!(rendered.contains("Mock fail"));
        assert!(ctx.response.is_none());
    }
}
