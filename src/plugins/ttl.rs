use super::{Context, Plugin};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;

#[derive(Deserialize)]
struct TtlConfig {
    min: Option<u32>,
    max: Option<u32>,
}

pub struct TtlPlugin {
    min: u32,
    max: u32,
}

impl TtlPlugin {
    pub fn new(config: Option<&serde_yaml::Value>) -> Result<Self> {
        let config: TtlConfig = if let Some(c) = config {
            serde_yaml::from_value(c.clone())?
        } else {
            TtlConfig {
                min: None,
                max: None,
            }
        };
        Ok(Self {
            min: config.min.unwrap_or(0),
            max: config.max.unwrap_or(u32::MAX),
        })
    }
}

#[async_trait]
impl Plugin for TtlPlugin {
    fn name(&self) -> &str {
        "ttl"
    }

    async fn next(&self, ctx: &mut Context) -> Result<()> {
        if let Some(response) = &mut ctx.response {
            let modify = |records: &mut Vec<hickory_proto::rr::Record>| {
                for record in records {
                    let ttl = record.ttl();
                    if ttl < self.min {
                        record.set_ttl(self.min);
                    } else if ttl > self.max {
                        record.set_ttl(self.max);
                    }
                }
            };

            modify(response.answers_mut());
            modify(response.name_servers_mut());
            modify(response.additionals_mut());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_proto::rr::{Name, Record};
    use std::str::FromStr;
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
    async fn test_ttl_clamping() {
        let yaml = r#"
            min: 10
            max: 100
        "#;
        let config: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let plugin = TtlPlugin::new(Some(&config)).unwrap();

        let mut ctx = make_ctx();

        // Populate response with records
        use hickory_proto::op::Message;
        let mut response = Message::new();

        let mut rec_low = Record::new();
        rec_low.set_name(Name::from_str("low.com.").unwrap());
        rec_low.set_ttl(5);

        let mut rec_high = Record::new();
        rec_high.set_name(Name::from_str("high.com.").unwrap());
        rec_high.set_ttl(200);

        let mut rec_ok = Record::new();
        rec_ok.set_name(Name::from_str("ok.com.").unwrap());
        rec_ok.set_ttl(50);

        response.add_answer(rec_low);
        response.add_answer(rec_high);
        response.add_answer(rec_ok);

        ctx.response = Some(response);

        plugin.next(&mut ctx).await.unwrap();

        let answers = ctx.response.unwrap().answers().to_vec();
        assert_eq!(answers[0].ttl(), 10);
        assert_eq!(answers[1].ttl(), 100);
        assert_eq!(answers[2].ttl(), 50);
    }
}
