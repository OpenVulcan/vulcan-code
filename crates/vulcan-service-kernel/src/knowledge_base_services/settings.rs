//! Exact human-selected knowledge modes and signed model capability admission.
//! 精确人工知识模式选择及签名模型能力准入。

use super::*;
use vulcan_ai_contract::{DistanceMetric, EmbeddingCapabilities};

/// Resolves current execution/capabilities for frozen `route` without alternate protocols or connections.
/// 不使用其他协议或连接，为冻结 `route` 解析当前执行及能力。
/// `state` owns the signed catalog; returns one consistent contract or an explicit error.
/// `state` 拥有签名目录；返回一致契约或明确错误。
pub(super) fn embedding_contract(
    state: &SharedServiceBridgeState,
    route: &KnowledgeRouteReference,
) -> Result<
    (
        vulcan_ai_contract::ExecutionRouteSnapshot,
        EmbeddingCapabilities,
    ),
    String,
> {
    if route.usage_scope != "internal_background" {
        return Err("ERR_KNOWLEDGE_USAGE_SCOPE".into());
    }
    // Credential identity is a declared route component, never a default-account fallback.
    // 凭据身份为声明路由组件，绝非默认账号兜底。
    let connection = route
        .connection_id
        .as_deref()
        .ok_or("ERR_KNOWLEDGE_CONNECTION_REQUIRED")?;
    // The binding family determines the sole allowed interpretation of the protocol identifier.
    // 绑定家族决定协议标识唯一允许的解释方式。
    let binding = match route.binding_kind {
        KnowledgeRouteBindingKind::Protocol => InferenceRouteBinding::Protocol {
            protocol_profile_id: route.protocol.clone(),
        },
        KnowledgeRouteBindingKind::Action => InferenceRouteBinding::Action {
            action_binding_id: route.protocol.clone(),
        },
    };
    // The resolver validates exact provider, connection, endpoint, model, operation and usage ownership.
    // 解析器验证精确供应商、连接、端点、模型、操作及使用归属。
    crate::public_ai_settings::resolve_knowledge_internal_inference_candidate(
        state,
        &route.provider_id,
        &route.service_entry_id,
        Some(connection),
        &route.model_id,
        &binding,
        AiOperation::EmbeddingCreate,
        InferenceUsageScope::InternalBackground,
    )
}

/// Freezes the distance the library searches with: the model-declared distance in
/// `capabilities`, or cosine when the model declares none. Cosine ignores vector length, so it
/// ranks correctly whether or not the provider normalizes its vectors; it is the library's own
/// choice, not a claim about the model.
/// 冻结库检索使用的距离：`capabilities` 中模型声明的距离；模型未声明时使用余弦。余弦与向量长度
/// 无关，无论供应商是否归一化向量都能正确排序；这是库自身的选择，而非对模型的断言。
pub(super) fn embedding_metric(
    capabilities: &EmbeddingCapabilities,
) -> Result<KnowledgeVectorMetric, String> {
    Ok(match capabilities.distance_metric {
        DistanceMetric::Euclidean => KnowledgeVectorMetric::Euclidean,
        DistanceMetric::Cosine | DistanceMetric::Unknown => KnowledgeVectorMetric::Cosine,
        DistanceMetric::DotProduct => KnowledgeVectorMetric::DotProduct,
    })
}

/// Selects the explicitly declared query/document task, or its declared provider default.
/// 选择显式声明的查询或文档任务，或其已声明供应商默认任务。
/// `is_query` determines intent; returns an admitted task without probing a provider.
/// `is_query` 决定意图；返回已准入任务，不探测供应商。
pub(super) fn embedding_task(
    capabilities: &EmbeddingCapabilities,
    is_query: bool,
) -> Result<EmbeddingTask, String> {
    // The capability type explicitly permits multiple tasks; preference is deterministic and frozen by its fingerprint.
    // 能力类型明确允许多个任务；偏好确定，且由其指纹冻结。
    let preferred = if is_query {
        EmbeddingTask::Query
    } else {
        EmbeddingTask::Document
    };
    if capabilities.tasks.contains(&preferred) {
        return Ok(preferred);
    }
    if capabilities.tasks.contains(&EmbeddingTask::ProviderDefault) {
        return Ok(EmbeddingTask::ProviderDefault);
    }
    Err("ERR_KNOWLEDGE_EMBEDDING_TASK_UNAVAILABLE".into())
}

/// Validates `dimensions` and `batch` against exact `capabilities`; returns explicit admission failures.
/// 根据精确 `capabilities` 验证 `dimensions` 及 `batch`；返回明确准入失败。
pub(super) fn validate_embedding_shape(
    capabilities: &EmbeddingCapabilities,
    dimensions: u32,
    batch: u32,
) -> Result<(), String> {
    if dimensions == 0
        || batch == 0
        || batch > MAX_KNOWLEDGE_EMBEDDING_BATCH_SIZE
        || capabilities
            .maximum_inputs
            .is_some_and(|maximum| batch > maximum)
        || (!capabilities.dimensions.is_empty() && !capabilities.dimensions.contains(&dimensions))
        || capabilities
            .minimum_dimensions
            .is_some_and(|minimum| dimensions < minimum)
        || capabilities
            .maximum_dimensions
            .is_some_and(|maximum| dimensions > maximum)
    {
        return Err("ERR_KNOWLEDGE_EMBEDDING_SHAPE_UNSUPPORTED".into());
    }
    if !capabilities
        .output_kinds
        .contains(&EmbeddingOutputKind::Dense)
        || !capabilities.encodings.contains(&EmbeddingEncoding::Float)
    {
        return Err("ERR_KNOWLEDGE_DENSE_FLOAT_REQUIRED".into());
    }
    embedding_task(capabilities, true)?;
    embedding_task(capabilities, false)?;
    embedding_metric(capabilities)?;
    Ok(())
}

/// Hashes exact `route`, `dimensions` and selected-model `capabilities` into the frozen semantic fingerprint.
/// 将精确 `route`、`dimensions` 及选定模型 `capabilities` 哈希为冻结语义指纹。
/// Returns an unambiguous digest independent of global catalog generations and derived consumer profiles.
/// 返回无歧义摘要，独立于全局目录代次及派生消费方配置档。
pub(super) fn embedding_fingerprint(
    route: &vulcan_ai_contract::ExecutionRouteSnapshot,
    dimensions: u32,
    capabilities: &EmbeddingCapabilities,
) -> Result<String, String> {
    // Include exact wire/capability revisions; catalog generations and labels are presentation only.
    // 纳入精确 Wire 及能力修订；目录代次及标签仅用于展示。
    let identity = (
        &route.provider_id,
        &route.service_entry_id,
        &route.connection_id,
        &route.model_id,
        &route.protocol_binding_id,
        &route.action_binding_id,
        &route.secondary_route_id,
        &route.capability_revision,
        &route.route_revision,
    );
    Ok(sha256_hex(
        &serde_json::to_vec(&(
            "knowledge-embedding-contract-v1",
            identity,
            dimensions,
            capabilities,
        ))
        .map_err(|error| error.to_string())?,
    ))
}

/// Converts explicit human `request` using current signed capabilities in `state`; returns the frozen policy.
/// 使用 `state` 中当前签名能力转换显式人工 `request`；返回冻结策略。
pub(super) fn knowledge_settings_from_request(
    state: &SharedServiceBridgeState,
    request: KnowledgeLibrarySettingsRequest,
) -> Result<KnowledgeLibrarySettings, String> {
    // Per-library modes are the sole retrieval policy.
    // 每库模式为唯一检索策略。
    let mode = match request.index_mode {
        KnowledgeIndexModeRequest::Browse => KnowledgeIndexMode::Browse,
        KnowledgeIndexModeRequest::Bm25 => KnowledgeIndexMode::Bm25,
        KnowledgeIndexModeRequest::Vector => KnowledgeIndexMode::Vector,
        KnowledgeIndexModeRequest::Hybrid => KnowledgeIndexMode::Hybrid,
    };
    if mode.has_vectors() != request.embedding.is_some() {
        return Err("ERR_KNOWLEDGE_EMBEDDING_REQUIRED".into());
    }
    // Nonvector modes schedule no inference and contain no hidden route configuration.
    // 非向量模式不调度推理，且不包含隐藏路由配置。
    let embedding = match request.embedding {
        None => None,
        Some(request) => {
            for value in [
                &request.route.provider_id,
                &request.route.protocol,
                &request.route.service_entry_id,
                &request.route.model_id,
                &request.route.connection_id,
            ] {
                if value.is_empty() || value.trim() != value {
                    return Err("ERR_KNOWLEDGE_ROUTE_IDENTITY".into());
                }
            }
            // Usage is a knowledge lifecycle declaration; the browser never chooses host authorization.
            // 使用范围为知识生命周期声明；浏览器绝不选择宿主授权。
            let route = KnowledgeRouteReference {
                provider_id: request.route.provider_id,
                protocol: request.route.protocol,
                binding_kind: match request.route.binding_kind {
                    KnowledgeRouteBindingKindRequest::Protocol => {
                        KnowledgeRouteBindingKind::Protocol
                    }
                    KnowledgeRouteBindingKindRequest::Action => KnowledgeRouteBindingKind::Action,
                },
                service_entry_id: request.route.service_entry_id,
                model_id: request.route.model_id,
                connection_id: Some(request.route.connection_id),
                usage_scope: "internal_background".into(),
            };
            // Official input count bounds the default, while explicit overrides are validated unchanged.
            // 官方输入数约束默认值，而显式覆盖值保持原值接受验证。
            let (execution, capabilities) = embedding_contract(state, &route)?;
            // Optional maximum_inputs is a declared capability shape, not an alternate route.
            // 可选 maximum_inputs 为声明能力形状，并非其他路由。
            let default_batch = capabilities
                .maximum_inputs
                .map_or(KNOWLEDGE_DEFAULT_EMBEDDING_BATCH_SIZE, |maximum| {
                    maximum.min(KNOWLEDGE_DEFAULT_EMBEDDING_BATCH_SIZE)
                });
            // Omission selects the declared default; zero never substitutes it.
            // 省略选择已声明默认值；零绝不替换为默认值。
            let batch_size = request.batch_size.unwrap_or(default_batch);
            validate_embedding_shape(&capabilities, request.dimensions, batch_size)?;
            Some(KnowledgeEmbeddingSettings {
                metric: embedding_metric(&capabilities)?,
                model_fingerprint: embedding_fingerprint(
                    &execution,
                    request.dimensions,
                    &capabilities,
                )?,
                route,
                dimensions: request.dimensions,
                batch_size,
            })
        }
    };
    // The same persisted-policy validator protects creation, mode changes and packages.
    // 相同持久策略验证器保护创建、模式变更及套件。
    let settings = KnowledgeLibrarySettings {
        index_mode: mode,
        embedding,
    };
    settings.validate()?;
    Ok(settings)
}
