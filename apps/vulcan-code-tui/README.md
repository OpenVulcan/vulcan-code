<div align="center">

# VulcanCode TUI

**Vulcan Code 的终端界面客户端**

Rust · ratatui · 全鼠标操作

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](../../LICENSE)

</div>

---

## 关于项目

VulcanCode TUI 是 [Vulcan Code](../../README.md) 的终端版客户端，位于工作区的 `apps/vulcan-code-tui`。它连接本机运行的
Vulcan Code 核心服务，在终端里提供与 WebUI 相同的工作台：项目与会话、对话与思考过程、工具调用、提问、
Git 变更与提交，以及记忆、知识库、代码语义引擎、技能、代理、插件管理和系统设置。

- **同一套服务接口**：TUI 与 WebUI 使用同一个 `POST /rpc` 与 `/events` WebSocket，按 WebUI 的函数与参数
  调用服务，不另造同功能、不同名的接口。
- **终端里的 Vulcan 风格**：文字品牌、主题配色、动态思考框、可折叠的工具组与详情面板。
- **全鼠标支持**：单击、双击、右键菜单、滚轮都可用，键盘与鼠标走同一条操作路径。
- **多语言**：文案从 WebUI 语言目录同步，语言种类与 WebUI 保持一致。

> **项目状态**：早期开发中。与核心服务的完整对接仍在进行，部分页面与操作尚未完成；Lua 插件面板、内置
> 浏览器等只能在图形界面显示的插件视图暂不提供。

## 环境要求

- Rust 工具链由仓库根的 [`rust-toolchain.toml`](../../rust-toolchain.toml) 固定，`rustup` 会自动安装对应版本。
- 连接真实服务时，需要在本机运行 Vulcan Code（桌面版或服务宿主）。TUI 读取服务自己发布的发现文件取得
  地址，无需手动配置。
- 支持鼠标与真彩色的终端（Windows Terminal、iTerm2、WezTerm、Kitty 等）。

## 快速开始

```bash
# 连接本机运行中的 Vulcan Code 服务
cargo run --release -p vulcan-code-tui --bin vulcan-tui

# 不连接服务，使用脚本化的模拟后端体验界面
cargo run --release -p vulcan-code-tui --bin vulcan-tui -- --backend mock
```

| 参数 | 说明 |
| --- | --- |
| `--backend service\|mock` | 默认 `service`，连接本机服务；`mock` 为脚本化演示，数据全部为合成数据 |
| `--service-root <dir>` | 服务用户根目录，即包含 `code/webui-service.json` 的上级目录；省略时依次搜索用户目录下的常用服务根 |
| `--locale <tag>` | 界面语言，例如 `zh-TW`；省略时跟随服务的界面语言，再按系统语言匹配 |
| `--theme dark\|light` | 配色主题，默认 `dark` |

> Windows 会锁定正在运行的可执行文件，重新编译前请先关闭 `vulcan-tui.exe`。

## 常用操作

| 按键 | 作用 |
| --- | --- |
| `F1` | 帮助 |
| `F2` | 切换区域（工作台 / 技能 / 插件 / 代理 / 记忆 / 知识库 / 设置） |
| `Ctrl+P` | 命令面板 |
| `Ctrl+N` | 新建会话 |
| `Ctrl+O` / `Ctrl+R` | 选择模型 / 切换推理强度 |
| `Ctrl+B` | 收起或展开左侧项目栏 |
| `Ctrl+\` | 收起或展开右侧面板 |
| `Alt+1` … `Alt+6` | 打开右侧面板的对应标签（变更、进度、终端、记忆、统计、代理） |
| `Ctrl+T` | 切换思考框停靠 / 浮动 |
| `Ctrl+F` | 浏览会话历史 |
| `Alt+Enter` / `Ctrl+J` | 输入框换行 |
| `/` | 斜杠命令 |
| `Ctrl+Q` | 退出 |

所有区域都可以直接用鼠标操作；浮层外单击即可关闭浮层。

## 开发

```bash
cargo fmt --check
cargo clippy -p vulcan-code-tui --all-targets -- -D warnings
cargo nextest run -p vulcan-code-tui
```

- 仓库根的 [`.cargo/config.toml`](../../.cargo/config.toml) 将编译并发限制为 16、测试线程限制为 4，请勿在验证命令中绕过。
- `src/render_smoke_tests.rs` 以无头后端在宽、窄两种尺寸下渲染全部区域与浮层；需要目视检查时设置
  `VULCAN_TUI_SNAPSHOT_DIR=<目录>` 后运行该测试输出快照。

### 多语言

文案只使用 `tr!("key")` 字面量键。语言清单与文案来自与 WebUI、服务共用的
[`crates/vulcan-i18n/catalogs`](../../crates/vulcan-i18n/catalogs)：TUI 先读 WebUI 的键，再读 `tui.*` 专属键
（`catalogs/tui/<locale>.json`）。测试会检查每个 `tr!` 键都存在、且 TUI 目录没有无人使用的键。

### 目录结构

| 目录 | 职责 |
| --- | --- |
| `src/model/` | 与服务协议对齐的数据模型 |
| `src/service_api/` | 按 WebUI 函数命名的服务调用（RPC 方法、参数与结果用途） |
| `src/backend/service/` | 服务发现、RPC 与事件流、工作台投影 |
| `src/backend/mock/` | 脚本化模拟后端 |
| `src/app/` | 应用状态、输入分派与唯一的副作用入口 |
| `src/ui/` | 纯渲染 |
| `src/i18n/` | 基于共享目录的运行时翻译与键检查 |

更多结构与交互约定见 [`tui-design.md`](../../docs/contracts/tui-design.md)，参与开发的约定见 [`AGENTS.md`](AGENTS.md)。

## 安全

服务发现文件包含访问令牌。TUI 只在程序内部解析它，令牌不会出现在界面、日志、错误信息或调试输出中；凭据
类字段在界面上只显示掩码，TUI 不在本地保存任何机密。调试时请勿打印发现文件或服务的 `/health` 响应。

## 许可证

随 Vulcan Code 以 [MIT 许可证](../../LICENSE) 开源。
