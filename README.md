# CleanDNS

CleanDNS is a modular, high-performance DNS forwarder and router written in Rust. Inspired by MosDNS, it provides a flexible plugin architecture to customize DNS request processing with ease.

## Features

- **Modular Plugin System**: Everything is a plugin (Forwarding, Matching, Caching, etc.).
- **Flexible Routing**: Route DNS queries based on Domains, IPs, or custom logic.
- **Split-Horizon DNS**: Route foreign domains to proxies and local domains to local resolvers.
- **Multiple Upstreams**:
  - **UDP**: Standard DNS forwarding.
  - **DoH (DNS over HTTPS)**: Secure DNS queries.
  - **Race Strategy**: Query multiple upstreams concurrently; fastest wins.
- **Proxy Support**: Full SOCKS5 support for both UDP and DoH upstream queries.
- **Logic Control**: `If`, `Sequence`, `Fallback`, `Return`, `Reject`, `Delay` plugins to build complex query pipelines.
- **Efficient Matching**: Fast domain and IP matching using text files (`domain_set`, `ip_set`).
- **Caching & TTL**: In-memory caching and TTL modification support.
- **Static Hosts**: Local hosts file support.
- **Statistics API**: Built-in HTTP API to monitor query counts, cache hits, and resolved IPs.

## Installation

Ensure you have Rust installed.

```bash
git clone https://github.com/tigerzhang/clean-dns.git
cd clean-dns
cargo build --release
```

The binary will be available at `target/release/clean-dns`.

## Usage

Run CleanDNS with a configuration file:

```bash
clean-dns -c config.yaml
```

## Configuration

The configuration is YAML-based. You define a list of **plugins** and an **entry** point.

### Example: Split Routing (Proxy + Local)

See `config.yaml` for a full example that routes Google/GitHub via a SOCKS5 proxy (DoH) and everything else to a local provider.

```yaml
bind: "127.0.0.1:5335"
api_port: 3002 # Optional: Port for the Statistics API (default: 3000)
entry: main
plugins:
  # 1. Define Data Sources
  - tag: proxy_list
    type: domain_set
    args:
      files: ["proxy_domains.txt"]

  # 2. Define Actions
  - tag: forward_proxy
    type: forward
    args:
      upstreams: ["https://8.8.8.8/dns-query"]
      socks5: "127.0.0.1:1080"

  - tag: forward_local
    type: forward
    args:
      upstreams: ["223.5.5.5:53"]

  - tag: forward_backup
    type: forward
    args:
      upstreams: ["1.1.1.1:53"]

  - tag: stop
    type: return
    args: {}

  # 3. Matchers & Helpers
  - tag: match_proxy_domains
    type: matcher
    args:
      domain: ["provider:proxy_list"]

  - tag: fallback_group
    type: fallback
    args:
      primary: [forward_local]
      secondary: [forward_backup]

  - tag: ttl_fix
    type: ttl
    args:
      min: 300
      max: 3600

  # 4. Logic Layer
  - tag: routing
    type: if
    args:
      if: "match_proxy_domains"
      exec: [forward_proxy, stop]
      else_exec: []

  - tag: connection_logic
    type: sequence
    args:
      exec: [routing, fallback_group, ttl_fix]

  # 5. Entry Point (Cache -> Logic)
  - tag: main
    type: cache
    args:
      size: 4096
      exec: [connection_logic]
```

### Supported Plugins

| Type         | Description                              | Args                                                    |
| ------------ | ---------------------------------------- | ------------------------------------------------------- |
| `forward`    | Forwards queries to upstream.            | `upstreams` (list), `concurrent` (int), `socks5` (addr) |
| `sequence`   | Executes a list of plugins in order.     | `exec` (list of tags)                                   |
| `if`         | Conditional execution.                   | `if` (matcher tag), `exec` (list), `else_exec` (list)   |
| `matcher`    | Returns true if query matches criteria.  | `domain` (list), `client_ip` (list)                     |
| `domain_set` | Loads domains from files.                | `files` (list)                                          |
| `ip_set`     | Loads IPs/CIDRs from files.              | `files` (list)                                          |
| `cache`      | Caches responses.                        | `size` (int), `exec` (list)                             |
| `hosts`      | Static DNS records.                      | `hosts` (map)                                           |
| `reject`     | Rejects the query.                       | `rcode` (int)                                           |
| `delay`      | Delays execution (debug/testing).        | `ms` (int)                                              |
| `return`     | Stops execution in the current sequence. | -                                                       |
| `fallback`   | Fallback to secondary if primary fails.  | `primary` (list), `secondary` (list)                    |
| `ttl`        | Modifies response TTL.                   | `min` (int), `max` (int)                                |

## License

MIT

## Statistics API

CleanDNS includes a built-in HTTP API to view runtime statistics.
By default, it listens on port `3000` (configurable via `api_port` in `config.yaml`).

### Endpoint: `GET /stats`

Returns a JSON object containing usage statistics per domain.

**Response Example:**

```json
{
  "domains": {
    "google.com.": {
      "count": 12,
      "last_resolved_at": "2023-10-27T10:00:00Z",
      "ips": ["142.250.1.100", "142.250.1.101"],
      "cache_hits": 5
    },
    "github.com.": {
      "count": 3,
      "last_resolved_at": "2023-10-27T10:05:00Z",
      "ips": ["140.82.112.4"],
      "cache_hits": 0
    }
  }
}
```
