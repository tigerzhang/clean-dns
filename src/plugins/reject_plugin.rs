use super::{Context, Plugin};
use anyhow::Result;
use async_trait::async_trait;
use hickory_proto::op::{Message, ResponseCode};
use serde::Deserialize;

#[derive(Deserialize)]
struct RejectConfig {
    #[serde(default = "default_rcode")]
    rcode: u8, // 5 = REFUSED, 3 = NXDOMAIN
}

fn default_rcode() -> u8 {
    5
}

pub struct RejectPlugin {
    rcode: ResponseCode,
}

impl RejectPlugin {
    pub fn new(config: Option<&serde_yaml::Value>) -> Result<Self> {
        let config: RejectConfig = if let Some(c) = config {
            serde_yaml::from_value(c.clone())?
        } else {
            RejectConfig { rcode: 5 }
        };

        // Convert u8 to ResponseCode safely (assuming low bits only for now)
        let rcode = ResponseCode::from(0, config.rcode);

        Ok(Self { rcode })
    }
}

#[async_trait]
impl Plugin for RejectPlugin {
    fn name(&self) -> &str {
        "reject"
    }

    async fn next(&self, ctx: &mut Context) -> Result<()> {
        let mut response = Message::new();
        response.set_header(ctx.request.header().clone());
        response.set_response_code(self.rcode);
        // Ensure it's a response
        response.set_message_type(hickory_proto::op::MessageType::Response);

        // Copy id
        response.set_id(ctx.request.id());

        ctx.response = Some(response);
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
    async fn test_reject_NXDOMAIN() {
        let yaml = r#"
            rcode: 3
        "#;
        let config: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let plugin = RejectPlugin::new(Some(&config)).unwrap();

        let mut ctx = make_ctx();
        plugin.next(&mut ctx).await.unwrap();

        assert!(ctx.response.is_some());
        assert_eq!(
            ctx.response.unwrap().response_code(),
            ResponseCode::NXDomain
        );
        assert!(ctx.abort);
    }
}
