//! Strict credential-scoped model discovery.
//! 严格的凭据作用域模型发现。

use crate::{
    ConnectionModel, ConnectionModelCatalog, ModelCapabilityEvidence,
    reasoning_catalog::{FixedReasoningProvider, fixed_reasoning_capabilities},
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{
    AiOperation, CapabilitySupport, DistanceMetric, EmbeddingCapabilities, EmbeddingEncoding,
    EmbeddingNormalization, EmbeddingOutputKind, EmbeddingTask, ModelCapabilities,
    ModelParameterDefault, ModelParameterDefinition, ModelParameterSchema, NativeToolKind,
    PromptCacheCapability, ReasoningControlMode, ReasoningEffort, RerankCapabilities,
    RerankScoreSemantics, RerankTruncation, ResourceKind,
};
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID, OPENAI_CHAT_PROFILE_ID,
    OPENAI_RESPONSES_PROFILE_ID,
};

/// Stable public parameter identifier for sampling temperature.
/// 采样温度的稳定公共参数标识。
pub const SAMPLING_TEMPERATURE_PARAMETER_ID: &str = "sampling.temperature";

/// Stable OpenRouter parameter identifier for a direct reasoning-token budget.
/// OpenRouter 直接推理 Token 预算的稳定参数标识。
pub const OPENROUTER_REASONING_MAX_TOKENS_PARAMETER_ID: &str = "reasoning.max_tokens";

/// Stable MiniMax parameter identifier for OpenAI-compatible reasoning separation.
/// MiniMax OpenAI 兼容推理分离的稳定参数标识。
pub const MINIMAX_REASONING_SPLIT_PARAMETER_ID: &str = "minimax.reasoning_split";

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
pub(crate) fn parse_minimax_model_page(body: &str) -> Result<MiniMaxModelPage, String> {
    let response: MiniMaxModelList = serde_json::from_str(body)
        .map_err(|error| format!("MiniMax model list is invalid: {error}"))?;
    let models = response
        .data
        .into_iter()
        .map(|model| {
            let capabilities =
                minimax_text_capabilities(model.id.as_str(), ANTHROPIC_MESSAGES_PROFILE_ID);
            ConnectionModel {
                id: model.id,
                display_name: model.display_name,
                capabilities,
                capability_evidence: ModelCapabilityEvidence::default(),
            }
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
pub fn parse_minimax_openai_model_catalog(
    connection_id: &str,
    generation: u64,
    body: &str,
) -> Result<ConnectionModelCatalog, String> {
    let response: OpenAiModelList = serde_json::from_str(body)
        .map_err(|error| format!("MiniMax OpenAI model list is invalid: {error}"))?;
    let models = response
        .data
        .into_iter()
        .map(|model| ConnectionModel {
            display_name: model.id.clone(),
            capabilities: minimax_text_capabilities(model.id.as_str(), OPENAI_CHAT_PROFILE_ID),
            id: model.id,
            capability_evidence: ModelCapabilityEvidence::default(),
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
    let response: OpenRouterModelList = serde_json::from_str(body)
        .map_err(|error| format!("OpenRouter model list is invalid: {error}"))?;
    let models = response
        .data
        .into_iter()
        .map(|model| {
            let capabilities = openrouter_reported_capabilities(&model);
            // display_name removes OpenRouter's catalog publisher prefix while preserving the exact routing id.
            // display_name 移除 OpenRouter 目录中的发布商前缀，同时保留精确路由 ID。
            let display_name =
                openrouter_model_display_name(model.name.as_deref(), model.id.as_str());
            ConnectionModel {
                display_name,
                id: model.id,
                capabilities,
                capability_evidence: ModelCapabilityEvidence::default(),
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
    let embedding_response: OpenRouterModelList = serde_json::from_str(embedding_body)
        .map_err(|error| format!("OpenRouter embedding model list is invalid: {error}"))?;
    let complete_response: OpenRouterModelList = serde_json::from_str(complete_body)
        .map_err(|error| format!("OpenRouter complete model list is invalid: {error}"))?;

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
                .iter()
                .any(|modality| modality == "rerank")
        });
        if is_rerank {
            merge_openrouter_specialized_model(
                &mut models,
                &model,
                openrouter_rerank_capabilities(&model),
            );
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
    let model = models
        .entry(source.id.clone())
        .or_insert_with(|| ConnectionModel {
            id: source.id.clone(),
            display_name: openrouter_model_display_name(source.name.as_deref(), source.id.as_str()),
            capabilities: ModelCapabilities::default(),
            capability_evidence: ModelCapabilityEvidence::default(),
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

/// Builds one verified Alibaba Coding Plan static model catalog.
/// 构建一个已验证的阿里云 Coding Plan 静态模型目录。
///
/// # Parameters
/// 参数
/// `connection_id` owns the in-memory catalog, `service_entry_id` selects the exact regional
/// product, and `generation` is the process-local refresh generation.
/// `connection_id` 拥有该内存目录，`service_entry_id` 选择精确区域产品，
/// `generation` 是进程内刷新代次。
///
/// # Returns
/// 返回值
/// The exact reviewed catalog or an explicit unsupported-entry error.
/// 返回精确的已审核目录，或明确的不支持入口错误。
pub fn build_alibaba_coding_plan_catalog(
    connection_id: &str,
    service_entry_id: &str,
    generation: u64,
) -> Result<ConnectionModelCatalog, String> {
    let evidence_revision = match service_entry_id {
        "coding-plan-cn" => "alibaba.coding-plan-cn.2026-07-23",
        "coding-plan-global" => "alibaba.coding-plan-global.2026-07-23",
        unsupported => {
            return Err(format!(
                "Alibaba static Coding Plan catalog is not registered for service entry `{unsupported}`."
            ));
        }
    };
    let definitions = [
        ("MiniMax-M2.5", "MiniMax M2.5", 196_608, &[][..]),
        ("glm-4.7", "GLM-4.7", 202_752, &[][..]),
        ("glm-5", "GLM-5", 202_752, &[][..]),
        (
            "kimi-k2.5",
            "Kimi K2.5",
            262_144,
            &[ResourceKind::Image, ResourceKind::Video][..],
        ),
        ("qwen3-coder-next", "Qwen3 Coder Next", 262_144, &[][..]),
        ("qwen3-coder-plus", "Qwen3 Coder Plus", 1_000_000, &[][..]),
        ("qwen3-max-2026-01-23", "Qwen3 Max 0123", 262_144, &[][..]),
        (
            "qwen3.5-plus",
            "Qwen3.5 Plus",
            1_000_000,
            &[ResourceKind::Image, ResourceKind::Video][..],
        ),
        (
            "qwen3.6-plus",
            "Qwen3.6 Plus",
            1_000_000,
            &[ResourceKind::Image, ResourceKind::Video][..],
        ),
        ("qwen3.7-plus", "Qwen3.7 Plus", 1_000_000, &[][..]),
    ];
    let models = definitions
        .into_iter()
        .map(|(model_id, display_name, context_limit, input_resources)| {
            let mut model = ConnectionModel {
                id: model_id.to_string(),
                display_name: display_name.to_string(),
                capabilities: ModelCapabilities {
                    operations: vec![AiOperation::ConversationRespond],
                    context_limit: Some(context_limit),
                    input_resources: input_resources.to_vec(),
                    evidence_revision: evidence_revision.to_string(),
                    ..ModelCapabilities::default()
                },
                capability_evidence: ModelCapabilityEvidence::default(),
            };
            enrich_alibaba_model(service_entry_id, &mut model);
            model
        })
        .collect();
    Ok(ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
        page_count: 1,
        discovery_schema_revision: String::from("alibaba.static-models.v1"),
    })
}

/// Describes one reviewed Alibaba conversation offering without provider discovery guesses.
/// 描述一项不含供应商发现猜测的已审核阿里云会话产品。
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
pub fn build_alibaba_token_plan_catalog(
    connection_id: &str,
    service_entry_id: &str,
    protocol_id: &str,
    generation: u64,
) -> Result<ConnectionModelCatalog, String> {
    const PERSONAL_CN: &[AlibabaStaticConversationModel] = &[
        alibaba_static_model(
            "qwen3.8-max-preview",
            "Qwen3.8 Max Preview",
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
        alibaba_static_model("glm-5.2", "GLM-5.2", 1_000_000, None, false, false),
        alibaba_static_model(
            "deepseek-v4-pro",
            "DeepSeek V4 Pro",
            1_000_000,
            None,
            false,
            false,
        ),
    ];
    const PERSONAL_GLOBAL: &[AlibabaStaticConversationModel] = &[
        alibaba_static_model(
            "qwen3.8-max-preview",
            "Qwen3.8 Max Preview",
            1_000_000,
            Some(131_072),
            true,
            true,
        ),
        alibaba_static_model("qwen3.7-plus", "Qwen3.7 Plus", 1_000_000, None, true, true),
        alibaba_static_model("qwen3.7-max", "Qwen3.7 Max", 1_000_000, None, false, false),
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
            "qwen3.8-max-preview",
            "Qwen3.8 Max Preview",
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
    for (id, display_name) in [
        ("text-embedding-v4", "Text Embedding V4"),
        ("text-embedding-v3", "Text Embedding V3"),
        ("qwen3-rerank", "Qwen3 Rerank"),
    ] {
        let mut model = ConnectionModel {
            id: id.to_string(),
            display_name: display_name.to_string(),
            capabilities: ModelCapabilities::default(),
            capability_evidence: ModelCapabilityEvidence::default(),
        };
        enrich_alibaba_model(service_entry_id, &mut model);
        catalog.models.push(model);
    }
    if service_entry_id == "model-studio-cn" {
        let mut model = ConnectionModel {
            id: String::from("qwen3.7-text-embedding"),
            display_name: String::from("Qwen3.7 Text Embedding"),
            capabilities: ModelCapabilities::default(),
            capability_evidence: ModelCapabilityEvidence::default(),
        };
        enrich_alibaba_model(service_entry_id, &mut model);
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
                    "qwen3.8-max-preview" | "qwen3.7-max" | "qwen3.7-plus"
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
            };
            if responses_harness {
                model.capabilities.tool_calling = CapabilitySupport::Unsupported;
                model.capabilities.parallel_tool_calling = CapabilitySupport::Unsupported;
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
    let response: OpenAiModelList = serde_json::from_str(body)
        .map_err(|error| format!("OpenAI model list is invalid: {error}"))?;
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

/// Applies code-owned capabilities to models returned by one compatible provider.
/// 为某个兼容供应商返回的模型应用代码持有的能力契约。
///
/// # Parameters
/// 参数
/// `provider_id`, `service_entry_id`, and `protocol_id` identify the exact route;
/// `catalog` is the already parsed credential-visible model list.
/// `provider_id`、`service_entry_id` 与 `protocol_id` 标识精确路由；
/// `catalog` 是已解析且对当前凭据可见的模型列表。
pub fn enrich_compatible_model_catalog(
    provider_id: &str,
    service_entry_id: &str,
    protocol_id: &str,
    catalog: &mut ConnectionModelCatalog,
) {
    for model in &mut catalog.models {
        if !provider_id.starts_with("custom:") {
            crate::apply_common_model_capability_defaults(
                model.id.as_str(),
                &mut model.capabilities,
            );
        }
        match provider_id {
            "openai" => enrich_openai_model(model),
            "kimi" => enrich_kimi_model(service_entry_id, protocol_id, model),
            "deepseek" => enrich_deepseek_model(protocol_id, model),
            "alibaba" => enrich_alibaba_model(service_entry_id, model),
            _ => {}
        }
    }
}

/// Applies official OpenAI reasoning metadata after provider identity is known.
/// 在供应商身份已知后应用 OpenAI 官方思考元数据。
///
/// # Parameters
/// 参数
/// `model` is one model parsed from the official OpenAI catalog.
/// `model` 是从 OpenAI 官方目录解析出的一个模型。
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
            vec![ResourceKind::Image],
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
    model.capabilities.tool_calling = CapabilitySupport::Native;
    model.capabilities.parallel_tool_calling = CapabilitySupport::Native;
    model.capabilities.reasoning = reasoning;
    model.capabilities.reasoning_control = reasoning_control;
    model.capabilities.reasoning_efforts = reasoning_efforts;
    model.capabilities.evidence_revision = format!("model-core.kimi.2026-07-29.{protocol_id}");
}

/// Resolves whether one known Kimi Coding Plan model remains exposed for an observed membership.
/// 根据已观测会员等级解析一个已知 Kimi Coding Plan 模型是否继续暴露。
///
/// # Parameters
/// 参数
/// `plan_name` is the normalized membership name from the usage endpoint; `model_id` is the
/// exact provider model identifier.
/// `plan_name` 是用量端点返回的规范化会员名称；`model_id` 是供应商精确模型标识。
///
/// # Returns
/// 返回值
/// `true` when the model is allowed by the requested product matrix, or when no confirmed plan is
/// available. Unknown model identifiers remain visible until the provider publishes a closed rule.
/// 当模型符合产品矩阵，或尚未获得已确认套餐时返回 `true`。在供应商发布封闭规则前，未知模型标识继续保留。
pub(crate) fn kimi_coding_plan_model_is_available(plan_name: Option<&str>, model_id: &str) -> bool {
    let Some(plan_name) = plan_name.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    match model_id {
        "k3" => matches!(plan_name, "Allegretto" | "Allegro"),
        "k3-256k" => matches!(plan_name, "Moderato" | "Allegretto" | "Allegro"),
        "kimi-for-coding" => matches!(plan_name, "Andante" | "Moderato" | "Allegretto" | "Allegro"),
        "kimi-for-coding-highspeed" => matches!(plan_name, "Allegretto" | "Allegro"),
        _ => true,
    }
}

/// Applies the exact current DeepSeek official text-model capability contract.
/// 应用当前 DeepSeek 官方文本模型的精确能力契约。
///
/// # Parameters
/// 参数
/// `protocol_id` identifies the exact compatible interface and `model` is mutated only when known.
/// `protocol_id` 标识精确兼容接口，`model` 仅在身份已知时原位更新。
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
    model.capabilities.tool_calling = CapabilitySupport::Native;
    model.capabilities.parallel_tool_calling = CapabilitySupport::Native;
    model.capabilities.reasoning = CapabilitySupport::Native;
    model.capabilities.reasoning_control = ReasoningControlMode::Toggle;
    model.capabilities.reasoning_efforts = vec![
        ReasoningEffort::None,
        ReasoningEffort::High,
        ReasoningEffort::Max,
    ];
    model
        .capabilities
        .model_parameters
        .extend(deepseek_model_parameters(model.id.as_str(), protocol_id));
    model.capabilities.evidence_revision = String::from("deepseek.api-docs.2026-07-30");
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
        default: ModelParameterDefault::ProviderDefault,
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
fn enrich_alibaba_model(service_entry_id: &str, model: &mut ConnectionModel) {
    if matches!(
        service_entry_id,
        "model-studio-cn" | "model-studio-singapore"
    ) {
        match model.id.as_str() {
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
        | "deepseek-v4-flash"
        | "deepseek-v3.2"
        | "kimi-k2.5"
        | "kimi-k2.6" => (
            ReasoningControlMode::Toggle,
            vec![ReasoningEffort::None, ReasoningEffort::Adaptive],
        ),
        "qwen3.8-max-preview"
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
    model.capabilities.tool_calling = CapabilitySupport::Native;
    model.capabilities.parallel_tool_calling = CapabilitySupport::Unknown;
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
    let response = serde_json::from_str::<OpenAiCodexModelList>(body)
        .map_err(|error| format!("OpenAI Codex model list is invalid: {error}"))?;
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
    let response: AnthropicModelList = serde_json::from_str(body)
        .map_err(|error| format!("Anthropic model list is invalid: {error}"))?;
    let models = response
        .data
        .into_iter()
        .map(|model| {
            let (reasoning_support, reasoning_efforts, evidence_revision) = if official_anthropic {
                let reasoning = fixed_reasoning_capabilities(
                    FixedReasoningProvider::Anthropic,
                    model.id.as_str(),
                );
                (
                    reasoning.support,
                    reasoning.efforts,
                    if reasoning.support == CapabilitySupport::Unknown {
                        String::from("anthropic.models.v1")
                    } else {
                        reasoning.evidence_revision.to_string()
                    },
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
    let response: GeminiModelList = serde_json::from_str(body)
        .map_err(|error| format!("Gemini model list is invalid: {error}"))?;
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
            let reasoning =
                fixed_reasoning_capabilities(FixedReasoningProvider::Google, model_id.as_str());
            ConnectionModel {
                id: model_id,
                display_name: model.display_name,
                capabilities: ModelCapabilities {
                    operations,
                    context_limit: model.input_token_limit,
                    output_limit: model.output_token_limit,
                    reasoning: reasoning.support,
                    reasoning_efforts: reasoning.efforts,
                    evidence_revision: if reasoning.support == CapabilitySupport::Unknown {
                        String::from("gemini.models.v1beta")
                    } else {
                        reasoning.evidence_revision.to_string()
                    },
                    ..ModelCapabilities::default()
                },
                capability_evidence: ModelCapabilityEvidence::default(),
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
    let response = serde_json::from_str::<GoogleCodeAssistModelList>(body)
        .map_err(|error| format!("Google Code Assist model list is invalid: {error}"))?;
    let models = response
        .models
        .into_iter()
        .map(|(id, model)| {
            let reasoning =
                fixed_reasoning_capabilities(FixedReasoningProvider::Google, id.as_str());
            ConnectionModel {
                display_name: model.display_name.unwrap_or_else(|| id.clone()),
                id,
                capabilities: ModelCapabilities {
                    operations: vec![AiOperation::ConversationRespond],
                    context_limit: model.max_tokens,
                    output_limit: model.max_output_tokens,
                    reasoning: reasoning.support,
                    reasoning_efforts: reasoning.efforts,
                    evidence_revision: if reasoning.support == CapabilitySupport::Unknown {
                        String::from("google-code-assist.fetch-available-models.v1internal")
                    } else {
                        reasoning.evidence_revision.to_string()
                    },
                    ..ModelCapabilities::default()
                },
                capability_evidence: ModelCapabilityEvidence::default(),
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
fn minimax_text_capabilities(model_id: &str, protocol_id: &str) -> ModelCapabilities {
    let model_family = minimax_model_family(model_id);
    let reasoning_contract =
        fixed_reasoning_capabilities(FixedReasoningProvider::MiniMax, model_id);
    let (input_resources, reasoning, context_limit, prompt_cache) = match model_family {
        MiniMaxTextModelFamily::M3 => (
            Vec::new(),
            CapabilitySupport::Native,
            Some(1_000_000),
            provider_default_prompt_cache(
                "MiniMax M3 caching follows the provider default for this compatible route.",
            ),
        ),
        MiniMaxTextModelFamily::M2 => (
            Vec::new(),
            CapabilitySupport::Native,
            Some(204_800),
            minimax_m2_prompt_cache(protocol_id),
        ),
        MiniMaxTextModelFamily::Unknown => (
            Vec::new(),
            CapabilitySupport::Unknown,
            None,
            provider_default_prompt_cache(
                "This newly discovered MiniMax model has no verified explicit cache contract.",
            ),
        ),
    };
    let verified_model = model_family != MiniMaxTextModelFamily::Unknown;
    let reasoning_support = if reasoning_contract.support == CapabilitySupport::Unknown {
        reasoning
    } else {
        reasoning_contract.support
    };
    let reasoning_control =
        reasoning_control_mode(reasoning_support, reasoning_contract.efforts.as_slice());
    ModelCapabilities {
        operations: vec![AiOperation::ConversationRespond],
        input_resources,
        output_resources: Vec::new(),
        embedding: None,
        rerank: None,
        tool_calling: if verified_model {
            CapabilitySupport::Native
        } else {
            CapabilitySupport::Unknown
        },
        parallel_tool_calling: CapabilitySupport::Unknown,
        reasoning: reasoning_support,
        provider_native_tools: Vec::new(),
        reasoning_control,
        reasoning_efforts: reasoning_contract.efforts,
        context_limit,
        // MiniMax documents a combined context limit, not a separate output maximum.
        // MiniMax 文档给出的是输入输出合计上下文限制，而不是独立输出上限。
        output_limit: None,
        prompt_cache,
        model_parameters: if verified_model {
            minimax_model_parameters(model_id, protocol_id)
        } else {
            Vec::new()
        },
        evidence_revision: reasoning_contract.evidence_revision.to_string(),
    }
}

/// Identifies the closed MiniMax text-model families with verified capability differences.
/// 标识具有已验证能力差异的封闭 MiniMax 文本模型族。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MiniMaxTextModelFamily {
    /// MiniMax M3 multimodal family.
    /// MiniMax M3 多模态模型族。
    M3,
    /// MiniMax M2 text-only family.
    /// MiniMax M2 纯文本模型族。
    M2,
    /// Newly discovered identity without a verified local capability contract.
    /// 尚无本地已验证能力契约的新发现身份。
    Unknown,
}

/// Resolves one exact MiniMax model identity into its verified family.
/// 把一个精确 MiniMax 模型身份解析为已验证模型族。
fn minimax_model_family(model_id: &str) -> MiniMaxTextModelFamily {
    match model_id {
        "MiniMax-M3" => MiniMaxTextModelFamily::M3,
        "MiniMax-M2"
        | "MiniMax-M2.1"
        | "MiniMax-M2.1-highspeed"
        | "MiniMax-M2.5"
        | "MiniMax-M2.5-highspeed"
        | "MiniMax-M2.7"
        | "MiniMax-M2.7-highspeed" => MiniMaxTextModelFamily::M2,
        _ => MiniMaxTextModelFamily::Unknown,
    }
}

/// Builds the verified M2 explicit prompt-cache capability.
/// 构建已验证的 M2 显式提示词缓存能力。
fn minimax_m2_prompt_cache(protocol_id: &str) -> PromptCacheCapability {
    if protocol_id == OPENAI_CHAT_PROFILE_ID {
        return provider_default_prompt_cache(
            "MiniMax OpenAI-compatible prompt caching is automatic and does not accept host-injected cache markers.",
        );
    }
    PromptCacheCapability {
        provider_default_behavior: String::from(
            "MiniMax automatic caching is used without host-injected cache controls.",
        ),
        supports_managed_explicit: true,
        supported_ttl_options: vec![vulcan_ai_contract::PromptCacheTtlOption {
            id: String::from("5m"),
            display_name: String::from("5 minutes"),
            description: Some(String::from(
                "MiniMax explicit Anthropic-compatible cache lifetime.",
            )),
        }],
        profile_default_ttl_option_id: Some(String::from("5m")),
        minimum_cacheable_tokens: Some(512),
        maximum_breakpoints: Some(4),
    }
}

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
fn provider_default_prompt_cache(description: &str) -> PromptCacheCapability {
    PromptCacheCapability {
        provider_default_behavior: description.to_string(),
        supports_managed_explicit: false,
        supported_ttl_options: Vec::new(),
        profile_default_ttl_option_id: None,
        minimum_cacheable_tokens: None,
        maximum_breakpoints: None,
    }
}

/// Builds verified MiniMax model parameters for one exact compatible protocol.
/// 为一个精确兼容协议构建已验证的 MiniMax 模型参数。
///
/// # Parameters
/// 参数
/// `model_id` is the exact discovered model and `protocol_id` is the selected route.
/// `model_id` 是精确发现的模型，`protocol_id` 是所选路由。
///
/// # Returns
/// 返回值
/// Closed parameter declarations valid for the selected protocol.
/// 返回适用于所选协议的封闭参数声明。
fn minimax_model_parameters(model_id: &str, protocol_id: &str) -> Vec<ModelParameterDefinition> {
    let mut parameters = vec![ModelParameterDefinition {
        id: String::from(SAMPLING_TEMPERATURE_PARAMETER_ID),
        display_name: String::from("Temperature"),
        description: String::from(
            "Controls MiniMax sampling randomness; the verified compatible-API range is greater than 0 through 1.",
        ),
        schema: ModelParameterSchema::Decimal {
            minimum: None,
            maximum: Some(String::from("1")),
            exclusive_minimum: Some(String::from("0")),
            exclusive_maximum: None,
        },
        default: ModelParameterDefault::ProviderDefault,
        protocol_ids: vec![protocol_id.to_string()],
        model_ids: vec![model_id.to_string()],
        dependent_required: Vec::new(),
        conflicts: Vec::new(),
        evidence_revision: String::from("minimax-compatible-text-api-2026-07-28"),
    }];
    if protocol_id == OPENAI_CHAT_PROFILE_ID {
        parameters.push(ModelParameterDefinition {
            id: String::from(crate::MINIMAX_REASONING_SPLIT_PARAMETER_ID),
            display_name: String::from("Separate reasoning"),
            description: String::from(
                "Returns MiniMax interleaved thinking in the OpenAI-compatible reasoning_details field.",
            ),
            schema: ModelParameterSchema::Boolean,
            default: ModelParameterDefault::ExplicitValue {
                value: vulcan_ai_contract::ModelParameterValue::Boolean(true),
            },
            protocol_ids: vec![String::from(OPENAI_CHAT_PROFILE_ID)],
            model_ids: vec![model_id.to_string()],
            dependent_required: Vec::new(),
            conflicts: Vec::new(),
            evidence_revision: String::from("minimax-openai-reasoning-split-2026-07-28"),
        });
    }
    parameters
}

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
        .map(|architecture| resource_kinds(architecture.input_modalities.as_slice()))
        .unwrap_or_default();
    let output_resources = model
        .architecture
        .as_ref()
        .map(|architecture| resource_kinds(architecture.output_modalities.as_slice()))
        .unwrap_or_default();
    // supports_text_conversation requires both sides of the exact OpenRouter architecture contract.
    // supports_text_conversation 要求 OpenRouter 精确架构契约的输入与输出两侧都声明文本。
    let supports_text_conversation = model.architecture.as_ref().is_some_and(|architecture| {
        architecture
            .input_modalities
            .iter()
            .any(|modality| modality == "text")
            && architecture
                .output_modalities
                .iter()
                .any(|modality| modality == "text")
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
            default: ModelParameterDefault::ProviderDefault,
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
            id: String::from(OPENROUTER_REASONING_MAX_TOKENS_PARAMETER_ID),
            display_name: String::from("Reasoning token budget"),
            description: String::from(
                "Sets OpenRouter reasoning.max_tokens when the selected model reports direct reasoning-budget support.",
            ),
            schema: ModelParameterSchema::Integer {
                minimum: Some(1),
                maximum: None,
                step: Some(1),
            },
            default: ModelParameterDefault::ProviderDefault,
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
    ModelCapabilities {
        operations: supports_text_conversation
            .then_some(AiOperation::ConversationRespond)
            .into_iter()
            .collect(),
        input_resources,
        output_resources,
        embedding: None,
        rerank: None,
        tool_calling: if supports("tools") {
            CapabilitySupport::Native
        } else {
            CapabilitySupport::Unsupported
        },
        parallel_tool_calling: if supports("parallel_tool_calls") {
            CapabilitySupport::Native
        } else {
            CapabilitySupport::Unsupported
        },
        reasoning: if model.reasoning.is_some() || supports("reasoning") {
            CapabilitySupport::Native
        } else {
            CapabilitySupport::Unsupported
        },
        reasoning_control: reasoning_control_mode(
            if model.reasoning.is_some() || supports("reasoning") {
                CapabilitySupport::Native
            } else {
                CapabilitySupport::Unsupported
            },
            reasoning_efforts.as_slice(),
        ),
        reasoning_efforts,
        provider_native_tools: Vec::new(),
        context_limit,
        output_limit: openrouter_effective_output_limit(model, context_limit),
        prompt_cache: PromptCacheCapability::default(),
        model_parameters,
        evidence_revision: String::from("openrouter-models-api-2026-07"),
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
        .and_then(|provider| provider.context_length);
    match (model.context_length, provider_context_limit) {
        (Some(model_limit), Some(provider_limit)) => Some(model_limit.min(provider_limit)),
        (Some(model_limit), None) => Some(model_limit),
        (None, Some(provider_limit)) => Some(provider_limit),
        (None, None) => None,
    }
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
        .and_then(|provider| provider.max_completion_tokens)?;
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
    input_modalities: Vec<String>,
    /// Reported output modality identifiers.
    /// 报告的输出模态标识。
    #[serde(default)]
    output_modalities: Vec<String>,
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
        enrich_compatible_model_catalog(
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
            enrich_compatible_model_catalog("kimi", "coding-plan", protocol_id, &mut catalog);

            let k3 = catalog.models.first().expect("K3");
            assert_eq!(k3.capabilities.input_resources, expected_resources);
        }
    }

    /// Verifies the product-level plan filter deliberately reserves one-million-token K3 for Allegretto and above.
    /// 验证产品级套餐过滤刻意将百万上下文 K3 保留给 Allegretto 及以上等级。
    #[test]
    fn kimi_coding_plan_model_access_matches_product_matrix() {
        assert!(kimi_coding_plan_model_is_available(
            Some("Andante"),
            "kimi-for-coding"
        ));
        assert!(!kimi_coding_plan_model_is_available(
            Some("Andante"),
            "k3-256k"
        ));
        assert!(kimi_coding_plan_model_is_available(
            Some("Moderato"),
            "k3-256k"
        ));
        assert!(!kimi_coding_plan_model_is_available(Some("Moderato"), "k3"));
        assert!(kimi_coding_plan_model_is_available(
            Some("Allegretto"),
            "k3"
        ));
        assert!(kimi_coding_plan_model_is_available(
            Some("Allegretto"),
            "kimi-for-coding-highspeed"
        ));
        assert!(kimi_coding_plan_model_is_available(None, "k3"));
    }

    /// Verifies Alibaba Coding Plan static catalogs retain exact regional identity and limits.
    /// 验证阿里云 Coding Plan 静态目录保留精确区域身份与限制。
    #[test]
    fn alibaba_coding_plan_static_catalog_is_region_scoped() {
        let catalog = build_alibaba_coding_plan_catalog("connection", "coding-plan-cn", 7)
            .expect("registered Coding Plan entry must build");
        assert_eq!(catalog.connection_id, "connection");
        assert_eq!(catalog.generation, 7);
        assert_eq!(catalog.models.len(), 10);
        let qwen = catalog
            .models
            .iter()
            .find(|model| model.id == "qwen3.7-plus")
            .expect("Qwen 3.7 Plus");
        assert_eq!(qwen.capabilities.context_limit, Some(1_000_000));
        assert_eq!(
            qwen.capabilities.evidence_revision,
            "model-core.alibaba.coding-plan-cn.2026-07-30"
        );
        assert!(build_alibaba_coding_plan_catalog("connection", "model-studio-cn", 8).is_err());
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
        assert_eq!(catalog.models.len(), 7);
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
        enrich_compatible_model_catalog(
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
        enrich_compatible_model_catalog(
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
        let catalog = parse_minimax_model_catalog(
            "connection",
            1,
            r#"{"data":[{"id":"MiniMax-M3","display_name":"MiniMax M3"},{"id":"MiniMax-M2.7","display_name":"MiniMax M2.7"},{"id":"MiniMax-Future","display_name":"Future"}],"has_more":false,"last_id":"MiniMax-Future"}"#,
        )
        .expect("documented identities should parse");
        let m3 = &catalog.models[0].capabilities;
        assert_eq!(m3.context_limit, Some(1_000_000));
        assert!(m3.input_resources.is_empty());
        assert!(!m3.prompt_cache.supports_managed_explicit);
        assert_eq!(m3.model_parameters[0].model_ids, vec!["MiniMax-M3"]);

        let m2 = &catalog.models[1].capabilities;
        assert_eq!(m2.context_limit, Some(204_800));
        assert!(m2.input_resources.is_empty());
        assert!(m2.prompt_cache.supports_managed_explicit);
        assert_eq!(m2.prompt_cache.minimum_cacheable_tokens, Some(512));

        let unknown = &catalog.models[2].capabilities;
        assert_eq!(unknown.context_limit, None);
        assert_eq!(unknown.tool_calling, CapabilitySupport::Unknown);
        assert!(unknown.model_parameters.is_empty());
    }

    /// Verifies MiniMax OpenAI discovery registers only its exact compatible-route parameters.
    /// 验证 MiniMax OpenAI 发现仅注册其精确兼容路由参数。
    #[test]
    fn minimax_openai_discovery_is_protocol_scoped() {
        let catalog = parse_minimax_openai_model_catalog(
            "connection",
            1,
            r#"{"object":"list","data":[{"id":"MiniMax-M2.7","object":"model","created":1,"owned_by":"minimax"}]}"#,
        )
        .expect("documented OpenAI-compatible response should parse");
        let capabilities = &catalog.models[0].capabilities;
        assert!(!capabilities.prompt_cache.supports_managed_explicit);
        assert!(capabilities.model_parameters.iter().all(|parameter| {
            parameter.protocol_ids == vec![String::from(OPENAI_CHAT_PROFILE_ID)]
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
        assert_eq!(
            catalog.models[0].capabilities.tool_calling,
            CapabilitySupport::Native
        );
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
                .any(|parameter| parameter.id == OPENROUTER_REASONING_MAX_TOKENS_PARAMETER_ID)
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

        assert_eq!(
            direct.models[0].capabilities.reasoning_efforts,
            vec![ReasoningEffort::None, ReasoningEffort::Adaptive]
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
            catalog.models[0].capabilities.tool_calling,
            CapabilitySupport::Unknown
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
        enrich_compatible_model_catalog(
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
        enrich_compatible_model_catalog(
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
                ReasoningEffort::Max,
            ]
        );
        assert!(capabilities.provider_native_tools.is_empty());
        let temperature = capabilities
            .model_parameters
            .iter()
            .find(|parameter| parameter.id == SAMPLING_TEMPERATURE_PARAMETER_ID)
            .expect("DeepSeek Chat should expose documented temperature control");
        assert_eq!(temperature.default, ModelParameterDefault::ProviderDefault);
        assert_eq!(
            temperature.protocol_ids,
            vec![String::from(OPENAI_CHAT_PROFILE_ID)]
        );

        for protocol_id in [ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID] {
            let mut protocol_catalog = catalog.clone();
            enrich_compatible_model_catalog(
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
}
