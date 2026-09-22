# 子代理与跨会话交互源码对照及临时方案

> 状态：持续探索中的临时方案；仅记录静态源码分析，不代表已实施或运行验证。
>
> 2026-09-20 已整理为[代理协作与动态上下文统一封装正式方案](20260920-04-AGENT_COMMUNICATION_AND_DYNAMIC_CONTEXT.md)，配套消息契约、生命周期与可靠恢复、动态上下文与协议投影三份设计文件。后续实施和验收以正式方案为入口；本文件保留源码证据、历史候选及后续研究记录。
>
> 当前方案：稳定顶部基线 + 按事件时点追加的流内宿主 `User` 信封；任务启动/方向引导与运行中临时问答使用不同的交互契约。2026-09-20 用户已确认临时提问发送即返回、需要时单独等待；自然空闲目标可自动仅答疑，明确暂停/停止的普通目标拒收新问题；子代理给主会话走原 turn 的插入通道，主会话须等所属子代理结束才能正常结束 turn；MainOnly 下允许子代理问题自动恢复主会话处理；提问方取得答案或明确撤销后才能结束；独立会话限同项目，跨项目需明确授权；普通到期内容保留历史正文，追加失效标记，到压缩时移除。前文按供应商选择流内 `Developer/System` 的段落保留为探索记录；以文末重审与已确认决策为当前依据。此确认不是实施授权。
>
> 范围：`D:\openvulcan\vulcan-code`、`D:\openvulcan\third_git\codex`，以及补充对照 `D:\dsh\deepseek-harness` 和各协议官方文档。除本文件外，本轮不修改项目文件。

## 任务目标

梳理 Vulcan 与 Codex 的子代理调度、主子代理通信和不同会话之间的交互链路；以 DSH 的动态注入实现和各供应商官方协议能力为补充参照，区分现有能力、缺口与可借鉴机制，提出适合当前项目的分阶段方案，并保留后续探索入口。

## 执行步骤

1. 记录两仓库当前源码版本与工作树边界，定位模型可见工具、调度器、会话状态、消息投递、持久化与界面投影。
2. 分别沿注册、参数、执行、状态回流链路核对主子代理交互；沿会话标识、权限、路由、投递与唤醒链路核对跨会话交互。
3. 用同一组能力维度对照两套实现，仅从当前源码可确认的行为形成结论。
4. 形成临时技术方案、分阶段落点、风险和待核实问题，供后续继续补充；本轮不实施。
5. 追加核对 DSH 的动态系统提示与来源消息投影，并以各供应商官方文档单独判断流内角色、位置和优先级，不以 Vulcan 当前代码限制反推上游协议。

## 技术选型

暂建议保留现有子代理树调度器，另在服务内核设计独立会话协作入口；两类身份、投递和状态不要混用。候选能力顺序为“查找与读取 → 发送与可靠准入 → 等待与主动通知 → 界面投影”。具体事件结构、持久化位置和权限策略仍需后续确认；本文件不冻结架构、协议或数据库变更。

## 验收标准

- 三个仓库的关键源码结论均有当前文件位置支撑；外部协议结论有供应商官方文档支撑，历史方案只用作线索。
- 分清同一代理树内的协作、独立会话之间的交互、界面导航与消息实际投递。
- 对尚未查到的链路明确标记待核实，不以推测代替实现事实。
- 文件包含可继续补充的方案与探索清单；没有项目运行、调试或测试记录。

## 源码快照与分析边界

本节及标注 2026-09-17 的源码结论保留当日分析依据。2026-09-20 的复核使用新的工作树快照，差异见文末重审；未逐条复核的历史行号不能当作当前实现定位。

- Vulcan Code：`df233c0123c1ab928bec26b903f0e8b339180697`；本轮读取的是含大量并行未提交改动的工作树，结论不能直接等同于该提交本身。
- Codex：`e269f2164cbb9f499e4f22301c393500e2a831f3`；工作树有未跟踪的 `.qwen/`、`.vulcan/`，本轮只分析源码目录。
- DSH：`0d1f50007f9bca3f52b06e1c3074fa14d5fb0720`；读取时工作树干净，本轮仅静态分析源码。
- 本轮只使用文件搜索、源码和 Git 元信息；未调试、启动、构建或执行测试。源码可证实接口与控制流，不能证明两套产品在具体运行环境中的实际表现。
- 历史计划文档只用于定位线索，下文行为判断以本轮源码为准。

### 术语边界

| 术语 | 本文件含义 |
| --- | --- |
| 子代理协作 | 主会话和由它创建的子代理之间的通信与等待。目标由直接父子关系或代理路径解析；是否允许子代理继续创建后代，另由工具策略决定。 |
| 独立会话协作 | 两个分别存在的普通会话/任务之间的查找、读取、消息提交与状态等待。来源关系不能伪装成父子关系。 |
| 界面导航 | 在界面切换到另一条会话并展示历史；它本身不等于投递消息或启动目标会话。 |
| 投递确认 | 宿主接受或持久保存一条消息；它不等于目标已经消费、完成或回报。 |

## 当前项目现状

1. `build_subagent_tool_candidates` 构建 `spawn_agent`、`agent_orchestration_info`、`send_message`、`wait_agent`、`interrupt_agent`、`close_agent`、`list_agents`，并通过工具桥加入候选集。候选集被加入并不单独证明每种运行模式都对模型可见，还要结合策略和执行准备核对。来源：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tool_surface.rs:17-25`、`crates/vulcan-agent-runtime/src/tooling/bridge.rs:99-102`。
2. 现有 `send_message` 以当前会话为调用者；普通目标通过 `SubagentRegistry::get_for_parent` 按**直接父会话和公开子代理标识**解析，`root` 别名由子代理解析到其直接父会话。因此这条路径不能直接当作任意两个独立会话的寻址器。来源：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs:219-258,340-355,397-411,1013-1015`。
3. `send_message(delivery_mode=interrupt)` 在类型定义中明确表示“下一次物理 API 边界前引导，不取消执行”；`queue` 则排在当前任务之后。目标空闲时有从 inbox 续跑的路径，消息被构造成带发送方的 `AgentMessage`。不能把工具参数名 `interrupt` 理解为停止目标回合。来源：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs:9-30,270-317,768-851,1157-1177`。
4. 子代理注册表记录子会话 ID、直接父会话 ID、创建时父回合及状态，并按父会话索引。界面导航投影也明确带 `parent_conversation_id` 和 `active_conversation_id`。来源：`crates/vulcan-session-core/src/subagent_registry.rs:91-127,245-254`、`crates/vulcan-service-api/src/subagents.rs:6-21`。
5. 普通会话的 RPC 有提交、继续、停止、打开/关闭子代理面板等入口；现有普通输入在目标会话活动时走其待发送通道。`service_continue_conversation` 是冻结继续的恢复入口，不能仅因名字相近就当作通用“给另一个会话发消息”。来源：`crates/vulcan-service-kernel/src/rpc_dispatch/conversation_runtime.rs:8-46,78-111`、`crates/vulcan-service-kernel/src/conversation_submission/queue.rs:240-307`、`crates/vulcan-service-kernel/src/conversation_submission.rs:1777-1871`。
6. 当前 V1 `UserInputSource` 只区分根输入、回合内补充和排队调度，`UserInputEvent` 记录客户端动作 ID、输入 ID 与文本，未在该类型中表达“另一个会话的代理发送”。直接复用用户输入入口会丢失来源语义；能否通过其他既有事件/元数据承载，仍待核对。来源：`crates/vulcan-session-core/src/v1/event.rs:388-430`。
7. 待消费的子代理 `agent_inboxes` 是 `TaskRuntime` 的进程内映射，`enqueue_inter_agent_message` 先写入映射再返回投递 ID；父级事件 mailbox 的基础实现也是内存队列。消费后可能另有可靠写入，但“已接受、尚未消费”阶段的重启恢复不能仅由这两个队列证明。来源：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime.rs:1027-1067`、`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs:434-465`、`crates/vulcan-session-boundary/src/subagent.rs:97-124`。
8. 子代理工具策略明确禁止子代理再次使用代理编排工具，并在子代理运行准备时执行。因此注册表的数据结构允许记录父子关系，不代表当前子代理可自主递归派生、给任意后代传话或等待任意树节点。来源：`crates/vulcan-session-core/src/child_agent_policy.rs:10-27`、`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_launch.rs:240-333`。
9. `wait_agent` 的目标限定为直接子代理；进程内父级 mailbox 用于唤醒和进度回报，而 detached 事件另用于时间线。子代理结束时存在另一条**可靠结果回流**：取得子代理的最终回答及精确统计，写入父会话 V1 文档的 `SubagentReturn`、`SubagentReturnStats`；若父回合已经结束，可创建 `TurnOrigin::SubagentReturn` 的父级回合。因此“父级 mailbox 在内存中”不能推导出最终回报也只有内存态。这个写入当前会查找已注册的父会话句柄，父会话不在注册表时如何补偿仍待核实。来源：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_methods.rs:112-289,381-405`、`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/parent_events.rs:104-128`、`crates/vulcan-service-kernel/src/agent_stream/detached_subagent.rs:194-282`、`crates/vulcan-session-core/src/v1/document/subagent.rs:285-486`。
10. 子代理 V1 文档保存在父会话下的 `TASK/task-{child_session_id}`，不作为普通根会话写入 SQLite 会话目录；界面通过父会话与 task 标识定位子代理文件并展示独立事件窗口。界面能打开子代理不表示普通会话目录能按子代理 ID 直接找到它。来源：`crates/vulcan-workspace-store/src/workspace_store/session_v1_store.rs:824-831,910-915,954-975`、`crates/vulcan-service-kernel/src/subagent_panels.rs:54-121`、`apps/vulcan-code-web/src/state/workbench-state.ts:113-117,844-877`。
11. 普通会话列表已由 `WorkbenchSnapshot.conversations` 提供，条目包含稳定 ID、标题、状态、运行阶段、更新时间与项目 ID；普通 V1 会话有按 ID 的最近窗口、历史分页和重连增量接口，重连还核对服务代次与内存/持久修订。它们可作为**受限读取能力的现有底座**，但原始工作台快照并非模型工具输出，子代理的一次性嵌入式读取也有不同定位规则。来源：`crates/vulcan-service-api/src/workbench.rs:60-78`、`crates/vulcan-service-api/src/conversation.rs:46-79`、`crates/vulcan-service-api/src/conversation_events_v1.rs:13-67`、`crates/vulcan-service-kernel/src/service_bridge_io.rs:663-673`、`crates/vulcan-service-kernel/src/conversation_events_v1.rs:97-202,219-319`。
12. 普通用户提交入口固定为 `ConversationSubmissionMode::UserPrompt` 并进入共享执行准入；已有会话的活动状态决定即时启动或用户待发送队列。该队列虽可借鉴会话准入与原子排队方式，但现有 RPC 参数和 V1 用户输入事件没有跨会话发送者，因此不能简单地由代理替另一个会话调用 `service_submit_conversation_prompt`。本轮在代理运行时、服务内核、服务 API 的相关入口中未找到等价的独立会话 `list/read/send/wait` 模型工具闭环。来源：`crates/vulcan-service-kernel/src/rpc_dispatch/conversation_runtime.rs:8-46,79-111`、`crates/vulcan-service-kernel/src/conversation_submission.rs:1933-2010`、`crates/vulcan-service-kernel/src/conversation_submission/queue.rs:240-307`、`crates/vulcan-session-core/src/v1/event.rs:388-430`。

**尚待核实：** 子代理待消费邮箱与 detached 监听器在服务重启后的恢复；父级内存 mailbox 与已落盘最终回报如何重建、去重和消费；子代理事件窗口依赖的普通会话索引在重连及分页时是否始终可定位；模型工具在不同模式下的实际可见性；普通会话目录对模型的暴露链；桌面与浏览器新服务形态下的多客户端权限边界。

## Codex 实现

### 子代理树

1. 多代理有 V1/V2 与禁用三种版本选择；配置覆盖、模型声明及特性决定实际版本，历史会话恢复还受已有元数据与继承版本影响。V2 注册 `spawn_agent`、`send_message`、`followup_task`、可选的 `wait_agent`、`interrupt_agent`、`list_agents`；V1 使用旧命名空间中的 `send_input`、`wait_agent`、`resume_agent`、`close_agent`。不能把某一版本的代码推断成全部运行状态；应用服务的 `turn/start.multiAgentMode` 已弃用且不控制这项选择。来源：`codex-rs/core/src/config/mod.rs:1537-1575`、`codex-rs/core/src/session/mod.rs:487-502`、`codex-rs/core/src/tools/spec_plan.rs:643-698,1245-1342`、`codex-rs/app-server-protocol/src/protocol/v2/turn.rs:269-272`。
2. V2 创建子代理时构造带父线程、父回合、根回合、代理路径、启动上下文的记录，并以 `InterAgentCommunication` 提交首条任务。普通 `send_message` 使用 `QueueOnly`；`followup_task` 使用 `TriggerTurn`，区分“仅传话”和“让空闲代理开始后续回合”。来源：`codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs:126-224`、`codex-rs/core/src/tools/handlers/multi_agents_v2/send_message.rs:39-47`、`codex-rs/core/src/tools/handlers/multi_agents_v2/followup_task.rs:39-47`。
3. 消息是带作者、接收者、是否触发回合的结构化通信，进入会话级输入邮箱；等待以输入活动订阅唤醒，并区分邮箱活动、用户引导和超时。来源：`codex-rs/core/src/agent/control/delivery.rs:19-59,77-124`、`codex-rs/core/src/session/input_queue.rs:20-33,68-137`、`codex-rs/core/src/tools/handlers/multi_agents_v2/wait.rs:52-104,169-195`。
4. 子代理终态会被格式化为 `trigger_turn=false` 的通信回送父线程，同时发出活动事件；回送不等于自动开启父线程新回合。来源：`codex-rs/core/src/session/mod.rs:2466-2532`。
5. V1 与 V2 的旧新机制需分别看待：V1 的 `send_input` 使用目标线程的 `start_or_steer_turn`，`wait_agent` 订阅目标状态，`close_agent` 关闭目标及存活后代；V2 的消息通过 `Op::InterAgentCommunication` 进入目标线程，等待订阅输入邮箱活动。来源：`codex-rs/core/src/agent/control.rs:201-367`、`codex-rs/core/src/tools/handlers/multi_agents/wait.rs:120-189`、`codex-rs/core/src/agent/control/legacy.rs:48-115`、`codex-rs/core/src/tools/handlers/multi_agents_v2/wait.rs:40-116`。
6. V2 代理注册表维护代理路径、线程 ID、深度和容量；恢复子代理时核对父线程、父子版本和归属，并对继承环境及权限进行校验。它给“目标是谁”和“谁有权恢复/控制目标”一个比仅凭会话 ID 更严的界限。来源：`codex-rs/core/src/agent/registry.rs:87-115,136-177,231-262`、`codex-rs/core/src/agent/control/spawn.rs:318-400,403-590,625-655`。

### 独立任务之间

1. 本次仓库中的 TUI 任务工具包含 `list_threads`、`read_thread`、`wait_threads`、`send_message_to_thread`、`create_thread`、`fork_thread` 等。这一层以**独立任务 ID**为目标，不使用子代理树的代理路径；`read_thread` 返回有界的会话/回合摘要。来源：`codex-rs/tui/src/dynamic_tools.rs:65-200,520-560`。
2. TUI 的 `send_message_to_thread` 先读取并恢复目标任务，再用 `turn/start` 向目标发送具名工具输出；`turn/start` 在服务端把它转换成 `FunctionCallOutput`，随后经 `start_or_steer_turn` 处理。目标空闲时开始回合，目标活动时可引导当前回合。这与子代理的 `QueueOnly` 完全不同。来源：`codex-rs/tui/src/dynamic_tools.rs:745-779,1277-1301`、`codex-rs/app-server/src/request_processors/turn_processor.rs:528-607,650-677`、`codex-rs/protocol/src/turn_input.rs:131-136`。
3. TUI 的 `wait_threads` 最多等待八个目标、最长等待两分钟，并拒绝自等待或重复目标；先读目标线程、回合与条目生成游标，再等待 TUI 的 `ThreadStatusChangedNotification` 广播，按目标完成、需要处理或超时返回有界快照。这与 V1 的目标状态订阅、V2 的输入邮箱活动订阅是三套不同等待机制。来源：`codex-rs/tui/src/dynamic_tools.rs:833-1143`、`codex-rs/core/src/tools/handlers/multi_agents/wait.rs:120-189`、`codex-rs/core/src/tools/handlers/multi_agents_v2/wait.rs:40-116`。
4. TUI 工具的创建/发送/派生属于需审批的委托工具；其动态工具直连模式会过滤这三种操作。TUI 把源线程 ID 写入委托文本，并以受认可的具名工具输出提交。Core 只从这类工具输出为**审阅上下文**捕获发送者最近的用户消息；该快照不会加入接收任务的模型提示词。不能把文本中的源标签或审阅快照当成目标模型已经可靠识别发送者的证明。来源：`codex-rs/tui/src/dynamic_tools_mcp.rs:60-70,118-129`、`codex-rs/tui/src/dynamic_tools.rs:253-270,1164-1199`、`codex-rs/core/src/agent/control/sender_context.rs:16-76`、`codex-rs/core/src/context/guardian_sender_messages.rs:1-16`。
5. 应用服务协议提供 `thread/start`、`thread/resume`、`thread/list`、`thread/read`、`turn/start`、`turn/steer` 等底座。`thread/inject_items` 允许附加原始模型可见条目，是不同层级的低级接口，不应直接照搬为普通会话消息 API。来源：`codex-rs/app-server-protocol/src/protocol/common.rs:559-569,768-772,854-875,1040-1056`、`codex-rs/app-server-protocol/src/protocol/v2/thread.rs:1691-1700`。
6. TUI `create_thread` 从源任务继承工作目录、项目、模型及权限相关设置并启动独立任务；`fork_thread` 只带已完成的历史，源任务当前进行中的回合不进入派生会话。这两种创建方式的上下文边界不同于子代理启动。来源：`codex-rs/tui/src/dynamic_tools.rs:557-687,688-743`。
7. 应用服务拒绝向 V2 `ThreadSpawn` 子代理直接输入；恢复此类子代理时也必须经父线程加载，且禁止调用方直接改写恢复配置。因此面向独立任务的 `send_message_to_thread` 不会绕过真正子代理的父级归属检查。来源：`codex-rs/app-server/src/request_processors/thread_input.rs:8-36`、`codex-rs/app-server/src/request_processors/thread_processor.rs:3726-3775`、`codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs:204-281`。
8. TUI 动态工具直接调用应用服务的 `ThreadResume`、`ThreadStart`、`ThreadFork` 和 `TurnStart`；这条独立任务链路没有调用 Core `AgentControl::spawn_agent*`。`DELEGATION_TOOLS` 仅把创建、发送和派生列为委托写操作，列表、读取、等待和标题/归档属于管理与观测能力。来源：`codex-rs/tui/src/dynamic_tools.rs:65-78,147-250,572-784,804-831,833-1143`。

**边界：** 上述跨独立任务入口是本仓库可见的 TUI/应用服务源码；`codex_app` 宿主插件的完整实现本轮尚未定位。不能把桌面宿主的所有行为归因于这里的 TUI 实现。

### 三种交互链路的时序

1. **Vulcan 父子代理：** 模型工具解析直接子代理 → `TaskRuntime` 按投递模式将消息放入子代理 inbox → 活动子代理在 API 边界可靠接纳引导，或空闲子代理续跑 → 子代理运行/结果事件通知父级 mailbox 和时间线 → 最终回答与统计落到父级 V1 文档。消息进入 inbox 到可靠接纳之间的服务重启语义仍未证实。
2. **Codex V2 父子代理：** 工具以代理路径解析目标并经过 `AgentControl` → 结构化 `InterAgentCommunication` 进入目标 `input_queue` → `QueueOnly` 仅入队，`TriggerTurn` 请求新回合 → `wait_agent` 由邮箱活动唤醒 → 子代理完成时向父级发送不触发回合的终态通信。目标归属、恢复环境及权限由 Core 再验证。
3. **Codex 独立任务：** TUI 动态工具以普通线程 ID 定位/恢复目标 → `send_message_to_thread` 提交具名工具输出给应用服务的 `turn/start` → Core 对目标执行空闲启动或活动引导 → `wait_threads` 结合目标回合/条目游标与线程状态通知返回进展。整个链路不经过 V2 子代理树的 `AgentControl::spawn_agent*`。

## 对照与缺口

| 能力 | Vulcan 当前源码 | Codex 当前源码 | 临时判断 |
| --- | --- | --- | --- |
| 子代理创建与身份 | 子会话、直接父会话和公开 ID；子代理工具策略禁止再次编排 | 子线程、代理路径与父/根回合；恢复时复核父级和权限 | 两者已有子代理树，但可继续派生的边界不同。 |
| 主到子消息 | `send_message` 可排队或下次 API 前引导；空闲目标可续跑 | V2 `send_message` 仅入队，`followup_task` 触发回合 | 先统一实际语义与命名，再决定是否拆分工具。 |
| 子到主回报 | 进程内 mailbox 负责唤醒/进度；精确最终结果另写入父会话 V1 文档 | V2 终态通信进入父级输入邮箱并唤醒等待，但不自动开启父级回合 | 分别研究“即时通知”和“可靠结果”；Vulcan 还需核对重启重建与去重。 |
| 独立任务查找、读取、发送、等待 | 普通会话服务与界面能力存在；尚未确认等价的模型工具闭环 | TUI 任务工具经应用服务操作普通任务，不调用子代理树调度器 | 这是本轮主要新增设计方向；不得直接借用子代理树目标解析。 |
| 消息来源 | 子代理消息有结构化发送方；普通 V1 用户输入没有跨会话发送者字段 | 宿主具名输出与受认可来源捕获 | 跨会话投递需要宿主认证的来源身份及可审计投递 ID。 |
| 投递/完成区分 | 子代理任务与普通提交已有不同回执 | `turn/start` 接受不等于目标完成，`wait_threads` 再跟进 | 新接口须分别报告准入、消费、目标回合状态。 |

## 协议无原生子代理时的身份与回复契约

### 本轮补充的源码事实

1. 这份 Codex 源码的 `WireApi` 只保留 `Responses`；V2 的 `InterAgentCommunication` 在内部保存 `author`、`recipient`、`trigger_turn`，进入模型历史时生成 `ResponseItem::AgentMessage`，其中有独立的 `author`、`recipient` 和内容字段。该实现不能直接复制到只接受常规消息角色/工具调用的 Provider。来源：`codex-rs/model-provider-info/src/lib.rs:89-107`、`codex-rs/core/src/client.rs:2116-2167`、`codex-rs/protocol/src/protocol.rs:807-910`、`codex-rs/protocol/src/models.rs:1036-1047`、`codex-rs/core/src/session/mod.rs:3907-3933`。
2. Codex 同时用模型说明明确角色与回复方式：根代理、子代理有分别的说明；子代理被告知最终回答由宿主立即送回父代理，收到的消息还展示类型、任务名、发送者和正文。这是**身份说明、类型化消息、宿主回流**的组合，并非模型仅靠一条自然语言自行决定回复对象。独立任务投递则由 TUI 生成带源线程 ID 的委托文本，作为具名工具输出提交到目标回合；其发送者用户消息快照只供审阅，不会加入目标模型提示词。来源：`codex-rs/prompts/src/model_messages/multi_agent.rs:7-44`、`codex-rs/core/src/context/inter_agent_message.rs:45-71`、`codex-rs/tui/src/dynamic_tools.rs:745-784,1164-1199,1277-1303`、`codex-rs/core/src/context/guardian_sender_messages.rs:1-16`。
3. Vulcan 已有内部 `AgentMessage` 类型，保留 `sender`、`recipient` 和宿主元数据；但当前 Provider 请求桥先把它降为 `UserText`，通用传输投影再把它渲染为 `user` 角色的 XML 文本。模型可见文本只有消息类型、任务名、发送者和正文，不含 `recipient`、投递 ID 或权威回复路由。XML 会转义正文，但 XML 文本本身不是认证凭证。来源：`crates/vulcan-core/src/types.rs:739-812`、`crates/vulcan-agent-runtime/src/agent_runtime/provider_request_bridge_messages/projection.rs:199-213`、`crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_vulcan/message_projection.rs:250-260`、`crates/vulcan-agent-runtime/src/agent_runtime/provider_visible_agent_messages.rs:82-111`。
4. Vulcan 子代理身份已经由提示词组合器的 Developer 节点呈现，包含身份、委托任务和“向父代理返回最终结果”的说明；最终结果的实际归属由执行 ID、父会话句柄及 V1 回流负责。当前子代理工具策略又禁止它主动调用 `send_message`，所以“回复父代理”主要依赖宿主收取最终回答，不靠模型猜父会话 ID。来源：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_launch.rs:200-275`、`crates/vulcan-prompt-composer/src/node/mod.rs:171-195`、`crates/vulcan-service-kernel/src/agent_stream/detached_subagent.rs:194-282`、`crates/vulcan-session-core/src/child_agent_policy.rs:10-27`。

### 拟定的不变量

| 问题 | 宿主必须保存的事实 | 模型需要看到的说明 |
| --- | --- | --- |
| 我是谁 | 当前会话 ID、代理/普通任务身份、父级归属及当前回合来源 | 每次请求有稳定身份说明；标清本回合来自用户、父代理还是独立任务。 |
| 谁发来了消息 | 投递 ID、源会话/回合、目标会话、投递类型及准入状态 | 受控信封显示发送方、消息类型和正文，正文作为外来内容。 |
| 我要回复谁 | 当前回合绑定的来源投递及宿主解析的回复通道 | 明确“最终回答由宿主回传”或“使用哪条投递的回复工具”；不能要求模型猜目标 ID。 |
| 回复是否送达 | 回执 ID、目标回合、消费与终态、失败原因 | 工具结果区分已接纳与已完成，必要时可等待或读取。 |

### 首版建议

1. **身份由宿主注入：** 稳定的会话角色规则可放入已有提示词组合器的 Developer 层；每次投递的来源、目标和回复规则必须绑定到消息流中对应的投递位置，不依赖全局提示词。来源来自注册表/持久事件，不能由用户正文、代理正文或模型输出声明。子代理继续使用父级归属；普通任务的对等会话消息使用独立的 `peer_delivery` 回合来源，不冒充用户输入或子代理。
2. **消息由宿主封装：** 内部投递先保存强类型来源、目标和投递 ID，再将动态提示词统一投影为带功能标签的 User。模型可见信封应补充当前缺失的目标、投递 ID 与明确的回复规则；认证与路由仍只以宿主记录为准。原生工具调用/结果保留各协议结构与配对，不将工具结果重写成普通 User 正文。本条依 2026-09-20 重审统一口径，取代早期按流内高优先级角色分级的候选。
3. **任务型投递对应目标回合：** 对启动独立任务的首版建议，目标繁忙时排队，不把任务投递引导进正在处理用户输入的回合；同一目标回合只绑定一条来源任务。这样目标最终回答可由宿主按回合归属自动回传原发送会话，模型即使只输出最终回答也不会回错对象。此规则**不适用于**运行中临时问答：问答附着既有任务/回合，有自己的问题 ID 和工具答复；方向插队也只是更新既有任务，不因收到引导便产生一次中途答复。
4. **临时问题通过工具回答：** 使用带 `question_id` 的提问和 `reply_question(question_id, answer)` 候选契约；宿主核对当前会话确为接收者，再从问题记录解析提问方。回答工具向回答方返回提交回执，真实答案另交给提问方。早期 `reply_to_delivery` 泛化中途回复候选不再用于问答，不能把任务最终结果、方向引导和临时答案混为同一个回复入口。
5. **双向结果可见：** 目标会话保留自己生成的回答；来源会话收到带投递 ID 的结果事件，必要时触发来源代理的后续回合。它仍是一条代理结果，不能投影成来源会话里的新用户指令。直接用户输入、对等任务输入与父子代理输入分别保留来源类型。
6. **恢复后身份不漂移：** 压缩、重连和进程恢复时，从持久的回合来源与投递记录重建稳定身份规则及每条待处理的流内投递；不能依赖旧上下文、会话标题或自然语言摘要记住“我在跟谁说话”。

这套设计能让宿主确定回复对象，并尽量让模型清楚当前身份；它无法保证模型每次都正确理解文本信封，因此权限、目标寻址、幂等和最终回流都必须由宿主执行。具体事件类型、提示词内容及数据库迁移仍属待确认的架构方案，本轮不实施。

## 协议角色与现有注入方式核对（2026-09-17）

### 核对口径：以生产出站链为准

公共 `MessageRole` 只有 `System`、`Developer`、`User`、`Assistant`、`Tool` 五种，尚无 `AgentMessage` 角色。生产请求经 `ProviderExecutionClient::encode_request`、`project_internal_baseline_for_provider`，先由 `encode_internal_responses_request` 构造非线路内部 Responses 封装，再按冻结的 `protocol_profile_id` 转换；不能仅查看 `vulcan-ai-protocols` 的独立 Profile 编码器。内部封装复用 Codex 请求编码：`System` 从 `input` 中排除并汇入顶层 `instructions`，`Developer` 仍按原消息顺序留在 `input`。来源：`crates/vulcan-ai-contract/src/content.rs:12-27`、`crates/vulcan-provider-adapters/src/execution/request_encoding.rs:48-61,238-286`、`crates/vulcan-ai-protocols/src/openai/internal_input.rs:192-216`、`crates/vulcan-ai-protocols/src/openai.rs:800-818,1063-1088`、`crates/vulcan-llm-core/src/internal_media.rs:316-393`。

| 已注册 Profile | 当前生产请求里真正保留在消息流中的角色或项 | `System` / `Developer` 的实际位置 | 对逐条代理投递的结论 |
| --- | --- | --- | --- |
| `openai-chat-completions.v1` | `messages` 中有 `user`、`assistant`、工具结果 `tool`，开头有合并后的 `system`。 | 两者都转为 `system`，随后 `collapse_system_messages` 将所有 `system` 文本抽出、合并并移到 `messages[0]`；不保留其原时间位置。 | 当前生产链**不能**用中途 `system` 承载投递，即使独立 Chat 编码器能逐条输出 `system`。要采用该方案，须先改变公共转换契约并核对所选模型入口。现状可用逐条 `user` 信封。 |
| `openai-responses.v1` | `input` 中按序保留 `developer`、`user`、`assistant`；工具结果是 `function_call_output` 等项。 | 生产链的 `System` 汇入顶层 `instructions`；`Developer` 留在 `input`。独立标准 Responses Profile 可把 `System` 放在 `input`，但生产链不走该独立编码路径。 | `developer` 是**代码层面可保留时间位置**的高优先级候选；须再核对冻结的具体模型/网关是否接受中途 `developer`。不用全局 `instructions` 表示单次投递。 |
| `openai-codex-responses.v1` | 与上一行相同的 `developer`、`user`、`assistant` 及工具项；可保留同路由回传项。 | `System` 汇入顶层 `instructions`；`Developer` 留在 `input`。 | 首选研究原生 `agent_message` 输入项的精确契约；Vulcan 当前只识别部分供应商回传项，未见宿主生成该项的投递编码。实施前可把流内 `developer` 作为候选，而不能声称原生投递已可用。 |
| `anthropic-messages.v1`、`anthropic-messages-video.v1`、`claude-code-messages.v1` | `messages` 中只有 `user`、`assistant`；工具调用为助手内容块，工具结果为用户内容块。 | `System` / `Developer` 被收集到请求顶层 `system`。 | 没有现成的流内高优先级角色；逐条代理投递使用 `user` 信封。 |
| `google.gemini.generate_content`、`google.code_assist.generate_content.v1` | `contents` 中为 `user`、`model`；函数响应走 `user` 内容，Code Assist 另加外层 `request` 封装。 | `System` / `Developer` 被收集到 `systemInstruction`。 | 没有现成的流内高优先级角色；逐条代理投递使用 `user` 信封。 |
| `gemini-interactions.v1` | `input` 使用 `user_input`、`model_output`、`function_call`、`function_result` 等步骤。 | `System` / `Developer` 最终成为顶层 `system_instruction`。 | 没有现成的流内高优先级角色；逐条代理投递使用 `user_input` 信封。 |
| `cohere.chat.v2` | 独立 Profile 可生成 `system`、`user`、`assistant` 及工具结果消息。 | 独立编码器会把 `System` / `Developer` 都映射为逐条 `system`。 | 当前公共生产转换链没有 Cohere 目标转换器，会在分派前拒绝；不能将独立 Profile 的角色能力列为当前可用投递能力。 |

关键来源：Chat 头部合并见 `crates/vulcan-llm-core/src/responses_to_chat.rs:21-60,448-465,771-806`；Anthropic 见 `crates/vulcan-llm-core/src/responses_to_anthropic.rs:32-59,106-126`；Gemini / Code Assist / Interactions 见 `crates/vulcan-llm-core/src/responses_to_gemini.rs:21-53,91-115,533-540`；Cohere 的独立编码与生产拒绝分别见 `crates/vulcan-ai-protocols/src/cohere.rs:51-62,271-301`、`crates/vulcan-provider-adapters/src/execution/route.rs:286-299`、`crates/vulcan-provider-adapters/src/execution/request_encoding.rs:245-251`。此表描述**基础生产协议投影**：其后还可能应用冻结路由声明的 Manifest / 自定义请求映射，最终能力应以该路由的完整请求字节为准（`crates/vulcan-provider-adapters/src/execution/request_encoding.rs:395-426`）。静态编码结果不等于对任何外部模型/网关实际接受这些位置的运行验证；模型级 `selected_protocol` 仍是唯一执行路由，不通过失败后换协议来试探。

### 目标模式与记忆注入提供的现有参照

1. **目标模式：** 活跃目标渲染成转义后的 `<thread_goal_context>`，由服务内核前置到本回合实际用户提示词；若由代理 mailbox 消息启动回合，则直接附在首条具体 mailbox 消息的载荷前，累计用量附在本回合末尾。它明确把目标正文称为用户提供的数据，并刻意不放入全局 Provider 提示词层。这个模式证明“回合级上下文绑定到具体消息”在现有架构中已有落点，但当前是文本拼接，不是独立协议角色。来源：`crates/vulcan-agent-runtime/src/agent_runtime/session_goal_prompt.rs:20-38`、`crates/vulcan-service-kernel/src/session_goal_lifecycle/prompt_context.rs:139-205`、`crates/vulcan-service-kernel/src/webui_prompt_lifecycle/precheck_continue.rs:732-766`、`crates/vulcan-agent-runtime/src/agent_runtime/provider_runtime_context.rs:1275-1304`。
2. **VMM 记忆：** 全量画像基线使用带来源的 `vmm_profile` 系统提示词 Hook；刷新差量用持久隐藏用户注入，生产重建把运行时上下文作为 `User` 消息；召回记忆则按准确的原始用户消息 ID 锚定，只在物理请求中前置到那条用户输入。压缩后尚无新真实用户可锚定时，会暂用请求局部持久块承载活动记忆。可借鉴的是**强类型来源、准确锚点、请求局部投影与缓存纪元约束**；全量画像的系统层不适合作为每条代理通信的载体。来源：`crates/vulcan-agent-runtime/src/agent_runtime/vmm_hooks.rs:1732-1752,1826-1877`、`crates/vulcan-agent-runtime/src/agent_runtime/context_injection_runtime.rs:21-79`、`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs:1153-1165,1207-1225,1364-1395`、`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink/context_injection.rs:32-88`。
3. **现有“动态 system”不等于流内投递能力：** `ProviderSystemUpdateLoweringPolicy` 的原生时间顺序 system 支持默认关闭，而公共请求桥拒绝 `NativeSystem` 投影。因此不能把这一现有机制当成各协议已实现的流内 `system` 路径。来源：`crates/vulcan-agent-runtime/src/agent_runtime/provider_system_update_lowering.rs:12-28,60-70`、`crates/vulcan-agent-runtime/src/agent_runtime/provider_request_bridge_messages/projection.rs:222-250`。

### 消息封装的修订判断（仅按当前 Vulcan 生产链）

| 内部消息语义 | 建议的模型侧承载 | 原因 |
| --- | --- | --- |
| 稳定会话身份与长期协作规则 | 现有基础 `System` / `Developer` 提示词层，按当前冻结协议投影。 | 这些是宿主制定的长期规则，允许进入对应协议的顶层指令；每次投递的来源仍需流内单独表达。 |
| 单次投递的宿主可信来源、目标及回复规则 | 优先使用经具体路由证实可保序的流内高优先级角色；按**当前 Vulcan 生产转换**，候选仅 Responses 的 `developer`。否则放入该投递的 `user` / `user_input` 信封。 | 该信息须跟随投递时间位置，并由宿主产生，不能从其他代理正文解析；各上游协议本身的能力见文末补充。 |
| 其他代理编写的消息、任务结果、跨会话引用 | 独立的 `user` / `user_input` 数据块；若 Codex 原生项契约确认并实现，可用专用 `agent_message`。 | 正文是外来数据，不因加了 XML 标签而成为系统指令，也不能伪装成当前目标代理的既有 `assistant` 消息。 |
| 本会话真实工具调用的返回 | 仅在存在匹配调用 ID 时使用协议原生工具结果项。 | Chat 的 `tool`、Responses 的 `function_call_output`、Anthropic 的 `tool_result`、Gemini 的函数响应都有调用配对语义，不能把无请求的代理推送直接伪装成工具返回。 |
| 本会话的最终回复或中途回复 | 模型生成 `assistant` 正文或调用显式回复工具；宿主根据绑定的投递 ID 路由。 | 回复收件人由会话与投递记录确定，不由自由文本标签决定。 |

- **先保存强类型事件，再按冻结路由投影。** 宿主记录来源会话、目标会话、投递 ID、消息类型、正文和目标回合；模型正文里的 `<vulcan_agent_message>` 只是可读边界标记，不作为认证、去重或回复寻址依据。恢复时从原事件重新投影，不从模型文本“反包裹”恢复身份。
- **保留投递所在的消息位置。** 标准 Responses / Codex Responses 若具体模型入口经确认支持流内 `developer`，可在投递前放一条仅含宿主可信元数据及回复规则的 `developer` 消息，再用紧邻的 `user` 消息承载转义后的 `<vulcan_agent_message>` 正文；不得把其他代理可写的正文整体提升为 `developer`。其余当前可用路由用单条 `user` / `user_input` 信封，稳定身份解释可来自原有基础提示词，但单次投递仍是消息流中的独立项。Chat 中途 `system` 须先解决当前头部合并，不能直接按独立编码器的表现设计。
- **原生项单独建模。** `third_git/codex` 的 `ResponseItem::AgentMessage` 含 `author`、`recipient` 和内容；Vulcan 的 `AiMessage` 尚无相应宿主投递内容项，且当前 `agent_message` 白名单用于供应商回传重放。若走 Codex 原生路径，应先确认上游请求契约，再设计宿主可生成、来源可验证的专用项，而不是借用回传项伪造。
- **“反包裹”只用于显示或受控导入。** 模型回复是否发给父代理或某独立会话，应由当前目标回合绑定的投递记录、最终结果回流或结构化回复工具确定；不能靠解析模型自由文本里的关闭标签决定收件人。

## 临时方案

### 建议的能力边界

保留当前 `TaskRuntime` 管理父子代理树。为普通独立会话另设服务内核的会话协作层，目标键为真实会话 ID，调用者由宿主运行上下文确定；仅在权限校验通过后建立一条带来源的协作记录。协作关系只作为消息/追踪关系，不能改写 `parent_session_id` 或把独立任务展示成子代理。

入口按已确认语义分为 `start_task`、`steer_task` 和 `ask_question` 三种候选契约：启动创建自己的任务归属，目标繁忙时排队；插队绑定仍在执行的原任务，只改变方向；临时问答用 `question_id` 关联提问与工具回答，不作为 `send` 的另一投递模式。早期 `queue`、`followup`、`start_or_steer` 仅保留作两仓库接口对照，不作为当前统一入口。原任务已经结束时不能把插队静默改成启动新任务。目标若是真正的子代理，继续走原有父级归属与工具策略，不从独立会话入口绕开。

候选投递记录应至少具备投递 ID、源会话/源回合、目标会话、消息类型、正文或受控引用、创建时间、准入状态、目标回合 ID、失败原因和消费游标。投递回执应明确区分“已接受”“已入队/已引导”“目标已消费”“目标已完成”；幂等重试与同 ID 不同正文冲突必须由目标会话权威状态处理。字段及存储位置只是设计候选，后续先与 V1 事件、元数据和运行时注册表对齐。

### 与当前项目的候选接入点

| 层 | 已有源码落点 | 拟补的能力与边界 |
| --- | --- | --- |
| 模型工具暴露 | `agent_runtime/task_runtime/tool_surface.rs`、`tooling/bridge.rs` | 单独注册独立会话协作工具，核对模式可见性、策略过滤、参数准备与执行；不复用仅解析直接子代理的 `send_message`。 |
| 会话目录与有界读取 | `service_bridge_io.rs` 的工作台快照、`conversation_events_v1.rs` 的窗口/分页/重连接口 | 在服务层用真实会话 ID 及项目归属构造有限摘要，再投影给模型；不直接暴露完整工作台快照或任意 V1 原始事件。 |
| 写入准入与目标运行 | `conversation_submission.rs`、`conversation_submission/queue.rs`、`session_runtime_registry_v1` | 沿用已有目标会话的唯一所有者、活动回合与队列事实；新建带宿主来源的投递契约，避免伪造用户提交或以 `service_continue_conversation` 代替跨会话发送。 |
| 状态与界面 | `conversation_events_v1.rs`、`workbench_session_panels.rs`、WebUI 会话状态 | 区分投递修订、目标回合修订和最终结果；以游标做等待与重连，在普通会话时间线展示任务间关系。 |

此表只定位现有代码边界，不表示这些文件已经具备跨会话投递，也不指定最终要增加的文件或数据库表。

### 分阶段落点

1. **只读协作视图：** 在会话目录上提供项目/工作区范围内的列出与有界读取；输出明确源、目标、状态、最新回合和游标。标题、摘要及对话文本作为数据，不直接变成目标代理的指令。
2. **宿主投递：** 启动型独立任务在目标繁忙时排队，并绑定自己的任务和目标回合；方向插队作用于正在执行的原任务，继续等它的最终 Assistant；临时问答在合法边界插入，独立问题 ID 由工具回复。三者通过现有会话准入门和持久状态接入，避免只写进内存邮箱或普通用户输入事件。早期将所有消息都排到任务结束后再开回合的首版建议已被此处替代。Codex 的 TUI `send_message_to_thread` 是空闲启动/活动引导，不能把它的名称直接移植为纯排队接口。
3. **等待与回流：** 对目标会话的持久修订和实时运行状态建立订阅及有界快照；等待首先返回完成、需要用户操作或失败的目标，携带每目标游标，并使调用会话新输入可打断等待。恢复后仍能凭游标继续，不重复投递摘要。
4. **界面与审计：** 在会话时间线呈现来源任务、投递与目标回合的关联；独立任务在普通会话列表中保持原有身份，子代理仍在父级导航中展示。消息来源由宿主写入，正文仅作为外来内容交给模型，不能由正文自称来自用户或系统。

### 候选实现中的硬边界

- 其他会话的文字即使在线路上使用 `User` 角色，也必须带宿主来源记录和可识别的协作信封，不能伪装成**真实用户直接输入**或系统指令；也不以消息正文里的 XML/JSON 标签作为权限依据。
- 创建独立任务、跨项目发送、打断活跃回合和修改目标配置应分别设权限门；同工作区多会话可能共享脏工作树，需防止文件所有权冲突。
- 若需要事件或 Schema 变更，先给出精确迁移与恢复设计，再由用户确认重大架构决策；本轮不实施。

## 后续探索清单

- [ ] 补齐 Vulcan 子代理待消费消息、detached 监听器、父级通知与 V1 最终回报在重启后的重建、去重和消费链路；核对 WebUI 子代理事件窗口在重连及分页时的定位。
- [ ] 核对 Vulcan 普通会话目录、读取和事件订阅接口是否已有可复用的服务层入口，尤其在桌面与浏览器共用服务时的权限范围。
- [ ] 继续核对 Codex V2 子代理消息与终态通知的持久化恢复语义；进一步拆分 TUI 实现与 `codex_app` 宿主尚未公开的部分。
- [x] 明确独立会话的首版范围与已有任务语义：同项目可协作，跨项目需明确授权；允许运行中方向引导；自然空闲目标可自动仅答疑，明确暂停/停止的普通目标拒收新问题。子到主为单独通道，MainOnly 下可因子代理问题自动恢复主会话处理；不能由此推导允许自动创建新的业务任务。
- [ ] 补齐项目授权记录、来源展示和新业务任务的创建入口；核对已有任务所有者及用户停止状态怎样进入统一准入判定。
- [x] 静态核对全部当前注册 Profile 的消息角色、生产转换链、目标模式与记忆的注入位置；记录独立编码器与生产链的差异。
- [x] 静态核对 DSH 的动态 system、来源消息、DeepSeek Chat/Messages 与通用 pi-ai 投影；对照各供应商官方文档，区分流内角色与顶层指令。
- [x] 静态审计 Vulcan 动态记忆、Plan、Goal、继续回合指引、环境上下文从生产者到 V1/非 V1 出站编码的位置；分别记录历史时点变化与缓存前缀变化。
- [ ] 继续核对 `transient_prefix` 的完整物理消费者与实际产品入口；当前只确认生成、状态携带和重写判定，未找到写入模型消息的生产代码。
- [ ] 进入可运行验证阶段后，选定同一会话的相邻请求，对最终编码的角色、顺序、正文和 Provider 缓存用量做配对证据；本轮按要求不运行。
- [ ] 在统一 User 方案下核对已选路由的信封顺序、相邻 User 合并、工具结果和媒体的结构约束；中途 Developer/System 及原生 agent_message 仅作为协议研究记录，不再是动态提示词统一方案的前置分流条件。本轮未运行请求，也未把静态编码结果当成上游验证。
- [ ] 确认投递说明在上下文压缩、continuation 复用及重连后如何从权威事件重建，并核对工具回执关联。
- [ ] 设计 `peer_delivery` 与来源回合、目标回合、最终回答的唯一关联；明确多消息排队、取消、失败和重试时的回复归属。
- [ ] 在以上事实闭环后，为 V1 事件与服务 RPC 画出精确改动点、故障恢复状态机、验收场景；再决定是否进入实施计划。

## 补充：中途高优先级消息是否打断上下文（2026-09-17）

这里把三种含义分开：规范会话历史是否丢失、提交给模型的消息先后关系是否改变、Provider 的缓存或延续游标是否复用。仅凭本轮静态源码，不能把第二种问题直接表述为第一种或第三种问题。

1. **已确认的条件性顺序缺陷：**V1 `SystemContext` 的非 TTL Upsert 和 Remove 作为按事件序号排列的 Provider 可见事实进入投影，`goal_state` 的创建、修改、完成和暂停使用 `Developer` 提示；`goal_continuation` 未指定角色时也默认投影为 `Developer`。V1 Provider sink 把这些事件按投影顺序放进中立消息流。可是 Chat 生产转换先将 `developer` 改成 `system`，再把所有 `system` 抽出、合并并放到 `messages[0]`。因此只要这类中途事件走 Chat，模型所见的目标生效、变更和退出位置就不再是原事件位置。这是可由源码确定的时序保真缺陷；并非用户与助手历史消息被删除。来源：`crates/vulcan-session-core/src/v1/document/modes.rs:82-94,240-251,555-567,690-700`、`crates/vulcan-session-core/src/v1/document/turns.rs:269-282`、`crates/vulcan-session-core/src/v1/projection.rs:493-522,769-775`、`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs:1178-1185,1474-1491`、`crates/vulcan-llm-core/src/responses_to_chat.rs:448-469,771-806`。
2. **另一个目标提示路径：**WebUI 目标上下文被加到当前用户提示词前，并按精确消息 ID 替换当前规范 `UserText`；若没有该用户消息，则附着在本次 Provider 可见的代理消息上。目标版本标记用于避免同一版本重复投递。这一路径保持用户消息角色，不受 Chat 的 `system` 合并函数直接影响；它确实改写当前规范消息的文本，但本轮没有发现它会删除以前的回合。来源：`crates/vulcan-service-kernel/src/webui_prompt_lifecycle/precheck_continue.rs:736-773`、`crates/vulcan-service-kernel/src/session_goal_lifecycle/prompt_context.rs:243-263,280-301`。
3. **其他协议的边界：**生产链中的 `System` 在内部 Codex Responses 封装时被汇总到顶层 `instructions`，不保留流内时点；`Developer` 仍可在 Responses `input` 中按序。Anthropic/Gemini 转换也把高优先级内容映射到顶层指令。故“支持某角色”与“支持这个角色在历史中途保持原位置”必须分别核对，不能用角色名称直接推出时序能力。来源：`crates/vulcan-ai-protocols/src/openai.rs:1063-1088`、`crates/vulcan-ai-protocols/src/openai/internal_input.rs:192-216` 及上文协议表所列转换器。
4. **尚未证实：**本轮未运行请求、调试或测试，不能断言某个实际模型已经因此答错，也不能断言每次切换目标都会让 Provider 缓存失效或延续游标断开。后续应静态核对 V1 与 WebUI 两套目标投递是否会在同一请求相遇，并逐路由确认最终发送的字节与延续资格；若进入实施阶段，再设计精确请求序列的验证。

方案约束：跨代理身份和单次投递的控制信息不得依赖会被重排的流内 `system`；选择流内 `developer` 前，必须确认生产转换和具体上游都保留其时点。否则用当前可保序的用户消息信封承载单次投递，同时将授权和回复归属留在宿主记录中。

## 补充：协议原生流内角色与 DSH 动态注入（2026-09-17）

本节响应“将来怎样统一封装”的问题。**协议原生能力、具体供应商/模型能力、DSH 的适配策略、Vulcan 当前代码能力是四层不同事实。**“请求顶层 `system` / `instructions` 可用”不能算作“在历史中途可插入高优先级消息”。“请求 schema 接收某角色”也不能直接推导该模型会按预期在那个时点生效。以下只读源码和官方文档，未向任何模型发请求。

### 官方协议能力矩阵：只统计消息流内位置

| 供应商与接口 | 流内消息/步骤 | 顶层指令 | 流内高优先级动态更新的文档结论 | 面向代理投递的常规数据位置 |
| --- | --- | --- | --- | --- |
| OpenAI Chat Completions | `messages[]` 声明 `developer`、`system`、`user`、`assistant`、`tool`；`developer` 是较新模型的开发者指令角色。 | 无独立的 `instructions` 请求字段。 | `developer` / `system` **在数组 schema 内**；本轮找到的 Chat API 参考没有像 DeepSeek 那样单独承诺“中途更新 system”的时序语义，也没有证明所有兼容网关照做。记为“流内角色可编码，动态生效能力需按模型与端点声明”，不能把它等同于顶层字段。 | `user`；有配对调用时才用 `tool`。 |
| OpenAI Responses | `input[]` 的消息项可用 `system`、`developer`、`user`、`assistant`；函数调用及返回另有类型化项。 | `instructions` 是请求级字段；配合 `previous_response_id` 时，上一次的 `instructions` 不自动延续。 | `input[]` 明示可放高优先级角色；仍应分别核对目标模型、代理网关和延续/缓存场景对**中途角色**的处理。`instructions` 不代表注入发生的历史位置。 | `user` 输入项；工具回执用有调用 ID 的原生输出项。 |
| DeepSeek Chat Completions | `messages[]` 为 `system`、`user`、`assistant`、`tool`；官方 Chat API 未列 `developer`。 | 无独立顶层系统指令字段。 | 官方明确支持中途插入 `system`；不支持在 Chat 历史中人工插入任意工具调用。 | `user`；工具结果只随合法调用配对。 |
| DeepSeek Responses | `input[]` 消息项支持 `user`、`assistant`、`system`、`developer`，以及相应类型化工具项。 | `instructions` 被放在最先的系统位置。 | 官方明确支持中途 `system`；虽然 `developer` 语法可接受，**语义按 `user` 处理**，不能拿它承载高优先级控制。 | `user`；原生工具输出需配对。 |
| Claude 原生 Messages | 常规 `messages[]` 为 `user`、`assistant`，工具调用/结果在其内容块；指定新模型还接受流内 `system`。没有 `developer`。 | `system`。 | 新版官方专题文档限定部分 Claude 模型可在 `messages[]` 插入 `system`，从该位置起生效；文本消息须在 `user`（包括工具结果）或特定服务工具结果后，且下一个是 `assistant` 或数组末尾；不能插在工具调用和结果之间。首条有内容的消息不能是流内 `system`。可选的 `clear_at` 属于另一个 beta 能力。通用 API 参考页仍写“不支持流内 system”，与专题文档冲突；必须按精确模型/平台/版本选择，不能把新能力推广到所有 Anthropic 兼容端点。 | `user`；连续 `user` 回合可能被合并，故物理消息边界不能当成唯一来源边界。 |
| DeepSeek Anthropic 兼容 `/messages` | Anthropic 形状的 `messages[]` 与顶层 `system`，不声明 `developer`。 | `system`。 | DeepSeek 官方工具指南明确把此入口列为支持中途 `system`；具体位置限制与模型能力仍按该入口声明，不能直接套用 Claude 型号清单。 | `user` 内容块；DSH 的直接适配器还按模型开关决定是否输出中途 `system`。 |
| Gemini `generateContent` | `contents[]` 使用 `user`、`model`；函数调用/结果作为相应内容 Part。 | `systemInstruction`。 | 官方文档未给 `contents[]` 中途 `system` / `developer` 角色；多轮推荐 `user` 与 `model` 交替。 | `user` 内容；相邻注入与真实输入可能需要在一个用户回合中保留明确子边界。 |
| Gemini Interactions | `input` 是 `user_input`、模型输出、函数调用/结果等类型化步骤。 | `system_instruction`。 | 未见流内 `system` / `developer` 步骤的官方契约。 | `user_input`。 |
| Cohere Chat V2 | `messages[]` 文档列 `system`、`user`、`assistant`、`tool`，按时间顺序传入。 | 未见独立于消息流的系统指令字段。 | `system` 有流内**语法位置**；本轮未找到官方对“中途修改系统指令优先级与持续范围”的明确保证。能力表应标成待具体模型确认。 | `user`；有配对调用时才用 `tool`。 |
| Google Code Assist 及其他兼容网关 | 其外层封装不是独立的通用公开消息协议；须查该端点的请求契约。 | 依端点而定。 | 不能仅凭底层 Gemini 或 OpenAI 的文档推定代理网关完整保留流内高优先级角色。 | 经端点确认后使用相应 `user` 型输入。 |

官方依据：[OpenAI Chat 角色参考](https://developers.openai.com/api/reference/cli/resources/chat)、[OpenAI Responses 输入与 instructions](https://developers.openai.com/api/reference/cli/resources/responses/methods/create)、[DeepSeek Chat 请求](https://api-docs.deepseek.com/api/create-chat-completion/)、[DeepSeek Responses 请求](https://api-docs.deepseek.com/api/create-response/)、[DeepSeek 中途 system 与工具调用说明](https://api-docs.deepseek.com/guides/tool_calls/)、[Claude 流内 system 专题与位置规则](https://platform.claude.com/docs/en/build-with-claude/mid-conversation-system-messages)、[Claude 通用 Messages 参考](https://platform.claude.com/docs/en/api/messages/create)、[Gemini GenerateContent](https://ai.google.dev/api/generate-content)、[Gemini 多轮角色规则](https://ai.google.dev/gemini-api/docs/generate-content/text-generation)、[Gemini Interactions](https://ai.google.dev/api/interactions-api-v1)、[Cohere Chat V2](https://docs.cohere.com/reference/chat)。此表是 2026-09-17 的文档快照，非实际请求验收。

### DSH 实际做法：两个不同的动态通道

1. **中立消息保留来源。** DSH 内部 `Message` 只有 `system`、`user`、`assistant` 三种中立角色，同时每条有稳定 ID 与必填 `source`；`source.form` 可区分 instructions、snapshot、notice、relay、recall，属于宿主的生产者/审计元数据，不是额外的 Provider 高优先级角色。子代理发给相邻代理的消息生成 `user`，正文有 `Agent <id> sent a message:` 前缀，来源另存 `senderSessionId`；子代理结束通知也作为 `user`，目标轮次续跑同样走 `user`。运行时上下文快照、AGENTS 工作区指令、目标自动续轮也分别生成带来源的 `user` 消息。来源：`D:\dsh\deepseek-harness\packages\llm\llm\src\message.ts:50-63,130-167,200-210`、`packages/subagent/subagent/src/continuation-messages.ts:14-72,130-159`、`packages/core/agent-loop/src/runtime-context.ts:108-157`、`packages/context/agent-instructions/src/index.ts:177-224`、`packages/goal/goal-round-driver/src/index.ts:170-184`。其中 `form:'instructions'` 表示 DSH 的上下文类别，**不能**解释为供应商的系统权威。
2. **真正改变操作级指令时，按路由能力更新 system。** DSH 的 `SystemPromptProjection` 以第一条系统消息为基线。只有已准备好的路由声明 `systemPromptUpdate:'in-history'`，且同一请求序列继续、渲染内容非空并变化时，才追加新的流内系统节点；其他情况替换头部节点，并清空旧的后续节点。模型能力由适配器显式提供，缺省含义是“仅首条 system 可读”；默认 DeepSeek 模型表中只有 `deepseek-flash` 声明该能力，其他型号可按配置声明。来源：`packages/core/agent-loop/src/runtime-context.ts:52-104`、`packages/core/agent-loop/src/agent.ts:360-379`、`packages/llm/llm/src/types.ts:384-392`、`packages/llm/llm-deepseek/src/common/models.ts:5-32`、`packages/llm/llm-deepseek/src/config.ts:68-81`。
3. **DeepSeek Chat 适配器**按中立历史顺序逐条输出 `system`，不会把中途 `system` 抽到请求开头；独立的 `options.system` 才作为首条 `system` 插入。来源：`packages/llm/llm-deepseek/src/protocols/chat-completions/serialize.ts:239-255,374-384,437-442`。
4. **DeepSeek Anthropic 兼容 Messages 适配器**只对声明 `in-history` 的模型输出后续 `role:'system'`，并把 DSH 允许的“先记录更新、后记录用户输入”重排到该接口要求的用户/工具结果之后、下一条助手消息之前；不满足前置用户回合则拒绝。未声明能力时历史系统文本进入顶层 `system`，正常循环通过前述头部替换避免随意产生这类历史系统。来源：`packages/llm/llm-deepseek/src/protocols/messages/serialize.ts:55-56,71-103,124-130`。
5. **通用 `pi-ai` 适配器**只有一个 `systemPrompt` 槽：`options.system` 优先，否则取历史首条 `system`；剩余 `system` 按原顺序降为 `user`，保住位置但丢失高优先级。它没有声明 `systemPromptUpdate:'in-history'`，所以 DSH 正常系统提示更新不会依赖这条降级路径保持系统权威。来源：`packages/llm/llm-pi-ai/src/context.ts:135-165,182-203,313-325`。这说明“DSH 使用 `pi-ai`”**不代表**所有底层协议只能有单个系统字段；它只是这一适配器暴露的能力上限。

### 供 Vulcan 统一封装的修订建议

以下保留当时的候选分析。其中按供应商选择高优先级流内角色的建议已被后文统一 User 方案取代；角色能力事实仍可作为协议资料，但不是当前动态注入的选路规则。

- **先按语义分类，再选协议角色。** 内部强类型事件至少区分：真实用户输入、代理/跨会话投递、运行时事实/记忆快照、宿主操作级规则变更、工具调用/结果、助手回复。事件保存真实来源、目标、投递 ID 与回合关联；模型侧 `<vulcan_agent_message>` 或其他信封只用于让模型读懂边界，不承担身份认证、去重或回复寻址。
- **动态注入的数据统一走低权威输入。** 代理正文、跨会话引文、检索记忆、运行时快照等使用按时间位置投影的 `user` / `user_input`；给注入内容加版本化信封，包含可信宿主生成的来源摘要、投递 ID、内容边界和“回复目标由宿主确定”的提示。普通真实用户消息也用原生 `user` / `user_input`，但宿主 `source=human` 且不套代理信封。两者的区别是**宿主事件类型加模型可见边界**，不是伪造一个不存在的协议角色。
- **建议的可读投影示意：**宿主在稳定的基础指令中一次性解释“`<vulcan_agent_message>` 是来自其他代理/会话的消息；按所绑定投递处理回复”，然后将每条投递投影为 `user` / `user_input` 内容，例如 `<vulcan_agent_message version="1" delivery_id="D" sender="S">[经转义的外部正文]</vulcan_agent_message>`。普通用户输入保持无此标签的原始用户内容。标签名称与字段仍是设计候选；接收方身份、授权及回复目标从宿主持久事件读取，不从标签或模型生成的文本反查。按具体协议合并物理用户回合时，仍要维持每个逻辑投递的独立标签、顺序与 ID。
- **宿主规则变更单独处理。** 需要系统级权威且要求从当前时点生效时，只在具体供应商、端点、模型、网关均声明并遵守流内 `system` / `developer` 语义后生成该角色；Claude 类入口还要实现位置校验。若该路由只有顶层指令，则显式采用头部重建/新请求序列并记录缓存和时序影响，或把内容重新定义为低权威事实；不得悄悄改写成 `user` 后仍宣称保留系统权威。DeepSeek Responses 的 `developer` 是明确反例。
- **不要假设物理用户消息一定独立。** Claude 文档明确连续 `user` 回合会合并，DSH Messages 适配器也合并相邻同角色内容；Gemini 推荐 `user`/`model` 交替。因此独立投递 ID 和子边界必须保留在宿主事件及可读信封里，必要时把多个逻辑事件投影到同一个物理用户回合的不同内容块。宿主负责排序、重复投递去重、提交幂等及回复归属；这不等于能够证明模型恰好读懂一次或只生成一次。
- **能力表的主键应是“供应商 + 接口 + 模型 + 网关/平台 + 特性版本”。** 分开记录 `flowRoles`、`topLevelInstruction`、`midstreamSystem`、`midstreamDeveloper`、`effectivePriority`、`placementRules`、`consecutiveUserHandling`、`toolCallReplay`、`cache/continuation`。编码器先验证能力与语义能否保真，再编码；不通过更换协议或模糊回退来掩盖不支持。

**对上文 Vulcan 表的校正：**那张表描述的是“目前 Vulcan 会把什么送到上游”，并不等于“上游协议只能这样”。尤其“Anthropic 只能顶层 system”和“Chat 不能流内 system”只能作为**现有 Vulcan 生产转换的限制**；官方 DeepSeek 与部分新 Claude 模型已经公开支持流内 `system`。这扩大了未来实现的可选空间，却不会自动修复现有 Chat 抽头、Anthropic 置顶等时序问题。现阶段能从源码确认的是顺序被改写的风险，不能仅据此断言规范会话历史丢失或某次模型回答已出错。

## 补充：动态注入、时序位置与 Provider 缓存前缀静态审计（2026-09-17）

本节只分析当前源码，不运行 VulcanCode、不发模型请求、不调试、不执行测试。这里的“打破缓存”严格指**相邻请求的已编码提示前缀在某处发生变化，使该变化之后的旧前缀不能按原样匹配**；实际缓存读取还取决于模型、网关、缓存策略、断点、长度与存活时间，不能由静态源码推定。OpenAI 与 Claude 的官方说明都以匹配的提示前缀为缓存依据：[OpenAI Prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching)、[Claude Prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching)。这不同于本地 Prompt Composer 片段缓存、V1 规范历史是否丢失，也不同于 Provider `previous_response_id` 一类延续资格。

### 已定位的动态内容及物理位置

| 来源与触发 | V1 中立请求中的位置和角色 | 静态结论 | 主要源码 |
| --- | --- | --- | --- |
| **继续当前回合时的隐藏模型指引**：提交 `hidden_model_guidance`。 | `continue_turn` 先记录当前 `UserInput`，再记录 `ttl_turns=1` 的 `SystemContext(Developer)`。V1 投影把当前仍有效的 TTL 状态放到历史事件之前的 `prefix_states`，并跳过原位置 Upsert；sink 在动态层与锁定基线后、按序历史前发送它。 | **已确认的时点前移**：事件产生于当前回合，却在本次物理请求中出现在先前历史前。一个逻辑 Turn 内的多次 API 不重复消耗 TTL；`TurnBoundary` 才使它过期，因而它可能持续覆盖本 Turn 的多个请求。原事件仍在规范日志中。 | `crates/vulcan-service-kernel/src/conversation_submission.rs:2249-2264`；`crates/vulcan-session-core/src/v1/document/turns.rs:209-282`；`crates/vulcan-session-core/src/v1/projection.rs:250-300,403-449,493-507`；`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs:1161-1185` |
| **Goal / Plan 的持久模式变更**。 | 非 TTL `SystemContext` 的 Upsert、Remove 按 V1 事件序号投影为 `Developer`，sink 先保留其历史位置。 | **中立层保序，部分出站协议不保序**：Chat 转换把 `developer` 转为 `system` 后合并到第一条；Anthropic Messages 汇集为顶层 `system`；Codex/标准 Responses 的 `System` 汇入顶层 `instructions`，但 `Developer` 仍留在 `input` 中，保持顺序。模式变更不能被笼统视为前端历史损坏，模型实际看到的生效位置依路由改变。 | `crates/vulcan-session-core/src/v1/document/modes.rs:80-95,823-869`；`crates/vulcan-session-core/src/v1/projection.rs:493-522`；`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs:1178-1185`；`crates/vulcan-llm-core/src/responses_to_chat.rs:448-469,771-806`；`crates/vulcan-llm-core/src/responses_to_anthropic.rs:106-126`；`crates/vulcan-ai-protocols/src/openai/internal_input.rs:192-230` |
| **Plan 模式的请求级适配层**：Plan 启用期间每次构建请求，以及退出后的待投递一次性上下文。 | 服务层从最新 `plan_mode` 事件再取 `content`，传入 `session_plan_context`；Prompt Composer 生成动态 `Developer` 段，V1 sink 在锁定基线和全部历史前发送。原 `plan_mode` 事件同时仍在按序历史中。 | **已确认的前置重复来源**：同一份 Plan 内容既来自前置动态段，又来自 V1 历史事件。若开启、关闭或改动内容，早期前缀发生变化；前置副本还弱化了事件原有时点。是否形成完全相同的上游两条消息由各编码器决定，但中立输入已有两份。 | `crates/vulcan-service-kernel/src/session_plan_runtime.rs:84-114`；`crates/vulcan-service-kernel/src/webui_prompt_lifecycle/precheck_continue.rs:781-783`；`crates/vulcan-agent-runtime/src/agent_runtime/provider_runtime_context.rs:178-187,341-359`；`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs:1157-1185,1364-1395` |
| **VMM 画像刷新差量及其他 Persistent 上下文块**。 | 画像差量由 `InjectionKind::Persistent` 产生，进入 `persistent_blocks`；V1 sink 将它并入 `dynamic_prompt_layers.runtime_context`，作为靠前的 `User` 消息，位于锁定基线和会话历史之前。 | **不是 V1 顶部 `system`，但仍是早期前缀改写风险**：画像刷新若改变块内容，过去可复用的后续前缀可能不再匹配；通用 Persistent 块也走同一位置。若在压缩后暂无可锚定真实用户消息，活动记忆临时基线同样进入此位置，属于压缩边界上的显式例外。非 V1 请求走下文所列的首条 System。 | `crates/vulcan-agent-runtime/src/agent_runtime/vmm_hooks.rs:1296-1315,1826-1877`；`crates/vulcan-context-injection/src/context_injection/state.rs:314-342`；`crates/vulcan-agent-runtime/src/agent_runtime/context_injection_runtime.rs:37-67`；`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs:1153-1175,1364-1395`；`crates/vulcan-vmm/src/session_state.rs:1766-1790` |
| **运行环境上下文**：工作区路径和执行模式。 | Prompt Composer 把 `Environment Context` 定为动态 `User` 段，经 `runtime_context` 在历史前发送。 | 平常内容稳定；工作区或执行模式变化时会改写早期前缀。它属于请求级事实，不是按历史事件追加。 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_runtime_context.rs:174-179,210-235,341-359`；`crates/vulcan-prompt-composer/src/node/mod.rs:143-156`；`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs:1388-1395` |
| **未标记基线的系统 Hook 与动态工具说明**。 | `baseline_provenance=None` 的 Hook 拼入动态 `system`；`tool_guidance` 也以 `System` 放在历史前。 | 存在可前置的扩展槽。工具与媒体能力变化本来就可能改变请求，本轮按用户指定将其与无关的记忆/模式重排分开；实际生产者和变化频率仍按调用链逐项核对。 | `crates/vulcan-agent-runtime/src/agent_runtime/provider_runtime_context.rs:341-359`；`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs:1364-1395` |

### 生产编码链与协议边界

普通 V1 请求的权威链为 `provider_v1_sink` 重建中立消息 → `ProviderExecutionClient::encode_request` → `encode_internal_responses_request` → `project_internal_request_for_provider` → HTTP transport；直接读取独立 `AnthropicProfile` 或 `CohereProfile` 的编码结果，不能替代这条生产链。来源：`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs:1157-1185,1308-1324`、`crates/vulcan-provider-adapters/src/execution/request_encoding.rs:238-286`、`crates/vulcan-ai-protocols/src/openai/internal_input.rs:96-125,192-230`、`crates/vulcan-llm-core/src/internal_media.rs:316-421`。

| 生产目标 | 高优先级内容的最终位置 | 对中途 `Developer` 的影响 |
| --- | --- | --- |
| Codex / 标准 Responses | `System` 汇入顶层 `instructions`；`input` 保留其他消息。 | `Developer` 保留在 `input` 原位置；但 V1 TTL 指引和 Plan 请求级副本在进入此转换前就已提前。 |
| Chat Completions | `Developer` 转 `system`，全部 `system` 合并为第一条消息。 | Goal / Plan 的历史 Developer、TTL Developer 都失去流内时点；变化也会改写靠前的 System 前缀。 |
| Anthropic Messages | 顶层 `instructions` 与输入中的 System/Developer 拼成顶层 `system`；`messages` 保留 User/Assistant。 | 中途 Developer 失去流内时点。当前生产转换参见 `responses_to_anthropic.rs`，不能套用独立 Anthropic Profile 的缓存断点行为。 |
| Gemini GenerateContent / Code Assist / Interactions | System/Developer 合并进各自顶层 `systemInstruction` 或 `system_instruction`。 | 中途 Developer 失去流内时点。 |
| Cohere Chat V2 | 当前 Responses-first `UpstreamProtocol` 无 Cohere 分支；独立 Profile 可编码顺序消息。 | 不能把独立 Profile 的顺序能力当成现行普通 V1 出站行为。 |

来源：`crates/vulcan-llm-core/src/responses_to_chat.rs:43-60,771-805`、`crates/vulcan-llm-core/src/responses_to_anthropic.rs:42-59,106-126`、`crates/vulcan-llm-core/src/responses_to_gemini.rs:21-140,533-607`、`crates/vulcan-provider-adapters/src/execution/route.rs:284-300`。

缓存机制也必须按**生产链**表述：Codex Responses 将稳定 `cache_scope_id` 写入 `prompt_cache_key`，标准 Responses 投影会移除该键；独立 Anthropic Profile 有 `cache_control` 的显式断点实现，但普通共享入口要求默认缓存策略，本轮未找到其 Anthropic 生产投影插入同样断点的证据。`prompt_cache_key`、Provider continuation、实际 cached tokens 是三个不同观测量。来源：`crates/vulcan-ai-protocols/src/openai/internal_input.rs:192-204,256-270`、`crates/vulcan-llm-core/src/internal_media.rs:428-442`、`crates/vulcan-ai-protocols/src/anthropic.rs:1273-1326`。

### 不应误报为同一问题的路径

- VMM **基线画像**使用 `MemoryProfile` 类型化基线来源，进入锁定的 System 基线；它与“刷新差量”不是一条注入路径。基线在新会话、压缩或权威来源变化时改变，本来就是基线切换。来源：`crates/vulcan-agent-runtime/src/agent_runtime/vmm_hooks.rs:1732-1752`、`crates/vulcan-agent-runtime/src/agent_runtime/provider_runtime_context.rs:395-505`。
- VMM **稳定锚点**按目标消息 ID 前置到对应的物理 `UserInput`，普通目标上下文也附着当前用户消息。这保留了宿主指定的消息位置；若锚点后来修改既有消息内容，仍应单独审计历史重写资格。来源：`crates/vulcan-agent-runtime/src/agent_runtime/context_injection_runtime.rs:61-112`、`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs:1153-1156,1187-1225`、`crates/vulcan-service-kernel/src/session_goal_lifecycle/prompt_context.rs:243-301`。
- **非 V1 / `ExcludedNonSession` 也是实际存在的执行分支**：`ControllerConfig::provider_prompt_layers_with_persistent_injection` 把 Persistent 块合进 `runtime_context`，Controller 请求构建器将完整渲染结果作为首条 System，再追加历史；环境上下文与 Plan 动态 Developer 也随结构化层进入这条合成 System。已注册 V1 会话在出站闸门重建后改投靠前的 User / Developer，并将 `exact_request.continuation` 清空。因此普通 V1 和非 V1 的同一注入块既有**位置差异**，也可能有**权限角色差异**；不能只看闸门前候选来描述 V1 的最终请求。来源：`crates/vulcan-core/src/types.rs:151-190`、`crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_vulcan/request.rs:177-194`、`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs:434-459,1157-1165,1307-1317`。
- **Transient 注入目前没有找到物理消费者**：`ContextInjectionState` 生成 `transient_prefix`，运行时携带它并用它参与 `HistoricalRewrite` 判定；本轮对非测试源码检索，只见状态、指纹、判定及透传，未见 V1 sink 或非 V1 请求构建器将正文写入模型消息。这是独立的疑似投影缺口，不能算作“被放到顶部”的缓存问题；需要继续沿全部出站适配器核实，不能据静态缺少调用就声称已观察到运行时丢失。来源：`crates/vulcan-context-injection/src/context_injection/state.rs:316-343`、`crates/vulcan-agent-runtime/src/agent_runtime/context_injection_runtime.rs:73-103`、`crates/vulcan-core/src/types.rs:281-377`。
- Workbench 同时有把 `hidden_model_guidance` 追加在请求提示词尾部的旧表示，而 V1 文档将它记录为 TTL Developer 事件；V1 权威请求从投影重建。本轮只确认**两条表示路径存在**，未证明同一个最终请求会重复携带它，后续应在明确的 V1 / 非 V1 分支下核对。来源：`crates/vulcan-agent-runtime/src/workbench_prompt_turn.rs:802-809,1111-1114`、`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs:1153-1185,1307-1310`。
- `provider_rewrite_fingerprint` 与 `continuation_reuse_is_proven` 是 Provider 延续游标的本地安全闸门；即使拒绝继续并回放历史，也不能由此推断上游提示缓存命中或未命中。来源：`crates/vulcan-core/src/types.rs:183-226`。

### 候选修正方向与后续核对

1. 将可变内容按**来源、目标时点、有效期、所需权威和可重放身份**建模；基础规则与锁定画像维持稳定前缀，新的记忆事实、代理消息和当前回合指引优先绑定当前事件或用户输入。若要求高优先级流内生效，只在协议、模型、网关均明确支持时投影相应角色；否则显式记录“需要重建顶部指令”这一语义与缓存代价，不能静默改写。
2. 让 Plan 只走一个模型可见来源；若 V1 事件已是权威，应先审计并移除请求级副本的必要性。继续回合的 TTL 指引应重新确定是否真的需要 `Developer` 权威；若需要，应按路由能力决定时序投递，不应由通用投影无提示地提前到所有历史前。
3. VMM 差量可评估改为带来源和版本的时间线事件或当前用户锚点；压缩后无锚点的临时基线保留显式特殊状态。改动前必须保持租约、撤销、去重、压缩恢复及 Provider 延续资格的现有契约。
4. 若后续进入实施，先为**同一会话相邻两次请求**定义已编码请求的逐项前缀比较，再核对上游 `cached_tokens` / `cache_read_input_tokens` 等实际观测；本轮不运行这类验证。尤其要区分 `tools`、顶层指令、消息数组与显式缓存断点：稳定的 cache key 本身不能挽回内容变化。

待继续核对：其他动态 Hook 的实际生产者（当前内建 VMM Hook 已有基线来源，未发现另一个非测试内建生产者）；非 V1 分支的具体产品触发面；TTL 指引过期后对连续请求前缀的具体影响；Transient 是否在其他适配层被消费。各协议的当前生产编码形式已列在上表，但具体上游模型是否支持流内高优先级语义仍要按“供应商 + 端点 + 模型 + 网关”确认。以上静态结论不得写成“所有路由都已发生实际缓存未命中”。

## 补充：按流内角色能力分级投递与确定性接收（2026-09-17）

> 历史候选：本节的 Developer → System → User 分级已被下一节统一流内 User 方案取代，不能再据本节要求各供应商分别选择动态提示词角色。

### 对所提分级规则的判断

这套规则适合成为**可信宿主动态控制消息**的出站选择顺序：具体上游确实支持流内、保序且具有预期优先级的 `developer` 时用 `developer`；否则，若具体上游支持流内、保序且具有预期优先级的 `system`，用 `system`；两者都不支持时，允许降为带来源标签的 `user` **仅限原本就可按用户作用域解释的通知或数据**。`developer` 的存在与它的权限不能只由请求 Schema 推断：DeepSeek Responses 接收 `developer`，但官方明示按 `user` 处理；Claude 的中途 `system` 只对指定模型和满足位置约束的消息开放。能力键应至少包含供应商、端点/协议、模型、网关及特性版本，不能做成仅按“Chat/Responses/Anthropic”判断的全局开关。依据：[OpenAI Chat 角色](https://developers.openai.com/api/reference/cli/resources/chat)、[OpenAI Responses 输入与 instructions](https://developers.openai.com/api/reference/cli/resources/responses/methods/create)、[DeepSeek Responses](https://api-docs.deepseek.com/guides/responses_api/)、[Claude 中途 system 消息](https://platform.claude.com/docs/en/build-with-claude/mid-conversation-system-messages)。

还需要把**权限**与**功能节点**分成两维。宿主生成的身份、模式、投递规则等控制语句，经过来源审计后才可选择高优先级流内角色；另一个代理的正文、工具结果、检索文本、记忆事实以及用户提供的目标正文是外来数据，默认保持 `user` 或原生工具结果。可以在同一时间点给高优先级的宿主控制语句和低优先级的数据分别赋予同一个逻辑事件 ID，但不能把外来正文夹进高优先级消息后借标签提升权限。Claude 官方对流内 `system` 尤其明确：第三方工具结果和检索文档不应以此角色投递。对于确实必须具备高优先级才能正确执行的规则，如果路由不支持保序角色，静默改成 `user` 会改变语义；应显式拒绝该投影，或按产品契约开启新的顶部基线时期并承认缓存前缀变化。依据：[Claude 中途 system 消息](https://platform.claude.com/docs/en/build-with-claude/mid-conversation-system-messages)。

### 哪些内容目前被提前到顶部

这里的“顶部”分两步判断：先看 V1/非 V1 的中立请求是否已把事件前置，再看协议编码是否再次收集到请求级 `instructions`、`system` 或首条 Chat `system`。**稳定基线在顶部是设计行为；有历史时点的动态消息被无差别收集才是时序缺陷。**

| 当前来源 | 目前中立层位置 | 当前出站后果 | 处理判断 |
| --- | --- | --- | --- |
| V1 按事件顺序的 Goal/Plan `SystemContext(Developer)`，以及任何未来的 `SystemContext(System)` | 原事件位置；Goal/Plan 现有生产者主要为 `Developer` | Chat 把 Developer 改成 System 再合并到首条；Anthropic/Gemini 汇至顶层；若是 System，甚至在共享 Responses 编码阶段就被抽入顶层 `instructions` | 这是明确需要修正的流内时点丢失；System 不能靠只修改最终 Chat 转换器挽回。来源：`crates/vulcan-session-core/src/v1/document/modes.rs:80-95,823-869`、`crates/vulcan-ai-protocols/src/openai.rs:1063-1088`、`crates/vulcan-llm-core/src/responses_to_chat.rs:448-469,771-806`。 |
| V1 `continue_turn` 的 TTL 隐藏指引 | 事件由本次继续回合产生，但投影为历史之前的 `Developer` 前缀 | Chat/Anthropic/Gemini 继续上提；Responses 虽保留 Developer 角色，位置也已经提前 | 先修 V1 时点模型及有效期，再修协议转换；不能只靠保留 Developer 角色。来源：`crates/vulcan-session-core/src/v1/document/turns.rs:209-282`、`crates/vulcan-session-core/src/v1/projection.rs:403-449,493-507`。 |
| 请求级 Plan 动态 `Developer`、代码库工具路由指引，以及压缩/提交摘要用途的动态 `Developer` | 位于历史之前 | Chat/Anthropic/Gemini 提升到顶部；Plan 历史事件另存一份，形成前置副本 | 分别审计是否是本次请求控制还是历史事件；Plan 应消除双来源，避免同一事实在不同时间点重复。来源：`crates/vulcan-service-kernel/src/session_plan_runtime.rs:84-114`、`crates/vulcan-agent-runtime/src/agent_runtime/provider_runtime_context.rs:178-187,267-275`、`crates/vulcan-agent-runtime/src/agent_runtime/execution_preparation.rs:411-415`。 |
| V1 动态 `System`：未标记基线来源的 System Hook，以及工具说明 `tool_guidance`；显式 `SystemRoleHint::System` 是开放入口，但本轮未发现内建非测试生产者 | 位于历史之前 | 共享 Responses 编码把所有 System 拼入顶层 `instructions`；仅凭后续线路体不能恢复原始时点 | 区分稳定规则、因工具目录变化而变的规则和真正事件更新；不能把全部 System 一律视作缓存缺陷。来源：`crates/vulcan-agent-runtime/src/agent_runtime/provider_runtime_context.rs:341-359`、`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs:1364-1395`、`crates/vulcan-session-core/src/v1/projection.rs:769-775`。 |
| 非 V1 / `ExcludedNonSession` 的 Persistent 记忆差量、环境上下文及动态 Plan 等 | 请求构建时完整渲染成第一条 System | 从构建阶段即失去原角色和发生时点，之后的协议修正无法自行拆回 | 需要让非 V1 也输出带来源的中立节点；否则同一注入在 V1 与非 V1 权限不同。来源：`crates/vulcan-core/src/types.rs:151-190`、`crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_vulcan/request.rs:177-194`。 |
| V1 VMM 刷新差量及环境上下文 | 历史之前的 `User`，不是 System | 没有被 System 合并，但内容变动仍可能改写靠前的已编码前缀 | 需要调整事件锚点或承认请求级前缀变化；不应误报为“被放到顶部 System”。来源：`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs:1153-1185,1364-1395`。 |
| VMM 基线画像、稳定系统规则、技能/插件与子代理身份基线 | 锁定基线 | 本来就以高优先级前缀投递 | 初次创建或显式换基线属正常；应保持可识别的基线版本，不能为了形式上的“流式”把它们强行降级。来源：`crates/vulcan-agent-runtime/src/agent_runtime/vmm_hooks.rs:1732-1752`、`crates/vulcan-agent-runtime/src/agent_runtime/execution_preparation.rs:260,568`。 |

对仍处于中立消息序列中的 `System`，当前**线路体第一次丢失时点**发生在 `encode_codex_instructions`：它遍历所有中立 `System`，拼进顶层 `instructions` 并将其从 `input` 排除。内部 replay sidecar 仍记录原消息及索引，但当前目标转换器没有依它恢复带来源的流内 System。随后 Chat 的 `append_input_item` 再把 `developer` 改成 `system`，`collapse_system_messages` 把所有 System 抽走合并为 `messages[0]`；Anthropic 汇总到顶层 `system`；Gemini 汇总到 `systemInstruction` / `system_instruction`。因此不能只删除 Chat 的最后一次合并：必须从带来源、时点和权限意图的中立事件开始，一直保留到目标路由的最后一次编码。V1 TTL 和非 V1 合成 System 的前移甚至发生在此编码之前，已在上表单列。来源：`crates/vulcan-ai-protocols/src/openai.rs:1063-1088`、`crates/vulcan-ai-protocols/src/openai/internal_input.rs:96-125,132-166,192-230`、`crates/vulcan-llm-core/src/responses_to_chat.rs:448-469,771-806`、`crates/vulcan-llm-core/src/responses_to_anthropic.rs:106-126`、`crates/vulcan-llm-core/src/responses_to_gemini.rs:21-140,533-607`。

### 标签、固定出站与固定接收的契约草案

1. **中立事件**：先保存 `event_id / delivery_id`、功能节点种类、宿主验证的发送者和接收者、产生时点、有效期、内容来源与信任等级、所需优先级、正文引用。标签名只是模型可见表示，不能替代这些权威字段。对 Goal、Plan、Memory、AgentDelivery、RuntimeUpdate 等节点采用有版本的受控种类表；具体文本标签可沿 `<vulcan_agent_message ...>` 思路，但属性和正文必须转义，并且标签不能让正文自行宣布高权限。
2. **固定出站**：同一个“路由能力 + 节点种类 + 信任等级 + 所需优先级”确定唯一编码器。编码器先决定 `developer`、`system`、`user` 或原生工具结果，再渲染该节点的可识别标签，并给宿主保存投影凭据：逻辑事件 ID、物理角色与位置、投递版本、目标回合及转换策略。编码器不得先把所有 System 混成一段没有来源的字符串。Claude 流内 System 的前后邻接、工具调用配对等限制也属于编码器的合法性条件。
3. **固定接收**：Provider 返回时按本次请求、回合、投递 ID 与工具调用 ID 关联到宿主记录，再解析 assistant 正文和原生工具调用。模型回显的标签不能作为发送者身份、目标会话或回报归属的权威依据；网关可能合并消息，模型也可能省略、修改或伪造标签。普通最终答复归属当前回合；显式发往另一会话的回复应使用带目标 ID 的宿主工具调用，并核验目标与投递权限。这样“固定出/固定收”是一个可审计编解码契约，并不要求模型逐字反包裹输入标签。
4. **现有复用点及缺口**：`ConversationPayload::render_agent_message_text` 已把代理消息转义成 `<agent_message>` 用户信封，宿主结构另存 `delivery_id`、`sender`、`recipient`；现有模型可见信封没有包含接收者/投递 ID。`ProviderSystemUpdateLoweringPolicy` 已区分 NativeSystem、UserWrapper、Reject，默认保守回退，但 canonical provider-visible bridge 当前对 NativeSystem 明确报错，因此还不具备完整原生流内 System 出站链。应在这两个基础上扩展类型化来源和投影凭据，不把“现有枚举存在”当作功能已贯通。来源：`crates/vulcan-core/src/types.rs:771-806`、`crates/vulcan-agent-runtime/src/agent_runtime/execution_models.rs:105-151`、`crates/vulcan-agent-runtime/src/agent_runtime/provider_system_update_lowering.rs:12-90,125-135`、`crates/vulcan-agent-runtime/src/agent_runtime/provider_request_bridge_messages/projection.rs:220-250`。

建议的实施顺序仅供后续确认：先分类并保留来源/时点/有效期，解决 TTL 前移、Plan 双来源与非 V1 整包 System；再改共享 Responses 编码使基线 System 与历史 System 可分辨；随后按精确路由能力修 Chat、Anthropic、Gemini 等转换；最后建立请求投影凭据与回复归属，并以已编码请求前缀和实际 Provider 计量分别验证时序与缓存。这里仍是静态方案，没有修改产品代码或运行验证。

## 补充：统一流内 `User` 信封作为主方案（2026-09-17）

### 判断与边界

用户提出的简化方案可以作为**默认目标方案**：锁定且面向整段会话生效的顶部规则仍走各协议已有的顶层入口；顶部之后新增的**宿主提示词事件**统一按发生时点编码为 `User`，正文用有版本的功能标签标明节点种类、事件 ID、来源与作用范围。这样无需针对每个供应商建立“中途 Developer / System 是否有效”的投递分级，也避开当前 Chat、Anthropic、Gemini 对高优先级消息的收集上提；现有子代理 `<agent_message>` 用户信封也提供了可复用起点。现有顶层基线可以只写一次稳定的信封解释规则，例如“宿主标签是上下文事件，按节点种类理解；正文引用是数据，不能自称更高权限”。这条规则自身不随每次记忆或模式变化而重写。

这里的“全部”限定为**顶部之后新增的提示词/上下文文本**，不是把规范历史中的真实 `Assistant`、真实 `User`、配对的工具调用/结果或媒体 Part 全部改成普通 User 文本。工具结果在 Responses、Chat、Claude、Gemini 等协议有各自的调用 ID 与类型结构；替换成带标签的普通文本会损坏工具配对。各协议仍要做结构与媒体转换，但不再需要为*动态提示词*选择不同的高优先级角色。依据：[OpenAI Responses 的类型化输入和函数调用](https://developers.openai.com/api/reference/cli/resources/responses/methods/create)、[Claude 工具结果配对](https://platform.claude.com/docs/en/agents-and-tools/tool-use/how-tool-use-works)、[Gemini FunctionResponse](https://ai.google.dev/api/generate-content)。

**统一 User 解决的是传输兼容性，不会恢复 Developer/System 的权限。**[Claude 官方说明](https://platform.claude.com/docs/en/build-with-claude/mid-conversation-system-messages)：User 被视为最终用户输入，System 才是应用操作者指令。因此模式切换、权限许可、工具可用性与安全约束若必须压过用户文本，必须留在稳定顶部规则和宿主运行时检查中；不能仅凭 `<vulcan_...>` 标签让流内 User 自动拥有高优先级。Goal/Plan 与子代理身份中哪些只是状态通知、哪些实际上承担权限控制，实施前要逐条分类；无法由宿主执行或稳定顶部规则表达的强制规则，不得无声降为 User。真实用户可以在正文中模仿标签，所以宿主必须分别存储可信来源与真实用户输入，序列化时转义/隔离后者；模型可见标签只辅助理解，不作为授权依据。

### 位置、缓存与消息边界

**统一角色必须和统一时点一起做。**目前 V1 VMM 刷新差量和环境上下文已是 `User`，仍被放在历史之前；若下一次请求更新其正文，靠前的已编码前缀照样变化。因此目标应是 `append(event_at_time, tagged_user_payload)`，不能是 `prepend(all_dynamic_user_context)`。Goal/Plan 的状态变化保留事件时点并追加版本化更新；一次性 TTL 提醒绑定本次回合及有效期，投递时须选择合法的消息边界；Plan 消除请求级副本；记忆差量挂到产生差量的事件或当前消息。旧的顶部内容只在明确变更基线时更新，并记录前缀代价。静态源头参见上节的前移清单；实际缓存命中仍须以后续请求计量判断。

不能假定“每个逻辑事件恰好是一条物理 User 消息”。[Claude Messages API](https://platform.claude.com/docs/en/api/messages/create)会把连续 User 回合合并；当前 Vulcan [Anthropic 转换器](../../crates/vulcan-llm-core/src/responses_to_anthropic.rs)也合并相邻同角色块，[Gemini 转换器](../../crates/vulcan-llm-core/src/responses_to_gemini.rs)将相邻 User Part 合到一个 Content，Gemini 的对话指南要求 User/Model 回合交替，见[官方说明](https://ai.google.dev/gemini-api/docs/generate-content/text-generation)。因此每个节点要有独立的起止标签、转义后的正文和宿主 `event_id`；同一物理 User 回合可以有多个有序节点。接收与回复归属仍以宿主投递记录、当前回合和工具调用 ID 为准，不从模型回显标签反推。

工具循环是此方案仍需协议结构处理的明确例外：[Claude 工具结果规范](https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls)要求 `tool_result` 紧跟对应调用，并在 User 内容数组中排在所有文本之前；只涉及客户端工具时可以在结果后追加文本，但若同一 Assistant 回合尚有未完成的服务端工具，结果消息必须只含 `tool_result`，追加带标签的普通 User 文本会报 400。连续 User 又会合并，不能靠另发一条 User 绕开。统一策略应是“节点逻辑角色一律 User，物理投递必须选合法边界”；某事件若必须立即在不允许附加文本的工具续轮生效，应延期到下一个合法边界，或明确标记该方案无法满足其时效要求，而不能假造工具结果或偷偷前移顶部 System。这保留了一个小范围的协议**结构**适配，但不需要按供应商选择流内高优先级**角色**。

### 对现有代码的最小可行改动面（仅供后续实施评估）

1. 建立“顶部基线 / 流内宿主事件 / 真实用户消息 / 工具结果”的类型化区分，并在**中立请求边界**把所有流内宿主提示词统一降为带标签的 User；同时拒绝或显式报告漏网的历史 `System/Developer`，避免又被现有出站转换器前移。当前子代理 `render_agent_message_text` 是 User 信封，可复用格式与转义，但要补宿主投递关联。
2. V1 投影修 TTL 指引的前移、Plan 双来源，Goal/Plan 的状态事件保留时点；VMM 差量、环境上下文不再作为可变的历史前置 User。动态工具路由文字可评估为 User 提示，真实工具可见性与执行权限仍由宿主控制。
3. 非 V1 / `ExcludedNonSession` 不能再把 Persistent 记忆、环境与 Plan 动态层整包渲染成首条 System；先拆成同一套带来源的流内 User 事件。稳定顶部基线继续使用既有协议编码器；在保持这一不变量后，Chat/Anthropic/Gemini 原有合并顶部高优先级消息的逻辑可能无需逐供应商重写，但必须核对没有动态内容漏入顶部。
4. 明确统一信封版本、节点种类、正文转义与大小限制；在宿主保存逻辑事件到物理消息/Part 的投影凭据。模型输出归属沿用会话和投递 ID；跨会话发送仍须显式工具调用，不能由纯文本标签直接执行。

该方案确实比逐供应商支持流内高优先级角色更容易兼容、维护面更小；“最稳妥”的前提是接受动态提示词仅有 User 优先级，并让宿主承担模式/权限的强制执行。它**不会仅靠改角色**修复缓存前缀，也不能省掉现有各协议的工具和媒体结构适配。本节只是静态决策建议，尚未实施或验证。

## 补充：任务完成链与临时问答链必须分离（2026-09-17）

### 用户修正后的两类消息机制

这里不是三种同义的 `send_message` 模式，而是**任务完成链**与**运行中临时问答链**两个契约；任务完成链里又有启动和插队引导两种入口。两者都可能在目标模型的消息流中呈现为带不同 `kind` 的宿主 User 信封，但宿主状态机、回复方式和完成条件不同。

| 入口与方向 | 收件方应做什么 | 何时形成回复 | 发送方是否还要回应 |
| --- | --- | --- | --- |
| `start_task`：主代理启动子代理，或独立会话启动目标任务 | 执行完整任务 | 目标的**最终 Assistant** 是一次终态回复；宿主绑定原任务 ID 并回流 | 不需要为了“完成回执”再调用反馈工具；主代理可继续自己的任务。 |
| `steer_task`：主代理在运行中插队改变方向 | 在下一合法模型/API 边界接纳引导，继续原任务 | **不产生单独中途答复**；继续等同一个任务的最终 Assistant，工具准入回执只代表引导已接纳 | 不需要立即问答。 |
| `ask_question`：主代理问子代理，或子代理主动问主代理 | 收件方看到与任务/回合关联的临时问题，**调用专用工具**回答指定 `question_id`；随后原任务继续 | 回答工具只向回答方返回提交回执；宿主按 `question_id` 将**非终态**答案送给提问方，不能用最终 Assistant 代替 | 提问方取回答案后继续原任务，最后仍可产生自己的最终 Assistant。 |

临时问答要允许**双向**：主问子时子代理回答，子问主时主代理回答。尤其子代理应有专门的 `ask_parent` 能力，而主代理应有 `reply_question(question_id, answer)` 一类工具；不能要求子代理把问题写进最终回答，也不能要求主代理用自己的最终 Assistant 来回答临时问题。工具名只是候选。**2026-09-20 用户已选择异步问答：提问工具持久接纳后立即返回问题 ID，提问方可继续其他工作，需要答案时再单独等待。**不再采用“提问工具一直挂起”的早期候选；等待须支持结果快照、游标、可处理的新问题唤醒及显式取消。

### 已有源码与需要新增的状态

- 现有 `send_message(delivery_mode=interrupt)` 的说明和准入逻辑都是“下一次物理 API 边界引导当前任务，不取消执行”；这接近上表的 `steer_task`，其工具返回的投递 ID 不等于目标已回答。来源：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs:9-30,270-317`。
- 现有子代理最终结果有独立的父级 V1 `SubagentReturn` 回流，不能把这个终态通道改成一次必须回答的问答。来源：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_methods.rs:381-405`、`crates/vulcan-service-kernel/src/agent_stream/detached_subagent.rs:194-282`。
- `wait_agent` 已把普通 `SubagentMailboxEventKind::Message` 当作可唤醒事件，但该枚举没有“问题/答案/终态”的独立语义。`deliver_root_inter_agent_message` 能构造子到父的普通 mailbox 消息，却没有 `question_id`、等待答案及工具答复闭环。更关键的是，现有子代理策略显式禁用 `send_message` 和通用 `ask`，因此不能把现存 `root` 别名误称为子代理已能主动向主代理提问。来源：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_methods.rs:112-160`、`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tool_surface.rs:1416-1421`、`crates/vulcan-session-boundary/src/subagent.rs:52-69`、`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs:329-389`、`crates/vulcan-session-core/src/child_agent_policy.rs:12-26`。

新增的问答权威记录至少要包含 `question_id`、提问者会话/回合、接收者会话/回合、所依附的任务或投递 ID、问题正文、状态、答案正文、回答工具调用 ID、创建/回答时间及消费修订。状态至少区分 `pending`、`answered`、`consumed`、`cancelled/expired`。重复请求必须区分相同幂等键的重试与真正的再次回答：同键同正文返回原提交回执，同键不同正文或目标不匹配显式拒绝；已结算问题不能被改答覆盖。等待超时只结束本次等待，不等于问题自动失效或任务取消。目标暂停、终止、重启和父/子任一侧结束时的结算见 2026-09-20 重审；消息正文及标签不能决定谁有权回答，宿主应检查当前接收者、父子归属和 `question_id`。这些字段是候选契约，尚未核对最终存储 Schema。

**防止互等死锁：**主代理可能正在 `wait_agent` 中等待子代理的终态，子代理此时提问。提问事件必须让该等待立即返回“需要回答”，不受其他等待目标是否结束限制；主代理收到问题后调用回答工具，再选择继续等待。单独的问题等待工具也须能因反向提问返回，不占住接收方唯一执行通道。2026-09-20 用户进一步明确：只要所属子代理仍在运行，主会话的逻辑 turn 就不能正常结束；子到主的消息只向原 turn 插入，不另开业务任务。MainOnly 只停主会话、保留子代理时，允许子代理的问题自动恢复主会话处理。自然完成/空闲目标自动仅答疑的选择仍用于其他合法的空闲目标，答完回到空闲，不重开已完成任务；已暂停/停止的普通目标拒收新问题并返回明确状态。物理 API 或执行段已结束不等于逻辑 turn 已完成。

### 与统一 User 信封的关系

收件方看到的三种宿主消息可以共享标签编码器，但必须有不同的 `kind`：`task_start`、`task_steer`、`question`；临时答案由 `reply_question` 工具关联 `question_id` 送回提问方，终态答案由宿主绑定任务/回合回流。逻辑 `question` 的文本按真实到达时点插入为 User，不提前到 System；若所选协议正在不可插入普通文本的工具结果边界，则保留待投递状态并在下一个合法边界交给模型。标签给模型说明“此条必须调用回答工具且不结束原任务”，但宿主工具与状态机负责强制关联。最终 Assistant 的回流无需主代理再次用 `reply_question` 确认。

这一更正覆盖前文“一条协作消息一个目标回合”的宽泛表述：该隔离规则只用于**启动任务的终态归属**，临时问答是既有任务内的独立请求/响应，方向插队是既有任务的引导。本文仍只做静态分析和方案记录，不实施或运行验证。

## 补充：方案重审与确认决策（2026-09-20）

本次重新读取 Vulcan 工作树，HEAD 为 `efc2d58c805448bb88b0d202a09ed2079378b7aa`，仍包含其他会话的未提交工作。复核由主代理结合调度、信封生命周期和方案契约三个子代理的静态审查完成；没有运行产品、调试、构建或测试。下述实现位置是本次复核的依据，方案中的新状态机尚未实现。

### 用户已确认的行为

| 决策 | 当前规则 |
| --- | --- |
| 提问与等待 | 提问持久接纳后立即返回问题 ID；AI 可继续其他工作，需要答案时再单独等待。 |
| 空闲目标的临时问题 | 自然完成任务、处于空闲的目标可以自动执行一次仅答疑；必须通过工具回答，答完回到空闲，原任务保持完成。用户明确暂停或停止的普通目标不自动唤醒，子到主另按下方 MainOnly 规则。 |
| 结束前仍有自己发出的问题 | 必须先取得答案或明确撤销，再正常结束任务；不能把最终 Assistant 当作隐式撤销。 |
| 独立会话协作范围 | 首版允许同项目的查找、读取、提问和方向引导；跨项目需要明确授权。父子代理仍遵守归属与工具策略。 |
| 临时记忆或提示到期 | 保留已经发给模型的历史正文，按时点追加失效标记；到压缩时从新的有效上下文中移除，不每次请求删除、替换此前正文。 |
| 向暂停/停止的普通目标提问 | 主到子或独立会话提问时拒收新问题，明确告知发送方对应智能体已经停止；不静默排队，也不自动恢复目标。 |
| 子代理向主会话发送 | 不套用面向已停止子代理的拒收规则；所属子代理仍运行时，主会话不能正常结束逻辑 turn，子到主只向原 turn 插入。临时问题仍必须通过工具回答，插入时机不把它改成方向引导语义。 |
| MainOnly 下子代理向主会话提问 | 允许自动恢复主会话，处理子代理的问题；这是用户对只停止主执行、保留子代理场景明确确认的例外。 |

以上是用户的产品选择；父子方向和 MainOnly 例外覆盖此前不区分方向的统一停止准入建议。下列具体状态、工具名与落盘结构属于待实施的工程设计，不把它们混写成已有运行能力。

“子代理结束”指当前主任务所属的子任务已经进入终态，不要求关闭仍可复用的子代理会话或面板，也不把其他独立任务的运行状态算进本 turn。停止、失败也须有明确结果，不能伪装成正常完成。新问题在准入时被拒收，只产生失败回执，不产生一个仍等答案的已接纳问题。

### 重审后必须补齐的工程契约

1. **两个会话的工具结果不能互相冒充。** `ask_question` 的原调用已经返回问题 ID，不能等答案到达后再改写它过去的工具结果。`reply_question` 只向回答者返回提交回执；答案正文先按 `question_id` 提交到问答权威记录，再交给提问者。若提问者有真实的待完成等待工具，答案可以作为该工具的原生结果返回；否则按下一合法边界追加 `question_answer` 类型的 User 信封。宿主记录交付途径和消费修订，避免一次答复同时被作为新事件重复投递到两个通道；主动重新读取仍可明确返回历史答案。消费指答案进入对应模型请求或真实工具回执的记录，不能声称已经证明模型理解了内容。问答标识不能取代原生工具调用 ID。

2. **可处理的问题要能立即结束等待，但不能冒充任务完成。** 当前 `wait_agent` 会把普通 Message 视为可行动事件；然而指定多个目标时，需要所有目标都进入 satisfied 集合才满足返回条件。因此仅把问题编码为 Message，仍可能让父代理等着其他子代理而无法回答。候选规则是：任何已授权、需要当前代理回答的问题使等待返回 `needs_reply` 及问题引用，不受其他目标完成状态限制；原目标和消费游标继续保留，问题不计作目标任务终态。单独的问题等待也必须允许反向问题唤醒，等待期间释放会话执行准入和事务锁，避免双方占住各自执行通道。该规则只解除调度互锁；如果答案本身循环依赖，应明确报告依赖或无法回答，不能制造答案。依据：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_methods.rs:112-289`、`crates/vulcan-session-boundary/src/subagent.rs:52-69`。

   现有 mailbox 的 `take_matching_events_for_parent` 会从队列移除已取事件；因此唤醒事件只能作为通知，不能成为问题是否仍存在的唯一依据。多个等待者、通知丢失和重启均应通过问题权威状态及各自游标恢复，不能因为一个等待者取走通知就让另一个等待者永久漏掉问题。依据：`crates/vulcan-session-boundary/src/subagent.rs:140-162`。

3. **任务完成、问题结算、执行结束是三个不同的时点。** 原任务的正常终态提交必须核对它发出的问题：答案已交付，或由提问者明确撤销；主会话还必须满足所属子代理均已结束。已经接纳给当前执行的问题应通过工具结算；实在无法回答时需显式表达该结果，不能用沉默或最终 Assistant 替代。问题准入、回答/撤销与任务 final 的提交须有统一次序，避免 final 前检查通过后又插入一个无人接手的问题：运行中子代理向主会话的问题归原开放 turn，不能误判成向已完成父任务发起新答疑；其他目标 final 后到达的问题按自然空闲与停止状态分别处理。若模型提前输出 final，不能先把逻辑任务标为完成再要求补答；应保留未完成状态并明确告知尚待结算的问题或子代理。等待超时只结束一次等待，不能自动撤销问题或中断任务。

   **仅答疑执行必须有独立的运行目的和结束路径。** 当前 `complete_turn_with_lifetime_usage` 必须取得最终 Assistant 才能完成，并可能选择队列或 Goal 续轮；`require_final_assistant` 还要求最后一次物理 API 的完成文本不伴随工具调用。直接沿用此链路，会迫使仅答疑执行补一次无关 final，甚至重新触发业务续轮。因此不能把它绑定成原任务的第二次完成：应在真实回答工具提交及必要工具回执闭合后结束本次答疑，不重发原任务的 `SubagentReturn`，不因答疑结束而自动重启原 Goal。执行消耗仍应正常记账。依据：`crates/vulcan-session-core/src/v1/document/turns.rs:646-713,849-884`、`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs:2230-2322`。

   当前向父级发送的路径写 mailbox 并通知，活动 backend 在物理执行结束后移除；这只能说明执行通道生命周期，不能据此认定父逻辑 turn 已自然完成。按用户最新约束，需要复用并补齐父回合等待子代理的机制，在等待阶段收到问题时恢复原 turn 的处理；不为此设计“已完成父任务重新答疑”的替代路径。来源入口：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs:329-389`、`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/lifecycle_methods.rs:436-450,638-643,654-662`。

   **追加复核发现服务层等待与 V1 完成门并不等价。** `runtime_workspace_helpers.rs:410-486` 统计运行中的子代理，`execution_report_memory.rs:457-464,525-574,708-723` 在 `AwaitingSubagents` 时保留服务侧未完成状态，`conversation_hooks/runtime.rs:503-526` 也可保留活动 Hook turn。但 `provider_v1_sink.rs:1963-1998` 在无工具完成时调用 V1 `complete_successful_turn`；该方法只检查已经落入父文档的 `pending_subagent_return_dispatch`，没有检查仍运行的子任务，随后直接调用文档回合完成方法。已有待回流结果能阻止该次闭合，不代表尚未生成结果的活跃子任务也被同一门保护。因此用户要求的“子代理未结束，主 turn 不得完成”尚不能视为各层统一的硬保证；这里存在持久 turn 提前闭合的静态风险，应在权威结算位置统一子任务、待回流和未结问答的条件。依据：`crates/vulcan-service-kernel/src/session_runtime_registry_v1.rs:2197-2233`、`crates/vulcan-session-core/src/v1/document/turns.rs:646-713`。本轮没有运行该场景，不能把风险记录当作已经复现的故障。

   **停止父会话另有明确例外。** 当前 `StopConversationRunMode` 区分 `AskIfRunningSubagents`、`MainOnly` 和 `MainAndSubagents`；`MainOnly` 允许只停主执行、保留子代理，只有 `MainAndSubagents` 才执行后代 `stop_task_tree`。不能用“正常 turn 等子代理”推导“主会话永远不会处于用户主动停止而子代理仍运行的状态”。用户已明确选择：MainOnly 下允许子代理的问题自动恢复主会话处理。工程上应保留父任务及原逻辑 turn 的可恢复关联，让恢复目的明确为处理这些问题，通过工具回答；不因答疑自动解除原 Goal 的暂停或新增业务任务。这个例外不扩展到停止目标子代理、停止独立会话或已全部取消的父子任务。依据：`crates/vulcan-service-kernel/src/conversation_run_services.rs:18-32,328-370,443-471`。

4. **可靠接纳要覆盖回执丢失，而不只覆盖重复消费。** 当前 `enqueue_inter_agent_message` 生成随机投递 ID 后先进入进程内队列，再返回 ID；消费阶段另有接受持久投影后移除队列的保护，不能笼统地说整条链路没有持久化。但发送回执丢失时，调用者仍可能不知道第一次生成的 ID。新问答和投递契约应在产生发送副作用前，由宿主为同一次真实工具调用分配可恢复的稳定操作标识，持久提交后才返回“已接受”；同键同正文重试返回原记录，同键不同正文或目标冲突报错。回答也先提交权威状态，再通知等待者；通知丢失通过查询或重放补齐，不能重新回答或覆盖已提交正文。来源：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs:177-185,434-461,1127-1150`。现有人类 Ask 的 `reply` 已采用先可靠提交再进程通知的顺序，可借鉴此局部模式，见 `crates/vulcan-agent-runtime/src/agent_runtime/question_runtime.rs:458-509`。

   状态提交与通知分阶段：持锁完成准入、版本检查和持久提交，释放锁以后才启动恢复、发送唤醒或调用 Provider。回答与撤销并发时由权威提交次序决定，后到操作返回既有结算状态，不覆盖正文或复活已撤销问题；重复操作按幂等键取原回执。当前消息准入已经先释放任务事务锁再启动空闲恢复，回答路径也先释放 pending 锁再通知，可以保留此模式。依据：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs:264-309`、`crates/vulcan-agent-runtime/src/agent_runtime/question_runtime.rs:458-509`。这是新增实现的锁顺序约束，并非已经复现当前死锁。

5. **统一 User 必须是按事件追加，不能只替换 role 字段。** 当前请求装配仍存在动态层位于历史之前、锚点通过请求局部副本改写已有 User 正文的路径。需要保存逻辑事件 ID、来源序号、生效范围及确定的插入位置；原生 Assistant、工具调用和工具结果继续按协议结构成组。合法边界是工具配对等约束满足后的下一可用 API 边界，**不是必须等整个任务的最终 Assistant**，否则临时问答会变成任务结束后才处理，方向引导也失去作用。依据：`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs:1118-1194,1364-1394`、`crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink/context_injection.rs:95-128`、`crates/vulcan-session-core/src/v1/projection.rs:641-671`。

   TTL 到期还必须区分“当前不再有效”和“后续请求不再包含旧正文”：前者可追加失效记录，后者需要改变已有请求内容。当前临时项到期删除，VMM 租约又有保留到压缩的生命周期；不能单凭统一 User 就声称这两种行为都不影响缓存。用户已选择普通到期保留历史正文并追加失效标记，压缩时移除。失效标记须引用准确事件及版本；已过期正文不得重新成为压缩后的有效状态或被摘要误写为当前指令。这里的移除指不带入压缩后的模型上下文，不是删除本地审计记录，也不承诺删除供应商侧缓存。依据：`crates/vulcan-context-injection/src/context_injection/state.rs:147-205,316-342`、`crates/vulcan-agent-runtime/src/agent_runtime/context_injection_runtime.rs:37-73`、`crates/vulcan-vmm/src/session_state.rs:1252-1304,1730-1759`。

6. **身份、未结问题和模式状态必须能够独立于对话摘要恢复。** 压缩会过滤边界以前的事件，并专门回填仍有效的系统状态；现有普通消息和 WaitingUser 工具记录没有形成新的跨会话问答闭环。问题双方、归属任务、状态、答案和消费修订必须从宿主持久权威恢复，不能靠摘要保留标签或模型回显 ID。压缩后只在明确的新上下文边界重建当前有效状态和未结问题，不能每次请求重写头部快照。Goal/Plan 的强类型状态、冲突校验、预算和工具执行约束继续由宿主维护，User 信封只是模型可见镜像。来源：`crates/vulcan-session-core/src/v1/projection.rs:330-339,463-471`、`crates/vulcan-context/src/protocol/projection.rs:605-663,694-764`、`crates/vulcan-session-core/src/v1/document/modes.rs:78-91,190-239,813-866`、`crates/vulcan-session-core/src/v1/config.rs:81-125`。

   旧事件解码和旧请求投影还需明确迁移边界。新 User 信封改变已发送的角色、字节或消息邻接时，不能复用旧 continuation 并声称历史完全相同；应记录投影版本及显式的上下文重建。当前结构存在严格字段解码，实施前需要给出向后兼容和重启恢复方案，不能直接把候选字段塞进旧格式。来源：`crates/vulcan-session-core/src/v1/event.rs:144-155,361-385,404-427`、`crates/vulcan-session-core/src/v1/document.rs:454-479`、`crates/vulcan-agent-runtime/src/agent_runtime/provider_request_preview.rs:498-535`。

7. **项目授权、用户停止和工具可见性都由宿主检查，并区分父子方向。** 项目归属来自宿主会话目录，跨项目授权须有可恢复且可撤销的记录；正文自称同项目、发送方填写的来源名称或 XML 标签都不能授权。主到子或独立会话提问遇到已暂停/停止目标时拒收并明确返回状态，自然空闲才适用自动仅答疑；子到主绑定原父 turn，不套用目标子代理的停止拒收分支，MainOnly 下明确允许恢复主会话处理问题。两条路径都校验真实会话身份、父子归属和原任务/turn 绑定，旧代际的迟到消息不能混入另一个新任务。子代理专用提问与回答工具须走注册、模式可见性、策略过滤和执行校验全链路，不能简单开放当前被禁用的通用 `ask`。现有 `ask` 面向真实用户，拒答会中断当前运行，其语义不能直接套用到代理间问题取消。依据：`crates/vulcan-session-core/src/child_agent_policy.rs:12-26`、`crates/vulcan-agent-runtime/src/agent_runtime/question_runtime.rs:79,90-166,321-424,524`、`docs/Ask与Markdown交互约定.md`。

### 本次复核修正的历史表述

- 删除“先让提问工具一直挂起”的候选，统一为发送即返回加单独等待；原发送工具结果不能事后改写为答案。
- 统一启动、插队和临时问题的首版语义，不再保留“所有繁忙时消息都排到任务结束”的相反建议；插队不能静默变成新任务。
- 将逐供应商 Developer/System 分级明确标为历史候选；当前动态提示词统一使用按事件时点追加的 User。
- 原文“恰好一次消费”收紧为宿主幂等提交和投递去重，不承诺模型恰好理解或生成一次。
- 2026-09-17 记录的 `SubagentReturn` 与统计回报对不能直接当作本次实现事实：本次 `append_subagent_return_with_stats` 虽保留旧函数名，当前函数体追加的是 `content_only: true` 的 `SubagentReturn`，统计链正在其他工作中调整。本任务不修改该并行代码。当前依据：`crates/vulcan-session-core/src/v1/document/subagent.rs:344-386`。

### 后续事实核对与实施边界

- **已确认的 MainOnly 例外：**用户只停止主会话、让子代理继续时，允许子到主问题自动恢复主会话处理；后续需要对齐实际停止代际、父 turn 关联与答疑目的，不能把它当作普通目标的停止拒收或已完成任务的新一轮业务执行。
- **停止后的结算补充：**用户已确认拒收新问题并通知对应智能体已停止，且子到主走原 turn 的插入通道；未选择“新旧问题都排队恢复”。已接纳以后目标才停止的竞态应保留可查询事实并通知提问方目标停止，由提问方按已确认规则显式撤销或决定后续处理，不能伪造答案、静默遗忘或自动重启目标。此处是配套工程建议，不把它写成用户另外确认过的自动取消政策。
- **后续静态核对：**精确确定问答记录的存储归属、跨会话事务边界、重启索引重建、授权恢复、仅答疑结束入口，以及未投递问题与 final 的原子结算位置；为服务层、Hook turn 和 V1 持久 turn 明确共同的子任务完成权威；核对所有相关工具的可见性和子代理策略。
- **后续实施方案：**列出旧会话格式与新投影版本的迁移、压缩后状态重建及故障恢复，不以当前方案代替 Schema 设计。继续核对 `transient_prefix` 的实际物理消费者；本轮不因未找到而断言线上一定存在相应缺陷。
- **验证边界：**没有运行任何模型请求，不能据此宣称缓存命中改善、上游接收全部合法或重启恢复已经通过。统一 User 是当前选定的设计方向；上述生命周期契约补齐后仍需另行取得实施授权。

## 本轮记录

2026-09-17：建立源码对照与临时方案首稿；补充子代理权限、可靠最终回报、V1/V2 版本边界、独立任务等待与直接输入限制。随后针对常规协议缺少原生子代理消息类型的问题，补充宿主身份、模型可见信封和确定性回复归属设计。又核对全部注册协议的实际生产角色投影，发现 Chat 的 system 被汇集到头部、标准 Responses 的 System 在生产链进入顶层 instructions；补充目标模式与 VMM 记忆的消息注入参照。最后按官方协议文档与 DSH 当前源码补齐流内角色、模型级动态 system 开关、注入数据/普通用户/宿主规则的统一封装，明确校正了上游协议能力与 Vulcan 当前转换能力的混淆。本轮再对动态注入及缓存前缀进行静态审计，定位 TTL 指引提前、Plan 双来源、VMM 差量和环境上下文前置，同时区分 V1 最终 User 注入与非 V1 合成 System、缓存前缀与 Provider 延续，并登记 Transient 注入疑似缺少物理投影的待核对问题。继续补充流内 Developer → 流内 System → User 的可信宿主控制消息分级、权限与功能标签分离、具体前移清单，以及由宿主投影凭据确定接收归属的编解码草案。根据用户进一步提出的统一 User 思路，将“稳定顶部基线 + 流内宿主 User 信封”记录为默认目标方案，并列明权限降级、时点、相邻 User 合并、工具配对和非 V1 整包 System 的边界。按最新修正再将启动任务/插队引导的终态回复链与临时问答的工具回复链分离，确认现有子代理工具策略阻止主动询问、普通 mailbox 消息缺少问题关联，并补充双向问答、等待唤醒及防互等状态。后续探索直接补充本文件并保留尚未证实项；文件暂留在 `docs/plan/`，不迁移到已完成目录。

2026-09-20：仅修改本临时方案文件。结合三个子代理的静态复核，记录用户确认的异步提问、单独等待、自然空闲仅答疑、结束前结算问题、同项目权限范围及普通到期保留历史并追加失效标记；补充答案与本地工具回执分离、多目标等待被问题唤醒、仅答疑独立结束、可靠接纳与幂等重试、压缩恢复及投影迁移边界。修正文内旧提问候选、旧首版排队建议和角色分级建议的冲突。根据用户对停止规则的补充，将主到子/独立会话的停止拒收与子到主的原 turn 插入分开，并明确主 turn 须等所属子任务结束、物理执行结束不能直接等同逻辑完成。追加核对发现服务层等待与 V1 持久回合完成门的条件不一致，记录父持久 turn 提前闭合风险；确认当前存在 MainOnly 停止模式，并记录用户允许此时因子代理问题自动恢复主会话处理的选择。没有修改产品源码、配置或公共状态，没有运行产品、调试、构建或测试；继续保留待核对事项，不迁移到已完成目录。

2026-09-20（正式化）：按用户要求新增总计划 `20260920-04-AGENT_COMMUNICATION_AND_DYNAMIC_CONTEXT.md`，在 `docs/architecture/agent-communication/` 保存三份设计文件，记录已确认行为、完整工作包、涉及文件、实施顺序、持久化与恢复设计、协议投影、迁移、注意事项和验收场景。当前只完成正式方案编制，实施仍未开始；本研究不再承担唯一的实施入口。
