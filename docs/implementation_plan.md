# Add Statistics Support

## Goal

Add functionality to collect and store DNS server statistics:

1. Domain names queried.
2. IP addresses resolved.
3. Resolve request count per domain.
4. Cache hit count.
5. Latest resolve timestamp per domain.

## Proposed Changes

### [NEW] [src/statistics.rs](file:///Users/zhanghu/vpn/clean-dns/src/statistics.rs)

- Define `Statistics` struct.
- Fields:
  - `domains: HashMap<String, DomainStats>`
  - `ips: HashMap<IpAddr, usize>`
  - `cache_hits: usize`
- Define `DomainStats`:
  - `count: usize`
  - `last_resolved_at: DateTime<Utc>` (or `SystemTime`)

### [MODIFY] [src/plugins/mod.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/mod.rs)

- Import `Statistics`.
- Add `stats: Arc<RwLock<Statistics>>` to `Context` struct.
- Update `Context::new` signature.

### [MODIFY] [Cargo.toml](file:///Users/zhanghu/vpn/clean-dns/Cargo.toml)

- Add `axum = "0.7"`
- Add `serde_json = "1.0"`

### [NEW] [src/api.rs](file:///Users/zhanghu/vpn/clean-dns/src/api.rs)

- Define `pub async fn start_api_server(stats: Arc<RwLock<Statistics>>, port: u16) -> Result<()>`
- Setup Axum router with route `GET /metrics` or `GET /stats`.
- Handler `get_stats` returns JSON representation of statistics.

### [MODIFY] [src/statistics.rs](file:///Users/zhanghu/vpn/clean-dns/src/statistics.rs)

- Derive `Serialize` for `Statistics` and `DomainStats`.

### [MODIFY] [src/server.rs](file:///Users/zhanghu/vpn/clean-dns/src/server.rs)

- Initialize `statistics: Arc<RwLock<Statistics>>` in `Server::new`.
- Pass `statistics` to `Context::new`.
- IN `handle_request`:
  - **Pre-processing**: Extract domain from `request`, update `domains` map (count++, timestamp).
  - **Post-processing**: Inspection `ctx.response`. If present, extract answer IPs and update `ips` map.

### [MODIFY] [src/plugins/cache.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/cache.rs)

- IN `next`:
  - On cache hit, access `ctx.stats` and increment `cache_hits`.

### [MODIFY] [src/config.rs](file:///Users/zhanghu/vpn/clean-dns/src/config.rs)

- Add `api_port: Option<u16>` to `Config` struct (default to e.g., 3000 if not set, or disable if None - let's default to a port or make it optional but recommended).
- Let's assume `api_port` in config.

### [MODIFY] [src/main.rs](file:///Users/zhanghu/vpn/clean-dns/src/main.rs)

- Spawn API server task if configured.

### Accessing Statistics

- For now, I will add a simple method to `Statistics` to dump data or just log it. The user didn't ask for an API endpoint API, but I might add a `Debug` print or a simple logical hook.
- **Self-correction**: To make it verifiable, I should probably add a way to dump it. Maybe just a simple log on shutdown or a method that can be called.
- Since the user task is "add Statistics support", simply collecting them in memory is the core. I'll add a helper to print them?
- Actually, the user might want to see them. I'll add a `print_stats` method or similar, but without an HTTP server, accessing it is hard.
- I will just implement the collection logic as requested. Creating a new API endpoint might be out of scope unless I see an API server existing.
- Checking `src/main.rs` might reveal if there's an HTTP server.

## Verification Plan

### Manual Verification

1.  **Run Server**: Start `clean-dns`.
2.  **Generate Load**: Use `dig` to query domains.
3.  **Query API**: `curl http://127.0.0.1:3000/stats`
4.  **Check Output**: Verify JSON output contains expected domains, counts, IPs, and cache hits. {
    "domains": { "google.com": { "count": 1, "last_resolved_at": "..." } },
    "ips": { "142.250.x.x": 1 },
    "cache_hits": 0
    }

### Automated Tests

- I can create a unit test in `server.rs` or `statistics.rs`?
- Since `Server` binds to a port, integration test is better.
- I will modify `main.rs` or create a test binary that runs the server and queries it, then checks the stats (if I expose them).
- **Better**: Unit test `Statistics` logic.
- **Better**: Integration test in `server.rs`? `Server::handle_request` is async. I can mock the socket? No, it takes `UdpSocket`.
- I'll rely on manual verification with logs for this feature as it involves runtime state.

I'll query `main.rs` to see how args are parsed and if I can add a flag to dump stats, or if there is an existing control channel.
