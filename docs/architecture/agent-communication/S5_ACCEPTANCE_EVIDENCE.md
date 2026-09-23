# 代理通信验收证据对照

更新日期：2026-09-23。范围：原正式方案 P00—P12、R01—R14、A01—A18。
本文件由原主代理在交接实施之后重新核验；不再沿用“多等待者不适用”“无法构造自动答疑”“真实 OAuth 本层不可达”的错误豁免。
历史调查与被推翻的判断可通过本文件在 `ae055eb` 及更早提交中的版本追溯；当前判断只见本文件，不混列过期结论。

状态：全部实现、组合证据、最终复验与归档完成；最后一次修复后连续五轮完整审核无新问题，累计 5/5。
“已验证”指仓库内可重复验证，真实付费供应商、真实用户账号与人工视觉验收另见第三节。

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
| R10 子到主为独立方向；`MainOnly` 允许子代理问题自动恢复主会话 | 准入表 `(MainOnly, InTurnQuestion)` → `wakes_retained_parent_turn = true`；真实闸门 `question_launch_restart_state` | `question_launch_admits_main_only_resume_and_refuses_stop_states_unchanged` | 已实现并已验证（真实自动答疑，见第二节） |
| R11 同项目可查找/读取/提问/引导；跨项目需明确授权 | `list_authorized_sessions` 项目范围；`document_grant_allows` 逐动作 | `native_dispatch_list_and_read_session_project_bounded_ranges`、`cross_project_*` 三项授权证据 | 已实现并已验证 |
| R12 临时记忆到期保留已发送正文并追加失效标记；压缩排除失效内容 | `state.rs` 的 `TRANSIENT_TURN_SCOPE` 生产者侧声明；失效事件不重写历史 | `ttl_expiry_appends_tagged_user_item_without_rewriting_emitted_prefix` | 已实现并已验证 |
| R13 单向通知不要求回复且独立于任务/方向/问题；下一合法边界自然插队 | `notifications.rs` 的发送与目标接纳；`projection.rs` 的通知投影 | `native_dispatch_send_notification_records_without_reply_contract`、`unanswered_question_and_notification_do_not_block_completion_and_late_question_does_not_revive`（通知不阻塞完成） | 已实现并已验证 |
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
| A07 | 双向提问、答案先到、多等待者、超时再等待不丢答复、不互等 | 超时/重等/撤销三项（`61718ca4`）+ 游标修复（`9f8a243c`）+ 答案先到（`550a5c75`）+ **双向提问闭环**（`e223082e`：子问父→父作答→子经真实等待入口观察到答案，且问题记录权威留在提问方）+ **父 → 子方向**（`parent_question_to_owned_embedded_child_binds_the_question_to_the_child`） | 已实现并已验证（并发等待与自动唤醒均已补齐，见第二节） |
| A08 | 主模型先 final 而子仍运行时各层保持未完成 | 完成门 `open_child_task_ids`；`main_only_parent_...` 不解冻 | 已实现并已验证 |
| A09 | 未回复不阻止完成；仅主动等待才等待；晚到答案不复活旧任务；无法回答可明确表达 | `unanswered_question_and_notification_do_not_block_completion_and_late_question_does_not_revive`、`late_answer_after_settlement_is_rejected_without_reviving_target_task`、**无法回答声明契约**（`explicit_unable_to_answer_is_echoed_and_travels_in_the_durable_body`，`4a52755b`） | 已实现并已验证（回答正文记录明确的无法回答说明） |
| A10 | 自然空闲仅答疑不二次完成；MainOnly 可恢复；停止拒收；全树停止后迟到不复活 | `question_finished_...`、`question_launch_admits_main_only_...`、`stopped_target_wakeup_...` | 已实现并已验证（真实自动答疑，见第二节） |
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
| P07 临时问答与等待 | `61718ca4`、`9f8a243c`、`77b1cbf0`、`550a5c75`、`f6a497c6`、`e223082e`、`a8c3e753` | 已实现并已验证（双向提问、真实自动唤醒及多等待者） |
| P08 父子工具完整暴露 | `d5c2865e`、`433dead7`、`6a65e457` | 已实现并已验证 |
| P09 独立会话权限 | `6a65e457` | 已实现并已验证 |
| P10 压缩与当前契约恢复 | `8fd2dbad`、`09d5e25a`、`c311ee31`、`f3b325f4` | 已实现并已验证 |
| P11 界面与资料 | 前端 123 项 + `conversation-collaboration-card.test.tsx`（8 项）+ 本文件 | 已实现并已验证（卡片组件行为已由 `298a29bc` 闭合并直接挂载生产组件；真实浏览器路由跳转与真实宿主撤销往返仍属未验证边界，见第三节） |
| P12 总验收和交付 | 本文件 + 状态入口 | 已完成（完整审核 5/5；正式方案与收尾记录已归档） |

## 二、本次补齐的组合证据与实际修复

| 缺口 | 生产链与断言 | 提交 |
| --- | --- | --- |
| 多等待者 | `concurrent_waiters_survive_one_cancel_and_observe_the_same_answer`：同一个来源 API 的三个真实工具调用并发等待，取消一个不取消问题；其他等待者及后续观察者收到同一答案 | `9e3f9ab7` |
| 原生压缩真实 OAuth | `native_child_maintenance_refreshes_oauth_and_retries_with_a_new_durable_api`：真实 TLS 401、OAuth 表单刷新、凭据修订替换、第二次物理请求、不同 API 身份、失败先结算、冷加载成功检查点 | `18e30287` |
| 独立运行时真实压缩 | `standalone_native_compaction_drives_real_http_and_confirmation`、`standalone_native_compaction_drives_real_http_refresh_and_confirmation`：调用真实生产者和独立消费者；不写入原会话 V1 压缩事实，请求与终态计数一致 | `e9ce1103`、`c6d169d7` |
| MainOnly 自动答疑 | `main_only_child_question_wakes_real_model_replies_and_preserves_paused_goal`：真实子问题端口、服务调度、TLS 模型流、回复工具和再次模型 final；子观察答案、父原回合保留、暂停 Goal 不变、只答疑执行结算 | `e9e0a4f` |
| 网络失败与取消落盘 | `failure_and_cancellation_projections_finish_real_domain_api`：12 类错误 × 3 传输状态 × 3 分派状态 × 2 重试结束选择，加宿主取消和无结构错误，共 218 组经过真实领域准入及结束 | `ae055eb` |
| 全部生产 Profile | 当前清单实际声明九 Profile，使用生产执行客户端编码验证；相邻请求与 TTL 生产者另有装配测试，明确两层证据的边界 | 见字段路由汇总第八节 |

实际发现和处理：

1. RuntimeOnly 远端生产者仍构造 V1 遥测，独立消费者在物理请求前拒绝：按显式所有权构造已有遥测上下文。
2. 独立消费者拒绝真实 Retrying 提示：只允许已准入且已终结的原尝试被重试提示引用；保留所有强制屏障校验。
3. 重试用途在排除非会话归属之前生成，导致重试误入 V1：先判断基础所有权，再派生用途。
4. 实际答疑网络失败被严格领域拒绝：结束原因和错误类别共用同一传输分类；取消遵循无错误及禁止继续的控制终态。未改变供应商配置、领域 Schema 或重试策略。

MainOnly 夹具的两次修正是测试构造问题：Chat User 实际编码为文本数组；服务启动必须启用投影屏障以取得工具的持久身份。未放松生产守卫。代理消息虽然走 User 角色，其来源是 AgentMessage，因此保留人类输入的断言按来源区分。

## 三、外部边界与已核实的设计选择

- 通知在接收方下一合法请求边界投影。自然空闲收到通知不启动新任务；已接纳正文可靠保留，不要求答复。来源的 Accepted 进度会进入恢复扫描，全停与撤权会形成明确失败及来源确认；不能把“目标尚未调用模型”误判为丢失。
- 真实供应商缓存命中率、真实账号 OAuth 刷新、付费调用与所有媒体上传接口未经本任务验证。当前证据包括生产协议编码、本地真实 TLS、真实 OAuth 组件和可靠存储，不将它们等同外网验收。
- 当前内置 Codex 代码模式模型的 JavaScript 宿主原语限制未被绕过。测试中的原生直接工具模型不证明内置代码模式可用。
- 界面卡片、窗口合并、分页和重连有组件及状态机验证；用户实际窗口的视觉体验、浏览器导航和真实宿主撤销往返仍可进行产品验收。
- 其他任务的供应商配置、VCE、VMM 画像业务与说明文档优化未修改。旧报告中的全量失败只是旧版本结果，不能视为当前失败；本次仅报告实际执行的定向结果。
- 当前版本不做历史格式迁移，不增加协议探测或自动降级。

## 四、本次验证记录

验证命令均在前面添加 `rtk`。Rust 共享测试使用 `-- --test-threads=1`，保留仓库编译并发限制。

| 验证 | 最新结果 |
| --- | --- |
| 服务实际调试构建 | 最终代码实际构建成功，零错误零警告，43.30 秒 |
| 服务协作组 | 新增组合及终态修复后 160 项通过，零警告、零错误 |
| 真实压缩组合 | 5 项通过；随后 MainOnly 单独 1 项通过 |
| 独立压缩消费者组 | 6 项通过 |
| 物理请求 V1 桥接组 | 13 项通过，包含 218 个真实领域终态组合 |
| 协作卡片、事件窗口存储和控制器 | 3 文件共 97 项通过 |
| 前端类型检查 | 通过，零错误 |
| 请求遥测生命周期 | 25 项通过，涵盖取消后恰好一次结束 |
| 相邻请求、TTL、动态生产者与 Plan | 四个生产装配用例分别通过 |
| 九 Profile 实际客户端编码 | 1 个矩阵用例通过，覆盖九种声明协议 |

历史广范围验证记录保留在 [前次提交审核报告](../../completed/20260923/02-AGENT_COMMUNICATION_COMMIT_REVIEW.md)，不冒充本次重新执行。

## 五、交付与审核

当前字段来源、Schema、工具暴露链、协议及用户使用说明见 [字段与路由汇总](S5_FIELD_AND_ROUTE_SUMMARY.md)。
工作包状态、独立提交及决策理由见 [实施状态](IMPLEMENTATION_STATUS.md)。
每轮完整审核覆盖契约、生命周期、可靠恢复、协议/动态上下文、授权、工具暴露、界面与验证缺口；不同重点不能替代完整覆盖。
发现问题已独立修复并从零计数，最后连续五轮均无新问题。逐轮记录及完整执行变更总结见[最终收尾报告](../../completed/20260923/03-BUILD_REPAIR_AND_AGENT_ACCEPTANCE.md)，累计 5/5。

本次领域协作组 90 项、动态转换组 5 项另行复跑通过。报告中的用例组存在包含关系，不简单相加作为独立总数。
