# Walkthrough - Statistics Support

I have implemented statistics collection and an API endpoint for the DNS server.

## Changes

### 1. Data Structure (`src/statistics.rs`)

Added `Statistics` struct to track:

- `domains`: Map of domain names to `DomainStats`.
- `DomainStats`:
  - `count`: Number of queries.
  - `last_resolved_at`: Timestamp of last query.
  - `ips`: Set of resolved IP addresses for the domain.
  - `cache_hits`: Number of cache hits for the domain.

### 2. API Endpoint (`src/api.rs`)

- Implemented an Axum-based HTTP server.
- Endpoint: `GET /stats` returns the current statistics in JSON format.
- Port is configurable via `config.yaml` (default `3000`).

### 3. Integration

- **Server (`src/server.rs`)**:
  - Initializes shared statistics.
  - Records incoming requests (domain, count, timestamp).
  - Records resolved IPs from responses.
- **Cache Plugin (`src/plugins/cache.rs`)**:
  - Records cache hits for specific domains.
- **Context (`src/plugins/mod.rs`)**:
  - Passes shared statistics to plugins.

## Verification Results

### Test Run

Ran `verify.sh` which:

1. Starts the server on port `5336` (API on `3002`).
2. Digs `apple.com` (Miss -> Cache).
3. Digs `apple.com` again (Hit).
4. Fetches `/stats`.

### Output

```json
{
  "domains": {
    "apple.com.": {
      "count": 2,
      "last_resolved_at": "...",
      "ips": ["17.253.144.10"],
      "cache_hits": 1
    }
  }
}
```

This confirms all requirements:

1. Domain names included.
2. IPs resolved included (per domain).
3. Request count included.
4. Cache hit count included.
5. Latest timestamp included.

## Automated Testing

Comprehensive unit and integration tests have been implemented to ensure stability.

### Unit Tests

- `src/statistics.rs`: Verifies data logic (inserts, updates, concurrency).
- `src/plugins/*`: Unit tests for all plugins (Cache, Matcher, Forward, Sequence, etc.) covering config parsing and core logic.

### Integration Tests

- `tests/integration_test.rs`:
  - **Server + Stats**: Verifies that a real DNS query via UDP updates the shared statistics and is served correctly.
  - **API Stats**: Verifies that the HTTP API endpoint (`GET /stats`) returns valid JSON with the expected structure.

### Running Tests

To run all tests:

```bash
cargo test
```

Result: All 21 tests passed (19 unit, 2 integration).
