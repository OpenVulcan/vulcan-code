<div align="center">

<img src="apps/vulcan-code-web/public/vulcan-logo.png" alt="Vulcan Code" width="120" />

# Vulcan Code

**开源的图形界面（GUI）AI Coding 桌面工作台**

Rust Core · Electron 桌面壳 · React WebUI

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Issues](https://img.shields.io/github/issues/OpenVulcan/vulcan-code)](https://github.com/OpenVulcan/vulcan-code/issues)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/OpenVulcan/vulcan-code/pulls)

**简体中文** | [English](README.en.md) | [繁體中文](README.zh-TW.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Español](README.es.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Русский](README.ru.md) | [Português](README.pt-BR.md)

</div>

---

## 关于项目

Vulcan Code 是一款**图形界面的 AI Coding 软件**。它把项目、会话、终端、浏览器、代码审查和 Git 提交放进同一个桌面工作台，让 AI 能理解你的代码、推进任务，并交付可以检查的结果。

这个项目已经在内部开发和调试了很长时间，现在以 **MIT 协议**开源。我们希望它不只是一个团队的产品，而是由社区一起打磨的工具：

- 🐛 遇到问题、有想法，欢迎提 [Issue](https://github.com/OpenVulcan/vulcan-code/issues)
- 🔧 修了 bug、做了新功能，欢迎提 [Pull Request](https://github.com/OpenVulcan/vulcan-code/pulls)
- 🤝 无论你是用户还是开发者，都可以参与共建，一起做一个大家都喜爱的 AI Coding 软件

## 特色功能

### 🧭 桌面工作台

- 多项目、多会话管理，文件树、审查、记忆、子代理等面板自由组合
- 内置项目终端，支持多标签；AI 可以持有长时间运行或需要交互的命令会话
- 流式回答，Markdown、代码块、Mermaid/SVG 图形与工具调用卡片直接渲染
- 可以编辑历史问题重新生成，也可以从任意已完成的回合分叉出新会话
- 中英文界面，明暗主题

### 🎯 任务推进

- **Goal 目标模式**：设定目标和 Token 预算，AI 按回合持续推进，可暂停、恢复、清除
- **Plan 计划模式**：先探索、出计划，确认后再执行
- **执行中引导**：任务运行时可以追加说明、排队后续请求
- **子代理协作**：内置探索、计划等代理角色，另附 [agency-agents](https://github.com/msitarzewski/agency-agents) 的 260+ 个角色模板（默认关闭，按需启用）
- **结构化提问**：AI 遇到需要你决定的问题时，用选项卡片向你提问

### 🧠 理解代码与积累经验

- **VCE 代码语义索引**：对项目建立向量索引，AI 先找到相关实现再动手修改；索引随代码变化自动同步
- **项目知识库**：导入 Markdown 资料目录，支持 BM25 与向量混合检索，可以按项目授权、按会话选择
- **MemoryMesh 长期记忆**：回合后自动提炼经验，回合前自动召回；记忆可以查看、修正、删除，并带回收站和来源追溯

### 🔍 结果可检查

- **Diff 审查**：统一/分栏视图、字词级差异、全文搜索，支持暂存、还原
- **Git 提交**：AI 生成提交信息，可以提交到当前分支或新分支，并直接推送
- **AI 浏览器**：AI 在真实 Chromium 页面里导航、点击、截图、读控制台；你可以随时接管控制权

### 🧩 可扩展

- **提示词技能**（`SKILL.md`）：把团队的做事方法沉淀下来，下次直接复用
- **LuaSkills**：把常用动作封装成可配置、可安装的程序化能力包
- **MCP**：支持本地进程与 HTTP 两种 MCP 服务器，带连接诊断和工具 Schema 查看
- **插件系统**：插件可以提供工具，也可以挂载自己的界面。内置插件：Context7 文档检索、AI 浏览器、RTK 命令输出压缩、用量统计、CDP/DAP 调试工具

### ⚙️ 模型与供应商

- 支持 OpenAI、Anthropic、Gemini、Cohere 等协议族，内置 OpenAI、Anthropic、Google、DeepSeek、Kimi、MiniMax、阿里、OpenRouter 等供应商配置，也可以添加自定义供应商
- 对话、记忆检索、代码索引、媒体生成可以各自选用不同的模型
- 扩展能力：网页搜索与抓取，图片/视频/音频理解，图片与视频生成，语音合成与转写
- 上下文窗口分层策略，自动或手动（`/compact`）压缩上下文
- Token 用量、延迟、参考费用统计

## 路线图

目前已经可用的是 **GUI 桌面版**，接下来计划：

- [ ] **TUI 版本**：在终端里使用 Vulcan Code
- [ ] **远端手机连接**：用手机远程查看和操控桌面上运行的会话
- [ ] **Linux / macOS 适配验证**：构建脚本已经具备，还需要在各平台完成完整验证

如果你对其中某项感兴趣，或者有新的想法，欢迎在 Issue 里讨论。

## 架构

```
┌──────────────────────────┐      ┌─────────────────────────────────────┐
│  Electron 桌面壳          │      │  Rust Core（vulcan-code-service）    │
│  窗口 / 托盘 / 宿主能力    │◀────▶│  会话内核 · Agent 运行时 · 工具执行   │
│  ┌────────────────────┐  │ RPC  │  Provider 协议 · MCP · LuaSkills     │
│  │ React WebUI        │  │ 事件  │  VCE 索引 · 知识库 · MemoryMesh      │
│  └────────────────────┘  │      │  终端宿主 · 浏览器宿主 · 插件运行时   │
└──────────────────────────┘      └─────────────────────────────────────┘
```

Core 服务可以独立在后台（托盘）运行，关闭桌面窗口不会中断正在执行的任务。

| 目录 | 说明 |
| --- | --- |
| `apps/vulcan-code-service` | Core 服务宿主，承载会话内核、Agent 运行时和 WebUI RPC/流服务 |
| `apps/vulcan-code-desktop` | Electron 桌面壳：窗口、托盘，负责启动本地 Core 并加载 WebUI |
| `apps/vulcan-code-web` | React/Vite WebUI：工作台、设置、会话、Diff、技能等界面 |
| `crates/vulcan-agent-runtime` | 共享代理运行时：Provider、工具、LuaSkills、MCP、流式执行 |
| `crates/vulcan-service-*` | 后端服务 API、核心服务内核与运行时适配 |
| `crates/vulcan-memorymesh-*` | MemoryMesh 长期记忆 |
| `crates/vulcan-runtime-assets` | 系统提示词、供应商清单、子代理模板等权威资源 |
| `plugins/` | 内置插件 |
| `docs/contracts/` | 开发契约文档 |

## 快速开始

> 当前在 **Windows x86_64** 上完成了完整验证。Linux 与 macOS 有对应的 `make.sh` 脚本，但还没有完整跑通，欢迎帮忙验证。

### 环境要求

- `make`
- [Rustup](https://rustup.rs/)：仓库通过 `rust-toolchain.toml` 固定工具链版本，进入目录后会自动安装
- PowerShell 7
- Node.js 与 pnpm

### 构建与运行

```bash
make init-system   # 首次：安装 Rust 工具、宿主依赖与受管 Node/pnpm
make sync          # 同步配置、系统技能、插件与浏览器运行时到 output/
make build         # 构建 Core、WebUI 与 Electron 桌面壳
make run           # 启动桌面端
```

也可以用 `make br` 一步完成构建和运行。发布构建使用 `make release`。

详细的环境说明见 [`docs/contracts/development-environment.md`](docs/contracts/development-environment.md)。

### 日常开发

Vite 开发服务器、Core 服务和 Electron 桌面壳彼此独立，可以分别启动和重启：

```bash
make vite start      # 启动 WebUI Vite 开发服务器（也支持 status / stop）
make service         # 只启动后台 Core，不打开桌面窗口
make core-restart    # 只改了 Rust Core 时：增量编译并热替换 Core（Windows Debug）
make sync            # 改了资源、配置、技能或插件后需要重新同步
```

- 只改 WebUI 页面时，保持 Vite 和桌面壳运行即可热更新
- debug 模式下 `make run` / `make br` 会自动确保 Vite 已启动
- 不要直接编辑 `output/`，权威资源在 `crates/vulcan-runtime-assets/resources/`

## 参与共建

我们欢迎任何形式的贡献：报告 bug、完善文档、改进交互、适配新平台、接入新供应商、开发插件和技能……

**提 Issue**

- Bug：请附上操作系统、复现步骤、预期与实际行为，以及相关日志或截图
- 功能建议：说明使用场景和想解决的问题

**提 PR**

1. Fork 仓库并从 `main` 创建分支
2. 修改前先阅读 [`AGENTS.md`](AGENTS.md)。它记录了仓库的长期约定，涉及特定领域时还要阅读对应的 [`docs/contracts/`](docs/contracts/) 文档
3. 提交前在本地完成验证：

   ```bash
   # Rust：优先对修改的 crate 做定向检查
   cargo check -p <crate> --all-targets
   cargo nextest run -p <crate>

   # WebUI
   pnpm --dir apps/vulcan-code-web run lint
   pnpm --dir apps/vulcan-code-web run test
   pnpm --dir apps/vulcan-code-web run build
   ```

4. 在 PR 描述里说明改了什么、为什么改，以及怎么验证的

几条约定：

- 面向 AI 的工具名称、描述、Schema 使用英文；面向用户的界面文案可以使用中文
- 仓库默认限制编译与测试并发为 4，请不要在验证命令中去掉这个限制
- 一个 PR 尽量只做一件事，便于评审

## 致谢

Vulcan Code 构建在众多优秀的开源项目之上，感谢这些项目的作者和社区。

**内置组件与工具**

- [RTK](https://github.com/rtk-ai/rtk)：命令输出压缩，减少 Token 消耗（Vulcan RTK 插件）
- [Context7](https://github.com/upstash/context7)：按版本检索最新的库文档（Context7 插件）
- [Playwright](https://github.com/microsoft/playwright) / [Playwright MCP](https://github.com/microsoft/playwright-mcp)：AI 浏览器的驱动与工具
- [LuaSkills](https://github.com/LuaSkills/luaskills)：LuaSkills 技能运行时
- [ripgrep](https://github.com/BurntSushi/ripgrep) 与 [ast-grep](https://github.com/ast-grep/ast-grep)：随包提供的文本与语法树搜索工具
- [OpenAI Codex](https://github.com/openai/codex) Code Mode：代码模式的 JavaScript 沙盒（V8 运行时、宿主进程、gRPC 协议与客户端）直接搬运自 Codex（Apache-2.0），本地改动标注在源码中，并记录在各 crate 的 `NOTICE.md`（`crates/vulcan-code-mode*/`、`native/code-mode-host/*/`）
- [rusty_v8](https://github.com/denoland/rusty_v8)：代码模式沙盒使用的 V8 引擎 Rust 绑定

**桌面端与界面**

- [Electron](https://www.electronjs.org/)：桌面壳
- [React](https://react.dev/) 与 [Vite](https://vite.dev/)：WebUI 框架与构建工具
- [shadcn/ui](https://ui.shadcn.com/)、[Radix UI](https://www.radix-ui.com/)、[Tailwind CSS](https://tailwindcss.com/)：组件体系与样式
- [xterm.js](https://xtermjs.org/)：终端渲染
- [Shiki](https://shiki.style/)：代码与 Diff 语法高亮
- [Mermaid](https://mermaid.js.org/)：图表渲染
- [Lucide](https://lucide.dev/)、[Iconify](https://iconify.design/)：图标

**Rust Core**

- [Tokio](https://tokio.rs/) 与 [Axum](https://github.com/tokio-rs/axum)：异步运行时与服务框架
- [rmcp](https://github.com/modelcontextprotocol/rust-sdk)：MCP 官方 Rust SDK
- [LanceDB](https://github.com/lancedb/lancedb) 与 [Apache Arrow](https://arrow.apache.org/)：向量索引与列式数据
- [SQLite](https://sqlite.org/)（[rusqlite](https://github.com/rusqlite/rusqlite)）：本地数据存储
- [jieba-rs](https://github.com/messense/jieba-rs)：中文分词，用于知识库全文检索
- [portable-pty](https://github.com/wezterm/wezterm)（WezTerm）：跨平台伪终端

**设计与实现参考**

- [OpenAI Codex](https://github.com/openai/codex)：Responses 协议通讯、重试与上下文压缩的实现参考；Apply Patch 语法与流式解析器改编自 Codex
- [cc-switch](https://github.com/farion1231/cc-switch)：多协议转换、流式工具调用与模型价格数据参考
- [CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI)：Gemini 协议与兼容转换参考
- [OpenCode](https://github.com/anomalyco/opencode)：供应商目录与系统提示词参考
- [agency-agents](https://github.com/msitarzewski/agency-agents)：子代理角色模板来源
- [Answer me with HTML](https://github.com/QingYunA/answer-me-with-html)：回答组件（发现、对比、流程等结构化区块）的设计参考；本项目自行定义组件语法并在会话内原生渲染，不依赖其 CLI

**社区**

- [LINUX DO](https://linux.do/)：感谢 LINUX DO 社区的支持与交流

完整的第三方依赖及许可证见 [`THIRD_PARTY_NOTICES.md`](crates/vulcan-runtime-assets/resources/licenses/THIRD_PARTY_NOTICES.md)，也可以在应用内的「设置 → 关于与许可证」中查看。

## 许可证

[MIT](LICENSE) © 2026 OpenVulcan
