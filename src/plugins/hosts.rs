use super::{Context, Plugin};
use anyhow::Result;
use async_trait::async_trait;
use hickory_proto::op::Message;
use hickory_proto::rr::{RData, Record, RecordType};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::net::IpAddr;
use std::str::FromStr;
use tracing::{info, warn};

#[derive(Deserialize)]
struct HostsConfig {
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    hosts: HashMap<String, String>,
}

pub struct Hosts {
    mappings: HashMap<String, IpAddr>,
}

impl Hosts {
    pub fn new(config: Option<&serde_yaml::Value>) -> Result<Self> {
        let config: HostsConfig = if let Some(c) = config {
            serde_yaml::from_value(c.clone())?
        } else {
            HostsConfig {
                files: vec![],
                hosts: HashMap::new(),
            }
        };

        let mut mappings = HashMap::new();

        // Load from files
        for path in config.files {
            if let Ok(file) = File::open(&path) {
                let reader = BufReader::new(file);
                for line in reader.lines() {
                    if let Ok(l) = line {
                        let parts: Vec<&str> = l.split_whitespace().collect();
                        if parts.len() >= 2 {
                            if let Ok(ip) = IpAddr::from_str(parts[0]) {
                                for domain in &parts[1..] {
                                    mappings.insert(domain.to_string(), ip);
                                }
                            }
                        }
                    }
                }
            } else {
                warn!("Failed to open hosts file: {}", path);
            }
        }

        // Load from inline config
        for (domain, ip_str) in config.hosts {
            if let Ok(ip) = IpAddr::from_str(&ip_str) {
                mappings.insert(domain, ip);
            } else {
                warn!("Invalid IP in hosts config: {}", ip_str);
            }
        }

        Ok(Self { mappings })
    }
}

#[async_trait]
impl Plugin for Hosts {
    fn name(&self) -> &str {
        "hosts"
    }

    async fn next(&self, ctx: &mut Context) -> Result<()> {
        if ctx.response.is_some() {
            return Ok(());
        }

        if let Some(query) = ctx.request.query() {
            let name = query.name().to_string();
            let name_clean = name.trim_end_matches('.');

            if let Some(ip) = self.mappings.get(name_clean) {
                let mut response = Message::new();
                response.set_id(ctx.request.id());
                response.set_message_type(hickory_proto::op::MessageType::Response);
                response.set_op_code(hickory_proto::op::OpCode::Query);
                response.set_recursion_desired(true);
                response.set_recursion_available(true);
                response.set_response_code(hickory_proto::op::ResponseCode::NoError);
                response.add_query(query.clone());

                let rdata = match ip {
                    IpAddr::V4(ipv4) => RData::A(hickory_proto::rr::rdata::A(*ipv4)),
                    IpAddr::V6(ipv6) => RData::AAAA(hickory_proto::rr::rdata::AAAA(*ipv6)),
                };

                let mut record = Record::with(query.name().clone(), RecordType::A, 60);
                record.set_data(Some(rdata));
                response.add_answer(record);

                ctx.response = Some(response);
                info!("Hosts match for {}: {}", name, ip);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Arc, RwLock};
    use tempfile::NamedTempFile;

    fn make_ctx(name: &str) -> Context {
        use crate::statistics::Statistics;
        use hickory_proto::op::{Message, Query};
        use hickory_proto::rr::{Name, RecordType};
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        let mut msg = Message::new();
        msg.add_query(Query::query(Name::from_str(name).unwrap(), RecordType::A));

        Context::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1234),
            msg,
            Arc::new(RwLock::new(Statistics::new())),
        )
    }

    #[tokio::test]
    async fn test_hosts_lookup() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "1.2.3.4 test.local").unwrap();
        let path = file.path().to_str().unwrap().to_string();

        let yaml = format!(
            r#"
            files:
              - "{}"
            hosts:
              entry.local: "5.6.7.8"
            "#,
            path
        );
        let config: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        let hosts = Hosts::new(Some(&config)).unwrap();

        // Match from file
        let mut ctx = make_ctx("test.local.");
        hosts.next(&mut ctx).await.unwrap();
        assert!(ctx.response.is_some());
        let answers = ctx.response.as_ref().unwrap().answers();
        assert_eq!(answers.len(), 1);
        if let Some(RData::A(ip)) = answers[0].data() {
            assert_eq!(ip.to_string(), "1.2.3.4");
        } else {
            panic!("Expected A record");
        }

        // Match from inline
        let mut ctx = make_ctx("entry.local.");
        hosts.next(&mut ctx).await.unwrap();
        assert!(ctx.response.is_some());
        if let Some(RData::A(ip)) = ctx.response.as_ref().unwrap().answers()[0].data() {
            assert_eq!(ip.to_string(), "5.6.7.8");
        }

        // No match
        let mut ctx = make_ctx("google.com.");
        hosts.next(&mut ctx).await.unwrap();
        assert!(ctx.response.is_none());
    }
}
