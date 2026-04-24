# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

BeeBotOS is a Web4.0 autonomous agent operating system built in Rust. It uses a 5-layer architecture (Blockchain -> Kernel -> Social Brain -> Agent Layer -> Applications) with WASM sandboxing, capability-based security, and blockchain integration.

This is a Cargo workspace with 11 core crates and 4 applications. The project uses nightly Rust (see `rust-toolchain.toml`).

## Common Commands

### Build
```bash
cargo build --workspace --release    # Release build
cargo build --workspace              # Debug build
cargo build -p beebotos-kernel       # Build specific crate
cargo build -p beebotos-gateway      # Build specific app
```

### Test
```bash
cargo test --workspace --all-features              # All tests
cargo test --workspace --lib                       # Unit tests only
cargo test --workspace --test '*'                  # Integration tests only
cargo test -p beebotos-kernel                      # Specific crate tests
cargo test test_name -- --nocapture                # Single test with output
cargo bench --workspace                            # Benchmarks
```

### Code Quality
```bash
cargo fmt --all                                    # Format all code
cargo fmt --all -- --check                         # Check formatting (CI)
cargo clippy --workspace --all-targets --all-features -- -D warnings  # Lint
cargo deny check                                   # Dependency/license check
cargo audit                                        # Security audit
cargo doc --workspace --no-deps                    # Generate docs
```

### Smart Contracts (Foundry)
```bash
cd contracts && forge build                        # Build Solidity contracts
cd contracts && forge test                         # Run contract tests
cd contracts && forge fmt                          # Format contracts
```

### Alternative: Using `just` or `make`
Both `justfile` and `Makefile` are provided with common recipes:

**`just` recipes:**
- `just build` - Release build
- `just debug` - Debug build
- `just test` - Run all tests
- `just test-filter <PATTERN>` - Run tests matching pattern
- `just check` - Full check (fmt + lint + test)
- `just lint` - Run clippy
- `just fmt` - Format code
- `just dev` - Watch mode with cargo-watch
- `just install` - Install CLI binary locally (`beebot`)
- `just contract-build` / `just contract-test` - Build/test contracts
- `just clean` - Clean build artifacts
- `just coverage` - Generate test coverage report (tarpaulin)
- `just setup` - Run dev environment setup script

**`make` recipes:**
- `make build` - Release build
- `make debug` - Debug build
- `make test` - Run all tests
- `make test-unit` - Unit tests only
- `make test-integration` - Integration tests only
- `make check` - Full check (fmt + lint + test)
- `make lint` - Run clippy
- `make fmt` - Format code
- `make dev` - Watch mode with cargo-watch
- `make install` / `make uninstall` - Install/remove CLI binary
- `make contracts-build` / `make contracts-test` - Build/test contracts
- `make coverage` - Generate test coverage report
- `make setup` - Run dev environment setup script

## Workspace Structure

### Crates (in `crates/`)
| Crate | Purpose | Key Dependencies |
|-------|---------|-----------------|
| `core` | Shared types, errors, event bus | `alloy-primitives`, `message-bus` |
| `kernel` | OS kernel: scheduler, security, WASM runtime, syscalls | `core`, `wasmtime`, `message-bus` |
| `brain` | Neural networks and cognitive models | `core` |
| `agents` | Agent runtime, A2A protocol, MCP, planning | `core`, `kernel`, `chain`, `gateway-lib`, `message-bus` |
| `chain` | Blockchain integration (multi-chain wallet) | `core`, `agents`, `message-bus` |
| `crypto` | Cryptographic utilities | `core` |
| `p2p` | Peer-to-peer networking | `core` |
| `sdk` | Developer SDK | - |
| `telemetry` | Observability, metrics, logging | - |
| `gateway-lib` | Shared infrastructure for gateway | `core` |
| `message-bus` | Inter-crate message bus | - |

### Applications (in `apps/`)
- `gateway` - API gateway service (Axum, port 8000)
- `web` - Web management UI (port 8090)
- `cli` - Command-line tool (`beebot` binary)
- `beehub` - Hub service

### Other Key Directories
- `contracts/` - Solidity smart contracts (Foundry project)
- `proto/` - Protocol Buffer definitions (a2a, agent, brain, kernel, etc.)
- `tests/` - Tests organized by scope:
  - `tests/unit/` - Unit tests organized by crate (`agents/`, `brain/`, `kernel/`)
  - `tests/integration/` - Integration tests (`agent_integration.rs`, `kernel_integration.rs`, etc.)
  - `tests/e2e/` - End-to-end tests (`agent_lifecycle.rs`, `a2a_protocol.rs`, etc.)
- `config/` - Configuration files (TOML)
- `skills/` - Skill definitions
- `docs/` - Documentation

## Architecture Rules

### Critical: `crates/agents` HTTP Framework Ban
`crates/agents` **must NOT** directly depend on any web framework. All HTTP-related functionality goes through `beebotos-gateway-lib`.

**Forbidden dependencies in `crates/agents`:**
- `axum`, `actix-web`, `rocket`, `warp`, `tide`, `salvo`

The dependency graph enforces this:
```
apps/gateway -> crates/gateway-lib -> crates/core
                      ↓
            crates/agents, kernel, chain, etc.
```

### Crate Dependency Direction
- `core` is the foundation - all crates depend on it
- `gateway-lib` provides shared infrastructure above `core` and is the only crate that directly depends on `axum`
- `agents` depends on `kernel`, `chain`, and `gateway-lib`
- `chain` depends on `agents` (circular dependency note: this exists in current code)
- Apps depend on crates but crates never depend on apps
- `agents` must not add `wasmtime` directly — use `kernel::wasm` interfaces instead

## Code Standards

### Rust Formatting (`rustfmt.toml`)
- `max_width = 100`
- 4-space indentation
- `imports_granularity = "Module"`
- `group_imports = "StdExternalCrate"`

### Clippy (`clippy.toml`)
- Cognitive complexity threshold: 30
- Max arguments: 7
- Max function lines: 100
- Type complexity threshold: 300

### Commits
Use Conventional Commits: `type(scope): subject`
Types: `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `chore`

### Git Hooks
Lefthook is configured (see `lefthook.yml`):
- **pre-commit**: `cargo fmt -- --check`, `cargo clippy -- -D warnings`, `cargo test --lib`
- **pre-push**: `cargo test --workspace`, `cargo fmt --all -- --check`
- **commit-msg**: `commitlint`

Note: pre-commit hooks run without `--workspace` and only match `*.rs` files, so they validate the affected crate rather than the full workspace.

## Configuration

### Environment Variables
Sensitive config uses `BEE__{SECTION}__{KEY}` format (maps to TOML hierarchy via config crate):
```bash
BEE__JWT__SECRET=...
BEE__MODELS__KIMI__API_KEY=...
BEE__CHANNELS__LARK__APP_SECRET=...
```

### Fixed Ports
- Gateway API: `8000`
- Web UI: `8090`

### Main Config Files
- `config/beebotos.toml` - Main gateway configuration (server, DB, JWT, rate limit, models, channels)
- `config/web-server.toml` - Web server configuration
- `.env` / `.env.example` - Environment variables (sensitive)

## Testing Strategy

### Coverage Requirements
| Module | Minimum | Target |
|--------|---------|--------|
| `kernel` | 85% | 90% |
| `brain` | 80% | 85% |
| `agents` | 80% | 85% |
| `chain` | 75% | 80% |
| `contracts` | 90% | 95% |

### Test Organization
- Unit tests: In source files (`#[cfg(test)]` modules)
- Integration tests: `tests/integration/`
- E2E tests: `tests/e2e/`

## Important Implementation Details

### Kernel (`crates/kernel`)
- Uses `wasmtime` for WASM runtime (enabled by default via `wasm` feature; `wasmtime` is an optional dependency)
- Scheduler supports work-stealing and priority-based scheduling
- 11 capability levels for security (`CapabilityLevel` enum)
- TEE support available via `security::tee` module
- Storage backends: RocksDB, redb, SQLite (feature-gated)

### Agents (`crates/agents`)
- Service mesh pattern with DID resolver integration
- Planning engine with multiple strategies (ChainOfThought, ReAct, Hybrid)
- Device automation support (Android/iOS controllers)
- Channel system for multi-platform messaging (Lark, WeChat, Discord, Telegram, Slack)
- State manager for agent lifecycle tracking
- WASM execution must go through `kernel::wasm` interfaces; do not add `wasmtime` directly to this crate

### Message Bus
Unified message bus (`beebotos-message-bus`) used across crates for inter-module communication. Each crate has its own message bus wrapper (e.g., `KernelMessageBus`, `AgentsMessageBus`).

### Toolchain
Nightly Rust is required (see `rust-toolchain.toml`). Components: `rustfmt`, `clippy`. Targets include `wasm32-unknown-unknown` and Windows targets (`x86_64-pc-windows-gnu`, `x86_64-pc-windows-msvc`).
