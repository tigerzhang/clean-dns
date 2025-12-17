# Plugin Comparison and Analysis

## Overview

The `clean-dns` project has successfully implemented the core architectural pillars of `mosdns`, including traffic control, data matching, and resolution. However, several advanced modifier and resilience plugins are currently missing.

## Plugin Comparison Table

| Category | Plugin Function | MosDNS Plugin | Clean-DNS Implementation | Status |
| :--- | :--- | :--- | :--- | :--- |
| **Logic** | Execute in order | `sequence` | `sequence` | ✅ Implemented |
| | Conditional Branching | `if` / `switch` | `if_plugin` | ✅ Implemented |
| | Stop Execution | `return` / `accept` | `return_plugin` | ✅ Implemented |
| | Reject Query | `reject` / `blackhole` | `reject_plugin` | ✅ Implemented |
| | Delay Response | `delay` | `delay_plugin` | ✅ Implemented |
| | Jump to Sequence | `goto` | *Missing* | ❌ |
| **Matching** | Match Domain Lists | `domain_set` | `domain_set` | ✅ Implemented |
| | Match IP Lists | `ip_set` | `ip_set` | ✅ Implemented |
| | Match Query/Resp | `matcher` | `matcher` | ✅ Implemented |
| **Resolution** | Upstream Forwarding | `forward` | `forward` | ✅ Implemented |
| | Caching | `cache` | `cache` | ✅ Implemented |
| | Local Hosts | `hosts` | `hosts` | ✅ Implemented |
| | Fallback / Failover | `fallback` | *Missing* | ❌ |
| **Modifiers** | Modify TTL | `ttl` | *Missing* | ❌ |
| | EDNS Client Subnet | `ecs` | *Missing* | ❌ |
| | Custom Records | `arbitrary` | *Missing* | ❌ |
| | Reverse Lookup | `reverse_lookup` | *Missing* | ❌ |

## Missing Plugins Analysis

To bring `clean-dns` closer to feature parity with `mosdns`, the following plugins are recommended for implementation:

### 1. TTL Plugin (`ttl`)
*   **Function:** Modifies the Time-To-Live (TTL) of records in the response.
*   **Use Case:**
    *   **Force Cache:** Increase TTL for domains that rarely change to reduce upstream queries.
    *   **Shorten Cache:** Decrease TTL for dynamic domains (like DDNS) to ensure freshness.
*   **Implementation Idea:** A plugin that parses the response `Message`, iterates over `answers`, and overwrites the `ttl` field based on a configured min/max range.

### 2. ECS Plugin (`ecs`)
*   **Function:** Handles EDNS Client Subnet (ECS) data.
*   **Use Case:**
    *   **Privacy:** Strip ECS data from client queries before forwarding to public resolvers to hide client IP.
    *   **Optimization:** Add a specific subnet when querying a CDN-friendly DNS to get better geo-located results.
*   **Implementation Idea:** Manipulate the `extensions` field in the DNS `Message` to add, remove, or modify the `EDNS` option.

### 3. Fallback Plugin (`fallback`)
*   **Function:** A robust version of `forward` that tries a primary upstream group and switches to a secondary group only if the primary fails or returns a specific error (like SERVFAIL).
*   **Use Case:** High availability. Try local ISP DNS first; if it fails or is hijacked, fallback to DoH/DoT.
*   **Implementation Idea:** Wraps two other plugins (primary and secondary). It runs primary; if it returns an error, it runs secondary.

### 4. Arbitrary Plugin (`arbitrary`)
*   **Function:** Returns a static, custom DNS response configured in the config file.
*   **Use Case:**
    *   Returning a specific IP for a blocked domain (instead of NXDOMAIN).
    *   Returning a TXT record for verification purposes without modifying the hosts file.

## Recommendation

Prioritize the **`ttl`** and **`fallback`** plugins next.
*   **`ttl`** provides immediate value for cache control.
*   **`fallback`** significantly improves the reliability of the DNS service.