use super::{Context, Plugin, SharedPlugin};
use anyhow::Result;
use async_trait::async_trait;
use hickory_proto::op::Message;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::info;

#[derive(Deserialize)]
struct CacheConfig {
    size: usize,
    #[serde(default)]
    exec: Vec<String>,
}

struct CacheEntry {
    response: Message,
    valid_until: Instant,
}

pub struct Cache {
    cache: Mutex<HashMap<String, CacheEntry>>,
    ttl: Duration,
    plugins: Vec<SharedPlugin>,
}

impl Cache {
    pub fn new(
        config: Option<&serde_yaml::Value>,
        registry: &HashMap<String, SharedPlugin>,
    ) -> Result<Self> {
        let config: CacheConfig = if let Some(c) = config {
            serde_yaml::from_value(c.clone())?
        } else {
            CacheConfig {
                size: 1024,
                exec: vec![],
            }
        };

        let mut plugins = Vec::new();
        for tag in config.exec {
            let p = registry
                .get(&tag)
                .ok_or_else(|| anyhow::anyhow!("Plugin not found: {}", tag))?;
            plugins.push(p.clone());
        }

        Ok(Self {
            cache: Mutex::new(HashMap::with_capacity(config.size)), // TODO: Real LRU
            ttl: Duration::from_secs(60),                           // Default TTL cap
            plugins,
        })
    }

    fn get_key(&self, request: &Message) -> Option<String> {
        if let Some(query) = request.query() {
            return Some(format!(
                "{:?}-{:?}-{:?}",
                query.name(),
                query.query_type(),
                query.query_class()
            ));
        }
        None
    }
}

#[async_trait]
impl Plugin for Cache {
    fn name(&self) -> &str {
        "cache"
    }

    async fn next(&self, ctx: &mut Context) -> Result<()> {
        let key = self.get_key(&ctx.request);

        if let Some(k) = &key {
            let mut cache = self.cache.lock().unwrap();
            if let Some(entry) = cache.get(k) {
                if entry.valid_until > Instant::now() {
                    let mut response = entry.response.clone();
                    response.set_id(ctx.request.id()); // Update ID to match request
                    ctx.response = Some(response);
                    info!("Cache hit for {}", k);
                    {
                        let mut stats = ctx.stats.write().unwrap();
                        if let Some(query) = ctx.request.query() {
                            stats.record_cache_hit(query.name().to_string());
                        }
                    }
                    return Ok(());
                } else {
                    cache.remove(k);
                }
            }
        }

        // Cache miss
        for plugin in &self.plugins {
            plugin.next(ctx).await?;
        }

        // Cache response if available
        if let Some(response) = &ctx.response {
            if let Some(k) = key {
                let mut cache = self.cache.lock().unwrap();
                // Simple TTL logic: check first answer's TTL or default
                // Keep it simple for now
                cache.insert(
                    k,
                    CacheEntry {
                        response: response.clone(),
                        valid_until: Instant::now() + self.ttl,
                    },
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::{Arc, RwLock};

    fn make_ctx(name: &str) -> Context {
        use hickory_proto::op::{Message, Query};
        use hickory_proto::rr::{Name, RecordType};
        use std::str::FromStr;

        let mut msg = Message::new();
        msg.add_query(Query::query(Name::from_str(name).unwrap(), RecordType::A));
        msg.set_id(123);

        use crate::statistics::Statistics;
        let stats = Arc::new(RwLock::new(Statistics::new()));

        Context::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1234),
            msg,
            stats,
        )
    }

    #[tokio::test]
    async fn test_cache_miss_hit() {
        // We need a dummy plugin registry for Cache::new if we used exec, but here exec is empty.
        let cache = Cache {
            cache: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(60),
            plugins: vec![],
        };

        let mut ctx = make_ctx("example.com.");

        // First call: Miss
        cache.next(&mut ctx).await.unwrap();
        assert!(ctx.response.is_none());

        // Manually Populate cache
        let key = cache.get_key(&ctx.request).unwrap();
        let mut response = ctx.request.clone();

        use hickory_proto::rr::Name;
        use hickory_proto::rr::{DNSClass, RData, Record, RecordType};
        use std::str::FromStr;

        let mut record = Record::new();
        record
            .set_name(Name::from_str("example.com.").unwrap())
            .set_rr_type(RecordType::A)
            .set_dns_class(DNSClass::IN)
            .set_ttl(60)
            .set_data(Some(RData::A(Ipv4Addr::new(1, 2, 3, 4).into())));

        response.add_answer(record);
        response.set_message_type(hickory_proto::op::MessageType::Response);

        {
            let mut map = cache.cache.lock().unwrap();
            map.insert(
                key,
                CacheEntry {
                    response: response.clone(),
                    valid_until: Instant::now() + Duration::from_secs(100),
                },
            );
        }

        // Second call: Hit
        let mut ctx2 = make_ctx("example.com.");
        cache.next(&mut ctx2).await.unwrap();

        assert!(ctx2.response.is_some());
        assert_eq!(ctx2.response.unwrap().answers().len(), 1);
    }
}
