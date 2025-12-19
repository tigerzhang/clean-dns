pub mod api;
pub mod config;
pub mod plugins;
pub mod server;
pub mod statistics;

use std::collections::HashMap;
use std::sync::Arc;

// Re-export specific types if needed by main or tests
pub use api::start_api_server;
pub use config::Config;
pub use plugins::SharedPlugin;
pub use server::Server;
pub use statistics::Statistics;

// Helper to initialize registry (logic moved from main)
pub fn create_plugin_registry(config: &Config) -> anyhow::Result<HashMap<String, SharedPlugin>> {
    use plugins::cache::Cache;
    use plugins::delay_plugin::DelayPlugin;
    use plugins::domain_set::DomainSetPlugin;
    use plugins::fallback::FallbackPlugin;
    use plugins::forward::Forward;
    use plugins::hosts::Hosts;
    use plugins::if_plugin::IfPlugin;
    use plugins::ip_set::IpSetPlugin;
    use plugins::matcher::Matcher;
    use plugins::reject_plugin::RejectPlugin;
    use plugins::return_plugin::ReturnPlugin;
    use plugins::sequence::Sequence;
    use plugins::system::System;
    use plugins::ttl::TtlPlugin;

    let mut registry: HashMap<String, SharedPlugin> = HashMap::new();

    for plugin_conf in &config.plugins {
        let tag = plugin_conf.tag.clone();
        let type_ = plugin_conf.type_.as_str();

        // info!("Loading plugin {} (type: {})", tag, type_); // Removed logging here, caller can log or we can pass logger?
        // Actually, main initialized tracing, so we can log.
        tracing::info!("Loading plugin {} (type: {})", tag, type_);

        let plugin: SharedPlugin = match type_ {
            "forward" => Arc::new(Forward::new(plugin_conf.args.as_ref())?),
            "sequence" => Arc::new(Sequence::new(plugin_conf.args.as_ref(), &registry)?),
            "matcher" => Arc::new(Matcher::new(plugin_conf.args.as_ref(), &registry)?),
            "hosts" => Arc::new(Hosts::new(plugin_conf.args.as_ref())?),
            "cache" => Arc::new(Cache::new(plugin_conf.args.as_ref(), &registry)?),
            "domain_set" => Arc::new(DomainSetPlugin::new(plugin_conf.args.as_ref())?),
            "ip_set" => Arc::new(IpSetPlugin::new(plugin_conf.args.as_ref())?),
            "if" => Arc::new(IfPlugin::new(plugin_conf.args.as_ref(), &registry)?),
            "return" => Arc::new(ReturnPlugin::new(plugin_conf.args.as_ref())?),
            "reject" => Arc::new(RejectPlugin::new(plugin_conf.args.as_ref())?),
            "system" => Arc::new(System::new(plugin_conf.args.as_ref())?),
            "delay" => Arc::new(DelayPlugin::new(plugin_conf.args.as_ref())?),
            "fallback" => Arc::new(FallbackPlugin::new(plugin_conf.args.as_ref(), &registry)?),
            "ttl" => Arc::new(TtlPlugin::new(plugin_conf.args.as_ref())?),
            _ => {
                tracing::warn!("Unknown plugin type: {}", type_);
                continue;
            }
        };
        registry.insert(tag, plugin);
    }
    Ok(registry)
}

pub fn get_entry_plugin(
    config: &Config,
    registry: &HashMap<String, SharedPlugin>,
) -> anyhow::Result<SharedPlugin> {
    if config.entry.is_empty() {
        tracing::warn!("No entry plugin specified, using 'main' or the last loaded one");
        registry
            .get("main")
            .cloned()
            .or_else(|| None)
            .ok_or_else(|| anyhow::anyhow!("No entry plugin found"))
    } else {
        registry
            .get(&config.entry)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Entry plugin '{}' not found", config.entry))
    }
}
