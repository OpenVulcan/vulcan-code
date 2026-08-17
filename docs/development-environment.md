# 开发环境初始化与 Cargo 并发策略

## 新环境初始化

在已经具备 `make`、Rustup stable、PowerShell 7 以及 Node.js/pnpm 的环境中，仓库依赖统一通过一个入口初始化：

```bash
make init-system
```

该命令按固定顺序执行：

1. 以 Cargo 并发度 2 安装缺失的 `cargo-hakari` 与 `cargo-nextest`，并将低于 `0.9.22` 的 `cargo-hakari` 更新到兼容版本。
2. 安装宿主依赖与内建工具。
3. 安装 LuaSkills、Codebase Memory 和托管 VMM 运行时依赖。

已满足版本要求的工具会被保留，不会重复安装。

Rust 工具已存在时会只打印版本并跳过安装，不会重复编译或覆盖现有工具。Windows 通过 `make.ps1` 执行，类 Unix 环境通过 `make.sh` 转发到同一套 PowerShell 依赖入口。

单独安装 Rust 工作区工具可以执行：

```bash
make deps-rust-tools
```

## workspace-hack 依赖去重

仓库使用 `cargo-hakari` 生成根目录下的 `workspace-hack` crate，并将统一后的依赖特性集中到该 crate，减少多 crate 工作区中的重复依赖编译。相关配置位于 `.config/hakari.toml`，生成结果位于 `workspace-hack/`。

依赖声明或 feature 变化后，使用以下命令检查并刷新生成结果：

```bash
cargo hakari generate
cargo hakari manage-deps
cargo hakari verify
```

`workspace-hack` 只能合并兼容的同版本依赖特性；不同主版本或无法安全统一的依赖仍会保留为多个版本。它不能替代 Cargo.lock，也不会把语义上不同的依赖强行改成同一版本。

## 默认资源限制

根目录 `.cargo/config.toml` 设置 `build.jobs = 4`，限制 Cargo 编译进程并发；同时以 `RUST_TEST_THREADS = 4` 约束常规测试 harness。`.config/nextest.toml` 将 nextest 默认测试线程限制为 4。

临时调整时可以显式覆盖：

```bash
cargo test --jobs 2 -- --test-threads=1
cargo nextest run --test-threads=1
```

涉及 Provider Manifest、服务内核注册表、端口或其他进程级共享资源的测试必须串行执行。工作区全量测试可能同时启动大量测试进程，应先执行目标 crate 的 `cargo check` 或定向测试。`cargo nextest` 不覆盖 doctest，文档测试需要额外执行：

```bash
cargo test --doc
```

如果确实需要临时提高并发度，应在任务记录中说明原因，并确认不会影响开发机响应性或共享资源测试的稳定性。
