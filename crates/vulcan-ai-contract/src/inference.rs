//! Restricted provider-neutral contracts for configured internal inference.
//! 面向已配置内部推理的受限供应商无关契约。

use crate::{
    AiMessage, AiOperation, ContentItem, ModelParameterValue, NativeToolKind,
    ParallelToolCallPolicy, PromptCachePolicy, ReasoningEffort, ReasoningOutputPolicy,
    RequestUsage, ResourceKind, ResourceReference, StructuredOutputPolicy, ToolChoicePolicy,
    ToolDefinition,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Identifies the only inference contract version accepted by this release.
/// 标识当前版本唯一接受的推理契约版本。
pub const INFERENCE_CONTRACT_VERSION: u32 = 1;

/// Identifies one permission granted to an inference caller.
/// 标识授予推理调用方的一项权限。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InferencePermission {
    /// Permits restricted LLM inference.
    /// 允许受限 LLM 推理。
    Llm,
    /// Permits embedding creation.
    /// 允许创建向量。
    Embedding,
    /// Permits document reranking.
    /// 允许文档重排。
    Rerank,
    /// Permits decision evaluation.
    /// 允许决策评估。
    Decision,
    /// Permits status and capability inspection.
    /// 允许查询状态与能力。
    Status,
    /// Permits cancellation of caller-owned executions.
    /// 允许取消调用方拥有的执行。
    Cancel,
}

/// Identifies the real usage context of one inference execution.
/// 标识一次推理执行的真实使用场景。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceUsageScope {
    /// User-driven interactive agent work.
    /// 用户驱动的交互式智能体工作。
    InteractiveAgent,
    /// Vulcan-owned background work.
    /// Vulcan 自有后台工作。
    InternalBackground,
    /// A separately authenticated local service.
    /// 独立鉴权的本地服务。
    ExternalService,
}

/// Selects scheduling independently of model-route authorization; only the host may grant this class.
/// 独立于模型路由授权选择调度类别；只有宿主可以授予此类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InferenceSchedulingClass {
    /// Preserve the existing queue selected by the authorized usage scope.
    /// 保留由授权用途范围选择的既有队列。
    #[default]
    ByUsageScope,
    /// Run a host-owned internal query in foreground capacity without changing its route authorization.
    /// 将宿主持有的内部查询放入前台容量，不改变其路由授权。
    InternalQuery,
}

/// Defines hard limits assigned by the host to one caller.
/// 定义宿主分配给一个调用方的硬限制。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceCallerBudget {
    /// Maximum concurrent executions owned by the caller.
    /// 调用方可拥有的最大并发执行数。
    pub max_concurrent_executions: u32,
    /// Optional maximum input units for one request.
    /// 单次请求可选的最大输入单位数。
    pub max_input_units_per_request: Option<u64>,
    /// Optional maximum output tokens for one LLM request.
    /// 单次 LLM 请求可选的最大输出 Token 数。
    pub max_output_tokens_per_request: Option<u64>,
    /// Optional maximum accepted requests in one rolling minute.
    /// 可选的每滚动分钟最大受理请求数。
    pub max_requests_per_minute: Option<u32>,
    /// Optional maximum aggregate input units in one rolling minute.
    /// 可选的每滚动分钟累计输入单位上限。
    pub max_input_units_per_minute: Option<u64>,
    /// Optional maximum wall-clock duration for one execution.
    /// 单次执行可选的最大墙钟时长。
    pub max_execution_duration_ms: Option<u64>,
}

/// Carries trusted caller identity injected by the host.
/// 承载由宿主注入的可信调用方身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceCallerContext {
    /// Stable host-issued caller identifier.
    /// 宿主签发的稳定调用方标识。
    pub caller_id: String,
    /// Consumer profiles this caller may use.
    /// 此调用方获准使用的消费者配置档。
    pub allowed_consumer_profile_ids: BTreeSet<String>,
    /// Fine-grained operation permissions.
    /// 细粒度操作权限。
    pub permissions: BTreeSet<InferencePermission>,
    /// Real execution usage context.
    /// 真实执行使用场景。
    pub usage_scope: InferenceUsageScope,
    /// Immutable host-issued scheduling authority, never a model or external request parameter.
    /// 不可变的宿主签发调度权限，绝非模型或外部请求参数。
    pub scheduling_class: InferenceSchedulingClass,
    /// Optional interactive session identity proven by the host.
    /// 由宿主证明的可选交互会话身份。
    pub interactive_session_id: Option<String>,
    /// Optional conversation session that owns this background call, proven by the host.
    /// 由宿主证明、持有本次后台调用的可选会话标识。
    pub owner_session_id: Option<String>,
    /// Optional absolute Unix deadline in milliseconds.
    /// 可选的 Unix 毫秒绝对截止时间。
    pub deadline_unix_ms: Option<u64>,
    /// Host-owned caller budget.
    /// 宿主管理的调用方预算。
    pub budget: InferenceCallerBudget,
}

impl InferenceCallerContext {
    /// Authorizes one consumer profile and operation using only host-injected identity.
    /// 仅使用宿主注入的身份授权一个消费者配置档与操作。
    ///
    /// # Parameters
    /// 参数
    /// `consumer_profile_id` and `permission` identify the requested configured operation.
    /// `consumer_profile_id` 与 `permission` 标识所请求的已配置操作。
    ///
    /// # Returns
    /// 返回值
    /// Success when the caller owns the profile and permission with a valid usage context.
    /// 调用方拥有配置档、权限及有效使用上下文时返回成功。
    pub fn authorize(
        &self,
        consumer_profile_id: &str,
        permission: InferencePermission,
    ) -> Result<(), String> {
        if self.scheduling_class == InferenceSchedulingClass::InternalQuery
            && self.usage_scope != InferenceUsageScope::InternalBackground
        {
            return Err(String::from(
                "Internal query scheduling requires a host-owned internal usage scope.",
            ));
        }
        if self.caller_id.trim().is_empty() {
            return Err(String::from("Inference caller identity must not be empty."));
        }
        if !self
            .allowed_consumer_profile_ids
            .contains(consumer_profile_id)
        {
            return Err(format!(
                "Inference caller `{}` is not authorized for consumer profile `{consumer_profile_id}`.",
                self.caller_id
            ));
        }
        if !self.permissions.contains(&permission) {
            return Err(format!(
                "Inference caller `{}` does not have the required operation permission.",
                self.caller_id
            ));
        }
        if self.usage_scope == InferenceUsageScope::InteractiveAgent
            && self
                .interactive_session_id
                .as_deref()
                .is_none_or(|session_id| session_id.trim().is_empty())
        {
            return Err(String::from(
                "Interactive inference requires a host-proven session identity.",
            ));
        }
        if self
            .owner_session_id
            .as_deref()
            .is_some_and(|session_id| session_id.trim().is_empty())
        {
            return Err(String::from(
                "Inference owner session identity must not be empty when present.",
            ));
        }
        if self.budget.max_concurrent_executions == 0 {
            return Err(String::from(
                "Inference caller concurrency budget must be greater than zero.",
            ));
        }
        if self.budget.max_requests_per_minute == Some(0) {
            return Err(String::from(
                "Inference caller request-rate budget must be greater than zero when configured.",
            ));
        }
        if self.budget.max_input_units_per_minute == Some(0) {
            return Err(String::from(
                "Inference caller input-rate budget must be greater than zero when configured.",
            ));
        }
        if self.budget.max_execution_duration_ms == Some(0) {
            return Err(String::from(
                "Inference caller execution-duration budget must be greater than zero when configured.",
            ));
        }
        Ok(())
    }
}

/// Carries host-owned routing metadata around one provider-neutral Responses execution.
/// 承载一次供应商无关 Responses 执行外围的宿主路由元数据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponsesExecutionRequest {
    /// Exact contract version.
    /// 精确契约版本。
    pub contract_version: u32,
    /// Caller-generated stable request identifier.
    /// 调用方生成的稳定请求标识。
    pub request_id: String,
    /// Optional idempotency key for in-flight coalescing only.
    /// 仅用于运行中合并的可选幂等键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    /// Authorized consumer profile identifier.
    /// 已授权的消费者配置档标识。
    pub consumer_profile_id: String,
    /// Consumer-profile-scoped business purpose identifier.
    /// 消费者配置档作用域内的业务用途标识。
    pub purpose_id: String,
    /// Ordered provider-neutral Responses messages.
    /// 有序的供应商无关 Responses 消息。
    pub messages: Vec<AiMessage>,
    /// Standard function tools visible to the selected model.
    /// 对所选模型可见的标准函数工具。
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
    /// Provider-native tools selected through standard Responses tool types.
    /// 通过标准 Responses 工具类型选择的供应商原生工具。
    #[serde(default)]
    pub native_tools: Vec<NativeToolKind>,
    /// Standard tool invocation policy.
    /// 标准工具调用策略。
    #[serde(default)]
    pub tool_choice: ToolChoicePolicy,
    /// Standard parallel tool-call policy.
    /// 标准并行工具调用策略。
    #[serde(default)]
    pub parallel_tool_calls: ParallelToolCallPolicy,
    /// Structured output constraint.
    /// 结构化输出约束。
    #[serde(default)]
    pub structured_output: StructuredOutputPolicy,
    /// Prompt-cache intent.
    /// 提示词缓存意图。
    #[serde(default)]
    pub prompt_cache: PromptCachePolicy,
    /// Optional caller-selected cache key, namespaced by the trusted host caller before dispatch.
    /// 可选的调用方缓存键，分派前由可信宿主调用方命名空间隔离。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    /// Whether the public response may expose opaque reasoning continuation payloads.
    /// 公共响应是否可以暴露不透明推理延续载荷。
    #[serde(default)]
    pub include_reasoning_encrypted_content: bool,
    /// Optional exact reasoning effort.
    /// 可选的精确推理强度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Public reasoning output policy.
    /// 公共推理输出策略。
    #[serde(default)]
    pub reasoning_output: ReasoningOutputPolicy,
    /// Capability-registered model parameters.
    /// 能力契约已注册的模型参数。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_parameters: BTreeMap<String, ModelParameterValue>,
    /// Ordered stop sequences retained by the shared Responses execution plane.
    /// 由共享 Responses 执行平面保留的有序停止序列。
    #[serde(default)]
    pub stop_sequences: Vec<String>,
    /// Input resources referenced by message content.
    /// 消息内容引用的输入资源。
    #[serde(default)]
    pub resources: Vec<ResourceReference>,
    /// Optional caller-narrowed output limit.
    /// 可选的调用方收窄输出上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_token_limit: Option<u64>,
}

impl ResponsesExecutionRequest {
    /// Validates the closed restricted LLM request before route resolution.
    /// 在解析路由前校验封闭的受限 LLM 请求。
    ///
    /// # Returns
    /// 返回值
    /// Success for one exact versioned request, or a deterministic contract error.
    /// 精确版本请求有效时返回成功，否则返回确定性契约错误。
    pub fn validate(&self) -> Result<(), String> {
        validate_contract_identity(
            self.contract_version,
            self.request_id.as_str(),
            self.consumer_profile_id.as_str(),
        )?;
        validate_optional_idempotency_key(self.idempotency_key.as_deref())?;
        if self
            .prompt_cache_key
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(String::from(
                "Responses prompt_cache_key must not be empty when provided.",
            ));
        }
        if self.purpose_id.trim().is_empty() {
            return Err(String::from("LLM inference purpose_id must not be empty."));
        }
        if self.messages.is_empty() {
            return Err(String::from(
                "LLM inference must contain at least one message.",
            ));
        }
        for (message_index, message) in self.messages.iter().enumerate() {
            if message.content.is_empty() {
                return Err(format!(
                    "LLM inference message {message_index} must contain at least one content item."
                ));
            }
            for content in &message.content {
                if let ContentItem::Text { text } = content
                    && text.is_empty()
                {
                    return Err(format!(
                        "LLM inference message {message_index} contains empty text."
                    ));
                }
            }
        }
        if self.output_token_limit == Some(0) {
            return Err(String::from(
                "LLM inference output_token_limit must be greater than zero.",
            ));
        }
        Ok(())
    }
}

/// Identifies one embedding task without provider-specific labels.
/// 标识一项不含供应商特定标签的向量任务。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingTask {
    /// Uses the exact model contract default.
    /// 使用精确模型契约默认值。
    ProviderDefault,
    /// Embeds a retrieval query.
    /// 对检索查询生成向量。
    Query,
    /// Embeds a retrievable document.
    /// 对可检索文档生成向量。
    Document,
    /// Embeds semantic-similarity input.
    /// 对语义相似度输入生成向量。
    SemanticSimilarity,
    /// Embeds classification input.
    /// 对分类输入生成向量。
    Classification,
    /// Embeds clustering input.
    /// 对聚类输入生成向量。
    Clustering,
    /// Embeds code-retrieval input.
    /// 对代码检索输入生成向量。
    CodeRetrieval,
}

/// Identifies the semantic shape of one embedding output.
/// 标识一项向量输出的语义形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingOutputKind {
    /// One dense vector.
    /// 一个稠密向量。
    Dense,
    /// One sparse vector.
    /// 一个稀疏向量。
    Sparse,
    /// Multiple ordered dense vectors.
    /// 多个有序稠密向量。
    MultiVector,
}

/// Identifies the wire encoding of embedding values.
/// 标识向量值的 Wire 编码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingEncoding {
    /// JSON floating-point values.
    /// JSON 浮点数值。
    Float,
    /// Provider-declared Base64 representation.
    /// 供应商声明的 Base64 表示。
    Base64,
}

/// Represents one embedding input content value.
/// 表示一项向量输入内容值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EmbeddingInputContent {
    /// Plain text input.
    /// 纯文本输入。
    Text {
        /// Exact text payload.
        /// 精确文本载荷。
        text: String,
    },
    /// Vulcan-side resource input reserved for capability-gated internal callers.
    /// 为受能力约束内部调用方保留的 Vulcan 侧资源输入。
    Resource {
        /// Exact resource reference.
        /// 精确资源引用。
        resource: ResourceReference,
    },
}

/// Carries one stable embedding input.
/// 承载一项稳定向量输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingInput {
    /// Stable caller-owned input identifier.
    /// 调用方拥有的稳定输入标识。
    pub input_id: String,
    /// Exact input content.
    /// 精确输入内容。
    pub content: EmbeddingInputContent,
}

/// Selects whether an embedding batch may omit only provider-confirmed invalid items.
/// 选择向量批次是否允许仅省略供应商已确认非法的单项输入。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingInvalidInputPolicy {
    /// Fails the complete batch when any input cannot be embedded.
    /// 任一输入无法生成向量时使完整批次失败。
    #[default]
    FailBatch,
    /// Allows omission only when the provider identifies the exact invalid input.
    /// 仅当供应商明确标识精确非法输入时允许省略该项。
    DropProviderConfirmedItems,
}

/// Carries one configured embedding request.
/// 承载一次已配置向量请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingRequest {
    /// Exact contract version.
    /// 精确契约版本。
    pub contract_version: u32,
    /// Caller-generated stable request identifier.
    /// 调用方生成的稳定请求标识。
    pub request_id: String,
    /// Optional idempotency key for in-flight coalescing only.
    /// 仅用于运行中合并的可选幂等键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    /// Authorized consumer profile identifier.
    /// 已授权的消费者配置档标识。
    pub consumer_profile_id: String,
    /// Ordered inputs.
    /// 有序输入。
    pub inputs: Vec<EmbeddingInput>,
    /// Requested semantic task.
    /// 请求的语义任务。
    pub task: EmbeddingTask,
    /// Optional exact target dimensions.
    /// 可选的精确目标维度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<u32>,
    /// Requested output shape.
    /// 请求的输出形态。
    pub output_kind: EmbeddingOutputKind,
    /// Requested value encoding.
    /// 请求的数值编码。
    pub encoding: EmbeddingEncoding,
    /// Explicit invalid-input handling policy.
    /// 显式非法输入处理策略。
    #[serde(default)]
    pub invalid_input_policy: EmbeddingInvalidInputPolicy,
}

impl EmbeddingRequest {
    /// Validates stable embedding input identity and request-level invariants.
    /// 校验稳定向量输入身份与请求级不变量。
    ///
    /// # Returns
    /// 返回值
    /// Success for one complete ordered batch, or a deterministic contract error.
    /// 完整有序批次有效时返回成功，否则返回确定性契约错误。
    pub fn validate(&self) -> Result<(), String> {
        validate_contract_identity(
            self.contract_version,
            self.request_id.as_str(),
            self.consumer_profile_id.as_str(),
        )?;
        validate_optional_idempotency_key(self.idempotency_key.as_deref())?;
        if self.inputs.is_empty() {
            return Err(String::from(
                "Embedding request must contain at least one input.",
            ));
        }
        if self.dimensions == Some(0) {
            return Err(String::from(
                "Embedding dimensions must be greater than zero.",
            ));
        }
        let mut input_ids = BTreeSet::new();
        for (input_index, input) in self.inputs.iter().enumerate() {
            let input_id = input.input_id.trim();
            if input_id.is_empty() {
                return Err(format!(
                    "Embedding input {input_index} must have a non-empty input_id."
                ));
            }
            if !input_ids.insert(input_id) {
                return Err(format!(
                    "Embedding input_id `{input_id}` appears more than once."
                ));
            }
            if matches!(
                &input.content,
                EmbeddingInputContent::Text { text } if text.is_empty()
            ) {
                return Err(format!("Embedding input `{input_id}` contains empty text."));
            }
        }
        Ok(())
    }
}

/// Identifies an embedding normalization declaration.
/// 标识一项向量归一化声明。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingNormalization {
    /// Normalization is not established by the contract.
    /// 契约未确定归一化情况。
    Unknown,
    /// Values are explicitly not normalized.
    /// 数值明确未归一化。
    None,
    /// Values use L2 normalization.
    /// 数值使用 L2 归一化。
    L2,
}

/// Identifies the distance metric declared for one embedding space.
/// 标识一个向量空间声明的距离度量。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistanceMetric {
    /// Provider metadata does not establish one exact distance metric.
    /// 供应商元数据未确定一种精确距离度量。
    Unknown,
    /// Cosine distance or similarity.
    /// 余弦距离或相似度。
    Cosine,
    /// Euclidean distance.
    /// 欧氏距离。
    Euclidean,
    /// Dot-product similarity.
    /// 点积相似度。
    DotProduct,
}

/// Carries immutable non-secret route identity for one execution.
/// 承载一次执行的不可变非秘密路由身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionRouteSnapshot {
    /// Exact provider identifier.
    /// 精确供应商标识。
    pub provider_id: String,
    /// Exact product or service-entry identifier.
    /// 精确产品或服务入口标识。
    pub service_entry_id: String,
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    pub connection_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    pub model_id: String,
    /// Optional exact protocol binding identifier.
    /// 可选的精确协议绑定标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_binding_id: Option<String>,
    /// Optional exact action binding identifier.
    /// 可选的精确动作绑定标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_binding_id: Option<String>,
    /// Optional exact provider-internal route identifier.
    /// 可选的精确供应商内部路由标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_route_id: Option<String>,
    /// Optional provider-internal upstream display name.
    /// 可选的供应商内部上游显示名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_provider_name: Option<String>,
    /// Optional provider-internal endpoint display name.
    /// 可选的供应商内部端点显示名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_endpoint_name: Option<String>,
    /// Capability evidence revision frozen at dispatch.
    /// 分派时冻结的能力证据修订。
    pub capability_revision: String,
    /// Runtime catalog revision frozen at dispatch.
    /// 分派时冻结的运行时目录修订。
    pub catalog_revision: String,
    /// Content-addressed revision of the exact provider route and request projection.
    /// 精确供应商路由与请求投影的内容寻址修订号。
    pub route_revision: String,
    /// Actual execution usage context.
    /// 实际执行使用场景。
    pub usage_scope: InferenceUsageScope,
}

/// Identifies the exact compatibility space of embedding values.
/// 标识向量值的精确兼容空间。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingSpaceFingerprint {
    /// Exact provider identifier.
    /// 精确供应商标识。
    pub provider_id: String,
    /// Exact product or service-entry identifier.
    /// 精确产品或服务入口标识。
    pub service_entry_id: String,
    /// Optional account-isolated deployment identity.
    /// 可选的账号隔离部署身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment_id: Option<String>,
    /// Optional exact provider-internal route that produced this embedding space.
    /// 可选的生成当前向量空间的精确供应商内部路由。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_route_id: Option<String>,
    /// Exact model identifier.
    /// 精确模型标识。
    pub model_id: String,
    /// Exact protocol or action binding identifier.
    /// 精确协议或动作绑定标识。
    pub binding_id: String,
    /// Capability evidence revision.
    /// 能力证据修订。
    pub capability_revision: String,
    /// Actual embedding task.
    /// 实际向量任务。
    pub task: EmbeddingTask,
    /// Actual output dimensions.
    /// 实际输出维度。
    pub dimensions: u32,
    /// Actual output shape.
    /// 实际输出形态。
    pub output_kind: EmbeddingOutputKind,
    /// Actual output encoding.
    /// 实际输出编码。
    pub encoding: EmbeddingEncoding,
    /// Contract-declared normalization.
    /// 契约声明的归一化方式。
    pub normalization: EmbeddingNormalization,
    /// Contract-declared distance metric.
    /// 契约声明的距离度量。
    pub distance_metric: DistanceMetric,
}

/// Carries one validated embedding payload.
/// 承载一项已校验向量载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EmbeddingPayload {
    /// One dense float vector.
    /// 一个稠密浮点向量。
    Dense {
        /// Ordered finite values.
        /// 有序有限数值。
        values: Vec<f32>,
    },
    /// One sparse float vector.
    /// 一个稀疏浮点向量。
    Sparse {
        /// Ordered non-negative indices.
        /// 有序非负索引。
        indices: Vec<u32>,
        /// Values corresponding one-to-one with indices.
        /// 与索引一一对应的数值。
        values: Vec<f32>,
    },
    /// Multiple ordered dense float vectors.
    /// 多个有序稠密浮点向量。
    MultiVector {
        /// Ordered finite vectors.
        /// 有序有限向量。
        vectors: Vec<Vec<f32>>,
    },
    /// Provider-declared Base64 payload.
    /// 供应商声明的 Base64 载荷。
    Base64 {
        /// Exact encoded value.
        /// 精确编码值。
        value: String,
    },
}

/// Carries one embedding result corresponding to one input.
/// 承载一项与输入对应的向量结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingResult {
    /// Original stable input identifier.
    /// 原始稳定输入标识。
    pub input_id: String,
    /// Original zero-based input index.
    /// 原始从零开始的输入索引。
    pub input_index: u32,
    /// Validated embedding payload.
    /// 已校验向量载荷。
    pub embedding: EmbeddingPayload,
}

/// Carries one input omitted only after the provider confirmed an item-scoped validation failure.
/// 承载一项仅在供应商确认单项校验失败后才被省略的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingDroppedInput {
    /// Original stable input identifier.
    /// 原始稳定输入标识。
    pub input_id: String,
    /// Original zero-based input index.
    /// 原始从零开始的输入索引。
    pub input_index: u32,
    /// Stable adapter-selected reason code.
    /// 适配器选择的稳定原因代码。
    pub reason_code: String,
    /// Controlled message without provider secrets.
    /// 不含供应商秘密的受控消息。
    pub controlled_message: String,
}

/// Carries one validated embedding response.
/// 承载一次已校验向量响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    /// Original request identifier.
    /// 原始请求标识。
    pub request_id: String,
    /// Immutable route snapshot.
    /// 不可变路由快照。
    pub route: ExecutionRouteSnapshot,
    /// Exact embedding-space fingerprint.
    /// 精确向量空间指纹。
    pub fingerprint: EmbeddingSpaceFingerprint,
    /// Results corresponding one-to-one with request inputs.
    /// 与请求输入一一对应的结果。
    pub results: Vec<EmbeddingResult>,
    /// Provider-confirmed item-scoped omissions.
    /// 供应商已确认的单项省略记录。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dropped: Vec<EmbeddingDroppedInput>,
    /// Provider request usage.
    /// 供应商请求用量。
    pub usage: RequestUsage,
    /// End-to-end execution duration in milliseconds.
    /// 端到端执行耗时毫秒数。
    pub duration_ms: u64,
}

impl EmbeddingResponse {
    /// Validates exact result and omission coverage against the originating request.
    /// 根据原始请求校验精确结果与省略项覆盖关系。
    ///
    /// # Parameters
    /// 参数
    /// `request` is the immutable request that produced this response.
    /// `request` 是生成当前响应的不可变请求。
    ///
    /// # Returns
    /// 返回值
    /// Success only when every input appears exactly once as a result or an allowed omission.
    /// 仅当每项输入都恰好作为结果或允许的省略项出现一次时返回成功。
    pub fn validate_for_request(&self, request: &EmbeddingRequest) -> Result<(), String> {
        if self.request_id != request.request_id {
            return Err(String::from(
                "Embedding response request_id does not match the originating request.",
            ));
        }
        if request.invalid_input_policy == EmbeddingInvalidInputPolicy::FailBatch
            && !self.dropped.is_empty()
        {
            return Err(String::from(
                "Embedding response omitted inputs under fail_batch policy.",
            ));
        }
        let mut covered_indices = BTreeSet::new();
        for result in &self.results {
            let index = result.input_index as usize;
            let Some(input) = request.inputs.get(index) else {
                return Err(format!(
                    "Embedding response result index {} is out of range.",
                    result.input_index
                ));
            };
            if result.input_id != input.input_id || !covered_indices.insert(result.input_index) {
                return Err(format!(
                    "Embedding response result identity `{}` is invalid or repeated.",
                    result.input_id
                ));
            }
        }
        for dropped in &self.dropped {
            let index = dropped.input_index as usize;
            let Some(input) = request.inputs.get(index) else {
                return Err(format!(
                    "Embedding response dropped index {} is out of range.",
                    dropped.input_index
                ));
            };
            if dropped.input_id != input.input_id
                || dropped.reason_code.trim().is_empty()
                || dropped.controlled_message.trim().is_empty()
                || !covered_indices.insert(dropped.input_index)
            {
                return Err(format!(
                    "Embedding response dropped identity `{}` is invalid or repeated.",
                    dropped.input_id
                ));
            }
        }
        if covered_indices.len() != request.inputs.len() {
            return Err(format!(
                "Embedding response covered {} of {} inputs.",
                covered_indices.len(),
                request.inputs.len()
            ));
        }
        Ok(())
    }
}

/// Represents one rerank text value with stable identity.
/// 表示一项具有稳定身份的重排文本值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RerankText {
    /// Stable caller-owned value identifier.
    /// 调用方拥有的稳定值标识。
    pub id: String,
    /// Exact text content.
    /// 精确文本内容。
    pub text: String,
}

/// Identifies an allowed rerank truncation policy.
/// 标识允许的重排截断策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RerankTruncation {
    /// Rejects input that exceeds exact model limits.
    /// 拒绝超过精确模型限制的输入。
    Disabled,
    /// Uses the exact provider contract default.
    /// 使用精确供应商契约默认值。
    ProviderDefault,
    /// Truncates the beginning when explicitly supported.
    /// 在明确支持时截断开头。
    Start,
    /// Truncates the end when explicitly supported.
    /// 在明确支持时截断末尾。
    End,
}

/// Identifies the meaning of raw rerank scores.
/// 标识原始重排分数的含义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RerankScoreSemantics {
    /// Provider did not establish score semantics.
    /// 供应商未确定分数语义。
    Unknown,
    /// Higher values mean greater relevance.
    /// 数值越高表示相关性越强。
    HigherIsMoreRelevant,
    /// Lower values mean greater relevance.
    /// 数值越低表示相关性越强。
    LowerIsMoreRelevant,
}

/// Carries one configured rerank request.
/// 承载一次已配置重排请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RerankRequest {
    /// Exact contract version.
    /// 精确契约版本。
    pub contract_version: u32,
    /// Caller-generated stable request identifier.
    /// 调用方生成的稳定请求标识。
    pub request_id: String,
    /// Optional idempotency key for in-flight coalescing only.
    /// 仅用于运行中合并的可选幂等键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    /// Authorized consumer profile identifier.
    /// 已授权的消费者配置档标识。
    pub consumer_profile_id: String,
    /// Exact query.
    /// 精确查询。
    pub query: RerankText,
    /// Ordered candidate documents.
    /// 有序候选文档。
    pub candidates: Vec<RerankText>,
    /// Maximum results to return.
    /// 最大返回结果数。
    pub top_n: u32,
    /// Requested truncation policy.
    /// 请求的截断策略。
    pub truncation: RerankTruncation,
    /// Whether provider-returned candidate content is requested.
    /// 是否请求供应商返回候选内容。
    pub return_content: bool,
}

impl RerankRequest {
    /// Validates stable rerank identities and candidate bounds.
    /// 校验稳定重排身份与候选边界。
    ///
    /// # Returns
    /// 返回值
    /// Success for one complete request, or a deterministic contract error.
    /// 完整请求有效时返回成功，否则返回确定性契约错误。
    pub fn validate(&self) -> Result<(), String> {
        validate_contract_identity(
            self.contract_version,
            self.request_id.as_str(),
            self.consumer_profile_id.as_str(),
        )?;
        validate_optional_idempotency_key(self.idempotency_key.as_deref())?;
        if self.query.id.trim().is_empty() || self.query.text.is_empty() {
            return Err(String::from(
                "Rerank query must have a non-empty id and text.",
            ));
        }
        if self.candidates.is_empty() {
            return Err(String::from(
                "Rerank request must contain at least one candidate.",
            ));
        }
        if self.top_n == 0 || self.top_n as usize > self.candidates.len() {
            return Err(format!(
                "Rerank top_n {} must be between 1 and the candidate count {}.",
                self.top_n,
                self.candidates.len()
            ));
        }
        let mut candidate_ids = BTreeSet::new();
        for (candidate_index, candidate) in self.candidates.iter().enumerate() {
            let candidate_id = candidate.id.trim();
            if candidate_id.is_empty() || candidate.text.is_empty() {
                return Err(format!(
                    "Rerank candidate {candidate_index} must have a non-empty id and text."
                ));
            }
            if !candidate_ids.insert(candidate_id) {
                return Err(format!(
                    "Rerank candidate id `{candidate_id}` appears more than once."
                ));
            }
        }
        Ok(())
    }
}

/// Validates fields shared by every public inference request.
/// 校验全部公共推理请求共享的字段。
///
/// # Parameters
/// 参数
/// The version and stable identifiers come directly from the decoded request.
/// 版本与稳定标识直接来自已解码请求。
///
/// # Returns
/// 返回值
/// Success when the exact contract and identities are valid.
/// 精确契约与身份有效时返回成功。
fn validate_contract_identity(
    contract_version: u32,
    request_id: &str,
    consumer_profile_id: &str,
) -> Result<(), String> {
    if contract_version != INFERENCE_CONTRACT_VERSION {
        return Err(format!(
            "Unsupported inference contract version {contract_version}; expected {INFERENCE_CONTRACT_VERSION}."
        ));
    }
    if request_id.trim().is_empty() {
        return Err(String::from("Inference request_id must not be empty."));
    }
    if consumer_profile_id.trim().is_empty() {
        return Err(String::from(
            "Inference consumer_profile_id must not be empty.",
        ));
    }
    Ok(())
}

/// Validates one optional key used only for running-request coalescing.
/// 校验一项仅用于运行中请求合并的可选键。
///
/// # Parameters
/// 参数
/// `idempotency_key` is the exact optional caller-provided identity.
/// `idempotency_key` 是调用方提供的精确可选身份。
///
/// # Returns
/// 返回值
/// Success when the key is absent or contains a non-empty identity.
/// 键不存在或包含非空身份时返回成功。
fn validate_optional_idempotency_key(idempotency_key: Option<&str>) -> Result<(), String> {
    if idempotency_key.is_some_and(|key| key.trim().is_empty()) {
        return Err(String::from(
            "Inference idempotency_key must not be empty when present.",
        ));
    }
    Ok(())
}

/// Identifies the exact compatibility profile of rerank scores.
/// 标识重排分数的精确兼容配置档。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RerankProfileFingerprint {
    /// Exact provider identifier.
    /// 精确供应商标识。
    pub provider_id: String,
    /// Exact product or service-entry identifier.
    /// 精确产品或服务入口标识。
    pub service_entry_id: String,
    /// Optional account-isolated deployment identity.
    /// 可选的账号隔离部署身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment_id: Option<String>,
    /// Optional exact provider-internal route that defines this score profile.
    /// 可选的定义当前分数画像的精确供应商内部路由。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_route_id: Option<String>,
    /// Exact model identifier.
    /// 精确模型标识。
    pub model_id: String,
    /// Exact protocol or action binding identifier.
    /// 精确协议或动作绑定标识。
    pub binding_id: String,
    /// Capability evidence revision.
    /// 能力证据修订。
    pub capability_revision: String,
    /// Raw score semantics.
    /// 原始分数语义。
    pub score_semantics: RerankScoreSemantics,
}

/// Carries one validated rerank result.
/// 承载一项已校验重排结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RerankResult {
    /// Original stable candidate identifier.
    /// 原始稳定候选标识。
    pub candidate_id: String,
    /// Original zero-based candidate index.
    /// 原始从零开始的候选索引。
    pub original_index: u32,
    /// Zero-based provider result rank.
    /// 从零开始的供应商结果排名。
    pub rank: u32,
    /// Unmodified finite provider score.
    /// 未修改的供应商有限分数。
    pub score: f64,
    /// Optional candidate content returned by the provider.
    /// 供应商返回的可选候选内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Carries one validated rerank response.
/// 承载一次已校验重排响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RerankResponse {
    /// Original request identifier.
    /// 原始请求标识。
    pub request_id: String,
    /// Immutable route snapshot.
    /// 不可变路由快照。
    pub route: ExecutionRouteSnapshot,
    /// Exact rerank-profile fingerprint.
    /// 精确重排配置档指纹。
    pub fingerprint: RerankProfileFingerprint,
    /// Ranked results.
    /// 已排序结果。
    pub results: Vec<RerankResult>,
    /// Provider request usage.
    /// 供应商请求用量。
    pub usage: RequestUsage,
    /// End-to-end execution duration in milliseconds.
    /// 端到端执行耗时毫秒数。
    pub duration_ms: u64,
}

impl RerankResponse {
    /// Validates ranked identities, positions, bounds, and score ordering against one request.
    /// 根据单个请求校验重排身份、位置、边界与分数顺序。
    ///
    /// # Parameters
    /// 参数
    /// `request` is the immutable rerank request that produced this response.
    /// `request` 是生成当前响应的不可变重排请求。
    ///
    /// # Returns
    /// 返回值
    /// Success only when every result maps exactly once to an original candidate and rank.
    /// 仅当每项结果都准确且唯一地映射到原始候选与排名时返回成功。
    pub fn validate_for_request(&self, request: &RerankRequest) -> Result<(), String> {
        request.validate()?;
        if self.request_id != request.request_id {
            return Err(String::from(
                "Rerank response request_id does not match the originating request.",
            ));
        }
        if self.results.len() > request.top_n as usize {
            return Err(format!(
                "Rerank response returned {} results, exceeding requested top_n {}.",
                self.results.len(),
                request.top_n
            ));
        }
        let mut covered_indices = BTreeSet::new();
        let mut previous_score = None;
        for (rank, result) in self.results.iter().enumerate() {
            let expected_rank = u32::try_from(rank)
                .map_err(|_| String::from("Rerank response rank exceeds the u32 contract."))?;
            if result.rank != expected_rank {
                return Err(format!(
                    "Rerank response rank {} does not match result position {expected_rank}.",
                    result.rank
                ));
            }
            let candidate_index = result.original_index as usize;
            let Some(candidate) = request.candidates.get(candidate_index) else {
                return Err(format!(
                    "Rerank response candidate index {} is out of range.",
                    result.original_index
                ));
            };
            if result.candidate_id != candidate.id {
                return Err(format!(
                    "Rerank response candidate identity `{}` does not match request candidate `{}` at index {}.",
                    result.candidate_id, candidate.id, result.original_index
                ));
            }
            if !covered_indices.insert(result.original_index) {
                return Err(format!(
                    "Rerank response candidate index {} appears more than once.",
                    result.original_index
                ));
            }
            if !result.score.is_finite() {
                return Err(format!(
                    "Rerank response score for candidate `{}` must be finite.",
                    result.candidate_id
                ));
            }
            if !request.return_content && result.content.is_some() {
                return Err(format!(
                    "Rerank response returned content for candidate `{}` when return_content is false.",
                    result.candidate_id
                ));
            }
            if let Some(previous_score) = previous_score {
                let ordered = match self.fingerprint.score_semantics {
                    RerankScoreSemantics::Unknown => true,
                    RerankScoreSemantics::HigherIsMoreRelevant => previous_score >= result.score,
                    RerankScoreSemantics::LowerIsMoreRelevant => previous_score <= result.score,
                };
                if !ordered {
                    return Err(String::from(
                        "Rerank response score order does not match its declared score semantics.",
                    ));
                }
            }
            previous_score = Some(result.score);
        }
        Ok(())
    }
}

/// Retains one normalized Responses output item together with its stable item identity.
/// 将一项归一化 Responses 输出与其稳定项目身份一起保留。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponsesExecutionOutputItem {
    /// Stable response item identifier emitted by the provider decoder.
    /// 供应商解码器发出的稳定响应项目标识。
    pub item_id: String,
    /// Provider-neutral semantic output content.
    /// 供应商无关的语义输出内容。
    pub content: ContentItem,
}

/// Carries one completed Responses execution result.
/// 承载一次已完成的 Responses 执行结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponsesExecutionResult {
    /// Original request identifier.
    /// 原始请求标识。
    pub request_id: String,
    /// Immutable route snapshot.
    /// 不可变路由快照。
    pub route: ExecutionRouteSnapshot,
    /// User-visible output text.
    /// 用户可见输出文本。
    pub output_text: String,
    /// Optional user-visible reasoning representation.
    /// 可选的用户可见推理表示。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_text: Option<String>,
    /// Whether the originating public request selected encrypted reasoning content.
    /// 原始公共请求是否选择了加密推理内容。
    #[serde(default)]
    pub include_reasoning_encrypted_content: bool,
    /// Optional structured output.
    /// 可选结构化输出。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<Value>,
    /// Ordered non-text response items retained from the normalized provider stream.
    /// 从归一化供应商流中保留的有序非文本响应项。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_items: Vec<ResponsesExecutionOutputItem>,
    /// Stable termination reason.
    /// 稳定终止原因。
    pub termination_reason: String,
    /// Provider request usage.
    /// 供应商请求用量。
    pub usage: RequestUsage,
    /// End-to-end execution duration in milliseconds.
    /// 端到端执行耗时毫秒数。
    pub duration_ms: u64,
}

/// Describes one configured operation route in a capability snapshot.
/// 描述能力快照中的一条已配置操作路由。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferenceOperationCapability {
    /// Exact semantic operation.
    /// 精确语义操作。
    pub operation: AiOperation,
    /// Optional consumer-scoped LLM purpose.
    /// 可选的消费者作用域 LLM 用途。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose_id: Option<String>,
    /// Whether the configured route can execute now.
    /// 已配置路由当前是否可以执行。
    pub available: bool,
    /// Controlled unavailability reason when resolution fails.
    /// 解析失败时的受控不可用原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    /// Immutable route identity when resolution succeeds.
    /// 解析成功时的不可变路由身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<ExecutionRouteSnapshot>,
    /// Strong embedding capability when this is an embedding route.
    /// 当前为向量路由时的强类型向量能力。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<EmbeddingCapabilities>,
    /// Strong asynchronous Batch capability when this embedding route supports it.
    /// 当前向量路由支持异步 Batch 时的强类型批次能力。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_batch: Option<EmbeddingBatchCapabilities>,
    /// Strong rerank capability when this is a rerank route.
    /// 当前为重排路由时的强类型重排能力。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerank: Option<RerankCapabilities>,
}

/// Identifies how one provider accepts asynchronous inference Batch inputs.
/// 标识供应商接收异步推理 Batch 输入的方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceBatchTransport {
    /// The complete ordered request array is submitted in one JSON body.
    /// 完整且有序的请求数组通过一个 JSON 请求体提交。
    InlineJson,
    /// Requests are encoded as JSONL, uploaded, and referenced by a file identifier.
    /// 请求编码为 JSONL、上传后通过文件标识引用。
    JsonlFile,
}

/// Identifies where a completed asynchronous Batch exposes its result items.
/// 标识完成的异步 Batch 在何处提供结果项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceBatchResultDelivery {
    /// Result items are embedded in the terminal Batch status response.
    /// 结果项内嵌在终态 Batch 状态响应中。
    InlineJson,
    /// Result and error items are exposed through provider file identifiers.
    /// 成功与错误结果通过供应商文件标识提供。
    JsonlFiles,
}

/// Declares verified provider limits and explicit host safety limits for embedding Batch work.
/// 声明向量 Batch 工作已验证的供应商限制和明确的宿主安全限制。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingBatchCapabilities {
    /// Provider input transport selected by the frozen manifest binding.
    /// 冻结 Manifest 绑定选择的供应商输入传输方式。
    pub transport: InferenceBatchTransport,
    /// Provider result delivery selected by the frozen manifest binding.
    /// 冻结 Manifest 绑定选择的供应商结果交付方式。
    pub result_delivery: InferenceBatchResultDelivery,
    /// Verified provider maximum request items per remote Batch, when published.
    /// 供应商已公布时每个远端 Batch 的最大请求项数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_maximum_requests: Option<u32>,
    /// Verified provider maximum complete payload bytes, when published.
    /// 供应商已公布时完整负载的最大字节数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_maximum_payload_bytes: Option<u64>,
    /// Verified provider maximum encoded item bytes, when published.
    /// 供应商已公布时单个编码请求项的最大字节数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_maximum_item_bytes: Option<u64>,
    /// Provider maximum retained file count for one account, when published.
    /// 供应商已公布时单个账号允许保留的最大文件数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_maximum_files: Option<u32>,
    /// Provider maximum aggregate retained file bytes for one account, when published.
    /// 供应商已公布时单个账号允许保留的文件总字节数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_maximum_total_file_bytes: Option<u64>,
    /// Provider file-upload requests per second, when published.
    /// 供应商已公布时文件上传接口每秒请求上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_file_upload_qps: Option<u32>,
    /// Provider aggregate file-operation requests per second, when published.
    /// 供应商已公布时文件类接口合计每秒请求上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_file_operation_qps: Option<u32>,
    /// Provider maximum simultaneously active Batch tasks, when published.
    /// 供应商已公布时同时活动的 Batch 任务上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_maximum_parallel_tasks: Option<u32>,
    /// Provider Batch creation requests per minute, when published.
    /// 供应商已公布时每分钟 Batch 创建请求上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_task_create_rpm: Option<u32>,
    /// Provider Batch status requests per minute, when published.
    /// 供应商已公布时每分钟 Batch 状态请求上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_task_status_rpm: Option<u32>,
    /// Provider Batch-list reconciliation requests per minute, when published.
    /// 供应商已公布时每分钟 Batch 列表对账请求上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_task_list_rpm: Option<u32>,
    /// Provider Batch cancellation requests per minute, when published.
    /// 供应商已公布时每分钟 Batch 取消请求上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_task_cancel_rpm: Option<u32>,
    /// Host-owned maximum remote tasks generated for one initialization run.
    /// 一次初始化运行允许生成的宿主持有最大远端任务数。
    pub host_policy_maximum_tasks_per_run: u32,
    /// Host-owned maximum request items used even when the provider publishes no bound.
    /// 即使供应商未公布上限也始终生效的宿主持有最大请求项数量。
    pub host_policy_maximum_requests: u32,
    /// Host-owned maximum payload bytes used to bound local memory and transport.
    /// 用于限制本地内存与传输的宿主持有最大负载字节数。
    pub host_policy_maximum_payload_bytes: u64,
    /// Host-owned maximum downloaded result bytes for one remote Batch task.
    /// 单个远端 Batch 任务允许下载的宿主持有最大结果字节数。
    pub host_policy_maximum_result_bytes: u64,
    /// Exact provider completion window submitted when the protocol exposes one, in seconds.
    /// 协议提供该参数时提交给供应商的精确完成窗口，单位为秒。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_completion_window_seconds: Option<u64>,
    /// Minimum provider-recommended interval between status polls.
    /// 供应商建议的最短状态轮询间隔。
    pub minimum_poll_interval_ms: u64,
    /// Whether the provider exposes a remote cancellation endpoint.
    /// 供应商是否提供远端取消端点。
    pub supports_cancel: bool,
    /// Whether a provider input file can be deleted after durable result ingestion.
    /// 结果持久化后是否可以删除供应商输入文件。
    pub deletes_input_file_after_ingestion: bool,
}

impl EmbeddingBatchCapabilities {
    /// Validates one complete asynchronous embedding Batch capability declaration.
    /// 校验一份完整的异步向量 Batch 能力声明。
    ///
    /// # Returns
    /// 返回值
    /// Unit for a coherent bounded declaration, or a precise contract error.
    /// 声明一致且有界时返回空值，否则返回明确契约错误。
    pub fn validate(&self) -> Result<(), String> {
        if self.provider_maximum_requests == Some(0)
            || self.provider_maximum_payload_bytes == Some(0)
            || self.provider_maximum_item_bytes == Some(0)
            || self.provider_maximum_files == Some(0)
            || self.provider_maximum_total_file_bytes == Some(0)
            || self.provider_file_upload_qps == Some(0)
            || self.provider_file_operation_qps == Some(0)
            || self.provider_maximum_parallel_tasks == Some(0)
            || self.provider_task_create_rpm == Some(0)
            || self.provider_task_status_rpm == Some(0)
            || self.provider_task_list_rpm == Some(0)
            || self.provider_task_cancel_rpm == Some(0)
            || self.host_policy_maximum_tasks_per_run == 0
            || self.host_policy_maximum_requests == 0
            || self.host_policy_maximum_payload_bytes == 0
            || self.host_policy_maximum_result_bytes == 0
            || self.provider_completion_window_seconds == Some(0)
            || self.minimum_poll_interval_ms == 0
        {
            return Err(String::from(
                "Embedding Batch capability contains a zero limit",
            ));
        }
        if self
            .provider_maximum_item_bytes
            .zip(self.provider_maximum_payload_bytes)
            .is_some_and(|(item, payload)| item > payload)
        {
            return Err(String::from(
                "Embedding Batch provider item limit exceeds the provider payload limit",
            ));
        }
        if self
            .provider_file_upload_qps
            .zip(self.provider_file_operation_qps)
            .is_some_and(|(upload, operations)| upload > operations)
        {
            return Err(String::from(
                "Embedding Batch upload QPS exceeds the aggregate file-operation QPS",
            ));
        }
        if !self.supports_cancel && self.provider_task_cancel_rpm.is_some() {
            return Err(String::from(
                "Embedding Batch cancellation rate is declared without cancellation support",
            ));
        }
        if self
            .provider_completion_window_seconds
            .is_some_and(|seconds| !seconds.is_multiple_of(3_600))
        {
            return Err(String::from(
                "Embedding Batch completion window must be expressed in complete hours",
            ));
        }
        match (self.transport, self.result_delivery) {
            (InferenceBatchTransport::InlineJson, InferenceBatchResultDelivery::InlineJson) => {
                if self.provider_maximum_files.is_some()
                    || self.provider_maximum_total_file_bytes.is_some()
                    || self.provider_file_upload_qps.is_some()
                    || self.provider_file_operation_qps.is_some()
                    || self.provider_task_list_rpm.is_some()
                    || self.deletes_input_file_after_ingestion
                {
                    return Err(String::from(
                        "Inline embedding Batch capability declares file-only limits or behavior",
                    ));
                }
                Ok(())
            }
            (InferenceBatchTransport::JsonlFile, InferenceBatchResultDelivery::JsonlFiles) => {
                if self.provider_maximum_payload_bytes.is_some_and(|maximum| {
                    self.provider_maximum_total_file_bytes
                        .is_some_and(|total| maximum > total)
                }) {
                    return Err(String::from(
                        "Embedding Batch file size exceeds the aggregate provider file quota",
                    ));
                }
                Ok(())
            }
            _ => Err(String::from(
                "Embedding Batch transport and result delivery are inconsistent",
            )),
        }
    }

    /// Returns the effective request-count limit after applying host policy.
    /// 应用宿主策略后返回有效请求项数量上限。
    pub fn effective_maximum_requests(&self) -> u32 {
        self.provider_maximum_requests
            .map_or(self.host_policy_maximum_requests, |provider| {
                provider.min(self.host_policy_maximum_requests)
            })
    }

    /// Returns the effective payload-byte limit after applying host policy.
    /// 应用宿主策略后返回有效负载字节上限。
    pub fn effective_maximum_payload_bytes(&self) -> u64 {
        self.provider_maximum_payload_bytes
            .map_or(self.host_policy_maximum_payload_bytes, |provider| {
                provider.min(self.host_policy_maximum_payload_bytes)
            })
    }

    /// Returns the effective remote-task count after applying host policy.
    /// 应用宿主策略后返回有效远端任务数量上限。
    pub fn effective_maximum_tasks_per_run(&self) -> u32 {
        self.provider_maximum_parallel_tasks
            .map_or(self.host_policy_maximum_tasks_per_run, |provider| {
                provider.min(self.host_policy_maximum_tasks_per_run)
            })
    }
}

/// Carries one consumer-profile-scoped configured inference capability snapshot.
/// 承载一份消费者配置档作用域的已配置推理能力快照。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferenceCapabilitySnapshot {
    /// Exact public contract version.
    /// 精确公共契约版本。
    pub contract_version: u32,
    /// Consumer profile represented by this snapshot.
    /// 当前快照表示的消费者配置档。
    pub consumer_profile_id: String,
    /// Complete configured operation list.
    /// 完整的已配置操作列表。
    pub operations: Vec<InferenceOperationCapability>,
}

/// Declares strong embedding limits for one exact model route.
/// 声明一条精确模型路由的强类型向量限制。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingCapabilities {
    /// Supported semantic tasks.
    /// 支持的语义任务。
    pub tasks: BTreeSet<EmbeddingTask>,
    /// Supported resource kinds.
    /// 支持的资源类型。
    #[serde(default)]
    pub input_resources: BTreeSet<ResourceKind>,
    /// Supported output shapes.
    /// 支持的输出形态。
    pub output_kinds: BTreeSet<EmbeddingOutputKind>,
    /// Supported value encodings.
    /// 支持的数值编码。
    pub encodings: BTreeSet<EmbeddingEncoding>,
    /// Exact supported dimensions when discrete.
    /// 离散情况下支持的精确维度。
    #[serde(default)]
    pub dimensions: BTreeSet<u32>,
    /// Optional inclusive minimum dimension.
    /// 可选的包含式最小维度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_dimensions: Option<u32>,
    /// Optional inclusive maximum dimension.
    /// 可选的包含式最大维度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_dimensions: Option<u32>,
    /// Optional exact default dimensions.
    /// 可选的精确默认维度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_dimensions: Option<u32>,
    /// Optional maximum inputs per request.
    /// 可选的单次请求最大输入数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_inputs: Option<u32>,
    /// Optional maximum tokens per input.
    /// 可选的单条输入最大 Token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_tokens_per_input: Option<u64>,
    /// Declared output normalization.
    /// 声明的输出归一化方式。
    pub normalization: EmbeddingNormalization,
    /// Declared distance metric.
    /// 声明的距离度量。
    pub distance_metric: DistanceMetric,
    /// Optional per-task input wrapping the model expects, such as an instruction-aware model's query
    /// instruction; each template holds [`EMBEDDING_INPUT_TEMPLATE_PLACEHOLDER`] exactly once.
    /// 模型期望的可选逐任务输入包裹，例如指令感知模型的查询指令；每个模板恰好包含一次
    /// [`EMBEDDING_INPUT_TEMPLATE_PLACEHOLDER`]。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub input_templates: BTreeMap<EmbeddingTask, String>,
}

/// Marks where an embedding input template places the original input.
/// 标记向量输入模板放置原始输入的位置。
pub const EMBEDDING_INPUT_TEMPLATE_PLACEHOLDER: &str = "{input}";

impl EmbeddingCapabilities {
    /// Reports whether `dimensions` is a width this route accepts.
    /// 判断 `dimensions` 是否为该路由接受的宽度。
    ///
    /// A non-empty discrete set must contain the width, and any declared inclusive minimum or maximum must bound it;
    /// a contiguous range is published as bounds with an empty set. Zero is never a valid width.
    /// 非空离散集合必须包含该宽度，且已声明的包含式最小、最大值必须限定它；连续区间以边界加空集合发布。零永远不是有效宽度。
    pub fn supports_dimensions(&self, dimensions: u32) -> bool {
        dimensions > 0
            && (self.dimensions.is_empty() || self.dimensions.contains(&dimensions))
            && self
                .minimum_dimensions
                .is_none_or(|minimum| dimensions >= minimum)
            && self
                .maximum_dimensions
                .is_none_or(|maximum| dimensions <= maximum)
    }

    /// Validates that every input template names a declared task and holds one placeholder.
    /// 校验每个输入模板都指向已声明任务且只包含一个占位符。
    ///
    /// # Returns
    /// 返回值
    /// Success when every template is applicable; otherwise the first offending task.
    /// 每个模板都可套用时返回成功；否则返回首个出错的任务。
    pub fn validate_input_templates(&self) -> Result<(), String> {
        for (task, template) in &self.input_templates {
            if !self.tasks.contains(task) {
                return Err(format!(
                    "Embedding input template targets undeclared task {task:?}."
                ));
            }
            if template
                .matches(EMBEDDING_INPUT_TEMPLATE_PLACEHOLDER)
                .count()
                != 1
            {
                return Err(format!(
                    "Embedding input template for {task:?} must contain `{EMBEDDING_INPUT_TEMPLATE_PLACEHOLDER}` exactly once."
                ));
            }
        }
        Ok(())
    }

    /// Wraps every text input of `request` in the template declared for its task.
    /// 用请求任务声明的模板包裹 `request` 的每条文本输入。
    ///
    /// # Returns
    /// 返回值
    /// The request unchanged when its task declares no template, otherwise a templated copy.
    /// 任务未声明模板时原样返回请求，否则返回套用模板后的副本。
    pub fn apply_input_template<'a>(
        &self,
        request: &'a EmbeddingRequest,
    ) -> std::borrow::Cow<'a, EmbeddingRequest> {
        let Some(template) = self.input_templates.get(&request.task) else {
            return std::borrow::Cow::Borrowed(request);
        };
        let mut templated = request.clone();
        for input in &mut templated.inputs {
            if let EmbeddingInputContent::Text { text } = &mut input.content {
                *text = template.replacen(EMBEDDING_INPUT_TEMPLATE_PLACEHOLDER, text, 1);
            }
        }
        std::borrow::Cow::Owned(templated)
    }

    /// Validates one request against an exact model-route embedding contract.
    /// 根据精确模型路由向量契约校验一个请求。
    ///
    /// # Parameters
    /// 参数
    /// `request` is already structurally valid and remains unchanged on failure.
    /// `request` 已通过结构校验，失败时保持不变。
    ///
    /// # Returns
    /// 返回值
    /// Success when every requested option is explicitly supported.
    /// 每个请求选项均被显式支持时返回成功。
    pub fn validate_request(&self, request: &EmbeddingRequest) -> Result<(), String> {
        request.validate()?;
        self.validate_request_options(request)?;
        if let Some(maximum_inputs) = self.maximum_inputs
            && request.inputs.len() > maximum_inputs as usize
        {
            return Err(format!(
                "Embedding input count {} exceeds the selected model limit {maximum_inputs}.",
                request.inputs.len()
            ));
        }
        Ok(())
    }

    /// Validates options and individual inputs for an asynchronous Batch of single-input requests.
    /// 校验由多个单输入请求组成的异步 Batch 的选项与各项输入。
    ///
    /// `request` is the aggregate host plan; its item count is governed by the separate Batch capability.
    /// `request` 是宿主聚合计划；其条目数量由独立 Batch 能力约束。
    /// Returns success when every per-request option and input resource is supported.
    /// 每项请求选项与输入资源均受支持时返回成功。
    pub fn validate_batch_request(&self, request: &EmbeddingRequest) -> Result<(), String> {
        request.validate()?;
        self.validate_request_options(request)
    }

    /// Validates embedding options shared by synchronous and asynchronous execution shapes.
    /// 校验同步与异步执行形态共享的向量选项。
    ///
    /// `request` is structurally valid. Returns success when task, output, dimensions, and resources match.
    /// `request` 已通过结构校验；任务、输出、维度与资源匹配时返回成功。
    fn validate_request_options(&self, request: &EmbeddingRequest) -> Result<(), String> {
        if !self.tasks.contains(&request.task) {
            return Err(format!(
                "Embedding task `{:?}` is not supported by the selected model route.",
                request.task
            ));
        }
        if !self.output_kinds.contains(&request.output_kind) {
            return Err(format!(
                "Embedding output kind `{:?}` is not supported by the selected model route.",
                request.output_kind
            ));
        }
        if !self.encodings.contains(&request.encoding) {
            return Err(format!(
                "Embedding encoding `{:?}` is not supported by the selected model route.",
                request.encoding
            ));
        }
        if let Some(dimensions) = request.dimensions
            && !self.supports_dimensions(dimensions)
        {
            return Err(format!(
                "Embedding dimensions {dimensions} are not supported by the selected model route."
            ));
        }
        for input in &request.inputs {
            if let EmbeddingInputContent::Resource { resource } = &input.content
                && !self.input_resources.contains(&resource.kind)
            {
                return Err(format!(
                    "Embedding resource kind `{:?}` is not supported by the selected model route.",
                    resource.kind
                ));
            }
        }
        Ok(())
    }
}

/// Declares strong rerank limits for one exact model route.
/// 声明一条精确模型路由的强类型重排限制。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RerankCapabilities {
    /// Supported query resource kinds; an empty set means text only.
    /// 支持的查询资源类型；空集合表示仅文本。
    #[serde(default)]
    pub query_resources: BTreeSet<ResourceKind>,
    /// Supported candidate resource kinds; an empty set means text only.
    /// 支持的候选资源类型；空集合表示仅文本。
    #[serde(default)]
    pub candidate_resources: BTreeSet<ResourceKind>,
    /// Optional maximum candidate count.
    /// 可选的最大候选数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_candidates: Option<u32>,
    /// Optional maximum tokens per text value.
    /// 可选的单项文本最大 Token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_tokens_per_item: Option<u64>,
    /// Optional maximum request token count.
    /// 可选的请求最大 Token 总数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_request_tokens: Option<u64>,
    /// Supported truncation policies.
    /// 支持的截断策略。
    pub truncation: BTreeSet<RerankTruncation>,
    /// Whether returning candidate content is supported.
    /// 是否支持返回候选内容。
    pub supports_return_content: bool,
    /// Raw score semantics.
    /// 原始分数语义。
    pub score_semantics: RerankScoreSemantics,
}

impl RerankCapabilities {
    /// Validates one request against an exact rerank contract.
    /// 根据精确重排契约校验一个请求。
    ///
    /// # Parameters
    /// 参数
    /// `request` is the immutable consumer request.
    /// `request` 是不可变消费者请求。
    ///
    /// # Returns
    /// 返回值
    /// Success when candidate, truncation, and output controls are supported.
    /// 候选、截断与输出控制均受支持时返回成功。
    pub fn validate_request(&self, request: &RerankRequest) -> Result<(), String> {
        request.validate()?;
        if let Some(maximum_candidates) = self.maximum_candidates
            && request.candidates.len() > maximum_candidates as usize
        {
            return Err(format!(
                "Rerank candidate count {} exceeds the selected model limit {maximum_candidates}.",
                request.candidates.len()
            ));
        }
        if !self.truncation.contains(&request.truncation) {
            return Err(format!(
                "Rerank truncation `{:?}` is not supported by the selected model route.",
                request.truncation
            ));
        }
        if request.return_content && !self.supports_return_content {
            return Err(String::from(
                "The selected rerank model route cannot return candidate content.",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one complete rerank request used by response-contract tests.
    /// 构建一份供响应契约测试使用的完整重排请求。
    fn rerank_request_fixture() -> RerankRequest {
        RerankRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: String::from("rerank-request"),
            idempotency_key: None,
            consumer_profile_id: String::from("vmm"),
            query: RerankText {
                id: String::from("query"),
                text: String::from("query text"),
            },
            candidates: vec![
                RerankText {
                    id: String::from("candidate-a"),
                    text: String::from("alpha"),
                },
                RerankText {
                    id: String::from("candidate-b"),
                    text: String::from("beta"),
                },
            ],
            top_n: 2,
            truncation: RerankTruncation::Disabled,
            return_content: false,
        }
    }

    /// Builds one ranked response with exact candidate identities and positions.
    /// 构建一份候选身份与位置准确的排序响应。
    fn rerank_response_fixture() -> RerankResponse {
        RerankResponse {
            request_id: String::from("rerank-request"),
            route: ExecutionRouteSnapshot {
                provider_id: String::from("provider"),
                service_entry_id: String::from("rerank"),
                connection_id: String::from("connection"),
                model_id: String::from("model"),
                protocol_binding_id: None,
                action_binding_id: Some(String::from("rerank")),
                secondary_route_id: None,
                secondary_provider_name: None,
                secondary_endpoint_name: None,
                capability_revision: String::from("capability"),
                catalog_revision: String::from("catalog"),
                route_revision: String::from("route"),
                usage_scope: InferenceUsageScope::InternalBackground,
            },
            fingerprint: RerankProfileFingerprint {
                provider_id: String::from("provider"),
                service_entry_id: String::from("rerank"),
                deployment_id: None,
                secondary_route_id: None,
                model_id: String::from("model"),
                binding_id: String::from("rerank"),
                capability_revision: String::from("capability"),
                score_semantics: RerankScoreSemantics::HigherIsMoreRelevant,
            },
            results: vec![
                RerankResult {
                    candidate_id: String::from("candidate-b"),
                    original_index: 1,
                    rank: 0,
                    score: 0.9,
                    content: None,
                },
                RerankResult {
                    candidate_id: String::from("candidate-a"),
                    original_index: 0,
                    rank: 1,
                    score: 0.5,
                    content: None,
                },
            ],
            usage: RequestUsage::default(),
            duration_ms: 1,
        }
    }

    /// Verifies rerank responses preserve exact candidate identities and ranked positions.
    /// 验证重排响应保留准确的候选身份与排序位置。
    #[test]
    fn rerank_response_validates_against_originating_request() {
        let request = rerank_request_fixture();
        let response = rerank_response_fixture();
        response
            .validate_for_request(&request)
            .expect("exact rerank response should be valid");

        let mut duplicate = response.clone();
        duplicate.results[1].original_index = 1;
        duplicate.results[1].candidate_id = String::from("candidate-b");
        assert!(duplicate.validate_for_request(&request).is_err());

        let mut mismatched_rank = response.clone();
        mismatched_rank.results[1].rank = 0;
        assert!(mismatched_rank.validate_for_request(&request).is_err());

        let mut unordered = response;
        unordered.results[1].score = 1.0;
        assert!(unordered.validate_for_request(&request).is_err());
    }

    /// Verifies an explicitly present idempotency key cannot collapse to an empty identity.
    /// 验证明示存在的幂等键不能折叠为空身份。
    #[test]
    fn optional_idempotency_key_rejects_empty_text() {
        validate_optional_idempotency_key(None).expect("missing key is valid");
        validate_optional_idempotency_key(Some("request-key")).expect("non-empty key is valid");
        let error = validate_optional_idempotency_key(Some(" \t "))
            .expect_err("whitespace key must be rejected");
        assert!(error.contains("idempotency_key"));
    }

    /// Verifies restricted LLM JSON cannot carry tools, continuation, or route overrides.
    /// 验证受限 LLM JSON 无法携带工具、延续状态或路由覆盖。
    #[test]
    fn restricted_llm_request_rejects_expansive_fields() {
        let error = serde_json::from_value::<ResponsesExecutionRequest>(serde_json::json!({
            "contract_version": INFERENCE_CONTRACT_VERSION,
            "request_id": "request-1",
            "consumer_profile_id": "vmm",
            "messages": [],
            "tools": [],
            "model_id": "untrusted-model"
        }))
        .expect_err("unknown expansive fields must be rejected");

        assert!(error.to_string().contains("unknown field"));
    }

    /// Verifies the first contract version round-trips for embedding requests.
    /// 验证首版契约可往返序列化向量请求。
    #[test]
    fn embedding_request_round_trips_with_exact_version() {
        let request = EmbeddingRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: String::from("request-1"),
            idempotency_key: None,
            consumer_profile_id: String::from("vmm"),
            inputs: vec![EmbeddingInput {
                input_id: String::from("chunk-1"),
                content: EmbeddingInputContent::Text {
                    text: String::from("hello"),
                },
            }],
            task: EmbeddingTask::Document,
            dimensions: Some(1_024),
            output_kind: EmbeddingOutputKind::Dense,
            encoding: EmbeddingEncoding::Float,
            invalid_input_policy: EmbeddingInvalidInputPolicy::DropProviderConfirmedItems,
        };
        let encoded = serde_json::to_value(&request).expect("request should serialize");
        let decoded = serde_json::from_value::<EmbeddingRequest>(encoded)
            .expect("request should deserialize");

        assert_eq!(decoded, request);
    }

    /// Verifies the declared template wraps text inputs of its task only, and malformed templates are rejected.
    /// 验证已声明模板只包裹其任务的文本输入，且格式错误的模板会被拒绝。
    #[test]
    fn embedding_input_template_wraps_only_its_task() {
        let mut capabilities = EmbeddingCapabilities {
            tasks: BTreeSet::from([EmbeddingTask::Query, EmbeddingTask::Document]),
            input_resources: BTreeSet::new(),
            output_kinds: BTreeSet::from([EmbeddingOutputKind::Dense]),
            encodings: BTreeSet::from([EmbeddingEncoding::Float]),
            dimensions: BTreeSet::from([4]),
            minimum_dimensions: None,
            maximum_dimensions: None,
            default_dimensions: Some(4),
            maximum_inputs: None,
            maximum_tokens_per_input: None,
            normalization: EmbeddingNormalization::Unknown,
            distance_metric: DistanceMetric::Unknown,
            input_templates: BTreeMap::from([(
                EmbeddingTask::Query,
                String::from("Instruct: find\nQuery:{input}"),
            )]),
        };
        capabilities
            .validate_input_templates()
            .expect("query template is valid");
        // The input itself may contain the placeholder text; only the template's own one is replaced.
        // 输入本身可以包含占位符文本；只替换模板自身的占位符。
        let request = |task| EmbeddingRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: String::from("request-template"),
            idempotency_key: None,
            consumer_profile_id: String::from("template-test-consumer"),
            inputs: vec![EmbeddingInput {
                input_id: String::from("input-1"),
                content: EmbeddingInputContent::Text {
                    text: String::from("what is {input}"),
                },
            }],
            task,
            dimensions: Some(4),
            output_kind: EmbeddingOutputKind::Dense,
            encoding: EmbeddingEncoding::Float,
            invalid_input_policy: EmbeddingInvalidInputPolicy::FailBatch,
        };
        let query = request(EmbeddingTask::Query);
        let templated = capabilities.apply_input_template(&query);
        assert_eq!(
            templated.inputs[0].content,
            EmbeddingInputContent::Text {
                text: String::from("Instruct: find\nQuery:what is {input}")
            }
        );
        let document = request(EmbeddingTask::Document);
        assert!(matches!(
            capabilities.apply_input_template(&document),
            std::borrow::Cow::Borrowed(_)
        ));
        for template in ["no placeholder", "{input} {input}"] {
            capabilities
                .input_templates
                .insert(EmbeddingTask::Query, String::from(template));
            assert!(
                capabilities.validate_input_templates().is_err(),
                "{template}"
            );
        }
        capabilities.input_templates =
            BTreeMap::from([(EmbeddingTask::Clustering, String::from("Cluster: {input}"))]);
        assert!(capabilities.validate_input_templates().is_err());
    }

    /// Verifies Batch aggregate size is governed by Batch limits rather than the synchronous request limit.
    /// 验证 Batch 聚合数量由 Batch 限制约束，而不是由同步请求条数限制约束。
    #[test]
    fn embedding_batch_validation_does_not_apply_synchronous_input_count() {
        // capabilities deliberately permits one input per synchronous call while retaining two valid Batch items.
        // capabilities 有意只允许同步调用包含一个输入，同时保留两个有效 Batch 条目。
        let capabilities = EmbeddingCapabilities {
            tasks: BTreeSet::from([EmbeddingTask::Document]),
            input_resources: BTreeSet::new(),
            output_kinds: BTreeSet::from([EmbeddingOutputKind::Dense]),
            encodings: BTreeSet::from([EmbeddingEncoding::Float]),
            dimensions: BTreeSet::from([1_024]),
            minimum_dimensions: None,
            maximum_dimensions: None,
            default_dimensions: Some(1_024),
            maximum_inputs: Some(1),
            maximum_tokens_per_input: None,
            normalization: EmbeddingNormalization::Unknown,
            distance_metric: DistanceMetric::Unknown,
            input_templates: BTreeMap::new(),
        };
        // request represents two independent single-input requests in the asynchronous provider file.
        // request 表示异步供应商文件中的两个独立单输入请求。
        let request = EmbeddingRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: String::from("request-batch"),
            idempotency_key: None,
            consumer_profile_id: String::from("batch-test-consumer"),
            inputs: vec![
                EmbeddingInput {
                    input_id: String::from("chunk-1"),
                    content: EmbeddingInputContent::Text {
                        text: String::from("alpha"),
                    },
                },
                EmbeddingInput {
                    input_id: String::from("chunk-2"),
                    content: EmbeddingInputContent::Text {
                        text: String::from("beta"),
                    },
                },
            ],
            task: EmbeddingTask::Document,
            dimensions: Some(1_024),
            output_kind: EmbeddingOutputKind::Dense,
            encoding: EmbeddingEncoding::Float,
            invalid_input_policy: EmbeddingInvalidInputPolicy::FailBatch,
        };

        assert!(capabilities.validate_request(&request).is_err());
        capabilities
            .validate_batch_request(&request)
            .expect("Batch aggregate must not inherit the synchronous item-count limit");
    }

    /// Builds one valid file-backed Batch capability for contract boundary tests.
    /// 为契约边界测试构建一份有效的文件型 Batch 能力。
    fn file_batch_capability_fixture() -> EmbeddingBatchCapabilities {
        EmbeddingBatchCapabilities {
            transport: InferenceBatchTransport::JsonlFile,
            result_delivery: InferenceBatchResultDelivery::JsonlFiles,
            provider_maximum_requests: Some(50_000),
            provider_maximum_payload_bytes: Some(500 * 1024 * 1024),
            provider_maximum_item_bytes: Some(6 * 1024 * 1024),
            provider_maximum_files: Some(10_000),
            provider_maximum_total_file_bytes: Some(100 * 1024 * 1024 * 1024),
            provider_file_upload_qps: Some(3),
            provider_file_operation_qps: Some(10),
            provider_maximum_parallel_tasks: Some(1_000),
            provider_task_create_rpm: Some(1_000),
            provider_task_status_rpm: Some(1_000),
            provider_task_list_rpm: Some(100),
            provider_task_cancel_rpm: Some(1_000),
            host_policy_maximum_tasks_per_run: 1_000,
            host_policy_maximum_requests: 50_000,
            host_policy_maximum_payload_bytes: 500 * 1024 * 1024,
            host_policy_maximum_result_bytes: 256 * 1024 * 1024,
            provider_completion_window_seconds: Some(86_400),
            minimum_poll_interval_ms: 60_000,
            supports_cancel: true,
            deletes_input_file_after_ingestion: true,
        }
    }

    /// Verifies file Batch providers may omit optional cancellation and cleanup operations.
    /// 验证文件型 Batch 供应商可以不提供可选的取消与清理操作。
    #[test]
    fn file_batch_capability_allows_optional_cancel_and_cleanup() {
        let mut capability = file_batch_capability_fixture();
        capability.supports_cancel = false;
        capability.provider_task_cancel_rpm = None;
        capability.deletes_input_file_after_ingestion = false;

        capability
            .validate()
            .expect("optional lifecycle operations should not be mandatory");
    }

    /// Verifies a cancellation rate cannot be advertised without a cancellation operation.
    /// 验证未提供取消操作时不能声明取消接口速率。
    #[test]
    fn batch_capability_rejects_orphan_cancel_rate() {
        let mut capability = file_batch_capability_fixture();
        capability.supports_cancel = false;

        let error = capability
            .validate()
            .expect_err("orphan cancellation rate must be rejected");
        assert!(error.contains("cancellation rate"));
    }

    /// Verifies the provider-neutral contract does not require Alibaba-specific published limits.
    /// 验证供应商无关契约不会强制要求阿里云特有的公开限制。
    #[test]
    fn file_batch_capability_allows_unpublished_provider_limits() {
        let mut capability = file_batch_capability_fixture();
        capability.provider_maximum_requests = None;
        capability.provider_maximum_payload_bytes = None;
        capability.provider_maximum_item_bytes = None;
        capability.provider_maximum_files = None;
        capability.provider_maximum_total_file_bytes = None;
        capability.provider_file_upload_qps = None;
        capability.provider_file_operation_qps = None;
        capability.provider_maximum_parallel_tasks = None;
        capability.provider_task_create_rpm = None;
        capability.provider_task_status_rpm = None;
        capability.provider_task_list_rpm = None;
        capability.provider_task_cancel_rpm = None;
        capability.supports_cancel = false;

        capability
            .validate()
            .expect("host policies must bound providers with unpublished limits");
    }

    /// Verifies host policy may safely lower complete payload size below the provider item ceiling.
    /// 验证宿主策略可以安全地把完整负载大小收紧到供应商单项上限以下。
    #[test]
    fn batch_capability_allows_host_payload_limit_below_provider_item_limit() {
        let mut capability = file_batch_capability_fixture();
        capability.host_policy_maximum_payload_bytes = 512 * 1024;

        capability
            .validate()
            .expect("host payload policy should be independently stricter");
    }

    /// Verifies a provider item ceiling cannot exceed that provider's complete payload ceiling.
    /// 验证供应商单项上限不能超过同一供应商的完整负载上限。
    #[test]
    fn batch_capability_rejects_provider_item_limit_above_provider_payload_limit() {
        let mut capability = file_batch_capability_fixture();
        capability.provider_maximum_payload_bytes = Some(1024);
        capability.provider_maximum_item_bytes = Some(1025);

        let error = capability
            .validate()
            .expect_err("provider limits must remain internally coherent");
        assert!(error.contains("provider item limit"));
    }

    /// Verifies completion windows remain serializable by the current whole-hour wire contract.
    /// 验证完成窗口可由当前整小时线协议序列化。
    #[test]
    fn batch_capability_rejects_partial_hour_completion_window() {
        let mut capability = file_batch_capability_fixture();
        capability.provider_completion_window_seconds = Some(86_401);

        let error = capability
            .validate()
            .expect_err("partial-hour completion windows are not serializable");
        assert!(error.contains("complete hours"));
    }

    /// Verifies configured rolling limits cannot silently disable every request.
    /// 验证已配置滚动限额不能静默禁用全部请求。
    #[test]
    fn caller_budget_rejects_zero_rate_limits() {
        let caller = InferenceCallerContext {
            // Preserve this fixture's existing usage-scope scheduling.
            // 保留此夹具既有的用途范围调度。
            scheduling_class: InferenceSchedulingClass::ByUsageScope,
            caller_id: String::from("vmm"),
            allowed_consumer_profile_ids: BTreeSet::from([String::from("vmm")]),
            permissions: BTreeSet::from([InferencePermission::Embedding]),
            usage_scope: InferenceUsageScope::InternalBackground,
            interactive_session_id: None,
            owner_session_id: None,
            deadline_unix_ms: None,
            budget: InferenceCallerBudget {
                max_concurrent_executions: 1,
                max_input_units_per_request: Some(1_024),
                max_output_tokens_per_request: None,
                max_requests_per_minute: Some(0),
                max_input_units_per_minute: Some(1_024),
                max_execution_duration_ms: Some(30_000),
            },
        };

        assert!(
            caller
                .authorize("vmm", InferencePermission::Embedding)
                .expect_err("zero request-rate limit must be rejected")
                .contains("request-rate")
        );
    }
}
