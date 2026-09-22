# 开发环境初始化与 Cargo 并发策略

## 固定 Rust 工具链

仓库根 `rust-toolchain.toml` 固定 Rust `1.97.1`，使用 `minimal` Profile 并安装 `clippy`、`rustfmt`。根工作区采用 Cargo Resolver 3，49 个成员全部通过 `rust-version.workspace = true` 继承同一 `1.97.1` 支持基线；本仓库不再声明或测试更低 MSRV。

进入仓库后 Rustup 会按该文件解析工具链。不得使用 `--ignore-rust-version` 绕过基线，也不得只修改部分成员版本。`make check-deps` 会同时验证 Resolver、固定频道、必要组件、最小 Profile、成员继承和 Cargo 元数据的实际解析结果。

固定工具链不等于跨宿主运行验收。Windows x86_64 在本轮完成全量验证；Linux x86_64、Linux ARM64、macOS x86_64 与 macOS ARM64 必须后续在各自宿主独立运行同一治理、构建和测试入口。

## 构建模式与本地服务端口

数据库引擎随 VulcanCode 进程内建运行，不再有独立的控制器进程、端点或配置文件。构建与运行脚本因此不再按 Debug/Release 改写任何数据库端点：`make build`、`make br`、`make run`、`make release` 及其 `release` 变体在这一点上行为一致，也不存在需要保留或迁移的 `output/config/controller.json`。原先的 VLDB 端口 `19802`（Debug）与 `19801`（Release）已随该配置一并移除。

Vite 仅在 Debug 开发期间使用 `17601`，不参与数据库端口选择。VulcanCode WebUI 服务使用 Release `17891–17920`、Debug `17921–17950`；Release 不配置 Vite 端口。当前受支持的 WebUI 运行环境是浏览器访问（Vite 开发服务器或已构建资源）与 `apps/vulcan-code-desktop` Electron 桌面壳；`apps/vulcan-code-service/tauri.conf.json` 与 `apps/vulcan-code-web/src/shell/*` 中并存的 Tauri 兼容分支（`tauriShell.ts`）仍未决定去留，不列入当前主要壳。内置 VMM 直接运行在 VulcanCode 进程内，不占用独立服务端口；外置 VMM 的端口完全来自用户配置。Windows 使用 PowerShell 构建与运行脚本，Linux x86_64、Linux ARM64 与 macOS 使用对应 Unix-like 脚本执行相同的构建规则。

> **关于 `protoc`。** `.cargo/config.toml` 把 `PROTOC` 与 `PROTOC_INCLUDE` 指向仓库随附的 `protoc`，使那些构建脚本直接调用 `protoc` 的第三方 crate（例如 `lancedb` 引入的 `lance-encoding`）无需系统级 protobuf 安装即可构建。这里刻意使用绝对路径：`[env]` 条目中的相对路径不会按配置文件位置解析。

## Windows Debug 下只更新 Core

### 内置浏览器依赖

“Vulcan 浏览器”保留插件设置入口，作为随包交付的内置组件。官方 MCP 固定为 `0.0.82`，其 Playwright 与 playwright-core 固定为 `1.64.0-alpha-1789764292000`；版本组合在 `scripts/browser-runtime.mjs` 中校验，不使用 `latest`。升级必须同时更新锁文件并重新验证 MCP 导航、快照、点击、截图及授权撤销。

先执行 `make init-system` 准备受管 Node `24.18.0` 和 pnpm `11.11.0`，再执行 `make sync`。这两个入口都会通过 `scripts/managed-node.mjs` 补齐缺失的受管 Node/pnpm，使用官方精确版本地址并校验 SHA-256/SHA-512，不依赖已有 `output/browser` 或正在运行的 Core。同步阶段按锁文件准备插件依赖，并把对应 Chromium 下载到仓库 `target/browser-runtime/`；`make build` 仅使用已准备、已同步的输入，生成 `output/browser`，缺失时失败，不在运行期间下载。系统 Node/pnpm 仍属于下文规定的开发机前置工具。

发行物中 `desktop/runtime/` 仅运行 Electron GUI；`browser/runtime/` 保存 Core 浏览器宿主及驱动，`browser/chromium/` 保存独立 Chromium，宿主由 `dependencies/runtimes/node/` 的受管 Node 启动。运行布局架构为 2，`browserHostExecutable` 与 `desktopExecutable` 分别声明两个程序。旧版用户目录下的浏览器下载不会覆盖随包版本，也不会在启动时自动删除。

首次启用只检查本地依赖，并在使用时发布插件私有执行代次，无须下载。组件缺失或损坏时修复应用安装；开发环境重新执行 `make sync` 和完整构建。Node、Electron 与 Chromium 的上游许可文件随对应目录保留。

首次启动或修改 Electron 壳、运行布局时，先执行 `make build`，再执行 `make run`。仅修改 WebUI 页面时，保留 Vite 与桌面壳运行即可，页面由 Vite 热更新。

仅修改 Rust Core 时，不必重新构建或关闭桌面壳。日常直接执行：

```powershell
make core-restart
```

只想验证编译而暂不重启时，使用 `make core-build`；它只把 `vulcan-code-service` 编译到 Cargo 的 Debug 目录，不替换正在运行的服务。`make core-restart` 自带增量编译；如果旧 Core 还在运行，命令提示在托盘点击 **Quit Service**，等待它完成排空后才原子替换 `output/bin/vulcan-code-service.exe` 并启动新 Core。桌面壳与 Vite 保持运行，WebUI 断线后按新 Core 的发现信息重连。重启期间正在执行的 AI 请求会中断，应在重连后重新发起。

此入口目前只支持 Windows Debug。修改 `vulcan-terminal-host`、Electron、运行布局或 Core 与壳之间的启动协议时，使用完整 `make build`；修改权威运行资源时，先显式执行 `make sync`。`make core-restart` 不会同步资源，也不会把正在运行的 Core 强行终止。

## 新环境初始化

在已经具备 `make`、Rustup stable、PowerShell 7 以及 Node.js/pnpm 的环境中，仓库依赖统一通过一个入口初始化：

```bash
make init-system
```

该命令按固定顺序执行：

1. 以 Cargo 并发度 2 安装缺失的 `cargo-hakari`、`cargo-nextest`、`cargo-deny` 与 `cargo-about`，并将低于仓库最低要求的工具更新到兼容版本。
2. 安装宿主依赖与内建工具。
3. 安装 LuaSkills 运行时依赖；原生 VMM 与原生代码语义索引都在 VulcanCode 进程内运行，不需要独立运行时安装。
4. 取得 LanceDB 依赖闭包构建所需的随仓库 `protoc`（`protoc 31.1`）。`lancedb` 引入的 `lance-encoding` 会在其构建脚本中直接调用 `protoc`，因此没有它工作区无法编译；`.cargo/config.toml` 把 `PROTOC` 与 `PROTOC_INCLUDE` 指向 `.cargo/protoc/` 下的该副本，开发机无需系统级 protobuf 安装。已存在且版本与摘要匹配的安装会被保留，脚本不会删除可用安装；下载产物与钉定的版本和 SHA256 不符时会直接报错，不会静默接受。该目录属于构建输入，已被 git 忽略，因此不随仓库提交。

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

该入口检查 Resolver 与工具链政策、工作区版本权威、Hakari 生成状态、RustSec 公告、许可证、来源、第三方公告源码制品、格式和受限并发静态编译，不会自动升级依赖或改写 `Cargo.lock`。完整规则以仓库当前的 `deny.toml`、`about.toml` 与 `make check-deps` 实际实现为准。

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
