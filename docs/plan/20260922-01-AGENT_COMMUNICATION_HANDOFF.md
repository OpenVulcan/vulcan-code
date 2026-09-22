# 代理协作剩余实现交接方案

日期：2026-09-22；当日序号：01。状态：检查点交接，产品目标尚未完成。

## 一、交接目的与权威顺序

用户要求先保存当前实现的提交点，再由另一模型完成剩余任务，原主代理审核。此文是执行入口，不是重新设计，也不缩减原目标。不继续使用未经统计的“70%”。当前连续完整无问题审核为 **0/5**。

阅读顺序：

1. 本文，明确边界和下一步。
2. [当前状态及有限剩余清单](../architecture/agent-communication/IMPLEMENTATION_STATUS.md)。
3. [原正式方案](20260920-04-AGENT_COMMUNICATION_AND_DYNAMIC_CONTEXT.md)：完整 P00—P12、R01—R14、A01—A18 保持有效。
4. [消息与工具契约](../architecture/agent-communication/MESSAGE_CONTRACT.md)、[生命周期与持久化](../architecture/agent-communication/LIFECYCLE_AND_PERSISTENCE.md)、[动态上下文与协议投影](../architecture/agent-communication/DYNAMIC_CONTEXT_PROJECTION.md)。
5. [验收矩阵](../architecture/agent-communication/ACCEPTANCE_MATRIX.md)定位现有证据；按需查[实施日志](../architecture/agent-communication/IMPLEMENTATION_LOG.md)，不要从头重复整个调查。
6. 只有需追溯候选原因时再看[原研究](20260917-03-SUBAGENT_CROSS_SESSION_INTERACTION_RESEARCH.md)。正式需求及最新用户修订优先于历史候选。

仓库按需约定也随检查点保存原文：[新增系统供应商流程](../新增系统供应商流程.md)、[Provider Manifest 作者手册](../provider-manifest-authoring.md)、[Ask 与 Markdown 交互约定](../Ask与Markdown交互约定.md)。涉及对应模块前按 `AGENTS.md` 读取；这些是原有参考资料，本次没有修改内容。

上述文件随检查点提交，接手者不依赖原会话上下文。历史日志中的“未实现”“正在测试”和百分比是当时快照；先对照最新状态和源码，不能因此重复实现。

## 二、检查点与安全边界

- 检查点标签：`checkpoint/agent-communication-20260922`。标签指向代码及本方案同一提交；实际哈希在交付回复中给出。创建后不得移动标签或修改该提交。
- 检查点父提交：`4f55dacc`，已包含独立 VCE 提交和更早用量分离提交。这些是依赖基线，不属于本任务产出。
- [检查点文件清单](../architecture/agent-communication/CHECKPOINT_FILES.txt)列出此次精确纳入的文件；不包含构建产物、用户数据库、凭据、整个第三方仓库。
- **这不是全绿版本。** 故意保留一个刚完成复现的失败回归，见第三节。不得删除、忽略或反转断言伪装通过。
- 正式目标当前暂停用于交接，未完成。接手者实施；原主代理负责审核最终差异和证据，审核前不归档正式计划。
- 建议从标签新建隔离工作树和 `codex/` 分支后实施，避免与当前工作区及并行工作混写。检查点引用用标签，不依赖不断变化的 `HEAD`。
- 不执行 `reset --hard`、`clean` 或整树覆盖恢复。需要回退时优先在新的工作树打开检查点进行比较；有新改动先另存，再由原主代理决定精确回退。
- 每次只做一个下述工作包。完成后保存中文结构化提交，正文写工作包、原因、验证和剩余项；不合并无关改动，不推送、不自行合并回主工作区。
- 禁止修改 VCE 存储/索引、浏览器标签/导航/独立调试窗口和独立用量统计设计。共享文件只改本任务明确区域。缺少证据时查明来源，不增加猜测性回退。
- 当前版本未发布：不增加旧格式迁移、兼容读取或多协议自动降级。非 V1 独立运行时是现存执行入口，不能当作历史兼容直接删除。

决策理由：保存真实中间状态和已知失败，比先临时修到表面通过更方便定位后续模型究竟改了什么；小工作包和独立提交方便原主代理审核及恢复。

## 三、第一工作包：S1.1 历史编辑保护（确认存在的缺陷）

### 已确认事实

文件以 `crates/vulcan-service-kernel/src/` 为前缀：

- `conversation_generations_v1.rs::service_edit_conversation_generation_v1` 是生产历史编辑入口，直接调用 `SessionHandle::publish_edit_generation`。
- `session_runtime_registry_v1.rs::publish_edit_generation` 保护当前文档写入及代次；Core `v1/document/generation.rs` 检查本地未结协作。
- 目标已经发布结算、来源尚未持久确认时，目标本地未结条件为假；编辑会清空当前协作状态，丢失来源恢复需要的回执。
- 删除已在 `session_v1_lifecycle/deletion.rs` 与 `deletion/settlement.rs` 保护该窗口；不得为编辑重新设计另一套消息账本。

### 已运行复现

```powershell
rtk cargo test -p vulcan-service-kernel --lib history_edit_preserves_unacknowledged_target_settlement -- --test-threads=1
```

2026-09-22，进程 45471 已退出：**0 通过，1 失败**。实际错误：

```text
History edit discarded the peer settlement receipt:
Ok(SessionEditGenerationReceipt { target_seq: Seq(1), segment_generation: 2, document_revision: 5 })
```

回归位于 `agent_collaboration/session_wait/tests/lifecycle.rs`；`unacknowledged_settlement_fixture` 使用实际 SQLite、真实根提交和领域转换，复现目标已结算而来源确认缺失。当前回归直接调用生产底层发布方法，尚未覆盖新的受保护服务入口，因为后者还没实现。

### 实现要求与建议

1. 阅读现有删除全过程、来源确认校验、生命周期锁、规范句柄和编辑发布调用点。
2. 将删除已有的精确物理树、来源及来源根、有序锁、集合复查、本地未结和可靠来源确认准入提取为可供编辑复用的内部入口。模块是否改名由实际结构决定，避免机械大改。
3. 编辑持有同一准入直到同步发布完成；核验传入句柄与规范根所有者一致。不让编辑封存或删除后代。锁必须在后续工作台刷新和模型启动前释放。
4. 生产服务入口实际调用受保护入口。测试也切换到这一入口，不能只修改测试专用包装。底层原子发布仍负责代次/写入校验。
5. 缺少来源确认时拒绝编辑，代次、目标回执及文件保持不变；正常原结算入口修复确认后，同一合法编辑应成功。新增正向断言，防止“永久禁止编辑”伪修复。
6. 冷态和嵌入树语义沿用已完成删除保护，保留正常初始化；外部冷来源仅读取，不注册半初始化句柄。禁止按标题猜目录，禁止把编辑失败变成偷偷撤销问题。
7. 若需改内部错误名可局部调整，但不借此更换公开协议或全局锁方案。现有删除行为不得退化。

验收：原失败回归转绿；补确认后编辑成功及可靠存储断言；重跑 `archived_target_deletion_` 两项与 `session_v1_lifecycle::tests` 组；服务生产检查通过。记录每次失败的真正原因，不弱化正确断言。

## 四、后续工作包与完成条件

### S1.2—S1.3 生命周期剩余组合

- 范围：`agent_collaboration/`、`agent_collaboration_execution/`、`conversation_run_services/`、`webui_prompt_lifecycle/`、`agent_stream/stream_task/`，按实际调用点收窄修改。
- 先对照已有测试和 A08—A12、A17。已通过的“归档、冷注册表、用户撤销、两个删除顺序、迟到撤销”不重新列为开发项。
- 补归档后实际模型工具回答的服务链证据；检查停止、MainOnly 子到主例外、仅答疑结束、迟到问题/答案不复活旧任务。用真实服务派发及领域落盘验证，不能只断言一个谓词。
- 业务最终完成只受所属子任务影响；问题和通知未回复不阻塞业务完成。MainOnly 例外不解除 Goal 暂停，不新建父业务任务；全树停止不自动恢复旧任务。
- 交付逐场景表：已有证据、真实缺口、变更、最终命令及结果。没有缺口就记录依据，禁止为增加测试数改架构。

### S2.1 独立运行时压缩消费（明确接线缺口）

- 入口：运行时 `agent_runtime/prompt_compaction.rs`、`task_runtime/runtime_execution.rs`、`provider_v1_bridge.rs`、`provider_request_telemetry.rs`。
- 已有 `PromptCompactionOwnership::{SessionV1, RuntimeOnly}`；V1 消费不得绕过可靠准备/观测/终态。
- 当前独立压缩消费只处理部分事件；本地路径可能丢弃屏障，远端仍有旧一元消费路径。沿实际事件类型和消费者查全，再决定明确的独立执行确认责任。
- 不允许仅将 V1 请求一律确认成 `ExcludedNonSession`：现有准备模板仍声明压缩用途，必须使请求、用途、消费者和确认事实一致。
- 不新增无消费者事件；不通过无限等待、静默失败或移除可靠屏障来消除死锁。
- 验收：受支持独立入口正常完成；拒绝/消费者消失时有限失败；V1 真实子维护仍正常落盘。复用现有夹具，必要测试精确覆盖所有权分支。

### S2.2—S2.3 原生压缩诊断和负向链

- 入口：运行时 `prompt_compaction/remote.rs`、`provider_transport_vulcan.rs`；适配器 `execution/responses_compact.rs`；服务 `agent_stream/provider_v1_sink/remote_checkpoint.rs`、`context_compaction/streaming.rs`。
- 原生物理尝试已有准备许可 → 单次 HTTP → 响应头确认 → 专用终态确认；重试先结算失败 API，再刷新同账号并使用新许可。
- 补实际请求诊断制品，复用普通请求现有追踪机制，默认不复制敏感正文、凭据；不另造整套日志平台。
- 补真实 401/刷新/第二次尝试及准备或可靠确认失败阻断证据，补主会话实际消费入口。每次物理请求身份独立且结果只能归其许可。
- 已通过子维护本地摘要和原生 HTTPS 冷加载两项。原生夹具 `codex-compaction-fixture` 是经真实发现/激活的内存直接工具模型。
- 当前内置 Codex 代码模式模型缺少 `codex_javascript_host_v1`，守卫正常拒绝。不得修改生产模型声明、跳过守卫或声称该测试证明内置模型可用；新增 JavaScript 宿主不属于本交接任务。

### S3.1—S3.3 动态上下文与协议

- 入口：`vulcan-context-injection`、运行时 `context_injection_runtime.rs` / `provider_runtime_context.rs` / `provider_system_update_lowering.rs` / `runtime_stream_launch_payload.rs`、Core 动态上下文/投影、服务 `provider_v1_sink/context_injection.rs`。
- 逐生产者检查 Goal、Plan、VMM、环境、临时指引、继续/摘要提示和 Hook；动态文本统一按发生点追加带标签 User，固定顶部稳定。真实用户、工具及媒体保留原生结构。
- 用相邻两次实际请求验证前缀；更新和 TTL 只追加，压缩才剔除失效正文并重建有效状态。不要逐请求删除旧正文或把动态内容重新移到 System/Developer 顶部。
- 九 Profile 编码已有证据；补当前真实可达生产路由表、工具配对/并行结果/媒体交错与合法插入边界。单次编码器夹具不冒充完整装配证明。
- continuation 的真实字节/角色/顺序安全判定、重建原因及诊断一致；已有 `runtime-host:` 延续保护不可未经证明删除。未知当前契约版本明确拒绝。
- 验收归入 A01—A03、A13—A15；不要求真实付费供应商调用，未验证真实缓存命中就明确说明。

### S4.1—S4.4 协作、授权、工具及界面

- 范围：服务 `agent_collaboration/{participants,session_messages,session_wait,notifications}.rs`、运行时 `tooling/collaboration.rs`、WebUI 协作卡、两条会话时间线及事件窗口。
- 先查现有证据，再补双向互问、答案先到、多等待者、超时重等、恢复与通知丢失。等待不占关键锁，不消费他人唯一答案；超时不撤销。
- 逐动作检查目录、读取、接纳、回复和实际正文交付权限。项目和父子身份来自宿主，不信标签。授权撤销后的迟到交付按冻结设计处理。
- 工具链从注册/Schema、模式可见性、策略过滤、准备、执行到结果投影逐段核对；专用代理问答不通过开放通用人类 Ask 代替。
- 独立卡片区分用户与代理消息、问题/通知/任务结果；核对分页、重连和多窗口，多个窗口不抢唯一消息。别改用户未要求的浏览器界面。
- 验收归入 A04—A12、A14、A16、A18；前端类型检查及相关定向测试即可，必要时再补实际交互证据。

### S5 总验收与主代理审核

- 逐项关闭原 P00—P12 / R01—R14 / A01—A18：每行必须指向当前源码和具体证据，写明未验证边界。原计划未勾选不代表未实现，但不能无证据批量勾选。
- 汇总最终字段唯一来源、Schema、实际路由、用户使用说明；刷新状态入口，保留旧日志作为历史证据。
- 实施模型完成每包后提供下面的交付模板，由原主代理复核；不得把实施模型自称“审核通过”计入最终五轮。
- 原主代理完整审核五轮，每轮覆盖整体契约、生命周期、可靠恢复、协议、授权、UI 及验证缺口，可有不同重点但不能把五个局部专项冒充五轮完整审核。
- 新发现真实问题时修复、复验并将连续无问题计数归零。无需为了计数无意义重复全量测试。
- 只有全部实现、验收和连续五轮完整审核完成，写完执行变更总结后才按仓库约定归档原计划；不能在本检查点阶段结束总目标。

## 五、实施方法与验证约束

- 严格先源码事实后实现，搜不到字段/所有者先查调用链；禁止多候选字段兜底和静默降级。
- 每包开始记录实际基线、待改精确文件和验收条件。每个决策写“事实 → 选择 → 原因 → 影响范围”。不要复述几百条旧日志。
- 所有终端命令前缀 `rtk`；PowerShell 内建用 `rtk proxy powershell -NoProfile -Command`。Windows 的 `rg` 文件名过滤用 `--glob`，不向不存在路径或通配符操作数盲查。
- 代码注释中英文，英文在前；AI 工具描述及错误英文；计划和用户报告中文。
- Cargo 保留 jobs=4，涉及共享资源的测试 `-- --test-threads=1`。不要同时编辑 Rust 和运行编译，也不要并发启动多个大链接。
- RTK 大测试链接可能数分钟无输出，应轮询原进程，不重启同一编译。退出码及最终结果确认后才标记完成。
- Rust 格式化仅显式修改文件：`rtk rustfmt --edition 2024 --config skip_children=true <文件>`，不全库格式化。
- 按风险选定向测试，修改共享实现时回归相关调用链；禁止默认工作区全量测试。资源变动只改权威资源目录并显式 `make.ps1 sync`，没有资源变动不做无关同步。
- 仅在测试假设与真实类型/业务不符时修夹具，写明证据；不能放松生产守卫以让测试通过。

### 已有可复用命令

在仓库根：

```powershell
rtk cargo test -p vulcan-service-kernel --lib history_edit_preserves_unacknowledged_target_settlement -- --test-threads=1
rtk cargo test -p vulcan-service-kernel --lib archived_target_deletion_ -- --test-threads=1
rtk cargo test -p vulcan-service-kernel --lib session_v1_lifecycle::tests -- --test-threads=1
rtk cargo check -p vulcan-service-kernel
rtk git diff --check
```

后续测试先 `rg` 实际测试名。WebUI 在 `apps/vulcan-code-web` 中使用既有 `rtk pnpm exec vitest run <精确文件>` 和 `rtk pnpm exec tsc -b --pretty false`，不要猜脚本名。

## 六、每包交付及审核模板

1. 工作包编号、起止提交、原需求编号。
2. 核心修改与原因；精确新增/修改/删除文件。
3. 原缺陷复现与修复证据；命令、退出码、通过数量、代码版本。没有执行的明确写未执行。
4. 是否改动模型可见契约、持久字段、授权、线程/锁顺序；原因与对应设计更新。
5. 已知失败和剩余项；禁止将历史旧失败当当前失败，或将已知当前失败省略。
6. 原主代理审核结论、问题编号及修复提交。实现完成不等于审核通过。

## 七、可直接交给接手模型的任务文本

> 请在检查点 `checkpoint/agent-communication-20260922` 的隔离工作树中继续代理协作与动态上下文实现。先读 `docs/plan/20260922-01-AGENT_COMMUNICATION_HANDOFF.md` 及其必读链接，严格按 S1.1 → S1 其余 → S2 → S3 → S4 顺序逐包完成，每包先查事实、定向修改、验证并保存中文结构化提交。第一步修复已确认失败的 `history_edit_preserves_unacknowledged_target_settlement`，不得忽略或削弱该测试。保持原全部需求，不兼容历史数据，不改 VCE、浏览器界面或用量统计，不推送或合并。每包提交验收证据和决策原因给原主代理审核；不自行宣布五轮审核完成或归档原计划。按本方案处理可确定的工程细节，发现需突破冻结架构或并行边界的问题时报告精确证据，不猜测实现。

## 八、本次交接变更总结

1. **核心调整：** 暂停继续修功能，保存现有代码、已知失败、正式方案与实施状态；建立可逐包执行和审核的交接入口。
2. **文件清单：** 新增本方案及检查点文件清单；更新实施状态、实施日志和原正式计划入口；其余代码按检查点原样保存，未实施第三节建议修复。
3. **关键事实：** 最新历史编辑回归实际失败；此前删除保护十九项及归档结算两项通过，最终服务生产检查零错误、十一项既有警告。最新失败测试编译已成功，不代表整体测试全绿。
4. **遗留与注意：** S1—S5 仍待闭合，五轮审核零轮；当前内置 Codex 代码模式原语限制仍存在。检查点用于恢复与比较，不是发布承诺；接手实施后由原主代理审核。
