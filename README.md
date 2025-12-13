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

## Installation

Ensure you have Rust installed.

```bash
git clone https://github.com/your-username/clean-dns.git
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

  # 3. Logic Layer
  - tag: routing
    type: if
    args:
      if: "match_proxy_domains" # Needs a matcher plugin
      exec: [forward_proxy, stop]
      else_exec: []

  - tag: connection_logic
    type: sequence
    args:
      exec: [routing, forward_local]

  # 4. Entry Point (Cache -> Logic)
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
