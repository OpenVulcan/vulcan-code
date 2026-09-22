# S5 验收证据对照（原方案 P00—P12 / R01—R14 / A01—A18）

本文件是 S5.2 的交付物：把原正式方案 `docs/plan/20260920-04-AGENT_COMMUNICATION_AND_DYNAMIC_CONTEXT.md`
的全部需求行，逐项对应到**当前源码事实**与**已提交的验收证据**。

- 本文件**不替代**原方案，也不缩减任何要求。
- **完整审核仍为 0/5**：五轮连续完整独自审核属于原主代理，本文件不声明任何一轮通过。
- 状态口径：`已实现并已验证` 表示有本仓库内可复跑的行为证据；`已实现，证据不足` 表示源码事实清楚
  但缺可执行验收；`未验证边界` 表示需要本层无法构造的环境才能验证。**不使用未经复跑的百分比。**

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
| A07 | 双向提问、答案先到、多等待者、超时再等待不丢答复、不互等 | 超时/重等/撤销三项（`61718ca4`）+ 游标修复（`9f8a243c`）+ 答案先到（`550a5c75`） | **已实现，证据不足**（多等待者、双向互问未覆盖） |
| A08 | 主模型先 final 而子仍运行时各层保持未完成 | 完成门 `open_child_task_ids`；`main_only_parent_...` 不解冻 | 已实现并已验证 |
| A09 | 未回复不阻止完成；仅主动等待才等待；晚到答案不复活旧任务；无法回答可明确表达 | `unanswered_question_and_notification_do_not_block_completion_and_late_question_does_not_revive`、`late_answer_after_settlement_is_rejected_without_reviving_target_task` | **已实现，证据不足**（`unable_to_answer` 只有回显无持久化效果，见 §二.2） |
| A10 | 自然空闲仅答疑不二次完成；MainOnly 可恢复；停止拒收；全树停止后迟到不复活 | `question_finished_...`、`question_launch_admits_main_only_...`、`stopped_target_wakeup_...` | **已实现，证据不足**（自动唤醒调度一跳，见 §二.1） |
| A11 | 重复与并发按稳定标识结算；回执丢失后恢复原记录；同键不同正文明确冲突 | 操作身份幂等与 `RequestIdentityMismatch` 证据；S2.3 服务端 11 项 | 已实现并已验证 |
| A12 | 各类崩溃窗口无静默丢失与虚假成功 | 归档/删除/冷加载证据；`remote_compaction_confirmation_failure_blocks_the_producer_without_fabricating_success`；**通知"来源已提交、目标未接纳"窗口的修复**（`25ecf35d`：修复计数 0→1、身份逐字不变、二次修复为 0） | 已实现并已验证 |
| A13 | 压缩与尾段加载后仍能确定问题双方/状态/授权/未结事项；失效提示不被恢复成当前指令 | S3.1 作用域修正（`42651b34`、`523fa6cd`）；压缩前后协作证据 | 已实现并已验证 |
| A14 | 标签伪造、错误收件人、跨树、未授权跨项目、撤销后发送均不能越权 | `cross_project_*` 三项 + `document_grant_allows` 逐动作（`6a65e457`） | 已实现并已验证 |
| A15 | 仅支持当前新契约；未知版本明确拒绝；写入中断有可恢复边界 | 严格版本读取与拒绝路径 | **已实现，证据不足**（两处未知版本拒绝无测试，见 §三） |
| A16 | 多窗口/分页重连按 ID/游标恢复；结果/答案/回执不互相冒充；消耗只计一次 | 前端窗口注册表 17 项 + 分页 39 项；`terminal_report_*` 用量分离 | **已实现，证据不足**（见 §二.4：用量分离测试当前失败） |
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
| P07 临时问答与等待 | `61718ca4`、`9f8a243c`、`77b1cbf0`、`550a5c75`、`f6a497c6` | **部分**（R10/A07 缺口同 §二.1） |
| P08 父子工具完整暴露 | `d5c2865e`、`433dead7`、`6a65e457` | 已实现并已验证 |
| P09 独立会话权限 | `6a65e457` | 已实现并已验证 |
| P10 压缩与当前契约恢复 | `8fd2dbad`、`09d5e25a`、`c311ee31`、`f3b325f4` | 已实现并已验证 |
| P11 界面与资料 | 前端 123 项 + 本文件 | **部分**（卡片组件行为无导入测试，见 §二.3） |
| P12 总验收和交付 | 本文件 + 状态入口 | **未完成**（逐项闭合与五轮审核未完成） |

## 二、尚未闭合的验收缺口（都是证据不足，不是已知实现错误）

1. **MainOnly 自动恢复的完整一跳与多等待者/双向互问**（R10、A07、A10）。
   已证明：真实启动闸门准入并声明 `wakes_retained_parent_turn`、两种停止状态拒绝、闸门为纯读取方
   （`3df19776`）；提问方业务完成后首次答案到达仍正确结算且不复活旧业务（`f6a497c6`）；
   答案先到时立即观察到而不等满超时（`550a5c75`）；超时/重等/撤销与游标语义（`61718ca4`、`9f8a243c`）。
   **未证明**：子代理提问真正触发调度（需要知识访问解析、WebUI 准备代次所有者、Agent 运行时桥
   与宿主句柄，本层无法构造）；工具回复后子代理收到答案；多等待者；双向互问。
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

3. **协作卡片的组件级行为**（R14、P11）——**已于 `298a29bc` 闭合**。
   新增 `apps/vulcan-code-web/src/components/conversation/conversation-collaboration-card.test.tsx`
   直接挂载生产组件并触发其回调，覆盖卡片身份路由、通知不提供回复控件、会话导航按可路由集合门控、
   撤销控件的三个可用条件、连续点击的单次提交、失败后可重试，以及投递更新后保留宿主原始时间戳。
   该文件 8 项通过，`tsc -b` 无错误。
   仍**未覆盖**：`task-message` 身份分支与 `child_task_non_success_projected` 的正文抑制分支
   （既有显示渲染器测试覆盖 6 种 task-message 事件，本包未重复）；真实浏览器路由跳转与真实宿主撤销往返。

4. **`terminal_report_separates_subagent_usage_from_parent_totals` 当前失败**（A16）。
   该失败**与本任务无关，也不属本任务所有权**（涉及独立用量统计，任务约束明确不改动）。

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

   **建议处置（交由拥有用量统计的会话）**：该测试应改为断言"子代理分段不进入父回合分组"
   或断言子代理用量出现在根生命周期累计中，而不是断言父回合存在 `Subagent` 分组。
   本任务只登记，未修改该文件。

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
- **准备许可失败的生产者阻断**：缺少 preparation barrier 的 test-support 构造器
  （既有同层构造器为 `pub(crate)`），属缺构造器而非缺证据。
- **continuation 判定与宿主事件保护**：相关函数为私有，无断言测试。
- **两处未知契约版本拒绝**（provider 路由租约的 `schema_version`、工具结果媒体信封版本）。
- **线编码器层的相邻前缀稳定性**：需要 `PublicAiRuntime` 与冻结路由/凭据/客户端。
- **真实 WebUI/RPC 与真实停止事务**；**真实冷注册表重启恢复**（编辑准入的冷态分支）。
- **多等待者与双向互问**。
- **通知恢复的其余窗口**（`25ecf35d` 只修复了 `Recorded` 未接纳这一窗口）：目标已接纳但未投影、
  目标接纳后来源进度写入失败、授权撤销时的恢复、`Failed` 状态通知的恢复语义；
  以及真实进程重启与冷注册表重建下的通知恢复一跳。
- **VMM Profile 生产者链**（本任务范围外，未触碰）。
- `native_compaction_uses_host_history_and_preserves_execution_identity` 在默认线程栈下溢出
  （既有问题，`RUST_MIN_STACK=67108864` 可通过）；该溢出会中断 `--lib` 全量运行，
  因此 `vulcan-agent-runtime` 的全量计数需带该变量获取。

## 四、复跑基线（本轮实际执行）

| 命令 | 结果 |
| --- | --- |
| `cargo test -p vulcan-session-core --lib -- --test-threads=1` | 460 passed / 0 failed |
| `cargo test -p vulcan-service-kernel --lib "agent_collaboration::" -- --test-threads=1` | 114 passed / 0 failed |
| `cargo test -p vulcan-agent-runtime --lib "provider_transport_vulcan"` | 49 passed / 0 failed |
| `cargo test -p vulcan-agent-runtime --lib "standalone_compaction_tests"` | 3 passed / 0 failed |
| `cargo test -p vulcan-provider-adapters --lib "responses_compact"` | 9 passed / 0 failed |
| `cargo test -p vulcan-agent-runtime --lib`（`RUST_MIN_STACK=67108864`） | 1021 passed / **1 failed**（§二.4）/ 4 ignored |
| `cargo test -p vulcan-context-injection --lib` | 20 passed / 0 failed |
| `npx vitest run src/components/conversation/conversation-collaboration-card.test.tsx` | 8 passed / 0 failed |
| `npx tsc -b`（`apps/vulcan-code-web`） | 0 错误 |
| `npx vitest run`（`apps/vulcan-code-web` 全量） | 1842 passed / **4 failed**（§二.5）/ 1 skipped |

## 五、本文件的边界

- 本文件只做**对照与登记**，不新增实现，也不改变任何状态判定的严格程度。
- "已实现并已验证"仅表示存在可复跑证据，**不等于**原主代理的五轮完整审核通过。
- 完整审核仍为 **0/5**；之前的百分比口径未重新统计，一律弃用。
