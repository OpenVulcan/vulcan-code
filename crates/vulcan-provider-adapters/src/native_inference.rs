//! Provider-native non-streaming embedding, rerank and decision execution.
//! 供应商原生非流式向量、重排与决策执行。

use crate::{
    ConnectionModel, CredentialExecutionSecret, ProviderExecutionClient, ProviderExecutionFailure,
    ProviderExecutionRoute,
    execution::{
        execution_headers, http_failure, http_send_transport_failure, parse_retry_after_headers,
        pre_dispatch_failure, read_bounded_response_text, response_headers_protocol_failure,
        transport_failure,
    },
};
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant, SystemTime},
};
use tokio::sync::oneshot;
use vulcan_ai_contract::{
    AiOperation, DecisionRequest, DecisionResponse, EmbeddingEncoding, EmbeddingInputContent,
    EmbeddingOutputKind, EmbeddingPayload, EmbeddingRequest, EmbeddingResponse, EmbeddingResult,
    EmbeddingSpaceFingerprint, EmbeddingTask, ExecutionRouteSnapshot, InferenceUsageScope,
    RequestUsage, RerankProfileFingerprint, RerankRequest, RerankResponse, RerankResult,
    UsageUpdateKind, UsageValue, UsageValueSource,
};
use vulcan_llm_core::{EncodedWireRequest, SystemOneWireRequest, decode_systemone_response};

mod openvulcan;

/// Maximum successful native inference response size retained in memory.
/// 原生推理成功响应允许保留在内存中的最大字节数。
const MAXIMUM_NATIVE_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

/// Returns the exact manifest-selected native action binding for one service operation.
/// 返回一个服务操作对应的精确 Manifest 选择原生动作绑定。
///
/// # Parameters
/// 参数
/// `entry` is the immutable service snapshot and `operation` selects one provider-neutral action.
/// `entry` 是不可变服务快照，`operation` 选择一个供应商无关动作。
///
/// # Returns
/// 返回值
/// The stable action binding when a native adapter exists; otherwise no binding.
/// 原生适配器存在时返回稳定动作绑定，否则不返回绑定。
pub fn registered_native_action_binding_id<'a>(
    entry: &'a crate::ServiceEntry,
    operation: AiOperation,
    model_id: &str,
) -> Option<&'a str> {
    select_native_binding(&entry.native_inference_bindings, operation, model_id)
        .map(|binding| binding.binding_id.as_str())
}

/// Returns the exact asynchronous embedding Batch capability selected by one model route.
/// 返回一条模型路由精确选择的异步向量 Batch 能力。
///
/// # Parameters
/// 参数
/// `entry` is the immutable service snapshot and `model_id` is the exact public model identifier.
/// `entry` 是不可变服务快照，`model_id` 是精确公开模型标识。
///
/// # Returns
/// 返回值
/// The declared Batch capability when the selected embedding binding supports it.
/// 所选向量绑定支持 Batch 时返回其已声明能力。
pub fn registered_native_embedding_batch_capabilities<'a>(
    entry: &'a crate::ServiceEntry,
    model_id: &str,
) -> Option<&'a vulcan_ai_contract::EmbeddingBatchCapabilities> {
    select_native_batch_binding(
        &entry.native_inference_bindings,
        AiOperation::EmbeddingCreate,
        model_id,
    )
    .and_then(|binding| binding.batch.as_ref())
    .map(|batch| &batch.capabilities)
}

/// Selects the most specific manifest binding for one operation and model.
/// 为一个操作和模型选择 Manifest 中最精确的绑定。
///
/// # Parameters
/// 参数
/// `bindings` is the immutable binding list, `operation` is the provider-neutral action, and
/// `model_id` is the exact public model identifier.
/// `bindings` 是不可变绑定列表，`operation` 是供应商无关操作，`model_id` 是精确公开模型标识。
///
/// # Returns
/// 返回值
/// The model-specific binding when declared, otherwise the operation-wide binding.
/// 已声明时返回模型专用绑定，否则返回操作通用绑定。
pub(crate) fn select_native_binding<'a>(
    bindings: &'a [crate::NativeInferenceBinding],
    operation: AiOperation,
    model_id: &str,
) -> Option<&'a crate::NativeInferenceBinding> {
    bindings
        .iter()
        .find(|binding| {
            binding.operation == operation
                && binding.batch.is_none()
                && binding
                    .model_ids
                    .iter()
                    .any(|candidate| candidate == model_id)
        })
        .or_else(|| {
            bindings.iter().find(|binding| {
                binding.operation == operation
                    && binding.batch.is_none()
                    && binding.model_ids.is_empty()
            })
        })
}

/// Selects the exact supplemental Batch binding for one operation and model.
/// 为一个操作和模型选择精确的补充 Batch 绑定。
///
/// # Parameters
/// 参数
/// `bindings` is the immutable binding list, `operation` is the provider-neutral action, and
/// `model_id` is the exact public model identifier.
/// `bindings` 是不可变绑定列表，`operation` 是供应商无关操作，`model_id` 是精确公开模型标识。
///
/// # Returns
/// 返回值
/// The model-specific asynchronous binding when declared; Batch declarations never replace the
/// synchronous action binding selected by [`select_native_binding`].
/// 已声明时返回模型专用异步绑定；Batch 声明绝不会替换 [`select_native_binding`] 选择的同步动作绑定。
pub(crate) fn select_native_batch_binding<'a>(
    bindings: &'a [crate::NativeInferenceBinding],
    operation: AiOperation,
    model_id: &str,
) -> Option<&'a crate::NativeInferenceBinding> {
    bindings.iter().find(|binding| {
        binding.operation == operation
            && binding.batch.is_some()
            && binding
                .model_ids
                .iter()
                .any(|candidate| candidate == model_id)
    })
}

/// Validates a configured action binding against the exact native provider route.
/// 根据精确原生供应商路由校验已配置动作绑定。
///
/// # Parameters
/// 参数
/// `route`, `operation`, `model_id`, and `configured_binding_id` are persisted immutable-route facts.
/// `route`、`operation`、`model_id` 与 `configured_binding_id` 是持久化不可变路由事实。
///
/// # Returns
/// 返回值
/// Success only when the action identifier exactly matches the registered native adapter.
/// 仅当动作标识与已注册原生适配器精确匹配时返回成功。
pub(crate) fn validate_native_action_binding(
    route: &ProviderExecutionRoute,
    operation: AiOperation,
    model_id: &str,
    configured_binding_id: &str,
) -> Result<(), ProviderExecutionFailure> {
    let (_, _, expected_binding_id) = match operation {
        AiOperation::EmbeddingCreate => NativeInferenceRoute::embedding(route, model_id)?,
        AiOperation::RerankDocuments => NativeInferenceRoute::rerank(route, model_id)?,
        AiOperation::DecisionEvaluate => NativeInferenceRoute::decision(route, model_id)?,
        _ => {
            return Err(pre_dispatch_failure(format!(
                "Operation `{}` is not a native inference action.",
                operation.as_str()
            )));
        }
    };
    if configured_binding_id != expected_binding_id {
        return Err(pre_dispatch_failure(format!(
            "Configured action binding `{configured_binding_id}` differs from registered binding `{expected_binding_id}`."
        )));
    }
    Ok(())
}

/// Binds one frozen service route to its manifest-selected native inference contract.
/// 将一条冻结服务路由绑定到其 Manifest 选择的原生推理契约。
#[derive(Debug, Clone, Copy)]
struct NativeInferenceRoute<'a> {
    /// Exact immutable binding declaration.
    /// 精确且不可变的绑定声明。
    binding: &'a crate::NativeInferenceBinding,
}

impl<'a> NativeInferenceRoute<'a> {
    /// Resolves one embedding endpoint from an immutable provider route.
    /// 从不可变供应商路由解析一个向量端点。
    ///
    /// # Parameters
    /// 参数
    /// `route` contains the exact provider and product selected before dispatch, and `model_id`
    /// selects a model-specific binding when the manifest declares one.
    /// `route` 包含分派前选择的精确供应商与产品，`model_id` 在 Manifest 声明时选择模型专用绑定。
    ///
    /// # Returns
    /// 返回值
    /// The wire route, relative endpoint, and stable binding identifier.
    /// 返回 Wire 路由、相对端点与稳定绑定标识。
    fn embedding(
        route: &'a ProviderExecutionRoute,
        model_id: &str,
    ) -> Result<(Self, &'a str, &'a str), ProviderExecutionFailure> {
        Self::for_operation(route, AiOperation::EmbeddingCreate, model_id)
    }

    /// Resolves one rerank endpoint from an immutable provider route.
    /// 从不可变供应商路由解析一个重排端点。
    ///
    /// # Parameters
    /// 参数
    /// `route` contains the exact provider and product selected before dispatch, and `model_id`
    /// selects a model-specific binding when the manifest declares one.
    /// `route` 包含分派前选择的精确供应商与产品，`model_id` 在 Manifest 声明时选择模型专用绑定。
    ///
    /// # Returns
    /// 返回值
    /// The wire route, relative endpoint, and stable binding identifier.
    /// 返回 Wire 路由、相对端点与稳定绑定标识。
    fn rerank(
        route: &'a ProviderExecutionRoute,
        model_id: &str,
    ) -> Result<(Self, &'a str, &'a str), ProviderExecutionFailure> {
        Self::for_operation(route, AiOperation::RerankDocuments, model_id)
    }

    /// Resolves one decision endpoint from an immutable provider route; only the System One
    /// projection can carry it.
    /// 从不可变供应商路由解析一个决策端点；只有 System One 投影可以承载它。
    ///
    /// # Returns
    /// 返回值
    /// The wire route, relative endpoint, and stable binding identifier.
    /// 返回 Wire 路由、相对端点与稳定绑定标识。
    fn decision(
        route: &'a ProviderExecutionRoute,
        model_id: &str,
    ) -> Result<(Self, &'a str, &'a str), ProviderExecutionFailure> {
        let resolved = Self::for_operation(route, AiOperation::DecisionEvaluate, model_id)?;
        if !matches!(
            resolved.0.binding.projection,
            crate::NativeInferenceProjectionKind::SystemOne
                | crate::NativeInferenceProjectionKind::OpenVulcan
        ) {
            return Err(pre_dispatch_failure(format!(
                "Decision action `{}` does not use the System One projection.",
                resolved.2
            )));
        }
        Ok(resolved)
    }

    /// Resolves one operation binding from the frozen service entry.
    /// 从冻结服务入口解析一个操作绑定。
    fn for_operation(
        route: &'a ProviderExecutionRoute,
        operation: AiOperation,
        model_id: &str,
    ) -> Result<(Self, &'a str, &'a str), ProviderExecutionFailure> {
        let binding =
            select_native_binding(&route.entry.native_inference_bindings, operation, model_id)
                .ok_or_else(|| {
                    pre_dispatch_failure(format!(
                        "Provider route `{}/{}` does not register native operation `{}`.",
                        route.provider.id,
                        route.entry.id,
                        operation.as_str()
                    ))
                })?;
        Ok((
            Self { binding },
            binding.endpoint_path.as_str(),
            binding.binding_id.as_str(),
        ))
    }
}

/// Carries OpenRouter's explicit no-fallback routing policy.
/// 承载 OpenRouter 明确禁用回退的路由策略。
#[derive(Debug, Serialize)]
struct OpenRouterProviderPolicy {
    /// Optional exact provider endpoint order selected from the cached endpoint catalog.
    /// 从缓存端点目录选择的可选精确供应商端点顺序。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    order: Vec<String>,
    /// Disables provider fallback for one frozen model target.
    /// 为一个冻结模型目标禁用供应商回退。
    allow_fallbacks: bool,
    /// Avoids inventing an endpoint-parameter requirement.
    /// 避免虚构端点参数要求。
    require_parameters: bool,
}

/// Carries one provider-native embedding request.
/// 承载一次供应商原生向量请求。
#[derive(Debug, Serialize)]
struct WireEmbeddingRequest {
    /// Exact immutable model identifier.
    /// 精确不可变模型标识。
    model: String,
    /// Ordered text inputs.
    /// 有序文本输入。
    input: Vec<String>,
    /// Optional exact target dimensions.
    /// 可选的精确目标维度。
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<u32>,
    /// Exact documented output encoding.
    /// 精确的文档化输出编码。
    encoding_format: String,
    /// Optional OpenRouter semantic input type.
    /// 可选 OpenRouter 语义输入类型。
    #[serde(skip_serializing_if = "Option::is_none")]
    input_type: Option<String>,
    /// Optional OpenRouter provider routing policy.
    /// 可选 OpenRouter 供应商路由策略。
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<OpenRouterProviderPolicy>,
}

/// Carries one provider-native rerank request.
/// 承载一次供应商原生重排请求。
#[derive(Debug, Serialize)]
struct WireRerankRequest {
    /// Exact immutable model identifier.
    /// 精确不可变模型标识。
    model: String,
    /// Exact text query.
    /// 精确文本查询。
    query: String,
    /// Ordered candidate texts.
    /// 有序候选文本。
    documents: Vec<String>,
    /// Exact maximum result count.
    /// 精确最大结果数量。
    top_n: u32,
    /// Optional OpenRouter provider routing policy.
    /// 可选 OpenRouter 供应商路由策略。
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<OpenRouterProviderPolicy>,
}

/// Carries Alibaba Model Studio's nested text-rerank request.
/// 承载阿里云百炼嵌套格式的文本重排请求。
#[derive(Debug, Serialize)]
struct WireAlibabaTextRerankRequest {
    /// Exact immutable model identifier.
    /// 精确不可变模型标识。
    model: String,
    /// Nested query and candidate inputs required by Alibaba's native API.
    /// 阿里云原生接口要求的嵌套查询与候选输入。
    input: WireAlibabaTextRerankInput,
    /// Native rerank controls accepted by the documented endpoint.
    /// 文档端点接受的原生重排控制参数。
    parameters: WireAlibabaTextRerankParameters,
}

/// Carries Alibaba's nested rerank input object.
/// 承载阿里云嵌套格式的重排输入对象。
#[derive(Debug, Serialize)]
struct WireAlibabaTextRerankInput {
    /// Exact text query.
    /// 精确文本查询。
    query: String,
    /// Ordered candidate texts.
    /// 有序候选文本。
    documents: Vec<String>,
}

/// Carries Alibaba's documented rerank parameters.
/// 承载阿里云文档规定的重排参数。
#[derive(Debug, Serialize)]
struct WireAlibabaTextRerankParameters {
    /// Exact maximum result count.
    /// 精确最大结果数量。
    top_n: u32,
}

/// Selects the exact serialized wire body for one rerank binding.
/// 为一条重排绑定选择精确的序列化 Wire 请求体。
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum WireRerankRequestBody {
    /// OpenAI-compatible or routed-compatible request body.
    /// OpenAI 兼容或带路由兼容请求体。
    Compatible(WireRerankRequest),
    /// Alibaba Model Studio native nested request body.
    /// 阿里云百炼原生嵌套请求体。
    AlibabaTextRerank(WireAlibabaTextRerankRequest),
}

/// Closed embedding value union returned by compatible providers.
/// 兼容供应商返回的封闭向量值联合。
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum WireEmbeddingValue {
    /// Dense numeric coordinates.
    /// 稠密数值坐标。
    Dense(Vec<f64>),
    /// Provider-declared Base64 coordinates.
    /// 供应商声明的 Base64 坐标。
    Base64(String),
}

/// Carries one indexed wire embedding result.
/// 承载一项带索引的 Wire 向量结果。
#[derive(Debug, Deserialize)]
struct WireEmbeddingItem {
    /// Original zero-based input index.
    /// 原始从零开始的输入索引。
    index: usize,
    /// Closed embedding representation.
    /// 封闭向量表示。
    embedding: WireEmbeddingValue,
}

/// Carries documented token fields shared by native responses.
/// 承载原生响应共享的文档化 Token 字段。
#[derive(Debug, Default, Deserialize)]
struct WireUsage {
    /// Provider-reported prompt tokens.
    /// 供应商报告的 Prompt Token。
    #[serde(default)]
    prompt_tokens: Option<u64>,
    /// Provider-reported input tokens.
    /// 供应商报告的输入 Token。
    #[serde(default)]
    input_tokens: Option<u64>,
    /// Provider-reported total tokens.
    /// 供应商报告的总 Token。
    #[serde(default)]
    total_tokens: Option<u64>,
}

/// Carries one compatible embedding response.
/// 承载一次兼容向量响应。
#[derive(Debug, Deserialize)]
struct WireEmbeddingResponse {
    /// Optional provider response identifier.
    /// 可选供应商响应标识。
    #[serde(default)]
    #[serde(rename = "id")]
    _id: Option<String>,
    /// Optional provider-reported model identity.
    /// 可选供应商报告模型身份。
    #[serde(default)]
    model: Option<String>,
    /// Indexed embedding values.
    /// 带索引的向量值。
    data: Vec<WireEmbeddingItem>,
    /// Optional provider usage.
    /// 可选供应商用量。
    #[serde(default)]
    usage: WireUsage,
}

/// Carries one provider-ranked rerank result.
/// 承载一项供应商排序后的重排结果。
#[derive(Debug, Deserialize)]
struct WireRerankItem {
    /// Original zero-based candidate index.
    /// 原始从零开始的候选索引。
    index: usize,
    /// Unmodified provider score.
    /// 未修改的供应商分数。
    relevance_score: f64,
}

/// Carries one compatible rerank response.
/// 承载一次兼容重排响应。
#[derive(Debug, Deserialize)]
struct WireRerankResponse {
    /// Optional provider response identifier.
    /// 可选供应商响应标识。
    #[serde(default)]
    #[serde(rename = "id")]
    _id: Option<String>,
    /// Optional provider-reported model identity.
    /// 可选供应商报告模型身份。
    #[serde(default)]
    model: Option<String>,
    /// Provider-ranked results.
    /// 供应商排序后的结果。
    results: Vec<WireRerankItem>,
    /// Optional provider usage.
    /// 可选供应商用量。
    #[serde(default)]
    usage: WireUsage,
}

/// Carries Alibaba Model Studio's nested text-rerank response.
/// 承载阿里云百炼嵌套格式的文本重排响应。
#[derive(Debug, Deserialize)]
struct WireAlibabaTextRerankResponse {
    /// Nested provider-ranked results.
    /// 嵌套的供应商排序结果。
    output: WireAlibabaTextRerankOutput,
    /// Optional provider usage.
    /// 可选供应商用量。
    #[serde(default)]
    usage: WireUsage,
}

/// Carries Alibaba's nested result object.
/// 承载阿里云嵌套格式的结果对象。
#[derive(Debug, Deserialize)]
struct WireAlibabaTextRerankOutput {
    /// Provider-ranked results.
    /// 供应商排序后的结果。
    results: Vec<WireRerankItem>,
}

/// Borrows the two immutable revisions frozen for one provider execution.
/// 借用一次供应商执行冻结的两个不可变修订号。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExecutionRevisionRefs<'a> {
    /// Runtime model-catalog revision.
    /// 运行时模型目录修订号。
    pub catalog: &'a str,
    /// Content-addressed provider-route revision.
    /// 内容寻址供应商路由修订号。
    pub route: &'a str,
}

impl ProviderExecutionClient {
    /// Executes one frozen native embedding route and validates complete batch identity.
    /// 执行一条冻结的原生向量路由并校验完整批次身份。
    ///
    /// # Parameters
    /// 参数
    /// `route`, `model`, and `catalog_revision` are immutable resolution facts; `request`,
    /// `usage_scope`, and `secret` are authorized execution inputs.
    /// `route`、`model` 与 `catalog_revision` 是不可变解析事实；`request`、
    /// `usage_scope` 与 `secret` 是已授权执行输入。
    ///
    /// # Returns
    /// 返回值
    /// A normalized all-or-nothing embedding response or a structured failure.
    /// 返回归一化的整批成功向量响应或结构化失败。
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute_embedding(
        &self,
        route: &ProviderExecutionRoute,
        model: &ConnectionModel,
        request: &EmbeddingRequest,
        usage_scope: InferenceUsageScope,
        revisions: ExecutionRevisionRefs<'_>,
        secret: &CredentialExecutionSecret,
        wire_request_capture: Option<oneshot::Sender<crate::execution::ProviderWireRequestCapture>>,
    ) -> Result<EmbeddingResponse, ProviderExecutionFailure> {
        let started_at = Instant::now();
        request.validate().map_err(pre_dispatch_failure)?;
        let capabilities = model.capabilities.embedding.as_ref().ok_or_else(|| {
            pre_dispatch_failure(format!(
                "Model `{}` does not expose an embedding contract.",
                model.id
            ))
        })?;
        capabilities
            .validate_request(request)
            .map_err(pre_dispatch_failure)?;
        // The model's declared input template wraps every input before any wire projection.
        // 模型声明的输入模板在任何线协议投影之前包裹每条输入。
        capabilities
            .validate_input_templates()
            .map_err(pre_dispatch_failure)?;
        let templated = capabilities.apply_input_template(request);
        let request: &EmbeddingRequest = &templated;
        let (native_route, endpoint, binding_id) =
            NativeInferenceRoute::embedding(route, model.id.as_str())?;
        if native_route.binding.projection == crate::NativeInferenceProjectionKind::OpenVulcan {
            return self
                .execute_openvulcan_embedding(
                    route,
                    model,
                    request,
                    usage_scope,
                    revisions,
                    secret,
                    wire_request_capture,
                )
                .await;
        }
        let wire_request = project_embedding_request(
            native_route,
            &route.secondary_route,
            model.id.as_str(),
            request,
        )?;
        let response = self
            .send_native_json(
                native_route,
                route,
                endpoint,
                &wire_request,
                secret,
                wire_request_capture,
            )
            .await?;
        let wire: WireEmbeddingResponse =
            serde_json::from_str(response.as_str()).map_err(|error| {
                response_headers_protocol_failure(format!(
                    "Native embedding response is invalid JSON: {error}"
                ))
            })?;
        validate_response_model(native_route, model.id.as_str(), wire.model.as_deref())?;
        if wire.data.len() != request.inputs.len() {
            return Err(response_headers_protocol_failure(format!(
                "Embedding output count {} differs from input count {}.",
                wire.data.len(),
                request.inputs.len()
            )));
        }
        let mut indexed = wire
            .data
            .into_iter()
            .map(|item| (item.index, item.embedding))
            .collect::<BTreeMap<_, _>>();
        if indexed.len() != request.inputs.len() {
            return Err(response_headers_protocol_failure(
                "Embedding response contains duplicate or missing indexes.",
            ));
        }
        let mut results = Vec::with_capacity(request.inputs.len());
        let mut actual_dimensions = None;
        for (input_index, input) in request.inputs.iter().enumerate() {
            let value = indexed.remove(&input_index).ok_or_else(|| {
                response_headers_protocol_failure(format!(
                    "Embedding response is missing input index {input_index}."
                ))
            })?;
            let (embedding, dimensions) =
                normalize_embedding_value(request.encoding, request.dimensions, value)?;
            if actual_dimensions
                .replace(dimensions)
                .is_some_and(|known| known != dimensions)
            {
                return Err(response_headers_protocol_failure(
                    "Embedding response contains inconsistent vector dimensions.",
                ));
            }
            results.push(EmbeddingResult {
                input_id: input.input_id.clone(),
                input_index: input_index as u32,
                embedding,
            });
        }
        let dimensions = actual_dimensions.ok_or_else(|| {
            response_headers_protocol_failure("Embedding response contains no vectors.")
        })?;
        Ok(EmbeddingResponse {
            request_id: request.request_id.clone(),
            route: execution_route_snapshot(
                route,
                model,
                usage_scope,
                revisions.catalog,
                revisions.route,
                Some(binding_id),
            ),
            fingerprint: EmbeddingSpaceFingerprint {
                provider_id: route.provider.id.clone(),
                service_entry_id: route.entry.id.clone(),
                // A credential is not a deployment: every account of one service entry runs the same model.
                // 凭据不是部署：同一服务入口的每个账号运行同一模型。
                deployment_id: None,
                secondary_route_id: secondary_route_id(&route.secondary_route),
                model_id: model.id.clone(),
                binding_id: binding_id.to_string(),
                capability_revision: model.capabilities.evidence_revision.clone(),
                task: request.task,
                dimensions,
                output_kind: request.output_kind,
                encoding: request.encoding,
                normalization: capabilities.normalization,
                distance_metric: capabilities.distance_metric,
            },
            results,
            dropped: Vec::new(),
            usage: normalized_usage(wire.usage),
            duration_ms: elapsed_milliseconds(started_at),
        })
    }

    /// Executes one frozen native rerank route and preserves original candidate identity.
    /// 执行一条冻结的原生重排路由并保留原始候选身份。
    ///
    /// # Parameters
    /// 参数
    /// `route`, `model`, and `catalog_revision` are immutable resolution facts; `request`,
    /// `usage_scope`, and `secret` are authorized execution inputs.
    /// `route`、`model` 与 `catalog_revision` 是不可变解析事实；`request`、
    /// `usage_scope` 与 `secret` 是已授权执行输入。
    ///
    /// # Returns
    /// 返回值
    /// A normalized ranked response or a structured provider/contract failure.
    /// 返回归一化排序响应或结构化供应商与契约失败。
    pub(crate) async fn execute_rerank(
        &self,
        route: &ProviderExecutionRoute,
        model: &ConnectionModel,
        request: &RerankRequest,
        usage_scope: InferenceUsageScope,
        revisions: ExecutionRevisionRefs<'_>,
        secret: &CredentialExecutionSecret,
    ) -> Result<RerankResponse, ProviderExecutionFailure> {
        let started_at = Instant::now();
        request.validate().map_err(pre_dispatch_failure)?;
        let capabilities = model.capabilities.rerank.as_ref().ok_or_else(|| {
            pre_dispatch_failure(format!(
                "Model `{}` does not expose a rerank contract.",
                model.id
            ))
        })?;
        capabilities
            .validate_request(request)
            .map_err(pre_dispatch_failure)?;
        let (native_route, endpoint, binding_id) =
            NativeInferenceRoute::rerank(route, model.id.as_str())?;
        let wire_request = project_rerank_request(
            native_route,
            &route.secondary_route,
            model.id.as_str(),
            request,
        )?;
        let response = self
            .send_native_json(native_route, route, endpoint, &wire_request, secret, None)
            .await
            .map_err(|failure| {
                if native_route.binding.projection
                    == crate::NativeInferenceProjectionKind::OpenVulcan
                {
                    openvulcan::non_replayable_failure(failure)
                } else {
                    failure
                }
            })?;
        let (wire_results, wire_model, wire_usage) =
            parse_rerank_response(native_route, response.as_str())?;
        validate_response_model(native_route, model.id.as_str(), wire_model.as_deref())?;
        if wire_results.is_empty() || wire_results.len() > request.top_n as usize {
            return Err(response_headers_protocol_failure(format!(
                "Rerank response returned {} results for top_n {}.",
                wire_results.len(),
                request.top_n
            )));
        }
        let mut observed_indexes = BTreeSet::new();
        let mut results = Vec::with_capacity(wire_results.len());
        for (rank, item) in wire_results.into_iter().enumerate() {
            if item.index >= request.candidates.len() || !observed_indexes.insert(item.index) {
                return Err(response_headers_protocol_failure(format!(
                    "Rerank response index {} is outside or duplicated in the candidate batch.",
                    item.index
                )));
            }
            if !item.relevance_score.is_finite() {
                return Err(response_headers_protocol_failure(
                    "Rerank response contains a non-finite relevance score.",
                ));
            }
            let candidate = &request.candidates[item.index];
            results.push(RerankResult {
                candidate_id: candidate.id.clone(),
                original_index: item.index as u32,
                rank: rank as u32,
                score: item.relevance_score,
                content: request.return_content.then(|| candidate.text.clone()),
            });
        }
        Ok(RerankResponse {
            request_id: request.request_id.clone(),
            route: execution_route_snapshot(
                route,
                model,
                usage_scope,
                revisions.catalog,
                revisions.route,
                Some(binding_id),
            ),
            fingerprint: RerankProfileFingerprint {
                provider_id: route.provider.id.clone(),
                service_entry_id: route.entry.id.clone(),
                // A credential is not a deployment: every account of one service entry runs the same model.
                // 凭据不是部署：同一服务入口的每个账号运行同一模型。
                deployment_id: None,
                secondary_route_id: secondary_route_id(&route.secondary_route),
                model_id: model.id.clone(),
                binding_id: binding_id.to_string(),
                capability_revision: model.capabilities.evidence_revision.clone(),
                score_semantics: capabilities.score_semantics,
            },
            results,
            usage: normalized_usage(wire_usage),
            duration_ms: elapsed_milliseconds(started_at),
        })
    }

    /// Executes one frozen System One decision route and checks every answer against its question.
    /// 执行一条冻结的 System One 决策路由，并按问题核对每个回答。
    ///
    /// # Parameters
    /// 参数
    /// `route`, `model`, and `revisions` are immutable resolution facts; `request`, `usage_scope`,
    /// and `secret` are authorized execution inputs.
    /// `route`、`model` 与 `revisions` 是不可变解析事实；`request`、`usage_scope` 与 `secret`
    /// 是已授权执行输入。
    ///
    /// # Returns
    /// 返回值
    /// A validated decision response or a structured provider/contract failure.
    /// 返回已校验决策响应或结构化供应商与契约失败。
    pub(crate) async fn execute_decision(
        &self,
        route: &ProviderExecutionRoute,
        model: &ConnectionModel,
        request: &DecisionRequest,
        usage_scope: InferenceUsageScope,
        revisions: ExecutionRevisionRefs<'_>,
        secret: &CredentialExecutionSecret,
    ) -> Result<DecisionResponse, ProviderExecutionFailure> {
        let started_at = Instant::now();
        request.validate().map_err(pre_dispatch_failure)?;
        if !model
            .capabilities
            .operations
            .contains(&AiOperation::DecisionEvaluate)
        {
            return Err(pre_dispatch_failure(format!(
                "Model `{}` does not declare decision_evaluate.",
                model.id
            )));
        }
        let (native_route, endpoint, binding_id) =
            NativeInferenceRoute::decision(route, model.id.as_str())?;
        let systemone_request = SystemOneWireRequest {
            model: model.id.as_str(),
            request,
        };
        // wire_request preserves the declared decision question mapping for both reviewed protocols.
        // wire_request 为两种已审核协议保留声明的判定问题映射。
        let mut wire_request = serde_json::to_value(systemone_request)
            .map_err(|_| pre_dispatch_failure("Failed to encode decision request."))?;
        let wire_request_id = uuid::Uuid::new_v4().to_string();
        if native_route.binding.projection == crate::NativeInferenceProjectionKind::OpenVulcan {
            openvulcan::project_decision(&mut wire_request, request, &wire_request_id)?;
        }
        let response = self
            .send_native_json(native_route, route, endpoint, &wire_request, secret, None)
            .await?;
        let response = if native_route.binding.projection
            == crate::NativeInferenceProjectionKind::OpenVulcan
        {
            openvulcan::decision_response(&response, &wire_request_id, &model.id)?
        } else {
            response
        };
        let wire = decode_systemone_response(response.as_str())
            .map_err(|error| response_headers_protocol_failure(error.message))?;
        validate_response_model(native_route, model.id.as_str(), wire.model.as_deref())?;
        let decision = DecisionResponse {
            request_id: request.request_id.clone(),
            route: execution_route_snapshot(
                route,
                model,
                usage_scope,
                revisions.catalog,
                revisions.route,
                Some(binding_id),
            ),
            answers: wire.answers,
            usage: RequestUsage {
                update_kind: UsageUpdateKind::Final,
                input_tokens: reported_usage_value(wire.usage.input_tokens),
                uncached_input_tokens: reported_usage_value(wire.usage.input_tokens),
                output_tokens: reported_usage_value(wire.usage.output_tokens),
                response_model: wire.model,
                response_id: wire.response_id,
                ..RequestUsage::default()
            },
            duration_ms: elapsed_milliseconds(started_at),
        };
        decision
            .validate_for_request(request)
            .map_err(response_headers_protocol_failure)?;
        Ok(decision)
    }

    /// Sends one bounded provider-native JSON request with existing credential projection.
    /// 使用现有凭据投影发送一次有界供应商原生 JSON 请求。
    ///
    /// # Parameters
    /// 参数
    /// `route`, `endpoint`, `body`, and `secret` form one immutable dispatch.
    /// `route`、`endpoint`、`body` 与 `secret` 构成一次不可变分派。
    ///
    /// # Returns
    /// 返回值
    /// Complete bounded UTF-8 JSON text or a structured execution failure.
    /// 返回完整有界 UTF-8 JSON 文本或结构化执行失败。
    async fn send_native_json<T: Serialize>(
        &self,
        native_route: NativeInferenceRoute<'_>,
        route: &ProviderExecutionRoute,
        endpoint: &str,
        body: &T,
        secret: &CredentialExecutionSecret,
        wire_request_capture: Option<oneshot::Sender<crate::execution::ProviderWireRequestCapture>>,
    ) -> Result<String, ProviderExecutionFailure> {
        let transport = self
            .protocols
            .transport_profile(route.protocol_route.protocol_profile_id.as_str())
            .map_err(|error| {
                pre_dispatch_failure(format!(
                    "Native inference transport profile is unavailable: {error}"
                ))
            })?;
        let headers = execution_headers(route, &BTreeMap::new(), secret, None)?;
        let base_url = crate::connection_endpoint::resolved_connection_protocol_base_url(
            &route.connection,
            &route.entry,
            &route.protocol_route,
        );
        let url = native_request_url(
            native_route,
            route.connection.service_origin_override.as_deref(),
            base_url.as_str(),
            endpoint,
        )?;
        if let Some(sender) = wire_request_capture {
            let encoded = EncodedWireRequest {
                path: endpoint.to_string(),
                headers: BTreeMap::new(),
                body: serde_json::to_value(body).map_err(|error| {
                    pre_dispatch_failure(format!(
                        "Native inference request body could not be captured: {error}"
                    ))
                })?,
            };
            let capture = crate::execution::provider_wire_request_capture(
                "http_json",
                "POST",
                url.as_str(),
                &encoded,
                &headers,
            );
            let _ = sender.send(capture);
        }
        let response = tokio::time::timeout(
            Duration::from_millis(transport.response_headers_timeout_ms),
            self.http
                .post(url.as_str())
                .headers(headers)
                .json(body)
                .send(),
        )
        .await
        .map_err(|_| {
            transport_failure(format!(
                "Native inference request to `{url}` exceeded the registered {} ms response-header timeout.",
                transport.response_headers_timeout_ms
            ))
        })?
        .map_err(|error| {
            http_send_transport_failure(
                &error,
                format!("Native inference request to `{url}` failed: {error}"),
            )
        })?;
        let status = response.status().as_u16();
        let retry_after = parse_retry_after_headers(response.headers(), SystemTime::now());
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let response_body = read_bounded_response_text(
            response,
            MAXIMUM_NATIVE_RESPONSE_BYTES,
            url.as_str(),
            status,
            retry_after.clone(),
            Duration::from_millis(transport.stream_idle_timeout_ms),
        )
        .await?;
        if !(200..300).contains(&status) {
            return Err(crate::execution::sanitize_provider_execution_failure(
                http_failure(
                    status,
                    retry_after,
                    crate::execution::redact_exact_credential(response_body.as_str(), secret)
                        .as_str(),
                ),
                secret,
            ));
        }
        if !content_type
            .as_deref()
            .is_some_and(|value| value.to_ascii_lowercase().starts_with("application/json"))
        {
            return Err(response_headers_protocol_failure(format!(
                "Native inference response from `{url}` used unsupported Content-Type `{}`.",
                content_type.as_deref().unwrap_or("<missing>")
            )));
        }
        let response_value =
            serde_json::from_str::<serde_json::Value>(&response_body).map_err(|error| {
                response_headers_protocol_failure(format!(
                    "Native inference response from `{url}` is invalid JSON: {error}"
                ))
            })?;
        crate::provider_manifest::validate_runtime_json_bounds(
            &response_value,
            "Native inference response",
        )
        .map_err(response_headers_protocol_failure)?;
        Ok(response_body)
    }
}

/// Joins one registered native endpoint to its exact service-entry base URL.
/// 将一个已注册原生端点拼接到其精确服务入口基础 URL。
///
/// # Parameters
/// 参数
/// `native_route` selects manifest-owned URL rules and `service_origin_override` identifies the
/// exact configured service; `base_url` already contains the generic product API root and
/// `endpoint` is relative to it.
/// `native_route` 选择 Manifest 持有的 URL 规则，`service_origin_override` 标识精确配置服务；
/// `base_url` 已包含通用产品 API 根，`endpoint` 相对于该根。
///
/// # Returns
/// 返回值
/// One URL without inventing or duplicating protocol path segments.
/// 返回不虚构或重复协议路径段的 URL。
fn native_request_url(
    native_route: NativeInferenceRoute<'_>,
    service_origin_override: Option<&str>,
    base_url: &str,
    endpoint: &str,
) -> Result<String, ProviderExecutionFailure> {
    if native_route.binding.requires_service_origin_override {
        let origin = service_origin_override
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                pre_dispatch_failure(format!(
                    "Native action `{}` requires a credential-scoped service origin.",
                    native_route.binding.binding_id
                ))
            })?;
        let base_path = native_route
            .binding
            .service_origin_base_path
            .as_deref()
            .ok_or_else(|| {
                pre_dispatch_failure(format!(
                    "Native action `{}` does not declare its required service-origin API root.",
                    native_route.binding.binding_id
                ))
            })?;
        return Ok(format!(
            "{}{}{}",
            origin.trim_end_matches('/'),
            base_path,
            endpoint
        ));
    }
    Ok(format!("{}{}", base_url.trim_end_matches('/'), endpoint))
}

/// Projects one validated public embedding request to an exact provider wire body.
/// 把一项已校验公共向量请求投影为精确供应商 Wire 正文。
///
/// # Parameters
/// 参数
/// `native_route`, `secondary_route`, `model_id`, and `request` are immutable execution facts.
/// `native_route`、`secondary_route`、`model_id` 与 `request` 是不可变执行事实。
///
/// # Returns
/// 返回值
/// A text-only dense request or a controlled pre-dispatch failure.
/// 返回纯文本稠密请求或受控分派前失败。
fn project_embedding_request(
    native_route: NativeInferenceRoute<'_>,
    secondary_route: &crate::SecondaryRouteSelection,
    model_id: &str,
    request: &EmbeddingRequest,
) -> Result<WireEmbeddingRequest, ProviderExecutionFailure> {
    if request.output_kind != EmbeddingOutputKind::Dense {
        return Err(pre_dispatch_failure(
            "Native embedding adapters currently support dense output only.",
        ));
    }
    let mut inputs = Vec::with_capacity(request.inputs.len());
    for input in &request.inputs {
        match &input.content {
            EmbeddingInputContent::Text { text } => inputs.push(text.clone()),
            EmbeddingInputContent::Resource { .. } => {
                return Err(pre_dispatch_failure(
                    "Native embedding adapters currently accept text inputs only.",
                ));
            }
        }
    }
    let encoding_format = match request.encoding {
        EmbeddingEncoding::Float => "float",
        EmbeddingEncoding::Base64 if request.dimensions.is_some() => "base64",
        EmbeddingEncoding::Base64 => {
            return Err(pre_dispatch_failure(
                "Base64 embedding output requires explicit dimensions.",
            ));
        }
    };
    let routed =
        native_route.binding.projection == crate::NativeInferenceProjectionKind::RoutedCompatible;
    let input_type = match (routed, request.task) {
        (_, EmbeddingTask::ProviderDefault) => None,
        (true, EmbeddingTask::Query) => Some(String::from("search_query")),
        (true, EmbeddingTask::Document) => Some(String::from("search_document")),
        _ => {
            return Err(pre_dispatch_failure(format!(
                "Embedding task `{:?}` is not supported by this native route.",
                request.task
            )));
        }
    };
    Ok(WireEmbeddingRequest {
        model: model_id.to_string(),
        input: inputs,
        dimensions: request.dimensions,
        encoding_format: encoding_format.to_string(),
        input_type,
        provider: routed.then(|| routed_provider_policy(secondary_route)),
    })
}

/// Projects one validated public rerank request to an exact provider wire body.
/// 把一项已校验公共重排请求投影为精确供应商 Wire 正文。
///
/// # Parameters
/// 参数
/// `native_route`, `secondary_route`, `model_id`, and `request` are immutable execution facts.
/// `native_route`、`secondary_route`、`model_id` 与 `request` 是不可变执行事实。
///
/// # Returns
/// 返回值
/// A text-only request or a controlled pre-dispatch failure.
/// 返回纯文本请求或受控分派前失败。
fn project_rerank_request(
    native_route: NativeInferenceRoute<'_>,
    secondary_route: &crate::SecondaryRouteSelection,
    model_id: &str,
    request: &RerankRequest,
) -> Result<WireRerankRequestBody, ProviderExecutionFailure> {
    let expected_truncation = native_route.binding.rerank_truncation.ok_or_else(|| {
        pre_dispatch_failure(format!(
            "Native rerank action `{}` has no truncation contract.",
            native_route.binding.binding_id
        ))
    })?;
    if request.truncation != expected_truncation {
        return Err(pre_dispatch_failure(format!(
            "Rerank route requires truncation `{:?}`.",
            expected_truncation
        )));
    }
    let documents = request
        .candidates
        .iter()
        .map(|candidate| candidate.text.clone())
        .collect();
    match native_route.binding.projection {
        crate::NativeInferenceProjectionKind::AlibabaTextRerank => Ok(
            WireRerankRequestBody::AlibabaTextRerank(WireAlibabaTextRerankRequest {
                model: model_id.to_string(),
                input: WireAlibabaTextRerankInput {
                    query: request.query.text.clone(),
                    documents,
                },
                parameters: WireAlibabaTextRerankParameters {
                    top_n: request.top_n,
                },
            }),
        ),
        crate::NativeInferenceProjectionKind::Compatible
        | crate::NativeInferenceProjectionKind::RoutedCompatible => {
            Ok(WireRerankRequestBody::Compatible(WireRerankRequest {
                model: model_id.to_string(),
                query: request.query.text.clone(),
                documents,
                top_n: request.top_n,
                provider: (native_route.binding.projection
                    == crate::NativeInferenceProjectionKind::RoutedCompatible)
                    .then(|| routed_provider_policy(secondary_route)),
            }))
        }
        crate::NativeInferenceProjectionKind::SystemOne
        | crate::NativeInferenceProjectionKind::OpenVulcan => Err(pre_dispatch_failure(
            "This native projection does not support rerank.",
        )),
    }
}

/// Parses one rerank response according to the binding's declared wire projection.
/// 按绑定声明的 Wire 投影解析一份重排响应。
///
/// # Parameters
/// 参数
/// `native_route` selects the manifest-owned response shape and `response` is bounded JSON text.
/// `native_route` 选择 Manifest 持有的响应结构，`response` 是有界 JSON 文本。
///
/// # Returns
/// 返回值
/// Normalized ranked items, optional provider model identity, and usage fields.
/// 归一化排序结果、可选供应商模型身份与用量字段。
fn parse_rerank_response(
    native_route: NativeInferenceRoute<'_>,
    response: &str,
) -> Result<(Vec<WireRerankItem>, Option<String>, WireUsage), ProviderExecutionFailure> {
    match native_route.binding.projection {
        crate::NativeInferenceProjectionKind::AlibabaTextRerank => {
            let wire: WireAlibabaTextRerankResponse =
                serde_json::from_str(response).map_err(|error| {
                    response_headers_protocol_failure(format!(
                        "Alibaba text-rerank response is invalid JSON: {error}"
                    ))
                })?;
            Ok((wire.output.results, None, wire.usage))
        }
        crate::NativeInferenceProjectionKind::Compatible
        | crate::NativeInferenceProjectionKind::RoutedCompatible => {
            let wire: WireRerankResponse = serde_json::from_str(response).map_err(|error| {
                response_headers_protocol_failure(format!(
                    "Native rerank response is invalid JSON: {error}"
                ))
            })?;
            Ok((wire.results, wire.model, wire.usage))
        }
        crate::NativeInferenceProjectionKind::SystemOne
        | crate::NativeInferenceProjectionKind::OpenVulcan => Err(
            response_headers_protocol_failure("This native projection does not support rerank."),
        ),
    }
}

/// Builds OpenRouter's exact provider policy from the frozen secondary-route selection.
/// 根据冻结的二级路由选择构建 OpenRouter 精确供应商策略。
///
/// # Parameters
/// 参数
/// `secondary_route` is the already validated automatic or pinned endpoint selection.
/// `secondary_route` 是已经校验的自动或固定端点选择。
///
/// # Returns
/// 返回值
/// A no-fallback policy with an exact provider order only for a pinned endpoint.
/// 返回禁止回退的策略；仅固定端点时包含精确供应商顺序。
fn routed_provider_policy(
    secondary_route: &crate::SecondaryRouteSelection,
) -> OpenRouterProviderPolicy {
    let order = match secondary_route {
        crate::SecondaryRouteSelection::Automatic => Vec::new(),
        crate::SecondaryRouteSelection::PinnedEndpoint { route_id, .. } => {
            vec![route_id.clone()]
        }
    };
    OpenRouterProviderPolicy {
        order,
        allow_fallbacks: false,
        require_parameters: false,
    }
}

/// Validates one provider-reported model identity against the frozen request.
/// 根据冻结请求校验供应商报告的模型身份。
///
/// # Parameters
/// 参数
/// `native_route` selects compatibility rules; `requested` and `reported` are exact identities.
/// `native_route` 选择兼容规则；`requested` 与 `reported` 是精确身份。
///
/// # Returns
/// 返回值
/// Success only for an omitted identity, exact match, or OpenRouter's observed bare identity.
/// 仅对省略身份、精确匹配或 OpenRouter 已观察的裸身份返回成功。
fn validate_response_model(
    native_route: NativeInferenceRoute<'_>,
    requested: &str,
    reported: Option<&str>,
) -> Result<(), ProviderExecutionFailure> {
    let Some(reported) = reported.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let exact = reported == requested;
    let routed_bare = native_route.binding.projection
        == crate::NativeInferenceProjectionKind::RoutedCompatible
        && requested
            .split_once('/')
            .is_some_and(|(_, bare)| bare == reported);
    let reviewed_alias = native_route
        .binding
        .response_model_aliases
        .get(requested)
        .is_some_and(|aliases| aliases.contains(reported));
    if exact || routed_bare || reviewed_alias {
        return Ok(());
    }
    Err(response_headers_protocol_failure(format!(
        "Native inference response model `{reported}` differs from immutable target `{requested}`."
    )))
}

/// Converts one closed wire embedding representation into the public payload.
/// 把一项封闭 Wire 向量表示转换为公共载荷。
///
/// # Parameters
/// 参数
/// `encoding`, `requested_dimensions`, and `value` are exact request/response facts.
/// `encoding`、`requested_dimensions` 与 `value` 是精确请求与响应事实。
///
/// # Returns
/// 返回值
/// The validated payload and actual dimension count.
/// 返回已校验载荷与实际维度数量。
fn normalize_embedding_value(
    encoding: EmbeddingEncoding,
    requested_dimensions: Option<u32>,
    value: WireEmbeddingValue,
) -> Result<(EmbeddingPayload, u32), ProviderExecutionFailure> {
    match (encoding, value) {
        (EmbeddingEncoding::Float, WireEmbeddingValue::Dense(values)) if !values.is_empty() => {
            let values = values
                .into_iter()
                .map(|value| {
                    let converted = value as f32;
                    if !value.is_finite() || !converted.is_finite() {
                        Err(response_headers_protocol_failure(
                            "Embedding response contains a non-finite coordinate.",
                        ))
                    } else {
                        Ok(converted)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            let dimensions = u32::try_from(values.len()).map_err(|_| {
                response_headers_protocol_failure(
                    "Embedding response dimensions exceed the public contract.",
                )
            })?;
            if requested_dimensions.is_some_and(|requested| requested != dimensions) {
                return Err(response_headers_protocol_failure(format!(
                    "Embedding response dimensions {dimensions} differ from requested dimensions {}.",
                    requested_dimensions.unwrap_or_default()
                )));
            }
            Ok((EmbeddingPayload::Dense { values }, dimensions))
        }
        (EmbeddingEncoding::Base64, WireEmbeddingValue::Base64(value)) if !value.is_empty() => {
            let dimensions = requested_dimensions.ok_or_else(|| {
                response_headers_protocol_failure(
                    "Base64 embedding response has no requested dimension contract.",
                )
            })?;
            Ok((EmbeddingPayload::Base64 { value }, dimensions))
        }
        _ => Err(response_headers_protocol_failure(
            "Embedding response representation differs from the requested encoding.",
        )),
    }
}

/// Creates an immutable route snapshot from already frozen resolution facts.
/// 根据已冻结解析事实创建不可变路由快照。
///
/// # Parameters
/// 参数
/// All arguments come from the selected route, exact model, and authorized execution context.
/// 全部参数均来自已选路由、精确模型与已授权执行上下文。
///
/// # Returns
/// 返回值
/// One non-secret route snapshot suitable for persistence and diagnostics.
/// 返回一个适合持久化与诊断的非秘密路由快照。
pub(crate) fn execution_route_snapshot(
    route: &ProviderExecutionRoute,
    model: &ConnectionModel,
    usage_scope: InferenceUsageScope,
    catalog_revision: &str,
    route_revision: &str,
    action_binding_id: Option<&str>,
) -> ExecutionRouteSnapshot {
    let (secondary_route_id, secondary_provider_name, secondary_endpoint_name) =
        secondary_route_snapshot(&route.secondary_route);
    ExecutionRouteSnapshot {
        provider_id: route.provider.id.clone(),
        service_entry_id: route.entry.id.clone(),
        connection_id: route.connection.id.clone(),
        model_id: model.id.clone(),
        protocol_binding_id: None,
        action_binding_id: action_binding_id.map(str::to_string),
        secondary_route_id,
        secondary_provider_name,
        secondary_endpoint_name,
        capability_revision: model.capabilities.evidence_revision.clone(),
        catalog_revision: catalog_revision.to_string(),
        route_revision: route_revision.to_string(),
        usage_scope,
    }
}

/// Returns the exact provider-internal route identifier when execution is pinned.
/// 当执行固定到供应商内部端点时返回其精确路由标识。
///
/// # Parameters
/// 参数
/// `selection` is the validated immutable secondary route selection.
/// `selection` 是已经校验的不可变二级路由选择。
///
/// # Returns
/// 返回值
/// The exact pinned route identifier, or no value for provider-managed automatic routing.
/// 返回精确固定路由标识；供应商自动路由时不返回值。
fn secondary_route_id(selection: &crate::SecondaryRouteSelection) -> Option<String> {
    match selection {
        crate::SecondaryRouteSelection::Automatic => None,
        crate::SecondaryRouteSelection::PinnedEndpoint { route_id, .. } => Some(route_id.clone()),
    }
}

/// Projects one validated secondary selection into non-secret immutable route facts.
/// 将一项已校验二级选择投影为不含秘密的不可变路由事实。
///
/// # Parameters
/// 参数
/// `selection` is the exact secondary route frozen before dispatch.
/// `selection` 是分派前冻结的精确二级路由。
///
/// # Returns
/// 返回值
/// Route identifier and display labels, or three empty values for automatic routing.
/// 返回路由标识和显示标签；自动路由时返回三个空值。
fn secondary_route_snapshot(
    selection: &crate::SecondaryRouteSelection,
) -> (Option<String>, Option<String>, Option<String>) {
    match selection {
        crate::SecondaryRouteSelection::Automatic => (None, None, None),
        crate::SecondaryRouteSelection::PinnedEndpoint {
            route_id,
            provider_display_name,
            endpoint_display_name,
        } => (
            Some(route_id.clone()),
            Some(provider_display_name.clone()),
            Some(endpoint_display_name.clone()),
        ),
    }
}

/// Converts documented provider token fields into final public usage.
/// 把文档化供应商 Token 字段转换为最终公共用量。
///
/// # Parameters
/// 参数
/// `usage` contains only directly reported wire values.
/// `usage` 仅包含 Wire 层直接报告值。
///
/// # Returns
/// 返回值
/// Final usage with unknown fields preserved as unknown rather than zero.
/// 返回最终用量，未报告字段保持未知而不是写为零。
fn normalized_usage(usage: WireUsage) -> RequestUsage {
    let input_tokens = usage.input_tokens.or(usage.prompt_tokens);
    RequestUsage {
        update_kind: UsageUpdateKind::Final,
        input_tokens: reported_usage_value(input_tokens),
        uncached_input_tokens: reported_usage_value(input_tokens),
        total_tokens: reported_usage_value(usage.total_tokens),
        ..RequestUsage::default()
    }
}

/// Wraps one optional provider token count without converting absence to zero.
/// 包装一项可选供应商 Token 数量且不把缺失转换为零。
///
/// # Parameters
/// 参数
/// `value` is the exact optional provider field.
/// `value` 是精确可选供应商字段。
///
/// # Returns
/// 返回值
/// A reported known count or an unknown value.
/// 返回已报告的已知数量或未知值。
fn reported_usage_value(value: Option<u64>) -> UsageValue {
    value.map_or(UsageValue::Unknown, |value| UsageValue::Known {
        value,
        source: UsageValueSource::Reported,
    })
}

/// Converts elapsed wall time to a saturating public millisecond value.
/// 把已耗墙钟时间转换为饱和公共毫秒值。
///
/// # Parameters
/// 参数
/// `started_at` is the monotonic request start instant.
/// `started_at` 是单调请求开始时刻。
///
/// # Returns
/// 返回值
/// Milliseconds capped at `u64::MAX`.
/// 返回封顶为 `u64::MAX` 的毫秒数。
fn elapsed_milliseconds(started_at: Instant) -> u64 {
    started_at
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::LazyLock;
    use vulcan_ai_contract::{
        EmbeddingInput, INFERENCE_CONTRACT_VERSION, RerankText, RerankTruncation,
    };

    /// Returns a routed-compatible native binding used by projection tests.
    /// 返回投影测试使用的带路由兼容原生绑定。
    fn routed_native_route() -> NativeInferenceRoute<'static> {
        static BINDING: LazyLock<crate::NativeInferenceBinding> =
            LazyLock::new(|| crate::NativeInferenceBinding {
                operation: AiOperation::RerankDocuments,
                model_ids: Vec::new(),
                binding_id: String::from("test.routed.v1"),
                endpoint_path: String::from("/rerank"),
                projection: crate::NativeInferenceProjectionKind::RoutedCompatible,
                service_origin_base_path: None,
                requires_service_origin_override: false,
                rerank_truncation: Some(RerankTruncation::Disabled),
                response_model_aliases: BTreeMap::from([
                    (
                        String::from("cohere/rerank-4-fast"),
                        BTreeSet::from([String::from("rerank-v4.0-fast")]),
                    ),
                    (
                        String::from("cohere/rerank-4-pro"),
                        BTreeSet::from([String::from("rerank-v4.0-pro")]),
                    ),
                ]),
                batch: None,
            });
        NativeInferenceRoute { binding: &BINDING }
    }

    /// Returns a plain-compatible native binding used by projection tests.
    /// 返回投影测试使用的普通兼容原生绑定。
    fn compatible_native_route() -> NativeInferenceRoute<'static> {
        static BINDING: LazyLock<crate::NativeInferenceBinding> =
            LazyLock::new(|| crate::NativeInferenceBinding {
                operation: AiOperation::RerankDocuments,
                model_ids: Vec::new(),
                binding_id: String::from("test.compatible.v1"),
                endpoint_path: String::from("/reranks"),
                projection: crate::NativeInferenceProjectionKind::Compatible,
                service_origin_base_path: None,
                requires_service_origin_override: false,
                rerank_truncation: Some(RerankTruncation::ProviderDefault),
                response_model_aliases: BTreeMap::new(),
                batch: None,
            });
        NativeInferenceRoute { binding: &BINDING }
    }

    /// Returns Alibaba Model Studio's model-specific nested text-rerank binding.
    /// 返回阿里云百炼按模型选择的嵌套文本重排绑定。
    fn alibaba_text_rerank_route() -> NativeInferenceRoute<'static> {
        static BINDING: LazyLock<crate::NativeInferenceBinding> =
            LazyLock::new(|| crate::NativeInferenceBinding {
                operation: AiOperation::RerankDocuments,
                model_ids: vec![String::from("qwen3.7-text-rerank")],
                binding_id: String::from("alibaba.model_studio.qwen3_7_text_rerank.v1"),
                endpoint_path: String::from("/services/rerank/text-rerank/text-rerank"),
                projection: crate::NativeInferenceProjectionKind::AlibabaTextRerank,
                service_origin_base_path: Some(String::from("/api/v1")),
                requires_service_origin_override: true,
                rerank_truncation: Some(RerankTruncation::ProviderDefault),
                response_model_aliases: BTreeMap::new(),
                batch: None,
            });
        NativeInferenceRoute { binding: &BINDING }
    }

    /// Returns an origin-required compatible native binding for one fixed regional API root.
    /// 返回一个使用固定区域 API 根路径且要求源地址的兼容原生绑定。
    fn origin_native_route(china: bool) -> NativeInferenceRoute<'static> {
        static CHINA: LazyLock<crate::NativeInferenceBinding> =
            LazyLock::new(|| crate::NativeInferenceBinding {
                operation: AiOperation::RerankDocuments,
                model_ids: Vec::new(),
                binding_id: String::from("test.origin.cn.v1"),
                endpoint_path: String::from("/reranks"),
                projection: crate::NativeInferenceProjectionKind::Compatible,
                service_origin_base_path: Some(String::from("/compatible-api/v1")),
                requires_service_origin_override: true,
                rerank_truncation: Some(RerankTruncation::ProviderDefault),
                response_model_aliases: BTreeMap::new(),
                batch: None,
            });
        static SINGAPORE: LazyLock<crate::NativeInferenceBinding> =
            LazyLock::new(|| crate::NativeInferenceBinding {
                operation: AiOperation::RerankDocuments,
                model_ids: Vec::new(),
                binding_id: String::from("test.origin.sg.v1"),
                endpoint_path: String::from("/reranks"),
                projection: crate::NativeInferenceProjectionKind::Compatible,
                service_origin_base_path: Some(String::from("/compatible-mode/v1")),
                requires_service_origin_override: true,
                rerank_truncation: Some(RerankTruncation::ProviderDefault),
                response_model_aliases: BTreeMap::new(),
                batch: None,
            });
        NativeInferenceRoute {
            binding: if china { &CHINA } else { &SINGAPORE },
        }
    }

    /// Builds one exact text embedding request for projection tests.
    /// 为投影测试构建一项精确文本向量请求。
    ///
    /// # Parameters
    /// 参数
    /// `task` selects the public semantic task.
    /// `task` 选择公共语义任务。
    ///
    /// # Returns
    /// 返回值
    /// One structurally valid dense float request.
    /// 返回一项结构有效的稠密 Float 请求。
    fn embedding_request(task: EmbeddingTask) -> EmbeddingRequest {
        EmbeddingRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: String::from("request-1"),
            idempotency_key: None,
            consumer_profile_id: String::from("vmm"),
            inputs: vec![EmbeddingInput {
                input_id: String::from("input-1"),
                content: EmbeddingInputContent::Text {
                    text: String::from("hello"),
                },
            }],
            task,
            dimensions: Some(2),
            output_kind: EmbeddingOutputKind::Dense,
            encoding: EmbeddingEncoding::Float,
            invalid_input_policy: Default::default(),
        }
    }

    /// Verifies OpenRouter projection preserves task semantics and disables fallback.
    /// 验证 OpenRouter 投影保留任务语义并禁用回退。
    #[test]
    fn openrouter_embedding_projection_is_frozen() {
        let request = embedding_request(EmbeddingTask::Document);
        let projected = project_embedding_request(
            routed_native_route(),
            &crate::SecondaryRouteSelection::Automatic,
            "openai/text-embedding-3-small",
            &request,
        )
        .expect("document embedding should project");
        let encoded = serde_json::to_value(projected).expect("request should serialize");

        assert_eq!(encoded["input_type"], "search_document");
        assert_eq!(encoded["provider"]["allow_fallbacks"], false);
        assert_eq!(encoded["provider"]["require_parameters"], false);
        assert_eq!(encoded["dimensions"], 2);
    }

    /// Verifies native inference preserves the exact cached OpenRouter endpoint selection.
    /// 验证原生推理保留精确的 OpenRouter 缓存端点选择。
    #[test]
    fn openrouter_native_projection_preserves_pinned_secondary_route() {
        let secondary_route = crate::SecondaryRouteSelection::PinnedEndpoint {
            route_id: String::from("deepinfra"),
            provider_display_name: String::from("DeepInfra"),
            endpoint_display_name: String::from("DeepInfra US"),
        };
        let projected = project_embedding_request(
            routed_native_route(),
            &secondary_route,
            "openai/text-embedding-3-small",
            &embedding_request(EmbeddingTask::Document),
        )
        .expect("pinned endpoint should project");
        let encoded = serde_json::to_value(projected).expect("request should serialize");

        assert_eq!(
            encoded["provider"]["order"],
            serde_json::json!(["deepinfra"])
        );
        assert_eq!(encoded["provider"]["allow_fallbacks"], false);
    }

    /// Verifies Alibaba projection refuses OpenRouter-only semantic task projection.
    /// 验证阿里云投影拒绝仅属于 OpenRouter 的语义任务投影。
    #[test]
    fn alibaba_embedding_projection_requires_provider_default_task() {
        let error = project_embedding_request(
            compatible_native_route(),
            &crate::SecondaryRouteSelection::Automatic,
            "text-embedding-v4",
            &embedding_request(EmbeddingTask::Query),
        )
        .expect_err("Alibaba compatible embedding task must stay provider-default");

        assert!(error.to_string().contains("not supported"));
    }

    /// Verifies incompatible embedding wire representations fail before publication.
    /// 验证不兼容向量 Wire 表示会在发布前失败。
    #[test]
    fn embedding_normalization_rejects_encoding_mismatch() {
        let error = normalize_embedding_value(
            EmbeddingEncoding::Float,
            Some(2),
            WireEmbeddingValue::Base64(String::from("AAAA")),
        )
        .expect_err("Base64 must not satisfy a float request");

        assert!(error.to_string().contains("differs"));
    }

    /// Verifies OpenRouter accepts only exact, bare, or documented upstream response identities.
    /// 验证 OpenRouter 仅接受精确、裸名或已有官方依据的上游响应身份。
    #[test]
    fn response_model_identity_is_exact_or_openrouter_bare() {
        assert!(
            validate_response_model(
                routed_native_route(),
                "openai/text-embedding-3-small",
                Some("text-embedding-3-small"),
            )
            .is_ok()
        );
        assert!(
            validate_response_model(
                routed_native_route(),
                "cohere/rerank-4-fast",
                Some("rerank-v4.0-fast"),
            )
            .is_ok()
        );
        assert!(
            validate_response_model(
                routed_native_route(),
                "cohere/rerank-4-pro",
                Some("rerank-v4.0-pro"),
            )
            .is_ok()
        );
        assert!(
            validate_response_model(
                routed_native_route(),
                "cohere/rerank-4-fast",
                Some("rerank-v4.0-pro"),
            )
            .is_err()
        );
        assert!(
            validate_response_model(
                compatible_native_route(),
                "text-embedding-v4",
                Some("other-model"),
            )
            .is_err()
        );
    }

    /// Verifies OpenRouter and Alibaba rerank requests require different exact truncation modes.
    /// 验证 OpenRouter 与阿里云重排请求要求不同的精确截断模式。
    #[test]
    fn rerank_projection_preserves_provider_truncation_contract() {
        let mut request = RerankRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: String::from("request-1"),
            idempotency_key: None,
            consumer_profile_id: String::from("vmm"),
            query: RerankText {
                id: String::from("query"),
                text: String::from("capital"),
            },
            candidates: vec![RerankText {
                id: String::from("candidate"),
                text: String::from("Paris"),
            }],
            top_n: 1,
            truncation: RerankTruncation::Disabled,
            return_content: true,
        };
        let openrouter_projected = project_rerank_request(
            routed_native_route(),
            &crate::SecondaryRouteSelection::Automatic,
            "cohere/rerank-v3.5",
            &request,
        )
        .expect("OpenRouter rerank should use the disabled truncation mode");
        let openrouter_encoded = serde_json::to_value(openrouter_projected)
            .expect("OpenRouter request should serialize");
        assert_eq!(openrouter_encoded["query"], "capital");
        assert_eq!(
            openrouter_encoded["documents"],
            serde_json::json!(["Paris"])
        );
        assert_eq!(openrouter_encoded["top_n"], 1);
        assert_eq!(openrouter_encoded["provider"]["allow_fallbacks"], false);
        assert_eq!(openrouter_encoded["provider"]["require_parameters"], false);
        assert!(
            project_rerank_request(
                compatible_native_route(),
                &crate::SecondaryRouteSelection::Automatic,
                "qwen3-rerank",
                &request,
            )
            .is_err()
        );
        request.truncation = RerankTruncation::ProviderDefault;
        let projected = project_rerank_request(
            compatible_native_route(),
            &crate::SecondaryRouteSelection::Automatic,
            "qwen3-rerank",
            &request,
        )
        .expect("Qwen3 rerank should use the provider-default truncation mode");
        let encoded = serde_json::to_value(projected).expect("Qwen3 request should serialize");
        assert_eq!(encoded["model"], "qwen3-rerank");
        assert_eq!(encoded["query"], "capital");
        assert_eq!(encoded["documents"], serde_json::json!(["Paris"]));
        assert_eq!(encoded["top_n"], 1);
        assert!(encoded.get("input").is_none());
        assert!(encoded.get("parameters").is_none());
        assert!(encoded.get("provider").is_none());
    }

    /// Verifies Alibaba Qwen3 rerank reads the documented top-level compatible response.
    /// 验证阿里云 Qwen3 重排读取文档规定的顶层兼容响应。
    #[test]
    fn alibaba_qwen3_rerank_reads_top_level_compatible_response() {
        let wire: WireRerankResponse = serde_json::from_value(serde_json::json!({
            "object": "list",
            "results": [
                {"index": 0, "relevance_score": 0.93},
                {"index": 2, "relevance_score": 0.34}
            ],
            "model": "qwen3-rerank",
            "id": "request-1",
            "usage": {"total_tokens": 79}
        }))
        .expect("Qwen3 compatible response should deserialize");

        assert_eq!(wire.model.as_deref(), Some("qwen3-rerank"));
        assert_eq!(wire.results.len(), 2);
        assert_eq!(wire.results[1].index, 2);
        assert_eq!(wire.usage.total_tokens, Some(79));
        assert!(
            serde_json::from_value::<WireRerankResponse>(serde_json::json!({
                "output": {"results": [{"index": 0, "relevance_score": 0.93}]},
                "usage": {"total_tokens": 79}
            }))
            .is_err()
        );
    }

    /// Verifies Alibaba Qwen3.7 rerank uses the documented nested request and response.
    /// 验证阿里云 Qwen3.7 重排使用文档规定的嵌套请求与响应。
    #[test]
    fn alibaba_qwen37_rerank_uses_nested_wire_contract() {
        let request = RerankRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: String::from("request-1"),
            idempotency_key: None,
            consumer_profile_id: String::from("vmm"),
            query: RerankText {
                id: String::from("query"),
                text: String::from("capital"),
            },
            candidates: vec![RerankText {
                id: String::from("candidate"),
                text: String::from("Paris"),
            }],
            top_n: 1,
            truncation: RerankTruncation::ProviderDefault,
            return_content: false,
        };
        let projected = project_rerank_request(
            alibaba_text_rerank_route(),
            &crate::SecondaryRouteSelection::Automatic,
            "qwen3.7-text-rerank",
            &request,
        )
        .expect("Alibaba Qwen3.7 rerank should use its native projection");
        let encoded = serde_json::to_value(projected).expect("request should serialize");
        assert_eq!(encoded["model"], "qwen3.7-text-rerank");
        assert_eq!(encoded["input"]["query"], "capital");
        assert_eq!(encoded["input"]["documents"], serde_json::json!(["Paris"]));
        assert_eq!(encoded["parameters"]["top_n"], 1);
        assert!(encoded.get("query").is_none());
        assert!(encoded.get("provider").is_none());

        let (results, model, usage) = parse_rerank_response(
            alibaba_text_rerank_route(),
            r#"{
                "output": {"results": [{"index": 0, "relevance_score": 0.93}]},
                "usage": {"total_tokens": 79}
            }"#,
        )
        .expect("Alibaba Qwen3.7 response should deserialize");
        assert_eq!(model, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].index, 0);
        assert_eq!(usage.total_tokens, Some(79));
    }

    /// Verifies Alibaba's product root is not duplicated by native inference endpoints.
    /// 验证阿里云产品根不会被原生推理端点重复拼接。
    #[test]
    fn alibaba_native_urls_preserve_the_registered_compatible_root() {
        assert_eq!(
            native_request_url(
                compatible_native_route(),
                None,
                "https://dashscope.aliyuncs.com/compatible-mode/v1",
                "/embeddings",
            )
            .expect("embedding URL"),
            "https://dashscope.aliyuncs.com/compatible-mode/v1/embeddings"
        );
        assert_eq!(
            native_request_url(
                origin_native_route(false),
                Some("https://ws-example.ap-southeast-1.maas.aliyuncs.com"),
                "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
                "/reranks",
            )
            .expect("rerank URL"),
            "https://ws-example.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1/reranks"
        );
        assert_eq!(
            native_request_url(
                origin_native_route(true),
                Some("https://ws-example.cn-beijing.maas.aliyuncs.com"),
                "https://dashscope.aliyuncs.com/compatible-mode/v1",
                "/reranks",
            )
            .expect("China rerank URL"),
            "https://ws-example.cn-beijing.maas.aliyuncs.com/compatible-api/v1/reranks"
        );
        assert_eq!(
            native_request_url(
                alibaba_text_rerank_route(),
                Some("https://ws-example.cn-beijing.maas.aliyuncs.com"),
                "https://dashscope.aliyuncs.com/compatible-mode/v1",
                "/services/rerank/text-rerank/text-rerank",
            )
            .expect("Qwen3.7 rerank URL"),
            "https://ws-example.cn-beijing.maas.aliyuncs.com/api/v1/services/rerank/text-rerank/text-rerank"
        );
    }

    /// Verifies Alibaba Qwen3 rerank refuses to dispatch without a workspace-scoped origin.
    /// 验证阿里云 Qwen3 重排没有工作区作用域源地址时拒绝分派。
    #[test]
    fn alibaba_qwen3_rerank_requires_workspace_service_origin() {
        let error = native_request_url(
            origin_native_route(false),
            None,
            "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
            "/reranks",
        )
        .expect_err("Qwen3 rerank must require a workspace origin");

        assert!(
            error
                .to_string()
                .contains("requires a credential-scoped service origin"),
            "{error}"
        );
    }

    /// Serves one canned HTTP response on loopback and returns the raw request it received.
    /// 在回环地址上返回一份预置 HTTP 响应，并返回收到的原始请求。
    async fn serve_once(
        status: &'static str,
        body: &'static str,
    ) -> (String, tokio::task::JoinHandle<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0_u8; 4096];
            loop {
                let count = stream.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
                    let headers = std::str::from_utf8(&bytes[..end]).unwrap();
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let reply = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(reply.as_bytes()).await.unwrap();
            String::from_utf8(bytes).unwrap()
        });
        (format!("http://{address}/v1"), server)
    }

    /// Runs one decision against the bundled TypeSafe route redirected to `base_url`.
    /// 针对重定向到 `base_url` 的随包 TypeSafe 路由执行一次决策。
    async fn run_decision(base_url: String) -> Result<DecisionResponse, ProviderExecutionFailure> {
        let mut route = crate::execution::test_support::bundled_manifest_route(
            "typesafe",
            "public-api",
            vulcan_llm_core::TYPESAFE_SYSTEMONE_PROFILE_ID,
        );
        route.entry.base_url = base_url;
        route.protocol_route.base_url_override = None;
        let model = ConnectionModel {
            id: String::from("jev-1.13.0"),
            display_name: String::from("Jev"),
            capabilities: vulcan_ai_contract::ModelCapabilities {
                operations: vec![AiOperation::DecisionEvaluate],
                ..vulcan_ai_contract::ModelCapabilities::default()
            },
            capability_evidence: crate::ModelCapabilityEvidence::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        };
        let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("fixture-key"))
            .unwrap()
            .execution_secret()
            .unwrap();
        let client = ProviderExecutionClient::new(
            reqwest::Client::builder().no_proxy().build().unwrap(),
            std::sync::Arc::new(vulcan_llm_core::ProtocolRegistry::first_batch().unwrap()),
        );
        client
            .execute_decision(
                &route,
                &model,
                &decision_request(),
                InferenceUsageScope::InternalBackground,
                ExecutionRevisionRefs {
                    catalog: "catalog",
                    route: "route",
                },
                &secret,
            )
            .await
    }

    /// Builds the three-option decision the permission judge asks.
    /// 构建权限判定所问的三选项决策。
    fn decision_request() -> DecisionRequest {
        DecisionRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: String::from("decision-1"),
            idempotency_key: None,
            consumer_profile_id: String::from("permission_judge"),
            state: serde_json::json!({"call": {"tool": "shell_command"}}),
            questions: vec![vulcan_ai_contract::DecisionQuestion {
                id: String::from("decision"),
                kind: vulcan_ai_contract::DecisionQuestionKind::Choice {
                    instructions: String::from("Judge."),
                    criteria: ["allow", "review", "block"]
                        .into_iter()
                        .map(|option| vulcan_ai_contract::DecisionCriterion {
                            option: option.to_string(),
                            criterion: format!("{option}."),
                        })
                        .collect(),
                },
            }],
        }
    }

    /// Verifies a decision posts the System One body to the manifest endpoint and keeps the answer.
    /// 验证决策把 System One 正文发送到 Manifest 端点并保留回答。
    #[tokio::test]
    async fn decision_posts_system_one_and_validates_the_answer() {
        let (base_url, server) = serve_once(
            "200 OK",
            r#"{"model":"jev-1.13.0","answers":{"decision":{"type":"choice","choice":"review","confidence":0.52,"probabilities":{"allow":0.02,"review":0.68,"block":0.3}}},"usage":{"input_tokens":404,"output_tokens":38}}"#,
        )
        .await;
        let response = run_decision(base_url).await.expect("decision");
        let request = server.await.unwrap();
        assert!(request.starts_with("POST /v1/systemone "), "{request}");
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer fixture-key")
        );
        let body: serde_json::Value =
            serde_json::from_str(&request[request.find("\r\n\r\n").unwrap() + 4..]).unwrap();
        assert_eq!(body["model"], "jev-1.13.0");
        assert_eq!(body["questions"]["decision"]["type"], "choice");
        assert_eq!(
            response.route.action_binding_id.as_deref(),
            Some("typesafe.systemone.v1")
        );
        assert_eq!(
            response.usage.input_tokens,
            UsageValue::Known {
                value: 404,
                source: UsageValueSource::Reported
            }
        );
        let vulcan_ai_contract::DecisionAnswer::Choice { choice, .. } =
            &response.answers["decision"];
        assert_eq!(choice, "review");
    }

    /// Verifies answers outside the declared options and other models are refused.
    /// 验证超出声明选项的回答与其他模型的回答都会被拒绝。
    #[tokio::test]
    async fn decision_refuses_undeclared_choices_and_other_models() {
        for body in [
            r#"{"model":"jev-1.13.0","answers":{"decision":{"type":"choice","choice":"maybe","confidence":0.5,"probabilities":{"allow":0.2,"review":0.3,"block":0.5}}}}"#,
            r#"{"model":"jev-latest","answers":{"decision":{"type":"choice","choice":"allow","confidence":0.5,"probabilities":{"allow":0.9,"review":0.05,"block":0.05}}}}"#,
        ] {
            let (base_url, server) = serve_once("200 OK", body).await;
            assert!(run_decision(base_url).await.is_err(), "{body}");
            server.await.unwrap();
        }
    }
}
