use super::{Context, DomainSet, Plugin};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use tracing::{info, warn};

#[derive(Deserialize)]
struct DomainSetConfig {
    files: Vec<String>,
}

pub struct DomainSetPlugin {
    domains: HashSet<String>,
}

impl DomainSetPlugin {
    pub fn new(config: Option<&serde_yaml::Value>) -> Result<Self> {
        let config: DomainSetConfig = if let Some(c) = config {
            serde_yaml::from_value(c.clone())?
        } else {
            return Err(anyhow::anyhow!("DomainSet requires config"));
        };

        let mut domains = HashSet::new();

        for path in config.files {
            if let Ok(file) = File::open(&path) {
                let reader = BufReader::new(file);
                for line in reader.lines() {
                    if let Ok(l) = line {
                        let l = l.trim();
                        if !l.is_empty() && !l.starts_with('#') {
                            domains.insert(l.to_string());
                        }
                    }
                }
                info!("Loaded domains from {}", path);
            } else {
                warn!("Failed to open domain file: {}", path);
            }
        }

        Ok(Self { domains })
    }
}

impl DomainSet for DomainSetPlugin {
    fn contains(&self, domain: &str) -> bool {
        // Simple exact or suffix match check
        // Ideally should use Aho-Corasick or a proper Tree
        if self.domains.contains(domain) {
            return true;
        }

        // Suffix check: very inefficient for now, but functional for small lists
        // "google.com" matches "www.google.com" if stored as "google.com"
        for d in &self.domains {
            if domain.ends_with(d) {
                // confirm it's a dot boundary
                let remainder = domain.len() - d.len();
                if remainder > 0 && domain.as_bytes()[remainder - 1] == b'.' {
                    return true;
                }
            }
        }
        false
    }
}

#[async_trait]
impl Plugin for DomainSetPlugin {
    fn name(&self) -> &str {
        "domain_set"
    }

    async fn next(&self, _ctx: &mut Context) -> Result<()> {
        // Data provider usually does nothing in the chain
        Ok(())
    }

    fn as_domain_set(&self) -> Option<&dyn DomainSet> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_domain_set_loading_and_matching() {
        // Create a temp file
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "example.com").unwrap();
        writeln!(file, "google.com").unwrap();
        // Suffix matches implicitly? No, logic says "ends_with(d)".
        // So "test.google.com" should match if "google.com" is in set.

        let path = file.path().to_str().unwrap().to_string();

        let yaml = format!(
            r#"
            files:
              - "{}"
            "#,
            path
        );
        let config: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();

        let plugin = DomainSetPlugin::new(Some(&config)).unwrap();

        assert!(plugin.contains("example.com"));
        assert!(plugin.contains("google.com"));
        assert!(plugin.contains("www.google.com")); // Suffix match
        assert!(!plugin.contains("yahoo.com"));
    }
}
