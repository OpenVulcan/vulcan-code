# 开发环境初始化与 Cargo 并发策略

## 固定 Rust 工具链

仓库根 `rust-toolchain.toml` 固定 Rust `1.97.1`，使用 `minimal` Profile 并安装 `clippy`、`rustfmt`。根工作区采用 Cargo Resolver 3，41 个成员全部通过 `rust-version.workspace = true` 继承同一 `1.97.1` 支持基线；本仓库不再声明或测试更低 MSRV。

进入仓库后 Rustup 会按该文件解析工具链。不得使用 `--ignore-rust-version` 绕过基线，也不得只修改部分成员版本。`make check-deps` 会同时验证 Resolver、固定频道、必要组件、最小 Profile、成员继承和 Cargo 元数据的实际解析结果。

固定工具链不等于跨宿主运行验收。Windows x86_64 在本轮完成全量验证；Linux x86_64、Linux ARM64、macOS x86_64 与 macOS ARM64 必须后续在各自宿主独立运行同一治理、构建和测试入口。

## 构建模式与本地服务端口

通过 `make build`、`make br` 或 `make run` 使用 Debug 模式时，构建与运行脚本会自动把 `output/config/controller.json` 的 Controller endpoint 更新为 `http://127.0.0.1:19802`。通过 `make release`、`make build release`、`make br release` 或 `make run release` 使用 Release 模式时，对应 endpoint 自动更新为 `http://127.0.0.1:19801`。更新过程只修改 `endpoint`，保留同一配置中的进程模式、超时、租约与其他设置。

Vite 仅在 Debug/Tauri 开发期间使用 `17601`，不参与 VLDB 端口选择。VulcanCode WebUI 服务使用 Release `17891–17920`、Debug `17921–17950`；Release 不配置 Vite 端口。内置 VMM 直接运行在 VulcanCode 进程内，不占用独立服务端口；外置 VMM 的端口完全来自用户配置。Windows 使用 PowerShell 构建与运行脚本，Linux x86_64、Linux ARM64 与 macOS 使用对应 Unix-like 脚本执行相同的 profile 配置规则。

## 新环境初始化

在已经具备 `make`、Rustup stable、PowerShell 7 以及 Node.js/pnpm 的环境中，仓库依赖统一通过一个入口初始化：

```bash
make init-system
```

该命令按固定顺序执行：

1. 以 Cargo 并发度 2 安装缺失的 `cargo-hakari`、`cargo-nextest`、`cargo-deny` 与 `cargo-about`，并将低于仓库最低要求的工具更新到兼容版本。
2. 安装宿主依赖与内建工具。
3. 安装 LuaSkills 和 Codebase Memory 运行时依赖；原生 VMM 不需要独立运行时安装。

已满足版本要求的工具会被保留，不会重复安装。

Rust 工具已存在时会只打印版本并跳过安装，不会重复编译或覆盖现有工具。Windows 通过 `make.ps1` 执行，类 Unix 环境通过 `make.sh` 转发到同一套 PowerShell 依赖入口。

单独安装 Rust 工作区工具可以执行：

```bash
make deps-rust-tools
```

## Rust 依赖治理检查

依赖版本、Feature 或成员清单变化后，执行仓库只读治理入口：

```bash
make check-deps
```

该入口检查 Resolver 与工具链政策、工作区版本权威、Hakari 生成状态、RustSec 公告、许可证、来源、第三方公告源码制品、格式和受限并发静态编译，不会自动升级依赖或改写 `Cargo.lock`。完整规则见 [`Rust 依赖治理规范`](rust-dependency-governance.md)。

## workspace-hack 依赖去重

仓库使用 `cargo-hakari` 生成根目录下的 `workspace-hack` crate，并将统一后的依赖特性集中到该 crate，减少多 crate 工作区中的重复依赖编译。相关配置位于 `.config/hakari.toml`，生成结果位于 `workspace-hack/`。

依赖声明或 feature 变化后，使用以下命令检查并刷新生成结果：

```bash
cargo hakari generate
cargo hakari manage-deps
cargo hakari verify
```

`workspace-hack` 只能合并兼容的同版本依赖特性；不同主版本或无法安全统一的依赖仍会保留为多个版本。它不能替代 Cargo.lock，也不会把语义上不同的依赖强行改成同一版本。当前生成平台覆盖 Windows x86_64、Linux x86_64、Linux ARM64、macOS x86_64 与 macOS ARM64。

## 默认资源限制

根目录 `.cargo/config.toml` 设置 `build.jobs = 4`，限制 Cargo 编译进程并发；同时以 `RUST_TEST_THREADS = 4` 约束常规测试 harness。`.config/nextest.toml` 将 nextest 默认测试线程限制为 4。

临时调整时可以显式覆盖：

```bash
cargo test --jobs 2 -- --test-threads=1
cargo nextest run --test-threads=1
```

涉及 Provider Manifest、服务内核注册表、端口或其他进程级共享资源的测试由 `shared-runtime-resources` Nextest 组串行执行。新增或重命名此类测试后，必须使用 `cargo nextest show-config test-groups` 核对实际匹配。工作区全量测试可能同时启动大量测试进程，应先执行目标 crate 的 `cargo check` 或定向测试。`cargo nextest` 不覆盖 doctest，文档测试需要额外执行：

```bash
cargo test --doc
```

如果确实需要临时提高并发度，应在任务记录中说明原因，并确认不会影响开发机响应性或共享资源测试的稳定性。
