# 回答模式与回答组件

## 权威与同步触发点

本契约归属于 `vulcan-session-core::v1::document::answer_mode`（会话偏好与上下文同步）、
`vulcan-session-core::v1::answer_components`（组件容量边界）与运行时资源
`crates/vulcan-runtime-assets/resources/agent-prompts/answer-mode-interactive.md`（组件语法的唯一
权威）。修改模式语义、持久字段、注入方式或组件语法时，必须同步本文、互动提示词、WebUI 与 TUI 两个
渲染器及其行为测试。

## 模式语义

- 回答模式只有 `standard`（默认）与 `interactive` 两值，与输出风格相互独立：输出风格决定语气与
  篇幅，回答模式只决定是否允许使用 `ui-*` 组件。
- `SessionConfig.answer_mode` 是会话唯一权威，含 `mode`、独立比较交换 `revision` 与可空
  `prepared` 快照。`service_set_session_answer_mode` 接收 `sessionId`、`mode`、`expectedRevision`，
  冲突时返回当前值，不自动覆盖或重试；生成中只保存期望，下一请求生效。
- 全局默认存于提示词配置 `answerMode`（缺省即 `standard`，不回写），通过
  `service_save_prompt_answer_mode` 保存，并在工作台快照 `defaultAnswerMode` 中投影。
- **快照语义**：新会话在首次提交物化时一次性取得草稿显式选择（`sessionSettings.answerMode`），
  缺省时复制当时的全局默认。之后修改全局默认不影响任何已有会话；明确分叉继承期望并重置准备快照。
- 元数据模式 3 → 4 将旧 `html_answers.enabled` 映射为 `interactive`/`standard`，见
  [元数据迁移](session-metadata-migrations.md)。

## 提示词注入

请求准备通过动态上下文键 `dynamic.html_answers`（沿用已持久化的键值）注入：互动模式注入
`answer-mode-interactive.md`；切回标准模式时，仅当该键曾经存在才注入 `answer-mode-standard.md`
停止说明。从未开启互动模式的会话不发送任何额外说明。重复同步幂等。互动提示词中的
`{{max_rows}}` 与 `{{max_series}}` 由 `ANSWER_COMPONENT_MAX_ROWS`、`ANSWER_CHART_MAX_SERIES` 派生。

## 组件渲染

组件是信息串为 `ui-<name>` 的围栏代码块，名称与语法以互动提示词为准，渲染器不得接受文档外的
名称。规则：

- 只渲染已闭合的围栏；结束围栏的反引号数不少于开始围栏，容器内嵌组件需用更长的外层围栏。
  流式中尚未闭合或语法无效的围栏一律保持代码显示，不猜测修复。
- 渲染只基于回复原文，不经服务端投影，不执行任何脚本；需要真实交互时由模型写出独立 HTML 文件并以
  链接交给内置浏览器打开。
- WebUI 由 `apps/vulcan-code-web/src/components/conversation/answer-components/` 原生绘制：
  `answer-component-model.ts` 是纯解析器（与 TUI 解析规则一致），`conversation-markdown.tsx` 的
  `MarkdownPreBlock` 依据 AST 源码位置确认围栏已闭合后才交给组件。流式回复中已闭合的组件即时渲染；
  Mermaid 仍只在回复完成后绘制。TUI 由 `apps/vulcan-code-tui/src/ui/answer_components.rs` 改写为
  终端可读的 Markdown（状态词转符号、图形组件转 Mermaid、树形带引导线、容器正文置于着色竖线之后）。
- 视觉分两类（样式在 `src/styles/answer-components.css`）：**框架型**承载可复用数据——Markdown
  表格、`ui-findings`、`ui-compare`（悬停复制为 Markdown）、`ui-chart`（图表与数据切换、复制数据）、
  `ui-files`（标题条显示文件数与增删合计）、`ui-tree`；**融入型**随正文阅读——其余组件。普通 Markdown
  表格同样使用框架型外框，纯数字列靠右；Mermaid 使用 `base` 主题并从工作台色票取色。
- 文件位置（`ui-findings` 位置、`ui-files` 路径）仅在可由项目根解析时成为 `file:` 链接，行号后缀只用于
  显示；图库路径按 Markdown 图片的同一规则解析为本地媒体地址。
- 大图仍使用可缩放的 Mermaid；`ui-flow` 与 `ui-sequence` 是嵌入正文的扁平图：`ui-flow` 每行源码
  平铺为一条链（同名即同一节点，只出现在新增或删除行上的节点带对应标记），`ui-sequence` 以参与者列与
  生命线排布消息、注释与阶段。
