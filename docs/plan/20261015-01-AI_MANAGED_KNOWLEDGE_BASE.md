# AI 管理知识库重构方案

> 状态：执行中。目标已建立；按阶段实施、验证与本地提交，不推送。完成后循环审核并自动修复，连续五轮无问题才结束。
> 方案依据 2026-10-15 对当前源码的查证；执行前必须按第 3 节逐项复核，事实以当时源码为准（AGENTS 第一节第 1 条）。

## 1. 目标

把知识库从“用户指定 Markdown 目录 → 扫描导入 → 后台 LLM 整理 → 向量”改为“人工管理生命周期，AI 管理内容”：

- 人工只做：创建空库（名称、用途说明、索引模式）、修改索引模式、绑定项目与会话选择、导入/导出套件、
  查看内容与变更历史、还原到指定变更点、补生成失败的向量、删除整库。
- AI 通过工具完成全部内容操作：建立目录分级、写入与编辑文档、填写摘要与关键词、决定哪些内容进入向量、
  从本地文件原样搬运、列目录、读取、检索。
- 不再存在“来源目录”“同步 Markdown”“忽略文件”“整理库（display corpus）”概念。

## 2. 已确认决策

| 编号 | 决策 | 来源 |
| --- | --- | --- |
| D1 | 不允许人工编辑任何文档或目录内容；WebUI/TUI 只读展示内容，所有内容变更必须由 AI 工具产生 | 用户 |
| D2 | AI 可直接搬运本地文件：主机原样复制字节进入知识库，正文不经过模型输入输出 | 用户 |
| D3 | 写入只进知识库数据库，不产生工作区文件，因此知识库写入工具不经过权限审批（分类为 `Ungoverned`），已授权并在会话中启用的知识库即可写 | 用户 |
| D4 | 每次变更都写入库内变更日志，可还原到任意指定变更点；向量模式下写入后自动生成向量；向量生成失败时人工可在界面对失败项补生成 | 用户 |
| D5 | 支持索引模式升级（及降级） | 用户 |
| D6 | 写入工具 `available_in_plan = false`：计划模式只做只读调研，不产生持久副作用 | 方案作者（用户可推翻） |
| D7 | 还原只由人工发起，AI 工具不提供还原；还原本身也记录为一次变更，因此可再次还原 | 方案作者 |
| D8 | 删除文档不设回收站，由变更日志承担恢复 | 方案作者 |
| D9 | 历史永久保留；历史压缩不在本方案范围 | 方案作者 |
| D10 | 导出套件只含当前内容与配置，不含历史与向量；导入后从一次 `import` 变更开始新历史，向量重新生成 | 方案作者 |
| D11 | 旧格式一律拒绝，不迁移：旧知识库、旧套件、含旧检索策略字段的会话元数据均按 AGENTS 第一节第 12 条处理 | AGENTS |

“人工补充向量”（D4）的解释：人工不能手填向量数值，而是在界面看到待生成/失败条目及错误原因，
对全部失败项或指定文档触发重新生成，必要时更换嵌入路由（等同向量空间变更，见 7.4）。

## 3. 现状事实（执行前逐项复核）

| 领域 | 当前位置 | 说明 |
| --- | --- | --- |
| 每库存储结构 | `crates/vulcan-workspace-store/src/knowledge_base/schema.rs` | `KNOWLEDGE_SCHEMA_VERSION = 1`；原始库表 `knowledge_sources/documents/chunks` 与暂存表；整理库 `display_documents/display_chunks` 分代次 |
| 运行时 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/*.rs` | `import`、`file_selection`、`raw_corpus`、`display`、`fts`（Jieba）、`vector`（LanceDB，按代次建表）、`package_*`、`authorization`、`operations` |
| 模型 | `knowledge_base/models.rs` | `KnowledgeRetrievalMode`（raw_bm25 / display_bm25 / display_hybrid）、`KnowledgeLibrarySettings`（ignored_files、curation、display_index、embedding、vector_index） |
| 切块 | `knowledge_base/markdown.rs` | `chunk_markdown`、`extract_document_title`、`sha256_hex` 可复用 |
| 应用库表 | `crates/vulcan-workspace-store/src/data_plane.rs` 约 639 行 | `knowledge_libraries`（含 `last_completed_import_at_ms`、`settings_json`）、`knowledge_library_project_bindings`、`knowledge_import_jobs`（含 `source_root` 及文件计数列） |
| 服务 | `crates/vulcan-service-kernel/src/knowledge_base_services.rs`（约 4,500 行） | 整理 LLM 流水线（固定提示词、批次、结构修复重试、观测）、向量构建、全部 `service_*` 实现 |
| RPC | `crates/vulcan-service-kernel/src/rpc_dispatch/knowledge_base.rs` | `RPC_METHODS` 共 36 个方法 |
| AI 工具 | `crates/vulcan-agent-runtime/src/tooling/native/knowledge_base.rs` | 只读 `knowledge_search`、`knowledge_browse`、`knowledge_read`；注册在 `tooling/native.rs` 约 519 行 |
| 工具暴露 | `tooling/bridge.rs` 约 134 行；`tooling/bridge/descriptors.rs::native_tool_id_is_knowledge_tool`；`agent_runtime/runtime.rs::append_universal_native_tools` | 无 `knowledge_access` 或库列表为空时不注册 |
| 专职会话 | `agent_runtime/execution_preparation/dedicated_session.rs` | 知识库工具在 `none` 与 `standard` 两档都保留 |
| 权限分类 | `agent_runtime/tool_permission.rs::classify_native` | 未列出的原生工具默认 `Ungoverned` |
| 访问快照 | `agent_runtime/execution_models.rs::ExecutionKnowledgeAccessContext` | `project_id`、`library_ids`、`retrieval_mode`、`knowledge_search_handler` |
| 会话元数据 | `crates/vulcan-session-core/src/v1/config.rs::KnowledgeLibrarySelectionConfig` | 含可选 `retrieval_mode`，`deny_unknown_fields`；`METADATA_SCHEMA_VERSION = 7`；子执行覆盖 `v1/child_execution/request_overrides.rs` 也含 `retrieval_mode` |
| 提交入口 | `crates/vulcan-service-kernel/src/conversation_submission.rs::SubmissionKnowledgeSelection` | 接受 `retrievalMode` |
| 问答注入 | `crates/vulcan-service-kernel/src/agent_stream/stream_task/question_knowledge.rs` | 读取 `retrieval_mode` |
| 文件读取 | `crates/vulcan-agent-runtime/src/tooling/native/file_read.rs` → `vulcan_host_tools::file_runtime` | 相对路径按工作区解析，允许绝对路径，`${env:NAME}` 展开，编码检测；读取为 `Ungoverned` |
| WebUI | `apps/vulcan-code-web/src/components/features/knowledge/*`（约 3,000 行）、`src/service/knowledge-base-service.ts`、`components/composer/composer-knowledge-control.tsx`、专职向导 `dedicated-wizard-steps.tsx`、工具展示 `tool-presentations/native-semantic-tool-presentation.ts` 等 | |
| TUI | `apps/vulcan-code-tui/src/service_api/knowledge_base.rs`、`src/app/pages/knowledge.rs`、模拟后端 `backend/mock/service.rs` | 必须与 WebUI 服务调用一对一镜像 |
| 文案 | `crates/vulcan-i18n/catalogs/webui/*.json`、`catalogs/tui/*.json` | 默认语言补全，非默认语言不得有多余键 |
| 契约 | 无知识库专项契约；`docs/contracts/tui-service-adaptation.md` 第 89–146 行列出知识库页面与 RPC 映射；`dedicated-sessions.md` 提到知识库工具 | |

## 4. 目标概念模型

```
知识库（Library）
├── 元信息：名称、用途说明（展示给 AI，指导其组织内容）、索引模式、嵌入配置     ← 人工
├── 目录（Folder）：路径 + AI 维护的目录说明                                    ← AI
├── 文档（Document）：路径、标题、摘要、关键词、正文、向量策略、来源、修订号     ← AI
├── 变更日志（Change）：每次工具调用或人工操作一条，含前后镜像                  ← 自动
└── 派生索引：切块、Jieba FTS、LanceDB 向量及其状态                              ← 主机后台
```

- 路径规则：`/` 分隔，去除首尾斜杠，禁止空段、`.`、`..`、反斜杠与控制字符；NFC 规范化；
  比较键为小写化路径（沿用现有 `relative_path_key` 思路）；深度与段长有上限（见第 12 节）。
- 目录可隐式存在（有文档即存在），也可显式创建以便先写说明；删除目录只允许空目录。
- 文档正文以 Markdown 为格式（用于切块与渲染），不再对应任何外部文件。
- 摘要状态 `summary_state`：`provided`（AI 已写）、`pending`（搬运后未写）、`stale`（搬运源更新后旧摘要待复核）。

## 5. 每库存储（新格式）

`KNOWLEDGE_SCHEMA_VERSION` 改为 `2`。打开知识库时版本不等于 2 一律拒绝，该库在列表中呈现为
`lifecycle_state = 'error'` 并附英文原因（如 `Unsupported knowledge library format version 1; delete the library.`），
只允许删除。不提供迁移。

建议表结构（执行会话可调整列名，但语义与约束不得减少）：

```sql
kb_meta(key TEXT PRIMARY KEY, value TEXT NOT NULL)
  -- schema_version, library_id, head_change_seq, dictionary_revision, indexed_dictionary_revision

kb_blobs(sha256 TEXT PRIMARY KEY, content TEXT NOT NULL, byte_size INTEGER NOT NULL)
  -- 内容寻址正文；历史镜像与当前文档共享，避免重复存储

kb_folders(path_key TEXT PRIMARY KEY, path TEXT NOT NULL, description TEXT NOT NULL,
           created_change_seq INTEGER NOT NULL, updated_change_seq INTEGER NOT NULL)

kb_documents(id TEXT PRIMARY KEY,            -- UUID，移动后不变
             path TEXT NOT NULL, path_key TEXT NOT NULL UNIQUE, folder_path TEXT NOT NULL,
             title TEXT NOT NULL, summary TEXT NOT NULL, summary_state TEXT NOT NULL,
             keywords_json TEXT NOT NULL, body_sha256 TEXT NOT NULL REFERENCES kb_blobs(sha256),
             line_count INTEGER NOT NULL, embed_policy TEXT NOT NULL,   -- summary | full | none
             provenance_json TEXT NOT NULL,      -- [{kind: file|url|session, ref, sha256?, observedAtMs}]
             revision INTEGER NOT NULL,
             created_change_seq INTEGER NOT NULL, updated_change_seq INTEGER NOT NULL)

kb_changes(seq INTEGER PRIMARY KEY AUTOINCREMENT, at_ms INTEGER NOT NULL,
           actor_kind TEXT NOT NULL,             -- ai | user | system
           session_id TEXT, turn_id TEXT, tool_call_id TEXT,
           operation TEXT NOT NULL,              -- create|update|edit|move|delete|folder|delete_folder|ingest|restore|import
           note TEXT NOT NULL,                   -- AI 提供的变更说明，可空字符串
           restore_target_seq INTEGER)

kb_change_items(change_seq INTEGER NOT NULL REFERENCES kb_changes(seq), ordinal INTEGER NOT NULL,
                entity TEXT NOT NULL,            -- document | folder
                entity_key TEXT NOT NULL,        -- 文档 id 或目录 path_key
                before_json TEXT, after_json TEXT,  -- 完整行镜像（正文以 body_sha256 引用）；新建 before 为空，删除 after 为空
                PRIMARY KEY(change_seq, ordinal))

kb_chunks(id TEXT PRIMARY KEY,                   -- {document_id}:{ordinal}，每次写入重建
          document_id TEXT NOT NULL REFERENCES kb_documents(id) ON DELETE CASCADE,
          ordinal INTEGER NOT NULL, kind TEXT NOT NULL,   -- summary | body
          heading_path TEXT NOT NULL, start_line INTEGER NOT NULL, end_line INTEGER NOT NULL,
          text TEXT NOT NULL, content_sha256 TEXT NOT NULL, embed INTEGER NOT NULL)

kb_vector_state(content_sha256 TEXT NOT NULL, vector_space_id TEXT NOT NULL,
                state TEXT NOT NULL,             -- pending | ready | failed
                attempts INTEGER NOT NULL, last_error TEXT NOT NULL, updated_at_ms INTEGER NOT NULL,
                PRIMARY KEY(content_sha256, vector_space_id))

kb_custom_words(...)  -- 沿用现有自定义词表语义
-- FTS 虚拟表仍为派生物，按现有 fts.rs 方式由 kb_chunks 构建
```

切块规则：每篇文档生成一个 `kind = summary` 切块（标题 + 摘要 + 关键词）与若干 `kind = body` 切块
（复用 `chunk_markdown`）。FTS 索引全部切块；`embed` 由文档 `embed_policy` 决定：`summary` 只嵌入摘要切块，
`full` 嵌入全部，`none` 都不嵌入。

所有写入在库级互斥（沿用 `library_mutation_guard`）下单事务完成：更新实体 → 写 `kb_changes` 与
`kb_change_items` → 重建受影响文档的切块与 FTS 行 → 为需要嵌入且无 `ready/pending` 状态的
`content_sha256` 插入 `pending` → 递增 `head_change_seq` → 提交后唤醒向量工作器。

## 6. 变更日志与还原

- 每次 AI 工具调用（含一次搬运多个文件）恰好产生一条 `kb_changes`。人工只有还原与套件导入两种动作
  会改变内容，同样各记录一条，以 `actor_kind` 区分。
- 还原到 `target_seq`：在库级互斥下，按 `seq` 倒序对 `seq > target_seq` 的每条变更应用 `before_json`，
  得到目标状态；然后把“当前状态 → 目标状态”的差异作为一条 `operation = restore`、
  `restore_target_seq = target_seq` 的新变更写入（因此还原可再次被还原）。不删除任何历史行。
- 还原后重建受影响文档的切块与 FTS；向量按 `content_sha256` 复用，缺失的标为 `pending`。
- 并发：AI 后续写入若携带的 `expectedRevision` 已因还原改变，按正常修订冲突拒绝，AI 重新读取后再写。
- 历史接口返回分页摘要（seq、时间、执行者、会话、操作、说明、影响条目数），详情返回条目镜像供界面做差异展示。

## 7. 索引模式与向量流水线

### 7.1 模式

| 模式 | FTS | 向量 | `knowledge_search` |
| --- | --- | --- | --- |
| `browse` | 否 | 否 | 该库不参与检索，只能列目录与读取 |
| `bm25` | 是 | 否 | Jieba BM25 |
| `vector` | 否 | 是 | 纯向量 |
| `hybrid` | 是 | 是 | BM25 与向量 RRF 融合（沿用 `RRF_K`） |

模式存于 `knowledge_libraries.settings_json`。创建时必选；选含向量的模式时必须同时选择嵌入路由，
维度与模型指纹在创建时冻结（沿用 `KnowledgeEmbeddingSettings` 的指纹与维度校验思路，删除整理相关字段）。

### 7.2 模式变更（升级与降级）

- 增加能力（如 `browse → bm25`、`bm25 → hybrid`）：设置立即保存为目标模式，并启动后台构建作业；
  构建期间该能力状态为 `building`，检索结果按库报告“FTS 构建中/向量未就绪 N 条”，不得静默把
  `hybrid` 当作 `bm25` 而不报告（AGENTS 第一节第 6 条）。
- 移除能力：立即删除对应派生索引（FTS 表或 LanceDB 表与 `kb_vector_state` 行）。
- 变更作业沿用现有 operations 框架，支持取消、进度、服务重启后自动恢复（FTS 重建可重跑；向量依状态表继续）。

### 7.3 向量工作器

- 每库一个后台工作器，在写入提交后、服务启动后、人工重试后被唤醒；按批取 `pending` 状态，
  经现有嵌入调用者（`knowledge_embedding_caller` / 注册）请求嵌入，写入 LanceDB，标记 `ready`。
- LanceDB 每个向量空间一张表，行主键为 `content_sha256`；检索命中后经 `kb_chunks.content_sha256`
  映射回当前切块（只返回当前存在的切块）。删除文档不立即删除向量行，以便还原复用；历史压缩时再回收（本方案不做）。
- 失败：单项失败记录 `last_error` 与 `attempts`，自动重试至上限（第 12 节常量）后保持 `failed`，
  不再自动重试；整个嵌入路由不可用时整批标记失败并在库状态中报告。
- 状态查询接口返回 pending/ready/failed 计数与失败条目（文档路径、切块、错误），供人工补生成（D4）。

### 7.4 向量空间变更

更换嵌入路由或维度即新向量空间：新建表、全部需嵌入的 `content_sha256` 在新空间置 `pending`，
全部 `ready` 后切换活动空间并删除旧表；切换前检索继续使用旧空间。

## 8. AI 工具契约

工具描述、Schema、结果与错误全部英文（AGENTS 第一节第 3 条）。结果为紧凑 Markdown，延续
`tool-result-presentation` 契约与现有 `result_markdown.rs` 风格；读到的内容始终标注为不可信参考资料。

| 工具 | 计划模式 | 说明 |
| --- | --- | --- |
| `knowledge_list`（取代 `knowledge_browse`） | 可用 | 省略 `libraryId` 时列出本回合启用的库（名称、用途说明、索引模式、文档数、待写摘要数）；指定时列出 `path` 下的子目录（含说明、文档数）与文档（id、路径、标题、摘要或 `[summary pending]`、修订号），支持 `depth`（1–3）、`cursor`、`limit` |
| `knowledge_read` | 可用 | `libraryId` + `documentId` 或 `path`，可选 `startLine`/`endLine`；删除 `targetType`/切块读取（检索结果直接给文档 id 与行范围） |
| `knowledge_search` | 可用 | `queries`、`libraryIds`、`topK`；删除 `retrievalMode` 参数，每库按自身模式检索；本回合启用的库全是 `browse` 时不注册此工具；结果附每库模式与索引滞后情况 |
| `knowledge_write` | 不可用（D6） | 见下表；每次调用一条变更，返回变更 seq、受影响文档 id/路径/新修订号、向量待生成数 |

`knowledge_write` 采用 `action` 平铺参数（与 `file` 工具一致，避免依赖 `oneOf`），服务端按动作校验必填项：

| action | 必填 | 可选 | 语义 |
| --- | --- | --- | --- |
| `create` | `path`、`title`、`summary`、`body` | `keywords`、`embed`、`sources`、`note` | 路径已存在则拒绝 |
| `update` | `documentId`/`path`、`expectedRevision` | `title`、`summary`、`body`、`keywords`、`embed`、`sources`、`note` | 给出的字段整体替换 |
| `edit` | `documentId`/`path`、`expectedRevision`、`edits[{oldText,newText,replaceAll?}]` | `summary`、`note` | 精确匹配替换，语义对齐 `file_edit`；匹配不到或不唯一即拒绝 |
| `move` | `documentId`/`path`、`expectedRevision`、`newPath` | `note` | id 不变 |
| `delete` | `documentId`/`path`、`expectedRevision` | `note` | 可经还原恢复 |
| `folder` | `path`、`description` | `note` | 创建目录或更新说明 |
| `delete_folder` | `path` | `note` | 仅空目录 |
| `ingest` | `sources[]`、`targetFolder` | `summaries{sourcePath: summary}`、`embed`、`note` | 见第 9 节 |

工具描述必须讲清：知识库是 AI 有意识维护的结构化长期资料，与自动萃取、会衰减的 VMM 记忆不同，
也不是工作区文件；写入前先 `knowledge_list` 了解现有结构；库的用途说明决定应写什么。

### 8.1 暴露链（AGENTS 第一节第 4 条，逐项核对）

1. 注册：`tooling/native.rs` 构建四个工具；`KNOWLEDGE_*_TOOL_ID` 常量更新。
2. 识别：`native_tool_id_is_knowledge_tool` 包含 `knowledge_list`、`knowledge_write`，移除 `knowledge_browse`。
3. 请求构建：`bridge.rs` 现有“无访问快照不注册”规则覆盖新工具；新增“全部为 `browse` 时不注册 `knowledge_search`”。
4. 全局可用：`append_universal_native_tools` 同步。
5. 模式可见性：`knowledge_write` 的 `available_in_plan = false`。
6. 权限：`classify_native` 对知识库工具保持 `Ungoverned`（D3）；`ingest` 读取本地文件与 `file_read`
   同级（`file_read` 本身为 `Ungoverned`），在测试中显式断言这两点，防止将来被默认规则改变。
7. 专职会话：两档都保留全部知识库工具，包括写入；同步 `dedicated-sessions.md`。
8. 子代理：沿用 `child_request_overrides` 继承访问快照；变更日志记录子会话 id。
9. 执行：写入前再次校验库仍授权给项目（沿用 `authorized_runtime`），并记录 session/turn/tool_call id。
10. 结果投影：WebUI `native-semantic-tool-presentation`、`v1-tool-domain-presentation` 与 TUI 对应展示。

## 9. 搬运（ingest）

- `sources` 每项为文件路径或 glob；路径解析、`${env:NAME}` 展开与文本解码完全复用
  `vulcan_host_tools::file_runtime`（与 `file_read` 同一入口，不另写路径规则）；目录遍历复用现有 `file`
  工具的遍历与忽略逻辑（执行会话先查证具体函数，不得新建第二套）。
- 只接受文本文件；二进制、解码失败、超过单文件上限的文件逐项拒绝并在结果中列出原因，其余照常写入。
- 目标路径 = `targetFolder` + 源相对路径（glob 基准目录之下的相对部分），扩展名保留。
- 标题取首个标题行，否则取文件名（复用 `extract_document_title`）。
- `provenance` 记录 `kind: file`、规范绝对路径、`sha256`、观测时间。
- 同一目标路径再次搬运：源 `sha256` 未变 → 不产生变更并在结果中标为 unchanged；已变 → 更新正文，
  修订号递增，已有摘要的 `summary_state` 置 `stale`。
- `summaries` 中给出的摘要直接写入（`provided`），其余为 `pending`；结果为每个新文档返回标题大纲
  （仅标题行，条数有上限），使 AI 能在不重读正文的前提下补写摘要。
- 整次搬运是一条 `operation = ingest` 的变更，包含全部条目。

## 10. 服务 RPC

删除：`service_list_knowledge_sources`、`service_import_knowledge_directory`、`service_preview_knowledge_import`、
`service_preview_knowledge_display`、`service_update_knowledge_ignored_files`、`service_build_knowledge_display_index`、
`service_build_knowledge_vector_index`、`service_cancel_knowledge_import`、`service_get_knowledge_import_job`。

改造：

- `service_create_knowledge_library`：名称、用途说明、索引模式、嵌入路由（含向量模式时必填）。
- `service_set_knowledge_library_settings`：只允许名称、用途说明、自定义词；内容相关字段一律拒绝。
- `service_list_knowledge_library_documents` / `_folders` / `service_read_knowledge_library_document` /
  `service_search_knowledge_library`：适配新结构，只读。
- `service_rebuild_knowledge_index` / `service_repair_knowledge_index` / `service_get_knowledge_index_integrity`：
  对象改为新派生索引。
- `service_export_knowledge_package` / `service_inspect_knowledge_package` / `service_import_knowledge_package` /
  `service_browse_knowledge_packages`：新套件格式（第 11.3 节）。
- `service_list_knowledge_documents` / `service_read_knowledge_document` / `service_search_knowledge`、
  会话选择两个方法：适配，删除 `retrievalMode`。

新增：

- `service_set_knowledge_index_mode`（目标模式、嵌入路由）→ 返回作业 id。
- `service_get_knowledge_operation`、`service_cancel_knowledge_operation`（取代导入作业方法）。
- `service_list_knowledge_changes`（分页）、`service_get_knowledge_change`（条目镜像）。
- `service_restore_knowledge_library`（`targetSeq`；人工确认后调用）。
- `service_get_knowledge_embedding_status`、`service_retry_knowledge_embeddings`（全部失败项或指定文档 id）。

服务内核删除整个整理流水线（固定提示词、批次分组、结构修复重试、整理观测、`knowledge_display_*`），
保留并改造嵌入调用者与注册。`rpc_dispatch/knowledge_base.rs::RPC_METHODS` 与 `rpc_dispatch.rs` 测试同步。

## 11. 其他持久格式

### 11.1 应用数据库（AGENTS 第一节第 12 条、`debug-database-maintenance.md`）

应用库保持 v1，直接修改 `build_application_schema_script()`：

- `knowledge_libraries`：删除 `last_completed_import_at_ms`（最近变更时间改由每库 `kb_changes` 提供）。
- 用 `knowledge_operations`（id、library_id、kind、state、phase、total_items、processed_items、current_item、
  warnings_json、observability_json、error_message、created/started/finished 时间）取代 `knowledge_import_jobs`。

同时扩展 `vulcan-debug-db` 的明确映射并补回归测试：

- `last_completed_import_at_ms` 允许移除（原值留在备份）。
- `knowledge_import_jobs` 属于被删除的导入功能：仅当不存在 `queued`/`running` 行时允许删除整表（原表留在备份），
  否则停止；不把旧导入计数猜测映射为新列。
- 完成后在本机 Debug 库执行 `--check`、`--apply`、再 `--check`。执行前确认 Core 已退出。

### 11.2 会话元数据

从 `KnowledgeLibrarySelectionConfig` 删除 `retrieval_mode` 与 `KnowledgeRetrievalMode`，
`METADATA_SCHEMA_VERSION` 7 → 8，并在 `session-metadata-migrations.md` 版本历史追加一句。
这会使全部旧本地会话无法打开（只能经 `service_delete_unsupported_conversation` 删除），与该契约既有做法一致。
子执行覆盖中的 `retrieval_mode` 同步删除；执行会话须查证它所在的持久文件及其版本契约，按对应契约递增。

### 11.3 知识库套件

`KNOWLEDGE_PACKAGE_VERSION` 递增，只接受新版本（删除现有接受 `1` 的分支）。新套件为 zip：
`manifest.json`（格式、版本、库名、用途说明、索引模式、嵌入配置快照）+ `content.jsonl`（目录与文档，正文内联），
不再打包 SQLite 文件、向量或历史（D10）。导入时校验大小与条目上限（沿用现有有界解压），
以一条 `operation = import` 的变更写入。

## 12. 边界常量（每个只定义一处，AGENTS 第一节第 11 条）

在 `vulcan-workspace-store` 知识库模块集中定义，工具 Schema 与描述通过引用派生：

| 常量 | 建议值 |
| --- | --- |
| 单篇正文最大字节 | 1 MiB |
| 单次搬运最大文件数 | 200 |
| 单次搬运最大总字节 | 32 MiB |
| 摘要最大字符 | 2,000 |
| 关键词最大个数 | 32 |
| 路径最大深度 / 段最大字符 | 8 / 128 |
| 搬运结果大纲每篇最多标题行 | 40 |
| 向量自动重试上限 | 3 |
| 单次读取最大行数 | 沿用 `MAX_KNOWLEDGE_READ_LINES` |
| 检索查询数 / 查询长度 | 沿用现有常量 |

## 13. WebUI 与 TUI

WebUI（`components/features/knowledge/`）：

- 删除：来源目录、导入确认、跳过文件、展示库预检/构建、忽略文件相关对话框与钩子。
- 创建对话框：名称、用途说明、索引模式（四选一，含成本说明）、嵌入路由（含向量时）。
- 库页面：只读目录树与文档阅读（无任何编辑入口，D1）；变更历史列表与详情差异；“还原到此处”需确认对话框；
  索引模式变更与作业进度；向量状态（待生成/失败计数、失败条目与错误、全部重试/按文档重试）；
  项目绑定；套件导入导出；删除库；旧格式库只显示原因与删除按钮。
- 输入区知识库选择与专职向导：删除检索策略选择，只选库。
- 工具展示：新增 `knowledge_list`、`knowledge_write` 呈现（写入显示动作、路径、变更 seq）。
- 全部文案走 `crates/vulcan-i18n/catalogs/webui/`，删除失效键（所有语言同步）。
- 提交前执行完整 `pnpm run build` 检查体积预算；新样式延迟加载，不提高预算。

TUI：先读 `apps/vulcan-code-tui/AGENTS.md`；服务调用与 WebUI 一对一镜像（函数、方法、参数）；
页面覆盖同等功能（还原需确认）；模拟后端同步；只用模拟后端验证；文案走 `catalogs/tui/`。

## 14. 契约与文档

- 新建 `docs/contracts/knowledge-base.md`：人工/AI 边界、存储与变更日志、还原语义、索引模式与向量状态、
  搬运规则、工具契约、边界常量出处。
- `AGENTS.md` 第二节新增一行（红线示例：“内容只能经 AI 工具写入；每次写入恰好一条变更且可还原；
  搬运正文不经过模型；索引模式变化不得静默降级”）。
- 更新 `tui-service-adaptation.md`（页面表与 RPC 映射）、`dedicated-sessions.md`、`tool-result-presentation.md`、
  `session-metadata-migrations.md`、`debug-database-maintenance.md`。
- `docs/completed/` 下的旧知识库文档不改（历史记录）。

## 15. 执行阶段与提交

每个阶段一个或多个独立提交，提交信息英文，结尾附项目要求的署名行；不推送。

1. **存储层**：新每库结构、实体写入 API、变更日志、还原、切块与 FTS 派生、旧版本拒绝；删除 `import`、
   `file_selection`、`raw_corpus`、`display` 及其测试。
2. **向量**：按内容哈希的 LanceDB 表、状态表、工作器、重试、模式升降级、向量空间切换。
3. **套件**：新格式导出、检查、导入。
4. **AI 工具**：四个工具、搬运、暴露链十项、结果 Markdown；删除访问快照中的 `retrieval_mode`。
5. **服务内核**：删除整理流水线，新 RPC，工作器托管与启动恢复。
6. **应用库与会话元数据**：DDL、维护工具映射、`METADATA_SCHEMA_VERSION`。
7. **WebUI**。
8. **TUI**。
9. **契约、文案、`AGENTS.md`**。

## 16. 测试要求（只约束行为与格式，AGENTS 第四节第 8 条）

- 写入：每个动作产生恰好一条变更；`expectedRevision` 不符被拒；路径冲突、非法路径、超限被拒；移动后 id 不变。
- 还原：对随机操作序列，还原到任一 seq 后的完整内容（文档与目录的规范化快照）等于该 seq 当时记录的快照；
  还原本身可被再次还原；还原不删除历史。
- 搬运：未变源不产生变更；变化源递增修订并把摘要置 `stale`；二进制与超限文件逐项拒绝且不影响其他文件；
  正文字节与源一致（编码转换后）。
- 向量（用假嵌入器）：写入后进入 `pending`；成功 `ready`；失败计次并在上限后停在 `failed`；人工重试恢复；
  还原复用已有哈希向量；向量空间切换期间检索仍用旧空间。
- 模式：四种模式检索行为；升级作业期间结果报告未就绪；降级删除派生索引；`browse` 库不注册 `knowledge_search`。
- 暴露：无快照不注册；计划模式无 `knowledge_write`；专职两档保留；权限分类为 `Ungoverned`。
- 格式：旧库版本、旧套件版本、旧会话元数据版本被明确拒绝；新套件往返导出导入内容一致。
- 维护工具：两项新映射的成功与拒绝路径。
- 不得锁定 AI 文案或工具描述的精确措辞；断言错误码、动作与状态。

## 17. 验证命令

```powershell
rtk cargo test -p vulcan-workspace-store --lib knowledge_base -- --test-threads=1
rtk cargo test -p vulcan-workspace-store --features debug-db-maintenance --bin vulcan-debug-db -- --test-threads=1
rtk cargo test -p vulcan-workspace-store --lib data_plane::tests -- --test-threads=1
rtk cargo test -p vulcan-workspace-store metadata_access --lib -- --test-threads=1
rtk cargo test -p vulcan-agent-runtime --lib knowledge
rtk cargo test -p vulcan-agent-runtime --lib tool_permission
rtk cargo test -p vulcan-agent-runtime --lib dedicated_session
rtk cargo test -p vulcan-service-kernel --lib knowledge -- --test-threads=1
rtk cargo test -p vulcan-service-kernel --lib rpc_dispatch -- --test-threads=1
rtk cargo check --workspace --all-targets
```

WebUI：知识库相关 vitest、`pnpm run build`（体积预算）。TUI：`cargo test -p vulcan-code-tui`。
最后执行 `make check`。运行 cargo 前清除 `NoDefaultCurrentDirectoryInExePath`。不调用付费服务；
向量测试只用假嵌入器。服务内核重新编译耗时长，最终端到端验证前先告知用户。

## 18. 非目标

历史压缩与向量回收；人工编辑内容；知识库之间的引用或链接；非文本文件（PDF、Office）搬运；
URL 直接搬运（AI 可先用现有网页工具取得内容再 `create`）；套件携带向量或历史。

## 19. 交付审核要求

执行会话完成后在本节下方追加“执行记录”：

- 每个阶段的提交哈希与一句说明。
- 与本方案的偏差及理由（含第 3 节复核时发现的事实差异）。
- 第 17 节每条命令的结果（通过数或失败输出）。
- 本机 Debug 库维护命令的 `--check` 输出。
- 未完成项与已知风险。

## 执行记录

### 执行基线与约束

- 开始日期：2026-10-15；基线：`5b6d65537603f79f38b23cd34dd07294ff1a0646`，初始工作树干净。
- 工作分支：`codex/ai-managed-knowledge-base`。
- 不实现老版本数据兼容、迁移或自动降级；应用开发数据库仅经独立显式维护入口备份后同步。
- 审核规则：实现及验证完成后逐轮核对完整变更，发现问题立即修复并重置连续通过计数；累计连续五轮无问题后交付。
- 当前阶段：存储、向量、套件与工具阶段已提交，继续服务内核及开发数据库维护；界面与最终审核尚未完成。

### 第一阶段：存储层

- 阶段提交：`3b11db3`（本地，未推送）。

- 已替换每库结构为版本 2，删除来源扫描、忽略选择、原始/整理库模块与旧测试；未增加旧格式读取或迁移分支。
- 正文按哈希保存；写入动作、搬运批次及人工还原均在库级锁与单个 SQLite 事务中维护实体、历史镜像、切块和全文索引。
- 授权在变更锁内复查；精确替换检查匹配次数；移动保持文档身份；还原保留全部历史并重新分配单调修订号，防止旧令牌恢复有效。
- 保留已有宿主绑定的路径、重解析点与删除边界校验；格式检查在绑定缓存命中时仍执行。错误格式可列出原因并删除。
- 增加全文完整性、事务重建、自定义词典与只读目录分页。空嵌套目录的隐式祖先同样可浏览。
- 阶段验证：知识库定向测试 15 项通过；`cargo check -p vulcan-workspace-store --all-targets` 通过；修改文件格式化与 `git diff --check` 通过。
- 当前编译仍提示下一阶段将接回的向量绑定方法未使用；跨 crate 消费者将在后续工具、服务、两端界面阶段统一适配。尚未执行全工作区验收与最终五轮审核。
- 事实差异：原套件版本实际为 2，新格式版本设为 3；原每条检索查询上限为 512 字符，已保留该取值并集中定义。新增 NFC 依赖使用锁文件中既有的 0.1.25。

### 第二阶段：向量与索引作业

- 阶段提交：`d5304ad`（本地，未推送）。

- 为让模式切换与后台作业可单独验证，提前在应用权威结构加入新的通用知识库操作表；第六阶段再删除旧导入表并补齐显式开发库维护映射。此为阶段依赖调整，最终结构与不兼容原则不变。
- 向量表按冻结语义空间与内容哈希寻址；当前切块决定检索成员，历史向量不会挤占当前结果。所有模式按库自身声明检索并返回能力状态，不回退到其他模式。
- 每库工作器共享宿主唤醒代次，覆盖写入、还原、重试与启动恢复。逐项检查维度及有限数值，失败最多自动尝试三次；就绪状态只在 Lance 写入成功后发布。
- 模式切换与重试使用持久稳定任务身份；取消状态不会因迟到结果而复活。空间更换期间保留旧空间，切换清理等待已有读取租约结束。
- 空向量库可在没有推理请求的情况下就绪；默认仅摘要与显式不嵌入策略已覆盖。正文读取保留原字节，并使 CR、CRLF、LF 的逻辑行与切块一致。
- 阶段验证：知识库定向测试 25 项通过；应用数据面测试 30 项通过、1 项既有性能测试忽略；严格 Clippy 全目标检查通过；后端中文字符串检查通过。
- 仍未执行跨 crate、WebUI/TUI 与最终完整验收；新作业表尚未用于真实本机开发库维护。最终连续五轮审核尚未开始。

### 后续实施检查点

- 套件使用版本 3，只含配置与当前目录/文档；旧套件拒绝，导入建立单条新历史，不携带索引或向量。
- 工具与服务必须同步完整暴露链，注册执行器时核对宿主生命周期，避免持有循环引用；服务层补齐重建作业入口。
- 后续审核需核对多查询融合期间的并发内容变化、历史详情的有界展示、搬运结果大纲的输出预算，并补齐生成代码的双语变量与参数说明。

### 第三阶段：当前内容套件

- 新版套件严格使用版本 3，只允许两个归档条目：配置清单与当前目录/文档数据；不导出数据库、历史或向量。
- 预检与导入均执行有界解压、完整文件指纹、条目名称、逻辑计数、内容摘要与实体结构校验；先校验再建库。导出使用同一逻辑快照及同目录原子落盘。
- 导入只创建新的独立库，建立一条人工导入历史并重新生成向量；删除旧“替换现有库”入口，以同时满足永久历史及导入从新历史开始的要求。失败清理与写入共用库锁，只清理确认未被写入的空库。
- 阶段验证：知识库定向测试 34 项通过，覆盖精确正文往返、摘要状态、词典、空库导入、旧版本拒绝、恶意归档与有界读取、错误指纹无副作用、向量重新生成和并发清理保护。
- 全工作区、界面与最终五轮审核尚未开始，当前只完成存储侧套件边界。

### 第四阶段实施中的依赖调整

- 第三阶段提交：`fc463b0`（本地，未推送），套件阶段严格全目标 Clippy 亦通过。
- 工具访问上下文移除旧检索回调，直接使用宿主共享存储中注册的嵌入执行器。新增只影响回合可见性的“存在可检索库”标记，不持久化第二份库模式。
- 因工具及子执行访问类型立即依赖持久字段形状，提前完成第六阶段的会话类型调整：元数据版本 8，子执行配置版本 2；已沿源码确认子执行配置位于同一 metadata.json 的 child_execution_profile 中，仍严格拒绝旧版本。
- 为记录真实回合，持久工具准备回执及宿主调用上下文补传权威回合身份，原生请求接收完整调用上下文；未从 API 或工具编号猜测回合。
- 方案第 8 节 sources 同时表示字符串数组与来源对象数组，与当前轻量 Provider Schema 投影（只选择一个非空类型）冲突。工具接口将 sources 固定用于搬运路径，另用 provenance 表示创建/更新来源对象；存储来源语义不变，避免联合结构静默丢失。
- 补充唯一来源的大纲累计字节预算、标题字符上限、列表深度常量；文件搬运复用原生路径、快照解码及 collect_files 遍历，结果只含元数据与有界真实 Markdown 标题。

- 后续服务集成必须核对套件导入的新库消费方身份：旧归档中的 consumer_profile_id 不能成为新库工作器的授权身份。服务启动注册、空间更换期间旧空间查询路由及新空间构建路由需按请求中冻结的嵌入配置执行，不能让单个已改写配置覆盖旧查询。

### 第四阶段：工具与访问边界

- 四个原生工具已接通注册、识别、全局候选、实时授权、动作校验和 Markdown 结果；新增最小宿主变更回执供界面显示序号、文档身份及修订号，不重复正文。
- 无访问快照时不注册工具；全浏览库不注册搜索；计划模式同时移除写入定义与描述，调用闸门亦保留禁止写入策略。专职两档均保留四工具。
- 宿主搬运复用既有遍历与忽略、环境及相对路径解析、原始快照和文本编解码；摘要关联失败有逐项诊断，二进制或超限来源不影响其他有效文件。单文件与批次目标边界按解码 UTF-8 计算，原始字节仍受原生文件运行时上限约束。
- 通过真实持久化测试核对实际子会话、回合及本地工具身份；删除旧检索策略字段，子请求仅持久化项目与库身份上限，恢复时重新读取当前授权与模式。
- 阶段验证：原生知识工具 13 项通过；宿主搬运 2 项通过；权限 17 项通过；专职过滤 1 项通过；工具执行桥 24 项通过；会话知识选择 2 项通过；准备回执重试/重载 1 项通过；子配置 6 项通过；元数据读取 4 项通过；存储知识库 36 项通过；严格 agent-runtime 全目标 Clippy 与后端中文字符串检查通过。
- 版本变更曾使一个既有“未来版本=2”的测试夹具失效；已改为当前版本加一并重新通过，同时增加旧版本拒绝路径。
- 尚未适配服务内核及两端展示，仍未宣称端到端可用，最终五轮审核计数保持为零。

### 第五阶段实施记录

- 第四阶段提交：`40866a5`（本地，未推送）。
- 为避免重复授权来源，冻结路由不再持久化消费方 ID；实际库 ID 随每次嵌入请求传递，服务按“实际库 ID + 冻结空间指纹”派生唯一内部配置档。套件不能指定其他库的授权，旧字段严格拒绝，不增加兼容分支。
- 创建时先准备物理存储，再以一笔应用事务发布注册行及全部初始授权；整个区间持有库锁，避免初始绑定失败时已有并发写入被清理。
- 为历史差异展示增加条目分页与累计字节预算，以及按精确变更点、文档和侧别读取历史正文的有界入口；当前文档读取改为索引查询精确选中行，复用相同行窗口逻辑。
- 为重建补齐稳定作业入口；服务恢复按持久作业意图识别重建与取消，覆盖应用库和每库元数据提交之间中断的窗口。
- 删除内核旧整理流水线与旧导入、来源、整理构建 RPC；新增历史分页、历史正文、还原、模式、通用作业及向量状态入口。会话提交和子会话恢复不再接受检索策略字段，只复核授权库集合及当前模式。
- 向量空间指纹冻结实际路由、连接、绑定、能力与执行修订、维度及模型能力声明；目录代次与展示名称不参与指纹。三种已声明距离分别传给 LanceDB，未声明距离明确拒绝，不猜测默认度量。其他引擎调用者仍使用既有欧氏距离入口。
- 嵌入输入预算从正文、输入身份及批次上限派生；每空间配置档不可改写，并在与删除清理相同的配置事务内复核库归属，避免延迟工作器重建已删除库的配置。执行器通过弱引用避免服务所有权循环，先安装再恢复作业。
- 多查询融合以库、切块、内容哈希和文档修订区分代次；最终重新核对当前行。新增闸门回归覆盖旧查询、并发内容更新、新查询的真实顺序。
- 历史响应使用统一累计字节预算；来源数量、引用长度、目录说明及审计说明补齐唯一常量，并同步工具 Schema。词典单项变更在库锁及事务内完成，不使用读取列表后整表替换。
- 阶段验证：服务内核知识库相关测试 27 项通过（含模拟宿主注册权限及撤销）；真实 LanceDB 三种距离排序回归 1 项、既有写入检索及过滤回归各 1 项通过；服务请求格式 3 项通过；存储知识库 40 项通过；RPC 路由 15 项通过；五个相关 crate 的严格全目标 Clippy 通过；后端中文字符串及差异空白检查通过。全工作区、界面及最终五轮审核尚未完成。

### 第六阶段实施记录

- 第五阶段提交：`035702a`（本地，未推送）。
- 应用数据库继续使用版本 1；权威 DDL 移除旧导入时间与旧导入作业表，采用第二阶段已加入的通用知识库操作表。元数据版本及子配置字段变更已在第四阶段提前完成，本阶段不再重复递增。
- 显式维护工具增加废弃整表映射；只接受已查证列形状且无排队、运行任务的旧表，原始表与时间戳保留于完整备份。未知列、额外索引、视图、触发器及其他表外键依赖均停止，不猜测转换计数。
- 同步前只读核对本机 Debug 库：旧知识库注册列及 23 个旧任务列与当前移除映射一致，旧任务表为空。
- 实际执行前两次检查进程，未发现 Vulcan Core；仅存在独立代理服务。首次 `vulcan-debug-db --check` 按预期以退出码 1 报告：需同步知识库注册及通用操作表、移除旧导入作业表、创建通用操作索引，无其他结构差异。
- `vulcan-debug-db --apply` 退出码 0，完整备份：`C:\Users\lingeasy\.vulcan-debug\code\sqlite\app-workspace-before-sync-yimv3l.sqlite3`；程序确认备份完整性、全部保留值及类型、版本与外键完整性后提交。随后 `--check` 退出码 0，精确输出：`Schema v1 is current; no changes.`。再以只读结构查询确认旧字段及旧任务表均已移除，新操作表存在。
- 阶段验证：维护工具 9 项通过；数据面 30 项通过、1 项既有性能测试忽略；元数据读取 4 项通过；新 DDL 下知识库 40 项通过；包含维护特性的严格全目标 Clippy 与差异空白检查通过。应用启动未增加迁移或旧格式兼容入口。

### 第七阶段实施记录

- 第六阶段提交：`656041d`（本地，未推送）。
- 知识库类型及 36 个服务函数改为当前封闭请求和响应。会话与项目调用方同步请求对象；不保留旧策略、来源、整理、语料或覆盖导入别名。
- 管理页接入空库创建、名称及用途、显式四种模式、精确嵌入路由与维度、项目授权与默认选择、原子词典更新、当前内容套件及删除。错误或旧格式库只展示原因及删除操作。
- 内容树按展开动作读取目录，使用服务游标；当前与历史正文分别使用其唯一端点，按服务返回行号续读，复用会话 Markdown 渲染。文档及目录内容不提供人工编辑或搬运控件。
- 永久历史支持摘要及条目分页、精确前后镜像和正文对比、明确确认后的还原；分页使用实际序号和续读偏移，不假定序号连续。
- 索引页独立展示完整性、稳定任务与向量待生成/失败状态；支持取消、全部重试及逐文档重试。轮询等待上一组读取完成，读取失败停止轮询，未知实时任务状态不授权启动或取消任务。
- 共用读取寿命同时防止乱序及离开后返回同一库的旧回调重新激活；变更提交后的读取失败单独报告，仅允许恢复读取，不重放变更。模式及内容变化撤销旧搜索结果，不自动重发付费查询。
- 四工具展示同步当前 Markdown 契约；写入附最小宿主回执，文件搬运正文不进入卡片。移除旧来源、整理及忽略界面、过期测试和样式，以及不再需要的超大组件和反馈空壳豁免。
- 11 个注册语言同步新文案并移除无人使用的旧知识库键；语言清单未复制。共享语言 Rust 测试 5 项通过。
- 阶段验证：WebUI 全量测试 2446 项通过、1 项既有用例跳过；补充的最终任务读取门禁回归所在文件 3 项通过；知识库与相关调用方定向 91 项通过。最终完整构建通过 lint、类型、架构、Vite 与预算检查，工作台样式 gzip 73992 字节，小于原有 74000 字节上限，未提高预算。
- 尚待第八阶段 TUI、契约收尾、全工作区门禁与连续五轮完整审核；审核计数仍为零。全仓措辞门禁另外发现此前阶段两个文件的中文输入断言需改为输入往返行为断言，将在最终门禁修正，不能据此声称全仓检查已通过。
