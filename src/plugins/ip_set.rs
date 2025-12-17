use super::{Context, IpSet, Plugin};
use anyhow::Result;
use async_trait::async_trait;
use ipnet::IpNet;
use serde::Deserialize;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::net::IpAddr;
use std::str::FromStr;
use tracing::{info, warn};

#[derive(Deserialize)]
struct IpSetConfig {
    files: Vec<String>,
}

pub struct IpSetPlugin {
    cidrs: Vec<IpNet>,
}

impl IpSetPlugin {
    pub fn new(config: Option<&serde_yaml::Value>) -> Result<Self> {
        let config: IpSetConfig = if let Some(c) = config {
            serde_yaml::from_value(c.clone())?
        } else {
            return Err(anyhow::anyhow!("IpSet requires config"));
        };

        let mut cidrs = Vec::new();

        for path in config.files {
            if let Ok(file) = File::open(&path) {
                let reader = BufReader::new(file);
                for line in reader.lines() {
                    if let Ok(l) = line {
                        let l = l.trim();
                        if !l.is_empty() && !l.starts_with('#') {
                            if let Ok(net) = IpNet::from_str(l) {
                                cidrs.push(net);
                            } else if let Ok(ip) = IpAddr::from_str(l) {
                                cidrs.push(IpNet::from(ip));
                            } else {
                                warn!("Invalid IP/CIDR in {}: {}", path, l);
                            }
                        }
                    }
                }
                info!("Loaded IPs from {}", path);
            } else {
                warn!("Failed to open ip file: {}", path);
            }
        }

        Ok(Self { cidrs })
    }
}

impl IpSet for IpSetPlugin {
    fn contains(&self, ip: IpAddr) -> bool {
        for cidr in &self.cidrs {
            if cidr.contains(&ip) {
                return true;
            }
        }
        false
    }
}

#[async_trait]
impl Plugin for IpSetPlugin {
    fn name(&self) -> &str {
        "ip_set"
    }

    async fn next(&self, _ctx: &mut Context) -> Result<()> {
        Ok(())
    }

    fn as_ip_set(&self) -> Option<&dyn IpSet> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_ip_set_loading_and_matching() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "192.168.1.0/24").unwrap();
        writeln!(file, "10.0.0.1").unwrap();

        let path = file.path().to_str().unwrap().to_string();

        let yaml = format!(
            r#"
            files:
              - "{}"
            "#,
            path
        );
        let config: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();

        let plugin = IpSetPlugin::new(Some(&config)).unwrap();

        // Match CIDR
        assert!(plugin.contains(IpAddr::from_str("192.168.1.50").unwrap()));
        // Match exact IP
        assert!(plugin.contains(IpAddr::from_str("10.0.0.1").unwrap()));
        // No match
        assert!(!plugin.contains(IpAddr::from_str("8.8.8.8").unwrap()));
    }
}
