# 任务目标

为当前仓库搭建一个基于 Rust + iced 的 GUI 基础设施，形成可编译、可扩展、可继续接入 AI 能力的桌面应用骨架，并在首版界面中引入 iced 的常用组件，便于后续功能迭代与组件验证。

# 详细执行步骤

1. 梳理当前仓库结构，确认是否已有 Rust 工作区、现有约束与第三方 iced 源码参考位置。
2. 在仓库根目录创建 Rust 工作区配置，并新增 GUI 应用 crate，补齐基础依赖、构建元数据与目录结构。
3. 设计 AI Coding 桌面端的首版信息架构，至少覆盖会话区、编辑区、控制区、状态区与活动区。
4. 基于 iced 当前源码 API 实现应用入口、状态管理、消息机制与页面布局，接入常用组件进行示例化展示。
5. 补充必要的工程说明文档，说明当前基础设施的定位、运行方式与后续扩展建议。
6. 执行编译验证并对照计划逐项检查，若存在未达标内容则继续修正直至闭环。

# 技术选型

- GUI 框架：优先使用仓库内 `third_party/iced` 作为路径依赖，确保与当前本地源码版本一致，便于排查问题与参考示例。
- 异步执行器：启用 iced 的 `tokio` 特性，为后续 AI 请求、流式响应与后台任务预留执行基础。
- 组件策略：首版优先接入适合 AI Coding 场景的常用组件，如按钮、文本输入、选择器、切换器、滚动容器、进度条、提示、分栏与文本编辑器等。
- 工程组织：采用“根工作区 + 独立 GUI crate + 模块化源文件”的结构，降低后续拆分服务层、领域层与渲染层的成本。

# 验收标准

- 仓库根目录存在可用的 Rust 工作区配置，能够识别 GUI 应用 crate。
- GUI 应用可成功通过 `cargo check` 或等价编译校验。
- 首版界面已经引入并实际使用多种 iced 组件，不是空白窗口或单一控件示例。
- 代码具备清晰的模块边界，便于后续接入 AI 对话、文件系统、任务编排与命令执行能力。
- 文档中明确说明了运行方式、结构定位与后续扩展方向。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 已在仓库根目录完成 Rust workspace 初始化，并新增 `apps/vulcan-code-gui` 作为首版 GUI 应用 crate。
- 已采用本地 `third_party/iced` 作为路径依赖，完成三栏式 AI Coding 工作台骨架搭建。
- 已在首版界面中接入 `pane_grid`、`text_editor`、`text_input`、`pick_list`、`radio`、`checkbox`、`toggler`、`slider`、`progress_bar`、`markdown`、`svg`、`scrollable`、`tooltip`、`button`、`container`、`rule` 等关键组件。
- 已补齐运行说明文档，并完成 `cargo check -p vulcan-code-gui` 编译校验。

## 2. 📂 文件变更清单

### 新增

- `Cargo.toml`
- `apps/vulcan-code-gui/Cargo.toml`
- `apps/vulcan-code-gui/src/main.rs`
- `apps/vulcan-code-gui/src/app.rs`
- `apps/vulcan-code-gui/src/model.rs`
- `apps/vulcan-code-gui/src/theme.rs`
- `apps/vulcan-code-gui/resources/vulcan-mark.svg`

### 修改

- `README.md`
- `docs/plan/20260506-01-ICED_GUI_FOUNDATION.md`

## 3. 💻 关键代码调整详情

- 新建根工作区配置，并通过 `exclude = ["third_party/iced"]` 规避嵌套 workspace 与本地 iced 源码共存时的 Cargo 冲突。
- 在 `app.rs` 中完成应用主状态、消息分发、分栏布局、会话面板、工作台面板、运行检查面板与底部状态栏的组合实现。
- 在 `model.rs` 中抽离模型提供方、模型预设、执行模式、会话摘要、活动日志等领域模型，确保后续接入真实 AI 协议层时具备清晰边界。
- 在 `theme.rs` 中封装应用主题与代码高亮主题映射逻辑，为后续主题扩展和编辑器高亮统一提供入口。
- 在 `README.md` 中补充了结构说明、组件清单、运行方式与后续演进方向。

## 4. ⚠️ 遗留问题与注意事项

- 当前版本为基础设施原型，尚未接入真实 AI 服务调用、命令执行、文件树、Diff 预览与流式响应能力。
- `cargo check -p vulcan-code-gui` 已通过，但编译过程仍会显示来自 `third_party/iced` 上游源码的一个 `unused import` warning，本次未修改第三方依赖源码。
- 若下一步继续扩展，建议优先补齐服务层抽象、文件系统浏览、终端输出与任务编排链路。
