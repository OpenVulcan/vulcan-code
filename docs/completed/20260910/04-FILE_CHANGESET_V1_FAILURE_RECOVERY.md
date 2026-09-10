# 文件变更结果与会话持久化异常的完整修复方案

- 日期：2026 年 9 月 10 日。
- 当日任务序号：04。
- 状态：已完成；实现、定向验证与连续五轮零问题审核均已闭环。
- 用户目标：解决编辑或新建文件后工具中断、后续请求提示已有活动请求、界面仅显示“继续”且缺少错误原因的问题。
- 本次交付：完成文件 ChangeSet、工具持久化屏障、失败终态与继续资格的跨层修复，并保存方案、契约、审核报告及逐文件台账。
- 范围：文件变更结果契约、工具结果确认、请求生命周期、持久化错误提示、继续安全性和回归验证。
- 执行约束：保护工作树已有并行修改；不修改真实用户会话、公共数据库或供应商配置。
- 文件保存：本文已从受忽略的 `docs/plan/` 迁移到 `docs/completed/20260910/04-FILE_CHANGESET_V1_FAILURE_RECOVERY.md`；本次不改变忽略规则。

## 一、任务目标与已核实事实

### 1.1 目标与范围

本方案处理同一故障的完整链条：文件工具产生不合法的变更结果，工具完成事件无法写入会话，控制器继续申请下一次请求，旧请求仍活动而拒绝新请求，最终将真实错误收尾为可继续的进程中断。

实施必须同时达到以下目标：

1. 原生文件工具产生的全部变更结果符合当前会话持久化契约，尤其是新建文件、空文件和大文件。
2. 能在写盘前发现的结果结构错误，必须在发生文件副作用前被拒绝。
3. 工具业务错误仍允许模型获得错误结果；持久化确认错误必须中止本轮，不能被转换为普通工具结果后继续请求。
4. 持久化成功后才释放对应的执行准入；确认失败时不丢失原因、不重复工具副作用、不释放错误的所有者。
5. 本地工具或会话错误显示真实来源，不能让后续的 `RequestAlreadyActive` 覆盖最初错误。
6. 工具和请求不遗留运行状态；不安全的旧回合不能直接继续，完成失败收尾后允许用户正常提交新的根输入。
7. 正常停止、网络错误恢复、普通工具失败、并发工具和已有文件编辑保持原有语义。
8. 用真实文件工具返回值贯穿解析、持久化、事件投影和后续请求验证，不能仅靠各层分别通过单元测试交付。

本文最初按“设计并保存方案”的授权建立；用户随后明确授权实施并要求连续五轮零问题审核。下文保留原始问题证据，所有实现结论均以当前源码和本次验证记录为准。

### 1.2 证据基线与限制

- 源码基线：`D:\openvulcan\vulcan-code`，检查时提交为 `9b77e5fb`。
- 用户日志关联会话：`29a5a15d-f6c1-4f03-bc64-1439bc753d20`。
- 首个明确错误为 `payload.files[0].hunks: lifecycle changes must not contain modify hunks`，随后出现 `A V1 Provider request is already active.`。
- 用户截图末尾是 `.vulcan/temp/build_crops.py` 的文件工具行，随后只有“继续”；前面还出现 `.vulcan/temp/build_preview.py`。
- 修复前源码可以确认 `file` 工具的 `write` 动作创建新文件时必然构造冲突的变更形状；当前实现已经改为生命周期正文并通过规范解析器回归。原始日志没有携带失败工具的完整名称、参数及返回值，因此历史调用的精确参数仍需通过真实记录或复现确认。
- 本次没有读取或修复真实用户会话账本，也没有调用真实 DeepSeek 路由；已使用真实公开文件工具结果、可控 Provider/Core 夹具、V1 会话域故障注入和 WebUI 定向测试完成无外部副作用的跨层验证。
- 旧记忆只用于定位请求所有权和继续恢复的历史线索。当前 Continue 已采用 V1 事件窗口，不能照搬旧 sidecar 或旧账本资格方案。

### 1.3 修复前已确认的故障链

下表保留实施前捕获的故障证据与当时行号，用于解释修复动机；它不是对修复后源码现状的描述。

| 环节 | 修复前源码位置 | 修复前行为及修复含义 |
| --- | --- | --- |
| 工具公开入口 | `crates/vulcan-agent-runtime/src/tooling/native/file.rs`，`execute_file_tool`，第 39 行起 | 宿主结果作为 `kind=change_set` 返回，生产者不是模型本身。 |
| 新建与修改分类 | `crates/vulcan-host-tools/src/file_runtime.rs`，`native_write_file`，第 717–761 行 | 不存在的文件标成 `create`，存在文件标成 `modify`；文件提交发生在返回工具结果前。 |
| 不合法结果构造 | 同文件，`whole_file_payload`，第 848–865 行 | 无论变更类型均生成非空 `hunks` 数组，包含新建空文件的情况。 |
| 严格契约 | `crates/vulcan-change-set/src/persistent.rs`，`validate_persistent_file`，第 321 行起 | `create/delete` 禁止修改块，并要求完整、截断或省略的生命周期正文。 |
| 工具确认提前退出 | `crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs`，`finish_tool`，第 1955 行起 | 先解析 ChangeSet，失败就退出；既没有完成工具事件，也没有完成 API 组。 |
| 错误确认丢失详情 | `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge.rs`，`reject_runtime_event_projection_barrier_from_event`，第 717 行起 | 拒绝方式是移除发送端；接收端只得到通用持久化屏障被拒绝错误。 |
| 致命错误被当成普通结果 | `crates/vulcan-core/src/controller/tool_execution.rs`，第 297、317、345 行附近 | 单工具、批次准备、并发工具路径都将执行器异常转换为普通工具结果。 |
| 旧请求仍被占用 | `provider_v1_sink.rs`，`prepare_request` 第 1416 行、`complete_api` 第 2109 行起 | 活动请求只在可靠完成后清空；新请求申请仍会遇到活动请求检查。此检查本身应保留。 |
| 原始错误被后续错误覆盖 | `crates/vulcan-service-kernel/src/agent_stream/stream_task.rs`，第 567–605、878–918、1289 行附近 | 工具错误写入临时错误变量；准备分支仍调用 `prepare_request` 并可覆盖错误，最终收尾主要读取运行报告。 |
| 错误被收尾为进程中断 | 同文件，`persist_v1_execution_failure_boundary`，第 114–155 行 | 存在活动 API 时先执行 `interrupt_active_api_for_process` 并立即返回，绕过原始错误提示的写入。 |
| 没有正文错误却允许继续 | `crates/vulcan-session-core/src/v1/document/api.rs`，第 621、1742、1784 行起 | 进程中断写入 `Interrupted + ProcessInterrupted + error=None + Allowed`，不生成错误提示。 |
| 前端据此显示继续 | `apps/vulcan-code-web/src/service/conversation-continuation-selector-v1.ts`，第 46–119 行 | 只要开放回合的最后 API 有可继续终态，就生成 Continue 候选。 |

因此，这不是单独改一个 JSON 字段就能完整解决的问题。生产结果、错误传递、失败结算及界面证据都需要闭合。

### 1.4 实施前已有可复用能力与测试缺口

- `file_runtime.rs::insert_lifecycle_content` 已按现有 `MAX_INLINE_LIFECYCLE_BYTES` 上限生成正文及截断表示；`build_create_change`、`build_delete_change`、`raw_lifecycle_payload` 已使用相应路径。
- `complete_api_group` 已支持失败 API 的工具状态收尾、错误提示、用量、输入投影和可靠修订，但默认只关闭物理 API，不自动关闭逻辑回合。
- `fail_turn_without_active_api` 会追加错误并关闭回合，但明确保留已经标成 `ProcessInterrupted` 的继续资格；不能通过先误标进程中断、再调用该函数来修复。
- `apply_tool_event` 在可靠写入后还会执行实时生命周期投影。必须区分写入前失败和已写入后的界面投影失败，不能认为所有异常都代表工具结果未落盘。
- `SessionHandle::mutate_durable_if_changed` 先修改内存，再通过 `flush_through` 等待可靠修订。因此写入返回错误时，内存里可能已经存在合法工具结果；“内存已更新但落盘未确认”也是必须单独处理的阶段。
- 现有 `native_write_converts_encoding_and_applies_new_file_defaults`、`native_write_accepts_empty_content` 主要验证文件字节，未将真实返回值送入 V1 解析器。
- 现有 `malformed_tool_finished_changeset_fails_without_fallback` 仅验证畸形结果被拒绝，未验证拒绝后的 Core 停止、API 收尾和正文错误。
- 现有 Core 测试覆盖将一般执行器失败保留为工具结果。修复必须增加专门的致命错误分类，而非将全部工具异常改为中断。

## 二、修复设计与技术选型

本节保留实施阶段的设计要求；其中“应”“需”等措辞描述验收约束，不表示功能仍未落地。当前实现与验证状态以本文末尾执行变更总结、稳定契约文档和审核报告为准。

### 2.1 总体选择

沿用现有 Rust 文件事务、`vulcan-change-set` 校验器、V1 会话所有者和事件窗口。增加必要的进程内错误分类与失败结算方法，不引入外部核心库，不切换供应商协议，不放宽持久化校验，不建立第二套会话账本。

文件系统事务和会话持久化不是一个原子事务。方案通过“写盘前验证结果”消除可预防的格式错误；对写盘后存储失败保留副作用不确定性并禁止自动重放。不能声称这会消除断电、磁盘故障造成的跨资源确认缺口。

### 2.2 明确宿主结果与 V1 持久化结构的边界

当前宿主结果不是直接输出 `PersistentChangeSet`。`parse_lifecycle_content` 从宿主原始字段构造持久化对象，修复必须使用实际输入契约：

| 变更类型 | 宿主结果中的正确字段 | V1 解析后的字段 |
| --- | --- | --- |
| 新建、删除文本文件 | `change`、`path`、`content_mode`、`total_line_count`；按模式携带 `content` 或 `content_head/content_tail`；不含非空 `hunks` | `lifecycle_content`；修改块为空 |
| 修改已有文件 | `change=modify`、`path`、非空 `hunks`；块内使用 `delete/insert`；不含生命周期正文字段 | `hunks.deleted/inserted`；`lifecycle_content` 为空 |
| 纯重命名 | `path=new_path`、不同的 `old_path/new_path`、生命周期正文；不含修改块 | 路径关系和生命周期正文一致 |
| 重命名并修改 | 正确的旧新路径、非空修改块；不同时携带生命周期正文 | 保持当前重命名加修改契约 |
| 无法解码的删除或纯移动 | 沿用明确的 `content_mode=omitted` 和已有二进制标识 | `PersistentLifecycleContent::Omitted`，不能伪造正文或行数 |

不能直接向宿主 JSON 塞入名为 `lifecycle_content` 的字段来代替上述字段：当前解析器读取的是 `content_mode`。也不能在消费者里猜测新旧形状、静默丢弃非法修改块，或把新建文件谎报成 `modify`。

### 2.3 文件生产端修复与提交前校验

1. 在 `native_write_file` 中，根据已确认的文件存在性选择两种明确的构造路径。新建使用规范化后的 `after_document.text` 及其逻辑行构造生命周期正文；已有文件修改沿用整文件修改块。
2. 将 `whole_file_payload` 收窄为只负责 `modify` 的构造函数，移除可任意传入 `change_kind` 的误用入口；更新相关内部测试调用。
3. 复用 `insert_lifecycle_content` 的完整与截断规则。保留实际编码、BOM、换行和 `vulcan.file_tools.v2` 中的前后格式信息。正文来自最终待写内容，不能来自未规范化的请求字符串。
4. 在 `vulcan-host-tools` 增加对工作区已有 `vulcan-change-set` 的依赖。当前后者仅依赖序列化库等基础设施，不反向依赖宿主工具；此处无需引入新外部库。
5. 为文件变更响应增加统一的内部构造与验证入口：先从 `PreparedChange` 构造最终 `kind=change_set` 封套，调用实际的 `persistent_change_set_from_host_result`，要求其成功返回变更集，再允许文件提交。
6. 以内部“已验证结果”值持有同一份待返回 JSON，避免验证一个对象、提交后又重新拼出另一个对象。解析过程不复制整份会话历史，不额外生成无界大文件正文。
7. 顺序固定为：准备所有文件及响应 → 验证整个变更封套 → 检查文件版本前置条件 → 提交文件事务 → 返回同一份已验证结果。发生文件提交失败时仍沿用现有事务回滚和 `disk_state_uncertain` 结果，不返回准备好的成功响应。
8. 校验覆盖六处现有 `commit_changes` 调用：原生写入、删除、移动、精确替换、批量补丁、旧结构化编辑。旧结构化编辑的预览也必须验证返回结构，但不触发写盘。
9. 只读的 `read/list/stat` 不进入 ChangeSet 校验。普通参数错误、找不到匹配、无变更、编码或过期版本等错误保留现有工具反馈。

提交前结构校验是新增保护，服务端和会话域中的现有严格校验仍保留，防止其他工具或后续生产者返回畸形结果。

### 2.4 将持久化错误从普通工具错误中分离

当前实现已经新增明确的内部致命错误分类 `CoreError::ToolDurabilityFailure`。它只表示工具批次准备、开始或结果确认的可靠性失败；普通业务错误仍使用已有结果或 `ToolFailure` 语义。

错误携带当前边界的真实身份、阶段、稳定原因码及安全处理后的原始详情。单工具身份来自实际请求，批次身份来自已准备请求列表；不能用空字符串或候选 ID 填充未知身份。

具体调整：

- 在既有 `RuntimeEventProjectionReceipt` 中增加显式拒绝回执，直接传递原始错误。拒绝方不能再仅靠丢弃发送端表达业务拒绝。
- 真正的通道断开和八秒超时分别形成稳定错误原因。可靠工具边界必须终止；仅显示排序的非可靠屏障保留已有语义，不能全局扩大影响。
- `StreamingRuntimeToolExecutor` 将该拒绝回执映射为明确的致命错误。不得先转换成 `InvalidState` 字符串再用字符串匹配恢复分类。
- `execute_tool`、`execute_tool_batch` 的准备与执行分支都直接传播该分类，不调用 `build_tool_execution_failure_result` 合成普通结果。
- 恢复工具批次经过相同分类；已经可靠完成的结果只复用，不能重新调用工具体。
- 补齐 `CoreError` 的显示、穷尽匹配和运行时映射。`provider_transport_retry` 明确对该错误返回不可自动重试；流式运行入口直接产生失败报告，不触发路由切换、压缩补偿或下一轮模型请求。

### 2.5 首个错误、并发工具和请求停止

服务层应记录当前流的首个边界失败，替代任意覆盖字符串的行为。记录由当前 `active_stream_task_id`、V1 API 身份及强类型工具边界产生。后续清理异常作为附加诊断保存，不替换首次失败。

首次失败后的执行规则：

1. 立即拒绝该工具屏障并保留原始详情；停止新的批次、工具开始和 Provider 准备准入。
2. `ProviderRequestPreparationBoundary` 若迟到，直接以首个失败拒绝其完成句柄，不再次调用 `sink.prepare_request`，也不分配新 API 记录或追踪槽。
3. 正在运行的同批次工具仍须收尾。当前“有错误就跳过所有 RuntimeEvent”的分支需拆开：拒绝新开始，接纳既有运行工具的真实结束事实，继续释放它们各自的等待句柄。
4. Core 保持已有并发批次的收集机制，记住首次致命错误并收集已启动兄弟调用的结束；最终返回错误，不把残缺批次作为成功结果交给模型。
5. 后续合法的兄弟结果可以正常持久化其工具状态，但失败中的 API 组不能因“最后一个工具结束”进入成功结算。
6. 不得通过丢弃 `FuturesUnordered` 或取消异步外壳，推断底层文件写入已经取消。`spawn_blocking` 和外部命令的副作用必须按真实退出事实判断。
7. 能通过既有所有者取消的运行任务使用现有取消路径；仍在运行或无法确认退出的任务保留准入阻塞并展示“正在收尾/尚未确认退出”。对于长生命周期命令保留按 `command_session_id` 查询的路径，不额外终止用户的其他进程。
8. 延迟回调、重复确认、旧流结束事件均校验原有身份，不得终结新一轮请求。

这组规则同时约束单工具和并发工具，不使用等待固定时长后盲目清空活动请求的处理方式。

### 2.6 失败结算必须区分四种状态

| 失败时的已确认事实 | 结算要求 |
| --- | --- |
| 工具结果在进入会话内存前就因契约错误被拒绝，API 仍活动 | 结算该 API 的宿主持久化失败；未确认工具标记为结果中断，保留已经可靠成功或失败的兄弟工具；写入原始错误并关闭逻辑回合。 |
| 合法工具结果已进入内存，但可靠写入返回失败 | 按精确事件身份及内存/可靠修订边界确认状态，保留已通过校验的结果；恢复的是原修订及失败结算的写入，不重跑工具，也不把内存结果擅自宣布为可靠成功。 |
| 工具结果已经可靠提交，但实时生命周期投影失败 | 保留已提交的工具结果与已有 API 统计，不回滚、不覆盖成功记录、不再执行工具；以宿主投影失败提示收尾，并从 V1 权威状态重新同步界面。 |
| API 尚未开始或没有活动 API | 使用原有 API 前失败能力追加错误并关闭对应回合；不虚构物理请求、工具身份或网络派发事实。 |

在 `ProviderV1StreamSink` 与 `SessionDocument` 增加专门的宿主边界失败结算入口，拆分可复用的暂存逻辑，保持唯一写入所有者。不能只将 `self.active` 设为空，也不能拿通用进程中断代替真实失败。

存储表示沿用当前 V1 Schema：

- 尚未终结的 API 使用已有合法组合：`ApiCallStatus::Failed`、`ApiEndReason::Unknown`、`ApiErrorCategory::Unknown`、`ContinuationEligibility::Forbidden`。这里的 `Unknown` 是当前枚举中的未分类宿主错误，具体来源由明确的错误码和消息表达。
- 使用现有 `RedactedApiError.error_code` 记录稳定的宿主执行失败原因；当前流收尾沿用 `stream_finished_without_v1_api_terminal`，原始 ChangeSet 或屏障错误保存在 `message` 与 SystemError `redacted_detail` 中，不冒充供应商协议错误。
- 追加 `DisplayNoticeKind::SystemError`、`severity=error` 的持久化提示，绑定对应 API；包含实际工具名称、阶段、错误字段位置及副作用确认情况。相同故障只写一条主提示。
- 原始工具完成未入账时，使用既有 `ToolCallStatus::Interrupted` 表示结果未可靠确认。它不等于文件事务已回滚，也不等于工具体一定没有执行。
- 若工具结果已合法进入会话内存，只是 flush 失败，后续结算按会话域的真实工具状态和可靠修订处理，不能按 sink 中尚未更新的镜像状态覆盖结果。恢复写入沿用原事件身份，可靠前沿确认后才允许结束告警和释放准入。
- 物理 API 统计、主错误提示、必要工具状态和失败回合关闭应由一次暂存后的可靠提交完成。当前 API 结束与回合关闭函数不能简单先后调用而留下半完成状态；需在同一候选变更中验证和提交。
- 当前 `TurnOutcome` 的失败收尾沿用现有停止语义及运行报告失败事实。成功完成 Hook、目标完成、队列自动续跑或子任务自动回流不能因失败结算回执被误触发。
- 使用 `active.pending_terminal` 已保存的真实用量、请求配置、时序和 Provider 源引用；保留已放置的 `provider_source` 一致性。不用默认零覆盖已知用量，不凭空创建第二次 API 统计。
- 若 API 已有终态，仅追加仍缺失的错误与逻辑回合关闭，不重写既有物理 API 事实。
- 保持现有暂存式原子验证：容量、引用、修订、时间与终态组合任何一步失败，均不提前发布部分变更。

已经接受的补充输入必须继续可追踪。沿现有 API 组结算规则将其按原 `input_id`、资源引用和绑定配置可靠投影一次，但失败后不自动派发，界面明确该补充尚未执行。尚未准入的普通待发送队列保留原队列；不静默删除、重建新身份或替用户重发。

只有工具收尾已确认、失败事件已可靠落盘且对应所有者仍匹配时，才清理 sink 活动状态、释放终态持久化所有者并开放新根输入。持久化失败保留阻塞，不能用无限刷新或再次点击继续来绕过。

### 2.7 原始错误的界面与实时诊断

正常可落盘路径以 V1 的错误 `DisplayNote` 为权威：正文持续显示，切换会话、刷新和重新载入后仍存在。不得只写控制台、只改变按钮或仅弹一次短暂提示。

面向用户的提示分清两种事实：

- 写盘前校验失败：说明“文件变更结果校验失败，本次文件操作未提交”，列出对应工具与原因。
- 写盘后确认失败：说明“工具结果未能保存到会话，文件可能已发生变化，本轮已停止”，展示实际已知信息；只有确有成功提交证据时才显示“文件已写入”。

`ConversationV1DisplayNoteView` 已能显示错误提示，应优先复用。`conversation-branch-presentation-v1.ts` 的错误投影也必须区分本地系统错误与真正 Provider 错误，不能为本地失败拼造 `provider unknown`、网络派发状态或未观察到副作用等默认事实。确有结构化 Provider 错误的原有卡片继续使用其真实字段。

实施阶段核实到 `SessionHandle::mutate_durable_if_changed` 已在磁盘 flush 前发布同一修订的 V1 volatile event patch，且事件窗口和当前会话句柄本身就是同进程实时权威。因此不再新增第二套 `SessionExecutionFailureChanged` 状态，最终采用以下闭环：

1. `SessionDocument::fail_execution_boundary_and_close_turn` 在同一候选修订中生成失败 ApiStats、SystemError、工具中断与 Turn 关闭事实。
2. `mutate_durable_if_changed` 先发布该修订的 volatile patch，再等待磁盘可靠前沿；当前 WebUI 直接沿既有 V1 事件窗口显示 `redacted_detail`。
3. 首次物理存储失败后，仅重试同一内存脏修订；幂等领域方法不追加第二条错误，也不重跑工具。
4. 两次存储均失败时保留精确终态准入 owner，Continue 与新请求均保持阻塞；同一服务进程仍以当前会话句柄中的失败事件为实时事实。
5. 一旦同一修订可靠落盘，既有事件窗口自然转为可重载的持久化事实，不需要并行错误账本或前端去重协议。
6. 若存储持续失败后进程退出，不能保证尚未落盘的原始详情可恢复；重启后只按真实持久化前沿恢复，不伪造成功或错误。

这一调整是实施时依据当前源码作出的寻优：复用已有单一 V1 事件权威，避免新增与会话文档竞争所有权的实时状态，同时仍满足同进程可见、可靠前沿确认和失败关闭要求。

### 2.8 Continue 与恢复语义

| 场景 | 模型自动进入下一轮 | 继续旧回合 | 用户下一步 |
| --- | --- | --- | --- |
| 文件变更和工具结果可靠成功 | 允许 | 按正常执行流程 | 无需额外操作 |
| 普通参数、匹配或工具业务失败，结果已可靠记录 | 保留当前业务反馈流程 | 由既有回合状态决定 | 模型可读取失败结果并调整 |
| 宿主结果契约或持久化确认失败 | 禁止 | 禁止 | 失败可靠结算后检查实际文件，再提交新根输入 |
| 终态记录尚未可靠保存或仍有未确认运行任务 | 禁止 | 禁止 | 显示持续告警并保持阻塞，等待故障恢复和真实结算 |
| 用户正常停止、真正进程中断、可恢复网络错误 | 保留既有策略 | 保留既有 V1 资格与恢复分类 | 正常继续，不被本修复一刀切禁止 |

前端继续资格仍来自 V1 权威事件和服务准入，不能通过解析英文错误文本猜测是否可继续。既有 volatile V1 事件只能进一步反映失败状态，不能单独授予继续资格；后端继续 RPC 必须复核同样的实时失败及持久化条件。

### 2.9 明确不采用的处理

- 不放宽 `validate_persistent_file`，不自动把 `create+hunks` 翻译成合法结果。
- 不在失败后直接重跑文件工具，也不把文件已经存在当成上一次成功的证明。
- 不对写盘后失败实施无条件回滚；文件可能已被后续工具或用户修改。
- 不通过清空活动请求、改成网络错误、切换模型或协议消除表面报错。
- 不用通用 `ProcessInterrupted` 消费已知的宿主错误。
- 不重新引入旧 sidecar、第二份转录或独立的继续状态权威。

## 三、执行步骤与文件范围

### 3.1 实施顺序

| 阶段 | 工作内容 | 完成门槛 |
| --- | --- | --- |
| 甲：建立复现与基线 | 检查最新工作树；补跨层原生文件返回值测试和可控 Provider 流夹具；记录修复前失败 | 真实 `file write` 新建结果可重现首个错误；故障夹具能断言后续请求数与错误事件缺失 |
| 乙：修复生产端 | 分离新建与修改构造；所有变更入口写盘前验证最终响应 | 新建、修改、删除、移动、补丁及预览矩阵通过；构造错误零文件副作用 |
| 丙：修复错误传播 | 显式拒绝回执、致命错误分类、Core 单工具与批次传播、运行时禁止重试 | 首次致命错误传到流报告；不会产生下一次 Provider 请求；普通工具错误行为保持 |
| 丁：修复持久化结算 | 首错保留、并发工具收尾、原子失败终态、回合关闭、所有者释放 | 正文错误可靠存在；活动工具和 API 无遗留；新根输入可准入；不安全 Continue 被拒绝 |
| 戊：补齐展示与断连 | 复用 V1 volatile patch 与事件窗口展示本地错误；存储失败时保留同一脏修订和终态 owner | 实时与重载路径使用同一事件权威；未可靠结算不会误显示可继续 |
| 己：完整验证与交付 | 全链路压力、故障注入、正常恢复回归、桌面或浏览器验收；补契约文档 | 本文验收矩阵全部闭合，记录实际结果和仍未验证的边界 |

阶段乙不能作为整项任务完成点。至少阶段丙、丁、戊和跨层验收完成后，才具备发布修复的条件。

### 3.2 预计变更文件

下表是实施范围，不表示本次已修改这些文件。新增模块应按职责组织，避免继续扩大已有超长源文件。

| 区域 | 主要文件 | 修改目的 |
| --- | --- | --- |
| 文件工具 | `crates/vulcan-host-tools/src/file_runtime.rs`、`crates/vulcan-host-tools/Cargo.toml` | 新建正文构造、提交前共享校验、复用已验证响应 |
| 变更契约 | `crates/vulcan-change-set/src/tests.rs` | 增加生产者与持久化形状边界的正反例；现有严格解析规则不放宽 |
| 原生工具跨层测试 | `crates/vulcan-agent-runtime/src/tooling/native/tests/file_tools.rs` | 真实公开工具返回值送入 V1 解析器 |
| Core 错误和调度 | `crates/vulcan-core/src/types.rs`、`controller/tool_execution.rs`、`controller/tests/streaming.rs`、`controller/tests/tool_batches.rs` | 致命错误传播、批次收集和普通业务失败回归 |
| 工具确认桥 | `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge.rs`、`tool_runtime_bridge/tests.rs`、`provider_transport_stream_executor.rs` | 明确拒绝详情、可靠屏障错误分类和取消/超时 |
| 运行报告与重试 | 同目录 `provider_transport_stream_runtime.rs`、`provider_transport_retry.rs` | 新错误类型匹配、报告透传、禁止重试 |
| V1 服务接入 | `crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs`、`stream_task.rs`、`detached_subagent.rs` | 首错、失败阶段、兄弟工具结算、迟到准备拒绝，以及根会话与分离子任务的失败回执处理 |
| 会话域与所有权 | `crates/vulcan-session-core/src/v1/document/api.rs`、`recovery.rs`、`turns.rs`；`crates/vulcan-service-kernel/src/session_runtime_registry_v1.rs` | 复用暂存逻辑完成原子终态与回合关闭，保持准入所有者 |
| 会话域回归 | `crates/vulcan-session-core/src/v1/document/document_tests/api.rs`、`tools.rs`、`turns.rs` | 幂等、原子失败、输入和用量保持、正常中断恢复 |
| 服务全链路测试 | `crates/vulcan-service-kernel/src/tests/session_kernel_tests.rs`、`agent_stream/provider_v1_sink.rs` 既有测试模块 | 验证非法 create+hunks、活动 API 失败结算及首次存储失败后的同修订重试 |
| 实时诊断接口 | `crates/vulcan-service-api/src/conversation.rs`、`conversation_events_v1.rs`；`crates/vulcan-service-kernel/src/conversation_events_v1.rs` | 实时失败增量、加载/重连快照；服务端状态归现有所有者管理 |
| 前端状态与类型 | `apps/vulcan-code-web/src/types/conversation.ts`、`service/conversation-event-window-store.ts`、`state/conversation-live-stream-store.ts`、`state/conversation-live-stream-context.tsx`、`state/workbench-state.ts`、`state/workbench-stream-projection.ts` | 同步新增诊断投影与精确去重、重连和订阅 |
| 前端正文与继续 | `components/conversation/chat-area.tsx`、`conversation-v1-display-renderer.tsx`、`conversation-branch-presentation-v1.ts`、`service/conversation-continuation-selector-v1.ts` 及对应测试 | 系统错误可见性、本地来源、失败期间继续拦截和新输入恢复 |
| 文档 | 新增 `docs/file-tool-changeset-and-failure-lifecycle.md`，并维护本计划、审核报告与逐文件台账 | 记录原始/持久化字段映射、可靠屏障与失败收尾契约；本任务不修改终端工具契约 |

表中 Core 与前端子路径分别相对其已列出的 crate 或 `apps/vulcan-code-web/src`。穷尽匹配、序列化快照、重连控制器和测试模块登记的补改，需在编译或调用链定位后限定到实际消费点。

实施时 `Cargo.lock` 仅允许必要的本地 crate 依赖边变化；不得为了此次修改重新升级外部依赖。当前该文件、根 `Cargo.toml`、服务 crate 的依赖及多个供应商和代码索引模块已有并行修改，必须保留。共享前端国际化文件如需加文案，只改本任务键值。

## 四、验证与验收标准

### 4.1 贯穿各层的测试基准

使用可控、无外部凭据的 Provider 夹具，输出真实公开工具请求；运行实际原生工具，获得真实宿主结果，再交给真实 ChangeSet 解析器和 V1 会话所有者。验证文件字节、工具事件、API 统计、错误提示、回合状态、请求次数及重载结果。

不得用手写的“预期合法 ChangeSet”替换原生工具结果。业务单测仍保留，但它们不能替代这一贯穿验证。所有文件写入、会话落盘与错误注入使用独立临时目录，不操作用户项目和公共数据。

### 4.2 验收矩阵

| 编号 | 输入或注入位置 | 必须验证的结果 |
| --- | --- | --- |
| 验收一 | `file write` 创建普通文本文件 | 文件字节正确；实际结果为生命周期正文；V1 成功落盘；工具结束后能正常请求下一轮 |
| 验收二 | 新建空文件、清空已有文件 | 两者分别为合法 `create` 与 `modify`；空新建不能携带修改块；零行统计不被伪造成非零 |
| 验收三 | 已有文件整写、`file_edit` 精确替换 | 实际修改块、行号、编码、BOM 与换行保持；成功工具不被误判为生命周期变更 |
| 验收四 | 生命周期正文大小在当前阈值前、等于阈值、超过阈值，包含中文和换行边界 | 完整/截断选择合法，字符不破坏，真实总行数和前后文本格式保持 |
| 验收五 | 文件删除、纯移动、补丁新增/删除/修改/移动加修改、旧编辑预览 | 整个结果通过 V1；实际操作与路径一致；预览无副作用；二进制省略信息不虚构正文 |
| 验收六 | 提交前注入 `create+hunks`、缺正文、错误路径等畸形记录；无效文件位于批次中间 | 整批校验失败；任何目标都未创建、删除或覆盖；没有成功响应；原始错误位置明确 |
| 验收七 | 写入已经完成，再注入非法宿主结果进入 `ToolFinished` | 首个错误保持；文件保持真实状态；不再申请或派发下一次 Provider；最终出现一个可靠系统错误，禁止继续 |
| 验收八 | 在批次准备、工具开始、工具完成分别拒绝、断开或超时屏障 | 开始前没有工具副作用；结束后不声称未执行；所有等待句柄被结束；致命错误穿透 Core |
| 验收九 | 普通工具业务失败、参数不合法、权限或匹配失败 | 合法错误结果可持久化并按原业务流程反馈模型；不因新增分类全部变成致命错误 |
| 验收十 | 三个以上并发工具，首、中、末不同位置失败，兄弟完成顺序不同 | 已启动任务真实收尾；成功兄弟结果保留；未启动调用不执行；失败组不成功结算；不漏槽、不重放 |
| 验收十一 | 工具结果持久化成功后，实时生命周期投影失败 | 已持久化结果与已有 API 统计不被覆盖；只报告投影故障；重载恢复真实结果；工具体执行次数仍为一次 |
| 验收十二 | 原始 ChangeSet 错误后再到达新请求准备、遥测、结束报告或清理异常 | 主错误仍是首次失败；不会新增 API/追踪记录；不会出现以 `already active` 取代根因的正文 |
| 验收十三 | 故障结算阶段存储写入失败，随后恢复 | 保留对应终态所有者和持续实时告警；恢复后重试的是同一结算写入；工具体不重跑；持久化确认后才开放输入 |
| 验收十四 | 重复 Finished、重复拒绝、重复终态提交、旧流回调晚于新流 | 一个工具终态、一份 API 统计、一条主错误；原有及新请求所有者不被交叉清理 |
| 验收十五 | 存在 Provider 用量、已绑定请求、Provider 源引用和补充输入时失败 | 已知用量不清零；请求身份不漂移；源引用验证通过；已接收输入不丢失、不自动重发 |
| 验收十六 | 新故障发生后刷新、切会话、重连、进程重启 | 已落盘错误持续存在；同进程未落盘错误可从实时快照恢复；重启后不伪造此前未落盘详情 |
| 验收十七 | 用户正常停止、网络错误 Continue、恢复工具批次 | 原有继续安全分类保持；已完成工具只复用；执行中断工具不自动重复副作用 |
| 验收十八 | 至少连续五十轮受控文件操作，交替新建、修改、移动和删除 | Provider 次数严格等于夹具预期，额外重试数为零，执行结束无残留活动 API/工具/屏障 |
| 验收十九 | 用户相同形态：生成预览脚本，再生成裁剪脚本 | 正常执行跨过两次新建；故障注入时明确展示工具结果保存错误，不以孤立“继续”结束 |
| 验收二十 | 合法工具结果进入内存后，首次 flush 失败，再次可靠写入成功 | 保留同一工具事件及结果；内存与可靠前沿正确推进；不重复记账、不把工具重新标成未执行、不重跑工具体 |

### 4.3 定向检查与运行命令

以下命令是本次实施采用的验证基线；实际执行结果记录在审核报告和最终执行变更总结中。先跑目标用例，再按风险扩大到受影响 crate，不启动无约束的工作区全量测试。

在仓库根目录执行：

```powershell
rtk cargo check -p vulcan-host-tools -p vulcan-change-set -p vulcan-core -p vulcan-agent-runtime -p vulcan-session-core -p vulcan-service-kernel -p vulcan-service-api --locked
rtk cargo test -p vulcan-host-tools --lib -- --test-threads=1
rtk cargo test -p vulcan-agent-runtime --lib file_changeset_v1_native_create_round_trips_through_persistent_contract -- --test-threads=1
rtk cargo test -p vulcan-change-set --lib -- --test-threads=1
rtk cargo test -p vulcan-core --lib -- --test-threads=1
rtk cargo test -p vulcan-session-core --lib -- --test-threads=1
rtk cargo test -p vulcan-agent-runtime --lib projection_barrier -- --test-threads=1
rtk cargo test -p vulcan-service-kernel --lib execution_failure_boundary -- --test-threads=1
rtk cargo test -p vulcan-service-api --lib
rtk rustfmt --edition 2024 --check <本任务精确 Rust 文件列表>
rtk git diff --check -- <本任务精确已跟踪文件列表>
```

在 `apps/vulcan-code-web` 目录执行：

```powershell
rtk pnpm test src/components/conversation/conversation-v1-display-renderer.test.tsx src/components/conversation/conversation-branch-presentation-v1.test.ts src/components/conversation/conversation-system-error-card.test.tsx src/service/conversation-continuation-selector-v1.test.ts
rtk pnpm run lint
rtk pnpm run check:ui-architecture
rtk pnpm run build
```

服务内核已增加活动 API 失败结算、成功 API 后宿主投影失败结算，以及首次物理存储失败后同一脏修订重试测试；本任务 8 个精确故障恢复用例均已通过。服务内核 `--lib` 全量测试可执行文件在无输出状态下长时间未结束，已人工中止；Agent Runtime 全量测试存在 3 个由其他会话供应商目录与回放路由改动导致的范围外失败，Service API 全量测试存在 1 个 VCE 快照范围外失败。生产代码已通过受影响 crate 的 `cargo check --locked`，详细证据保存在审核报告中，没有越界修改相关夹具。

保留仓库 `build.jobs=4` 和默认 `RUST_TEST_THREADS=4`。涉及进程级屏障注册表、Provider 清单、服务注册表和回环端口的测试使用单线程；使用 nextest 时维持对应串行组，文档测试如有变更需单独执行。

### 4.4 桌面或浏览器验收

在与修复源码对应的构建上，使用独立验收会话复现用户截图中的操作模式。至少验证新建两个脚本、修改已有脚本和执行校验命令能自然连贯完成；再通过可控测试故障验证错误卡、文件状态说明、继续阻止和新根提交。

记录构建版本、工具名称、参数、实际文件状态、会话/API/工具 ID、预期及实际请求次数、界面结果。不把 Mock Provider 测试写成 DeepSeek 真机已验证；真实 DeepSeek 路由只在具备测试环境和允许的调用条件下验收，不改动用户现有供应商配置。

最终发布不能仅复用旧二进制。资源本身未变化时不为此次修复强制执行资源同步；如实际改动权威资源，再依仓库约定显式执行 `make.ps1 sync`，不直接编辑 `output/`。

## 五、风险、历史会话与交付规则

### 5.1 历史故障会话

修复生产者不会自动补回已丢失的历史错误。旧会话可能只保留 `ProcessInterrupted`，而文件已经存在；这两项不足以证明当时工具成功，也不足以可靠重建当时的变更结果。

- 不批量重写旧的中断统计，不按文件存在性伪造历史 ChangeSet，不清空公共数据库。
- 原有 Continue 恢复对已开始或已中断工具必须遵循当前真实的恢复分类，不能重新执行不确定副作用；将这一点作为本次回归项。
- 对用户提供的会话若需要专项恢复，先只读导出已记录事件及实际相关文件，再另行设计有明确目标的恢复操作。本文不把修改该真实会话列为实施前提。
- 无法恢复原始错误详情时明确写“原始错误未保存”，不能把本次静态判断作为历史运行事实写入账本。

### 5.2 实施风险与边界

| 风险 | 应对 |
| --- | --- |
| 将所有执行器异常设为致命，破坏模型纠错流程 | 仅新增强类型可靠性错误；普通失败回归必须保留 |
| 新终态违反现有 V1 校验或损失 Provider 源引用 | 复用合法终态组合、已冻结事实和暂存校验；覆盖加载器重载验证 |
| API 失败但回合仍开放，Continue 被禁后会话无法再输入 | API 终态、错误提示和回合关闭整体结算；验收实际新根输入 |
| 并发任务仍在写文件却已释放会话 | 保持真实任务收尾与所有者约束；无法确认退出时持续可见并保持阻塞 |
| 非法工具结果丢弃后被误判为“未写文件” | 明确区分提交前拒绝和写盘后确认失败；不自动回滚或重放 |
| 后续准备、结束报告或清理错误遮蔽根因 | 首个错误不可覆盖；附加异常单独记录并关联同一故障 |
| 失败时用量归零、用户补充被丢弃或目标被完成 | 从原请求终态事实结算；保留输入身份，禁止失败后自动链式执行 |
| 存储失败后实时错误与持久化错误分叉 | 复用同一 V1 内存修订的 volatile patch；恢复时只推进可靠前沿，不建立第二状态源 |
| 直接修改本地共享依赖文件覆盖并行工作 | 实施前重新检查 diff，仅改本任务依赖边；不还原、不全量暂存 |

### 5.3 完成检查表

- [x] 真实新建文件返回值通过当前 V1 校验，完整、空文件和截断模式均覆盖。
- [x] 所有实际文件变更入口在提交前验证最终返回封套，格式失败无文件副作用。
- [x] 可靠屏障原始错误穿透 Core，单工具、批次及恢复路径均不派发额外请求。
- [x] 并发兄弟工具正确收尾，无丢失结果、悬挂等待或迟到跨所有者修改。
- [x] 工具确认失败、实时投影失败和真正停止分别按真实阶段处理。
- [x] 失败统计、持久化错误、回合关闭与准入释放一致，重复结算幂等。
- [x] 正文错误通过 V1 事件持续可见；存储不可写时保持同一脏修订与准入 owner，重载只恢复可靠前沿。
- [x] 不安全 Continue 被拒绝，失败收尾后新根输入可用，正常恢复语义不回退。
- [x] 实际用量、请求配置、Provider 源、待执行输入和目标状态按既有 V1 领域规则保持。
- [x] 定向测试、跨层故障注入、关键路径重复回归和 WebUI 组件验收完成；真实 DeepSeek 桌面调用明确保留为外部运行验收边界。
- [x] 契约文档已同步，所有未验证事项均明确列出，不宣称未执行的真机验收已经通过。

### 5.4 计划状态与执行记录

实现、验证和连续五轮零问题审核已经完成。本文件已在写入下述执行变更总结后迁移到 `docs/completed/20260910/04-FILE_CHANGESET_V1_FAILURE_RECOVERY.md`。本会话没有执行 Git 暂存、提交或推送。

## 六、执行变更总结

### 6.1 核心修复与调整概述

- 修复原生 `file write` 创建文件时错误生成 `create+hunks` 的根因：新建使用生命周期正文，已有文件整写继续使用修改块。
- 所有原生写入、删除、移动、精确编辑、Apply Patch 和旧编辑预览在文件副作用前使用规范 V1 ChangeSet 解析器校验真实公共结果。
- 投影屏障能够携带原始宿主拒绝原因，并映射为不可重试的 `ToolDurabilityFailure`；Core 会停止 Provider 循环，同时等待已经启动的并发兄弟工具完成。
- 服务层保留首个 checkpoint 错误，只允许既有兄弟工具提交终态，并在本地拒绝新的请求准备，避免 `A V1 Provider request is already active` 覆盖根因。
- 会话域将活动 API 失败、SystemError 与 Turn 关闭合并为单 revision 原子事务；成功 ApiStats 后的宿主投影失败则保留原终态并追加独立、可重载的失败事实。
- 终态持久化首次存储失败时重试同一脏修订；可靠落盘前保持精确准入 owner，从而阻止不安全 Continue 或重复工具副作用。
- 复用现有 V1 DisplayNote 和 Continue 选择器：错误正文直接可见，`Forbidden` 终态不会产生孤立“继续”入口。

### 6.2 📂文件变更清单

新增文件：

- `docs/file-tool-changeset-and-failure-lifecycle.md`
- `docs/reviews/20260910-file-changeset-v1-failure-recovery/AUDIT_REPORT.md`
- `docs/reviews/20260910-file-changeset-v1-failure-recovery/FILE_COVERAGE.md`
- `docs/completed/20260910/04-FILE_CHANGESET_V1_FAILURE_RECOVERY.md`

修改文件：

- `crates/vulcan-host-tools/Cargo.toml`
- `crates/vulcan-host-tools/src/file_runtime.rs`
- `crates/vulcan-core/src/types.rs`
- `crates/vulcan-core/src/controller/tool_execution.rs`
- `crates/vulcan-core/src/controller/tests/tool_batches.rs`
- `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge.rs`
- `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge/tests.rs`
- `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_stream_executor.rs`
- `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_retry.rs`
- `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_stream_runtime.rs`
- `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_execution.rs`
- `crates/vulcan-agent-runtime/src/tooling/native/tests/file_tools.rs`
- `crates/vulcan-session-core/src/v1/document/recovery.rs`
- `crates/vulcan-session-core/src/v1/document/document_tests/api.rs`
- `crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs`
- `crates/vulcan-service-kernel/src/agent_stream/stream_task.rs`
- `crates/vulcan-service-kernel/src/agent_stream/stream_task/finished.rs`
- `crates/vulcan-service-kernel/src/agent_stream/detached_subagent.rs`
- `crates/vulcan-service-kernel/src/context_compaction/streaming.rs`
- `crates/vulcan-service-kernel/src/tests/session_kernel_tests.rs`

删除文件：无。原计划文件通过迁移进入完成目录，不作为内容删除。

### 6.3 💻关键代码调整详情

- `whole_file_create_payload` 与 `whole_file_modify_payload` 明确分离生命周期和修改表示；`commit_validated_changes` 在重验文件前置条件与事务提交前执行规范解析。
- `RuntimeEventProjectionReceipt::Rejected` 与 `RuntimeEventProjectionError` 保留精确拒绝详情；根流、分离子任务和压缩流统一使用带原因拒绝。
- `CoreError::ToolDurabilityFailure` 在单工具、批次准备和并发工具路径中直接传播；并发路径保留第一个致命错误并汇合全部已启动 Future。
- `SessionDocument::fail_execution_boundary_and_close_turn` 在隔离候选上完成 API 失败与回合关闭，合并私有 revision 回执后才替换权威文档。
- `ProviderV1StreamSink` 一次性转移 API 终态是否已包含失败的强类型标志；`v1_api_terminal_satisfies_report` 判定是否还需追加宿主失败边界。
- `persist_v1_execution_failure_boundary` 对同一脏修订执行最多两次物理持久化尝试，`settle_stream_terminal_authority` 仅在终态可靠后释放 owner。

### 6.4 ⚠️遗留问题与注意事项

- 未改写用户日志中的历史失败会话，也未根据现存文件推测旧工具是否成功；历史原始错误若当时未落盘，无法事后可靠恢复。
- 未调用真实 DeepSeek 路由或启动对应桌面构建做真机验收；当前结论来自真实原生文件工具返回值、可控 Provider/Core 夹具、V1 会话持久化故障注入和 WebUI 定向测试。
- `vulcan-service-kernel --lib` 全量测试进程曾无输出长时间未结束并被中止；同一目标中的本任务 8 项精确测试均通过。
- 其他会话范围仍有 Agent Runtime 3 项供应商目录/回放路由失败，以及 Service API 1 项 VCE 快照失败；这些不涉及本任务文件，未越界修改。
- 工作期间其他会话将 HEAD 从 `9b77e5fb286e496c354d7c408320c6687f9799c2` 推进到 `19eb29408d3f8fcbebfd1633f0e8dfa7ba708139`。基线推进后已重新编译并重跑本任务验证；本会话没有执行暂存、提交或推送。
- 最终验证：7 个受影响 crate 的 `cargo check --locked` 通过；Host Tools 105 项、ChangeSet 12 项、Core 43 项、Session Core 284 项、Agent 本任务 7 项、Service 本任务 8 项、WebUI 27 项均通过；19 个 Rust 文件格式检查和精确差异检查通过。
- 审核门槛：经历十二次问题驱动归零后，最终连续 5 轮完整审核均未发现问题。
