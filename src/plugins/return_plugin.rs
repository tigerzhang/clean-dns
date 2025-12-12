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
