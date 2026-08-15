# Ask 与 Markdown 交互约定

本文仅在修改 `ask` 工具、提问运行时、Ask WebUI、只读 Markdown 预览或会话 Markdown 渲染时阅读。

当前源码权威入口：

- [`question_runtime.rs`](../crates/vulcan-agent-runtime/src/agent_runtime/question_runtime.rs)；
- [`tooling/bridge.rs`](../crates/vulcan-agent-runtime/src/tooling/bridge.rs)；
- [`AskQuestionDialog.tsx`](../apps/vulcan-code-web/src/components/ask/AskQuestionDialog.tsx)；
- [`conversation-markdown.tsx`](../apps/vulcan-code-web/src/components/conversation/conversation-markdown.tsx)；
- [`markdown.css`](../apps/vulcan-code-web/src/styles/markdown.css)。

## 一、Ask 工具边界

- 模型可见工具只注册 `ask`，不保留 `question` 别名。
- `ask` 的 description、Schema 和参数说明使用英文。
- 工具最终模型输出使用 Markdown。
- 运行时必须复用共享 `QuestionRuntime`；刷新 Runtime Foundation 或启动流式请求时不能重建并丢失
  等待中的问题。
- 关闭或放弃 Ask 必须先进入明确的拒绝确认，再中断工具调用和后续对话执行。

## 二、当前键盘状态机

- `Esc`：答题阶段进入放弃确认；确认阶段返回答题。
- `Enter`：答题阶段校验当前题并前进；提交确认阶段提交；放弃确认阶段确认放弃。
- `Shift+Enter`：在文本输入中保留换行，不触发前进。
- `←/→`：答题阶段切换题目。
- `↑/↓`：非文本输入焦点下移动选项。
- `Space`：答题阶段切换当前单选或多选项。

键盘行为必须以当前 `AskQuestionDialog.tsx` 状态机为准。修改后同步更新本节，不能继续引用早期
“Space 在确认页提交”等已经失效的交互。

## 三、文本题、选项题与确认

- 纯文本题进入后直接显示并聚焦输入框。
- 带选项题可以保留“其他回答”，只有选中后才展开自定义输入。
- 提交前必须校验全部必答题；存在未完成项时回到精确题目和错误位置。
- 提交确认与放弃确认是两个明确状态，不能用关闭按钮绕过。
- Ask 面板必须限制在当前会话区域，不能覆盖或劫持其他窗口的键盘输入。

## 四、Markdown

- 会话 Markdown 复用当前 `conversation-markdown` 渲染链路，不建立第二套解析器。
- 需要“可选择、可复制、只读”的预览时，使用可选择文本或只读编辑区，不退回不可复制的普通展示。
- 仅显示 Markdown 外观但用户实际需要复制内容时，也必须保留文本选择能力。
- Markdown 样式修改应作用于精确范围，不能污染代码块、工具卡片或其他非会话区域。

## 五、修改后的最低验证

- `ask` 是唯一模型可见提问工具；
- 共享 `QuestionRuntime` 在刷新和流式启动后仍保留等待请求；
- 键盘、文本题、单选、多选、其他回答、提交确认和放弃确认均有测试；
- Markdown 可选择、可复制且经过当前会话渲染链路；
- overlay、焦点和快捷键不影响会话之外的控件。

