use super::{Context, Plugin};
use anyhow::Result;
use async_trait::async_trait;

pub struct ReturnPlugin;

impl ReturnPlugin {
    pub fn new(_config: Option<&serde_yaml::Value>) -> Result<Self> {
        Ok(Self)
    }
}

#[async_trait]
impl Plugin for ReturnPlugin {
    fn name(&self) -> &str {
        "return"
    }

    async fn next(&self, ctx: &mut Context) -> Result<()> {
        ctx.abort = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, RwLock};

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
    async fn test_return_plugin() {
        let plugin = ReturnPlugin::new(None).unwrap();
        let mut ctx = make_ctx();
        plugin.next(&mut ctx).await.unwrap();
        assert!(ctx.abort);
    }
}
