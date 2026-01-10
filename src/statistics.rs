use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;

#[derive(Debug, Default, Serialize, Clone)]
pub struct Statistics {
    pub domains: HashMap<String, DomainStats>,
}

#[derive(Debug, Serialize, Clone)]
pub struct DomainStats {
    pub count: usize,
    pub last_resolved_at: DateTime<Utc>,
    pub last_resolved_remote: bool,
    pub ips: HashSet<IpAddr>,
    pub cache_hits: usize,
}

impl Statistics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_request(&mut self, domain: String) {
        let entry = self.domains.entry(domain).or_insert(DomainStats {
            count: 0,
            last_resolved_at: Utc::now(),
            last_resolved_remote: false,
            ips: HashSet::new(),
            cache_hits: 0,
        });
        entry.count += 1;
        entry.last_resolved_at = Utc::now();
    }

    pub fn record_cache_hit(&mut self, domain: String) {
        if let Some(entry) = self.domains.get_mut(&domain) {
            entry.cache_hits += 1;
        } else {
            // Should not happen if request recorded first, but if cache hit happens before request record?
            // In current logic: request recorded first.
            // But strict cache hit might bypass request record if I change order?
            // In `server.rs`, `record_request` is called before `plugin.next`.
            // In `cache.rs`, `next` calls `record_cache_hit`.
            // So entry should exist.
            // Safety: insert if not exists (though resolved time might be slightly off, or just use now)
            let entry = self.domains.entry(domain).or_insert(DomainStats {
                count: 0, // Did we count it as request? Yes, explicit record_request called in server.
                last_resolved_at: Utc::now(),
                last_resolved_remote: false,
                ips: HashSet::new(),
                cache_hits: 0,
            });
            entry.cache_hits += 1;
        }
    }

    pub fn record_resolved_ip(&mut self, domain: &str, ip: IpAddr, is_remote: bool) {
        if let Some(entry) = self.domains.get_mut(domain) {
            entry.ips.insert(ip);
            entry.last_resolved_remote = is_remote;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_record_request() {
        let mut stats = Statistics::new();
        stats.record_request("example.com.".to_string());

        assert_eq!(stats.domains.get("example.com.").unwrap().count, 1);
        stats.record_request("example.com.".to_string());
        assert_eq!(stats.domains.get("example.com.").unwrap().count, 2);
    }

    #[test]
    fn test_record_cache_hit() {
        let mut stats = Statistics::new();
        // Must record request first to init entry
        stats.record_request("example.com.".to_string());
        stats.record_cache_hit("example.com.".to_string());

        assert_eq!(stats.domains.get("example.com.").unwrap().cache_hits, 1);
    }

    #[test]
    fn test_record_resolved_ip() {
        let mut stats = Statistics::new();
        stats.record_request("example.com.".to_string());

        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        // Test local resolution
        stats.record_resolved_ip("example.com.", ip, false);

        let entry = stats.domains.get("example.com.").unwrap();
        assert_eq!(entry.ips.len(), 1);
        assert!(entry.ips.contains(&ip));
        assert_eq!(entry.last_resolved_remote, false);

        // Duplicate IP should not increase count, but update remote status?
        // Logic says yes.
        stats.record_resolved_ip("example.com.", ip, true);

        let entry = stats.domains.get("example.com.").unwrap();
        assert_eq!(entry.ips.len(), 1);
        assert_eq!(entry.last_resolved_remote, true);
    }
}
