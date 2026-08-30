# 未提交改动根因循环审核

日期：2026-08-30

结论：审核与修复闭环完成。发现三项根因缺口；每次发现后重新计数，最后丙、丁两轮完整复审均无新增问题。该结论限于本次源码与测试覆盖，不代表真实桌面及付费供应商端到端验收已经完成。

## 范围与完成条件

- 覆盖开始审核时全部未提交文件，以及审核中新修改的关联逻辑。
- 逐项沿运行时事实、持久化边界、服务事件、前端消费追踪；不把增加刷新或兜底当作根因修复。
- 每轮完整覆盖后计数；发现新问题立即归零，修复并复验后重新开始，连续两轮无新问题才结束。
- 不提交代码、不重启用户桌面进程，不使用真实供应商付费调用。

## 轮次记录

|轮次|覆盖|发现|修复与验证|连续无问题轮次|
|---|---|---|---|---|
|发现轮|86 个初始文件及本轮修复|两项根因缺口|运行时 134 项、V1 后继准入 3 项通过|0|
|复审甲|86 个文件：消息与状态所有权、持久化和前端消费、断言变更|无新增功能问题|关键 V1 流入口 17 项通过；文件内容已建立哈希基线|1|
|复审乙|86 个文件的失败、重放、交接和契约调用点|旧重连失败越过页面生命周期|红灯后修复通过；计数归零|0|
|复审丙|86 个文件及新生命周期修改；重查消息唯一消费方、状态推送和迟到事件隔离|无新增问题|前端 681 项与类型检查通过；子代理 40 项、目标 32 项通过|1|
|复审丁|86 个文件及本轮修复；反向核对工具契约、失败出口、持久化、消费者和断言变化|无新增问题|运行时再次 134 项通过；定向格式与差异检查通过；86 文件内容未发生漂移|2|

## 发现与根因

### 一：排队消息从旧控制器消费，却被真实请求重建丢弃

- 事实链：子执行的干预钩子提前取空 inbox，仅修改旧控制器快照；服务的物理请求闸门从 V1 账本重新构建消息，覆盖旧控制器内容。队列归零不代表模型收到消息。
- 修复：移除这条竞争性消费入口和仅验证旧控制器的无效测试；按既有 queue 契约，在当前任务结束后，由后继执行将消息作为可靠 V1 根输入准入。插队先中止，再使用同一后继路径。
- 回归：补充运行中不可取空 inbox、多条消息顺序、独立后继执行身份断言；保留后继准入及 V1 物理请求重建测试。

### 二：并发等待只修复了显式目标，默认等待仍超时

- 事实链：默认等待仅在进入时记住运行中集合；终态事件被其他等待认领后，不检查权威终态，也不刷新是否仍有活动任务。
- 红灯证据：并发默认等待回归在一秒保护超时内失败，原实现第二个调用仍等待十秒工具超时。
- 修复：所有等待均核对所观察目标的权威终态；无活动判断重新读取状态，同时纳入已排队和启动中的后继所有权，避免把上一执行完成误当作没有任务。
- 回归：并发默认等待均返回终态；已排队后继不会被误判为无活动任务。修复后运行时 134 项通过。

### 三：旧重连失败在子代理页面退役后仍写入状态

- 事实链：控制器已经通过 effect 租约区分销毁，但重连请求的 catch 回调直接调用共享 setError；页面切换、关闭后，在途旧请求的失败仍会污染新的一次性回读页面。
- 红灯证据：执行真实 hook 的 effect 回调，控制重连 Promise 在 cleanup 后拒绝；原实现仍写入错误。当前所有者失败的对照场景正常。
- 修复：将异步失败回调绑定到同一个 effect 生命周期；cleanup 先取消状态写入资格，再移除监听。不是隐藏当前所有者的真实错误，也不重新加载数据。
- 测试范围：effect 生命周期单元测试，不宣称覆盖真实浏览器 React 挂载或 StrictMode。

## 验证边界

测试与源码审核不等于正在运行桌面程序的真实供应商验收。

- 前端首次全部 116 个测试文件、679 项测试通过；新增生命周期对照测试后全量 116 个文件、681 项测试及类型检查通过。
- 运行时 134 项、V1 后继准入 3 项、V1 流入口 17 项、子代理 40 项、目标 32 项通过。
- 会话核心 237 项、工作台模型 58 项、服务 API 69 项通过。
- 工作区 V1 持久化测试 58 项通过、2 项原有忽略。
- 本轮修改的 Rust 文件已通过定向格式规范检查；仅格式化本轮修改的四个运行时文件，没有全库重写。
- 服务内核全量串行测试：741 通过、34 失败。失败位于供应商注册及 Manifest 信任校验相关用例；单独进程重跑首个 OpenRouter 用例仍在未修改的夹具初始化处失败，尚未进入会话提交代码。没有为了通过测试修改供应商配置或信任边界；不宣称全量通过。
- `git diff --check` 通过；换行提示不属于差异错误。

## 复审方法与范围控制

- 每轮覆盖同一份 86 文件台账。发现轮逐份读取完整差异，之后分别核对调用方、错误路径、状态所有权及测试断言，而非用重复运行测试代替审核。
- 甲轮完成后对 86 个文件建立内容哈希；乙轮第三项修复仅改变两个事件窗口文件，其余 84 个文件一致。丙、丁轮仍逐组复核既有结论和跨模块不变量。
- 未修改无关供应商初始化代码、用户持久化配置或运行产物；没有新增生产依赖、轮询刷新、兼容字段猜测或第二条消息消费入口。

## 最后两轮的交叉检查

- 消息路径：确认子任务 inbox 仅由后继执行消费，运行中旧控制器不再取空消息；V1 根输入投影包含后继消息。成功、停止、失败和旧执行收尾使用相同的后继所有权判定。
- 等待路径：默认和显式目标都检查权威终态；已排队或启动中的后继不能被上一结果满足；并发默认等待不依赖哪一方先认领 mailbox。
- 状态路径：主流、分离流和压缩流均通过同一工具事件入口先持久化再投影生命周期；关闭保留当前回合行，新用户回合撤销旧行准入，迟到反馈不得恢复已关闭缓存。
- 回流与目标路径：回流和回合关闭在相同写锁下选择；父回合已结束时由可靠回流开启自己的后续回合；目标创建只在空闲或所属 API 的运行中创建工具边界准入。
- 前端路径：页面退役后的异步失败不污染替代页面；当前失败仍可见；分叉使用终态统计锚点且不分发新请求；模型首次配置不提示切换，名称来自显示目录；思考模式、滚动边界与无 Git 标题布局保持独立展示逻辑。
- 回归质量：删除的是不经过真实 V1 请求重建的旧控制器专用测试，不是以删断言规避失败；补充后继根输入、并发等待和异步生命周期的对应断言。其余测试断言变化与已确认的分叉、导航及显示契约逐项核对。

## 本次审核额外产生的变更

保留进入任务时已有的全部改动。以下仅列本轮审核新增的修复，不把原有 86 文件变更全部归为本轮实现。

|类型|文件|本轮调整|
|---|---|---|
|修改|crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs|删除旧控制器 inbox 消费钩子和专用辅助函数，明确可靠后继入口|
|修改|crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_execution.rs|停止安装竞争性旧消费钩子|
|修改|crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_methods.rs|默认等待也检查权威终态，并刷新活动、启动及排队范围|
|修改|crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/runtime_flow.rs|删除旧控制器专用测试，强化排队保留、顺序及后继身份验证|
|修改|crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/registry_mailbox.rs|并发默认等待及排队后继回归|
|修改|crates/vulcan-service-kernel/src/session_v1_lifecycle.rs|补充持久化后继输入投影断言，修正相关注释归属|
|修改|apps/vulcan-code-web/src/state/conversation-session-event-window-owner-v1.ts|重连失败回调绑定所属 effect 生命周期|
|修改|apps/vulcan-code-web/src/state/conversation-session-event-window-owner-v1.test.ts|以真实 effect 回调验证当前和退役请求失败的区别|
|新增|docs/reviews/20260830-UNCOMMITTED_ROOT_CAUSE_AUDIT.md|审核台账、根因证据、轮次与验证边界|

没有删除实体文件，没有自动提交。已移除的代码和测试可通过当前工作树差异查看。服务内核 34 项供应商初始化相关失败仍需在对应配置与信任环境中单独处理；本轮未用修改该边界的方式让测试变绿，也未在独立 HEAD 检出中证明其历史基线。

## 文件覆盖台账

初始文件数：86。

|文件|第一轮|复审轮次|
|---|---|---|
|apps/vulcan-code-web/src/components/conversation/chat-area.tsx|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/chat-area/conversation-scroll-controller.test.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/chat-area/conversation-scroll-controller.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/chat-area/types.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/conversation-block-renderer.tsx|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/conversation-blocks.tsx|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/conversation-branch-presentation-v1.test.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/conversation-branch-presentation-v1.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/conversation-branch-workspace-v1.test.tsx|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/conversation-branch-workspace-v1.tsx|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/conversation-tool-presentation-card.test.tsx|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/conversation-tool-presentation-card.tsx|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/conversation-transcript-parser.test.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/conversation-transcript-parser.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/conversation-v1-display-renderer.test.tsx|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/conversation-v1-display-renderer.tsx|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/conversation-v1-tool-card.test.tsx|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/conversation-v1-tool-card.tsx|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/conversation-v1-workspace.tsx|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/session-todolist-panel.tsx|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/thinking-display.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/tool-presentations/subagent-orchestration-presentation.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/tool-presentations/types.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/workspace/WorkspaceContextPanel.tsx|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/workspace/workbench-layout/conversation-actions.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/workspace/workspace-shell/types.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/i18n-messages.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/service/conversation-actions-service.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/service/conversation-event-display-projection.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/state/conversation-session-event-window-owner-v1.test.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/state/conversation-session-event-window-owner-v1.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/state/workbench-state.test.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/state/workbench-state.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/styles/conversation.css|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/styles/session-tools.css|已审|甲、乙、丙、丁已审|
|crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_methods.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/lifecycle_methods.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/parent_events.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_execution.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/task_records.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/registry_mailbox.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/runtime_flow.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tool_surface.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-api/src/conversation_events_v1.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-api/src/conversation_generations_v1.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/agent_stream/detached_subagent.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/agent_stream/projection.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/agent_stream/stream_task.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/agent_stream/stream_task/finished.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/context_compaction/streaming.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/conversation_generations_v1.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/conversation_submission/activation.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/lib.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/runtime_workspace_helpers.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/session_goals.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/session_runtime_registry_v1.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/session_v1_lifecycle.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/state.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/subagent_event_ingest.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/subagent_panels.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/tests/conversation_submission_activation_tests.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/tests/detached_subagent_tests.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/tests/goal_todolist_tests.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/tests/session_kernel_tests.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/tests/test_support.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/webui_prompt_lifecycle/pending_prepare.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/webui_prompt_lifecycle/precheck_continue.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/webui_turn_chaining.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-service-kernel/src/workbench_surface_projection.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-session-core/src/v1/config.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-session-core/src/v1/document.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-session-core/src/v1/document/document_tests.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-session-core/src/v1/document/generation.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-workbench-model/src/subagent/metadata.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-workbench-model/src/subagent/models.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-workbench-model/src/subagent/panel_state.rs|已审|甲、乙、丙、丁已审|
|crates/vulcan-workspace-store/src/workspace_store/session_v1/tests.rs|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/conversation-model-switch.test.tsx|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/conversation-model-switch.tsx|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/session-todolist-panel.test.tsx|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/thinking-display.test.ts|已审|甲、乙、丙、丁已审|
|apps/vulcan-code-web/src/components/conversation/tool-presentations/v1-subagent-orchestration-presentation.ts|已审|甲、乙、丙、丁已审|
