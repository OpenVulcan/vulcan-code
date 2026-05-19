# Agent View 分块渲染结构修正计划

## 任务目标

修正主对话面板流式输出卡顿问题，将主输出渲染链路从整篇文档级别渲染进一步调整为真正的块级渲染，使历史工具调用、历史思考块、历史文本块不再参与当前流式输出块的重绘与缓存失效。

## 详细执行步骤

1. 梳理当前主对话面板渲染入口，确认流式输出后是否仍然经过整篇 `AgentViewDocument` 渲染与装饰。
2. 将主对话面板拆为多个独立块级 `SelectableTranscript` 组件，每个 block 使用独立 surface id 与渲染缓存。
3. 为 block 级 surface id 增加稳定缓存键，避免流式 Markdown 文本变化导致当前块 surface id 反复变化。
4. 检查工具详情、思考详情、历史块装饰逻辑，确保仍能通过原有 `ui-block-id` 正常打开弹窗。
5. 执行格式化、编译和相关测试，确认改动不会破坏现有 agent-view 与 GUI 构建。

## 技术选型

- 保留现有 `vulcan-agent-view` 渲染组件，不引入新 UI 框架。
- 在 GUI 层完成主对话面的块级组合，优先降低改动风险。
- surface id 使用消息身份与 block 序号生成稳定键，工具弹窗仍使用原有 block id 兼容当前详情索引。

## 验收标准

1. 主对话面板不再将完整 `AgentViewDocument` 作为单个 `SelectableTranscript` 渲染。
2. 每个历史 block 拥有独立渲染缓存，当前流式输出只影响当前 block。
3. 工具调用详情和思考详情弹窗仍可通过 block 装饰属性打开。
4. `cargo check -p vulcan-code-gui` 通过。
5. `cargo test -p vulcan-code-gui` 与 `cargo test -p vulcan-agent-view` 通过，或明确说明无法执行的原因。

## 执行变更总结

### 1. 核心修复与调整概述

- 将主对话面板的渲染入口改为直接遍历缓存中的 `session_agent_view_documents`，每个 `AgentViewBlock` 独立生成一个块级 `SelectableTranscript`，不再把整份文档交给单个组件处理。
- 为每个块级组件增加稳定 surface id 与 lazy dependency，优先使用 `message-id + block_index` 作为缓存键，避免流式 Markdown 正文变化导致组件身份反复变化。
- 主界面渲染工具块时改为生成摘要快照，只保留展示和弹窗定位所需属性，显式剥离工具正文，避免隐藏的大型工具输出进入主转录组件树。
- 保留原有 `ui-block-id` 生成逻辑，工具详情和思考详情弹窗仍通过既有详情缓存与 block id 解析完整内容。

### 2. 📂文件变更清单

- 修改：`apps/vulcan-code-gui/src/app/views/transcript.rs`
- 新增：`docs/completed/20260519/08-AGENT_VIEW_BLOCK_RENDERING_CORRECTION.md`
- 删除：`docs/plan/20260519-08-AGENT_VIEW_BLOCK_RENDERING_CORRECTION.md`

### 3. 💻关键代码调整详情

- 新增 `SessionTranscriptBlockLazyDependency`，用于控制块级 lazy widget 仅在 block 内容、主题或目标命令变化时重建。
- 新增 `session_transcript_block_cache_key`，将块级 surface id 从原来的内容 hash 改为优先依赖稳定消息身份。
- 新增 `session_transcript_inline_render_block`，在主界面工具块渲染前构建轻量摘要块，阻止工具正文进入主面板渲染链路。
- 重写 `view_main_canvas_segmented_transcript_stream`，由整文档渲染改为逐 block 推入 column，每个 block 独立持有渲染缓存。
- 新增 `inline_render_block_strips_tool_body_from_main_transcript` 回归测试，固定主界面工具块不能携带重型正文的性能约束。

### 4. ⚠️遗留问题与注意事项

- 当前已经完成块级组件拆分与工具正文隔离；如果后续仍遇到极端超长会话滚动卡顿，下一步应继续实现真正的虚拟滚动，只布局视口附近 block。
- 块级右键“全选”当前会针对单个 block 生效；如果需要跨 block 全选，需要单独设计会话级选择聚合。

## 验证结果

- `cargo check -p vulcan-code-gui`：通过。
- `cargo test -p vulcan-agent-view`：通过，49 个测试全部成功。
- `cargo test -p vulcan-code-gui`：通过，331 个测试全部成功。
- `git diff --check`：通过，仅出现仓库现有 Windows 行尾提示。
