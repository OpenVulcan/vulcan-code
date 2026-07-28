//! Strict credential-scoped model discovery.
//! 严格的凭据作用域模型发现。

use crate::{ConnectionModel, ConnectionModelCatalog};
use serde::Deserialize;
use std::collections::BTreeMap;
use vulcan_ai_contract::{
    AiOperation, CapabilitySupport, ModelCapabilities, ModelParameterDefault,
    ModelParameterDefinition, ModelParameterSchema, PromptCacheCapability, ReasoningEffort,
    ResourceKind,
};
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
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
        })
        .collect();
    Ok(ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
    })
}

/// Builds one complete connection-scoped MiniMax catalog from all fetched pages.
/// 从全部已获取分页构建一个完整的连接作用域 MiniMax 目录。
///
/// # Parameters
/// 参数
/// `connection_id` is the exact credential connection, `generation` is the refresh
/// generation, and `models` contains every page in provider order.
/// `connection_id` 是精确凭据连接，`generation` 是刷新代次，`models` 包含按供应商顺序排列的全部分页。
///
/// # Returns
/// 返回值
/// A process-memory-only model catalog.
/// 返回一个仅存在于进程内存中的模型目录。
pub(crate) fn build_minimax_model_catalog(
    connection_id: &str,
    generation: u64,
    models: Vec<ConnectionModel>,
) -> ConnectionModelCatalog {
    ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
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
            ConnectionModel {
                display_name: model.name.unwrap_or_else(|| model.id.clone()),
                id: model.id,
                capabilities,
            }
        })
        .collect();
    Ok(ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
    })
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
        .map(|model| ConnectionModel {
            display_name: model.id.clone(),
            id: model.id,
            capabilities: ModelCapabilities {
                operations: vec![AiOperation::ConversationRespond],
                evidence_revision: String::from("openai.models.v1"),
                ..ModelCapabilities::default()
            },
        })
        .collect();
    Ok(ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
    })
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
        })
        .collect();
    Ok(ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
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
pub(crate) fn parse_anthropic_model_page(body: &str) -> Result<AnthropicModelPage, String> {
    let response: AnthropicModelList = serde_json::from_str(body)
        .map_err(|error| format!("Anthropic model list is invalid: {error}"))?;
    let models = response
        .data
        .into_iter()
        .map(|model| ConnectionModel {
            id: model.id,
            display_name: model.display_name,
            capabilities: ModelCapabilities {
                operations: vec![AiOperation::ConversationRespond],
                prompt_cache: anthropic_prompt_cache(),
                evidence_revision: String::from("anthropic.models.v1"),
                ..ModelCapabilities::default()
            },
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
/// `connection_id`, `generation`, and `models` identify the complete in-memory snapshot.
/// `connection_id`、`generation` 与 `models` 标识完整内存快照。
///
/// # Returns
/// 返回值
/// One process-local model catalog.
/// 返回一个进程内模型目录。
pub(crate) fn build_anthropic_model_catalog(
    connection_id: &str,
    generation: u64,
    models: Vec<ConnectionModel>,
) -> ConnectionModelCatalog {
    ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
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
            ConnectionModel {
                id: model
                    .name
                    .strip_prefix("models/")
                    .unwrap_or(model.name.as_str())
                    .to_string(),
                display_name: model.display_name,
                capabilities: ModelCapabilities {
                    operations,
                    context_limit: model.input_token_limit,
                    output_limit: model.output_token_limit,
                    evidence_revision: String::from("gemini.models.v1beta"),
                    ..ModelCapabilities::default()
                },
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
/// `connection_id`, `generation`, and `models` identify the complete in-memory snapshot.
/// `connection_id`、`generation` 与 `models` 标识完整内存快照。
///
/// # Returns
/// 返回值
/// One process-local model catalog.
/// 返回一个进程内模型目录。
pub(crate) fn build_gemini_model_catalog(
    connection_id: &str,
    generation: u64,
    models: Vec<ConnectionModel>,
) -> ConnectionModelCatalog {
    ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
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
        .map(|(id, model)| ConnectionModel {
            display_name: model.display_name.unwrap_or_else(|| id.clone()),
            id,
            capabilities: ModelCapabilities {
                operations: vec![AiOperation::ConversationRespond],
                context_limit: model.max_tokens,
                output_limit: model.max_output_tokens,
                evidence_revision: String::from(
                    "google-code-assist.fetch-available-models.v1internal",
                ),
                ..ModelCapabilities::default()
            },
        })
        .collect();
    Ok(ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models,
        generation,
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
    ModelCapabilities {
        operations: vec![AiOperation::ConversationRespond],
        input_resources,
        output_resources: Vec::new(),
        tool_calling: if verified_model {
            CapabilitySupport::Native
        } else {
            CapabilitySupport::Unknown
        },
        parallel_tool_calling: CapabilitySupport::Unknown,
        reasoning,
        reasoning_efforts: Vec::new(),
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
        evidence_revision: String::from("minimax-compatible-text-api-2026-07-28"),
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
    let mut model_parameters = Vec::new();
    let reasoning_efforts = model
        .reasoning
        .as_ref()
        .map(|reasoning| {
            let mut efforts = reasoning.supported_efforts.as_ref().map_or_else(
                || {
                    vec![
                        ReasoningEffort::None,
                        ReasoningEffort::Minimal,
                        ReasoningEffort::Low,
                        ReasoningEffort::Medium,
                        ReasoningEffort::High,
                        ReasoningEffort::XHigh,
                        ReasoningEffort::Max,
                    ]
                },
                |efforts| normalized_reasoning_efforts(efforts.iter().map(String::as_str)),
            );
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
        operations: vec![AiOperation::ConversationRespond],
        input_resources,
        output_resources,
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
        reasoning_efforts,
        context_limit: model.context_length,
        output_limit: model
            .top_provider
            .as_ref()
            .and_then(|provider| provider.max_completion_tokens),
        prompt_cache: PromptCacheCapability::default(),
        model_parameters,
        evidence_revision: String::from("openrouter-models-api-2026-07"),
    }
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
#[serde(deny_unknown_fields)]
struct OpenAiModelList {
    /// Returned model records.
    /// 返回的模型记录。
    data: Vec<OpenAiModelEntry>,
    /// Documented object discriminator.
    /// 文档化对象判别值。
    #[serde(rename = "object")]
    _object: String,
}

/// Documented OpenAI model record fields used by Vulcan.
/// Vulcan 使用的文档化 OpenAI 模型记录字段。
#[derive(Debug, Deserialize)]
struct OpenAiModelEntry {
    /// Exact model identifier.
    /// 精确模型标识。
    id: String,
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
    /// Supported effort labels; null means all gateway effort labels are accepted.
    /// 支持的推理标签；null 表示接受全部网关推理标签。
    supported_efforts: Option<Vec<String>>,
    /// Whether the model accepts a direct reasoning token budget.
    /// 模型是否接受直接推理 Token 预算。
    #[serde(default)]
    supports_max_tokens: bool,
    /// Whether reasoning is mandatory for the model.
    /// 模型是否强制启用推理。
    #[serde(default)]
    mandatory: bool,
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
    /// Maximum completion tokens reported for the top endpoint.
    /// 首选端点报告的最大补全 Token。
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
            r#"{"data":[{"id":"vendor/model","name":"Model","context_length":8192,"supported_parameters":["tools","temperature"],"architecture":{"input_modalities":["text","image"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096},"reasoning":{"supported_efforts":["max","high","none"],"supports_max_tokens":true,"mandatory":true}}]}"#,
        )
        .expect("documented response should parse");
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

    /// Verifies OpenAI discovery preserves identity without inventing model capabilities.
    /// 验证 OpenAI 发现保留模型身份且不臆造模型能力。
    #[test]
    fn openai_discovery_remains_conservative() {
        let catalog = parse_openai_model_catalog(
            "connection",
            1,
            r#"{"object":"list","data":[{"id":"gpt-test","object":"model","created":1,"owned_by":"openai"}]}"#,
        )
        .expect("documented response should parse");
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
            "openai.models.v1"
        );
    }

    /// Verifies Anthropic discovery uses documented identity and pagination fields.
    /// 验证 Anthropic 发现使用文档化身份及分页字段。
    #[test]
    fn anthropic_discovery_parses_page_contract() {
        let page = parse_anthropic_model_page(
            r#"{"data":[{"type":"model","id":"claude-test","display_name":"Claude Test","created_at":"2026-01-01T00:00:00Z"}],"has_more":false,"first_id":"claude-test","last_id":"claude-test"}"#,
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
