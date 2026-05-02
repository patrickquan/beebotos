# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

BeeBotOS is a Web4.0 autonomous agent operating system built in Rust. It uses a 5-layer architecture (Blockchain -> Kernel -> Social Brain -> Agent Layer -> Applications) with WASM sandboxing, capability-based security, and blockchain integration.

This is a Cargo workspace with 11 core crates and 4 applications. The project uses nightly Rust (see `rust-toolchain.toml`).

## 开发工作流（Dev Workflow）

### 编译、启动、停止服务 — 必须使用脚本
**编译和启动任何服务时，禁止使用 `cargo build`、`wasm-pack` 等手动命令。** 必须使用项目提供的统一脚本：

| 平台 | 开发脚本（编译+启动+菜单） | 运行脚本（仅启动/停止） |
|------|---------------------------|------------------------|
| Windows | `beebotos-dev.ps1` | `beebotos-run.ps1` |
| Linux/macOS | `beebotos-dev.sh` | `beebotos-run.sh` |

**常用命令（Windows 示例）：**
```powershell
.\beebotos-dev.ps1 run gateway      # 编译并启动 gateway（最常用）
.\beebotos-dev.ps1 build gateway    # 仅编译 gateway
.\beebotos-dev.ps1 build web        # 编译 web 前端（含 wasm-pack）
.\beebotos-dev.ps1 start gateway    # 仅启动 gateway
.\beebotos-dev.ps1 stop gateway     # 停止 gateway
.\beebotos-dev.ps1 restart gateway  # 重启 gateway
.\beebotos-dev.ps1 status           # 查看所有服务状态
.\beebotos-dev.ps1 menu             # 交互式菜单
```

**常用命令（Linux/macOS 示例）：**
```bash
./beebotos-dev.sh run gateway
./beebotos-dev.sh build web
./beebotos-dev.sh start gateway
./beebotos-dev.sh stop gateway
./beebotos-dev.sh restart gateway
./beebotos-dev.sh status
```

支持的服务：`gateway`（8000）、`web`（8090）、`beehub`（8080）、`cli`（仅安装）

### 测试与代码质量（可直接使用 cargo）
```bash
cargo test --workspace --all-features              # 全部测试
cargo test --workspace --lib                       # 仅单元测试
cargo test test_name -- --nocapture                # 单个测试并输出
cargo fmt --all                                    # 格式化
cargo fmt --all -- --check                         # 检查格式（CI）
cargo clippy --workspace --all-targets --all-features -- -D warnings  # Lint
cargo deny check                                   # 依赖/许可证检查
cargo audit                                        # 安全审计
cargo doc --workspace --no-deps                    # 生成文档
cargo bench --workspace                            # 基准测试
```

### 智能合约（Foundry）
```bash
cd contracts && forge build                        # 编译 Solidity
cd contracts && forge test                         # 运行合约测试
cd contracts && forge fmt                          # 格式化合约
```

### 其他工具
- `just` / `make` — 便捷命令（`build`、`test`、`lint`、`fmt`、`setup` 等）
- `scripts/setup-dev.sh` — 开发环境初始化
- `lefthook` — Git hooks（pre-commit 自动运行 fmt + clippy + test）

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
  - `tests/fixtures/` - Shared test fixtures
  - Standalone integration tests also exist directly under `tests/` (e.g., `lark_message_test.rs`, `test_channel_registry.rs`, `test_config.rs`)
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
Sensitive config uses `BEE__{SECTION}__{KEY}` format — note the **double** underscore separator, which the `config` crate maps to TOML hierarchy:
```bash
BEE__JWT__SECRET=...                   # → [jwt] secret = ...
BEE__MODELS__KIMI__API_KEY=...         # → [models.kimi] api_key = ...
BEE__CHANNELS__LARK__APP_SECRET=...    # → [channels.lark] app_secret = ...
```
See `.env.example` for the full set of expected variables.

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

## 前端开发注意事项

### Bash/MSYS2 环境下编译 Web
在 bash（如 Git Bash、MSYS2）中，由于 `beebotos-dev.ps1` 的 shebang 是 `#!/usr/bin/env pwsh`，而系统通常只有 `powershell`（无 `pwsh`），直接运行 `./beebotos-dev.ps1` 会报错。正确调用方式：
```bash
powershell -File beebotos-dev.ps1 build web
```

### WebSocket 事件类型必须前后端对齐
后端发送的 WebSocket 事件 `state` 字段（如 `processing`）必须在前端 `ChatEventType` 枚举中有对应变体，否则 serde 反序列化失败，前端控制台报错且无法处理事件。

### Release 模式下的 WASM 闭包生命周期
前端 WebSocket 事件处理避免使用 `Closure::once` + `setTimeout(0)` + `forget()` 模式。在 release 编译优化后，该模式可能导致闭包在组件销毁后仍被执行，触发 Leptos reactive disposed panic。应改用 `wasm_bindgen_futures::spawn_local`：
```rust
// 不推荐
let closure = Closure::once(move || { state.handle_chat_event(event); });
window.set_timeout_with_callback_and_timeout_and_arguments_0(closure.as_ref().unchecked_ref(), 0).unwrap();
closure.forget();

// 推荐
wasm_bindgen_futures::spawn_local(async move {
    state.handle_chat_event(event);
});
```

### 打包后环境与开发环境差异
部分问题（如 reactive disposed panic）仅在 `wasm-pack build --release` 后的生产环境出现，开发环境（debug）可能正常。验证前端修复时，**必须**用 release 模式编译测试。

### Leptos 组件中 WASM 闭包必须配套清理
使用 `Closure::wrap` + `setInterval`/`setTimeout` + `forget()` 创建的 WASM 闭包，
在组件重新渲染（非卸载）后，旧闭包仍可能持有 disposed 的 Signal。
必须在组件卸载时通过 `on_cleanup` 清除定时器：
```rust
let interval_id = window.set_interval_with_callback_and_timeout_and_arguments_0(
    closure.as_ref().unchecked_ref(),
    50,
).unwrap();
on_cleanup(move || {
    window.clear_interval_with_handle(interval_id);
});
closure.forget();
```

## Related Documentation

- `AGENTS.md` — extended guide for AI coding assistants (deeper coding-style, NatSpec, deployment, security details). Read this if `CLAUDE.md` lacks the context you need.
- `readme.md` — project overview, 5-layer architecture diagram, quick start
- `CONTRIBUTING.md` — contribution workflow
- `contracts/STRUCTURE.md` — Solidity contract layout
- `tasks/todo.md` — current in-progress task list (the project follows the "plan → todo.md → verify" workflow)
