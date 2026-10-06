//! Strict credential-scoped model discovery.
//! 严格的凭据作用域模型发现。

#[cfg(test)]
use crate::reasoning_catalog::{FixedReasoningProvider, fixed_reasoning_capabilities};
use crate::{
    ConnectionModel, ConnectionModelCatalog, ModelCapabilityEvidence,
    provider_manifest::ResolvedProviderManifest,
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
use vulcan_ai_contract::NativeToolKind;
use vulcan_ai_contract::{
    AiOperation, CapabilitySupport, DistanceMetric, EmbeddingCapabilities, EmbeddingEncoding,
    EmbeddingNormalization, EmbeddingOutputKind, EmbeddingTask, ModelCapabilities,
    ModelParameterDefault, ModelParameterDefinition, ModelParameterSchema, PromptCacheCapability,
    ReasoningControlMode, ReasoningEffort, RerankCapabilities, RerankScoreSemantics,
    RerankTruncation, ResourceKind, StructuredOutputCapability,
};
#[cfg(test)]
use vulcan_llm_core::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID;
use vulcan_llm_core::{
    ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
};

/// Stable public parameter identifier for sampling temperature.
/// 采样温度的稳定公共参数标识。
pub const SAMPLING_TEMPERATURE_PARAMETER_ID: &str = "sampling.temperature";

/// Stable provider-neutral parameter identifier for a direct reasoning-token budget.
/// 直接推理 Token 预算的稳定供应商无关参数标识。
pub const REASONING_MAX_TOKENS_PARAMETER_ID: &str = "reasoning.max_tokens";

/// Stable MiniMax migration-oracle parameter identifier used only by co-located tests.
/// 仅供同位测试使用的 MiniMax 迁移对照参数稳定标识。
#[cfg(test)]
pub const MINIMAX_REASONING_SPLIT_PARAMETER_ID: &str = "minimax.reasoning_split";

/// Maximum UTF-8 bytes accepted from one model-discovery response page.
/// 单个模型发现响应分页允许接受的最大 UTF-8 字节数。
pub(crate) const MODEL_DISCOVERY_MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Parses one model-discovery response through shared byte, depth, and node-count bounds.
/// 通过共享字节、深度与节点数量上限解析一个模型发现响应。
///
/// # Parameters
/// 参数
/// `body` is the provider-controlled JSON page and `label` scopes the controlled diagnostic.
/// `body` 是供应商控制的 JSON 分页，`label` 限定受控诊断范围。
///
/// # Returns
/// 返回值
/// The requested closed response type or a deterministic size, syntax, shape, depth, or count
/// failure.
/// 返回所需封闭响应类型，或确定性的大小、语法、结构、深度或数量失败。
fn parse_discovery_json<Response>(body: &str, label: &str) -> Result<Response, String>
where
    Response: DeserializeOwned,
{
    if body.len() > MODEL_DISCOVERY_MAX_RESPONSE_BYTES {
        return Err(format!(
            "{label} exceeds the {MODEL_DISCOVERY_MAX_RESPONSE_BYTES}-byte catalog boundary."
        ));
    }
    let value: Value =
        serde_json::from_str(body).map_err(|error| format!("{label} is invalid: {error}"))?;
    crate::provider_manifest::validate_runtime_json_bounds(&value, label)?;
    serde_json::from_value(value).map_err(|error| format!("{label} is invalid: {error}"))
}

/// Parses a MiniMax Anthropic-compatible model list without inferred fields.
/// 在不推测字段的情况下解析 MiniMax Anthropic 兼容模型列表。
///
/// # Parameters
/// 参数
/// `connection_id` identifies the owning connection, `generation` is the in-memory refresh
/// generation, and `body` is the exact provider response.
/// `connection_id` 标识所属连接，`generation` 是内存刷新代次，`body` 是精确供应商响应。
///
/// # Returns
/// 返回值
/// A connection-scoped in-memory catalog or a controlled contract error.
/// 返回连接作用域内存目录或受控契约错误。
#[cfg(test)]
pub fn parse_minimax_model_catalog(
    connection_id: &str,
    generation: u64,
    body: &str,
) -> Result<ConnectionModelCatalog, String> {
    let page = parse_minimax_model_page(body)?;
    Ok(build_minimax_model_catalog(
        connection_id,
        generation,
        page.models,
        1,
    ))
}

/// Parses one documented MiniMax model-list page for strict client-side pagination.
/// 解析一个文档化的 MiniMax 模型列表分页，以供客户端严格分页。
///
/// # Parameters
/// 参数
/// `body` is the exact MiniMax Anthropic-compatible list-models response.
/// `body` 是精确的 MiniMax Anthropic 兼容模型列表响应。
///
/// # Returns
/// 返回值
/// Parsed models plus the authoritative `has_more` and `last_id` cursor fields.
/// 返回解析后的模型以及权威的 `has_more` 和 `last_id` 游标字段。
#[cfg(test)]
pub(crate) fn parse_minimax_model_page(body: &str) -> Result<MiniMaxModelPage, String> {
    let response: MiniMaxModelList = parse_discovery_json(body, "MiniMax model list")?;
    let models = response
        .data
        .into_iter()
        .map(|model| ConnectionModel {
            id: model.id,
            display_name: model.display_name,
            capabilities: conservative_discovered_text_capabilities("minimax.anthropic.models.v1"),
            capability_evidence: ModelCapabilityEvidence::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        })
        .collect();
    Ok(MiniMaxModelPage {
        models,
        has_more: response.has_more,
        last_id: response.last_id,
    })
}

/// Parses the documented MiniMax OpenAI-compatible model list.
/// 解析文档化的 MiniMax OpenAI 兼容模型列表。
///
/// # Parameters
/// 参数
/// `connection_id` identifies the exact credential, `generation` is process-local,
/// and `body` is the provider response from `/v1/models`.
/// `connection_id` 标识精确凭据，`generation` 仅存在于进程内，
/// `body` 是 `/v1/models` 返回的供应商响应。
///
/// # Returns
/// 返回值
/// A protocol-scoped MiniMax catalog with OpenAI-compatible parameter declarations.
/// 返回带 OpenAI 兼容参数声明的协议作用域 MiniMax 目录。
#[cfg(test)]
pub fn parse_minimax_openai_model_catalog(
    connection_id: &str,
    generation: u64,
    body: &str,
) -> Result<ConnectionModelCatalog, String> {
    let response: OpenAiModelList = parse_discovery_json(body, "MiniMax OpenAI model list")?;
    let models = response
        .data
        .into_iter()
        .map(|model| ConnectionModel {
            display_name: model.id.clone(),
            capabilities: conservative_discovered_text_capabilities("minimax.openai.models.v1"),
            id: model.id,
            capability_evidence: ModelCapabilityEvidence::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        })
        .collect::<Vec<_>>();
    validate_model_catalog_identities(models.as_slice(), "MiniMax OpenAI model list")?;
    Ok(ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
        page_count: 1,
        discovery_schema_revision: String::from("minimax.openai.models.v1"),
    })
}

/// Validates non-empty, unique model identifiers for one atomic catalog response.
/// 校验一份原子目录响应中的模型标识均非空且唯一。
///
/// # Parameters
/// 参数
/// `models` is the parsed page or catalog and `operation` scopes the controlled diagnostic.
/// `models` 是已解析分页或目录，`operation` 限定受控诊断。
///
/// # Returns
/// 返回值
/// Success for a stable identity set, otherwise one exact catalog-contract error.
/// 标识集合稳定时返回成功，否则返回一项精确目录契约错误。
fn validate_model_catalog_identities(
    models: &[ConnectionModel],
    operation: &str,
) -> Result<(), String> {
    // seen tracks exact upstream identities without normalizing provider-owned values.
    // seen 跟踪精确上游标识，不对供应商拥有的值做归一化。
    let mut seen = BTreeSet::new();
    for model in models {
        if model.id.trim().is_empty() {
            return Err(format!("{operation} returned an empty model identifier."));
        }
        if !seen.insert(model.id.as_str()) {
            return Err(format!(
                "{operation} returned duplicate model `{}`.",
                model.id
            ));
        }
    }
    Ok(())
}

/// Builds one complete connection-scoped MiniMax catalog from all fetched pages.
/// 从全部已获取分页构建一个完整的连接作用域 MiniMax 目录。
///
/// # Parameters
/// 参数
/// `connection_id` is the exact credential connection, `generation` is the refresh
/// generation, `models` contains every page in provider order, and `page_count` records
/// how many upstream pages were consumed.
/// `connection_id` 是精确凭据连接，`generation` 是刷新代次，`models` 包含按供应商顺序排列的全部分页，
/// `page_count` 记录实际读取的上游分页数。
///
/// # Returns
/// 返回值
/// A process-memory-only model catalog.
/// 返回一个仅存在于进程内存中的模型目录。
#[cfg(test)]
pub(crate) fn build_minimax_model_catalog(
    connection_id: &str,
    generation: u64,
    models: Vec<ConnectionModel>,
    page_count: u32,
) -> ConnectionModelCatalog {
    ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
        page_count,
        discovery_schema_revision: String::from("minimax.anthropic.models.v1"),
    }
}

/// Parses an OpenRouter model list using only documented response fields.
/// 仅使用文档化响应字段解析 OpenRouter 模型列表。
///
/// # Parameters
/// 参数
/// `connection_id` identifies the owning connection, `generation` is the in-memory refresh
/// generation, and `body` is the exact provider response.
/// `connection_id` 标识所属连接，`generation` 是内存刷新代次，`body` 是精确供应商响应。
///
/// # Returns
/// 返回值
/// A connection-scoped in-memory catalog or a controlled contract error.
/// 返回连接作用域内存目录或受控契约错误。
pub fn parse_openrouter_model_catalog(
    connection_id: &str,
    generation: u64,
    body: &str,
) -> Result<ConnectionModelCatalog, String> {
    let response: OpenRouterModelList = parse_discovery_json(body, "OpenRouter model list")?;
    let models = response
        .data
        .into_iter()
        .map(|model| {
            let capabilities = openrouter_reported_capabilities(&model);
            let capability_evidence = openrouter_capability_evidence(&model);
            // display_name removes OpenRouter's catalog publisher prefix while preserving the exact routing id.
            // display_name 移除 OpenRouter 目录中的发布商前缀，同时保留精确路由 ID。
            let display_name =
                openrouter_model_display_name(model.name.as_deref(), model.id.as_str());
            ConnectionModel {
                display_name,
                id: model.id,
                capabilities,
                capability_evidence,
                protocol_variants: BTreeMap::new(),
                resolved_protocol_binding: None,
            }
        })
        .collect::<Vec<_>>();
    validate_model_catalog_identities(models.as_slice(), "OpenRouter model list")?;
    Ok(ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
        page_count: 1,
        discovery_schema_revision: String::from("openrouter.models.v1"),
    })
}

/// Parses and atomically merges OpenRouter conversation, embedding, and rerank catalogs.
/// 解析并原子合并 OpenRouter 会话、向量与重排目录。
///
/// # Parameters
/// 参数
/// `connection_id` and `generation` identify the in-memory snapshot; the three bodies are exact
/// responses from `/models`, `/embeddings/models`, and `/models?output_modalities=all`.
/// `connection_id` 与 `generation` 标识内存快照；三个正文分别来自 `/models`、
/// `/embeddings/models` 与 `/models?output_modalities=all` 的精确响应。
///
/// # Returns
/// 返回值
/// One connection-scoped catalog only when all three source documents are valid.
/// 仅在三个来源文档均有效时返回一个连接作用域目录。
pub fn parse_openrouter_complete_model_catalog(
    connection_id: &str,
    generation: u64,
    conversation_body: &str,
    embedding_body: &str,
    complete_body: &str,
) -> Result<ConnectionModelCatalog, String> {
    let conversation =
        parse_openrouter_model_catalog(connection_id, generation, conversation_body)?;
    let embedding_response: OpenRouterModelList =
        parse_discovery_json(embedding_body, "OpenRouter embedding model list")?;
    let complete_response: OpenRouterModelList =
        parse_discovery_json(complete_body, "OpenRouter complete model list")?;

    let mut models = conversation
        .models
        .into_iter()
        .map(|model| (model.id.clone(), model))
        .collect::<BTreeMap<_, _>>();
    for model in embedding_response.data {
        let architecture = model.architecture.as_ref().ok_or_else(|| {
            format!(
                "OpenRouter embedding model `{}` is missing architecture metadata.",
                model.id
            )
        })?;
        if !architecture
            .output_modalities
            .as_deref()
            .unwrap_or_default()
            .iter()
            .any(|modality| modality == "embeddings")
        {
            return Err(format!(
                "OpenRouter embedding model `{}` does not declare the `embeddings` output modality.",
                model.id
            ));
        }
        merge_openrouter_specialized_model(
            &mut models,
            &model,
            openrouter_embedding_capabilities(&model),
        );
    }
    for model in complete_response.data {
        let is_rerank = model.architecture.as_ref().is_some_and(|architecture| {
            architecture
                .output_modalities
                .as_deref()
                .unwrap_or_default()
                .iter()
                .any(|modality| modality == "rerank")
        });
        if is_rerank {
            merge_openrouter_specialized_model(
                &mut models,
                &model,
                openrouter_rerank_capabilities(&model),
            );
        } else {
            // The complete directory also contains image, video, audio, and other dedicated
            // output models absent from the conversation endpoint. Keep their exact identities
            // for account management without inventing an executable native operation.
            // 完整目录还包含会话端点缺失的图片、视频、音频等专用输出模型；保留其精确身份
            // 供账号管理，但不凭空赋予可执行原生操作。
            models
                .entry(model.id.clone())
                .or_insert_with(|| ConnectionModel {
                    id: model.id.clone(),
                    display_name: openrouter_model_display_name(
                        model.name.as_deref(),
                        model.id.as_str(),
                    ),
                    capabilities: openrouter_reported_capabilities(&model),
                    capability_evidence: openrouter_capability_evidence(&model),
                    protocol_variants: BTreeMap::new(),
                    resolved_protocol_binding: None,
                });
        }
    }
    Ok(ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models: models.into_values().collect(),
        generation,
        page_count: 1,
        discovery_schema_revision: String::from("openrouter.complete-models.v1"),
    })
}

/// Merges one dedicated OpenRouter operation into its exact routing model identity.
/// 把一项 OpenRouter 专用操作合并到其精确路由模型身份。
///
/// # Parameters
/// 参数
/// `models` is the unpublished atomic snapshot, `source` owns display identity, and
/// `specialized` carries exactly one dedicated operation.
/// `models` 是尚未发布的原子快照，`source` 拥有显示身份，`specialized` 仅承载一项专用操作。
///
/// # Returns
/// 返回值
/// This function updates the unpublished map in place and returns no value.
/// 此函数原地更新尚未发布的映射且不返回值。
fn merge_openrouter_specialized_model(
    models: &mut BTreeMap<String, ConnectionModel>,
    source: &OpenRouterModelEntry,
    specialized: ModelCapabilities,
) {
    let source_evidence = openrouter_capability_evidence(source);
    let model = models
        .entry(source.id.clone())
        .or_insert_with(|| ConnectionModel {
            id: source.id.clone(),
            display_name: openrouter_model_display_name(source.name.as_deref(), source.id.as_str()),
            capabilities: ModelCapabilities::default(),
            capability_evidence: ModelCapabilityEvidence::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        });
    for operation in specialized.operations {
        if !model.capabilities.operations.contains(&operation) {
            model.capabilities.operations.push(operation);
        }
    }
    for resource in specialized.input_resources {
        if !model.capabilities.input_resources.contains(&resource) {
            model.capabilities.input_resources.push(resource);
        }
    }
    model.capabilities.context_limit = model
        .capabilities
        .context_limit
        .or(specialized.context_limit);
    if specialized.embedding.is_some() {
        model.capabilities.embedding = specialized.embedding;
    }
    if specialized.rerank.is_some() {
        model.capabilities.rerank = specialized.rerank;
    }
    model.capabilities.evidence_revision = String::from("openrouter-complete-catalog-api-2026-07");
    merge_model_capability_evidence(&mut model.capability_evidence, source_evidence);
}

/// Merges explicit model-directory evidence without turning omission into a negative declaration.
/// 合并模型目录的显式证据，且不会把字段缺失误解为否定声明。
///
/// # Parameters
/// 参数
/// `target` contains evidence already observed for the exact model identity and `source` contains
/// evidence from one additional atomic OpenRouter catalog response.
/// `target` 包含同一精确模型身份已观测的证据，`source` 包含另一份 OpenRouter 原子目录响应的证据。
///
/// # Returns
/// 返回值
/// This function updates `target` in place and returns no value.
/// 此函数原地更新 `target`，不返回值。
fn merge_model_capability_evidence(
    target: &mut ModelCapabilityEvidence,
    source: ModelCapabilityEvidence,
) {
    if source.context_limit_declared {
        target.context_limit_declared = true;
    }
    if source.output_limit_declared {
        target.output_limit_declared = true;
    }
    if source.input_modalities_declared {
        target.input_modalities_declared = true;
        target.supports_text_input = source.supports_text_input;
    }
    if source.output_modalities_declared {
        target.output_modalities_declared = true;
        target.supports_text_output = source.supports_text_output;
    }
    if source.structured_output_declared {
        target.structured_output_declared = true;
    }
}

/// Builds the conservative contract proven by OpenRouter's dedicated embedding catalog.
/// 构建由 OpenRouter 专用向量目录证明的保守契约。
///
/// # Parameters
/// 参数
/// `model` is one exact entry from `/embeddings/models`.
/// `model` 是 `/embeddings/models` 返回的一项精确条目。
///
/// # Returns
/// 返回值
/// Dense embedding support with only provider-default task semantics and documented encodings.
/// 返回仅含供应商默认任务语义与文档化编码的稠密向量能力。
fn openrouter_embedding_capabilities(model: &OpenRouterModelEntry) -> ModelCapabilities {
    ModelCapabilities {
        operations: vec![AiOperation::EmbeddingCreate],
        embedding: Some(EmbeddingCapabilities {
            tasks: BTreeSet::from([
                EmbeddingTask::ProviderDefault,
                EmbeddingTask::Query,
                EmbeddingTask::Document,
            ]),
            input_resources: BTreeSet::new(),
            output_kinds: BTreeSet::from([EmbeddingOutputKind::Dense]),
            encodings: BTreeSet::from([EmbeddingEncoding::Float, EmbeddingEncoding::Base64]),
            dimensions: BTreeSet::new(),
            minimum_dimensions: None,
            maximum_dimensions: None,
            default_dimensions: None,
            maximum_inputs: None,
            maximum_tokens_per_input: model.context_length,
            normalization: EmbeddingNormalization::Unknown,
            distance_metric: DistanceMetric::Unknown,
        }),
        context_limit: model.context_length,
        evidence_revision: String::from("openrouter-embeddings-models-api-2026-07"),
        ..ModelCapabilities::default()
    }
}

/// Builds the conservative text subset proven by OpenRouter's rerank output modality.
/// 构建由 OpenRouter 重排输出模态证明的保守纯文本子集。
///
/// # Parameters
/// 参数
/// `model` is one exact entry from the complete OpenRouter model list.
/// `model` 是 OpenRouter 完整模型列表中的一项精确条目。
///
/// # Returns
/// 返回值
/// Text rerank support without invented limits or unsupported truncation controls.
/// 返回不虚构限制且不暴露未支持截断控制的纯文本重排能力。
fn openrouter_rerank_capabilities(model: &OpenRouterModelEntry) -> ModelCapabilities {
    ModelCapabilities {
        operations: vec![AiOperation::RerankDocuments],
        rerank: Some(RerankCapabilities {
            query_resources: BTreeSet::new(),
            candidate_resources: BTreeSet::new(),
            maximum_candidates: None,
            maximum_tokens_per_item: model.context_length,
            maximum_request_tokens: model.context_length,
            truncation: BTreeSet::from([RerankTruncation::Disabled]),
            supports_return_content: true,
            score_semantics: RerankScoreSemantics::HigherIsMoreRelevant,
        }),
        context_limit: model.context_length,
        evidence_revision: String::from("openrouter-models-rerank-modality-2026-07"),
        ..ModelCapabilities::default()
    }
}

/// Describes one reviewed Alibaba conversation offering without provider discovery guesses.
/// 描述一项不含供应商发现猜测的已审核阿里云会话产品。
#[cfg(test)]
#[derive(Clone, Copy)]
struct AlibabaStaticConversationModel {
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    id: &'static str,
    /// User-visible model name.
    /// 用户可见模型名称。
    display_name: &'static str,
    /// Exact context-window ceiling.
    /// 精确上下文窗口上限。
    context_limit: Option<u64>,
    /// Optional verified output-token ceiling.
    /// 可选已验证输出 Token 上限。
    output_limit: Option<u64>,
    /// Whether image input is proven for this product.
    /// 当前产品是否已证明支持图片输入。
    image_input: bool,
    /// Whether video input is proven for this product.
    /// 当前产品是否已证明支持视频输入。
    video_input: bool,
}

/// Builds one exact Alibaba Token Plan product catalog.
/// 构建一个精确阿里云 Token Plan 产品目录。
///
/// # Parameters
/// 参数
/// `connection_id`, `service_entry_id`, `protocol_id`, and `generation` are frozen route facts.
/// `connection_id`、`service_entry_id`、`protocol_id` 与 `generation` 是冻结路由事实。
///
/// # Returns
/// 返回值
/// A product- and protocol-scoped static catalog or an explicit unsupported-product error.
/// 返回产品与协议作用域静态目录，或明确不支持产品错误。
#[cfg(test)]
pub fn build_alibaba_token_plan_catalog(
    connection_id: &str,
    service_entry_id: &str,
    protocol_id: &str,
    generation: u64,
) -> Result<ConnectionModelCatalog, String> {
    const PERSONAL_CN: &[AlibabaStaticConversationModel] = &[
        alibaba_static_model(
            "qwen3.8-max",
            "Qwen3.8 Max",
            1_000_000,
            Some(131_072),
            true,
            true,
        ),
        alibaba_static_model(
            "qwen3.8-flash",
            "Qwen3.8 Flash",
            1_000_000,
            Some(131_072),
            true,
            true,
        ),
        alibaba_static_model("qwen3.7-max", "Qwen3.7 Max", 1_000_000, None, false, false),
        alibaba_static_model("qwen3.7-plus", "Qwen3.7 Plus", 1_000_000, None, true, true),
        alibaba_static_model(
            "qwen3.6-flash",
            "Qwen3.6 Flash",
            1_000_000,
            None,
            true,
            false,
        ),
        alibaba_static_model(
            "deepseek-v4-pro",
            "DeepSeek V4 Pro",
            1_000_000,
            None,
            false,
            false,
        ),
        alibaba_static_model(
            "deepseek-v4-pro-0813",
            "DeepSeek V4 Pro 0813",
            1_000_000,
            None,
            false,
            false,
        ),
        alibaba_static_model_without_verified_context(
            "deepseek-v4-flash-0731",
            "DeepSeek V4 Flash 0731",
            false,
            false,
        ),
        alibaba_static_model("glm-5.2", "GLM-5.2", 1_000_000, None, false, false),
    ];
    const PERSONAL_GLOBAL: &[AlibabaStaticConversationModel] = &[
        alibaba_static_model(
            "qwen3.8-max",
            "Qwen3.8 Max",
            1_000_000,
            Some(131_072),
            true,
            true,
        ),
        alibaba_static_model(
            "qwen3.8-flash",
            "Qwen3.8 Flash",
            1_000_000,
            Some(131_072),
            true,
            true,
        ),
        alibaba_static_model("qwen3.7-max", "Qwen3.7 Max", 1_000_000, None, false, false),
        alibaba_static_model("qwen3.7-plus", "Qwen3.7 Plus", 1_000_000, None, true, true),
        alibaba_static_model(
            "qwen3.6-flash",
            "Qwen3.6 Flash",
            1_000_000,
            None,
            true,
            false,
        ),
        alibaba_static_model(
            "deepseek-v4-pro",
            "DeepSeek V4 Pro",
            1_000_000,
            None,
            false,
            false,
        ),
        alibaba_static_model(
            "deepseek-v4-pro-0813",
            "DeepSeek V4 Pro 0813",
            1_000_000,
            None,
            false,
            false,
        ),
        alibaba_static_model_without_verified_context(
            "deepseek-v4-flash-0731",
            "DeepSeek V4 Flash 0731",
            false,
            false,
        ),
        alibaba_static_model("glm-5.2", "GLM-5.2", 1_000_000, None, false, false),
    ];
    const TEAM_CN: &[AlibabaStaticConversationModel] = &[
        alibaba_static_model(
            "qwen3.8-max",
            "Qwen3.8 Max",
            1_000_000,
            Some(131_072),
            true,
            true,
        ),
        alibaba_static_model("qwen3.7-max", "Qwen3.7 Max", 1_000_000, None, false, false),
        alibaba_static_model("qwen3.7-plus", "Qwen3.7 Plus", 1_000_000, None, true, true),
        alibaba_static_model("qwen3.6-plus", "Qwen3.6 Plus", 1_000_000, None, true, true),
        alibaba_static_model(
            "qwen3.6-flash",
            "Qwen3.6 Flash",
            1_000_000,
            None,
            true,
            false,
        ),
        alibaba_static_model(
            "deepseek-v4-pro",
            "DeepSeek V4 Pro",
            1_000_000,
            None,
            false,
            false,
        ),
        alibaba_static_model(
            "deepseek-v4-flash",
            "DeepSeek V4 Flash",
            1_000_000,
            None,
            false,
            false,
        ),
        alibaba_static_model(
            "deepseek-v3.2",
            "DeepSeek V3.2",
            131_072,
            None,
            false,
            false,
        ),
        alibaba_static_model(
            "kimi-k2.7-code",
            "Kimi K2.7 Code",
            262_144,
            None,
            true,
            true,
        ),
        alibaba_static_model("kimi-k2.6", "Kimi K2.6", 262_144, None, false, false),
        alibaba_static_model("kimi-k2.5", "Kimi K2.5", 262_144, None, true, true),
        alibaba_static_model("glm-5.2", "GLM-5.2", 1_000_000, None, false, false),
        alibaba_static_model("glm-5.1", "GLM-5.1", 198_000, None, false, false),
        alibaba_static_model("glm-5", "GLM-5", 198_000, None, false, false),
        alibaba_static_model("MiniMax-M2.5", "MiniMax M2.5", 196_608, None, false, false),
    ];
    const TEAM_GLOBAL: &[AlibabaStaticConversationModel] = &[
        alibaba_static_model("qwen3.7-max", "Qwen3.7 Max", 1_000_000, None, false, false),
        alibaba_static_model("qwen3.7-plus", "Qwen3.7 Plus", 1_000_000, None, true, true),
        alibaba_static_model("qwen3.6-plus", "Qwen3.6 Plus", 1_000_000, None, true, true),
        alibaba_static_model(
            "qwen3.6-flash",
            "Qwen3.6 Flash",
            1_000_000,
            None,
            true,
            false,
        ),
        alibaba_static_model(
            "deepseek-v4-pro",
            "DeepSeek V4 Pro",
            1_000_000,
            None,
            false,
            false,
        ),
        alibaba_static_model(
            "deepseek-v4-flash",
            "DeepSeek V4 Flash",
            1_000_000,
            None,
            false,
            false,
        ),
        alibaba_static_model(
            "deepseek-v3.2",
            "DeepSeek V3.2",
            128_000,
            None,
            false,
            false,
        ),
        alibaba_static_model(
            "kimi-k2.7-code",
            "Kimi K2.7 Code",
            256_000,
            None,
            true,
            true,
        ),
        alibaba_static_model("kimi-k2.6", "Kimi K2.6", 256_000, None, false, false),
        alibaba_static_model("kimi-k2.5", "Kimi K2.5", 256_000, None, true, true),
        alibaba_static_model("glm-5.2", "GLM-5.2", 198_000, None, false, false),
        alibaba_static_model("glm-5.1", "GLM-5.1", 198_000, None, false, false),
        alibaba_static_model("glm-5", "GLM-5", 198_000, None, false, false),
        alibaba_static_model("MiniMax-M2.5", "MiniMax M2.5", 192_000, None, false, false),
    ];
    let definitions = match service_entry_id {
        "token-plan-personal-cn" => PERSONAL_CN,
        "token-plan-personal-global" => PERSONAL_GLOBAL,
        "token-plan-team-cn" => TEAM_CN,
        "token-plan-team-global" => TEAM_GLOBAL,
        unsupported => {
            return Err(format!(
                "Alibaba static Token Plan catalog is not registered for service entry `{unsupported}`."
            ));
        }
    };
    build_alibaba_static_conversation_catalog(
        connection_id,
        service_entry_id,
        protocol_id,
        generation,
        definitions,
    )
}

/// Builds one exact Alibaba Model Studio regional catalog.
/// 构建一个精确阿里云百炼 Model Studio 区域目录。
///
/// # Parameters
/// 参数
/// `connection_id`, `service_entry_id`, `protocol_id`, and `generation` are frozen route facts.
/// `connection_id`、`service_entry_id`、`protocol_id` 与 `generation` 是冻结路由事实。
///
/// # Returns
/// 返回值
/// A reviewed regional catalog including conversation, embedding, and rerank operations.
/// 返回包含会话、向量与重排操作的已审核区域目录。
#[cfg(test)]
pub fn build_alibaba_model_studio_catalog(
    connection_id: &str,
    service_entry_id: &str,
    protocol_id: &str,
    generation: u64,
) -> Result<ConnectionModelCatalog, String> {
    const CHINA: &[AlibabaStaticConversationModel] = &[
        alibaba_static_model(
            "qwen3.5-flash",
            "Qwen3.5 Flash",
            1_000_000,
            Some(65_536),
            true,
            true,
        ),
        alibaba_static_model(
            "qwen3.5-plus",
            "Qwen3.5 Plus",
            1_000_000,
            Some(65_536),
            true,
            true,
        ),
        alibaba_static_model(
            "qwen3.6-plus",
            "Qwen3.6 Plus",
            1_000_000,
            Some(65_536),
            true,
            true,
        ),
        alibaba_static_model(
            "qwen3.7-plus",
            "Qwen3.7 Plus",
            1_000_000,
            Some(65_536),
            true,
            true,
        ),
        alibaba_static_model(
            "qwen3.7-max",
            "Qwen3.7 Max",
            1_000_000,
            Some(65_536),
            false,
            false,
        ),
        alibaba_static_model(
            "qwen3.7-max-2026-06-08",
            "Qwen3.7 Max 2026-06-08",
            1_000_000,
            Some(65_536),
            true,
            true,
        ),
        alibaba_static_model("glm-5.1", "GLM-5.1", 202_752, Some(131_072), false, false),
        alibaba_static_model(
            "glm-5.2-fast-preview",
            "GLM-5.2 Fast Preview",
            1_048_576,
            Some(131_072),
            false,
            false,
        ),
        alibaba_static_model(
            "ZHIPU/GLM-5.2",
            "ZHIPU GLM-5.2",
            1_048_576,
            Some(131_072),
            false,
            false,
        ),
        alibaba_static_model(
            "kimi/kimi-k3",
            "Kimi K3",
            1_048_576,
            Some(1_048_576),
            true,
            true,
        ),
        alibaba_static_model(
            "MiniMax/MiniMax-M3",
            "MiniMax M3",
            1_048_576,
            Some(32_768),
            true,
            true,
        ),
        alibaba_static_model(
            "stepfun/step-3.7-flash",
            "Step 3.7 Flash",
            262_144,
            Some(262_144),
            true,
            true,
        ),
        alibaba_static_model(
            "xiaomi/mimo-v2.5-pro",
            "Xiaomi MiMo V2.5 Pro",
            1_048_576,
            Some(131_072),
            false,
            false,
        ),
        alibaba_static_model(
            "deepseek-v4-pro",
            "DeepSeek V4 Pro",
            1_000_000,
            Some(393_216),
            false,
            false,
        ),
        alibaba_static_model(
            "deepseek-v4-flash",
            "DeepSeek V4 Flash",
            1_000_000,
            Some(393_216),
            false,
            false,
        ),
    ];
    const SINGAPORE: &[AlibabaStaticConversationModel] = &[
        alibaba_static_model(
            "qwen3.5-flash",
            "Qwen3.5 Flash",
            1_000_000,
            Some(65_536),
            true,
            true,
        ),
        alibaba_static_model(
            "qwen3.5-plus",
            "Qwen3.5 Plus",
            1_000_000,
            Some(65_536),
            true,
            true,
        ),
        alibaba_static_model(
            "qwen3.6-plus",
            "Qwen3.6 Plus",
            1_000_000,
            Some(65_536),
            true,
            true,
        ),
        alibaba_static_model(
            "glm-5.2-fast-preview",
            "GLM-5.2 Fast Preview",
            1_048_576,
            Some(131_072),
            false,
            false,
        ),
        alibaba_static_model(
            "deepseek-v4-flash",
            "DeepSeek V4 Flash",
            1_000_000,
            Some(393_216),
            false,
            false,
        ),
    ];
    let definitions = match service_entry_id {
        "model-studio-cn" => CHINA,
        "model-studio-singapore" => SINGAPORE,
        unsupported => {
            return Err(format!(
                "Alibaba static Model Studio catalog is not registered for service entry `{unsupported}`."
            ));
        }
    };
    let mut catalog = build_alibaba_static_conversation_catalog(
        connection_id,
        service_entry_id,
        protocol_id,
        generation,
        definitions,
    )?;
    let native_models = if service_entry_id == "model-studio-cn" {
        vec![
            ("text-embedding-v1", "Text Embedding V1"),
            ("text-embedding-v2", "Text Embedding V2"),
            ("text-embedding-v4", "Text Embedding V4"),
            ("text-embedding-v3", "Text Embedding V3"),
            ("qwen3-rerank", "Qwen3 Rerank"),
            ("qwen3.7-text-embedding", "Qwen3.7 Text Embedding"),
            (
                "qwen3.7-text-embedding-flash",
                "Qwen3.7 Text Embedding Flash",
            ),
            ("qwen3.7-text-rerank", "Qwen3.7 Text Rerank"),
        ]
    } else {
        vec![
            ("text-embedding-v4", "Text Embedding V4"),
            ("text-embedding-v3", "Text Embedding V3"),
            ("qwen3-rerank", "Qwen3 Rerank"),
        ]
    };
    for (id, display_name) in native_models {
        let model = ConnectionModel {
            id: id.to_string(),
            display_name: display_name.to_string(),
            capabilities: ModelCapabilities::default(),
            capability_evidence: ModelCapabilityEvidence::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        };
        // The bundled manifest rules carry these reviewed embedding/rerank contracts.
        // 已审核向量/重排契约由内置清单规则承载。
        catalog.models.push(model);
    }
    Ok(catalog)
}

/// Creates one compile-time Alibaba static model definition.
/// 创建一项编译期阿里云静态模型定义。
///
/// # Parameters
/// 参数
/// All arguments are copied from reviewed product evidence.
/// 全部参数均复制自已审核产品证据。
///
/// # Returns
/// 返回值
/// One immutable static definition.
/// 返回一项不可变静态定义。
#[cfg(test)]
const fn alibaba_static_model(
    id: &'static str,
    display_name: &'static str,
    context_limit: u64,
    output_limit: Option<u64>,
    image_input: bool,
    video_input: bool,
) -> AlibabaStaticConversationModel {
    AlibabaStaticConversationModel {
        id,
        display_name,
        context_limit: Some(context_limit),
        output_limit,
        image_input,
        video_input,
    }
}

/// Creates one static Alibaba model whose context ceiling is not yet published as exact evidence.
/// 创建一个尚未发布精确上下文上限证据的阿里云静态模型。
///
/// # Parameters
/// 参数
/// The arguments preserve only the verified identity and input-modality facts.
/// 参数仅保留已验证的模型身份与输入模态事实。
///
/// # Returns
/// 返回值
/// A static model definition without an invented context or output limit.
/// 返回不虚构上下文或输出上限的静态模型定义。
#[cfg(test)]
const fn alibaba_static_model_without_verified_context(
    id: &'static str,
    display_name: &'static str,
    image_input: bool,
    video_input: bool,
) -> AlibabaStaticConversationModel {
    AlibabaStaticConversationModel {
        id,
        display_name,
        context_limit: None,
        output_limit: None,
        image_input,
        video_input,
    }
}

/// Builds one protocol-scoped catalog from reviewed Alibaba product definitions.
/// 根据已审核阿里云产品定义构建一个协议作用域目录。
///
/// # Parameters
/// 参数
/// The route identity and definition slice are exact code-owned facts.
/// 路由身份与定义切片是精确代码持有事实。
///
/// # Returns
/// 返回值
/// A deterministic catalog whose Responses subset is kept separate from Chat.
/// 返回将 Responses 子集与 Chat 分离的确定性目录。
#[cfg(test)]
fn build_alibaba_static_conversation_catalog(
    connection_id: &str,
    service_entry_id: &str,
    protocol_id: &str,
    generation: u64,
    definitions: &[AlibabaStaticConversationModel],
) -> Result<ConnectionModelCatalog, String> {
    if !matches!(
        protocol_id,
        OPENAI_CHAT_PROFILE_ID | ANTHROPIC_MESSAGES_PROFILE_ID | OPENAI_RESPONSES_PROFILE_ID
    ) {
        return Err(format!(
            "Alibaba static catalog does not support protocol `{protocol_id}`."
        ));
    }
    let responses_harness = protocol_id == OPENAI_RESPONSES_PROFILE_ID;
    let models = definitions
        .iter()
        .filter(|definition| {
            !responses_harness
                || matches!(
                    definition.id,
                    "qwen3.8-max" | "qwen3.8-flash" | "qwen3.7-max" | "qwen3.7-plus"
                )
        })
        .map(|definition| {
            let mut input_resources = Vec::new();
            if !responses_harness && definition.image_input {
                input_resources.push(ResourceKind::Image);
            }
            if !responses_harness && definition.video_input {
                input_resources.push(ResourceKind::Video);
            }
            let mut model = ConnectionModel {
                id: definition.id.to_string(),
                display_name: if responses_harness {
                    format!("{} Harness", definition.display_name)
                } else {
                    definition.display_name.to_string()
                },
                capabilities: ModelCapabilities {
                    operations: vec![AiOperation::ConversationRespond],
                    input_resources,
                    context_limit: definition.context_limit,
                    output_limit: definition.output_limit,
                    evidence_revision: if service_entry_id == "token-plan-personal-global" {
                        String::from("alibaba.token-plan.personal-global.2026-08-01")
                    } else {
                        format!("model-core.alibaba.{service_entry_id}.2026-07-30")
                    },
                    ..ModelCapabilities::default()
                },
                capability_evidence: ModelCapabilityEvidence::default(),
                protocol_variants: BTreeMap::new(),
                resolved_protocol_binding: None,
            };
            if responses_harness {
                model.capabilities.reasoning = CapabilitySupport::Unsupported;
                model.capabilities.reasoning_control = ReasoningControlMode::FixedOff;
            } else {
                enrich_alibaba_model(service_entry_id, &mut model);
            }
            model
        })
        .collect();
    Ok(ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
        page_count: 1,
        discovery_schema_revision: String::from("alibaba.models.v1"),
    })
}

/// Returns the user-facing model name from one OpenRouter catalog entry.
/// 从单个 OpenRouter 目录条目返回面向用户的模型名称。
///
/// # Parameters
/// 参数
/// `reported_name` is OpenRouter's optional publisher-qualified name and `model_id`
/// is the exact routing identifier used when no display name is reported.
/// `reported_name` 是 OpenRouter 可选的发布商限定名称，`model_id`
/// 是未报告显示名称时使用的精确路由标识。
///
/// # Returns
/// 返回值
/// The model portion after OpenRouter's first publisher separator, or the exact
/// reported name/model identifier when the separator is absent.
/// 返回 OpenRouter 首个发布商分隔符后的模型部分；若不存在分隔符，
/// 则返回精确报告名称或模型标识。
fn openrouter_model_display_name(reported_name: Option<&str>, model_id: &str) -> String {
    // normalized_name rejects blank catalog names before applying the OpenRouter display convention.
    // normalized_name 在应用 OpenRouter 显示约定前拒绝空白目录名称。
    let Some(normalized_name) = reported_name.map(str::trim).filter(|name| !name.is_empty()) else {
        return model_id.to_string();
    };
    if let Some((publisher, model_name)) = normalized_name.split_once(':')
        && !publisher.trim().is_empty()
        && !model_name.trim().is_empty()
    {
        return model_name.trim().to_string();
    }
    normalized_name.to_string()
}

/// Parses an OpenAI model list without inferring unavailable capabilities.
/// 在不推测缺失能力的情况下解析 OpenAI 模型列表。
///
/// # Parameters
/// 参数
/// `connection_id` identifies the exact credential, `generation` is process-local, and `body`
/// is the documented OpenAI models response.
/// `connection_id` 标识精确凭据，`generation` 仅存在于进程内，`body` 是文档化 OpenAI 模型响应。
///
/// # Returns
/// 返回值
/// A conservative memory-only catalog.
/// 返回保守的纯内存目录。
pub fn parse_openai_model_catalog(
    connection_id: &str,
    generation: u64,
    body: &str,
) -> Result<ConnectionModelCatalog, String> {
    let response: OpenAiModelList = parse_discovery_json(body, "OpenAI model list")?;
    let models = response
        .data
        .into_iter()
        .map(|model| {
            let input_resources = model
                .input_modalities
                .as_deref()
                .map(compatible_resource_kinds)
                .unwrap_or_default();
            let output_resources = model
                .output_modalities
                .as_deref()
                .map(compatible_resource_kinds)
                .unwrap_or_default();
            let supports_text_input = model
                .input_modalities
                .as_ref()
                .is_none_or(|modalities| compatible_supports_text(modalities));
            let supports_text_output = model
                .output_modalities
                .as_ref()
                .is_none_or(|modalities| compatible_supports_text(modalities));
            let capability_evidence = ModelCapabilityEvidence {
                context_limit_declared: model.context_window.is_some(),
                output_limit_declared: model.max_output_tokens.is_some(),
                input_modalities_declared: model.input_modalities.is_some(),
                output_modalities_declared: model.output_modalities.is_some(),
                structured_output_declared: false,
                supports_text_input: model
                    .input_modalities
                    .as_ref()
                    .map(|modalities| compatible_supports_text(modalities)),
                supports_text_output: model
                    .output_modalities
                    .as_ref()
                    .map(|modalities| compatible_supports_text(modalities)),
            };
            let capabilities = ModelCapabilities {
                operations: vec![AiOperation::ConversationRespond],
                context_limit: model.context_window,
                output_limit: model.max_output_tokens,
                input_resources,
                output_resources,
                evidence_revision: String::from("openai-compatible.models.v1"),
                ..ModelCapabilities::default()
            };
            let mut capabilities = capabilities;
            if !supports_text_input || !supports_text_output {
                capabilities
                    .operations
                    .retain(|operation| *operation != AiOperation::ConversationRespond);
            }
            ConnectionModel {
                display_name: model.display_name.unwrap_or_else(|| model.id.clone()),
                id: model.id,
                capabilities,
                capability_evidence,
                protocol_variants: BTreeMap::new(),
                resolved_protocol_binding: None,
            }
        })
        .collect::<Vec<_>>();
    validate_model_catalog_identities(models.as_slice(), "OpenAI model list")?;
    Ok(ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
        page_count: 1,
        discovery_schema_revision: String::from("openai-compatible.models.v1"),
    })
}

/// Applies the code-owned migration oracle to one compatible catalog in tests.
/// 在测试中把代码持有的迁移对照基线应用到一个兼容目录。
///
/// # Parameters
/// 参数
/// `provider_id`, `service_entry_id`, and `protocol_id` identify the exact route;
/// `catalog` is the already parsed credential-visible model list.
/// `provider_id`、`service_entry_id` 与 `protocol_id` 标识精确路由；
/// `catalog` 是已解析且对当前凭据可见的模型列表。
#[cfg(test)]
pub fn enrich_compatible_model_catalog_for_migration_oracle(
    provider_id: &str,
    service_entry_id: &str,
    protocol_id: &str,
    catalog: &mut ConnectionModelCatalog,
) {
    for model in &mut catalog.models {
        if !provider_id.starts_with("custom:") {
            let discovered = model.capabilities.clone();
            if !crate::apply_common_model_capability_defaults(
                model.id.as_str(),
                &mut model.capabilities,
            ) {
                model.capabilities = discovered;
            }
        }
        apply_migration_oracle_model_capabilities(
            provider_id,
            service_entry_id,
            protocol_id,
            model,
        );
    }
}

/// Enriches a discovered catalog with manifest-declared model rules first.
/// 先用 Manifest 声明的模型规则补全已发现目录。
///
/// Models that match a manifest rule receive its inherited and protocol-scoped capability
/// templates. An unmatched model receives the exact entry default only when
/// `default_binding_allowed`; explicit discovery limits/modalities remain bounded evidence.
/// 命中 Manifest 规则的模型获得其继承与协议作用域能力模板。未命中模型仅在
/// `default_binding_allowed` 下获得精确入口默认值；明确发现到的限制与模态仍作为有界证据保留。
pub fn enrich_compatible_model_catalog_with_manifest(
    provider_id: &str,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    protocol_id: &str,
    catalog: &mut ConnectionModelCatalog,
    resolved: &ResolvedProviderManifest,
) {
    for model in &mut catalog.models {
        if !provider_id.starts_with("custom:") {
            let discovered = model.capabilities.clone();
            if !crate::apply_common_model_capability_defaults(
                model.id.as_str(),
                &mut model.capabilities,
            ) {
                model.capabilities = discovered;
            }
        }
        let matched = apply_manifest_model_rule(
            resolved,
            service_entry_id,
            credential_profile_id,
            protocol_id,
            model,
        );
        if !matched
            && resolved.unknown_model_policy
                == crate::provider_manifest::UnknownModelPolicy::DefaultBindingAllowed
            && let Ok(defaults) = resolved.model_defaults_for_entry(service_entry_id)
            && let Some(default_capabilities) = defaults.capabilities.as_ref()
        {
            // Dedicated discovery endpoints prove specialized operations, and the signed native
            // binding must independently admit the same exact model before either survives the
            // generic conversation default.
            // 专用发现端点证明特殊操作；签名原生绑定也必须独立准入同一精确模型，
            // 该操作才能在通用会话默认能力投影后保留。
            let signed_native_operations = resolved
                .descriptor
                .service_entries
                .iter()
                .find(|entry| entry.id == service_entry_id)
                .map(|entry| {
                    entry
                        .native_inference_bindings
                        .iter()
                        .filter(|binding| {
                            binding.batch.is_none()
                                && (binding.model_ids.is_empty()
                                    || binding.model_ids.iter().any(|id| id == &model.id))
                        })
                        .map(|binding| binding.operation)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            apply_manifest_default_capabilities_with_discovery_evidence(
                model,
                default_capabilities,
                signed_native_operations.as_slice(),
            );
        }
    }
}

/// Applies a signed default capability snapshot while retaining explicitly declared discovery evidence.
/// 应用签名默认能力快照，同时保留发现结果中明确声明的证据。
///
/// # Parameters
/// 参数
/// `model` owns raw discovery evidence; `defaults` is the compiled entry baseline;
/// `signed_native_operations` contains exact non-batch actions authorized by that entry.
/// `model` 持有原始发现证据；`defaults` 是已编译入口基线；`signed_native_operations`
/// 包含该入口精确授权的非批处理动作。
fn apply_manifest_default_capabilities_with_discovery_evidence(
    model: &mut ConnectionModel,
    defaults: &ModelCapabilities,
    signed_native_operations: &[AiOperation],
) {
    let discovered = model.capabilities.clone();
    let evidence = &model.capability_evidence;
    let mut capabilities = defaults.clone();
    if evidence.context_limit_declared {
        capabilities.context_limit = discovered.context_limit;
    }
    if evidence.output_limit_declared {
        capabilities.output_limit = discovered.output_limit;
    }
    if evidence.input_modalities_declared {
        capabilities.input_resources = discovered.input_resources;
    }
    if evidence.output_modalities_declared {
        capabilities.output_resources = discovered.output_resources;
    }
    // Explicit non-text evidence may narrow a signed conversation default, but discovery never
    // grants an operation omitted by the signed snapshot.
    // 明确的非文本证据可以收紧签名会话默认值，但发现结果绝不会授予签名快照未声明的操作。
    if evidence.supports_text_input == Some(false) || evidence.supports_text_output == Some(false) {
        capabilities
            .operations
            .retain(|operation| *operation != AiOperation::ConversationRespond);
    }
    if signed_native_operations.contains(&AiOperation::EmbeddingCreate)
        && discovered
            .operations
            .contains(&AiOperation::EmbeddingCreate)
        && let Some(embedding) = discovered.embedding
    {
        capabilities.operations.push(AiOperation::EmbeddingCreate);
        capabilities.embedding = Some(embedding);
    }
    if signed_native_operations.contains(&AiOperation::RerankDocuments)
        && discovered
            .operations
            .contains(&AiOperation::RerankDocuments)
        && let Some(rerank) = discovered.rerank
    {
        capabilities.operations.push(AiOperation::RerankDocuments);
        capabilities.rerank = Some(rerank);
    }
    capabilities.evidence_revision = format!(
        "{}+discovery:{}",
        capabilities.evidence_revision, discovered.evidence_revision
    );
    model.capabilities = capabilities;
}

/// Applies one manifest model rule to a discovered model.
/// 将一条 Manifest 模型规则应用到已发现模型。
///
/// # Returns
/// 返回值
/// `true` when a rule matched and its templates were applied; `false` when no rule covers
/// the model in the exact service entry.
/// 命中规则并应用其模板时返回 `true`；精确服务入口内无规则覆盖该模型时返回 `false`。
pub fn apply_manifest_model_rule(
    resolved: &ResolvedProviderManifest,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    protocol_id: &str,
    model: &mut ConnectionModel,
) -> bool {
    let Some(instance) =
        resolved.rule_for_model(service_entry_id, credential_profile_id, model.id.as_str())
    else {
        return false;
    };
    if let Some(display_name) = &instance.display_name {
        model.display_name = display_name.clone();
    }
    // Protocol-independent templates form the base; protocol-scoped templates override them.
    // 协议无关模板构成基座；协议作用域模板在其上覆盖。
    if let Some(template) = &instance.capabilities {
        apply_resolved_capability_template(
            &mut model.capabilities,
            template,
            &instance.capability_provenance,
            None,
        );
    }
    if let Some(template) = instance.protocol_capabilities.get(protocol_id) {
        apply_resolved_capability_template(
            &mut model.capabilities,
            template,
            &instance.capability_provenance,
            Some(protocol_id),
        );
    }
    // Model parameters are merged by stable id when the rule declares them; parameters with
    // an explicit protocol allowlist apply only to that protocol.
    // 规则声明模型参数时按稳定 ID 合并；带显式协议白名单的参数仅作用于该协议。
    for parameter in &instance.model_parameters {
        let applies = parameter.protocol_ids.is_empty()
            || parameter.protocol_ids.iter().any(|id| id == protocol_id);
        if !applies {
            continue;
        }
        if let Some(existing) = model
            .capabilities
            .model_parameters
            .iter_mut()
            .find(|existing| existing.id == parameter.id)
        {
            // A model-rule declaration is higher-precedence authority than discovery or
            // provider defaults for the same stable parameter identity (plan §14.3/§15.2).
            // 对同一稳定参数身份，模型规则声明的优先级高于发现结果或供应商默认
            // （方案 §14.3/§15.2）。
            *existing = parameter.clone();
        } else {
            model.capabilities.model_parameters.push(parameter.clone());
        }
    }
    model.capabilities.evidence_revision = format!(
        "manifest:{}.{}:{}",
        resolved.descriptor.id, instance.rule_id, resolved.revision
    );
    true
}

/// Builds the provider-neutral baseline for an identity-only text discovery response.
/// 为仅含身份的文本发现响应构建供应商无关基线。
///
/// # Parameters
/// 参数
/// `evidence_revision` identifies the exact upstream response schema.
/// `evidence_revision` 标识精确的上游响应 Schema。
///
/// # Returns
/// 返回值
/// Conversation membership only; every vendor/model difference must come from a manifest rule.
/// 仅返回会话成员关系；全部供应商/模型差异必须来自 Manifest 规则。
#[cfg(test)]
fn conservative_discovered_text_capabilities(evidence_revision: &str) -> ModelCapabilities {
    ModelCapabilities {
        operations: vec![AiOperation::ConversationRespond],
        evidence_revision: evidence_revision.to_string(),
        ..ModelCapabilities::default()
    }
}

/// Applies one compiled manifest capability template with its exact presence map.
/// 使用精确 presence 映射应用一个已编译 Manifest 能力模板。
///
/// # Parameters
/// 参数
/// `target` is the discovery evidence, `template` is the compiled value, `provenance` records
/// every field actually declared by inheritance, and `protocol_id` selects protocol-scoped keys.
/// `target` 是发现证据，`template` 是编译值，`provenance` 记录继承后实际声明的每个字段，
/// `protocol_id` 选择协议作用域键。
///
/// Explicit empty lists and explicit `null` values are authoritative. Omitted fields never
/// overwrite discovery evidence, even when their compiled Rust default is empty or unknown.
/// 显式空列表与显式 `null` 具有权威性；省略字段绝不覆盖发现证据，即使其 Rust 编译默认值
/// 为空或未知。
fn apply_resolved_capability_template(
    target: &mut ModelCapabilities,
    template: &ModelCapabilities,
    provenance: &BTreeMap<String, crate::provider_manifest::ResolvedFieldProvenance>,
    protocol_id: Option<&str>,
) {
    let field_key = |field: &str| match protocol_id {
        Some(protocol_id) => format!("protocol.{protocol_id}.{field}"),
        None => field.to_string(),
    };
    let declared = |field: &str| provenance.contains_key(field_key(field).as_str());
    if declared("operations") {
        target.operations = template.operations.clone();
    }
    if declared("input_resources") {
        target.input_resources = template.input_resources.clone();
    }
    if declared("output_resources") {
        target.output_resources = template.output_resources.clone();
    }
    if declared("structured_output") {
        target.structured_output = template.structured_output;
    }
    if declared("embedding") {
        target.embedding = template.embedding.clone();
    }
    if declared("rerank") {
        target.rerank = template.rerank.clone();
    }
    if declared("reasoning") {
        target.reasoning = template.reasoning;
    }
    if declared("provider_native_tools") {
        target.provider_native_tools = template.provider_native_tools.clone();
    }
    if declared("reasoning_control") {
        target.reasoning_control = template.reasoning_control;
    }
    if declared("reasoning_efforts") {
        target.reasoning_efforts = template.reasoning_efforts.clone();
    }
    if declared("context_limit") {
        target.context_limit = template.context_limit;
    }
    if declared("output_limit") {
        target.output_limit = template.output_limit;
    }
    if declared("prompt_cache") {
        target.prompt_cache = template.prompt_cache.clone();
    }
    if declared("model_parameters") {
        target.model_parameters = template.model_parameters.clone();
    }
    if declared("evidence_revision") {
        target.evidence_revision = template.evidence_revision.clone();
    }
}

/// Applies the code-owned migration oracle branch for one model in tests.
/// 在测试中为单个模型应用代码持有的迁移对照分支。
#[cfg(test)]
fn apply_migration_oracle_model_capabilities(
    provider_id: &str,
    service_entry_id: &str,
    protocol_id: &str,
    model: &mut ConnectionModel,
) {
    match provider_id {
        "openai" => enrich_openai_model(model),
        "kimi" => enrich_kimi_model(service_entry_id, protocol_id, model),
        "deepseek" => enrich_deepseek_model(protocol_id, model),
        "alibaba" => enrich_alibaba_model(service_entry_id, model),
        _ => {}
    }
}

/// Applies official OpenAI reasoning metadata after provider identity is known.
/// 在供应商身份已知后应用 OpenAI 官方思考元数据。
///
/// # Parameters
/// 参数
/// `model` is one model parsed from the official OpenAI catalog.
/// `model` 是从 OpenAI 官方目录解析出的一个模型。
#[cfg(test)]
fn enrich_openai_model(model: &mut ConnectionModel) {
    let reasoning = fixed_reasoning_capabilities(FixedReasoningProvider::OpenAi, model.id.as_str());
    model.capabilities.reasoning = reasoning.support;
    model.capabilities.reasoning_efforts = reasoning.efforts;
    model.capabilities.evidence_revision = if reasoning.support == CapabilitySupport::Unknown {
        String::from("openai.models.v1")
    } else {
        reasoning.evidence_revision.to_string()
    };
}

/// Applies the closed Kimi capability mapping copied from the verified Model Core catalog.
/// 应用从已验证 Model Core 目录同步的封闭 Kimi 能力映射。
///
/// # Parameters
/// 参数
/// `service_entry_id` and `protocol_id` identify the product route; `model` is mutated in place.
/// `service_entry_id` 与 `protocol_id` 标识产品路由；`model` 会被原位更新。
#[cfg(test)]
fn enrich_kimi_model(service_entry_id: &str, protocol_id: &str, model: &mut ConnectionModel) {
    let (
        display_name,
        context_limit,
        reasoning,
        reasoning_control,
        reasoning_efforts,
        input_resources,
    ) = match (service_entry_id, model.id.as_str()) {
        ("coding-plan", "kimi-for-coding") => (
            "Kimi K2.7 Code",
            Some(262_144),
            CapabilitySupport::Native,
            ReasoningControlMode::FixedOn,
            Vec::new(),
            vec![ResourceKind::Image, ResourceKind::Video],
        ),
        ("coding-plan", "kimi-for-coding-highspeed") => (
            "Kimi K2.7 Code HighSpeed",
            Some(262_144),
            CapabilitySupport::Native,
            ReasoningControlMode::FixedOn,
            Vec::new(),
            vec![ResourceKind::Image, ResourceKind::Video],
        ),
        ("coding-plan", "k3-256k") => (
            "Kimi K3 256K",
            Some(262_144),
            CapabilitySupport::Native,
            ReasoningControlMode::EffortSelection,
            vec![
                ReasoningEffort::Low,
                ReasoningEffort::High,
                ReasoningEffort::Max,
            ],
            vec![ResourceKind::Image],
        ),
        ("coding-plan", "k3") => (
            "Kimi K3",
            Some(1_048_576),
            CapabilitySupport::Native,
            ReasoningControlMode::EffortSelection,
            vec![
                ReasoningEffort::Low,
                ReasoningEffort::High,
                ReasoningEffort::Max,
            ],
            vec![ResourceKind::Image, ResourceKind::Video],
        ),
        (_, "kimi-k3") => (
            "Kimi K3",
            Some(1_048_576),
            CapabilitySupport::Native,
            ReasoningControlMode::EffortSelection,
            vec![
                ReasoningEffort::Low,
                ReasoningEffort::High,
                ReasoningEffort::Max,
            ],
            vec![ResourceKind::Image, ResourceKind::Video],
        ),
        (_, "kimi-k2.7-code") => (
            "Kimi K2.7 Code",
            Some(262_144),
            CapabilitySupport::Native,
            ReasoningControlMode::FixedOn,
            Vec::new(),
            vec![ResourceKind::Image, ResourceKind::Video],
        ),
        (_, "kimi-k2.7-code-highspeed") => (
            "Kimi K2.7 Code HighSpeed",
            Some(262_144),
            CapabilitySupport::Native,
            ReasoningControlMode::FixedOn,
            Vec::new(),
            vec![ResourceKind::Image, ResourceKind::Video],
        ),
        (_, "kimi-k2.6") => (
            "Kimi K2.6",
            Some(262_144),
            CapabilitySupport::Unknown,
            ReasoningControlMode::Unknown,
            Vec::new(),
            vec![ResourceKind::Image, ResourceKind::Video],
        ),
        (_, "kimi-k2.5") => (
            "Kimi K2.5",
            Some(262_144),
            CapabilitySupport::Unknown,
            ReasoningControlMode::Unknown,
            Vec::new(),
            vec![ResourceKind::Image],
        ),
        (_, "moonshot-v1-8k") => (
            "Moonshot V1 8K",
            Some(8_192),
            CapabilitySupport::Unsupported,
            ReasoningControlMode::Unknown,
            Vec::new(),
            Vec::new(),
        ),
        (_, "moonshot-v1-32k") => (
            "Moonshot V1 32K",
            Some(32_768),
            CapabilitySupport::Unsupported,
            ReasoningControlMode::Unknown,
            Vec::new(),
            Vec::new(),
        ),
        (_, "moonshot-v1-128k") => (
            "Moonshot V1 128K",
            Some(131_072),
            CapabilitySupport::Unsupported,
            ReasoningControlMode::Unknown,
            Vec::new(),
            Vec::new(),
        ),
        (_, "moonshot-v1-8k-vision-preview") => (
            "Moonshot V1 8K Vision",
            Some(8_192),
            CapabilitySupport::Unsupported,
            ReasoningControlMode::Unknown,
            Vec::new(),
            vec![ResourceKind::Image],
        ),
        (_, "moonshot-v1-32k-vision-preview") => (
            "Moonshot V1 32K Vision",
            Some(32_768),
            CapabilitySupport::Unsupported,
            ReasoningControlMode::Unknown,
            Vec::new(),
            vec![ResourceKind::Image],
        ),
        (_, "moonshot-v1-128k-vision-preview") => (
            "Moonshot V1 128K Vision",
            Some(131_072),
            CapabilitySupport::Unsupported,
            ReasoningControlMode::Unknown,
            Vec::new(),
            vec![ResourceKind::Image],
        ),
        _ => return,
    };
    let input_resources = if protocol_id == ANTHROPIC_MESSAGES_PROFILE_ID {
        // The standard Anthropic route has an image encoder; Kimi's dedicated video route owns video encoding.
        // 标准 Anthropic 路由只有图片编码器；视频由 Kimi 专用视频路由负责编码。
        input_resources
            .into_iter()
            .filter(|resource| *resource == ResourceKind::Image)
            .collect()
    } else if protocol_id == ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID {
        // The dedicated Kimi video route keeps the model-declared image and video resources.
        // Kimi 专用视频路由保留模型声明的图片与视频资源。
        input_resources
    } else {
        input_resources
    };
    model.display_name = display_name.to_string();
    model.capabilities.context_limit = context_limit;
    model.capabilities.input_resources = input_resources;
    model.capabilities.reasoning = reasoning;
    model.capabilities.reasoning_control = reasoning_control;
    model.capabilities.reasoning_efforts = reasoning_efforts;
    model.capabilities.evidence_revision = format!("model-core.kimi.2026-07-29.{protocol_id}");
}

/// Applies the exact current DeepSeek official text-model capability contract.
/// 应用当前 DeepSeek 官方文本模型的精确能力契约。
///
/// # Parameters
/// 参数
/// `protocol_id` identifies the exact compatible interface and `model` is mutated only when known.
/// `protocol_id` 标识精确兼容接口，`model` 仅在身份已知时原位更新。
#[cfg(test)]
fn enrich_deepseek_model(protocol_id: &str, model: &mut ConnectionModel) {
    model.capabilities.provider_native_tools = match protocol_id {
        ANTHROPIC_MESSAGES_PROFILE_ID | OPENAI_RESPONSES_PROFILE_ID => {
            vec![NativeToolKind::WebSearch]
        }
        _ => Vec::new(),
    };
    let display_name = match model.id.as_str() {
        "deepseek-v4-flash" => "DeepSeek V4 Flash",
        "deepseek-v4-pro" => "DeepSeek V4 Pro",
        _ => return,
    };
    model.display_name = display_name.to_string();
    model.capabilities.context_limit = Some(1_000_000);
    model.capabilities.output_limit = Some(393_216);
    model.capabilities.reasoning = CapabilitySupport::Native;
    model.capabilities.reasoning_control = ReasoningControlMode::Toggle;
    model.capabilities.reasoning_efforts = vec![
        ReasoningEffort::None,
        ReasoningEffort::High,
        ReasoningEffort::Low,
        ReasoningEffort::Max,
    ];
    model
        .capabilities
        .model_parameters
        .extend(deepseek_model_parameters(model.id.as_str(), protocol_id));
    model.capabilities.evidence_revision =
        String::from("deepseek.api-docs.thinking-mode.2026-08-13");
}

/// Builds optional DeepSeek sampling controls for the two documented compatible interfaces.
/// 为 DeepSeek 两种已文档化兼容接口构建可选采样控制。
///
/// # Parameters
/// 参数
/// `model_id` is the exact V4 identity and `protocol_id` is the selected wire protocol.
/// `model_id` 是精确的 V4 身份，`protocol_id` 是选中的线路协议。
///
/// # Returns
/// 返回值
/// A provider-default temperature definition for supported interfaces, otherwise an empty list.
/// 对受支持接口返回使用供应商默认值的温度定义，否则返回空列表。
#[cfg(test)]
fn deepseek_model_parameters(model_id: &str, protocol_id: &str) -> Vec<ModelParameterDefinition> {
    if !matches!(
        protocol_id,
        OPENAI_CHAT_PROFILE_ID | ANTHROPIC_MESSAGES_PROFILE_ID
    ) {
        return Vec::new();
    }
    vec![ModelParameterDefinition {
        id: String::from(SAMPLING_TEMPERATURE_PARAMETER_ID),
        display_name: String::from("Temperature"),
        description: String::from(
            "Controls DeepSeek non-thinking sampling randomness from 0 through 2; provider default omits the field.",
        ),
        schema: ModelParameterSchema::Decimal {
            minimum: Some(String::from("0")),
            maximum: Some(String::from("2")),
            exclusive_minimum: None,
            exclusive_maximum: None,
        },
        default: ModelParameterDefault::ProviderDefault {},
        protocol_ids: vec![protocol_id.to_string()],
        model_ids: vec![model_id.to_string()],
        dependent_required: Vec::new(),
        conflicts: Vec::new(),
        evidence_revision: String::from("deepseek.api-docs.temperature.2026-07-30"),
    }]
}

/// Applies Alibaba reasoning contracts only to exact model identities present in Model Core.
/// 仅对 Model Core 中存在的精确模型身份应用阿里云推理契约。
///
/// # Parameters
/// 参数
/// `model` is one provider-returned model whose unknown identities remain conservative.
/// `model` 是供应商返回的单个模型，未知身份保持保守能力。
#[cfg(test)]
fn enrich_alibaba_model(service_entry_id: &str, model: &mut ConnectionModel) {
    if matches!(
        service_entry_id,
        "model-studio-cn" | "model-studio-singapore"
    ) {
        match model.id.as_str() {
            "text-embedding-v1" | "text-embedding-v2" if service_entry_id == "model-studio-cn" => {
                apply_alibaba_embedding_capabilities(model, &[1536], 1536, 1536, 1536, 25, 2_048);
                return;
            }
            "text-embedding-v4" => {
                apply_alibaba_embedding_capabilities(
                    model,
                    &[64, 128, 256, 512, 768, 1024, 1536, 2048],
                    64,
                    2048,
                    1024,
                    10,
                    8_192,
                );
                return;
            }
            "text-embedding-v3" => {
                apply_alibaba_embedding_capabilities(
                    model,
                    &[64, 128, 256, 512, 768, 1024],
                    64,
                    1024,
                    1024,
                    10,
                    8_192,
                );
                return;
            }
            "qwen3.7-text-embedding" if service_entry_id == "model-studio-cn" => {
                apply_alibaba_embedding_capabilities(
                    model,
                    &[256, 512, 768, 1024, 1536, 2048, 2560],
                    256,
                    2560,
                    1024,
                    20,
                    128_000,
                );
                return;
            }
            "qwen3-rerank" => {
                model.capabilities = ModelCapabilities {
                    operations: vec![AiOperation::RerankDocuments],
                    rerank: Some(RerankCapabilities {
                        query_resources: BTreeSet::new(),
                        candidate_resources: BTreeSet::new(),
                        maximum_candidates: Some(500),
                        maximum_tokens_per_item: Some(4_000),
                        maximum_request_tokens: Some(120_000),
                        truncation: BTreeSet::from([RerankTruncation::ProviderDefault]),
                        supports_return_content: true,
                        score_semantics: RerankScoreSemantics::HigherIsMoreRelevant,
                    }),
                    reasoning: CapabilitySupport::Unsupported,
                    reasoning_control: ReasoningControlMode::FixedOff,
                    context_limit: Some(120_000),
                    evidence_revision: String::from("model-core.alibaba.qwen3-rerank.2026-07-30"),
                    ..ModelCapabilities::default()
                };
                return;
            }
            _ => {}
        }
    }
    let (reasoning_control, reasoning_efforts) = match model.id.as_str() {
        "qwen3.7-plus"
        | "qwen3.6-plus"
        | "qwen3.5-plus"
        | "qwen3.5-flash"
        | "qwen3.7-max"
        | "qwen3.7-max-2026-06-08"
        | "qwen3.6-flash"
        | "glm-5.2"
        | "glm-5.1"
        | "glm-5"
        | "glm-4.7"
        | "glm-5.2-fast-preview"
        | "ZHIPU/GLM-5.2"
        | "xiaomi/mimo-v2.5-pro"
        | "stepfun/step-3.7-flash"
        | "deepseek-v4-pro"
        | "deepseek-v4-pro-0813"
        | "deepseek-v4-flash"
        | "deepseek-v3.2"
        | "kimi-k2.5"
        | "kimi-k2.6" => (
            ReasoningControlMode::Toggle,
            vec![ReasoningEffort::None, ReasoningEffort::Adaptive],
        ),
        "qwen3.8-max"
        | "qwen3.8-flash"
        | "qwen3-max-2026-01-23"
        | "qwen3-coder-next"
        | "qwen3-coder-plus"
        | "MiniMax-M2.5"
        | "MiniMax/MiniMax-M3"
        | "kimi-k2.7-code"
        | "kimi/kimi-k3" => (
            ReasoningControlMode::FixedOn,
            vec![ReasoningEffort::Adaptive],
        ),
        _ => return,
    };
    model.capabilities.reasoning = CapabilitySupport::Native;
    model.capabilities.reasoning_control = reasoning_control;
    model.capabilities.reasoning_efforts = reasoning_efforts;
    model.capabilities.evidence_revision =
        format!("model-core.alibaba.{service_entry_id}.2026-07-30");
}

/// Replaces one Alibaba catalog model with its exact compatible embedding contract.
/// 使用精确兼容向量契约替换一项阿里云目录模型能力。
///
/// # Parameters
/// 参数
/// `model` is the provider-visible model; all numeric arguments are closed reviewed limits.
/// `model` 是供应商可见模型；全部数值参数均为已审核的封闭限制。
///
/// # Returns
/// 返回值
/// This function mutates the model capability snapshot in place and returns no value.
/// 此函数原地修改模型能力快照且不返回值。
#[cfg(test)]
fn apply_alibaba_embedding_capabilities(
    model: &mut ConnectionModel,
    dimensions: &[u32],
    minimum_dimensions: u32,
    maximum_dimensions: u32,
    default_dimensions: u32,
    maximum_inputs: u32,
    maximum_tokens_per_input: u64,
) {
    model.capabilities = ModelCapabilities {
        operations: vec![AiOperation::EmbeddingCreate],
        embedding: Some(EmbeddingCapabilities {
            tasks: BTreeSet::from([EmbeddingTask::ProviderDefault]),
            input_resources: BTreeSet::new(),
            output_kinds: BTreeSet::from([EmbeddingOutputKind::Dense]),
            encodings: BTreeSet::from([EmbeddingEncoding::Float]),
            dimensions: dimensions.iter().copied().collect(),
            minimum_dimensions: Some(minimum_dimensions),
            maximum_dimensions: Some(maximum_dimensions),
            default_dimensions: Some(default_dimensions),
            maximum_inputs: Some(maximum_inputs),
            maximum_tokens_per_input: Some(maximum_tokens_per_input),
            normalization: EmbeddingNormalization::Unknown,
            distance_metric: DistanceMetric::Unknown,
        }),
        reasoning: CapabilitySupport::Unsupported,
        reasoning_control: ReasoningControlMode::FixedOff,
        context_limit: Some(maximum_tokens_per_input),
        evidence_revision: String::from("model-core.alibaba.embeddings.2026-07-30"),
        ..ModelCapabilities::default()
    };
}

/// Parses the OpenAI Codex subscription model catalog without local model guesses.
/// 在不进行本地模型猜测的情况下解析 OpenAI Codex 订阅模型目录。
///
/// # Parameters
/// 参数
/// `connection_id`, `generation`, and `body` identify one exact in-memory refresh.
/// `connection_id`、`generation` 与 `body` 标识一次精确内存刷新。
///
/// # Returns
/// 返回值
/// A conservative catalog using only returned Codex fields.
/// 返回仅使用 Codex 返回字段的保守目录。
pub fn parse_openai_codex_model_catalog(
    connection_id: &str,
    generation: u64,
    body: &str,
) -> Result<ConnectionModelCatalog, String> {
    let response = parse_discovery_json::<OpenAiCodexModelList>(body, "OpenAI Codex model list")?;
    let models = response
        .models
        .into_iter()
        .map(|model| ConnectionModel {
            display_name: model.display_name.unwrap_or_else(|| model.slug.clone()),
            id: model.slug,
            capabilities: ModelCapabilities {
                operations: vec![AiOperation::ConversationRespond],
                context_limit: model.context_window.or(model.max_context_window),
                reasoning: if model.supported_reasoning_levels.is_empty() {
                    CapabilitySupport::Unknown
                } else {
                    CapabilitySupport::Native
                },
                reasoning_efforts: normalized_reasoning_efforts(
                    model
                        .supported_reasoning_levels
                        .iter()
                        .map(|level| level.effort.as_str()),
                ),
                evidence_revision: String::from("openai.codex.models.v1"),
                ..ModelCapabilities::default()
            },
            capability_evidence: ModelCapabilityEvidence::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        })
        .collect();
    Ok(ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
        page_count: 1,
        discovery_schema_revision: String::from("openai.codex.models.v1"),
    })
}

/// Parses one Anthropic model-list page.
/// 解析一页 Anthropic 模型列表。
///
/// # Parameters
/// 参数
/// `body` is the exact documented Anthropic response.
/// `body` 是精确的文档化 Anthropic 响应。
///
/// # Returns
/// 返回值
/// Parsed models plus authoritative pagination fields.
/// 返回解析后的模型及权威分页字段。
pub(crate) fn parse_anthropic_model_page(
    body: &str,
    official_anthropic: bool,
) -> Result<AnthropicModelPage, String> {
    let response: AnthropicModelList = parse_discovery_json(body, "Anthropic model list")?;
    let models = response
        .data
        .into_iter()
        .map(|model| {
            let (reasoning_support, reasoning_efforts, evidence_revision) = if official_anthropic {
                // Manifest model rules carry the reviewed reasoning efforts; unresolved models
                // stay conservative with the discovery revision.
                // 已审核思考档位由清单模型规则承载；未解析模型保持保守并使用发现修订号。
                (
                    CapabilitySupport::Unknown,
                    Vec::new(),
                    String::from("anthropic.models.v1"),
                )
            } else {
                (
                    CapabilitySupport::Unknown,
                    Vec::new(),
                    String::from("anthropic-compatible.models.v1"),
                )
            };
            ConnectionModel {
                id: model.id,
                display_name: model.display_name,
                capabilities: ModelCapabilities {
                    operations: vec![AiOperation::ConversationRespond],
                    reasoning: reasoning_support,
                    reasoning_efforts,
                    prompt_cache: if official_anthropic {
                        anthropic_prompt_cache()
                    } else {
                        Default::default()
                    },
                    evidence_revision,
                    ..ModelCapabilities::default()
                },
                capability_evidence: ModelCapabilityEvidence::default(),
                protocol_variants: BTreeMap::new(),
                resolved_protocol_binding: None,
            }
        })
        .collect();
    Ok(AnthropicModelPage {
        models,
        has_more: response.has_more,
        last_id: response.last_id,
    })
}

/// Builds a complete Anthropic connection catalog from fetched pages.
/// 从已获取分页构建完整 Anthropic 连接目录。
///
/// # Parameters
/// 参数
/// `connection_id`, `generation`, and `models` identify the complete in-memory snapshot;
/// `page_count` and `discovery_schema_revision` preserve discovery evidence.
/// `connection_id`、`generation` 与 `models` 标识完整内存快照；
/// `page_count` 与 `discovery_schema_revision` 保留发现证据。
///
/// # Returns
/// 返回值
/// One process-local model catalog.
/// 返回一个进程内模型目录。
pub(crate) fn build_anthropic_model_catalog(
    connection_id: &str,
    generation: u64,
    models: Vec<ConnectionModel>,
    page_count: u32,
    discovery_schema_revision: &str,
) -> ConnectionModelCatalog {
    ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
        page_count,
        discovery_schema_revision: discovery_schema_revision.to_string(),
    }
}

/// Parses one Google Gemini model-list page using only returned metadata.
/// 仅使用返回元数据解析一页 Google Gemini 模型列表。
///
/// # Parameters
/// 参数
/// `body` is the exact Gemini models response.
/// `body` 是精确 Gemini 模型响应。
///
/// # Returns
/// 返回值
/// Parsed models plus the authoritative next-page token.
/// 返回解析后的模型及权威下一页 Token。
pub(crate) fn parse_gemini_model_page(body: &str) -> Result<GeminiModelPage, String> {
    let response: GeminiModelList = parse_discovery_json(body, "Gemini model list")?;
    let models = response
        .models
        .into_iter()
        .map(|model| {
            let operations = model
                .supported_generation_methods
                .iter()
                .filter_map(|method| match method.as_str() {
                    "generateContent" => Some(AiOperation::ConversationRespond),
                    "embedContent" => Some(AiOperation::EmbeddingCreate),
                    _ => None,
                })
                .collect();
            let model_id = model
                .name
                .strip_prefix("models/")
                .unwrap_or(model.name.as_str())
                .to_string();
            // Manifest model rules carry the reviewed reasoning efforts; unresolved models stay
            // conservative with the discovery revision.
            // 已审核思考档位由清单模型规则承载；未解析模型保持保守并使用发现修订号。
            ConnectionModel {
                id: model_id,
                display_name: model.display_name,
                capabilities: ModelCapabilities {
                    operations,
                    context_limit: model.input_token_limit,
                    output_limit: model.output_token_limit,
                    reasoning: CapabilitySupport::Unknown,
                    reasoning_efforts: Vec::new(),
                    evidence_revision: String::from("gemini.models.v1beta"),
                    ..ModelCapabilities::default()
                },
                capability_evidence: ModelCapabilityEvidence::default(),
                protocol_variants: BTreeMap::new(),
                resolved_protocol_binding: None,
            }
        })
        .collect();
    Ok(GeminiModelPage {
        models,
        next_page_token: response.next_page_token,
    })
}

/// Builds a complete Gemini connection catalog from fetched pages.
/// 从已获取分页构建完整 Gemini 连接目录。
///
/// # Parameters
/// 参数
/// `connection_id`, `generation`, and `models` identify the complete in-memory snapshot;
/// `page_count` records how many upstream pages were consumed.
/// `connection_id`、`generation` 与 `models` 标识完整内存快照；
/// `page_count` 记录实际读取的上游分页数。
///
/// # Returns
/// 返回值
/// One process-local model catalog.
/// 返回一个进程内模型目录。
pub(crate) fn build_gemini_model_catalog(
    connection_id: &str,
    generation: u64,
    models: Vec<ConnectionModel>,
    page_count: u32,
) -> ConnectionModelCatalog {
    ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
        page_count,
        discovery_schema_revision: String::from("google.gemini.models.v1beta"),
    }
}

/// Parses the credential-scoped Google Code Assist model map.
/// 解析凭据作用域的 Google Code Assist 模型映射。
///
/// # Parameters
/// 参数
/// `connection_id` identifies the exact credential, `generation` is process-local, and `body`
/// is the reviewed `fetchAvailableModels` response.
/// `connection_id` 标识精确凭据，`generation` 仅存在于进程内，`body`
/// 是已审核的 `fetchAvailableModels` 响应。
///
/// # Returns
/// 返回值
/// A memory-only catalog preserving every model returned by the account.
/// 返回保留该账号所返回每个模型的纯内存目录。
pub fn parse_google_code_assist_model_catalog(
    connection_id: &str,
    generation: u64,
    body: &str,
) -> Result<ConnectionModelCatalog, String> {
    let response =
        parse_discovery_json::<GoogleCodeAssistModelList>(body, "Google Code Assist model list")?;
    let models = response
        .models
        .into_iter()
        .map(|(id, model)| {
            // Manifest model rules carry the reviewed reasoning efforts; unresolved models stay
            // conservative with the discovery revision.
            // 已审核思考档位由清单模型规则承载；未解析模型保持保守并使用发现修订号。
            ConnectionModel {
                display_name: model.display_name.unwrap_or_else(|| id.clone()),
                id,
                capabilities: ModelCapabilities {
                    operations: vec![AiOperation::ConversationRespond],
                    context_limit: model.max_tokens,
                    output_limit: model.max_output_tokens,
                    reasoning: CapabilitySupport::Unknown,
                    reasoning_efforts: Vec::new(),
                    evidence_revision: String::from(
                        "google-code-assist.fetch-available-models.v1internal",
                    ),
                    ..ModelCapabilities::default()
                },
                capability_evidence: ModelCapabilityEvidence::default(),
                protocol_variants: BTreeMap::new(),
                resolved_protocol_binding: None,
            }
        })
        .collect();
    Ok(ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
        page_count: 1,
        discovery_schema_revision: String::from(
            "google-code-assist.fetch-available-models.v1internal",
        ),
    })
}

/// Builds verified MiniMax capabilities for one exact protocol route.
/// 为一个精确协议路由构建已验证的 MiniMax 能力。
///
/// # Parameters
/// 参数
/// `model_id` is the exact identity returned by MiniMax model discovery and
/// `protocol_id` is the credential-selected compatible protocol.
/// `model_id` 是 MiniMax 模型发现返回的精确身份，
/// `protocol_id` 是凭据选择的兼容协议。
///
/// # Returns
/// 返回值
/// A model-specific capability set; unknown identities receive a conservative contract.
/// 返回模型专属能力集合；未知身份获得保守契约。
/// Builds the protocol-wide cache capability documented for every active Claude model.
/// 构建所有活跃 Claude 模型均已文档化支持的协议级缓存能力。
fn anthropic_prompt_cache() -> PromptCacheCapability {
    PromptCacheCapability {
        provider_default_behavior: String::from(
            "Anthropic provider defaults are used without host-injected cache breakpoints.",
        ),
        supports_managed_explicit: true,
        supported_ttl_options: vec![
            vulcan_ai_contract::PromptCacheTtlOption {
                id: String::from("5m"),
                display_name: String::from("5 minutes"),
                description: Some(String::from("Anthropic default ephemeral cache lifetime.")),
            },
            vulcan_ai_contract::PromptCacheTtlOption {
                id: String::from("1h"),
                display_name: String::from("1 hour"),
                description: Some(String::from("Anthropic extended ephemeral cache lifetime.")),
            },
        ],
        profile_default_ttl_option_id: Some(String::from("5m")),
        minimum_cacheable_tokens: None,
        maximum_breakpoints: Some(4),
    }
}

/// Builds a provider-default-only cache capability.
/// 构建仅支持供应商默认行为的缓存能力。
/// Builds the minimum safe capability set for an unverified OpenRouter model.
/// 为未验证的 OpenRouter 模型构建最小安全能力集合。
fn openrouter_reported_capabilities(model: &OpenRouterModelEntry) -> ModelCapabilities {
    // Resolve model-wide and primary-provider limits before projecting catalog capabilities.
    // 在投影目录能力前解析模型级与首选供应商级限制。
    let context_limit = openrouter_effective_context_limit(model);
    let supports = |parameter: &str| {
        model
            .supported_parameters
            .iter()
            .any(|candidate| candidate == parameter)
    };
    let input_resources = model
        .architecture
        .as_ref()
        .and_then(|architecture| architecture.input_modalities.as_deref())
        .map(resource_kinds)
        .unwrap_or_default();
    let output_resources = model
        .architecture
        .as_ref()
        .and_then(|architecture| architecture.output_modalities.as_deref())
        .map(resource_kinds)
        .unwrap_or_default();
    // supports_text_conversation requires both sides of the exact OpenRouter architecture contract.
    // supports_text_conversation 要求 OpenRouter 精确架构契约的输入与输出两侧都声明文本。
    let supports_text_conversation = model.architecture.as_ref().is_some_and(|architecture| {
        architecture
            .input_modalities
            .as_ref()
            .is_some_and(|modalities| modalities.iter().any(|modality| modality == "text"))
            && architecture
                .output_modalities
                .as_ref()
                .is_some_and(|modalities| modalities.iter().any(|modality| modality == "text"))
    });
    let mut model_parameters = Vec::new();
    let reasoning_efforts = model
        .reasoning
        .as_ref()
        .map(|reasoning| {
            let mut efforts = match &reasoning.supported_efforts {
                OpenRouterSupportedEfforts::Omitted => {
                    vec![ReasoningEffort::None, ReasoningEffort::Adaptive]
                }
                OpenRouterSupportedEfforts::AllGateway => {
                    vec![
                        ReasoningEffort::None,
                        ReasoningEffort::Minimal,
                        ReasoningEffort::Low,
                        ReasoningEffort::Medium,
                        ReasoningEffort::High,
                        ReasoningEffort::XHigh,
                        ReasoningEffort::Max,
                    ]
                }
                OpenRouterSupportedEfforts::Listed(efforts) => {
                    normalized_reasoning_efforts(efforts.iter().map(String::as_str))
                }
            };
            if reasoning.mandatory {
                efforts.retain(|effort| *effort != ReasoningEffort::None);
            }
            efforts
        })
        .unwrap_or_default();
    if supports("temperature") {
        model_parameters.push(ModelParameterDefinition {
            id: String::from(SAMPLING_TEMPERATURE_PARAMETER_ID),
            display_name: String::from("Temperature"),
            description: String::from(
                "Controls sampling randomness when the selected OpenRouter model and endpoint expose the parameter.",
            ),
            schema: ModelParameterSchema::Decimal {
                minimum: None,
                maximum: None,
                exclusive_minimum: None,
                exclusive_maximum: None,
            },
            default: ModelParameterDefault::ProviderDefault {},
            protocol_ids: vec![
                String::from(ANTHROPIC_MESSAGES_PROFILE_ID),
                String::from(OPENAI_CHAT_PROFILE_ID),
                String::from(OPENAI_RESPONSES_PROFILE_ID),
            ],
            model_ids: vec![model.id.clone()],
            dependent_required: Vec::new(),
            conflicts: Vec::new(),
            evidence_revision: String::from("openrouter-models-api-supported-parameters"),
        });
    }
    if model
        .reasoning
        .as_ref()
        .is_some_and(|reasoning| reasoning.supports_max_tokens)
    {
        model_parameters.push(ModelParameterDefinition {
            id: String::from(REASONING_MAX_TOKENS_PARAMETER_ID),
            display_name: String::from("Reasoning token budget"),
            description: String::from(
                "Sets OpenRouter reasoning.max_tokens when the selected model reports direct reasoning-budget support.",
            ),
            schema: ModelParameterSchema::Integer {
                minimum: Some(1),
                maximum: None,
                step: Some(1),
            },
            default: ModelParameterDefault::ProviderDefault {},
            protocol_ids: vec![
                String::from(OPENAI_CHAT_PROFILE_ID),
                String::from(OPENAI_RESPONSES_PROFILE_ID),
                String::from(ANTHROPIC_MESSAGES_PROFILE_ID),
            ],
            model_ids: vec![model.id.clone()],
            dependent_required: Vec::new(),
            conflicts: Vec::new(),
            evidence_revision: String::from("openrouter-reasoning-metadata-2026-07"),
        });
    }
    // An omitted OpenRouter reasoning object is absence of evidence, not an explicit disablement.
    // OpenRouter 未返回 reasoning 对象表示缺少证据，而不是明确关闭思考能力。
    let (reasoning, reasoning_control) = if model.reasoning.is_some() {
        (
            CapabilitySupport::Native,
            reasoning_control_mode(CapabilitySupport::Native, reasoning_efforts.as_slice()),
        )
    } else if supports("reasoning") {
        (CapabilitySupport::Native, ReasoningControlMode::FixedOn)
    } else {
        (CapabilitySupport::Unknown, ReasoningControlMode::Unknown)
    };
    ModelCapabilities {
        operations: supports_text_conversation
            .then_some(AiOperation::ConversationRespond)
            .into_iter()
            .collect(),
        input_resources,
        output_resources,
        structured_output: StructuredOutputCapability::JsonObject,
        embedding: None,
        rerank: None,
        reasoning,
        reasoning_control,
        reasoning_efforts,
        provider_native_tools: Vec::new(),
        context_limit,
        output_limit: openrouter_effective_output_limit(model, context_limit),
        prompt_cache: PromptCacheCapability::default(),
        model_parameters,
        evidence_revision: String::from("openrouter-models-api-2026-07"),
    }
}

/// Records exactly which OpenRouter capability fields were present in one model record.
/// 精确记录一条 OpenRouter 模型记录中实际出现的能力字段。
///
/// # Parameters
/// 参数
/// `model` is the exact entry returned by the documented OpenRouter Models API.
/// `model` 是 OpenRouter 文档化 Models API 返回的精确条目。
///
/// # Returns
/// 返回值
/// Presence-aware evidence used to bound signed manifest defaults without granting unrelated
/// capabilities such as tool calling.
/// 返回带字段存在性语义的证据，用于约束签名清单默认值且不会授予工具调用等无关能力。
fn openrouter_capability_evidence(model: &OpenRouterModelEntry) -> ModelCapabilityEvidence {
    let architecture = model.architecture.as_ref();
    ModelCapabilityEvidence {
        context_limit_declared: openrouter_declared_token_limit(model.context_length).is_some()
            || model.top_provider.as_ref().is_some_and(|provider| {
                openrouter_declared_token_limit(provider.context_length).is_some()
            }),
        output_limit_declared: model.top_provider.as_ref().is_some_and(|provider| {
            openrouter_declared_token_limit(provider.max_completion_tokens).is_some()
        }),
        input_modalities_declared: architecture
            .is_some_and(|architecture| architecture.input_modalities.is_some()),
        output_modalities_declared: architecture
            .is_some_and(|architecture| architecture.output_modalities.is_some()),
        structured_output_declared: false,
        supports_text_input: architecture
            .and_then(|architecture| architecture.input_modalities.as_ref())
            .map(|modalities| modalities.iter().any(|modality| modality == "text")),
        supports_text_output: architecture
            .and_then(|architecture| architecture.output_modalities.as_ref())
            .map(|modalities| modalities.iter().any(|modality| modality == "text")),
    }
}

/// Resolves the safe context boundary shared by OpenRouter model and top-provider metadata.
/// 解析 OpenRouter 模型级与首选端点元数据共同允许的安全上下文边界。
///
/// `model` is the exact model entry returned by OpenRouter's Models API.
/// `model` 是 OpenRouter Models API 返回的精确模型条目。
/// Returns the strictest declared context limit, or the only declared limit when one side is absent.
/// 返回两侧已声明限制中的较严格值；仅一侧声明时返回该值。
fn openrouter_effective_context_limit(model: &OpenRouterModelEntry) -> Option<u64> {
    // Read the provider-specific context paired with max_completion_tokens in the same object.
    // 读取与 max_completion_tokens 位于同一对象中的端点专属上下文。
    let provider_context_limit = model
        .top_provider
        .as_ref()
        .and_then(|provider| openrouter_declared_token_limit(provider.context_length));
    match (
        openrouter_declared_token_limit(model.context_length),
        provider_context_limit,
    ) {
        (Some(model_limit), Some(provider_limit)) => Some(model_limit.min(provider_limit)),
        (Some(model_limit), None) => Some(model_limit),
        (None, Some(provider_limit)) => Some(provider_limit),
        (None, None) => None,
    }
}

/// Treats a zero reported token limit as an absent declaration.
/// 将上游报告的零 Token 上限视为未声明。
/// `value` is one optional upstream limit; returns only a positive declared limit.
/// `value` 是上游可选上限；只返回明确声明的正数上限。
fn openrouter_declared_token_limit(value: Option<u64>) -> Option<u64> {
    value.filter(|limit| *limit > 0)
}

/// Bounds OpenRouter's top-provider completion limit by the resolved context boundary.
/// 使用已解析的上下文边界约束 OpenRouter 首选端点输出上限。
///
/// `model` is the exact model entry returned by OpenRouter's Models API.
/// `model` 是 OpenRouter Models API 返回的精确模型条目。
/// `context_limit` is the safe model/provider context intersection.
/// `context_limit` 是模型与端点上下文的安全交集。
/// Returns a catalog-safe completion limit while preserving an omitted upstream value as unknown.
/// 返回可安全写入目录的输出上限，并将上游未声明值继续保留为未知。
fn openrouter_effective_output_limit(
    model: &OpenRouterModelEntry,
    context_limit: Option<u64>,
) -> Option<u64> {
    // Preserve omission as unknown instead of manufacturing a provider capability.
    // 将未声明继续保留为未知，不制造供应商能力事实。
    let output_limit = model
        .top_provider
        .as_ref()
        .and_then(|provider| openrouter_declared_token_limit(provider.max_completion_tokens))?;
    // Dedicated media discovery must preserve reported limits without applying a chat window.
    // 专用媒体模型的发现结果保留原始限制，不套用文本会话窗口进行收敛。
    let evidence = openrouter_capability_evidence(model);
    if evidence.supports_text_input == Some(false) || evidence.supports_text_output == Some(false) {
        return Some(output_limit);
    }
    Some(context_limit.map_or(output_limit, |limit| output_limit.min(limit)))
}

/// Resolves an exact UI control mode from verified reasoning support and effort values.
/// 从已验证推理支持与档位值解析精确 UI 控制模式。
///
/// # Parameters
/// 参数
/// `support` is the route's reasoning fidelity and `efforts` is its closed selectable list.
/// `support` 是路由推理保真度，`efforts` 是其封闭可选列表。
///
/// # Returns
/// 返回值
/// A deterministic control mode without inventing provider capabilities.
/// 返回不会虚构供应商能力的确定性控制模式。
fn reasoning_control_mode(
    support: CapabilitySupport,
    efforts: &[ReasoningEffort],
) -> ReasoningControlMode {
    if support == CapabilitySupport::Unsupported {
        return ReasoningControlMode::FixedOff;
    }
    if !support.is_supported() {
        return ReasoningControlMode::Unknown;
    }
    if efforts.is_empty() {
        return ReasoningControlMode::FixedOn;
    }
    if efforts == [ReasoningEffort::None] {
        return ReasoningControlMode::FixedOff;
    }
    if efforts == [ReasoningEffort::Adaptive] {
        return ReasoningControlMode::ProviderAdaptive;
    }
    if efforts == [ReasoningEffort::None, ReasoningEffort::Adaptive] {
        return ReasoningControlMode::Toggle;
    }
    ReasoningControlMode::EffortSelection
}

/// Normalizes provider-declared reasoning labels into the workbench-supported public levels.
/// 把供应商声明的推理标签归一化为工作台支持的公共档位。
///
/// # Parameters
/// 参数
/// `levels` contains exact provider labels returned by model discovery.
/// `levels` 包含模型发现返回的精确供应商标签。
///
/// # Returns
/// 返回值
/// Ordered, deduplicated public levels; unsupported labels remain unavailable.
/// 有序且去重的公共档位；不支持的标签保持不可用。
fn normalized_reasoning_efforts<'a>(
    levels: impl IntoIterator<Item = &'a str>,
) -> Vec<ReasoningEffort> {
    levels
        .into_iter()
        .filter_map(|level| match level {
            "none" => Some(ReasoningEffort::None),
            "minimal" => Some(ReasoningEffort::Minimal),
            "low" => Some(ReasoningEffort::Low),
            "medium" => Some(ReasoningEffort::Medium),
            "high" => Some(ReasoningEffort::High),
            "xhigh" => Some(ReasoningEffort::XHigh),
            "max" => Some(ReasoningEffort::Max),
            "ultra" => Some(ReasoningEffort::Ultra),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Maps only documented OpenRouter modality identifiers into public resource kinds.
/// 仅把文档化 OpenRouter 模态标识映射为公共资源类别。
fn resource_kinds(modalities: &[String]) -> Vec<ResourceKind> {
    modalities
        .iter()
        .filter_map(|modality| match modality.as_str() {
            "image" => Some(ResourceKind::Image),
            "audio" => Some(ResourceKind::Audio),
            "video" => Some(ResourceKind::Video),
            "file" => Some(ResourceKind::File),
            "text" => None,
            _ => None,
        })
        .collect()
}

/// MiniMax documented model-list response.
/// MiniMax 文档化模型列表响应。
#[cfg(test)]
#[derive(Debug, Deserialize)]
struct MiniMaxModelList {
    /// Exact model entries.
    /// 精确模型条目。
    data: Vec<MiniMaxModelEntry>,
    /// Whether another page exists.
    /// 是否存在下一页。
    has_more: bool,
    /// Cursor identifying the last entry in this page.
    /// 标识本页最后一个条目的游标。
    last_id: Option<String>,
}

/// Parsed MiniMax page shared with the HTTP pagination client.
/// 与 HTTP 分页客户端共享的已解析 MiniMax 页面。
#[cfg(test)]
pub(crate) struct MiniMaxModelPage {
    /// Exact model projections in provider order.
    /// 按供应商顺序排列的精确模型投影。
    pub(crate) models: Vec<ConnectionModel>,
    /// Authoritative continuation flag.
    /// 权威续页标记。
    pub(crate) has_more: bool,
    /// Authoritative next-page cursor source.
    /// 权威下一页游标来源。
    pub(crate) last_id: Option<String>,
}

/// MiniMax documented model entry.
/// MiniMax 文档化模型条目。
#[cfg(test)]
#[derive(Debug, Deserialize)]
struct MiniMaxModelEntry {
    /// Exact model identifier.
    /// 精确模型标识。
    id: String,
    /// Provider display name.
    /// 供应商显示名。
    display_name: String,
}

/// OpenRouter documented model-list response.
/// OpenRouter 文档化模型列表响应。
#[derive(Debug, Deserialize)]
struct OpenRouterModelList {
    /// Exact model entries.
    /// 精确模型条目。
    data: Vec<OpenRouterModelEntry>,
}

/// Documented OpenAI model-list response.
/// 文档化 OpenAI 模型列表响应。
#[derive(Debug, Deserialize)]
struct OpenAiModelList {
    /// Returned model records.
    /// 返回的模型记录。
    data: Vec<OpenAiModelEntry>,
    /// Documented object discriminator.
    /// 文档化对象判别值。
    #[serde(rename = "object")]
    #[serde(default)]
    _object: Option<String>,
    /// Remaining compatible-provider pagination metadata is intentionally ignored.
    /// 其余兼容供应商分页元数据会被有意忽略。
    #[serde(flatten)]
    _remaining: BTreeMap<String, serde_json::Value>,
}

/// Documented OpenAI model record fields used by Vulcan.
/// Vulcan 使用的文档化 OpenAI 模型记录字段。
#[derive(Debug, Deserialize)]
struct OpenAiModelEntry {
    /// Exact model identifier.
    /// 精确模型标识。
    id: String,
    /// Optional provider display name.
    /// 可选供应商显示名称。
    #[serde(default, alias = "name")]
    display_name: Option<String>,
    /// Optional provider-declared context window.
    /// 可选供应商声明的上下文窗口。
    #[serde(default)]
    context_window: Option<u64>,
    /// Optional provider-declared output-token ceiling.
    /// 可选供应商声明的输出 Token 上限。
    #[serde(default)]
    max_output_tokens: Option<u64>,
    /// Optional provider-declared input modalities.
    /// 可选供应商声明的输入模态。
    #[serde(default)]
    input_modalities: Option<Vec<String>>,
    /// Optional provider-declared output modalities.
    /// 可选供应商声明的输出模态。
    #[serde(default)]
    output_modalities: Option<Vec<String>>,
    /// Remaining compatible-provider fields are not interpreted as capabilities.
    /// 其余兼容供应商字段不解释为能力。
    #[serde(flatten)]
    _remaining: BTreeMap<String, serde_json::Value>,
}

/// Projects compatible-provider modality strings into the closed resource vocabulary.
/// 将兼容供应商模态字符串投影到封闭资源词表。
fn compatible_resource_kinds(modalities: &[String]) -> Vec<ResourceKind> {
    modalities
        .iter()
        .filter_map(
            |modality| match modality.trim().to_ascii_lowercase().as_str() {
                "image" => Some(ResourceKind::Image),
                "audio" => Some(ResourceKind::Audio),
                "video" => Some(ResourceKind::Video),
                "file" => Some(ResourceKind::File),
                _ => None,
            },
        )
        .collect()
}

/// Reports whether one compatible-provider modality list explicitly includes text.
/// 报告兼容供应商模态列表是否明确包含文本。
fn compatible_supports_text(modalities: &[String]) -> bool {
    modalities
        .iter()
        .any(|modality| modality.trim().eq_ignore_ascii_case("text"))
}

/// Exact Codex subscription model-list container.
/// 精确 Codex 订阅模型列表容器。
#[derive(Deserialize)]
struct OpenAiCodexModelList {
    /// Models visible to the authenticated account.
    /// 已认证账号可见的模型。
    models: Vec<OpenAiCodexModelEntry>,
}

/// Exact Codex model fields consumed by Vulcan model selection.
/// Vulcan 模型选择消费的精确 Codex 模型字段。
#[derive(Deserialize)]
struct OpenAiCodexModelEntry {
    /// Stable upstream model slug.
    /// 稳定上游模型 Slug。
    slug: String,
    /// Optional upstream display name.
    /// 可选上游显示名。
    #[serde(default)]
    display_name: Option<String>,
    /// Optional current context-window declaration.
    /// 可选当前上下文窗口声明。
    #[serde(default)]
    context_window: Option<u64>,
    /// Optional alternate context-window declaration.
    /// 可选备用上下文窗口声明。
    #[serde(default)]
    max_context_window: Option<u64>,
    /// Exact reasoning levels returned by the Codex account catalog.
    /// Codex 账号目录返回的精确推理档位。
    #[serde(default)]
    supported_reasoning_levels: Vec<OpenAiCodexReasoningLevel>,
    /// Remaining provider fields are not interpreted as public capabilities.
    /// 其余供应商字段不解释为公共能力。
    #[serde(flatten)]
    _remaining: BTreeMap<String, serde_json::Value>,
}

/// One exact Codex reasoning-level declaration.
/// 一项精确的 Codex 推理档位声明。
#[derive(Deserialize)]
struct OpenAiCodexReasoningLevel {
    /// Stable upstream effort label.
    /// 稳定的上游推理强度标签。
    effort: String,
}

/// Parsed Anthropic page shared with the HTTP pagination client.
/// 与 HTTP 分页客户端共享的 Anthropic 已解析分页。
pub(crate) struct AnthropicModelPage {
    /// Models from this page.
    /// 本页模型。
    pub(crate) models: Vec<ConnectionModel>,
    /// Authoritative continuation flag.
    /// 权威继续标记。
    pub(crate) has_more: bool,
    /// Authoritative last-item cursor.
    /// 权威最后一项游标。
    pub(crate) last_id: Option<String>,
}

/// Documented Anthropic model-list response.
/// 文档化 Anthropic 模型列表响应。
#[derive(Debug, Deserialize)]
struct AnthropicModelList {
    /// Returned models.
    /// 返回的模型。
    data: Vec<AnthropicModelEntry>,
    /// Authoritative continuation flag.
    /// 权威继续标记。
    has_more: bool,
    /// Last-item cursor.
    /// 最后一项游标。
    last_id: Option<String>,
}

/// Documented Anthropic model record.
/// 文档化 Anthropic 模型记录。
#[derive(Debug, Deserialize)]
struct AnthropicModelEntry {
    /// Exact model identifier.
    /// 精确模型标识。
    id: String,
    /// User-visible model name.
    /// 用户可见模型名称。
    display_name: String,
}

/// Parsed Gemini page shared with the HTTP pagination client.
/// 与 HTTP 分页客户端共享的 Gemini 已解析分页。
pub(crate) struct GeminiModelPage {
    /// Models from this page.
    /// 本页模型。
    pub(crate) models: Vec<ConnectionModel>,
    /// Opaque continuation token.
    /// 不透明继续 Token。
    pub(crate) next_page_token: Option<String>,
}

/// Documented Gemini model-list response.
/// 文档化 Gemini 模型列表响应。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiModelList {
    /// Returned Gemini models.
    /// 返回的 Gemini 模型。
    models: Vec<GeminiModelEntry>,
    /// Opaque continuation token.
    /// 不透明继续 Token。
    #[serde(default)]
    next_page_token: Option<String>,
}

/// Documented Gemini model metadata used by Vulcan.
/// Vulcan 使用的文档化 Gemini 模型元数据。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiModelEntry {
    /// Resource name such as `models/gemini-*`.
    /// 资源名称，例如 `models/gemini-*`。
    name: String,
    /// User-visible model name.
    /// 用户可见模型名称。
    display_name: String,
    /// Optional input token limit.
    /// 可选输入 Token 限制。
    #[serde(default)]
    input_token_limit: Option<u64>,
    /// Optional output token limit.
    /// 可选输出 Token 限制。
    #[serde(default)]
    output_token_limit: Option<u64>,
    /// Provider-reported supported generation methods.
    /// 供应商报告的支持生成方法。
    #[serde(default)]
    supported_generation_methods: Vec<String>,
}

/// Exact Google Code Assist model-map response.
/// 精确 Google Code Assist 模型映射响应。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GoogleCodeAssistModelList {
    /// Models keyed by provider identity.
    /// 按供应商身份索引的模型。
    models: BTreeMap<String, GoogleCodeAssistModelEntry>,
}

/// Exact Code Assist fields currently used by model selection and local narrowing.
/// 当前供模型选择与本地收窄使用的精确 Code Assist 字段。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleCodeAssistModelEntry {
    /// Optional provider display name.
    /// 可选供应商显示名。
    #[serde(default)]
    display_name: Option<String>,
    /// Optional provider-declared context limit.
    /// 可选供应商声明上下文限制。
    #[serde(default)]
    max_tokens: Option<u64>,
    /// Optional provider-declared output limit.
    /// 可选供应商声明输出限制。
    #[serde(default)]
    max_output_tokens: Option<u64>,
    /// Remaining provider fields are deliberately ignored because they are not public capabilities.
    /// 其余供应商字段被刻意忽略，因为它们不是公共能力。
    #[serde(flatten)]
    _remaining: BTreeMap<String, serde_json::Value>,
}

/// OpenRouter documented model entry.
/// OpenRouter 文档化模型条目。
#[derive(Debug, Deserialize)]
struct OpenRouterModelEntry {
    /// Exact model identifier.
    /// 精确模型标识。
    id: String,
    /// Optional display name.
    /// 可选显示名。
    name: Option<String>,
    /// Provider-reported context length.
    /// 供应商报告的上下文长度。
    context_length: Option<u64>,
    /// Provider-reported supported request parameter identifiers.
    /// 供应商报告的受支持请求参数标识。
    #[serde(default)]
    supported_parameters: Vec<String>,
    /// Provider-reported architecture and modalities.
    /// 供应商报告的架构与模态。
    architecture: Option<OpenRouterArchitecture>,
    /// Provider-reported top endpoint limits.
    /// 供应商报告的首选端点限制。
    top_provider: Option<OpenRouterTopProvider>,
    /// Provider-reported reasoning controls, absent for models without selectable reasoning.
    /// 供应商报告的推理控制；不可选择推理的模型不提供该字段。
    #[serde(default)]
    reasoning: Option<OpenRouterReasoning>,
}

/// OpenRouter model-scoped reasoning metadata.
/// OpenRouter 模型作用域推理元数据。
#[derive(Debug, Deserialize)]
struct OpenRouterReasoning {
    /// Supported effort declaration preserving OpenRouter's omitted-versus-null distinction.
    /// 保留 OpenRouter 字段缺失与 null 差异的推理档位声明。
    #[serde(default)]
    supported_efforts: OpenRouterSupportedEfforts,
    /// Whether the model accepts a direct reasoning token budget.
    /// 模型是否接受直接推理 Token 预算。
    #[serde(default)]
    supports_max_tokens: bool,
    /// Whether reasoning is mandatory for the model.
    /// 模型是否强制启用推理。
    #[serde(default)]
    mandatory: bool,
}

/// Preserves OpenRouter's three documented `supported_efforts` states.
/// 保留 OpenRouter 文档化的三种 `supported_efforts` 状态。
#[derive(Debug, Default)]
enum OpenRouterSupportedEfforts {
    /// The field is omitted, so the model supports reasoning without selectable strength.
    /// 字段缺失，表示模型支持推理但不提供强度选择。
    #[default]
    Omitted,
    /// The field is null, so all gateway effort values are accepted.
    /// 字段为 null，表示接受全部网关档位。
    AllGateway,
    /// The field contains the exact selectable gateway values.
    /// 字段包含精确可选的网关档位。
    Listed(Vec<String>),
}

/// Deserializes a present OpenRouter effort declaration while serde default handles omission.
/// 反序列化已出现的 OpenRouter 档位声明，字段缺失由 serde 默认值处理。
impl<'de> Deserialize<'de> for OpenRouterSupportedEfforts {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let efforts = Option::<Vec<String>>::deserialize(deserializer)?;
        Ok(match efforts {
            Some(efforts) => Self::Listed(efforts),
            None => Self::AllGateway,
        })
    }
}

/// OpenRouter documented model architecture subset.
/// OpenRouter 文档化模型架构子集。
#[derive(Debug, Deserialize)]
struct OpenRouterArchitecture {
    /// Reported input modality identifiers.
    /// 报告的输入模态标识。
    #[serde(default)]
    input_modalities: Option<Vec<String>>,
    /// Reported output modality identifiers.
    /// 报告的输出模态标识。
    #[serde(default)]
    output_modalities: Option<Vec<String>>,
}

/// OpenRouter documented top-provider subset.
/// OpenRouter 文档化首选端点子集。
#[derive(Debug, Deserialize)]
struct OpenRouterTopProvider {
    /// Provider-specific context limit paired with max_completion_tokens.
    /// 与 max_completion_tokens 配对的端点专属上下文限制。
    #[serde(default)]
    context_length: Option<u64>,
    /// Maximum completion tokens reported for the top endpoint.
    /// 首选端点报告的最大补全 Token。
    #[serde(default)]
    max_completion_tokens: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Verifies MiniMax discovery rejects missing documented identity fields.
    /// 验证 MiniMax 发现拒绝缺失文档化身份字段。
    #[test]
    fn minimax_discovery_is_strict() {
        let error = parse_minimax_model_catalog(
            "connection",
            1,
            r#"{"data":[{"id":"m"}],"has_more":false,"last_id":"m"}"#,
        )
        .expect_err("missing display_name must fail");
        assert!(error.contains("display_name"));
    }

    /// Verifies the removed MiniMax parsers and provider-neutral parsers preserve route identities.
    /// 验证已移除的 MiniMax 解析器与供应商无关解析器保留相同路由身份。
    #[test]
    fn minimax_legacy_discovery_oracles_match_generic_parsers() {
        // anthropic_body is one representative documented compatible page.
        // anthropic_body 是一份代表性的文档化兼容分页。
        let anthropic_body = r#"{"data":[{"id":"MiniMax-M3","display_name":"MiniMax M3"},{"id":"MiniMax-M2.7","display_name":"MiniMax M2.7"}],"has_more":true,"last_id":"MiniMax-M2.7"}"#;
        // legacy_anthropic is the code-owned behavior being retired from production.
        // legacy_anthropic 是正从生产路径退役的代码持有行为。
        let legacy_anthropic =
            parse_minimax_model_page(anthropic_body).expect("legacy MiniMax page must parse");
        // generic_anthropic is the provider-neutral replacement selected by the manifest.
        // generic_anthropic 是由 Manifest 选择的供应商无关替代实现。
        let generic_anthropic = parse_anthropic_model_page(anthropic_body, false)
            .expect("generic Anthropic-compatible page must parse");
        assert_eq!(
            legacy_anthropic
                .models
                .iter()
                .map(|model| (&model.id, &model.display_name))
                .collect::<Vec<_>>(),
            generic_anthropic
                .models
                .iter()
                .map(|model| (&model.id, &model.display_name))
                .collect::<Vec<_>>()
        );
        assert_eq!(legacy_anthropic.has_more, generic_anthropic.has_more);
        assert_eq!(legacy_anthropic.last_id, generic_anthropic.last_id);

        // openai_body is one representative standard OpenAI model-list envelope.
        // openai_body 是一份代表性的标准 OpenAI 模型列表封装。
        let openai_body = r#"{"object":"list","data":[{"id":"MiniMax-M3","object":"model","created":1,"owned_by":"minimax"},{"id":"MiniMax-M2.7","object":"model","created":1,"owned_by":"minimax"}]}"#;
        // legacy_openai is the former named parser retained only as a migration oracle.
        // legacy_openai 是仅作为迁移基准保留的旧具名解析器。
        let legacy_openai = parse_minimax_openai_model_catalog("connection", 7, openai_body)
            .expect("legacy MiniMax OpenAI catalog must parse");
        // generic_openai is the production provider-neutral replacement.
        // generic_openai 是生产路径中的供应商无关替代实现。
        let generic_openai = parse_openai_model_catalog("connection", 7, openai_body)
            .expect("generic OpenAI-compatible catalog must parse");
        assert_eq!(
            legacy_openai
                .models
                .iter()
                .map(|model| (&model.id, &model.display_name))
                .collect::<Vec<_>>(),
            generic_openai
                .models
                .iter()
                .map(|model| (&model.id, &model.display_name))
                .collect::<Vec<_>>()
        );
        assert_eq!(legacy_openai.connection_id, generic_openai.connection_id);
        assert_eq!(legacy_openai.generation, generic_openai.generation);
        assert_eq!(legacy_openai.page_count, generic_openai.page_count);
    }

    /// Verifies Kimi Coding Plan publishes the documented modality and reasoning contracts.
    /// 验证 Kimi Coding Plan 发布文档化的多模态与推理契约。
    #[test]
    fn kimi_coding_plan_enrichment_publishes_model_capabilities() {
        let mut catalog = parse_openai_model_catalog(
            "connection",
            1,
            r#"{"data":[
                {"id":"k3"},
                {"id":"k3-256k"},
                {"id":"kimi-for-coding"},
                {"id":"kimi-for-coding-highspeed"}
            ]}"#,
        )
        .expect("Kimi model list should parse");
        enrich_compatible_model_catalog_for_migration_oracle(
            "kimi",
            "coding-plan",
            OPENAI_CHAT_PROFILE_ID,
            &mut catalog,
        );

        let k3 = catalog
            .models
            .iter()
            .find(|model| model.id == "k3")
            .expect("K3");
        assert_eq!(k3.capabilities.context_limit, Some(1_048_576));
        assert_eq!(
            k3.capabilities.input_resources,
            vec![ResourceKind::Image, ResourceKind::Video]
        );
        assert_eq!(
            k3.capabilities.reasoning_control,
            ReasoningControlMode::EffortSelection
        );

        let k3_256k = catalog
            .models
            .iter()
            .find(|model| model.id == "k3-256k")
            .expect("K3 256K");
        assert_eq!(k3_256k.capabilities.context_limit, Some(262_144));
        assert_eq!(
            k3_256k.capabilities.input_resources,
            vec![ResourceKind::Image]
        );

        for model_id in ["kimi-for-coding", "kimi-for-coding-highspeed"] {
            let model = catalog
                .models
                .iter()
                .find(|model| model.id == model_id)
                .expect("K2.7 Code model");
            assert_eq!(
                model.capabilities.input_resources,
                vec![ResourceKind::Image, ResourceKind::Video]
            );
            assert_eq!(
                model.capabilities.reasoning_control,
                ReasoningControlMode::FixedOn
            );
        }
    }

    /// Verifies Kimi's standard and dedicated Anthropic routes expose their exact media encoders.
    /// 验证 Kimi 标准 Anthropic 路由与专用视频路由公开各自精确的媒体编码器。
    #[test]
    fn kimi_anthropic_routes_publish_exact_media_capabilities() {
        for (protocol_id, expected_resources) in [
            (ANTHROPIC_MESSAGES_PROFILE_ID, vec![ResourceKind::Image]),
            (
                ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
                vec![ResourceKind::Image, ResourceKind::Video],
            ),
        ] {
            let mut catalog =
                parse_openai_model_catalog("connection", 1, r#"{"data":[{"id":"k3"}]}"#)
                    .expect("Kimi model list should parse");
            enrich_compatible_model_catalog_for_migration_oracle(
                "kimi",
                "coding-plan",
                protocol_id,
                &mut catalog,
            );

            let k3 = catalog.models.first().expect("K3");
            assert_eq!(k3.capabilities.input_resources, expected_resources);
        }
    }

    /// Verifies the Personal International Token Plan keeps its exact published model identity.
    /// 验证个人国际版 Token Plan 保留其精确发布模型身份。
    #[test]
    fn alibaba_personal_global_token_plan_catalog_is_registered() {
        let catalog = build_alibaba_token_plan_catalog(
            "connection",
            "token-plan-personal-global",
            ANTHROPIC_MESSAGES_PROFILE_ID,
            8,
        )
        .expect("Personal International Token Plan must build");
        assert_eq!(catalog.models.len(), 9);
        let flash = catalog
            .models
            .iter()
            .find(|model| model.id == "deepseek-v4-flash-0731")
            .expect("DeepSeek V4 Flash 0731");
        assert_eq!(flash.capabilities.context_limit, None);
        assert_eq!(
            flash.capabilities.evidence_revision,
            "alibaba.token-plan.personal-global.2026-08-01"
        );
        assert!(catalog.models.iter().any(|model| model.id == "glm-5.2"));
        assert!(
            catalog
                .models
                .iter()
                .any(|model| model.id == "deepseek-v4-pro-0813")
        );
    }

    /// Verifies Alibaba Model Studio publishes only reviewed embedding and rerank contracts.
    /// 验证阿里云百炼 Model Studio 仅发布已审核的向量与重排契约。
    #[test]
    fn alibaba_model_studio_enrichment_is_product_and_region_scoped() {
        let mut catalog = parse_openai_model_catalog(
            "connection",
            1,
            r#"{"object":"list","data":[{"id":"text-embedding-v4","object":"model","created":1,"owned_by":"alibaba"},{"id":"text-embedding-v3","object":"model","created":1,"owned_by":"alibaba"},{"id":"qwen3.7-text-embedding","object":"model","created":1,"owned_by":"alibaba"},{"id":"qwen3-rerank","object":"model","created":1,"owned_by":"alibaba"}]}"#,
        )
        .expect("OpenAI-compatible identity list should parse");
        enrich_compatible_model_catalog_for_migration_oracle(
            "alibaba",
            "model-studio-cn",
            OPENAI_CHAT_PROFILE_ID,
            &mut catalog,
        );

        let embedding = catalog
            .models
            .iter()
            .find(|model| model.id == "text-embedding-v4")
            .expect("text embedding v4");
        assert_eq!(
            embedding.capabilities.operations,
            vec![AiOperation::EmbeddingCreate]
        );
        assert_eq!(
            embedding
                .capabilities
                .embedding
                .as_ref()
                .expect("embedding contract")
                .maximum_inputs,
            Some(10)
        );
        let embedding_v3 = catalog
            .models
            .iter()
            .find(|model| model.id == "text-embedding-v3")
            .expect("text embedding v3");
        let embedding_v3_capabilities = embedding_v3
            .capabilities
            .embedding
            .as_ref()
            .expect("V3 embedding contract");
        assert_eq!(embedding_v3_capabilities.maximum_dimensions, Some(1024));
        assert_eq!(embedding_v3_capabilities.default_dimensions, Some(1024));
        assert_eq!(embedding_v3_capabilities.maximum_inputs, Some(10));
        let qwen37 = catalog
            .models
            .iter()
            .find(|model| model.id == "qwen3.7-text-embedding")
            .expect("qwen 3.7 embedding");
        assert_eq!(
            qwen37
                .capabilities
                .embedding
                .as_ref()
                .expect("China embedding contract")
                .maximum_dimensions,
            Some(2560)
        );
        let rerank = catalog
            .models
            .iter()
            .find(|model| model.id == "qwen3-rerank")
            .expect("qwen rerank");
        assert_eq!(
            rerank
                .capabilities
                .rerank
                .as_ref()
                .expect("rerank contract")
                .maximum_candidates,
            Some(500)
        );

        let mut singapore = parse_openai_model_catalog(
            "connection",
            2,
            r#"{"object":"list","data":[{"id":"qwen3.7-text-embedding","object":"model","created":1,"owned_by":"alibaba"}]}"#,
        )
        .expect("OpenAI-compatible identity list should parse");
        enrich_compatible_model_catalog_for_migration_oracle(
            "alibaba",
            "model-studio-singapore",
            OPENAI_CHAT_PROFILE_ID,
            &mut singapore,
        );
        assert!(
            singapore.models[0].capabilities.embedding.is_none(),
            "China-only embedding must stay unavailable in Singapore"
        );
    }

    /// Verifies MiniMax discovery keeps M3 and M2 capability differences explicit.
    /// 验证 MiniMax 发现明确保留 M3 与 M2 的能力差异。
    #[test]
    fn minimax_discovery_projects_exact_model_families() {
        let mut catalog = parse_minimax_model_catalog(
            "connection",
            1,
            r#"{"data":[{"id":"MiniMax-M3","display_name":"MiniMax M3"},{"id":"MiniMax-M2.7","display_name":"MiniMax M2.7"},{"id":"MiniMax-Future","display_name":"Future"}],"has_more":false,"last_id":"MiniMax-Future"}"#,
        )
        .expect("documented identities should parse");
        assert!(
            catalog
                .models
                .iter()
                .all(|model| model.capabilities.context_limit.is_none()),
            "raw discovery must not infer vendor capability tables"
        );
        let manifests = crate::provider_manifest::load_bundled_resolved_catalog();
        let manifest = manifests
            .manifest_arc("minimax")
            .expect("MiniMax manifest must exist");
        enrich_compatible_model_catalog_with_manifest(
            "minimax",
            "global",
            Some("payg-api-key"),
            ANTHROPIC_MESSAGES_PROFILE_ID,
            &mut catalog,
            &manifest,
        );
        let m3 = &catalog.models[0].capabilities;
        assert_eq!(m3.context_limit, Some(1_000_000));
        // MiniMax M3 is the manifest-declared multimodal model for image and video input.
        // MiniMax M3 是 Manifest 明确声明支持图片与视频输入的多模态模型。
        assert_eq!(
            m3.input_resources,
            vec![ResourceKind::Image, ResourceKind::Video]
        );
        assert_eq!(
            m3.structured_output,
            StructuredOutputCapability::Unsupported
        );
        assert!(!m3.prompt_cache.supports_managed_explicit);
        assert_eq!(m3.model_parameters[0].id, SAMPLING_TEMPERATURE_PARAMETER_ID);

        let m2 = &catalog.models[1].capabilities;
        assert_eq!(m2.context_limit, Some(204_800));
        assert!(m2.input_resources.is_empty());
        assert_eq!(
            m2.structured_output,
            StructuredOutputCapability::Unsupported
        );
        assert!(m2.prompt_cache.supports_managed_explicit);
        assert_eq!(m2.prompt_cache.minimum_cacheable_tokens, Some(512));

        let unknown = &catalog.models[2].capabilities;
        assert_eq!(unknown.context_limit, None);
        assert!(unknown.model_parameters.is_empty());
    }

    /// Verifies MiniMax OpenAI discovery registers only its exact compatible-route parameters.
    /// 验证 MiniMax OpenAI 发现仅注册其精确兼容路由参数。
    #[test]
    fn minimax_openai_discovery_is_protocol_scoped() {
        let mut catalog = parse_minimax_openai_model_catalog(
            "connection",
            1,
            r#"{"object":"list","data":[{"id":"MiniMax-M3","object":"model","created":1,"owned_by":"minimax"},{"id":"MiniMax-M2.7","object":"model","created":1,"owned_by":"minimax"}]}"#,
        )
        .expect("documented OpenAI-compatible response should parse");
        let manifests = crate::provider_manifest::load_bundled_resolved_catalog();
        let manifest = manifests
            .manifest_arc("minimax")
            .expect("MiniMax manifest must exist");
        enrich_compatible_model_catalog_with_manifest(
            "minimax",
            "global",
            Some("payg-api-key"),
            OPENAI_CHAT_PROFILE_ID,
            &mut catalog,
            &manifest,
        );
        let capabilities = &catalog
            .models
            .iter()
            .find(|model| model.id == "MiniMax-M2.7")
            .expect("MiniMax M2.7 must remain in the enriched catalog")
            .capabilities;
        assert_eq!(
            capabilities.structured_output,
            StructuredOutputCapability::Unsupported
        );
        assert!(!capabilities.prompt_cache.supports_managed_explicit);
        assert!(capabilities.model_parameters.iter().all(|parameter| {
            parameter
                .protocol_ids
                .iter()
                .any(|protocol_id| protocol_id == OPENAI_CHAT_PROFILE_ID)
        }));
        let reasoning_split = capabilities
            .model_parameters
            .iter()
            .find(|parameter| parameter.id == MINIMAX_REASONING_SPLIT_PARAMETER_ID)
            .expect("reasoning split should be declared");
        assert_eq!(
            reasoning_split.default,
            ModelParameterDefault::ExplicitValue {
                value: vulcan_ai_contract::ModelParameterValue::Boolean(true)
            }
        );
        let m3 = &catalog
            .models
            .iter()
            .find(|model| model.id == "MiniMax-M3")
            .expect("MiniMax M3 must remain in the enriched catalog")
            .capabilities;
        assert_eq!(
            m3.structured_output,
            StructuredOutputCapability::Unsupported
        );
    }

    /// Verifies OpenRouter discovery uses only explicit API capability fields.
    /// 验证 OpenRouter 发现仅使用 API 明确返回的能力字段。
    #[test]
    fn openrouter_discovery_uses_explicit_capabilities() {
        let catalog = parse_openrouter_model_catalog(
            "connection",
            1,
            r#"{"data":[{"id":"deepseek/deepseek-v4-pro","name":"DeepSeek: DeepSeek V4 Pro","context_length":8192,"supported_parameters":["tools","temperature"],"architecture":{"input_modalities":["text","image"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096},"reasoning":{"supported_efforts":["max","high","none"],"supports_max_tokens":true,"mandatory":true}}]}"#,
        )
        .expect("documented response should parse");
        assert_eq!(catalog.models[0].display_name, "DeepSeek V4 Pro");
        assert_eq!(catalog.models[0].capabilities.context_limit, Some(8192));
        assert_eq!(catalog.models[0].capabilities.output_limit, Some(4096));
        assert_eq!(
            catalog.models[0].capabilities.input_resources,
            vec![ResourceKind::Image]
        );
        assert_eq!(
            catalog.models[0].capabilities.reasoning_efforts,
            vec![ReasoningEffort::High, ReasoningEffort::Max]
        );
        assert_eq!(catalog.models[0].capabilities.model_parameters.len(), 2);
        assert!(
            catalog.models[0]
                .capabilities
                .model_parameters
                .iter()
                .any(|parameter| parameter.id == REASONING_MAX_TOKENS_PARAMETER_ID)
        );
    }

    /// Verifies an unmatched default-bound model keeps only permitted discovery evidence.
    /// 验证未匹配但允许默认绑定的模型仅保留获准的发现证据。
    #[test]
    fn default_binding_capabilities_are_signed_and_discovery_bounded() {
        let mut catalog = parse_openrouter_model_catalog(
            "connection",
            1,
            r#"{"data":[{"id":"deepseek/future-model","name":"Future","context_length":131072,"supported_parameters":["tools"],"architecture":{"input_modalities":["text","image"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":8192}}]}"#,
        )
        .expect("explicit discovery evidence must parse");
        let manifests = crate::provider_manifest::load_bundled_resolved_catalog();
        let manifest = manifests
            .manifest("anthropic")
            .expect("Anthropic manifest must exist");
        enrich_compatible_model_catalog_with_manifest(
            "anthropic",
            "public-api",
            Some("api-key"),
            ANTHROPIC_MESSAGES_PROFILE_ID,
            &mut catalog,
            manifest,
        );

        let capabilities = &catalog.models[0].capabilities;
        assert_eq!(capabilities.context_limit, Some(131_072));
        assert_eq!(capabilities.output_limit, Some(8_192));
        assert_eq!(capabilities.input_resources, vec![ResourceKind::Image]);
        assert!(
            capabilities
                .operations
                .contains(&AiOperation::ConversationRespond)
        );
        assert_eq!(
            capabilities.structured_output,
            vulcan_ai_contract::StructuredOutputCapability::JsonSchema
        );
        assert!(
            capabilities
                .evidence_revision
                .contains("anthropic.default-capabilities.v2+discovery:")
        );
    }

    /// Verifies inconsistent OpenRouter completion metadata cannot invalidate the whole account catalog.
    /// 验证 OpenRouter 不一致的输出元数据不会使整个账号目录失效。
    #[test]
    fn openrouter_discovery_bounds_completion_limit_by_context() {
        // Parse documented field scopes with a deliberately inconsistent provider output limit.
        // 使用文档化字段作用域解析一组刻意不一致的端点输出上限。
        let catalog = parse_openrouter_model_catalog(
            "connection",
            1,
            r#"{"data":[{"id":"google/gemini-3.1-flash-lite-image","name":"Google: Nano Banana 2 Lite","context_length":65536,"supported_parameters":["max_tokens"],"architecture":{"input_modalities":["text","image"],"output_modalities":["text","image"]},"top_provider":{"context_length":131072,"max_completion_tokens":131072}}]}"#,
        )
        .expect("OpenRouter metadata should be normalized into a safe catalog");

        assert_eq!(catalog.models[0].capabilities.context_limit, Some(65_536));
        assert_eq!(catalog.models[0].capabilities.output_limit, Some(65_536));
    }

    /// Verifies OpenRouter exposes conversation only when both text modalities are declared.
    /// 验证 OpenRouter 仅在输入与输出均声明文本模态时公开会话能力。
    #[test]
    fn openrouter_conversation_requires_text_input_and_output() {
        let catalog = parse_openrouter_model_catalog(
            "connection",
            1,
            r#"{"data":[{"id":"example/chat","name":"Chat","context_length":8192,"supported_parameters":[],"architecture":{"input_modalities":["text"],"output_modalities":["text"]}},{"id":"example/embed","name":"Embed","context_length":8192,"supported_parameters":[],"architecture":{"input_modalities":["text"],"output_modalities":["embeddings"]}},{"id":"example/image-only","name":"Image Only","context_length":8192,"supported_parameters":[],"architecture":{"input_modalities":["image"],"output_modalities":["text"]}}]}"#,
        )
        .expect("documented modality records should parse");

        let operations = catalog
            .models
            .iter()
            .map(|model| (model.id.as_str(), model.capabilities.operations.as_slice()))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            operations["example/chat"],
            [AiOperation::ConversationRespond]
        );
        assert!(operations["example/embed"].is_empty());
        assert!(operations["example/image-only"].is_empty());
    }

    /// Verifies omitted OpenRouter reasoning metadata remains unknown instead of becoming an explicit disablement.
    /// 验证 OpenRouter 省略思考元数据时保持未知，而不是被错误解释为明确关闭。
    #[test]
    fn openrouter_omitted_reasoning_metadata_remains_unknown() {
        let catalog = parse_openrouter_model_catalog(
            "connection",
            1,
            r#"{"data":[{"id":"example/chat","name":"Chat","context_length":8192,"supported_parameters":[],"architecture":{"input_modalities":["text"],"output_modalities":["text"]}}]}"#,
        )
        .expect("OpenRouter record without reasoning metadata should parse");
        let capabilities = &catalog.models[0].capabilities;

        assert_eq!(capabilities.reasoning, CapabilitySupport::Unknown);
        assert_eq!(
            capabilities.reasoning_control,
            ReasoningControlMode::Unknown
        );
        assert!(capabilities.reasoning_efforts.is_empty());
    }

    /// Verifies atomic OpenRouter discovery separates conversation, embedding, and rerank evidence.
    /// 验证 OpenRouter 原子发现会分离会话、向量与重排证据。
    #[test]
    fn openrouter_complete_discovery_merges_only_exact_specialized_operations() {
        let catalog = parse_openrouter_complete_model_catalog(
            "connection",
            9,
            r#"{"data":[{"id":"example/chat","name":"Example: Chat","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]}}]}"#,
            r#"{"data":[{"id":"example/embed","name":"Example: Embed","context_length":4096,"supported_parameters":[],"architecture":{"input_modalities":["text","image"],"output_modalities":["embeddings"]}}]}"#,
            r#"{"data":[{"id":"example/chat","name":"Example: Chat","context_length":8192,"supported_parameters":[],"architecture":{"input_modalities":["text"],"output_modalities":["text"]}},{"id":"example/rerank","name":"Example: Rerank","context_length":32768,"supported_parameters":[],"architecture":{"input_modalities":["text"],"output_modalities":["rerank"]}}]}"#,
        )
        .expect("all three exact source documents should merge");

        assert_eq!(catalog.generation, 9);
        assert_eq!(catalog.models.len(), 3);
        let embedding = catalog
            .models
            .iter()
            .find(|model| model.id == "example/embed")
            .expect("embedding model");
        assert_eq!(
            embedding.capabilities.operations,
            vec![AiOperation::EmbeddingCreate]
        );
        assert!(
            embedding
                .capabilities
                .embedding
                .as_ref()
                .expect("embedding contract")
                .input_resources
                .is_empty()
        );
        let rerank = catalog
            .models
            .iter()
            .find(|model| model.id == "example/rerank")
            .expect("rerank model");
        assert_eq!(
            rerank.capabilities.operations,
            vec![AiOperation::RerankDocuments]
        );
        assert_eq!(
            rerank
                .capabilities
                .rerank
                .as_ref()
                .expect("rerank contract")
                .truncation,
            BTreeSet::from([RerankTruncation::Disabled])
        );
    }

    /// Verifies signed generic native bindings retain specialized OpenRouter discovery evidence.
    /// 验证签名的通用原生绑定保留 OpenRouter 专用发现证据。
    #[test]
    fn openrouter_unknown_specialized_models_keep_signed_native_operations() {
        let mut catalog = parse_openrouter_complete_model_catalog(
            "connection",
            1,
            r#"{"data":[]}"#,
            r#"{"data":[{"id":"vendor/novel-embed","name":"Novel Embed","context_length":4096,"architecture":{"input_modalities":["text"],"output_modalities":["embeddings"]}}]}"#,
            r#"{"data":[{"id":"vendor/novel-rerank","name":"Novel Rerank","context_length":4096,"architecture":{"input_modalities":["text"],"output_modalities":["rerank"]}},{"id":"vendor/novel-image","name":"Novel Image","context_length":4096,"architecture":{"input_modalities":["text"],"output_modalities":["image"]}}]}"#,
        )
        .expect("dedicated directories should parse");
        let manifests = crate::provider_manifest::load_bundled_resolved_catalog();
        let manifest = manifests
            .manifest("openrouter")
            .expect("OpenRouter manifest should resolve");
        enrich_compatible_model_catalog_with_manifest(
            "openrouter",
            "public-api",
            Some("api-key"),
            ANTHROPIC_MESSAGES_PROFILE_ID,
            &mut catalog,
            manifest,
        );
        let embedding = catalog
            .models
            .iter()
            .find(|model| model.id == "vendor/novel-embed")
            .expect("embedding identity should remain visible");
        assert_eq!(
            embedding.capabilities.operations,
            vec![AiOperation::EmbeddingCreate]
        );
        assert!(embedding.capabilities.embedding.is_some());
        let rerank = catalog
            .models
            .iter()
            .find(|model| model.id == "vendor/novel-rerank")
            .expect("rerank identity should remain visible");
        assert_eq!(
            rerank.capabilities.operations,
            vec![AiOperation::RerankDocuments]
        );
        assert!(rerank.capabilities.rerank.is_some());
        let image = catalog
            .models
            .iter()
            .find(|model| model.id == "vendor/novel-image")
            .expect("image-only identity should remain visible");
        assert!(image.capabilities.operations.is_empty());
        assert_eq!(
            image.capabilities.output_resources,
            vec![ResourceKind::Image]
        );
    }

    /// Verifies the same MiniMax-M3 family keeps provider-specific reasoning contracts.
    /// 验证同一 MiniMax-M3 系列保留供应商专属思考契约。
    #[test]
    fn minimax_m3_reasoning_contract_differs_between_direct_and_openrouter_catalogs() {
        let direct = parse_minimax_model_catalog(
            "minimax-connection",
            1,
            r#"{"data":[{"id":"MiniMax-M3","display_name":"MiniMax-M3","created_at":1}],"has_more":false,"last_id":"MiniMax-M3"}"#,
        )
        .expect("MiniMax direct catalog should parse");
        let openrouter = parse_openrouter_model_catalog(
            "openrouter-connection",
            1,
            r#"{"data":[{"id":"minimax/minimax-m3","name":"MiniMax: MiniMax M3","context_length":1048576,"supported_parameters":["reasoning","tools"],"architecture":{"input_modalities":["text","image","video"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":512000},"reasoning":{"mandatory":false}}]}"#,
        )
        .expect("OpenRouter catalog should parse");

        // The direct parser stays conservative; the bundled manifest rule carries the reviewed
        // reasoning contract (toggle with none/adaptive).
        // 直接解析保持保守；已审核思考契约由内置清单规则承载（toggle + none/adaptive）。
        assert!(direct.models[0].capabilities.reasoning_efforts.is_empty());
        let manifests = super::super::provider_manifest::load_bundled_resolved_manifests();
        let minimax = manifests
            .get("minimax")
            .expect("bundled minimax manifest must load");
        let mut model = direct.models[0].clone();
        assert!(super::apply_manifest_model_rule(
            minimax,
            "global",
            None,
            ANTHROPIC_MESSAGES_PROFILE_ID,
            &mut model
        ));
        assert_eq!(
            model.capabilities.reasoning_efforts,
            vec![ReasoningEffort::None, ReasoningEffort::Adaptive]
        );
        assert_eq!(
            model.capabilities.reasoning_control,
            ReasoningControlMode::Toggle
        );
        assert_eq!(
            openrouter.models[0].capabilities.reasoning_efforts,
            vec![ReasoningEffort::None, ReasoningEffort::Adaptive]
        );
    }

    /// Verifies explicit null still means every OpenRouter gateway effort is selectable.
    /// 验证显式 null 仍表示所有 OpenRouter 网关档位均可选择。
    #[test]
    fn openrouter_null_supported_efforts_exposes_all_gateway_levels() {
        let catalog = parse_openrouter_model_catalog(
            "openrouter-connection",
            1,
            r#"{"data":[{"id":"example/reasoner","name":"Example Reasoner","context_length":8192,"supported_parameters":["reasoning"],"reasoning":{"supported_efforts":null,"mandatory":false}}]}"#,
        )
        .expect("OpenRouter catalog should parse");

        assert_eq!(
            catalog.models[0].capabilities.reasoning_efforts,
            vec![
                ReasoningEffort::None,
                ReasoningEffort::Minimal,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::XHigh,
                ReasoningEffort::Max,
            ]
        );
    }

    /// Verifies compatible discovery preserves identity without inventing provider capabilities.
    /// 验证兼容目录发现保留模型身份且不臆造供应商能力。
    #[test]
    fn openai_compatible_discovery_remains_conservative() {
        let catalog = parse_openai_model_catalog(
            "connection",
            1,
            r#"{"object":"list","first_id":"gpt-test","has_more":false,"data":[{"id":"gpt-test","object":"model","created":1,"owned_by":"openai"}]}"#,
        )
        .expect("documented response should parse");
        assert_eq!(catalog.page_count, 1);
        assert_eq!(
            catalog.discovery_schema_revision,
            "openai-compatible.models.v1"
        );
        assert_eq!(catalog.models[0].id, "gpt-test");
        assert_eq!(
            catalog.models[0].capabilities.operations,
            vec![AiOperation::ConversationRespond]
        );
        assert_eq!(
            catalog.models[0].capabilities.evidence_revision,
            "openai-compatible.models.v1"
        );
        assert_eq!(
            catalog.models[0].capability_evidence,
            ModelCapabilityEvidence::default()
        );
    }

    /// Verifies a compatible catalog rejects empty and duplicate upstream model identities.
    /// 验证兼容目录会拒绝空白及重复的上游模型标识。
    #[test]
    fn openai_compatible_discovery_requires_stable_model_identities() {
        // empty_error proves whitespace-only provider identifiers never enter runtime catalogs.
        // empty_error 证明仅含空白的供应商标识绝不会进入运行时目录。
        let empty_error = parse_openai_model_catalog("connection", 1, r#"{"data":[{"id":"   "}]}"#)
            .expect_err("an empty model identity must fail");
        assert!(empty_error.contains("empty model identifier"));

        // duplicate_error proves one atomic response cannot rely on later UI de-duplication.
        // duplicate_error 证明一份原子响应不能依赖后续 UI 去重。
        let duplicate_error = parse_openai_model_catalog(
            "connection",
            1,
            r#"{"data":[{"id":"model-a"},{"id":"model-a"}]}"#,
        )
        .expect_err("duplicate model identities must fail");
        assert!(duplicate_error.contains("duplicate model `model-a`"));
    }

    /// Verifies explicit non-text output prevents a compatible model from entering conversation catalogs.
    /// 验证显式非文本输出会阻止兼容模型进入会话目录。
    #[test]
    fn openai_compatible_explicit_non_text_output_is_not_a_conversation_model() {
        let catalog = parse_openai_model_catalog(
            "connection",
            1,
            r#"{"object":"list","data":[{"id":"text-embedding-3-small","object":"model","created":1,"owned_by":"relay","input_modalities":["text"],"output_modalities":["embedding"]}]}"#,
        )
        .expect("explicit modality evidence should parse");

        assert!(
            !catalog.models[0]
                .capabilities
                .operations
                .contains(&AiOperation::ConversationRespond)
        );
        assert!(
            catalog.models[0]
                .capability_evidence
                .input_modalities_declared
        );
        assert!(
            catalog.models[0]
                .capability_evidence
                .output_modalities_declared
        );
        assert_eq!(
            catalog.models[0].capability_evidence.supports_text_input,
            Some(true)
        );
        assert_eq!(
            catalog.models[0].capability_evidence.supports_text_output,
            Some(false)
        );
    }

    /// Verifies a relay model name cannot acquire official OpenAI reasoning from wire format alone.
    /// 验证中转站模型名不能仅凭 Wire 格式获得 OpenAI 官方思考能力。
    #[test]
    fn relay_discovery_does_not_inherit_official_openai_reasoning() {
        let catalog = parse_openai_model_catalog(
            "connection",
            1,
            r#"{"object":"list","data":[{"id":"gpt-5.6","object":"model","created":1,"owned_by":"relay"}]}"#,
        )
        .expect("compatible response should parse");

        assert_eq!(
            catalog.models[0].capabilities.reasoning,
            CapabilitySupport::Unknown
        );
        assert!(catalog.models[0].capabilities.reasoning_efforts.is_empty());
        assert_eq!(
            catalog.models[0].capabilities.evidence_revision,
            "openai-compatible.models.v1"
        );
    }

    /// Verifies official OpenAI enrichment still applies after provider identity is known.
    /// 验证供应商身份已知后仍会应用 OpenAI 官方能力修订。
    #[test]
    fn official_openai_enrichment_applies_reasoning() {
        let mut catalog = parse_openai_model_catalog(
            "connection",
            1,
            r#"{"object":"list","data":[{"id":"gpt-5.6","object":"model","created":1,"owned_by":"openai"}]}"#,
        )
        .expect("official response should parse");
        enrich_compatible_model_catalog_for_migration_oracle(
            "openai",
            "public-api",
            OPENAI_CHAT_PROFILE_ID,
            &mut catalog,
        );

        assert_ne!(
            catalog.models[0].capabilities.reasoning,
            CapabilitySupport::Unknown
        );
        assert_ne!(
            catalog.models[0].capabilities.evidence_revision,
            "openai-compatible.models.v1"
        );
    }

    /// Verifies DeepSeek discovery exposes documented reasoning and optional temperature controls.
    /// 验证 DeepSeek 模型发现暴露文档化的思考与可选温度控制。
    #[test]
    fn deepseek_official_discovery_exposes_toggle_reasoning() {
        let mut catalog = parse_openai_model_catalog(
            "connection",
            1,
            r#"{"object":"list","data":[{"id":"deepseek-v4-pro","object":"model","created":1,"owned_by":"deepseek"}]}"#,
        )
        .expect("documented response should parse");
        enrich_compatible_model_catalog_for_migration_oracle(
            "deepseek",
            "official-api",
            OPENAI_CHAT_PROFILE_ID,
            &mut catalog,
        );

        let capabilities = &catalog.models[0].capabilities;
        assert_eq!(capabilities.reasoning_control, ReasoningControlMode::Toggle);
        assert_eq!(
            capabilities.reasoning_efforts,
            vec![
                ReasoningEffort::None,
                ReasoningEffort::High,
                ReasoningEffort::Low,
                ReasoningEffort::Max,
            ]
        );
        assert!(capabilities.provider_native_tools.is_empty());
        let temperature = capabilities
            .model_parameters
            .iter()
            .find(|parameter| parameter.id == SAMPLING_TEMPERATURE_PARAMETER_ID)
            .expect("DeepSeek Chat should expose documented temperature control");
        assert_eq!(
            temperature.default,
            ModelParameterDefault::ProviderDefault {}
        );
        assert_eq!(
            temperature.protocol_ids,
            vec![String::from(OPENAI_CHAT_PROFILE_ID)]
        );

        for protocol_id in [ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID] {
            let mut protocol_catalog = catalog.clone();
            enrich_compatible_model_catalog_for_migration_oracle(
                "deepseek",
                "official-api",
                protocol_id,
                &mut protocol_catalog,
            );
            assert_eq!(
                protocol_catalog.models[0]
                    .capabilities
                    .provider_native_tools,
                vec![NativeToolKind::WebSearch]
            );
        }
    }

    /// Verifies Anthropic discovery uses documented identity and pagination fields.
    /// 验证 Anthropic 发现使用文档化身份及分页字段。
    #[test]
    fn anthropic_discovery_parses_page_contract() {
        let page = parse_anthropic_model_page(
            r#"{"data":[{"type":"model","id":"claude-test","display_name":"Claude Test","created_at":"2026-01-01T00:00:00Z"}],"has_more":false,"first_id":"claude-test","last_id":"claude-test"}"#,
            true,
        )
        .expect("documented response should parse");
        assert!(!page.has_more);
        assert_eq!(page.last_id.as_deref(), Some("claude-test"));
        assert_eq!(
            page.models[0].capabilities.operations,
            vec![AiOperation::ConversationRespond]
        );
        assert!(
            page.models[0]
                .capabilities
                .prompt_cache
                .supports_managed_explicit
        );
        assert_eq!(
            page.models[0]
                .capabilities
                .prompt_cache
                .supported_ttl_options
                .iter()
                .map(|option| option.id.as_str())
                .collect::<Vec<_>>(),
            vec!["5m", "1h"]
        );
        let catalog = build_anthropic_model_catalog(
            "connection",
            9,
            page.models,
            3,
            "anthropic.models.2023-06-01",
        );
        assert_eq!(catalog.page_count, 3);
        assert_eq!(
            catalog.discovery_schema_revision,
            "anthropic.models.2023-06-01"
        );
    }

    /// Verifies Gemini discovery maps only provider-reported methods and limits.
    /// 验证 Gemini 发现只映射供应商报告的方法与限制。
    #[test]
    fn gemini_discovery_uses_reported_methods_and_limits() {
        let page = parse_gemini_model_page(
            r#"{"models":[{"name":"models/gemini-test","displayName":"Gemini Test","inputTokenLimit":1024,"outputTokenLimit":256,"supportedGenerationMethods":["generateContent","embedContent","unknownMethod"]}],"nextPageToken":"next"}"#,
        )
        .expect("documented response should parse");
        assert_eq!(page.next_page_token.as_deref(), Some("next"));
        assert_eq!(page.models[0].id, "gemini-test");
        assert_eq!(page.models[0].capabilities.context_limit, Some(1024));
        assert_eq!(
            page.models[0].capabilities.operations,
            vec![
                AiOperation::ConversationRespond,
                AiOperation::EmbeddingCreate
            ]
        );
    }

    /// Verifies Codex discovery preserves its account-visible slug and context declaration.
    /// 验证 Codex 发现会保留账号可见的 Slug 与上下文声明。
    #[test]
    fn codex_discovery_uses_account_visible_models() {
        let catalog = parse_openai_codex_model_catalog(
            "connection",
            3,
            r#"{"models":[{"slug":"gpt-codex","display_name":"GPT Codex","context_window":200000,"visibility":"list","supported_reasoning_levels":[{"effort":"none"},{"effort":"low"},{"effort":"medium"},{"effort":"high"},{"effort":"xhigh"},{"effort":"max"},{"effort":"ultra"},{"effort":"future"}]}]}"#,
        )
        .expect("Codex model response should parse");
        assert_eq!(catalog.generation, 3);
        assert_eq!(catalog.models[0].id, "gpt-codex");
        assert_eq!(catalog.models[0].display_name, "GPT Codex");
        assert_eq!(catalog.models[0].capabilities.context_limit, Some(200000));
        assert_eq!(
            catalog.models[0].capabilities.reasoning_efforts,
            vec![
                ReasoningEffort::None,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::XHigh,
                ReasoningEffort::Max,
                ReasoningEffort::Ultra,
            ]
        );
    }

    /// Verifies Code Assist discovery maps only returned identity and token limits.
    /// 验证 Code Assist 发现仅映射返回的身份与 Token 限制。
    #[test]
    fn code_assist_discovery_uses_returned_model_map() {
        let catalog = parse_google_code_assist_model_catalog(
            "connection",
            4,
            r#"{"models":{"gemini-code":{"displayName":"Gemini Code","maxTokens":1048576,"maxOutputTokens":65536,"recommended":true}}}"#,
        )
        .expect("Code Assist model response should parse");
        assert_eq!(catalog.generation, 4);
        assert_eq!(catalog.models[0].id, "gemini-code");
        assert_eq!(catalog.models[0].display_name, "Gemini Code");
        assert_eq!(catalog.models[0].capabilities.context_limit, Some(1048576));
        assert_eq!(catalog.models[0].capabilities.output_limit, Some(65536));
    }

    /// Compiles the bundled deepseek manifest for manifest-rule enrichment tests.
    /// 编译内置 deepseek 清单供 Manifest 规则补全测试使用。
    fn bundled_deepseek_manifest() -> crate::provider_manifest::ResolvedProviderManifest {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("vulcan-runtime-assets")
            .join("resources")
            .join("provider-manifests")
            .join("manifests")
            .join("deepseek.json");
        let text = std::fs::read_to_string(&path).expect("read bundled deepseek manifest");
        crate::provider_manifest::compile_manifest_text(
            &text,
            crate::provider_manifest::ManifestOwnership::System,
        )
        .expect("bundled deepseek manifest must compile")
    }

    /// Verifies manifest rules apply protocol-scoped provider-native tools.
    /// 验证 Manifest 规则应用协议作用域供应商原生工具。
    #[test]
    fn manifest_rule_applies_protocol_scoped_web_search() {
        let resolved = bundled_deepseek_manifest();
        let mut model = ConnectionModel {
            id: String::from("deepseek-v4-pro"),
            display_name: String::from("DeepSeek V4 Pro"),
            capabilities: ModelCapabilities::default(),
            capability_evidence: Default::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        };
        let matched = apply_manifest_model_rule(
            &resolved,
            "official-api",
            None,
            ANTHROPIC_MESSAGES_PROFILE_ID,
            &mut model,
        );
        assert!(matched, "flash must match the exact rule");
        assert_eq!(
            model.capabilities.provider_native_tools,
            vec![NativeToolKind::WebSearch]
        );
        assert_eq!(model.capabilities.context_limit, Some(1_000_000));
        assert_eq!(model.capabilities.reasoning, CapabilitySupport::Native);
        assert_eq!(
            model.capabilities.reasoning_control,
            ReasoningControlMode::Toggle
        );
        assert!(
            model
                .capabilities
                .model_parameters
                .iter()
                .any(|parameter| parameter.id == SAMPLING_TEMPERATURE_PARAMETER_ID)
        );

        // The Responses protocol carries the same provider-native search tool as Anthropic.
        // Responses 协议与 Anthropic 一样携带供应商原生搜索工具。
        let mut responses_model = ConnectionModel {
            id: String::from("deepseek-v4-pro"),
            display_name: String::from("DeepSeek V4 Pro"),
            capabilities: ModelCapabilities::default(),
            capability_evidence: Default::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        };
        assert!(apply_manifest_model_rule(
            &resolved,
            "official-api",
            None,
            OPENAI_RESPONSES_PROFILE_ID,
            &mut responses_model,
        ));
        assert_eq!(
            responses_model.capabilities.provider_native_tools,
            vec![NativeToolKind::WebSearch]
        );

        // The chat protocol carries no native search tool for the same model.
        // 同一模型在 chat 协议下不携带原生搜索工具。
        let mut chat_model = ConnectionModel {
            id: String::from("deepseek-v4-pro"),
            display_name: String::from("DeepSeek V4 Pro"),
            capabilities: ModelCapabilities::default(),
            capability_evidence: Default::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        };
        apply_manifest_model_rule(
            &resolved,
            "official-api",
            None,
            OPENAI_CHAT_PROFILE_ID,
            &mut chat_model,
        );
        assert!(chat_model.capabilities.provider_native_tools.is_empty());

        // V4 Pro exposes the same native search capability through Anthropic and Responses.
        // V4 Pro 通过 Anthropic 与 Responses 暴露相同的原生搜索能力。
        let mut pro_anthropic_model = ConnectionModel {
            id: String::from("deepseek-v4-pro"),
            display_name: String::from("DeepSeek V4 Pro"),
            capabilities: ModelCapabilities::default(),
            capability_evidence: Default::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        };
        assert!(apply_manifest_model_rule(
            &resolved,
            "official-api",
            None,
            ANTHROPIC_MESSAGES_PROFILE_ID,
            &mut pro_anthropic_model,
        ));
        assert_eq!(
            pro_anthropic_model.capabilities.provider_native_tools,
            vec![NativeToolKind::WebSearch]
        );
        // V4 Pro Responses must stay capability-equivalent to V4 Flash Responses.
        // V4 Pro Responses 必须与 V4 Flash Responses 保持能力等价。
        let mut pro_responses_model = ConnectionModel {
            id: String::from("deepseek-v4-pro"),
            display_name: String::from("DeepSeek V4 Pro"),
            capabilities: ModelCapabilities::default(),
            capability_evidence: Default::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        };
        assert!(apply_manifest_model_rule(
            &resolved,
            "official-api",
            None,
            OPENAI_RESPONSES_PROFILE_ID,
            &mut pro_responses_model,
        ));
        assert_eq!(
            pro_responses_model.capabilities.provider_native_tools,
            vec![NativeToolKind::WebSearch]
        );
        // V4 Pro Chat remains conversation-only and must not inherit native search.
        // V4 Pro Chat 保持纯会话能力，绝不能继承原生搜索。
        let mut pro_chat_model = ConnectionModel {
            id: String::from("deepseek-v4-pro"),
            display_name: String::from("DeepSeek V4 Pro"),
            capabilities: ModelCapabilities::default(),
            capability_evidence: Default::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        };
        assert!(apply_manifest_model_rule(
            &resolved,
            "official-api",
            None,
            OPENAI_CHAT_PROFILE_ID,
            &mut pro_chat_model,
        ));
        assert!(pro_chat_model.capabilities.provider_native_tools.is_empty());
    }

    /// Verifies a model-rule parameter replaces a lower-precedence parameter with the same stable id.
    /// 验证模型规则参数会替换具有相同稳定 ID 的低优先级参数。
    #[test]
    fn manifest_rule_parameter_overrides_existing_same_id_definition() {
        let resolved = bundled_deepseek_manifest();
        let instance = resolved
            .rule_for_model("official-api", None, "deepseek-v4-pro")
            .expect("pro must match the bundled exact rule");
        let expected = instance
            .model_parameters
            .iter()
            .find(|parameter| parameter.id == SAMPLING_TEMPERATURE_PARAMETER_ID)
            .expect("the bundled rule must declare sampling temperature")
            .clone();
        // staleDefinition models lower-precedence discovery data for the same parameter id.
        // staleDefinition 模拟同一参数 ID 的低优先级发现数据。
        let mut stale_definition = expected.clone();
        stale_definition.description = String::from("stale discovery definition");
        let mut model = ConnectionModel {
            id: String::from("deepseek-v4-pro"),
            display_name: String::from("DeepSeek V4 Pro"),
            capabilities: ModelCapabilities {
                model_parameters: vec![stale_definition],
                ..ModelCapabilities::default()
            },
            capability_evidence: Default::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        };

        assert!(apply_manifest_model_rule(
            &resolved,
            "official-api",
            None,
            ANTHROPIC_MESSAGES_PROFILE_ID,
            &mut model,
        ));

        let actual = model
            .capabilities
            .model_parameters
            .iter()
            .find(|parameter| parameter.id == SAMPLING_TEMPERATURE_PARAMETER_ID)
            .expect("effective parameters must retain sampling temperature");
        assert_eq!(actual, &expected);
    }

    /// Verifies V4 Pro selects Responses and keeps its own protocol declaration.
    /// 验证 V4 Pro 选择 Responses 并保持自身的协议声明。
    ///
    /// v4-flash 在官方侧已不存在，故不再镜像该已退役身份。
    #[test]
    fn manifest_rule_v4_pro_declares_responses_parameters() {
        let resolved = bundled_deepseek_manifest();
        let pro_rule = resolved
            .rule_for_model("official-api", None, "deepseek-v4-pro")
            .expect("pro must match a rule");
        let pro = resolved
            .rule_for_model("official-api", None, "deepseek-v4-pro")
            .expect("pro must match a rule");
        assert_eq!(
            pro.selected_protocol.as_deref(),
            Some(OPENAI_RESPONSES_PROFILE_ID)
        );
        assert_eq!(pro.selected_protocol, pro_rule.selected_protocol);
        assert_eq!(pro.supported_protocols, pro_rule.supported_protocols);
    }

    /// Verifies unmatched models report no manifest match so legacy enrichment remains.
    /// 验证未命中模型报告无 Manifest 匹配，使旧补全路径保持。
    #[test]
    fn unmatched_model_reports_no_manifest_match() {
        let resolved = bundled_deepseek_manifest();
        let mut model = ConnectionModel {
            id: String::from("unknown-model-9"),
            display_name: String::from("Unknown"),
            capabilities: ModelCapabilities::default(),
            capability_evidence: Default::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        };
        let matched = apply_manifest_model_rule(
            &resolved,
            "official-api",
            None,
            ANTHROPIC_MESSAGES_PROFILE_ID,
            &mut model,
        );
        assert!(!matched);
    }
}

#[cfg(test)]
mod manifest_migration_tests {
    //! Equivalence oracle: migrated manifest rules must reproduce the legacy code-owned
    //! capability enrichment exactly (except evidence revisions).
    //! 等价 oracle：迁移后的清单规则必须精确复现旧版代码持有能力补全（证据修订号除外）。

    use super::*;
    use crate::provider_manifest::load_bundled_resolved_manifests;

    /// Compares one model's capabilities after manifest-rule enrichment versus the legacy oracle.
    /// 对比一个模型经清单规则补全与旧版 oracle 补全后的能力。
    fn assert_rule_equals_legacy(
        provider_id: &str,
        service_entry_id: &str,
        protocol_id: &str,
        model_id: &str,
        apply_legacy: impl FnOnce(&mut ConnectionModel),
    ) {
        let manifests = load_bundled_resolved_manifests();
        let manifest = manifests
            .get(provider_id)
            .unwrap_or_else(|| panic!("missing bundled manifest for `{provider_id}`"));

        let mut via_manifest = ConnectionModel {
            id: model_id.to_string(),
            display_name: model_id.to_string(),
            capabilities: ModelCapabilities::default(),
            capability_evidence: ModelCapabilityEvidence::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        };
        assert!(
            apply_manifest_model_rule(
                manifest,
                service_entry_id,
                None,
                protocol_id,
                &mut via_manifest
            ),
            "model `{model_id}` must match a `{provider_id}` manifest rule"
        );

        let mut via_legacy = ConnectionModel {
            id: model_id.to_string(),
            display_name: model_id.to_string(),
            capabilities: ModelCapabilities::default(),
            capability_evidence: ModelCapabilityEvidence::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        };
        apply_legacy(&mut via_legacy);

        let a = &via_manifest.capabilities;
        let b = &via_legacy.capabilities;
        assert_eq!(
            a.context_limit, b.context_limit,
            "context drift `{model_id}`"
        );
        assert_eq!(a.output_limit, b.output_limit, "output drift `{model_id}`");
        assert_eq!(
            a.input_resources, b.input_resources,
            "input resource drift `{model_id}`"
        );
        assert_eq!(
            a.output_resources, b.output_resources,
            "output resource drift `{model_id}`"
        );
        assert_eq!(a.reasoning, b.reasoning, "reasoning drift `{model_id}`");
        assert_eq!(
            a.reasoning_control, b.reasoning_control,
            "reasoning control drift `{model_id}`"
        );
        assert_eq!(
            a.reasoning_efforts, b.reasoning_efforts,
            "reasoning efforts drift `{model_id}`"
        );
        assert_eq!(a.embedding, b.embedding, "embedding drift `{model_id}`");
        assert_eq!(a.rerank, b.rerank, "rerank drift `{model_id}`");
        assert_eq!(
            a.provider_native_tools, b.provider_native_tools,
            "native tools drift `{model_id}`"
        );
        // Model-parameter identities differ by design: manifest rules share one definition per
        // rule while the legacy table emitted one definition per model. Compare every other field.
        // 模型参数身份存在设计差异：清单规则按规则共享一份定义，而旧版表按模型逐份输出。
        // 因此比较其余全部字段。
        assert_eq!(
            a.model_parameters.len(),
            b.model_parameters.len(),
            "model parameter count drift `{model_id}`"
        );
        for (left, right) in a.model_parameters.iter().zip(b.model_parameters.iter()) {
            assert_eq!(left.id, right.id, "parameter id drift `{model_id}`");
            assert_eq!(
                left.display_name, right.display_name,
                "param name `{model_id}`"
            );
            assert_eq!(
                left.description, right.description,
                "param desc `{model_id}`"
            );
            assert_eq!(left.schema, right.schema, "param schema `{model_id}`");
            assert_eq!(left.default, right.default, "param default `{model_id}`");
            // The legacy table emits the single calling protocol while the manifest declares the
            // full applicability set; both must cover the exact route under test.
            // 旧版表输出单个调用协议，而清单声明完整适用集合；两者都必须覆盖被测精确路由。
            assert_eq!(
                right.protocol_ids,
                vec![protocol_id.to_string()],
                "legacy param protocols `{model_id}`"
            );
            assert!(
                left.protocol_ids.iter().any(|id| id == protocol_id),
                "manifest param must cover `{protocol_id}` for `{model_id}`"
            );
            assert_eq!(
                left.dependent_required, right.dependent_required,
                "param deps `{model_id}`"
            );
            assert_eq!(
                left.conflicts, right.conflicts,
                "param conflicts `{model_id}`"
            );
        }
    }

    /// Builds the frozen code-owned Alibaba catalog used only as a migration oracle.
    /// 构建仅作为迁移对照使用的冻结代码持有阿里目录。
    ///
    /// # Parameters
    /// 参数
    /// `service_entry_id` and `protocol_id` identify the exact legacy product route.
    /// `service_entry_id` 与 `protocol_id` 标识精确旧版产品路由。
    ///
    /// # Returns
    /// 返回值
    /// The exact legacy catalog or an explicit unsupported-route failure.
    /// 返回精确旧版目录或明确的不支持路由失败。
    fn legacy_alibaba_static_catalog(
        service_entry_id: &str,
        protocol_id: &str,
    ) -> ConnectionModelCatalog {
        match service_entry_id {
            "token-plan-personal-cn"
            | "token-plan-personal-global"
            | "token-plan-team-cn"
            | "token-plan-team-global" => {
                build_alibaba_token_plan_catalog("migration", service_entry_id, protocol_id, 1)
            }
            "model-studio-cn" | "model-studio-singapore" => {
                build_alibaba_model_studio_catalog("migration", service_entry_id, protocol_id, 1)
            }
            unsupported => panic!("unexpected Alibaba migration entry `{unsupported}`"),
        }
        .expect("legacy Alibaba static catalog must build")
    }

    /// Builds the same route using only manifest inventory, rule, and protocol authority.
    /// 仅使用 Manifest 清单、规则与协议权威构建同一路由。
    ///
    /// # Parameters
    /// 参数
    /// `manifest`, `service_entry_id`, and `protocol_id` are the frozen compiled route facts.
    /// `manifest`、`service_entry_id` 与 `protocol_id` 是冻结编译路由事实。
    ///
    /// # Returns
    /// 返回值
    /// A catalog containing only identities authorized for the exact protocol.
    /// 返回仅包含精确协议已授权身份的目录。
    fn manifest_alibaba_static_catalog(
        manifest: &ResolvedProviderManifest,
        service_entry_id: &str,
        protocol_id: &str,
    ) -> ConnectionModelCatalog {
        // credential_profile_id selects the exact product credential used by production discovery.
        // credential_profile_id 选择生产发现使用的精确产品凭据。
        let credential_profile_id = match service_entry_id {
            "token-plan-personal-cn"
            | "token-plan-personal-global"
            | "token-plan-team-cn"
            | "token-plan-team-global" => "token-plan-api-key",
            "model-studio-cn" | "model-studio-singapore" => "api-key",
            unsupported => panic!("unexpected Alibaba migration entry `{unsupported}`"),
        };
        // declared_models is the sole identity source for ManifestStatic.
        // declared_models 是 ManifestStatic 的唯一身份来源。
        let declared_models = manifest
            .catalog_models
            .get(service_entry_id)
            .expect("manifest-static inventory must exist");
        // models contains only exact rule-authorized variants for this protocol.
        // models 仅包含此协议由精确规则授权的变体。
        let mut models = Vec::new();
        for declared in declared_models {
            // instance proves the model is covered for this service and credential.
            // instance 证明该模型覆盖此服务与凭据。
            let instance = manifest
                .rule_for_model(
                    service_entry_id,
                    Some(credential_profile_id),
                    declared.model_id.as_str(),
                )
                .expect("manifest-static identity must have a rule");
            if !instance
                .supported_protocols
                .iter()
                .any(|protocol| protocol == protocol_id)
            {
                continue;
            }
            // model begins without provider-neutral binary capability defaults.
            // model 以不含供应商无关二进制能力默认的状态开始。
            let mut model = ConnectionModel {
                id: declared.model_id.clone(),
                display_name: declared.display_name.clone(),
                capabilities: ModelCapabilities::default(),
                capability_evidence: ModelCapabilityEvidence::default(),
                protocol_variants: BTreeMap::new(),
                resolved_protocol_binding: None,
            };
            assert!(apply_manifest_model_rule(
                manifest,
                service_entry_id,
                Some(credential_profile_id),
                protocol_id,
                &mut model,
            ));
            models.push(model);
        }
        ConnectionModelCatalog {
            connection_id: String::from("migration"),
            models,
            generation: 1,
            page_count: 1,
            discovery_schema_revision: String::from("provider-manifest.static-models.v1"),
        }
    }

    /// Verifies all six Alibaba static products moved their executable facts into Manifest.
    /// 验证全部六个阿里静态产品已把可执行事实迁入 Manifest。
    #[test]
    fn alibaba_manifest_static_catalogs_equal_legacy_oracle() {
        // manifests provides the current fully compiled bundled authority.
        // manifests 提供当前完全编译的内置权威。
        let manifests = load_bundled_resolved_manifests();
        // manifest is the exact Alibaba snapshot under comparison.
        // manifest 是参与比较的精确阿里快照。
        let manifest = manifests
            .get("alibaba")
            .expect("bundled Alibaba manifest must exist");
        // service_protocols enumerates every production static route.
        // service_protocols 枚举每条生产静态路由。
        let service_protocols = [
            ("token-plan-personal-cn", OPENAI_CHAT_PROFILE_ID),
            ("token-plan-personal-cn", OPENAI_RESPONSES_PROFILE_ID),
            ("token-plan-personal-cn", ANTHROPIC_MESSAGES_PROFILE_ID),
            ("token-plan-personal-global", OPENAI_CHAT_PROFILE_ID),
            ("token-plan-personal-global", OPENAI_RESPONSES_PROFILE_ID),
            ("token-plan-personal-global", ANTHROPIC_MESSAGES_PROFILE_ID),
            ("token-plan-team-cn", OPENAI_CHAT_PROFILE_ID),
            ("token-plan-team-cn", OPENAI_RESPONSES_PROFILE_ID),
            ("token-plan-team-cn", ANTHROPIC_MESSAGES_PROFILE_ID),
            ("token-plan-team-global", OPENAI_CHAT_PROFILE_ID),
            ("token-plan-team-global", OPENAI_RESPONSES_PROFILE_ID),
            ("token-plan-team-global", ANTHROPIC_MESSAGES_PROFILE_ID),
            ("model-studio-cn", OPENAI_CHAT_PROFILE_ID),
            ("model-studio-cn", OPENAI_RESPONSES_PROFILE_ID),
            ("model-studio-cn", ANTHROPIC_MESSAGES_PROFILE_ID),
            ("model-studio-singapore", OPENAI_CHAT_PROFILE_ID),
            ("model-studio-singapore", OPENAI_RESPONSES_PROFILE_ID),
            ("model-studio-singapore", ANTHROPIC_MESSAGES_PROFILE_ID),
        ];
        for (service_entry_id, protocol_id) in service_protocols {
            // legacy is the frozen pre-migration product output.
            // legacy 是冻结的迁移前产品输出。
            let legacy = legacy_alibaba_static_catalog(service_entry_id, protocol_id);
            // via_manifest is produced without any common or provider-named capability seed.
            // via_manifest 在不使用公共或供应商命名能力种子的情况下生成。
            let via_manifest =
                manifest_alibaba_static_catalog(manifest, service_entry_id, protocol_id);
            // The legacy oracle covers migrated identities only; newly declared models are
            // validated by the manifest conformance suite and need no oracle-table edit.
            // 旧版对照只覆盖已迁移身份；新增模型由清单一致性套件校验，无须改动旧表。
            let manifest_models = via_manifest
                .models
                .iter()
                .map(|model| (model.id.as_str(), model))
                .collect::<BTreeMap<_, _>>();
            for expected in &legacy.models {
                let actual = manifest_models.get(expected.id.as_str()).unwrap_or_else(|| {
                    panic!(
                        "migrated model `{}` disappeared from `{service_entry_id}` / `{protocol_id}`",
                        expected.id
                    )
                });
                // Empty legacy operations identify embedding/rerank identities whose capabilities
                // were already manifest-owned before this static-catalog migration. Their dedicated
                // rule-oracle tests remain authoritative; this guard still checks membership/order.
                // 旧版操作为空表示向量/重排身份，其能力在本次静态目录迁移前已由 Manifest 持有。
                // 专用规则对照测试继续作为权威；本守卫仍检查其成员与顺序。
                if expected.capabilities.operations.is_empty() {
                    continue;
                }
                let actual_capabilities = &actual.capabilities;
                let expected_capabilities = &expected.capabilities;
                assert_eq!(
                    actual_capabilities.operations, expected_capabilities.operations,
                    "operation drift `{service_entry_id}` / `{protocol_id}` / `{}`",
                    actual.id
                );
                assert_eq!(
                    actual_capabilities.input_resources, expected_capabilities.input_resources,
                    "input resource drift `{service_entry_id}` / `{protocol_id}` / `{}`",
                    actual.id
                );
                assert_eq!(
                    actual_capabilities.output_resources, expected_capabilities.output_resources,
                    "output resource drift `{service_entry_id}` / `{protocol_id}` / `{}`",
                    actual.id
                );
                assert_eq!(
                    actual_capabilities.context_limit, expected_capabilities.context_limit,
                    "context drift `{service_entry_id}` / `{protocol_id}` / `{}`",
                    actual.id
                );
                assert_eq!(
                    actual_capabilities.output_limit, expected_capabilities.output_limit,
                    "output drift `{service_entry_id}` / `{protocol_id}` / `{}`",
                    actual.id
                );
                assert_eq!(
                    actual_capabilities.embedding, expected_capabilities.embedding,
                    "embedding drift `{service_entry_id}` / `{protocol_id}` / `{}`",
                    actual.id
                );
                assert_eq!(
                    actual_capabilities.rerank, expected_capabilities.rerank,
                    "rerank drift `{service_entry_id}` / `{protocol_id}` / `{}`",
                    actual.id
                );
                assert_eq!(
                    actual_capabilities.reasoning, expected_capabilities.reasoning,
                    "reasoning drift `{service_entry_id}` / `{protocol_id}` / `{}`",
                    actual.id
                );
                assert_eq!(
                    actual_capabilities.reasoning_control, expected_capabilities.reasoning_control,
                    "reasoning control drift `{service_entry_id}` / `{protocol_id}` / `{}`",
                    actual.id
                );
                assert_eq!(
                    actual_capabilities.reasoning_efforts, expected_capabilities.reasoning_efforts,
                    "reasoning effort drift `{service_entry_id}` / `{protocol_id}` / `{}`",
                    actual.id
                );
            }
        }
    }

    /// Verifies every migrated Kimi model rule equals the legacy enrich branch.
    /// 验证每个已迁移 Kimi 模型规则与旧版补全分支等价。
    #[test]
    fn kimi_rules_equal_legacy_enrichment() {
        for model_id in [
            "kimi-for-coding",
            "kimi-for-coding-highspeed",
            "k3-256k",
            "k3",
            "kimi-k3",
            "kimi-k2.7-code",
            "kimi-k2.7-code-highspeed",
            "kimi-k2.6",
            "kimi-k2.5",
            "moonshot-v1-8k",
            "moonshot-v1-32k",
            "moonshot-v1-128k",
            "moonshot-v1-8k-vision-preview",
            "moonshot-v1-32k-vision-preview",
            "moonshot-v1-128k-vision-preview",
        ] {
            assert_rule_equals_legacy(
                "kimi",
                "coding-plan",
                OPENAI_CHAT_PROFILE_ID,
                model_id,
                |model| enrich_kimi_model("coding-plan", OPENAI_CHAT_PROFILE_ID, model),
            );
        }
        // The Anthropic route narrows video resources on the coding-plan entry.
        // Anthropic 路由在 coding-plan 入口收窄视频资源。
        assert_rule_equals_legacy(
            "kimi",
            "coding-plan",
            ANTHROPIC_MESSAGES_PROFILE_ID,
            "kimi-for-coding",
            |model| enrich_kimi_model("coding-plan", ANTHROPIC_MESSAGES_PROFILE_ID, model),
        );
    }

    /// Verifies migrated Alibaba embedding and rerank rules equal the legacy enrich branch.
    /// 验证已迁移的阿里云向量与重排规则和旧版补全分支等价。
    ///
    /// Conversation rules now combine the old static-catalog facts with enrichment facts and are
    /// exhaustively covered by `alibaba_manifest_static_catalogs_equal_legacy_oracle` above.
    /// 会话规则现在合并旧静态目录事实与补全事实，并由上方
    /// `alibaba_manifest_static_catalogs_equal_legacy_oracle` 完整覆盖。
    #[test]
    fn alibaba_embedding_and_rerank_rules_equal_legacy_enrichment() {
        for model_id in [
            "text-embedding-v1",
            "text-embedding-v2",
            "text-embedding-v4",
            "text-embedding-v3",
            "qwen3.7-text-embedding",
            "qwen3-rerank",
        ] {
            assert_rule_equals_legacy(
                "alibaba",
                "model-studio-cn",
                OPENAI_CHAT_PROFILE_ID,
                model_id,
                |model| enrich_alibaba_model("model-studio-cn", model),
            );
        }
    }

    /// Verifies every migrated DeepSeek model rule equals the legacy enrich branch.
    /// 验证每个已迁移 DeepSeek 模型规则与旧版补全分支等价。
    #[test]
    fn deepseek_rules_equal_legacy_enrichment() {
        // v4-flash is retired on the official provider, so it has no manifest rule to compare.
        // v4-flash 已从官方下线，清单中没有可比较的规则。
        {
            let model_id = "deepseek-v4-pro";
            assert_rule_equals_legacy(
                "deepseek",
                "official-api",
                ANTHROPIC_MESSAGES_PROFILE_ID,
                model_id,
                |model| enrich_deepseek_model(ANTHROPIC_MESSAGES_PROFILE_ID, model),
            );
            assert_rule_equals_legacy(
                "deepseek",
                "official-api",
                OPENAI_CHAT_PROFILE_ID,
                model_id,
                |model| enrich_deepseek_model(OPENAI_CHAT_PROFILE_ID, model),
            );
        }
    }

    /// Verifies display names are owned by bundled manifest rules.
    /// 验证显示名称由内置 Manifest 规则持有。
    #[test]
    fn known_display_names_match_legacy() {
        let cases = [
            ("kimi", "coding-plan", "kimi-for-coding", "Kimi K2.7 Code"),
            ("kimi", "coding-plan", "k3-256k", "Kimi K3 256K"),
            ("kimi", "coding-plan", "k3", "Kimi K3"),
            ("kimi", "platform-cn", "kimi-k3", "Kimi K3"),
            (
                "kimi",
                "platform-cn",
                "moonshot-v1-8k-vision-preview",
                "Moonshot V1 8K Vision",
            ),
            (
                "deepseek",
                "official-api",
                "deepseek-v4-pro",
                "DeepSeek V4 Pro",
            ),
        ];
        let manifests = load_bundled_resolved_manifests();
        for (provider_id, entry_id, model_id, expected) in cases {
            let mut model = ConnectionModel {
                id: model_id.to_string(),
                display_name: String::new(),
                capabilities: ModelCapabilities::default(),
                capability_evidence: ModelCapabilityEvidence::default(),
                protocol_variants: BTreeMap::new(),
                resolved_protocol_binding: None,
            };
            let manifest = manifests
                .get(provider_id)
                .unwrap_or_else(|| panic!("missing bundled manifest for `{provider_id}`"));
            assert!(
                apply_manifest_model_rule(
                    manifest,
                    entry_id,
                    None,
                    OPENAI_CHAT_PROFILE_ID,
                    &mut model,
                ),
                "display rule must match `{model_id}`"
            );
            assert_eq!(model.display_name, expected, "display drift `{model_id}`");
        }
    }
}
