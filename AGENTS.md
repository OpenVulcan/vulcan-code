# Vulcan Code TUI Agent Notes

本文件只保存进入本仓库的任何会话都必须知道的长期约定。结构说明见
[`docs/tui-design.md`](docs/tui-design.md)。

## 一、边界

1. **独立项目。** 本仓库是独立 crate，不修改 `../vulcan-code`；上游只作为 i18n 与品牌资源的只读来源。
2. **当前阶段只有模拟后端。** `src/backend/mock/` 按脚本模拟服务；接入真实服务时替换后端，
   UI 与 `app/` 只通过 `backend::Command` / `BackendEvent` 交互，不直接访问后端内部。
3. **不做 Lua 插件 UI。** 系统插件（记忆、知识库、代理、技能、代码语义索引）以声明式页面实现；
   LuaSkills 与 Vulcan 插件只做管理（启用、工具模式、安装、卸载），插件 UI 挂载点只列出不渲染，
   面板留待后续双向适配。
4. **不保存机密。** 凭据只在输入时遮蔽显示并单次发送；界面只展示末尾几位。不得读取或打印服务发现文件
   （`~/.vulcan*/code/webui-service.json`）。

## 二、代码约定

1. 注释先英文一行、再中文一行；面向 AI 的文本使用英文，面向用户的文案走 i18n。
2. 所有副作用经 `app::action::perform(app, UiAction)` 单一路径；键盘与鼠标最终调用同一函数。
3. 鼠标：视图绘制时用 `app.hits.add(rect, Target)` / `add_scroll` 登记，最新登记优先；每帧开头
   `hits.reset()`。新增可交互元素必须同时登记命中目标。
4. 浮层清屏使用 `ui::text::clear_area`，不要直接 `Clear.render`（否则宽字符会遮住浮层左边框）。
5. 同一语义的边界值只允许一处定义。
6. 系统页面按区域放在 `src/app/pages/<area>.rs`，只产出 `Row`；渲染与输入由 `ui/pages.rs` 与
   `app/input/pages.rs` 通用处理，新增行类型时两处同时补齐。

## 三、多语言

1. 只用 `tr!("key", name = value)` 字面量键；占位符写作 `{name}`。
2. 上游 WebUI 目录（`i18n/sync.json` 的 `webuiSrc`）是权威来源：用到哪些键就同步哪些，语言种类同步上游。
3. TUI 专属文案使用 `tui.*` 键，写入 `i18n/overlay/<locale>.json`（en-US 为默认语言，另有 zh-CN、
   zh-TW、zh-HK）；其他语言缺失时回退到 en-US。
4. 修改文案或新增 `tr!` 调用后执行：
   - `cargo run --bin vulcan-tui-i18n -- sync`
   - `cargo run --bin vulcan-tui-i18n -- sync --check`（CI 同款检查）
5. 不直接编辑 `i18n/generated/`。

## 四、验证

1. `.cargo/config.toml` 限制 `build.jobs = 4` 与 `RUST_TEST_THREADS = 4`，不得绕过。
2. 常规验证：`cargo clippy --all-targets`、`cargo test`、`cargo fmt --check`。
3. `tests/render_smoke.rs` 以无头后端在宽、窄两种尺寸下渲染全部区域与浮层，并用 `hits.find` 驱动鼠标点击；
   需要目视检查布局时：`VULCAN_TUI_SNAPSHOT_DIR=<目录> cargo test --test render_smoke`。
