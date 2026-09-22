# vulcan-code

基于 Rust Core 后端、Electron 桌面壳与 React WebUI 的 AI Coding 桌面端仓库。

## 当前结构

- `Cargo.toml`：根工作区配置，统一管理 Core 服务、桌面壳宿主与共享运行时 crate。
- `apps/vulcan-code-service`：Core 服务宿主（托盘或后台模式），承载 V1 会话内核、Agent 运行时桥、MCP 运行时与 WebUI RPC/流服务。
- `apps/vulcan-code-desktop`：Electron 桌面壳，负责窗口、托盘宿主能力、启动本地 Core 与加载 WebUI。
- `apps/vulcan-code-web`：React/Vite WebUI，承载工作台、设置、会话、Diff、技能与 VMM 管理界面。
- `crates/vulcan-service-*`：WebUI 后端服务 API、核心服务内核与运行时适配。
- `crates/vulcan-agent-runtime`：共享代理运行时，负责 Provider、工具、LuaSkills、MCP、VMM 与流式执行。
- `docs/plan`：任务执行计划文件目录。

仓库不再包含 `apps/vulcan-code-tauri`；WebUI 的 `src/shell/*` 仍保留 Tauri 壳兼容分支，属于尚未决策移除的兼容入口，不属于当前主要桌面壳。

## 运行方式

Vite 开发服务器、Core 服务与 Electron 桌面壳相互独立启动与重启：

```bash
./make vite start    # 独立启动 WebUI Vite 开发服务器
./make vite status   # 查看 Vite 开发服务器状态
./make vite stop     # 停止 Vite 开发服务器
./make service       # 只启动托盘或后台 Core（不打开桌面工作台）
./make run           # 运行 Electron 桌面壳；debug 下先确保 Vite 已启动
./make br            # 构建 Core/WebUI/Electron，并运行桌面壳；debug 下先确保 Vite 已启动
```

debug 模式下的 `run` 与 `br` 会先调用独立的 Vite 启动入口，已有服务器可继续复用；也可使用 `make vite start|status|stop` 单独管理。release 模式直接使用已经构建的 WebUI 资源，不依赖 Vite。仅修改 WebUI 页面时保留 Vite 与桌面壳即可热更新；仅修改 Rust Core 时使用 `make core-build` / `make core-restart`，避免重建 Electron 与 WebUI。

AI 使用的 Core 浏览器需要单独打包与当前桌面端 `playwright-core` 版本匹配的 Chromium。首次在 Windows PowerShell 调试时，先安装该版本的完整 Chromium；关闭正在运行的 Debug GUI 和 Core 后，在**同一个 PowerShell 会话**中指定浏览器来源并构建：

```powershell
node apps/vulcan-code-desktop/node_modules/playwright-core/cli.js install chromium --no-shell
$chromiumExe = node -e "process.stdout.write(require('./apps/vulcan-code-desktop/node_modules/playwright-core').chromium.executablePath())"
$env:VULCAN_CHROMIUM_SOURCE_ROOT = Split-Path (Split-Path $chromiumExe -Parent) -Parent
./make build
```

以后运行 `./make br` 或 `./make build` 时，仍须在执行该命令的环境中设置 `VULCAN_CHROMIUM_SOURCE_ROOT`；没有设置时，生成的 `runtime-layout.json` 会明确省略 Chromium，`browser_open` 不可用。`./make core-restart` 只替换 Core 可执行文件，不会改变浏览器打包结果。可检查 `output/runtime-layout.json` 的 `chromiumExecutable` 与 `browser-host` 能力确认当前布局是否包含浏览器。

## 资源同步

```bash
./make sync
```

`./make sync` 只负责把配置、运行时资源、系统技能和插件同步到 `output`；资源未变化时，后续 `build`、`release` 与 `br` 无需重复同步。不要直接编辑 `output/`。

## 构建方式

```bash
./make build
./make release
```

## 前端验证

```bash
pnpm --dir apps/vulcan-code-web run lint
pnpm --dir apps/vulcan-code-web run test
pnpm --dir apps/vulcan-code-web run build
pnpm --dir apps/vulcan-code-web run check:build-budget
```

## 开发环境初始化

新环境先执行 `make init-system`，统一安装 Rust 工作区工具与仓库依赖；Cargo 并发限制、workspace-hack、nextest 分组与依赖治理入口见 [`docs/contracts/development-environment.md`](docs/contracts/development-environment.md)。
