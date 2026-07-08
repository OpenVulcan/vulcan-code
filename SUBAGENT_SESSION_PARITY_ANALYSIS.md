# 子代理会话同构化专项分析

日期：2026-07-06

状态：持续实施中，当前文件同时记录分析、阶段修复与验证证据。

## 任务背景

本专项用于记录子代理与主会话在提示词、运行状态、会话记录、统计、回流和 fork 语义上的问题分析与长期修复方向。该文件作为独立任务资料，不并入 `TASK_LOG.md`。

用户明确要求：子代理和主代理唯一的本质差异应尽可能少。子代理不允许继续创建子代理；除此之外，会话记录机制、统计机制、恢复机制、目录结构语义都应与主会话一致。

换句话说，子代理目录如果被单独复制出来，应该可以按主会话等价载入。子代理不是一种“弱化版任务记录”，而是一个完整会话，只是运行在子会话目录下。

## 用户补充的硬性约束

1. 子代理不允许创建子代理。
2. 子代理的会话记录机制必须与主会话一致。
3. 子代理统计逻辑必须与主会话一致。
4. 子代理恢复逻辑必须与主会话一致。
5. 子代理记录应落在子会话目录，但格式和语义应能等同主会话载入。
6. 子代理额外差异只能来自特殊提示词。
7. 主代理系统提示词可以是动态的。
8. 子代理系统提示词应在创建时从主代理克隆并固定，不应之后跟随主代理动态变化。
9. 子代理需要额外记录创建时固定下来的提示词快照或子代理提示词差异。
10. 工具限制属于工具策略层，不应该污染上下文和会话记录。
11. 子代理不触发主会话级 hook，例如记忆写入、postaction、自动总结、全局副作用类后处理。
12. 子代理结果只能通过明确回流记录交给父会话，是否触发主会话 hook 应由父会话后续正常 turn 决定。
13. 子代理工具策略必须有统一配置源，不允许 ask、plan、todolist、子代理调度、记忆等明确禁用工具在多个区域分别维护。
14. 子代理模式下明确禁用的工具，即使被手写到允许列表，也必须按未允许处理。
15. 克隆 / mirror 模式下，子代理应注册当前主会话所有工具，但对 ask、plan、todolist、子代理调度、记忆等禁用工具调用返回固定禁用反馈，不应以错误中止当前 turn。
16. default 模式下，明确禁用的工具不注册，其余工具同步主会话。
17. agent 模式未设置工具限制时等同 default；设置限制时按智能体配置和统一禁用规则共同裁剪。
18. 智能体配置界面或配置 schema 中不应允许选择子代理明确禁用的工具。
19. 子代理需要支持自动上下文压缩，并且压缩预算应按子代理实际模型判断。
20. fork 只有 `is_fork` 概念，fork 的对象是 runtime state，不是会话显示记录。
21. fork 后上下文必须一字不差，除必要身份字段外不应按轮次裁剪、重建或改写。
22. 不存在“fork 几轮”的概念，也不应以轮次做判定。
23. 不考虑旧子代理记录结构的历史兼容，新结构按干净设计落地。
24. UI 展示统一使用“智能体”，不要在用户可见界面混用“子代理”。

## 用户最新校正规则

以下内容修正此前分析中的潜在歧义，后续实现以本节为准。

### 沙箱与权限

当前项目尚未实现完整沙箱权限控制机制，因此权限继承只能作为未来设计方向记录，不能在本轮子代理同构任务中落成实际配置。只有当沙箱权限控制机制真正实现后，才能把子代理权限策略纳入可配置项。

### 记忆与 hook

记忆工具禁止智能体直接调用，记忆相关处理应由主代理负责。智能体不触发记忆、postaction 等主会话级 hook，也不通过工具调用绕过主会话处理链路。

### 资源生成与引用

智能体生成的资源不需要额外设计资源 id 归属规则。智能体只需把资源地址反馈给主代理，主代理可以根据地址直接使用。主代理自身不负责接管或重写资源 id。

### 停止语义

父会话主动终止与智能体没有天然级联关系。点击停止主会话时，如果存在正在工作的智能体，应弹出确认：

1. 仅停止主会话。
2. 同时停止主会话和正在工作的智能体。
3. 取消停止。

这里“正在工作”不包含待命智能体。待命智能体不应因为主会话停止而自动关闭或中断。

### 禁用工具调用

`todolist` / `todo list` 属于智能体明确禁用工具，和 ask、plan、智能体调度、记忆工具处于同一禁用层级。

禁用工具被调用时，仍应记录为一次工具调用，因为调用失败不等于没有调用。clone / mirror 模式下，所有注册但不允许使用的工具，需要在隐式提示中告知模型：

- 当前处于智能体模式。
- 哪些已注册工具实际不可用。
- 调用这些工具会得到固定禁用反馈。

该提示是隐式运行提示，不应作为用户可见文案。

### 等待输入状态与临时 UI 状态

ask 弹窗、plan 等待确认等状态属于等待用户输入状态。在这类状态下不可能同时调度智能体，因此不需要考虑克隆这些等待状态。

智能体不允许用户直接输入，因此也不存在临时输入框、临时弹窗草稿等需要复制的问题。

### 克隆时点与 runtime state

智能体创建克隆必须基于创建时最新的 API 通讯运行时状态，而不是基于某个旧 turn 或显示记录反推。例如主会话已经进行了 20 次 API 通讯后创建智能体，克隆截止点就是这 20 次 API 后的最新运行时状态。

如果主会话未压缩，clone / mirror 应复制未压缩的 API 通讯运行时记录，确保对话上下文等同主代理上下文。如果主会话已经压缩，则复制压缩后的实际运行时状态。关键是直接复制 API 通讯运行时记录，不从显示记录反推。

### 固定快照

智能体拥有创建时固定快照。该快照不同于主代理的动态配置，创建后不随主代理后续动态变更而变化。固定快照至少应覆盖提示词组装输入、模型配置、工具策略、压缩预算判断所需模型信息等。

### 回流显示记录

智能体回流主会话必须有明确的用户可见记录。该记录不是隐藏用户提示词，而是显示层记录，用于用户查询智能体反馈和查看统计。

该记录应包含：

- 智能体名称。
- 智能体任务 id。
- 反馈内容。
- 该次智能体 turn 的统计信息。
- 状态。

智能体回流应拆成两条明确链路：

1. AI 可见回流消息：保持当前语义，直接进入父会话 runtime，用于后续 API 通讯，但不进入显示记录。
2. 用户可见显示记录：独立写入一条智能体反馈记录，用于统计、展示和用户查询，不作为 runtime 事实源。

显示记录不应通过自身反推 API 通讯状态，也不应伪装成隐藏用户输入。runtime 回流消息和显示反馈记录需要通过同一个智能体任务 id / turn id 建立关联。

## 已确认的真实现象

### 1. 子代理首轮输入异常偏大

真实测试中，父会话输入约 2 万多 tokens，而子代理 `default` / `mirror` 首轮输入超过 3 万 tokens。该现象不符合常识：如果只是继承上下文，子代理输入不应比父会话基础输入明显膨胀。

样本中两个子代理的 system prompt layer 约 57.5k 字符，且以下内容重复出现：

- `You are VulcanCode`
- `Developer Instructions`
- `Runtime Context`
- `LuaSkills Implicit Dispatch`

结论：这是系统提示词重复拼接，不是正常继承造成。

### 2. default 子代理不应继承却出现继承语义

实际落盘元数据中，`default` 子代理出现了类似 `inherit_parent_config=true`、`main_clone` 一类语义。按照产品语义：

- `default` 应是普通独立子代理。
- `mirror` 才应继承父会话上下文。
- `custom` 应是 `default` 加一次性自定义 system prompt。
- `agent` 应按已启用智能体画像启动。

当前内部模式、继承、fork、clone 概念混用，导致模型看到的创建模式与实际运行结构不一致。

### 3. 子代理会话记录不是主会话同构记录

实际子代理 `s001.jsonl` 中主要记录为：

- task meta
- runtime marker
- task feedback

它不像主会话一样完整记录 user / assistant / tool / turn stats 等显示事件。子代理界面运行时依赖缓存投影，重启后依赖另一套状态恢复，因此出现：

- 运行时看到过程，重启后过程丢失。
- 子代理统计运行时为空，重启后才出现或显示不同。
- 子代理关闭后状态同步异常。
- 子代理列表和详情之间存在时序不一致。

结论：子代理不是走主会话同一条会话管线，而是有一套任务反馈投影链路。

### 4. 子代理回流污染父会话统计

当前实现会把子代理完成后的 usage 放入父会话的 pending usage，然后在父会话结束时合并进父会话统计。

结果包括：

- 父会话统计包含子代理 token。
- 子代理独立回流可能产生一个 0 token 父会话统计块。
- 随后又出现一个只包含子代理 usage 的统计块。
- UI 无法稳定显示“常规 / 智能体”两行统计，因为数据模型没有一等分组。

结论：子代理消耗不应该改变主会话常规统计。它应作为独立子代理反馈记录存在，最终统计块再以分组形式展示。

### 5. fork 语义混入回退重建

底层 `fork_segmented_session_directory` 较接近正确方向：复制目录并改写会话 / 子任务身份，不重放、不截断。

但 service / UI 层的 `forkConversation` 会携带 `beforeMessageId`，后端先 fork，再调用 `rollback_persisted_session_before_user_prompt` 截断会话并重建 runtime log。

这导致 fork 同时承担两个概念：

- 完整 runtime state fork。
- 从某条消息前回退出来的分支。

用户要求 fork 只能表示前者。后者如果需要，应作为独立的“编辑 / 回退 / 从消息分支”能力，不应叫 fork。

## 已确认的源码参考路径

### 提示词重复拼接

- `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/controller_config.rs`
  - `build_task_controller_config`
  - `build_subagent_clone_system_prompt`
  - `build_legacy_task_system_prompt`
- `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_launch.rs`
  - `build_task_runtime_preparation_request`
  - `task_execution_outcome_from_report`
  - `subagent_usage_segments_from_report`
- `crates/vulcan-agent-runtime/src/agent_runtime/provider_runtime_context.rs`
  - `build_runtime_prompt_layers_for_agent`
  - `build_runtime_prompt_assembly_input`
  - `build_request_agent_prompt_override_snapshot`
- `crates/vulcan-prompt-composer/src/strategy/primary.rs`
- `crates/vulcan-prompt-composer/src/strategy/subagent_clone.rs`
- `crates/vulcan-prompt-composer/src/strategy/subagent_functional.rs`

当前关键错误链路：

1. 父级 `base.system_prompt` 已经是完整运行时提示词。
2. 子代理创建时又把它包进一份子代理 system prompt。
3. 该完整 prompt 被放进 `agent_system_prompt_override`。
4. provider runtime 再走一次 Primary prompt assembly。
5. 基础提示词、开发者指令、运行时上下文、工具说明被二次拼接。

### creation mode / fork turns 映射

- `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tool_surface.rs`
  - `spawn_agent_internal_mode_and_fork`
- `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/arguments.rs`
  - `effective_spawn_agent_fork_turns`
  - `normalize_spawn_agent_fork_turns`

当前 `mirror` 被映射成 `fork_turns=all`，`default/custom/agent` 映射成 `none`。底层仍保留按轮次数 fork 的概念，这与“fork runtime state，不按轮次判定”的要求冲突。

### 子代理统计与回流

- `crates/vulcan-agent-runtime/src/turn_stats.rs`
  - `build_session_turn_stats_from_state_with_segments`
- `crates/vulcan-workbench-model/src/turn_stats.rs`
  - `SessionTurnStats`
  - `SessionTurnUsageSegment`
  - `SessionTurnUsageSegmentKind`
- `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/persistence_methods.rs`
  - `append_task_context_turn_stats_record`
  - `merge_task_usage_segments_into_record_stats`
- `crates/vulcan-service-kernel/src/session_runtime_kernel.rs`
  - `enqueue_webui_subagent_mailbox_input`
  - `subagent_mailbox_usage_segments_from_metadata`
- `crates/vulcan-service-kernel/src/agent_stream/mailbox.rs`
  - `append_webui_mailbox_intervention_messages`
  - `record_webui_mailbox_usage_segments_for_inputs`
- `crates/vulcan-service-kernel/src/agent_stream/stream_task.rs`
  - 子代理 pending usage 被合并到 `report.embedded_turn_usage_segments`
- `crates/vulcan-service-kernel/src/subagent_event_ingest.rs`
  - `take_pending_webui_subagent_usage_segments`

当前关键错误链路：

1. 子代理完成事件携带 usage。
2. usage 被放入父会话 pending usage。
3. 父会话结束时把 pending usage 合并进父会话 terminal report。
4. 主会话 turn stats 因此包含子代理消耗。
5. 独立回流时可能出现零统计、重复统计、子代理-only 统计。

### 子代理界面 / 持久化投影

- `crates/vulcan-service-kernel/src/subagent_event_ingest.rs`
- `crates/vulcan-service-kernel/src/agent_stream/detached_subagent.rs`
- `crates/vulcan-service-kernel/src/task_mirror_status_projection.rs`
  - `replace_webui_subagent_task_conversation_state`

当前问题是子代理详情更多依赖运行态投影缓存，而不是同构的主会话记录。因此 live view 和 reload view 会出现差异。

### fork

- `apps/vulcan-code-web/src/service/conversation-actions-service.ts`
  - `forkConversation`
- `apps/vulcan-code-web/src/components/conversation/conversation-workspace.tsx`
- `apps/vulcan-code-web/src/components/conversation/conversation-render-items.tsx`
- `crates/vulcan-service-kernel/src/conversation_actions.rs`
  - `service_fork_conversation`
- `crates/vulcan-workspace-store/src/workspace_store/persistence/edit_fork.rs`
  - `fork_persisted_session`
  - `rollback_persisted_session_before_user_prompt`
- `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/session_fork.rs`
  - `fork_segmented_session_directory`
- `crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/message_events.rs`
  - `truncate_segmented_session_before_message`

当前关键错误链路：

1. UI fork 请求可以携带 `beforeMessageId`。
2. service 先复制会话。
3. 然后按 `beforeMessageId` 回滚并重建 runtime log。
4. fork 结果不再是一字不差的 runtime state 副本。

## 长期修复设计

### 1. 抽出主会话共享处理流程

应把主会话完整 turn 管线抽成共享内核：

1. 准备执行请求。
2. 组装提示词。
3. 调用 provider。
4. 写入 conversation state。
5. 写入 display records。
6. 生成 terminal report。
7. 构建 turn stats。
8. 持久化。
9. reload 时从同一记录源恢复。

主会话和子会话都走这条管线。区别只能通过参数表达：

- `session_role = main | subagent`
- `tool_policy`
- `hook_policy`
- `subagent_prompt_snapshot`
- `can_spawn_subagent = false`
- `storage_root = main_session_dir | task_session_dir`

不要让子代理继续使用 task feedback 作为主要会话记录。

### 2. 子代理目录必须可等价载入为主会话

子代理目录内应该具备主会话需要的同构记录：

- state
- runtime log
- segmented display events
- conversation records
- turn stats
- attachments / tool artifacts
- metadata

如果将子代理目录复制出来并赋予普通会话身份，应能按主会话方式载入。除身份字段、父子关系字段、子代理创建提示词快照外，不应需要专门解释器。

### 3. 提示词只允许组合一次

子代理启动时不应生成一整段最终 system prompt 再交给主会话 Primary prompt assembly。

正确模型：

1. 创建子代理时记录父会话当时的提示词快照。
2. 记录子代理额外提示词。
3. provider runtime 在最终执行前组合一次。
4. 子代理后续 turn 固定使用创建时快照，不追随主会话动态系统提示词变化。

子代理提示词差异应表现为一条额外记录或额外 prompt segment，而不是复制整套主提示词再嵌套。

### 3.1 扩展主会话提示词组装器承载子代理

更合适的长期方向不是维护一套独立的子代理提示词组装器，而是扩展主会话现有提示词组装器，让同一个组装入口根据参数产出主会话或子代理提示词。

推荐模型：

1. 保留一个统一 prompt assembly pipeline。
2. 通过结构化参数表达当前会话角色，例如 `main`、`subagent_default`、`subagent_mirror`、`subagent_agent`、`subagent_custom`。
3. 主会话动态系统提示词仍按现有逻辑实时计算。
4. 子代理创建时复制主会话当时的 prompt assembly 输入或处理结果快照，并固定在子会话元数据中。
5. 子代理后续 turn 使用固定快照作为基础，再追加子代理专属 prompt segment。
6. 工具可见性、禁止继续创建子代理、模型覆盖、agent profile 等都作为独立策略参数进入组装器，不通过字符串拼接污染基础提示词。

这样主会话和子代理可以共享同一个提示词处理器、同一套去重与分层逻辑，只在参数层面区分行为。相比单独维护子代理 prompt builder，这种方案更容易保证：

- 基础提示词不会重复出现。
- 主会话和子代理行为一致。
- 子代理创建时快照可追溯。
- 后续新增系统提示词段落时不会漏同步到子代理。
- `default`、`mirror`、`agent`、`custom` 的差异能在类型和参数层面表达，而不是靠拼接后的文本猜测。

### 4. 子代理禁止再创建子代理应由工具策略控制

子代理不能创建子代理，这是工具可见性 / 工具策略问题。

不应为了禁止子代理继续派生而改变会话记录结构、上下文结构或提示词拼接结构。

### 4.1 子代理工具策略需要统一配置源

子代理工具限制必须由一个统一策略源控制，例如统一的 `SubagentToolPolicy` 或等价配置。所有区域都应读取同一份规则，包括：

- 工具注册。
- 工具 schema 暴露。
- 智能体配置可选工具列表。
- runtime 调用拦截。
- UI 展示。
- 测试断言。

不允许每个区域各自写一份禁用列表。后续如果新增一个子代理禁用规则，应该只改一个地方，所有链路自动应用。

子代理明确禁用的工具属于硬禁用。即使用户、智能体配置或手写 allowlist 里指定开启这些工具，也必须按未允许处理，不应绕过统一策略。

明确禁用工具至少包括交互 / 调度类能力，例如：

- ask。
- plan。
- 创建或调度子代理。
- 会导致子代理继续派生子代理的工具。

具体工具 id 不应散落在各处硬编码，而应由统一策略源产出。

### 4.2 不同子代理模式下的工具注册语义

#### default 模式

default 子代理应同步主会话可用工具，但不注册子代理硬禁用工具。

也就是说，default 模式下模型根本看不到 ask、plan、todolist、子代理调度、记忆工具等硬禁用工具；其余工具跟随主会话工具集合和当前工具策略。

#### clone / mirror 模式

clone / mirror 模式需要尽可能保持当前主会话运行状态的一致性，因此工具集合层面应注册当前主会话全部工具。

但对 ask、plan、todolist、子代理调度、记忆工具等子代理硬禁用工具，运行时必须拦截调用并返回固定反馈，而不是报错中止 turn。

固定反馈语义应类似：

`当前处于子代理模式，禁止调用该交互工具。请直接完成当前子任务，或将需要父会话处理的信息写入最终反馈。`

这里的关键点：

- 工具可以显示为存在，以保证 clone / mirror 模式对主会话工具环境的一致性。
- 交互类工具被调用时必须返回明确文本。
- 返回应作为工具结果进入模型上下文。
- 不应抛错中断整个执行。
- 不应触发真实 ask、plan 或子代理调度副作用。

#### agent 模式

agent 模式如果没有工具限制，应等同 default 模式。

agent 模式如果配置了工具限制，应在智能体配置允许范围内注册工具，但仍必须叠加子代理统一硬禁用规则。

智能体配置中不应出现子代理明确禁用工具作为可选项。如果历史配置或手写配置包含这些工具，也按未允许处理。

### 4.3 子代理自动上下文压缩

子代理需要支持自动上下文压缩，且压缩判断应基于子代理实际使用的模型，而不是父会话模型。

规则：

- default / custom / agent / mirror 都应能触发自动压缩。
- 压缩预算使用子代理 provider / model 的上下文窗口。
- mirror 模式继承父 runtime state 后，也应按子代理模型重新计算是否需要压缩。
- 压缩流程应走与主会话同构的压缩记录和统计机制。
- 子代理压缩不能触发主会话级 hook。
- 子代理压缩记录落在子会话目录。

### 4.4 子代理不触发主会话级 hook

子代理虽然应复用主会话的执行、记录、统计和恢复管线，但不应触发主会话级 hook。这里的 hook 包括但不限于：

- 记忆写入。
- postaction。
- 自动总结。
- 全局结果回收。
- 会改变父会话或全局状态的后处理。

原因是子代理是父会话派生出的工作会话，它的输出应先形成一条明确的子代理反馈记录，再由父会话在后续常规流程中决定如何吸收。子代理如果直接触发记忆或 postaction，会绕过父会话的上下文判断，造成全局状态被子任务污染。

因此共享主会话管线时，需要把 hook 作为策略层参数隔离：

- 主会话：按正常规则触发 hook。
- 子代理：默认关闭主会话级 hook。
- 子代理回流：只写反馈记录和子代理统计，不直接触发全局 hook。
- 父会话消费回流后：如果父会话正常 turn 满足条件，再由父会话触发自己的 hook。

### 5. 回流应成为独立记录

子代理完成后，父会话应插入独立记录，例如：

`智能体 abcd_agent 信息反馈`

该记录包含：

- 智能体公开名称。
- 智能体真实任务 id。
- 反馈文本。
- 智能体该 turn 统计。
- 智能体状态。
- provider / model。
- 可展开的详细消耗。

这条记录是用户可见显示记录，用于统计展示和查询智能体反馈。它不是隐藏用户提示词，也不应作为 runtime 事实源。

AI 可见回流消息继续直接写入父会话 runtime，保持当前“回流给 AI”的语义，但不进入显示记录。显示记录和 runtime 回流消息是两条独立链路，通过同一个智能体任务 id / turn id 关联。

### 6. 统计模型应支持分组

`SessionTurnStats` 需要支持一等分组，而不是只靠 `usage_segments` 猜测：

- 常规：父会话本轮 provider / tool / token。
- 智能体：本轮消费的子代理反馈与对应消耗。
- 总计：可选，由 UI 展示时汇总。

子代理自身的统计仍然是普通常规统计，因为在子代理会话内部，它就是该会话的主 provider turn。

### 7. fork 只保留 runtime state fork

fork API 应去掉 `beforeMessageId` 语义。

保留：

- 完整复制 runtime state。
- 改写 session id / task id / 必要身份字段。
- 记录 `is_fork` 和来源信息作为展示标记。

移出：

- 按消息回退。
- 按轮次裁剪。
- runtime log 重建。
- `fork_turns` 数字语义。

如果仍需要“从某条消息前分支”，应建立独立能力，例如 `branch_from_message` 或编辑回退功能，不再叫 fork。

## 验收标准

### 提示词

- default 子代理首轮不包含父会话历史上下文。
- mirror 子代理继承父 runtime state，但基础系统提示词只出现一次。
- custom 子代理只比 default 多一份自定义 system prompt。
- agent 子代理只比 default 多画像提示词 / 工具策略。
- 子代理创建后提示词快照固定，不随父会话后续动态提示词变化。

### 会话记录

- 子代理 turn 记录与主会话 turn 记录同构。
- 子代理目录复制出来后，应能按主会话方式恢复。
- live view 和 reload view 显示一致。
- 子代理过程记录不因重启丢失。

### 统计

- 子代理自己的统计与主会话统计构造逻辑相同。
- 父会话常规统计不包含子代理 token。
- 父会话统计可显示“常规 / 智能体”两组。
- 子代理回流只通过独立反馈记录携带消耗。
- 不再出现 provider 调用非零但 token 全 0 的错误统计，除非 provider 确实没有返回 usage。

### 回流

- 子代理完成后父会话出现独立反馈记录。
- 点击反馈记录可查看子代理该 turn 消耗。
- 反馈记录是用户可见显示记录，不作为隐藏用户提示词，也不作为 runtime 事实源。
- AI 可见回流消息继续直接进入父会话 runtime，但不进入显示记录。
- 显示反馈记录与 AI 可见 runtime 回流消息通过同一个智能体任务 id / turn id 关联。
- 父会话消费智能体结果时，父会话统计仍保留常规消耗与智能体消耗分离。

### fork

- fork 结果是完整 runtime state 副本。
- fork 不按轮次、不按消息裁剪。
- fork 不重建会话显示记录。
- fork 只记录 `is_fork` 和来源展示信息。
- 工具限制不影响上下文一字不差复制。
- clone / mirror 基于创建时最新 API 通讯运行时状态复制，不从显示记录反推。
- 主会话已压缩则复制压缩后的实际运行时状态，未压缩则复制未压缩的 API 通讯记录。

### 停止

- 主会话停止不天然级联停止智能体。
- 点击停止主会话且存在正在工作的智能体时，必须提供“仅停止主会话 / 同时停止智能体 / 取消停止”三种选择。
- 待命智能体不应因主会话停止自动关闭。

## 风险点

1. 当前已有测试把子代理 usage 合并进父会话统计当作正确行为，后续需要改测试语义。
2. 当前 UI 可能依赖 task feedback / status projection 展示子代理，需要改为读取同构会话记录。
3. 当前 `SessionTurnStats` 是合计模型，直接改会影响 tooltip、摘要、目标统计、历史记录兼容。
4. fork API 改语义会影响现有“从某条消息分叉”的入口，需要单独迁移或重命名。
5. 不考虑旧子代理记录结构兼容，旧会话可以删除或按坏文件修复策略处理，新结构必须按干净设计写入。

## 推荐实施顺序

1. 修复提示词组合：取消子代理完整 prompt 二次注入，改为结构化 prompt 快照。
2. 固化子代理模式语义：default/custom/agent/mirror 明确边界。
3. 抽出共享会话 turn 管线，让子代理和主会话共用记录 / 统计 / 恢复逻辑。
4. 增加子代理提示词快照记录。
5. 改回流为独立反馈记录，移除父会话 pending usage 合并。
6. 扩展 `SessionTurnStats` 支持常规 / 智能体分组。
7. 拆分 fork 与按消息回退。
8. 增加 JSONL 坏文件修复和防重复 seq 重建测试。

## 当前结论

后续修复不应继续在现有 task feedback / pending usage / projection cache 上缝补。正确方向是让子代理成为完整会话，只是：

- 写入子会话目录。
- 创建时固定父提示词快照。
- 额外携带子代理提示词记录。
- 禁止子代理创建子代理。

除此之外，子代理应与主会话完全同构。

## 2026-07-07 阶段性落实记录

本轮针对前述风险补充确认与修复：

- `todolist` 的真实工具 ID 已确认是 `todolist`，并已纳入智能体硬禁用工具清单。default / custom / agent 模式隐藏该工具，mirror / clone 语义下保留工具形状但调用返回固定拒绝。
- fork 链路已去除 `beforeMessageId` 语义，后端请求结构显式拒绝未知的 `beforeMessageId` 字段，服务端不再 fork 后调用 rollback。
- 前端 fork 动作已从历史最终回答的“从此处分叉”改为只在最新最终回答上执行“复制当前完整会话”。
- 智能体反馈统计仍作为父会话显示记录存在，但目标预算统计入口已显式忽略 `SubagentFeedback`，避免智能体 token 被计入主会话目标预算。
- Provider 请求链路经追踪确认只读取 canonical `messages`，智能体反馈显示记录位于 `records`，不会成为模型可见上下文。

继续复核后的结论：

- `TaskFeedback` 当前是完成 / 停止时的节点事件类型，不再承载完整智能体详情。
- `replace_webui_subagent_task_conversation_state` 替换的是智能体自身 `task_id` 对应的 runtime/display state，不从父级 TaskFeedback 伪造详情。
- 智能体详情恢复主要来自 TaskSession 持久化上下文、运行态 conversation_state、以及 UI detail cache 的镜像。

仍需继续复测：

- 智能体目录复制出来后是否能完全按主会话方式恢复，需要更接近真实数据的端到端持久化测试。
- 真实 UI 复测仍需要覆盖重启后过程记录、右侧智能体统计、父会话常规统计与智能体统计分离。

## 2026-07-07 TaskSession runtime 同步补充

本轮继续沿“智能体会话与主会话同构，但 AI runtime 与显示记录分离”的规则补齐持久化：

- TaskSession 完成或运行快照写入 `state.json` 时，现在会同步写入同目录的 Provider-facing runtime 日志三件套：`runtime/r001.jsonl`、`runtime_metadata.json`、`runtime.index.json`。
- 同步逻辑复用 workspace-store 既有主会话 runtime 同步器，不新增子代理专用文件格式，坏行修复、prefix 不匹配重建、runtime index 重建继续走同一套底层机制。
- `SegmentedTaskSessionStore` 新增公开目录解析入口，agent-runtime 不再自行拼 `task/<id>` 路径。
- 经过验证，runtime 日志读回时只恢复 Provider 消息窗口，不恢复 `TurnStats` 等显示记录。这个行为是正确的：显示统计保存在 TaskSession context 的 `records` 中，不能混入模型可见 runtime。
- 子代理重启后过程记录 / 输出 / 统计的 UI 恢复路径仍来自 TaskSession context 的 `conversation_state.records`；runtime 日志负责 API 通讯运行态，不承担显示记录来源。

已补充测试：

- `store_task_outcome_persists_segmented_task_session_artifacts`：验证 TaskSession context 有统计记录，同时任务目录存在 runtime 三件套，runtime 只包含 Provider 消息、不包含显示统计。
- `store_task_outcome_merges_explicit_usage_without_losing_record_calls`：验证显式 usage 能补齐 token，同时不重复放大 Provider 调用次数。
- `hydration_preloads_unclosed_subagent_surface_from_segmented_store`：验证重启预加载能恢复智能体过程、输出和统计，隐藏 summary 不会显示。

本轮验证命令：

- `cargo fmt`
- `cargo test -p vulcan-agent-runtime store_task_outcome_persists_segmented_task_session_artifacts -- --nocapture`
- `cargo test -p vulcan-agent-runtime store_task_outcome_merges_explicit_usage_without_losing_record_calls -- --nocapture`
- `cargo test -p vulcan-agent-runtime persistence_store -- --nocapture`
- `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`
- `cargo test -p vulcan-agent-runtime tool_args -- --nocapture`
- `cargo test -p vulcan-service-kernel hydration_preloads_unclosed_subagent_surface_from_segmented_store -- --nocapture`
- `cargo test -p vulcan-service-kernel subagent_navigation_tests -- --nocapture`
- `cargo test -p vulcan-workspace-store runtime_sync -- --nocapture`
- `cargo test -p vulcan-workspace-store runtime_rebuild -- --nocapture`

## 2026-07-07 TaskSession 主会话兼容目录补充

本轮继续处理“子代理目录复制出来后应能按主会话方式读取”的结构问题：

- 子任务目录内的 `metadata.json` 现在写入主会话兼容元数据，包含 `session_id`、`project_tag`、`active_segment`、`next_global_seq`、`segment_policy`、`segments` 等主会话加载必需字段。
- 子任务专属元数据迁移到 `task_metadata.json`，用于保存 `TaskSessionMetadata`、智能体配置、父子关系、公开任务名等任务索引信息。
- 子任务目录内的 `state.json` 改为主会话兼容状态形状，同时额外保留 `task_context` 和 `runtime_checkpoint`，避免主会话运行态字段与任务运行态字段互相挤压。
- 保存子任务 context 时同步填充主会话兼容 `runtime_state` 与 `current_context`，让 provider/model、conversation_state、compaction anchor 和预算指纹能通过主会话 state loader 恢复。
- 分段序号只以主会话兼容 `metadata.json` 的 `next_global_seq` 为唯一来源，`task_metadata.json` 只是镜像分段摘要，避免两个 metadata 同时维护 seq 导致重复行号。
- TaskSession 的显示同步现在复用 workspace-store 的主会话显示记录写入器；Provider-facing runtime 同步仍只写 `messages`，不写显示统计。
- workspace-store 现在暴露按目录读取 display replay 的公共入口，TaskSession 目录可以用主会话显示重放链路读出正文与统计。
- 子任务私有事件仍保留完整 payload，但 JSONL 外层 `type` 改为主会话存储可识别的 `display_event` / `user_prompt` / `assistant_output` / `subagent_task_event_linked`，防止显示重写或坏文件修复把未知事件丢弃。
- 服务层会话水合遇到缺少 `metadata.json` / `state.json` / 初始分段这类不完整目录时跳过该子代理恢复，不再让一个坏子目录拖垮整个父会话。

这轮确认的关键问题：

- 之前 `TaskFeedback` 外层直接写成任务私有事件类型，workspace-store 的主会话显示重写器不认识该类型；一旦同步显示记录，任务反馈会被当作未知行跳过，造成 `load_events` 读不到反馈记录。
- 旧结构让 `metadata.json` 承载任务专属元数据，不符合“子代理目录可作为主会话读取”的要求。
- 如果 `metadata.json` 与 `task_metadata.json` 各自维护 seq，后续显示记录和任务事件追加会天然存在重复 seq 风险，因此必须只有一个序号源。

已补充验证：

- `cargo fmt --check`
- `cargo check -p vulcan-agent-runtime -p vulcan-task-session -p vulcan-workspace-store -p vulcan-service-kernel`
- `cargo test -p vulcan-task-session -- --nocapture`
- `cargo test -p vulcan-agent-runtime store_task_outcome_persists_segmented_task_session_artifacts -- --nocapture`
- `cargo test -p vulcan-agent-runtime persistence_store -- --nocapture`
- `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`
- `cargo test -p vulcan-agent-runtime tool_args -- --nocapture`
- `cargo test -p vulcan-agent-runtime registry_mailbox -- --nocapture`
- `cargo test -p vulcan-agent-runtime runtime_flow -- --nocapture`
- `cargo test -p vulcan-workspace-store runtime_sync -- --nocapture`
- `cargo test -p vulcan-workspace-store runtime_rebuild -- --nocapture`
- `cargo test -p vulcan-service-kernel detached_subagent_tests -- --nocapture`
- `cargo test -p vulcan-service-kernel stop_task_mirror_tests -- --nocapture`
- `cargo test -p vulcan-service-kernel mailbox_tests -- --nocapture`
- `cargo test -p vulcan-service-kernel subagent_navigation_tests -- --nocapture`
- `cargo test -p vulcan-service-kernel session_hydration -- --nocapture`
- `cargo test -p vulcan-service-kernel run_state -- --nocapture`

## 2026-07-07 hook / 工具 / 统计聚合复核补充

本轮继续按用户补充要求复核“智能体与主会话同构但副作用隔离”的剩余链路，确认结果如下：

- `todolist` 已在统一智能体硬禁用工具集合中，与 `ask`、`submit_plan`、智能体调度工具、VMM 记忆工具同级处理。
- default / custom / agent 智能体会隐藏硬禁用工具；mirror / clone 保留父工具形状但调用时返回固定拒绝，并且仍会产生 ToolStarted / ToolFinished 记录。
- `TaskSessionConfigResolver` 会在所有 profile / invocation 覆盖之后强制关闭 `hooks_enabled`、`vmm_enabled`、`memory_recall_enabled`、`memory_writeback_enabled`、`database_projection_enabled`，手写开启也不能绕过。
- TaskSession 终态报告走 `apply_task_session_execution_report_to_memory` 快路径，返回 `completion_request: None`，不会直接触发完成回合 hook、VMM 写回或 postaction。
- 父会话在等待智能体时的完成 hook 只由父会话 completion gate 闭合，使用父会话 `conversation_state`，不把子代理自身终态当主会话终态处理。
- 主会话停止流程已经支持运行中智能体三选一：先返回 `needs_confirmation`，再由前端选择“仅停止主会话”或“同时停止智能体”；取消则不修改运行态。
- 智能体反馈显示记录仍只追加到父会话 `records`，AI 可见回流继续通过 mailbox `AgentMessage` 进入 provider-facing `messages`，两者通过 task id 关联但不混用。

本轮实际优化：

- 抽出 `merge_usage_segment_into_turn_stats`，统一主会话统计、智能体反馈统计、TaskSession 自身统计的 usage segment 聚合逻辑。
- 删除 `subagent_event_ingest` 内部重复的智能体反馈统计加法。
- TaskSession 自身统计补齐 token 时也复用共享聚合函数，仅在合并前清零显式分段的 provider/tool 次数以保留原本“不重复放大调用次数”的语义。

本轮验证：

- `cargo fmt --check`
- `cargo test -p vulcan-agent-runtime turn_stats -- --nocapture`
- `cargo test -p vulcan-agent-runtime persistence_store -- --nocapture`
- `cargo test -p vulcan-service-kernel detached_subagent_tests -- --nocapture`

继续保留的验收边界：

- 真实 UI 仍需要复测重启后右侧智能体详情、父会话反馈记录、统计 tooltip 与过程记录是否完全一致。
- 子任务目录复制成普通会话后的完整载入仍需要后续更接近真实目录的端到端测试。

## 2026-07-07 todolist / 智能体工具策略事实源收口

用户补充确认 `todolist` 也属于智能体不可用范围后，本轮继续复核发现：运行时已经会隐藏或拒绝 `todolist`，但硬禁用工具清单仍定义在 `runtime_launch.rs` 局部，准备阶段测试也手写了一份期望列表；同时 WebUI 智能体配置页的工具策略表直接从工作台工具快照列出原生工具，没有从后端拿“智能体硬禁用工具”字段。

这会造成两个问题：

- 后续新增 ask / plan / todolist 同类规则时，运行时、测试、前端候选表容易再次分叉。
- `todolist` 在运行时会被禁用，但配置界面仍可能把它展示为可选工具，造成用户以为智能体可以开启该工具。

本轮修复：

- 在 `vulcan-session-core` 新增共享智能体工具策略事实源 `child_agent_forbidden_tool_ids`。
- 运行时准备阶段 `apply_child_tool_policy` 不再维护局部数组，default / custom / agent 统一隐藏共享清单，mirror / clone 统一保留工具形状但返回固定拒绝。
- 准备阶段测试的期望列表改为读取共享事实源，只保留 `todolist` 专项断言防止回归。
- `SubagentCatalogSummary` 增加 `child_agent_forbidden_tool_ids`，由后端 catalog 投影从共享事实源填充。
- WebUI 智能体工具策略候选表从 catalog summary 读取禁用集合，并过滤原生工具、LuaSkills、MCP 以及“已配置但当前未解析到”的候选项。
- 浏览器 fallback 不手写禁用清单，避免前端产生第二份规则；真实生产路径以服务端投影为准。

本轮没有在保存时直接删除历史手写的禁用工具项。当时原因是通用工具策略里 `allow_only` 空列表的底层语义尚未固化，贸然把“只允许一个禁用工具”的配置删成空列表可能把语义从“无可用工具”变成“未启用 allow 策略”。后续已通过 `write_user_definition_preserves_empty_allow_only_tool_policy` 固定空 allow-only 读写语义；执行侧也始终通过共享硬禁用清单最终裁决，因此手写允许 `todolist` 不会让智能体实际获得该工具。

已验证：

- `cargo fmt --check`
- `cargo test -p vulcan-session-core child_agent_forbidden_tools_include_todolist -- --nocapture`
- `cargo test -p vulcan-agent-runtime child_forbidden_tool_ids_include_todolist -- --nocapture`
- `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`
- `cargo check -p vulcan-service-api -p vulcan-service-kernel`
- `pnpm lint`（`apps/vulcan-code-web`）

继续保留的验收边界：

- 真实 UI 需要确认智能体配置工具策略表不再出现 `todolist`、`ask`、`submit_plan`、智能体调度工具和 VMM 记忆工具。
- `allow_only` 空列表语义已在后续复核中固定为“无普通工具”；若后续要自动清理历史手写策略，仍必须走后端统一策略清洗，不能靠前端删除绕过。

## 2026-07-07 固定运行时快照与提示词膨胀修复补充

本轮继续按“智能体与主会话同构，差异只通过额外提示词和工具策略表达”的规则追踪提示词链路，确认并修复一个会导致子代理 token 异常放大的问题。

确认的问题：

- 子代理每轮执行前会重新解析 TaskSession 配置。旧逻辑优先使用当前父会话 `base_controller_config`，没有把已存储的 `request.controller_config` 当作固定运行时快照。
- 子代理终态保存时曾从 `RuntimeStreamLaunch.system_preamble` 反推 `CoreControllerConfig`。`system_preamble` 已经是完整组合结果，包含基础提示词、子代理说明、工具说明等内容，不能作为下一轮基础提示词。
- 这会造成续跑或重启后把“已经组合过的完整子代理提示词”再次输入共享提示词组装器，形成提示词膨胀。用户观察到的“父会话两万多输入，子代理三万多输入”与该链路高度吻合。
- 准备请求没有显式传递固定父提示词快照时，子代理 prompt 组合可能回落到当前活动 agent 提示词，违背“创建时冻结”的规则。

修复后的规则：

- 子代理的 `controller_config` 是创建/续跑时固定的父级运行时快照，只保存基础运行时状态。
- 子代理身份、委托任务、硬禁用工具说明、专家/自定义功能提示词放在 `ExecutionSubagentPromptContext`，由主会话共享提示词组装器统一处理。
- outcome 落盘保存固定 controller 快照，不再保存完整 `system_preamble`。
- `build_task_controller_config` 遇到已存储 controller 快照时直接复用，后续不再用当前父会话重建。

本轮验证：

- `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`
- `cargo test -p vulcan-agent-runtime persistence_store -- --nocapture`
- `cargo test -p vulcan-agent-runtime runtime_flow -- --nocapture`
- `cargo test -p vulcan-agent-runtime provider_runtime_context -- --nocapture`
- `cargo check -p vulcan-agent-runtime -p vulcan-service-api -p vulcan-service-kernel`
- `pnpm lint`（`apps/vulcan-code-web`）
- `cargo fmt --check`
- `git diff --check`
- `cargo test -p vulcan-session-core child_agent_forbidden_tools_include_todolist -- --nocapture`

后续真实 UI 重点：

- 新建默认智能体与克隆智能体，分别确认首次详情区实时出现过程记录。
- 重启后确认过程记录、最终输出和统计仍然存在。
- 对比父会话与子代理首轮输入 token，确认不再出现“短任务子代理首轮比父级当前上下文明显更大”的异常膨胀。

## 2026-07-07 用户可见命名与 todolist 禁用边界补充

已确认：

1. `todolist` 属于智能体不可调用工具，已经进入共享硬禁用工具事实源。
2. default / custom / agent 智能体不注册 `todolist`，mirror / clone 保留工具形状但执行时返回固定拒绝。
3. WebUI 智能体配置候选从后端投影的共享禁用清单过滤，不再自行维护第二份规则。
4. 用户可见翻译值统一使用“智能体 / Agent”，避免继续在界面层使用“子代理 / Subagent”造成测试描述混乱。

本轮刻意没有修改：

1. 内部类型名、i18n key、协议字段中的 `subagent` 标识。
2. 历史配置中已经保存的工具策略内容。
3. 智能体运行时、存储或提示词组装链路。

原因：

1. 内部标识不是用户可见文案，直接大规模改名会扩大协议迁移面。
2. 历史 `allow_only` 空列表语义已在后续复核中固定为“无普通工具”，清理配置候选时仍不能改写该工具策略含义。
3. 当前执行侧已经由共享禁用清单最终裁决，手写允许 `todolist` 不会让智能体实际获得该工具。

## 2026-07-07 能力策略归一化事实源补充

继续审计旧链路时确认：智能体能力策略同时存在统一 `policy.mode + policy.selectors` 与旧镜像列表 `allowed_tools / disabled_tools`、`enabled / disabled`。运行时工具曝光已经优先读取统一 policy，但 catalog、保存入口、spawn 参数解析和 resolver 仍分别维护本地转换逻辑。

真实风险：

1. 新保存格式只要某处没有把 policy 同步回旧列表，UI 计数、编辑回填和运行时实际曝光可能不一致。
2. resolver 之前会把 policy 和旧列表分开覆盖，存在“下层 policy + 上层旧列表”混合的可能。
3. 后续新增 `todolist` 这类硬禁用规则时，如果仍依赖多处本地转换，很容易再次出现配置界面与执行侧不一致。

已完成收敛：

1. `vulcan-session-core` 新增共享能力策略归一化与应用函数。
2. catalog、服务保存入口、spawn 参数解析删除重复 selector 归一化 helper，统一调用共享函数。
3. resolver 改为一次性应用规范化 policy，并同步旧镜像列表。
4. WebUI catalog 投影中的列表、数量和能力诊断改为从统一 policy 派生。

仍保留：

1. API / 存储结构中的旧镜像字段暂未删除。
2. sidecar 旧三态 map 仍可解析为统一 policy。

保留原因：

1. 这些字段仍是现有 API 类型的一部分，直接删除属于协议迁移，需要更大范围调用方调整。
2. 本轮目标是消灭重复转换和事实源漂移，先让旧字段成为派生镜像，而不是继续当主事实。

## 2026-07-07 智能体消息回流显示记录事实源收口

本轮继续追踪父会话通过 `send_message` 投递给智能体后的显示链路，确认存在一条旧式文本反推路径：

1. 智能体 inbox 消费时会把父级消息组装为 `[Inter-agent messages]` 内部提示词。
2. 该提示词作为 Provider 可见用户回合进入智能体 runtime。
3. WebUI 显示层曾解析这段内部文本，再反推出一条“智能体消息”显示记录。

这条路径的问题不是单纯 UI 文案，而是事实源错位：显示记录依赖 Provider 输入文本格式。只要文本格式变化、流式插入时机变化或重启恢复来源变化，就可能再次出现重复显示、丢失显示、运行态和重启态不一致。

本轮修复后的规则：

1. Provider 可见回流消息仍按当前语义进入 runtime，但不再作为显示记录来源。
2. 智能体收到父级消息时，同时写入一条独立 `DisplayNote` 记录。
3. 这条显示记录只进入 `records`，不进入 Provider 可见 `messages`。
4. 同一条投递生成稳定 `delivery_id`，队列、host payload、实时显示镜像、controller 记录共享该 ID。
5. 运行中智能体通过 controller 追加显示记录，空闲后续跑智能体通过恢复 snapshot 追加显示记录。
6. WebUI 实时镜像优先使用 `delivery_id` 去重，最终显示以 canonical runtime records 为事实源。
7. `[Inter-agent messages]` 在显示层只作为内部注入文本被过滤，不再被解析成用户可见消息。

关键落点：

- `vulcan-core` 新增 `append_display_record`，用于追加仅显示记录，不改变 Provider 可见历史。
- `QueuedInterAgentMessage` 增加 `delivery_id`。
- `agent_control.rs` 新增运行中 / 空闲续跑两条显示记录写入路径。
- `conversation_display.rs` 删除 `[Inter-agent messages]` 文本解析投影。
- `replace_display_messages_with_runtime_state_records` 不再把旧 `session_messages` 中的 display-only 智能体消息补回去。

仍保留的边界：

1. Provider 可见智能体 inbox 格式暂仍是 `[Inter-agent messages]` 文本。
2. 本轮目标是断开“显示记录从 runtime 文本反推”的错误链路，不同时扩大为 typed inbox 协议迁移。
3. 后续如果继续同构，可考虑把智能体 inbox 也升级为类型化内部消息，但那需要同步调整 idle resume、运行中 intervention、持久化和 provider request 组装。

### 2026-07-07 智能体 inbox 类型化消息补充

本轮继续完成上一阶段留下的 inbox 协议差异：父会话发给智能体的 `send_message` 不再组装为 `[Inter-agent messages]` 普通用户文本，而是进入与主会话 mailbox 相同的 `AgentMessage` 结构。

已确认事实：

1. `ConversationPayload::AgentMessage` 已经是 core/runtime/provider 投影共同识别的类型化代理消息。
2. 主会话接收智能体回流时已经通过 `append_agent_message` 写入该结构。
3. 旧父到子 inbox 仍残留两条文本注入链路：空闲续跑的 `resume_idle_agent_from_inbox` 和运行中安全边界的 `build_child_agent_prompt_intervention`。
4. `todolist` 是当前唯一真实工具 id，没有 `todo_list` 或 `todo list` 注册别名；共享硬禁用清单已经包含 `todolist`。

已完成修复：

1. `QueuedInterAgentMessage` 保留稳定 `delivery_id`，并新增 `target_agent_id` 用于构建模型可见 `task_name` 与 `recipient`。
2. 空闲续跑在恢复 snapshot 中追加 `ConversationPayload::AgentMessage`，再追加独立 `DisplayNote`。
3. 运行中安全边界直接调用 `append_agent_message`，不再调用 `append_interjected_user_turn`。
4. `should_append_user_turn` 支持识别尾部连续 `AgentMessage` 的渲染文本，避免空闲续跑 prompt 被重复追加成 `UserText`。
5. 服务层删除 `[Inter-agent messages]` 兼容隐藏分支，内部消息隐藏只认规范 `<agent_message>`。

当前结论：

1. 父到子消息现在有两个明确来源：Provider 可见的 typed `AgentMessage`，用户可见的独立 `DisplayNote`。
2. 二者共享 `delivery_id`，但统计和显示不再依赖普通用户 prompt 文本。
3. 这一阶段仍未处理所有统计展示问题；父会话回流统计分行、智能体独立 per-turn 统计和智能体归档策略仍是后续阶段。

### 2026-07-07 智能体实时统计为空的原因与修复

本轮继续追踪用户反馈的“运行时智能体面板统计全为 0，重启后才显示 token”问题，确认不是 Provider 没返回 usage，而是实时显示与持久化恢复用了两条不同统计链路：

1. 智能体完成后，父会话回流和持久化 outcome 会从 `ExecutionRunReport.last_successful_provider_usage` 构造 `usage_segments`，所以重启恢复或父级反馈记录能看到 token。
2. 智能体实时面板自身的 `TurnStats` 走 `apply_terminal_execution_report_to_state -> build_session_turn_stats_from_state_with_segments`，该统计器只读取规范 `Assistant` 消息上的 `usage`。
3. 某些终态报告路径没有把 `last_successful_provider_usage` 写回终态 `Assistant` 协议消息，导致实时面板能看到 provider 调用次数，但 token 字段全是 0。
4. 另一个边界是 TaskSession 实时内存路径可能只有终态 assistant 消息，没有可见用户起始记录；旧统计器在这种情况下把规范消息起点设为 `messages.len()`，等于主动忽略了刚追加的 assistant usage。

已完成修复：

1. 终态报告收口时，如果终态 assistant 协议消息已存在但 `usage` 为空，则用 `last_successful_provider_usage` 补齐。
2. 如果终态 assistant 协议消息不存在，则追加消息时直接携带同一份 usage。
3. 统计器仍优先按可见用户记录对齐；只有完全没有可见用户起始记录时，才从规范消息里寻找最近的 `UserText` 或触发型 `AgentMessage` 作为当前轮次起点。
4. 如果也没有这类起点，则只统计最后一条承载 provider work 的 assistant 消息，避免把历史消息整段继承进来。
5. 父会话智能体回流统计仍保持独立 `SubagentFeedback` 记录，不写入父会话普通回合的 provider usage。

验证覆盖：

1. `terminal_report_attaches_usage_to_appended_assistant_message`：终态消息不存在时，追加的 assistant 消息和 `TurnStats` 都带 token。
2. `terminal_report_attaches_usage_to_existing_assistant_message`：终态消息已存在但 usage 为空时，不重复追加消息，只补 usage。
3. `current_turn_stats_use_terminal_assistant_when_no_user_record_exists`：只有 assistant 终态消息时也能构建实时统计。
4. `current_turn_stats_use_triggering_agent_message_when_no_user_record_exists`：智能体回流触发的轮次不会把更早用户回合统计进来。
5. `task_session_execution_report_fast_path_projects_live_usage_stats`：WebUI TaskSession 内存快路径立即投影非零 token，不依赖重启恢复。

`todolist` 规则补充：

1. 用户说的 todo list 对应真实工具 id 是 `todolist`。
2. `todolist` 已在共享智能体硬禁用清单内。
3. default / custom / agent 模式不注册该工具；clone / mirror 为保持工具形状可以注册但调用会返回智能体禁用提示。

## 2026-07-07 提示词固定快照类型化补充

本轮继续处理智能体首轮提示词膨胀和提示词组装边界问题。已确认运行时代码中仍残留 `agent_system_prompt_override: Option<String>` 裸字符串字段，虽然当前没有再把完整 system preamble 塞回去，但这个字段名和类型会给后续修改留下错误入口。

修复后的事实：

1. 运行时代码已删除 `agent_system_prompt_override` 字段。
2. `ExecutionPreparationRequest` 与 `ExecutionPreparation` 现在只携带 `agent_prompt_snapshot`。
3. `agent_prompt_snapshot` 的类型为 `ExecutionAgentPromptSnapshot`，并带有 `ExecutionAgentPromptSnapshotSource`。
4. 当前智能体执行只能通过 `ExecutionAgentPromptSnapshot::fixed_parent_runtime` 创建固定父级运行时提示词快照。
5. prompt composer 通过 `build_request_agent_prompt_snapshot` 消费该结构化快照，不再读取裸字符串 override。
6. TaskSession 准备请求测试已断言智能体携带的快照来源是 `FixedParentRuntime`。
7. `todolist` 继续由共享智能体硬禁用工具事实源裁决，runtime 与 session-core 测试均覆盖。

设计意义：

1. 父级基础提示词继承仍保留，但它被表达为“agent prompt layer 快照”，而不是“已经组装好的 system preamble”。
2. 后续如果有人试图重新把完整提示词字符串塞进智能体请求，必须先绕过结构化类型和测试，风险会更早暴露。
3. 这一步只收口提示词快照入口，不改变当前工具策略、统计记录或 UI 展示路径。

验证覆盖：

1. `rg -n "agent_system_prompt_override" crates/vulcan-agent-runtime/src -g "*.rs"`：无结果。
2. `runtime_prompt_layers_use_request_scoped_agent_prompt_snapshot`：结构化快照进入共享提示词组合器。
3. `task_runtime_preparation_request_carries_subagent_prompt_context`：智能体请求携带固定父级运行时提示词快照和结构化智能体上下文。
4. `preparation_config`：18 个准备阶段测试通过。
5. `child_agent_forbidden_tools_include_todolist`：共享工具禁用事实源包含 `todolist`。

## 2026-07-07 智能体统计链路分叉补充

用户继续复测后确认：之前只修补终态 assistant usage 或父级外层 payload，不足以覆盖所有界面。实际存在三条统计链路，必须分别对齐：

1. 智能体右侧独立面板实时显示：来自 `DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::Finished(report))`，服务层进入 `apply_task_session_execution_report_to_memory_in_place`，最终复用通用 terminal report 统计器。
2. 父会话可见的智能体反馈记录：来自 `DelegateFinished` 事件的 metadata，WebUI feedback 记录读取 `usage_segments`。
3. 重启后的恢复显示：来自 TaskRuntime 已持久化的 `TaskExecutionOutcome.usage_segments` 和历史 transcript 重建。

这解释了“运行时全是 0，重启后又有 token”的现象：Provider 实际返回了 usage，持久化 outcome 也保存了 usage，但右侧实时面板走的通用 terminal report 统计器没有把 report 层的 `last_successful_provider_usage` 作为权威兜底来源。此前修改只影响一部分链路，所以用户看到的界面仍然没有变化。

本轮补齐点：

1. 通用 terminal report 统计器在规范 assistant 消息没有 usage、但 report 带有 `last_successful_provider_usage` 时，会合成一个 `AssistantResponse` usage segment。
2. 合成前会移除同一轮中“只有 provider 调用次数、没有任何 token”的空 assistant segment，避免显示两个 assistant segment。
3. 如果统计器已经从消息中算出 provider 调用次数，fallback usage segment 不再重复增加 provider 调用次数，只补 token。
4. `build_task_host_payload` 同时把 `usage_segments` 写入外层 payload 和内层 `subagent_mailbox_event.payload`，避免不同消费者读不同位置时丢失统计。
5. `todolist` 已确认属于智能体不可调用工具；它继续由共享硬禁用事实源统一裁决，不在各模式里重复散落配置。

当前边界：

1. 本轮修复的是现有三条统计链路的数据同步，不改变“智能体回流给 AI 的消息只进入 runtime、不进入显示记录”的策略。
2. 父会话普通统计仍不应该被智能体消耗污染；智能体消耗应通过独立反馈记录和最终统计块分离显示。
3. 智能体自身会话仍应按主会话同构记录，只是目录不同、禁止创建智能体、禁止 ask/plan/todolist 等交互工具。

验证覆盖：

1. `terminal_report_backfills_usage_when_checkpoint_messages_lack_usage`：规范 assistant 消息没有 usage 时，实时统计能从 report usage 补回 token。
2. `task_invocation_result_omits_embedded_conversation_state`：父级 host payload 外层和 nested mailbox payload 都包含同一份 `usage_segments`。
3. `task_session_execution_report_fast_path_projects_live_usage_stats`：服务层内存快路径能投影非零 token。
4. `preparation_config`、`todolist`、`session_event_runtime::tests`、`detached_subagent_tests`、`mailbox_tests` 与跨 crate `cargo check` 均通过。

## 2026-07-07 统计用量分组一等化补充

本轮继续处理“父会话常规统计与智能体消耗分离”这一底层模型问题。此前已经避免智能体用量进入父会话普通 turn stats，但共享统计模型仍只提供 `usage_segments` 明细，调用方需要自己理解 `AssistantResponse`、`SubagentTask`、`ContextCompaction` 分别代表什么。这仍然容易导致 UI、目标统计、反馈记录各自解释 segment。

本轮调整后的事实：

1. `SessionTurnStats` 新增一等 `usage_groups`。
2. `usage_groups` 当前包含三类稳定分组：
   - `regular`：主助手常规响应。
   - `subagent`：智能体反馈消耗。
   - `context_compaction`：上下文压缩消耗。
3. 共享 `merge_usage_segment_into_turn_stats` 会在累加总计的同时维护分组。
4. assistant response 这种原本在统计器里手动累计的常规消耗，也会写入 `regular` 分组。
5. WebUI 摘要优先展示后端提供的 `usage_groups`，旧记录没有分组时才回退到旧总计。
6. 智能体反馈记录现在能携带明确的 `subagent` 分组，父会话普通 turn stats 仍不包含智能体 token。

这一步不是在 UI 上临时拼两行文案，而是把“常规 / 智能体 / 压缩”变成共享统计模型事实。后续最终统计块、tooltip、预算统计或导出功能都应该读取同一份分组字段。

验证覆盖：

1. `vulcan-workbench-model turn_stats`：验证 `usage_groups` 可序列化，并能按 owner 聚合 regular / subagent。
2. `vulcan-agent-runtime turn_stats`：验证 assistant + context compaction 产生 regular / context_compaction 两组，智能体 segment 产生 subagent 组。
3. `subagent_completion_appends_independent_parent_feedback_rows`：验证父会话可见智能体反馈记录的 turn stats 含 `subagent` 分组。
4. `mailbox_intervention_keeps_subagent_usage_out_of_parent_turn_stats`：验证父会话 mailbox 消费智能体结果时，父普通 turn stats 仍没有智能体 token。
5. `pnpm --prefix apps/vulcan-code-web lint`：验证前端分组摘要读取通过类型检查。

## 2026-07-07 TaskSession 目录主会话读取验证补充

本轮继续落实“智能体目录复制出来也应等同主会话载入”的底层约束。源码追踪确认，当前不需要新增子代理专用读取链：

1. `SegmentedTaskSessionStore` 创建智能体目录时会写 `metadata.json`、`task_metadata.json`、`state.json` 和分段 JSONL。
2. `metadata.json` 使用主会话兼容字段：`session_id`、`project_id`、`active_segment`、`task_dir`、`segments`、`tasks`。
3. `state.json` 使用主会话兼容字段：`session_id`、`runtime_state`、`current_context`、`session_boundary_state`。
4. `workspace_store::read_segmented_session_runtime_conversation_state` 与 `read_segmented_session_display_conversation_state` 都是目录级通用读取函数，注释明确允许主会话目录或 TaskSession 目录。
5. `workspace_store::load_segmented_session_task_mirrors_from_directory` 读取的是 metadata `tasks`，因此嵌套智能体镜像也不应有专用读取结构。

本轮没有修改运行时逻辑，只补强端到端防回退测试：

1. `store_task_outcome_persists_segmented_task_session_artifacts` 现在直接读取智能体目录的 `metadata.json`，验证其主会话兼容字段。
2. 同一测试直接读取智能体目录的 `state.json`，验证 `runtime_state` 和 `current_context.conversation_state` 已按主会话状态文档写入。
3. 同一测试通过 `load_segmented_session_task_mirrors_from_directory` 读取智能体目录，确认主会话任务镜像读取器能接受 TaskSession metadata。
4. 同一测试继续通过 `read_segmented_session_runtime_conversation_state` 和 `read_segmented_session_display_conversation_state` 读取 runtime/display，确认不依赖 WebUI 智能体缓存。
5. `load_segmented_session_fork_source_from_directory` 也能读取 TaskSession metadata 并返回空 fork source，说明 fork-source 兼容字段没有破坏主会话读取器。

验证结果：

1. `cargo fmt -p vulcan-agent-runtime`：通过。
2. `cargo test -p vulcan-agent-runtime store_task_outcome_persists_segmented_task_session_artifacts -- --nocapture`：通过。
3. `cargo test -p vulcan-agent-runtime persistence_store -- --nocapture`：16 个测试通过。
4. `git diff --check -- crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs docs/plan/20260706-01-SUBAGENT_SESSION_PARITY_REARCHITECTURE.md`：通过，仅有既有 LF/CRLF 提示。

当前结论：

1. 智能体目录层已具备主会话式读取能力。
2. 这次补的是回归约束，不是缝补 WebUI 投影。
3. 后续仍需继续处理自动压缩、工具禁用统一事实源的全链路审计、停止主会话时询问是否同时停止工作中智能体，以及真实界面复测。
4. `todolist` 已按用户补充纳入智能体不可调用工具范围，后续任何统一工具策略改造都必须保留这一点。

## 2026-07-07 智能体禁用工具统一事实源审计补充

用户补充：`todo list` 也属于智能体不可使用范围。源码追踪后的事实如下：

1. 真实工具 id 是 `todolist`，定义在 `crates/vulcan-agent-runtime/src/tooling/native.rs`。
2. 智能体硬禁用工具清单的唯一事实源是 `crates/vulcan-session-core/src/child_agent_policy.rs` 的 `child_agent_forbidden_tool_ids()`。
3. 共享清单当前包含：
   - 智能体调度工具：`spawn_agent`、`agent_orchestration_info`、`send_message`、`wait_agent`、`interrupt_agent`、`close_agent`、`list_agents`。
   - 交互/计划工具：`ask`、`submit_plan`、`todolist`。
   - 记忆工具：`vmm_memory_search`、`vmm_turn_details`、`vmm_memory_write`、`vmm_memory_delete`。
4. `runtime_launch.rs` 的 `apply_child_tool_policy` 是智能体执行请求统一入口：
   - 普通智能体路径把共享清单加入 `tool_request_hidden_tool_ids`，最终不注册这些工具。
   - mirror 路径把共享清单加入 `tool_request_rejection_tool_ids`，保留工具形状但调用时返回策略拒绝。
5. `provider_transport_stream_executor.rs` 在真实工具执行前检查拒绝列表，因此 mirror 路径不会真的执行 `todolist`。
6. `execution_preparation.rs` 先完成普通工具曝光，再执行 `remove_request_hidden_tools`，所以智能体 request hidden list 可以覆盖 `todolist` 的通用保留规则。

本轮补充的回归护栏：

1. 新增 `request_hidden_tools_remove_universal_todolist_after_exposure`。
2. 该测试验证当 request hidden list 包含 `todolist` 时，最终 provider 可见 `tool_descriptors` 没有 `todolist`。
3. 同一测试验证最终可执行 `resolved_tool_definitions` 也没有 `todolist`。
4. 与现有 `child_forbidden_tool_ids_include_todolist`、`task_runtime_preparation_request_hides_parent_tools_by_default`、`task_runtime_preparation_request_preserves_tools_but_rejects_calls_for_mirror` 组合后，覆盖共享清单、普通智能体隐藏、mirror 智能体拒绝、最终工具快照四段链路。

验证结果：

1. `cargo fmt -p vulcan-agent-runtime`：通过。
2. `cargo test -p vulcan-session-core child_agent_forbidden_tools_include_todolist -- --nocapture`：通过。
3. `cargo test -p vulcan-agent-runtime request_hidden_tools_remove_universal_todolist_after_exposure -- --nocapture`：通过。
4. `cargo test -p vulcan-agent-runtime child_forbidden_tool_ids_include_todolist -- --nocapture`：通过。
5. `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`：20 个测试通过；锁污染测试打印的 panic 是预期捕获行为。
6. `cargo test -p vulcan-agent-runtime execution_preparation -- --nocapture`：13 个测试通过。
7. `git diff --check -- crates/vulcan-agent-runtime/src/agent_runtime/execution_preparation/tests.rs docs/plan/20260706-01-SUBAGENT_SESSION_PARITY_REARCHITECTURE.md`：通过，仅有既有 LF/CRLF 提示。

当前结论：

1. `todolist` 已按用户要求属于智能体不可调用工具范围。
2. 这条规则不是某个模式的临时配置，而是共享事实源驱动。
3. 普通主会话 `todolist` 仍保持通用可见；智能体执行请求通过 request hidden list 在最终工具快照中硬隐藏。
4. 后续新增智能体禁用工具时，只应改共享清单，不能在 UI、spawn 参数解析或各模式里各写一份。

## 2026-07-07 TaskSession 自动压缩预算策略同构补充

本轮处理“智能体需要能自动上下文压缩，并且根据它自己的模型判断”的链路问题。源码追踪后的事实是：TaskSession 不是完全没有自动压缩入口，而是入口与预算策略存在断层。

已确认的入口：

1. `prepare_task_runtime_stream_launch_with_preflight_compaction` 会在子代理启动前根据 `launch.preparation.context_budget.should_compact()` 决定是否先压缩。
2. `run_task_with_shared_runtime_context` 会在子代理完成后根据终态 report 的 `context_budget.should_compact()` 决定是否后台压缩。
3. `spawn_task_session_context_compaction` 本身也会检查 `resolved_config.effective_config.context_compaction.auto_compaction_enabled`。
4. `run_task_session_context_compaction` 已经通过 `task_context_compaction_policy_from_snapshot` 把 `auto_compaction_enabled` 映射到底层 `ContextCompactionPolicy.auto`。

真正的问题点：

1. `build_request_context_compaction_budget_snapshot` 固定传入 `ContextCompactionPolicy::default()`，所以预算快照不知道 TaskSession 解析后的开关。
2. `ProviderRequestBudgetGuard` 在真实发送前重新计算预算时，也重新使用默认策略。
3. 因此即使 TaskSession 配置层关闭了自动压缩，预算快照和发送前预算门仍可能按默认自动压缩语义判断。
4. 这属于“共享流程入口存在，但关键策略没有进入共享预算门”的问题，不是单纯 UI 显示问题。

本轮修复后的事实：

1. `ExecutionPreparationRequest` 增加 `context_compaction_policy`。
2. `ExecutionPreparation` 保留同一份策略，供运行期预算和 Provider 发送前预算门使用。
3. `build_task_runtime_preparation_request` 从 `TaskSessionResolvedConfig.effective_config.context_compaction` 生成策略并写入子代理执行请求。
4. `build_request_context_compaction_budget_snapshot` 接收调用方传入的策略；未传时仍回到默认策略，保持主会话默认行为。
5. `seed_runtime_request_snapshot`、`apply_latest_request_snapshot`、`ProviderRequestBudgetGuard` 现在都读取同一份 `ExecutionPreparation.context_compaction_policy`。

明确没有做的事：

1. 没有把 `ContextCompactionPolicySnapshot.context_budget` 猜测映射成 `input_limit` 或其他字段。
2. 没有把 `max_provider_requests` 猜测映射成 `max_turns` 或 provider request 限制。
3. 原因是当前底层 `vulcan_context::ContextCompactionPolicy` 只有 `auto/prune/tail_turns/preserve_recent_tokens/reserved`，没有这两个字段的唯一归属。
4. 后续若要让这两个配置生效，必须先扩展共享预算策略结构和计算语义，再接入 TaskSession 配置解析结果。

验证覆盖：

1. `request_budget_snapshot_uses_resolved_compaction_policy`：验证 runtime 预算门面会使用传入策略，关闭时 `compaction_auto_enabled == false`，开启时为 `true`。
2. `task_runtime_preparation_request_uses_resolved_runtime_policy`：验证 TaskSession 子请求携带解析后的压缩策略。
3. `preparation_config`、`execution_preparation`、`runtime_stream_finalize`、`runtime_stream_launch_payload`、`runtime_stream_setup`、`task_runtime` 全部通过。
4. `task_runtime` 共 104 个测试通过，锁污染测试打印的 panic 是测试主动触发并捕获的预期行为。

## 2026-07-07 智能体 hook / 记忆 / postaction 副作用隔离补充

本轮针对“智能体不触发 hook，比如记忆以及 postaction”的规则做源码追踪。结论分两层：

1. TaskSession 自身执行链路已有隔离基础。
2. 分离子代理回流链路存在一条旧父级 hook 闭合逻辑，已修掉。

已确认的隔离事实：

1. `vulcan_session_core::ExecutionRuntimePolicy::task_session()` 固定关闭：
   - `hooks_enabled`
   - `vmm_enabled`
   - `memory_recall_enabled`
   - `memory_writeback_enabled`
   - `database_projection_enabled`
2. `TaskSessionConfigResolver::resolve_runtime_policy(...)` 会先叠加 profile / invocation runtime override，再调用 `enforce_task_session_projection_boundary(...)`，因此外部配置无法重新打开这些副作用。
3. `build_task_runtime_preparation_request(...)` 会把已解析的 runtime policy 写入子代理执行请求。
4. `prepare_execution_with_tool_snapshot(...)` 按 runtime policy 决定是否保留：
   - `project_vmm`
   - `session_vmm`
   - `system_prompt_hook_blocks`
   TaskSession policy 下三者最终都会清空。
5. `apply_execution_report_to_memory(...)` 会先判断 session 是否是普通主会话；不是普通主会话时走 `apply_task_session_execution_report_to_memory(...)` 快路径，返回 `completion_request: None`，所以 TaskSession 终态报告本身不会触发 completed-turn hook。

发现的实际问题：

1. 分离子代理终态事件处理里，旧逻辑会在最后一个子代理完成后调用 `maybe_prepare_webui_parent_completion_after_subagent_finished(...)`。
2. 该函数会对父会话调用 `prepare_turn_completion_dispatch_with_status(... Completed ...)`。
3. 随后事件处理会调用 `spawn_conversation_completion_hook_task(...)`，最终可能执行 VMM completed-turn hook / PostAction。
4. 这意味着“子代理完成”会直接推动父会话后置副作用，而不是等父会话消费回流并由父会话自己的终态触发。
5. 该行为和当前规则不一致，也容易在父会话尚未处理子代理结果时过早写回记忆。

本轮修复后的事实：

1. 分离子代理终态事件仍会：
   - ingest 子代理运行时事件；
   - 投递父会话 mailbox；
   - 清理子代理完成门控；
   - 刷新智能体导航；
   - 继续父会话 pending prompt。
2. 分离子代理终态事件不再：
   - 准备父会话 completed-turn hook；
   - 调用 `spawn_conversation_completion_hook_task(...)`；
   - 消费父会话 active hook turn；
   - 直接触发 VMM PostAction。
3. 父会话收到 mailbox 后继续运行，最终 hook 仍由父会话自己的后续终态决定。
4. 删除了旧辅助函数 `maybe_prepare_webui_parent_completion_after_subagent_finished` 与 `latest_assistant_content_for_parent_completion`，避免未来误用。

新增/更新的回归护栏：

1. `prepare_execution_clears_side_effect_inputs_for_task_session_policy`：即使 TaskSession 请求误带 hook block 与 VMM 配置，执行准备结果也会清空。
2. `task_session_execution_report_fast_path_skips_completion_hook`：确认 TaskSession 终态报告快路径不产生 completion request。
3. `subagent_completion_feedback_keeps_parent_hook_turn_open`：确认子代理完成反馈不会闭合父级 active hook turn。
4. `detached_subagent_tests` 全组通过，说明回流、导航、mailbox、实时用量投影没有被这次移除打断。

额外修正：

1. 前一阶段给 `ExecutionPreparationRequest` 增加 `context_compaction_policy` 后，`git/commit_summary.rs` 的内部执行请求漏填该字段。
2. 本轮补为 `context_compaction_policy: None`，保持 Git 提交摘要内部执行的原语义。

验证结果：

1. `cargo fmt -p vulcan-service-kernel -p vulcan-agent-runtime`：通过。
2. `cargo test -p vulcan-agent-runtime execution_preparation -- --nocapture`：14 个测试通过。
3. `cargo test -p vulcan-session-core task_session_policy_disables_side_effects -- --nocapture`：通过。
4. `cargo test -p vulcan-task-session task_session_runtime_overrides_keep_side_effects_forbidden -- --nocapture`：通过。
5. `cargo test -p vulcan-service-kernel detached_subagent_tests -- --nocapture`：24 个测试通过。
6. `cargo test -p vulcan-agent-runtime task_runtime -- --nocapture`：104 个测试通过，锁污染 panic 为预期捕获行为。
7. `git diff --check -- ...`：通过，仅有既有 LF/CRLF 提示。

## 2026-07-07 TaskSession context_budget 预算门接入补充

本轮继续处理上一阶段留下的 `context_budget` 与 `max_provider_requests` 缺口。源码追踪后结论如下：

1. `ContextCompactionPolicySnapshot.context_budget` 的注释是“上下文 token 预算”，且共享预算计算里的唯一对应承载点是 `ContextBudgetCapabilities.effective_input_limit()`。
2. 真实请求预算快照、预检自动压缩判断、Provider 发送前预算守卫都通过 `build_request_context_compaction_budget_snapshot(...)` 进入同一条预算门。
3. `ContextCompactionPolicySnapshot.max_provider_requests` 的注释是“压缩前允许的最大供应商请求数量”，当前代码只有解析、持久化和 outcome 里的 `provider_turns_executed` 记录，没有“累计 provider 请求数达到阈值后触发压缩”的软门。
4. `max_turns` 是 provider run loop 的硬停止上限，直接把 `max_provider_requests` 映射到 `max_turns` 会把“该压缩了”误变成“执行停止了”，因此本轮不做该映射。

本轮修复后的事实：

1. `ExecutionPreparationRequest` 与 `ExecutionPreparation` 新增 `context_budget_override`，表示请求级输入 token 预算上限。
2. `build_task_runtime_preparation_request(...)` 从已解析的 `resolved_config.effective_config.context_compaction.context_budget` 生成 `context_budget_override`，只接受非零且可转换为 `usize` 的值。
3. `build_request_context_compaction_budget_snapshot(...)` 会把该上限合入预算能力快照，并且不会超过模型自身声明的输入 / 上下文窗口上限。
4. 预检预算、终态预算刷新、Provider 发送前 `ProviderRequestBudgetGuard` 都读取同一个 `context_budget_override`。
5. 主会话与内部摘要 / 压缩用途默认传 `None`，不改变主会话原有预算行为。

新增/更新的回归护栏：

1. `request_budget_snapshot_uses_request_context_budget_limit`：验证请求级预算能压低 `effective_input_limit`，并且配置预算大于模型窗口时仍按模型窗口封顶。
2. `task_runtime_preparation_request_uses_resolved_runtime_policy`：扩展断言，确认 TaskSession 已解析的 `context_budget` 会进入子执行请求。
3. `request_budget_snapshot_uses_resolved_compaction_policy`、`context_budget` 全组继续验证压缩策略 reserve 与 auto 开关不回退。

明确未完成：

1. `max_provider_requests` 仍未进入运行时决策。
2. 后续要实现它，应先建立 TaskSession 累计 provider 请求计数与“达到阈值后先压缩再继续”的软门，不能复用 `max_turns` 硬停止语义。

验证结果：

1. `cargo fmt -p vulcan-agent-runtime -p vulcan-service-kernel`：通过。
2. `cargo test -p vulcan-agent-runtime request_budget_snapshot_uses_request_context_budget_limit -- --nocapture`：通过。
3. `cargo test -p vulcan-agent-runtime task_runtime_preparation_request_uses_resolved_runtime_policy -- --nocapture`：通过。
4. `cargo test -p vulcan-agent-runtime request_budget_snapshot_uses_resolved_compaction_policy -- --nocapture`：通过。
5. `cargo test -p vulcan-agent-runtime context_budget -- --nocapture`：通过，2 个测试通过。
6. `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`：通过，20 个测试通过；锁污染 panic 是测试主动触发并捕获的预期行为。
7. `cargo test -p vulcan-service-kernel task_session_execution_report_fast_path_skips_completion_hook -- --nocapture`：通过。
8. `cargo check -p vulcan-agent-runtime -p vulcan-service-kernel --all-targets`：通过。
9. `git diff --check -- ...`：通过，仅有既有 LF/CRLF 提示。

## 2026-07-07 TaskSession max_provider_requests 软压缩门接入补充

本轮继续处理上一阶段明确未完成的 `max_provider_requests`。源码追踪后结论如下：

1. `ContextCompactionPolicySnapshot.max_provider_requests` 的语义是“压缩前允许的最大 provider 请求数量”，不是最大执行轮数。
2. 子代理终态 outcome 已经在子代理节点事件 payload 中保存 `provider_turns_executed`。
3. TaskSession 上下文压缩完成后会在同一子代理节点追加 `context_compaction_status: "completed"` 的 RuntimeMarker。
4. 因此累计口径应为“最近一次完成压缩 marker 之后的子代理事件 provider_turns_executed 总和”，而不是父会话统计、UI 显示统计或压缩任务自身 provider_request_count。
5. 直接复用 `max_turns` 会把“达到阈值后压缩再继续”误变成“达到阈值后停止执行”，仍然不允许。

本轮修复后的事实：

1. 新增 `task_provider_requests_since_latest_compaction_event(...)`，只统计最近一次完成压缩 marker 之后的 `provider_turns_executed`。
2. 完成压缩 marker 自身的 `provider_request_count` 不参与子代理普通执行 provider 请求累计。
3. 新增 `task_provider_request_limit_is_enabled(...)` 与 `task_provider_request_limit_reached(...)`，要求自动压缩开启且阈值为正才启用该门。
4. 子代理启动前会先检查历史累计，如果已经达到阈值，会先尝试执行一次 TaskSession 上下文压缩，再启动本轮 provider。
5. 子代理完成后会用“历史累计 + 本轮 provider_request_count”判断是否需要派生一次非阻塞上下文压缩。
6. 该门只影响子代理自己的上下文压缩，不改变主会话统计、不改变子代理执行轮次上限、不改变父会话回流显示策略。

新增/更新的回归护栏：

1. `task_provider_request_counter_uses_latest_compaction_boundary`：验证计数从最近一次完成压缩 marker 后开始，并且不把压缩任务自己的 provider 请求计入普通子代理执行累计。
2. `task_provider_request_limit_requires_auto_compaction_and_positive_limit`：验证必须开启自动压缩且配置正数阈值才会触发。
3. `preparation_config` 全组继续通过，覆盖 TaskSession 请求组装、子代理工具禁用、预算策略与提示词快照相关行为。

验证结果：

1. `cargo fmt -p vulcan-agent-runtime`：通过。
2. `cargo test -p vulcan-agent-runtime task_provider_request_counter_uses_latest_compaction_boundary -- --nocapture`：通过。
3. `cargo test -p vulcan-agent-runtime task_provider_request_limit_requires_auto_compaction_and_positive_limit -- --nocapture`：通过。
4. `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`：通过，22 个测试通过；锁污染 panic 是测试主动触发并捕获的预期行为。
5. `cargo test -p vulcan-agent-runtime inter_agent_message_queue_resumes_idle_agent -- --nocapture`：通过。
6. `cargo test -p vulcan-agent-runtime task_runtime_preparation_request_uses_resolved_runtime_policy -- --nocapture`：通过。
7. `cargo check -p vulcan-agent-runtime -p vulcan-service-kernel --all-targets`：通过。

剩余注意事项：

1. 该阶段仍没有做坏文件自动修复，也没有调整关闭后智能体归档目录。
2. 如果后续要处理用户观察到的重复 seq / 半行 JSONL 问题，应单独进入 TaskSession 写入与恢复清理阶段，不应混在 provider 请求压缩门里。

## 2026-07-07 TaskSession JSONL 写入序号与坏尾行修复补充

本轮针对用户观察到的重复 seq、缺号、半行 JSONL 与会话正文加载失败继续追踪。源码事实如下：

1. 主会话分段存储已经有共享文件锁、追加前坏尾行修复、metadata 重建、以及“按已写最大 seq 修正 next_global_seq”的机制。
2. TaskSession 自己的 `append_task_segment_event(...)` 是独立实现，修复前没有拿共享追加锁。
3. TaskSession 追加时只相信 `metadata.next_global_seq`，没有像根 task-link 那样按已持久化最大 `seq` 修正陈旧 metadata。
4. `SessionEventRecord.sequence` 与 `TaskInvocationEvent.sequence` 由调用方传入，调用点实际用过时间戳或 `events.len()` 这类非写入层序号。
5. TaskSession 读取时又按这些内层 `sequence` 排序，导致外层 JSONL seq 与内层事件 sequence 可能分裂。
6. TaskSession 追加 JSONL 时原来分两次写入 JSON 与换行，中途失败会留下半行。
7. TaskSession `read_segment_lines(...)` 原来直接严格解析 JSONL，遇到半行或拼接行会直接失败，没有主会话那套“截断坏尾并重建干净 metadata”的保护。

本轮修复后的事实：

1. TaskSession 追加事件现在使用与主会话分段存储相同的共享追加锁。
2. 追加前会按 TaskSession 自身 segment 执行坏尾行修复，保留首条坏行之前的干净记录，坏行与后续段视为不可恢复尾部并丢弃。
3. 修复会重建 TaskSession segment metadata、`active_segment` 与 `next_global_seq`。
4. 追加前会读取已保留的最大 JSONL `seq`，并将 `metadata.next_global_seq` 修正到最大 `seq + 1`，避免陈旧 metadata 造成重复序号。
5. 写入层会把实际分配的 JSONL `seq` 回填到 `session_event.sequence` 与 `task_invocation_event.sequence`。
6. 读取层也以外层 JSONL `seq` 覆盖内层事件 `sequence`，把 JSONL 行序号作为唯一事实来源。
7. TaskSession JSONL 追加现在先构造完整 `JSON + newline` 字节串，再一次写入并 flush，减少 JSON 与换行分离写导致半行的窗口。

新增/更新的回归护栏：

1. `task_segment_append_uses_highest_existing_sequence_and_backfills_event`：验证 TaskSession metadata 陈旧时仍按已写最大 seq 继续追加，并且内层 `session_event.sequence` 被回填。
2. `task_segment_append_repairs_truncated_jsonl_tail_line`：验证 TaskSession 分段尾部出现截断 JSON 后，下一次追加会修复坏尾并继续以干净 seq 写入。
3. `vulcan-task-session` 全量测试继续覆盖根 task-link、嵌套任务、metadata mirror 与 TaskSession artifact roundtrip。

验证结果：

1. `cargo fmt -p vulcan-task-session -p vulcan-agent-runtime`：通过。
2. `cargo test -p vulcan-task-session task_segment_append_uses_highest_existing_sequence_and_backfills_event -- --nocapture`：通过。
3. `cargo test -p vulcan-task-session task_segment_append_repairs_truncated_jsonl_tail_line -- --nocapture`：通过。
4. `cargo test -p vulcan-task-session -- --nocapture`：通过，36 个测试通过。
5. `cargo test -p vulcan-agent-runtime persistence_store -- --nocapture`：通过，16 个测试通过。
6. `cargo check -p vulcan-task-session -p vulcan-agent-runtime -p vulcan-service-kernel --all-targets`：通过。

剩余注意事项：

1. 本轮修的是 TaskSession 自身写入与读取事实源，不负责修复用户已决定删除的历史坏会话。
2. 关闭后智能体归档目录迁移仍未处理。
3. 如果主会话仍出现加载失败，需要单独检查当前 WebUI/Service 是否所有入口都走了主会话已有修复读路径。

## 2026-07-07 主会话正文加载坏尾行恢复与 todolist 禁用确认补充

本轮继续追踪用户观察到的主会话正文加载失败和“todo list 也属于不可子代理使用范围”的补充要求。源码事实如下：

1. UI 中的“会话正文加载失败”文案来自 WebUI 对 workspace-store 错误的展示；底层报错文本来自 `segmented_session_store/segment_io.rs` 的 JSONL 严格解析。
2. 主会话追加路径已经有共享追加锁、坏尾行修复、metadata 重建和一次写入完整 JSONL 行的保护。
3. 修复前，主会话正文窗口读取入口会先调用修复函数，但修复函数自己持锁结束后，窗口读取又在无锁状态下读取 metadata 与 segment 行。
4. 这意味着加载端依赖“修复之后没有任何中间写入状态”这一假设；对于 UI 正文加载这种边界，语义不够稳定。
5. `todolist` 已经被纳入 `vulcan-session-core::child_agent_forbidden_tool_ids()`，该清单同时供默认子代理隐藏工具、镜像/克隆子代理保留 schema 但拒绝调用、以及提示词隐式说明使用。

本轮修复后的事实：

1. `read_segmented_session_event_window(...)` 现在在同一把共享追加锁内完成坏尾行修复、metadata 读取和窗口读取。
2. `read_segmented_session_turn_window(...)` 也改为锁内修复读，覆盖 WebUI 正文按 turn 加载的主要路径。
3. `replay_segmented_session_context(...)` 改为锁内修复后再回放，避免运行时状态重建遇到半行 JSONL 时继续暴露严格解析错误。
4. 移除了已经没有调用者的无锁 `repair_segmented_session_jsonl_files_if_needed(...)`，保留唯一的锁内修复入口，避免后续新增旁路。
5. 新增主会话加载坏尾行回归测试，直接模拟 segment 末尾半截 JSON，然后通过 turn window 读取验证可恢复。
6. 确认 `todolist` 不需要单独再写一套规则，它已经由同一个智能体禁用事实源覆盖。

新增/更新的回归护栏：

1. `read_turn_window_repairs_corrupt_jsonl_tail_before_loading`：验证主会话正文加载路径遇到坏尾行时会先修复再读取，且 metadata 的 `next_global_seq` 与 segment 行数同步恢复。
2. `child_agent_forbidden_tools_include_todolist`：验证共享禁用清单包含 `todolist`，并支持大小写归一化匹配。
3. `preparation_config` 全组继续验证默认子代理隐藏禁用工具，镜像子代理保留 schema 但拒绝调用同一份禁用工具列表。

验证结果：

1. `cargo fmt -p vulcan-workspace-store`：通过。
2. `cargo test -p vulcan-workspace-store read_turn_window_repairs_corrupt_jsonl_tail_before_loading -- --nocapture`：通过。
3. `cargo test -p vulcan-workspace-store segmented_session_store -- --nocapture`：通过，45 个测试通过。
4. `cargo test -p vulcan-session-core child_agent_forbidden_tools_include_todolist -- --nocapture`：通过。
5. `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`：通过，22 个测试通过；锁污染 panic 是测试主动触发并捕获的预期行为。
6. `cargo check -p vulcan-workspace-store -p vulcan-agent-runtime --all-targets`：通过。
7. `git diff --check -- ...`：通过，仅有 LF/CRLF 提示。

剩余注意事项：

1. 本轮仍不修补用户已决定删除的历史坏会话。
2. 关闭后智能体归档目录迁移仍未处理。
3. 子代理回流统计拆分、主会话统计不被子代理污染、以及子代理提示词组装完全共享主会话处理器仍是后续大项。

## 2026-07-07 智能体统计 / 提示词 / 停止语义状态校准

本轮针对上一节遗留项做源码复核与窄测试，避免把旧阶段结论继续当作当前事实。

当前源码事实：

1. `agent_system_prompt_override` 已经没有生产代码命中；智能体执行请求现在使用 `ExecutionAgentPromptSnapshot` 与 `ExecutionSubagentPromptContext`。
2. 固定父级提示词快照由智能体创建时记录，后续执行通过共享 runtime prompt layers 入口组合；智能体身份、委托任务、硬禁用工具说明和自定义 / 专家功能提示词作为结构化上下文进入同一个组装器。
3. `SessionTurnStats` 已经有 `usage_groups`，并按 `Regular` / `Subagent` / `ContextCompaction` 分组。
4. 父会话收到智能体回流时，会追加独立的智能体反馈记录；该反馈记录的统计类型是 `SubagentFeedback`，不会被当作父会话普通 provider 回合。
5. mailbox 消费智能体回流时，智能体 usage 不再混入父会话当前 turn 的常规统计。
6. 停止主会话时的服务层和前端已经具备运行中智能体三选一语义：先返回确认，再由用户选择“只停止主会话”或“同时停止智能体”；取消不会修改运行态。
7. 父会话已经停止后，detached 智能体终态不会触发父会话 pending prompt dispatch，mailbox 也不会向已停止父会话继续塞入新输入。
8. `todolist` 继续由共享智能体硬禁用工具事实源覆盖，默认 / 自定义 / agent 智能体隐藏，mirror / clone 智能体保留 schema 但调用返回固定拒绝。

本轮验证结果：

1. `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`：通过，覆盖固定提示词快照、结构化智能体提示词上下文、硬禁用工具和 `todolist` 规则。
2. `cargo test -p vulcan-agent-runtime runtime_prompt_layers -- --nocapture`：通过，覆盖共享提示词组装器消费请求级固定快照与智能体上下文。
3. `cargo test -p vulcan-workbench-model session_turn_stats_groups_usage_segments_by_owner -- --nocapture`：通过，覆盖统计分组模型。
4. `cargo test -p vulcan-service-kernel mailbox_intervention_keeps_subagent_usage_out_of_parent_turn_stats -- --nocapture`：通过，覆盖智能体回流不污染父会话普通统计。
5. `cargo test -p vulcan-service-kernel subagent_completion_appends_independent_parent_feedback_rows -- --nocapture`：通过，覆盖父会话独立智能体反馈记录与反馈统计。
6. `cargo test -p vulcan-service-kernel stop_conversation_running_subagent_summaries_preserve_task_identity -- --nocapture`：通过，覆盖停止确认摘要保留真实 task 身份。
7. `cargo test -p vulcan-service-kernel stopped_detached_parent_event_skips_pending_prompt_dispatch -- --nocapture`：通过，覆盖父会话停止后不继续调度 pending prompt。
8. `cargo test -p vulcan-service-kernel session_mailbox_skips_stopped_parent_session -- --nocapture`：通过，覆盖已停止父会话不再接收 mailbox 输入。

状态校准：

1. “智能体回流统计拆分、主会话统计不被智能体污染、提示词组装共享化”在当前未提交代码中已经有源码实现和回归测试覆盖，不应再作为未实现大项处理。
2. 仍需真实 UI 复测默认 / 克隆 / mirror / custom 智能体：实时过程记录、重启后过程与统计恢复、统计 tooltip 的常规 / 智能体分组展示。
3. 关闭后智能体归档目录迁移仍未处理；用户此前明确说当前不要做这个调整，因此继续记录为长期待办，不在本阶段实施。
4. 如果后续真实 UI 复测仍出现统计为 0 或提示词 token 异常，应以新会话样本追踪具体入口，不应回到 pending usage 或完整 prompt 字符串二次注入的旧方案。

## 2026-07-07 智能体目录复制等价读取护栏

本轮继续处理“智能体目录如果复制出来，应能等同主会话目录读取”的验收表达。源码复核发现：

1. `store_task_outcome_persists_segmented_task_session_artifacts` 已经验证智能体原始目录可通过 `read_segmented_session_runtime_conversation_state(...)` 与 `read_segmented_session_display_conversation_state(...)` 读取。
2. 该测试也已经验证 `metadata.json`、`state.json`、`runtime_metadata.json`、`runtime.index.json` 和 display turn stats 均存在。
3. 但此前测试没有把智能体目录搬离父会话根目录后再读取，因此“读取器是否暗中依赖父目录上下文”没有被显式卡住。

本轮调整：

1. 在现有持久化测试中新增测试专用递归目录复制 helper。
2. 将完成后的智能体目录复制到独立临时目录。
3. 在复制后的目录上直接调用通用 runtime/display 读取器。
4. 断言复制目录中仍能读取最终 assistant message、用户可见输出和 `TurnStats` 记录。

验证结果：

1. `cargo fmt -p vulcan-agent-runtime`：通过。
2. `cargo test -p vulcan-agent-runtime store_task_outcome_persists_segmented_task_session_artifacts -- --nocapture`：通过。
3. `cargo test -p vulcan-agent-runtime persistence_store -- --nocapture`：通过，16 个测试通过。

状态校准：

1. 智能体目录的 runtime/display 读取已经具备“复制后独立读取”的回归护栏。
2. 这仍不等同于 UI 层真实重启复测；UI 复测仍需要用户用实际桌面工作台确认右侧智能体详情和父会话反馈展示。
3. 关闭后智能体目录归档迁移仍未实施，继续按用户要求暂缓。

## 2026-07-07 智能体配置禁用工具投影护栏

本轮继续复核“智能体配置里不应该出现硬禁用工具”的实现边界。源码事实如下：

1. 后端 `subagent_catalog_summary_from_catalog(...)` 已经把 `vulcan_session_core::child_agent_forbidden_tool_ids()` 投影为 `SubagentCatalogSummary.child_agent_forbidden_tool_ids`。
2. WebUI `buildAgentToolPolicyOptions(...)` 从 `workbench.subagentCatalog.childAgentForbiddenToolIds` 构建规范化集合，过滤 native tools、LuaSkills、MCP tools，以及“已配置但当前未解析到”的候选项。
3. 浏览器 fallback 的 `childAgentForbiddenToolIds` 仍为空；这是为了不在前端手写第二份禁用清单。真实生产路径应以服务端投影为准。

本轮调整：

1. 在服务端 catalog 投影测试中新增 `subagent_catalog_summary_projects_child_forbidden_tools`。
2. 测试断言 WebUI catalog summary 的禁用工具列表等于共享事实源。
3. 测试额外断言列表包含 `todolist`，防止用户补充的规则后续被漏投影。

验证结果：

1. `cargo fmt -p vulcan-service-kernel`：通过。
2. `cargo test -p vulcan-service-kernel subagent_catalog_summary_projects_child_forbidden_tools -- --nocapture`：通过。
3. `cargo test -p vulcan-service-kernel subagent_fixed_model_tests -- --nocapture`：通过，6 个测试通过。

状态校准：

1. 智能体配置候选过滤的生产数据源已有后端投影与测试护栏。
2. fallback 不引入手写清单，避免出现第二事实源。
3. 真实 UI 仍需确认智能体配置工具策略表不展示 `todolist`、`ask`、`submit_plan`、智能体调度工具和 VMM 记忆工具。

## 2026-07-07 智能体 scoped 工具拒绝执行护栏

用户再次确认 todo list 也属于智能体不可使用范围后，本轮继续沿执行路径复核，重点确认 mirror / clone 这类“保留父会话工具形状”的智能体不会真的执行 `todolist` 等父级工具。源码事实如下：

1. `crates/vulcan-session-core/src/child_agent_policy.rs` 继续作为智能体硬禁用工具唯一事实源，真实工具 id 是 `todolist`。
2. `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_launch.rs` 中 default / custom / agent 模式把硬禁用工具写入 request hidden list，mirror / clone 模式把同一清单写入 request rejection list。
3. mirror / clone 的语义不是“隐藏工具”，而是“保留 schema 但调用时拒绝”，这样可以保持克隆上下文工具形状一致，同时阻止智能体修改父会话状态。
4. `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_stream_executor.rs` 会在真实工具执行前检查 scoped rejection list，命中时返回普通工具结果，状态为 `rejected`，不会报错中止整个 turn。
5. 调用失败不等于没调用；拒绝调用仍会产生工具完成事件，界面和统计层可以按一次工具调用处理。

本轮补充护栏：

1. 在 `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge/tests.rs` 新增 `streaming_tool_executor_rejects_only_scoped_tool_ids`。
2. 测试构造 scoped rejection list，只拒绝 `failing_tool`，同时允许 `demo_tool` 正常执行。
3. 测试断言被拒绝工具返回正常工具结果，host payload status 为 `rejected`。
4. 测试断言未列入拒绝清单的工具仍返回真实 `change_set` 结果。
5. 测试断言两次调用都产生 `ToolFinished` 事件，避免以后把策略拒绝误改成直接报错或静默跳过。

验证结果：

1. `cargo fmt -p vulcan-agent-runtime`：通过。
2. `cargo test -p vulcan-agent-runtime tool_runtime_bridge`：通过，12 个测试通过。
3. `cargo test -p vulcan-session-core child_agent_forbidden_tools_include_todolist -- --nocapture`：通过。
4. `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`：通过，22 个测试通过。
5. `cargo check -p vulcan-agent-runtime --all-targets`：通过。

状态校准：

1. `todolist` 已按用户要求属于智能体不可调用工具范围。
2. 普通智能体不注册 `todolist`，mirror / clone 智能体即使保留工具形状也只能得到固定拒绝结果。
3. scoped 拒绝链路已经有执行器级测试，能防止以后误改成“全拒绝”或“完全不拒绝”。
4. 真实 UI 仍需用户复测工具策略表和右侧智能体详情中的实际表现。

## 2026-07-07 提示词重复注入复核

本轮继续复核“默认智能体首轮 token 异常膨胀”的旧问题是否仍有生产入口。源码事实如下：

1. `agent_system_prompt_override` 在 `crates/vulcan-agent-runtime/src`、`crates/vulcan-service-kernel/src`、`crates/vulcan-core/src` 中已无生产命中。
2. 智能体执行准备请求通过 `ExecutionAgentPromptSnapshot` 传递创建时固定的父级提示词快照。
3. 智能体身份、委托任务、禁用工具提示、自定义功能提示词通过 `ExecutionSubagentPromptContext` 进入共享 prompt composer。
4. `build_task_controller_config(...)` 不再把完整智能体提示词嵌回父级 effective config。

复核验证：

1. `rg -n "agent_system_prompt_override" crates/vulcan-agent-runtime/src crates/vulcan-service-kernel/src crates/vulcan-core/src`：无结果。
2. `cargo test -p vulcan-agent-runtime runtime_prompt_layers_use_request_scoped_agent_prompt_snapshot -- --nocapture`：通过。
3. `cargo test -p vulcan-agent-runtime task_runtime_preparation_request_carries_subagent_prompt_context -- --nocapture`：通过。
4. `cargo test -p vulcan-agent-runtime build_task_controller_config_does_not_embed_subagent_prompt -- --nocapture`：通过。

状态校准：

1. 当前未发现“完整提示词字符串二次注入”的生产入口。
2. 如果真实 UI 复测仍出现智能体首轮 token 明显高于父会话，应以新会话样本追踪实际 provider request preview / usage，而不是回退到旧的 `agent_system_prompt_override` 假设。

## 2026-07-07 硬禁用工具覆盖 allowlist 护栏

本轮继续复核“智能体模式下明确禁用的工具，即使被手写到允许列表，也必须按未允许处理”。源码事实如下：

1. 普通主会话里 `todolist` 是通用工具，普通工具选择器不能隐藏它。
2. 智能体硬禁用不依赖普通工具选择器，而是在 TaskSession 请求上写入 `tool_request_hidden_tool_ids` 或 `tool_request_rejection_tool_ids`。
3. `prepare_execution_with_tool_snapshot(...)` 先完成普通工具曝光，再应用 request hidden list，因此智能体请求级隐藏可以覆盖普通主会话的通用工具保留规则。

本轮补充护栏：

1. 扩展 `request_hidden_tools_remove_universal_todolist_after_exposure`。
2. 测试先把 `tool_exposure_override.allowed_tools` 设置为只允许 `todolist`。
3. 同一请求再把 `todolist` 放入 `tool_request_hidden_tool_ids`。
4. 最终断言 provider 可见 `tool_descriptors` 和可执行 `resolved_tool_definitions` 都不包含 `todolist`。

验证结果：

1. `cargo fmt -p vulcan-agent-runtime`：通过。
2. `cargo test -p vulcan-agent-runtime request_hidden_tools_remove_universal_todolist_after_exposure -- --nocapture`：通过。
3. `cargo test -p vulcan-agent-runtime execution_preparation -- --nocapture`：通过，14 个测试通过。
4. `cargo check -p vulcan-agent-runtime --all-targets`：通过。

状态校准：

1. 智能体硬禁用工具可以覆盖手写 allow-only。
2. 本轮没有新增局部禁用规则，仍依赖 `child_agent_forbidden_tool_ids()` 这一统一事实源与 request hidden / rejection 两条共享链路。

## 2026-07-07 用户可见智能体命名收敛

本轮继续复核“UI 展示统一使用智能体，不要在用户可见界面混用子代理”。源码检索后区分两类内容：

1. `subagent` 协议字段、类型名、内部函数名和注释仍然存在，这些不是本轮用户可见文案，不直接改名。
2. 智能体管理页、浏览器 fallback 预览、工具详情 fallback 文案、服务端 RPC 错误字符串中仍有少量用户可能看到的 `子智能体` / `subagent task` 表达。

本轮调整：

1. `AgentsArea.tsx` 中用户级 Markdown 文件说明从“子智能体”改为“智能体”。
2. `AgentsArea.tsx` 中搜索输入 aria-label 与 placeholder 从“搜索子智能体 / Search subagents”改为“搜索智能体 / Search agents”。
3. `browser-fallback-workbench.ts` 中 agents 区域说明与示例 catalog notice 改为 agent 语义。
4. `conversation-tool-execution-model.ts` 中创建任务详情 fallback 从 `Created subagent task` 改为 `Created agent task`。
5. `conversation-actions.ts` 中旧错误匹配文案改为“没有找到可打开的智能体任务”。
6. `subagent_panels.rs` 中会返回给前端的错误文本从 `invalid subagent task id`、`subagent task ... is not open`、`closed subagent tasks` 和“子智能体会话尚未完成初始化”改为用户可见的 agent / 智能体语义；`subagent_control` 和 `subagent_task_id` 协议字段不改。

验证结果：

1. `rg -n "Created subagent task|没有找到可打开的子代理任务|invalid subagent task id|subagent task \`|closed subagent tasks|Subagents, expert|Search subagents|子智能体会话尚未" apps/vulcan-code-web/src crates/vulcan-service-kernel/src/subagent_panels.rs`：只剩注释级命中，无目标用户可见字符串。
2. `npm run lint`：通过。
3. `cargo check -p vulcan-service-kernel --all-targets`：通过。

状态校准：

1. 用户可见文案继续向“智能体 / Agent”收敛。
2. 内部协议和类型名仍保留 `subagent`，后续若要重命名必须作为独立协议迁移任务处理，不能混入本轮 UI 文案清理。

## 2026-07-07 fork / mirror 运行时快照语义复核

用户继续补充 fork 底层要求：智能体继承应复制当前运行时状态，不应依赖显示记录回推，不应存在按轮次裁剪，也不应保留 `fork_turns` 这类模型可见语义。本轮按源码重新追踪，结论如下：

1. `spawn_agent` 模型可见 schema 只接受 `message`、`task_name`、`creation_mode`、`agent_id`、`system_prompt`、`model`。
2. `task_tool_args_from_spawn_agent_args(...)` 明确拒绝 `fork_turns`、`agent_type`、`reasoning_effort` 等旧字段。
3. `creation_mode = mirror` 映射到内部 `SUBAGENT_MODE_MIRROR`，再由 `task_initial_context_source(...)` 解析为 `TaskInitialContextSource::CloneLatestRuntime`。
4. `CloneLatestRuntime` 的数据源是 `TaskRuntime.execution_context_for_session(current_session_id).parent_snapshot`，也就是当前父级 runtime bootstrap 后保存的快照，不是会话显示记录或 JSONL 重放。
5. default / custom / agent 模式仍使用 `TaskInitialContextSource::Fresh`，不会继承父会话消息。
6. 会话级 `forkConversation` 请求结构当前只有 `conversationId` 与 `title`，并通过 `deny_unknown_fields` 拒绝旧 `beforeMessageId`。
7. 会话级 fork 底层复制分段会话目录并改写结构身份，`fork_source` 只作为界面展示来源标记；这条链路不参与智能体上下文组装。

本轮补充护栏：

1. 扩展 `spawn_agent_clone_latest_runtime_preserves_parent_snapshot`。
2. 父级 runtime snapshot 增加一条隐藏 `ConversationRecord`，模拟父会话 runtime 中存在的持久记录。
3. 测试断言 clone-latest 子快照不仅保留消息与 provider continuation，也保留这条 record 的 id、body 与 metadata。
4. 这将“一字不差复制 runtime snapshot”的要求固化到测试，避免以后只复制消息窗口而丢持久记录。

验证结果：

1. `cargo fmt -p vulcan-agent-runtime`：通过。
2. `cargo test -p vulcan-agent-runtime spawn_agent_clone_latest_runtime_preserves_parent_snapshot -- --nocapture`：通过。
3. `cargo test -p vulcan-agent-runtime spawn_agent_arguments_reject_fork_turns -- --nocapture`：通过。
4. `cargo test -p vulcan-service-kernel conversation_fork_request_rejects_before_message_id -- --nocapture`：通过。
5. `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`：通过，22 个测试通过。
6. `cargo test -p vulcan-session-core child_agent_forbidden_tools_include_todolist -- --nocapture`：通过。

状态校准：

1. 当前未发现智能体生产链路仍使用按轮次 fork 或 `fork_turns` 语义。
2. 当前未发现会话 fork API 仍允许 `beforeMessageId` 历史边界。
3. `todolist` 已纳入智能体硬禁用清单；普通智能体隐藏，mirror / clone 保留工具形状但固定拒绝调用。
4. 后续若要把会话级 fork 的 `fork_source` 重命名成 `is_fork`，应作为独立展示元数据迁移任务处理，不应混入智能体 runtime snapshot 语义。

## 2026-07-07 智能体副作用隔离复核

用户补充要求：智能体不应触发 hook，例如记忆与 post-action。本轮沿 runtime policy、配置解析和执行准备三层确认，结论如下：

1. `ExecutionRuntimePolicy::task_session()` 默认关闭 `hooks_enabled`、`vmm_enabled`、`memory_recall_enabled`、`memory_writeback_enabled`、`database_projection_enabled`。
2. `TaskSessionConfigResolver::resolve_runtime_policy(...)` 会先合并 profile 与 invocation 中的 runtime policy 覆盖，再调用 `enforce_task_session_projection_boundary(...)` 强制关闭全部副作用开关。
3. 因此即使智能体配置或调用参数尝试开启 VMM / 记忆 / hook，最终 resolved runtime policy 仍为关闭。
4. `prepare_execution_with_tool_snapshot(...)` 会根据 `request.runtime_policy` 决定是否保留 `project_vmm`、`session_vmm`、`system_prompt_hook_blocks`。
5. 当 runtime policy 是 TaskSession 隔离策略时，执行准备层会把请求中携带的 hook blocks 与 VMM 配置剥离为空。
6. VMM PostAction 是 conversation hook 的 completion 路径；hooks 与 VMM 都关闭后，智能体 turn 不会走主会话记忆写回或 post-action。

验证结果：

1. `cargo test -p vulcan-session-core task_session_policy_disables_side_effects -- --nocapture`：通过。
2. `cargo test -p vulcan-task-session task_session_runtime_overrides_keep_side_effects_forbidden -- --nocapture`：通过。
3. `cargo test -p vulcan-agent-runtime prepare_execution_clears_side_effect_inputs_for_task_session_policy -- --nocapture`：通过。

状态校准：

1. 当前未发现智能体会触发主会话 hook、VMM 记忆召回、记忆写回、post-action 或普通数据库投影。
2. 本轮没有新增代码，因为已有三层护栏已经覆盖该要求。
3. 后续如果新增 runtime side effect 类型，应接入 `ExecutionRuntimePolicy::task_session()` 与 `enforce_task_session_projection_boundary(...)`，不能只在单个调用点局部关闭。

## 2026-07-07 停止主会话时运行中智能体确认链路复核

用户提出交互要求：点击停止当前主会话时，如果有智能体正在工作，应弹出提示，让用户选择只停止主会话、同时停止智能体，或取消停止。本轮沿前后端链路复核，结论如下：

1. 后端 `StopConversationRunMode` 已包含三种模式：`AskIfRunningSubagents`、`MainOnly`、`MainAndSubagents`。
2. `service_stop_conversation_run(...)` 默认使用 `AskIfRunningSubagents`，会先读取 live cache 中仍在运行的智能体任务。
3. 如果存在运行中智能体，后端返回 `StopConversationRunResponse::NeedsConfirmation`，不先修改运行态。
4. 前端 `handleStopConversationRun(...)` 首次调用 `stopConversationRun("ask_if_running_subagents")`。
5. 收到 `needs_confirmation` 后，前端通过 `requestStopConversationRunChoice(...)` 打开确认弹窗。
6. `StopConversationRunDialog` 提供三个路径：取消、只停止主会话、同时停止智能体。
7. 用户选择“只停止主会话”时，前端发送 `main_only`；用户选择“同时停止智能体”时，前端发送 `main_and_subagents`。
8. 后端只有在 `MainAndSubagents` 下才会遍历运行中智能体并调用 `task_runtime.stop_task_tree(...)`。

验证结果：

1. `cargo test -p vulcan-service-kernel stop_conversation_running_subagent_summaries_preserve_task_identity -- --nocapture`：通过。
2. `npm run lint`：通过。
3. `rg -n "conversation.stopWithAgents|stopWithAgents|StopConversationRunChoice|RequestStopConversationRunChoice|stopConversationRunPromptItems|stopConversationRunModeFromChoice" apps/vulcan-code-web/src`：确认前端弹窗、i18n 文案、请求选择和后端 mode 映射均存在。

状态校准：

1. 当前停止主会话交互已满足“取消 / 只停主会话 / 同时停智能体”要求。
2. 真实 UI 仍需用户手动复测弹窗显示、按钮顺序和实际停止行为。

## 2026-07-07 TaskSession 目录复制后的普通会话读取护栏补充

本轮继续复核“智能体会话目录复制出来后应等同普通主会话载入”的要求。源码事实如下：

1. TaskSession 目录内已经写入主会话兼容的 `metadata.json`、`state.json`、display JSONL 和 provider-facing runtime JSONL。
2. `store_task_outcome_persists_segmented_task_session_artifacts` 已覆盖复制 TaskSession 目录后通过主会话 runtime reader 与 display reader 读取输出和 `TurnStats`。
3. TaskSession 首条元事件的外层类型为 `subagent_task_meta`。
4. `subagent_task_meta` 已经是 workspace-store `SegmentedSessionEventKind::SubagentTaskMeta` 的稳定已知类型，因此普通 display reader 不应把它当未知坏数据清理。

本轮补充护栏：

1. 扩展 `store_task_outcome_persists_segmented_task_session_artifacts`。
2. 在复制出来的 TaskSession 目录经过主会话 display reader 重放后，重新读取 `s001.jsonl`。
3. 断言 `subagent_task_meta` 仍保留在复制目录中。
4. 这覆盖了“普通会话读取不会破坏 TaskSession 元事件”的实际运行风险。

验证结果：

1. `cargo fmt -p vulcan-agent-runtime`：通过。
2. `cargo test -p vulcan-agent-runtime store_task_outcome_persists_segmented_task_session_artifacts -- --nocapture`：通过。

状态校准：

1. 本轮没有修改生产逻辑；当前事件类型已满足主会话 reader 的已知类型要求。
2. TaskSession 目录复制后的 runtime、display、输出、统计与元事件保留均已有回归护栏。
3. 真实 UI 仍需继续复测智能体右侧面板在运行时与重启后的过程、输出和统计显示。

## 2026-07-07 目标预算对智能体用量分组的计入口径修正

本轮继续复核“智能体不应改变主会话统计预算”的要求。源码事实如下：

1. `goal_token_delta_for_turn_stats(...)` 已经会忽略整条 `SessionTurnStatsKind::SubagentFeedback`。
2. 但是 `SessionTurnStats` 现在已经支持 `usage_groups`。
3. 如果某个普通统计块同时携带 `Regular` 与 `Subagent` 分组，旧算法会直接按聚合字段 `input_tokens`、`cached_input_tokens`、`output_tokens` 和 `reasoning_tokens` 计算目标 token。
4. 这意味着只排除 `SubagentFeedback` 不足以长期保证目标预算不被智能体分组污染。

本轮修复后的事实：

1. `goal_token_delta_for_turn_stats(...)` 在 `usage_groups` 非空时，优先按分组计算。
2. 分组计算明确跳过 `SessionTurnUsageGroupKind::Subagent`。
3. `Regular` 与 `ContextCompaction` 分组继续计入目标预算，保留自动压缩用量进入预算的既有语义。
4. 对没有 `usage_groups` 的旧统计记录，继续使用旧聚合算法，避免历史数据行为突然变化。

新增/更新的回归护栏：

1. `goal_token_delta_ignores_subagent_usage_groups_inside_regular_stats`：验证一个普通统计块同时包含常规与智能体分组时，目标预算只按常规分组计算。
2. `latest_session_turn_stats_includes_embedded_compaction_usage_segments`：继续验证压缩分组仍计入目标预算。
3. `goal_token_delta_ignores_subagent_feedback_stats`：继续验证整条智能体反馈记录不计入目标预算。

验证结果：

1. `cargo fmt -p vulcan-service-kernel`：通过。
2. `cargo test -p vulcan-service-kernel goal_token_delta_ignores_subagent_usage_groups_inside_regular_stats -- --nocapture`：通过。
3. `cargo test -p vulcan-service-kernel latest_session_turn_stats_includes_embedded_compaction_usage_segments -- --nocapture`：通过。
4. `cargo test -p vulcan-service-kernel goal_token_delta_ignores_subagent_feedback_stats -- --nocapture`：通过。

状态校准：

1. 目标预算现在同时具备“反馈整行忽略”和“混合统计分组内忽略智能体分组”两层保护。
2. Transcript 统计展示仍保留完整聚合与分组明细；本轮只改变目标预算计入口径。
3. 后续新增用量分组时，必须显式决定是否进入目标预算，不能继续默认按聚合总数字段计算。

## 2026-07-07 TaskSession 自身统计用量分段归属修正

本轮继续复核“子代理自己的统计逻辑应该与主会话完全相同”的要求。源码事实如下：

1. `task_execution_outcome_from_report(...)` 会把智能体终态 report 转成 `TaskExecutionOutcome`。
2. 这份 outcome 的 `usage_segments` 是父会话视角，用于父会话反馈记录和 host payload，因此分段类型是 `SessionTurnUsageSegmentKind::SubagentTask`。
3. `append_task_context_turn_stats_record(...)` 在持久化 TaskSession 自己的上下文记录时，也会用 `outcome.usage_segments` 补齐缺失的 token。
4. 如果规范 assistant 消息没有 usage，旧逻辑会把父会话视角的 `SubagentTask` 原样写入子会话自己的 TurnStats。
5. 这会让智能体自身统计显示为“智能体分组”，而不是普通助手响应，和子会话内部语义不一致。

本轮修复后的事实：

1. 新增 `task_context_usage_segments_from_outcome(...)`。
2. 该转换只用于 TaskSession 自身上下文统计落盘。
3. 转换时把 `SubagentTask` 改为 `AssistantResponse`。
4. 父会话反馈记录、mailbox payload 和 host payload 仍继续使用 `SubagentTask`，不改变父会话“智能体”分组展示。
5. 原有 provider/tool 调用次数去重逻辑保留，显式 usage 只补 token，不重复放大调用次数。

新增/更新的回归护栏：

1. `store_task_outcome_merges_explicit_usage_without_losing_record_calls`：更新断言，确认子会话自身补齐的显式 usage 分段归属为 `AssistantResponse`，并聚合到 `regular` 分组。
2. `store_task_outcome_persists_segmented_task_session_artifacts`：继续验证 TaskSession context、runtime、display 与统计可读。

验证结果：

1. `cargo fmt -p vulcan-agent-runtime`：通过。
2. `cargo test -p vulcan-agent-runtime store_task_outcome_merges_explicit_usage_without_losing_record_calls -- --nocapture`：通过。
3. `cargo test -p vulcan-agent-runtime store_task_outcome_persists_segmented_task_session_artifacts -- --nocapture`：通过。

状态校准：

1. 父会话视角与子会话自身视角现在明确分离：父会话看到智能体分组，子会话自己看到常规助手分组。
2. 这修复的是统计语义错位，不改变智能体结果回流、父会话反馈记录或 Provider-facing runtime。
3. 后续如果 outcome 中新增父会话专用分段类型，必须为 TaskSession 自身统计定义对应的本地归属。

## 2026-07-07 空 allow_only 工具策略语义修正

本轮继续复核“智能体禁用工具规则应集中统一，手写允许不应绕过硬禁用”的要求。用户补充明确：`todo list` 也属于不可由智能体使用的范围。

源码事实如下：

1. `vulcan_session_core::CHILD_AGENT_FORBIDDEN_TOOL_IDS` 已包含 `todolist`。
2. 智能体执行请求构建阶段会读取这份统一清单。
3. 默认/专家智能体通过 `tool_request_hidden_tool_ids` 移除禁用工具。
4. 克隆/镜像等需要保持父级工具形状的智能体，通过 `tool_request_rejection_tool_ids` 保留 schema 但调用时返回固定策略拒绝信息。
5. `request_hidden_tools_remove_universal_todolist_after_exposure` 已覆盖“即使 `todolist` 被显式允许，后置隐藏层仍能把它从 Provider 可见工具与可执行定义中移除”。

继续追查时发现一个独立的底层策略语义问题：

1. `ToolExposureSelectorState` 过去只记录全局允许集合 `global_allowed`。
2. 显式 `policy.mode = allow_only` 且 `policy.selectors = []` 时，`global_allowed` 为空。
3. 过滤器旧逻辑把“允许集合为空”解释为“没有启用允许策略”。
4. 因此空 `allow_only` 会被错误回退为开放默认工具面。
5. 这会破坏长期工具策略语义：配置层无法用统一策略表达“不给普通工具”。

本轮修复后的事实：

1. `ToolExposureSelectorState` 新增 `global_allow_active`。
2. `selector_sets_from_policy(...)` 同时返回允许集合、禁用集合和 allow 模式激活状态。
3. 原生工具、LuaSkills、LuaSkills prompt、MCP 和最终公开工具描述过滤均改为依据 `has_global_allow()` 判断是否处于允许模式。
4. 空 `allow_only` 现在会阻止普通工具，而不是开放全部普通工具。
5. 主会话通用 goal/todolist 工具仍保持既有通用规则；智能体路径继续由请求隐藏/拒绝层强制移除这些父会话工具。

新增/更新的回归护栏：

1. `execution_tool_exposure_empty_allow_only_hides_regular_tools`：验证空 `allow_only` 只保留通用工具，不再放出普通原生工具。
2. `execution_preparation` 测试组：继续覆盖 AI fallback、LuaSkills、MCP/Native 过滤、请求隐藏和 agent 路由。
3. `preparation_config` 测试组：继续覆盖智能体 `todolist` 禁用、默认隐藏和镜像拒绝策略。
4. `child_agent_forbidden_tools_include_todolist`：验证核心统一禁用清单包含 `todolist`。

验证结果：

1. `cargo fmt -p vulcan-agent-runtime`：通过。
2. `cargo test -p vulcan-agent-runtime execution_tool_exposure_empty_allow_only_hides_regular_tools -- --nocapture`：通过。
3. `cargo test -p vulcan-agent-runtime execution_preparation -- --nocapture`：通过。
4. `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`：通过。
5. `cargo test -p vulcan-session-core child_agent_forbidden_tools_include_todolist -- --nocapture`：通过。
6. `cargo test -p vulcan-session-core capability_policy_from_selector_lists_normalizes_allow_only -- --nocapture`：通过。

状态校准：

1. `todolist` 现在在事实链路上属于智能体硬禁用工具，而不是 UI 层临时隐藏。
2. 手写允许工具配置不能绕过智能体禁用层。
3. 空 `allow_only` 的语义已经从“误判为无限制”修正为“普通工具空集合”。
4. 若未来需要隐藏主会话通用工具，应继续走请求隐藏层，不应改普通全局 allow 策略。

## 2026-07-07 智能体配置禁用工具统一净化

本轮继续追查“智能体配置里不应该可选子代理明确禁用的工具；允许指定工具开启手写不允许的工具也应该等同未允许”的要求。

源码事实如下：

1. WebUI 的工具选择表已经读取 `workbench.subagentCatalog.childAgentForbiddenToolIds`。
2. `buildAgentToolPolicyOptions(...)` 会从 native、LuaSkills、MCP 和草稿已配置项中移除命中禁用清单的工具。
3. 但 `draftFromDefinition(...)` 仍会把历史定义里的 `toolPolicyTools` 转成草稿字符串。
4. `saveRequestFromDraft(...)` 之前会直接把草稿字符串 `listFromText(draft.toolPolicyTools)` 发给服务端。
5. 服务端 `apply_request_tool_policy(...)` 之前只按请求构建策略，没有剔除智能体禁用工具。
6. 目录加载 `normalize_tool_exposure(...)` 之前也只规范化策略，没有剔除智能体禁用工具。
7. 因此旧文件、手写 sidecar 或手写请求可以把 `todolist`、`ask`、`spawn_agent` 等禁用工具保留在智能体配置中；执行层仍会拦截，但配置层会变脏。

本轮修复后的事实：

1. `vulcan-session-core::sanitize_child_agent_tool_policy(...)` 成为智能体配置工具净化的统一入口。
2. `vulcan-session-core::sanitize_child_agent_tool_exposure(...)` 用该策略净化完整 `ToolExposureSnapshot`，并同步 legacy mirror lists。
3. `allow_only` 中只剩禁用工具时，保留空 `AllowOnly`，用于表达“不给普通工具”。
4. `deny_only` 中只剩禁用工具时，折叠为默认 `All`，因为这些工具无论如何不能配置给智能体。
5. `vulcan-task-session` 的目录规范化读取路径会调用统一净化函数。
6. `vulcan-service-kernel` 的智能体定义保存路径会调用统一净化函数。
7. 执行层隐藏/拒绝策略保持不变，继续作为最终硬拦截。

新增/更新的回归护栏：

1. `child_agent_tool_exposure_sanitizes_allowed_forbidden_tools`：验证 `allow_only` 会移除 `todolist` 与 `ask`，只保留可用工具。
2. `child_agent_tool_exposure_preserves_empty_allow_only_after_sanitize`：验证全是禁用工具时仍保留空 allow-only 语义。
3. `child_agent_tool_exposure_collapses_redundant_forbidden_deny_only`：验证只 deny 禁用工具时折叠为默认策略。
4. `sidecar_legacy_deny_tool_policy_drops_child_forbidden_tools`：验证历史 sidecar 中的 `spawn_agent: deny` 不再进入有效智能体配置。
5. `apply_request_tool_policy_sanitizes_child_forbidden_tools`：验证服务端保存请求会移除禁用工具。

验证结果：

1. `cargo fmt -p vulcan-session-core -p vulcan-task-session -p vulcan-service-kernel`：通过。
2. `cargo test -p vulcan-session-core child_agent_tool_exposure -- --nocapture`：通过。
3. `cargo test -p vulcan-task-session sidecar_legacy_deny_tool_policy_drops_child_forbidden_tools -- --nocapture`：通过。
4. `cargo test -p vulcan-service-kernel apply_request_tool_policy_sanitizes_child_forbidden_tools -- --nocapture`：通过。
5. `cargo test -p vulcan-task-session catalog -- --nocapture`：通过。
6. `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`：通过。
7. `cargo test -p vulcan-agent-runtime execution_preparation -- --nocapture`：通过。

状态校准：

1. 智能体禁用工具现在在配置读取、服务端保存、UI 选择列表和执行层四个入口上都来自同一份 session-core 清单。
2. 手写允许 `todolist` / `ask` / `spawn_agent` 不会再被保存为智能体有效工具策略。
3. 这次仍不实现关闭后归档迁移；关闭后 task 目录长期归档是独立后续任务。

## 2026-07-07 旧反馈链路残留清理与终态事件语义改名

本轮继续按“不能让旧回流事实源和新 TaskSession 事实源并存”的要求，审核旧链路残留。

源码事实如下：

1. `pending_subagent_usage` 与 `agent_system_prompt_override` 在 `crates` / `apps` 范围内已经没有残留引用。
2. `fork_turns` 只剩在测试中验证旧参数会被拒绝、debug 不再输出该字段，不存在生产路径继续按轮次 fork。
3. `TaskFeedback` 结构体没有生产构造、读取或导出需求。
4. `TaskFeedbackAppended` 枚举值没有生产分发或测试需求。
5. `SessionEventKind::TaskFeedback` 实际承载的是 completed/stopped 的智能体任务终态事件，不是“回流反馈”事件。
6. `replace_webui_subagent_task_conversation_state(...)` 仍被 `subagent_event_ingest`、`subagent_workspace_mutation` 和测试使用；它的作用是用规范 TaskSession conversation state 替换 WebUI 镜像状态，不是独立事实源。

本轮修复后的事实：

1. 删除 `TaskFeedback` 结构体。
2. 删除 `TaskFeedbackAppended` 枚举值。
3. 将 `SessionEventKind::TaskFeedback` 改名为 `SessionEventKind::TaskTerminal`。
4. `task_session_event_kind_for_status(...)` 现在将 completed/stopped 映射为 `TaskTerminal`。
5. TaskSession 持久化测试与工作台导航恢复测试均改为断言 `TaskTerminal`。
6. 保留 `replace_webui_subagent_task_conversation_state(...)`，因为它只是 UI 状态镜像入口，后续可单独做命名收敛。

新增/更新的回归护栏：

1. `event_record_round_trips_through_json`：验证新的 `TaskTerminal` 事件可以序列化往返。
2. `store_task_outcome_persists_segmented_task_session_artifacts`：验证智能体终态落盘仍包含终态事件。
3. `navigation_store` 测试组：验证工作台仍能从 segmented TaskSession store 恢复已完成智能体面板。
4. `child_agent` 测试组：继续验证 `todolist` 属于统一智能体禁用工具，并且配置净化规则未被本轮改名影响。

验证结果：

1. `cargo fmt -p vulcan-session-core -p vulcan-agent-runtime -p vulcan-workbench-model -p vulcan-task-session`：通过。
2. `cargo test -p vulcan-session-core event_record_round_trips_through_json -- --nocapture`：通过。
3. `cargo test -p vulcan-task-session -- --nocapture`：通过。
4. `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`：通过。
5. `cargo test -p vulcan-agent-runtime store_task_outcome_persists_segmented_task_session_artifacts -- --nocapture`：通过。
6. `cargo test -p vulcan-workbench-model navigation_store -- --nocapture`：通过。
7. `cargo test -p vulcan-session-core child_agent -- --nocapture`：通过。

状态校准：

1. 旧“反馈对象”不再作为任何事实来源存在。
2. 智能体回流给 AI runtime 的消息、用户可见回流记录、TaskSession 自身终态事件仍是三类不同记录。
3. 本项目不考虑旧智能体记录兼容，因此内部事件 kind 从 `task_feedback` 改为 `task_terminal` 是可接受的长期语义修正。
4. 后续继续重构时，应避免重新引入“反馈”作为统一事实对象；父会话显示应始终从 TaskSession 规范记录投影。

## 2026-07-07 智能体面板陈旧运行态完成兜底移除

本轮继续审核 WebUI/工作台缓存层是否仍在用“猜测状态”弥补同构记录。源码事实如下：

1. `TaskRuntime::restore_persisted_subagent_registry_for_parent_tree(...)` 遇到持久化 TaskSession 仍是 active，但当前进程没有真实运行态时，会将其恢复为 `Stopped` 并记录 `runtime_lost`。
2. `restore_task_session_record_from_metadata(...)` 同样会把 active metadata 转成 stopped 记录。
3. 但工作台模型层 `normalize_subagent_task_panel_state(...)` 在没有实时运行句柄、也没有终态持久化快照时，会把缓存中的 `Running` 猜成 `Completed`。
4. 这会让“没有终态事实”的智能体看起来像自然完成，和底层 TaskRuntime 的 runtime-lost 语义相冲突。
5. 该逻辑只发生在 UI 面板状态归一化层，不是 Provider runtime、TaskSession 持久化或父会话回流的必要逻辑。

本轮修复后的事实：

1. `normalize_subagent_task_panel_state(...)` 有实时运行事实时仍显示 `Running`。
2. 有终态持久化快照时仍以持久化终态为准。
3. 没有实时运行事实且没有终态快照时，不再猜成 `Completed`，而是标记为 `Stopped`。
4. 这让工作台缓存层与 TaskRuntime 恢复语义一致：运行态丢失不能伪装成自然完成。
5. 本轮没有复用“用户主动停止”摘要，避免把 runtime lost 错说成用户主动停止。

新增/更新的回归护栏：

1. `stale_running_state_without_live_runtime_becomes_stopped`：验证无实时运行与无终态快照时，陈旧 running 会转为 stopped。
2. `panel_state` 测试组：继续验证分段存储列表、详情加载、归档过滤、实时运行覆盖终态缓存等行为。
3. `subagent_navigation` 服务测试组：验证导航投影、预加载、关闭/终态状态、跨会话选择等行为未被该状态语义调整破坏。

验证结果：

1. `cargo fmt -p vulcan-workbench-model`：通过。
2. `cargo test -p vulcan-workbench-model panel_state -- --nocapture`：通过。
3. `cargo test -p vulcan-service-kernel terminal_child_phase_marks_stale_running_subagent_completed_in_navigation -- --nocapture`：通过。
4. `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过。
5. `cargo check -p vulcan-workbench-model -p vulcan-service-kernel --all-targets`：通过。

状态校准：

1. WebUI 缓存层不再把缺失终态事实的智能体标为 completed。
2. 缓存仍只是渲染镜像；真实运行事实来自 TaskRuntime，真实终态事实来自 TaskSession 持久化记录。
3. 后续如果 UI 仍显示错误终态，应追踪具体投影入口，不应重新引入按文本或缓存内容猜测 completed 的逻辑。

## 2026-07-07 父会话任务镜像陈旧运行态完成映射移除

本轮沿父会话 spawn_agent 工具卡片状态投影继续追踪，避免只修智能体详情面板而留下父卡片显示错误。

源码事实如下：

1. 父会话工具卡片恢复会读取 TaskSession mirror summary。
2. `apply_task_mirror_statuses_to_parent_messages(...)` 调用 `normalized_task_mirror_execution_status(...)` 生成工具卡片展示状态。
3. 该函数在没有实时运行态时，过去会把 `running`、`created`、`pending`、`in_progress` 映射为 `completed`。
4. 未知状态也默认映射为 `completed`。
5. 这与 TaskRuntime 的运行态丢失恢复语义冲突：持久化 active 但当前无运行事实时，应恢复为 stopped / runtime_lost，而不是自然完成。

本轮修复后的事实：

1. 明确 `completed` / `complete` / `success` / `succeeded` 才映射为 `completed`。
2. 明确 `failed` / `error` / `errored` 仍映射为 `failed`。
3. 明确停止类状态仍映射为 `stopped`。
4. 活动类状态在没有 live runtime 时映射为 `stopped`。
5. 未知状态也映射为 `stopped`，避免无成功事实时展示成功。
6. live runtime 仍优先覆盖持久化终态，保持真正运行中的智能体显示为 running。

新增/更新的回归护栏：

1. `task_mirror_active_status_without_live_runtime_repairs_to_stopped`：验证父会话工具卡片遇到 active mirror 且无 live runtime 时显示 stopped。
2. `task_mirror_completed_status_repairs_stale_running_tool_card`：继续验证明确 completed mirror 可以修复陈旧 running 工具卡片。
3. `task_mirror_display_keeps_running_when_task_is_live`：继续验证实时运行事实优先级最高。
4. `subagent_navigation` 与 `panel_state` 测试组继续覆盖导航和详情面板没有被父卡片映射调整破坏。

验证结果：

1. `cargo fmt -p vulcan-service-kernel`：通过。
2. `cargo test -p vulcan-service-kernel stop_task_mirror_tests -- --nocapture`：通过。
3. `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过。
4. `cargo test -p vulcan-workbench-model panel_state -- --nocapture`：通过。
5. `cargo check -p vulcan-service-kernel -p vulcan-workbench-model --all-targets`：通过。
6. `rg -n '"running" \| "created" \| "pending" \| "in_progress" => String::from\("completed"\)|normalized_task_mirror_execution_status' crates/vulcan-service-kernel/src crates/vulcan-workbench-model/src -S`：确认 active -> completed 映射已移除。

状态校准：

1. 父会话工具卡片不再把缺失终态事实的 active mirror 展示为 completed。
2. 智能体详情面板与父会话工具卡片现在都遵循“无 live runtime 且无成功终态事实 => stopped”的方向。
3. 服务层导航里“终态 workspace phase 清理 stale listener”的逻辑已在后续专项审计中确认：`SessionRunPhase::Ready` 只抑制 live listener，不生成 completed。

## 2026-07-07 用户可见停止摘要命名修正

本轮继续处理用户可见命名收敛，同时审计服务层导航 stale listener 的 completed 来源。

源码事实如下：

1. `terminal_child_phase_marks_stale_running_subagent_completed_in_navigation` 的 completed 状态来自测试种子里的 `SubagentTaskRunState::Completed`。
2. `seed_test_subagent_registry_from_panel_states(...)` 会把该状态映射为 `TaskSessionStatus::Completed`。
3. `SessionRunPhase::Ready` 只用于抑制 stale listener 的继续运行展示，不负责把状态猜成 completed。
4. 因此该导航测试路径里的 completed 是 registry/detail 终态事实，不是按 Ready 阶段推断出来的。
5. 但规范停止摘要常量仍直接显示“子代理已按用户请求主动停止”，属于用户可见命名残留。

本轮修复后的事实：

1. `SUBAGENT_TASK_STOPPED_SUMMARY_TEXT` 改为“智能体已按用户请求主动停止...”。
2. 同步更新该常量的英文/中文注释，使其明确服务于 agent cards / panels / WebUI projections。
3. 没有修改 TaskSession、runtime fork、回流链路或统计链路。
4. `todolist` / `todo list` 已重新写入本专项约束，归入智能体明确禁用工具。

验证结果：

1. `rg -n "子代理已按用户请求主动停止" crates apps -S`：无命中。
2. `cargo fmt -p vulcan-workbench-model -p vulcan-service-kernel`：通过。
3. `cargo test -p vulcan-service-kernel mailbox_tests -- --nocapture`：通过。
4. `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过。
5. `cargo test -p vulcan-workbench-model panel_state -- --nocapture`：通过。
6. `cargo test -p vulcan-service-kernel stop_task_mirror_tests -- --nocapture`：通过。
7. `cargo check -p vulcan-workbench-model -p vulcan-service-kernel --all-targets`：通过。

状态校准：

1. 本轮只修用户可见停止摘要命名，不把内部 crate / module / 测试名中的 `subagent` 一次性改名。
2. 服务层导航的 completed 测试名已在后续命名清理中改为 `registry_terminal_status_remains_visible_after_stale_listener_is_suppressed`。
3. 智能体禁用工具的事实清单现在明确包含 `todolist`，后续统一禁用配置必须覆盖它。

## 2026-07-07 导航成员资格不再由渲染缓存补齐

本轮继续审核“智能体创建后是否立即进入列表”和“关闭后切换会话仍存在”的共同根因，重点检查服务层导航是否仍在用 UI 缓存补事实。

源码事实如下：

1. `workbench_surface_projection.rs` 的文件级说明和函数注释都声明：智能体导航成员关系只来自 runtime registry，detail cache 仅用于渲染。
2. 实际实现中，`authoritative_subagent_tree_projection_from_registry(...)` 在父会话存在 visible round 时，会调用 `append_running_cached_subagent_navigation_projections(...)`。
3. 该函数会把 `subagent_surface_detail_cache` 中 `run_state = Running` 且不在 registry 里的任务追加进导航。
4. 这等于把“渲染缓存”提升成“成员事实源”，与同构目标冲突，也解释了为什么缓存状态容易导致关闭后仍显示。
5. 继续追踪 TaskRuntime 后发现：`prepare_task_execution_request(...)` 在执行 backend 前已经创建内存 `TaskSessionRecord`，但此前没有同步到 `SubagentRegistry`。
6. `DelegateStarted` 发出时 registry 还没看到 created 记录，所以旧代码用 UI 缓存补导航。
7. 现有 prestart 失败契约合理：如果父流在 DelegateStarted 前关闭，不应留下记录或占用 public id。

本轮修复后的事实：

1. 新增 `task_session_status_from_record_status(...)`，用于严格解析内存 TaskSession 状态；它不会把未知状态兜底成 running。
2. 新增 `TaskRuntime::upsert_prepared_task_registry_entry(...)`，把已准备好的 created TaskSession 记录同步到 runtime registry。
3. `prepare_task_execution_request(...)` 成功后立即同步 registry，因此 backend 收到调用时 registry 已有 created 智能体。
4. `discard_prestarted_task_record(...)` 同步删除 prestart 阶段写入的 created registry entry，保持“父流未看到启动事件则不占 public id”的语义。
5. 删除 `append_running_cached_subagent_navigation_projections(...)`，导航成员资格不再从 `subagent_surface_detail_cache` 补齐。
6. 服务层测试改为验证 render-only cache 不会创建导航成员关系。
7. runtime prestart 测试新增断言：backend 调用期间 registry 已有 created entry，prestart 失败后 registry 仍会被清空。

验证结果：

1. `rg -n "append_running_cached_subagent_navigation_projections|DelegateStarted cache state exposes|before_registry_refresh|cached state exposes|registry refresh lands|cache should expose running" crates/vulcan-service-kernel/src crates/vulcan-agent-runtime/src -S`：无命中。
2. `cargo fmt -p vulcan-agent-runtime -p vulcan-service-kernel`：通过。
3. `cargo test -p vulcan-agent-runtime invoke_spawn_agent_prestart_cancel_does_not_reserve_public_id -- --nocapture`：通过。
4. `cargo test -p vulcan-service-kernel delegate_started_cache_without_registry_does_not_create_navigation_membership -- --nocapture`：通过。
5. `cargo test -p vulcan-service-kernel detached_subagent_tests -- --nocapture`：通过。
6. `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过。
7. `cargo test -p vulcan-agent-runtime registry_mailbox -- --nocapture`：通过。
8. `cargo test -p vulcan-agent-runtime runtime_flow -- --nocapture`：通过。
9. `cargo check -p vulcan-agent-runtime -p vulcan-service-kernel --all-targets`：通过。

状态校准：

1. 智能体“创建后立即出现在列表”现在应由 runtime registry created 事实保证，不再由 WebUI detail cache 补出来。
2. detail cache 仍保留为渲染详情缓存，但不能决定导航成员资格。
3. prestart 失败仍不占用 public id，避免父会话未成功看到启动事件却留下半条智能体。
4. 后续如果仍出现关闭后残留，应继续追 registry 清理与 TaskSession 状态恢复，不应回到 UI cache 补导航。

## 2026-07-07 状态解析缺失不再猜测为终态

本轮继续审核“显示层状态事实源”，重点处理旧工具卡片 payload 与父工具卡 metadata 回挂中的状态兜底。

源码事实如下：

1. `parse_subagent_task_panel_payload(...)` 从工具卡片 payload 解析面板状态时，只要缺少 `execution_status`，旧逻辑会通过 `unwrap_or("completed")` 把它当成已完成。
2. 未知 `execution_status` 也会落入默认分支，被投影成 completed。
3. `merge_tool_display_metadata(...)` 的执行阶段排序把未知状态排在 running 与 completed 中间；如果 later metadata 带未知状态，可能覆盖已有规范终态 payload。
4. `attach_webui_task_session_metadata_to_parent_tool_message(...)` 在缺失状态时会假定 running，再把这个状态写回父工具卡 metadata。
5. `todolist` / `todo list` 已在 `vulcan-session-core::CHILD_AGENT_FORBIDDEN_TOOL_IDS` 统一禁用表中；默认智能体隐藏该工具，clone/mirror 保留工具形状但调用时返回子代理模式拒绝，不需要额外分散配置。

本轮修复后的事实：

1. `subagent_run_state_from_execution_status(...)` 改为返回 `Option<SubagentTaskRunState>`。
2. 只有规范 TaskSession 状态会被解析：`created`、`running`、`waiting_children`、`waiting_followup`、`compacting` 映射为运行中；`completed`、`archived` 映射为完成；`failed`、`stopped` 映射为各自终态。
3. 缺失或未知 `execution_status` 会让旧工具卡片 payload 解析失败，不再进入智能体面板。
4. metadata 合并排序只接受规范状态；已知状态 payload 优先于未知状态 payload，未知状态不能覆盖已有规范 payload。
5. 父工具卡 metadata 回挂缺少明确状态时直接跳过，并写 trace，不再假定 running。
6. 新增测试覆盖：缺失状态不解析、未知状态不解析、活动态映射为 running、未知状态不覆盖 completed。

验证结果：

1. `cargo fmt -p vulcan-workbench-model -p vulcan-service-kernel`：通过。
2. `cargo test -p vulcan-workbench-model metadata -- --nocapture`：通过。
3. `cargo test -p vulcan-service-kernel detached_subagent_tests -- --nocapture`：通过。
4. `cargo test -p vulcan-service-kernel mailbox_tests -- --nocapture`：通过。
5. `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`：通过；其中 `child_forbidden_tool_ids_include_todolist` 明确确认 `todolist` 禁用规则。
6. `cargo test -p vulcan-agent-runtime execution_preparation -- --nocapture`：通过。
7. `cargo check -p vulcan-workbench-model -p vulcan-service-kernel -p vulcan-agent-runtime --all-targets`：通过。

状态校准：

1. 旧工具卡片不能再凭缺失字段生成“完成态”智能体面板。
2. 父工具卡不能再把缺失状态写成 running。
3. `todolist` 已经属于智能体禁用工具范围，后续统一工具策略必须继续从 `child_agent_forbidden_tool_ids()` 读取，不要在各区域复制配置。

## 2026-07-07 mailbox 回流消息类型不再默认 completed

本轮继续审核“回流给 AI 的 runtime 消息”链路，重点查找状态缺失时是否仍会被猜成终态。源码事实如下：

1. `enqueue_webui_subagent_mailbox_input(...)` 的语义是把终态智能体结果加入父会话 mailbox。
2. 入队函数此前没有校验终态状态，只是把 `subagent_mailbox_event` 或原始 metadata 放进 `TurnInput`。
3. `webui_agent_message_from_turn_input(...)` 生成 Provider 可见 `AgentMessage` 时，如果找不到 `status` / `execution_status` / `kind`，会 `unwrap_or("completed")`。
4. 这会把结构不完整的 mailbox 事件渲染成 `FINAL_ANSWER`，让父会话把缺少终态事实的回流当成智能体最终答案。
5. 该问题发生在 AI 可见 runtime 回流链路，不是用户可见反馈记录、TaskSession 持久化或统计分组链路。

本轮修复后的事实：

1. 新增 `canonical_subagent_mailbox_terminal_status_label(...)`，只接受明确终态：`completed`、`failed`、`stopped` 及对应别名。
2. 新增 `subagent_mailbox_terminal_status_for_enqueue(...)`，在入队时从 metadata / mailbox event 中提取并规范化终态状态。
3. 缺失或非终态状态会返回 `SubagentMailboxEnqueueRejection::InvalidTerminalStatus`，不会进入父级 mailbox。
4. 成功入队的 `TurnInput` 会写入规范 `delivery_status`，后续 Provider 渲染优先使用该字段。
5. `webui_agent_message_from_turn_input(...)` 删除 `completed` 默认值；旧输入如果没有明确终态，只会渲染为普通 `MESSAGE`，不会伪装成 `FINAL_ANSWER`。

新增/更新的回归护栏：

1. `webui_subagent_mailbox_rejects_missing_terminal_status`：验证缺少明确终态的 mailbox 事件被拒绝入队。
2. `mailbox_message_rendering_does_not_default_missing_status_to_final_answer`：验证旧式无状态输入不会再渲染为最终答案。
3. `mailbox` 测试组继续覆盖 wait_agent、重复投递、停止父会话、智能体 usage 不进入父会话普通统计等路径。
4. `detached_subagent_tests` 与 `subagent_navigation` 继续验证回流、详情、导航和终态投影没有被状态校验破坏。

验证结果：

1. `cargo fmt -p vulcan-service-kernel`：通过。
2. `cargo test -p vulcan-service-kernel mailbox -- --nocapture`：通过。
3. `cargo test -p vulcan-service-kernel detached_subagent_tests -- --nocapture`：通过。
4. `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过。
5. `cargo check -p vulcan-service-kernel --all-targets`：通过。
6. `git diff --check -- crates/vulcan-service-kernel/src/session_runtime_kernel.rs crates/vulcan-service-kernel/src/agent_stream/mailbox.rs crates/vulcan-service-kernel/src/tests/mailbox_tests.rs SUBAGENT_SESSION_PARITY_ANALYSIS.md TASK_LOG.md docs/plan/20260706-01-SUBAGENT_SESSION_PARITY_REARCHITECTURE.md`：通过，仅有 LF/CRLF 提示。

状态校准：

1. 智能体回流给 AI 的 runtime 消息现在也要求明确终态事实，不能再靠缺省 completed 推断。
2. 用户可见反馈记录、AI 可见 mailbox 回流和 TaskSession 终态事件继续保持三条分离链路。
3. 后续如果新增非终态智能体消息通道，应使用独立消息语义，不应复用“终态 mailbox 入队”函数。

## 2026-07-07 host payload 状态文本不再默认 running

本轮继续沿同类状态兜底追踪到 agent-runtime 的父级 host payload 构建链路。源码事实如下：

1. `build_task_host_payload(...)` 接收 `execution_status: &str`，再调用 `task_session_status_from_execution_status_text(...)` 生成 `TaskSessionStatus`。
2. 该解析函数此前只识别 `running`、`completed`、`stopped`、`failed`，但未知字符串会默认返回 `TaskSessionStatus::Running`。
3. `build_task_host_payload(...)` 会使用该状态构建 `subagent_mailbox_event`，进而影响父级事件、mailbox 回流和 UI metadata。
4. 因此坏状态文本可能被写成 running，造成“没有运行事实却显示/记录成运行中”的风险。
5. 正常生产调用点传入的是内部枚举 `TaskExecutionStatus::as_str()` 或固定 `"failed"`，不需要依赖该默认值。

本轮修复后的事实：

1. `task_session_status_from_execution_status_text(...)` 改为返回 `Result<TaskSessionStatus, String>`。
2. 该函数只接受规范状态文本：`running`、`completed`、`stopped`、`failed`。
3. 未知状态返回错误，不再投影成 running。
4. `build_task_host_payload(...)` 使用 `expect("task host payload execution status must be canonical")` 表达内部调用不变量；后续调用点如果传错，会立刻暴露，而不是写坏状态。

新增/更新的回归护栏：

1. `task_session_status_from_execution_status_text_rejects_unknown_status`：验证未知状态不会解析成 running。
2. `registry_mailbox` 测试组继续验证 host payload 与 mailbox 事件正常包含智能体用量和终态事件。
3. `runtime_flow` 测试组继续验证任务执行、缓存、停止和恢复流程。
4. `persistence_store` 测试组继续验证 TaskSession 持久化、恢复和终态写入。

验证结果：

1. `cargo fmt -p vulcan-agent-runtime`：通过。
2. `cargo test -p vulcan-agent-runtime task_session_status_from_execution_status_text_rejects_unknown_status -- --nocapture`：通过。
3. `cargo test -p vulcan-agent-runtime registry_mailbox -- --nocapture`：通过。
4. `cargo test -p vulcan-agent-runtime runtime_flow -- --nocapture`：通过。
5. `cargo test -p vulcan-agent-runtime persistence_store -- --nocapture`：通过。
6. `cargo check -p vulcan-agent-runtime --all-targets`：通过。
7. `rg -n 'task_session_status_from_execution_status_text\(|unwrap_or\("completed"\)|unwrap_or\("running"\)|unwrap_or\("stopped"\)|_\s*=>\s*TaskSessionStatus::Running' crates/vulcan-agent-runtime/src/agent_runtime/task_runtime crates/vulcan-service-kernel/src crates/vulcan-workbench-model/src -S`：仅剩函数自身、调用点和测试命中，没有默认运行态兜底。
8. `git diff --check -- crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/status_events.rs crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/parent_events.rs crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests.rs`：通过，仅有 LF/CRLF 提示。

状态校准：

1. host payload 状态文本现在和内存 record 状态解析一样，缺失或未知都不能被静默投影成运行态。
2. 该修复不改变正常 completed / failed / stopped / running 生产路径，只删除坏输入的静默容错。
3. 后续继续审计其它字符串状态解析时，应优先改成显式 `Result` 或 `Option`，而不是切换到另一个默认状态。

## 2026-07-07 failed TaskSession 也写入终态生命周期事件

本轮继续复核 TaskSession 本地事件类型与父级 invocation 事件类型是否同构。源码事实如下：

1. `SessionEventKind::TaskTerminal` 的语义是 “Task terminal lifecycle event”。
2. `TaskSessionStatus::Completed` 与 `TaskSessionStatus::Stopped` 会映射为 `SessionEventKind::TaskTerminal`。
3. `TaskSessionStatus::Failed` 旧逻辑映射为 `SessionEventKind::RuntimeMarker`。
4. 父级侧 `TaskInvocationEventKind` 已经把 failed 明确映射为 `TaskFailed`。
5. `persist_task_record_failure(...)` 会把失败状态写入 TaskSession metadata，并追加本地事件；因此旧映射会导致失败任务在节点本地记录中不是终态生命周期事件。

本轮修复后的事实：

1. `task_session_event_kind_for_status(...)` 将 `Completed`、`Failed`、`Stopped` 统一映射为 `TaskTerminal`。
2. 活动态、压缩态、归档态等非终态仍保持 `RuntimeMarker` 或 checkpoint 语义。
3. 父级 invocation event 仍继续使用 `TaskFailed`，不改变父级镜像结构。
4. 该修复让失败任务、完成任务和停止任务在 TaskSession 自身事件日志中都使用同一类终态生命周期事件。

新增/更新的回归护栏：

1. `failed_task_session_status_uses_terminal_event_kind`：验证 failed 状态使用 `TaskTerminal`。
2. `persistence_store` 测试组继续验证 TaskSession 持久化和恢复。
3. `preparation_config` 测试组继续验证 provider-request compaction 只统计 `TaskTerminal` 事件边界。

验证结果：

1. `cargo fmt -p vulcan-agent-runtime`：通过。
2. `cargo test -p vulcan-agent-runtime failed_task_session_status_uses_terminal_event_kind -- --nocapture`：通过。
3. `cargo test -p vulcan-agent-runtime task_session_status_from_execution_status_text -- --nocapture`：通过。
4. `cargo test -p vulcan-agent-runtime persistence_store -- --nocapture`：通过。
5. `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`：通过。
6. `cargo check -p vulcan-agent-runtime --all-targets`：通过。
7. `rg -n 'TaskSessionStatus::Failed\s*=>\s*SessionEventKind::RuntimeMarker|TaskSessionStatus::Failed.*RuntimeMarker|task_session_event_kind_for_status' crates/vulcan-agent-runtime/src/agent_runtime/task_runtime -S`：确认没有 failed -> RuntimeMarker 映射残留。
8. `git diff --check -- crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/status_events.rs crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests.rs`：通过，仅有 LF/CRLF 提示。

状态校准：

1. 失败任务现在在 TaskSession 本地事件层也是终态，不再只是 runtime marker。
2. 该阶段没有改变错误文本、父级回流、mailbox kind 或 UI 文案。
3. 后续审计终态语义时，completed / failed / stopped 应作为同一组 terminal lifecycle 状态处理。

## 2026-07-07 failed TaskSession 统一进入缓存终态与防重开逻辑

本轮继续复核 TaskSession 终态集合是否在不同路径保持一致，并同步核验用户补充的 `todo list` 禁用规则。源码事实如下：

1. `crates/vulcan-session-core/src/child_agent_policy.rs` 的 `CHILD_AGENT_FORBIDDEN_TOOL_IDS` 已包含 `todolist`。
2. `child_agent_tool_is_forbidden("TODOLIST")` 已通过大小写归一测试覆盖。
3. `runtime_launch.rs` 的 default / agent 路径会隐藏统一禁用工具，mirror / clone 路径会注册但拒绝统一禁用工具。
4. `preparation_config.rs` 已验证智能体准备阶段的禁用工具清单包含 `todolist`。
5. TaskSession 状态枚举此前只有 `is_active()`，没有统一的“执行结果终态”定义。
6. 缓存读取路径 `task_session_record_has_cached_status(...)` 与 `task_session_metadata_has_cached_status(...)` 手写了 completed / stopped，漏掉 failed。
7. 持久化状态合并 `resolve_persisted_task_session_status(...)` 也只防止 completed / stopped 被迟到 active 状态重开，漏掉 failed。

可能影响：

1. 失败的智能体记录可能不会被当作已落定缓存结果，后续按同一 task id 查询时有机会重新启动后端执行。
2. failed metadata 在收到迟到 running 更新时，可能被重新投影成 running，造成 UI、注册表或落盘状态从失败回到运行中。
3. 该问题不属于提示词继承开关，也不涉及主会话统计；它是 TaskSession 终态定义分散造成的状态语义风险。

本轮修复后的事实：

1. 在 `TaskSessionStatus` 上新增 `is_terminal()`，统一表达执行结果终态。
2. `is_terminal()` 当前只包含 `Completed`、`Failed`、`Stopped`。
3. `Archived` 没有放入 `is_terminal()`，因为归档是存储生命周期状态，不是一次执行结果。
4. 缓存读取和迟到状态合并改为使用 `is_terminal()`。
5. 内存 record 缓存判断继续通过 `task_session_status_from_record_status(...)` 严格解析状态文本，未知文本不会被当作缓存终态。

新增/更新的回归护栏：

1. `task_session_status_terminal_flags_cover_all_settled_results`：验证 completed / failed / stopped 都属于终态。
2. `task_session_status_terminal_flags_exclude_active_and_archived_states`：验证活动态和 archived 不属于执行结果终态。
3. `failed_task_session_status_is_cached_terminal_state`：验证 failed metadata / record 都能被缓存读取。
4. `resolve_persisted_task_session_status_preserves_failed_terminal_state`：验证迟到 running 不能重开 failed。
5. `invoke_task_returns_cached_failed_record_without_backend_restart`：验证 failed 内存记录再次查询不会启动后端。
6. `late_running_snapshot_does_not_reopen_failed_task`：验证 failed 持久化记录收到迟到 running 后仍保持 failed。

验证结果：

1. `cargo fmt -p vulcan-agent-runtime -p vulcan-task-session`：通过。
2. `cargo test -p vulcan-task-session task_session_status_terminal_flags -- --nocapture`：通过。
3. `cargo test -p vulcan-agent-runtime failed_task_session_status -- --nocapture`：通过。
4. `cargo test -p vulcan-agent-runtime invoke_task_returns_cached_failed_record_without_backend_restart -- --nocapture`：通过。
5. `cargo test -p vulcan-agent-runtime late_running_snapshot_does_not_reopen_failed_task -- --nocapture`：通过。
6. `cargo test -p vulcan-agent-runtime runtime_flow -- --nocapture`：通过。
7. `cargo test -p vulcan-agent-runtime persistence_store -- --nocapture`：通过。
8. `cargo check -p vulcan-task-session -p vulcan-agent-runtime --all-targets`：通过。
9. `git diff --check -- crates/vulcan-task-session/src/model.rs crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_results.rs crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/status_events.rs crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests.rs crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/runtime_flow.rs crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs`：通过，仅有 LF/CRLF 提示。

状态校准：

1. `todo list` / `todolist` 已明确属于智能体禁用工具，当前源码和测试已覆盖该规则。
2. completed / failed / stopped 现在在缓存读取、防重开和本地终态事件三处语义一致。
3. 后续如新增 TaskSession 终态相关判断，应优先使用 `TaskSessionStatus::is_terminal()`，避免重新手写状态集合。

## 2026-07-07 智能体工具策略编辑器不再保留禁用工具残影

本轮沿用户补充的 `todo list` 禁用规则继续审核配置 / UI 链路。源码事实如下：

1. `vulcan-session-core` 统一禁用清单已经包含 `todolist`。
2. `vulcan-service-kernel` 的 `subagent_catalog_summary_from_catalog(...)` 会把 `child_agent_forbidden_tool_ids` 投影给 WebUI。
3. `AgentsArea.tsx` 的 `buildAgentToolPolicyOptions(...)` 已按后端投影禁用清单过滤 native / LuaSkills / MCP 工具选项，也过滤已保存但当前快照未解析到的 configured 项。
4. `skills_services.rs` 的 `apply_request_tool_policy(...)` 会在保存时调用 `sanitize_child_agent_tool_exposure(...)`，因此后端不会持久化 `todolist` 这类禁用工具。
5. 仍存在一个 UI 残影：策略表 `selectedItems` 直接来自草稿文本，旧配置中如果已有 forbidden selector，表头计数仍会把它算进去，下一次选择更新也会继续携带它。

可能影响：

1. 用户看到“仅允许 1 个工具”这类计数，但列表里实际没有对应工具行。
2. 用户可能误以为智能体仍保存了 `todolist` 等禁用工具，虽然后端保存时会剔除。
3. 这不会突破运行时禁用规则，但属于配置界面与实际策略不一致的误导。

本轮修复后的事实：

1. `AgentCapabilityPolicyTable` 增加 `selectableItemIds`，以当前 options 作为仍可选择的真实集合。
2. `selectedPolicyItems` 只保留仍在 options 中的 selector。
3. 表头计数改用 `selectedPolicyItems.length`。
4. 复选框更新也基于 `selectedPolicyItems`，因此旧 forbidden selector 不会在下一次编辑操作中继续回写草稿。
5. 后端保存清洗仍作为最终防线保留。

验证结果：

1. `npm run lint`（`apps/vulcan-code-web`）：通过。
2. `git diff --check -- apps/vulcan-code-web/src/components/features/agents/AgentsArea.tsx`：通过，仅有 LF/CRLF 提示。
3. `cargo test -p vulcan-service-kernel child_forbidden -- --nocapture`：通过。
4. `cargo test -p vulcan-service-kernel subagent_catalog_summary_projects_child_forbidden_tools -- --nocapture`：通过。
5. `cargo test -p vulcan-session-core child_agent -- --nocapture`：通过。

状态校准：

1. 现在 `todolist` 的禁用链路覆盖运行时、保存层、目录投影、前端选项和前端已选计数。
2. 该修复没有改变实际智能体运行时工具策略，只消除旧配置在 UI 上的误导性残留。
3. 后续如新增禁用工具，只需更新统一清单，WebUI 过滤、保存清洗和准备阶段策略会继续通过同一来源生效。

## 2026-07-07 mirror / clone 禁用工具返回普通拒绝结果核验

本轮审核 clone / mirror 模式下“已注册但禁用工具被调用”的实际执行语义。源码事实如下：

1. `runtime_launch.rs` 的 mirror / clone 路径会调用 `reject_registered_child_tools(...)`。
2. 该函数写入 `tool_request_rejection_reason = SUBAGENT_REJECTED_TOOL_REASON`，并把统一禁用清单写入 `tool_request_rejection_tool_ids`。
3. `execution_preparation.rs` 会把该拒绝原因和限定工具列表带入 `ExecutionPreparation`。
4. `provider_runtime_dispatch.rs` / `provider_transport_stream_runtime.rs` 会继续把拒绝配置传给 `StreamingRuntimeToolExecutor`。
5. `StreamingRuntimeToolExecutor::execute_tool(...)` 会先发 `ToolStarted`，命中拒绝后调用 `build_tool_rejection_invocation_result(...)`，再发 `ToolFinished`。
6. `build_tool_rejection_invocation_result(...)` 返回的是普通 `ToolInvocationResult`，`is_error=false`，不会让 provider turn 因策略拒绝中止。

结论：

1. 当前实现符合“调用失败不等于没调用”的要求，拒绝调用会留下工具开始和结束事件。
2. 当前实现符合“固定反馈而不是报错中止”的要求，模型会收到普通工具结果。
3. 本轮没有代码修改。

验证结果：

1. `cargo test -p vulcan-agent-runtime streaming_tool_executor_rejects_tool_requests_before_host_execution -- --nocapture`：通过。
2. `cargo test -p vulcan-agent-runtime streaming_tool_executor_rejects_only_scoped_tool_ids -- --nocapture`：通过。
3. `cargo test -p vulcan-agent-runtime task_runtime_preparation_request_preserves_tools_but_rejects_calls_for_mirror -- --nocapture`：通过。

状态校准：

1. 这条链路暂不需要修复。
2. 后续如果新增禁用工具，只要统一禁用清单更新，mirror / clone 的固定拒绝结果会同步覆盖。

## 2026-07-07 复核记录：智能体副作用隔离与 todolist 禁用

本轮按用户补充再次确认：`todo list` 对应真实工具 id `todolist`，属于智能体不可使用范围，继续由统一智能体禁用工具事实源控制。

源码链路核验结果：

1. `ExecutionRuntimePolicy::task_session()` 默认关闭 hooks、VMM、记忆召回、记忆写回与数据库投影。
2. TaskSession 配置解析之后仍会强制关闭上述主会话级副作用能力。
3. 子代理执行准备会清空请求侧 system prompt hook blocks，并由共享准备层按 runtime policy 剥离 VMM 配置。
4. TaskSession 终态报告走 `apply_task_session_execution_report_to_memory(...)` 快路径，不产生 `completion_request`。
5. detached child execution listener 对子代理 `Finished(report)` 只更新子会话内存和显示投影，不启动父会话 completed-turn hook。
6. `stream_task.rs` 只在普通主会话事件路径把 tool runtime event 记录到 conversation hook runtime。

验证结果：

1. `cargo test -p vulcan-agent-runtime prepare_execution_clears_side_effect_inputs_for_task_session_policy -- --nocapture`：通过。
2. `cargo test -p vulcan-service-kernel task_session_execution_report_fast_path_skips_completion_hook -- --nocapture`：通过。
3. `cargo test -p vulcan-service-kernel subagent_completion_feedback_keeps_parent_hook_turn_open -- --nocapture`：通过。
4. `cargo test -p vulcan-session-core child_agent_forbidden_tools_include_todolist -- --nocapture`：通过。

结论：

1. 当前无需新增代码修复。
2. 子代理反馈只负责显示、统计和 mailbox/runtime 回流，不直接触发主会话级 hook、记忆写回或 VMM postaction。
3. `todolist` 必须继续作为统一禁用工具清单成员维护，不允许在 UI 或各执行模式中复制第二份规则。

## 2026-07-07 修复记录：后台智能体导航 delta 必须绑定真实父会话

本轮继续追查关闭后导航残留。源码事实显示，底层关闭链路本身已经把 `close_agent` 目标设置为 `Archived`，普通导航也只从 `open_subagent_registry_entries_for_parent_tree(...)` 读取未归档条目。因此“切换回来还存在”的更合理根因不是 registry 被重开，而是前端导航缓存没有收到正确父会话的清理 delta。

具体问题：

1. 旧的 `emit_webui_subagent_navigation_changed_delta(...)` 只按 `workspace_snapshot.selected_session` 计算父会话。
2. detached 智能体终态、runtime ingest 和 `close_agent` 投影这些事件都可能属于非当前选中会话。
3. 当事件属于后台会话时，旧逻辑会把空导航或新导航发到当前选中会话，导致真正父会话的前端缓存没有被清理。

修复方式：

1. 新增 `subagent_navigation_snapshot_for_parent_from_cached_state(...)`，让导航投影可以显式指定父会话。
2. 新增 `emit_webui_subagent_navigation_changed_delta_for_parent(...)`，delta 的 `parent_conversation_id` 使用事件真实父会话。
3. 当前选中会话才更新服务端顶层 `workbench.subagent_navigation`；后台会话只发 delta 给前端归一化缓存。
4. `subagent_event_ingest.rs`、`agent_stream/detached_subagent.rs`、`agent_stream/projection.rs` 三条事件链路均改为传入真实父会话。

验证结果：

1. `cargo test -p vulcan-service-kernel background_empty_subagent_navigation_delta_preserves_requested_parent_conversation_id -- --nocapture`：通过。
2. `cargo test -p vulcan-service-kernel subagent_navigation_delta -- --nocapture`：通过。
3. `cargo test -p vulcan-service-kernel close_agent_cleanup_removes_closed_webui_subagent_cache_entries -- --nocapture`：通过。
4. `cargo test -p vulcan-service-kernel close_agent_closed_task_ids_are_extracted_from_matched_agents -- --nocapture`：通过。
5. `cargo test -p vulcan-service-kernel detached_subagent_tests -- --nocapture`：通过。

注意事项：

1. 完成态智能体没有关闭时仍会保留在导航内，这是当前设计，不属于本次问题。
2. 关闭后的任务目录仍会保留，导航可见性依赖 registry 归档状态和前端清理 delta，不依赖目录是否物理移动。
3. 后续所有后台智能体事件都必须显式带父会话 id，不能再从当前选中会话推断。

## 2026-07-07 复核记录：todo list 属于智能体硬禁用工具

用户补充确认 `todo list` 也属于智能体不可使用范围。本轮只做事实复核，没有修改执行代码。

源码链路核验结果：

1. 真实工具 id 是 `todolist`。
2. `crates/vulcan-session-core/src/child_agent_policy.rs` 的 `child_agent_forbidden_tool_ids()` 已把 `todolist` 放入统一智能体硬禁用工具清单。
3. default / custom / agent 智能体通过 `tool_request_hidden_tool_ids` 隐藏统一禁用工具。
4. mirror / clone 智能体通过 `tool_request_rejection_tool_ids` 保留父工具形状，但调用时返回固定拒绝结果。
5. `prepare_execution_with_tool_snapshot(...)` 会在普通工具曝光后执行 `remove_request_hidden_tools(...)`，因此 request hidden list 可以移除通用 todolist 工具。
6. WebUI 智能体配置候选通过后端 catalog summary 的 `child_agent_forbidden_tool_ids` 过滤，不维护第二份禁用列表。

验证结果：

1. `cargo test -p vulcan-session-core child_agent_forbidden_tools_include_todolist -- --nocapture`：通过。
2. `cargo test -p vulcan-agent-runtime child_forbidden_tool_ids_include_todolist -- --nocapture`：通过。
3. `cargo test -p vulcan-agent-runtime request_hidden_tools_remove_universal_todolist_after_exposure -- --nocapture`：通过。
4. `cargo test -p vulcan-service-kernel subagent_catalog_summary_projects_child_forbidden_tools -- --nocapture`：通过。

结论：

1. 当前 `todo list` 禁用链路已经覆盖共享事实源、执行准备、最终工具快照和 WebUI 配置投影。
2. 本轮无需新增代码修复。
3. 后续新增智能体禁用工具时，只改统一事实源，不能在 UI、spawn 参数解析或各执行模式中复制第二份规则。

## 2026-07-07 修复记录：未命名智能体兜底标签不再显示子代理

本轮继续按“用户可见界面统一使用智能体”的规则复核命名残留。

源码链路核验结果：

1. WebUI i18n 中智能体相关中文文案已经使用“智能体”。
2. 非测试代码中仍存在大量 `subagent` 内部协议名、日志标签和中文注释，这些不是用户可见文案，不应在本阶段改名。
3. `task_session_tool_display_label(...)` 会为任务卡片、导航和详情面板提供共享用户可见标签。
4. 该函数在所有可用名称都缺失时仍返回“子代理”，这是一个真实用户可见兜底残留。

修复方式：

1. 将最终兜底标签改为“智能体”。
2. 新增 `task_session_display_label_uses_agent_fallback`，防止兜底标签回退。

验证结果：

1. `cargo fmt -p vulcan-workbench-model`：通过。
2. `cargo test -p vulcan-workbench-model task_session_display_label_uses_agent_fallback -- --nocapture`：通过。
3. `cargo test -p vulcan-workbench-model subagent::tests::metadata -- --nocapture`：通过。
4. 非测试代码用户可见中文字符串字面量搜索不再命中“子代理”或“子智能体”。
5. `git diff --check -- crates/vulcan-workbench-model/src/subagent/metadata.rs crates/vulcan-workbench-model/src/subagent/tests/metadata.rs`：通过，仅有 LF/CRLF 提示。

结论：

1. 未命名智能体任务卡片、导航行和右侧详情标题不会再兜底显示“子代理”。
2. 内部协议字段、类型名、日志标签仍保留 `subagent`，后续如需迁移应作为单独协议迁移任务处理。

## 2026-07-07 复核记录：stale listener 导航终态来源审计与测试语义清理

本轮继续审计服务层导航里“终态 workspace phase 清理 stale listener”的路径，避免 `SessionRunPhase::Ready` 被误读为能把智能体状态猜成 completed。

源码事实如下：

1. `webui_subagent_task_has_live_run_for_phase(...)` 只判断是否还有 live run：TaskRuntime 仍在跑则为 live；否则只有 listener 存在且 workspace phase 仍是 active 时才视为 live。
2. `SessionRunPhase::Ready` 只会让陈旧 listener 不再被当成 live run，不会生成 completed 状态。
3. 导航成员与状态来自 `authoritative_subagent_tree_projection_from_registry(...)`，也就是 TaskRuntime 的 open registry。
4. `subagent_task_panel_state_from_registry_entry(...)` 把 `SubagentStatus::Completed / Failed / Stopped / Archived` 映射成 UI run state，详情缓存只补描述、结果文本和 conversation detail。
5. 原测试 `terminal_child_phase_marks_stale_running_subagent_completed_in_navigation` 的 completed 来自测试 fixture 中的 `SubagentTaskRunState::Completed`，不是由 `Ready` 推断出来的。

调整方式：

1. 将误导性的测试名改为 `registry_terminal_status_remains_visible_after_stale_listener_is_suppressed`。
2. 将测试注释改为“registry 终态在 stale listener 被抑制后仍可见”，不再描述成 terminal phase 把状态标记为 completed。
3. 不修改运行代码，因为本轮未发现该路径存在状态猜测或 UI 缓存补丁。

验证结果：

1. `cargo test -p vulcan-service-kernel registry_terminal_status_remains_visible_after_stale_listener_is_suppressed -- --nocapture`：通过。
2. `cargo test -p vulcan-service-kernel subagent_navigation_tests -- --nocapture`：通过，22 个测试通过。

结论：

1. 服务层导航没有通过 `SessionRunPhase::Ready` 伪造 completed。
2. 未关闭的完成/失败/停止智能体仍属于 open registry，会继续在导航中可见；关闭后归档为 archived 才退出默认导航。
3. 后续若 UI 仍出现“关闭后仍在列表”，应继续追 `close_agent` 是否写入 archived、导航 delta 是否带真实父会话、前端归一化缓存是否收到删除，而不是回到 workspace phase 猜状态。

## 2026-07-07 复核记录：空 allow_only 工具策略读写语义固定

本轮继续处理“智能体配置里不应该可选明确禁用工具；手写允许禁用工具也等同未允许”的规则，重点核清此前文档中未确认的 `allow_only` 空列表语义。

源码事实如下：

1. `CapabilityPolicySnapshot { mode: AllowOnly, selectors: [] }` 是合法显式策略，表示不暴露普通工具。
2. `sanitize_child_agent_tool_exposure(...)` 会移除智能体硬禁用工具；如果 allow-only 中全是禁用工具，结果仍保持空 `AllowOnly`，不会折叠成 all。
3. 执行曝光层 `selector_sets_from_policy(...)` 对空 `AllowOnly` 设置 `global_allow_active = true`，因此普通工具不会被暴露。
4. 智能体执行准备阶段再通过 `tool_request_hidden_tool_ids` 或 `tool_request_rejection_tool_ids` 处理 `todolist`、ask、plan、智能体调度和 VMM 记忆工具。
5. 保存入口 `apply_request_tool_policy(...)` 与 WebUI 工具候选表都使用后端投影的 `child_agent_forbidden_tool_ids`，没有维护第二份硬禁用列表。
6. TaskSession 配置合并 `overlay_tool_exposure(...)` 通过 policy mode 判断覆盖关系，因此空 `AllowOnly` 不会因为 selectors 为空而被当成“无覆盖”。
7. sidecar 写回 `SubagentToolPolicySidecar::from_snapshot(...)` 在 mode 非 all 时会写出现代 `toolPolicy` 对象，即使 tools 为空。

调整方式：

1. 不修改运行代码。
2. 新增 `write_user_definition_preserves_empty_allow_only_tool_policy`，用真实用户定义写入和 catalog 重载验证空 allow-only 往返。

验证结果：

1. `cargo test -p vulcan-task-session write_user_definition_preserves_empty_allow_only_tool_policy -- --nocapture`：通过。
2. `cargo test -p vulcan-session-core child_agent_tool_exposure_preserves_empty_allow_only_after_sanitize -- --nocapture`：通过。
3. `cargo test -p vulcan-agent-runtime execution_tool_exposure_empty_allow_only_hides_regular_tools -- --nocapture`：通过。
4. `cargo test -p vulcan-service-kernel apply_request_tool_policy_sanitizes_child_forbidden_tools -- --nocapture`：通过。

结论：

1. 当前持久化配置、执行曝光、保存入口和 WebUI 候选表已经保持同一语义：空 allow-only 等于无普通工具，不等于 all。
2. 旧 spawn 参数 `tools` 只能表达非空允许/禁止列表，不能代表持久化配置的显式空 allow-only；后续不应用旧参数能力反推新策略语义。
3. 后续若新增工具策略字段，必须继续保持 mode 与 selectors 的组合语义，不允许只按 selectors 是否为空判断是否覆盖。

## 2026-07-07 复验补充：todolist 硬禁用经过最终工具快照确认

用户补充 `todo list` 也属于智能体不可使用范围后，本轮重新沿运行链路复验。重点不是看常规选择器，因为 `todolist` 在执行准备层属于通用原生工具，普通 allow/deny 策略会被它绕过；真正的约束必须来自智能体硬禁用链路。

源码事实如下：

1. `child_agent_forbidden_tool_ids()` 是统一事实源，当前包含 `todolist`。
2. WebUI 智能体工具候选来自 catalog summary 的 `child_agent_forbidden_tool_ids`，所以不会维护前端第二份禁用列表。
3. 保存智能体配置时，`apply_request_tool_policy(...)` 会调用 `sanitize_child_agent_tool_exposure(...)`，手写允许 `todolist` 等同未允许。
4. default / custom / agent 智能体由 `build_task_runtime_preparation_request(...)` 把 `todolist` 写入 `tool_request_hidden_tool_ids`。
5. mirror / clone 智能体为保留父工具形状，会把 `todolist` 写入 `tool_request_rejection_tool_ids`，调用时返回子智能体模式固定拒绝。
6. `prepare_execution_with_tool_snapshot(...)` 在普通工具曝光后调用 `remove_request_hidden_tools(...)`，同时移除 provider 可见 descriptors 与宿主可执行 definitions。

复验命令：

1. `cargo test -p vulcan-session-core child_agent -- --nocapture`：通过，4 个测试通过。
2. `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`：通过，22 个测试通过；锁污染测试中的 panic 输出为测试主动制造的预期场景。
3. `cargo test -p vulcan-agent-runtime request_hidden_tools_remove_universal_todolist_after_exposure -- --nocapture`：通过。
4. `cargo test -p vulcan-service-kernel child_forbidden_tools -- --nocapture`：通过，2 个测试通过。

结论：

1. `todolist` 不是仅测试或 UI 层处理，当前已贯穿共享事实源、保存清洗、配置候选、执行准备、最终工具快照。
2. 本轮无需新增运行代码。
3. 后续新增智能体禁用工具时，仍必须只改统一事实源，并通过执行准备测试确认 hidden/rejection 两条链路同步生效。

## 2026-07-07 修复记录：TaskSession 只读路径坏尾修复收口

本轮继续处理用户此前指出的 JSONL 坏文件与重启恢复问题，重点核查子任务目录读取侧是否所有入口都能容错。

源码事实如下：

1. TaskSession 追加路径已经会在写入前调用 `repair_task_segment_jsonl_files_if_needed_locked(...)`。
2. `load_events(...)` 等读取路径已经能通过 `read_segment_lines_repairing_locked(...)` 跳过坏尾，但没有统一写回两份 metadata。
3. `load_latest_event_payload(...)` 仍绕过容错读取，直接严格读取 `SegmentedEventLine`；尾行半写入时会报错。
4. `subagent_task_panel_state_detail_from_segmented_task_session_store(...)` 会把该错误 `.ok().flatten()` 掉，导致右侧详情少最新 payload。
5. `restore_persisted_subagent_registry_for_parent_tree(...)` 也依赖该 payload 恢复 last task message，读取失败会让恢复后的智能体信息变少。

调整方式：

1. 新增 `read_repaired_task_segment_lines(...)`，作为 TaskSession 读取分段行的统一入口。
2. 该入口在同一把追加锁下修复坏尾、读取修复后分段行，并在发生修复时同步写回 `metadata.json` 与 `task_metadata.json`。
3. `load_latest_event_payload(...)` 改走该入口，不再直接严格读取 JSONL。
4. `load_events(...)`、`load_task_compaction_checkpoints(...)`、嵌套 invocation 读取和分段摘要读取也改走同一入口。
5. 严格 `read_json_lines(...)` 现在只作为测试辅助保留，避免生产代码重新绕过容错入口。

验证结果：

1. `cargo test -p vulcan-task-session load_latest_event_payload_repairs_truncated_task_jsonl_tail -- --nocapture`：通过。
2. `cargo test -p vulcan-task-session -- --nocapture`：通过，40 个测试通过。
3. `cargo check -p vulcan-task-session -p vulcan-agent-runtime -p vulcan-workbench-model -p vulcan-service-kernel --all-targets`：通过，无警告。
4. `git diff --check -- crates/vulcan-task-session/src/segmented_store/invocations.rs crates/vulcan-task-session/src/segmented_store/store_impl.rs crates/vulcan-task-session/src/segmented_store/traits.rs crates/vulcan-task-session/src/segmented_store/io.rs crates/vulcan-task-session/src/segmented_store/tests.rs`：通过，仅有既有 LF/CRLF 提示。

结论：

1. 子任务目录的最新 payload、事件列表、压缩检查点、嵌套 invocation 和分段摘要读取现在共享同一个坏尾修复入口。
2. 这能避免“修复了物理 JSONL 但 UI 仍读旧 metadata”的半修复状态。
3. 后续若出现重启后右侧智能体详情缺失，应继续查是否还有新入口直接严格读取 TaskSession JSONL。

## 2026-07-07 修复记录：智能体详情恢复读取错误不得降级为空状态

本轮继续沿重启恢复链路审计，重点不是 JSONL 坏尾本身，而是坏文件修复后的调用方是否仍会吞掉读取错误。

源码事实如下：

1. `load_webui_unclosed_subagent_surface_preload(...)` 在会话水合时会用运行时注册表条目恢复未关闭智能体的右侧详情。
2. 该入口会调用 `subagent_task_panel_state_from_registry_entry_for_preload(...)`，再读取 `subagent_task_panel_state_detail_from_segmented_task_session_store(...)`。
3. 详情读取来源包括 TaskSession metadata、`state.json` 中的 `task_context`、以及最新 TaskSession event payload。
4. 修改前 `subagent_task_panel_state_detail_from_segmented_task_session_store(...)` 对三类读取都使用 `.ok().flatten()`，真实读取错误会被当成“没有详情”。
5. 修改前 `subagent_task_panel_states_from_segmented_task_session_store(...)` 对列表读取使用 `unwrap_or_default()`，读取失败时会表现成智能体列表为空。
6. 这些降级会导致界面恢复时只剩注册表轻量信息，用户看到的就是右侧过程、结果或统计变空，而不是收到明确的坏文件/读取失败信号。

调整方式：

1. `subagent_parent_ref_from_task_session_store(...)` 改为返回 `Result<ParentSessionRef, SessionStoreError>`，metadata 读取失败直接冒泡。
2. `subagent_task_panel_state_detail_from_segmented_task_session_store(...)` 改为返回 `Result<Option<SubagentTaskPanelState>, SessionStoreError>`，只有确实没有 TaskSession metadata 才是 `Ok(None)`。
3. `subagent_task_panel_states_from_segmented_task_session_store(...)` 改为返回 `Result<Vec<_>, SessionStoreError>`，不再把列表读取失败改写为空列表。
4. `load_webui_unclosed_subagent_surface_preload(...)` 在详情读取失败时返回错误，阻止预加载把损坏详情降级为 metadata-only 状态。
5. 新增 `detail_load_reports_corrupted_task_context`，用损坏的 `state.json` 验证详情读取会报错，而不是返回空详情。

验证结果：

1. `cargo test -p vulcan-workbench-model subagent -- --nocapture`：通过，33 个测试通过。
2. `cargo check -p vulcan-workbench-model -p vulcan-service-kernel --all-targets`：通过。
3. `cargo test -p vulcan-service-kernel subagent -- --nocapture`：通过，73 个测试通过。

结论：

1. 本轮修复的是恢复读取语义，不改变智能体执行、回流或统计计算。
2. 坏文件仍需要由底层修复链路处理，但调用方不能再把读取错误伪装成“没有过程/没有统计”。
3. 后续继续检查 WebUI 恢复链路时，应优先搜索 `.ok().flatten()`、`unwrap_or_default()` 这类把存储错误压成空状态的入口。

## 2026-07-07 修复记录：registry 与 TaskSession 详情父级冲突不得静默降级

本轮继续检查“唯一来源”要求，重点是恢复链路是否会在两个来源不一致时隐藏冲突。

源码事实如下：

1. `load_webui_unclosed_subagent_surface_preload(...)` 的成员关系来自 TaskRuntime open registry。
2. 右侧详情来自 TaskSession 持久化 metadata / context / 最新 payload。
3. `subagent_task_panel_state_from_registry_entry_for_preload(...)` 修改前会先读取详情，再用 `.filter(|state| state.parent_session_id == parent_session_id)` 丢掉父级不一致的详情。
4. 被丢掉后会退回 `metadata_state`，也就是 registry-only 轻量状态。
5. 这会把“registry 父级”和“持久化详情父级”冲突伪装成“没有详情”，用户看到的表现仍可能是过程/统计缺失，而日志里没有明确冲突。
6. `subagent_task_panel_states_from_segmented_task_session_store(...)` 也存在类似的尾部过滤，会把父级不一致状态从列表里静默移除。

调整方式：

1. 服务预加载读取到详情后，若详情 `parent_session_id` 与 registry parent 不一致，直接返回 `SessionStoreError`。
2. 模型层列表投影读取到状态后，若缺少直接 parent 或 parent 与请求父级不一致，也返回 `SessionStoreError`。
3. 新增 `hydration_preload_reports_registry_detail_parent_mismatch`：
   - 先创建正常 TaskSession 并恢复 registry。
   - 再修改磁盘 TaskSession metadata 的 parent。
   - 最后执行水合预加载，断言必须返回 `parent mismatch` 错误，且错误中包含 registry parent 与 persisted parent。

验证结果：

1. `cargo test -p vulcan-service-kernel hydration_preload_reports_registry_detail_parent_mismatch -- --nocapture`：通过。
2. `cargo test -p vulcan-workbench-model subagent -- --nocapture`：通过，33 个测试通过。
3. `cargo test -p vulcan-service-kernel subagent_navigation_tests -- --nocapture`：通过，23 个测试通过。
4. `cargo check -p vulcan-workbench-model -p vulcan-service-kernel --all-targets`：通过。

结论：

1. 恢复链路不再允许 registry 与 TaskSession 详情两个来源发生父级冲突后静默选择其中一个。
2. 本轮不是 UI 层补丁，而是把数据来源一致性约束提升到恢复读取边界。
3. 后续审计继续搜索“读到 A 后过滤掉再用 B”的链路，尤其是 `.filter(...)` 接 fallback 状态的写法。

## 2026-07-07 复验记录：todolist 禁用、提示词组装与统计拆分

本轮根据用户补充“todo list 也属于不可智能体使用范围”重新检查当前未提交代码，未新增运行代码。

### 事实链路

1. `crates/vulcan-session-core/src/child_agent_policy.rs` 的 `CHILD_AGENT_FORBIDDEN_TOOL_IDS` 是当前唯一禁用工具事实源，包含 `todolist`。
2. 智能体定义与手写工具策略会通过 `sanitize_child_agent_tool_exposure(...)` 清洗，显式允许 `todolist` 等同未允许。
3. default / custom / agent 模式在 `build_task_runtime_preparation_request(...)` 中走 `hide_child_tools(...)`，把 `todolist` 等父会话工具加入 `tool_request_hidden_tool_ids`。
4. mirror / clone 模式走 `reject_registered_child_tools(...)`，保留父工具 schema，但把 `todolist` 等父会话工具加入 `tool_request_rejection_tool_ids`。
5. 工具执行侧 `StreamingRuntimeToolExecutor` 会将策略拒绝构造成普通工具结果，并继续发送工具开始与工具完成事件，不会把整轮 provider turn 异常中止。
6. 提示词膨胀旧链路已经被移除：`build_task_controller_config(...)` 不再包裹父级完整 system prompt，子智能体提示由共享 prompt composer 的 `subagent_prompt_context` 与 `functional_system_prompt` 输入。
7. mirror 克隆上下文来自 `TaskInitialContextSource::CloneLatestRuntime`，模型可见 `spawn_agent` 不再接受 `fork_turns`。
8. 智能体统计已经拆成：
   - 子 TaskSession 自身终态 `TurnStats`。
   - 父会话显示用 `SubagentFeedback` 独立统计行。
   - 父会话常规 turn stats 不合并智能体 usage。
9. WebUI 摘要读取 `usage_groups`，tooltip 显示的 `body` 来自后端 `SessionTurnStats::detail_body()`，其中包含“常规 / 智能体 / 压缩”分组。

### 复验命令

1. `cargo test -p vulcan-session-core child_agent -- --nocapture`：通过，4 个测试通过。
2. `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`：通过，22 个测试通过。
3. `cargo test -p vulcan-agent-runtime tool_runtime_bridge -- --nocapture`：通过，12 个测试通过。
4. `cargo test -p vulcan-agent-runtime tool_args -- --nocapture`：通过，26 个测试通过。
5. `cargo test -p vulcan-service-kernel detached_subagent_tests -- --nocapture`：通过，25 个测试通过。
6. `cargo test -p vulcan-service-kernel mailbox_intervention_keeps_subagent_usage_out_of_parent_turn_stats -- --nocapture`：通过。

### 当前判断

1. `todolist` 禁用已贯穿配置保存、请求准备、提示词说明、工具快照与执行拒绝。
2. 当前未提交代码里，用户此前指出的“默认智能体初始输入异常膨胀”的旧源码根因已经被移除；后续若仍复现，需要抓实际 provider request preview / `ExecutionRunReport`，确认是否存在新的注入源。
3. 当前未提交代码里，智能体实时统计与恢复统计有后端测试覆盖；若 UI 仍显示 0，应优先检查该次实际 report 是否缺 `last_successful_provider_usage` 或 conversation assistant usage。
4. `policy_rejection` 不是当前有效测试过滤名；实际覆盖策略拒绝正常返回的是 `tool_runtime_bridge`。

## 2026-07-07 修复记录：分离智能体终态补丁统一使用 surface 投影

本轮继续检查“运行时过程有、终态或重启后显示不一致”的残留链路。源码事实如下：

1. `ExecutionStreamEvent::Finished` 的分离智能体路径会先调用 `apply_task_session_execution_report_to_memory_in_place(...)`，再调用 `emit_detached_child_terminal_messages_patch(...)` 给当前界面发终态消息补丁。
2. `apply_task_session_execution_report_to_memory_in_place(...)` 会把终态报告投影进 workspace 的子会话 runtime/display 结构。
3. 共享 surface 读取入口 `surface_messages_for_session_without_subagent_navigation_refresh(...)` 已经具备统一规则：子会话 workspace 显示行优先，若显示行为空则回退到 `subagent_surface_detail_cache` 中的会话详情。
4. 修改前 `emit_detached_child_terminal_messages_patch(...)` 绕过共享入口，直接调用 `display_session_messages_from_workspace(...)`。
5. 这意味着终态补丁在 workspace 显示行暂时为空但详情缓存仍有过程时，会投影成空消息，导致当前活动智能体界面收不到包含过程的终态刷新补丁。

调整方式：

1. `emit_detached_child_terminal_messages_patch(...)` 改为调用 `surface_messages_for_session_without_subagent_navigation_refresh(...)`。
2. 保持 workspace 优先，不改变正常终态报告已经写入显示行时的行为。
3. 新增私有模块测试 `detached_terminal_patch_uses_cached_surface_detail_when_workspace_rows_are_empty`，直接覆盖终态补丁函数的缓存回退边界。

验证结果：

1. `cargo test -p vulcan-service-kernel detached_terminal_patch_uses_cached_surface_detail_when_workspace_rows_are_empty -- --nocapture`：通过。
2. `cargo test -p vulcan-service-kernel detached_subagent_tests -- --nocapture`：通过，25 个测试通过。
3. `cargo test -p vulcan-service-kernel subagent_navigation_tests -- --nocapture`：通过，23 个测试通过。

结论：

1. 分离智能体终态刷新现在和会话切换、面板打开、水合恢复使用同一套 surface 消息来源规则。
2. 这次不是新增一条 UI 兜底，而是删除旧的直接 workspace 读取分叉，避免同一智能体界面在不同入口看到不同消息来源。
3. 后续继续审计时，凡是面向当前工作台界面的智能体消息读取，应优先走共享 surface 投影，不应直接读取 `display_session_messages_from_workspace(...)` 后 `unwrap_or_default()`。

## 2026-07-07 修复记录：智能体导航注册表读取失败必须显式返回错误

本轮继续按“错误不能伪装成空结果”的规则检查智能体导航与界面投影。源码事实如下：

1. `subagent_navigation_snapshot_from_cached_state(...)` 与当前活动界面选择最终都会读取 `authoritative_subagent_tree_projection_from_registry(...)`。
2. 修改前，当 `agent_runtime.task_runtime.open_subagent_registry_entries_for_parent_tree(...)` 返回错误时，投影函数只写一条 `subagent_navigation_registry_unavailable` trace，然后返回空 `Vec`。
3. 该错误来源不是“没有智能体”的业务分支，而可能是 registry 锁、root session 归属解析、resident session 列表读取等基础状态异常。
4. 空 `Vec` 会继续被上层解释成“没有未关闭智能体”，从而让刷新、水合、切换、面板打开或导航 delta 产生空导航，并可能清除当前活动智能体面板。

调整方式：

1. 将智能体导航与活动界面投影改为 `Result` 返回：
   - `active_surface_session_id_from_state(...)`
   - `active_surface_messages_from_workspace_and_state(...)`
   - `subagent_navigation_snapshot_from_state(...)`
   - `subagent_navigation_snapshot_from_cached_state(...)`
   - `subagent_navigation_snapshot_for_parent_from_cached_state(...)`
2. `authoritative_subagent_tree_projection_from_registry(...)` 不再吞掉 registry 读取错误；没有 `agent_runtime` 仍是正常空结果，runtime 存在但读取失败则返回错误。
3. `build_subagent_panel_route_response_locked(...)` 改成 `Result<SubagentPanelRouteResponse, String>`，打开/停止/关闭智能体面板时如果导航重建失败，直接返回 RPC 错误，不发空导航 delta。
4. 会话水合、工作台刷新、保留状态选择、启动投影、冷刷新持久化等调用点同步用 `?` 传播错误。
5. 测试中增加本地包装函数，只在测试断言里把正常 `Result` 展开，避免测试代码重复处理错误。

验证结果：

1. `cargo check -p vulcan-service-kernel --all-targets`：通过。
2. `cargo test -p vulcan-service-kernel subagent_navigation_tests -- --nocapture`：通过，23 个测试通过。
3. `cargo test -p vulcan-service-kernel detached_subagent_tests -- --nocapture`：通过，25 个测试通过。
4. `cargo test -p vulcan-service-kernel conversation_selection_tests -- --nocapture`：通过，11 个测试通过。
5. `cargo test -p vulcan-session-core child_agent_forbidden_tools_include_todolist -- --nocapture`：通过。
6. `cargo test -p vulcan-agent-runtime child_forbidden -- --nocapture`：通过。
7. `cargo test -p vulcan-service-kernel subagent_fixed_model_tests -- --nocapture`：通过，6 个测试通过。

结论：

1. 智能体导航不会再把 registry 读取失败降级为“无智能体”。
2. 这次修复的是实际负面影响：错误状态下 UI 会被错误空导航清理；不是单纯调整代码风格。
3. `todolist` 已确认属于统一智能体禁用工具集合，并由 session-core、agent-runtime、service-kernel 三层测试覆盖。
4. 后续继续审计应优先搜索“Err -> trace -> empty/default”的路径，尤其是会影响 UI 状态清理的投影函数。

## 2026-07-07 修复记录：智能体反馈坏统计不再写成 0-token

本轮继续按“错误不能伪装成空结果”的规则检查智能体反馈统计链路。源码事实如下：

1. 智能体完成后，父会话可见的反馈展示由 `append_webui_subagent_feedback_records_if_missing(...)` 追加。
2. 该路径从 delegate-finished metadata 的 `payload.usage_segments` 读取智能体用量，随后生成一条“智能体反馈”和一条“智能体统计”显示记录。
3. 修改前，`subagent_feedback_usage_segments_from_metadata(...)` 在 `usage_segments` 字段存在但结构解析失败时，只打印错误并返回空列表。
4. 返回空列表会让上层继续追加 0-token 统计记录。
5. 更严重的是，同一轮还会写入 `subagent_feedback_record` 去重标记，后续正确的反馈不会自动覆盖这条坏统计。

调整方式：

1. `subagent_feedback_usage_segments_from_metadata(...)` 改为返回 `Result<Vec<SessionTurnUsageSegment>, String>`。
2. metadata 没有 payload，或 payload 没有 `usage_segments` 时，仍按“没有显式用量”返回空列表。
3. `usage_segments` 字段存在但解析失败时，返回错误，不再生成 0-token 统计行。
4. `append_webui_subagent_feedback_records_if_missing(...)` 改为返回 `Result<bool, String>`。
5. `ingest_webui_subagent_runtime_event(...)` 通过 `?` 向上传播该错误，避免把坏统计写入父会话显示记录。
6. 新增测试覆盖坏 `usage_segments` 不能留下 `subagent_feedback_record` 的场景。

验证结果：

1. `cargo test -p vulcan-service-kernel malformed_subagent_feedback_usage_segments_do_not_append_zero_stats -- --nocapture`：通过。
2. `cargo test -p vulcan-service-kernel detached_subagent_tests -- --nocapture`：通过，26 个测试通过。
3. `cargo check -p vulcan-service-kernel --all-targets`：通过。
4. `git diff --check -- crates/vulcan-service-kernel/src/subagent_event_ingest.rs crates/vulcan-service-kernel/src/tests/detached_subagent_tests.rs`：通过，仅有 LF/CRLF 提示。

结论：

1. `usage_segments` 缺失和 `usage_segments` 损坏现在是两个不同语义。
2. 缺失仍允许显示空用量，用于无 provider 调用或未来显式无用量场景。
3. 损坏会显式失败，不再污染父会话显示记录，也不再留下阻止后续修复的去重标记。
4. 如果真实 UI 仍出现 0-token，需要优先检查该次 metadata 是否根本没有 `usage_segments`，或者上游 `ExecutionRunReport` 是否没有产生 usage。

## 2026-07-07 修复记录：智能体反馈按单次执行身份记录

本轮继续检查“长期智能体会话”和“单次执行反馈”是否被混用。源码事实如下：

1. `TaskExecutionRequest.task_id` 表示智能体会话身份，同一个智能体后续可以通过队列或 `send_message` 继续执行。
2. 父会话显示反馈由 `append_webui_subagent_feedback_records_if_missing(...)` 写入。
3. 修改前，反馈显示记录只按 `SUBAGENT_TASK_ID_KEY` 去重。
4. 因此同一智能体第二次完成时，会被误判为第一轮完成事件的重复投递。
5. 实际影响是父会话看不到第二次智能体反馈，也没有第二次智能体统计记录。

调整方式：

1. 在 `TaskExecutionRequest` 增加 `execution_id`，表示一次具体智能体执行。
2. `prepare_task_execution_request(...)` 为每次执行生成新的 UUID。
3. `build_task_host_payload(...)`、`task_state_event_payload(...)`、`build_subagent_mailbox_payload(...)` 都写入同一个 `execution_id`。
4. 服务层从 delegate-finished metadata 读取 `execution_id`，显示反馈按 `task_id + execution_id` 去重。
5. 缺失 `execution_id` 的完成反馈直接报错，不生成无法追踪的显示记录。
6. 新增 `subagent_feedback_dedupes_by_execution_id_not_task_id_only`，覆盖同一智能体两次执行都应显示反馈与统计。

验证结果：

1. `cargo check -p vulcan-agent-runtime -p vulcan-service-kernel`：通过。
2. `cargo test -p vulcan-agent-runtime task_runtime_publishes_subagent_mailbox_events -- --nocapture`：通过。
3. `cargo test -p vulcan-service-kernel subagent_feedback_dedupes_by_execution_id_not_task_id_only -- --nocapture`：通过。
4. `cargo test -p vulcan-service-kernel subagent_completion_appends_independent_parent_feedback_rows -- --nocapture`：通过。
5. `cargo test -p vulcan-service-kernel malformed_subagent_feedback_usage_segments_do_not_append_zero_stats -- --nocapture`：通过。
6. `cargo test -p vulcan-service-kernel delegate_finished_ingest_terminalizes_stale_running_payload -- --nocapture`：通过。
7. `cargo test -p vulcan-agent-runtime --lib task_runtime -- --list`：通过。
8. `cargo test -p vulcan-service-kernel --lib detached_subagent_tests -- --list`：通过。
9. `git diff --check -- <本轮相关文件>`：通过，仅有 LF/CRLF 提示。

结论：

1. 智能体长期会话身份与单次执行身份已经在回流显示链路中分离。
2. 同一个智能体的后续执行不会再被第一轮反馈去重吞掉。
3. 这次修复不是 UI 兜底，而是补齐 runtime 事件协议字段，并让父会话显示记录从该协议字段判断重复。
4. 后续继续审计所有“按 task_id 去重”的路径；凡是语义是单次执行、单次完成或单次统计，都必须使用执行级身份。

## 2026-07-07 修复记录：智能体 mailbox 按单次执行身份回流

本轮继续检查第 221 轮留下的同源风险：用户可见反馈已经按 `execution_id` 记录，但父会话 AI 可见 mailbox 回流仍可能按长期 `task_id` 幂等。源码事实如下：

1. `enqueue_webui_subagent_mailbox_input(...)` 会把智能体终态事件转换成父会话 `TurnInputKind::SubagentMessage`。
2. 修改前，该输入的 `input_id` 是 `subagent-mailbox-{parent_session_id}-{task_id}`。
3. `TurnInputQueue::enqueue_mailbox_input(...)` 使用 `input_id` 判断是否已经排队或已消费。
4. 同一个智能体后续通过队列或 `send_message` 再次执行时，长期 `task_id` 不变。
5. 因此第二次完成事件会被 mailbox 队列误判为重复输入，主模型收不到第二次智能体结果。
6. 这会造成父会话显示记录和模型实际上下文不一致：界面上能看到智能体返回，模型却没有获得该次回流。

调整方式：

1. `enqueue_webui_subagent_mailbox_input(...)` 必须从 `metadata.payload.execution_id` 读取单次执行身份。
2. 缺失、非字符串或空白 `execution_id` 直接返回 `InvalidExecutionId`，不写入 mailbox。
3. mailbox `input_id` 改为 `subagent-mailbox-{parent_session_id}-{task_id}-{execution_id}`。
4. mailbox payload 增加 `execution_id` 字段，用于持久化恢复、调试和 UI 查询。
5. 宿主合成的停止通知由 `build_stopped_task_session_metadata(...)` 写入稳定 `execution_id = stopped-{task_id}-{updated_at}`。
6. 测试覆盖同一 `task_id` 不同 `execution_id` 应分别入队，同一 `execution_id` 重复事件应幂等跳过。

验证结果：

1. `cargo check -p vulcan-service-kernel -p vulcan-workbench-model`：通过。
2. `cargo fmt --check`：通过。
3. `cargo test -p vulcan-service-kernel webui_subagent_mailbox_allows_same_task_with_distinct_execution_ids -- --nocapture`：通过。
4. `cargo test -p vulcan-service-kernel subagent_completion_queues_each_distinct_execution_for_same_task -- --nocapture`：通过。
5. `cargo test -p vulcan-service-kernel mailbox_tests -- --nocapture`：通过，21 个测试通过。
6. `cargo test -p vulcan-service-kernel session_kernel_tests -- --nocapture`：通过，11 个测试通过。
7. `cargo test -p vulcan-service-kernel detached_subagent_tests -- --nocapture`：通过，28 个测试通过。
8. `cargo test -p vulcan-service-kernel --lib`：通过，317 个测试通过。
9. `cargo test -p vulcan-workbench-model --lib`：通过，54 个测试通过。
10. `cargo test -p vulcan-session-core child_agent_forbidden_tools_include_todolist -- --nocapture`：通过。
11. `cargo test -p vulcan-agent-runtime child_forbidden_tool_ids_include_todolist -- --nocapture`：通过。
12. `git diff --check -- <本轮相关文件>`：通过，仅有 LF/CRLF 提示。

结论：

1. 父会话显示反馈和 AI 可见 mailbox 回流现在都以单次执行身份作为幂等单位。
2. `task_id` 继续表示智能体是谁，`execution_id` 表示这一次执行是哪一轮。
3. 智能体停止通知虽然是宿主合成事件，也必须带明确执行身份，不能成为协议例外。
4. 后续继续审计消费游标、持久化恢复、统计聚合和回流展示里仍以 `task_id` 表示单次执行的路径。

## 2026-07-07 补充分析：已渲染提示词回灌导致智能体初始上下文膨胀

用户指出新建克隆/默认智能体后，子代理 token 统计明显大于主会话，并且重启后显示层仍丢失处理过程。按真实会话目录排查后，确认其中一个独立问题是“提示词材料来源混淆”。

事实链路：

1. 问题会话的子代理 `task/*/state.json` 中，`context.system_prompt_layers[0]` 已经包含完整 provider preamble。
2. 该 preamble 内 `You are VulcanCode`、`Developer Instructions`、`Tool Guidance`、`Runtime Context`、项目规则均重复出现。
3. 主会话的 `live_context_assembly_fingerprint` 同类分区只出现一次，说明不是 AGENTS.md 或项目规则天然重复。
4. `CoreControllerConfig.system_prompt` 在当前控制器里是 provider 渲染后的最终系统前导语，不是原始可继承提示词层。
5. 旧代码将 `controller_config.system_prompt` 写入 `SessionContextDocument.system_prompt_layers`，并把 `outcome.controller_config` 直接保存进 TaskSession 内存 record。
6. 恢复时 `task_controller_config_from_context(...)` 又把持久化的 `system_prompt_layers` 放回 `CoreControllerConfig.system_prompt`。
7. 后续继续执行时，提示词组合器会把这段已经渲染过的完整 preamble 当成原始输入，再与当前 provider prompt / developer prompt / project rules 重新组合，于是形成重复。

结论：

1. 这不是 UI 统计展示错，而是子代理 runtime 状态被污染。
2. 这不是“继承上下文所以 token 高”的正常现象；默认智能体也能异常高，是因为控制器快照/持久化恢复链路带入了已渲染提示词。
3. `controller_config` 只能保存 provider/model/temperature/max_turns/extra 等控制参数，不能保存 provider-rendered prompt material。
4. 子代理继承主会话上下文时，应复制运行时对话状态和固定代理提示词快照，不应复制父回合的 `context_injection`。

本轮修复：

1. `build_task_controller_config(...)` 清空可复用控制器配置里的 `system_prompt`、`prompt_layers`、`context_injection`。
2. 新增 `task_controller_config_for_resume_snapshot(...)`，保证内存 record 与持久化恢复只保存可恢复控制器状态。
3. TaskSession context 写盘时不再写入 `controller_config.system_prompt`，只保存解析后的有效自定义提示词层。
4. 从 context 恢复控制器时不再把 `system_prompt_layers` 还原进 `CoreControllerConfig.system_prompt`。
5. 子代理启动请求清理父会话 `context_injection`。
6. `todolist` 继续纳入统一智能体硬禁用工具策略，子代理不能直接调用。

验证：

1. `cargo fmt --check`：通过。
2. `cargo test -p vulcan-agent-runtime task_runtime --lib`：通过，113 个测试通过。
3. `cargo test -p vulcan-session-core child_agent --lib`：通过，4 个测试通过。

后续仍需继续分析的范围：

1. 子代理显示记录与 provider 可见消息仍需完全拆分，回流给 AI 的消息进 runtime，用户可见查询记录独立保存。
2. 子代理统计应按“单次执行 turn”计算，不能把长期智能体累计用量混进某一轮。
3. 主/子会话的记录格式应继续收敛到同一机制，让子会话目录复制出来也能等同主会话载入。
4. 坏 JSONL/半行写入修复功能仍需作为独立恢复机制实现，本轮不修补历史坏会话。

## 2026-07-07 修复记录：主会话终态生命周期不再把坏状态默认完成

本轮继续沿“常规主会话在特定情况下也可能被打坏”的方向审计，不只看智能体回流。源码事实如下：

1. `finish_session_external_turn_lifecycle(...)` 是 provider 流结束后把终态边界事件写入会话生命周期 store 的入口。
2. 该函数从 `terminal_event.payload.status` 解析生命周期终态。
3. 修改前解析逻辑为：`completed` 记为完成，其他字符串记为失败，缺失字段默认 `completed`。
4. 当前唯一正常调用方 `ExecutionRunReport::turn_completed_boundary_event(...)` 会写入显式 `completed` / `failed`，所以普通成功/失败流当下不会触发缺失字段问题。
5. 但该函数自身接受任意 `SessionBoundaryEvent`，如果未来入口或损坏事件缺失 `payload.status`，生命周期落盘会把未知终态写成完成。

实际影响：

1. 坏终态事件会污染会话生命周期记录。
2. 后续恢复、统计或 UI 状态读取可能把异常终态当作成功完成。
3. 这与本专项要求的“未知状态必须显式失败或显式跳过，不能默认 completed/running”冲突。

调整方式：

1. 新增 `session_lifecycle_terminal_status_from_event(...)` 作为显式终态解析入口。
2. 只接受 `completed` 和 `failed`。
3. 缺失 `payload.status` 或未知状态直接返回错误，不写生命周期完成记录。
4. 新增 `session_lifecycle_terminal_status_requires_explicit_known_status`，覆盖 completed、failed、缺失 status 和未知 status。

验证结果：

1. `cargo fmt --check`：通过。
2. `cargo test -p vulcan-service-kernel session_lifecycle_terminal_status_requires_explicit_known_status -- --nocapture`：通过。
3. `cargo test -p vulcan-service-kernel session_kernel_tests -- --nocapture`：通过，12 个测试通过。
4. `cargo check -p vulcan-service-kernel --all-targets`：通过。

结论：

1. 主会话生命周期终态不再允许缺失状态默认完成。
2. 这是实际运行语义修复，不是样式优化；它防止坏终态写入成功结果。
3. 后续继续审计 `unwrap_or(Completed)`、未知状态默认化、`Err -> empty/default` 这类路径。

## 2026-07-07 修复记录：智能体详情恢复不再吞掉坏 conversation_state

本轮继续检查“右侧智能体详情缺过程”的恢复链路。源码事实如下：

1. `subagent_task_panel_state_detail_from_segmented_task_session_store(...)` 会读取 TaskSession metadata、context 和最新事件 payload。
2. `store.load_context(...)` 只能保证 `state.json` 反序列化成 `SessionContextDocument`。
3. `SessionContextDocument.conversation_state` 本身是 `serde_json::Value`，还需要在 `subagent_task_panel_state_from_task_session(...)` 中二次反序列化成 `SessionConversationState`。
4. 修改前这里使用 `.ok()`，字段结构坏时会静默变成 `None`。
5. 因此即使 `state.json` 是合法 JSON，只要内部 `conversation_state` 结构损坏，右侧过程会被当作“没有过程”而不是坏数据。

实际影响：

1. 重启或详情恢复后可能丢失智能体过程记录。
2. 系统不会暴露恢复错误，用户只能看到空详情。
3. 这会掩盖数据写入或恢复链路中的真实损坏点。

调整方式：

1. `subagent_task_panel_state_from_task_session(...)` 改为返回 `Result<Option<SubagentTaskPanelState>, SessionStoreError>`。
2. `conversation_state` 字段反序列化失败时返回包含 TaskSession id 的错误。
3. `subagent_task_panel_state_detail_from_segmented_task_session_store(...)` 直接传播该错误。
4. 列表路径仍保持 metadata-only，不新增每个子任务的 context 扫描成本。
5. 新增 `detail_load_reports_invalid_context_conversation_state`，覆盖合法 state.json 内部字段结构损坏的恢复场景。

验证结果：

1. `cargo fmt --check`：通过。
2. `cargo test -p vulcan-workbench-model detail_load_reports_invalid_context_conversation_state -- --nocapture`：通过。
3. `cargo test -p vulcan-workbench-model subagent -- --nocapture`：通过，34 个测试通过。
4. `cargo check -p vulcan-workbench-model --all-targets`：通过。
5. `cargo test -p vulcan-service-kernel subagent_navigation_tests -- --nocapture`：通过，23 个测试通过。
6. `git diff --check -- crates/vulcan-workbench-model/src/subagent/panel_state.rs crates/vulcan-workbench-model/src/subagent/store.rs crates/vulcan-workbench-model/src/subagent/tests/panel_state.rs`：通过，仅有 LF/CRLF 提示。

结论：

1. 智能体详情恢复不再把坏 `conversation_state` 字段伪装成无过程详情。
2. 这直接对应“运行时有过程，重启后过程没了”的一类实际负面影响。
3. 后续继续检查恢复链路里是否还有 `.ok()` / `unwrap_or_default()` 处理结构化字段解析失败。

## 2026-07-07 修复记录：send_message 投影不再吞掉坏任务标识

本轮继续检查智能体会话正文过程记录，重点是父会话向智能体发送消息后，右侧智能体会话是否可靠显示这条用户消息。

源码事实如下：

1. `append_webui_subagent_send_message_from_event_if_needed(...)` 只处理 `ToolFinished` 中 `host_result.kind = subagent_message` 且 `payload.tool = send_message` 的事件。
2. 该函数会把父会话发给智能体的消息追加到智能体自身会话正文，作为用户可见过程记录。
3. 修改前函数返回 `Option<SessionId>`。
4. 非相关事件、root 回流、重复投影返回 `None` 是正确语义。
5. 但已识别为 send_message payload 后，缺失或非法 `task_session_id`、缺消息文本、缺显示记录 id 也会返回 `None`。

实际影响：

1. 坏协议会被当作“没有需要追加的消息”。
2. 用户发给智能体的可见过程记录可能消失。
3. 父会话工具调用与智能体正文记录会不一致，排查时没有错误信号。

调整方式：

1. `append_webui_subagent_send_message_from_event_if_needed(...)` 改为返回 `Result<Option<SessionId>, String>`。
2. 不相关事件和重复事件继续返回 `Ok(None)`。
3. 已识别 send_message payload 的关键字段缺失或格式错误返回 `Err`。
4. `append_runtime_event_projection(...)` 用 `?` 传播错误，不再吞掉坏投影。
5. 新增 `send_message_projection_reports_invalid_task_session_id`，验证非法任务 ID 会报错且不追加半条记录。

验证结果：

1. `cargo test -p vulcan-service-kernel send_message_projection_reports_invalid_task_session_id -- --nocapture`：通过。
2. `cargo test -p vulcan-service-kernel mailbox_tests -- --nocapture`：通过，22 个测试通过。
3. `cargo check -p vulcan-service-kernel --all-targets`：通过。
4. `cargo fmt --check`：通过。
5. `git diff --check -- crates/vulcan-service-kernel/src/agent_stream/projection.rs crates/vulcan-service-kernel/src/subagent_event_ingest.rs crates/vulcan-service-kernel/src/tests/mailbox_tests.rs`：通过，仅有 LF/CRLF 提示。

结论：

1. 父会话 `send_message` 到智能体正文的显示投影不再把坏任务 ID 当作无事件。
2. 这修复的是实际过程记录缺失风险，不是 UI 文案优化。
3. 后续继续查“已识别为智能体事件后仍用 Option 吞掉关键字段损坏”的路径。

## 2026-07-07 修复记录：send_message 协议解析收口到共享模型层

本轮继续检查智能体事件协议是否存在多处手工解析。源码事实如下：

1. `invoke_send_message(...)` 生产的 host result 固定为 `kind = subagent_message`。
2. 父级发往子智能体的 payload 固定带 `tool = send_message`、`subagent_task_id`、`delivery_id`、`message` 等字段。
3. root 回流也使用 `kind = subagent_message`，但 `target = root`，不应该追加到子智能体正文显示。
4. 修改前 `append_webui_subagent_send_message_from_event_if_needed(...)` 在 service 层直接解析这些 JSON 字段。
5. `vulcan-workbench-model` 已经承载智能体 task-session 投影解析，但 send_message 显示投影协议还没有收口进去。

实际影响：

1. service 层和共享模型层会分别理解同一份智能体协议。
2. 后续如果 send_message payload 调整，容易出现一处修了、一处仍旧按旧结构解析。
3. 这类分叉正是之前“运行时有、重启后无、主会话/智能体显示不一致”的来源之一。

调整方式：

1. 新增 `SubagentSendMessageProjection`，把子任务 ID、显示记录 ID、消息正文和显示 metadata 作为结构化结果返回。
2. 新增 `subagent_send_message_projection_from_tool_metadata(...)`，由共享模型层统一解析 `subagent_message`。
3. 保留原显示记录 ID 规则：`delivery_id` 优先，其次 `tool_call_id`，最后解析标准工具返回标题。
4. service 层只调用共享解析器并追加显示行，不再手工读 payload 字段。
5. `todolist` 已确认在 `CHILD_AGENT_FORBIDDEN_TOOL_IDS` 统一硬禁用清单中，属于智能体不可直接调用的父会话工具。

验证结果：

1. `cargo test -p vulcan-workbench-model subagent_send_message_projection -- --nocapture`：通过，4 个测试通过。
2. `cargo test -p vulcan-workbench-model subagent -- --nocapture`：通过，38 个测试通过。
3. `cargo test -p vulcan-service-kernel mailbox_tests -- --nocapture`：通过，22 个测试通过。
4. `cargo check -p vulcan-service-kernel -p vulcan-workbench-model --all-targets`：通过。
5. `cargo fmt --check`：通过。
6. `cargo test -p vulcan-session-core child_agent_forbidden_tools_include_todolist -- --nocapture`：通过。
7. `git diff --check` 覆盖本轮相关文件：通过，仅有 LF/CRLF 提示。

结论：

1. `send_message` 显示投影协议的唯一解析入口已收口到共享模型层。
2. service 层不再掌握该 payload 的字段布局，只消费结构化投影事实。
3. 后续应继续处理 `close_agent` / `subagent_control` 这类仍在 service 层解析 host result 的路径。

## 2026-07-07 修复记录：close_agent 控制协议解析收口到共享模型层

本轮继续处理 service 层本地解析智能体 host result 的问题。源码事实如下：

1. `invoke_close_agent(...)` 生产的 host result 固定为 `kind = subagent_control`，payload 中 `tool = close_agent`。
2. `matched_agents` 来自 `archive_subagent_registry_entries(...)`，表示已经归档的 registry 条目。
3. `stopped_subagent_task_ids` 来自 `stop_task_tree(...)`，表示关闭过程中被停止的活动 task 节点。
4. 运行时投影入口此前把两者合并用于 UI 缓存清理，但解析逻辑在 service 层本地实现。
5. 面板关闭入口此前只读 `matched_agents`，同样在 service 层本地拆 JSON。
6. 运行时投影入口修改前使用多层 `filter_map`，非法任务 ID 会被静默丢弃。

实际影响：

1. close_agent 的同一份协议存在两处 service 解析实现。
2. 一个入口读 matched + stopped，另一个入口只读 matched，语义差异没有结构化表达。
3. 坏任务 ID 被丢弃时，UI 清理可能残留已关闭或已停止的智能体项，且没有错误信号。

调整方式：

1. 新增 `SubagentCloseAgentProjection`，明确包含 `matched_task_ids`、`stopped_task_ids`、`affected_task_ids`。
2. 新增 `subagent_close_agent_projection_from_tool_metadata(...)`，供运行时事件投影使用。
3. 新增 `subagent_close_agent_projection_from_host_result(...)`，供直接 close_agent 调用结果使用。
4. 运行时投影入口使用 `affected_task_ids`，保持原有 UI 清理并集语义。
5. 面板关闭入口使用 `matched_task_ids`，保持“关闭的 registry 条目”语义。
6. 非 close 的 `subagent_control`，例如 `interrupt_agent`，明确返回 `None`。
7. 非 `subagent_control` 的无关 host result 即使没有 payload，也返回 `None`，避免误报 close_agent 错误。

验证结果：

1. `cargo test -p vulcan-workbench-model subagent_close_agent_projection -- --nocapture`：通过，4 个测试通过。
2. `cargo test -p vulcan-service-kernel close_agent_closed_task_ids_are_extracted_from_matched_agents -- --nocapture`：通过。
3. `cargo test -p vulcan-workbench-model subagent -- --nocapture`：通过，42 个测试通过。
4. `cargo test -p vulcan-service-kernel mailbox_tests -- --nocapture`：通过，22 个测试通过。
5. `cargo check -p vulcan-service-kernel -p vulcan-workbench-model --all-targets`：通过。
6. `cargo fmt --check`：通过。
7. `git diff --check` 覆盖本轮相关文件：通过，仅有 LF/CRLF 提示。

结论：

1. `close_agent` 控制协议的解析归属已收口到共享模型层。
2. “已关闭”和“被停止”两个概念被结构化区分，避免 UI 清理和面板关闭服务继续各自解释 JSON。
3. 后续继续查剩余 `subagent_control`、`host_result` 解析点，尤其是 list/wait/interrupt 这类仍可能有旁路语义。

## 2026-07-07 修复记录：task mirror 状态 metadata 同步收口到共享模型层

本轮继续审计 service 层是否还维护智能体 task-session metadata 内部结构。源码事实如下：

1. `task_mirror_status_projection.rs` 会用 task mirror 修复父级 task 卡片状态。
2. 修改前该文件本地实现 `task_session_display_metadata_update(...)`。
3. 该函数会同时写入顶层 `display_state`、`tool_display_state`、直接 `payload.execution_status`、`host_result.payload.execution_status`。
4. 修改前 `merge_task_mirror_tool_display_metadata(...)` 也在 service 层，用于实时 running 覆盖陈旧终态 payload。
5. 这些都是 task-session metadata 结构规则，不是 service 层运行时调度事实。

实际影响：

1. workbench-model 和 service-kernel 共同维护 task-session metadata 结构，协议归属分裂。
2. 后续如果 task-session metadata 结构调整，task mirror 可能继续按旧字段更新 host_result payload。
3. 这会影响父级 task 卡片状态修复，可能重新出现“运行中/已结束显示不一致”的问题。

调整方式：

1. 将 `task_session_display_metadata_update(...)` 迁移到 `vulcan-workbench-model/src/subagent/metadata.rs`。
2. 将 `merge_task_mirror_tool_display_metadata(...)` 迁移到共享模型层。
3. service 层删除本地实现，仅保留 `normalized_task_mirror_execution_status(...)`，因为它依赖当前进程 live running 状态。
4. 新增测试验证直接 payload 与 host_result payload 的状态别名同步。
5. 新增测试验证 live running override 不会被陈旧 completed payload 覆盖。

验证结果：

1. `cargo test -p vulcan-workbench-model task_session_display_metadata -- --nocapture`：通过，2 个测试通过。
2. `cargo test -p vulcan-service-kernel stop_task_mirror_tests -- --nocapture`：通过，9 个测试通过。
3. `cargo test -p vulcan-workbench-model subagent -- --nocapture`：通过，44 个测试通过。
4. `cargo check -p vulcan-service-kernel -p vulcan-workbench-model --all-targets`：通过。
5. `cargo fmt --check`：通过。
6. `git diff --check` 覆盖本轮相关文件：通过，仅有 LF/CRLF 提示。

结论：

1. task mirror 状态修复不再由 service 层手工维护 task-session metadata 内部字段。
2. service 层只保留运行时事实判断，metadata 写入协议归属到 workbench-model。
3. 后续继续查显示层基于字符串判断智能体显示记录类型的路径，尤其是显示去重逻辑。

## 2026-07-07 修复记录：智能体 send_message 显示记录类型判断收口到共享模型层

本轮继续检查父会话显示拼装是否仍有智能体协议旁路。源码事实如下：

1. `send_message` 显示投影已经由 `vulcan-workbench-model` 统一解析。
2. `conversation_display.rs` 修改前仍保留本地 `session_message_is_display_only_agent_message(...)`。
3. 该函数直接用 `subagent_message` + `send_message` 字符串判断 display note 类型。
4. 这个判断用于父会话显示去重，属于智能体显示记录协议的一部分。

实际影响：

1. 同一类 `send_message` 显示记录在“生成/解析”和“显示去重”两个地方各自解释。
2. 后续如果 display note metadata 结构调整，service 显示层可能继续按旧字段判断。
3. 这会增加父会话显示与子智能体显示状态再次分叉的风险。

调整方式：

1. 新增共享 `session_message_is_subagent_send_message_display_note(...)`。
2. 该函数只识别 `DisplayNote` 且 metadata 中 `kind = subagent_message`、`tool = send_message` 的记录。
3. `conversation_display.rs` 改为调用共享模型层分类器。
4. 删除 service 层本地字符串判断函数。
5. 新增测试覆盖规范记录、普通非显示记录、其他工具记录。

验证结果：

1. `cargo test -p vulcan-workbench-model session_message_is_subagent_send_message_display_note -- --nocapture`：通过，3 个测试通过。
2. `cargo test -p vulcan-service-kernel subagent_surface_conversation_state_uses_canonical_display_only_agent_messages -- --nocapture`：通过。
3. `cargo test -p vulcan-service-kernel prompt_runtime_state_tests -- --nocapture`：通过，21 个测试通过。
4. `cargo test -p vulcan-session-core child_agent_forbidden_tools_include_todolist -- --nocapture`：通过。
5. `cargo test -p vulcan-workbench-model subagent`：通过，47 个测试通过。
6. `cargo check -p vulcan-service-kernel -p vulcan-workbench-model --all-targets`：通过。
7. `cargo fmt --check`：通过。
8. `git diff --check` 覆盖本轮相关文件：通过，仅有 LF/CRLF 提示。

结论：

1. 智能体 send_message display note 的类型判断已经归属共享模型层。
2. service 显示层只消费共享分类结果，不再直接理解 `subagent_message` / `send_message` 字段组合。
3. 后续应继续审计 `subagent_feedback` 与 `subagent_feedback_stats` 这类父级可见回流显示记录。

## 2026-07-07 修复记录：智能体反馈显示记录协议收口到共享模型层

本轮继续分析父会话可见智能体回流记录。源码事实如下：

1. `subagent_event_ingest.rs` 会在 delegate-finished 后追加两条父会话显示记录。
2. 第一条是 `record_kind = DisplayNote`、`kind = subagent_feedback`，用于显示智能体反馈正文。
3. 第二条是 `record_kind = TurnStats`、`kind = subagent_feedback_stats`，用于显示智能体自身消耗。
4. 这两条记录明确不进入 provider 上下文，只用于用户在主会话查看智能体回流和统计。
5. 修改前 service 层本地维护 `execution_id` 读取、去重、usage segment 解析、状态映射和统计聚合。
6. 统计数据结构在 `vulcan-workbench-model`，但完整 usage segment 聚合函数在 `vulcan-agent-runtime`。

实际影响：

1. 父会话显示记录协议继续由 service 层手工拼接，和模型层协议归属不一致。
2. 反馈正文记录和反馈统计记录可能随着后续修改再次与持久化/重载解析分叉。
3. usage segment 解析出错时如果处理路径不一致，可能重新出现 0-token 统计或丢失统计的问题。
4. 模型层无法独立生成 `SubagentFeedback` 统计，导致 service 层继续持有本不该持有的结构知识。

调整方式：

1. 在 `SessionTurnStats` 新增 `merge_usage_segment(...)`，让统计模型自身具备完整聚合能力。
2. runtime 层 `merge_usage_segment_into_turn_stats(...)` 改为兼容转调模型方法。
3. 在 `SubagentTaskRunState` 新增 `metadata_key(...)`，统一反馈 metadata 的运行状态字符串。
4. 新增 `SubagentFeedbackDisplayRecords`，结构化返回反馈正文行和反馈统计行。
5. 新增 `subagent_feedback_display_records_from_tool_metadata(...)` 统一构建两条父会话显示记录。
6. 新增 `subagent_feedback_execution_id_from_tool_metadata(...)` 和 `subagent_feedback_record_matches(...)`，让 service 保持先按 execution id 去重的执行顺序。
7. service 层删除本地构建和解析函数，只负责追加共享模型层生成的记录。

验证结果：

1. `cargo test -p vulcan-workbench-model subagent_feedback`：通过，4 个测试通过。
2. `cargo test -p vulcan-service-kernel subagent_feedback`：通过，3 个测试通过。
3. `cargo test -p vulcan-service-kernel subagent_completion_appends_independent_parent_feedback_rows`：通过。
4. `cargo test -p vulcan-agent-runtime turn_stats`：通过，13 个测试通过。
5. `cargo test -p vulcan-service-kernel prompt_runtime_state_tests`：通过，21 个测试通过。
6. `cargo test -p vulcan-workbench-model subagent`：通过，51 个测试通过。
7. `cargo test -p vulcan-service-kernel mailbox_tests`：通过，22 个测试通过。
8. `cargo test -p vulcan-workbench-model turn_stats`：通过，2 个测试通过。
9. `cargo check -p vulcan-service-kernel -p vulcan-workbench-model -p vulcan-agent-runtime --all-targets`：通过，无警告。
10. `cargo fmt --check`：通过。
11. `git diff --check` 覆盖本轮相关文件：通过，仅有 LF/CRLF 提示。

结论：

1. 父会话可见的智能体反馈正文记录与统计记录已由共享模型层统一生成。
2. service 层不再维护 `subagent_feedback` / `subagent_feedback_stats` 的字段结构。
3. 子代理回流统计继续与父会话常规统计分离，且 `SubagentFeedback` 不计入父会话目标预算。
4. 后续继续检查 service 层剩余智能体状态字符串与显示协议是否还能继续收口。

## 2026-07-07 修复记录：智能体运行状态字符串收口到模型层

本轮继续收口剩余智能体字符串协议。源码事实如下：

1. `SubagentTaskRunState` 是共享模型层的运行状态枚举。
2. 前一阶段已经为它新增 `metadata_key(...)`。
3. service 层仍有 `subagent_run_state_key(...)`，重复映射 running/completed/stopped/failed。
4. 该函数只用于 WebUI summary 与 trace 字段，不涉及 runtime 状态来源判断。

实际影响：

1. 同一个状态枚举存在两处字符串映射。
2. 后续如果状态文本扩展或修正，service 层可能继续输出旧值。
3. 这类重复虽然小，但会让“主会话和智能体显示记录机制一致”的目标继续留下旁路。

调整方式：

1. `subagent_event_ingest.rs` 改为调用 `normalized_state.run_state.metadata_key()`。
2. `workbench_surface_projection.rs` 改为调用 `state.run_state.metadata_key()`。
3. 删除 service 层 `subagent_run_state_key(...)`。
4. 删除 `lib.rs` 对该函数的导入。

验证结果：

1. `cargo test -p vulcan-service-kernel subagent_navigation`：通过，28 个测试通过。
2. `cargo check -p vulcan-service-kernel -p vulcan-workbench-model --all-targets`：通过。
3. `cargo fmt --check`：通过。
4. `rg -n "subagent_run_state_key\\(" crates/vulcan-service-kernel/src crates/vulcan-workbench-model/src`：无匹配。
5. `git diff --check` 覆盖本轮相关文件：通过，仅有 LF/CRLF 提示。

结论：

1. 智能体运行状态字符串映射已收口到共享模型枚举。
2. service 层继续负责 registry/runtime 来源状态转换，但不再维护最终显示字符串表。
3. 后续继续查剩余非测试路径中的智能体协议构造。

## 2026-07-07 修复记录：task mirror metadata update 构建收口到共享模型层

本轮继续检查非测试路径中直接构造 task-session metadata 的位置。源码事实如下：

1. `task_mirror_status_projection.rs` 从 `SessionTaskMirrorSnapshot` 读取 task id、line count、updated_at。
2. service 层根据 live runtime 判断出最终 `execution_status`。
3. 修改前 service 层直接构造 spawn-agent task-session metadata update。
4. 这份 metadata update 的字段结构属于共享 task-session 显示协议。

实际影响：

1. task mirror 的 metadata update 结构与模型层其他 task-session metadata 构建逻辑分离。
2. 后续如果 task-session metadata 字段调整，task mirror 修复路径可能继续写旧结构。
3. service 层继续直接依赖 `SUBAGENT_TASK_KIND`，不利于协议归属集中。

调整方式：

1. 新增 `TaskSessionMirrorDisplayMetadataUpdate`，包含匹配 payload 与 metadata update。
2. 新增 `task_session_mirror_display_metadata_update(...)`，由共享模型层构建 task mirror update。
3. `task_mirror_status_projection.rs` 改为消费共享构建结果。
4. service 生产代码不再直接导入 `SUBAGENT_TASK_KIND`。
5. 新增模型层测试覆盖 payload 字段与 display state 同步。

验证结果：

1. `cargo test -p vulcan-workbench-model task_session_mirror_display_metadata`：通过。
2. `cargo test -p vulcan-service-kernel stop_task_mirror_tests`：通过，9 个测试通过。
3. `cargo check -p vulcan-service-kernel -p vulcan-workbench-model --all-targets`：通过，无警告。
4. `cargo fmt --check`：通过。
5. `git diff --check` 覆盖本轮相关文件：通过，仅有 LF/CRLF 提示。

结论：

1. task mirror 修复父级 task 卡片的 metadata update 构建已收口到共享模型层。
2. service 层仍保留读取 mirror、判断 live running、查找并应用 update 的职责。
3. 后续继续审计非测试路径中剩余智能体协议字段。

## 2026-07-07 修复记录：TaskSession 请求清理父级动态运行态

本轮继续分析“默认智能体初始输入异常膨胀、子代理统计与主会话污染”的底层来源。源码事实如下：

1. 子代理运行请求由 `build_task_runtime_preparation_request(...)` 从父会话 `ExecutionPreparationRequest` 克隆后改写。
2. 该函数已经覆盖了 `session_id`、`execution_scope`、`prompt`、`conversation_state`、`tool_exposure_override` 等核心字段。
3. 修改前仍有若干父回合动态字段会跟随模板保留，包括：
   - `tool_request_rejection_*`
   - `tool_request_hidden_tool_ids`
   - `boundary_events`
   - `runtime_system_updates`
   - `session_goal_context`
   - `session_plan_context`
   - `embedded_turn_usage_segments`
4. `runtime_system_updates` 会在 Provider 历史投影时再次注入；`session_goal_context` 与 `session_plan_context` 会进入提示词组合器；`embedded_turn_usage_segments` 属于统计而不是子代理输入。

实际影响：

1. 默认智能体即使使用 fresh conversation snapshot，也可能继承父会话的动态 system update、计划/目标上下文和统计片段。
2. clone/mirror 智能体已经从父 runtime snapshot 复制上下文，若继续保留父模板 `runtime_system_updates`，会有再次叠加或重复注入风险。
3. 父请求临时工具拒绝/隐藏状态会污染子代理工具策略，使“默认隐藏、mirror 保留但拒绝”的子代理规则不再完全由子代理策略本身决定。
4. 这可以解释“默认子代理输入 token 反而比主会话更大”的一类现象：不是会话显示记录变多，而是父模板附带的动态运行态没有被隔离。

调整方式：

1. 在 `runtime_launch.rs` 新增 `clear_parent_request_runtime_state_for_task_session(...)`。
2. 子代理请求覆盖完 resolved config 后，先统一清理父级动态运行态，再调用 `apply_child_tool_policy(...)`。
3. 清理内容包括父级工具临时策略、hook block、context injection、boundary events、runtime system updates、session goal/plan context、embedded usage segments。
4. 新增回归测试 `task_runtime_preparation_request_clears_parent_transient_runtime_state`，专门构造带污染字段的父请求，并断言子请求只保留子代理自身策略。

验证结果：

1. `cargo test -p vulcan-agent-runtime task_runtime_preparation_request_clears_parent_transient_runtime_state`：通过。
2. `cargo test -p vulcan-agent-runtime preparation_config`：通过，24 个测试通过。
3. `cargo check -p vulcan-agent-runtime --all-targets`：通过。
4. `cargo test -p vulcan-agent-runtime task_runtime_preparation_request`：通过，6 个测试通过。
5. `cargo test -p vulcan-agent-runtime child_forbidden_tool_ids_include_todolist`：通过。
6. `cargo fmt --check`：通过。
7. `git diff --check` 覆盖本轮相关文件：通过，仅有 LF/CRLF 提示。
8. 一次 `cargo test` 多过滤词命令因 Cargo 参数用法错误失败，随后按单过滤前缀重跑通过；该失败不属于代码失败。

结论：

1. TaskSession 启动请求不再继承父会话的临时动态运行态。
2. 默认智能体仍隐藏父会话保留工具，且 `todolist` 已包含在统一禁用清单中。
3. mirror/clone 智能体仍保留禁用工具 schema 并在调用时返回固定策略拒绝，不中止运行。
4. 后续继续审计子代理提示词组装与 runtime snapshot 捕获是否还存在重复上下文来源。

## 2026-07-07 修复记录：父回合统计不再聚合智能体用量

本轮按用户提供的实际会话继续核对统计链路。源码与会话事实如下：

1. 实际会话 `2026-07-06-fb447b5e-8640-408f-bf17-21d9c9fb254a` 中，`subagent_task_completed` 事件已经携带智能体自己的 `usage_segments`。
2. 同一会话的主回合 `TurnStats` 又把 `assistant_response` 和 `subagent_task` 两段合并到顶层总数，导致主会话输入、输出、总 token 被智能体消耗放大。
3. 独立回流后还会产生一条由 `<agent_message>` 触发的父级 API 回合，这条回合可能先出现 0 token 统计，再被智能体用量补入。
4. `SessionTurnStats` 已有 `usage_groups`，可以表达“常规/智能体”分离，但共享构建器和终态 report 合并路径仍把 `subagent_task` 加进顶层聚合。

实际影响：

1. 主会话统计会被智能体执行消耗污染，用户看到的主会话 token 不再代表主模型本轮真实消耗。
2. tooltip 和统计 chip 可能同时出现“顶层总数包含智能体”和“分组里还有智能体”，口径不清。
3. goal/budget 统计虽然已有分组排除逻辑，但顶层显示与分组显示不一致，容易误判预算或性能。
4. 子代理面板自己的统计不应受影响，它仍应聚合自己的一个 turn 消耗。

调整方式：

1. 新增 `merge_explicit_usage_segment_into_turn_stats(...)`，集中处理显式用量分段的父回合归属规则。
2. 当统计对象是 `UserTurn` 且分段是 `SubagentTask` 时，只合并到 `usage_groups`，不修改顶层 provider/tool/token 聚合字段。
3. `build_session_turn_stats_from_state_with_segments(...)` 改为使用该共享规则。
4. `merge_terminal_explicit_usage_segments(...)` 同步使用该共享规则，避免终态 report 路径继续污染主统计。
5. 子代理反馈统计仍由 `SubagentFeedback` 行聚合自己的分段，不改变子代理面板显示。

验证结果：

1. `cargo test -p vulcan-agent-runtime explicit_subagent_segments_group_without_polluting_parent_turn_totals`：通过。
2. `cargo test -p vulcan-agent-runtime terminal_report_keeps_embedded_subagent_usage_out_of_parent_totals`：通过。
3. `cargo test -p vulcan-agent-runtime turn_stats`：通过，14 个测试通过。
4. `cargo test -p vulcan-agent-runtime session_event_runtime`：通过，17 个测试通过。

结论：

1. 父级主回合顶层统计现在只代表主会话自身消耗。
2. 智能体消耗继续保留在 `usage_segments` 与 `usage_groups` 中，UI 可以继续展示“智能体”分组。
3. 独立智能体反馈统计仍由专属反馈记录承载，不再需要把智能体消耗塞进主回合总数。
4. 后续继续检查 `<agent_message>` 回流显示/写入层，确认内部回流记录不会作为普通用户消息暴露。

## 2026-07-07 独立 mailbox 回流仍退化成普通 prompt 的根因记录

本轮继续追踪用户反馈的“主会话也会被打坏、回流应该直接进入 runtime 而不进显示记录”。源码证据显示，当前系统内同时存在两条智能体回流链路：

1. 运行中父会话干预链路：`append_webui_mailbox_intervention_messages(...)` 会从 current-turn buffer 取出 `TurnInputKind::SubagentMessage`，再调用 `VulcanCoreController::append_agent_message(...)` 写入类型化 `ConversationPayload::AgentMessage`。
2. 空闲父会话独立唤醒链路：`maybe_start_webui_mailbox_turn_if_ready(...)` 先调用 `webui_session_mailbox_input_from_kernel(...)`，把同一批 `SubagentMessage` 渲染成 `<agent_message>...` 字符串，然后以 `prompt` 形式启动一个 `SessionMailbox` 回合。

第一条链路符合目标语义：AI 可见消息进入 runtime，显示记录独立处理，不伪装成用户输入。

第二条链路仍存在结构性问题：

1. `begin_shared_prompt_turn(...)` 虽然设置了 `append_canonical_user_message: false` 和 `append_visible_user_prompt_record: false`，但它仍把 `<agent_message>` 字符串放入 `ExecutionPreparationRequest.prompt`。
2. `build_runtime_stream_launch_payload(...)` 会把该 prompt 作为 `effective_prompt` 传给 Provider 执行层。
3. `execute_host_controlled_stream(...)` 最终调用 `VulcanCoreController::run_user_turn_streaming_with_checkpoints_and_interventions(...)`。
4. `vulcan-core` 的 run loop 收到非空 prompt 后会执行 `append_user_turn(...)`，除非最后一条规范消息已经是同样的 `UserText` 或尾部连续 `AgentMessage` 渲染文本。
5. 空闲独立唤醒链路在进入 run loop 前并没有把 mailbox 项写成类型化 `AgentMessage`，因此 `<agent_message>` 字符串有机会被追加为普通 `UserText`。
6. `start_webui_session_kernel_turn(... ActiveTurnKind::MailboxTriggered)` 本身会把 mailbox 输入移动到 current-turn buffer，但 `maybe_start_webui_mailbox_turn_if_ready(...)` 随后立刻调用 `drain_current_turn_inputs()`，提前消费掉结构化输入。
7. 这样运行时干预器后续再执行 `append_webui_mailbox_intervention_messages(...)` 时已经拿不到原始 `SubagentMessage`，只能依赖前面那段字符串 prompt。

这解释了当前多类现象：

1. 主会话落盘可能出现 `<agent_message>` 或其等价内容被普通用户回合化。
2. 显示层虽然有过滤内部 `<agent_message>` 的逻辑，但底层 runtime / JSONL 已经被污染，重启、重放、坏文件修复时仍可能出现差异。
3. 独立回流可能产生一条额外父级 API 回合，其统计再与智能体用量补丁互相叠加，造成“空统计 / 重启后统计不同 / 父会话统计异常”。
4. 这条路径与运行中干预链路行为不同，所以会出现“有时正常、有时主会话被打坏”的不稳定表现。

长期修复方案：

1. `SessionMailbox` 不应把智能体回流渲染成 prompt 字符串来启动父会话。
2. `ExecutionPreparationRequest` 或其上层启动输入需要支持结构化 mailbox 输入，例如 `provider_visible_turn_inputs: Vec<TurnInput>` 或更窄的 `mailbox_agent_messages`。
3. mailbox 触发回合启动后，current-turn `SubagentMessage` 应保留到 provider-loop 干预阶段，由统一函数追加为 `ConversationPayload::AgentMessage`。
4. 如果需要在 provider loop 前完成注入，应在构建 core snapshot 时追加类型化 `AgentMessage`，而不是写入 prompt。
5. mailbox 触发回合的 `prompt` 不应承载 `<agent_message>`；若底层仍要求非空 prompt，需要新增明确的内部执行目的或 controller API，而不是使用可落盘的伪用户文本占位。
6. 用户可见反馈记录继续由独立显示记录承载，包含智能体名称、任务 id、反馈文本和该 turn 统计；这条记录不得成为 runtime 事实源。
7. 回流给 AI 的消息和用户可见反馈记录必须通过同一 `task_id / execution_id / delivery_id` 关联，但两者分别写入 `messages` 与 `records`。

回归测试应覆盖：

1. 空闲父会话收到智能体完成事件并自动唤醒后，规范 `messages` 中新增的是 `AgentMessage`，不是 `UserText("<agent_message>...")`。
2. 同一回流只产生一条用户可见智能体反馈记录，不从 `<agent_message>` 文本反推显示记录。
3. `maybe_start_webui_mailbox_turn_if_ready(...)` 不再提前丢弃 provider-loop 需要消费的结构化 mailbox 输入。
4. mailbox-triggered turn 的父会话统计只记录父模型本次 API 消耗，智能体消耗只出现在智能体反馈记录和 `usage_groups.subagent` 中。
5. 重启后 display replay 与 live view 对同一回流显示一致。
6. `todolist` 继续属于智能体硬禁用工具；该规则仍只从共享事实源读取，不在 mailbox 修复中新增第二份禁用清单。

## 2026-07-07 修复记录：mailbox 回流改为类型化 Provider 输入

本轮完成了上节根因中的第一阶段底层修复：空闲父会话被智能体回流唤醒时，不再把 `<agent_message>` 字符串作为普通用户 prompt 送入 Provider 执行链。

关键变化：

1. `ExecutionPreparationRequest` 新增 `provider_visible_agent_messages`，用于表达“本轮要让 Provider 看到的智能体消息”。
2. runtime bootstrap 和 request snapshot 都会把这些消息追加为 core 层 `ConversationPayload::AgentMessage`。
3. transport 层新增显式 turn start mode：普通用户 prompt 与已播种 Provider 输入不再靠空字符串或伪 prompt 区分。
4. core controller 新增 seeded turn streaming 入口，只运行当前 snapshot 中已经存在的 Provider 输入，不再调用普通用户 prompt 追加逻辑。
5. `maybe_start_webui_mailbox_turn_if_ready(...)` 改为从 mailbox 收集类型化智能体消息并传入 shared prompt turn，不再调用生产链路的 `<agent_message>` 字符串渲染函数。
6. 旧字符串渲染只保留在测试/诊断范围，避免后续生产路径再次依赖文本反推语义。

这次修复解决的实际问题：

1. 空闲回流不再有机会把 `<agent_message>` 写成 `UserText`，降低 JSONL 和显示重放被内部消息污染的风险。
2. request preview、runtime bootstrap、provider dispatch 使用同一种结构化输入，避免“预览看起来是 mailbox、执行时变成用户输入”的分裂。
3. 运行中回流与空闲回流都以 `AgentMessage` 作为 AI 可见事实，后续可以在显示层单独追加用户可见的“智能体反馈记录”。
4. mailbox payload 不再夹带子代理统计，子代理消耗后续应由独立反馈记录和统计分组承载。

关于 `todolist` 的确认：

1. `crates/vulcan-session-core/src/child_agent_policy.rs` 的 `CHILD_AGENT_FORBIDDEN_TOOL_IDS` 已包含 `todolist`。
2. `child_agent_forbidden_tool_ids_include_todolist` 在 session-core 与 agent-runtime 两侧均有测试。
3. 本轮没有新增第二份禁用配置；智能体禁用工具仍只从共享事实源读取。

验证结果：

1. `cargo check -p vulcan-agent-runtime -p vulcan-service-kernel`：通过。
2. `cargo fmt --check`：通过。
3. `cargo test -p vulcan-core run_seeded_turn_streaming_uses_preseeded_agent_message`：通过。
4. `cargo test -p vulcan-agent-runtime launch_payload_uses_seeded_agent_messages_without_user_prompt`：通过。
5. `cargo test -p vulcan-agent-runtime launch_payload_assembly_trims_prompt_and_preserves_controller_seed`：通过。
6. `cargo test -p vulcan-service-kernel mailbox_intervention_appends_typed_agent_message`：通过。
7. `cargo test -p vulcan-service-kernel mailbox_message_rendering_does_not_default_missing_status_to_final_answer`：通过。
8. `cargo test -p vulcan-service-kernel mailbox_trigger_turn_keeps_subagent_usage_out_of_mailbox_payload`：通过。
9. `cargo test -p vulcan-service-kernel webui_session_kernel_mailbox_turn_accepts_subagent_input`：通过。
10. `cargo test -p vulcan-service-kernel webui_subagent_mailbox_accepts_sibling_during_mailbox_turn`：通过。
11. `cargo test -p vulcan-agent-runtime child_forbidden_tool_ids_include_todolist`：通过。
12. `cargo test -p vulcan-session-core child_agent_forbidden_tools_include_todolist`：通过。

剩余工作：

1. 用户可见的智能体反馈记录仍需继续拆成独立显示记录，并与 runtime 内部 `AgentMessage` 用同一 delivery id 关联。
2. 统计显示仍需继续完成“常规”和“智能体”双行口径，且子代理自身面板只显示自己该 turn 的消耗。
3. prompt 组装器需要继续抽成主会话/子会话共享流程，避免默认智能体、克隆智能体、mirror 智能体走不同拼装链。
4. 子代理的 hook、记忆、postaction、自动压缩和固定创建时 runtime snapshot 规则仍需后续阶段实现。

## 2026-07-07 收尾修复：seeded mailbox request snapshot 不再显示内部占位 prompt

收尾审核继续核对 mailbox 类型化入口，确认实际 Provider 执行在 `SeededProviderInput` 模式下已经不会调用普通用户 prompt 入口。但 request snapshot 的诊断 prompt 仍存在一个口径问题：

1. mailbox 触发回合使用内部隐藏 prompt `Mailbox-triggered agent message turn.` 绑定生命周期。
2. `build_core_provider_request_snapshot(...)` 已经把类型化智能体消息追加为 `AgentMessage`。
3. 修改前 `provider_request_snapshot_prompt(...)` 只要看到非空 prompt 就直接返回，因此诊断 snapshot 的 `prompt` 字段可能显示内部占位文本，而不是本轮真正的智能体回流消息。

这个问题不影响实际 Provider 输入，但会影响请求预览、调试和后续问题定位。按长期语义，它仍属于旧 prompt 口径残留，因此已收掉。

调整方式：

1. `provider_request_snapshot_prompt(...)` 改为优先渲染 `provider_visible_agent_messages`。
2. 只有不存在 seeded agent messages 时，才返回普通用户 prompt。
3. 新增测试覆盖 seeded mailbox 场景与普通用户 prompt 场景。

验证结果：

1. `cargo fmt --check`：通过。
2. `cargo test -p vulcan-agent-runtime request_snapshot_prompt`：通过，2 个测试通过。
3. `cargo test -p vulcan-agent-runtime launch_payload_uses_seeded_agent_messages_without_user_prompt`：通过。
4. `cargo test -p vulcan-service-kernel webui_session_kernel_mailbox_turn_accepts_subagent_input`：通过。
5. `cargo test -p vulcan-core run_seeded_turn_streaming_uses_preseeded_agent_message`：通过。
6. `cargo test -p vulcan-session-core child_agent_forbidden_tools_include_todolist`：通过。
7. 一次 `cargo test` 多过滤词命令因 Cargo 只接受一个测试过滤词而失败，随后用 `request_snapshot_prompt` 过滤前缀重跑通过。

结论：

1. seeded mailbox 的执行链路和 request snapshot 诊断口径现在都以类型化智能体消息为事实源。
2. 内部生命周期 prompt 不再覆盖 request snapshot 中的智能体消息诊断文本。
3. 普通用户回合仍保留普通 prompt 语义。

## 2026-07-07 目标结束收尾记录

用户已明确要求结束当前长期目标，本文件记录停止点，避免后续继续时遗漏上下文。

已完成并验证的部分：

1. 父级主回合统计与智能体用量已经完成第一层分离：`SubagentTask` 不再污染父级 `UserTurn` 顶层总数。
2. 空闲 mailbox 回流已经从 `<agent_message>` 字符串 prompt 改为类型化 `AgentMessage` 输入，避免把子代理反馈写成普通用户消息。
3. request snapshot 的 seeded mailbox 诊断口径已经补齐，调试视图优先展示真实智能体回流消息。
4. `todolist` 已纳入统一智能体禁用工具清单，未新增第二份禁用规则。

已经探查但未继续实现的部分：

1. 当前显示层已有 `SubagentFeedbackDisplayRecords` 与 `append_webui_subagent_feedback_records_if_missing(...)`。
2. 这些显示记录已经尝试生成“智能体反馈”和“智能体统计”两条显示行。
3. 仍需继续审核它们是否和 runtime 内部 `ExecutionProviderVisibleAgentMessage` 使用同一 `delivery_id` 作为事实关联，而不是只靠 `task_id + execution_id` 去重。
4. 仍需继续确认 UI 统计块是否能把常规会话统计与智能体反馈统计分行展示。

仍未完成的系统性重构项：

1. 主会话与子会话 prompt 组装器共享化。
2. 克隆模式复制 runtime state 而不是复刻显示记录。
3. 子代理固定创建时快照，后续不跟随主会话动态系统提示词变化。
4. 子代理禁用 ask、plan、todolist、智能体相关交互工具，并统一从同一策略源读取。
5. 子代理不触发记忆和 postaction，由主代理处理相关后续行为。
6. 子代理自动压缩按自身模型与上下文预算执行。
7. 父会话停止时，如有正在工作的智能体，需要让用户选择仅停止主会话、同时停止智能体或取消停止。

当前结束原因：

1. 不是因为整体智能体会话等价重构已经完成。
2. 是因为用户明确要求“收尾并且结束目标”。
3. 后续若继续，应从“用户可见智能体反馈记录与双行统计”开始，而不是重新从 mailbox 类型化入口开始。
