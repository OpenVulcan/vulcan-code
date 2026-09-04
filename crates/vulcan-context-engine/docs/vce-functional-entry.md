# VCE 功能入口与宿主复用方案

本文记录 `vulcan-context-engine` 当前已经实现的直接 Rust 功能入口，以及未来嵌入 VulcanCode 时必须由宿主完成的边界。实现依据是 OCE 服务端领域逻辑与 `oce-client` 同步逻辑，不是它们的 MCP、HTTP 或部署方式。

## 1. 架构结论

`VceEngine` 是唯一主功能入口。VCE 独立拥有代码语义处理规则，但不拥有外部资源：

```text
VulcanCode
  ├─ 注册统一 AI 能力 ─────────────> VceAiPort
  ├─ 提供工作区不可变源码快照 ─────> VceSourcePort
  ├─ 注册 VLDB/SQLite 代码命名空间 ─> VceIndexPort
  ├─ 文件/Git 监听与任务调度
  ├─ 页面、权限与工具注册
  └─ 调用 VceEngine 功能入口

VCE
  ├─ 工作区与 checkpoint 状态机
  ├─ 结构、Chunk、Symbol、Embedding 计划
  ├─ 刷新与暂存/发布编排
  ├─ 查询规划与检索融合
  └─ 管理查询及持久化状态契约
```

CBM 不在链路中：VCE 不调用 CBM、不使用 CBM 数据、不把 CBM 作为回退，也不在 AI 工具面保留 CBM 工具。VMM 只与 VCE 共享宿主 AI/VLDB 能力契约与底层基础设施，消费者注册、记忆数据和代码语义数据完全分离。

## 2. 直接调用入口

```rust
let mut engine = VceEngine::new(config)?;

let refresh = engine
    .refresh(request, &index_port, &ai_port, &source_port)
    .await?;

let retrieval = engine
    .retrieve(
        RetrieveRequest::current_checkpoint("query-1", "find request routing"),
        &index_port,
        &ai_port,
    )
    .await?;
```

### 2.1 `VceEngine` 已实现方法

| 方法 | 作用 | 外部调用 |
| --- | --- | --- |
| `new` | 校验配置并创建空引擎 | 无 |
| `with_structure_analyzer` | 注册语义结构分析器并自动附加递归回退 | 无 |
| `from_workspace_snapshot` | 恢复并校验旧工作区状态 | 无 |
| `from_state_snapshot` | 恢复带 Schema/namespace/vector identity 的完整状态 | 无 |
| `workspace_snapshot` / `state_snapshot` | 输出纯工作区或完整版本化状态 | 无 |
| `state_identity` | 输出持久化与索引绑定身份 | 无 |
| `observe` / `remove` | 写入未保存覆盖或显式删除 | 无 |
| `sync_plan` / `plan_query` / `current_scope` | 纯差异、查询与范围逻辑 | 无 |
| `refresh` | 协调、读取、结构化、Embedding、暂存和原子发布 | AI、源码、索引端口 |
| `retrieve` | dense/exact/path、RRF、Rerank、coverage | AI、索引端口 |
| `status` / `statistics` / `workspace_statistics` | 本地管理数据 | 无 |
| `local_management_snapshot` | 本地状态与经绑定校验的 AI 能力投影；不一致时显式失败 | 只读 AI 能力 |
| `management_snapshot` | 合并本地与宿主索引状态 | 索引端口 |
| `management_query` | 文件、Blob、历史、完整性和维护计划查询 | 索引端口 |

`observe`、`remove` 和所有 `plan`/`snapshot` 方法都不会偷偷调用 AI 或存储。宿主决定何时刷新、何时保存状态以及何时执行维护命令。

## 3. 统一 AI 注册契约

`VceAiPort` 对应一次固定宿主注册。每个请求的 `VceAiRequestContext` 都包含：

- `operation_id`：幂等、日志和关联身份；
- `registration_id`：统一宿主注册身份；
- `consumer_profile_id`：路由、权限、额度和用途画像；
- `vector_space_id`：代码向量逻辑空间；
- `purpose`：本次调用的明确用途。

VulcanCode 为原生进程内 VCE consumer 注册 `HostAiPort` 时，`InferenceCallerContext.usage_scope` 必须使用 `InternalBackground`，其持久化/候选路由值为 `internal_background`。`external_service` 仅属于已移除的外部 sidecar 推理授权语义，不适用于 VCE。

用途只有以下稳定枚举：

| 用途 | 调用时机 | 必需性 |
| --- | --- | --- |
| `CodeDocumentEmbedding` | 为源码 Chunk 建立内容向量 | 主链路必需 |
| `CodePathEmbedding` | 为路径语义文档建立向量 | 可降级 |
| `CodeQueryEmbedding` | 为查询变体建立内容查询向量 | 主链路必需 |
| `CodePathQueryEmbedding` | 建立路径查询向量 | 可降级 |
| `CodeQueryRewrite` | 生成有限查询改写 | 可选 |
| `CodeCandidateRerank` | 对融合候选重排 | 可选 |

VulcanCode、VMM-Core、知识库和 VCE 应复用相同的宿主注册字段、用途和调用规则，但每个消费者必须生成并持有独立的 `registration_id`、`consumer_profile_id` 与 `vector_space_id`。它们不能共用业务表；VCE 的 namespace、Schema 和 checkpoint chain 也始终独立。

VCE 会在调用前校验 AI binding 与当前能力快照，在响应后校验 operation ID、完整分段集合、向量维度、预处理版本、Chunker 版本及可选物理模型指纹。未激活注册不能同时声明 Embedding、查询改写或 Rerank 可用。配置变化后宿主适配器必须刷新 capability handle/snapshot，不能沿用首次注册的陈旧 handle；领域引擎会在每次刷新、检索和管理投影时重新读取 `capability_snapshot()`，并在单次入口中只消费这一份已验证快照。没有备用 Provider 探测或静默切换。

## 4. 源码与索引端口

### 4.1 `VceSourcePort`

宿主可以只把 path、Blob、size、mtime 作为 `FileObservation::identified` 传入。只有 `find_missing` 判定需要索引时，VCE 才同时按 `index_stage_blob_limit` 和 `index_stage_max_bytes` 组成 `VceSourceReadRequest`：

1. 宿主返回完全相同的规范路径集合；
2. VCE 校验字节长度和 mtime；
3. VCE 重新计算带 `vce-blob-v1` 域标记和路径字节长度边界的 `SHA-256(path, content)`，避免裸拼接边界碰撞；
4. 任一项变化均拒绝旧内容；
5. 成功发布后释放文件系统正文，显式编辑器覆盖继续保留。

因此初次大仓扫描可以由 VulcanCode 流式计算文件身份，不需要 VCE 同时持有全部源码与向量。

### 4.2 `VceIndexPort`

每个索引端口必须先暴露固定 `VceIndexBinding`：

- 代码专属 `namespace_id`；
- `checkpoint_chain_id`；
- `schema_version`；
- 完整 `EmbeddingSpace`。

字段不完全一致时，VCE 在任何读写前失败。

| 操作 | 契约 |
| --- | --- |
| `find_missing` | 响应 ID 匹配；`unknown` 与 `nonindexed` 必须互斥且是请求子集 |
| `stage` | 以 refresh/generation/batch operation 幂等暂存完整 Blob 替换；回执 Blob 集合必须完全相同 |
| `abort_stage` | 使一个未发布 refresh 的暂存记录不可再发布 |
| `commit` | 以 `expected_checkpoint_version` 执行 CAS，原子发布 staged、deleted 与 checkpoint |
| `search_dense` | 只在明确 Blob 范围内搜索内容向量 |
| `search_path` | 只在明确 Blob 范围内搜索路径向量及对应首 Chunk 回填 |
| `search_exact` | 只在明确 Blob 范围内查询符号出现位置 |
| `management_snapshot` | 返回聚合计数、完整性和当前/最近作业 |
| `management_query` | 返回类型化分页数据、历史、问题与只读维护计划 |

`CommitReceipt` 必须精确匹配 operation、refresh、generation、checkpoint chain/version、indexed/deleted 数量与集合。提交失败或回执无效时 VCE 尝试 `abort_stage`，且不会推进本地 checkpoint。宿主对相同 operation ID 必须返回同一结果，不能重复执行非幂等写入。

## 5. 刷新流程

```text
RefreshRequest
  -> 校验 request/mode/path 唯一性
  -> Full reconcile 或 Paths reconcile（事务式内存替换）
  -> sync_plan(added/deleted/skipped)
  -> find_missing
  -> 每个有界 Blob 批次：
       source read + hash recheck（Blob 数量和源码总字节双重上限）
       semantic analyzer -> recursive fallback
       content Embedding batches（有界并发）
       path Embedding（失败只标记辅助通道）
       stage + exact receipt validation
  -> IndexCommit(CAS + checkpoint)
  -> exact commit receipt validation
  -> local checkpoint
  -> release committed filesystem content
```

Full 与 Paths 协调都先在临时记录集合完成全部校验，最后一次性替换；后段出现非法路径或重复路径时，generation 和记录不会部分推进。公开可构造的 observation 也会重新校验 content/size、skip reason、deleted metadata 和规范 Blob。

VulcanCode 已有文件监听和 Git watcher，因此 VCE 不创建第二套监听。宿主把 `Created/Modified/Removed/Renamed` 合并后重新观察受影响路径，调用 `RefreshRequest::paths`；ignore 规则变化或恢复要求则调用 Full。后台单飞、事件合并、取消、deadline 和持久任务重试属于统一宿主调度规则，不在 VCE 内自建线程。

## 6. 结构、符号与向量

默认分析器执行递归切块，并从 Rust、Python、Go、C/C++ 风格定义及常见 endpoint 注解中保守提取符号。语义 cAST 通过 `VceStructureAnalyzer` 注入，输出必须包括 analyzer ID、兼容版本、原文 Chunk 和 Symbol occurrence。

普通完整行 Chunk 保留 OCE 的 occurrence ID。只有一条物理行因硬字符预算被切成多个片段时，VCE 才把首行 Unicode 字符偏移加入身份；这样同一行上的重复文本仍是不同出现位置，且跨片段字符串/注释词法状态不会丢失。

`with_structure_analyzer` 不会丢掉默认回退：主分析器失败或输出版本无效时，`FallbackStructureAnalyzer` 自动使用递归切块，并把原因写入 `structure_fallback_reason`。VCE 不把普通 lexical parser 冒充为完整 cAST；具体实现仍需保证 AST window 预算、兄弟合并、小窗口吸收、边界映射和原始行号。

向量处理已经实现：按输入数和总字符分批、长输入按边界分段、有限 overlap、按片段字符数加权平均、L2 normalization、批次有界并发、响应覆盖/维度严格校验。并发完成顺序不会改变最终分段顺序。

## 7. 检索流程

```text
RetrieveRequest
  -> checkpoint 或显式 Blob scope
  -> intent + identifiers + query variants
  -> optional query rewrite
  -> variants bounded-concurrent:
       query Embedding -> dense search
       optional path-query Embedding -> path search
  -> bounded exact symbol search
  -> normalized weighted RRF for semantic lists
  -> intent-aware exact merge（CALL_CHAIN 保留语义上下文）
  -> configurable path boost + first-Chunk fallback
  -> source priority
  -> optional Rerank
  -> explicit symbol-location endpoint promotion
  -> confidence filter + coverage selector
  -> line-numbered context
```

单个有效 dense 列表保持宿主原分数，不会被 RRF 改写成约 `1/61`；多个列表按理论最大贡献归一化。exact 不作为普通 RRF 列表混入，而按意图合并。最终结果执行单路径上限、同路径重叠抑制、跨路径优先、结果数上限和硬字符预算。

宿主搜索响应必须通过 operation、数量、scope、规范路径、SHA-256 Blob、ID、内容 hash、行号、有限分数和重复出现校验。path、exact、rewrite 和 Rerank 的可恢复失败会写入 `VceDegradation`；dense 或内容向量失败不会伪装成空结果。

## 8. 状态与管理器

`VceStateSnapshot` 使用 `VCE_STATE_SCHEMA_VERSION`，包含：

- namespace/checkpoint chain/index schema/Embedding space identity；
- 工作区 generation、记录和 checkpoint；
- 查询与刷新计数；
- 最近刷新/检索摘要及最近错误。

恢复时会拒绝未知 Schema、身份漂移、重复或非规范路径、错误 Blob、记录代数倒退、状态字段冲突以及 checkpoint 与 committed membership 不一致。运行中的 Future、取消令牌和宿主任务句柄不进入快照。

`management_query` 只接受类型化查询，不接受 SQL、LanceDB 表名或任意谓词。VCE 会重新校验公开字段、分页上限、响应变体、过滤条件、游标、文件/Chunk/Blob 身份、作业阶段、完整性问题和维护动作。维护计划只读；真正删除、修复和重建必须通过 VulcanCode 独立授权命令执行。

## 9. VulcanCode 原生集成边界

VCE 领域内核已经完成：

- OCE/OCE-client 领域流程、数据模型和失败语义提取；
- 工作区、结构/回退、Symbol、Embedding、刷新、检索、管理和状态恢复逻辑；
- AI/Source/Index 端口、namespace/vector/checkpoint 绑定及严格回执；
- OCE 工具名称的静态目录映射；
- 单元测试、格式化和严格 Clippy 验证。

VulcanCode 宿主已经完成：

- VMM-Core、知识库和 VCE 共用 `HostAiPort` 契约以及注册字段、用途、额度、deadline、错误和取消规则，但分别创建和持有消费者注册；
- `VceHostAiAdapter` 为每个项目注册 `InternalBackground` capability，并在调用时刷新快照、冻结向量指纹、传播 Future 取消；
- `vulcan-workspace-store` 通过 VLDB Controller 持有独立 SQLite namespace、精确符号索引与两张 LanceDB 向量表，并实现 stage、事务内 CAS commit、abort、恢复和派生清理；
- service-owned 共享 watcher 同时服务 Git 与 VCE，VCE subscriber 独立完成路径归并、节流、失败重排和关闭收敛；
- `codebase_retrieval` 已进入 Native Tool Catalog、策略过滤、执行上下文、结果投影和 VCE/CodeKit 路由说明；
- 服务 API 与项目管理页已接入设置、状态、统计、分页数据、Blob/Chunk、作业、完整性和带 revision/checkpoint 的维护确认链；
- 原生 CBM Controller、managed MCP、工具入口和运行时安装/同步链已经退出，历史数据只保留不删除。

具体语言的完整 cAST parser/window analyzer 仍是可插拔增强项，不是当前索引可用性的前置条件。默认 `RecursiveStructureAnalyzer` 已提供确定性分块、保守符号抽取与失败回退；后续 parser 必须通过 `VceStructureAnalyzer` 注册并提升 `chunker_version`，不能绕过既有兼容性与回退边界。

## 10. 许可路线

VCE 采用 MIT 许可独立实现。OCE 的 Apache-2.0 源码与说明仅用于确认行为、边界和失败语义；本仓库不逐行翻译其 Python 服务端、不复制 MCP/HTTP/Milvus 基础设施，也不继承 CBM 设计。
