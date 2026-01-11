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

CleanDNS now supports multiple commands:

### Start Server (Default)

```bash
clean-dns run -c config.yaml
# Or simply:
clean-dns -c config.yaml
```

### Generate Geosite Data

To use the `geosite` plugin, you must compile v2fly community data into a `geosite.dat` file:

```bash
# Ensure submodules are initialized
git submodule update --init --recursive

# Compile the data
clean-dns make-geosite -s data/domain-list-community/data -o geosite.dat
```

## Configuration

The configuration is YAML-based. You define a list of **plugins** and an **entry** point.

### Example: Split Routing (Proxy + Local)

See `config.yaml` for a full example that routes Google/GitHub via a SOCKS5 proxy (DoH) and everything else to a local provider.

```yaml
bind: "127.0.0.1:53"
api_port: 3002
entry: main
log:
  level: info

plugins:
  # --- Data Providers ---
  - tag: proxy_list
    type: domain_set
    args:
      files:
        - "proxy_domains.txt"

  - tag: cn_list
    type: geosite
    args:
      file: "geosite.dat"
      code: "cn"

  - tag: private_list
    type: geosite
    args:
      file: "geosite.dat"
      code: "private"

  # --- Actions ---
  # 1. Proxy Forwarder (DoH over SOCKS5)
  - tag: forward_proxy
    type: forward
    args:
      upstreams:
        - "https://8.8.8.8/dns-query"
      socks5: "127.0.0.1:1080" # Tunnel DoH through local proxy

  # 2. Default Forwarder (AliDNS for speed in CN)
  - tag: forward_local
    type: forward
    args:
      upstreams:
        - "223.5.5.5:53"

  # Backup Forwarder (Cloudflare)
  - tag: forward_backup
    type: forward
    args:
      upstreams:
        - "114.114.114.114:53"

  # System Resolver (Uses host's default DNS)
  - tag: forward_system
    type: system
    args: {}

  # 3. Control Flow
  - tag: stop
    type: return
    args: {}

  # --- Matchers ---
  - tag: match_proxy_domains
    type: matcher
    args:
      domain:
        - "provider:proxy_list"

  - tag: match_direct_domains
    type: matcher
    args:
      domain:
        - "provider:cn_list"
        - "provider:private_list"

  - tag: fallback_group
    type: fallback
    args:
      primary: forward_system # Use system default DNS first
      secondary: forward_local # Fallback to AliDNS if system fails

  - tag: ttl_fix
    type: ttl
    args:
      min: 300
      max: 600

  # --- Logic ---
  - tag: routing_logic
    type: if
    args:
      if: "match_direct_domains"
      exec:
        - forward_local
        - stop
      else_exec:
        - forward_proxy
        - stop

  # --- Main Sequence ---
  - tag: main_sequence
    type: sequence
    args:
      exec:
        - routing_logic # 1. Check if needs proxy
        - fallback_group # 2. Fallback to local DNS (with backup)
        - ttl_fix

  # --- Entry Point (Cache) ---
  - tag: main
    type: cache
    args:
      size: 10000
      exec:
        - main_sequence
```

### Supported Plugins

| Type         | Description                              | Args                                                    |
| ------------ | ---------------------------------------- | ------------------------------------------------------- |
| `forward`    | Forwards queries to upstream.            | `upstreams` (list), `concurrent` (int), `socks5` (addr) |
| `sequence`   | Executes a list of plugins in order.     | `exec` (list of tags)                                   |
| `if`         | Conditional execution.                   | `if` (matcher tag), `exec` (list), `else_exec` (list)   |
| `matcher`    | Returns true if query matches criteria.  | `domain` (list), `client_ip` (list)                     |
| `domain_set` | Loads domains from files.                | `files` (list)                                          |
| `geosite`    | Loads domains from geosite.dat.          | `file` (path), `code` (str)                             |
| `ip_set`     | Loads IPs/CIDRs from files.              | `files` (list)                                          |
| `cache`      | Caches responses.                        | `size` (int), `exec` (list)                             |
| `hosts`      | Static DNS records.                      | `hosts` (map)                                           |
| `reject`     | Rejects the query.                       | `rcode` (int)                                           |
| `delay`      | Delays execution (debug/testing).        | `ms` (int)                                              |
| `return`     | Stops execution in the current sequence. | -                                                       |
| `fallback`   | Fallback to secondary if primary fails.  | `primary` (list), `secondary` (list)                    |
| `ttl`        | Modifies response TTL.                   | `min` (int), `max` (int)                                |
| `system`     | Uses the host's default DNS resolver.    | -                                                       |

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
      "last_resolved_remote": true,
      "ips": ["142.250.1.100", "142.250.1.101"],
      "cache_hits": 5
    },
    "github.com.": {
      "count": 3,
      "last_resolved_at": "2023-10-27T10:05:00Z",
      "last_resolved_remote": true,
      "ips": ["140.82.112.4"],
      "cache_hits": 0
    }
  }
}
```
