# 开发环境初始化与 Cargo 并发策略

## 固定 Rust 工具链

仓库根 `rust-toolchain.toml` 固定 Rust `1.97.1`，使用 `minimal` Profile 并安装 `clippy`、`rustfmt`。根工作区采用 Cargo Resolver 3，49 个成员全部通过 `rust-version.workspace = true` 继承同一 `1.97.1` 支持基线；本仓库不再声明或测试更低 MSRV。

进入仓库后 Rustup 会按该文件解析工具链。不得使用 `--ignore-rust-version` 绕过基线，也不得只修改部分成员版本。`make check-deps` 会同时验证 Resolver、固定频道、必要组件、最小 Profile、成员继承和 Cargo 元数据的实际解析结果。

固定工具链不等于跨宿主运行验收。Windows x86_64 在本轮完成全量验证；Linux x86_64、Linux ARM64、macOS x86_64 与 macOS ARM64 必须后续在各自宿主独立运行同一治理、构建和测试入口。

## 构建模式与本地服务端口

数据库引擎随 VulcanCode 进程内建运行，不再有独立的控制器进程、端点或配置文件。构建与运行脚本因此不再按 Debug/Release 改写任何数据库端点：`make build`、`make br`、`make run`、`make release` 及其 `release` 变体在这一点上行为一致，也不存在需要保留或迁移的 `output/config/controller.json`。原先的 VLDB 端口 `19802`（Debug）与 `19801`（Release）已随该配置一并移除。

Vite 仅在 Debug 开发期间使用 `17601`，不参与数据库端口选择。VulcanCode WebUI 服务使用 Release `17891–17920`、Debug `17921–17950`；Release 不配置 Vite 端口。当前受支持的 WebUI 运行环境是浏览器访问（Vite 开发服务器或已构建资源）与 `apps/vulcan-code-desktop` Electron 桌面壳。Core 使用独立的 `tray-icon` 组件、Tao 原生事件循环和 `notify-rust` 故障通知，不依赖 Tauri、Wry 或 WebView2；Windows 可执行文件资源由 `winresource` 嵌入。WebUI 目录中保留的旧 Tauri 兼容适配器不是 Core 构建依赖，也不属于当前支持的宿主。内置 VMM 直接运行在 VulcanCode 进程内，不占用独立服务端口；外置 VMM 的端口完全来自用户配置。Windows 使用 PowerShell 构建与运行脚本，Linux x86_64、Linux ARM64 与 macOS 使用对应 Unix-like 脚本执行相同的构建规则。

> **关于 `protoc`。** `.cargo/config.toml` 把 `PROTOC` 与 `PROTOC_INCLUDE` 指向仓库随附的 `protoc`，使那些构建脚本直接调用 `protoc` 的第三方 crate（例如 `lancedb` 引入的 `lance-encoding`）无需系统级 protobuf 安装即可构建。通过 Cargo 的 `relative = true` 从仓库根解析路径，因此检出目录可以移动；Windows 启动命令时自动补充 `.exe`。显式设置的环境变量仍优先。

## Windows Debug 下只更新 Core

### 内置浏览器依赖

“Vulcan 浏览器”保留插件设置入口，作为随包交付的内置组件。官方 MCP 固定为 `0.0.82`，其 Playwright 与 playwright-core 固定为 `1.64.0-alpha-1789764292000`；版本组合在 `scripts/browser-runtime.mjs` 中校验，不使用 `latest`。升级必须同时更新锁文件并重新验证 MCP 导航、快照、点击、截图及授权撤销。

先执行 `make init-system` 准备受管 Node `24.18.0` 和 pnpm `11.11.0`，再执行 `make sync`。这两个入口都会通过 `scripts/managed-node.mjs` 补齐缺失的受管 Node/pnpm，使用官方精确版本地址并校验 SHA-256/SHA-512，不依赖已有 `output/browser` 或正在运行的 Core。同步阶段按锁文件准备插件依赖，并把对应 Chromium 下载到仓库 `target/browser-runtime/`；`make build` 仅使用已准备、已同步的输入，生成 `output/browser`，缺失时失败，不在运行期间下载。系统 Node/pnpm 仍属于下文规定的开发机前置工具。

发行物中 `desktop/runtime/` 仅运行 Electron GUI；`browser/runtime/` 保存 Core 浏览器宿主及驱动，`browser/chromium/` 保存独立 Chromium，宿主由 `dependencies/runtimes/node/` 的受管 Node 启动。运行布局架构为 3，`browserHostExecutable` 与 `desktopExecutable` 分别声明两个程序；`tuiExecutable` 声明与服务同目录发布的终端客户端 `bin/vulcan-tui`，Windows 托盘的“打开终端客户端”只按该条目启动。旧版用户目录下的浏览器下载不会覆盖随包版本，也不会在启动时自动删除。

首次启用只检查本地依赖，并在使用时发布插件私有执行代次，无须下载。组件缺失或损坏时修复应用安装；开发环境重新执行 `make sync` 和完整构建。Node、Electron 与 Chromium 的上游许可文件随对应目录保留。

首次启动或修改 Electron 壳、运行布局时，先执行 `make build`，再执行 `make run`。仅修改 WebUI 页面时，保留 Vite 与桌面壳运行即可，页面由 Vite 热更新。

Windows 下需要完整重建并重新启动时使用 `make br`（发布模式为 `make br release`）。全部编译成功后、写入发行目录之前，它会强制停止可执行文件属于当前发行目录的进程，包括核心、桌面壳、终端宿主和内置浏览器，等待退出后发布并启动新桌面；其他发行目录及外部 Vite 不在停止范围内。普通 `make build` 不自动停止程序，发现发行目录仍有进程时在发布前明确拒绝。强制重启会中断运行中的任务；需要先排空时可先从托盘退出服务。

仅修改 Rust Core 时，不必重新构建或关闭桌面壳。日常直接执行：

```powershell
make core-restart
```

只想验证编译而暂不重启时，使用 `make core-build`；它只把 `vulcan-code-service` 编译到 Cargo 的 Debug 目录，不替换正在运行的服务。`make core-restart` 自带增量编译，使用桌面运行入口共用的布局校验；编译和启动契约校验成功后，自动强制停止精确路径匹配的当前 Debug Core，原子替换 `output/bin/vulcan-code-service.exe` 并启动新 Core，无需手动退出托盘。其他发行目录或 Release 进程不在停止范围内；如果其他程序占有同一 Debug 数据目录，则拒绝重启。桌面壳与 Vite 保持运行，WebUI 断线后按新 Core 的发现信息重连。重启期间正在执行的 AI 请求会中断，应在重连后重新发起；需要优雅排空时可先自行从托盘退出服务。

此入口目前只支持 Windows Debug。修改 `vulcan-terminal-host`、Electron、运行布局或 Core 与壳之间的启动协议时，使用完整 `make build`；修改权威运行资源时，先显式执行 `make sync`。`make core-restart` 不会同步资源；新 Core 退出且尚未健康就绪时，恢复原可执行文件备份。运行布局架构版本由 `scripts/desktop-runtime.mjs` 定义并校验，核心脚本不再维护独立版本数字。

核心重启保留可见的 Debug 控制台，不再隐藏窗口。宣布成功必须同时满足新进程健康身份匹配、托盘事件循环就绪以及 Windows 确认托盘注册；本次进程通过启动器指定的 `VULCAN_CORE_STARTUP_REPORT` 写入独立确认文件，位于 `target/core-restart/`。该变量只用于重启确认，不替代服务发现。Windows 可以将已注册图标放在折叠区域；图标外观改变不意味着服务未启动。关闭工作台窗口仍只结束桌面客户端，后台核心退出使用托盘菜单。

## Core 原生托盘边界

托盘实现位于 `apps/vulcan-code-service/src/tray.rs`，仅在 `desktop-tray` 特性下编译。`tray-icon` 负责图标与菜单，Tao 仅负责主线程的操作系统事件循环，不创建工作台窗口；窗口继续由 Electron 提供。这样托盘无需引入 Tauri 应用框架、WebView 运行时、插件注册或前端 IPC。

菜单项、图标及其事件分发始终留在主线程。退出菜单启动独立排空线程，排空成功后通知主线程先释放托盘，再结束事件循环；排空失败则恢复服务准入并保留托盘。服务所有者随事件循环保留，不再通过手动泄漏延长生命周期。启动确认继续检查 Windows 实际注册的托盘矩形，不以 HTTP 就绪代替托盘就绪；Debug 控制台继续使用独立的原生控制台模块。

Windows 图标与版本资源由 `winresource` 嵌入。故障通知直接调用 `notify-rust`；其 Windows 后端包名 `tauri-winrt-notification` 是独立的 WinRT 通知封装，不包含 Tauri 应用框架。Linux 原生菜单仍使用 GTK3，其宏依赖 `proc-macro-error 1` 的现有停维护公告保留为 `deny.toml` 中唯一显式例外；升级 Tao 或 `tray-icon` 时必须重新检查此依赖链。

依赖移除后的验证包括 Core 全目标编译、图标解码测试、服务启动参数与发现身份测试，以及 `cargo tree -p vulcan-code-service --target all --locked` 对 Tauri/Wry/WebView 的依赖排除检查。跨平台菜单外观和桌面通知必须在对应操作系统实际验收，不能用 Windows 编译结果代替。

## 新环境初始化

在已经具备 `make`、Rustup stable、PowerShell 7 以及 Node.js/pnpm 的环境中，仓库依赖统一通过一个入口初始化：

```bash
make init-system
```

该命令按固定顺序执行：

1. 以 Cargo 并发度 2 安装缺失的 `cargo-hakari`、`cargo-nextest`、`cargo-deny` 与 `cargo-about`，并将低于仓库最低要求的工具更新到兼容版本。
2. 安装宿主依赖与内建工具。
3. 安装 LuaSkills 运行时依赖；原生 VMM 与原生代码语义索引都在 VulcanCode 进程内运行，不需要独立运行时安装。
4. 取得 LanceDB 依赖闭包构建所需的随仓库 `protoc`（`protoc 31.1`）。`lancedb` 引入的 `lance-encoding` 会在其构建脚本中直接调用 `protoc`，因此没有它工作区无法编译；`.cargo/config.toml` 把 `PROTOC` 与 `PROTOC_INCLUDE` 指向 `.cargo/protoc/` 下的该副本，开发机无需系统级 protobuf 安装。安装器先按锁定归档摘要验证，再解压并核验版本及头文件，准备完成后才替换安装；准备失败保留现有安装。下载产物不符时直接报错，不静默接受。该目录属于构建输入，已被 git 忽略，因此不随仓库提交。

已满足版本要求的工具会被保留，不会重复安装。

Rust 工具已存在时会只打印版本并跳过安装，不会重复编译或覆盖现有工具。Windows 通过 `make.ps1` 执行，类 Unix 环境通过 `make.sh` 转发到同一套 PowerShell 依赖入口。

单独安装 Rust 工作区工具可以执行：

```bash
make deps-rust-tools
```

## Rust 依赖治理检查

### 持续集成中的精简准备

开发机继续使用 `make init-system`。GitHub Actions 的 Rust 作业运行在已有编译器的 Windows 镜像中，只准备当前检查实际使用的依赖：由 `install-locked-tools.mjs` 安装 protoc、nextest 与内置 sg/rg，执行 `make.ps1 deps lua` 和 `managed-node.mjs output`，在运行时回归前显式 `make.ps1 sync`。不为这些测试编译安装未使用的 hakari、deny、about 或 RTK。

nextest 的 CI 二进制仅支持该作业使用的 Windows x64，安装到忽略目录 `target/ci-tools/bin/`，不覆盖开发机全局 Cargo 工具。唯一版本与归档 SHA-256 来源为 `scripts/build-inputs.lock.json`；下载器检查缓存摘要，安装器检查实际版本，不使用 latest 或未校验的归档。开发机其他平台仍使用原初始化入口。分类与缓存规则见 [仓库检查](repository-quality-gates.md)。

### 依赖治理入口

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

正式服务日志的归属、轮转和过滤规则见 [服务诊断日志](service-diagnostics.md)。被忽略测试的分类、离线串行门禁、并发代码压力复现及外部验收前置条件见 [显式测试入口](explicit-test-gates.md)。

服务内核的开发模式采用根 `Cargo.toml` 中的包级 `debug = "limited"`：保留源码位置和回溯信息，减少完整类型及局部变量调试信息。原因是 Windows 实测完整调试内核归档达到 4,247,716,754 字节，符号索引已变为 `/SYM64/`，MSVC 报无效库格式并在服务链接时产生 49 个未解析符号；这些符号实际仍存在于归档中。该设置只影响内核包，测试模式继承开发模式，发布模式及其他依赖的设置保持原定义。需要完整局部变量调试信息时，应先解决内核归档体积问题，再覆盖此设置。第三方依赖另以 `[profile.dev.package."*"] debug = "line-tables-only"` 只保留行表：回溯仍含源码位置，链接时间与 PDB 体积随之下降；工作区 crate 不受此设置影响。需要单步进入第三方源码时，临时为该依赖单独覆盖为 `debug = true`。

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

### 本地测试入口

仓库尚未启用 CI，整套 Rust 测试只能由本地入口定期执行：

| 命令 | 内容 |
| --- | --- |
| `make test` | 以 nextest `fast` 配置运行全工作区测试，跳过依赖真实流逝时间或完整插件生命周期的测试（实测每项 13–200 秒）。 |
| `make test full` | 运行全部 nextest 测试、`scripts/test-isolated-callbacks.mjs` 中的进程隔离回调测试，以及 `cargo test --doc`。 |

两者都由 `scripts/test-rust.mjs` 执行，并继承 `.config/nextest.toml` 的线程上限与串行组。`fast` 的排除名单只在 `.config/nextest.toml` 的 `[profile.fast]` 中维护；新增长耗时测试时更新该过滤器，并用 `cargo nextest list --profile fast` 核对实际排除结果。合并或发布前执行 `make test full`。

### 手动验证夹具

下列脚本不属于 `make test`，只在修改对应模块时手动执行；它们都只使用合成数据或可丢弃进程，不触碰用户配置、数据库或正在运行的发行实例。修改对应生产代码后若夹具不再成立，必须同一任务中修正或删除，不能留下无法通过的脚本。

| 脚本 | 何时执行 | 执行方式 |
| --- | --- | --- |
| `scripts/test_core_debug_cycle.ps1` | 修改 Debug Core 强制重启归属逻辑 | `pwsh -File`，无参数 |
| `scripts/test_distribution_processes.ps1` | 修改发行目录发布与进程占用处理 | `pwsh -File`，无参数 |
| `scripts/test_cargo_artifact_cleanup.ps1` | 修改 Cargo 产物清理；会执行真实隔离 Cargo 构建，耗时较长 | `pwsh -File`，无参数 |
| `scripts/test_rust_third_party_notices_index.ps1` | 修改第三方声明索引器或 `THIRD_PARTY_NOTICES.md` | `pwsh -File`，无参数 |
| `scripts/test_context7_plugin.lua` | 修改 Context7 插件 Bundle | 在 Vulcan Lua 运行时中执行，传入 `plugin_path`；`online` 可选开启在线冒烟 |
| `scripts/test_statistics_cost_pagination.lua`、`scripts/test_statistics_page_projections.lua` | 修改用量统计插件的费用累加或分页视图 | 在 Vulcan Lua 运行时中执行，传入 `plugin_path`，`change_revision` 可选 |
| `scripts/test_vulcan_plugin_lua_modules.lua` | 修改 Vulcan Plugin 白名单 Lua 模块桥 | 在 Vulcan Lua 运行时中执行，传入 `bridge_path` 与 `workspace_root` |
| `scripts/sync_common_model_capabilities.ps1` | 同步公共模型能力目录与 Provider Manifest | `pwsh -File`；会改写 `common_model_capabilities.json`，执行后审阅差异 |
| `scripts/audit_cliproxy_protocol_changes.ps1` | 审计 CLIProxyAPI 协议兼容差异 | `pwsh -File`；参数默认指向本机 CLIProxyAPI 检出与最近审核基线，可用 `-RepositoryPath`、`-BaselineCommit` 覆盖；只读上游仓库，改写 `docs/protocol-compatibility/cliproxy-audit-latest.md` |
| `scripts/plugin-runtime-validation/` | 历史证据：LuaSkills 0.5.9 → 0.6 迁移的性能对照，迁移已完成，不作为当前验证入口 | 见目录内 `README.md` |


## 锁定命令行构建输入

scripts/build-inputs.lock.json 是内建 sg、rg 和 protoc 的版本、各平台归档摘要及许可证摘要的唯一来源。安装入口共用 install-locked-tools.mjs，不查询最新发行版；升级必须评审清单差异并验证对应平台。此清单不替代 Cargo、pnpm 及已有受管 Node 的锁定机制。

已验证归档和许可证按摘要缓存在 target/build-input-cache/。联网初始化填充缓存后，可以运行 pwsh -File scripts/install_bundled_tools.ps1 -Offline 和 pwsh -File scripts/install_protoc.ps1 -Offline 重建对应安装。缺失缓存或摘要损坏直接失败，不联网、不换版本。离线能力只覆盖这些输入；完整项目离线构建还需预先准备 Cargo、pnpm 等各自的依赖缓存。

安装使用独立暂存目录，先验证工具、许可证和 protoc 头文件，再替换目标。并发安装同一目标会明确拒绝；异常退出遗留的 .installing 目录需要确认旧安装进程已退出后清理，.previous 保存待恢复的旧安装，不可直接删除。常规替换失败会恢复旧目录。

发布构建每次执行前端与桌面的 pnpm install --frozen-lockfile，Rust 构建使用 --locked。依赖声明与锁文件不一致必须失败，不能在发行过程中更新锁文件。

## 类 Unix 完整发行目录切换

先退出当前发行物的服务与桌面，再执行构建。Unix 构建入口在所有编译成功后，将已有同步资源复制到仓库 target 内的独立暂存目录，加入新二进制并运行既有桌面打包、布局校验。准备失败不改动正式目录；缺少同步资源仍需显式执行 make sync，不在构建时隐式同步。

切换先将正式目录改名为 .previous，再提升暂存目录。提升失败立即回滚；回滚本身失败时明确报告旧版保留路径。两个目录重命名之间存在短暂空窗，所以这是可恢复的版本切换，不承诺无中断的运行中升级或断电持久性。上一版保留到下次调用验证当前版本通过后才清理。

新进程可恢复中断的切换：正式目录缺失时恢复 .previous；新目录无效时将它移到独立 .rejected-* 目录供诊断，再恢复旧版。进程所有权保护拒绝并发发布；确认原进程已退出后可以回收锁。所有权文件缺失、损坏或进程身份仍存活时保守拒绝，需要操作者先核实，不能强行删除正在使用的目录。

目录级故障测试位于 scripts/directory-publication.test.mjs，覆盖准备失败、校验失败、磁盘不足、权限失败、回滚失败和重启恢复。它们不替代各平台真实 Electron、Chromium 与 Core 的打包及启动验收。
