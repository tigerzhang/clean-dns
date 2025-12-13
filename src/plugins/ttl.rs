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
            TtlConfig { min: None, max: None }
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