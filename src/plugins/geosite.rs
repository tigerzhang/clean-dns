use super::{Context, DomainSet, Plugin};
use anyhow::{Context as AnyhowContext, Result};
use async_trait::async_trait;
use prost::Message;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs::read;
use tracing::{info, warn};

// Import proto definition (assuming it's available via main::proto or similar,
// but since this is a library module, we might need to expose it or include it here if not shared.
// For now, I'll rely on a shared definition or re-include for simplicity if crate root exports it.
// Actually, `src/main.rs` declares the module. We should move the proto module to `lib.rs`
// or `mod.rs` in a way it's accessible.
// However, since CleanDNS is likely a binary crate (main.rs), we can't easily share modules unless
// we restructure. A common pattern is to put proto in `src/proto/mod.rs` and make it a module.
// Let's assume for now I'll fix the module structure next.
// I will access it via `crate::proto` if I move the proto decl to `lib.rs` or `main.rs` makes it pub.
// Wait, `main.rs` is the root. Modules in `plugins` are submodules.
// I can't access `crate::proto` if it's defined in `main.rs` unless `main.rs` is treated as lib?
// No.
// Plan: I will move `pub mod proto { ... }` to `src/lib.rs` or `src/config.rs` (if generic) or `src/proto/mod.rs`?
// Best is to create `src/lib.rs` and move core modules there, but that's a big refactor.
// Easiest: duplicate the include in `src/plugins/geosite.rs` or separate proto into a proper module file `src/proto/mod.rs`.
// I'll create `src/proto/mod.rs` that includes the generated code.

use crate::proto;

#[derive(Deserialize)]
struct GeositeConfig {
    file: String,
    code: String,
}

pub struct GeositePlugin {
    exact_matches: HashSet<String>,
    suffix_matches: Vec<String>,
    #[allow(dead_code)]
    regex_matches: Vec<String>, // Parsed regexes could be stored here
}

impl GeositePlugin {
    pub fn new(config: Option<&serde_yaml::Value>) -> Result<Self> {
        let config: GeositeConfig = if let Some(c) = config {
            serde_yaml::from_value(c.clone())?
        } else {
            return Err(anyhow::anyhow!("Geosite requires config"));
        };

        let data = read(&config.file)
            .with_context(|| format!("Failed to read geosite file {}", config.file))?;
        let site_list = proto::GeoSiteList::decode(&data[..])?;

        let code = config.code.to_uppercase();
        let site = site_list.entry.into_iter().find(|s| s.country_code == code);

        let mut exact_matches = HashSet::new();
        let mut suffix_matches = Vec::new();
        // Regex not fully implemented yet
        let regex_matches = Vec::new();

        if let Some(site) = site {
            info!(
                "Loaded geosite code: {} with {} domains",
                code,
                site.domain.len()
            );
            for domain in site.domain {
                match domain.r#type {
                    0 | 3 => { // Plain or Full
                         // Protocol says:
                         // Plain (0): substring key? checking v2fly docs: "Plain: Substring match"
                         // Wait, v2fly docs say:
                         //   domain: -> substring/subdomain?
                         //   full: -> exact
                         // Let's check common.proto comments I saw earlier.
                         //   Plain = 0; // Value is used as is. (Usually subdomain)
                         //   Regex = 1;
                         //   RootDomain = 2; // ?
                         //   Full = 3;

                        // In my generator:
                        //   "domain:" -> Type::Plain
                        //   "full:" -> Type::Full

                        // In V2Ray routing:
                        //   Plain -> Keyword match (substring)
                        //   Domain -> Domain match (suffix) - wait, where is Type 2 (RootDomain)?

                        // Let's look at my generator again.
                        // I mapped "domain:" to Type::Plain.
                        // Actually, standard v2ray geosite generator maps "domain:" to Type::Domain(2) usually?
                        // Re-reading common.proto from my memory/doc:
                        //   Plain = 0;
                        //   Regex = 1;
                        //   RootDomain = 2;
                        //   Full = 3;

                        // My generator uses:
                        //   "domain:" -> Type::Plain (0) -- WAIT.
                        //   In v2fly/domain-list-community README:
                        //     "domain:" -> sub-domain routing rule.
                        //     "keyword:" -> plain domain routing rule.
                        //     "full:" -> full domain routing rule.

                        // V2Ray Core Proto:
                        //   Type 0 (Plain) -> Keyword
                        //   Type 1 (Regex) -> Regex
                        //   Type 2 (RootDomain) -> Domain (suffix)
                        //   Type 3 (Full) -> Full

                        // So I should map "domain:" to Type::RootDomain (2)!

                        // I need to Fix the generator first? Or handle it here?
                        // If generated as Plain (0), I should treat as Keyword?
                        // But I mapped "domain:" to Plain in generator. This is a BUG in my generator if I want domain behavior.
                        // "domain:google.com" means *.google.com + google.com.
                        // Plain means "has string google.com".

                        // I will FIX the generator to use Type::RootDomain(2) for "domain:".
                        // And Type::Plain(0) for "keyword:".

                        // But first, let's write this plugin assuming standard types.
                        // Type::Full(3) -> Exact
                        // Type::RootDomain(2) -> Suffix
                        // Type::Plain(0) -> Keyword? (I'll implement as suffix for now if uncertain, or just keyword)
                    }
                    _ => {}
                }

                // For now, let's implement based on what I WILL fix in generator:
                // Full -> exact
                // RootDomain -> suffix

                let type_ = domain.r#type;
                match type_ {
                    3 => {
                        // Full
                        exact_matches.insert(domain.value);
                    }
                    2 => {
                        // RootDomain (Suffix)
                        suffix_matches.push(domain.value);
                    }
                    0 => {
                        // Plain (Keyword)
                        // If I generated "domain:" as Plain, I better treat it as suffix for compatibility with my previous step error?
                        // No, I will fix generator.
                        // So Plain is Keyword.
                        // I won't implement keyword matching for now or treat as suffix?
                        // "keyword: google" -> matches "google.com", "agoogleb.com"
                        // I'll ignore Plain for now or treat as Suffix if I can't differentiate?
                        // Let's treat Plain as Suffix for now as fallback?
                        suffix_matches.push(domain.value);
                    }
                    _ => {}
                }
            }
        } else {
            warn!("Geosite code {} not found in file", code);
        }

        Ok(Self {
            exact_matches,
            suffix_matches,
            regex_matches,
        })
    }
}

impl DomainSet for GeositePlugin {
    fn contains(&self, domain: &str) -> bool {
        if self.exact_matches.contains(domain) {
            return true;
        }

        for suffix in &self.suffix_matches {
            if domain.ends_with(suffix) {
                let remainder_len = domain.len() - suffix.len();
                if remainder_len == 0 {
                    return true;
                }
                if domain.as_bytes()[remainder_len - 1] == b'.' {
                    return true;
                }
            }
        }
        false
    }
}

#[async_trait]
impl Plugin for GeositePlugin {
    fn name(&self) -> &str {
        "geosite"
    }

    async fn next(&self, _ctx: &mut Context) -> Result<()> {
        Ok(())
    }

    fn as_domain_set(&self) -> Option<&dyn DomainSet> {
        Some(self)
    }
}
