# Vulcan Context Engine

`vulcan-context-engine`（VCE）是面向 VulcanCode 的 Rust 原生代码语义上下文内核。它提取 OCE 服务端与 `oce-client` 已验证的处理思路，以 MIT 许可独立实现，不复制 OCE 的 MCP、HTTP、Milvus、Provider 或进程模型。

VCE 的职责是把源码变化变成可提交的语义索引，并在精确工作集内执行：

```text
reconcile -> structure/chunk -> embed -> stage -> atomic checkpoint
query plan -> dense/exact/path -> RRF -> rerank -> coverage -> context
```

## 当前功能入口

上游直接调用 `VceEngine`，不需要也不应该在 VCE 内建立 MCP Server：

```rust
let mut engine = VceEngine::new(config)?;

let refreshed = engine
    .refresh(refresh_request, &index_port, &ai_port, &source_port)
    .await?;

let context = engine
    .retrieve(
        RetrieveRequest::current_checkpoint("query-1", "find request routing"),
        &index_port,
        &ai_port,
    )
    .await?;
```

可直接使用的功能包括：

- `refresh`：全量或路径级协调、缺失分类、有界源码读取、结构分析、向量化、分批暂存和 CAS 检查点发布；
- `retrieve`：启发式意图与查询规划、有界并发查询、dense/exact/path 召回、RRF、精确结果合并、可选 Rerank、路径增强和覆盖选择；
- `observe` / `remove`：管理未保存编辑器覆盖层；
- `sync_plan` / `plan_query` / `current_scope`：不访问宿主的纯逻辑入口；
- `status` / `statistics` / `workspace_statistics`：本地管理投影；
- `management_snapshot` / `management_query`：宿主索引统计、分页数据、作业历史、完整性问题和维护计划查询；
- `state_snapshot` / `from_state_snapshot`：带 Schema、命名空间、向量空间和 checkpoint 身份校验的版本化恢复入口。

## 宿主注入边界

VCE 只依赖三个领域端口：

- `VceAiPort`：由 VulcanCode 注册的统一 LLM、Embedding、Rerank 能力；原生进程内 VCE consumer 必须使用 `InternalBackground`（持久化值 `internal_background`）usage scope，每次请求都携带固定 `registration_id`、`consumer_profile_id`、`vector_space_id` 和用途；
- `VceIndexPort`：由 VulcanCode 注册的 VLDB/SQLite 适配，负责代码专属 namespace、暂存、原子发布、召回和管理查询；
- `VceSourcePort`：由 VulcanCode 工作区提供不可变源码快照，VCE 按 Blob、size、mtime 重新校验，且只按配置批量读取。

VCE 不创建 LLM 客户端、Embedding 客户端、LanceDB、SQLite、Milvus、文件监听器、Git 监听器、页面管理器或工具注册器。VulcanCode、VMM-Core、知识库和 VCE 可以共用同一套 AI 注册规则，但 VCE 的代码索引、VMM 的记忆数据和知识库数据必须使用独立 namespace 与 Schema。

`external_service` 只表示已经移除的外部 sidecar 授权语义，不能用于原生 VCE。宿主配置发生变化后，适配器必须重新取得 capability handle/snapshot；VCE 在每次刷新、检索与管理投影时读取当前 `capability_snapshot()`，不缓存首次注册时的陈旧能力状态。

## 一致性与轻量化

- 文件记录、generation 和 checkpoint 恢复会校验规范路径、SHA-256 Blob、状态不变量及完整成员关系；
- metadata-only 观察允许宿主先流式计算 Blob，VCE 只为实际缺失 Blob 拉取正文，并同时按 Blob 数量和源码总字节数分批；
- 文件系统正文提交后从 VCE 内存释放，显式编辑器覆盖层继续保留；
- 每批索引先 `stage`，再以 `expected_checkpoint_version` 原子 `commit`；回执必须精确匹配 operation、refresh、generation、chain、版本及 Blob 集合；
- 准备、暂存或发布失败会中止未发布事务，本地 checkpoint 不前进；
- Embedding 批次和查询变体采用可配置有界并发，结果按原始顺序恢复；
- path Embedding/path search、查询改写、exact 超限和 Rerank 属于可降级能力，原因与 operation ID 会保留；主要内容 Embedding 和 dense 召回失败则显式终止；
- 搜索响应会校验 operation ID、范围、数量、规范路径、Blob/Chunk 身份、行号、内容 hash、分数和重复项。

## 结构分析

默认 `RecursiveStructureAnalyzer` 提供确定性递归切块和保守的多语言定义/入口符号提取。`VceStructureAnalyzer` 是语义 cAST 实现入口；通过 `with_structure_analyzer` 注册后，VCE 会自动组合 `FallbackStructureAnalyzer`。主分析器失败或返回无效结果时，递归切块继续工作，失败原因随 Blob 保留给管理层。

VCE 不把“仅接入 parser”宣称为完整 cAST。OCE 的 AST window 分配、兄弟合并、小窗口吸收和原文行号保持仍应由具体 `VceStructureAnalyzer` 实现，并以 `chunker_version` 参与向量空间兼容性。

## 工具与管理面

`VceToolCatalog` 只提供给 VulcanCode 工具注册器的静态映射：唯一建议向 AI 暴露的入口是 `codebase_retrieval`。`find_missing`、`batch_upload`、`checkpoint_blobs`、`sync`、`sync_paths`、`status`、`observe`、`remove`、`watch` 等 OCE 语义操作保持 HostOnly，不增加模型工具分散度。

页面管理器可通过类型化契约读取：

- 文件/Blob 清单和 Blob Chunk 详情；
- 内容、路径、符号通道状态；
- 当前/历史作业及稳定阶段；
- namespace 完整性问题；
- 只读修复、重建或暂存清理计划。

VCE 不执行维护计划；删除或重建必须由 VulcanCode 的独立命令链完成授权和持久化。

## 模块职责

| 模块 | 职责 |
| --- | --- |
| `workspace` / `state` | 工作区事务、overlay、generation、checkpoint、版本化恢复 |
| `identity` / `model` | 稳定 Blob/Chunk 身份和源码数据模型 |
| `chunking` / `structure` / `symbol` | 递归切块、语义分析扩展、回退与精确符号 |
| `embedding` / `indexing` | 分段、分批、加权池化、归一化和索引准备 |
| `query` / `retrieval` | 意图、查询变体、融合、提升、重排和覆盖选择 |
| `ports` | AI、源码、索引、搜索、暂存、发布与管理端口 |
| `management` | 分页查询、作业、完整性与维护计划契约 |
| `tooling` | OCE 工具语义到 VulcanCode 工具管理器的静态映射 |
| `host_adapter` | 可选的 `vulcan-ai-contract` 适配辅助 |

更详细的行为来源和对接约束见：

- [`docs/oce-code-semantic-logic-extraction.md`](docs/oce-code-semantic-logic-extraction.md)
- [`docs/vce-functional-entry.md`](docs/vce-functional-entry.md)
- [`docs/vce-tool-surface.md`](docs/vce-tool-surface.md)
- [`docs/vulcancode-native-integration.md`](docs/vulcancode-native-integration.md)

## 验证

```powershell
rtk cargo test -p vulcan-context-engine --all-features
rtk cargo clippy -p vulcan-context-engine --all-targets --all-features --no-deps -- -D warnings
rtk cargo fmt -p vulcan-context-engine -- --check
```

本 crate 使用 MIT 许可证，见 [`LICENSE`](LICENSE)。
