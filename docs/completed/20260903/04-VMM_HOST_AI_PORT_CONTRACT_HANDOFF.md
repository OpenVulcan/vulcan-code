# VulcanCode 宿主 AI Port 契约交接说明

> 状态：公共契约、宿主适配、知识库迁移、Native VMM 对接与契约验证均已完成并冻结。

## 1. 文档目的

本文档是 `vulcan-code` 向外部 `vulcan-context-engine`（VCE）交接的宿主 AI 能力契约说明。
VCE 后续只能依赖本文档冻结的 Rust 契约，不依赖 `ConfiguredInferenceService`、Provider、
VLDB、LanceDB、SQLite、MCP、HTTP 或 VulcanCode 的内部启动结构。

本文档的范围是统一 LLM、Embedding、Rerank 的调用边界。它不替换知识库，也不规定 VCE 的
代码语义索引、分块、融合、覆盖率选择或存储实现。

## 2. 严格边界

### 2.1 本仓库负责

- 在 `crates/vulcan-ai-contract` 提供进程内、Rust 原生、Provider 无关的 `HostAiPort`。
- 在 `vulcan-service-kernel` 中以 `ConfiguredInferenceService` 作为当前真实执行上游，提供宿主适配器。
- 将 VMM-Core 的 `HostEffect::{Llm, Embedding, Rerank}` 映射到该 Port；Core 本身不创建 Provider。
- 将现有知识库的 LLM、Embedding、未来 Rerank 调用切换到该 Port。
- 保留知识库原有文档、chunk、generation、BM25、RRF、向量表、检索结果编排和业务投影。

### 2.2 本仓库不负责

- 不在本仓库实现 VCE/OCE。
- 不把知识库改造成 MCP，也不以 VCE 替换知识库。
- 不让 VCE、VMM-Core 或知识库自行创建 Provider、`ConfiguredInferenceService`、VLDB、LanceDB 或 SQLite。
- 不为外部 gRPC VMM 重新实现迁移、查询级别或服务端项目管理；外部模式沿用 VMM 服务本身的契约。

## 3. 契约所在位置与依赖方向

契约源文件：

`crates/vulcan-ai-contract/src/host.rs`

公开入口：

`crates/vulcan-ai-contract/src/lib.rs`

当前依赖只增加了 `async-trait` 和 `futures-core`。依赖方向冻结为：

```text
vulcan-ai-contract
        ↑
vulcan-service-kernel（唯一宿主适配实现，当前上游是 ConfiguredInferenceService）
        ↑
VulcanCode Composition Root / AgentRuntimeBridge
        ↑
VMM-Core、知识库、VCE 等内部 Rust 消费者
```

消费者只依赖 `vulcan-ai-contract` 的公开类型和由宿主发放的 handle；不得反向依赖
`vulcan-service-kernel`。

## 4. 当前源码已存在并直接复用的类型

以下类型已经存在于 `vulcan-ai-contract`，本次 Port 只将它们组合成统一调用边界，不重新定义同义字段：

- `InferenceCallerContext`：调用方权限、用途范围、预算、交互会话和调用方级 deadline。
- `InferenceCapabilitySnapshot`：Profile 当前可执行的 LLM、Embedding、Rerank 能力快照。
- `InferencePermission`：`Llm`、`Embedding`、`Rerank`、`Status`、`Cancel`。
- `InferenceUsageScope`：`InteractiveAgent`、`InternalBackground`、`ExternalService`。
- `ResponsesExecutionRequest` / `ResponsesExecutionResult`：受控 Responses LLM 请求和聚合结果。
- `EmbeddingRequest` / `EmbeddingResponse`：有序输入、结果、物理 Embedding 指纹和 usage。
- `RerankRequest` / `RerankResponse`：查询、候选文档、有序重排结果和 usage。
- `AiStreamEvent`：规范化的响应开始、文本、推理、工具、usage、完成、失败和取消事件。
- `ExecutionRouteSnapshot`：Provider、服务、连接、模型、协议、动作及版本的非敏感路由快照。
- `AiExecutionError`：认证、权限、额度、限流、过载、不可用、超时、取消等归一化错误。
- `AiOperation`：包括 `ConversationRespond`、`EmbeddingCreate`、`RerankDocuments`。
- `RequestUsage`：请求级 usage 记录。

这些类型是公共 AI 语义的来源。知识库自己的 prompt、结构化 Schema、Schema 修复策略、批处理、
索引 generation、BM25/RRF 和结果展示仍属于知识库私有逻辑。

## 5. 已冻结的 HostAiPort 公共契约

公开模块路径：`vulcan_ai_contract::host`

公开重导出：`vulcan_ai_contract::*`

### 5.1 取消源

```rust
pub trait HostAiCancellation: Send + Sync {
    fn is_cancelled(&self) -> bool;
    fn cancelled<'a>(&'a self)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}
```

该 trait 不绑定 Tokio。没有取消源时使用 `NoopHostAiCancellation`。宿主适配器必须在真实上游
调用、等待、流读取和取消路径上观察该信号，不能把取消只当作 UI 状态。

### 5.2 调用上下文

```rust
pub struct HostAiCallContext {
    pub operation_id: String,
    pub effect_id: Option<String>,
    pub command_id: Option<String>,
    pub deadline_unix_ms: Option<u64>,
    pub attempt: u32,
    pub retry_reason: Option<HostAiRetryReason>,
    pub cancellation: Arc<dyn HostAiCancellation>,
}
```

方法：

- `new(operation_id)`：创建第一次调用，`attempt = 1`，使用无操作取消源。
- `with_deadline(deadline_unix_ms)`：附加操作级绝对 Unix 毫秒 deadline。
- `with_cancellation(cancellation)`：附加宿主拥有的协作式取消源。
- `with_attempt(attempt, retry_reason)`：声明后续物理尝试及原因。
- `with_core_correlation(effect_id, command_id)`：附加 VMM-Core 关联标识。
- `effective_deadline(caller_deadline_unix_ms)`：取操作级和调用方级 deadline 的较早者。
- `validate(request_id)`：校验操作、请求、尝试和关联标识不为空。

`operation_id` 在同一逻辑操作的重试之间保持不变；`request_id` 标识一次具体请求；
`attempt` 从 1 开始递增。重试不能通过生成一个新的 `operation_id` 隐藏在 usage 或诊断之外。

`InferenceCallerBudget.max_execution_duration_ms` 冻结为“每次物理 AI 分发的最大执行时长”，由宿主
在 LLM、Embedding、Rerank 每次开始执行时转换为 `当前时间 + 最大时长` 的绝对 deadline，再与
`HostAiCallContext.deadline_unix_ms`、调用方显式绝对 deadline 取最早值。长生命周期 capability
注册不得把一次注册时刻生成的绝对 deadline 当作后续所有批次的共同截止时间，否则批处理会在句柄
存活期间永久过期。

### 5.3 重试原因

```rust
pub enum HostAiRetryReason {
    ProviderTransient,
    SchemaRepair,
    ConsumerRequested,
}
```

重试是否允许由宿主最终决定。`SchemaRepair` 只描述知识库已有的结构化输出修复行为，不能由 Port
替换知识库的 Schema 或 prompt。

### 5.4 宿主错误

```rust
pub enum HostAiErrorCode {
    Contract,
    Authentication,
    PermissionDenied,
    PaymentRequired,
    RateLimited,
    ProviderOverloaded,
    ProviderUnavailable,
    Configuration,
    CapabilityUnavailable,
    LimitExceeded,
    InvalidRequest,
    DimensionMismatch,
    FingerprintMismatch,
    Timeout,
    Cancelled,
    OperationNotFound,
    Provider,
    Internal,
}

pub struct HostAiError {
    pub code: HostAiErrorCode,
    pub message: String,
    pub request_id: Option<String>,
    pub operation_id: Option<String>,
    pub effect_id: Option<String>,
    pub command_id: Option<String>,
    pub attempt: u32,
    pub retryable: bool,
    pub dispatch_outcome: DispatchOutcome,
    pub retry_after: Option<RetryAfterHint>,
    pub route: Option<Box<ExecutionRouteSnapshot>>,
    pub provider_error: Option<Box<AiExecutionError>>,
}
```

错误必须显式携带能够获取到的 request、operation、effect、command、attempt、dispatch、route 和
Provider 归一化信息。`route` 与 `provider_error` 采用 `Box` 只用于限制 Rust `Result` 的错误体尺寸；
Serde JSON 形状保持不变，调用方通过 `as_deref()` 读取内部值。宿主适配器不能用“尝试另一个字段”
或静默降级掩盖未确认的错误归属。

### 5.5 能力注册和 handle

```rust
pub struct HostAiCapabilityRequirement {
    pub operation: AiOperation,
    pub purpose_id: Option<String>,
    pub required: bool,
}

pub struct HostAiCapabilityRegistration {
    pub registration_id: String,
    pub owner_id: String,
    pub consumer_profile_id: String,
    pub caller: InferenceCallerContext,
    pub requirements: Vec<HostAiCapabilityRequirement>,
}

pub struct HostAiCapabilityHandle {
    pub registration_id: String,
    pub owner_id: String,
    pub consumer_profile_id: String,
    pub caller: InferenceCallerContext,
    pub snapshot: InferenceCapabilitySnapshot,
}
```

`HostAiPort::register_capability` 是组合根控制的注册入口。注册时校验 Profile、caller 权限和
`required` 能力；调用方不能伪造或修改宿主发放的 handle。`HostAiCapabilityRegistration` 是进程内
注册描述，不是跨进程传输 DTO，因此不假定 `InferenceCallerContext` 可序列化。

进程内 Rust 消费者的 `InferenceCallerContext.usage_scope` 冻结为 `InternalBackground`，持久化与
WebUI 候选路由的序列化值为 `internal_background`。`external_service` 是旧外置 sidecar 推理授权语义，
不适用于 Native VMM 或 VCE；VMM 设置页也只能从 `internal_background` 候选中选择路由。

Profile、凭据或模型配置变化后，消费者必须调用 `HostAiPort::capability_snapshot` 刷新能力事实，
重新校验全部 `required` 项，并用新快照更新自己持有的 handle。宿主注册表内部刷新不会反向修改
调用方已经克隆的旧 handle，因此禁止长期复用首次注册时捕获的 `snapshot`。

### 5.6 Embedding 的逻辑空间封装

```rust
pub struct HostAiEmbeddingRequest {
    pub vector_space_id: String,
    pub request: EmbeddingRequest,
}

pub struct HostAiEmbeddingResponse {
    pub vector_space_id: String,
    pub response: EmbeddingResponse,
}
```

`vector_space_id` 是消费者/存储所有者选择的稳定逻辑空间，不等于 Provider、模型、维度或 LanceDB
表名。实际兼容性仍由 `EmbeddingResponse.fingerprint` 判定；fingerprint 变化必须由存储所有者
按自己的索引代次策略处理。

Embedding 语义冻结如下：

- `EmbeddingRequest.inputs` 的顺序是权威顺序。
- 响应结果必须按请求输入顺序返回，并保留请求 ID/索引关联。
- `dimensions`、`output_kind`、`encoding`、任务类型和 fingerprint 由现有请求/响应契约决定。
- 批处理大小由消费者的业务编排决定；Port 只执行一个类型化批次，不重写批次顺序。
- Port 不写 VLDB/LanceDB，不创建向量表，也不重建知识库索引。
- 本仓库知识库使用稳定的 `knowledge_base:{library_id}` 作为逻辑 `vector_space_id`；generation
  仍由知识库自己的向量表/索引流程管理。

### 5.7 Rerank 响应身份与排序校验

`RerankResponse::validate_for_request` 是公共响应门禁，宿主适配器、`ConfiguredInferenceService`
和 `MockHostAiPort` 必须使用同一语义。冻结规则如下：

- `response.request_id` 必须与原请求完全一致。
- 返回数量不得超过 `request.top_n`；允许供应商返回少于 `top_n` 的结果。
- 结果数组顺序是权威排名顺序，每项 `rank` 必须从 `0` 开始并与数组位置一致。
- 每项 `original_index` 必须处于候选范围内且不得重复；`candidate_id` 必须与该下标处的原始候选
  ID 完全一致，禁止按文本或模糊名称重新匹配。
- 每项分数必须为有限值；当 fingerprint 声明“高分更相关”或“低分更相关”时，结果分数必须按
  对应方向单调排列；`Unknown` 不假定分数方向。
- `return_content=false` 时响应不得附带候选正文；正文是否完整不参与候选身份判断。
- 任一门禁失败都归类为供应商响应执行错误，不能作为成功 usage/结果继续传给消费者。

### 5.8 LLM 异步执行

```rust
pub enum HostAiExecutionPhase {
    Running,
    Completed,
    Failed,
    Cancelled,
}

pub struct HostAiExecutionSnapshot {
    pub execution_id: String,
    pub operation_id: String,
    pub request_id: String,
    pub phase: HostAiExecutionPhase,
    pub route: Option<ExecutionRouteSnapshot>,
    pub error: Option<HostAiError>,
}

pub type HostAiEventStream =
    Pin<Box<dyn Stream<Item = Result<AiStreamEvent, HostAiError>> + Send + 'static>>;

pub struct HostAiExecutionHandle {
    pub operation_id: String,
    pub effect_id: Option<String>,
    pub command_id: Option<String>,
    pub execution_id: String,
    pub request_id: String,
    pub caller_id: String,
    pub consumer_profile_id: String,
    pub attempt: u32,
}
```

`HostAiExecutionHandle` 还保留宿主取消源，并通过 `cancellation()` 提供给适配器内部使用。
执行流必须支持 replay + live 语义；`stream_response` 返回历史事件和后续实时事件，
`wait_response` 返回聚合 `ResponsesExecutionResult`，`execution_status` 返回状态快照，
`cancel_execution` 发出协作式取消请求。

结构化输出必须同时满足两层约束：消费者提交并由宿主终结器校验准确 JSON Schema；对于只提供
JSON Object 开关的协议，`ConfiguredInferenceService` 可将线路请求投影为 `JsonObject`，但不能
降低最终宿主 Schema 校验。Provider 的全部 Manifest/自定义投影结束后，执行 配置、Responses
`text.format`、Gemini MIME、Interactions/Cohere response format 等协议原生 JSON 控制必须仍然
存在；任何后置投影删除或改变该控制都必须在网络分派前失败，不能以普通文本请求继续发送。

### 5.9 Port 方法全集

```rust
pub trait HostAiPort: Send + Sync {
    fn register_capability(
        &self,
        registration: HostAiCapabilityRegistration,
    ) -> Result<HostAiCapabilityHandle, HostAiError>;

    fn capability_handle(
        &self,
        registration_id: &str,
    ) -> Result<HostAiCapabilityHandle, HostAiError>;

    fn unregister_capability(
        &self,
        registration_id: &str,
    ) -> Result<(), HostAiError>;

    fn capability_snapshot(
        &self,
        handle: &HostAiCapabilityHandle,
    ) -> Result<InferenceCapabilitySnapshot, HostAiError>;

    async fn create_embeddings(
        &self,
        handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: HostAiEmbeddingRequest,
    ) -> Result<HostAiEmbeddingResponse, HostAiError>;

    async fn rerank_documents(
        &self,
        handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: RerankRequest,
    ) -> Result<RerankResponse, HostAiError>;

    async fn start_response_execution(
        &self,
        handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: ResponsesExecutionRequest,
    ) -> Result<HostAiExecutionHandle, HostAiError>;

    fn execution_status(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<HostAiExecutionSnapshot, HostAiError>;

    fn stream_response(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<HostAiEventStream, HostAiError>;

    async fn wait_response(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<ResponsesExecutionResult, HostAiError>;

    fn cancel_execution(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<(), HostAiError>;
}
```

Port 是进程内 Rust trait，不是 gRPC/HTTP 协议。`HostAiPort: Send + Sync`，异步方法使用
`async_trait`，事件流为 `Send + 'static`，以支持 Tokio 任务、后台索引和 VMM-Core 的异步宿主执行。

## 6. 统一入口负责什么，消费者负责什么

### 6.1 宿主 Port/适配器负责

- Profile 和 route 解析、caller 授权、能力检查。
- Provider 实际执行、统一超时/deadline、取消、并发/输入/输出限额。
- 宿主规定的重试许可和 attempt/usage/诊断归属。
- 统一错误映射、路由快照和 Provider 错误保留。
- LLM execution handle 的所有权校验、状态、流、等待和取消。
- Embedding/Rerank 的通用请求校验和响应身份校验。

### 6.2 知识库/VMM-Core/VCE 各自负责

- 选择自己的 `owner_id`、`registration_id`、consumer profile 和 purpose。
- 业务 prompt、Schema、Schema 修复重试和输出投影。
- Embedding 输入的业务分块、批次边界、顺序和向量写入。
- VMM-Core 的 command/effect 编排和存储 effect；VCE 的代码语义 pipeline。
- 使用 `vector_space_id` 选择逻辑空间，但不把它解释成具体 Provider 或存储表。

## 7. 知识库迁移的精确边界

迁移前知识库源码有以下三个直接获取 `Arc<ConfiguredInferenceService>` 的调用层；当前实现已经把
这些调用全部切换为共享 `HostAiPort` 与 capability handle：

- `knowledge_display_index`：构造知识展示请求，并通过 start/status/stream/wait/cancel 完成 LLM 任务。
- `knowledge_vector_index`：按知识库已有批次创建 EmbeddingRequest，并将结果写入现有知识库向量表。
- `service_search_one_knowledge_library`：创建查询向量并调用现有 display hybrid 检索。

迁移后的调用链为：

```text
知识库 prompt/schema/批次/检索编排
        ↓
HostAiCapabilityHandle + HostAiCallContext
        ↓
HostAiPort
        ↓
ConfiguredInferenceHostAiPort（service-kernel）
        ↓
ConfiguredInferenceService（当前真实执行上游）
        ↓
已有 Provider/PublicAiRuntime
```

必须保留的知识库逻辑：

- `knowledge_display_request` 的 prompt、结构化 Schema、文档上下文和 Schema 修复策略。
- `KnowledgeOperationReporter` 的进度、usage、输出预览、取消投影和持久化。
- `run_knowledge_vector_index` 的文档 chunk、批量顺序、向量 fingerprint/维度检查、generation
  管理和 `runtime.write_vector_rows`。
- `service_search_one_knowledge_library` 的 raw/display 模式、向量就绪检查、混合检索、BM25/RRF
  和结果整形。

只替换的内容：

- 直接持有或调用 `ConfiguredInferenceService` 的 AI 执行入口。
- 直接传入 caller/profile 的 service 调用方式，改为注册能力后使用 handle/context。
- service 错误向知识库错误的边界映射。

当前源码中知识库 Profile 同步已支持 LLM 和 Embedding；`Rerank` 在公共 Port 中预留，但当前
知识库同步函数尚未声明知识库 Rerank route，因此不能假定知识库已有 Rerank 业务调用。

## 8. VMM-Core 对接边界

上游 `vmm-core-contracts` 已存在 `HostEffect::{Llm, Embedding, Rerank, StorageQuery,
StorageMutation}`。内核直接 Rust 集成时：

- Core 通过 `HostAiPort` 触发 LLM/Embedding/Rerank，不暴露给 VulcanCode 的外部接口。
- `effect_id`、`command_id`、deadline、attempt 和 cancellation 进入 `HostAiCallContext`。
- Core 的 effect/command 生命周期由 Core 自己维护；AI Provider、usage、route 和权限由宿主维护。
- Storage effect 仍走宿主提供的 storage port；本契约不把 VLDB 写入混入 AI Port。
- VMM 的写入内容与查询必须处于同一个对称向量空间，因此 VMM Embedding 固定使用
  `EmbeddingTask::ProviderDefault`、`EmbeddingOutputKind::Dense`、`EmbeddingEncoding::Float` 和路由声明的
  确定默认维度；不得改成 `SemanticSimilarity`，也不得借用 VCE 的 `Document`/`Query` 非对称任务语义。
- VMM 启用与能力刷新阶段必须验证上述完整请求形态；不能只验证存在 `EmbeddingCreate` 操作后等到首次
  搜索或写入才暴露模型路由不兼容。
- Core 已复制为工作区内独立的 contracts/config/engine/storage/core crate；具体 HostEffect、
  StorageQuery、StorageMutation 与项目级数据表均以这些本地源码及
  `vulcan-service-kernel::native_vmm`、`vulcan-workspace-store::vmm_data_plane` 为权威。

## 9. 当前源码已实现 / 可合理抽取 / 尚未定义

### 9.1 当前源码已实现

- `vulcan-ai-contract` 已有请求、能力、事件、usage、路由、错误和操作类型。
- `vulcan-service-kernel::ConfiguredInferenceService` 已实现能力快照、Embedding、Rerank、
  LLM 异步执行、状态、replay + live stream、聚合等待和取消。
- 知识库已有 LLM 展示索引、Embedding 索引、查询向量、profile 同步和知识库错误投影。
- `vulcan-ai-contract::host` 已提供本说明中的 Port 类型定义，且 `cargo check -p vulcan-ai-contract`
  已通过。
- `ConfiguredInferenceHostAiPort` 已作为 `ConfiguredInferenceService` 的私有宿主适配器实现，
  `AgentRuntimeBridge` 已持有 `Arc<dyn HostAiPort>`。
- Native VMM 已使用稳定注册 ID `vmm-native:runtime` 和 consumer profile `vmm` 注册四项必需 LLM、
  一项必需 Embedding 及可选画像指令/Rerank，并在启用和调用前刷新能力快照；Embedding 激活门禁
  同时验证 `ProviderDefault + Dense + Float + 确定维度`。
- `RerankResponse::validate_for_request` 已统一校验请求身份、候选 ID/下标、连续排名、结果上限、
  有限分数、声明的分数方向和 `return_content` 约束；配置推理、宿主 Port 与 mock 共用该门禁。

### 9.2 已由现有模式完成抽取

- 知识库使用稳定注册 ID 注册 LLM/Embedding capability，并按运行时 Profile 刷新能力快照。
- VMM-Core 的 HostEffect 到 `HostAiCallContext`/Port 调用已在 Native 驱动中完成映射。
- 合约 crate 的 `test-support` feature 和 `MockHostAiPort` 已供 VCE 与契约测试做纯 Rust 验证。

### 9.3 尚未定义，不能假定

- VCE 自己的 owner/profile/purpose/registration 命名规则。
- VCE 的 logical `vector_space_id` 命名和向量存储 schema。
- Provider 具体错误是否允许重试、重试次数和退避；这些由宿主策略与最终实现决定。
- 知识库是否新增 Rerank 业务步骤；当前源码没有该调用链，不能因 Port 支持 Rerank 就自动启用。
- 外部 gRPC VMM 的服务端迁移结果、配置查询级别和项目管理细节。

## 10. 契约变更通知规则

本文档和 `host.rs` 是同一版本的契约来源。后续若发生以下任一变化，必须先更新本文档，再向
VCE 线程发送变更消息：

- 增删字段、改变字段含义、修改 ID/attempt/deadline/cancellation 语义。
- 修改 `HostAiPort` 方法签名、异步/Send/Sync 约束、流或等待语义。
- 修改 Embedding 顺序、维度、fingerprint 或 `vector_space_id` 语义。
- 修改错误码、retryable、dispatch、usage 或诊断保留规则。
- 将知识库的私有编排错误地提升为公共 Port 语义，或反向将公共宿主策略下沉到知识库。

通知必须包含：变更文件、旧语义、新语义、影响的消费者、验证命令和未解决问题。没有源码和
测试证据的内容只能放在“尚未定义，不能假定”中。

## 11. 下一阶段执行顺序

1. 在 service-kernel 实现 `ConfiguredInferenceHostAiPort`，并把它注入 `AgentRuntimeBridge`。
2. 迁移知识库三条 AI 调用链，保持知识库存储和检索编排不变。
3. 增加 mock/test adapter 和契约测试，覆盖能力注册、权限、取消、deadline、重试元数据、流、
   Embedding 顺序及 fingerprint/维度校验。
4. 复制并清理 VMM-Core 到 VulcanCode 的 native 模块，映射 HostEffect；不实现 VCE。
5. 完成整体 VMM 对接后，执行逐轮源码审查、测试和修复；连续五轮审查均无新问题后结束目标。

## 12. 执行变更总结

### 12.1 核心修复与调整概述

公共契约已落在 `vulcan-ai-contract::host`，并继续复用该 crate 既有的 Responses、Embedding、
Rerank、能力、路由、usage 与错误类型。`ConfiguredInferenceHostAiPort` 是 service-kernel 私有适配器，
它调用既有 `ConfiguredInferenceService` 实际执行 Provider 请求；Composition Root 只创建一个共享
`Arc<dyn HostAiPort>` 并注入内部消费者。知识库没有被替换，仅将原来直连
`ConfiguredInferenceService` 的 LLM/Embedding 执行点改为 capability handle + call context + Port。

Native VMM 已用相同规则注册 `vmm` consumer profile，把 Core 的 LLM、Embedding 和 Rerank Effect
映射到公共契约。VCE 由并行任务实现自己的领域层，本任务只提供并验证公共 HostAI 契约；契约说明
已经发送到指定 VCE 任务，后续修复没有改变公共字段或调用语义，因此无需发布破坏性变更通知。

### 12.2 文件变更清单

- 新增：`crates/vulcan-ai-contract/src/host.rs`、`test_support.rs`，
  `crates/vulcan-service-kernel/src/internal_ai.rs`，Native VMM 宿主驱动与对应契约测试。
- 修改：`vulcan-ai-contract` 请求/响应校验与重导出；service-kernel Composition Root、
  `AgentRuntimeBridge`、`ConfiguredInferenceService` 和 `knowledge_base_services.rs`；Native VMM
  capability 注册及 Effect 映射；相关 Cargo feature/dependency 与测试。
- 删除：service-api/service-kernel 中绕开公共 Port 的旧 inference/response 包装和旧访问层；
  ExternalGrpc VMM 的协议与客户端未因此删除或改成 HostAI 实现。

### 12.3 关键代码调整详情

- capability 注册冻结 `registration_id`、`owner_id`、`consumer_profile_id`、调用方权限、用途要求和
  `InternalBackground` 用量作用域；宿主拒绝伪造、跨 owner/profile 或未授权 handle。
- `HostAiCallContext` 冻结逻辑操作 ID、物理请求 ID、attempt、重试原因、Core 关联、绝对 deadline 和
  协作式取消；`InferenceCallerBudget.max_execution_duration_ms` 在每次真实分发时转换为绝对截止时间，
  再取全部截止时间中的最早值。
- LLM 使用宿主持有的 execution handle，支持状态、replay+live stream、聚合等待与取消；Embedding
  冻结输入顺序、逻辑 `vector_space_id`、维度/编码/指纹；Rerank 冻结候选 ID、原下标、连续 rank、
  数量、正文返回和分数方向门禁。
- `HostAiError` 保留请求/操作/Core 关联、attempt、dispatch、retry、route 与归一化 Provider 错误；
  `route`、`provider_error` 的 Box 只改变 Rust 内存布局，不改变 Serde JSON。
- 知识库保留 prompt、Schema 修复、批次、generation、VLDB/LanceDB 写入和混合检索。持久操作取消
  通过私有 `KnowledgeHostAiCancellation` 投影到每次 LLM 启动/流/等待和每个 Embedding 批次；该修复
  复用了既有公共取消接口，没有扩展公共契约。
- Native VMM 的必需 LLM/Embedding 与可选画像指令/Rerank 在启用和调用前刷新；Effect 失败显式
  转换为 Core 可降级失败，Storage Effect 独立走宿主 VLDB，不混入 AI Port。VMM Embedding 固定为
  对称 `ProviderDefault` 任务并在激活阶段校验完整请求形态；VCE 的 `Document`/`Query` 任务保持独立。
- LLM 结构化输出保留“调用方 Schema + 提供方 JSON 模式 + 宿主最终校验”三层边界；最终线路编码器
  会检查协议原生 JSON 控制仍存在，避免 Manifest 或模型投影静默退化为普通文本请求。

### 12.4 遗留问题与注意事项

- 知识库当前没有 Rerank 业务步骤；公共 Port 的 Rerank 能力只供已声明的消费者使用。
- VCE 的最终宿主注册 identity、逻辑向量空间命名和存储 schema 仍由 VCE 集成层决定，公共 Port 不
  替它创建 LLM、Embedding、Rerank 或向量存储实例。
- 外部 gRPC VMM 不是 HostAI 消费者，其 Provider、查询级别和迁移行为不纳入本契约。
- 验证结果：HostAI+VCE 全特性 225 项测试通过；service-kernel 的 internal_ai、Native driver 和
  knowledge 定向测试分别 2、3、16 项通过；HostAI/VCE 全目标及 service-kernel 生产库严格 Clippy
  均为零告警；最终 14 个相关 Rust crate 共 48 个套件 2280 项通过，8 项既有忽略。
- 最终累计审核覆盖起始基线、全部中间本地提交和归档文档；F-036 EOF 空行修复后重新连续五轮没有发现新问题。
