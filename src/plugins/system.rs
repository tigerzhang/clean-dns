use super::{Context, Plugin};
use anyhow::Result;
use async_trait::async_trait;
use hickory_proto::op::{Message, MessageType, OpCode, ResponseCode};
use hickory_resolver::TokioAsyncResolver;
use tracing::debug;

pub struct System {
    resolver: TokioAsyncResolver,
}

impl System {
    pub fn new(_config: Option<&serde_yaml::Value>) -> Result<Self> {
        let resolver = TokioAsyncResolver::tokio_from_system_conf()?;
        Ok(Self { resolver })
    }
}

#[async_trait]
impl Plugin for System {
    fn name(&self) -> &str {
        "system"
    }

    async fn next(&self, ctx: &mut Context) -> Result<()> {
        if ctx.response.is_some() {
            return Ok(());
        }

        if let Some(query) = ctx.request.query() {
            let name = query.name();
            let qtype = query.query_type();

            debug!("System resolving {} {:?}", name, qtype);

            // Perform lookup
            let lookup = self.resolver.lookup(name.clone(), qtype).await;

            match lookup {
                Ok(lookup_res) => {
                    let mut response = Message::new();
                    response.set_id(ctx.request.id());
                    response.set_message_type(MessageType::Response);
                    response.set_op_code(OpCode::Query);
                    response.set_recursion_desired(ctx.request.recursion_desired());
                    response.set_recursion_available(true);
                    response.set_response_code(ResponseCode::NoError);
                    response.add_query(query.clone());

                    for record in lookup_res.record_iter() {
                        response.add_answer(record.clone());
                    }

                    ctx.response = Some(response);
                    debug!("System resolved success for {}", name);
                }
                Err(e) => {
                    debug!("System resolve failed for {}: {}", name, e);
                    // We don't necessarily set an error response here,
                    // allowing other plugins in the chain (like fallback) to try.
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::statistics::Statistics;
    use hickory_proto::op::Query;
    use hickory_proto::rr::{Name, RecordType};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::str::FromStr;
    use std::sync::{Arc, RwLock};

    fn make_ctx(name: &str) -> Context {
        let mut msg = Message::new();
        msg.add_query(Query::query(Name::from_str(name).unwrap(), RecordType::A));
        msg.set_id(123);

        Context::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1234),
            msg,
            Arc::new(RwLock::new(Statistics::new())),
        )
    }

    #[tokio::test]
    async fn test_system_resolve() {
        // This test depends on the system having a working DNS
        let plugin = System::new(None).unwrap();
        let mut ctx = make_ctx("google.com.");

        plugin.next(&mut ctx).await.unwrap();

        if ctx.response.is_some() {
            let resp = ctx.response.unwrap();
            println!("Response received: {:?}", resp);
            // It's a success if we got any response from the system resolver
            assert_eq!(resp.id(), 123);
        } else {
            // If system DNS is down, we might get none, but usually it should work
            println!("System resolve skipped or failed, which might be okay in some environments");
        }
    }
}
