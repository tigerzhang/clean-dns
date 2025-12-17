use anyhow::{Context, Result};
use clap::Parser;
use clean_dns::{api, config, create_plugin_registry, get_entry_plugin, Server, Statistics};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tracing::{error, info};

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

    let registry = create_plugin_registry(&config)?;
    let entry_plugin = get_entry_plugin(&config, &registry)?;

    let statistics = Arc::new(RwLock::new(Statistics::new()));

    let api_port = config.api_port.unwrap_or(3000);
    let stats_for_api = statistics.clone();
    tokio::spawn(async move {
        if let Err(e) = api::start_api_server(stats_for_api, api_port).await {
            error!("Failed to start API server: {}", e);
        }
    });

    let bind_addr: SocketAddr = config.bind.parse().context("Invalid bind address")?;
    let server = Server::new(bind_addr, entry_plugin, statistics);

    server.run().await?;

    Ok(())
}
