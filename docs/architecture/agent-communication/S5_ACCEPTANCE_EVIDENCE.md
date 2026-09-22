# S5 验收证据对照（原方案 P00—P12 / R01—R14 / A01—A18）

本文件是 S5.2 的交付物：把原正式方案 `docs/plan/20260920-04-AGENT_COMMUNICATION_AND_DYNAMIC_CONTEXT.md`
的全部需求行，逐项对应到**当前源码事实**与**已提交的验收证据**。

- 本文件**不替代**原方案，也不缩减任何要求。
- **完整审核仍为 0/5**：五轮连续完整独自审核属于原主代理，本文件不声明任何一轮通过。
- 状态口径：`已实现并已验证` 表示有本仓库内可复跑的行为证据；`已实现，证据不足` 表示源码事实清楚
  但缺可执行验收；`未验证边界` 表示需要本层无法构造的环境才能验证。**不使用未经复跑的百分比。**

## 〇、阅读约定（相对时间措辞如何解读）

本文件在**多轮实施中增量写成**，文内出现的"本轮""前一轮""我此前"等相对措辞**不是相对你阅读的
时刻**，而是指某一次实施轮次。为避免后续读者误解，约定如下：

- "本轮" = **产生该句所在段落最后修改的那一次提交**；判定依据是该句所属条目的**提交号**，
  而不是任何时间描述。
- "我此前／上一轮／前一轮（的登记）" = 本文件**保留的历史判断**，且**已被同条目中的更晚说明取代**。
  这类句子一律与取代它的提交号写在一起，供审核追溯判断演进；**以更晚的说明为准**。
- **权威顺序**：① 当前源码 ② 状态列（§一）③ 提交号所指的提交信息 ④ 本文件的叙述文字。
  叙述文字与源码冲突时**以源码为准**，并应把冲突作为缺陷登记，而不是采信本文件。
- 本文件刻意**不删除**被推翻的自身判断。审核者若只读一处，请读 §一 状态列与 §二/§三 的缺口列表。

## 一、需求行对照

### 1.1 R01—R14（正式方案的需求行）

| 需求 | 源码事实 | 已提交证据 | 状态 |
| --- | --- | --- | --- |
| R01 稳定顶部保留；非顶部宿主提示按事件时点追加 `User` + 版本化功能标签 | `provider_request_bridge_messages/projection.rs`；固定顶部与动态项的分离由 `provider_request_preview.rs` 的复用判定守护 | `request_dynamic_producers_stay_out_of_fixed_top_layers`、`session_plan_context_reaches_request_as_tagged_user_event`、`two_adjacent_requests_keep_fixed_top_and_append_dynamic_user_items`、`ttl_expiry_appends_tagged_user_item_without_rewriting_emitted_prefix` | 已实现并已验证 |
| R02 真实用户/助手历史/原生工具调用与结果/媒体保持各自身份；标签不授权 | 原生结构在 `provider_request_bridge` 与 `tool_result_media.rs` 中保持；授权只经 `CollaborationGrantAction` | 投影与媒体边界证据（`provider_request_bridge_messages` 30 项、媒体投影用例） | 已实现并已验证 |
| R03 启动任务与方向插队同属任务完成链；插队改方向不取消执行 | `session_messages.rs` 的方向投递与 `task_launch.rs` 的排队任务 | `native_dispatch_send_notification_records_without_reply_contract`、任务排队与后继绑定证据 | 已实现并已验证 |
| R04 临时问答为独立事务；须用工具回答指定 `question_id`；允许无答复 | `reply_question` 只接受 `question_id`；`AskQuestion`/`ReplyQuestion` 端口 | `native_dispatch_ask_question_records_question_and_delivery`、`native_dispatch_cancel_question_settles_pending_question` | 已实现并已验证 |
| R05 提问持久接纳后立即返回问题 ID；新问题唤醒等待中的接收方 | `ask_question` 提交后返回；`incoming_question_snapshot` 优先分支 | `native_dispatch_wait_question_returns_bounded_pending_snapshot`、`wait_agent_returns_incoming_question_without_consuming_it` | 已实现并已验证 |
| R06 通知与问题均不阻塞完成；发送立即返回；仅主动等待才等待；超时不自动撤销；晚到答案仍归原问题 | 完成门不含问题/通知义务（`session_runtime_registry_v1.rs`）；`wait_question` 不修改问题 | `unanswered_question_and_notification_do_not_block_completion_and_late_question_does_not_revive`、`question_wait_timeout_does_not_consume_the_question_for_a_later_wait`、`question_wait_cursor_resumes_instead_of_waking_on_session_revision` | 已实现并已验证 |
| R07 主会话子任务未结束时主逻辑 turn 不得正常完成；子到主消息插入原 turn | 完成门的 `open_child_task_ids` 分支 | `main_only_parent_admits_owned_child_question_without_resuming_goal_or_task`、S1.1 回执保护证据 | 已实现并已验证 |
| R08 自然完成且空闲的合法目标可自动执行一次仅答疑，结束后返回空闲 | `NaturallyIdle` + `NaturallyIdleQuestion` 准入 | `question_finished_preserves_business_state_and_restores_original_control`、`naturally_idle_admits_only_qa` | 已实现并已验证 |
| R09 遇到明确暂停/停止目标时拒收并明确告知 | `admit_collaboration_execution` 的 `Stopped` / `MainAndSubagents` 分支 | `question_launch_admits_main_only_resume_and_refuses_stop_states_unchanged`、`stopped_target_wakeup_fails_queued_task_without_resuming_business`、`question_launch_rejects_stopped_targets` | 已实现并已验证 |
| R10 子到主为独立方向；`MainOnly` 允许子代理问题自动恢复主会话 | 准入表 `(MainOnly, InTurnQuestion)` → `wakes_retained_parent_turn = true`；真实闸门 `question_launch_restart_state` | `question_launch_admits_main_only_resume_and_refuses_stop_states_unchanged` | **已实现，证据不足**（见 §二.1） |
| R11 同项目可查找/读取/提问/引导；跨项目需明确授权 | `list_authorized_sessions` 项目范围；`document_grant_allows` 逐动作 | `native_dispatch_list_and_read_session_project_bounded_ranges`、`cross_project_*` 三项授权证据 | 已实现并已验证 |
| R12 临时记忆到期保留已发送正文并追加失效标记；压缩排除失效内容 | `state.rs` 的 `TRANSIENT_TURN_SCOPE` 生产者侧声明；失效事件不重写历史 | `ttl_expiry_appends_tagged_user_item_without_rewriting_emitted_prefix` | 已实现并已验证 |
| R13 单向通知不要求回复且独立于任务/方向/问题；下一合法边界自然插队 | `notifications.rs` 的发送与目标接纳；`projection.rs` 的通知投影 | `native_dispatch_send_notification_records_without_reply_contract`、`unanswered_...`（通知不阻塞完成） | 已实现并已验证 |
| R14 代理通知/问题/答案/任务回报使用独立交互框，视觉上区别真实用户提示 | 前端 `conversation-event-display-projection` 与协作卡片；卡片自身按后状态派生卡片身份、撤销与导航控件 | 事件投影与事件窗口测试；**协作卡片自身 8 项行为测试**（`298a29bc`，直接挂载并触发回调：身份路由、通知无回复控件、导航按可路由集合、撤销三条件、单次提交、失败可重试、时间戳保留） | 已实现并已验证 |

### 1.2 A01—A18（验收语句）

| 项 | 需求要点 | 已提交证据 | 状态 |
| --- | --- | --- | --- |
| A01 | 稳定顶部不混入动态提示；新增事件按发生顺序 | `request_dynamic_producers_stay_out_of_fixed_top_layers` 等 4 项 | 已实现并已验证 |
| A02 | Goal/Plan/TTL/VMM/环境变化不重写已发消息；Plan 不重复投影 | 同锚点逆序修正与插入边界 12 项（`fc65b351`） | 已实现并已验证 |
| A03 | 连续 User 合并、并行工具结果、媒体及服务端工具续轮保持合法结构 | 工具配对/乱序并行结果按身份配对/媒体交错证据（`fc65b351`）；`provider_request_bridge_messages` 30 项 | 已实现并已验证 |
| A04 | 启动最终回复归对应任务；queue 与 interrupt 绑定正确 | 排队任务终态与后继绑定证据；`stopped_target_wakeup_fails_queued_task_without_resuming_business` | 已实现并已验证 |
| A05 | 主问子/子问主/同项目独立会话问答发送即返回；回执与答案分开 | `native_dispatch_ask_question_...`、`native_dispatch_wait_...`、S2.3 服务端 11 项 | 已实现并已验证 |
| A06 | 父等待多个子任务时一个子问题立即唤醒；问题不计作子任务完成 | `wait_agent_returns_incoming_question_without_consuming_it`；完成门不含问题义务 | 已实现并已验证 |
| A07 | 双向提问、答案先到、多等待者、超时再等待不丢答复、不互等 | 超时/重等/撤销三项（`61718ca4`）+ 游标修复（`9f8a243c`）+ 答案先到（`550a5c75`）+ **双向提问闭环**（`e223082e`：子问父→父作答→子经真实等待入口观察到答案，且问题记录权威留在提问方）+ **父 → 子方向**（`parent_question_to_owned_embedded_child_binds_the_question_to_the_child`） | 已实现并已验证（多等待者不适用，见 §二.1） |
| A08 | 主模型先 final 而子仍运行时各层保持未完成 | 完成门 `open_child_task_ids`；`main_only_parent_...` 不解冻 | 已实现并已验证 |
| A09 | 未回复不阻止完成；仅主动等待才等待；晚到答案不复活旧任务；无法回答可明确表达 | `unanswered_question_and_notification_do_not_block_completion_and_late_question_does_not_revive`、`late_answer_after_settlement_is_rejected_without_reviving_target_task`、**无法回答声明契约**（`explicit_unable_to_answer_is_echoed_and_travels_in_the_durable_body`，`4a52755b`） | **已实现，证据不足**（"无法回答明确表达"已补证据；A09 其余要求项未在本包覆盖，见 §二.2） |
| A10 | 自然空闲仅答疑不二次完成；MainOnly 可恢复；停止拒收；全树停止后迟到不复活 | `question_finished_...`、`question_launch_admits_main_only_...`、`stopped_target_wakeup_...` | **已实现，证据不足**（自动唤醒调度一跳，见 §二.1） |
| A11 | 重复与并发按稳定标识结算；回执丢失后恢复原记录；同键不同正文明确冲突 | 操作身份幂等与 `RequestIdentityMismatch` 证据；S2.3 服务端 11 项 | 已实现并已验证 |
| A12 | 各类崩溃窗口无静默丢失与虚假成功 | 归档/删除/冷加载证据；`remote_compaction_confirmation_failure_blocks_the_producer_without_fabricating_success`；**通知"来源已提交、目标未接纳"窗口的修复**（`25ecf35d`：修复计数 0→1、身份逐字不变、二次修复为 0） | 已实现并已验证 |
| A13 | 压缩与尾段加载后仍能确定问题双方/状态/授权/未结事项；失效提示不被恢复成当前指令 | S3.1 作用域修正（`42651b34`、`523fa6cd`）；压缩前后协作证据 | 已实现并已验证 |
| A14 | 标签伪造、错误收件人、跨树、未授权跨项目、撤销后发送均不能越权 | `cross_project_*` 三项 + `document_grant_allows` 逐动作（`6a65e457`） | 已实现并已验证 |
| A15 | 仅支持当前新契约；未知版本明确拒绝；写入中断有可恢复边界 | 严格版本读取与拒绝路径；**两处未知版本拒绝已补齐**（`persisted_route_lease_with_unknown_schema_version_is_refused`、`media_output_rejects_unknown_envelope_version`，`e71b0b30`/`f586e9e3`，均经变异验证）；**写入中断可恢复边界**由既有 `question_finished_storage_failure_recovers_committed_outcome` 与 `question_finished_persists_queued_successor_and_recovers_failed_publication` 覆盖（脚本化刷写故障注入） | 已实现并已验证 |
| A16 | 多窗口/分页重连按 ID/游标恢复；结果/答案/回执不互相冒充；消耗只计一次 | 窗口控制器 39 项（含 `restores paged history before reconnecting and retains it while missing events arrive`、`prepends only an adjacent complete-Turn page and advances its cursor`、`shares one in-flight historical page across concurrent prepend requests`、`rehydrates when reconnect reports that the service process changed`）、窗口注册表 17 项、事件窗口存储 49 项（含 **多窗口不抢唯一消息** `keeps two windows over one session independent for the same single event`，`5c848967`；**同 seq 跨实体不互相冒充** `refuses to read a different collaboration kind as a revision of another`，`822bfddc`，经变异验证）；`terminal_report_*` 用量分离（失败已修正，`2bf00026`） | 已实现并已验证 |
| A17 | 归档仍可结算；未结时删除受阻；取消/结算后删除及重启时迟到操作有确定状态 | 归档/删除生命周期 19 项 + 编辑准入修复（`ca83e0c2`） | 已实现并已验证 |
| A18 | 子代理可继续工作；主代理在合法边界收到通知且无回答义务；交互框区分 | 通知投影与独立节点证据；前端 123 项 | 已实现并已验证 |

### 1.3 P00—P12（工作包）

| 工作包 | 对应提交 | 状态 |
| --- | --- | --- |
| P00 事实与落盘边界 | 全部包的前置调查 | 已完成（字段唯一来源与可达 Profile 已整理进本文件与状态入口） |
| P01 类型与身份 | `a98103c7`（G3 身份去重） | 已实现并已验证 |
| P02 可靠投递与恢复 | `6cf4a777`、`fa1daa28`、`3ce58418` | 已实现并已验证 |
| P03 完成门与运行目的 | `680a54cd`、`3ce58418` | 已实现并已验证 |
| P04 信封与中立投影 | `bca79df4` | 已实现并已验证 |
| P05 动态生产者迁移 | `bca79df4`、`42651b34`、`523fa6cd` | 已实现并已验证 |
| P06 协议生产编码 | `fc65b351`、`fa1daa28` | 已实现并已验证 |
| P07 临时问答与等待 | `61718ca4`、`9f8a243c`、`77b1cbf0`、`550a5c75`、`f6a497c6`、`e223082e`、`a8c3e753` | 已实现并已验证（双向提问均已证明；R10/A10 仅剩"模型级调度一跳"，见 §二.1） |
| P08 父子工具完整暴露 | `d5c2865e`、`433dead7`、`6a65e457` | 已实现并已验证 |
| P09 独立会话权限 | `6a65e457` | 已实现并已验证 |
| P10 压缩与当前契约恢复 | `8fd2dbad`、`09d5e25a`、`c311ee31`、`f3b325f4` | 已实现并已验证 |
| P11 界面与资料 | 前端 123 项 + `conversation-collaboration-card.test.tsx`（8 项）+ 本文件 | 已实现并已验证（卡片组件行为已由 `298a29bc` 闭合并直接挂载生产组件；真实浏览器路由跳转与真实宿主撤销往返仍属未验证边界，见 §三） |
| P12 总验收和交付 | 本文件 + 状态入口 | **未完成**（五轮审核未完成） |

## 二、尚未闭合的验收缺口（都是证据不足，不是已知实现错误）

> 本节按"每条缺口单独编号"记录，**其中已闭合的条目保留原文并就地标注闭合提交**，便于审核追溯
> 判断的演进过程。当前**仍然开放**的只有第 1 项（R10/A10 的模型级一跳）与第 5 项（前端既有失败，
> 不属本任务）。第 2、3、4 项均已闭合。

1. **MainOnly 自动恢复的完整一跳与多等待者/父子交叉互问**（R10、A07、A10）。
   已证明：真实启动闸门准入并声明 `wakes_retained_parent_turn`、两种停止状态拒绝、闸门为纯读取方
   （`3df19776`）；提问方业务完成后首次答案到达仍正确结算且不复活旧业务（`f6a497c6`）；
   答案先到时立即观察到而不等满超时（`550a5c75`）；超时/重等/撤销与游标语义（`61718ca4`、`9f8a243c`）；
   **双向提问闭环**：子问父→父作答→子在子代理自己的问题上观察到逐字一致的答案，且问题记录权威
   留在提问方（`e223082e`）。
   **"多等待者"不适用**：`wait_question` 只授权问题的来源或目标，一个问题只有一个来源，因此同一
   问题不存在多个独立等待者；"父等待多个子任务"属 A06，已由 `wait_agent_returns_incoming_question_...`
   与 `session_wait_returns_incoming_question_without_consuming_it` 覆盖（保留原目标与游标）。
   **"工具回复后子代理收到答案"这一旧登记已失效（前一轮更正）**：同一测试
   `child_question_to_parent_round_trips_the_answer_back_to_the_child` 已经覆盖它——父代理经
   `ReplyQuestion` 作答后，子代理**通过真实等待入口** `WaitQuestion` 观察，断言 `status == "ok"`、
   `question_state == "answered"`、且 `answer.body` **逐字等于**父代理产出的正文。
   **"父子交叉互问"已双向验证（本节曾两次改写；当前结论见下）**：
   - **子 → 父方向：已证明**（`e223082e`），父代理的作答经 `ReplyQuestion` 落盘为规范答案。
   **"父子交叉互问"已双向验证（本轮更正——我上一轮的错误登记已撤销）**：
   - **子 → 父方向：已证明**（`e223082e`）。
   - **父 → 子方向：已证明（本轮新增）**。
     `parent_question_to_owned_embedded_child_binds_the_question_to_the_child` 由提问会话以
     `AskQuestion(target = 其自有嵌入子会话)` 提问，断言：返回 `status == "ok"`；问题记录留在提问方
     且 `state == Pending`、`target.session_id == 子会话`、`source.session_id == 提问方`；
     子会话持有该精确问题的已接纳入站投递、`source` 为提问方、`target` 为子会话、`settlement` 为 `None`。
   - **我上一轮的登记是错的，已撤销**：我曾把它写成 `#[ignore]` 的"可复现拒绝"
     （`Err("ask_parent requires an embedded child session")`）并称"未定论"。**那是我的夹具错误**：
     我把嵌入子会话建在 `message_fixture` 第三个返回值 `target` 之下，却用**另一个**会话
     （`invocation.source_handle`）去提问；归属是对**调用方自身**用量账本的证明，因此被拒。
     逐值诊断确认：提问方与 `target` 是两个不同会话，提问方账本 `children_by_session_id` 为空。
     更正后（子会话建在提问会话之下并由其预留 `child_task`，此时账本登记该子会话）请求成功。
   - **由此得到的教训已固化**：该测试现在额外断言提问方账本确实登记了该子会话，
     使同类夹具漂移**明确失败**，而不是伪装成一次"产品授权拒绝"。
   **仍未证明**：子代理提问真正触发调度（需要知识访问解析、WebUI 准备代次所有者、
   Agent 运行时桥与宿主句柄，本层无法构造）。
   通知投递丢失后恢复已由 `25ecf35d` 补齐（原为**实现缺口**而非证据不足）。

2. **`reply_question` 的 `unable_to_answer`**（A09）。
   源码事实（本轮逐行核对）：该字段是回复工具的一个**布尔开关**（Schema 中 `default: false`，
   且 `required` 只含 `question_id` 与 `answer`），服务端读取它、并在返回给调用方的回执中
   以 `unable_to_answer` 回显；**它不进入任何持久字段**。
   但"无法回答"这一语义**仍有可靠承载**：`answer` 正文是必填的、并且是规范答案记录，
   模型在声明无法回答时仍必须给出正文，因此该声明随答案正文一起持久化。
   结论：A09 的"可通过工具明确表达无法回答"在"必填正文 + 显式开关"这一形态下是可达的，
   **缺的是把该开关本身作为可独立查询的终态事实**。
   是否要把"无法回答"提升为独立领域状态（而不是由正文承载）是一项**领域模型决策**：
   方案 A 持久化该开关；方案 B 移除该字段并让正文单独承载。本任务不擅自决定领域语义，
   登记为待决策缺口，并在 §二 的状态中标注为"证据不足"而非"缺陷"。

   **本轮收口（`4a52755b`）并撤回上述"需领域决策"的登记**：领域记录**已经指定了载体**——
   `CollaborationAnswerRecord.body` 的注释即"Exact answer body, **including explicit inability to
   answer**"，且 `answer` 为必填（`minLength 1`、无 default）。因此"无法回答"由正文承载是
   **既有领域决定**，不是缺字段；工具参数是回显给回答方的确认。该条的真实缺口只是**契约从未被断言**，
   已由 `explicit_unable_to_answer_is_echoed_and_travels_in_the_durable_body` 固定：回答方回显为
   true、问题所有者读到的 `body` 等于提交正文、问题结算为 `Answered`、序列化后的可靠答案记录
   **不含** `unable_to_answer` 字段（防止两面静默漂移）。
   **仍不主张**该参数是可独立查询的终态事实；若日后要求这一点，仍需领域选择，本包不替其决定。

3. **协作卡片的组件级行为**（R14、P11）——**已于 `298a29bc` 闭合**。
   新增 `apps/vulcan-code-web/src/components/conversation/conversation-collaboration-card.test.tsx`
   直接挂载生产组件并触发其回调，覆盖卡片身份路由、通知不提供回复控件、会话导航按可路由集合门控、
   撤销控件的三个可用条件、连续点击的单次提交、失败后可重试，以及投递更新后保留宿主原始时间戳。
   该文件 8 项通过，`tsc -b` 无错误。
   仍**未覆盖**：`task-message` 身份分支与 `child_task_non_success_projected` 的正文抑制分支
   （既有显示渲染器测试覆盖 6 种 task-message 事件，本包未重复）；真实浏览器路由跳转与真实宿主撤销往返。

   **A16"完整分页重连组合未断言"这一旧登记已失效（本轮更正）**：我此前只检索了存储与注册表测试，
   漏看了窗口控制器测试文件。`conversation-event-window-controller.test.ts` 的
   `restores paged history before reconnecting and retains it while missing events arrive`
   已经覆盖完整组合——先水合并 prepend 更早分页（seq 1–4），dispose 后由新控制器
   `restoreCachedSnapshot` 立即渲染同一缓存且**不开启**实时订阅，再 `resumeCachedWindow`
   只恢复缺失修订区间（断言 `reconnect` 以缓存游标 4 调用、旧分页 1–4 保留、最终为 1–5），
   并断言 `subscribeLive` 恰好一次。同文件的并发 prepend 单飞、分页失败可重试、
   服务进程代次变化后 rehydrate 等亦已覆盖。据此 A16 升级为**已实现并已验证**。

4. **`terminal_report_separates_subagent_usage_from_parent_totals` 曾长期失败（A16），已修正**。
   **根因（已定位到具体语句）**：生产代码在 `crates/vulcan-agent-runtime/src/turn_stats.rs`
   中**有意**把子代理分段排除在父回合统计之外：
   - 第 234—237 行在合并显式分段前过滤掉 `SessionTurnUsageSegmentKind::SubagentTask`，
     注释写明"子代理用量属于根生命周期累计，不属于父会话回合"；
   - `merge_explicit_usage_segment_into_turn_stats` 与相邻助手分段路径对同一类型**再次提前返回**。
   因此父回合的 `usage_groups` 中永远不会出现 `SessionTurnUsageGroupKind::Subagent`，
   而测试末尾却 `.expect("subagent group should exist")`。

   **测试与生产在检查点 `280f31a2` 已同时如此**：`git show 280f31a2:<两个文件>` 显示过滤语句与
   该断言当时即已共存。因此这不是本任务引入的回归，而是**测试相对其自身生产不变式已经过期**——
   该测试的文档注释本身就写着"验证子代理用量保持独立，不会污染父级直接统计"，
   而它末尾的组断言恰好要求把子代理用量并入父回合分组，与前 4 条断言（`provider_call_count == 1`、
   `input_tokens == 31`、`total_tokens == 34`，即父统计未被污染）自相矛盾。

   **已按此方向修正（`2bf00026`）**：该测试改为在**两个**父级可见面上断言排除——
   `usage_groups` 只含 `Regular`、`usage_segments` 只含 `AssistantResponse`，并逐组断言不含子代理的
   `total_tokens`/`input_tokens`/`tool_call_count`。实测父回合为
   `groups=["Regular"]`、`segments=["AssistantResponse"]`，即分组与分段**两路**都过滤。
   **变异验证**：把生产过滤器改成不排除 `SubagentTask` 后，修正后的测试 FAILED，父回合分段变为
   `[SubagentTask, AssistantResponse]`，证明测试守的是真实泄漏路径；生产文件已恢复且无 diff。
   修正后 `vulcan-agent-runtime --lib` 全量 **1025 passed / 0 failed**，该 crate 首次全绿。
   **本项关闭了"用量分离测试失败"；A16 其后已整体升级为"已实现并已验证"**（`84095813`，
   多窗口唯一消息与同 seq 跨实体不互相冒充补齐后），故此处不再有"证据不足"的遗留。

   归因证据：`git log 280f31a2..HEAD -- crates/vulcan-agent-runtime/src/session_event_runtime`
   与 `-- crates/vulcan-agent-runtime/src/turn_stats.rs` **均为空**；本任务的提交只触及
   协作、压缩与测试文件。

5. **前端 4 项既有失败**（不属本任务所有权）。
   `npx vitest run` 全量为 1842 passed / 4 failed / 1 skipped。四项失败位于
   `composer-panel/goal-strip.test.tsx`（期望 `150 / 20K`）、
   `features/skills/vulcan-plugins-panel.test.tsx`（期望 Playwright MCP / Core 浏览器就绪文案）、
   `conversation/conversation-v1-tool-card.test.tsx` 与
   `conversation/conversation-tool-presentation-card.test.tsx`（均期望 `subagent-orchestration-compact-usage`）。
   归因证据：这四个文件与其生产代码在 `280f31a2..HEAD` 均**未被任何提交改动**，
   且在本任务的卡片测试加入之前即已失败。按 AGENTS.md 一.2 记录边界，不越界修复。

## 三、未验证边界（需要本层无法构造的环境）

- **运行中的真实 Provider 流**：V1 `SystemContext → ProjectedItem → provider_v1_sink` 的物理消息、
  真实模型回答、`RuntimeOnly` 远端压缩端到端一跳、真实 401 → OAuth 刷新 → 第二次真实 HTTP。
- **"401 → 刷新 → 重放"的中间连接已补齐并加守护**（`b619280a`），但**仍缺真实端到端编排**。
  已修复的结构性风险：运行时重试判定 `refreshed_oauth_credential_retry_decision` 的原有测试
  **自行复刻**"凭据已刷新"标记，不经过生产者也不经过适配器的 Core 错误投影，因此**若适配器不再
  发出该子类别或不再置 `retryable`，该测试仍会通过**而真实重放失效。
  修复方式：把生产者的标注抽成唯一来源
  `provider_transport_vulcan::streaming::mark_credential_refreshed_after_unauthorized`，
  并新增跨层测试 `host_retry_decision_accepts_the_adapters_real_credential_recovery_marker`：
  先断言未恢复的 401 不授权重放，再**调用适配器自身的标注函数**并经 `public_execution_failure`
  投影，最后断言宿主判定给出一次性零延迟重放。**已做变异验证**（把生产者写成 "MUTANT" 后该测试
  确实失败，恢复后通过），因此它是真断言而非永真测试。
  仍缺：**真实的"401 → 真实刷新 → 第二次真实 HTTP"**。合成凭据不可续期，
  `refresh_after_unauthorized` 会走 `Ok(false)` 分支，无法驱动重放路径；补齐需要可注入的凭据覆盖。
  本项另两个子缺口经复核**已有覆盖**，不需新增：
  `standalone_confirmation_refusal_and_vanished_consumer_fail_within_bounds`（许可被拒绝 →
  生产者收到强类型拒绝而非被丢弃的许可）与
  `provider_v1_sink/tests/remote_checkpoint.rs` 的
  `remote_compaction_confirmation_failure_blocks_the_producer_without_fabricating_success`
  （终态持久化失败 → 不跨屏障、不伪造成功）。
- **"父子交叉互问"（A07 的更强组合）**：**最新范围与结论见 §二.1**（本轮重写）。此处保留历史
  记录以便审核追溯判断演进，但**以下"未定论"的表述已被 §二.1 取代**：
  - **夹具侧是自洽的，不是问题所在**（前一轮更正）：父句柄的 `lifetime_usage_snapshot().root_session_id`
    等于父会话自身 `session_id`；子会话的 `root_session_id` 同样等于父会话；
    子会话在父的 `children_by_session_id` 中被登记且 `parent_session_id` 指向父；
    父文档预留的 `record.child.root_session_id` 等于该同一个父会话 ID。
    这也与生产发布规则一致：`child_creation/publication.rs` 要求
    `record.child.root_session_id == root_locator.session_id()`，而该函数对嵌入子会话按
    `parent_session_id == root_session_id` 处理。因此**"两棵树不一致"的推断不成立**。
  - 前一轮记为"未定论"的部分：以父句柄调用 `resolve_authorized_target(parent_invocation, child_id, Ask)` 时
    实测 `owned` 为 `false` 并回退到 `verified_direct_parent`。**本轮已判定该"矛盾"结论不可靠**，
    原因见 §二.1：`owned == false` 本就是设计上的回退条件，而把 `false` 当成异常依赖了生产侧插桩；
    那段插桩已全部移除，`git diff` 确认生产与测试文件均逐字恢复 HEAD。
  - **结论边界**：曾记为"未定论"的父 → 子拒绝**已被证明是我的夹具错误**（详见 §二.1），
    既不是生产缺陷也不是夹具缺陷本身，而是夹具构造有误；更正后该方向**已通过**。
- **准备许可失败的生产者阻断**：缺少 preparation barrier 的 test-support 构造器
  （既有同层构造器为 `pub(crate)`），属缺构造器而非缺证据。
- **两处"输出只被传递、从未被断言"的判定已补证据**（`1dd4301f`），并更正我此前"函数私有、
  无断言测试"的说法——其中"私有"对两处均不成立：
  - `provider_failure_continuation_eligibility`（`provider_v1_bridge.rs`）本就是 `pub(crate)`，
    它决定"失败的供应商尝试能否续跑"，但此前**没有任何直接测试**：唯一调用点把它作为参数传进
    终态事实，**从未断言返回值**。已由
    `provider_continuation_eligibility_requires_a_replay_safe_attempt` 固定（威胁点优先：语义输出/
    副作用三种组合、未知 `dispatch_outcome` 一律禁止；再断言安全方向与重试耗尽）。
  - `is_runtime_host_event_message_id`（`provider_transport_vulcan/request.rs`）决定"未覆盖的宿主
    事件是否阻止延续复用"，此前无断言测试。已由
    `uncovered_host_event_blocks_continuation_reuse` 固定，且该守卫原先把前缀写成字面量
    `"runtime-host:"`，本轮按 AGENTS.md 第十一条改为引用生产者的
    `RUNTIME_HOST_MESSAGE_PREFIX`，使守卫与生产者由同一常量相连。
  - **变异验证**：分别去掉连续判定里的 `dispatch_outcome != Unknown`、把宿主前缀判定改为
    `ends_with`，两处新测试各自 FAILED 并点出对应原因；生产改动均已恢复。
  - **正向路径已补齐（本轮更正）**：我上一轮曾判定"证明正向路径需要复现 ContextInjection 持有的
    改写指纹快照，本层无法构造"并据此删掉了正向断言。该判断**是错的**：
    `ControllerConfig::provider_rewrite_fingerprint()` 是公开方法，仓库既有测试已在用它构造合法延续。
    已补回对照断言（`b2650597`）：同一夹具去掉宿主事件、并把
    `continuation.context_rewrite_fingerprint` 设为该公开指纹后，构建器必须返回 `Some`。
    **变异验证**：在构建器末尾插入 `return None;` 后该断言 FAILED，证明它是有效断言而非恒真。
    因此"宿主事件复用守卫的正向路径未断言"这一旧登记**已失效**。
- **A15 的"写入中断可恢复边界"（此前登记为未覆盖——本轮更正：实为已有覆盖）**：我此前只按
  "归档/删除生命周期"检索，漏掉了 `question_finish_fixture` 的**脚本化刷写故障注入**
  （`crates/vulcan-service-kernel/src/tests/question_execution_finish_tests.rs`，经
  `register_test_session_v1_document_with_scripted_flush_failures`）。两处既有测试已经断言了
  A15 所要求的三件事：
  - `question_finished_storage_failure_recovers_committed_outcome`：让执行完成刷写与随后意图刷写
    （序号 2、3）失败后，断言处理返回 false、**终态持久化所有者被保留**、
    `retry_pre_intent_terminal_settlement` 返回 `SettledByRetry`，且终态记录仍为 `None`
    （边界可恢复而非伪造成功）、完成事件恰好一条、`memory_revision == durable_revision`；
  - `question_finished_persists_queued_successor_and_recovers_failed_publication`：对后继发布失败
    （序号 4、5）注入故障，断言未结算且所有权保留，并在同一测试内与未注入故障的一轮对照。
  因此 A15 的"仅支持当前新契约 + 未知版本明确拒绝 + 写入中断有可恢复边界"三部分**均已有本仓库内
  可复跑证据**。此前把它列为缺口是我检索范围过窄所致，据此更正。
- **线编码器层的相邻前缀稳定性**：需要 `PublicAiRuntime` 与冻结路由/凭据/客户端。
- **真实 WebUI/RPC 与真实停止事务**；**真实冷注册表重启恢复**（编辑准入的冷态分支）。
- **父子交叉互问**：**两个方向均已证明**（子 → 父 `e223082e`；父 → 子
  `parent_question_to_owned_embedded_child_binds_the_question_to_the_child`）。
  我此前记的"未定论"与"可复现拒绝"**均为我的夹具错误**，已撤销，详见 §二.1。
  仅"子代理提问真正触发调度"仍属更高层未构造。
- **通知"目标已接纳但未投影"的恢复会话级闸门（本轮新增，源码级已确认，未修复、未定性质）**：
  这是本任务登记过的最具体的一条待判断发现，值得审核者优先看。行为链如下（均为逐行核对的源码事实）：
  1. `project_collaboration_before_api` 会把 `Accepted` 且未投影的入站通知列为待投影
     （`pending_projections` 第二段只筛 `state == Accepted`，**不要求**它是未接纳），因此该状态
     **确实存在**且是设计上要投影的；
  2. 但启动恢复的**会话级**闸门 `needs_delivery_recovery`（`startup_recovery.rs:20`）不检查
     `inbound_notifications`：它只检查 `task_message_effects` 的 `Claimed`、`outbound_task_messages`
     未接纳、`task_results` 未确认、`inbound_task_messages` 的 `TaskStart` 无效果、以及
     `outbound_deliveries` 的状态/结算确认；
  3. 该闸门在 `startup_recovery.rs:126` 为假时 `continue`，**整会话跳过修复**。
  4. **恢复端确实修通知，但只修来源侧的 `Recorded`，不修目标侧的 `Accepted`**：
     `reconcile_canonical_collaboration_outbox` 内有一段专门重放"来源已提交、目标未接纳"的通知
     （`collaboration.rs:2122-2149`，注释明确 "Notifications live in their own map … the same
     source-commit-before-target-acceptance window strands them unless they are replayed here"），
     但它筛的是**来源** `notifications` 中 state == `Recorded` 的记录；`deliver_recorded_notification`
     只把目标推进到 **`Accepted`**（`AcceptNotification`），**不投影**。
   **因此可确认的窗口**：`accept_notification` 成功、而随后来源侧
   `record_notification_progress(Accepted)` 失败或进程中止时，目标处于
   **`Accepted` 且未投影**，而该状态**不在任何恢复路径的筛选条件内**：
   - 目标侧：会话级闸门不检查 `inbound_notifications`（第 2 点），整会话被跳过；
   - 来源侧：该通知已是 `Accepted`，不满足来源恢复的 `Recorded` 筛选，也不会被重放。
   **推论**：若该目标会话此后不再发生物理 API 准备，这条通知不会被投影。
   **已由测试固定当前行为**：`accepted_but_unprojected_inbound_notification_is_not_a_session_level_recovery_trigger`
   （`startup_recovery.rs` 的 `#[cfg(test)] mod tests`）构造仅含该状态的 `CollaborationState`，
   断言 `needs_delivery_recovery` 为**假**。**变异验证**：在闸门中加入
   `inbound_notifications ... == Accepted` 后该测试 FAILED（失败信息点明"若此行为改变，请更新 S5 §三"），
   证明它确实锁定该行为；生产改动已回退（`git diff` 无删除行）。
   **本轮复查确认这不是夹具问题**（前一轮我在 A07 上正是栽在夹具误判，故此项重新核对）：
   该观察**不依赖任何夹具**——`pending_projections` 与 `needs_delivery_recovery` 的谓词是逐行读源码
   读出的，并有一条只针对纯谓词的断言；用 `Select-String "notification"` 核对该函数区间**零命中**，
   确认它确实完全不检查通知。
   **同一状态在另一处判定下的相反结果（本轮补全）**：`CollaborationState::requires_edit_or_delete_settlement`
   （`crates/vulcan-session-core/src/v1/collaboration.rs:1121`）**确实**把
   `inbound_notifications` 中 `Accepted` 的项算作"仍欠对端的事实"，因此**会阻止编辑/删除**
   （其唯一消费者是 `session_v1_lifecycle/guards.rs:155`；冷态成员走同一谓词）。
   也就是说：**同一个状态会阻止删除，却不触发启动修复**。
   **但我没有把这一点判为规则十一缺陷（本轮实测）**：两者是**不同语义**的谓词——前者是"替换或删除
   历史是否会丢弃欠对端的事实"（单一定义，注释明确写明"canonical document delegates to it"），
   后者是"是否需要加载规范文档以修复"。因此这不构成同一个边界值的重复定义；只是两者的覆盖集合
   在此处产生差异，属需要领域判断的点，而非已确认缺陷。
   **我没有断言的是**：(a) 该通知被永久丢失——下一次物理 API 准备仍会经
   `project_collaboration_before_api` 投影它；(b) **这是缺陷**——通知按设计是在目标**下一次 API 准备**
   时才投影（`PendingProjection::Notification` 无其他消费者），所以"目标不再运行则不投影"可能与设计一致，
   而不是漏修。区分 (a)/(b) 需要领域判断，或需要一条"恢复期也投影目标侧通知"的新路径（那会改变
   启动恢复的行为面，超出本任务不扩大改动范围的约束）。
   **为什么本轮不修**：会话级闸门与恢复期投影都属启动恢复的行为面；`25ecf35d` 修的是同一功能的
   **相邻**窗口（来源侧 `Recorded`），直接扩大闸门或新增恢复期投影有改变启动恢复成本与行为特征的风险。
   据实登记为**待领域判断的具体点**，不弱化任何守卫、不改生产代码。
- **通知恢复的其余窗口**（`25ecf35d` 修复 `Recorded` 未接纳窗口；`72deb8c0` 补齐"目标停止后拒绝
  且记录完整保留、恢复运行后成功投递"这一跨越停止的时序；`599e630c` 补齐"跨项目授权撤销后拒绝、
  重新授权后投递同一条保留记录"这一授权时序）：目标接纳后来源进度写入失败、
  `Failed` 状态通知的恢复语义；以及真实进程重启与冷注册表重建下的通知恢复一跳。
  另：真实 WebUI/RPC 的授权撤销往返未经 HTTP/RPC 边界验证。
- **VMM Profile 生产者链**（本任务范围外，未触碰）。
- `native_compaction_uses_host_history_and_preserves_execution_identity` 在默认线程栈下溢出
  （既有问题，`RUST_MIN_STACK=67108864` 可通过）；该溢出会中断 `--lib` 全量运行，
  因此 `vulcan-agent-runtime` 的全量计数需带该变量获取。

## 四、复跑基线（各口径最近一次实测结果）

| 命令 | 结果 |
| --- | --- |
| `cargo test -p vulcan-session-core --lib -- --test-threads=1` | 460 passed / 0 failed |
| `cargo test -p vulcan-service-kernel --lib "agent_collaboration::" -- --test-threads=1` | 122 passed / 0 failed |
| `cargo test -p vulcan-service-kernel --lib "question_finished_" -- --test-threads=1` | 5 passed / 0 failed |
| `cargo test -p vulcan-service-kernel --lib`（全量，`RUST_MIN_STACK=67108864`） | 1534 passed / **7 failed** / 15 ignored，见下方说明 |
| `cargo test -p vulcan-agent-runtime --lib "provider_transport_vulcan::" -- --test-threads=1` | 51 passed / 0 failed |
| `cargo test -p vulcan-agent-runtime --lib "standalone_compaction_tests"` | 3 passed / 0 failed |
| `cargo test -p vulcan-provider-adapters --lib "responses_compact"` | 9 passed / 0 failed |
| `cargo test -p vulcan-agent-runtime --lib`（`RUST_MIN_STACK=67108864`） | 1027 passed / **0 failed** / 4 ignored（既有失败已于 `2bf00026` 修正） |
| `cargo test -p vulcan-context-injection --lib` | 20 passed / 0 failed |
| `npx vitest run src/service/conversation-event-window-store.test.ts` | 49 passed / 0 failed |
| `npx vitest run src/service/conversation-event-window-controller.test.ts` | 39 passed / 0 failed |
| `npx vitest run src/components/conversation/conversation-collaboration-card.test.tsx` | 8 passed / 0 failed |
| `npx tsc -b`（`apps/vulcan-code-web`） | 0 错误 |
| `npx vitest run`（`apps/vulcan-code-web` 全量） | 1842 passed / **4 failed**（§二.5）/ 1 skipped；本轮全量运行时另见 1 次环境抖动，单独复跑通过 |

**`vulcan-service-kernel --lib` 全量的 7 项失败（本轮首次实测登记，不属本任务所有权）**：
`conversation_submission::submission_model_selection_tests::{new_session_rebinds_invalid_global_account_for_same_model,
repeated_submission_selection_preserves_lease_until_target_changes}`、
`native_vmm::driver::lexical_driver_tests::{real_controller_compaction_dispatches_builtin_vmm_hook,
real_controller_nonempty_analysis_materializes_memory_and_profiles}`、
`tests::conversation_submission_queue_tests::queue_service_idempotent_retry_does_not_duplicate_process_mirror_or_hook`、
`tests::detached_subagent_tests::context_compaction_without_v1_durability_cannot_write_legacy_checkpoint`、
`vce::manager::jobs::tests::in_process_compaction_recovers_without_duplicate_submission`。

**归属判定（按文件而非按猜测）**：这 7 项分别落在 `conversation_submission`（供应商/模型选择）、
`native_vmm`、`vce::manager` 等区域，属另一并行会话正在处理的供应商与模型工作。我已逐提交列出
本任务全部提交实际触碰的文件（51 个），**与这 7 项失败的源文件无交集**；本任务对
`vulcan-service-kernel` 的改动集中在 `agent_collaboration/**`、`agent_stream/stream_task/question_launch/tests.rs`、
`session_v1_lifecycle/**` 与 `conversation_generations_v1.rs`。因此这 7 项**登记不修复**。

## 五、引用完整性闸门（建议原主代理审核前复跑）

本文件的证据价值取决于**它引用的每个名字在当前源码里仍然存在**。为此本轮对本文件做了一次
自动化引用核验：

- 方法：提取本文件全部反引号内的 snake_case 标识符（共 65 个），逐一在 `crates/`、`apps/` 中
  查找同名项。
- 结果：**65 个标识符全部存在，无死引用**（其中 6 个为非测试标识符：`provider_request_bridge_messages`、
  `provider_request_bridge`、`open_child_task_ids`、`wakes_retained_parent_turn`、
  `children_by_session_id`、`child_task_non_success_projected`）。
- 同一闸门已在状态入口 `IMPLEMENTATION_STATUS.md` 上复跑：其 33 个引用全部存在；该文件原有
  2 个失效引用已修正（测试重命名所致，实现与断言仍在），并以"（已失效名）"前缀标出、按闸门口径
  不计入检查。
- 边界：该闸门只证明**引用可达**，不证明被测行为仍然正确，也不证明断言强度。行为正确性仍需按
  §四 复跑。
- 后续维护：任何一次重命名或删除测试后，必须重新执行本闸门；本文件新增证据行时同样适用。
  明确标注为"已失效名"的历史对照物不使用反引号，因此不计入闸门。

## 六、交付清单（供原主代理五轮审核）

本节按交接方案 §六 的模板汇总交付物。**实施侧不宣布任何一轮审核通过；五轮仍为 0/5。**

### 6.1 提交清单（按工作包）

| 工作包 | 提交 | 内容 |
| --- | --- | --- |
| S1.1 | `680a54cd`、`6cf4a777` | 历史编辑清除未确认目标回执的修复与编辑准入加锁 |
| S1.2 | `6cf4a777` | 归档后真实模型回答的服务链证据 |
| S1.3 | `3ce58418` | 停止、MainOnly、仅答疑、迟到消息四项服务级证据 |
| S2.1 | `8fd2dbad` | RuntimeOnly 压缩屏障消费与明确所有权 |
| S2.2 | `09d5e25a`、`c3707e3d`、`1dd4301f` | 原生压缩诊断制品接线；两处"输出只被传递、从未被断言"的判定补断言；宿主前缀收敛为共享常量 |
| S2.3 | `c3707e3d`、`f3b325f4`、`b619280a` | OAuth 刷新后新尝试等三段证据；**打通并守护"适配器标记 → 宿主重试判定"的中间连接** |
| S3.1 | `42651b34`、`523fa6cd` | 临时注入作用域合同矛盾修复 |
| S3.2 | `bca79df4` | 逐生产者可达性与固定顶部边界证据 |
| S3.3 | `fc65b351`、`fa1daa28`、`1dd4301f` | 逆序插入修复、判定与诊断一致性修复、两处判定补断言 |
| S4.1 | `61718ca4`、`9f8a243c`、`550a5c75`、`f6a497c6`、`e223082e`、`9aaeff1e` | 超时/重等/撤销、游标语义修复、答案先到、迟回答、**双向提问闭环**、测试辅助对齐领域规则 |
| S4.2 | `6a65e457` | 六种授权动作逐项与撤销证据 |
| S4.3 | `298a29bc`、`5c848967`、`822bfddc` | 协作卡片 8 项行为证据；多窗口唯一消息；同 seq 跨实体不冒充 |
| S4.4 | `4a52755b`、`e71b0b30`、`f586e9e3` | G4 无法回答契约；A15 两处未知版本拒绝 |
| S5.1/S5.2 | `45832d32`、`4b02e817`、`f477fb18`、`2bf00026`、`193bab6f`、`245f5c16`、`c2366c63` 等 | 需求行级对照、字段与路由汇总、引用完整性闸门、既有失败修正与状态更正 |

### 6.2 本轮交付物（原主代理直接审核这三份）

1. 本文件：P00—P12 / R01—R14 / A01—A18 逐行对照、缺口、未验证边界、复跑基线、引用闸门、交付清单。
2. `S5_FIELD_AND_ROUTE_SUMMARY.md`：字段唯一来源、工具 Schema、实际路由、用户使用说明。
3. `IMPLEMENTATION_STATUS.md`：工作包级状态与决策记录，含引用完整性节。

### 6.3 决策理由的记录位置

每条提交信息内均为"源码事实 → 决策 → 理由 → 影响范围"四段式；本节 6.1 只列末级提交，
过程性判断（含被推翻的判断）在对应提交信息与 §三 中逐条保留。

### 6.4 已知失败与剩余项（不得省略）

- **既有失败（非本任务引入，仅登记，不修复）**：
  - 前端 4 项（`goal-strip`、`vulcan-plugins-panel`、`conversation-v1-tool-card`、
    `conversation-tool-presentation-card`）；
  - `vulcan-service-kernel --lib` 全量 **7 项**（供应商/模型选择 2 项、`native_vmm` 2 项、
    会话队列 1 项、分离子代理 1 项、`vce::manager` 1 项；逐项名称与归属判定见 §四）；
  - 栈溢出类 2 处：`vulcan-agent-runtime` 的
    `native_compaction_uses_host_history_and_preserves_execution_identity` 与
    `vulcan-service-kernel` 的
    `capability_metadata_reopen_and_rebuild_rpc_preserves_conversation`，均可用
    `RUST_MIN_STACK=67108864` 通过；
  - **A16 的用量分离失败已修正**（`2bf00026`），**不再**属于既有失败。
- **验收缺口（"证据不足"/未定性，见 §二）**：
  - R10/A10 的**模型级自动唤醒调度一跳**——需要真实 `AgentRuntimeBridge` 与真实调度环境，本层无法构造；
  - **父子交叉互问**——**已双向验证**（本轮更正，我此前的错误登记已撤销）：子 → 父由 `e223082e`、
    父 → 子由 `parent_question_to_owned_embedded_child_binds_the_question_to_the_child` 覆盖，详见 §二.1；
  - **通知"已接纳但未投影"的会话级闸门**——已定位到完整窗口（详见 §三），并用测试固定当前行为
    （`50ea0dbe`）；**它不是夹具问题**（本轮已按 A07 的教训重新核对：结论来自逐行读谓词 +
    一条只针对纯谓词的断言，且已用检索确认闸门完全不检查通知）。仍未定性为"缺陷"还是"符合设计的
    延迟投影"——通知按设计在目标下一次 API 准备时才投影；
  - 前端既有 4 项失败不属本任务所有权（§二.5）。
- **未验证边界（见 §三）**：真实 OAuth 端到端一跳（结构性不可达，见 §三 说明）、真实 WebUI/RPC 与真实
  停止事务、真实冷注册表重启、线编码器相邻前缀稳定性、V1 端到端 `SystemContext → ProjectedItem →
  provider_v1_sink`、VMM Profile 生产者链（范围外）、通知恢复其余窗口。
- **本会话内被推翻的自身判断（据实保留，供审核追溯）**：共**七处**——A15 两处"未覆盖"、A16 用量分离
  "不属本任务所有权"、G4"需领域决策"、宿主事件复用"正向路径不可证"、A16"完整分页重连组合未断言"、
  R10"工具回复后子代理收到答案未证明"；另有 `wait_session` 疑点经实测排除、父子交叉互问的前一版结论
  被推翻。**共同原因是同一个方法缺陷：按单一范围检索后即下结论，未换文件、未查公开入口、未读完整测试。**
  建议审核者对本文件其余"证据不足"主张先假定可能已有覆盖，再复核一遍。

### 6.5 是否改动模型可见契约、持久字段、授权、线程/锁顺序

按交接方案 §六 第 4 项逐类回答（依据为本案全部提交的实际 diff）：

| 类别 | 是否改动 | 说明 |
| --- | --- | --- |
| 模型可见契约（工具名/Schema/参数/描述） | **否** | 11 个协作工具的注册与 Schema 未变；`reply_question` 的 `unable_to_answer` 本就存在于 Schema，本案只补断言（`4a52755b`），未改契约。 |
| 持久字段 | **否** | 未新增/删除/改名任何持久字段；未做历史数据兼容（按要求不支持历史数据）。 |
| 授权 | **否** | 未改授权模型；仅补逐动作与撤销证据（`6a65e457`）与跨项目撤销时序证据（`599e630c`）。 |
| 线程/锁顺序 | **否**（本案内） | 未改锁顺序；编辑准入的加锁范围由第 1 轮审核问题 P1-2 的修复（`ca83e0c2`）确定，本案未再改动。 |
| 生产行为是否变化 | **是，三处已记录的修复性变化** | (1) `9f8a243c` 问题等待游标改用协作状态内容；(2) `1dd4301f` 宿主事件前缀改为引用共享常量 `RUNTIME_HOST_MESSAGE_PREFIX`（AGENTS.md 第十一条）；(3) `25ecf35d` 抽出 `deliver_recorded_notification` 修复通知恢复窗口。三处均在各自提交信息中记录，且有独立证据。 |

### 6.6 原主代理审核结论、问题编号及修复提交

**本案不填写本项**，原因是它按定义只能由原主代理给出：
第 1 轮审核（范围 `280f31a2..470386c7`）的 3 项逻辑问题与 2 类验收缺口已在其后修复（见
`IMPLEMENTATION_STATUS.md` 对应小节）。第 2—5 轮**尚未开始**，完整审核仍为 **0/5**。
实施侧**不得**自行宣布任何一轮通过或归档正式计划。

## 七、本文件的边界

- 本文件只做**对照与登记**，不新增实现，也不改变任何状态判定的严格程度。
- "已实现并已验证"仅表示存在可复跑证据，**不等于**原主代理的五轮完整审核通过。
- 完整审核仍为 **0/5**；之前的百分比口径未重新统计，一律弃用。
