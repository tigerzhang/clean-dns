use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use prost::Message;
// use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, RwLock};
use tracing::{error, info};

use clean_dns::proto;
use clean_dns::{api, config, create_plugin_registry, get_entry_plugin, Server, Statistics};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Config file path (used if no subcommand or for generic run)
    #[arg(short, long, default_value = "config.yaml")]
    config: String,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the DNS server (default)
    Run {
        #[arg(short, long, default_value = "config.yaml")]
        config: String,
    },
    /// Compile v2fly domain-list-community files into a geosite.dat
    MakeGeosite {
        /// Source directory containing domain list files (e.g., data/domain-list-community/data)
        #[arg(short, long)]
        source: String,
        /// Output file path (e.g., geosite.dat)
        #[arg(short, long, default_value = "geosite.dat")]
        output: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    match args.command {
        Some(Commands::MakeGeosite { source, output }) => {
            make_geosite(source, output).await?;
        }
        Some(Commands::Run { config }) => {
            run_server(config).await?;
        }
        None => {
            // Default behavior: run server with top-level config arg
            run_server(args.config).await?;
        }
    }

    Ok(())
}

async fn run_server(config_path: String) -> Result<()> {
    let config = config::Config::from_file(&config_path)?;
    info!("Loaded config from {}", config_path);

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

async fn make_geosite(source: String, output: String) -> Result<()> {
    info!("Compiling geosite from {} to {}", source, output);
    let source_path = Path::new(&source);
    let mut geosite_list = proto::GeoSiteList { entry: vec![] };

    let entries = std::fs::read_dir(source_path).context("Failed to read source directory")?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let filename = path.file_name().unwrap().to_string_lossy().to_string();
            // Skip hidden files or unrelated files
            if filename.starts_with('.') || filename.ends_with(".md") {
                continue;
            }

            info!("Processing {}", filename);
            let mut domains = vec![];
            load_domain_file(&path, &mut domains)?;

            geosite_list.entry.push(proto::GeoSite {
                country_code: filename.to_uppercase(),
                domain: domains,
            });
        }
    }

    // Serialize and write to file
    let mut file = File::create(&output).context("Failed to create output file")?;
    let mut buf = Vec::new();
    geosite_list.encode(&mut buf)?;
    file.write_all(&buf)?;
    info!(
        "Successfully created {} with {} entries",
        output,
        geosite_list.entry.len()
    );

    Ok(())
}

fn load_domain_file(path: &Path, domains: &mut Vec<proto::Domain>) -> Result<()> {
    let file = File::open(path).context(format!("Failed to open {:?}", path))?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        let mut line = line.trim();

        // Strip comments
        if let Some(idx) = line.find('#') {
            line = &line[..idx];
        }
        line = line.trim();

        if line.is_empty() {
            continue;
        }

        // Strip attributes for now (simplification)
        if let Some(idx) = line.find(char::is_whitespace) {
            line = &line[..idx];
        }

        let (type_, value) = if let Some(val) = line.strip_prefix("include:") {
            // Recursive handling: find the included file in the same directory
            let parent = path.parent().unwrap();
            let included_path = parent.join(val);
            load_domain_file(&included_path, domains)?;
            continue;
        } else if let Some(val) = line.strip_prefix("full:") {
            (proto::domain::Type::Regex, val)
        } else if let Some(val) = line.strip_prefix("regexp:") {
            (proto::domain::Type::Regex, val)
        } else if let Some(val) = line.strip_prefix("domain:") {
            (proto::domain::Type::RootDomain, val)
        } else if let Some(val) = line.strip_prefix("keyword:") {
            (proto::domain::Type::Plain, val)
        } else {
            (proto::domain::Type::RootDomain, line)
        };

        // Note: proto::clean_dns::domain::Type might be different depending on how prost generates nested modules.
        // check router.proto structure.
        // package clean_dns;
        // message Domain { enum Type { ... } }
        // So Rust path: proto::Domain::Type ? No, prost usually flattens or uses modules.
        // Let's check router.proto.

        domains.push(proto::Domain {
            r#type: type_ as i32,
            value: value.to_string(),
            attribute: vec![],
        });
    }
    Ok(())
}
