# CleanDNS Verification Walkthrough

## Verification Steps

1.  **Build**: ran `cargo check` and `cargo run`.
2.  **Server Start**: `clean-dns` started successfully on `127.0.0.1:5335`.
3.  **Data Provider Verification**:
    - Configured `domain_set` with `domain_list.txt` containing `youtube.com`.
    - Configured `matcher` to use `provider:youtube_list`.
    - **Match**: `youtube.com` resolved successfully (forwarded to 8.8.8.8).
    - **No Match**: `google.com` timed out.
4.  **IP Matching Verification**:
    - Configured `ip_set` with `ip_list.txt`.
    - Configured `matcher` to use `client_ip: ["provider:ip_tag"]`.
    - **Match**: When list contained `127.0.0.1`, query from localhost resolved.
    - **No Match**: When list contained `192.168.1.1`, query from localhost timed out.
5.  **Executable Plugins Verification**:
    - Configured `if` plugin: `if: match_youtube`, `exec: [stop]`, `else_exec: [google]`.
    - Configured `return` plugin (tag: `stop`).
    - **Condition Met**: `youtube.com` matched, executed `stop` (return). Query timed out (execution aborted).
    - **Condition Not Met**: `google.com` did not match, executed `google` (forward). Query resolved.

## Artifacts

- `clean-dns` binary (debug build).
- `config.yaml`.
- `domain_list.txt`.
- `ip_list.txt`.

## Implemented Plugins

- `sequence`: Chain execution.
- `forward`: Upstream forwarding.
- `matcher`: Domain & IP routing.
- `hosts`: Local static mapping.
- `cache`: In-memory TTL caching.
- `domain_set`: Domain list provider.
- `ip_set`: IP/CIDR list provider.
- `if`: Conditional execution.
- `return`: Abort execution.
