# OCE 代码语义检索逻辑提取

## 当前 VCE 实施边界补充

本文件中的 OCE/CBM 对照内容只用于恢复行为和划分边界。当前 VCE 不依赖 CBM；CBM 是待移除能力，不作为代码索引来源、调用接口、运行时依赖或降级实现。代码语义数据只通过 VCE 的 `VceEngine` 和宿主注册的 `VceAiPort`、`VceSourcePort`、`VceIndexPort` 处理。

VulcanCode 的文件/Git 监听、AI 注册、VLDB/LanceDB 存储、工具注册和页面管理属于上游宿主。VCE 接收 `RefreshRequest`，按需通过源码端口读取不可变正文，返回检索结果、管理快照或类型化管理查询结果，不在内部创建第二套监听器、Provider、数据库或管理页面。

> 本文用于 `vulcan-context-engine` 的 Rust 原生实现设计。
>
> 本文提取的是 OCE/OCE-client 的代码索引、向量处理、检索、同步和结果选择逻辑，不提取 MCP 的进程、stdio、工具注册、FastAPI 路由、CLI 或 HTTP 传输实现。

## 0. 文档结论

OCE 的核心不是一个 MCP 服务，而是一条代码上下文生成流水线：

```text
项目文件快照
  -> 增量变更识别
  -> 文件内容地址化
  -> 语义切块
  -> 代码块向量化
  -> dense / exact / path 多路召回
  -> 多查询融合
  -> 基础重排与可选 LLM 重排
  -> 源码优先、置信度过滤、覆盖度选择
  -> 带路径和行号的代码上下文
```

它解决的是“自然语言问题如何找到当前项目中最相关的代码片段”，而不是：

- 记忆存储；
- 调用图或架构图；
- MCP 工具生命周期；
- 独立向量数据库运维；
- AI 模型自身的工具注册。

### 0.1 Vulcan 目标边界

```text
Vulcan 宿主
├── 项目、会话、权限和工具注册
├── 文件监听、未保存编辑器内容、增量变更
├── 后台任务与索引生命周期
└── 将用户请求转换为 OCE 领域调用

VulcanCode 统一能力注册 / VLDB
├── 注册 LLM、Embedding、Rerank 服务
├── 提供不可变源码快照
├── 实现向量写入、过滤和相似度查询
└── 以独立 namespace 持久化代码、记忆与知识库数据

OCE Rust 库
├── 代码切块逻辑
├── 索引计划和检索管线
├── 召回融合、重排和上下文选择
└── 与存储和模型服务通过抽象接口交互

CodeKit RG
└── 精确文本、正则和路径定位；与 VCE 语义召回互补
```

OCE Rust 库不应直接依赖 VLDB 的具体实现，也不应直接注册 AI 工具。存储、源码、模型和工具注册都由上级提供；VMM-Core 可以复用同一 AI 注册契约，但不承载 VCE 的代码语义业务。

## 1. 当前 OCE 的实现事实

本次提取基于以下仓库中的说明和实现：

- `D:\openvulcan\third_git\oce`：OCE 服务端，`pyproject.toml` 版本为 `0.2.0`；
- `D:\openvulcan\third_git\oce-client`：工作区同步和检索客户端；
- 重点依据 OCE 的 `README.zh-CN.md`、`AGENTS.md`、领域服务、应用服务、索引基础设施和客户端状态实现。

OCE 当前的公开实现使用 SQLite/PostgreSQL 保存元数据，使用 Milvus/Milvus Lite 保存 dense 向量；这只是当前基础设施选择，不是应当复制到 Vulcan 的目标边界。

## 2. 领域数据模型

### 2.1 文件记录与工作区快照

OCE-client 的本地工作区状态不是简单的“当前目录重新扫描”，而是一个带版本的本地清单。每条文件记录包含：

- 相对路径，统一使用 `/`；
- 当前 `blob_name`；
- 最近一次已经提交到索引的 `committed_blob_name`；
- `present`、`skipped` 或 `deleted` 状态；
- 必要时保存内容；
- 文件大小和 `mtime_ns`；
- `filesystem` 或 `explicit` 来源；
- 本地 generation；
- 跳过原因。

工作区快照还包含：

- 当前 checkpoint；
- 当前文件 generation；
- 每个路径的文件记录。

这使同步判断可以区分：

```text
文件当前状态 != 最近已提交状态
```

而不必每次都把整个项目重新送入索引。

### 2.2 Blob：文件内容地址

服务端把文件抽象为 Blob。当前客户端和服务端使用兼容的内容地址：

```text
blob_name = SHA256(normalized_relative_path + UTF-8(file_content))
```

这是 OCE 当前的兼容公式，不是 VCE 与 OCE 存储互通要求。裸拼接无法区分 `("ab", "c")` 与
`("a", "bc")` 这类边界，因此 VCE 使用 `vce-blob-v1` 域标记和规范路径字节长度进行无歧义 framing，
再顺序写入路径与正文。VulcanCode 预计算身份时必须使用 VCE 公式，不能直接复用 OCE 裸拼接值。

路径和内容都参与 hash，因此以下任一变化都会生成新的 Blob：

- 文件内容变化；
- 文件相对路径变化。

Blob 状态：

| 状态 | 含义 |
| --- | --- |
| `pending` | 已接收，等待切块/Embedding/向量写入 |
| `ready` | 可以参与检索；空文件、二进制或忽略文件也可以是 ready |
| `error` | 索引处理失败，保留错误信息和重试状态 |

Blob 还记录路径、大小、语言、文件类型、关联 Chunk、重试次数、最后访问时间和 staging 状态。

### 2.3 Chunk：带源代码位置的内容块

Chunk 是内容地址化的、可定位的源码片段：

- `content_hash = SHA256(chunk_content)`；
- `path`；
- 原始源码文本；
- 1-based、闭区间的 `start_line` 和 `end_line`；
- `chunk_type`，例如 AST、recursive 或专用格式类型。

Chunk 文本必须直接来自原始源码行。不能使用切块器内部经过 strip 的文本作为最终上下文，否则文本内容与报告行号可能不一致。

写入向量索引时，OCE 使用稳定的 Chunk occurrence ID：

```text
chunk_id = SHA256(blob_name + "\\n" + content_hash + "\\n" + start_line + ":" + end_line)
```

VCE 对首行偏移为零的普通 Chunk 严格保留这个 OCE 身份。若硬字符预算把同一物理行切成多个片段，仅后续片段使用 `start_char_offset` 参与扩展身份；否则同一行上的重复文本会碰撞。该偏移按 Unicode 标量计数，并同时进入 source occurrence 去重键、宿主命中校验和 Symbol occurrence 校验。

Embedding 文本不是裸代码，而是：

```text
File: <relative path>

<chunk content>
```

路径前缀让向量同时感知文件位置和代码内容。

### 2.4 Chunk 去重与 Blob 关联

Chunk 内容以 `content_hash` 作为持久化主键；Blob 与 Chunk 通过关联表保存具体出现位置和顺序。这样可以：

- 对相同文本内容进行复用；
- 保留同一 Chunk 在不同 Blob/路径中的出现位置；
- 让最终结果仍能恢复正确路径和行号。

### 2.5 Symbol occurrence

服务端额外维护 `symbol_occurrences`：

- 标识符名称；
- Blob 名称；
- Chunk 内容 hash；
- 标识符类型；
- 起止行号。

当前提取器重点识别两类定义：

- `endpoint`：Tauri/PyTauri command、Web 路由等入口定义；
- `definition`：函数、类、结构、接口等普通定义。

精确检索优先返回 endpoint，再返回普通 definition。它不是向量检索，也不是代码图谱，而是低延迟的标识符定位通道。

### 2.6 Chain 与 checkpoint

Chain 是客户端工作集的版本化集合：

- Chain 使用 UUID；
- version 单调递增；
- members 是 Blob 名称集合；
- checkpoint token 表示 Chain ID 和 version。

检索范围的真实计算是：

```text
scope = (checkpoint.members ∪ added_blobs) - deleted_blobs
```

没有 checkpoint 且没有 `added_blobs` 时，服务端拒绝全库检索；`deleted_blobs` 单独存在不能构成检索范围声明。有效但为空的范围返回空结果，不降级为全库搜索。

## 3. 工作区同步和增量处理

### 3.1 文件准入

OCE-client 的本地扫描规则：

- 使用 `os.walk`，不跟随符号链接目录；
- 默认单文件最大约 1 MiB；
- 含 NUL 字节的内容视为二进制；
- 非 UTF-8 内容不准入；
- 目录和文件使用分层 ignore。

ignore 优先级：

```text
hard rules
  > runtime patterns
  > .oceignore
  > .gitignore
  > default rules
```

hard rule 不能被上层 negation 取消。服务端还有一组依赖、生成物、构建产物、二进制和媒体文件过滤规则；Rust 实现应把客户端和服务端的准入规则收敛成一个权威规则集，避免两边接受范围不一致。

### 3.2 全量 reconcile

全量 reconcile 的处理顺序：

1. generation 加一；
2. 读取上一次本地快照；
3. 保留仍有效的 `explicit` 未保存文件覆盖层；
4. 使用 ignore 规则扫描工作区；
5. 对每个文件先检查旧记录的 `mtime_ns`、大小和 Blob 名称；
6. 三者一致时不重新读取文件，只复用旧 Blob 名称；
7. 不一致时读取文件、重新 stat、计算内容地址；
8. 如果磁盘文件与 explicit overlay 冲突，保留 overlay；
9. 分批写入本地状态；
10. 扫描结束后把旧清单中未出现的文件标记为 missing/deleted；
11. 保存新的 generation，返回快照。

这是 OCE-client 最重要的性能思想：`mtime + size` 只用于避免读取和 hash，最终内容地址仍然是权威一致性判断。

### 3.3 路径级 reconcile

文件监听触发时不必重新扫描整个工作区：

1. 将变更路径解析为绝对路径；
2. `.gitignore`、`.oceignore` 或目录变化会升级为全量 reconcile；
3. 普通文件只处理受影响的路径及其已跟踪子路径；
4. 文件删除、被 ignore、超过大小、无法读取时标记删除；
5. 文件存在时重新读取、重新 hash、更新记录；
6. 对冲突的 explicit 内容继续保留 explicit 版本；
7. 将变更以一个 generation 原子写入状态。

### 3.4 explicit 未保存内容

`observe` 不直接发布到远端索引，而是：

- 校验路径未被 ignore；
- 校验内容是文本且不超过大小限制；
- 计算 path+content hash；
- 以 `source=explicit` 写入本地状态；
- generation 加一。

`remove` 同样只在本地状态中标记删除，直到下一次同步才发布。这样编辑器内容可以先进入检索视图，但不会被文件系统扫描立即覆盖。

### 3.5 生成同步计划

同步计划比较两个集合：

```text
current_names   = 当前 present 文件的 blob 集合
committed_names = 已提交到 checkpoint 的 blob 集合

added   = current_names - committed_names
deleted = committed_names - current_names
```

跳过的文件单独记录，不能悄悄当成已索引文件。

### 3.6 durable sync

OCE-client 的同步不是一次临时请求，而是可重试的本地操作：

1. 以当前本地清单重新生成计划；
2. 把旧的 pending/failed outbox 标记为 superseded；
3. 写入新的 sync outbox；
4. 对当前 Blob 分批执行 `find_missing`；
5. 将 unknown 或 nonindexed Blob 加入上传/索引集合；
6. 上传前重新读取文件并重新计算 hash；
7. hash 不一致时拒绝继续，避免把编辑过程中的旧内容写入新索引；
8. 按数量和字节数分批提交；
9. 校验服务端返回的 Blob 名称集合必须与本批完全一致；
10. 等待这些 Blob 进入 ready；
11. 创建或推进 checkpoint；
12. 如果 checkpoint 链已经在服务端丢失，则从当前全部 present Blob 重建；
13. 提交本地 `committed_blob_name`、checkpoint 和 synced generation；
14. 标记 outbox complete；
15. 任一步异常都保留 failed outbox 和错误信息。

在 Vulcan 原生实现中不需要上传文件到 HTTP 服务，但应保留这些语义：当前清单是权威来源、写入可重试、Hash 二次校验、checkpoint/generation 一致提交、失败可恢复。

### 3.7 watcher 与后台 indexer

OCE-client 的后台 indexer：

- 可选启动初次全量同步；
- 将文件监听事件合并成 generation；
- ignore 文件或未初始化状态触发全量恢复；
- 普通变化只积累路径集合；
- 同一时刻由一个后台线程消费一批全量/路径任务；
- 成功后更新 `synced_generation`；
- 失败后设置 `recovery_required`，下一次触发全量恢复；
- 状态对外表现为 `idle`、`indexing`、`ready` 或 `error`；
- 检索调用在超时前等待 ready，否则返回 indexing/error 状态。

### 3.8 当前客户端的一个限制

`WorkspaceContext.retrieve` 会校验 `workspace` 和 `working_set` 两种 scope，但当前实现对两者发送的 Blob 范围没有实际区别：有 checkpoint 时都使用 checkpoint，没有 checkpoint 时都发送当前 present Blob 名称。因此 Rust 实现不能把这个参数名直接当成已经完成的 working-set 语义，需要由 Vulcan 明确提供授权范围和变更范围。

## 4. 服务端索引写入逻辑

### 4.1 ingest 是轻量入口

`IndexingPipeline.ingest` 的目标是快速接收 Blob，不在上传入口完成完整切块和向量化：

1. 查询 Blob 是否已存在；
2. 判断二进制或服务端忽略路径；
3. 对二进制/忽略文件删除旧向量和旧 Blob 关联，然后保存为 ready；
4. 对已有 pending/ready Blob 只 touch，不重复索引；
5. 对新文本 Blob 只写 pending 元数据；
6. 将原文保存到 staging；
7. 发布 Blob created 事件；
8. 立即返回，chunk_count 为 0。

这一步不切块、不调用 Embedding，便于批量接收和异步处理。

### 4.2 embed_pending 三阶段

#### 第一阶段：从 staging 补切块

- 只处理 pending Blob；
- 如果 Blob 没有 Chunk，从 staging 读取原文；
- staging 缺失且文件非空时标记 error；
- 空文件直接 ready；
- 使用 Chunker 生成 Chunk；
- 保存 Chunk 并回填 Blob 的 Chunk 引用；
- 没有有效 Chunk 时直接 ready 并删除 staging。

#### 第二阶段：批量 Embedding 与向量写入

- 查找这些 Blob 的 pending Chunk；
- 按 `embed_batch_size` 分批；
- 对每个 Chunk 的 `embedding_text()` 生成向量；
- 严格校验向量数量必须等于 Chunk 数量；
- 将 Chunk ID、内容 hash、Blob 名称、原文和位置元数据一起 upsert 到向量索引；
- 成功后把 Chunk 标记为 embedded；
- 任一批失败时，根据 `mark_failures` 决定是否把相关 Blob 标记 error，并发布 failed 事件。

#### 第三阶段：ready 与路径索引

- pending Blob 标记 ready；
- 删除 staging；
- 发布 Blob ready 事件；
- 最后批量构建路径索引；
- 路径索引写入失败只记录日志，不阻塞主代码向量索引。

如果 Embedding 被关闭，OCE 只完成切块并标记 ready，不写向量，也跳过路径向量索引。这个模式可用于只验证切块或离线阶段。

### 4.3 删除逻辑

删除 Blob 时：

1. 删除元数据和关联 Chunk；
2. 删除该 Blob 的所有代码向量；
3. 删除该 Blob 的路径向量；
4. 路径索引删除失败不阻塞主删除流程。

Rust 实现应保持主索引删除和辅助路径索引删除的失败隔离，但必须留下可重试或可重建状态，不能形成永久孤儿索引。

### 4.4 后台 worker

后台 worker 的处理模型：

- 启动前恢复上次崩溃时仍处于 processing 的任务；
- 创建多个消费协程；
- 每个 Blob 使用独立 UoW/事务绑定的 repository；
- 从队列取 Blob；
- 执行 `embed_pending`；
- 成功后 ack；
- 失败后 fail 并增加 Blob retry_count；
- 未超过最大重试时保留 staging 并重新入队；
- 超过上限时标记 error 并清理 staging。

## 5. 代码切块逻辑

### 5.1 Chunker 路由

当前装配顺序是：

```text
LanguageChunkerRouter
├── CastChunker：支持语言优先走 AST 语义切块
├── MarkdownChunker：Markdown 专用逻辑
├── JspChunker：JSP 专用逻辑
├── VueChunker：Vue 专用逻辑
└── RecursiveChunker：统一 fallback
```

当前默认值：

- RecursiveChunker 目标 chunk 大小约 6000 字符，切分器 overlap 约 200；
- CastChunker 最大 AST chunk 约 1500 字符，默认 AST node overlap 为 0；
- FixedChunker 已废弃。

语言由文件扩展名映射到规范语言标识，再由 Router 选择专用实现。未识别语言或 AST 解析失败时使用递归字符切块。

### 5.2 cAST/AST 语义切块

cAST 切块不调用 LLM，主要过程是：

1. 根据语言获得 tree-sitter parser；
2. 解析源码为 AST；
3. 统计非空白字符数量，避免空格影响大小预算；
4. 如果整个 AST 未超过上限，作为一个窗口；
5. 否则递归把 AST 子节点分配到窗口；
6. React/JSX 组件有独立的顶层窗口处理；
7. 仅在兄弟节点合并后仍不超预算时合并相邻窗口；
8. 可选地从前后窗口加入有限 AST 节点重叠；
9. 把 AST window 转换为 ASTChunk 和 code window；
10. 将 AST 的 byte row/column 范围转换成 1-based 闭区间行号；
11. 把过小或被前一窗口完全覆盖的范围并入邻近范围；
12. 用原始源码行生成最终 Chunk 文本；
13. 超出最大字符预算的 span 被裁剪或不产出。

重要设计点：AST window 可以按列切开同一个构造，最终必须重新映射到行区间并合并，否则可能产生只有函数签名前缀或孤立闭括号的无效 Chunk。

AST Chunk 的 metadata 还可以携带 filepath、祖先节点、签名等结构信息，但最终向量文本仍应保持“路径前缀 + 可定位的原始代码”。

### 5.3 RecursiveChunker fallback

RecursiveChunker 用语言感知的 RecursiveCharacterTextSplitter 产生候选起点，但不直接使用 splitter 返回的文本作为最终内容：

1. 对空内容或不含字母/数字的内容返回空列表；
2. 使用语言专用分隔符，顺序通常是段落、行、空格、字符；
3. 要求 splitter 提供 `start_index`；
4. 将字符 offset 映射到 1-based 行号；
5. 把所有起点转换成连续、不重叠的行区间；
6. 裁掉尾部空行；
7. 从原始 `lines` 中重新切出文本；
8. 对单行超长内容执行硬上限处理；
9. 只保留含有效字母或数字的 Chunk。

虽然 splitter 配置了 overlap，但带行号的最终 Chunk 会主动去掉相邻区间的重复行。重复文本会占用两个检索槽位并导致同一源码被重复送入上下文。

### 5.4 Chunk 身份和上下文文本

切块必须同时保存：

- 精确源代码文本；
- 相对路径；
- 1-based 行号；
- Chunk 类型；
- 内容 hash；
- Blob occurrence ID。

Embedding 文本使用：

```text
File: <path>

<verbatim source chunk>
```

结果渲染也必须从同一份 verbatim 文本和起始行号生成，不能重新从检索结果拼接或推断行号。

## 6. Embedding 与向量处理

### 6.1 文档向量

当前 OCE 的 Embedding 客户端是 OpenAI-compatible API，目标实现应改为调用 VulcanCode 注册的统一 HostAI Embedding 服务，但保留以下处理语义：

1. 输入是带路径前缀的 Chunk embedding 文本；
2. 单条输入超过模型字符上限时拆成多个片段；
3. 每个片段的边界优先落在换行或空格，避免任意截断；
4. 片段之间保留有限 overlap；
5. 多条文本按最大条数和总字符数分批；
6. 多个批次可以受并发上限约束并行请求；
7. 对每条文本的多个片段向量按片段字符数加权平均；
8. 平均后的向量执行 L2 normalization；
9. 检查返回数量和向量维度；
10. 记录 Embedding usage。

当前默认参数来自 OCE 配置：

- 维度约 1024；
- 单批最多约 32 条文本；
- 单批总字符预算约 32000；
- 单条模型输入上限约 8000 字符；
- 长输入 overlap 约 400 字符；
- 最大并发约 4。

这些是当前实现默认值，不应在 Rust 代码中硬编码；VulcanCode 宿主应通过统一 HostAI 注册提供模型、维度、批量、并发和 token 使用信息。

### 6.2 查询向量

查询向量和文档向量使用同一 Embedding 服务。若配置了 query instruction，先将 instruction 添加到查询文本前，再走与文档相同的分段、批量和归一化流程。

模型、维度和 query instruction 变化都会影响旧索引的可用性。目标存储必须保存至少以下索引身份信息：

- embedding provider/model；
- dimensions；
- distance metric；
- query/document preprocessing 版本；
- chunker 版本；
- index generation。

### 6.3 Embedding 错误边界

以下情况必须显式失败，不能静默写入部分结果：

- 返回向量数量与输入数量不一致；
- 任意向量维度不匹配；
- 一条长输入的分段向量数量与分段数量不一致；
- 模型或凭据不可用；
- 索引模型身份与现有向量不兼容。

## 7. 当前向量和辅助索引结构

当前 OCE 使用 Milvus 作为基础设施，但目标 Rust 实现不应绑定 Milvus。当前行为可抽象为三类索引：

### 7.1 内容 dense vector 索引

每个向量项包含：

- `chunk_id`；
- `content_hash`；
- `blob_name`；
- 原始 Chunk 内容；
- dense vector；
- `path`、`start_line`、`end_line` 等 metadata。
- 行内切分时的 `start_char_offset` metadata；普通 Chunk 为零。

当前 Milvus 实现使用 HNSW 和 COSINE，查询时：

- 传入 query vector；
- 可按 `blob_name` 做范围过滤；
- 返回 top-k；
- 应用 vector threshold；
- 转成统一 `SearchHit`。

### 7.2 路径语义索引

路径索引是独立通道，不直接把路径当作普通代码 Chunk。每个路径文档由以下内容组成：

- 规范化完整路径；
- 文件名；
- 文件名 stem；
- 目录、文件名、下划线、连字符和 camelCase 分解后的 token；
- 扩展名对应的通用语义，例如 Rust source、JSON configuration、Markdown documentation。

路径文档只使用路径自身信息，不注入某个仓库或技术栈的先验知识。

路径索引项包含：

- `path_id`；
- `blob_name`；
- `path`；
- `path_document`；
- `path_vector`。

路径向量和内容向量使用同一个 Embedding 能力，但属于独立索引。路径索引只在 ready Blob 完成后写入；写入失败不影响内容主索引。

### 7.3 exact symbol 索引

精确索引不是向量索引，主要保存 SQL/结构化记录：

- identifier；
- endpoint/definition kind；
- Blob；
- Chunk；
- 行号。

它解决的是“查询中明确出现了某个函数、类型或模块名”这一类问题，避免让向量模型承担精确字符串匹配。

### 7.4 VLDB 目标映射

在 Vulcan 中，以上能力应映射为 VLDB 的上层能力契约：

```text
OCE 代码语义层
├── content vector namespace
├── path vector namespace
├── exact symbol metadata/index
└── code chunk metadata

VulcanCode HostAI
└── 向量生成

VulcanCode VLDB 数据面
├── 向量 upsert/delete/search/filter
├── 代码索引持久化
└── generation / snapshot / 事务能力
```

VMM 记忆索引和 OCE 代码索引可以共用宿主向量服务与底层存储引擎，但必须使用独立注册、向量空间、namespace、权限、schema、generation 和生命周期。

## 8. 查询意图和检索策略

### 8.1 查询输入

领域检索入口接收：

- 自然语言查询；
- 允许检索的 Blob 集合，或等价的项目/工作集范围；
- 可选检索审计容器。

范围语义：

- `None` 表示不做索引过滤，但上层服务默认不允许无边界检索；
- 空集合表示没有可搜索内容，直接返回空列表；
- 非空集合必须在每一路召回中生效。

### 8.2 启发式意图分类

当前分类优先级：

1. 存在符号锚点：
   - 带调用方向动词 -> `CALL_CHAIN`；
   - 带引用/使用动词 -> `REFERENCE`；
   - 其他 -> `SYMBOL`。
2. 不存在符号锚点：
   - 文件名 token 或路径词且不是功能实现语义 -> `PATH`；
   - 架构/机制/流程词 -> `OVERVIEW`；
   - 其他 -> `FEATURE`。

符号锚点包括：

- 反引号标识符；
- snake_case 标识符；
- `module::function` 形式的限定标识符。

提取 exact identifiers 时支持：

- 反引号内容；
- snake_case；
- Rust `::` 路径；
- PascalCase 类型名。

反引号中的符号名不会参与调用动词判断，避免 `invoke_handler` 中的 `invoke` 被误判成调用链查询。

当前存在 `COMPOUND` 策略定义，但启发式分类主函数主要返回 SYMBOL、CALL_CHAIN、REFERENCE、PATH、FEATURE 和 OVERVIEW；COMPOUND 更像是供 LLM 意图分类或未来扩展使用的策略槽位。

### 8.3 策略表

| 意图 | 路径索引 | 查询改写 | LLM 重排 | 主要目标 |
| --- | ---: | ---: | ---: | --- |
| `SYMBOL` | 否 | 是 | 是 | 找符号定义，定义位置优先 |
| `CALL_CHAIN` | 否 | 否 | 是 | 保留方向和边界，给重排器判断关系 |
| `REFERENCE` | 否 | 是 | 否 | 找符号使用/引用位置 |
| `PATH` | 是 | 是 | 是 | 找哪个文件，再找文件内相关片段 |
| `FEATURE` | 否 | 是 | 是 | 根据功能描述找到实现 |
| `OVERVIEW` | 否 | 否 | 是 | 找架构、机制和流程说明 |
| `COMPOUND` | 否 | 是 | 是 | 处理多条件/多 facet 查询 |

注意：当前主 `RetrievalPipeline` 实际消费的策略字段主要是 `enable_path_index`、
`enable_query_rewrite` 和 `enable_llm_rerank`。`boost_definitions`、`boost_docs`、
`enable_multi_hop` 和 `enable_reference_graph` 虽然已经出现在策略数据结构中，但在当前
主检索链中没有发现对应的执行逻辑；它们不能被当作已经完成的图谱、多跳或引用图能力。

### 8.4 LLM 查询改写

查询改写是可选的，不是检索的必需步骤：

1. 将原始查询发送给共享 LLM；
2. 解析行分隔的多个候选改写；
3. 清理编号、Markdown 符号和空白；
4. 丢弃过短或无效候选；
5. 限制改写数量；
6. 确保原查询仍然在查询列表中；
7. 失败时只使用原查询。

当前配置默认关闭普通查询改写，主要用于跨语言文件名和特殊路径场景。Rust 目标应复用 VulcanCode 注册的统一 HostAI LLM 服务，但默认不能让每次检索无条件产生额外模型调用。

### 8.5 启发式查询规划

查询规划不一定调用 LLM：

1. 压缩连续空白；
2. 按句号、分号、换行、列表边界等拆出 facet；
3. 删除列表符号；
4. 丢弃短于最小字符数的 facet；
5. 去重；
6. 如果没有至少两个有效 facet，则只保留规范化原查询；
7. 否则返回“原查询 + 最多若干 facet”。

当前默认总查询数上限约为 4，facet 最小长度约为 8 字符。

## 9. 完整检索管线

### 9.1 主入口顺序

`RetrievalPipeline.search` 的主顺序是：

1. 空 scope 直接返回空结果；
2. 可选 LLM 意图分类；
3. 根据策略判断是否进入路径增强分支；
4. 必要时查询改写；
5. 对每个查询执行 query planner；
6. 对每个 planned query 并发执行 dense 召回；
7. 使用 exact symbol store 进行精确召回；
8. 多路结果进行融合和去重；
9. 执行基础 reranker；
10. 执行源码优先的路径因子；
11. 必要时执行 LLM 语义重排；
12. 对符号定义查询提升 endpoint；
13. 应用置信度下限；
14. 执行覆盖度和字符预算选择；
15. 返回最终命中。

`rerank` 和 `select` 是两个不同职责：

- `rerank` 解决“单个候选是否更相关”；
- `select` 解决“最终上下文是否覆盖足够多文件且不重复”。

### 9.2 Dense 召回

对每个 planned query：

1. 使用统一 Embedding 服务生成 query vector；
2. 单查询使用较大的 default top-k；
3. 多 facet 查询降低每个子查询的 top-k；
4. 将允许的 Blob 名称作为索引过滤条件；
5. 执行向量相似度搜索；
6. 丢弃低于 vector threshold 的命中；
7. 转成统一 `SearchHit`。

如果多个查询并行，后续不能简单拼接结果，需要使用 RRF 融合。

### 9.3 Exact 召回

Exact 召回只有在以下条件同时满足时才执行：

- exact store 已配置；
- scope 是明确且有限的；
- scope Blob 数不超过上限，当前约 2000；
- 查询中确实提取出代码标识符；
- top-k 大于零。

当前 exact 查询带约 2 秒超时；超时直接返回空 exact 结果，保留 dense 结果，不阻塞主检索。

活动的 `SymbolSearchStore` 从 `symbol_occurrences` 查询标识符，并按 kind 分配分数；相同 content hash 去重，endpoint 优先于 definition。

### 9.4 多查询 RRF 融合

多路结果使用加权 Reciprocal Rank Fusion：

```text
score(hit) = Σ weight(list) / (rrf_k + rank)
```

当前规则：

- 原查询权重为 1.0；
- facet 查询使用较低权重，默认约 0.75；
- `rrf_k` 默认约 60；
- 以 `(blob_name, path, start_line, end_line, start_char_offset, content_hash/content)` 识别同一个 source occurrence；
- 首次出现顺序用于分数相同时稳定排序；
- 分数按理论最大分归一化；
- 融合后截断到默认候选窗口。

### 9.5 exact 与 dense 合并

普通查询：

- exact 和 semantic 命中按 source occurrence 去重；
- 同一命中保留较高分；
- 统一按分数降序。

CALL_CHAIN 查询：

- semantic 命中作为主要候选；
- exact-only 命中保留有限席位，默认不超过候选窗口约三分之一；
- exact-only 分数限制在 semantic anchor 分数以内；
- 这样既保留调用关系语义，又不会让精确字符串命中完全挤掉上下文。

### 9.6 路径增强检索

路径索引解决“哪个文件”，内容索引解决“文件里的哪一段”。两者不能按文件简单去重，否则正确的函数 Chunk 会被该文件的第一个 import/use Chunk 替换。

路径增强流程：

1. 可选生成原查询和改写查询变体；
2. 每个变体生成 query vector；
3. 查询路径索引，通常每个变体取约 20 个文件；
4. 每个 Blob 只保留最高路径分；
5. 对所有查询变体继续做内容 dense 召回和 query planner；
6. 内容结果先 RRF 融合；
7. 对已经有内容命中的 Chunk 加上路径分数 boost；
8. 只有内容完全没有覆盖的路径文件才回填该文件第一个 Chunk；
9. 以 Chunk 粒度排序，而不是按文件覆盖；
10. 路径类查询使用文档中立的 source priority factor；
11. 执行 rerank、置信度过滤和 coverage select。

路径 boost 的核心原则：路径命中增强同文件内容命中，但不替换内容命中。

### 9.7 基础 rerank

基础 reranker 是一个可注入的协议。默认可使用 Noop，使管线退化为向量/精确召回排序；也可以使用 API rerank 模型。

### 9.8 LLM 语义重排

LLM 重排候选必须携带：

- path；
- 原始 score；
- content；
- start_line；
- end_line。

只给路径会让重排退化为文件名匹配，无法判断函数定义、调用链和具体代码语义。

当前处理：

1. 候选最多约 50 条；
2. 每个候选送入有限字符的代码片段；
3. 要求 LLM 返回候选下标；
4. 容忍编号、列表符号等格式变体；
5. 去重并限制输出 top-k；
6. 返回数量不足时按原顺序补齐；
7. 响应无有效下标或请求失败时回退原始顺序。

LLM 重排应使用 VulcanCode 注册的统一 HostAI Rerank 服务，并且默认受策略、预算和缓存控制；它不是每个查询必须发生的步骤。

### 9.9 源码优先因子

普通查询会对路径应用乘性优先因子：

- `LICENSE`、`NOTICE`、`COPYING` 等法律文件约 0.1；
- 主 `README` 不降权；
- 多语言 README 约 0.2；
- `docs/` 和 `.md/.rst/.txt` 约 0.5；
- 测试文件约 0.6；
- `index.*`、`types.ts` 等入口/类型文件约 0.85；
- 普通源码为 1.0。

有效排序分数：

```text
effective_score = raw_score × priority_factor(path)
```

路径查询不使用源码优先降权，因为用户此时就是在找文档、配置或测试文件。

### 9.10 endpoint 提升

对于明确询问“函数在哪里定义/实现位置”的 SYMBOL 查询：

1. 提取查询中的代码标识符；
2. 识别 Tauri/PyTauri command 或 Web 路由 endpoint 定义形态；
3. 如果命中内容包含相应 endpoint 定义，则把它稳定提升到前面。

这是一个结果后处理规则，不是图谱推理。

### 9.11 置信度过滤

对每个命中计算有效分数：

```text
effective_score = score × priority_factor(path)
```

低于 `confidence_floor` 的命中被剔除。默认配置允许关闭门槛，但目标实现应保留显式门槛能力，避免把纯弱匹配送入模型上下文。

### 9.12 覆盖度选择

`CoverageSelector` 不是普通 top-k：

1. 第一轮优先选择不同文件的最高命中；
2. 第二轮才允许继续选择同一文件；
3. 同一路径达到 `max_per_path` 后停止；
4. 相同 source occurrence 去重；
5. 同一路径的行区间重叠比例达到约 0.6 时跳过；
6. 总字符数是硬预算，放不下的大 Chunk 被跳过并继续尝试小 Chunk；
7. `top_k` 是软上限，最终数量可能更少。

默认配置约束：

- 单文件最多 2 个 Chunk；
- 总上下文字符预算约 32000；
- 行区间重叠阈值约 0.6；
- 最终选择约 10 条。

## 10 结果输出逻辑

OCE 当前最终结果包含两个层次：

1. 内部 `SearchHit`：保留 Blob、路径、内容、分数、content hash 和行号；
2. 面向代理的格式化文本：每个 Chunk 独立渲染。

格式：

```text
Path: relative/path/to/file.rs
Lines: 20-45
    20	<source line>
    21	<source line>
```

要求：

- 保持最终命中分数顺序；
- 不合并同文件的多个 Chunk；
- 行号从 `start_line` 递增；
- 没有命中时返回固定空结果头；
- 原生 Rust 层应优先保留结构化命中，格式化只作为最后的渲染步骤。

## 11. 与“工具调用”对应的逻辑操作

以下是从 OCE API/client 提取出的业务操作语义，不是 MCP 设计。Rust 目标中可以由 Vulcan 注册一个或少量上层工具来调用这些操作。

### 11.1 主要 AI 检索操作

| 操作语义 | 输入 | 处理 | 输出 |
| --- | --- | --- | --- |
| 代码语义检索 | 查询 + 项目/工作集范围 | 解析范围，确保索引可用，执行完整 RetrievalPipeline | 结构化命中和带行号上下文 |

这是唯一应该成为普通 AI 默认入口的代码语义操作。它不应让模型直接选择 dense、exact、path、rerank 等内部步骤。

### 11.2 同步与索引控制操作

| 当前操作语义 | 作用 |
| --- | --- |
| `find_missing` | 给定 Blob 名称，区分服务端 unknown 和 nonindexed |
| `batch_upload` / `ingest_blobs` | 接收 path+content，计算内容地址，写入 pending/staging，按模式同步或排队嵌入 |
| `blob_status` | 查询 Blob 是否存在、是否 ready，以及 checkpoint 是否存在 |
| `checkpoint_blobs` | 创建或推进工作集 Chain，应用 added/deleted 集合 |
| `sync` | 全量 reconcile 后生成同步计划并提交 |
| `sync_paths` | 只处理监听到的变更路径 |
| `status` | 只读取本地索引状态，不触发远端检索 |
| `observe` | 将未保存编辑器内容放入 explicit overlay |
| `remove` | 将工作区相对路径标记为删除 |
| `watch` | 监听变更并触发增量同步 |

在 Vulcan 中，`sync`、`sync_paths`、`observe`、`remove` 和 `watch` 更适合作为宿主内部生命周期操作，而不是模型可见工具。

### 11.3 管理操作

凭据 reload、队列状态、队列 reset、滞留任务 requeue、GC、调用指标和资源指标属于运维/内部操作，不应进入 AI 默认工具目录：

- 它们不解决代码检索问题；
- 它们会增加工具选择噪声；
- 它们必须受宿主权限和生命周期控制。

## 12. Rust 原生适配建议

### 12.1 OCE 库应该保留的内容

- Blob/Chunk/Hit 等领域类型；
- 文件内容地址化算法；
- cAST 与递归切块算法；
- Chunk 行号和原文一致性规则；
- path document 构建；
- query intent 分类；
- query planner；
- dense/exact/path 召回编排；
- RRF 和 exact 合并；
- source priority、endpoint promotion、confidence floor；
- LLM rerank 的候选结构和失败回退；
- coverage selector；
- 带行号结果渲染。

### 12.2 不应放入 OCE 库的内容

- MCP server 和 stdio；
- FastAPI/HTTP/ACE 路由；
- Python CLI；
- Milvus/LanceDB/VLDB 具体 API；
- 文件监听器实现；
- Vulcan 项目权限和会话路由；
- 模型凭据管理；
- Redis 或其他具体队列；
- 记忆数据模型。

### 12.3 推荐的抽象端口

当前 Rust 实现已经收敛为三个领域端口：

- `VceAiPort`：统一承载 LLM query rewrite、文档/查询 Embedding 和 Rerank；
- `VceSourcePort`：按精确 Blob 预期提供有界不可变源码快照；
- `VceIndexPort`：承载 staging、CAS checkpoint 发布、dense/path/exact 查询及管理数据。

VCE 负责端口调用、暂存事务协议、结果校验、索引计划和检索结果；上级负责真实持久化、任务队列、权限、取消、deadline 和进程故障恢复。

### 12.4 HostAI/VLDB 的复用方式

准确的复用边界是：

```text
VCE 代码逻辑
  -> VulcanCode VCE 独立 AI 注册 -> 统一 HostAI 服务
  -> VulcanCode VceSourcePort -> 当前授权工作区
  -> VulcanCode VceIndexPort -> VLDB/SQLite 代码专属 namespace
```

不能把代码向量直接当成 VMM 记忆记录，也不能让 VCE 自己再配置一套 LLM/Embedding 凭据。Milvus 不应进入 Rust 目标依赖；VLDB 内部已经包含 LanceDB，VCE 只依赖 `VceIndexPort` 能力契约。

### 12.5 工具注册方式

Vulcan 工具层应只负责：

- 注册一个统一的代码语义检索入口；
- 注入当前项目和授权范围；
- 在检索前触发宿主侧 reconcile/ready 检查；
- 调用 OCE Rust 库；
- 将结构化命中投影成 AI 可读上下文。

模型不应看到 OCE 的 dense、path、exact、fusion、rerank 等内部步骤，更不应同时看到一组功能重叠的 CBM 工具。

当前 VCE 已在 `src/tooling.rs` 预置这条上层映射：仅将 `codebase_retrieval` 作为 AI 可见入口，
其余同步、索引和工作区维护操作保留为 Host-only 定义。字段、Schema 和 VulcanCode 适配顺序见
[`docs/vce-tool-surface.md`](vce-tool-surface.md)；该映射不承担实际执行、Provider 或存储所有权。

## 13. 失败和一致性要求

必须保留的显式语义：

| 场景 | 当前 OCE 处理 | Rust 目标要求 |
| --- | --- | --- |
| 空检索范围 | 返回空结果 | 不允许降级全库 |
| 缺少 checkpoint 和 added | 拒绝请求 | 要求上层提供授权范围 |
| checkpoint 格式非法 | 参数错误 | 显式错误 |
| checkpoint 链不存在 | reset/404 语义 | 触发范围重建，不静默缩小 |
| exact 超时 | 放弃 exact，保留 dense | fail-open |
| LLM 改写失败 | 使用原查询 | 不阻塞检索 |
| LLM 重排失败 | 保留原顺序 | 不阻塞检索 |
| path index 失败 | 使用内容检索 | 不阻塞主索引 |
| staging 缺失且 Blob 非空 | Blob error | 保留错误和重试状态 |
| Embedding 数量/维度错误 | 写入失败 | 禁止部分成功 |
| 文件在同步中变化 | hash 二次校验失败 | 禁止提交旧内容 |
| worker 进程崩溃 | 恢复 processing 任务 | 可重试、可幂等 |

## 14. 不应盲目复制的当前实现限制

### 14.1 客户端和服务端过滤规则重复

OCE-client 和 OCE 服务端各有文件过滤规则。Rust 目标需要建立单一准入规则，或者明确“宿主准入”和“索引准入”的关系，否则会出现：

- 本地认为已同步，服务端拒绝；
- 本地认为未索引，服务端已有；
- path index 和 content index 接受范围不同。

### 14.2 当前 API 是内容上传模型

OCE-client 通过 path+content 上传 Blob。Vulcan 原生集成不需要复制这个 HTTP 上传边界，应直接把宿主已经拥有的文件快照和 VLDB 写入操作连接起来。

### 14.3 当前 `working_set` 尚未形成独立语义

不能因为 API 或 CLI 有 `working_set` 参数，就推断已经实现了不同的检索集合。目标实现应从 Vulcan 当前项目/会话授权模型重新定义范围。

### 14.4 当前 Milvus 是基础设施适配器

OCE 领域层依赖的是 `SearchStore`、`VectorIndex`、`PathSearchStore` 和 exact store 协议，Milvus 位于 infrastructure 层。Rust 迁移应保留协议职责，不复制 Milvus collection、HNSW 或 Python 客户端调用方式。

### 14.5 cAST 不等于简单 tree-sitter 扫描

cAST 的价值在于 AST 节点窗口、非空白字符预算、兄弟节点合并、边界映射、过小窗口吸收和原文行号保持。只引入 tree-sitter parser 而没有这些窗口逻辑，不能认为已经实现 OCE 的切块能力。

### 14.6 LLM 步骤不是默认必需

OCE 支持意图分类、查询改写和 LLM 重排，但这些属于可选增强。目标应优先保证：

```text
Embedding + dense/exact/path + fusion + coverage
```

然后再按宿主模型服务的成本和实际收益启用 LLM 阶段，避免把检索变成多次模型调用链。

## 15. 实现验收清单

### 工作区和索引

- [x] VCE 可复用 mtime/size/blob；metadata-only 观察只为缺失 Blob 读取正文，并按 Blob 数和总字节双重分批；
- [x] `RefreshMode::Paths` 只协调明确变化与删除路径；
- [ ] ignore 文件和目录变化由 VulcanCode watcher 识别后触发 Full；
- [x] 未保存 overlay 不会被磁盘扫描覆盖；
- [ ] 二进制和非法 UTF-8 的准入判定由宿主文件读取层完成；VCE 已处理删除、skip 与超限文本；
- [x] 同一 path+content 不会重复生成新索引记录；
- [x] generation、checkpoint、统计和最近操作可以通过版本化状态恢复。

### 切块和向量

- [x] Chunk 文本与 start/end line 由切块器及索引前校验共同保证；
- [x] 语义分析器失败时自动递归切块，并保留失败原因；
- [ ] 具体语言 cAST 实现仍需完成 AST window 小窗口吸收；VCE 已提供严格注册与回退入口；
- [x] 长 embedding 输入按边界分段、加权池化并归一化；
- [x] 文档向量和查询向量使用一致的模型身份与预处理；
- [x] operation、分段覆盖、维度和批次响应错误会显式失败；
- [x] index binding 强制内容/路径向量与 VMM 记忆向量保持 namespace 隔离。

### 检索质量

- [x] 功能描述进入 semantic dense 召回；
- [x] 明确函数/类型名生成 exact symbol 查询；
- [x] 文件名/配置文件查询启用 path index；
- [x] 多 facet 查询使用归一化加权 RRF，而不是简单拼接；
- [x] CALL_CHAIN 合并为 exact 保留语义上下文；
- [x] 路径命中增强同文件 Chunk，并只在缺失时追加首 Chunk 回填；
- [x] endpoint 定义在符号定义查询中稳定提升；
- [x] 最终上下文跨文件覆盖且按出现位置去重；
- [x] 总字符预算是硬限制。

### 工具和边界

- [x] VCE 静态工具目录只允许统一 `codebase_retrieval` 进入 AI 可见面；
- [x] VCE Rust 库不依赖 MCP、HTTP、Milvus 或 LanceDB 具体 API；
- [x] CodeKit RG 保持精确文本/正则定位，VCE exact symbol 属于语义索引内部辅助通道；
- [x] VMM-Core 与 VCE 只共享宿主模型/向量注册契约和底层服务，不共享注册实例，也不承载代码检索业务逻辑；
- [x] CBM 不进入 VCE 依赖、降级路径或默认代码检索工具面；
- [ ] VulcanCode 仍需把单一工具定义接入真实原生工具完整暴露链。

## 16. 源码和说明索引

### OCE 说明

- `D:\openvulcan\third_git\oce\README.zh-CN.md:L24-L50`：产品定位、dense/exact/path、cAST、LLM 重排和覆盖度选择；
- `D:\openvulcan\third_git\oce\README.zh-CN.md:L74-L164`：个人/服务模式和当前依赖；
- `D:\openvulcan\third_git\oce\README.zh-CN.md:L240-L278`：数据面操作语义；
- `D:\openvulcan\third_git\oce\README.zh-CN.md:L280-L380`：分层架构和完整检索管线；
- `D:\openvulcan\third_git\oce\AGENTS.md:L7-L19`：领域边界、检索主链路和协议方向。

### OCE 领域与应用逻辑

- `D:\openvulcan\third_git\oce\src\oce\domain\blob\blob.py:L26-L108`：Blob 状态和生命周期；
- `D:\openvulcan\third_git\oce\src\oce\domain\chunk\types.py:L13-L87`：Chunk、ChunkRef、LocatedChunk 和 embedding 文本；
- `D:\openvulcan\third_git\oce\src\oce\domain\chain\chain.py:L21-L123`：Chain/checkpoint 工作集；
- `D:\openvulcan\third_git\oce\src\oce\domain\services\indexing.py:L36-L284`：ingest、异步切块/嵌入、ready 和路径索引；
- `D:\openvulcan\third_git\oce\src\oce\domain\services\retrieval.py:L97-L679`：完整召回、融合、重排、路径增强和选择；
- `D:\openvulcan\third_git\oce\src\oce\domain\services\query_classifier.py:L13-L251`：意图分类和 identifier 提取；
- `D:\openvulcan\third_git\oce\src\oce\domain\services\retrieval_strategy.py:L13-L109`：意图到检索策略；
- `D:\openvulcan\third_git\oce\src\oce\domain\services\selector\coverage_selector.py:L10-L88`：覆盖度、重叠和字符预算选择；
- `D:\openvulcan\third_git\oce\src\oce\domain\services\path_document_builder.py:L1-L112`：路径语义文档和路径准入。

### OCE 基础设施逻辑

- `D:\openvulcan\third_git\oce\src\oce\infrastructure\astchunk\cast_chunker.py:L41-L235`：cAST 到带行号 Chunk 的转换；
- `D:\openvulcan\third_git\oce\src\oce\infrastructure\astchunk\astchunk_builder.py:L146-L705`：AST window 分配、合并、重叠和转换；
- `D:\openvulcan\third_git\oce\src\oce\domain\chunk\recursive_chunker.py:L28-L224`：递归 fallback、offset 到行号映射和非重叠输出；
- `D:\openvulcan\third_git\oce\src\oce\infrastructure\embed\openai_embedder.py:L16-L217`：分段、批处理、并发、池化和维度校验；
- `D:\openvulcan\third_git\oce\src\oce\infrastructure\milvus3\client.py:L33-L264`：当前 dense vector 的 collection、upsert、search 和 delete；
- `D:\openvulcan\third_git\oce\src\oce\infrastructure\milvus3\path_index.py:L20-L225`：当前 path vector 的写入和检索；
- `D:\openvulcan\third_git\oce\src\oce\infrastructure\persistence\models.py:L89-L239`：Blob、staging、Chunk、关联、Chain 和 symbol 元数据；
- `D:\openvulcan\third_git\oce\src\oce\infrastructure\persistence\symbol_extractor.py:L19-L137`：symbol 定义提取和查询标识符提取；
- `D:\openvulcan\third_git\oce\src\oce\infrastructure\persistence\symbol_search_store.py:L20-L146`：exact symbol 查询和 kind 排序。

### OCE-client 同步逻辑

- `D:\openvulcan\third_git\oce-client\README.md:L1-L119`：本地清单、同步、工作区和生命周期说明；
- `D:\openvulcan\third_git\oce-client\src\oce_client\context.py:L35-L478`：快照、reconcile、同步计划、outbox 和 retrieve；
- `D:\openvulcan\third_git\oce-client\src\oce_client\filesystem.py:L14-L62`：文件扫描和准入；
- `D:\openvulcan\third_git\oce-client\src\oce_client\ignore.py:L48-L84`：分层 ignore；
- `D:\openvulcan\third_git\oce-client\src\oce_client\state.py:L14-L288`：本地 SQLite 状态、generation、checkpoint 和 outbox；
- `D:\openvulcan\third_git\oce-client\src\oce_client\indexer.py:L13-L265`：watcher 事件合并、后台同步和 ready/error 状态；
- `D:\openvulcan\third_git\oce-client\src\oce_client\models.py:L7-L102`：FileRecord、BlobDelta、UploadPlan、SyncResult 和 WorkspaceSnapshot。

## 17. 当前提取范围之外

以下内容明确不作为本底稿的目标实现：

- `oce-client-mcp` 的 stdio server；
- MCP tool schema、MCP host 生命周期和传输协议；
- OCE 的 FastAPI/ACE HTTP 路由；
- OCE CLI 的命令行解析；
- PostgreSQL、Redis、Milvus 的部署和运维；
- OCE admin 凭据管理面；
- CBM 的代码图谱逻辑；
- VMM 记忆数据模型。

它们可以作为未来适配层参考，但不应进入 `vulcan-context-engine` 的核心代码语义库。
