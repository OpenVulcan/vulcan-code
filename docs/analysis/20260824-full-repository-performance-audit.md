# Vulcan Code 全代码仓库性能与冗余审核报告

> 状态：审核完成  
> 开始日期：2026-08-24  
> 审核基线：`main` 分支，开始时工作树干净

## 1. 审核目标

本报告聚焦性能、执行效率、重复转换、可上移到全局或边界阶段的一次性处理、明显冗余和无必要验证。审核只记录当前源码事实与建议，不直接修改产品实现。

## 2. 范围基线

- Git 跟踪文件：2,313 个。
- 纳入源文件审核：1,460 个，其中原始可执行代码 1,416 个，另补充 43 个 CSS 与 1 个生产 HTML 入口。
- AST 可解析代码：1,397 个，约 794,417 行；19 个 PowerShell 文件使用 PowerShell 7 解析器另行审核，CSS/HTML 通过导入归属、生产构建和预算检查审核。
- 覆盖等级和排除口径见同日计划与覆盖台账。
- 逐文件覆盖台账：`docs/analysis/20260824-full-repository-performance-audit-coverage.md`。
- 已归档执行计划：`docs/completed/20260824/01-FULL_REPOSITORY_PERFORMANCE_AUDIT.md`。

## 3. 方法与证据等级

- `S0`：清单确认。
- `S1`：结构审核和规则扫描。
- `S2`：关键实现及调用链审核。
- `S3`：通过交叉源码、测试、静态分析或基准完成验证。

问题置信度分为高、中、低；未达到 `S2` 的候选不会直接列为正式问题。

## 4. 审核进度

| 批次 | 范围 | 状态 | 已完成覆盖 |
| --- | --- | --- | ---: |
| 0 | 仓库清单与全局 AST 地图 | 完成 | 1,460 / 1,460 |
| 1 | 应用入口与基础核心 crate | 完成 | 182 |
| 2 | Provider、协议与配置链 | 完成 | 90 |
| 3 | Agent、Service、存储与知识库 | 完成 | 592 |
| 4 | WebUI | 完成 | 510 |
| 5 | 插件与脚本 | 完成 | 42 |
| 6 | CSS 与生产 HTML 入口 | 完成 | 44 |
| 7 | 跨仓模式与验证 | 完成 | 1,460 / 1,460 |

## 5. 已确认问题

### P-001：上下文压缩的每个输出增量都会重建并复制完整会话展示窗口

- 优先级：P1
- 分类：重复转换、重复分配、分支内全量处理、锁内重建
- 置信度：高
- 验证状态：源码调用链已确认；尚未进行真实 Provider 长会话基准
- 涉及文件：
  - `crates/vulcan-service-kernel/src/context_compaction/streaming.rs:35-128,352-434`
  - `crates/vulcan-service-kernel/src/context_compaction/draft.rs:89-159`
  - `crates/vulcan-service-kernel/src/context_compaction.rs:121-179`
  - `crates/vulcan-service-kernel/src/conversation_display.rs:77-92,446-478`
  - `crates/vulcan-context/src/protocol/projection.rs:596-605,813-898`

确认的执行链如下：

1. `stream_webui_context_compaction_execution` 逐个消费 Provider 流事件；每个 `OutputDelta` 都直接进入 `append_webui_context_compaction_output_delta`。
2. 该函数在持有全局 Service 状态锁时克隆完整 `SessionRuntimeStateEntry`，再调用通用的全量展示合并函数。
3. 全量合并会克隆并转换 `conversation_state.records`，构建多个集合，扫描并复制已加载消息；`push_loaded_session_message_once` 对累积结果反复线性查重，使该阶段最坏可达到二次复杂度。
4. 随后再次获取全局状态锁，克隆当前会话的完整 Workbench 消息窗口，把全部 Session 消息重新转换为 Conversation 消息，构建差异后再替换整个会话窗口。

该路径恰好在“历史已经足够长、触发上下文压缩”时运行，并按输出 delta 重复执行，因此总成本不是一次全量合并，而是 `增量数量 × 完整历史重建成本`。即使最终下发的是 patch，patch 生成前仍已经完成全部复制、转换和比较；较长摘要会进一步放大锁持有时间、分配量和 UI 更新压力。

建议把压缩正文流接入普通流式内容已有的增量投影模型：创建正文行时做一次完整定位，后续 delta 只更新该记录及已加载窗口中的对应行，并直接发送单行 upsert；在完成或失败边界再做一次权威全量合并。可增加合并阈值或按帧批量发送，并把该路径加入现有 `hot_path_scan_guard_tests.rs`，明确禁止 `runtime_state.clone()` 和 `merge_runtime_state_records_into_display_messages()` 回到逐 delta 路径。

### P-002：Provider 流记录在投影、用量提取和协议解码之间重复复制并多次解析 JSON

- 优先级：P1
- 分类：重复转换、重复解析、高频分支内处理
- 置信度：高
- 验证状态：源码调用链与系统 Manifest 配置已确认；尚未做各协议吞吐基准
- 涉及文件：
  - `crates/vulcan-provider-adapters/src/execution.rs:2679-3056,3144-3529,3602-3621,5014-5032`
  - `crates/vulcan-ai-protocols/src/shared.rs:86-124`
  - `crates/vulcan-ai-protocols/src/openai_stream.rs:474-513,1509-1798`
  - `crates/vulcan-ai-protocols/src/gemini.rs:383-407,777-818`
  - `crates/vulcan-runtime-assets/resources/provider-manifests/manifests/*.json`

确认的重复工作分为三层：

1. HTTP SSE 的每条 frame 都调用 `project_response_record_text`。当没有响应投影配置时，该函数仍用 `text.to_string()` 复制整条记录；当前多数会话路由的 `response_profile_refs` 为空，因此这是常规路径而非特殊分支。
2. 启用了 Manifest 用量提取的路由会在 `extract_profiled_usage_events` 中把同一 `projected_data` 解析成 `serde_json::Value`，随后协议解码器再次 `serde_json::from_str`。DeepSeek、OpenRouter、TokenRhythm、OpenCode Go 和部分 Alibaba 会话路由存在 SSE 用量配置。
3. Responses WebSocket 路径已经持有解析后的 `frame.value`，但 `ProtocolStreamDecoder::push_json` 的默认实现又把它序列化为字符串并调用 `push_sse`，OpenAI Responses 解码器随后重新解析。Google Code Assist 包装器也会解析外层 JSON、序列化 `/response` 子树，再交给 Gemini 解码器解析一次。

因此常规 HTTP SSE 至少存在一次无必要字符串分配；启用用量提取时同一 frame 至少解析两次；启用响应投影或包装协议时还会增加一次解析和一次序列化。该成本按 Provider frame 数量线性累积，位于所有正文、思考和工具流事件之前。

建议分两阶段处理：第一阶段让无响应投影分支返回借用值（例如 `Cow<'a, str>`），立即消除所有未配置投影路由的逐 frame 复制；第二阶段建立一次解析的流记录结构，使响应投影、用量提取和协议解码共享同一 `Value`。`ProtocolStreamDecoder` 应提供真正消费已解析 JSON 的实现，WebSocket 不再走 `Value -> String -> Value`；Google Code Assist 直接把借用的 `response` 子值交给 Gemini 解码逻辑。第二阶段涉及协议接口调整，实施前应补充 OpenAI Chat、Responses、Anthropic、Gemini、Cohere、Interactions 的等价性测试与吞吐基准。

### P-003：每次 Provider 请求编码前会完整克隆大型执行请求至少两次

- 优先级：P1
- 分类：重复复制、重复分支处理、请求热路径
- 置信度：高
- 验证状态：源码所有权和调用链已确认；尚未量化长上下文下的复制字节数
- 涉及文件：
  - `crates/vulcan-provider-adapters/src/execution.rs:2163-2185,2232-2240,2256-2299,2581-2675,3144-3166,3664-3703`
  - `crates/vulcan-ai-contract/src/request.rs:107-174`

`ProviderExecutionClient::execute` 已经取得 `AiExecutionRequest` 所有权，并把它移动到唯一选中的 HTTP 或 WebSocket 分支；但后续没有利用这份所有权：

1. `request_without_nonpersistent_responses_continuation` 无条件执行 `request.clone()`，只为可能把一个 `continuation` 字段设为 `None`。
2. `responses_websocket_request_for_active_session` 同样先完整克隆请求，再按一个布尔条件清空同一字段。
3. 上述副本随后传给 `encode_request_with_credential`；其内部 `encode_request` 再次执行 `let mut baseline_request = request.clone()`，仍然只为按协议清空 `continuation`。

`AiExecutionRequest` 不只是小型控制结构；它拥有完整 `messages`、工具定义、原生工具、停止词、模型参数和资源引用。长会话下，重复克隆会复制整份消息和工具上下文，而真正的编码紧随其后还要再次遍历这些数据生成 JSON。HTTP 常规路径因此在每次物理 Provider 请求前至少产生两份完整请求副本；WebSocket 活跃会话路径也存在同样的双重复制。

建议把“是否投影 continuation”变成一次边界决策，而不是通过多次改写完整请求实现。低风险方案是在取得所有权的 HTTP/WebSocket 分支内只移动一次请求并原地调整 `continuation`，随后使用消费型内部编码入口；若公共 `encode_request(&AiExecutionRequest)` 必须保留用于预览，则让底层编码器接收 continuation 覆盖值或借用视图，避免为了隐藏单个字段克隆全部载荷。应增加一个包含大消息列表和工具 Schema 的基准，分别测量当前双克隆、单次原地调整和借用视图方案。

### P-004：每个临时插件租约都会重新读取并重复验证全局桥接代码和插件包代码

- 优先级：P2
- 分类：重复 I/O、重复路径验证、可上移到全局或插件代际的处理
- 置信度：高
- 验证状态：源码调用链已确认；实际插件调用频率和磁盘缓存命中率待运行量化
- 涉及文件：
  - `crates/vulcan-plugin-runtime/src/runtime.rs:3138-3295,3335-3434,3605-3690,3850-3979,4086-4273,6401-6539`

`create_managed_lease` 被探测、生命周期、Action、无状态工具、持久 UI Runtime 和有状态工具共同调用。对于不复用 Runtime 的普通 Action 和无状态工具，每次调用都会：

1. 在调用者中规范化 `workspace_root`，进入 `create_managed_lease` 后再规范化同一路径。
2. `plugin_execution_package_root` 已经把包固定到已授权代际，`create_managed_lease` 仍再次规范化包根和 bundle 文件。
3. 每次从磁盘重新读取插件 `bundle_source`。
4. 每次重新规范化全局 `bridge_package_root` 和固定 `runtime/bridge.lua`，检查固定 `dependencies.yaml`，再从磁盘重新读取相同 `bridge_source`。
5. 把两份源码重新装入新租约；其中创建新 Lua 租约是隔离语义的一部分，但读取和验证不变资产不是。

全局桥接包在 Manager 生命周期内不随单次调用变化，插件 bundle 也已经由包代际和内容指纹固定。当前实现把本可在 Manager 初始化和插件代际发布时完成的磁盘解析、规范化与读取，放回每次临时租约分支。操作系统文件缓存只能减少物理磁盘访问，不能消除路径解析、字符串分配和源码复制。

建议在 Manager 初始化时构建经过验证的全局桥接资产（规范根、模块路径、依赖文件状态和 `Arc<str>` 源码）；在插件目录发布时按 `plugin_id + generation + content_fingerprint` 缓存经过验证的执行资产。租约创建只借用这些冻结资产，并只对调用特有的 workspace 权限做一次规范化。为保留安全边界，可使用不可伪造的内部类型表达“已规范化且已验证”，而不是在相邻函数重复 `canonicalize`；插件更新、禁用或代际切换时原子失效缓存。

### P-005：工作区文件树在异步 RPC 线程上同步遍历，并对每个条目重复规范化、取元数据和根路径字符串转换

- 优先级：P1
- 分类：阻塞异步执行器、逐条重复验证、重复系统调用、可由特殊分支处理替代全局处理
- 置信度：高
- 验证状态：源码调用链和 `ignore::WalkBuilder` 配置已确认；不同规模工作区的延迟尚未运行量化
- 涉及文件：
  - `crates/vulcan-service-kernel/src/rpc_dispatch/workspace_io.rs:47-151`
  - `crates/vulcan-service-kernel/src/workspace_files.rs:56-72,121-190`
  - `crates/vulcan-service-kernel/src/workspace_files/entry_summary.rs:42-66,105-185`
  - `crates/vulcan-service-kernel/src/workspace_files/path_security.rs:18-121`

`workspace.files.list` 从异步 RPC `dispatch` 直接等待 `service_workspace_files_list`，但该异步函数内部没有异步 I/O 或 `spawn_blocking` 边界，而是在当前执行器线程同步运行 `ignore::WalkBuilder`、`fs::canonicalize`、`fs::metadata`、目录递归和排序。开启“只显示可预览文件”后，每个当前层目录还会调用 `workspace_directory_has_previewable_descendant`，同步扫描该目录的任意深度后代，直到找到首个可预览文件或遍历完整子树。

扫描中的每个候选条目都会进入 `workspace_visible_file_entry_metadata`：先 `canonicalize`，再单独 `metadata`；Windows 下的根内判断还会把条目路径和同一个工作区根路径各自规范成字符串并转小写。`WalkBuilder` 已配置 `follow_links(false)`，普通非符号链接条目天然来自既定根下，却仍承担为防止符号链接逃逸而设置的全量规范化成本；固定根的字符串标准化也在每个条目上重复执行。对于大目录和预览过滤失败的子树，这些成本会集中阻塞 Service RPC 执行器。

建议先把完整枚举放到 `spawn_blocking`，避免文件系统延迟占用异步调度线程；随后让条目摘要直接接收 `ignore::DirEntry`，复用其文件类型/元数据能力。安全校验应保持，但按条目类型分支：符号链接继续规范化并验证根边界，普通条目使用从已验证起点派生的词法路径；Windows 根比较键在请求开始时只构建一次。预览后代判断可以在一次遍历中建立当前层目录的“含可预览后代”集合，或缓存到文件树快照，而不是把同步深扫隐藏在逐目录摘要函数内。验收应覆盖符号链接越界、大小写、隐藏文件、ignore 规则和 1 万/10 万条目目录基准。

### P-006：当前生产 WebUI 构建已经超过仓库自己设定的 Workbench CSS 预算

- 优先级：P2
- 分类：首屏/工作台资源体积、性能预算回归
- 置信度：高
- 验证状态：从干净源码重新执行生产构建并复跑预算检查，稳定复现
- 涉及文件：
  - `apps/vulcan-code-web/scripts/check-build-budget.mjs:57-114`
  - `apps/vulcan-code-web/scripts/webui-build-budget.json:1-9`
  - `apps/vulcan-code-web/vite.config.ts:11-40`
  - `apps/vulcan-code-web/src/**/*.css`

2026-08-24 在 `apps/vulcan-code-web` 执行 `pnpm build` 成功，随后预算测量得到 `workbenchCss.raw = 445,710`、`workbenchCss.gzip = 66,506`；仓库提交的上限分别是 `440,000` 和 `66,000`，两个维度都失败。失败不是旧产物造成：生产构建刚生成的 `WorkbenchApp-*.css` 同样是 445.71 kB / 66.51 kB。类型检查和架构测试均通过，因此这是独立的资源预算回归，而非构建失败的伴随现象。

当前超限幅度约为原始体积 1.30%、gzip 体积 0.77%，尚不适合仅凭体积断定具体用户侧延迟，但它已经突破项目设置的回归护栏。建议先用 CSS 覆盖率和重复选择器/声明分析找出贡献最大的规则，优先核对 `skill-cards.css`、`settings.css`、`special-blocks.css`、`session-tools.css` 等大文件是否全部被 Workbench 主块静态引入；只有独立懒加载界面使用的样式应随对应异步块拆分。修复后保持预算不放宽，并以生产构建后 `pnpm check:build-budget` 通过为验收条件。

后续导入链核对已确认一个直接成因：`WorkbenchApp.tsx` 静态导入唯一的 `styles.css`，该文件再静态导入 shell、sidebar、workspace、session tools、skills、memory、conversation、composer、settings、visual lab、Markdown、特殊块、工具展示、media、plugin host、terminal 等全部样式。即使 Agent Skills、Knowledge、Memory、Settings 等功能 JavaScript 已拆为延迟 chunk，其显式样式仍统一进入 Workbench CSS；因此“全局静态样式聚合”已确认，重复选择器占比仍待工具量化。

### P-007：会话渲染为稳定集合/数组身份而反复执行完整 JSON 序列化再反序列化

- 优先级：P2
- 分类：重复转换、渲染期线性分配、以序列化代替值稳定器
- 置信度：高
- 验证状态：React 派生链与增量正文旁路已交叉核对；实际长会话渲染耗时待 Profile
- 涉及文件：
  - `apps/vulcan-code-web/src/components/conversation/chat-area.tsx:314-332`
  - `apps/vulcan-code-web/src/components/conversation/conversation-workspace.tsx:229-288`
  - `apps/vulcan-code-web/src/components/features/skills/lua-skills-settings-panel.tsx:204-213,296-312`（低频同类）

`ChatArea` 先把隐藏思考块 `Set` 投影为排序数组，再对完整数组 `JSON.stringify`，随后 `useMemo` 中 `JSON.parse` 回数组并构造新 `Set`。`ConversationWorkspace` 对压缩边界前的全部 action key 和全部 turn key 各执行一组同样的 `stringify -> parse`；其中 turn 数量随会话持续增长。这样做的目标是当上游投影对象重建但业务键值未变时保持集合/数组引用稳定，目标合理，但实现会在每次组件渲染时至少线性扫描并分配签名字符串，值发生变化时还要再解析和复制一遍。

正常正文 delta 已由 `ConversationLiveStreamStore` 的链式片段和 `WorkbenchApp` 的 React 投影旁路处理，不会让该成本按每个 token 触发；因此本项不应与 P-001 等同定级。但工具事件、会话状态、折叠状态和其他本地交互仍会触发会话组件渲染，长会话会持续支付与全部 key 数量成正比的序列化成本。LuaSkills 设置页也通过序列化再解析整个配置来隔离快照对象，频率较低但属于同一冗余模式。

建议提供一个通用的 `useStableStringArray` / `useStableStringSet`：用 `ref` 保存上次结果，长度相同后逐项比较，值未变则复用旧引用，变化时直接保留当前数组或构造一次 Set；这样仍是必要的 O(n) 值比较，但不再生成 JSON 文本或反向解析。配置文档可使用服务端稳定 revision，或结构化字段比较。应以长会话 React Profiler 验证提交次数不增加，并为值相同但输入引用变化、顺序变化和集合变化补测试。

### P-008：知识库 Markdown 导入只把目录扫描移出异步执行器，正文读取、哈希、解码、分块和整库暂存仍集中在一个异步任务中

- 优先级：P1
- 分类：阻塞异步执行器、CPU 密集处理、整库内存聚合、批次边界过晚
- 置信度：高
- 验证状态：导入启动、扫描、处理和 SQLite 暂存调用链已确认；大知识库内存峰值与调度延迟待基准
- 涉及文件：
  - `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:2585-2681,4362-4653,4810-4855,7333-7347,7480-7526,7808-7910`
  - `crates/vulcan-workspace-store/src/knowledge_base/schema.rs:78-193`

导入入口使用 `tokio::spawn` 启动分离任务；目录枚举本身在 `scan_markdown_files_with_progress` 中正确使用 `spawn_blocking`。但枚举完成后的核心处理回到了 Tokio 异步任务：对每个 Markdown 文件同步 `fs::read`，同步计算 SHA-256、文本解码、换行规范化、标题与行数提取、`chunk_markdown`，然后把完整文档正文和每个分块正文都包装成 `ControllerSqliteValue`。

这些结果没有按文件或固定字节窗口及时写入，而是累计到 `staged_paths`、`staged_documents` 和 `staged_chunks` 三个全库向量；全部候选处理完成后才一次性交给 `stage_import_rows`。因此大目录导入同时存在两个放大机制：文件 I/O 和 CPU 分块会占用 Tokio worker，影响同一 Runtime 的其他异步工作；所有变更文档及其分块正文会在提交前共同驻留内存，峰值随本次变更知识库总量增长，而不是随单个批次增长。后续 `execute_library_batch` 即使内部有数据库批处理，也已经无法降低调用前的整库内存峰值。

建议把“读取/哈希/解码/分块”变成 `spawn_blocking` 中的有界生产者，以文件数或正文总字节数形成批次；异步消费者收到批次后立即写入 staging 表并释放正文，最后再执行现有原子 commit。扫描路径也可边扫描边批量暂存，避免额外保留全部 path 行。通道必须有界以形成背压，并保留当前取消标志、100ms 进度节流和 staging 事务语义。验收应至少覆盖全未修改、部分修改、取消、单文件超大、海量小文件和数据库失败回滚，并记录执行器延迟与峰值 RSS。

### P-009：全目标 Clippy 暴露出成批可证明冗余的 clone/collect，其中部分位于会话、流式边界和大型结果载荷路径

- 优先级：P2（聚合清理项；单点多为 P3）
- 分类：冗余复制、无效转换、明显冗余表达式
- 置信度：高
- 验证状态：`clippy::redundant_clone`、`clippy::clone_on_copy`、`clippy::needless_collect` 等静态规则已运行；高价值样本已读取源码确认
- 代表文件：
  - `crates/vulcan-core/src/controller/metadata.rs:3-16`
  - `crates/vulcan-core/src/controller/transcript.rs:105-132`
  - `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_stream_runtime.rs:621-714`
  - `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_delta.rs:712-739`
  - `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_results.rs:216-259`
  - `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/request_records.rs:195-241`
  - `crates/vulcan-context-compaction/src/rewrite.rs:170-188,578-580`
  - `crates/vulcan-host-tools/src/file_runtime.rs:72-82,1002-1022`
  - `crates/vulcan-provider-adapters/src/graph.rs:93-112`

工作区全目标静态分析输出了 195 条与本审核方向匹配的提示；该数字包含测试代码和重复构建目标，不能直接当作生产缺陷数量。生产路径中仍存在一批工具可证明不改变语义的冗余复制：例如用户回合把附件 Vec 克隆后再放入唯一消息；`json!` 宏本身借用并序列化字段，多个调用点却提前克隆 String/Value；上下文压缩计划把马上借用的 `tail_selection` 再克隆；`Option<ReasoningEffort>` 是 `Copy` 类型却调用 clone；文件工具先把切片 collect 成 Vec，紧接着再次截断 collect。

影响较大的样本位于 Agent 状态边界：上下文长度错误时完整 `conversation_state` 被冗余克隆，随后同一 controller snapshot 又重新转换一次；`runtime_stream_delta` 接收已拥有的 conversation state/reason 后仍克隆进状态；缓存子代理结果为 model/host 两份 JSON 时，对可能很长的 `result_text` 和多个字符串先做了没有必要的 Rust String clone。它们不是正常 token delta 的主路径，但会放大本就重载的压缩、恢复、任务结果和持久化边界。

另有 `CredentialAuthorization` 因 `BrowserCallback` 大字段使整个枚举至少 296 字节，以及文件工具 `1 * 1024 * 1024` 的无效乘法等低优先级提示。前者目前主要存在于小规模配置/注册表，不应在没有测量的情况下贸然装箱；后者只需机械清理。

建议建立两批修复：第一批只接受 Clippy 可证明的无语义变化（移除 redundant clone、Copy clone、无效 collect/表达式），按 crate 分小提交并运行定向测试；第二批单独处理需要重排所有权的完整状态转换，例如在错误边界只转换一次 snapshot，再按两个事件的实际所有权决定一次必要 clone。测试代码中的 clone 只在明显降低测试内存/噪声时清理，不与生产热路径混为一谈。可在 CI 为生产 lib/bins 启用这些规则为 deny，但在历史告警清零前不要直接对全目标一刀切。

### P-010：运行时资源同步每次都完整枚举并覆盖复制近 100 MiB 载荷，没有任何未变化快路径

- 优先级：P2
- 分类：开发/发布效率、全局无条件处理、重复 I/O
- 置信度：高
- 验证状态：脚本控制流与当前源载荷规模已确认；未执行实际同步计时以避免改写 output
- 涉及文件：
  - `scripts/sync_tauri_runtime.ps1:232-302`
  - `scripts/sync_tauri_runtime.sh:115-156`

PowerShell 的 `Copy-DirectoryContents` 先用递归 `Get-ChildItem` 把完整源树物化成数组，再逐项创建目录或调用复制 helper；Shell 对等实现直接执行 `cp -R`。两者都没有比较目标的大小、摘要、版本或代际。主流程对 config、runtime resources、系统技能、LuaSkills runtime、bundled tools 等依次调用，其中 bundled tools 和系统技能还先清空目标。按当前工作树只读统计，这些输入合计约 908 个文件、99.7 MiB：runtime resources 5.6 MiB、LuaSkills runtime 22.7 MiB、bundled tools 54.0 MiB、插件约 9.0 MiB，另有其他小目录。每次显式 `make sync` 即使只改一个 Manifest，也会枚举并覆盖处理整套载荷。

这不是产品运行时热点，且项目约定 build/release 不隐式同步，所以定为 P2。但它直接拖慢资源、插件和运行时依赖开发循环，也增加目标文件被占用时的失败面。建议为每个独立资源根生成源代际指纹（稳定文件路径、大小和内容摘要），与 output 中上次成功代际比较；未变目录整体跳过，已变目录在 staging 中增量构建后原子发布。至少先把 54 MiB bundled tools 和 23 MiB Lua runtime 按版本/摘要跳过。不要只依赖修改时间，也不要牺牲当前锁文件容错与安全根检查。

### P-011：DAP sidecar 对分片帧每收到一个网络 chunk 都复制整个未完成缓冲区，最坏呈二次复制

- 优先级：P1
- 分类：重复复制、网络热路径、分支应局部处理却全缓冲处理
- 置信度：高
- 验证状态：接收、缓冲、解析及上限代码已确认；现有 4 个 sidecar 测试通过，但未覆盖大正文高碎片率
- 涉及文件：
  - `plugins/vulcan-debug-runtime/node/sidecar.mjs:20-30,276-310,350-401`
  - `plugins/vulcan-debug-runtime/node/sidecar.test.mjs:21-404`

DAP 单帧正文允许达到 16 MiB。`socket.on("data")` 对每个 chunk 执行 `Buffer.concat([state.dapBuffer, chunk])`，而 `parseDapFrames` 在正文未收齐时保留这份合并结果；因此一个被拆成 `k` 个网络 chunk 的大帧会反复复制此前所有字节，总复制量接近 `1 + 2 + ... + k` 个 chunk。以常见 64 KiB 分片估算，16 MiB 帧约 256 个 chunk，累计内存复制量可达到约 2 GiB 量级，尽管最终有效正文只有 16 MiB。

解析完成后通过 `subarray` 前移是零复制的，但无法抵消收齐前的累计复制。现有测试验证生命周期、真实 Node Inspector、DAP 基本流和超大 header 拒绝，没有构造接近上限且高度碎片化的正文。建议把接收缓冲改成 chunk deque/游标状态机：header 阶段只保留受限头部；解析出 Content-Length 后累计正文 chunk 与总长度，收齐时最多合并一次，或直接按目标长度写入预分配 Buffer。保留当前总缓冲上限和失败语义，并新增 1 字节/小 chunk 碎片、同一 chunk 多帧、半帧关闭和 16 MiB 边界基准。`enqueueEvent` 为计算大小先 JSON 序列化、最终响应再序列化的次要重复，也可在重构时缓存编码长度，但不应优先于缓冲算法。

### P-012：Provider token/tool 增量跨两级无界通道传递，慢消费者可使内存随输出持续增长

- 优先级：P1
- 分类：无背压队列、流式热路径、潜在无界内存
- 置信度：高
- 验证状态：Sender 类型、逐事件发送点、双层转发和输出 Stream 消费边界已确认；慢消费者压力测试待补
- 涉及文件：
  - `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_live_delta.rs:1-9`
  - `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_stream_runtime.rs:288-343,404-539,858-876`
  - `crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_vulcan.rs:1554-2012`

`ProviderLiveDeltaSender` 固定为 `tokio::mpsc::UnboundedSender`。一次执行先建立 Adapter/Tool 到 Controller 的 `attempt_sender` 无界通道，再建立 Controller 到返回 `ProviderDeltaStream` 的 `live_sender` 无界通道。正文、Markdown、思考和工具参数片段都按 Provider 事件直接 send；Controller 用 `select!` 排空第一级后同步 send 到第二级，外层 `async_stream` 只有在下游继续 poll 时才从第二级取下一项并 yield。

因此，只要 Service Kernel 的持久化、状态锁、WebUI 事件发送或调用者调度短暂慢于 Provider 产出，第二级队列就会累积；调用者长时间不 poll 时，后台 controller 仍可继续接收完整 Provider 输出。队列没有条目数、总字节数或正文合并上限，长输出和细粒度工具参数流会让内存按未消费增量持续增长。第一级通常被同一 controller 任务并发排空，风险较小，但工具执行或调度停顿时同样没有硬边界。

建议先把 Controller 到返回 Stream 的第二级改成有界 `mpsc::channel`，转发时 `send().await`，让真实下游背压能到达 controller；容量应由事件个数和预期 burst 基准确定。第一级因多个同步回调当前依赖非 async send，可采用受限增量合并器：正文/思考连续片段在固定字节上限内合并，控制/工具边界事件禁止丢弃；或把回调写入小型同步队列，由单一 async forwarder 进入有界通道。必须测试下游停止 poll、极细 token、超长工具 JSON、取消和终态事件不可丢失。`LiveFileWatcher` 也使用无界原始事件通道，但文件事件允许按路径/时间窗合并，应另以有界 coalescing 处理，不能照搬 Provider 的“绝不丢控制事件”策略。

## 6. 审核疑问与收口结果

| 编号 | 疑问 | 当前证据 | 后续验证 | 状态 |
| --- | --- | --- | --- | --- |
| Q-001 | 超大单文件是否在高频路径重复完成相同配置解析、转换或验证 | 已在 Provider 流、插件租约和知识库导入确认三类重复解析/验证/整库处理，分别见 P-002、P-004、P-008 | 后续只需通过 Profile 决定三项修复的先后，不再保留结构疑问 | 已收口：确认问题 |
| Q-002 | 前端派生模型是否在 React 渲染路径和相邻 hooks 中重复构建 | 三处集合身份稳定器确认重复 `stringify -> parse`，见 P-007；正常正文 delta 已证实走链式片段并绕过 React 全量投影 | 仅需用大 timeline Profile 量化 P-007，不存在“正文逐 token 全量复制”问题 | 已收口：一项确认、一项排除 |
| Q-003 | Provider Manifest、配置与请求投影是否在会话/请求分支中重复标准化 | 响应侧重复字符串复制/JSON 解析见 P-002，请求侧完整执行请求重复 clone 见 P-003；媒体与工具路径未发现另一项可独立成立的问题 | 在 P-002/P-003 重构验收中分别加入协议 golden 与大请求分配基准 | 已收口：确认问题 |
| Q-004 | 上下文压缩进度事件是否也会以高频率触发同一全量合并 | `append_webui_context_compaction_progress` 与 OutputDelta 复用同一克隆、投影和合并路径，见 P-001 | 用真实长会话记录事件频率和单次分配量 | 已收口：机制确认，量化未执行 |
| Q-005 | 工作区文件树的“只显示可预览文件”是否会在大目录下形成可感知卡顿 | 同步深扫、逐条规范化/元数据读取和后代重复扫描机制已确认，见 P-005 | 对 1 万/10 万条目、无可预览后代目录做 RPC 延迟与执行器阻塞基准 | 已收口：机制确认，量化未执行 |
| Q-006 | Workbench CSS 超限由真实重复规则、全局静态导入还是近期功能自然增长主导 | 已确认 `WorkbenchApp -> styles.css -> 全部功能样式` 的全局静态聚合，且生产预算已失败，见 P-006；重复规则与自然增长的精确占比未测 | 做 CSS 覆盖率与重复声明分析，并把懒加载功能样式移入对应异步入口后复测 | 已收口：主因确认，细分占比未执行 |
| Q-007 | 会话 key 数量增长后，JSON 身份稳定器在真实交互中的占比是否可感知 | 三处直接 `stringify -> parse` 已确认，turn key 随会话增长，见 P-007 | 以 1,000/10,000 个 timeline key 做 React Profile 和微基准 | 已收口：机制确认，量化未执行 |
| Q-008 | 知识库导入的峰值内存是否接近“全部文档正文 + 全部分块正文 + 控制器参数”的叠加 | 整库文档、分块与 staged 参数在 `stage_import_rows` 前同时存活已确认，见 P-008 | 以 1/5/20 GiB Markdown 集合采集 RSS、Tokio 延迟和 staging 调用载荷 | 已收口：机制确认，量化未执行 |
| Q-009 | 冗余 clone 聚合项中哪些能在 Profile 中形成可见热点 | Clippy 全目标输出 195 条相关提示；已确认若干大型状态/String 样本，见 P-009 | 先机械清理生产 lib/bins，再比较压缩错误、子代理缓存结果和会话写入分配量 | 已收口：冗余成立，热点排序未执行 |
| Q-010 | `make sync` 在 SSD、杀毒软件开启和文件占用三种环境下的实际耗时 | 近 100 MiB 无条件枚举/复制机制已确认，见 P-010 | 分目录记录冷/热同步耗时和实际写入字节数 | 已收口：机制确认，环境量化未执行 |
| Q-011 | DAP 大正文的真实网络分片分布与累计复制量 | 二次复制机制和 16 MiB 上限已确认，见 P-011 | 加入可控 chunk 大小的 loopback 基准并采集 copied bytes/延迟 | 已收口：机制确认，网络基准未执行 |
| Q-012 | 慢 WebUI/持久化消费者下 Provider 无界队列的峰值条目和字节数 | 双层无界通道与逐事件发送已确认，见 P-012 | 暂停下游 poll，分别流入 10 MiB 正文和大工具参数，采集队列/内存曲线 | 已收口：无界风险确认，压力测试未执行 |

## 7. 已排除候选

- 正常对话正文增量没有在 WebUI 中逐 token 复制完整消息：`ConversationLiveStreamStore` 用反向链式片段保存增量，流式 Markdown 只复制受字节上限约束的活动尾部，`WorkbenchApp` 还会把纯正文 delta 排除在 React Workbench 投影之外。该链已核对，不列为问题。
- `runtimeSettingsValidationError` 与 Rust 侧约束表面重复，但它在提交前提供本地化、即时表单反馈，属于明确的 UX 边界校验，不认定为“无必要验证”；后端继续保留权威校验仍是必须的。
- 分段会话存储的追加路径没有为每条新消息重读完整历史：它使用驻留游标、受限活动尾部和追加文件锁，测试还覆盖非活动分段不重读。该实现与本次审核目标一致，不列为问题。

## 8. 处理建议

### 8.1 建议处理顺序

| 顺序 | 处理组 | 对应问题 | 理由与验收重点 |
| ---: | --- | --- | --- |
| 1 | 先恢复质量门禁基线 | P-006，以及第 9 节的两个 Rust 基线失败 | CSS 预算已经实际失败；Provider Config 全目标测试无法编译，Agent Runtime 有稳定失败。先恢复这些基线，后续性能改动才有可靠回归信号。 |
| 2 | 收紧无界内存与二次复制 | P-012、P-011 | 两项都可在输入持续增长或慢消费者条件下产生非线性/无界资源占用。验收必须含取消、边界事件不丢失、碎片化和慢消费者压力测试。 |
| 3 | 重构 Provider 与压缩热路径的数据所有权 | P-001、P-002、P-003 | 三项共同问题是大型状态在相邻层重复 clone、字符串化、解析和投影。建议先建立长会话/大请求分配基线，再以“单次解析、借用读取、边界一次拥有”为目标分三次改动，避免一次架构大改。 |
| 4 | 把阻塞和整库驻留改成有界流水线 | P-005、P-008 | 工作区遍历应移入阻塞线程并减少逐项系统调用；知识库导入应按有界批次读取、分块和 staged 写入。验收看 Tokio 延迟、峰值 RSS、取消与部分失败清理。 |
| 5 | 建立代际缓存和懒加载边界 | P-004、P-006、P-010 | 插件桥接资产、功能 CSS、运行时同步资源都存在“稳定内容每次全局处理”。使用内容代际/模块边界跳过未变化工作，不能依赖模糊时间戳或静默复用过期状态。 |
| 6 | 做低风险机械清理 | P-007、P-009 | 先移除静态工具可证明的无语义 clone/collect 和 JSON 身份稳定器，再 Profile 决定是否继续重排复杂所有权。不要删除安全、协议或 UX 边界的必要验证。 |

### 8.2 建议建立的长期基线

1. 为 Provider 流建立“每 MiB 输出的 JSON 解析次数、分配字节、队列峰值、慢消费者行为”基准。
2. 为 1,000/10,000 回合会话建立压缩增量、Workbench 投影和 React timeline Profile。
3. 为 1 万/10 万文件工作区建立普通树与仅可预览树的 RPC 延迟和 Tokio 阻塞指标。
4. 为 1/5/20 GiB Markdown 知识库导入记录吞吐、峰值 RSS、取消时间和 staged 批次数。
5. 把 Workbench CSS raw/gzip 预算继续保留为 CI 门禁，并增加按懒加载功能入口拆分后的 chunk 归属检查。
6. 对资源同步记录每个资源根的输入代际、跳过数量、实际复制字节和冷/热耗时。

### 8.3 关于“无必要验证”的结论

本次没有发现可以不加条件地删除的高风险安全/协议校验。确认存在的重复验证主要是 P-004 的稳定插件/全局桥接资产在每个临时租约重复读取和校验，应改为按内容代际缓存，而不是取消校验。前后端设置校验、路径安全校验和协议边界校验承担不同信任边界，不能仅因规则相似就合并或删除。

## 9. 验证边界

### 9.1 已执行验证

- WebUI：`pnpm lint` 通过；架构测试 6/6 通过；Vitest 94 个文件、429 项测试通过；生产构建成功，共转换 4,613 个模块。
- WebUI 构建预算：`pnpm check:build-budget` 失败，Workbench CSS 为 raw 445,710 字节、gzip 66,506 字节，分别超过 440,000/66,000 字节预算，构成 P-006 的运行证据。
- Rust：关键 crate 定向测试通过，包括 Provider Adapters 636 项、Service Kernel 935 项、Workspace Store 276 项（1 ignored）、Plugin Runtime 59 项、Context Compaction 45 项、Host Tools 57 项。
- Node/脚本：6 个非 Web MJS 文件通过 `node --check`；DAP sidecar 4 项测试通过；19/19 PowerShell 文件通过 PowerShell 7 解析；7/7 Python 文件通过 AST 语法解析；7 个 Lua 与 8 个 Shell 文件完成 AST/流程审核。
- 静态分析：生产 lib/bins Clippy 编译完成；全目标 Clippy 收集到 195 条与冗余 clone/collect 等方向相关的提示，已按生产代码、测试代码和重复目标区分，形成 P-009。

### 9.2 已知基线失败

- `cargo clippy --workspace --all-targets` 无法完成：`crates/vulcan-provider-config` 的测试构造缺少 `account_model_access` 和 `tool_schema_projection` 字段。这是现有测试编译基线问题，不是本次文档审核造成。
- `vulcan-agent-runtime` 定向测试中，`store_task_outcome_persists_segmented_task_session_artifacts` 稳定复现“非追加 display mutation”失败；本次未越界修复。
- 排除上述两个 crate 的工作区测试在并行运行时，Provider 多协议发现测试曾因临时本机 HTTP 端口连接失败而失败；同一测试独立精确重跑 1/1 通过，判定为并行环境瞬时失败，但未再次执行整个工作区以宣称全绿。

### 9.3 未执行的运行验证

没有连接真实 Provider 账户，也没有执行长会话、大工作区、GiB 级知识库、慢消费者、DAP 大帧碎片和资源同步写入基准。因此 P-001 至 P-012 的源码机制与边界条件已经确认，但除 CSS 预算外，CPU、锁等待、峰值内存、RPC 延迟、帧率和实际节省比例均未冒充已量化结果。Shell 文件因本机没有 Bash、Lua 文件因本机没有独立 Lua 解释器，分别以 AST、调用链和可用的 Rust Plugin Runtime 测试为验证边界。

## 10. 最终结论

本次共审核 1,460 个源文件，全部至少达到 `S1`：1,408 个完成结构/规则扫描，3 个完成关键实现审核，49 个对问题或排除候选完成交叉验证。最终确认 12 项问题，其中 P1 7 项、P2 5 项，没有仅凭文件名或单一规则提示列入正式问题的低置信度条目。

问题集中在四类系统性模式：大型会话/请求状态在相邻层重复复制和投影；同一 Provider 数据反复字符串化与 JSON 解析；异步边界内同步深扫、整库驻留或无界排队；稳定资源在每次租约、构建入口或同步任务中被全局重复处理。最应优先处理的是 P-012、P-011 这两项资源边界风险，以及 P-001/P-002/P-003 三项流式与大型状态所有权问题。

本次审核没有修改产品源码。建议不要把 12 项问题合并成一次大重构；应先恢复现有质量门禁，再按第 8 节顺序为每组建立基准、分小批实现和验证。
