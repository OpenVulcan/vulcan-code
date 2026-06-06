# 当前任务执行记录

## 任务来源

- 计划文档：`D:/openvulcan/vulcan-code/docs/plan/20260605-03-SUBAGENT_TASK_SESSION_ARCHITECTURE.md`
- 当前目标：完整实现子代理 TaskSession 架构，覆盖 iced 与 WebUI，并在每个功能节点后自动审核与修复。

## 已确认架构决策

1. TaskSession 完全禁止普通会话数据库投影，仅使用文件态。
2. 子代理默认跳过 conversation hook，并保持 VMM、记忆、search 投影关闭。
3. 子代理支持默认继承父配置、profile 配置、task invocation 对象覆盖三类入口。
4. 旧 payload 不做自动迁移；缺少 TaskSession 文件时只展示 task 返回记录或不可复看提示。
5. 子代理允许通过 profile 或 invocation 显式声明特殊能力、技能、LuaSkills、MCP 与工具集合。

## 当前工作节点

### 节点 1：计划读取与仓库取证

- 状态：已完成。
- 已读取计划文档目录结构，确认执行顺序为：
  1. `vulcan-session-core` 公共 crate 与协议层。
  2. `vulcan-main-session` 主代理 crate。
  3. `vulcan-task-session` 子代理 crate 与文件层。
  4. runtime 执行适配、配置解析、压缩、共享路由、旧 payload 降级、清理旧逻辑。
- 已检查 workspace 根 `Cargo.toml`，当前尚未包含 `vulcan-session-core`、`vulcan-main-session`、`vulcan-task-session` 三个目标 crate。
- 已确认 `TASK_NOW.md` 之前不存在，本文件为本轮创建。
- 已用 CodeKit 盘点相关链路：
  - `vulcan-agent-runtime/src/agent_runtime/task_runtime.rs` 目前承载主要子代理调度逻辑。
  - `vulcan-workbench-model/src/subagent.rs` 承载子代理显示状态与 task payload 解析。
  - iced GUI 的 `app_shell/subagent_runtime.rs` 与 `pending_prompt_queue.rs` 存在子代理状态、停止、队列阻塞逻辑。
  - Tauri `service_bridge.rs` 复制了大量 WebUI 子代理缓存、停止、恢复和 detached listener 逻辑。
  - WebUI 前端通过后端 `service_open_subagent_panel`、`service_close_subagent_panel`、`service_stop_subagent_task` 显示子代理导航。

### 节点 2：公共 crate 与协议层

- 状态：已完成第一版。
- 新增 `crates/vulcan-session-core`，并加入 workspace。
- 新增公共模块：
  - `model.rs`：`SessionNodeId`、`RootSessionId`、`ParentSessionRef`、`SessionNodeKind`、`SessionTreePath`。
  - `event.rs`：`SessionEventKind`、`SessionEventRecord`。
  - `context.rs`：`SessionContextDocument`、`SessionContextSnapshot`、`SessionCompactionCheckpoint`、`SessionRuntimeCheckpoint`。
  - `config.rs`：`SessionRequestedConfig`、`SessionEffectiveConfig`、工具、技能、模型、压缩策略快照。
  - `policy.rs`：`ExecutionScope`、`ExecutionRuntimePolicy`。
  - `executor.rs`：`SessionExecutionRequest`、`SessionExecutionEvent`、`SessionExecutionStream`、`SessionExecutionOutcome`、`SessionNodeExecutor`。
  - `store.rs`：`SessionNodeStore`。
  - `router.rs`：`SessionNodeRouter`、`SessionNodeViewSnapshot`。
- 已补单元测试：
  - TaskSession policy 禁用 hook/VMM/记忆/search/database projection。
  - requested config 默认继承父配置。
  - root path 目标为 root session。
  - event record JSON 往返序列化。

### 节点 3：主代理 crate 边界

- 状态：已完成第一版。
- 新增 `crates/vulcan-main-session`，并加入 workspace。
- 新增主代理私有边界：
  - `runtime_policy.rs`：主会话运行时策略包装，默认启用 hook/VMM/记忆/search/database projection。
  - `projection.rs`：主会话数据库投影、search、BM25、FTS 策略。
  - `task_entry.rs`：主会话侧子代理入口索引，只保存子代理 ID、公开 task ID、状态、摘要和反馈，不嵌入 transcript。
  - `store.rs`：`MainSessionStore` 继承公共 `SessionNodeStore`，增加父级 task entry 索引操作。
  - `router.rs`：`MainSessionRouter` 继承公共 `SessionNodeRouter`，增加 root task entry 查询操作。
- 已补单元测试：
  - 主会话 runtime policy 默认开启普通副作用。
  - 主会话 projection policy 默认保持普通索引行为。
  - 主会话 task entry 只索引 child TaskSession。

### 节点 4：子代理 crate 与文件态边界

- 状态：已完成第一版。
- 新增 `crates/vulcan-task-session`，并加入 workspace。
- 新增子代理私有边界：
  - `model.rs`：`TaskSessionId`、`TaskSessionStatus`、`TaskSessionSummary`、`TaskSessionMetadata`、`TaskInvocationEvent`、`TaskFeedback`。
  - `profile.rs`：`SubagentProfile`，支持继承开关、独立系统提示词、模型、工具、技能、LuaSkills、MCP、压缩策略覆盖。
  - `config_resolver.rs`：`TaskInvocationConfig`、`TaskSessionConfigResolver`，按“全局默认 < 父配置 < profile < invocation”解析有效配置。
  - `store.rs`：`TaskSessionStore`，只定义文件态 task-session 元数据、父级 task.jsonl、events/context/checkpoint 相关存储契约。
  - `file_store.rs`：`FileTaskSessionStore`，实现主会话目录下 `tasks/` 文件域、`task-index.json`、父级 `task.jsonl`、子代理 `events.jsonl`、`context/current.json`、`runtime_state.json` 和 checkpoints 读写。
  - `router.rs`：`TaskSessionRouter`，定义 iced/WebUI 共用的打开、停止、恢复、历史分页和活动列表路由。
  - `scheduler.rs`：`TaskSessionScheduler`，定义创建、恢复、停止、级联停止和活动任务查询契约。
  - `compaction.rs`：TaskSession 压缩任务与压缩产物存储契约。
  - `legacy_fallback.rs`：旧 task payload 只读降级，不迁移、不生成 TaskSession 目录。
- 补充公共层 `SessionContextCompactor`，避免子代理自行创建压缩接口。
- 已补单元测试：
  - TaskSession 默认继承父配置，但 runtime policy 关闭 hook/VMM/记忆/search/database projection。
  - profile 可关闭父配置继承。
  - invocation 覆盖 profile 的模型、工具和系统提示词。
  - 旧 payload 缺少 TaskSession 目录时只展示持久化返回记录，不迁移。
  - TaskSession metadata 固化文件态隔离策略。
  - 文件态 store 可创建、保存、读取 TaskSession 元数据，追加/读取事件，保存/读取 context，并维护活动任务摘要索引。

### 节点 5：执行 scope/policy 接入

- 状态：已完成第一版。
- 已在 `vulcan-session-core` 增加 `ExecutionRuntimePolicy::context_compaction()`，用于压缩请求复用隔离策略。
- 已在 `ExecutionPreparationRequest` 中新增：
  - `execution_scope`：主会话、TaskSession、ContextCompaction 的共享执行范围。
  - `runtime_policy`：本次执行请求的 hook/VMM/记忆/search/database projection 策略。
- 已调整普通主会话请求构造：
  - `workbench_prompt_turn.rs` 使用 `ExecutionScope::MainSession` 与 `ExecutionRuntimePolicy::main_session()`。
  - API 请求预览使用 `ExecutionScope::MainSession` 与主会话策略。
- 已调整上下文压缩请求构造：
  - prompt compaction 使用 `ExecutionScope::ContextCompaction` 与 `ExecutionRuntimePolicy::context_compaction()`。
- 已调整 `prepare_execution`：
  - 当 `runtime_policy.vmm_enabled = false` 时传入空 VMM 配置。
  - 当 `runtime_policy.hooks_enabled = false` 时清空 system prompt hook blocks。
  - 普通主会话默认行为保持不变。

### 节点 6：Task invocation 配置接入

- 状态：已完成第一版。
- 已在 task 工具参数中新增子代理配置覆盖入口：
  - `subagent_profile_id`：指定子代理 profile。
  - `inherit_parent_config`：控制是否继承父配置。
  - `system_prompt`：指定子代理独立基础系统提示词。
  - `system_prompt_append`：追加子代理系统提示词片段。
- 已将 `TaskRuntime` 内部 task record 与 execution request 接入 `vulcan-task-session::TaskInvocationConfig`。
- 已保持默认行为：未传上述配置时仍按父配置 clone 语义运行。
- 已处理恢复语义：恢复已存在 task 时，新 invocation 只覆盖显式传入字段，不会清空旧记录中的 profile 或系统提示词配置。
- 已将子代理独立系统提示词路由到现有 prompt composer：
  - 无 functional prompt 时使用 `SubagentClone`。
  - 有 `system_prompt` 或 `system_prompt_append` 时使用 `SubagentFunctional`。
  - 不在 `TaskRuntime` 内额外复刻 prompt 拼接逻辑。

### 节点 7：TaskSession 文件态运行接入

- 状态：已完成第一版。
- 已在 workspace-store 暴露 `session_event_store_directory(project_id, session_id)`，作为主会话文件域统一路径来源。
- 已在 `ExecutionPreparationRequest` / `ExecutionPreparation` 增加 `task_session_store_root`，由宿主真实执行路径填入。
- 已在 iced GUI 与 WebUI/Tauri 真实会话执行路径填入同一个主会话 event-store 目录；API 预览与上下文压缩保持 `None`，避免误写 task 文件域。
- 已在 `TaskRuntime` 增加 root-session 到文件域根目录的注册表，provider 执行开始前注册，嵌套子代理继承同一个根目录。
- 已修正嵌套子代理根 ID：`TaskExecutionRequest` 新增 `root_session_id`，`current_session_id` 继续表示直接调用方。
- 已在 task 运行状态变化时写入 `FileTaskSessionStore`：
  - `task-session.json`
  - 父级 `task.jsonl`
  - 子代理 `events.jsonl`
  - `context/current.json`
  - `runtime_state.json`
- 文件态写入失败当前记录 stderr，不中断模型执行，避免因文件系统瞬时问题破坏主对话。

### 节点 8：TaskSession 文件态复看与导航路由接入

- 状态：已完成第一版。
- 已在 `vulcan-workbench-model` 增加共享转换入口：
  - `subagent_run_state_from_task_session_status`
  - `subagent_task_panel_state_from_task_session`
- 已在 `vulcan-workbench-model` 增加共享 store 路由入口：
  - `subagent_task_panel_states_from_task_session_store`
- 共享路由入口负责读取父级 `task.jsonl`、子代理 `task-session.json`、`context/current.json` 与最新生命周期 event payload，并统一转换为 iced/WebUI 共用的 `SubagentTaskPanelState`。
- 已让 iced GUI 的 `rebuild_subagent_task_cache_for_session` 优先读取 `FileTaskSessionStore`：
  - GUI 只负责根据项目归属构造 `FileTaskSessionStore`。
  - 具体状态列表读取交由共享 `subagent_task_panel_states_from_task_session_store`。
  - 旧 task metadata payload 只作为 fallback，且不会覆盖文件态同 ID 状态。
- 已让 WebUI/Tauri 的 `rebuild_webui_subagent_task_cache_for_session` 使用同一共享 store 路由入口和同一 `FileTaskSessionStore`。
- 已修复 WebUI 已完成子代理复看路由：
  - 已打开的子代理只要有文件态/内存态 transcript detail，就继续作为 active surface。
  - `SubagentNavigationSnapshot` 在已打开可复看子代理时保留 active task id，避免前端把消息错误贴回主会话。
- 保留旧 payload 降级语义：缺少 TaskSession 文件态时，仍只从已有 task 返回记录展示可用内容，不迁移旧数据。

### 节点 9：子代理运行中队列回流与复看入口修正

- 状态：已完成第一版。
- 已在 `vulcan-workbench-model` 增加共享队列优先级入口：
  - `pending_prompt_dispatch_prefers_delegate_completion`
  - `PendingSessionPromptSource::starts_user_parent_turn`
- 队列语义调整为：
  - Anthropic 运行时继续保留 iced 原有 delegate completion 优先闭合语义。
  - 任意 Provider 下，只要同一父会话仍有运行中子代理，内部队列会优先分发 `DelegateCompletion`。
  - 用户跟进仍会被 `RunningSubagents` 阻塞，直到没有运行中子代理或队列中可先闭合的 delegate completion。
- 已让 iced GUI 的 peek 与 dequeue 均使用同一个共享 helper，避免“看见 delegate、取走 user”的顺序错位。
- 已让 WebUI/Tauri 的下一条队列选择使用同一共享 helper。
- 已修复 WebUI/Tauri 在 delegate completion 回流时清空子代理导航轮次的问题：只有真正用户来源的新父会话轮次才清空当前子代理轮次。
- 已让 WebUI 转录中的 task 工具卡片读取 `task_session_id` 后调用现有 `service_open_subagent_panel` 路由；缺少 `task_session_id` 的旧 payload 仍保留普通工具详情弹窗。
- 已补 WebUI 标题栏子代理菜单 hover 热区，避免从机器人按钮移动到竖向列表时闪退。

### 节点 10：清理父 task payload 内嵌子代理转录新写入路径

- 状态：已完成第一版。
- 已移除 `build_task_host_payload` 对 `conversation_state` 的入参和写入。
- 已移除完成、停止、失败等子代理终态事件 payload 中的完整子代理转录嵌入。
- 已调整后台子代理终态处理顺序：
  - 先通过 `store_task_outcome` / `mark_task_failure` 写入文件态 TaskSession。
  - 再发布 `DelegateFinished` 给父会话和 WebUI/iced 路由。
  - 这样父级 payload 不再承载完整转录后，复看仍能优先从文件态 context 读取。
- 已移除 `build_stopped_task_session_metadata` 对 `conversation_state` 的新写入。
- 旧 payload 只读降级仍保留：如果历史 payload 里已有 `conversation_state`，仍可作为旧数据 fallback 读取，但不会再生成新的内嵌 transcript payload。

## 验证结果

- `cargo test -p vulcan-session-core`：通过，无诊断。
- `cargo test -p vulcan-main-session`：通过，无诊断。
- `cargo test -p vulcan-session-core -p vulcan-task-session -p vulcan-main-session`：通过，无诊断。
- `vulcan-task-session` 隔离边界文本审计：命中项仅为测试断言，没有数据库、BM25、FTS、workspace history 或搜索投影实现。
- `cargo test -p vulcan-task-session`：文件态 store 测试通过，无诊断。
- `cargo test -p vulcan-agent-runtime task_runtime::tests::`：通过，存在 3 个既有 VMM dead code warning，与本次改动无关。
- `cargo test -p vulcan-agent-runtime runtime_stream_launch_payload::tests::`：通过，存在同上既有 warning。
- `cargo check -p vulcan-code-gui`：通过，无诊断。
- `cargo check -p vulcan-code-tauri`：通过，无诊断。
- `TaskSession` 文件态运行接入测试：`cargo test -p vulcan-agent-runtime task_runtime::tests::` 通过，存在 3 个既有 VMM dead code warning，与本次改动无关。
- `ExecutionPreparationRequest` 构造点搜索：仅剩四个实际构造点，均已补齐 `execution_scope` 与 `runtime_policy`。
- `cargo test -p vulcan-workbench-model`：通过，无诊断。
- `cargo test -p vulcan-task-session`：通过，无诊断。
- `TaskSession` 复看与导航路由接入：`cargo check -p vulcan-code-gui` 通过，无诊断。
- `TaskSession` WebUI 文件态路由接入：`cargo check -p vulcan-code-tauri` 通过，无诊断。
- `cargo test -p vulcan-workbench-model running_subagents_force_delegate_dispatch_priority`：通过，无诊断。
- `cargo test -p vulcan-workbench-model pending_prompt`：通过，无诊断。
- `npm run lint`（`apps/vulcan-code-web`）：通过，无诊断。
- `cargo check -p vulcan-code-gui`：本轮补丁后再次通过，无诊断。
- `cargo check -p vulcan-code-tauri`：本轮补丁后再次通过，无诊断。
- `cargo test -p vulcan-agent-runtime task_invocation_result_omits_embedded_conversation_state`：通过，无诊断。
- `cargo test -p vulcan-workbench-model stopped_task_session_metadata_omits_embedded_conversation_state`：通过，无诊断。
- `cargo test -p vulcan-agent-runtime task_runtime::tests::`：本轮补丁后通过，仍仅存在既有 VMM dead code warning。
- `cargo test -p vulcan-workbench-model`：本轮补丁后通过，无诊断。
- `cargo check -p vulcan-code-tauri`：本轮补丁后通过，无诊断。
- `cargo check -p vulcan-code-gui`：本轮补丁后通过，无诊断。

## 下一步

### 节点 15：TaskSession 工具曝光覆盖进入共享准备链路

### 2026-06-06 节点 38：WebUI 停止收尾投影对齐 iced

状态：已完成。

本节点发现并修复 WebUI/Tauri 停止执行链条与 iced 不一致的问题：

1. iced 在停止活跃根执行、运行中子代理、precheck 或内部系统队列时，会将停止作为一轮规范会话收尾写入 transcript：
   - 可见 `执行已停止` 消息。
   - 隐藏 `TurnTerminated` 记录。
   - 可见 `Turn Stats · stopped` 统计记录。
   - 随后执行单会话完整持久化。
2. WebUI 原先只清理 runtime state、设置 `SessionRunPhase::Stopped`，并执行 runtime-state-only 持久化，没有写入同等的规范转录收尾记录。
3. 已将 `build_session_record_message_id` 上提到 `vulcan-workbench-model`，iced 与 WebUI 共用同一会话记录 ID 构造规则。
4. 已让 WebUI `stop_webui_session_work` 按 iced 语义判断是否确实存在活跃工作；空闲误点不追加历史记录。
5. WebUI 停止时现在追加同 iced 等价的可见停止消息、隐藏 `TurnTerminated` 与 stopped `TurnStats`。
6. WebUI stopped `TurnStats` 使用共享 `vulcan_agent_runtime::turn_stats::build_session_turn_stats_from_state` 构建，不在 Tauri 层复刻统计逻辑。
7. WebUI 停止后改为 `persist_workspace_session_update` 单会话完整保存，确保规范记录、父 task 卡片 metadata 和运行状态一起落盘。

本节点自审结论：

1. WebUI 新增代码只承担 Tauri workspace snapshot 的容器适配；停止统计、消息类型、ID 规则均复用共享结构。
2. 该修复没有改变 TaskSession 文件态隔离策略，也没有给 TaskSession 增加数据库或搜索投影。
3. 相比 iced，WebUI 停止路径之前缺少规范 turn closing projection；现在已对齐主要执行语义。
4. 当前仍需继续审核剩余 WebUI/Tauri 子代理桥接是否还有类似“只改运行态、不写共享规范记录”的差异。

本节点验证：

1. `cargo fmt`：通过。
2. `cargo test -p vulcan-code-tauri webui_stop_session_work_appends_stopped_turn_projection_once -- --nocapture`：通过。
3. `cargo test -p vulcan-workbench-model runtime_intervention_guidance -- --nocapture`：通过。
4. `cargo test -p vulcan-workbench-model subagent::tests:: -- --nocapture`：通过。
5. `cargo check -p vulcan-code-tauri`：通过。
6. `cargo check -p vulcan-code-gui`：通过。
7. `git diff --check`：无空白错误，仅有 Windows 换行提示。

### 2026-06-06 节点 39：WebUI 单独停止子代理父会话持久化对齐

状态：已完成。

本节点继续审核停止链条，发现并修复 WebUI 单独停止子代理后的持久化差异：

1. iced 的 `StopSelectedSubagentTask` 在停止成功后会收集受影响的直接父会话，并对普通父会话执行单会话持久化。
2. WebUI `service_stop_subagent_task` 之前只通过 `with_workspace_mutation` 更新内存中的父 task 卡片 metadata 和子代理停止提示，随后直接返回 workbench snapshot，没有保存普通父会话。
3. 该差异会导致重启后 WebUI 父 task 卡片可能恢复成旧的 running/completed metadata，尤其影响用户手动停止子代理后的状态可追踪性。
4. 已在 WebUI 停止子代理时计算 `stopped_task_id_set` 和 `stopped_parent_session_ids`。
5. 已仅对存在于普通 `workspace_snapshot.sessions` 中的父会话执行 `persist_workspace_session_update`。
6. 子代理自身仍不进入普通会话数据库投影；如果父级是 TaskSession，则不调用普通 workspace-store 保存，继续遵守 TaskSession 文件态隔离原则。

本节点自审结论：

1. 修复只补 WebUI 缺失的普通父会话持久化，没有给 TaskSession 增加数据库保存路径。
2. 停止执行仍走共享 `TaskRuntime::stop_task_tree` 与共享状态标记结构，没有新增 WebUI 私有停止语义。
3. iced 和 WebUI 在“单独停止子代理后父 task 卡片 stopped metadata 应落盘”这一点已对齐。

本节点验证：

1. `cargo fmt`：通过。
2. `cargo check -p vulcan-code-tauri`：通过。
3. `cargo test -p vulcan-code-tauri webui_stop_session_work_appends_stopped_turn_projection_once -- --nocapture`：通过。
4. `cargo check -p vulcan-code-gui`：通过。
5. `cargo test -p vulcan-workbench-model subagent::tests:: -- --nocapture`：通过。
6. `cargo test -p vulcan-workbench-model runtime_intervention_guidance -- --nocapture`：通过。
7. `git diff --check`：无空白错误，仅有 Windows 换行提示。

- 状态：已完成第一版。
- 已新增 `vulcan-session-core::selector`，提供统一工具 selector 规范化与匹配入口，TaskRuntime 不再保留私有 selector 归一化。
- 已在 `ExecutionPreparationRequest` 保留请求级 `tool_exposure_override`，并由 TaskSession effective config 写入该字段。
- 已在 `execution_preparation.rs` 新增共享 `resolve_execution_tool_exposure`：
  - 统一解析 native tools、LuaSkills direct tools、LuaSkills implicit dispatcher entries、LuaSkills prompt entries、MCP tools 与 AI capability 配置。
  - 支持全局 allow/deny selector。
  - 支持 LuaSkills JSON `enabled/disabled` 等专属覆盖。
  - 支持 MCP JSON tool/server 专属覆盖。
  - 支持 AI capability allowed/disabled 覆盖并落到 `disabled_capability_kinds`。
- 已让 `prepare_execution` 与 `runtime_stream_bootstrap_resources` 均调用同一个 `resolve_execution_tool_exposure`，避免 preparation 预览与 bootstrap 真实 provider/tool-manager 链路不一致。
- 已从 `bootstrap_runtime_stream` / `RuntimePreparedLaunchInput` 移除独立传递的 AI tool config，bootstrap 统一从 request 覆盖推导生效配置。
- 已删除 TaskRuntime 中 `apply_task_tool_exposure_to_child_launch`、`filter_task_tool_definitions` 等启动后补丁式过滤逻辑，避免出现第二套工具曝光语义。
- 已给 `McpRuntimeToolBinding` 增加只读 `origin_tool_id`、`server_id`、`origin_tool_name` 访问器，用于共享 MCP selector 过滤，不改 MCP 调用语义。

#### 节点 15 验证

1. `cargo check -p vulcan-agent-runtime`：通过，无诊断。
2. `cargo test -p vulcan-agent-runtime execution_tool_exposure_ -- --nocapture`：通过，仅有既有 VMM dead code warning。
3. `cargo test -p vulcan-agent-runtime task_runtime::tests:: -- --nocapture`：通过，仅有既有 VMM dead code warning。
4. `cargo test -p vulcan-agent-runtime prepare_execution_ -- --nocapture`：通过，仅有既有 VMM dead code warning。
5. `cargo check -p vulcan-code-gui`：通过，无诊断。
6. `cargo check -p vulcan-code-tauri`：通过，无诊断。
7. `cargo test -p vulcan-agent-runtime mcp::runtime::tests:: -- --nocapture`：通过，仅有既有 VMM dead code warning。
8. `cargo test -p vulcan-session-core selector -- --nocapture`：通过，无诊断。
9. `git diff --check`：仅输出 Windows 换行提示，无空白错误。

## 下一步

1. 继续按照计划进入后续节点：检查 TaskSession 文件态压缩、长期任务上下文状态与子代理 profile 路由是否还有未接入共享 executor 的残留。
2. 继续保持规则：除显示层外，子代理执行逻辑必须复用共享运行链，不允许在 WebUI/iced/TaskRuntime 内克隆独立 provider/tool/prompt 组装逻辑。
3. 后续将 iced 与 WebUI 共同改为调用共享 router，删除各自 task payload 推导逻辑。

### 节点 16：TaskSession 文件态恢复与压缩存储契约

- 状态：已完成第一段。
- 已完成内容：
  1. `TaskSessionStore` 新增按直接父级与公开 task id 查找文件态 TaskSession 元数据的共享契约。
  2. `FileTaskSessionStore` 通过 `task-index.json` 实现 `find_task_session_by_parent_and_public_id`，不扫描 UI payload、不访问数据库、不迁移旧数据。
  3. `FileTaskSessionStore` 实现 `TaskSessionCompactionStore`，将压缩后的 context 与 checkpoint 写回 TaskSession 自己的文件域。
  4. `TaskRuntime` 恢复已有 task 时，先匹配直接父级内存记录；内存缺失时再通过文件态 store 查找同一父级下的公开 task id。
  5. `TaskRuntime` 可从 `context/current.json` 恢复子代理的 provider/model/system prompt/conversation_state，继续通过共享执行桥续跑。
  6. 损坏的文件态 `conversation_state` 会显式报错，不静默退化为空上下文执行。
- 当前自审结论：
  1. 文件态恢复逻辑只消耗 `vulcan-task-session` store 契约，没有在 iced/WebUI 增加子代理恢复规则。
  2. 公开 task id 的查找按直接父级隔离，允许不同父级拥有同名子代理，符合 TaskSession 树语义。
  3. 当前只完成压缩结果存储端，自动压缩调度尚未接入；下一步应通过共享 `SessionContextCompactor` 进入 TaskSession 压缩流程。
- 当前验证：
  1. `cargo test -p vulcan-task-session file_store_round_trips_task_session_artifacts -- --nocapture`：通过。
  2. `cargo test -p vulcan-agent-runtime prepare_task_execution_request_restores_file_backed_context_for_resume -- --nocapture`：通过，仅有既有 VMM dead code warning。
  3. `cargo test -p vulcan-task-session -- --nocapture`：通过。
  4. `cargo test -p vulcan-agent-runtime task_runtime::tests:: -- --nocapture`：通过，仅有既有 VMM dead code warning。
  5. `cargo fmt`：已执行。
  6. `cargo check -p vulcan-agent-runtime`：通过。
  7. `cargo check -p vulcan-code-gui`：通过。
  8. `cargo check -p vulcan-code-tauri`：通过。

## 审核记录

### 审核 1：公共 crate 第一版

- 审核范围：`vulcan-session-core` 新 crate、workspace 注册、公共协议层测试。
- 发现问题：
  1. 初始版本缺少模块文档，触发 `missing_docs` warning。
  2. 初始 executor 只有终态 `execute`，缺少计划要求的 `SessionExecutionStream`，后续会导致流式事件接口再次分叉。
- 修复结果：
  1. 已补全 `lib.rs` 模块双语文档。
  2. 已新增 `SessionExecutionStream`，并在 `SessionNodeExecutor` 中加入 `stream` 方法。
- 验证：`cargo test -p vulcan-session-core` 通过，无诊断。

### 审核 2：主代理 crate 第一版

- 审核范围：`vulcan-main-session` 新 crate、workspace 注册、主会话策略和 task entry 边界。
- 发现问题：
  1. 初始 `MainSessionRuntimePolicy` 缺少序列化派生，后续进入快照或日志时会不一致。
- 修复结果：
  1. 已补 `Serialize` / `Deserialize` 派生。
- 验证：`cargo test -p vulcan-main-session` 通过，无诊断。

### 审核 3：子代理 crate 第一版

- 审核范围：`vulcan-task-session` 新 crate、`vulcan-session-core` 压缩 trait 补充、配置解析和隔离策略。
- 发现问题：
  1. 初始压缩策略覆盖沿用 bool 快照，无法表达“显式关闭继承来的自动压缩”。
  2. 初始 `TaskSessionMetadata::new` 固定 `depth = 1`，嵌套子代理如果绕过 scheduler 会写入错误深度。
- 修复结果：
  1. 已在公共层新增 `ContextCompactionPolicyRequest`，请求层使用 `Option<bool>` 表达自动压缩开关覆盖，最终 effective snapshot 仍保持 bool。
  2. 已将 `TaskSessionMetadata::new` 改为显式传入 `depth`，scheduler 不再事后修补字段。
- 验证：
  1. `cargo test -p vulcan-session-core -p vulcan-task-session -p vulcan-main-session` 通过，无诊断。
  2. `vulcan-task-session` 中数据库、BM25、FTS、workspace history、search projection 文本审计仅命中测试断言。

### 审核 4：TaskSession 文件态 store

- 审核范围：`FileTaskSessionStore` 文件布局、JSON/JSONL 读写、父级 task 索引、活动摘要查询。
- 发现问题：
  1. 初始测试误以为创建 TaskSession 后父级 `task.jsonl` 为空，实际应写入 `task_created` 索引事件。
  2. 初始 compaction checkpoint 读取缺少泛型类型标注。
  3. 初始索引摘要转换方法未被使用，会产生 dead code warning。
- 修复结果：
  1. 已将测试改为验证父级任务日志包含对应 `task_created` 事件。
  2. 已为 checkpoint 向量补充 `Vec<SessionCompactionCheckpoint>` 显式类型。
  3. 已新增 `list_indexed_task_sessions` 与 `list_active_indexed_task_sessions`，让文件索引摘要能力成为后续 router 可复用的正式接口。
- 验证：
  1. `cargo test -p vulcan-task-session` 通过，无诊断。
  2. `cargo test -p vulcan-session-core -p vulcan-task-session -p vulcan-main-session` 通过，无诊断。

### 审核 5：ExecutionPreparation scope/policy 接入

- 审核范围：`ExecutionPreparationRequest`、`prepare_execution`、普通会话构造、上下文压缩构造、GUI 构造点。
- 发现问题：
  1. 初始接入时直接把裸 `Uuid` 填入 `ExecutionScope`，与公共协议层 `SessionNodeId` 新类型不匹配。
  2. `cargo test -p vulcan-agent-runtime` 整包存在多个既有运行期失败，容易误判为本次改动引起。
- 修复结果：
  1. 已将所有 scope 构造点统一使用 `SessionNodeId::from(...)`。
  2. 已改用受影响模块定向测试，并补充 `cargo check -p vulcan-code-gui` 验证 GUI 构造点。
- 验证：
  1. `cargo test -p vulcan-agent-runtime task_runtime::tests::` 通过，只有既有 VMM warning。
  2. `cargo test -p vulcan-agent-runtime runtime_stream_launch_payload::tests::` 通过，只有既有 VMM warning。
  3. `cargo check -p vulcan-code-gui` 通过，无诊断。

### 审核 6：Task invocation 配置接入

- 审核范围：task 工具 schema、`TaskInvocationConfig` 保存/恢复、子代理系统提示词生成路由。
- 发现问题：
  1. 初始实现把独立系统提示词传给 `SubagentClone`，但 composer 的 clone 策略按设计不会渲染 functional prompt。
  2. 若恢复已有 task 时直接使用新 invocation config，未传字段可能覆盖掉旧记录中的 profile 或系统提示词配置。
- 修复结果：
  1. 已改为按是否存在 functional prompt 路由：默认 `SubagentClone`，独立提示词场景 `SubagentFunctional`。
  2. 已新增恢复合并逻辑，仅让显式传入字段覆盖旧 task record 配置。
- 验证：
  1. `cargo test -p vulcan-agent-runtime task_runtime::tests::` 通过，只有既有 VMM warning。

### 审核 7：TaskSession 文件态运行接入

- 审核范围：workspace-store 路径导出、ExecutionPreparation 文件域字段、GUI/Tauri 注入点、TaskRuntime 文件态写入。
- 发现问题：
  1. `TaskRuntime` 没有 project_id/session event-store 路径，不能在 runtime 内猜测文件位置。
  2. 嵌套子代理新建记录时原 `origin_session_id` 会被写成父 task id，不能作为文件态根主会话 ID。
  3. 文件 IO 如果持有 task runtime registry 锁，后续容易产生卡顿或锁竞争。
- 修复结果：
  1. 已由 workspace-store 暴露主会话 event-store 目录，GUI/Tauri 在真实执行前注入到 request。
  2. 已新增 `root_session_id` 并让嵌套 task 继承父记录 root，`current_session_id` 保持直接调用者语义。
  3. 已将内存 registry 更新与文件写入分离，先释放 runtime 锁，再落盘。
  4. 已补 `store_task_outcome_persists_file_backed_task_session_artifacts` 测试，验证 metadata/context/events/parent task log 均落盘。
- 验证：
  1. `cargo test -p vulcan-agent-runtime task_runtime::tests::` 通过，只有既有 VMM warning。
  2. `cargo check -p vulcan-code-gui` 通过，无诊断。
  3. `cargo check -p vulcan-code-tauri` 通过，无诊断。

### 审核 8：TaskSession 文件态复看与导航路由接入

- 审核范围：`vulcan-workbench-model` 共享转换函数、iced `subagent_runtime.rs`、WebUI/Tauri `service_bridge.rs`。
- 发现问题：
  1. 初始 GUI 导入只改了 workspace crate re-export，漏掉 `app_shell/model.rs` 的二次 re-export。
  2. WebUI 原 active surface 逻辑只在子代理仍 running 时返回 task id，导致已完成子代理即使可复看也会退回主会话。
  3. 旧 payload fallback 如果直接追加，可能覆盖文件态同 task id 的规范状态。
  4. 初始 iced/Tauri 各自保留了本地读取 `FileTaskSessionStore` 的 helper，虽然很薄，但仍不符合“抽取、复用、路由、不克隆”原则。
- 修复结果：
  1. 已补 `app_shell/model.rs` 的 `subagent_task_panel_states_from_task_session_store` 导出。
  2. 已新增 WebUI 可复看详情判定，已打开且有 transcript 的完成态子代理继续保持 active task id。
  3. 已在 iced/WebUI 缓存重建中先记录文件态 task id，旧 payload fallback 只补缺失项。
  4. 已把 store 到 panel state 的读取逻辑上移到 `vulcan-workbench-model`，iced/Tauri 只构造 store 并调用共享函数。
- 验证：
  1. `cargo test -p vulcan-workbench-model` 通过，无诊断。
  2. `cargo test -p vulcan-task-session` 通过，无诊断。
  3. `cargo check -p vulcan-code-gui` 通过，无诊断。
  4. `cargo check -p vulcan-code-tauri` 通过，无诊断。

### 审核 9：子代理运行中队列与 WebUI 复看入口

- 审核范围：共享 pending prompt router、iced dequeue、WebUI/Tauri 下一条队列选择、WebUI task 工具点击路由。
- 发现问题：
  1. 原共享 block gate 已允许 `DelegateCompletion` 在兄弟子代理运行中继续分发，但队列选择仍可能先看到排在前面的 `UserFollowup`，导致 delegate completion 被用户队列挡住。
  2. iced 的 `dequeue_session_pending_prompt` 只按 Anthropic 规则取队列，和 `maybe_start_next_pending_prompt` 的共享窥视逻辑存在潜在不一致。
  3. WebUI/Tauri 在开始任意下一条队列项时都会清理子代理导航轮次，delegate completion 回流也会误清。
  4. WebUI task 工具卡片仍默认打开工具详情弹窗，未使用已有后端子代理路由。
- 修复结果：
  1. 已新增共享 `pending_prompt_dispatch_prefers_delegate_completion`，把 Anthropic delegate 优先和运行中子代理 delegate 优先收敛到同一入口。
  2. iced/WebUI 均改用该共享入口决定队列 peek/take 优先级。
  3. 已新增 `starts_user_parent_turn`，WebUI/Tauri 只在用户来源队列项开始时清空子代理轮次。
  4. WebUI task 工具卡片现在从 metadata 提取 `task_session_id`，并通过现有 `service_open_subagent_panel` 打开子代理；旧 payload 缺失该字段时仍展示工具详情。
- 验证：
  1. `cargo test -p vulcan-workbench-model running_subagents_force_delegate_dispatch_priority` 通过。
  2. `cargo test -p vulcan-workbench-model pending_prompt` 通过。
  3. `npm run lint` 通过。
  4. `cargo check -p vulcan-code-gui` 通过。
  5. `cargo check -p vulcan-code-tauri` 通过。

### 审核 10：父 task payload transcript 泄漏清理

- 审核范围：`TaskRuntime` 终态 host payload、WebUI/iced stopped metadata、旧 payload fallback。
- 发现问题：
  1. `build_task_invocation_result` 会在非 running outcome 中将完整 `conversation_state` 写入父 task host payload。
  2. 后台子代理完成事件在文件态 TaskSession 写入之前发布，导致父 payload 一旦去掉转录，前端可能在短窗口内无法复看。
  3. `build_stopped_task_session_metadata` 会从缓存 state 中把 `conversation_state` 再写回父 task metadata。
- 修复结果：
  1. 已从 `build_task_host_payload` 中删除 `conversation_state` 字段。
  2. 已将后台终态处理顺序改为先写 TaskSession 文件态，再发布 `DelegateFinished`。
  3. 已从 stopped metadata 新写入路径删除 `conversation_state`。
  4. 保留 `parse_subagent_task_panel_payload` 对旧 `conversation_state` 的只读解析，符合“不迁移旧 payload，只降级显示”的决策。
- 验证：
  1. `cargo test -p vulcan-agent-runtime task_invocation_result_omits_embedded_conversation_state` 通过。
  2. `cargo test -p vulcan-workbench-model stopped_task_session_metadata_omits_embedded_conversation_state` 通过。
  3. `cargo test -p vulcan-agent-runtime task_runtime::tests::` 通过，只有既有 VMM warning。
  4. `cargo test -p vulcan-workbench-model` 通过。
  5. `cargo check -p vulcan-code-tauri` 通过。
  6. `cargo check -p vulcan-code-gui` 通过。

### 节点 11：嵌套 TaskSession 父级路由与停止范围

- 目标：让 iced 与 WebUI 在打开历史子代理、子代理内再打开子代理、停止整棵子代理树时都走同一套文件态 TaskSession 路由。
- 核心调整：
  1. `vulcan-workbench-model` 新增共享父级解析：同一个 `subagent_task_panel_states_from_task_session_store` 现在能按根主会话或直接 TaskSession 父级列出直接子代理。
  2. iced 打开工具卡片时使用当前 transcript surface 作为父级，不再限制只有根会话 task 卡片才能进入子代理面板。
  3. WebUI 将当前 active surface id 传给 task 卡片打开逻辑，Tauri 用选中根会话定位文件态 store，再按直接父级读取子代理。
  4. iced 与 WebUI 的 stop-all/busy 判断改为遍历父级树，停止时仍调用共享 `TaskRuntime::stop_task`，并按直接父级更新对应 task 卡片。
- 验证：
  1. `cargo test -p vulcan-workbench-model`
  2. `cargo test -p vulcan-workbench-model loads_nested_panel_states_from_task_session_store`
  3. `npm run lint`（`apps/vulcan-code-web`）
  4. `cargo check -p vulcan-code-gui`
  5. `cargo check -p vulcan-code-tauri`

### 审核 11：嵌套路由与执行链条一致性

- 审核范围：共享 TaskSession store-to-panel 路由、iced 工具卡片打开链条、WebUI/Tauri active surface 与 stop-all 链条。
- 发现问题：
  1. 初始共享 `subagent_task_panel_states_from_task_session_store` 只构造 `ParentSessionRef::MainSession`，文件态 store 虽支持 `ParentSessionRef::TaskSession`，但上层完全没用到。
  2. iced 的工具卡片打开逻辑额外限制 `selected_root_session_id == session_id`，导致子代理 transcript 里的 task 卡片会退回普通工具弹层。
  3. WebUI task 卡片父 id 使用选中根会话 id，导致子代理 surface 内的 task 卡片无法按直接父级打开。
  4. stop-all 与 busy 判断只看直接父级桶，嵌套运行中的子代理可能不影响停止按钮和停止范围。
- 修复结果：
  1. 已在共享模型层解析根父级与 TaskSession 父级，iced/WebUI 只调用共享函数。
  2. 已删除 iced 侧不再使用的根会话专用打开 helper，避免额外逻辑回流。
  3. 已让 WebUI/Tauri active surface 消息、导航、打开命令从后端统一路由。
  4. 已把 iced/WebUI 的子代理 busy/stop-all 扩展为父树遍历，仍复用共享 task runtime 停止能力。
- 验证：同节点 11 验证项，全部通过。

### 节点 12：iced 执行链运行中子代理树刷新对齐

- 目标：消除 iced 与 WebUI 在运行中子代理树判断上的刷新时机差异，避免重启、历史打开或嵌套 TaskSession 文件态存在时 iced 只看旧缓存。
- 核心调整：
  1. `subagent_runtime.rs` 新增 `refresh_subagent_task_tree_cache_for_session`，按父级树逐层调用已有共享文件态 TaskSession store-to-panel 路由。
  2. 新增 `running_subagent_task_states_for_tree_after_refresh`，发送、停止等执行链条先刷新文件态树，再用原有 normalized runtime 判定真实运行状态。
  3. `pending_prompt_queue.rs` 新增刷新后 dispatch/blocking helper，`dequeue_session_pending_prompt`、`dispatch_pending_prompt_entry_now`、`maybe_start_next_pending_prompt`、`stop_all_session_work`、`submit_selected_session_prompt` 均改用刷新后判断。
  4. `last_user_message_edit.rs` 的最后用户消息回滚 gate 改为可变校验，并在校验运行中子代理前刷新文件态树。
- 验证：
  1. `cargo check -p vulcan-code-gui`
  2. `cargo test -p vulcan-code-gui session_has_blocking_system_prompt_work_includes_active_compaction`
  3. `cargo test -p vulcan-code-gui begin_session_run_shows_user_prompt_before_precheck_finishes`
  4. `git diff --check`

### 审核 12：iced/WebUI 运行中子代理树刷新一致性

- 审核范围：iced `subagent_runtime.rs`、`pending_prompt_queue.rs`、最后用户消息编辑 gate，与 WebUI/Tauri 的 `running_webui_subagent_task_states_for_tree` 对照。
- 发现问题：
  1. WebUI/Tauri 的运行中树遍历会在每个父节点调用共享文件态缓存重建；iced 原先的树遍历是只读缓存，文件态嵌套 TaskSession 可能漏判。
  2. iced 的发送、插入、自动下一条、停止和历史回滚 gate 原先都可能沿用旧缓存判断运行中子代理。
- 修复结果：
  1. iced 新增刷新后运行中树入口，行为对齐 WebUI/Tauri 的父树遍历语义。
  2. 发送/停止/队列分发/回滚 gate 均切到刷新后入口。
  3. 只读显示路径保持只读缓存，避免 view 层产生隐藏副作用。
- 验证：节点 12 验证项全部通过。

### 节点 13：子代理 profile、工具与调用覆盖入口

- 状态：部分完成，继续推进。
- 已完成内容：
  1. `TaskToolInvocationArgs` 增加 `profile` 短别名，并将 task 工具 schema 必填项收敛为仅 `prompt`，避免命名 profile 调用被 `subagent` 必填项阻断。
  2. `TaskToolInvocationArgs` 支持 provider/model/reasoning/execution_mode、tools、ai_capabilities、skills/prompt_skills、LuaSkills、MCP、context_compaction、extra 等覆盖入口。
  3. `TaskInvocationConfig` 合并逻辑保留旧 TaskSession 记录中的显式配置，续跑时只覆盖本轮明确传入字段。
  4. `RuntimeFoundation` 将已解析的 runtime agent 定义转换为 `SubagentProfile` 快照，并通过 `TaskRuntime::configure_subagent_profiles` 同步进入 TaskSession resolver。
  5. profile 转换复用已有 `resolve_agent_native_tools`、`resolve_agent_mcp_tool_allowlist` 与 native catalog，不让 `TaskRuntime` 自行读取配置文件。
  6. `TaskRuntime` 执行前使用共享 `TaskSessionConfigResolver` 解析 profile、父配置与 invocation 覆盖。
  7. 子代理实际执行使用解析后的 system prompt、同 Provider 下的 model 覆盖、temperature/max_turns、工具 allow/deny 过滤。
  8. 文件态 `task-session.json` 写入 `requested_config` 与 `effective_config`，优先使用真实执行 outcome 的 `resolved_config`。
- 已补测试：
  1. `runtime_agent_definition_becomes_subagent_profile_snapshot`
  2. `build_task_runtime_subagent_profiles_uses_native_catalog_descriptors`
  3. `task_invocation_config_parses_profile_tool_skill_and_compaction_overrides`
  4. `task_tool_schema_allows_profile_without_subagent_label`
  5. `filter_task_tool_definitions_applies_allowed_and_disabled_selectors`
  6. `task_runtime_resolves_registered_subagent_profile`
- 当前自审结论：
  1. 工具集合覆盖已真实进入子代理 provider-visible tool definitions 和 `VulcanManagedToolRuntimeBridge`。
  2. profile 与 invocation 的 prompt skill、LuaSkills、MCP 覆盖目前已进入 `effective_config` 与文件态审计快照，但还没有重新走 `prepare_execution` 的 prompt skill / LuaSkills / MCP 组装链路。
  3. 不应通过手工拼接 prompt skill 文案补这个缺口；下一步必须把 TaskSession 执行桥改为复用 `prepare_execution` 或等价公共 executor。
  4. 跨 Provider 的子代理模型切换当前不强行生效，只允许同 Provider 下替换 model，避免用父 Provider transport 发送到不匹配的子 Provider；后续需通过 provider router 解决。
- 当前验证：
  1. `cargo test -p vulcan-agent-runtime task_runtime::tests::`：通过，仅有既有 VMM dead code warning。
  2. `cargo test -p vulcan-agent-runtime runtime_agent_definition_becomes_subagent_profile_snapshot`：通过，仅有既有 VMM dead code warning。
  3. `cargo check -p vulcan-agent-runtime`：通过，无诊断。
  4. `cargo check -p vulcan-code-gui`：通过，无诊断。
  5. `cargo check -p vulcan-code-tauri`：通过，无诊断。

### 下一节点：TaskSession 执行桥复用 prepare_execution

- 目标：让子代理执行不再只基于父会话已准备好的 `CoreControllerConfig` 与工具集合局部派生，而是通过共享执行适配器重新组装 TaskSession 自己的 prompt skills、LuaSkills、MCP、AI capability、tool manager、context/current 与压缩策略。
- 原则：
  1. 不在 `TaskRuntime` 内手写 prompt skill/LuaSkills/MCP 组装逻辑。
  2. 不让 iced/WebUI 参与任何执行链判断。
  3. 子代理仍使用 `ExecutionRuntimePolicy::task_session()`，保持 hook/VMM/记忆/search/database projection 关闭。
  4. 子代理文件态 store 继续作为唯一持久化来源。

### 节点 14：TaskSession 执行桥复用 prepare_execution

- 状态：核心桥接完成，直接 JSON LuaSkills/MCP 覆盖仍作为后续缺口保留。
- 核心调整：
  1. `ExecutionPreparationRequest` 增加请求级 `execution_agent_id`，`prepare_execution` 优先按该 agent id 解析 runtime agent，未知 id 直接失败，不再静默回退 active agent。
  2. `RuntimeFoundation` 暴露按 id 读取 agent definition 的共享入口，bootstrap resources 也按 `preparation.agent_id` 对齐 active agent 资源。
  3. 主会话 runtime launch 在 provider loop 生命周期内注册 `TaskExecutionRuntimeContext`，TaskRuntime 可从父 session id 读取已准备好的 config、foundation、LuaSkills runtime 与 request 模板。
  4. 子代理执行从 `TaskSessionResolvedConfig` 构造 TaskSession scope/policy/request，然后复用 `prepare_execution -> bootstrap_runtime_stream -> build_runtime_stream_launch_payload -> drive_runtime_stream_provider_loop`。
  5. 子代理 prompt skill 覆盖通过 `session_prompt_skill_overrides` 进入共享 prepare 链；profile 形式的 native/LuaSkills/MCP/tool 通过 `execution_agent_id` 进入 runtime foundation 解析链。
  6. 工具 allow/deny 在 child launch 上过滤 `resolved_tool_definitions` 并同步裁剪 `preparation.tool_descriptors`，真实 Provider 请求不暴露未授权工具。
  7. 共享执行分支会从终态 report 还原 `final_assistant_message_id`，避免父 task payload 丢失最终助手消息引用。
  8. 已删除 `TaskRuntime` 内旧的私有 `VulcanCoreController` fallback；缺少共享执行上下文时 task 后端明确失败，防止偷偷回退到旧语义。
  9. `PreparedHostToolRuntimeContext` 删除不再读取的 `tools_manager/resolved_tool_definitions` 字段，只保留实际使用的 runtime bridge、task runtime 与 provider tool definitions。
- 当前自审结论：
  1. 正常主会话与嵌套子代理路径现在都必须通过共享 runtime stream 桥执行；`TaskRuntime` 不再自行创建 controller/provider/tool executor。
  2. TaskSession 仍使用 `ExecutionRuntimePolicy::task_session()`，hook/VMM/记忆/search/database projection 默认关闭。
  3. profile 配置可以通过 runtime agent id 复用已有 native/LuaSkills/MCP/工具组装逻辑。
  4. invocation 直接传入的 prompt skill 覆盖已生效；直接 JSON 形式的 `luaskills` / `mcp` 覆盖目前仍只进入配置快照，尚未转成临时 runtime agent/foundation 路由，后续不能通过手写组装补，要继续走抽取/路由。
- 已补测试：
  1. `prepare_execution_uses_request_execution_agent_override`
  2. `prepare_execution_rejects_missing_request_execution_agent`
  3. `task_execution_outcome_from_report_preserves_final_assistant_message_id`
  4. `build_task_execution_handler_rejects_missing_shared_context`
- 当前验证：
  1. `cargo test -p vulcan-agent-runtime task_runtime::tests:: -- --nocapture`：15 个测试通过，仅剩既有 VMM dead code warning。
  2. `cargo test -p vulcan-agent-runtime prepare_execution_ -- --nocapture`：2 个测试通过。
  3. `cargo check -p vulcan-agent-runtime`：通过，无新增 warning。
  4. `cargo check -p vulcan-code-gui`：通过。
  5. `cargo check -p vulcan-code-tauri`：通过。

### 节点 15：TaskSession 工具曝光覆盖进入共享准备链路

- 状态：已完成第一版。
- 已将 TaskSession `tools`、`luaskills`、`mcp`、`ai_capabilities` 覆盖接入共享 `prepare_execution` 与 bootstrap 过滤链。
- 已新增 `vulcan-session-core::selector` 作为统一 selector 规范化与匹配入口。
- 已删除 TaskRuntime 内启动后补丁式工具过滤，避免第二套工具曝光语义。
- 验证：`cargo check -p vulcan-agent-runtime`、`cargo test -p vulcan-agent-runtime execution_tool_exposure_ -- --nocapture`、`cargo test -p vulcan-agent-runtime task_runtime::tests:: -- --nocapture`、`cargo test -p vulcan-agent-runtime prepare_execution_ -- --nocapture`、`cargo check -p vulcan-code-gui`、`cargo check -p vulcan-code-tauri`、`cargo test -p vulcan-agent-runtime mcp::runtime::tests:: -- --nocapture`、`cargo test -p vulcan-session-core selector -- --nocapture`、`git diff --check` 均通过；只有既有 VMM dead code warning 与 Windows 换行提示。

### 节点 16：TaskSession 文件态恢复与压缩存储契约

- 状态：已完成第一段。
- 已实现 `TaskSessionStore::find_task_session_by_parent_and_public_id`，按直接父级从文件态恢复公开 task id。
- 已实现 `FileTaskSessionStore` 压缩结果保存接口，压缩后 context/checkpoint 只写 TaskSession 文件域。
- `TaskRuntime` 可从 `context/current.json` 恢复子代理 provider/model/system prompt/conversation_state 并继续共享执行桥。
- 验证：`cargo test -p vulcan-task-session file_store_round_trips_task_session_artifacts -- --nocapture`、`cargo test -p vulcan-agent-runtime prepare_task_execution_request_restores_file_backed_context_for_resume -- --nocapture`、`cargo test -p vulcan-task-session -- --nocapture`、`cargo test -p vulcan-agent-runtime task_runtime::tests:: -- --nocapture`、`cargo fmt`、`cargo check -p vulcan-agent-runtime`、`cargo check -p vulcan-code-gui`、`cargo check -p vulcan-code-tauri` 均通过；只有既有 VMM warning。

### 节点 17：TaskSession 自动上下文压缩接入

- 状态：已完成第一版。
- 已将 GUI 提示词式压缩执行器下沉到 `vulcan-agent-runtime::agent_runtime::prompt_compaction`，GUI 只保留薄导出。
- 已新增 foundation-based prompt compaction 入口，供 TaskSession 复用共享 `RuntimeFoundation`、LuaSkills runtime 与 runtime stream 链路。
- 已在 `vulcan-task-session` 中实现共享压缩结果应用逻辑，写回 TaskSession 自己的 `context/current.json` 与 checkpoint。
- 压缩锚点使用合成 summary message id，后续规划能继续找到上下文锚点。
- 子代理完成后按共享 preparation 的 context budget 判定是否后台压缩，压缩不阻塞父会话反馈。
- 压缩继续使用 `ExecutionRuntimePolicy::context_compaction()`，不触发 hook、VMM、记忆、搜索或数据库投影。
- 已修正压缩规划策略读取 TaskSession resolved config 的 `auto_compaction_enabled`，并补单元测试。
- 验证：`cargo fmt`、`cargo test -p vulcan-task-session -- --nocapture`、`cargo test -p vulcan-agent-runtime task_runtime::tests:: -- --nocapture`、`cargo check -p vulcan-agent-runtime`、`cargo check -p vulcan-code-gui`、`cargo check -p vulcan-code-tauri`、`git diff --check` 均通过；仅有既有 VMM warning 与 Windows 换行提示。

### 节点 18：TaskSession 运行前前置压缩保护

- 状态：已完成第一版。
- 子代理 shared runtime 后端会先写入文件态 Running 状态和当前 context，确保前置压缩有稳定 TaskSession 文件域。
- `invoke_task` 对 Running outcome 不再重复持久化，避免父级 task 日志出现重复 running 事件。
- 子代理首次 prepare/bootstrap 后，如共享 context budget 判定需要压缩，会在 Provider 正式执行前先执行一次文件态 TaskSession 压缩。
- 前置压缩复用共享 `stream_prompt_compaction_foundation_job`，完成后把 compacted conversation state 回填到本次 invocation snapshot，再重新走共享 prepare/bootstrap。
- 后置压缩和前置压缩共用同一个 `run_task_session_context_compaction`，只依赖 `TaskSessionResolvedConfig`，不再依赖完成态 outcome。
- `store_task_outcome` 保存 context 时保留已有 `compaction_anchor` 和 `context_budget_fingerprint`，防止压缩锚点被终态覆盖。
- 停止分支构造 stopped outcome 时优先读取 TaskSession 文件态 current context，文件态不可用时才回退 invocation snapshot。
- 验证：`cargo fmt`、`cargo test -p vulcan-agent-runtime store_task_outcome_persists_file_backed_task_session_artifacts -- --nocapture`、`cargo test -p vulcan-agent-runtime current_task_snapshot_from_store_reads_file_backed_context -- --nocapture`、`cargo test -p vulcan-agent-runtime task_runtime::tests:: -- --nocapture`、`cargo check -p vulcan-agent-runtime`、`cargo check -p vulcan-code-gui`、`cargo check -p vulcan-code-tauri`、`cargo test -p vulcan-task-session -- --nocapture` 均通过；仅有既有 VMM warning。

### 节点 19：旧 payload 降级语义复核

- 状态：已完成第一版。
- 发现并修复 `vulcan-workbench-model::parse_subagent_task_panel_payload` 仍从旧父 task payload 读取 `conversation_state` 的残留路径。
- 修正后旧 payload 只保留 task id、状态、描述、子代理标签与结果摘要；完整子代理转录只能从文件态 TaskSession `context/current.json` 恢复。
- 已同步更正计划文档中节点 10 的旧记录，避免后续上下文压缩后继续按“旧 conversation_state 可 fallback 读取”的错误语义执行。
- 自审结论：
  1. 修复位于共享模型层，iced 与 WebUI 共用，不新增界面私有逻辑。
  2. 父 task host payload 仍不写 `conversation_state`；TaskRuntime 写完整上下文的位置仍是 TaskSession 文件态 context。
  3. 旧 payload 结果摘要仍可走工具详情弹层，不影响历史 task 返回查看。
- 验证：`cargo fmt`、`cargo test -p vulcan-workbench-model legacy_task_payload_ignores_embedded_conversation_state -- --nocapture`、`cargo test -p vulcan-workbench-model -- --nocapture`、`cargo check -p vulcan-code-gui`、`cargo check -p vulcan-code-tauri` 均通过；`git diff --check` 仅有 Windows 换行提示。

### 节点 20：运行中子代理树遍历抽取

- 状态：已完成第一版。
- 新增 `vulcan-workbench-model::collect_running_subagent_task_states_for_tree`，统一父子树递归遍历与运行态筛选。
- iced GUI 的 `running_subagent_task_states_for_tree` 与 WebUI/Tauri 的 `running_webui_subagent_task_states_for_tree` 均改为调用共享函数。
- GUI/Tauri 只保留各自状态读取适配：GUI 读取本地缓存并调用 live 归一化，Tauri 刷新文件态缓存并按 detached listener / TaskRuntime 判断 live。
- 自审结论：
  1. 队列阻塞、停止全部任务、子代理运行态判断现在共享同一套树遍历形状。
  2. 没有新增 UI 私有业务逻辑，反而删除了 GUI/Tauri 两份手写循环。
  3. 后续若需要调整“嵌套子代理是否阻塞父级”，只需改共享模型层。
- 验证：`cargo fmt`、`cargo test -p vulcan-workbench-model collects_running_subagent_task_states_for_tree -- --nocapture`、`cargo test -p vulcan-workbench-model -- --nocapture`、`cargo check -p vulcan-code-gui`、`cargo check -p vulcan-code-tauri` 均通过；`git diff --check` 仅有 Windows 换行提示。

### 节点 21：子代理复看 gate 与旧 task 卡片 fallback

- 状态：已完成第一版。
- 新增 `vulcan-workbench-model::subagent_task_panel_state_has_recoverable_conversation`，统一判断文件态 TaskSession 面板是否包含可复看 conversation。
- iced GUI 的 `subagent_task_panel_has_recoverable_detail` 与 WebUI/Tauri 的 `webui_subagent_panel_has_recoverable_detail` 均改为复用共享 helper。
- WebUI `service_open_subagent_panel` 对齐 iced：运行中子代理可打开，终态子代理必须有可复看详情，否则返回不可恢复错误。
- Web 前端 `onOpenSubagentPanel` 返回 `boolean`；task 工具卡片打开子代理失败时，回退打开普通工具详情，用于展示旧 payload 已持久化的 task 返回记录。
- 自审结论：
  1. 旧父 payload 不会恢复完整子代理面板；完整转录只来自文件态 TaskSession 或实时内存投影。
  2. WebUI/iced 的可复看判断已经统一到共享模型层。
  3. Web 前端 fallback 仅是显示层处理，不新增子代理业务逻辑。
- 验证：`cargo fmt`、`cargo test -p vulcan-workbench-model panel_state_recoverable_conversation_accepts_messages -- --nocapture`、`cargo test -p vulcan-workbench-model -- --nocapture`、`cargo check -p vulcan-code-gui`、`cargo check -p vulcan-code-tauri`、`npm run lint`（`apps/vulcan-code-web`）均通过；`git diff --check` 仅有 Windows 换行提示。

## 当前下一步

1. 继续检查 TaskSession router/视图复看与旧 payload 降级是否完全走共享 store/router。
2. 继续做端到端验证：子代理复看、嵌套子代理、停止整棵树、队列回流、前置/后置压缩。
3. 最终完成后再追加计划文件的「执行变更总结」并迁移到 `docs/completed/`；当前大计划尚未整体闭环，不能提前迁移。

### 节点 22：子代理完成回流共享队列容量修正

- 已审核并修复 WebUI delegate completion 入队满时静默丢弃的问题。
- 核心规则改在 `vulcan-workbench-model` 共享队列计划：
  - 用户可见队列仍保持 10 条限制。
  - 普通用户跟进满队列时仍会拒绝。
  - delegate completion 属于系统回流，可使用额外内部预留容量，避免父会话无法吸收子代理结果。
- WebUI/Tauri 在预留容量也耗尽时写入服务日志，不再静默吞掉。
- 自审结论：
  1. 队列容量语义位于共享模型层，iced/WebUI 共用。
  2. WebUI 只保留兜底诊断，没有新增私有调度逻辑。
  3. 不删除用户排队项，不改变可见队列上限。
- 验证：
  1. `cargo fmt` 通过。
  2. `cargo test -p vulcan-workbench-model internal_queue_enqueue_allows_delegate_reserve_when_users_fill_queue -- --nocapture` 通过。
  3. `cargo test -p vulcan-workbench-model -- --nocapture` 通过。
  4. `cargo check -p vulcan-code-gui` 通过。
  5. `cargo check -p vulcan-code-tauri` 通过。
  6. `git diff --check` 仅有 Windows 换行提示。

### 节点 23：父级 task 卡片 metadata 反挂语义抽取

- 已审核并修复 WebUI 没有在 delegate started/finished 时把 task-session metadata 反挂回父级 task 工具卡片的问题。
- 已将工具卡片 metadata 浅合并规则下沉到 `vulcan-workbench-model::merge_tool_display_metadata`：
  - iced GUI 的同名函数改为薄 wrapper。
  - WebUI/Tauri 反挂父 task 卡片时复用同一合并规则。
  - 保持旧 iced execution rank 语义，避免抽取时改变排序。
- WebUI/Tauri 在 ingest delegate runtime event 时会更新父级 runtime record 与当前投影消息。
- 自审结论：
  1. task-session metadata 合并规则已进入共享模型层。
  2. WebUI 只保留工作区快照写回适配，不新增业务判断。
  3. 不迁移旧 payload，不伪造旧 TaskSession 目录。
- 验证：
  1. `cargo fmt` 通过。
  2. `cargo test -p vulcan-workbench-model merge_tool_display_metadata_preserves_advanced_task_session_payload -- --nocapture` 通过。
  3. `cargo test -p vulcan-workbench-model -- --nocapture` 通过。
  4. `cargo check -p vulcan-code-gui` 通过。
  5. `cargo check -p vulcan-code-tauri` 通过。
  6. `git diff --check` 仅有 Windows 换行提示。

### 节点 24：单个子代理停止语义树级对齐

- 状态：已完成第一版。
- 已审核并修复 WebUI `service_stop_subagent_task` 仍只停止单个 `task_session_id` 的差异。
- 共享运行时 `TaskRuntime::stop_task_tree` 作为唯一停止子树语义来源：
  - 收集目标 task 与所有后代 task。
  - 只中止仍有 active abort handle 的任务。
  - 返回实际被停止的 task id 集合，避免界面误标历史节点。
- iced `StopSelectedSubagentTask` 与 WebUI/Tauri `service_stop_subagent_task` 均使用 `stop_task_tree`。
- WebUI/Tauri 只负责把实际停止的 task id 回写到对应直接父级 task 卡片、子代理缓存与 detached listener。
- 已保留 stale listener 清理：当 runtime 已无活动句柄但 WebUI listener 未清时，会清理根 task listener，避免停止按钮继续误判运行中。
- 已追加审计并修正会话级停止入口：iced `mark_running_subagent_tasks_stopped` 与 WebUI `service_stop_conversation_run` 也改为使用 `stop_task_tree`，当前仓库已无 `.stop_task(` 调用残留。
- 自审结论：
  1. 停止整棵 TaskSession 子树的业务逻辑位于共享 runtime，不在 WebUI 额外复刻。
  2. iced/WebUI 都只做状态投影与持久化适配。
  3. 单测覆盖父 task、子 task、孙 task 与旁支 task，确保不误伤旁支。
  4. 会话级停止仍保留“界面认为运行中的都标 stopped”的用户语义，同时底层中止动作改为共享树级停止。
- 验证：
  1. `cargo fmt` 通过。
  2. `cargo test -p vulcan-agent-runtime stop_task_tree_aborts_active_descendants_without_siblings -- --nocapture` 通过，仅有既有 VMM dead code warning。
  3. `cargo test -p vulcan-agent-runtime task_runtime::tests:: -- --nocapture` 通过，仅有既有 VMM dead code warning。
  4. `cargo check -p vulcan-code-gui` 通过。
  5. `cargo check -p vulcan-code-tauri` 通过。
  6. `cargo test -p vulcan-workbench-model -- --nocapture` 通过。
  7. `git diff --check` 仅有 Windows 换行提示。
  8. 补充审计后再次执行 `rg "\\.stop_task\\("`，确认无残留调用；再次执行 `cargo fmt`、聚焦 runtime 测试、GUI/Tauri check 与 `git diff --check` 均通过。

### 节点 25：子代理 active run 生命周期修正

- 状态：已完成第一版。
- 深度审核发现 `run_task_with_shared_runtime_context` 中的两个 guard 生命周期错误：
  - `AbortChildRunOnDrop` 在启动函数返回时立即 drop，会提前 abort child run。
  - `ActiveTaskRunGuard` 在启动函数返回时立即 drop，会提前移除 `active_task_runs`。
- 已将两个 guard 移入后台 terminal watcher task，确保它们覆盖整个子代理真实运行周期。
- 子代理完成、失败或停止后，watcher 退出时才释放 guard 并移除 active run entry。
- 自审结论：
  1. 运行态事实源重新回到共享 `TaskRuntime::active_task_runs`。
  2. iced/WebUI 停止按钮、队列阻塞、运行中导航均会通过同一个事实源判断，不再靠界面私有状态硬撑。
  3. 该修复对应“子代理还在跑但按钮变回发送、队列继续派发”的核心根因之一。
- 验证：
  1. `cargo fmt` 通过。
  2. `cargo test -p vulcan-agent-runtime task_runtime::tests:: -- --nocapture` 通过，仅有既有 VMM dead code warning。
  3. `cargo check -p vulcan-code-gui` 通过。
  4. `cargo check -p vulcan-code-tauri` 通过。
  5. `git diff --check` 仅有 Windows 换行提示。

### 节点 26：WebUI 子代理运行态事实源收敛

- 状态：已完成第一版。
- 代码审核发现 WebUI/Tauri 端仍将 `detached_subagent_task_listeners` 作为“子代理仍在运行”的判断来源之一，而 iced 只通过共享 `TaskRuntime::is_task_running` 判断真实运行态。
- 已删除 Tauri 端 listener 参与 running 判定的额外逻辑：
  - `normalize_webui_subagent_task_state` 不再接收 `has_live_listener`。
  - `webui_subagent_task_has_live_run` 只查询共享 `TaskRuntime` active abort handle。
  - 打开子代理面板、队列回流、导航快照、缓存重建、active surface 选择均改为同一运行态事实源。
- 保留 detached listener 本身作为事件路由，不再把它当作执行状态来源，避免 stale listener 造成“队列卡住、按钮状态错位、子代理列表不消失”等 WebUI 专属问题。
- 自审结论：
  1. WebUI 的运行态事实源已和 iced 对齐到共享 `TaskRuntime`。
  2. 本节点没有新增 WebUI 私有业务规则，而是删除了 Tauri 多出来的 listener 运行态判断。
  3. listener 仍用于接收 detached event，职责收敛为事件通道。
- 验证：
  1. `cargo fmt` 通过。
  2. `cargo check -p vulcan-code-tauri` 通过。
  3. `cargo check -p vulcan-code-gui` 通过。
  4. `rg` 确认 `has_live_listener`、`live_listener_task_ids` 和 listener 参与 running 判定无残留。
  5. `rg "stop_task\\(" -n crates apps` 确认只剩共享 `TaskRuntime::stop_task` 定义，无调用残留。
  6. `git diff --check` 仅有 Windows 换行提示。

### 节点 27：队列阻塞与子代理完成回流审核

- 状态：已完成审核，未发现需要修改的问题。
- 审核范围：
  - iced `maybe_start_next_pending_prompt` / `dequeue_session_pending_prompt` / `enqueue_delegate_completion_followup_if_needed`。
  - WebUI/Tauri `maybe_prepare_next_webui_pending_prompt` / `take_next_webui_pending_prompt` / `enqueue_webui_delegate_completion_followup_if_needed`。
  - 共享 `pending_prompt_dispatch_prefers_delegate_completion`、`pending_prompt_dispatch_block_reason_for_source` 与内部队列入队策略。
- 审核结论：
  1. iced 与 WebUI 均通过共享 `pending_prompt_dispatch_prefers_delegate_completion` 决定是否优先取 `DelegateCompletion`。
  2. 共享 block helper 明确允许 `DelegateCompletion` 在仍有运行中子代理时通过，同时阻塞普通用户 followup。
  3. WebUI detached parent runtime 事件在收到 `DelegateFinished` 后会写入 task metadata、入队 delegate completion，并立即调用 `maybe_prepare_next_webui_pending_prompt`。
  4. finished metadata 的 `execution_status` 为终态，因此不会因为 active guard 的极短释放窗口被重新归一化为 running。
  5. 当前语义支持“一个子代理完成就回流一个 delegate completion”，不需要等所有子代理完成。
- 验证：
  1. `cargo test -p vulcan-workbench-model running_subagents_force_delegate_dispatch_priority -- --nocapture` 通过。
  2. `cargo test -p vulcan-workbench-model pending_prompt_dispatch_allows_delegate_completion_during_running_subagents -- --nocapture` 通过。

### 节点 28：子代理复看与历史打开路由审核

- 状态：已完成审核，未发现需要修改的问题。
- 审核范围：
  - Web 前端 `ToolExecutionBlock` 点击 task 工具卡片后的 `onOpenSubagentPanel` 与 fallback 工具详情。
  - Tauri `service_open_subagent_panel`、`active_surface_session_id_from_state`、`active_surface_messages_from_workspace_and_state`、`subagent_navigation_snapshot_from_state`。
  - 共享 `parse_subagent_task_panel_payload` 与 `subagent_task_panel_state_has_recoverable_conversation`。
- 审核结论：
  1. Web 前端只有在 metadata 中存在 `task_session_id` 时才请求后端打开子代理面板，后端拒绝后会回退普通工具详情。
  2. Tauri 打开面板时会先刷新共享 TaskSession 文件态路由；终态子代理必须存在可复看详情，否则返回“不可恢复”错误。
  3. 旧 payload 即便包含 `conversation_state`，共享解析也会强制置空，不会伪造可复看的 TaskSession。
  4. 可复看详情只来自文件态 TaskSession context 或当前内存消息投影，符合“不迁移旧 payload”的已确认策略。
- 验证：
  1. `cargo test -p vulcan-workbench-model legacy_task_payload_ignores_embedded_conversation_state -- --nocapture` 通过。
  2. `cargo test -p vulcan-workbench-model panel_state_recoverable_conversation_accepts_messages -- --nocapture` 通过。

### 节点 29：TaskSession 文件态隔离边界审核

- 状态：已完成审核，未发现需要修改的问题。
- 审核范围：
  - `crates/vulcan-task-session`
  - `crates/vulcan-session-core`
  - `crates/vulcan-main-session`
  - GUI/Tauri 中 `FileTaskSessionStore` 与 `session_event_store_directory` 对接点。
- 审核结论：
  1. `vulcan-task-session` 的 `Cargo.toml` 只依赖 serde、context、context-compaction 与 session-core，没有数据库、BM25、FTS、LanceDB、workspace history search 等依赖。
  2. `vulcan-task-session` 搜索命中的 `index` 均为文件态 `task-index.json` 与任务父子索引，不是数据库/搜索索引。
  3. `database_projection_enabled` / `search_projection_enabled` 字段只存在公共 policy 与主会话策略中；TaskSession 默认策略测试断言它们为关闭。
  4. GUI/Tauri 对 TaskSession 的读取均通过 `FileTaskSessionStore` 和共享 `subagent_task_panel_states_from_task_session_store`，没有写普通会话数据库投影。
- 验证：
  1. 隔离关键词搜索确认 TaskSession 未引入数据库或搜索投影实现。
  2. `cargo test -p vulcan-task-session -- --nocapture` 通过。

### 节点 30：TaskSession 完成报告副作用隔离修复

- 状态：已完成第一版。
- 深度审核发现 detached child 的 `ExecutionStreamEvent::Finished(report)` 在 iced 与 WebUI/Tauri 中都会复用主会话完成报告处理函数：
  - iced：`apply_execution_run_report(report)`。
  - WebUI/Tauri：`apply_execution_report_to_memory(app, state, agent_runtime, report)`。
- 原实现会让 TaskSession 完成报告触发主会话完成副作用，包括 conversation hook completion、VMM profile sync、普通会话持久化、主会话压缩和下一队列分发。
- 已按主会话成员关系增加副作用 gate：
  - 只有 `session_id` 属于普通主会话列表时，才运行 completion hook、terminate hook、VMM profile sync、VMM snapshot refresh。
  - iced 端只有主会话才执行普通会话压缩、普通会话持久化和 `maybe_start_next_pending_prompt`。
  - TaskSession report 仍可更新转录、运行相位、消息投影和内存态显示，但不进入主会话副作用链。
- 自审结论：
  1. 子代理默认跳过 hook/VMM/记忆写回的语义重新成立。
  2. 没有新建子代理私有完成链路，而是在现有主会话完成入口上增加共享 gate。
  3. 子代理后续协调仍由 `DelegateFinished` 的父 runtime event 入队 `DelegateCompletion` 负责，不由 child report 直接推进普通队列。
- 验证：
  1. `cargo fmt` 通过。
  2. `cargo check -p vulcan-code-gui` 通过。
  3. `cargo check -p vulcan-code-tauri` 通过。
  4. `rg` 搜索确认完成 hook、terminate hook、压缩、普通持久化和下一队列分发均在主会话 gate 下。
  5. `git diff --check` 仅有 Windows 换行提示。
  6. `cargo test -p vulcan-agent-runtime task_runtime::tests:: -- --nocapture` 通过，仅有既有 VMM dead code warning。
  7. `cargo test -p vulcan-session-core -p vulcan-task-session -p vulcan-main-session -- --nocapture` 通过。
  8. `cargo test -p vulcan-workbench-model -- --nocapture` 通过。
  9. `pnpm run lint`（`apps/vulcan-code-web`）通过。

### 节点 31：TaskSession 中间事件副作用残留审计与修复

- 状态：已完成第一版。
- 审核发现 iced 的 `push_session_runtime_event` 会在吸收子代理事件之前无条件调用 `conversation_hook_runtime.record_tool_runtime_event`。
- 已将 iced hook 记录改为仅在 `session_id` 属于普通主会话列表时执行；TaskSession 事件仍正常进入显示投影和子代理面板缓存。
- 审核发现 iced 的 `apply_execution_conversation_checkpoint` 与 WebUI/Tauri 的 `apply_conversation_checkpoint` 会把 TaskSession checkpoint 通过普通 workspace-store runtime-state 路径持久化。
- 已将 checkpoint 持久化改为仅主会话执行；TaskSession checkpoint 仍更新内存 conversation state、消息投影和滚动显示，但不进入普通会话持久化。
- 审核发现 `build_task_runtime_preparation_request` 写入实际执行请求时重新写死 `ExecutionRuntimePolicy::task_session()`，而 metadata 使用的是 `TaskSessionConfigResolver` 的解析结果。
- 已改为执行请求直接使用 `resolved_config.runtime_policy`，让文件 metadata 与真实执行态同源，避免后续 profile 显式策略和执行态分叉。
- 自审结论：
  1. TaskSession 终态报告、中间 checkpoint、运行时工具事件都已避免进入普通主会话 hook/VMM/持久化副作用链。
  2. 本节点没有新增 WebUI 或 iced 私有业务逻辑，只按主会话成员关系隔离现有共享入口。
  3. `vulcan-task-session` 依赖仍保持文件态边界，没有数据库、BM25、FTS、workspace history search 或普通投影依赖。
  4. TaskSession 的执行请求与解析配置已保持同源，减少后续独立 profile 扩展时出现“显示配置与真实执行不一致”的风险。
- 验证：
  1. `cargo fmt` 通过。
  2. `cargo check -p vulcan-code-gui` 通过。
  3. `cargo check -p vulcan-code-tauri` 通过。
  4. `cargo test -p vulcan-agent-runtime task_runtime::tests:: -- --nocapture` 通过，仅有既有 VMM dead code warning。
  5. `cargo test -p vulcan-task-session -- --nocapture` 通过。
  6. `cargo test -p vulcan-session-core -p vulcan-task-session -p vulcan-main-session -- --nocapture` 通过。
  7. `cargo test -p vulcan-workbench-model -- --nocapture` 通过。
  8. `git diff --check` 仅有 Windows 换行提示。

### 节点 32：TaskSession 文件态压缩与长期恢复链路审核

- 状态：已完成审核并补充测试。
- 审核范围：
  - `run_task_session_context_compaction`
  - `spawn_task_session_context_compaction`
  - `prepare_task_runtime_stream_launch_with_preflight_compaction`
  - `current_task_snapshot_from_store`
  - `FileTaskSessionStore` 的 context、checkpoint 与 event 读写。
- 审核结论：
  1. TaskSession 压缩只通过 `task_runtime.task_session_store_for_root` 获取 `FileTaskSessionStore`，不调用普通 workspace-store。
  2. 压缩输入来自 TaskSession 自己的 `context/current.json`，压缩结果写回同一个 TaskSession 文件域。
  3. 压缩 checkpoint 写入 TaskSession `checkpoints/`，并追加 TaskSession 自己的 `events.jsonl` runtime marker。
  4. 子代理恢复时优先通过 `find_task_session_by_parent_and_public_id` 与 `load_context` 读取文件态上下文，不读取旧 payload 的内嵌转录。
  5. 已补 `task_runtime_preparation_request_uses_resolved_runtime_policy`，锁定实际执行请求必须使用 `TaskSessionConfigResolver` 的 runtime policy 解析结果。
- 验证：
  1. `cargo fmt` 通过。
  2. `cargo test -p vulcan-agent-runtime task_runtime_preparation_request_uses_resolved_runtime_policy -- --nocapture` 通过，仅有既有 VMM dead code warning。
  3. `cargo test -p vulcan-agent-runtime task_runtime::tests:: -- --nocapture` 通过，仅有既有 VMM dead code warning。
  4. `cargo test -p vulcan-task-session -- --nocapture` 通过。
  5. `cargo check -p vulcan-code-gui` 通过。
  6. `cargo check -p vulcan-code-tauri` 通过。
  7. `git diff --check` 仅有 Windows 换行提示。

### 节点 33：TaskSession profile/invocation 覆盖进入真实启动链路

- 状态：已完成第一版。
- 审核发现 TaskSession 的 `system_prompt` / `system_prompt_append` / profile prompt 会进入 `TaskSessionConfigResolver` 与文件态 metadata，但真实 provider 启动系统提示词仍只通过 `build_runtime_prompt_layers_for_agent(preparation, active_agent)` 组合。
- 这意味着独立子代理提示词存在“记录正确、请求不正确”的风险，尤其会让 profile 或 invocation 中的独立系统提示词只显示/持久化，却不影响真实模型请求。
- 已在共享执行模型中增加请求级代理系统提示词覆盖：
  - `ExecutionPreparationRequest::agent_system_prompt_override`
  - `ExecutionPreparation::agent_system_prompt_override`
- 已让 runtime prompt composer 统一消费该覆盖：
  - `provider_runtime_context::build_runtime_prompt_assembly_input` 优先使用请求级覆盖构建 `AgentPromptSnapshot`。
  - 无覆盖时继续使用 active agent 的系统提示词，普通主会话行为不变。
- 已让 TaskSession 启动请求从已组合的 `controller_config.system_prompt` 写入该共享字段，确保子代理 clone/functional prompt 进入真实 provider-facing prompt。
- 已修复恢复语义：
  - 恢复已有子代理时，如果本次 invocation 显式变更 profile、继承策略、系统提示词、模型或 runtime extra，则清空旧 controller 快照并重建。
  - 未显式传入这些字段时继续复用历史 controller 配置，避免长期任务每轮无意重组。
- 自审结论：
  1. 修复位于共享 `ExecutionPreparationRequest -> ExecutionPreparation -> runtime prompt composer` 链路，不是 iced/WebUI 私有补丁。
  2. TaskSession 的 profile/invocation 配置现在既进入 metadata，也进入真实启动请求。
  3. 工具、LuaSkills、MCP、prompt skills 仍通过共享 `resolve_execution_tool_exposure` 与 prompt-skill resolver 生效，没有界面层解析。
- 验证：
  1. `cargo fmt` 通过。
  2. `cargo test -p vulcan-agent-runtime runtime_prompt_layers_use_request_scoped_agent_prompt_override -- --nocapture` 通过，仅有既有 VMM dead code warning。
  3. `cargo test -p vulcan-agent-runtime task_runtime_preparation_request_carries_controller_prompt_override -- --nocapture` 通过，仅有既有 VMM dead code warning。
  4. `cargo test -p vulcan-agent-runtime task_runtime::tests:: -- --nocapture` 通过，仅有既有 VMM dead code warning。
  5. `cargo test -p vulcan-agent-runtime provider_runtime_context::tests:: -- --nocapture` 通过，仅有既有 VMM dead code warning。
  6. `cargo check -p vulcan-code-gui` 通过。
  7. `cargo check -p vulcan-code-tauri` 通过。

### 节点 34：iced/WebUI 子代理导航与复看判定共享化

- 状态：已完成第一版。
- 审核发现 iced 与 WebUI/Tauri 在子代理缓存合并、打开 gate、导航筛选和运行中任务树遍历上仍存在界面侧重复推导：
  - iced `selected_session_active_subagent_tasks` 只按 running 过滤，活动且可复看的终态子代理可能不再出现在导航中。
  - WebUI `subagent_navigation_snapshot_from_state` 手写“running + active recoverable”筛选。
  - 两端 `open_subagent_task_panel` / `service_open_subagent_panel` 手写“running 或可复看”判定。
  - WebUI 子代理导航轮次隐藏逻辑直接扫描本地缓存，未复用共享运行中子树遍历。
- 已在 `vulcan-workbench-model::subagent` 增加共享 helper：
  - `subagent_task_panel_is_openable`
  - `select_subagent_navigation_panel_states`
- 已让 iced 使用共享导航选择器：
  - running 子代理继续显示。
  - 当前打开且可复看的终态子代理也保留在导航中，避免只剩一个子代理时入口闪退。
- 已让 WebUI/Tauri 使用同一共享导航选择器和打开 gate：
  - `service_open_subagent_panel` 不再私有判断 openable。
  - `active_surface_session_id_from_state` 不再私有判断 openable。
  - `subagent_navigation_snapshot_from_state` 不再私有拼装 running/active recoverable 列表。
  - `refresh_webui_subagent_navigation_round_visibility` 改用共享运行中子树遍历。
- 自审结论：
  1. 本节点只抽取显示路由判定，不修改 provider 执行、task runtime 调度、队列派发或持久化结构。
  2. iced/WebUI 的子代理导航选择现在复用同一个 workbench-model helper，界面只负责转成各自的 UI/DTO。
  3. 队列阻塞和委派回调仍使用已有 `runtime_state` 共享 helper，没有新增 WebUI 私有队列逻辑。
- 验证：
  1. `cargo fmt` 通过。
  2. `cargo test -p vulcan-workbench-model subagent::tests:: -- --nocapture` 通过，12 个子代理共享模型测试全通过。
  3. `cargo check -p vulcan-code-gui` 通过。
  4. `cargo check -p vulcan-code-tauri` 通过。
  5. `cargo test -p vulcan-agent-runtime task_runtime::tests::stop_task_tree_aborts_active_descendants_without_siblings -- --nocapture` 通过，仅有既有 VMM dead code warning。
  6. `cargo test -p vulcan-workbench-model pending_prompt_dispatch -- --nocapture` 通过。
  7. `git diff --check` 无空白错误，仅有 Windows 换行提示。

### 节点 35：运行时策略覆盖与 WebUI precheck 队列判定对齐

- 状态：已完成第一版。
- 审核发现：
  1. WebUI/Tauri 的 pending prompt 阻塞判定曾把 stale `Preparing` 状态当成有效 precheck 进度；iced 使用的是活动 precheck turn 映射，不会被旧状态长期卡住。
  2. TaskSession 默认策略已禁用 hook/VMM/记忆/搜索/数据库投影，但缺少 profile 或 invocation 层对 runtime policy 的显式覆盖入口。
  3. 若后续需要“特殊能力子代理”，只靠硬编码 `ExecutionRuntimePolicy::task_session()` 会让 metadata、profile、invocation 与真实执行请求再次分叉。
- 已完成内容：
  1. WebUI/Tauri 新增 `webui_pending_prompt_has_precheck_progress`，只有存在有效 `active_precheck_turn_id` 时才认定 precheck 仍在进行。
  2. `session_has_blocking_system_prompt_work` 改用该 helper，对齐 iced 的活动 precheck 语义，避免 stale `Preparing` 卡住队列。
  3. `vulcan-session-core` 新增 `ExecutionRuntimePolicyRequest`，作为 profile 和 invocation 的可选 runtime policy 覆盖请求。
  4. `SessionRequestedConfig`、`SubagentProfile`、`TaskInvocationConfig` 接入 runtime policy request。
  5. `TaskSessionConfigResolver` 从 `ExecutionRuntimePolicy::task_session()` 起步，按 profile 覆盖、invocation 覆盖解析最终策略。
  6. 解析后强制 `search_projection_enabled = false` 与 `database_projection_enabled = false`，满足用户确认的“禁止数据库投影”边界。
  7. task 工具参数 schema 新增 `runtime_policy` 对象；恢复已有 task 时仅在 incoming runtime policy 有显式覆盖时替换旧配置。
- 自审结论：
  1. runtime policy 覆盖位于 `vulcan-session-core` 与 `vulcan-task-session` 公共配置解析链路，不是 WebUI/iced 私有补丁。
  2. 特殊能力可通过 profile 或 invocation 显式开启 hook/VMM/记忆等运行能力，但 TaskSession 普通会话数据库和搜索投影仍被强制关闭。
  3. WebUI precheck 修复是对齐 iced 活动 precheck 判定，不改变队列派发共享 router。
- 验证：
  1. `cargo fmt`：通过。
  2. `cargo test -p vulcan-task-session task_session_runtime_overrides_keep_projection_forbidden -- --nocapture`：通过。
  3. `cargo test -p vulcan-agent-runtime task_invocation_config_parses_profile_tool_skill_and_compaction_overrides -- --nocapture`：通过，仅有既有 VMM dead code warning。
  4. `cargo test -p vulcan-code-tauri webui_precheck_progress_requires_active_turn_id -- --nocapture`：通过。
  5. `cargo check -p vulcan-agent-runtime`：通过，仅有既有 VMM dead code warning。
  6. `cargo check -p vulcan-task-session`：通过。
  7. `cargo test -p vulcan-workbench-model pending_prompt_dispatch -- --nocapture`：通过。
  8. `cargo check -p vulcan-code-gui`：通过。
  9. `cargo check -p vulcan-code-tauri`：通过。
  10. `cargo test -p vulcan-task-session -- --nocapture`：通过。
  11. `cargo test -p vulcan-workbench-model subagent::tests:: -- --nocapture`：通过。
  12. `cargo test -p vulcan-agent-runtime task_runtime::tests:: -- --nocapture`：通过，仅有既有 VMM dead code warning。
  13. `cargo test -p vulcan-workbench-model pending_prompt -- --nocapture`：通过。
  14. `cargo test -p vulcan-session-core -- --nocapture`：通过。
  15. `git diff --check`：无空白错误，仅有 Windows 换行提示。

### 节点 36：用户 Agent profile 高级子代理配置接入

- 状态：已完成第一版。
- 审核发现：
  1. `RuntimeAgentDefinition` 已经支持 `skill_ids`、`native_tools`、`mcp_tools`、`tool_policy` 与 runtime policy，但用户自定义 Agent Markdown 和 GUI 配置器只保存基础字段。
  2. 这会导致“子代理支持独立技能、工具、系统提示词和运行时策略”的公共链路已经存在，但用户配置入口保存时丢失高级字段。
  3. 若在 WebUI 或 iced 侧单独补执行逻辑，会再次违反“抽取、复用、路由、不克隆”的原则。
- 已完成内容：
  1. `UserAgentProfile` 与 frontmatter schema 接入高级字段：技能、原生工具、MCP 工具、共享工具策略、runtime policy。
  2. `ExecutionRuntimePolicyRequest` 增加 camelCase 序列化与 snake_case 兼容别名，保证 YAML profile、task invocation 和旧字段风格都可解析。
  3. 用户 Agent profile 保存/读取会保留高级字段，不再在 GUI 编辑后丢失。
  4. `RuntimeAgentDefinition` 从用户 profile 投影高级字段，再转换为 `SubagentProfile`，由 TaskSession resolver 统一消费。
  5. task invocation 的 runtime policy 解析支持 camelCase 与 snake_case，保持模型调用参数和 profile YAML 风格一致。
  6. iced Agent profile 配置器只新增表单编辑项，不参与执行调度；执行仍走 `UserAgentProfile -> RuntimeAgentDefinition -> SubagentProfile -> TaskSessionConfigResolver`。
- 自审结论：
  1. 本节点只补齐配置入口与共享配置投影，没有新增 WebUI/Tauri/iced 私有执行链。
  2. 子代理高级能力现在通过统一 runtime/profile/config resolver 流转，界面层只负责编辑和展示。
  3. 即使 profile 或 invocation 请求打开搜索/数据库投影，TaskSession resolver 的强约束测试仍确认最终会禁止 DB/search 投影。
- 验证：
  1. `cargo check -p vulcan-code-gui`：通过。
  2. `cargo check -p vulcan-code-tauri`：通过。
  3. `cargo test -p vulcan-agent-runtime task_runtime::tests:: -- --nocapture`：20 个测试通过，仅有既有 VMM dead code warning。
  4. `cargo test -p vulcan-task-session task_session_runtime_overrides_keep_projection_forbidden -- --nocapture`：通过。
  5. `cargo test -p vulcan-session-core -- --nocapture`：通过。
  6. `cargo test -p vulcan-agent-runtime agent_profile::tests:: -- --nocapture`：通过，仅有既有 VMM dead code warning。
  7. `cargo test -p vulcan-agent-runtime runtime_config_projects_user_profile_advanced_overrides -- --nocapture`：通过，仅有既有 VMM dead code warning。
  8. `cargo test -p vulcan-agent-runtime runtime_agent_definition_becomes_subagent_profile_snapshot -- --nocapture`：通过，仅有既有 VMM dead code warning。
  9. `git diff --check`：无空白错误，仅有 Windows 换行提示。

## 当前下一步

1. 继续深度审核剩余 TaskSession 执行链条，重点检查是否还有 WebUI/iced 私有推导或重复路由。
2. 继续推进长期任务压缩触发和子代理文件态恢复在真实 UI 流程中的验证。
3. 完成所有节点后再进行完整自我审核、补充执行变更总结，并按计划迁移文档到 `docs/completed/`。

### 节点 42：导航轮次保留与 WebUI 文件态归一化继续共享化

- 状态：已完成第一版。
- 发现并修复的问题：
  1. iced 与 WebUI/Tauri 都有“父会话是否仍应保留当前子代理导航轮次”的条件判断，但此前仍各自拼接 active run、running subagents、delegate completion 队列事实。
  2. 已在 `vulcan-workbench-model` 新增 `subagent_navigation_round_should_remain_visible`，由共享模型层统一表达“父运行、运行中子代理、待处理委派回流任一存在则保留导航轮次”。
  3. iced 的 `session_should_keep_subagent_navigation_round` 与 WebUI/Tauri 的 `refresh_webui_subagent_navigation_round_visibility` 均改为调用共享 helper。
  4. 审核还发现 WebUI 的 `normalize_webui_subagent_task_state` 原先只使用实时 TaskRuntime 事实，未像 iced 一样把文件态或父记录中的终态快照传给共享 `normalize_subagent_task_panel_state`。
  5. 已为 WebUI 增加 `persisted_webui_subagent_task_panel_state_snapshot`，按文件态、runtime canonical records、投影消息顺序查找最新可恢复面板快照。
  6. WebUI 归一化现在只在缓存仍标记 `running` 且共享 runtime 已确认不再运行时读取持久快照，避免导航列表常规刷新时产生额外文件扫描。
- 自审结论：
  1. 本节点没有新增子代理执行、停止、队列派发或 provider 调用语义，只补齐 iced/WebUI 对同一共享模型 helper 的输入。
  2. WebUI 不再用自己的布尔表达式决定导航轮次保留，也不再缺失 iced 已有的文件态终态快照归一化输入。
  3. TaskSession 仍只通过文件态恢复，未引入数据库投影或搜索索引写入。
- 验证：
  1. `cargo fmt`：通过。
  2. `cargo test -p vulcan-workbench-model subagent_navigation_round_visibility_tracks_coordination_work -- --nocapture`：通过。
  3. `cargo test -p vulcan-workbench-model subagent::tests:: -- --nocapture`：通过。
  4. `cargo check -p vulcan-code-tauri`：通过。
  5. `cargo check -p vulcan-code-gui`：通过。
  6. `cargo test -p vulcan-code-tauri webui_stop_session_work_appends_stopped_turn_projection_once -- --nocapture`：通过。
  7. `git diff --check`：无空白错误，仅有 Windows 换行提示。

### 节点 43：WebUI 普通会话持久化与 TaskSession 标识隔离门禁

- 状态：已完成第一版。
- 发现并修复的问题：
  1. `DetachedTaskRuntimeEvent::ParentRuntime` 的 `parent_session_id` 是直接父级；嵌套子代理完成时它可能是 TaskSession id，而不是普通主会话 id。
  2. WebUI detached parent 事件路径此前会对该 id 尝试普通 `spawn_workspace_session_persist_task` 与 `maybe_prepare_next_webui_pending_prompt`。
  3. 普通 `maybe_prepare_next_webui_pending_prompt` 实际需要 `workspace_snapshot.sessions` 中的普通 session title；TaskSession id 会 no-op，但仍属于错误路由入口。
  4. 已新增 `workspace_snapshot_contains_regular_session`，统一判断某个 id 是否属于普通活动或归档 workspace-store 会话。
  5. 已让 `spawn_workspace_session_persist_task` 与 `spawn_workspace_session_runtime_state_persist_task` 在非普通会话 id 上直接返回，避免 TaskSession 误入普通数据库持久化。
  6. 已让 WebUI delegate-completion 普通 pending queue 只服务普通主会话；TaskSession 父级不再被塞进普通 WebUI 队列。
  7. 已让 detached parent 完成事件只有在直接父级是普通会话时才触发普通持久化与普通 pending prompt 派发。
- 自审结论：
  1. 该修复是保护性门禁，不新增子代理执行逻辑，也不让 WebUI 代管 TaskSession 父任务继续执行。
  2. TaskSession 仍保持文件态隔离，不进入 workspace-store 单会话持久化或 runtime-state-only 持久化。
  3. 嵌套 TaskSession 的“父任务完成子任务后继续执行”仍需要 TaskRuntime 文件态回流入口，这是架构层后续项，不能由 WebUI 普通队列临时代替。
- 验证：
  1. `cargo fmt`：通过。
  2. `cargo test -p vulcan-code-tauri webui_regular_session_detection_excludes_task_session_ids -- --nocapture`：通过。
  3. `cargo test -p vulcan-code-tauri webui_stop_session_work_appends_stopped_turn_projection_once -- --nocapture`：通过。
  4. `cargo check -p vulcan-code-tauri`：通过。
  5. `cargo check -p vulcan-code-gui`：通过。
  6. `cargo test -p vulcan-workbench-model subagent::tests:: -- --nocapture`：通过。
  7. `git diff --check`：无空白错误，仅有 Windows 换行提示。

### 节点 37：长期恢复链路与 WebUI 运行时引导门禁审核

- 状态：已完成第一版。
- 已审核链路：
  1. `build_prompt_compaction_launch_request` 使用 `ExecutionScope::ContextCompaction` 与 `ExecutionRuntimePolicy::context_compaction()`，TaskSession 压缩不会继承子代理运行策略。
  2. `TaskRuntime::prepare_task_execution_request` 可通过文件态 `task-index.json` 与 `context/current.json` 恢复已有 TaskSession，不依赖旧父 payload。
  3. `run_task_with_shared_runtime_context` 通过共享 runtime stream 运行子代理，完成后逐个发布 `DelegateFinished` 给直接父级。
  4. `subagent_task_panel_states_from_task_session_store` 支持 root 与 nested TaskSession 直接父级读取，iced/WebUI 均复用。
  5. iced 与 WebUI 子代理导航、openable gate、运行态树遍历已经收敛到 `vulcan-workbench-model` 共享 helper。
- 发现并修复的问题：
  1. WebUI 的动态运行时引导入口 `take_webui_internal_guidance_prompts` 直接调用旧的 guided take helper，只按 Anthropic 偏好取队列，未使用共享 `PendingPromptDispatchContext`。
  2. 该路径会在仍有运行中子代理时，把普通用户引导注入 provider runtime boundary，绕过 iced/shared 队列阻塞语义。
  3. 已新增 `take_runtime_intervention_guided_pending_prompt_entries`，复用 `pending_prompt_dispatch_prefers_delegate_completion` 与 `next_pending_prompt_dispatch_source`。
  4. 当仍有运行中子代理且下一条不是 `DelegateCompletion` 时，共享 helper 返回空列表，保持用户引导在队列中等待。
  5. WebUI/Tauri 现在在取运行时引导前刷新文件态子代理树，并用同一个 `webui_pending_prompt_dispatch_context` 调用共享 helper。
- 自审结论：
  1. 修复位于 `vulcan-workbench-model` 共享队列层，WebUI 只传入运行态事实，没有创建新的私有调度语义。
  2. iced 的普通队列分发与 WebUI 运行时引导现在共用同一套“运行中子代理优先闭合委派回调、阻塞普通用户跟进”的规则。
  3. TaskSession 文件态恢复、压缩、复看与嵌套打开链路本轮未发现新的数据库投影或旧 payload 恢复残留。
- 验证：
  1. `cargo fmt`：通过。
  2. `cargo test -p vulcan-workbench-model runtime_intervention_guidance -- --nocapture`：2 个测试通过。
  3. `cargo test -p vulcan-workbench-model -- --nocapture`：31 个测试通过。
  4. `cargo check -p vulcan-code-tauri`：通过。
  5. `cargo check -p vulcan-code-gui`：通过。
  6. `cargo test -p vulcan-agent-runtime task_runtime::tests::prepare_task_execution_request_restores_file_backed_context_for_resume -- --nocapture`：通过，仅有既有 VMM dead code warning。
  7. `cargo test -p vulcan-agent-runtime task_runtime::tests::store_task_outcome_persists_file_backed_task_session_artifacts -- --nocapture`：通过，仅有既有 VMM dead code warning。
  8. `cargo test -p vulcan-workbench-model subagent::tests:: -- --nocapture`：12 个测试通过。
  9. `git diff --check`：无空白错误，仅有 Windows 换行提示。

## 当前下一步

1. 继续完整自我审核，重点检查 WebUI/Tauri 中是否还有与 iced 不一致的子代理停止、队列、导航或恢复私有逻辑。
2. 重点复核 `mark_webui_subagent_tasks_stopped_in_state`、`stop_webui_session_work`、detached listener 生命周期是否只是桥接显示，不能成为新的运行态事实源。
3. 完成所有节点后再追加计划文件的「执行变更总结」并迁移到 `docs/completed/`；当前大计划尚未整体闭环，不能提前迁移。

### 节点 40：子代理导航显示门禁共享化

- 状态：已完成第一版。
- 发现并修复的问题：
  1. iced 的子代理导航显示前会先判断当前会话是否仍处于子代理协调轮次，或是否仍有运行中子代理。
  2. WebUI/Tauri 的 `subagent_navigation_snapshot_from_state` 只要缓存中存在子代理状态就会生成导航快照，没有套同等轮次门禁。
  3. 该差异会让历史 TaskSession 缓存被误当成当前调度面板，表现为旧子代理列表残留、标题栏机器人入口长期存在或悬浮状态不稳定。
  4. 已在 `vulcan-workbench-model` 新增 `subagent_navigation_surface_is_visible`，把“当前轮次 / 运行中子代理 / 当前已打开子代理面板”的 UI 显示门禁抽为共享 helper。
  5. iced 的 `selected_session_active_subagent_tasks` 与 WebUI/Tauri 的 `subagent_navigation_snapshot_from_state` 现在都调用该共享 helper。
  6. WebUI 仍保留“已打开子代理面板时显示主会话返回入口”的能力；这只影响界面路由，不改变 task 执行或队列调度。
- 自审结论：
  1. 本节点只处理导航快照是否暴露给界面，不参与 TaskRuntime、Provider、队列派发、文件态存储或数据库投影。
  2. WebUI 不再通过历史缓存直接决定标题栏子代理菜单可见性，避免和 iced 当前轮次语义分叉。
  3. 新增规则位于共享模型层，iced 与 WebUI 都走同一个 helper，符合“抽取、复用、路由、不克隆”原则。
- 验证：
  1. `cargo fmt`：通过。
  2. `cargo test -p vulcan-workbench-model subagent_navigation_surface_visibility_matches_current_round -- --nocapture`：通过。
  3. `cargo test -p vulcan-workbench-model subagent::tests:: -- --nocapture`：通过。
  4. `cargo check -p vulcan-code-gui`：通过。
  5. `cargo check -p vulcan-code-tauri`：通过。

## 当前下一步

1. 继续完整自我审核 WebUI/Tauri 中剩余直接解析 task payload、直接改父 task 卡片、直接处理 detached listener 的函数。
2. 重点判断这些函数是否可以继续上提共享，或是否只是 WebUI 快照/显示容器适配。
3. 继续验证 TaskSession 文件态运行、恢复、停止、复看和队列回流不会触发普通会话数据库投影。

### 节点 41：父 task 卡片匹配逻辑上提共享

- 状态：已完成第一版。
- 发现并修复的问题：
  1. iced 的 `find_task_session_tool_message_id_for_payload` 和 WebUI/Tauri 的 `find_webui_task_session_tool_message_id_for_payload` 都在本地扫描父级 task 工具卡片，并用同一套分数规则匹配 task-session payload。
  2. WebUI 还额外复制了 message/record 两套 collect 函数，属于显示桥接层中的重复业务判断。
  3. 已在 `vulcan-workbench-model` 新增共享 `find_task_session_tool_message_id_for_payload`，统一处理投影消息与规范会话记录中的匹配。
  4. iced 和 WebUI/Tauri 均改为调用共享 finder；各自只保留“如何把 metadata 写回自己的容器”的显示投影适配。
  5. 删除 WebUI/Tauri 本地 `collect_best_webui_task_session_message_match` 与 `collect_best_webui_task_session_record_match`。
- 自审结论：
  1. 匹配规则已经从 GUI/Tauri 层收敛到共享模型层，减少两端无意义差异。
  2. WebUI/Tauri 剩余的 merge/replace/seed 函数属于 workspace snapshot 容器更新，不再包含 task 卡片匹配算法。
  3. 本节点没有改变 TaskSession 执行、队列、停止或持久化语义。
- 验证：
  1. `cargo fmt`：通过。
  2. `cargo test -p vulcan-workbench-model finds_task_session_tool_message_id_across_messages_and_records -- --nocapture`：通过。
  3. `cargo test -p vulcan-workbench-model subagent::tests:: -- --nocapture`：通过。
  4. `cargo check -p vulcan-code-gui`：通过。
  5. `cargo check -p vulcan-code-tauri`：通过。

## 当前下一步

1. 继续扫 `service_bridge.rs` 中剩余 WebUI-only 子代理桥接函数，判断是否只剩容器适配。
2. 重点复核 detached listener 完成事件后队列回流、父会话持久化、导航轮次隐藏是否都走共享 helper。
3. 继续运行必要验证，最后做完整自审与计划总结。

### 节点 44：父级 TaskSession 委派完成回流进入共享 TaskRuntime

- 状态：已完成第一版。
- 已确认父 TaskSession 自己调用子代理后，父 provider loop 通常已经结束，不能依赖 WebUI/iced 普通 pending queue 继续父任务。
- 已在共享 `TaskRuntime` 增加父 TaskSession follow-up FIFO 队列，终态回调用 child task id 去重，按完成顺序串行注入父 TaskSession。
- 已复用 `build_delegate_completion_followup_prompt`、`prepare_task_runtime_stream_launch_with_preflight_compaction` 和 `drive_runtime_stream_provider_loop`，没有在 WebUI 或 iced 新增私有执行逻辑。
- 已把父 TaskSession follow-up 运行注册进 `active_task_runs`，让现有 `stop_task_tree` 能继续终止父级续跑和后代任务。
- 已调整 detached stream 关闭条件：父任务初始轮次结束但仍有后代运行时，保持事件流，不提前关闭。
- 已把无效 follow-up 跳过从递归改为循环，降低异常队列积压时的运行时风险。
- 已增加父 follow-up 启动预留门禁，避免多个子任务同时完成时并发启动同一父 TaskSession 的多轮续跑。
- 已让 `is_task_running` 与重复 task 启动检查识别 active 和 starting 状态，并让 `stop_task_tree` 能取消 starting follow-up。
- 验证：
  1. `cargo fmt`：通过。
  2. `cargo test -p vulcan-agent-runtime task_session_parent_followup -- --nocapture`：通过，4 个测试通过。
  3. `cargo test -p vulcan-agent-runtime stop_task_tree -- --nocapture`：通过，2 个测试通过。
  4. `cargo test -p vulcan-agent-runtime task_runtime::tests:: -- --nocapture`：通过，25 个测试通过。
  5. `cargo test -p vulcan-workbench-model pending_prompt_dispatch -- --nocapture`：通过。
  6. `cargo test -p vulcan-code-tauri webui_regular_session_detection_excludes_task_session_ids -- --nocapture`：通过。
  7. `cargo test -p vulcan-code-tauri webui_stop_session_work_appends_stopped_turn_projection_once -- --nocapture`：通过。
  8. `cargo test -p vulcan-workbench-model subagent::tests:: -- --nocapture`：通过，15 个测试通过。
  9. `cargo check -p vulcan-code-gui`：通过。
  10. `cargo check -p vulcan-code-tauri`：通过。
  11. `git diff --check`：无空白错误，仅有 Windows 换行提示。

## 当前下一步

1. 继续最终自我审核 WebUI/Tauri 中剩余子代理桥接函数，确认只剩显示容器适配。
2. 继续复核父 TaskSession follow-up 的停止、失败、嵌套父级回流和 detached stream 生命周期是否存在边界遗漏。
3. 完成所有节点后再追加计划文件的「执行变更总结」并迁移到 `docs/completed/`；当前大计划尚未整体闭环，不能提前迁移。

### 节点 45：WebUI/Tauri detached bridge 与 iced 路由差异复核

- 状态：已完成审核，无新增代码。
- 已确认 WebUI/Tauri 的 detached 子代理 bridge 剩余代码主要是 `WorkspaceSnapshot` 显示容器适配。
- `enqueue_webui_delegate_completion_followup_if_needed` 已用 `workspace_snapshot_contains_regular_session` 阻止 TaskSession 父级进入普通 WebUI pending prompt 队列。
- `apply_conversation_checkpoint` 只有普通主会话触发 runtime-state 持久化；TaskSession checkpoint 只更新内存显示态。
- `apply_execution_report_to_memory` 只有普通主会话触发 completion hook、VMM profile sync 与 VMM snapshot replacement；TaskSession 完成只更新显示态。
- TaskSession 父级续跑、嵌套回流、停止窗口和重复启动门禁已进入共享 `TaskRuntime`，不是 WebUI/iced 私有逻辑。
- 验证沿用节点 44 最新通过结果，并补充本轮 `git diff --check`：无空白错误，仅有 Windows 换行提示。

## 当前下一步

1. 继续核对计划验收项是否全部覆盖，决定是否进入最终「执行变更总结」。
2. 若发现遗漏，继续补齐；若无遗漏，再迁移计划到 `docs/completed/`。

### 节点 46：WebUI 子代理活动面板跨会话抢屏修复

- 状态：已完成第一版。
- 用户复现：加载会话时未点击任何内容会突然跳入子代理执行内容；点击其他会话后会被迟到快照拉回旧子代理；点击主会话回不去。
- 根因定位：WebUI/Tauri 保留的 `active_subagent_panel` 没有像 iced 一样在选择主会话时清空，并且后端快照函数允许旧 active panel 参与当前选中会话的 surface 计算。
- 已修复：
  1. `service_select_conversation` 选择普通会话时清空 `active_subagent_panel` 并重建当前会话子代理缓存。
  2. `active_surface_session_id_from_state` 和 `subagent_navigation_snapshot_from_state` 只允许当前 `workspace.selected_session` 的子代理控制显示面；跨父会话残留会被清空。
  3. WebUI `ChatArea` 只接受 `parentConversationId === selectedConversationId` 的 `subagentNavigation`。
  4. WebUI 侧栏选择会话时先本地执行 `closeSubagentSurfaceForConversation`，防止异步快照返回前闪回旧子代理。
  5. 增加 `webui_active_subagent_panel_cannot_cross_selected_session` 回归测试。
- 验证：
  1. `cargo fmt`：通过。
  2. `cargo test -p vulcan-code-tauri webui_active_subagent_panel_cannot_cross_selected_session -- --nocapture`：通过。
  3. `cargo check -p vulcan-code-tauri`：通过。
  4. `npm run lint`（`apps/vulcan-code-web`）：通过。
  5. `cargo test -p vulcan-workbench-model pending_prompt_dispatch -- --nocapture`：3 个测试通过。
  6. `cargo test -p vulcan-workbench-model subagent::tests:: -- --nocapture`：15 个测试通过。

## 当前下一步

1. 让用户实测：加载普通会话、点击当前会话、点击其他会话，都不应自动跳进旧子代理内容。
2. 继续定位“两个子代理启动迟到/无下文”的执行链，优先确认是否是子代理 listener 事件没有及时回写或父会话 pending follow-up 没有继续触发。
3. 若仍显示“正在请求”但实际无运行任务，继续检查 WebUI 的 running 子代理判定是否与共享 TaskRuntime live-run 状态一致。
