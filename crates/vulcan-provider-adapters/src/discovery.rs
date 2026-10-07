//! Strict credential-scoped model discovery.
//! 严格的凭据作用域模型发现。

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
use vulcan_llm_core::{
    ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
};

/// Stable public parameter identifier for sampling temperature.
/// 采样温度的稳定公共参数标识。
pub const SAMPLING_TEMPERATURE_PARAMETER_ID: &str = "sampling.temperature";

/// Stable provider-neutral parameter identifier for a direct reasoning-token budget.
/// 直接推理 Token 预算的稳定供应商无关参数标识。
pub const REASONING_MAX_TOKENS_PARAMETER_ID: &str = "reasoning.max_tokens";

/// MiniMax reasoning-split parameter identifier the bundled manifest declares, named for tests.
/// 内置清单声明的 MiniMax 思考拆分参数标识，供测试引用。
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
