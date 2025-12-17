# Executable Plugins Implementation Plan

We will implement `if` and `return` plugins to mirror MosDNS's `executable` package capabilities.

## Proposed Changes

### [MODIFY] [src/plugins/mod.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/mod.rs)

- Update `Context`: Add `pub abort: bool`.
- Define `Condition` trait:
  ```rust
  pub trait Condition: Send + Sync {
      fn check(&self, ctx: &Context) -> bool;
  }
  ```
- Update `Plugin` trait:
  ```rust
  fn as_condition(&self) -> Option<&dyn Condition> { None }
  ```

### [MODIFY] [src/plugins/sequence.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/sequence.rs)

- Update loop to check `ctx.abort`.

### [MODIFY] [src/plugins/matcher.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/matcher.rs)

- Implement `Condition` trait for `Matcher` (exposing `matches` logic).
- Note: `Matcher` can still run its own `exec` if configured, but when used as a condition, it just returns bool.

### [NEW] [src/plugins/return_plugin.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/return_plugin.rs)

- Sets `ctx.abort = true`.

### [NEW] [src/plugins/if_plugin.rs](file:///Users/zhanghu/vpn/clean-dns/src/plugins/if_plugin.rs)

- Config:
  ```yaml
  if: "provider:some_matcher"
  exec: [...]
  else_exec: [...]
  ```
- Logic:
  ```rust
  if cond.check(ctx) {
      exec.run(ctx)
  } else {
      else_exec.run(ctx)
  }
  ```

## Verification

- Create config with `if` plugin.
- Verify `return` stops execution.
