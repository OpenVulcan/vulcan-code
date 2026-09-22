# 当前实施状态与剩余清单

更新日期：2026-09-22。原目标仍为正式方案全部 P00—P12、R01—R14、A01—A18，以及连续五轮完整审核无新问题。

最新安排：用户要求保存提交点并交接剩余实现，原主代理转为审核。执行入口见[交接方案](../../plan/20260922-01-AGENT_COMMUNICATION_HANDOFF.md)。总目标尚未完成，当前暂停用于交接；不再继续本轮功能修改。

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
- [ ] S1.3 对照 A08—A12、A17 销项停止、MainOnly、仅答疑和迟到消息组合；优先使用已有证据，只补真实缺口。本轮已新增真实服务入口驱动的三项组合证据（停止目标排队任务终态失败且不自动恢复、已停止主会话只接纳所属子问题、未回答问题与通知不阻塞完成且迟到问题不复活任务），待核对整合。

### S2 压缩收尾

- [x] S2.1 非 V1 独立运行时的压缩屏障消费及明确所有权：已修复。RuntimeOnly 启动在派发前显式声明非会话归属，独立消费方成为强类型确认所有者；准备边界走 `into_excluded_non_session()`，可靠性边界校验后恰好确认一次，任何会话投影事实一律 fail-closed。提交 `8fd2dbad`。
- [x] S2.2 原生压缩请求诊断制品接线。已确认改动前压缩路径**零制品**（许可里的追踪归属被丢弃、请求捕获显式传空、传输层无响应捕获），且用户开关在压缩分派链不可达。按"复用既有机制 + 只传逐分派冻结开关"实施：开关关闭时行为与改动前完全一致（不预留、不接触追踪目录），开启时复用同一 `ProviderApiTraceStore` 与同一渲染器（制品结构与脱敏规则未改），请求与响应两条捕获通道都接上。证据：`native_compaction_request_publishes_the_reused_provider_api_trace_artifact`（真实路由 + 回环 HTTP + 双通道捕获，断言制品路由/路径/正文逐值相等、终态分类、请求头凭据已脱敏、制品原始字节不含凭据）、`native_compaction_trace_requires_the_frozen_opt_in_decision`（关闭时不预留且追踪根目录不存在）、`ordinary_trace_reservation_requires_the_frozen_opt_in`、`ordinary_provider_request_trace_shape_is_unchanged`。提交 `09d5e25a`（`responses_compact.rs` 的生产部分因暂存区竞态落在 `c3707e3d`，已在该提交信息中如实说明）。
- [x] S2.3 OAuth 刷新后的新尝试、可靠确认失败阻断、主会话入口的专门验证。**结论：三段生产逻辑改动前已正确接线，缺的是可执行证据**，因此只补测试未改生产逻辑。证据：服务端 `remote_checkpoint` 11 项（含 `remote_compaction_retry_settles_failed_permit_then_confirms_only_the_new_permit`、`remote_compaction_confirmation_failure_blocks_the_producer_without_fabricating_success`、`remote_compaction_permit_refusal_keeps_the_admitted_attempt_unmodified`、`main_session_physical_entry_consumes_the_durable_native_checkpoint`），适配器 9 项（含真实回环 401→200、各自尝试身份、无隐藏重试）。提交 `c3707e3d`、`f3b325f4`。
- **S2.3 未验证边界（精确登记）**：真实 401 → OAuth 刷新 → 第二次真实 HTTP 的端到端一跳未驱动；**准备许可失败的生产者侧阻断无法跨 crate 驱动，缺少 preparation barrier 的 test-support 构造器**（应与 `events.rs` 既有 durability barrier 测试构造器同层，现有同层构造器为 `pub(crate)`），而不是缺少证据；主会话真实 provider 往返回路未驱动。

### S3 协议及动态注入

- [x] S3.1 动态生产者逐项到最终请求的连续前缀、更新、到期及压缩验证。已修正确认缺陷：临时注入宿主块作用域与会话作用域不一致，导致任何真实会话作用域临时注入在 V1 请求准备时直接失败；同一内容项的失效事件却硬编码回合作用域，属合同内部矛盾。修正生产者侧声明（临时块恒为回合作用域，持久块保留声明作用域），并把作用域常量收敛为单一来源。提交 `42651b34`、`523fa6cd`。
- [x] S3.2 逐生产者到最终请求的可达性与固定顶部边界。**纠正了任务书的假设**：`provider_layers` 并**不是**未守护缺口——它由三重事实守护（唯一消费者是闸门前候选、该候选明确拒绝非会话派发、V1 所有者整体重建），因此只做文档化与回归断言，未做结构性改造。新增证据：`request_dynamic_producers_stay_out_of_fixed_top_layers`、`session_plan_context_reaches_request_as_tagged_user_event`、`two_adjacent_requests_keep_fixed_top_and_append_dynamic_user_items`（相邻两次真实请求，断言固定顶部字节不变、更新只追加、被替代旧正文保留）、`ttl_expiry_appends_tagged_user_item_without_rewriting_emitted_prefix`。提交 `bca79df4`。
- [x] S3.3 continuation 的真实安全判定与重建诊断一致性；工具配对、并行结果、媒体交错与插入边界。已修正两个确认缺陷：(1) 同一生效锚点上的多条系统更新**逆序**插入，违反与输入一致的时间顺序契约（提交 `fc65b351`）；(2) dispatch 因边界后存在未覆盖 `runtime-host:` 消息返回 None，而预览诊断缺少该分支并会渲染"允许复用"，即判定与诊断不一致（提交 `fa1daa28`）。另补充工具配对校验、乱序并行结果按身份配对、媒体交错与插入边界的 12 项证据。**未覆盖**：continuation 五项判定与宿主事件保护仍无断言测试（相关函数为私有）；两处未知契约版本拒绝仍无测试。

### S4 协作与界面

- [ ] S4.1 双向等待、先到答案、多等待者、超时重等及问题／通知恢复逐项对应既有测试，补未覆盖组合。
- [x] S4.2 目录、读取、发送、回复及实际正文交付逐动作核对授权。已核实六种授权动作逐项匹配、跨项目必需授权、撤销即时生效；补 Ask/Notify 端口级逐动作与撤销证据。提交 `6a65e457`。
- [ ] S4.3 当前界面、分页、多窗口重连及消息类型展示复验。已有证据（本轮实测）：`conversation-event-display-projection.test.ts`（16 项，含"协作信封在 HostOnly 时间线上保持严格可见"与"通知信封是独立 HostOnly 时间线节点"）、`conversation-event-window-store.test.ts`（47 项）、`conversation-event-window-controller.test.ts`（39 项，含分页）、`selected-conversation-event-window-registry.test.ts`（17 项，含多窗口隔离）、`conversation-event-window-react.test.tsx`（4 项），共 123 项通过；`tsc -b` 类型检查通过。**未覆盖**：`conversation-collaboration-card.tsx` 组件本身没有任何导入它的测试，其挂载、回调与渲染行为只有类型检查，没有行为证据。
- [ ] S4.4 工具注册、模式过滤、执行、结果投影完整暴露链复核。已完成 G1（Plan 可见性过期断言）、G2（七条协作命令端口级分发测试）、G3（`wait_agent` 身份重复定义）、G8（协作工具文档注释不可逆损坏）。剩余 G4（`reply_question.unable_to_answer` 只有回显、无持久化效果，需领域模型决策）、G5（仅答疑模式工具收窄范围）、G6/G9（低优先级）。

### S5 总验收与交付

- [ ] S5.1 整理最终字段来源、当前 Schema、路由和用户资料，清除当前状态表中的过期待办。
- [ ] S5.2 原 P00—P12、R01—R14、A01—A18 逐项归档证据并关闭工作包。
- [ ] S5.3 连续五轮完整独自审核无新问题。目前 0/5；局部修复和测试不计轮次。
- [ ] S5.4 总结、按仓库约定归档正式计划，达到全部要求后结束目标。

## 执行约束

用户最新授权保存本次检查点并由后续模型实施、原主代理审核；本轮不启动子代理。保护并行 VCE 和用量改动；不兼容历史数据；按交接方案精确提交，不推送。不增加新的产品范围，发现缺陷必须归入上述原要求。测试先定位具体风险，已经充分验证的内容不反复全量运行。详细失败与修复证据仍保存在实施报告。

## 当前工作树状态（供审核核对）

多个工作包正在同一工作树内并行实施，`crates/vulcan-agent-runtime` 存在在途未完成编辑，因此该 crate 及其下游 `vulcan-provider-adapters`、`vulcan-service-kernel` 暂时无法编译：

- `provider_transport_vulcan/trace.rs` 报 E0425/E0422（`ProviderWireRequestTraceDispatch` 尚未定义）；
- `responses_compact.rs` 报 E0425（`read_bounded_response_text_with_capture` 尚未定义）与 E0061（方法签名已改为 5 参，调用点仍传 4 参）。

这些均为 S2.2/S2.3 在途实现，不是已提交代码的失败。相关验证结论必须在在途编辑落定、crate 重新可编译后复跑，不得用当前状态判断已提交成果。此外 `crates/vulcan-runtime-assets/resources/provider-manifests/` 的改动属于**其他会话的供应商工作**，本任务不修改、不暂存、不提交。

检查点标签核对：`refs/tags/checkpoint/agent-communication-20260922` 指向标注对象 `7f4b684d`，其提交为 `280f31a2`（"保存代理协作与动态上下文实现检查点"），与交接方案一致，未被移动；`280f31a2` 仍是当前 `HEAD` 的祖先。

