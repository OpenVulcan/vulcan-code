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
| 运行控制 | 同文件 `CollaborationRunControl` | `NaturallyIdle` / `MainOnly` / `MainAndSubagents` / `Stopped` |
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
- **重建与修复入口**：`reconcile_session_v1_collaboration_outbox`（问题、任务消息、通知）、
  `reconcile_task_results_for_session`（任务结果）、`startup_recovery`（冷启动扫描）。

## 四、边界值与语义速查

| 项 | 值 | 超出时的行为 |
| --- | --- | --- |
| 协作文本（问题、答案、消息、通知、撤销原因） | ≤ 16384 字符 | 工具校验拒绝，错误文本点明工具与字段 |
| 列表游标 | ≤ 512 字符 | 工具校验拒绝 |
| 列表条数 | ≤ 100 | 工具校验拒绝 |
| 等待超时 | 1 — 3 600 000 毫秒 | 工具校验拒绝；`MIN` 省略不表示无限等待 |
| 协作稳定标识 | ≤ 512 字节 | 领域层拒绝 |
| 正文（领域层） | ≤ 64 KiB 字节 | 领域层拒绝 |

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
