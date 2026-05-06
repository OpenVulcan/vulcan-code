# vulcan-code

基于 Rust + 官方 iced 的 AI Coding 桌面端基础设施仓库。

## 当前结构

- `Cargo.toml`：根工作区配置，统一管理 GUI 应用与官方 iced 依赖。
- `apps/vulcan-code-gui`：首版桌面应用 crate，包含状态管理、Pane Grid 布局与常用组件接入。
- `docs/plan`：任务执行计划文件目录。

## 已接入的 iced 组件

- `pane_grid`：搭建三栏式工作台布局。
- `text_editor`：预留补丁预览与代码编辑区域。
- `text_input`：承载工作区路径与 Prompt 输入。
- `pick_list`、`radio`、`checkbox`、`toggler`、`slider`：承载模型、主题与执行策略配置。
- `progress_bar`：展示上下文预算占用。
- `markdown`：渲染内置的集成指引。
- `svg`：渲染应用标识。
- `scrollable`、`container`、`rule`、`tooltip`、`button`：补齐工作台结构与交互细节。

## 运行方式

```bash
cargo run -p vulcan-code-gui
```

## 下一步建议

1. 接入真实命令执行与终端输出流。
2. 接入文件树、Diff 预览与补丁应用能力。
3. 接入 AI 服务协议层、流式响应与多 Agent 编排。
