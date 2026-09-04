# Vulcan Context Engine 原生完整对接执行方案

> 状态：已完成
>
> 制定日期：2026-09-04
>
> 目标仓库：`D:\openvulcan\vulcan-code`
>
> 实施对象：`crates/vulcan-context-engine` 与 VulcanCode 宿主、存储、工具、项目生命周期及 WebUI 管理面的完整原生对接
>
> 完成门槛：全部阶段验收通过，并完成连续五轮无新增问题的全范围代码审核

## 1. 计划目的

本计划用于把已经迁入 VulcanCode 工作区、但尚无运行时消费者的 `vulcan-context-engine`（以下简称 VCE）正式接入 VulcanCode 主链，使其成为代码语义检索的原生实现，并最终退出当前原生 CBM 主链。

本次对接不是建立 VCE MCP Server，也不是逐行翻译 OCE。VCE 已经以 MIT 许可独立实现了 OCE 服务端与 `oce-client` 中可复用的领域处理思路；本计划负责把这些领域能力连接到 VulcanCode 已有的统一 AI、VLDB、文件监听、项目管理、工具注册、服务 API 和 WebUI 基础设施。

最终必须同时满足以下目标：

1. VulcanCode 通过同一套 `HostAiPort` 注册规则向 VCE、Native VMM 和知识库提供 LLM、Embedding、Rerank 能力。
2. VCE、VMM、知识库只共享宿主能力规则和底层服务实例，不共享业务数据、向量空间、SQLite Schema、LanceDB 表或生命周期状态。
3. VCE 只负责代码语义逻辑；AI、文件快照、SQLite、LanceDB、文件监听、工具注册、页面管理均由 VulcanCode 上层持有并注入。
4. 项目首次启用时执行全量构建，后续跟随现有文件监听事件进行有界、合并、可恢复的路径级刷新。
5. AI 侧只新增一个语义工具 `codebase_retrieval`；同步、状态、观察、移除、维护等入口只供宿主和管理页调用。
6. CBM 不作为 VCE 的依赖、降级路径或兼容层；完成验收后移除 CBM 原生主链和重复工具面，同时保留用户已有 CBM 数据，不在本任务中执行不可恢复删除。
7. 提供完整的项目设置、状态、统计、文件与 Blob 查询、作业历史、完整性检查、维护计划和重建入口。
8. 完成重启恢复、项目切换、配置变更、向量模型变化、文件风暴、失败重试、取消、关闭和回滚验证。

## 2. 当前源码事实基线

### 2.1 已完成并可直接复用

- `crates/vulcan-context-engine` 已加入根工作区成员和 workspace dependency。
- VCE 来源为独立仓库提交 `654951cadc59f6902b4d3ac37a432de90e4a905f`，迁入后以当前工作区版本为唯一维护来源，不保留运行时跨仓依赖。
- VCE 已提供唯一领域入口 `VceEngine`，具备刷新、检索、覆盖层、状态快照、管理查询和版本化恢复能力。
- VCE 已提供三个宿主端口：`VceAiPort`、`VceIndexPort`、`VceSourcePort`。
- VCE 已实现确定性 Blob/Chunk 身份、结构分析扩展、递归回退、分批向量化、暂存、CAS 检查点发布、多通道召回、RRF、可选 Rerank、路径增强和覆盖选择。
- VCE 已提供 `VceToolCatalog` 静态映射，并明确只有 `codebase_retrieval` 建议对 AI 可见。
- `crates/vulcan-ai-contract` 已冻结统一 `HostAiPort` 契约。
- `crates/vulcan-service-kernel/src/internal_ai.rs` 已提供基于 `ConfiguredInferenceService` 的进程内宿主实现。
- 知识库与 Native VMM 已迁移到统一 `HostAiPort`，可作为 VCE 的直接宿主接入参考。
- `crates/vulcan-workspace-store` 已具备 VLDB Controller、SQLite、LanceDB、generation 和向量检索的成熟用法；知识库的 Jieba FTS5 只作为安全查询参考，不直接进入 VCE 检索主链。
- `crates/vulcan-file-watcher` 已提供 `LiveFileWatcher`、订阅者、路径注册和节流接收器。
- `vulcan-agent-runtime` 已具备 Native Tool Catalog、工具暴露过滤、执行上下文 handler 注入和结果投影链。
- 当前 VCE 全 feature 测试已通过；仓外实现阶段的五轮连续审核记录已存在。

### 2.2 尚未完成，属于本计划范围

- `vulcan-service-kernel` 尚未依赖并实例化 VCE。
- 尚无 `VceAiPort` 的 VulcanCode 具体适配器。
- 尚无代码语义专属 `VceIndexPort`、SQLite Schema、精确符号索引和 LanceDB 表。
- 尚无基于项目根目录和不可变源码快照的 `VceSourcePort`。
- 尚未订阅 VulcanCode 文件监听事件，也没有项目级串行刷新任务。
- 尚未将 `codebase_retrieval` 注册到 Native Tool Catalog。
- 尚未把当前会话的权威项目身份注入 VCE 工具执行链。
- 尚无 VCE 服务 API、RPC、项目设置、管理页面和事件投影。
- 尚未完成 CBM 设置迁移、主链切换和原生设施退出。
- `workspace_isolation.rs` 仍以“VCE 零消费者”为当前阶段断言，正式接入后必须替换。

### 2.3 明确不属于本计划

- 不建立 VCE MCP、HTTP、sidecar 或独立守护进程。
- 不引入 Milvus。
- 不让 VCE 直接打开 LanceDB 或 SQLite。
- 不把 VMM 的记忆数据、知识库文档数据并入代码语义索引。
- 不让 VCE 调用 CBM、Codebase Memory MCP 或 CBM Controller。
- 不替换 CodeKit 的精确文本、AST 和文件导航能力。
- 不删除用户磁盘上的既有 CBM 数据。
- 不在事实来源未确认时虚构编辑器未保存缓冲区协议。

## 3. 冻结架构与依赖方向

### 3.1 总体结构

```text
ConfiguredInferenceService
          │
          ▼
统一 HostAiPort（宿主持有 Provider、路由、配额、重试、取消、用量）
          │
          ├──────────► Native VMM Host Driver ─────► VMM-Core
          │
          ├──────────► 知识库 AI 适配器 ──────────► 知识库业务流水线
          │
          └──────────► VCE AI 适配器 ─────────────► VceEngine
                                                       │
项目快照与文件监听 ─► VCE Source 适配器 ───────────────┤
                                                       │
VLDB SQLite/LanceDB ─► VCE Index 适配器 ───────────────┘
                                                       │
                           ┌───────────────────────────┼────────────────────┐
                           ▼                           ▼                    ▼
                 Native Tool Handler           Service RPC          WebUI 管理面
```

### 3.2 依赖规则

依赖方向冻结如下：

```text
vulcan-ai-contract
        ↑
vulcan-context-engine（仅在 host-ai-contract feature 下使用公共 AI 类型）
        ↑
vulcan-service-kernel（组合根、VCE 运行管理器和三个具体适配器）
        ├────────► vulcan-workspace-store（VCE 专属数据面）
        ├────────► vulcan-file-watcher（共享物理监听器）
        ├────────► vulcan-agent-runtime（工具执行 handler）
        └────────► vulcan-service-api（RPC 数据契约）
```

必须遵守：

- `vulcan-context-engine` 不得依赖 `vulcan-service-kernel`、`vulcan-workspace-store`、`vulcan-agent-runtime`、VMM 或 CBM。
- 存储实现放在 `vulcan-workspace-store`，组合、生命周期和端口适配放在 `vulcan-service-kernel`。
- `vulcan-agent-runtime` 只依赖一个抽象检索 handler，不直接依赖 service kernel。
- WebUI 只调用 service API，不直接理解 LanceDB、SQLite、checkpoint 或 HostAI handle。

### 3.3 业务域隔离

| 领域 | 业务用途 | AI 契约 | SQLite | LanceDB | 生命周期 |
| --- | --- | --- | --- | --- | --- |
| VCE | 代码语义检索 | 统一 `HostAiPort` | VCE 专属 Schema | VCE 专属向量表 | 项目级 |
| Native VMM | 记忆 | 统一 `HostAiPort` | VMM 专属 Schema | VMM 专属向量表 | 服务级/记忆域 |
| 知识库 | 用户文档知识 | 统一 `HostAiPort` | 知识库专属 Schema | 知识库专属向量表 | 资料库级 |

“共用”只表示统一注册接口、统一 Provider 配置服务和统一 VLDB 基础设施，不表示复用业务表、向量空间、索引 generation 或状态机。

## 4. 关键技术决策

### 4.1 实现来源与许可证

- VCE 保持当前 MIT 独立实现路线。
- OCE 与 `oce-client` 仅作为行为规格和思路来源，不逐行翻译 Apache-2.0 源码，不复制其 MCP、HTTP、Milvus、Provider 或进程管理代码。
- 后续每次补齐 OCE 行为时，先写行为测试，再以 VCE 现有领域模型独立实现。

### 4.2 唯一运行时入口

- 每个已启用项目对应一个 `VceEngine` 实例。
- 上层只通过 `VceRuntimeManager` 管理项目实例，不允许 service RPC、WebUI 或工具 handler 直接持有可变 `VceEngine`。
- 每个项目只有一个串行命令 worker；刷新、重建、维护、恢复和关闭均进入同一命令队列。
- 检索读取当前已提交 checkpoint，不等待正在执行的刷新；不得读取暂存 generation。

### 4.3 统一 HostAI 注册规则

VCE 不新增第二套 AI 服务规则。注册、权限、预算、路由、用量和取消全部使用已冻结的 `HostAiCapabilityRegistration`、`HostAiCapabilityHandle`、`HostAiCallContext` 和 `HostAiPort`。

本计划冻结 VCE 的宿主身份规则：

| 字段 | 规则 |
| --- | --- |
| `consumer_profile_id` | 固定系统 Profile `vce`，由公共 AI 设置页配置 LLM、Embedding、Rerank 角色 |
| `owner_id` | `vce-project:{project_id}` |
| `registration_id` | `vce-project:{project_id}:inference` |
| `caller_id` | `vulcan-code-vce:{project_id}` |
| `usage_scope` | 固定 `InternalBackground` |
| 注册级 deadline | `None`，避免长生命周期 handle 永久过期 |
| 请求级 deadline | 每次物理调用根据 caller budget 与操作 deadline 计算 |
| 必需能力 | `EmbeddingCreate`，`purpose_id = None` |
| 可选能力 | `ConversationRespond` + `code_retrieval.query_rewrite`；`RerankDocuments` + `purpose_id = None` |
| 权限 | `Status`、`Embedding`，并按已配置可选能力加入 `Llm`、`Rerank`、`Cancel` |

说明：

- `VceAiPurpose` 中六个稳定用途字符串继续作为 VCE 领域遥测与调用语义；HostAI 注册层只对按用途路由的 LLM 查询改写声明 `purpose_id`，Embedding 和 Rerank 按当前统一契约使用 `None`。
- 一个项目只保持一个活动 VCE capability handle；公共 AI 设置变化时由运行管理器注销旧 handle、重新注册并原子替换快照。
- Embedding 是索引与检索的必需能力；缺失时项目状态为 `blocked_configuration`，不得伪装为空结果。
- 查询改写和 Rerank 是可选能力；缺失时 VCE 使用现有显式降级路径，并在结果诊断和管理状态中记录原因。
- VCE 不缓存 Provider 凭据、模型路由或首次注册时的陈旧能力快照。

### 4.4 项目、命名空间和向量空间身份

统一采用由宿主计算、VCE 校验的确定性身份：

| 身份 | 规则 |
| --- | --- |
| 项目身份 | 当前 workbench 权威 `project_id` |
| 根目录身份 | 规范化绝对项目路径的 SHA-256，取完整值持久化、短值仅用于表名 |
| `checkpoint_chain_id` | `project:{project_id}:root:{root_hash}` |
| `index_namespace_id` | 由 `VceConfig::new` 生成 `vce:{checkpoint_chain_id}` |
| `vector_space_id` | `code-semantic:{project_id}:{root_hash}` |
| `index_schema_version` | 取 VCE 当前公开 Schema 版本，初始为 `1` |
| 物理向量兼容性 | `physical_fingerprint + dimension + preprocessing_version + chunker_version` |

项目路径变化不能继续复用旧 checkpoint chain。检测到同一 `project_id` 的根目录 hash 变化时，必须停止旧 worker，建立新身份并执行全量刷新；旧数据只标记为孤立命名空间，等待显式维护命令清理。

Embedding Provider、模型、维度、预处理或切块版本变化时，必须创建新的构建 generation 并全量重建；不得对不兼容向量执行查询，也不得静默沿用旧物理指纹。

### 4.5 存储归属与提交协议

- SQLite 和 LanceDB 连接均由 VulcanCode 创建，通过 `vulcan-workspace-store` 的 VCE 数据面持有。
- VCE 端只接收 `VceIndexPort`。
- SQLite 是 checkpoint、generation、文件、Blob、Chunk、符号、作业和完整性元数据的权威来源。
- LanceDB 只保存内容向量和路径向量，不保存唯一业务真相。
- 精确检索按 OCE 实际处理逻辑使用 SQLite `vce_symbols` 标识符出现位置索引；它只接收 VCE 已提取并校验的 exact identifier，不把自然语言或模型原始输入拼成 FTS 表达式。
- SQLite 与 LanceDB 不具备跨引擎 XA 事务，因此使用“暂存 generation + SQLite 权威活动指针 + 幂等恢复”协议实现原子可见性。

提交顺序冻结为：

1. 在 SQLite 建立 `staging` generation 和 operation 记录。
2. 幂等写入 SQLite 暂存 Blob/Chunk/符号元数据和 LanceDB 暂存向量。
3. 校验 operation、refresh、generation、chain、预期 checkpoint 版本和完整 Blob 集合。
4. 在单个 SQLite 事务内写入最终成员关系、更新 `active_generation_id` 与 checkpoint 版本，并把 operation 标记为 `committed`。
5. 查询端先从 SQLite 当前 checkpoint 取得精确 Blob 成员集合，再以 namespace、Blob 集合、vector space、物理指纹和维度过滤 LanceDB；exact 查询只读取当前已提交 Blob 关联的 `vce_symbols`。
6. 进程崩溃后扫描非活动 generation：已提交的继续保留，未提交的执行幂等中止与垃圾清理。

不得先把新向量暴露给检索再更新 checkpoint；不得把 LanceDB 写入成功等同于 VCE 提交成功。

### 4.6 文件监听与源码快照

- 物理监听基础设施复用 `vulcan-file-watcher`，不在 VCE 内创建 watcher。
- 将当前 Git watcher 所持有的文件监听能力抽取为 service-owned 共享工作区监听中心；Git 刷新和 VCE 各自拥有独立 subscriber。
- VCE subscriber 对每个项目注册项目根目录，经过节流后把事件归并为规范相对路径集合。
- 初次启用、根目录变化、Schema 变化或向量兼容性变化调用 `RefreshRequest::full`。
- 稳定的新增、修改、删除、重命名事件调用 `RefreshRequest::paths`。
- 一个项目同一时间最多执行一个刷新；刷新期间到达的事件合并进下一批 dirty set。
- 刷新失败不丢弃 dirty set；按宿主退避策略重试，并保留最后错误。
- 删除事件必须进入 VCE 的缺失分类和 checkpoint 提交流程，不能只删除 LanceDB 行。
- Git 变化只作为文件变化来源之一；VCE 不依赖 Git 仓库存在，也不通过 `git status` 代替文件快照。

`VceSourcePort` 必须：

- 从权威项目根目录解析规范相对路径，拒绝越界、设备路径、目录穿越和不受支持的符号链接逃逸。
- 为每次刷新创建不可变 manifest，包含路径、size、mtime 和 Blob 身份。
- 按 VCE 请求读取缺失 Blob 正文，并受 Blob 数量、单文件大小和批次总字节数限制。
- 读取后重新校验 size、mtime 和 Blob；变化时显式返回快照冲突并重新排队，而不是索引混合版本。
- 使用 VCE 的忽略、文件大小和语言支持规则，不把二进制、构建产物或敏感工作区外文件送入 AI。

编辑器未保存覆盖层只在确认 VulcanCode 当前真实编辑器文档事件来源后接入 `observe/remove`。实施阶段必须先找到唯一权威事件与文档身份；如果当前产品没有该来源，本次发布明确保持 filesystem-only，不编写候选字段或多路 fallback。

### 4.7 工具面

AI 只看到一个新工具：

```text
codebase_retrieval(information_request: string)
```

工具规则：

- Schema、名称和英文描述从 `VceToolCatalog` 投影，不在 agent runtime 重新维护第二份同义定义。
- 模型只能提供 `information_request`，不能指定项目路径、workspace ID、namespace、checkpoint、Profile 或向量空间。
- 工具 handler 从当前执行上下文取得权威 `project_id`，再调用 `VceRuntimeManager::retrieve`。
- 返回值投影包含选中代码上下文、规范相对路径、行号、来源 Blob、分数/通道摘要、checkpoint 和显式降级信息。
- 未启用、正在初建、配置阻塞、项目不存在和索引失败必须返回可区分的结构化错误，不返回伪造空结果。

下列 OCE 语义保持宿主专用，不注册为 AI 工具：

- `find_missing`
- `batch_upload`
- `checkpoint_blobs`
- `sync`
- `sync_paths`
- `status`
- `observe`
- `remove`
- `watch`
- 管理查询、完整性检查、重建和暂存清理

工具职责分工冻结为：

| 场景 | 首选能力 |
| --- | --- |
| 不知道实现位置，需要按自然语言理解代码职责 | VCE `codebase_retrieval` |
| 已知字面量、日志、函数名或正则，需要精确定位 | CodeKit/RG |
| 已知文件，需要 AST、结构或安全整函数替换 | CodeKit |
| 项目状态、索引统计、重建、完整性和维护 | VCE 宿主 API/WebUI，不对 AI 暴露 |

### 4.8 管理面与权限

- Workbench 主快照只携带轻量项目 VCE 摘要和 revision，不内嵌大列表。
- 文件、Blob、Chunk、作业、完整性和维护计划通过独立分页 RPC 查询。
- 所有破坏性维护操作分成“生成只读计划”和“显式执行命令”两步。
- 清理孤立 namespace、删除索引、全量重建必须校验当前项目身份、配置 revision 和预期 checkpoint。
- WebUI 不直接拼 SQL、LanceDB filter 或维护命令。
- 页面显示 Provider 无关的能力状态，不暴露凭据、连接密钥或内部请求正文。

## 5. 目标组件与文件布局

以下为计划中的目标布局；实施时若当前源码结构已变化，必须先以 AST 和调用链重新验证，再更新本计划，禁止凭历史路径直接修改。

### 5.1 `vulcan-workspace-store`

新增建议目录：

```text
crates/vulcan-workspace-store/src/vce/
├── mod.rs
├── models.rs
├── schema.rs
├── data_plane.rs
├── fts.rs
├── vector.rs
├── recovery.rs
└── tests.rs
```

职责：

- 定义 VCE 专属 SQLite Schema 和 VLDB 绑定。
- 建立 VCE 专属 SQLite 精确符号索引。
- 建立内容向量与路径向量 LanceDB 表。
- 提供 namespace、generation、checkpoint、作业和维护查询。
- 提供暂存、提交、中止、恢复和孤立数据识别。
- 不暴露原始数据库句柄给 VCE。

### 5.2 `vulcan-service-kernel`

新增建议目录：

```text
crates/vulcan-service-kernel/src/vce/
├── mod.rs
├── runtime_manager.rs
├── project_runtime.rs
├── ai_port.rs
├── index_port.rs
├── source_port.rs
├── watcher.rs
├── services.rs
└── tests.rs
```

职责：

- `VceRuntimeManager`：项目实例、生命周期、能力刷新、命令队列和状态事件。
- `VceHostAiAdapter`：统一 HostAI handle 到 `VceAiPort` 的映射。
- `VceWorkspaceIndexAdapter`：VCE 数据面到 `VceIndexPort` 的映射。
- `VceProjectSourceAdapter`：项目快照到 `VceSourcePort` 的映射。
- watcher subscriber：文件事件合并、节流和刷新调度。
- service methods：设置、状态、查询、重建、维护和工具检索入口。

需要修改的宿主入口：

- `crates/vulcan-service-kernel/src/service_bootstrap.rs`
- `crates/vulcan-service-kernel/src/state.rs`
- `crates/vulcan-service-kernel/src/lib.rs`
- `crates/vulcan-service-kernel/src/rpc_dispatch/`
- `crates/vulcan-service-kernel/src/workbench_refresh.rs`
- 公共 AI 设置与项目设置相关模块

### 5.3 `vulcan-agent-runtime`

新增建议文件：

```text
crates/vulcan-agent-runtime/src/tooling/native/codebase_retrieval.rs
```

需要修改：

- `crates/vulcan-agent-runtime/src/tooling/native.rs`
- `crates/vulcan-agent-runtime/src/tooling/native/registry.rs`
- `crates/vulcan-agent-runtime/src/tooling/bridge.rs`
- `crates/vulcan-agent-runtime/src/execution_models.rs`
- `crates/vulcan-agent-runtime/src/execution_preparation/tool_exposure.rs`
- `crates/vulcan-agent-runtime/src/execution_preparation/codebase_tool_routing_prompt.rs`

职责：

- 定义与注入 `VceRetrievalHandler` 抽象。
- 注册唯一 AI 可见工具。
- 从会话执行上下文取得项目身份。
- 更新工具路由提示，明确 VCE 与 CodeKit 分工。
- 移除 Host-managed CBM 上下文和重复工具路由。

### 5.4 `vulcan-service-api`

新增建议文件：

```text
crates/vulcan-service-api/src/vce.rs
```

公共契约至少包括：

- 项目 VCE 设置、revision 与更新命令。
- 轻量状态摘要和能力状态。
- 分页文件列表。
- Blob 与 Chunk 详情。
- 当前和历史作业。
- 统计信息。
- 完整性问题。
- 维护计划。
- 全量重建、重试、取消和维护执行命令。
- 事件名称和 payload revision。

### 5.5 WebUI

新增建议目录：

```text
apps/vulcan-code-web/src/components/features/code-context/
├── code-context-area.tsx
├── code-context-summary.tsx
├── code-context-files.tsx
├── code-context-blob-detail.tsx
├── code-context-jobs.tsx
├── code-context-integrity.tsx
└── code-context-maintenance.tsx
```

需要修改：

- `apps/vulcan-code-web/src/components/project/project-settings-dialog.tsx`
- `apps/vulcan-code-web/src/components/project/project-settings-sections.tsx`
- `apps/vulcan-code-web/src/components/project/project-settings-types.ts`
- `apps/vulcan-code-web/src/state/workbench-state.ts`
- `apps/vulcan-code-web/src/types/projects-files.ts`
- `apps/vulcan-code-web/src/types/workbench.ts`
- `apps/vulcan-code-web/src/service/`
- `apps/vulcan-code-web/src/i18n-messages.ts`

用户界面统一使用“代码语义索引”或“代码上下文”，不要求用户理解 VCE、Embedding 指纹、LanceDB 或 checkpoint 内部实现。

## 6. 存储模型实施规格

### 6.1 SQLite 权威表

计划建立以下逻辑表；实际 SQL 名统一使用 `vce_` 前缀：

| 表 | 核心职责 |
| --- | --- |
| `vce_project_bindings` | 项目、根目录 hash、namespace、chain、vector space、配置 revision、活动状态 |
| `vce_embedding_spaces` | 逻辑向量空间、物理指纹、维度、预处理和切块版本 |
| `vce_checkpoints` | checkpoint 版本、已提交 generation、提交时间、源码 revision 和完整成员 JSON |
| `vce_state_snapshots` | 与 checkpoint 同事务发布的 VCE 状态快照 |
| `vce_generations` | refresh/generation 状态、operation、时间和错误 |
| `vce_staged_blobs` / `vce_staged_chunks` / `vce_staged_symbols` | 发布前的完整暂存成员与通道状态 |
| `vce_blobs` | 当前 checkpoint 的 Blob、规范路径、分析版本、结构/辅助通道诊断和聚合计数 |
| `vce_chunks` | 当前 checkpoint 的 Chunk 身份、Blob、路径、行号、正文、内容 hash 和向量状态 |
| `vce_symbols` | 精确符号、定义种类、路径、行号和 Chunk 归属 |
| `vce_operations` | refresh/retrieve/maintenance 的稳定 operation、阶段、重试和错误 |
| `vce_commit_receipts` | operation 指纹与幂等提交回执 |
| `vce_commit_guards` | 在 SQLite 事务内部强制 checkpoint CAS 的临时保护行 |
| `vce_vector_cleanup` | 中止、删除和被替换 Blob 的派生向量清理队列与结果 |

所有主键和唯一约束必须包含 namespace 或项目身份，禁止跨项目碰撞。所有路径只存规范相对路径；绝对根目录只存于项目绑定记录，不复制到每个 Chunk。

### 6.2 SQLite 精确符号索引

- 使用普通 SQLite `vce_symbols` 表和复合索引 `(namespace_id, identifier, kind, path, start_line)`，与 OCE 的 exact symbol store 行为一致。
- VCE 在查询规划阶段提取并校验 exact identifier；存储层只执行参数化标识符匹配，不接收 SQL、任意谓词或 FTS 表达式。
- exact 行绑定 namespace、Blob、Chunk、路径、行号、字符偏移和出现类型；查询范围由当前 checkpoint 的 Blob 成员集合限定。
- exact 搜索结果由 adapter 转换为 VCE 类型，并完整校验路径、Blob、Chunk、行号、内容 hash 和重复项。
- 自然语言语义召回交给内容向量与路径向量；VCE 不再复制知识库全文检索能力，避免与 CodeKit `rg` 和知识库搜索重叠。

### 6.3 LanceDB

使用两个独立逻辑表：

- 内容向量表：Chunk 代码语义向量。
- 路径向量表：路径文档向量。

每行至少包含：namespace、generation、vector_space_id、physical_fingerprint、dimension、Blob ID、Chunk ID、path、行号、内容 hash、向量和写入时间。

表名必须包含稳定的 VCE 前缀与维度，避免与 VMM、知识库冲突。检索必须同时过滤 namespace、当前 checkpoint Blob 集合、vector space、物理指纹和维度；Blob 集合而非 generation 是复用未变化旧 Blob 的权威可见性边界。

### 6.4 恢复与清理

- 启动时先读取 SQLite 权威活动指针，再恢复 `VceStateSnapshot`。
- 状态快照身份与 index binding 不一致时拒绝恢复并进入 `rebuild_required`。
- 未提交 generation 不参与查询。
- 中止和崩溃残留的向量行通过 `vce_vector_cleanup` 幂等清理任务处理；未提交的 SQLite 暂存行不参与查询。
- 孤立 namespace 默认只报告，不自动删除。
- 管理页先生成维护计划；只有用户显式执行后才清理。

## 7. 运行时流程

### 7.1 服务启动

1. `run_service_bootstrap` 创建现有 `ConfiguredInferenceService` 和共享 `Arc<dyn HostAiPort>`。
2. 完成 VMM 数据面与 Native VMM 初始化，保持现有顺序和失败语义。
3. 创建 VCE 数据面工厂和共享文件监听中心。
4. 创建 `VceRuntimeManager`，注入 HostAI Port、数据面工厂、项目仓储、事件发送器和 watcher。
5. 将 manager 或其只读/执行 handler 放入 `ServiceBridgeState` 与 `AgentRuntimeBridge`。
6. 读取项目设置，只为启用 VCE 的项目启动 runtime。
7. 对每个项目校验根目录、AI capability、存储 binding 和恢复快照。
8. 可恢复则直接进入 `ready`；缺失或不兼容则排队全量刷新。

VCE 单项目失败不能阻止 VulcanCode 主服务启动，但必须在项目状态和日志中显式显示失败；HostAI 公共服务本身的既有启动失败语义保持不变。

### 7.2 初次全量刷新

1. 建立不可变文件 manifest。
2. 调用 VCE `sync_plan`/`refresh` 的全量入口。
3. VCE 分类缺失、变化和删除 Blob。
4. `VceSourcePort` 只提供缺失正文。
5. 结构分析、递归回退、切块和符号提取。
6. 按 VCE 配置进行分段、批量 Embedding 和有界并发。
7. 分批写入暂存 generation。
8. 校验完整回执并 CAS 发布 checkpoint。
9. 释放已提交 filesystem 正文，只保留必要状态。
10. 发出 project-scoped 状态 revision 和 Workbench 轻量刷新事件。

### 7.3 增量刷新

1. watcher 将原始事件映射为规范相对路径。
2. 对连续事件节流、去重并合并 rename/delete。
3. worker 空闲时提取 dirty set，调用 `RefreshRequest::paths`。
4. 执行期间新增事件进入下一 dirty set。
5. 成功后只移除本批已确认路径。
6. 快照冲突、暂时性 IO 或 Provider 暂时错误按显式策略重排。
7. 配置错误、维度或指纹不匹配停止自动重试，进入 `blocked_configuration` 或 `rebuild_required`。

### 7.4 检索

1. Native Tool handler 根据会话取得唯一 `project_id`。
2. manager 检查项目已启用且存在活动 checkpoint。
3. 构建 `RetrieveRequest::current_checkpoint`，默认关闭无条件 LLM query rewrite，允许可选 Rerank。
4. VCE 生成查询计划并执行 dense、exact、path 多通道召回。
5. 执行 RRF、精确合并、可选 Rerank、路径增强和覆盖选择。
6. adapter 校验每个宿主响应的 operation、范围、路径、Blob、Chunk、行号、hash、分数和重复项。
7. 返回有界代码上下文和诊断，不把数据库行或 Provider 原始响应暴露给模型。

### 7.5 配置变化

- VCE 开关变化：通过带 revision 的项目设置命令启停，不静默改写。
- AI Profile 路由变化：刷新 capability handle；若 Embedding 物理空间变化，进入全量重建。
- Rerank 或查询改写缺失：更新 capability snapshot，不重建主索引。
- 语言/忽略/文件大小/切块配置变化：提升配置 revision，并根据兼容性分类为路径刷新或全量重建。
- 项目根目录变化：建立新 chain 和 namespace。

### 7.6 关闭与项目移除

1. 停止接收该项目的新工具调用。
2. 注销 watcher 路径。
3. 取消并等待刷新/维护任务。
4. 持久化最终状态与作业结果。
5. 注销 HostAI capability。
6. 释放数据面句柄。
7. 默认保留磁盘索引；删除数据必须走单独维护命令。

## 8. 项目设置与配置迁移

### 8.1 新设置模型

项目设置新增独立 VCE 域，至少包含：

- `enabled`
- `revision`
- 文件大小限制
- 忽略规则扩展
- 是否允许查询改写
- 是否允许 Rerank
- 索引并发与查询预算的受控高级设置
- 当前配置兼容性摘要

系统 AI 设置新增 `vce` consumer profile，复用现有公共 AI 配置模型，不建立 VCE 专属 Provider 配置文件。

### 8.2 CBM 开关迁移

当前 `codebase_memory_enabled` 只作为一次性迁移来源：

1. 数据库迁移新增 VCE 设置和迁移标记。
2. 对尚未迁移的项目，把旧开关值复制为 VCE 初始 `enabled` 值，以保留用户明确意图。
3. 写入迁移标记后，运行时只读取 VCE 设置，不允许 `vce_enabled || codebase_memory_enabled` 之类长期 fallback。
4. UI 明确提示“代码语义索引已迁移”，不偷偷开启旧值原本关闭的项目。
5. 删除 CBM 字段前先完成一版兼容读取和持久化迁移验证；删除字段作为后续明确 Schema 迁移，不与运行时 fallback 混用。

## 9. CBM 主链退出方案

### 9.1 原则

- VCE 对 CBM 保持零依赖。
- 不以 CBM 作为 VCE 查询失败时的降级。
- 不向 AI 同时暴露语义重叠的 CBM 与 VCE 工具。
- CBM 可在未来以独立插件存在，但插件生命周期、工具和数据不属于原生 VCE。

### 9.2 移除范围

完成 VCE 端到端验收后，移除或替换以下原生主链：

- `vulcan-service-kernel/src/codebase_memory_controller.rs`
- `ServiceBridgeState.codebase_memory_controller`
- service bootstrap 中 CBM reconcile 和 recovery monitor
- CBM 设置 RPC 与 Workbench 投影
- `vulcan-agent-runtime/src/mcp/managed_codebase_memory.rs`
- execution request 中 `managed_codebase_memory`
- tooling bridge 的 CBM managed context 注入
- CBM 专属工具路由提示
- `HostManagedCodebaseMemory` MCP 配置来源
- WebUI 的 `codebase-memory-service.ts` 和 CBM 项目设置组件/类型
- 原生安装、初始化或环境检测中只为 CBM 存在的路径

### 9.3 保留范围

- CodeKit/RG/AST 精确工具继续保留。
- 用户磁盘上的历史 CBM 数据不自动删除。
- 若存在独立插件化 CBM，其安装、启用和工具命名必须与原生 VCE 分离。
- 迁移版本保留可诊断的旧设置读取，但不得重新启动 CBM 服务。

## 10. 分阶段执行计划

每个阶段均须先检查脏工作树，只修改和暂存本阶段拥有的文件；发现并行会话修改时必须隔离，不得覆盖。每个阶段先小范围验证，再进入下一阶段。

### 阶段 0：事实冻结与实施基线

任务：

1. 记录当前 HEAD、工作树、VCE 测试基线、HostAI/VMM/知识库测试基线。
2. 用 AST 和调用点确认本文列出的 service bootstrap、状态、项目设置、工具和 watcher 路径仍有效。
3. 建立本计划的逐文件覆盖台账与问题台账。
4. 冻结 VCE identity、RPC 名称、事件名称和用户文案词汇。
5. 确认当前编辑器是否存在权威未保存文档事件；没有则记录 filesystem-only 边界。

验收：

- 无未确认字段、候选路径或隐式 fallback 进入实现。
- 基线失败与本任务新增失败分开记录。

建议提交：无需代码提交；只提交经确认的计划修订或契约文档。

### 阶段 1：VCE 专属数据面

任务：

1. 在 `vulcan-workspace-store` 建立 VCE 模块、Schema 和迁移。
2. 建立 SQLite 精确符号索引和 LanceDB 专属 binding。
3. 实现 namespace、generation、checkpoint 和 operation 存储。
4. 实现 stage、commit、abort、search、management query 和 recovery。
5. 实现内容、路径、exact 三类检索。
6. 增加 schema、隔离、CAS 冲突、崩溃恢复、幂等和跨项目测试。

验收：

- 不与 VMM、知识库表名或向量空间冲突。
- 未提交 generation 永远不可见。
- 同一 commit 重放幂等。
- checkpoint 版本冲突显式失败。
- 重启恢复后活动 checkpoint 与向量/精确符号成员一致。

建议提交标题：`实现 VCE 专属 VLDB 数据面与原子检查点存储`

### 阶段 2：HostAI 与三个 VCE 端口适配

任务：

1. 在 service kernel 增加 `vulcan-context-engine` 依赖并启用 `host-ai-contract` feature。
2. 实现固定身份与 caller budget 的 VCE capability 注册。
3. 实现 `VceHostAiAdapter`，覆盖 Embedding、查询改写、Rerank、快照刷新、错误和取消映射。
4. 实现 `VceWorkspaceIndexAdapter`。
5. 实现 `VceProjectSourceAdapter` 和路径安全校验。
6. 增加 HostAI request/response identity、维度、指纹、顺序和错误映射测试。

验收：

- VCE 不接触 `ConfiguredInferenceService`。
- 全部 AI 请求使用 `InternalBackground`。
- registration/profile/vector identity 不匹配时在边界拒绝。
- Embedding 缺失阻断，Rerank/改写缺失显式降级。
- Provider 配置刷新后不继续使用旧 handle。

建议提交标题：`接入 VCE 统一宿主 AI 与源码索引端口`

### 阶段 3：项目运行管理器与恢复

任务：

1. 实现 `VceRuntimeManager` 和项目级 runtime。
2. 建立项目命令队列、状态机、取消源和只读检索快照。
3. 接入 `ServiceBridgeState`、`AgentRuntimeBridge` 和 bootstrap。
4. 实现启用、禁用、启动恢复、全量刷新、重建、停止和项目移除。
5. 增加单项目失败隔离和多项目并发隔离。

状态至少包含：

- `disabled`
- `starting`
- `restoring`
- `indexing`
- `ready`
- `degraded_optional_ai`
- `blocked_configuration`
- `rebuild_required`
- `failed`
- `stopping`

验收：

- 同项目无并发写入引擎。
- 不同项目可并行，但受全局 AI 和存储预算限制。
- 服务重启能恢复 ready 项目。
- 单项目失败不阻断服务与其他项目。

建议提交标题：`建立 VCE 项目运行管理器与服务生命周期`

### 阶段 4：共享文件监听与增量刷新

任务：

1. 抽取 service-owned 共享文件监听中心。
2. 将现有 Git watcher 改为共享 watcher subscriber，保持现有行为和测试。
3. 为 VCE 增加项目 subscriber、节流、路径归并和 dirty set。
4. 处理新增、修改、删除、重命名、目录批量变化和文件风暴。
5. 增加刷新失败重排、配置阻断和关闭取消。

验收：

- 不增加 VCE 自有物理 watcher。
- Git 刷新行为无回归。
- 事件风暴不会并发启动多个同项目 refresh。
- 变化发生在读取期间时不会提交混合快照。
- 删除和重命名后的旧 Chunk 不再可检索。

建议提交标题：`复用工作区文件监听驱动 VCE 增量刷新`

### 阶段 5：Native Tool 完整暴露链

任务：

1. 增加 `VceRetrievalHandler` 抽象和执行上下文注入。
2. 从 `VceToolCatalog` 投影 `codebase_retrieval` 定义。
3. 接入 Native Tool Catalog、暴露过滤、请求构建、执行和结果投影。
4. 以当前会话权威项目身份路由。
5. 更新路由提示，明确 VCE 与 CodeKit 分工。
6. 验证所有 agent mode、策略和工具选择路径。

验收：

- AI 只能看到一个 VCE 语义工具。
- 生命周期和管理操作不可被 AI 调用。
- 模型参数不能越权切换项目。
- 工具从注册到结果投影链路完整。
- 无 CBM 上下文注入或重复语义工具。

建议提交标题：`注册 VCE 原生代码语义检索工具`

### 阶段 6：服务 API、项目设置与 WebUI 管理面

任务：

1. 增加 `vulcan-service-api::vce` 类型和事件。
2. 增加唯一归属的 VCE RPC dispatch 模块和方法名测试。
3. 扩展 Workbench 轻量项目摘要与 revision。
4. 增加项目设置中的代码语义索引开关和状态。
5. 增加代码上下文管理区域：概要、统计、文件、Blob/Chunk、作业、完整性和维护。
6. 实现全量重建、失败重试、取消、生成维护计划和显式执行。
7. 补齐中文和英文用户文案及浏览器降级快照。

验收：

- 大列表全部分页，不膨胀 WorkbenchSnapshot。
- UI revision 冲突可见，不覆盖较新设置。
- disabled/unavailable/failed/ready 状态不混淆。
- 维护操作有预览、确认、checkpoint 校验和审计结果。
- 页面刷新、重连和服务重启后状态一致。

建议提交标题：`完善 VCE 项目设置与代码上下文管理页面`

### 阶段 7：设置迁移与 CBM 原生主链切换

任务：

1. 添加一次性项目设置迁移和迁移标记。
2. 验证旧开关 true/false、空项目、新项目和重复迁移。
3. 停止 bootstrap 启动 CBM Controller、reconcile 和 recovery。
4. 移除 managed CBM MCP 注入、工具和路由提示。
5. 移除 CBM RPC、Workbench 投影和 WebUI 设置入口。
6. 删除已无引用的 CBM 原生代码和依赖。
7. 保留磁盘数据并记录插件化边界。

验收：

- 运行时依赖图不存在 VCE → CBM。
- 原生服务不再启动 CBM。
- AI 工具面不再出现 CBM 语义工具。
- 用户原有启用意图迁移正确。
- 旧数据未被删除。

建议提交标题：`切换代码语义主链并退出原生 CBM 集成`

### 阶段 8：完整验证、性能与故障注入

任务：

1. 执行 crate 定向测试、工作区依赖图测试、API/RPC 测试和 WebUI 测试。
2. 执行真实临时项目端到端测试：初建、检索、修改、删除、重命名、重启和项目切换。
3. 注入 Provider 超时、限流、维度变化、指纹变化、SQLite 失败、LanceDB 失败、CAS 冲突和进程中断。
4. 验证文件风暴、超大文件、二进制、路径穿越和符号链接逃逸。
5. 记录索引耗时、峰值内存、增量刷新延迟、检索延迟、AI 调用次数和存储体积。
6. 与移除前 CBM 的代表性项目结果做离线质量对照，但不在运行时双路调用。

最低性能验收以同一机器、同一项目、同一 AI 配置的基线报告为准，必须至少满足：

- 单文件变更不触发无关全量重建。
- 检索不等待未提交增量刷新。
- AI 调用量可按 operation/purpose/project 审计。
- VCE 空闲时无轮询式持续 AI 调用。
- 内存不会保留全部项目源码正文。
- 可选 LLM query rewrite 默认关闭，不产生无条件 LLM 调用。

建议提交标题：`补齐 VCE 端到端故障恢复与性能验收`

### 阶段 9：文档、资源同步与交付

任务：

1. 更新 VCE README、功能入口、工具面和 VulcanCode 对接文档。
2. 更新开发环境说明，移除 CBM 原生初始化要求。
3. 更新架构图、项目设置说明、故障排查和数据保留策略。
4. 如果运行时资源发生变化，修改权威资源后执行 `make.ps1 sync`，不得直接编辑 `output/`。
5. 生成实施变更总结、逐文件清单、测试证据和回滚说明。
6. 按第 13 节执行连续五轮审核。

验收：

- 文档与当前源码一致。
- 没有把计划或旧完成记录当作唯一运行时契约。
- 所有新增工具、RPC、设置、事件和 Schema 有测试与文档。
- 五轮审核完成后，追加执行变更总结，并把本文件迁移为 `docs/completed/20260904/01-VCE_FULL_NATIVE_INTEGRATION.md`。

建议提交标题：`完成 VCE 原生集成交付与文档收敛`

## 11. 测试矩阵

### 11.1 VCE 内核与公共契约

- `cargo test -p vulcan-context-engine --all-features`
- `cargo clippy -p vulcan-context-engine --all-targets --all-features -- -D warnings`
- doctest 如存在则单独执行。
- 替换“零消费者”断言为精确依赖边界断言：只允许预定宿主 crate 消费 VCE。

### 11.2 Workspace Store

- Schema 初始化与重复迁移。
- 项目和 namespace 隔离。
- stage/commit/abort 幂等。
- CAS 冲突。
- SQLite 成功/LanceDB 失败与反向故障。
- 当前 checkpoint Blob 成员过滤。
- exact identifier 参数化查询、种类排序与作用域防护。
- dimension/fingerprint 不匹配。
- 崩溃恢复和孤立 generation 清理计划。

### 11.3 Service Kernel

- capability 注册、刷新、注销。
- Profile 未配置和可选能力缺失。
- 项目启停、恢复、移除、根路径变化。
- 文件事件归并、失败重排和关闭取消。
- 多项目隔离。
- Workbench revision 与事件顺序。
- RPC 方法唯一归属。

### 11.4 Agent Runtime

- NativeToolCatalog 存在 `codebase_retrieval`。
- 所有允许模式可见，禁止模式正确过滤。
- Schema 只接受 `information_request`。
- 项目身份只来自执行上下文。
- handler unavailable/disabled/indexing/failed/ready 投影。
- CodeKit 与 VCE 工具描述无冲突。
- CBM 工具和 managed context 不再出现。

### 11.5 WebUI

- 项目设置开关、revision 冲突和错误恢复。
- 管理页分页、空状态、加载、失败和重连。
- 作业实时状态和历史状态分层。
- 完整性与维护计划确认流程。
- 中文/英文文案。
- 浏览器无服务模式不伪造 ready。

### 11.6 端到端

至少建立以下固定夹具：

- Rust/TypeScript/Markdown 混合小项目。
- 包含同名符号的多目录项目。
- Git 与非 Git 项目。
- 含忽略目录、大文件、二进制和符号链接的项目。
- 两个项目并行启用。

场景：

1. 首次启用并检索。
2. 修改函数后增量刷新并检索新内容。
3. 删除和重命名文件后旧结果消失。
4. 刷新期间检索仍读取旧完整 checkpoint。
5. 服务重启恢复。
6. Provider/Embedding 模型变化触发 rebuild required。
7. Rerank 不可用时结果仍可用且诊断明确。
8. 项目切换不会串库。
9. 禁用后工具返回 disabled，后台任务停止。
10. CBM 不运行且历史数据仍在磁盘。

## 12. 安全、可靠性与可观测性要求

### 12.1 安全

- 所有文件路径在宿主边界规范化并验证位于项目根目录内。
- AI 工具参数不能指定文件系统根、项目或 namespace。
- 日志不得记录 Provider 凭据和完整敏感源码正文。
- 管理查询对源码正文使用明确权限与有界返回。
- 维护命令必须带 project、revision、checkpoint 三重前置条件。

### 12.2 可靠性

- 所有逻辑操作使用稳定 `operation_id`；物理重试递增 attempt，不生成新逻辑身份掩盖重试。
- 失败不得推进 checkpoint。
- 可重试与不可重试错误按 HostAI 和 VCE 错误码显式分类。
- service shutdown 必须等待 worker 收敛或记录可恢复中止状态。
- 所有 channel、task 和 subscriber 都有明确所有者和释放路径。

### 12.3 可观测性

按 project/operation/purpose 记录：

- 当前状态与阶段。
- dirty path 数量。
- 扫描、读取、结构分析、Embedding、暂存、提交耗时。
- 文件、Blob、Chunk、符号和向量数量。
- AI 请求数、输入量、usage、Provider 路由快照和降级原因。
- checkpoint/generation/version。
- 最近错误和可重试性。

管理页只显示非敏感摘要；完整诊断继续走日志和受控详情 RPC。

## 13. 连续五轮代码审核规则

完整实现后执行五轮全范围审核。任何一轮发现真实问题并完成修复后，连续无问题计数归零，从第一轮重新开始。

每轮必须覆盖全部变更文件，而不是只看上一轮修复：

1. 第一轮：架构边界、依赖方向、VMM/知识库隔离、CBM 零依赖。
2. 第二轮：状态机、并发、取消、关闭、错误和恢复。
3. 第三轮：SQLite/LanceDB/精确符号一致性、CAS、幂等和迁移。
4. 第四轮：工具完整暴露链、项目权限、RPC、WebUI revision 和安全。
5. 第五轮：测试充分性、性能、文档、资源同步、无死代码和无陈旧 CBM 引用。

每轮输出：

- 覆盖文件台账。
- 发现项及严重级别。
- 修复提交。
- 定向复测和回归测试结果。
- 当前连续零问题轮数。

完成条件是连续五轮均为零新增问题，不是累计执行五次。

## 14. 提交策略

### 14.1 提交边界

- 按阶段独立提交，禁止把无关脏工作树带入。
- 每次只暂存本阶段明确文件。
- Schema、迁移、代码和对应测试必须同提交。
- 工具定义、注册、暴露、执行和结果投影必须同一提交闭环。
- WebUI 类型、service、state、组件和测试必须同一提交闭环。

### 14.2 提交信息

全部提交使用中文，采用“首行简述 + 空行 + 多行列表详述”。示例：

```text
实现 VCE 专属 VLDB 数据面与原子检查点存储

- 增加代码语义专属 SQLite 精确符号索引与 LanceDB 绑定
- 实现 generation 暂存、CAS 发布、恢复和清理协议
- 补齐项目隔离、故障注入与幂等测试
```

## 15. 回滚与发布策略

### 15.1 发布前开关

- VCE 运行时接入阶段使用独立项目设置控制，不通过隐藏环境变量长期维持双主链。
- 在 CBM 删除提交前完成完整 VCE 验收。
- 不采用运行时自动回退 CBM；回滚通过版本回退或明确恢复旧构建完成。

### 15.2 数据回滚

- VCE 使用独立 namespace，因此回滚不会污染 VMM、知识库或历史 CBM 数据。
- 新 Schema 迁移必须向前兼容读取或提供明确版本拒绝，不执行自动破坏性降级。
- 回滚版本不识别 VCE 数据时保持数据原样，不自动清理。

### 15.3 失败发布处理

- HostAI 配置问题：项目进入配置阻塞，主服务可用。
- VCE 索引损坏：隔离项目 namespace，生成重建计划。
- watcher 回归：可禁用 VCE 项目 runtime，但不重新启动 CBM。
- 工具回归：从 Native Tool Catalog 暂时撤下 VCE 工具，管理面保留诊断与重建；修复后再发布。

## 16. 最终验收清单

### 16.1 架构

- [x] VCE 只依赖允许的公共契约。
- [x] VMM、知识库、VCE 共用 HostAI 规则但数据完全隔离。
- [x] VCE 不创建 AI、SQLite、LanceDB、Milvus、watcher、工具或页面管理器。
- [x] VCE 对 CBM 零依赖。

### 16.2 功能

- [x] 项目启用后可全量索引。
- [x] 文件变化可增量刷新。
- [x] 重启可恢复。
- [x] `codebase_retrieval` 可从当前项目返回语义上下文。
- [x] CodeKit 精确工具继续工作。
- [x] 管理页可查询状态、统计、文件、Blob、作业和完整性。
- [x] 重建、重试、取消和维护命令闭环。

### 16.3 一致性

- [x] 未提交 generation 不可见。
- [x] CAS 冲突不推进 checkpoint。
- [x] Embedding 指纹/维度变化不会混用旧向量。
- [x] 删除/重命名后旧结果消失。
- [x] 多项目无串库。

### 16.4 工具与权限

- [x] AI 只新增一个 VCE 工具。
- [x] 模型不能选择任意项目或路径。
- [x] 工具完整暴露链有测试。
- [x] HostOnly 操作未进入模型工具列表。

### 16.5 CBM 退出

- [x] 原生 CBM 服务不再启动。
- [x] managed CBM MCP 和路由提示已移除。
- [x] 项目启用意图已一次性迁移。
- [x] 历史 CBM 数据未删除。

### 16.6 质量

- [x] 所有目标 crate 定向测试通过。
- [x] Clippy、格式、doctest 和 WebUI 测试通过。
- [x] 端到端与故障注入通过。
- [x] 性能基线有记录。
- [x] 连续五轮完整审核零新增问题。
- [x] 执行变更总结已追加。
- [x] 计划已迁移至 `docs/completed/20260904/01-VCE_FULL_NATIVE_INTEGRATION.md`。

## 17. 执行变更总结

### 17.1 核心修复与调整概述

1. 将仓外 MIT VCE 作为工作区 Rust crate 接入，保持其只负责代码语义领域逻辑，并通过 `VceAiPort`、`VceIndexPort`、`VceSourcePort` 接受宿主注入。
2. 在 Service Kernel 建立项目级 `VceRuntimeManager`、统一 HostAI 适配器、文件系统快照适配器、共享 watcher subscriber、串行刷新、取消、关闭、恢复和错误投影。
3. 在 Workspace Store 建立 VCE 专属 SQLite/LanceDB 数据面，实现 generation 暂存、精确成员 CAS 发布、幂等中止/恢复、符号查询、向量过滤、管理查询和迁移。
4. 将唯一 AI 工具 `codebase_retrieval` 接入 Native Tool Catalog、策略过滤、执行上下文和结果投影；项目、路径、namespace 与模型配置继续由宿主持有。
5. 增加 VCE Service API、RPC、Workbench 摘要和 WebUI 代码上下文管理面，覆盖设置、状态、统计、分页文件、Blob、作业、完整性、维护计划、重建、重试和取消。
6. 退出 CBM Controller、managed MCP、工具路由、页面入口与安装/同步/打包脚本；保留一次性设置迁移字段与历史磁盘数据。
7. 修复首次监听竞态、取消批次丢失、陈旧重试复活、检查点成员伪造、exact 查询范围截断及 WebUI revision 冲突恢复问题。
8. 完成真实 Controller、端到端、迁移矩阵、文件风暴、故障映射、全量回归、性能基线和连续五轮零问题审核。

### 17.2 文件变更清单

#### 新增

- `crates/vulcan-context-engine/`：完整 VCE 内核、文档、许可和黑盒测试。
- `crates/vulcan-service-kernel/src/vce/`：AI、源码、运行管理、服务与 watcher 适配。
- `crates/vulcan-service-kernel/src/rpc_dispatch/vce.rs`：VCE 独立 RPC 路由。
- `crates/vulcan-service-api/src/vce.rs`：VCE 线协议和管理投影。
- `crates/vulcan-workspace-store/src/vce_data_plane.rs`：VCE 专属 SQLite/LanceDB 数据面。
- `crates/vulcan-agent-runtime/src/tooling/native/codebase_retrieval.rs`：唯一 AI 可见原生工具。
- `crates/vulcan-agent-runtime/src/agent_runtime/execution_preparation/codebase_tool_routing_prompt.rs`：VCE 与 CodeKit/RG 的调用分工提示。
- `apps/vulcan-code-web/src/components/features/code-context/`、`apps/vulcan-code-web/src/service/vce-service.ts`、`apps/vulcan-code-web/src/types/vce.ts`：管理页面、服务和类型。
- `docs/reviews/20260904-vce-native-integration-review.md`：最终五轮审核与逐文件覆盖台账。

#### 修改

- 根工作区清单与锁文件、Service Kernel 启动/关闭/项目/Workbench/RPC 链、Workspace Store 项目 Schema 与迁移、Agent Runtime 工具链、WebUI 项目设置与消息、Make 和运行资源同步入口、开发环境及 CBM 退役说明。

#### 删除

- `crates/vulcan-service-kernel/src/codebase_memory_controller.rs`。
- `crates/vulcan-agent-runtime/src/mcp/managed_codebase_memory.rs`。
- `apps/vulcan-code-web/src/service/codebase-memory-service.ts`。
- `scripts/install_codebase_memory_runtime.ps1`、`scripts/package_codebase_memory_runtime.ps1`、`scripts/sync_codebase_memory_runtime.ps1`。

完整逐文件清单见 `docs/reviews/20260904-vce-native-integration-review.md`。

### 17.3 关键代码调整详情

- `VceRuntimeManager`：按项目创建、恢复、刷新、检索、取消和关闭实例；项目配置变化时原子替换独立 HostAI capability handle。
- `VulcanCodeVceAiPort`：使用统一 `HostAiPort` 契约，并对 Embedding、Responses、Rerank 请求、上下文和响应执行公共校验。
- `VulcanCodeVceSourcePort`：构建不可变 manifest，规范化项目相对路径，限制文件/批次大小并在读取后复验 Blob。
- `VulcanCodeVceDataPlane`：以 SQLite 为检查点权威，以 LanceDB 为向量存储；提交前校验完整成员转换、generation、chain、指纹、维度和 CAS 版本。
- `VceWatcherService`：先注册监听再首次扫描，按项目合并 dirty set，失败/取消只对仍受监听项目重排，移除项目不会被陈旧任务复活。
- `codebase_retrieval`：定义来自 `VceToolCatalog`，模型只提供自然语言需求；执行上下文注入当前项目与检索 handler。
- VCE RPC/WebUI：只读查询分页；修改命令带 revision/checkpoint 前置条件；设置冲突后重新加载权威项目概览。
- CBM 迁移：首次加载依据旧 `codebase_memory_enabled` 建立 VCE 设置，`migrated_from_codebase_memory` 防止重复覆盖。

### 17.4 遗留问题与注意事项

- 无阻塞性遗留问题。
- 真实生产 Provider 网络、配额、计费和模型质量未在本地凭据环境中验收；当前完成的是宿主契约、失败映射与确定性伪 Provider 验证。
- 缺少冻结的同项目、同模型 CBM 对照语料，因此不声明 VCE 与 CBM 的量化检索质量等价。
- 当前未确认权威的未保存编辑器事件源，本次接入保持 filesystem-only；不得用候选字段或多路 fallback 猜测接入。
- 历史 CBM 数据未自动删除；如后续提供 CBM 插件，必须使用独立身份、工具面和生命周期。
- Workspace Store 全目标历史测试仍有既有 Clippy 告警；VCE 发布路径所用 `--lib -D warnings` 已通过，详情见最终审核报告。
