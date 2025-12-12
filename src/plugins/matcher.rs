use super::{Condition, Context, Plugin, SharedPlugin};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use tracing::{info, warn};

#[derive(Deserialize)]
struct MatcherConfig {
    #[serde(default)]
    domain: Vec<String>,
    #[serde(default)]
    client_ip: Vec<String>,
    #[serde(default)]
    exec: Vec<String>,
}

pub struct Matcher {
    domains: Vec<String>,
    domain_providers: Vec<SharedPlugin>,
    ip_providers: Vec<SharedPlugin>,
    plugins: Vec<SharedPlugin>,
}

impl Matcher {
    pub fn new(
        config: Option<&serde_yaml::Value>,
        registry: &HashMap<String, SharedPlugin>,
    ) -> Result<Self> {
        let config: MatcherConfig = if let Some(c) = config {
            serde_yaml::from_value(c.clone())?
        } else {
            MatcherConfig {
                domain: vec![],
                client_ip: vec![],
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

        let mut direct_domains = Vec::new();
        let mut domain_providers = Vec::new();

        for d in config.domain {
            if d.starts_with("provider:") {
                let tag = &d["provider:".len()..];
                let p = registry
                    .get(tag)
                    .ok_or_else(|| anyhow::anyhow!("Provider plugin not found: {}", tag))?;
                if p.as_domain_set().is_some() {
                    domain_providers.push(p.clone());
                } else {
                    return Err(anyhow::anyhow!("Plugin {} is not a DomainSet", tag));
                }
            } else {
                direct_domains.push(d);
            }
        }

        let mut ip_providers = Vec::new();
        for ip_ref in config.client_ip {
            if ip_ref.starts_with("provider:") {
                let tag = &ip_ref["provider:".len()..];
                let p = registry
                    .get(tag)
                    .ok_or_else(|| anyhow::anyhow!("Provider plugin not found: {}", tag))?;
                if p.as_ip_set().is_some() {
                    ip_providers.push(p.clone());
                } else {
                    return Err(anyhow::anyhow!("Plugin {} is not an IpSet", tag));
                }
            } else {
                warn!(
                    "Direct IP/CIDR matching not yet implemented in Matcher, ignoring: {}",
                    ip_ref
                );
            }
        }

        Ok(Self {
            domains: direct_domains,
            domain_providers,
            ip_providers,
            plugins,
        })
    }

    fn matches(&self, ctx: &Context) -> bool {
        // Match Domain
        if !self.domains.is_empty() || !self.domain_providers.is_empty() {
            if let Some(query) = ctx.request.query() {
                let name = query.name().to_string();
                let name_clean = name.trim_end_matches('.');

                for d in &self.domains {
                    if name_clean == d || name_clean.ends_with(&format!(".{}", d)) {
                        return true;
                    }
                }

                for p in &self.domain_providers {
                    if let Some(ds) = p.as_domain_set() {
                        if ds.contains(name_clean) {
                            return true;
                        }
                    }
                }
            }
        }

        // Match Client IP
        if !self.ip_providers.is_empty() {
            let ip = ctx.client_addr.ip();
            for p in &self.ip_providers {
                if let Some(is) = p.as_ip_set() {
                    if is.contains(ip) {
                        return true;
                    }
                }
            }
        }

        false
    }
}

impl Condition for Matcher {
    fn check(&self, ctx: &Context) -> bool {
        self.matches(ctx)
    }
}

#[async_trait]
impl Plugin for Matcher {
    fn name(&self) -> &str {
        "matcher"
    }

    async fn next(&self, ctx: &mut Context) -> Result<()> {
        if self.matches(ctx) {
            info!("Matcher matched, executing sub-plugins");
            for plugin in &self.plugins {
                plugin.next(ctx).await?;
                if ctx.response.is_some() || ctx.abort {
                    break;
                }
            }
        }
        Ok(())
    }

    fn as_condition(&self) -> Option<&dyn Condition> {
        Some(self)
    }
}
