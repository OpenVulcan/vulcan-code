# 当前实施状态与剩余清单

更新日期：2026-09-23。原目标仍为正式方案全部 P00—P12、R01—R14、A01—A18，以及连续五轮完整审核无新问题。

**需求行级验收对照见 [S5 验收证据对照](S5_ACCEPTANCE_EVIDENCE.md)**（提交 `45832d32`）：该文件把原方案每一行需求
逐项对应到源码事实与已提交证据，并集中登记尚未闭合的验收缺口、未验证边界与复跑基线。本文件继续承载工作包级状态与决策记录。

**字段唯一来源、工具契约、实际路由与使用说明见 [S5 字段与路由汇总](S5_FIELD_AND_ROUTE_SUMMARY.md)**（提交 `4b02e817`）。

最新安排：用户要求保存提交点并交接剩余实现，原主代理转为审核。执行入口见[交接方案](../../plan/20260922-01-AGENT_COMMUNICATION_HANDOFF.md)。总目标尚未完成；五轮审核仍为 0/5，且**只能由原主代理完成**。

## 引用完整性（审核本文件或 S5 证据文件前先复跑）

两份文档的价值取决于**其引用的每个测试名在当前源码中仍然存在**。
本次实测：本文件 26 个标识符中发现并修正 2 个失效名（见下）；`S5_ACCEPTANCE_EVIDENCE.md` 的 54 个标识符全部存在。

方法：提取反引号内的 snake_case 标识符，逐一在 `crates/`、`apps/` 中查找同名 `fn` 或同标识符。
**边界**：只证明引用可达，不证明行为正确或断言强度。任何重命名或删除测试后必须重跑。

**本次修正的两个失效引用**（均为测试重命名所致，实现与断言仍在，不属功能缺失；以下"（已失效名）"按闸门口径不计入引用）：
- （已失效名）question_wait_cursor_selects_the_baseline_without_advancing_the_question
  → 现名 `question_wait_cursor_resumes_instead_of_waking_on_session_revision`
  （`9f8a243c` 审核 P2-3 修复游标语义时一并改名）。
- （已失效名）cancel_after_timeout_settles_the_question_and_blocks_further_waiting
  → 现名 `cancel_after_observation_settles_the_question_and_blocks_further_waiting`。

闸门口径补充：**不使用反引号**的标识符（如以"（已失效名）"前缀标出的历史名）不计入引用完整性检查，
因为它们是被明确标注为已失效的对照物，不是当前证据引用。

## 进度口径纠正

此前反复报告的约 70% 是未重新统计的粗估，现停止沿用，不以新的猜测百分比替代。实施报告记录了大量实际修复，但工作包总表未及时关账，造成整体进度不可见。此文件作为当前状态入口；旧日志保留当时事实，不代表当前待办。

“已实现”与“完成验收”分开。既有定向测试不自动等于全部组合通过；也不因尚未总验收而把已经实现的功能重新列为待开发。每次只更新发生变化的剩余项及相应证据。

## 已有实现

| 能力 | 当前实现与证据边界 |
| --- | --- |
| 消息语义 | 启动、引导、问题、答案、通知和最终任务结果有独立类型及处理；问题、通知不阻塞业务完成。已有领域和服务定向证据。 |
| 可靠通信 | 稳定操作、问题、投递及执行身份，持久化、幂等、查询、恢复已实现；问题、通知、任务的全部故障组合仍需销项。 |
| 调度 | 父子完成门、仅答疑、MainOnly 例外、辅助执行隔离及停止恢复已接线；组合验收未完成。 |
| 动态上下文 | 带标签 User 事件及 Goal、Plan、记忆、环境等迁移已实现；到期、撤下、压缩回填和历史前缀相关缺陷已修复并有测试。 |
| 协议及压缩 | 九种已注册配置有生产编码测试；本地摘要和原生压缩已有真实回环 HTTPS、可靠提交与冷加载证明。未证明全部实际可达路由及真实供应商缓存命中。 |
| 界面与授权 | 独立协作卡片、两条时间线、同项目权限及显式跨项目授权已有实现；当前版本组合复验仍需完成。 |
| 删除保护 | D330—D331 修复冷态子未结消息被递归删除，以及目标结算尚未获来源确认就删除的问题。最近最终生命周期 19 项、归档结算 2 项通过，生产检查通过。 |

## 剩余清单

### S1 生命周期闭环

- [x] S1.1 历史编辑清除来源尚未确认的目标回执：已复现并修复。编辑准入改为同时锁住目标会话与全部同伴，并在持锁期间完成发布，未确认回执不再可能被丢弃。证据：`history_edit_preserves_unacknowledged_target_settlement`、`history_edit_succeeds_after_source_settlement_acknowledgement`、`edit_admission_holds_target_lifecycle_gate`。提交 `680a54cd`、`6cf4a777`。
- [x] S1.2 归档后实际模型回答的服务链验证：归档保留开放回合、辅助执行与已接纳投递，模型回答经 `reply_question` 完成可靠结算并回写来源确认。证据：`archived_target_model_answer_completes_durable_settlement`（断言来源 `question_state == answered`、出站投递带 `settlement_ack`，归档目标保留入站 `settlement`）。提交 `6cf4a777`。
- [x] S1.3 对照 A08—A12、A17 销项停止、MainOnly、仅答疑和迟到消息组合。四个场景中只有三个存在真实缺口，仅答疑结束已有 5 项真实收尾器证据，**未为凑数新增测试**。新增 4 项真实服务级证据：`stopped_target_wakeup_fails_queued_task_without_resuming_business`（驱动真实自动恢复入口，证明全树停止不自动恢复旧任务）、`main_only_parent_admits_owned_child_question_without_resuming_goal_or_task`（真实嵌入子任务经原生端口送达 MainOnly 父会话，断言模式/运行控制/回合/输入计数四元组不变且不扩大到无关发送者）、`unanswered_question_and_notification_do_not_block_completion_and_late_question_does_not_revive`（真实完成门返回完成分支，且迟到问题不复活已完成任务）、`late_answer_after_settlement_is_rejected_without_reviving_target_task`（服务端口拒绝已结算问题的迟到回答，双方修订不变且磁盘重载一致）。提交 `3ce58418`。**未验证边界**：非确定性异常退出的机制未定证（候选：不可取消的阻塞生命周期写入器与运行时析构等待；已给出定证方法）；未驱动真实 WebUI/RPC 与运行中的真实 provider 流；未驱动真实停止事务；MainOnly 用例未创建 Active Goal（生产 Goal 分发权威还要求活动回合来源为 Goal 续跑，普通用户回合下该谓词恒为空）。

### S2 压缩收尾

- [x] S2.1 非 V1 独立运行时的压缩屏障消费及明确所有权：已修复。RuntimeOnly 启动在派发前显式声明非会话归属，独立消费方成为强类型确认所有者；准备边界走 `into_excluded_non_session()`，可靠性边界校验后恰好确认一次，任何会话投影事实一律 fail-closed。提交 `8fd2dbad`。
- [x] S2.2 原生压缩请求诊断制品接线。已确认改动前压缩路径**零制品**（许可里的追踪归属被丢弃、请求捕获显式传空、传输层无响应捕获），且用户开关在压缩分派链不可达。按"复用既有机制 + 只传逐分派冻结开关"实施：开关关闭时行为与改动前完全一致（不预留、不接触追踪目录），开启时复用同一 `ProviderApiTraceStore` 与同一渲染器（制品结构与脱敏规则未改），请求与响应两条捕获通道都接上。证据：`native_compaction_request_publishes_the_reused_provider_api_trace_artifact`（真实路由 + 回环 HTTP + 双通道捕获，断言制品路由/路径/正文逐值相等、终态分类、请求头凭据已脱敏、制品原始字节不含凭据）、`native_compaction_trace_requires_the_frozen_opt_in_decision`（关闭时不预留且追踪根目录不存在）、`ordinary_trace_reservation_requires_the_frozen_opt_in`、`ordinary_provider_request_trace_shape_is_unchanged`。提交 `09d5e25a`（`responses_compact.rs` 的生产部分因暂存区竞态落在 `c3707e3d`，已在该提交信息中如实说明）。
- [x] S2.3 OAuth 刷新后的新尝试、可靠确认失败阻断、主会话入口的专门验证。**结论：三段生产逻辑改动前已正确接线，缺的是可执行证据**，因此只补测试未改生产逻辑。证据：服务端 `remote_checkpoint` 11 项（含 `remote_compaction_retry_settles_failed_permit_then_confirms_only_the_new_permit`、`remote_compaction_confirmation_failure_blocks_the_producer_without_fabricating_success`、`remote_compaction_permit_refusal_keeps_the_admitted_attempt_unmodified`、`main_session_physical_entry_consumes_the_durable_native_checkpoint`），适配器 9 项（含真实回环 401→200、各自尝试身份、无隐藏重试）。提交 `c3707e3d`、`f3b325f4`。
- **S2.3 未验证边界（精确登记）**：真实 401 → OAuth 刷新 → 第二次真实 HTTP 的端到端一跳未驱动；**准备许可失败的生产者侧阻断无法跨 crate 驱动，缺少 preparation barrier 的 test-support 构造器**（应与 `events.rs` 既有 durability barrier 测试构造器同层，现有同层构造器为 `pub(crate)`），而不是缺少证据；主会话真实 provider 往返回路未驱动。

### S3 协议及动态注入

- [x] S3.1 动态生产者逐项到最终请求的连续前缀、更新、到期及压缩验证。已修正确认缺陷：临时注入宿主块作用域与会话作用域不一致，导致任何真实会话作用域临时注入在 V1 请求准备时直接失败；同一内容项的失效事件却硬编码回合作用域，属合同内部矛盾。修正生产者侧声明（临时块恒为回合作用域，持久块保留声明作用域），并把作用域常量收敛为单一来源。提交 `42651b34`、`523fa6cd`。
- [x] S3.2 逐生产者到最终请求的可达性与固定顶部边界。**纠正了任务书的假设**：`provider_layers` 并**不是**未守护缺口——它由三重事实守护（唯一消费者是闸门前候选、该候选明确拒绝非会话派发、V1 所有者整体重建），因此只做文档化与回归断言，未做结构性改造。新增证据：`request_dynamic_producers_stay_out_of_fixed_top_layers`、`session_plan_context_reaches_request_as_tagged_user_event`、`two_adjacent_requests_keep_fixed_top_and_append_dynamic_user_items`（相邻两次真实请求，断言固定顶部字节不变、更新只追加、被替代旧正文保留）、`ttl_expiry_appends_tagged_user_item_without_rewriting_emitted_prefix`。提交 `bca79df4`。
- [x] S3.3 continuation 的真实安全判定与重建诊断一致性；工具配对、并行结果、媒体交错与插入边界。已修正两个确认缺陷：(1) 同一生效锚点上的多条系统更新**逆序**插入，违反与输入一致的时间顺序契约（提交 `fc65b351`）；(2) dispatch 因边界后存在未覆盖 `runtime-host:` 消息返回 None，而预览诊断缺少该分支并会渲染"允许复用"，即判定与诊断不一致（提交 `fa1daa28`）。另补充工具配对校验、乱序并行结果按身份配对、媒体交错与插入边界的 12 项证据。
  **后续已补齐（本轮更正，原记"未覆盖"已失效）**：提交 `1dd4301f` 为两处"输出只被传递、从未被断言"的判定补了直接断言——`provider_continuation_eligibility_requires_a_replay_safe_attempt`（失败供应商尝试能否续跑）与 `uncovered_host_event_blocks_continuation_reuse`（未覆盖宿主事件阻止延续复用）；两处均经变异验证。同一提交把宿主前缀守卫由字面量 `"runtime-host:"` 改为引用生产者的 `RUNTIME_HOST_MESSAGE_PREFIX`（AGENTS.md 第十一条单一来源）。两处未知契约版本拒绝亦已由 `e71b0b30`、`f586e9e3` 补齐。
  **正向路径已补齐（本轮更正）**：我上一轮判定"证明正向路径需要复现 ContextInjection 持有的改写指纹快照，
  本层无法构造"并据此删掉了正向断言——该判断**是错的**：`ControllerConfig::provider_rewrite_fingerprint()`
  是公开方法，仓库既有测试已在用它构造合法延续。已补回对照断言（`b2650597`），并经变异验证
  （在构建器末尾插入 `return None;` 后该断言 FAILED）。

### S4 协作与界面

- [x] S4.1 双向等待、先到答案、多等待者、超时重等及问题／通知恢复逐项对应既有测试，补未覆盖组合。已覆盖：超时不消费问题且后续等待仍可观察（两次等待持有独立操作身份）、调用方游标选择等待基线且不推进问题（**实测纠正**：上报修订是所属会话的可靠修订，因此以它重新等待返回快照而非超时，原断言把实现假设当成了产品契约，已改写为检验真实语义）、超时后撤销结算问题并使后续等待不再报告待处理。证据：`question_wait_timeout_does_not_consume_the_question_for_a_later_wait`、`question_wait_cursor_resumes_instead_of_waking_on_session_revision`、`cancel_after_observation_settles_the_question_and_blocks_further_waiting`（后两条的前身在测试重命名前分别为（已失效名）question_wait_cursor_selects_the_baseline_without_advancing_the_question 与（已失效名）cancel_after_timeout_settles_the_question_and_blocks_further_waiting）。提交 `61718ca4`、`9f8a243c`。
  **后续已补齐（本轮更正，原记"答案先到、双向互问、通知恢复未覆盖"已失效）**：答案先到由 `answer_arriving_before_the_wait_is_observed_immediately`（`550a5c75`）补齐；迟回答由 `answer_after_asker_completion_settles_without_reviving_the_asker_turn`（`f6a497c6`）补齐；双向提问闭环由 `child_question_to_parent_round_trips_the_answer_back_to_the_child`（`e223082e`）补齐；通知恢复的"来源已提交、目标未接纳"窗口由 `25ecf35d` 修复并验证（修复计数 0→1、二次修复为 0），并另补停止时序（`72deb8c0`）与跨项目授权撤销（`599e630c`）。
  **"多等待者"经核查不适用**：`wait_question` 只授权问题的来源或目标，一个问题只有一个来源，因此同一问题不存在多个独立等待者。
  **仍未证明**：父子交叉互问（见 `S5_ACCEPTANCE_EVIDENCE.md` §三，已定位到可复现矛盾点但未定论）；"子代理提问真正触发调度"需要更高层构造。
- [x] S4.2 目录、读取、发送、回复及实际正文交付逐动作核对授权。已核实六种授权动作逐项匹配、跨项目必需授权、撤销即时生效；补 Ask/Notify 端口级逐动作与撤销证据。提交 `6a65e457`。
- [x] S4.3 当前界面、分页、多窗口重连及消息类型展示复验。证据（本轮累计实测）：`conversation-event-display-projection.test.ts`（16 项）、`conversation-event-window-store.test.ts`（**49 项**，含后补的多窗口与同 seq 跨实体两项）、`conversation-event-window-controller.test.ts`（**39 项，含"先恢复分页再重连"的完整组合**）、`selected-conversation-event-window-registry.test.ts`（17 项，含多窗口隔离）、`conversation-event-window-react.test.tsx`（4 项）；`tsc -b` 类型检查通过。
  **后续已补齐（本轮更正，原记"协作卡片无行为证据"已失效）**：`298a29bc` 新增 `conversation-collaboration-card.test.tsx`（8 项），**直接挂载生产卡片并触发回调**：身份路由、通知无回复控件、导航按可路由集合、撤销三条件、单次提交、失败可重试、时间戳保留。
  **"完整分页重连组合未断言"这一旧登记亦已失效（本轮更正）**：我此前只检索存储与注册表测试，漏看了控制器测试文件。
  `restores paged history before reconnecting and retains it while missing events arrive` 已覆盖完整组合。
  **仍在 §二.3/§三 登记**：卡片覆盖不到 `task-message` 身份分支与 `child_task_non_success_projected` 的正文抑制分支；真实浏览器路由跳转与真实宿主撤销往返未驱动。
- [x] S4.4 工具注册、模式过滤、执行、结果投影完整暴露链复核。已完成 G1（Plan 可见性过期断言）、G2（七条协作命令端口级分发测试）、G3（`wait_agent` 身份重复定义）、G8（协作工具文档注释不可逆损坏）。
  **G4 已收口（本轮更正，原记"需领域模型决策"已失效）**：`reply_question.unable_to_answer` 此前被我登记为需领域决策，查证后**领域已指定载体**——`CollaborationAnswerRecord.body` 的注释即 "Exact answer body, **including explicit inability to answer**"，且 `answer` 必填（`minLength 1`）；工具参数是回显给回答方的确认，问题所有者只读到正文。因此真实缺口只是**契约从未被断言**，已由 `explicit_unable_to_answer_is_echoed_and_travels_in_the_durable_body`（`4a52755b`）固定：回显为 true、所有者读到的正文逐字一致、结算为 `Answered`、序列化后的可靠答案记录不含该字段（防两面漂移）。
  **仍未做**：G5（仅答疑模式工具收窄范围）、G6/G9（低优先级）。

### S5 总验收与交付

- [x] S5.1 整理最终字段来源、当前 Schema、路由和用户资料，清除当前状态表中的过期待办。已完成：字段唯一来源、工具路由与用户使用说明见 [S5 字段与路由汇总](S5_FIELD_AND_ROUTE_SUMMARY.md)；本文件已清除失效引用与过期待办（见"引用完整性"节）。
- [x] S5.2 原 P00—P12、R01—R14、A01—A18 逐项归档证据并关闭工作包。已完成对照（[S5 验收证据对照](S5_ACCEPTANCE_EVIDENCE.md)）：A15 已升级为**已实现并已验证**；A09、A10、A16 仍为"证据不足"，各自缺口在 §二/§三 逐条写明；R10 仍为"证据不足"。
- [ ] S5.3 连续五轮完整独自审核无新问题。目前 0/5；局部修复和测试不计轮次。**该五轮属于原主代理，实施侧不得自行计入。**
- [ ] S5.4 总结、按仓库约定归档正式计划，达到全部要求后结束目标。

## 原主代理第 1 轮审核结论与修复（审核范围 280f31a2..470386c7）

审核提出 3 项逻辑问题与 2 类验收缺口。三项逻辑问题**全部确认成立并已修复**，每项均带独立复跑证据：

- **P1-1 RuntimeOnly 远端压缩被独立校验误判失败**（提交 `c311ee31`）。根因比审核描述更深：远端准备屏障原先**显式拒绝** `ExcludedNonSession` 许可，而独立所有者放行的恰好是该变体，两条路径在类型层面互斥；同时 `durable_events` 按归属置空使 RuntimeOnly 走"无屏障"分支，独立所有者的准入计数为零。修复：远端物理尝试总会发布强制屏障并由各自归属确认；`ownership` 随请求传递；准备屏障仅对 RuntimeOnly 接受非会话许可，其余归属维持原拒绝。**未删除任何校验。** 未补端到端回归（需真实执行对象与回环 HTTP，本层无法构造），验证级别为编译 + 既有路径回归 + 源码契约核对。
- **P1-2 历史编辑漏查嵌入子会话树**（提交 `ca83e0c2`）。已确认：树成员本已枚举并加锁，但未结协作与来源确认检查只作用于根，子会话自身未结清的问题或投递会被放行，而编辑会重置 `child_tasks`，使启动恢复失去发现嵌入子会话的入口。修复：按成员逐个校验（热态经句柄、冷态经定位器恢复快照，两者共用同一判定），来源确认检查接收整棵树。**并把未结判定上移为 `CollaborationState` 的唯一方法**：初版冷态路径曾另写一份判定，遗漏了任务消息、子任务、问题执行、通知与"已投影"入站投递，属仓库规则禁止的第二处定义。
- **P2-3 等待游标使用会话修订号**（提交 `9f8a243c`）。已确认，且审核指出"新增测试仍未解决"成立——我上一轮的测试只断言问题仍为待处理，没有断言实际等待。修复：游标改为问题与其投递的确定性内容摘要（FNV-1a 64 位），只在协作事实变化时变化；无游标等待仍在其自身截止时间结束。测试改写为断言**超时本身**。

两项**验收缺口**尚未闭合，且不是"实现可能有错"而是"证据不足"：

- **S1.3 的 MainOnly 与迟到回答未覆盖完整生命周期**（部分进展，提交 `3df19776`）。已在生产问题流任务使用的**真实启动闸门** `question_launch_restart_state` 上补证据：`MainOnly` 准入且准入表声明 `wakes_retained_parent_turn`/`suppresses_business_completion` 为真；`Stopped` 与 `MainAndSubagents` 均拒绝；三种状态下闸门都是纯读取方（回合标识、占位执行、入站投递与可靠修订逐字不变）。**仍未闭合**：未验证"子代理提问触发自动唤醒调度"（需要知识访问解析、WebUI 准备代次所有者、Agent 运行时桥与宿主句柄，本层无法构造）；未验证"工具回复后子代理收到答案"；未验证恢复答疑时 Goal 应符合的约定；**迟回答部分（提问方业务完成后首次答案到达）尚未处理**。
- **S2.3 的远端重试未验证真实 OAuth 恢复编排**：现有测试手工创建两次压缩流，绕过"401→结算→刷新凭据→重新取得许可→持久化终态"的真实编排；即使编排漏掉刷新、许可或终态屏障，该测试仍会通过。

**本轮新识别并记录的边界与疑点**：

- `wait_question` 与 `wait_session` 各持一套独立基线。前者已改为协作内容游标（`9f8a243c`）；后者以**目标会话的文档修订号**为基线（`session_wait.rs`）。**疑点已核查并排除**（提交 `77b1cbf0`）：两者的差异是关键——`wait_question` 原先比较的是**监视者自身会话**的修订，而监视者每次调用等待工具都会推进它；`wait_session` 比较的是**被观察会话**的修订，监视者自身的写入不进入该比较。真实端口测试确认：无游标等待在自身截止时间结束且返回的基线等于被观察会话修订，以该游标恢复仍为超时。
- `question_launch` 的三态准入表原先只有谓词级证据（`agent_collaboration_execution.rs` 的单元测试），真实闸门证据由 `3df19776` 补齐。

## 执行约束

用户最新授权保存本次检查点并由后续模型实施、原主代理审核；本轮不启动子代理。保护并行 VCE 和用量改动；不兼容历史数据；按交接方案精确提交，不推送。不增加新的产品范围，发现缺陷必须归入上述原要求。测试先定位具体风险，已经充分验证的内容不反复全量运行。详细失败与修复证据仍保存在实施报告。

## 当前工作树状态（供审核核对）

第 1 轮审核范围内的并行在途编辑已全部落定，`vulcan-agent-runtime`、`vulcan-provider-adapters`、`vulcan-service-kernel`、`vulcan-session-core` 均可编译。**最新复跑（供审核直接引用，完整清单见 S5 §四）**：

- `agent_collaboration::` **122 passed / 0 failed**（此前记录的 113 已过期）；
- `vulcan-session-core --lib` **460 passed / 0 failed**；
- `vulcan-agent-runtime --lib` **1027 passed / 0 failed / 4 ignored**（`RUST_MIN_STACK=67108864`）；
- `vulcan-service-kernel --lib` 全量 **1534 passed / 7 failed / 15 ignored**，7 项归属**另一并行会话的供应商与模型工作**，逐项名称与归属判定见 S5 §四，本任务登记不修复；
- `apps/vulcan-code-web`：`tsc -b` 0 错误；`vitest run` 全量 1842 passed / 4 failed（既有，不属本任务）。

`crates/vulcan-runtime-assets/resources/provider-manifests/` 的改动仍属于**其他会话的供应商工作**，本任务不修改、不暂存、不提交。

检查点标签核对：`refs/tags/checkpoint/agent-communication-20260922` 指向标注对象 `7f4b684d`，其提交为 `280f31a2`（"保存代理协作与动态上下文实现检查点"），与交接方案一致，未被移动；`280f31a2` 仍是当前 `HEAD` 的祖先。

