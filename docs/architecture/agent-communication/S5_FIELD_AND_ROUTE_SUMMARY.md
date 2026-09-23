# S5.3 字段唯一来源、Schema、实际路由与使用说明汇总

本文件是 S5.3 的交付物：把协作与动态上下文的**权威定义位置、工具契约、实际执行路由和用户可见
使用说明**汇总到一处，供审核逐条回查源码。它只做汇总与指路，**不重新定义任何语义**；
与源码不一致时以源码为准，且该不一致本身按缺陷处理。

## 一、权威定义位置（Schema 与版本）

| 概念 | 权威位置 | 说明 |
| --- | --- | --- |
| 协作元数据版本 | `crates/vulcan-session-core/src/v1/collaboration.rs` 的 `COLLABORATION_METADATA_VERSION` | 当前为 `1`；只支持当前新契约，不兼容历史数据（A15） |
| 协作状态结构 | 同文件 `CollaborationState` | 问题、投递、通知、任务消息、任务结果、子任务、答疑执行、授权、运行控制的唯一容器 |
| 协作标识边界 | 同文件 `COLLABORATION_ID_MAX_BYTES` | 所有协作稳定标识共享的上限 |
| 协作正文边界 | 同文件 `COLLABORATION_BODY_MAX_BYTES` | **领域层**正文上限；工具层另有更严的上限，见 §四 |
| 取消原因边界 | 同文件 `COLLABORATION_REASON_MAX_BYTES` | 撤销原因专用上限 |
| 未结判定 | 同文件 `CollaborationState::requires_edit_or_delete_settlement` | 编辑/删除是否受阻的**唯一**判定（S1.3 收敛结果） |
| 问题状态 | 同文件 `CollaborationQuestionState` | `Pending` / `Answered` / `Cancelled`；不存在 `Declined` |
| 授权动作 | 同文件 `CollaborationGrantAction` | `Read` / `Ask` / `Notify` / `Steer` / `Start` / `Reply` |
| 运行控制 | 同文件 `CollaborationRunControl` | `Running` / `NaturallyIdle` / `MainOnly` / `MainAndSubagents` / `Stopped` |
| 协作命令类型 | `crates/vulcan-agent-runtime/src/tooling/collaboration.rs` 的 `AgentCollaborationCommand` | 工具边界到服务边界的类型化命令 |
| 协作工具身份 | 同文件的 `*_TOOL_ID` 常量 | 见 §三 |
| 协作工具边界值 | 同文件的 `MAX_*` / `MIN_*` 常量 | 见 §四 |
| 宿主提示事件前缀 | `crates/vulcan-context/src/runtime_host.rs` 的 `RUNTIME_HOST_MESSAGE_PREFIX` | `runtime-host:`；宿主事件消息标识的唯一权威 |

## 二、字段唯一来源核查（AGENTS.md 一.11）

本轮对**协作边界值**做了重复字面量核查，结论是目前无重复：

| 边界值 | 唯一定义 | 核查方式 |
| --- | --- | --- |
| 协作文本上限 | `MAX_AGENT_COLLABORATION_TEXT_CHARS`（运行时层，`16 * 1024` 字符） | 在 `vulcan-session-core` 全库检索 `16384` 与 `16 * 1024` **零命中**；工具层 Schema 与运行时校验都引用该常量，无第二处字面量 |
| 游标长度上限 | `MAX_AGENT_COLLABORATION_CURSOR_CHARS`（512 字符） | 只出现在常量、Schema `maxLength` 与校验错误文本中 |
| 列表条数上限 | `MAX_AGENT_COLLABORATION_LIMIT`（100） | 只出现在常量、Schema `maximum` 与校验中 |
| 等待超时区间 | `MIN_AGENT_COLLABORATION_TIMEOUT_MS`（1）与 `MAX_AGENT_COLLABORATION_TIMEOUT_MS`（3_600_000） | 由单一 `validate_timeout` 引用；服务内核中的 `3_600_000` 命中属于**无关概念**（`provider_settings` 的请求延迟、VMM 策略测试的时间步长），不是协作超时的第二定义 |

**领域层与工具层存在两个不同的正文上限**（64 KiB 字节 vs 16 Ki 字符）。二者不是同一概念的两处
定义：领域层是持久化守卫，工具层是模型输入守卫，工具层更严。该差异已在此登记；若后续要求二者
一致，应以单一来源派生，而不是各自保留字面量。

## 三、工具契约与实际路由（暴露链逐段）

协作工具共 **11** 个。每个工具的暴露链固定为：**注册 → 请求构建 → 模式可见性 → 策略过滤 →
执行 → 结果投影**。

| 工具 | Schema 构造位置 | 执行路由（服务层） |
| --- | --- | --- |
| `ask_question` | `ask_question_args_schema` | `agent_collaboration::invoke` → `AskQuestion` → `ask_question` |
| `ask_parent` | `ask_parent_args_schema` | 同上 → `AskParent`（子到主方向） |
| `reply_question` | `reply_question_args_schema` | 同上 → `ReplyQuestion` → `reply_question` |
| `wait_question` | `wait_question_args_schema` | 同上 → `WaitQuestion` → `wait_question` |
| `cancel_question` | `cancel_question_args_schema` | 同上 → `CancelQuestion` → `cancel_question` |
| `list_sessions` | `list_sessions_args_schema` | 同上 → `ListSessions` |
| `read_session` | `read_session_args_schema` | 同上 → `ReadSession` |
| `send_session_message` | `send_session_message_args_schema` | 同上 → `SendSessionMessage` → `session_messages` |
| `send_notification` | `send_notification_args_schema` | 同上 → `SendNotification` → `notifications::send_notification` |
| `wait_session` | `wait_session_args_schema` | 同上 → `WaitSession` → `session_wait::wait_session` |
| `wait_agent` | 复用 `vulcan_subagent_core::WAIT_AGENT_TOOL` | **不走 `invoke` 分派**：由运行时的 `wait_for_incoming_question` 专用方法处理（`validate_invocation(..., WAIT_AGENT_TOOL_ID)` 后轮询 `incoming_question_snapshot`） |

- **命令分派**：`crates/vulcan-service-kernel/src/agent_collaboration.rs` 的 `invoke`，
  按上述 **10** 个 `AgentCollaborationCommand` 变体分派（`AskQuestion`、`AskParent`、
  `ReplyQuestion`、`WaitQuestion`、`CancelQuestion`、`ListSessions`、`ReadSession`、
  `SendSessionMessage`、`SendNotification`、`WaitSession`）。`wait_agent` 是第 11 个工具，
  但**没有对应命令变体**，走专用入口，见上表。
  其工具身份来自 `vulcan-subagent-core` 的 `WAIT_AGENT_TOOL`，因此工具身份不存在第二处定义。
- **授权**：`resolve_authorized_target(invocation, target, action)` 按动作逐一判定，
  嵌入目标必须是自有子会话或已校验的直接父会话；跨项目另需显式授权。
- **可靠写入**：生产写路径统一经 `agent_collaboration::lifecycle_write::with_canonical_handles`，
  并由 `SessionRuntimeRegistry::acquire_lifecycle_transition` 持有生命周期门。
- **重建与修复入口**：生产启动扫描经 `recover_indexed_collaboration_owned` 调用
  `reconcile_canonical_collaboration_outbox`（问题、任务消息、通知）和任务结果修复。
  `reconcile_session_v1_collaboration_outbox` 仅为测试加载包装，不是生产启动入口。

## 四、边界值与语义速查

| 项 | 值 | 超出时的行为 |
| --- | --- | --- |
| 协作文本（问题、答案、消息、通知、撤销原因） | ≤ 16384 字符 | 工具校验拒绝，错误文本点明工具与字段 |
| 列表游标 | ≤ 512 字符 | 工具校验拒绝 |
| 列表条数 | ≤ 100 | 工具校验拒绝 |
| 等待超时 | 1 — 3 600 000 毫秒 | 工具校验拒绝；`MIN` 省略不表示无限等待 |
| 协作稳定标识 | ≤ 512 字节 | 领域层拒绝 |
| 正文（领域层） | ≤ 64 KiB 字节 | 领域层拒绝 |
| 撤销原因（领域层） | ≤ 4 KiB 字节 | 即使通过工具字符上限，也须通过领域字节上限 |

**等待语义（最易误用的三点）**：

1. 超时只结束**本次等待**，不撤销问题、不取消会话任务；
2. 被观察会话的修订变化不等于任务完成；
3. 入站问题会把等待打断并返回 `waited_session_id` 与原始 `after_revision`，以便恢复等待。

## 五、用户可见使用说明

- **WebUI 协作卡**：一条协作事件渲染为独立卡片，卡片身份由事件后状态派生
  （`interaction` / `notification` / `task-message` / `task-result`）。通知不提供撤销控件，
  因为通知不携带回复义务；待处理问题提供撤销控件，其可用性要求处理器存在、尚未提交、
  且该问题位于当前事件窗口的可撤销集合中。
- **会话导航**：卡片上的来源/目标按钮只在会话确实可路由时可用，点击以精确会话身份打开，
  标签中不暴露会话 ID。
- **对话框隔离**：代理问答使用独立交互框，不复用面向人类用户的 `ask` 面板；
  通知、问答与真实用户输入在视觉上区分。

## 六、复核入口

- 需求行级证据对照：[S5 验收证据对照](S5_ACCEPTANCE_EVIDENCE.md)
- 工作包级状态与决策记录：[当前实施状态](IMPLEMENTATION_STATUS.md)
- 本轮边界值核查结论：见 §二。

## 七、本文件的边界

- 本文件是**汇总**，不是新契约；任何与源码冲突之处以源码为准并按缺陷处理。
- §二 的核查范围是协作边界值；**未**对全仓所有概念的边界值做穷尽核查。
- §三 的"执行路由"给出服务层入口，未逐行展开模式可见性与策略过滤的实现细节。
- 完整五轮审核仍为 **0/5**，本文件不声明任何一轮通过。

## 八、当前可达协议及动态消息装配（2026 年 9 月 23 日复核）

本节是当前资源与源码快照，不是供应商能力的新权威。清单更新、协议注册或请求编码器变化时必须重新核对。
按 `resources/provider-manifests/manifests/*.json` 的 `services[].protocol_routes[]` 统计，
当前有九种被内置服务声明引用的 Profile；注册表另有 Cohere，但当前内置清单没有该路由。

| Profile | 代表性已声明服务入口 | 动态宿主消息在编码后的载体 |
| --- | --- | --- |
| `openai-chat-completions.v1` | `openai/public-api`、`alibaba/model-studio-cn` | `messages` 中原位置的 `user` 文本 |
| `openai-responses.v1` | `openai/public-api`、`openrouter/public-api` | `input` 中原位置的 `user` 消息 |
| `openai-codex-responses.v1` | `openai/codex-subscription`、`agentspace/chatgpt-plan` | 原生 Responses 输入中的带标签 `user`，不把正文提升为路由或权限 |
| `anthropic-messages.v1` | `anthropic/public-api`、`deepseek/official-api` | `messages` 中 `user` 内容块；相邻同角色按编码器规则合并 |
| `anthropic-messages-video.v1` | `kimi/coding-plan`、`minimax/cn` | 与 Messages 相同的 User 宿主文本；媒体另走声明的媒体投影 |
| `claude-code-messages.v1` | `anthropic/claude-code-subscription` | Messages User 内容块，订阅身份与正文分离 |
| `google.gemini.generate_content` | `google/gemini-api` | `contents` 中 `user` 的文本 parts |
| `gemini-interactions.v1` | `google/gemini-api`、`google/gemini-oauth` | Interactions 输入中用户内容，按其编码器保持事件次序 |
| `google.code_assist.generate_content.v1` | `google/code-assist` | Code Assist 请求包装内的 GenerateContent 用户 parts |

可达条件：入口声明、具体模型选定协议、冻结连接、凭据、模型能力及工具宿主均通过正常校验。
“清单有路由”不等于任意模型都可运行；当前 Codex 代码模式缺少 JavaScript 宿主原语的限制仍保留。
真实用户、助手、工具结果及媒体保留各自原生结构，只有非顶部宿主注入统一为带版本标签的 User。

核查链：

1. 固定顶部与动态生产者分离：`provider_runtime_context.rs`、`context_injection_runtime.rs`；
   Goal、Plan、临时指引、环境和记忆输出须先进入有身份的动态事件。
2. 规范历史与 V1 发布：`provider_request_bridge_messages/projection.rs`、
   `provider_v1_sink/context_injection.rs`；未在准备屏障持久确认的动态事件不能先行物理发送。
3. 相邻请求：`runtime_stream_launch_payload.rs::two_adjacent_requests_keep_fixed_top_and_append_dynamic_user_items`；
   TTL 验收检查保留旧正文并追加失效事件。
4. 九 Profile 最终编码：`execution/tests/host_message_projection.rs::production_host_user_envelopes_preserve_top_prefix_and_tool_pairs`
   使用实际执行客户端编码，验证固定顶部、动态顺序、原生工具配对及媒体。
5. 真实 TLS 压缩及答疑用例补服务到网络的组合证据；九 Profile 的全面覆盖仍属于确定性编码验收。

上述分层证据不声称已测真实供应商缓存命中率、真实订阅账号刷新或所有媒体上传端点。
