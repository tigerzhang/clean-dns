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
                        // k is the key, which contains domain.
                        // But wait, `get_key` includes query type and class. "google.com.-A-IN".
                        // Logic in `Statistics` expects domain name "google.com.".
                        // I should extract domain from request or parse key.
                        // `ctx.request` is available.
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
