use anyhow::{Context, Result};
use clap::Parser;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{info, warn};

mod config;
mod plugins;
mod server;

use crate::plugins::cache::Cache;
use crate::plugins::delay_plugin::DelayPlugin;
use crate::plugins::domain_set::DomainSetPlugin;
use crate::plugins::fallback::FallbackPlugin;
use crate::plugins::forward::Forward;
use crate::plugins::hosts::Hosts;
use crate::plugins::if_plugin::IfPlugin;
use crate::plugins::ip_set::IpSetPlugin;
use crate::plugins::matcher::Matcher;
use crate::plugins::reject_plugin::RejectPlugin;
use crate::plugins::return_plugin::ReturnPlugin;
use crate::plugins::sequence::Sequence;
use crate::plugins::ttl::TtlPlugin;
use crate::plugins::SharedPlugin;
use crate::server::Server;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "config.yaml")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let config = config::Config::from_file(&args.config)?;

    info!("Loaded config from {}", args.config);

    let mut registry: HashMap<String, SharedPlugin> = HashMap::new();

    for plugin_conf in config.plugins {
        let tag = plugin_conf.tag.clone();
        let type_ = plugin_conf.type_.as_str();

        info!("Loading plugin {} (type: {})", tag, type_);

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
            "delay" => Arc::new(DelayPlugin::new(plugin_conf.args.as_ref())?),
            "fallback" => Arc::new(FallbackPlugin::new(plugin_conf.args.as_ref(), &registry)?),
            "ttl" => Arc::new(TtlPlugin::new(plugin_conf.args.as_ref())?),
            _ => {
                warn!("Unknown plugin type: {}", type_);
                continue;
            }
        };

        registry.insert(tag, plugin);
    }

    let entry_plugin = if config.entry.is_empty() {
        warn!("No entry plugin specified, using 'main' or the last loaded one");
        // Fallback logic
        registry
            .get("main")
            .cloned()
            .or_else(|| {
                // If no main, maybe just pick one?
                None
            })
            .ok_or_else(|| anyhow::anyhow!("No entry plugin found"))?
    } else {
        registry
            .get(&config.entry)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Entry plugin '{}' not found", config.entry))?
    };

    let bind_addr: SocketAddr = config.bind.parse().context("Invalid bind address")?;
    let server = Server::new(bind_addr, entry_plugin);

    server.run().await?;

    Ok(())
}
