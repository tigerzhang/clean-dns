# CleanDNS Implementation Tasks

- [x] **Project Initialization**

  - [x] Create Rust project
  - [x] Define `Plugin` trait
  - [x] Implement `Sequence` plugin
  - [x] Implement `Forward` plugin
  - [x] Basic Server implementation
  - [x] Verify with `dig`

- [x] **Core Plugins Implementation**

  - [x] **Matcher**: Implement simple domain/IP matching
  - [x] **Cache**: Implement in-memory DNS cache
  - [ ] **Fallback**: Implement primary/secondary upstream logic
  - [x] **Hosts**: Implement local hosts file support
  - [ ] **Query Matcher**: Match query type (A, AAAA, etc.)

- [x] **Configuration & Loading**

  - [x] Implement dynamic plugin loading from `config.yaml`
  - [x] Support arguments for plugins (e.g., upstream addr, files)

- [ ] **Advanced Features (Optional)**
  - [ ] **IPSet/DomainSet**: Optimized matching
  - [ ] **ECS**: EDNS Client Subnet support
