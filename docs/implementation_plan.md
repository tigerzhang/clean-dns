# Add Unit and Integration Tests

## Goal

Add comprehensive testing to ensure reliability of statistics and server functionality.

## Proposed Changes

### [REFAC] Refactor to Library Structure

To support `tests/` integration tests, we need to expose modules via `lib.rs`.

#### [NEW] [src/lib.rs](file:///Users/zhanghu/vpn/clean-dns/src/lib.rs)

- Expose modules: `api`, `config`, `plugins`, `server`, `statistics`.
- Re-export necessary types.

#### [MODIFY] [src/main.rs](file:///Users/zhanghu/vpn/clean-dns/src/main.rs)

- Reduce to a thin wrapper that calls `clean_dns::...`.
- Use `clean_dns::config::Config`, `clean_dns::server::Server`, etc.

### [NEW] Unit Tests

#### [MODIFY] [src/statistics.rs](file:///Users/zhanghu/vpn/clean-dns/src/statistics.rs)

- Add `#[cfg(test)] mod tests { ... }`.
- Test `record_request`: verify count and timestamp update.
- Test `record_resolved_ip`: verify IP set growth and uniqueness.
- Test `record_cache_hit`: verify count increment.

#### [MODIFY] [src/plugins/forward.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/forward.rs)

- Test config parsing (upstreams, socks5).

#### [MODIFY] [src/plugins/matcher.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/matcher.rs)

- Test domain matching (exact, suffix) and client IP matching.

#### [MODIFY] [src/plugins/cache.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/cache.rs)

- Test key generation.
- Test hit/miss logic.

#### [MODIFY] [src/plugins/sequence.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/sequence.rs)

- Test execution order of child plugins.

#### [MODIFY] [src/plugins/if_plugin.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/if_plugin.rs)

- Test condition true/false branches.

#### [MODIFY] [src/plugins/domain_set.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/domain_set.rs)

- Test loading from file (mock or temporary file).
- Test matching logic.

#### [MODIFY] [src/plugins/ip_set.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/ip_set.rs)

- Test parsing CIDRs.
- Test IP matching.

#### [MODIFY] [src/plugins/hosts.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/hosts.rs)

- Test host lookup and record generation.

#### [MODIFY] [src/plugins/reject_plugin.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/reject_plugin.rs)

- Test response code setting.

#### [MODIFY] [src/plugins/return_plugin.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/return_plugin.rs)

- Test abort flag setting.

#### [MODIFY] [src/plugins/delay_plugin.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/delay_plugin.rs)

- Test parsing.

#### [MODIFY] [src/plugins/fallback.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/fallback.rs)

- Test primary success.
- Test primary failure -> secondary execution.

#### [MODIFY] [src/plugins/ttl.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/ttl.rs)

- Test min/max TTL clamping on response.

### [NEW] Integration Tests

#### [NEW] [tests/integration.rs](file:///Users/zhanghu/vpn/clean-dns/tests/integration.rs)

- Test suite using `tokio::test`.
- Helper to start server on random/test ports.
- Test 1: **API Stats**:
  - Start server.
  - Call `GET /stats`.
  - Assert empty/initial stats.
- Test 2: **DNS Query + Stats**:
  - Start server.
  - Send DNS query (using `hickory-client` or raw socket).
  - Assert response is valid.
  - Call `GET /stats`.
  - Assert stats reflect the query (count=1, domain correct).

### [MODIFY] [Cargo.toml](file:///Users/zhanghu/vpn/clean-dns/Cargo.toml)

- Add `reqwest` to dev-dependencies (blocking or async, needed for API test).
- Add `tokio` with `test-util` feature? It has `full`.

## Verification Plan

### Automated Tests

- Run `cargo test`.
- Verify all unit and integration tests pass.
