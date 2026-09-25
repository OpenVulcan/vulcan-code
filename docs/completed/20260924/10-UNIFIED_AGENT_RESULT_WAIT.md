# 统一代理消息接收与等待

## 目标

模型仅使用 `wait_agents_result` 等待代理通信。不预判问题、任务结果或通知的到达顺序，不传递会话修订或内容哈希。发送立即返回；等待独立进行。

## 已确认约束

- 入站问题、回答、子任务结果、跨会话任务结果和阶段通知统一接收，内部接纳与投递回执不单独结束等待。
- 空闲目标收到问题使用代理消息执行入口启动答疑；运行中插入当前执行。均不属于人类输入，不记录用户记忆。
- 有尚未交付的消息优先返回；没有消息且没有实际运行或排队的关联工作时立即返回。
- 人为停止反馈停止；重启未恢复或执行消失反馈中断，不依赖最终文本或回合结束标记。
- 等待期间持续核对存活状态；停止任一目标可先返回该目标的结果，不终止其他工作。
- 不考虑旧数据兼容，不保留面向模型的三套旧等待入口。
- 继续保护并行会话文件；独立提交，不推送。

## 技术选择与执行步骤

1. 沿工具注册、执行分发、协作状态、上下文投递、完成工具结果的持久化路径取得事实。
2. 复用规范协作事件及精确任务身份构建收件快照；成功写入工具结果后确认交付，调用失败不能提前消费。
3. 合并模型等待工具及权限、说明、前端展示，移除模型手动管理的修订游标。
4. 复用实时执行归属与现有停止清理，核对排队、启动交接和恢复窗口，避免把正常启动误判为中断。
5. 核对空闲问题的启动及回答、结束去重；保持问题回复和业务任务完成的不同含义。
6. 定向测试提前到达、重复等待、混合结果、停止、执行丢失、无目标、授权撤销及并发投递；检查涉及的测试目标编译。
7. 记录每个决策与原因、证据和遗留项；完成后迁移至完成目录。

## 验收标准

- 模型只见统一等待入口，无需读取对端日志判断结果。
- 成功、停止、中断和超时明确区分；无可等待工作立即返回。
- 不遗漏先到结果，不重复交付已进入上下文的消息，失败重试不吞消息。
- 不误判正常启动、合法排队或其他仍在运行任务；实时身份变化不会串用历史状态。
- 非用户消息不触发用户记忆，收到问题可回复后继续原任务。

## 决策记录

- 统一入口仍保留内部消息类型及稳定关联身份，因为问题回答和任务终态不能互相冒充。
- 交付确认归属成功持久化的工具结果，不能在等待函数返回前标记已读；否则取消或崩溃会丢消息。
- 优先复用现有投递投影与任务终态修复，避免建立第二套生命周期权威。

## 进度

实现、定向验证和连续五轮代码复查已完成。尚未替换用户正在运行的服务；真实供应商及桌面重启验收不在本次本地验证结论内。

## 实施决策补充

1. **正文只有一条交付路径。** 统一等待返回消息类型、精确关联身份、终态与等待状态；正文使用现有规范上下文投影，在下一次供应商请求前以带标签消息交付。原因：在工具结果和上下文各复制一次正文，会重复输入、增加用量并产生两套消费游标。模型无需再调用读取会话工具拼接结果。
2. **报告确认与正文消费分开。** 停止、丢失等诊断报告只有成功持久化原生工具输出才算已报告；正文继续由现有投影凭据去重。单纯调用等待、返回临时值或取消调用均不提前消费。
3. **准备也必须有实际所有者。** 发送意图、接纳、调度、异步画像准备及执行流之间使用可释放守卫衔接；仅持久化运行状态、开放回合或进程内旧标记不能证明仍在执行。子执行进一步核对不可变执行编号，不能借用同名会话后来的任务。
4. **丢失执行不伪造正常回复。** 未启动任务写入明确的执行丢失失败；已经认领的任务关闭其原业务回合并冻结中断结果；丢失子任务释放父侧结束义务。失败效果必须先独立可靠提交，再由结果对账生成后续修订，符合现有凭据验证约束。
5. **迟到结果进入当前回合。** 保留任务原始发送回合和授权身份，允许未投影结果进入后续接收回合；已经消费的旧结果不会重新触发续跑。
6. **用户插队优先。** 当前回合的真实待输入记录或原请求被结束，会释放等待；未来用户排队任务不会。代理通知也不制造回复或等待义务。
7. **主会话只停止自身的例外保留。** 子代理向其精确父会话提问时，已有自动答疑入口可恢复主会话；通过真实父子归属校验，不按项目名或同根关系放行。
8. **旧工具不再向模型暴露。** 旧的内部等待方法及其已有宿主测试仍保留为内部代码，本次不进行无关邮箱运行时删除；模型注册、参数解析入口及提示说明全部使用统一等待。
9. **并行工作隔离。** 工作区另有待办、分支与差异浏览修改。本任务仅追加一个无待等待工作翻译键，提交时按单独补丁暂存；不暂存其他界面或 Git 模块修改。

## 验证中发现并修正的问题

- 最初使用不存在的投递失败枚举值，编译失败；已读取权威枚举，新增语义明确的目标执行丢失原因。
- 同一原子修订内同时写失败效果和结果违反原有后继修订凭据；定向测试发现后，改为先提交失败效果，再复用结果对账。
- 分离画像准备原来只有内核状态，无法区分实际准备与残留；补上从生成任务前到执行流接管的守卫。
- 用户插队原来只唤醒旧邮箱等待；新入口改为检查规范回合内输入，保留排队与插队差异。
- 前端整文件测试有一个与本任务无关的用量浮层触发元素缺失失败；统一等待相关用例单独通过，保持并行修改不动。
- 最后复查发现分离子任务事件流无终态结束或取消后，监听登记与未发布回流门控可能残留；新增与实际监听 Future 同寿命的守卫。有效终态仍在交付重试时保持所有权，退出只清理精确代次，保留已发布回执及替代执行。该修复独立提交为 `1e72197`。

## 连续五轮代码复查

最后一项监听退出缺陷修正后重新计数。以下均为独立切入点的源码复查，不代表五轮线上运行验收；未使用子代理。

| 轮次 | 检查内容 | 结论与证据 |
| --- | --- | --- |
| 一 | 工具注册、参数解析、策略过滤、子代理权限、提示词说明 | 未发现新增问题；模型可见入口只有统一等待，旧名称仅留在内部调用与既有测试；候选注册及提示词测试通过 |
| 二 | 发送、接纳、准备、执行、监听、交付重试的所有权交接 | 未发现新增问题；派生前捕获守卫，精确代次释放；缺失所有者和用户停止测试通过，监听退出与终态写盘重试测试通过 |
| 三 | 已到达结果、迟到结果、重复等待、工具持久化确认、取消重试 | 未发现新增问题；正文复用规范投影，诊断仅由完成工具输出确认；迟到结果投影、重建及消费测试通过 |
| 四 | 原执行与替代执行隔离、跨项目授权、主会话仅停止自身的答疑例外、非用户输入 | 未发现新增问题；沿真实参与者与执行身份校验；主会话停止后子代理提问的本地模型集成测试通过，目标保持原暂停目标及无新增人类输入 |
| 五 | 用户排队与插队、前端等待状态、编译目标、提交边界 | 未发现本次修改新增问题；排队保持等待、插队解除等待；前端二十一项定向测试及类型检查通过；后端所有测试目标检查通过；并行会话提交独立保留 |

## 执行变更总结

### 一、核心修复与调整概述

- 使用 `wait_agents_result({ timeout_ms })` 统一接收问题、回答、通知与任务结果，不需要目标选择或修订游标。
- 无消息且无实际可等待工作立即返回 `no_pending_work`；人为停止反馈 `stopped`，无执行所有者反馈 `abandoned`，有效工作超时反馈 `timeout`；用户插队反馈 `interrupted`。
- 等待结果中的消息元数据标识就绪内容；完整正文由紧接工具之后的既有带标签上下文投影交付，无需额外读取对方会话。
- 复用原空闲答疑入口与运行中注入路径，不新增用户提示词或用户记忆 Hook。
- 不迁移旧数据，不增加兼容模型工具别名，不改用户配置。

### 二、关键代码调整详情

- `wait_agents_result` 从规范来源状态选择关联工作，读取实际执行归属；在双方生命周期锁内再次校验，避免晚到事件与启动交接竞态。
- `DispatchOwner` 跨接纳、异步调度及画像准备保护真实待执行工作；`ListenerOwner` 覆盖监听 Future 的全部退出路径；`is_task_execution_running` 按不可变执行标识验证。
- `abandon_unowned_execution` 在宿主证明实际执行已不存在之后，以原子领域变更结束开放回合，保持中断与用户停止语义分离。
- 任务失败先可靠提交，再通过既有结果对账生成下一修订的回执；缺失子任务同时释放父侧完成义务。
- 迟到的跨会话结果保留来源任务身份，进入当前接收回合；已消费结果不重新触发续跑。
- 新等待卡复用现有工具展示，无工作状态显示“当前没有可等待的工作”。

### 三、验证记录

- 协作原生端口：二十三项通过，覆盖八项新增统一等待场景及原授权、投递路径。
- 监听退出所有权：一项参数化测试通过，覆盖无终态结束、首次轮询前取消、执行替换及已发布回执。
- 真实磁盘终态交付重试：一项通过，覆盖流已结束后的可靠重试。
- 主会话仅停止自身、子代理自动答疑：一项生产服务链路的本地供应商夹具测试通过。
- 任务结果投影五项、协作续跑两项、无执行所有者中断一项通过。
- 工具契约五项、桥接候选一项、提示词说明一项通过。
- `cargo check -p vulcan-service-kernel --all-targets` 通过。
- 前端等待展示与执行模型二十一项、等待卡定向两项通过；`pnpm exec tsc -b --pretty false` 通过。
- `git diff --check` 通过。

### 四、遗留问题与注意事项

- 曾执行的三份前端整文件测试共五十项通过、一项失败：用量浮层测试 `portals focused subagent usage outside a clipped tool fold` 找不到触发元素。本任务对该卡只改无工作文案，未修改相关用量功能；该失败不计为统一等待通过证据，也不越界修改并行工作。
- 本地模型集成测试使用受控供应商夹具；未调用用户真实凭据、未重启桌面应用、未替换现有运行服务。需要重新构建并重启核心后，运行实例才会使用新入口。
- 保留旧内部等待方法供既有内部调用及测试使用，但模型注册、Schema 和提示词入口不暴露它们。未将旧问题修订号改造成新的公开等待契约。
- 运行结束不等于问题必须被取消；没有实际执行时报告状态并解除等待，待答问题本身及之后可能到达的真实答案仍由原规范状态机管理。
- 并行提交 `7c8c57b8`、`4964c37e` 保持独立；本任务不推送。

### 五、文件变更清单

以下列出本任务两个提交的文件；除完成记录外均为代码或定向测试。没有删除代码文件，计划文件迁移至本完成记录。

- 新增：`crates/vulcan-service-kernel/src/agent_stream/detached_subagent/listener_owner.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_stream/detached_subagent.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_stream/detached_subagent/admission_tests.rs`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-tool-execution-model.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-tool-metadata.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-tool-presentation-card.test.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-tool-presentation-card.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/conversation-tool-result-tabs.tsx`
- 修改：`apps/vulcan-code-web/src/components/conversation/tool-presentations/subagent-orchestration-presentation.test.ts`
- 修改：`apps/vulcan-code-web/src/components/conversation/tool-presentations/subagent-orchestration-presentation.ts`
- 修改：`apps/vulcan-code-web/src/i18n-messages.en-US.ts`
- 修改：`apps/vulcan-code-web/src/i18n-messages.ts`
- 修改：`apps/vulcan-code-web/src/i18n-messages.zh-CN.ts`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/lifecycle_methods.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/tool_args.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tool_surface.rs`
- 修改：`crates/vulcan-agent-runtime/src/tooling/bridge/tests.rs`
- 修改：`crates/vulcan-agent-runtime/src/tooling/collaboration.rs`
- 修改：`crates/vulcan-ai-contract/src/host_message.rs`
- 修改：`crates/vulcan-prompt-composer/src/tests/mod.rs`
- 修改：`crates/vulcan-prompt-composer/src/tool_help/builtin.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_collaboration.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_collaboration/child_question_request/tests/compaction/main_only.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_collaboration/launch.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_collaboration/native_dispatch_tests/cases.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_collaboration/native_dispatch_tests/mod.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_collaboration/native_dispatch_tests/wake.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_collaboration/projection.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_collaboration/session_messages.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_collaboration/session_wait.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_collaboration/startup_recovery/native_execution.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_collaboration/task_delivery.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_collaboration/task_launch.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_collaboration/task_results.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_stream/runtime_coordination.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 修改：`crates/vulcan-service-kernel/src/state.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/detached_subagent_tests.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/test_support.rs`
- 修改：`crates/vulcan-service-kernel/src/webui_prompt_lifecycle/pending_prepare.rs`
- 修改：`crates/vulcan-service-kernel/src/webui_prompt_lifecycle/precheck_continue.rs`
- 修改：`crates/vulcan-session-core/src/child_agent_policy.rs`
- 修改：`crates/vulcan-session-core/src/v1/collaboration.rs`
- 修改：`crates/vulcan-session-core/src/v1/collaboration/task_result_consumption.rs`
- 修改：`crates/vulcan-session-core/src/v1/document/collaboration/continuation.rs`
- 修改：`crates/vulcan-session-core/src/v1/document/collaboration/execution_failure_tests.rs`
- 修改：`crates/vulcan-session-core/src/v1/document/collaboration/task_messages/result_projection.rs`
- 修改：`crates/vulcan-session-core/src/v1/document/collaboration/task_messages/tests/results/delivery/projection.rs`
- 修改：`crates/vulcan-session-core/src/v1/document/recovery.rs`
- 修改：`crates/vulcan-subagent-core/src/lib.rs`
- 新增：`crates/vulcan-service-kernel/src/agent_collaboration/dispatch_owner.rs`
- 新增：`crates/vulcan-service-kernel/src/agent_collaboration/native_dispatch_tests/unified_wait.rs`
- 新增：`crates/vulcan-service-kernel/src/agent_collaboration/result_wait.rs`
