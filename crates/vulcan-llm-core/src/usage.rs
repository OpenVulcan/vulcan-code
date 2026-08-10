//! Profile-driven provider usage extraction and deterministic cost calculation.
//! 由 Profile 驱动的供应商用量提取与确定性成本计算。

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use vulcan_ai_contract::{
    RequestUsage, UsageBillingDetail, UsageUpdateKind, UsageValue, UsageValueSource,
    derive_uncached_input, derive_usage_sum,
};

use crate::LlmCoreError;

/// Identifies the raw transport record consumed by one usage profile.
/// 标识一个用量 Profile 所消费的原始传输记录。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageRecordKind {
    /// Complete non-streaming JSON response body.
    /// 完整的非流式 JSON 响应正文。
    JsonBody,
    /// One complete server-sent event payload.
    /// 一条完整的 SSE 事件负载。
    SseEvent,
    /// One complete Responses WebSocket frame.
    /// 一条完整的 Responses WebSocket 帧。
    WebSocketFrame,
}

/// Identifies a normalized field extracted from provider evidence.
/// 标识从供应商证据中提取的归一化字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageField {
    /// Total provider-reported input tokens.
    /// 供应商报告的输入 Token 总量。
    InputTokens,
    /// Provider-reported uncached input tokens.
    /// 供应商报告的非缓存输入 Token。
    UncachedInputTokens,
    /// Provider-reported output tokens.
    /// 供应商报告的输出 Token。
    OutputTokens,
    /// Provider-reported reasoning output tokens.
    /// 供应商报告的推理输出 Token。
    ReasoningTokens,
    /// Provider-reported cache-read tokens.
    /// 供应商报告的缓存读取 Token。
    CacheReadTokens,
    /// Provider-reported cache-write tokens.
    /// 供应商报告的缓存写入 Token。
    CacheWriteTokens,
    /// Provider-reported five-minute cache-write tokens.
    /// 供应商报告的五分钟缓存写入 Token。
    CacheWriteFiveMinuteTokens,
    /// Provider-reported one-hour cache-write tokens.
    /// 供应商报告的一小时缓存写入 Token。
    CacheWriteOneHourTokens,
    /// Provider-reported audio input tokens.
    /// 供应商报告的音频输入 Token。
    AudioInputTokens,
    /// Provider-reported audio output tokens.
    /// 供应商报告的音频输出 Token。
    AudioOutputTokens,
    /// Provider-reported total tokens.
    /// 供应商报告的 Token 合计。
    TotalTokens,
    /// Provider-reported decimal request cost.
    /// 供应商报告的请求十进制成本。
    ReportedCost,
    /// Provider-reported decimal request cost denominated in United States dollars.
    /// 供应商报告的以美元计价的请求十进制成本。
    ReportedCostUsd,
    /// Provider-returned model identity.
    /// 供应商返回的模型标识。
    ResponseModel,
    /// Provider-returned response identity.
    /// 供应商返回的响应标识。
    ResponseId,
    /// Provider-reported billing partitions retained outside normalized totals.
    /// 保留在规范化合计之外的供应商计费分区。
    BillingDetails,
}

/// Declares the exact JSON representation accepted by one field binding.
/// 声明一个字段绑定可接受的精确 JSON 表示形式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageValueEncoding {
    /// Requires an unsigned JSON integer.
    /// 要求无符号 JSON 整数。
    UnsignedInteger,
    /// Requires a JSON string containing an unsigned integer.
    /// 要求包含无符号整数的 JSON 字符串。
    UnsignedIntegerString,
    /// Accepts a JSON number or decimal string without converting through floating point.
    /// 接受 JSON 数字或十进制字符串，且不经过浮点转换。
    Decimal,
    /// Requires a JSON string.
    /// 要求 JSON 字符串。
    String,
    /// Requires the documented Alibaba Responses billing-detail array.
    /// 要求文档化的阿里云 Responses 计费明细数组。
    AlibabaResponsesBillingDetails,
}

/// Binds one normalized field to explicit provider JSON Pointer aliases.
/// 将一个归一化字段绑定到显式供应商 JSON Pointer 别名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageFieldBinding {
    /// Normalized destination field.
    /// 归一化目标字段。
    pub field: UsageField,
    /// Ordered aliases explicitly registered by this profile.
    /// 当前 Profile 显式注册的有序别名。
    pub pointers: Vec<String>,
    /// Exact JSON representation accepted at every alias.
    /// 每个别名位置可接受的精确 JSON 表示。
    pub encoding: UsageValueEncoding,
}

/// Declares one provider-approved derivation between reported dimensions.
/// 声明一个供应商认可的已报告维度推导关系。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageDerivation {
    /// Copies provider-reported input tokens into the uncached-input dimension.
    /// 将供应商报告的输入 Token 复制到非缓存输入维度。
    UncachedInputFromInput,
    /// Derives uncached input as input minus cache read.
    /// 将非缓存输入推导为输入总量减缓存读取量。
    UncachedInputFromInputMinusCacheRead,
    /// Derives cache write as the sum of five-minute and one-hour writes.
    /// 将缓存写入推导为五分钟与一小时写入之和。
    CacheWriteFromDurations,
    /// Derives standard total input from disjoint uncached, cache-read, and cache-write dimensions.
    /// 根据互斥的非缓存、缓存读取与缓存写入维度推导标准输入总量。
    InputFromDisjointDimensions,
}

/// Freezes the usage extraction contract selected by one resolved route.
/// 冻结一条已解析路由选定的用量提取契约。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageExtractionProfile {
    /// Stable profile identifier.
    /// 稳定的 Profile 标识。
    pub profile_id: String,
    /// Immutable profile revision.
    /// 不可变的 Profile 修订号。
    pub revision: u64,
    /// Raw record kind accepted by this profile.
    /// 当前 Profile 接受的原始记录类型。
    pub record_kind: UsageRecordKind,
    /// Merge relationship of matching records.
    /// 匹配记录的合并关系。
    pub update_kind: UsageUpdateKind,
    /// Optional event type JSON Pointer.
    /// 可选的事件类型 JSON Pointer。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_type_pointer: Option<String>,
    /// Required event type value when an event pointer is configured.
    /// 配置事件指针时要求的事件类型值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_type_value: Option<String>,
    /// Optional JSON Pointer that must exist before bindings are evaluated.
    /// 在评估字段绑定前必须存在的可选 JSON Pointer。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_pointer: Option<String>,
    /// Exact field bindings for the selected provider route.
    /// 已选供应商路由的精确字段绑定。
    pub bindings: Vec<UsageFieldBinding>,
    /// Provider-approved derivations applied after extraction.
    /// 提取后应用的供应商认可推导。
    #[serde(default)]
    pub derivations: Vec<UsageDerivation>,
}

impl UsageExtractionProfile {
    /// Validates one profile before it can be frozen into a route.
    /// 在 Profile 冻结到路由前对其进行校验。
    ///
    /// # Returns
    /// 返回值
    /// Success for an unambiguous profile, otherwise a controlled configuration error.
    /// Profile 无歧义时成功，否则返回受控配置错误。
    pub fn validate(&self) -> Result<(), LlmCoreError> {
        if self.profile_id.trim().is_empty() {
            return Err(LlmCoreError::new("Usage profile id must not be empty."));
        }
        match (&self.event_type_pointer, &self.event_type_value) {
            (Some(pointer), Some(value)) if pointer.starts_with('/') && !value.is_empty() => {}
            (None, None) => {}
            _ => {
                return Err(LlmCoreError::new(
                    "Usage event type pointer and value must be configured together.",
                ));
            }
        }
        if self
            .required_pointer
            .as_ref()
            .is_some_and(|pointer| !pointer.starts_with('/'))
        {
            return Err(LlmCoreError::new(
                "Usage required JSON Pointer must start with `/`.",
            ));
        }
        let mut fields = BTreeSet::new();
        let mut pointers = BTreeSet::new();
        for binding in &self.bindings {
            if !fields.insert(binding.field) {
                return Err(LlmCoreError::new(format!(
                    "Usage field `{:?}` is registered more than once.",
                    binding.field
                )));
            }
            if binding.pointers.is_empty() {
                return Err(LlmCoreError::new(format!(
                    "Usage field `{:?}` has no registered JSON Pointer.",
                    binding.field
                )));
            }
            for pointer in &binding.pointers {
                if !pointer.starts_with('/') {
                    return Err(LlmCoreError::new(format!(
                        "Usage JSON Pointer `{pointer}` must start with `/`."
                    )));
                }
                if !pointers.insert(pointer.as_str()) {
                    return Err(LlmCoreError::new(format!(
                        "Usage JSON Pointer `{pointer}` is registered more than once."
                    )));
                }
            }
        }
        Ok(())
    }

    /// Extracts one matching raw record without converting missing values to zero.
    /// 从一条匹配的原始记录中提取用量，且不会把缺失值转换为零。
    ///
    /// # Parameters
    /// 参数
    /// `record_kind` identifies the transport evidence and `value` is the untouched JSON record.
    /// `record_kind` 标识传输证据，`value` 是未经转换的 JSON 记录。
    ///
    /// # Returns
    /// 返回值
    /// `None` for a non-matching event, or a provenance-preserving observation.
    /// 非匹配事件返回 `None`，否则返回保留来源的观测结果。
    pub fn extract(
        &self,
        record_kind: UsageRecordKind,
        value: &Value,
    ) -> Result<Option<UsageObservation>, LlmCoreError> {
        self.validate()?;
        if record_kind != self.record_kind {
            return Ok(None);
        }
        if let (Some(pointer), Some(expected)) = (&self.event_type_pointer, &self.event_type_value)
        {
            let Some(actual) = value.pointer(pointer).and_then(Value::as_str) else {
                return Ok(None);
            };
            if actual != expected {
                return Ok(None);
            }
        }
        if self.required_pointer.as_ref().is_some_and(|pointer| {
            value
                .pointer(pointer)
                .is_none_or(serde_json::Value::is_null)
        }) {
            return Ok(None);
        }

        let mut observation = UsageObservation {
            profile_id: self.profile_id.clone(),
            profile_revision: self.revision,
            usage: RequestUsage {
                update_kind: self.update_kind,
                ..RequestUsage::default()
            },
            source_paths: BTreeMap::new(),
            response_model: None,
            response_id: None,
            validation_flags: Vec::new(),
        };
        for binding in &self.bindings {
            let Some((pointer, raw)) = resolve_registered_alias(value, binding)? else {
                continue;
            };
            observation.assign(binding, pointer, raw)?;
        }
        observation.apply_derivations(&self.derivations);
        observation.enrich_usage();
        Ok(Some(observation))
    }
}

/// Builds versioned built-in extraction profiles for one exact upstream protocol.
/// 为一个精确上游协议构建版本化内置提取 Profile。
///
/// # Parameters
/// 参数
/// `protocol` is the frozen wire protocol selected by route resolution.
/// `protocol` 是路由解析冻结的 Wire 协议。
///
/// # Returns
/// 返回值
/// Profiles for non-streaming JSON and streaming records supported by that protocol.
/// 返回该协议支持的非流式 JSON 与流式记录 Profile。
pub fn standard_usage_profiles(protocol: crate::UpstreamProtocol) -> Vec<UsageExtractionProfile> {
    match protocol {
        crate::UpstreamProtocol::OpenAiResponses => responses_usage_profiles(),
        crate::UpstreamProtocol::OpenAiChatCompletions => chat_usage_profiles(),
        crate::UpstreamProtocol::AnthropicMessages => anthropic_usage_profiles(),
        crate::UpstreamProtocol::GeminiGenerateContent
        | crate::UpstreamProtocol::GoogleCodeAssist => gemini_usage_profiles(),
        crate::UpstreamProtocol::GeminiInteractions => interactions_usage_profiles(),
    }
}

/// Rebuilds the historical Alibaba Responses profiles as a migration-test oracle.
/// 作为迁移测试对照重建历史阿里云 Responses Profile。
///
/// # Returns
/// 返回值
/// Historical JSON, SSE, and WebSocket profiles for comparing manifest-owned definitions.
/// 返回用于对比 Manifest 权威定义的历史 JSON、SSE 与 WebSocket Profile。
#[cfg(any(test, feature = "provider-migration-oracles"))]
pub fn alibaba_responses_usage_profiles() -> Vec<UsageExtractionProfile> {
    responses_usage_profiles()
        .into_iter()
        .map(|mut profile| {
            profile.profile_id = format!("alibaba.{}", profile.profile_id);
            let pointer = match profile.record_kind {
                UsageRecordKind::JsonBody => "/usage/x_details",
                UsageRecordKind::SseEvent | UsageRecordKind::WebSocketFrame => {
                    "/response/usage/x_details"
                }
            };
            profile.bindings.push(UsageFieldBinding {
                field: UsageField::BillingDetails,
                pointers: vec![pointer.to_string()],
                encoding: UsageValueEncoding::AlibabaResponsesBillingDetails,
            });
            profile
        })
        .collect()
}

/// Builds revised Gemini Interactions terminal-event usage profiles.
/// 构建修订版 Gemini Interactions 终态事件用量 Profile。
fn interactions_usage_profiles() -> Vec<UsageExtractionProfile> {
    let bindings = vec![
        token_binding(
            UsageField::InputTokens,
            "/interaction/usage/total_input_tokens",
        ),
        token_binding(
            UsageField::CacheReadTokens,
            "/interaction/usage/total_cached_tokens",
        ),
        token_binding(
            UsageField::OutputTokens,
            "/interaction/usage/total_output_tokens",
        ),
        token_binding(
            UsageField::ReasoningTokens,
            "/interaction/usage/total_thought_tokens",
        ),
        token_binding(UsageField::TotalTokens, "/interaction/usage/total_tokens"),
        string_binding(UsageField::ResponseId, "/interaction/id"),
    ];
    vec![
        UsageExtractionProfile {
            profile_id: String::from("gemini.interactions.completed.usage.v1"),
            revision: 1,
            record_kind: UsageRecordKind::SseEvent,
            update_kind: UsageUpdateKind::Final,
            event_type_pointer: Some(String::from("/event_type")),
            event_type_value: Some(String::from("interaction.completed")),
            required_pointer: Some(String::from("/interaction/usage/total_input_tokens")),
            bindings,
            derivations: vec![UsageDerivation::UncachedInputFromInputMinusCacheRead],
        },
        UsageExtractionProfile {
            profile_id: String::from("gemini.interactions.completed.legacy-usage.v1"),
            revision: 1,
            record_kind: UsageRecordKind::SseEvent,
            update_kind: UsageUpdateKind::Final,
            event_type_pointer: Some(String::from("/type")),
            event_type_value: Some(String::from("interaction.completed")),
            required_pointer: Some(String::from("/interaction/usage/input_tokens")),
            bindings: vec![
                token_binding(UsageField::InputTokens, "/interaction/usage/input_tokens"),
                token_binding(
                    UsageField::CacheReadTokens,
                    "/interaction/usage/cached_tokens",
                ),
                token_binding(UsageField::OutputTokens, "/interaction/usage/output_tokens"),
                token_binding(
                    UsageField::ReasoningTokens,
                    "/interaction/usage/reasoning_tokens",
                ),
                token_binding(UsageField::TotalTokens, "/interaction/usage/total_tokens"),
                string_binding(UsageField::ResponseId, "/interaction/id"),
            ],
            derivations: vec![UsageDerivation::UncachedInputFromInputMinusCacheRead],
        },
        UsageExtractionProfile {
            profile_id: String::from("gemini.interactions.finish.usage.v1"),
            revision: 1,
            record_kind: UsageRecordKind::SseEvent,
            update_kind: UsageUpdateKind::Final,
            event_type_pointer: Some(String::from("/event_type")),
            event_type_value: Some(String::from("finish")),
            required_pointer: Some(String::from("/metadata/total_usage")),
            bindings: vec![
                token_binding(
                    UsageField::InputTokens,
                    "/metadata/total_usage/total_input_tokens",
                ),
                token_binding(
                    UsageField::CacheReadTokens,
                    "/metadata/total_usage/total_cached_tokens",
                ),
                token_binding(
                    UsageField::OutputTokens,
                    "/metadata/total_usage/total_output_tokens",
                ),
                token_binding(
                    UsageField::ReasoningTokens,
                    "/metadata/total_usage/total_thought_tokens",
                ),
                token_binding(
                    UsageField::TotalTokens,
                    "/metadata/total_usage/total_tokens",
                ),
            ],
            derivations: vec![UsageDerivation::UncachedInputFromInputMinusCacheRead],
        },
    ]
}

/// Rebuilds the historical DeepSeek Chat profiles as a migration-test oracle.
/// 作为迁移测试对照重建历史 DeepSeek Chat Profile。
///
/// # Returns
/// 返回值
/// Historical non-streaming and SSE profiles for comparing manifest-owned definitions.
/// 返回用于对比 Manifest 权威定义的历史非流式与 SSE Profile。
#[cfg(any(test, feature = "provider-migration-oracles"))]
pub fn deepseek_chat_usage_profiles() -> Vec<UsageExtractionProfile> {
    let bindings = vec![
        token_binding(UsageField::InputTokens, "/usage/prompt_tokens"),
        token_binding(
            UsageField::UncachedInputTokens,
            "/usage/prompt_cache_miss_tokens",
        ),
        token_binding(
            UsageField::CacheReadTokens,
            "/usage/prompt_cache_hit_tokens",
        ),
        token_binding(UsageField::OutputTokens, "/usage/completion_tokens"),
        token_binding(
            UsageField::ReasoningTokens,
            "/usage/completion_tokens_details/reasoning_tokens",
        ),
        token_binding(UsageField::TotalTokens, "/usage/total_tokens"),
        string_binding(UsageField::ResponseModel, "/model"),
        string_binding(UsageField::ResponseId, "/id"),
    ];
    vec![
        UsageExtractionProfile {
            profile_id: String::from("deepseek.chat.body.v1"),
            revision: 1,
            record_kind: UsageRecordKind::JsonBody,
            update_kind: UsageUpdateKind::Final,
            event_type_pointer: None,
            event_type_value: None,
            required_pointer: Some(String::from("/usage")),
            bindings: bindings.clone(),
            derivations: Vec::new(),
        },
        UsageExtractionProfile {
            profile_id: String::from("deepseek.chat.sse.usage.v1"),
            revision: 1,
            record_kind: UsageRecordKind::SseEvent,
            update_kind: UsageUpdateKind::Final,
            event_type_pointer: None,
            event_type_value: None,
            required_pointer: Some(String::from("/usage")),
            bindings,
            derivations: Vec::new(),
        },
    ]
}

/// Builds standard Responses body and terminal-event profiles.
/// 构建标准 Responses 正文与终态事件 Profile。
fn responses_usage_profiles() -> Vec<UsageExtractionProfile> {
    let bindings = vec![
        token_binding(UsageField::InputTokens, "/usage/input_tokens"),
        token_binding(
            UsageField::CacheReadTokens,
            "/usage/input_tokens_details/cached_tokens",
        ),
        token_binding(UsageField::OutputTokens, "/usage/output_tokens"),
        token_binding(
            UsageField::ReasoningTokens,
            "/usage/output_tokens_details/reasoning_tokens",
        ),
        token_binding(
            UsageField::AudioInputTokens,
            "/usage/input_tokens_details/audio_tokens",
        ),
        token_binding(
            UsageField::AudioOutputTokens,
            "/usage/output_tokens_details/audio_tokens",
        ),
        token_binding(UsageField::TotalTokens, "/usage/total_tokens"),
        string_binding(UsageField::ResponseModel, "/model"),
        string_binding(UsageField::ResponseId, "/id"),
    ];
    vec![
        UsageExtractionProfile {
            profile_id: String::from("openai.responses.body.v1"),
            revision: 1,
            record_kind: UsageRecordKind::JsonBody,
            update_kind: UsageUpdateKind::Final,
            event_type_pointer: None,
            event_type_value: None,
            required_pointer: Some(String::from("/usage")),
            bindings: bindings.clone(),
            derivations: vec![UsageDerivation::UncachedInputFromInputMinusCacheRead],
        },
        UsageExtractionProfile {
            profile_id: String::from("openai.responses.sse.completed.v1"),
            revision: 1,
            record_kind: UsageRecordKind::SseEvent,
            update_kind: UsageUpdateKind::Final,
            event_type_pointer: Some(String::from("/type")),
            event_type_value: Some(String::from("response.completed")),
            required_pointer: Some(String::from("/response/usage")),
            bindings: prefix_bindings(bindings, "/response"),
            derivations: vec![UsageDerivation::UncachedInputFromInputMinusCacheRead],
        },
        UsageExtractionProfile {
            profile_id: String::from("openai.responses.ws.completed.v1"),
            revision: 1,
            record_kind: UsageRecordKind::WebSocketFrame,
            update_kind: UsageUpdateKind::Final,
            event_type_pointer: Some(String::from("/type")),
            event_type_value: Some(String::from("response.completed")),
            required_pointer: Some(String::from("/response/usage")),
            bindings: prefix_bindings(responses_usage_bindings(), "/response"),
            derivations: vec![UsageDerivation::UncachedInputFromInputMinusCacheRead],
        },
    ]
}

/// Builds standard OpenAI-compatible Chat usage profiles.
/// 构建标准 OpenAI-compatible Chat 用量 Profile。
fn chat_usage_profiles() -> Vec<UsageExtractionProfile> {
    let bindings = vec![
        token_binding(UsageField::InputTokens, "/usage/prompt_tokens"),
        token_binding(
            UsageField::CacheReadTokens,
            "/usage/prompt_tokens_details/cached_tokens",
        ),
        token_binding(UsageField::OutputTokens, "/usage/completion_tokens"),
        token_binding(
            UsageField::ReasoningTokens,
            "/usage/completion_tokens_details/reasoning_tokens",
        ),
        token_binding(UsageField::TotalTokens, "/usage/total_tokens"),
        string_binding(UsageField::ResponseModel, "/model"),
        string_binding(UsageField::ResponseId, "/id"),
    ];
    vec![
        UsageExtractionProfile {
            profile_id: String::from("openai.chat.body.v1"),
            revision: 1,
            record_kind: UsageRecordKind::JsonBody,
            update_kind: UsageUpdateKind::Final,
            event_type_pointer: None,
            event_type_value: None,
            required_pointer: Some(String::from("/usage")),
            bindings: bindings.clone(),
            derivations: vec![UsageDerivation::UncachedInputFromInputMinusCacheRead],
        },
        UsageExtractionProfile {
            profile_id: String::from("openai.chat.sse.usage.v1"),
            revision: 1,
            record_kind: UsageRecordKind::SseEvent,
            update_kind: UsageUpdateKind::Final,
            event_type_pointer: None,
            event_type_value: None,
            required_pointer: Some(String::from("/usage")),
            bindings,
            derivations: vec![UsageDerivation::UncachedInputFromInputMinusCacheRead],
        },
    ]
}

/// Rebuilds the historical OpenRouter Chat profiles as a migration-test oracle.
/// 作为迁移测试对照重建历史 OpenRouter Chat Profile。
#[cfg(any(test, feature = "provider-migration-oracles"))]
pub fn openrouter_chat_usage_profiles() -> Vec<UsageExtractionProfile> {
    // profiles reuse only standard Chat token fields before adding the provider-owned cost extension.
    // profiles 仅复用标准 Chat Token 字段，随后再加入供应商自有成本扩展。
    let mut profiles = chat_usage_profiles();
    for profile in &mut profiles {
        profile.profile_id = profile
            .profile_id
            .replacen("openai.chat.", "openrouter.chat.", 1);
        profile.bindings.push(UsageFieldBinding {
            field: UsageField::ReportedCostUsd,
            pointers: vec![String::from("/usage/cost")],
            encoding: UsageValueEncoding::Decimal,
        });
    }
    profiles
}

/// Builds Anthropic message-start and message-delta usage profiles.
/// 构建 Anthropic message-start 与 message-delta 用量 Profile。
fn anthropic_usage_profiles() -> Vec<UsageExtractionProfile> {
    let body_bindings = vec![
        token_binding(UsageField::InputTokens, "/usage/input_tokens"),
        token_binding(
            UsageField::CacheReadTokens,
            "/usage/cache_read_input_tokens",
        ),
        token_binding(
            UsageField::CacheWriteTokens,
            "/usage/cache_creation_input_tokens",
        ),
        token_binding(
            UsageField::CacheWriteFiveMinuteTokens,
            "/usage/cache_creation/ephemeral_5m_input_tokens",
        ),
        token_binding(
            UsageField::CacheWriteOneHourTokens,
            "/usage/cache_creation/ephemeral_1h_input_tokens",
        ),
        token_binding(UsageField::OutputTokens, "/usage/output_tokens"),
        string_binding(UsageField::ResponseModel, "/model"),
        string_binding(UsageField::ResponseId, "/id"),
    ];
    vec![
        UsageExtractionProfile {
            profile_id: String::from("anthropic.message.body.usage.v1"),
            revision: 1,
            record_kind: UsageRecordKind::JsonBody,
            update_kind: UsageUpdateKind::Final,
            event_type_pointer: None,
            event_type_value: None,
            required_pointer: Some(String::from("/usage")),
            bindings: body_bindings.clone(),
            derivations: vec![
                UsageDerivation::UncachedInputFromInput,
                UsageDerivation::CacheWriteFromDurations,
                UsageDerivation::InputFromDisjointDimensions,
            ],
        },
        UsageExtractionProfile {
            profile_id: String::from("anthropic.message_start.usage.v1"),
            revision: 1,
            record_kind: UsageRecordKind::SseEvent,
            update_kind: UsageUpdateKind::Snapshot,
            event_type_pointer: Some(String::from("/type")),
            event_type_value: Some(String::from("message_start")),
            required_pointer: Some(String::from("/message/usage")),
            bindings: prefix_bindings(body_bindings, "/message"),
            derivations: vec![
                UsageDerivation::UncachedInputFromInput,
                UsageDerivation::CacheWriteFromDurations,
                UsageDerivation::InputFromDisjointDimensions,
            ],
        },
        UsageExtractionProfile {
            profile_id: String::from("anthropic.message_delta.usage.v1"),
            revision: 1,
            record_kind: UsageRecordKind::SseEvent,
            update_kind: UsageUpdateKind::Final,
            event_type_pointer: Some(String::from("/type")),
            event_type_value: Some(String::from("message_delta")),
            required_pointer: Some(String::from("/usage")),
            bindings: vec![token_binding(
                UsageField::OutputTokens,
                "/usage/output_tokens",
            )],
            derivations: Vec::new(),
        },
    ]
}

/// Builds Gemini body and streamed-record usage profiles.
/// 构建 Gemini 正文与流式记录用量 Profile。
fn gemini_usage_profiles() -> Vec<UsageExtractionProfile> {
    let bindings = vec![
        token_binding(UsageField::InputTokens, "/usageMetadata/promptTokenCount"),
        token_binding(
            UsageField::CacheReadTokens,
            "/usageMetadata/cachedContentTokenCount",
        ),
        token_binding(
            UsageField::OutputTokens,
            "/usageMetadata/candidatesTokenCount",
        ),
        token_binding(
            UsageField::ReasoningTokens,
            "/usageMetadata/thoughtsTokenCount",
        ),
        token_binding(UsageField::TotalTokens, "/usageMetadata/totalTokenCount"),
        string_binding(UsageField::ResponseModel, "/modelVersion"),
        string_binding(UsageField::ResponseId, "/responseId"),
    ];
    vec![
        UsageExtractionProfile {
            profile_id: String::from("google.gemini.body.v1"),
            revision: 1,
            record_kind: UsageRecordKind::JsonBody,
            update_kind: UsageUpdateKind::Final,
            event_type_pointer: None,
            event_type_value: None,
            required_pointer: Some(String::from("/usageMetadata")),
            bindings: bindings.clone(),
            derivations: vec![UsageDerivation::UncachedInputFromInputMinusCacheRead],
        },
        UsageExtractionProfile {
            profile_id: String::from("google.gemini.stream.v1"),
            revision: 1,
            record_kind: UsageRecordKind::SseEvent,
            update_kind: UsageUpdateKind::Snapshot,
            event_type_pointer: None,
            event_type_value: None,
            required_pointer: Some(String::from("/usageMetadata")),
            bindings,
            derivations: vec![UsageDerivation::UncachedInputFromInputMinusCacheRead],
        },
    ]
}

/// Returns canonical Responses usage bindings without an event envelope.
/// 返回不含事件信封的规范 Responses 用量绑定。
fn responses_usage_bindings() -> Vec<UsageFieldBinding> {
    vec![
        token_binding(UsageField::InputTokens, "/usage/input_tokens"),
        token_binding(
            UsageField::CacheReadTokens,
            "/usage/input_tokens_details/cached_tokens",
        ),
        token_binding(UsageField::OutputTokens, "/usage/output_tokens"),
        token_binding(
            UsageField::ReasoningTokens,
            "/usage/output_tokens_details/reasoning_tokens",
        ),
        token_binding(UsageField::TotalTokens, "/usage/total_tokens"),
        string_binding(UsageField::ResponseModel, "/model"),
        string_binding(UsageField::ResponseId, "/id"),
    ]
}

/// Builds one unsigned-integer token binding.
/// 构建一个无符号整数 Token 绑定。
fn token_binding(field: UsageField, pointer: &str) -> UsageFieldBinding {
    UsageFieldBinding {
        field,
        pointers: vec![pointer.to_string()],
        encoding: UsageValueEncoding::UnsignedInteger,
    }
}

/// Builds one exact string metadata binding.
/// 构建一个精确字符串元数据绑定。
fn string_binding(field: UsageField, pointer: &str) -> UsageFieldBinding {
    UsageFieldBinding {
        field,
        pointers: vec![pointer.to_string()],
        encoding: UsageValueEncoding::String,
    }
}

/// Prefixes every registered pointer for one explicit event envelope.
/// 为一个显式事件信封添加所有已注册指针前缀。
fn prefix_bindings(bindings: Vec<UsageFieldBinding>, prefix: &str) -> Vec<UsageFieldBinding> {
    bindings
        .into_iter()
        .map(|mut binding| {
            binding.pointers = binding
                .pointers
                .into_iter()
                .map(|pointer| format!("{prefix}{pointer}"))
                .collect();
            binding
        })
        .collect()
}

/// Copies one known reported usage value while marking the resulting dimension as derived.
/// 复制一个已知的供应商报告用量值，并将结果维度标记为推导值。
fn derive_usage_copy(value: UsageValue) -> UsageValue {
    match value {
        UsageValue::Known { value, .. } => UsageValue::Known {
            value,
            source: UsageValueSource::Derived,
        },
        UsageValue::Unknown => UsageValue::Unknown,
    }
}

/// Carries one normalized usage update together with exact source paths.
/// 承载一条归一化用量更新及其精确来源路径。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageObservation {
    /// Profile identity used for extraction.
    /// 用于提取的 Profile 标识。
    pub profile_id: String,
    /// Profile revision used for extraction.
    /// 用于提取的 Profile 修订号。
    pub profile_revision: u64,
    /// Normalized missing-aware usage update.
    /// 归一化且可表达缺失的用量更新。
    pub usage: RequestUsage,
    /// Exact JSON Pointer used for every known field.
    /// 每个已知字段实际使用的精确 JSON Pointer。
    pub source_paths: BTreeMap<UsageField, String>,
    /// Provider-returned model identity when reported.
    /// 供应商报告时返回的模型标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_model: Option<String>,
    /// Provider-returned response identity when reported.
    /// 供应商报告时返回的响应标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    /// Validation findings that preserve evidence without inventing values.
    /// 在不虚构数值的前提下保留证据的校验标记。
    #[serde(default)]
    pub validation_flags: Vec<String>,
}

impl UsageObservation {
    /// Copies extraction provenance into the normalized request usage payload.
    /// 将提取来源复制到归一化请求用量负载中。
    fn enrich_usage(&mut self) {
        self.usage.extraction_profile_id = Some(self.profile_id.clone());
        self.usage.extraction_profile_revision = Some(self.profile_revision);
        self.usage.source_paths = self
            .source_paths
            .iter()
            .map(|(field, path)| (usage_field_name(*field).to_string(), path.clone()))
            .collect();
        self.usage.validation_flags = self.validation_flags.clone();
        self.usage.response_model = self.response_model.clone();
        self.usage.response_id = self.response_id.clone();
    }

    /// Assigns one validated provider value to its normalized destination.
    /// 将一个已校验供应商值写入其归一化目标。
    fn assign(
        &mut self,
        binding: &UsageFieldBinding,
        pointer: &str,
        raw: &Value,
    ) -> Result<(), LlmCoreError> {
        match binding.field {
            UsageField::ReportedCost => {
                self.usage.cost = Some(parse_decimal(raw, binding.encoding, pointer)?);
            }
            UsageField::ReportedCostUsd => {
                self.usage.cost = Some(parse_decimal(raw, binding.encoding, pointer)?);
                self.usage.cost_currency = Some(String::from("USD"));
            }
            UsageField::ResponseModel => {
                self.response_model = Some(parse_string(raw, binding.encoding, pointer)?);
            }
            UsageField::ResponseId => {
                self.response_id = Some(parse_string(raw, binding.encoding, pointer)?);
            }
            UsageField::BillingDetails => {
                self.usage.billing_details =
                    parse_alibaba_responses_billing_details(raw, binding.encoding, pointer)?;
            }
            field => {
                let count = parse_unsigned(raw, binding.encoding, pointer)?;
                assign_usage_value(&mut self.usage, field, count)?;
            }
        }
        self.source_paths.insert(binding.field, pointer.to_string());
        Ok(())
    }

    /// Applies only the derivations explicitly registered by the selected profile.
    /// 仅应用已选 Profile 显式注册的推导关系。
    fn apply_derivations(&mut self, derivations: &[UsageDerivation]) {
        for derivation in derivations {
            match derivation {
                UsageDerivation::UncachedInputFromInput => {
                    if matches!(self.usage.uncached_input_tokens, UsageValue::Unknown) {
                        self.usage.uncached_input_tokens =
                            derive_usage_copy(self.usage.input_tokens);
                        if !matches!(self.usage.uncached_input_tokens, UsageValue::Unknown) {
                            self.source_paths.insert(
                                UsageField::UncachedInputTokens,
                                String::from("derived:input_tokens"),
                            );
                        }
                    }
                }
                UsageDerivation::UncachedInputFromInputMinusCacheRead => {
                    if matches!(self.usage.uncached_input_tokens, UsageValue::Unknown) {
                        self.usage.uncached_input_tokens = derive_uncached_input(
                            self.usage.input_tokens,
                            self.usage.cache_read_tokens,
                        );
                        if !matches!(self.usage.uncached_input_tokens, UsageValue::Unknown) {
                            self.source_paths.insert(
                                UsageField::UncachedInputTokens,
                                String::from("derived:input_tokens-cache_read_tokens"),
                            );
                        }
                    }
                }
                UsageDerivation::CacheWriteFromDurations => {
                    if matches!(self.usage.cache_write_tokens, UsageValue::Unknown) {
                        self.usage.cache_write_tokens = derive_usage_sum(
                            self.usage.cache_write_5m_tokens,
                            self.usage.cache_write_1h_tokens,
                        );
                        if !matches!(self.usage.cache_write_tokens, UsageValue::Unknown) {
                            self.source_paths.insert(
                                UsageField::CacheWriteTokens,
                                String::from("derived:cache_write_5m_tokens+cache_write_1h_tokens"),
                            );
                        }
                    }
                }
                UsageDerivation::InputFromDisjointDimensions => {
                    let input = derive_usage_sum(
                        derive_usage_sum(
                            self.usage.uncached_input_tokens,
                            self.usage.cache_read_tokens,
                        ),
                        self.usage.cache_write_tokens,
                    );
                    if !matches!(input, UsageValue::Unknown) {
                        self.usage.input_tokens = input;
                        self.source_paths.insert(
                            UsageField::InputTokens,
                            String::from(
                                "derived:uncached_input_tokens+cache_read_tokens+cache_write_tokens",
                            ),
                        );
                    }
                }
            }
        }
    }
}

/// Maximum provider event identities retained by one physical usage attempt.
/// 单次物理用量尝试允许保留的最大供应商事件身份数量。
const USAGE_ACCUMULATOR_MAX_EVENT_IDS: usize = 4_096;

/// Maximum UTF-8 bytes accepted for one provider usage event identity.
/// 单个供应商用量事件身份允许接受的最大 UTF-8 字节数。
const USAGE_ACCUMULATOR_EVENT_ID_MAX_BYTES: usize = 1_024;

/// Maximum distinct validation flags retained by one physical usage attempt.
/// 单次物理用量尝试允许保留的最大不同校验标记数量。
const USAGE_ACCUMULATOR_MAX_VALIDATION_FLAGS: usize = 256;

/// Maximum UTF-8 bytes accepted for one accumulated validation flag.
/// 单条累计校验标记允许接受的最大 UTF-8 字节数。
const USAGE_ACCUMULATOR_VALIDATION_FLAG_MAX_BYTES: usize = 1_024;

/// Accumulates one physical attempt's usage with duplicate and terminal guards.
/// 在重复与终态守卫下累计一次物理 attempt 的用量。
#[derive(Debug, Clone, Default)]
pub struct UsageAccumulator {
    /// Accumulated normalized usage.
    /// 已累计的归一化用量。
    usage: RequestUsage,
    /// Latest source path for every known field.
    /// 每个已知字段的最新来源路径。
    source_paths: BTreeMap<UsageField, String>,
    /// Event identities already consumed exactly once.
    /// 已精确消费一次的事件标识。
    seen_event_ids: BTreeSet<String>,
    /// Canonical terminal observation used for idempotency.
    /// 用于幂等判断的规范终态观测。
    terminal_observation: Option<String>,
    /// Accumulated validation flags.
    /// 已累计的校验标记。
    validation_flags: Vec<String>,
}

impl UsageAccumulator {
    /// Applies one observation exactly once and freezes the first final update.
    /// 精确应用一条观测，并冻结第一条终态更新。
    ///
    /// # Parameters
    /// 参数
    /// `event_id` is the optional provider sequence identity and `observation` is the extracted update.
    /// `event_id` 是可选供应商序列标识，`observation` 是提取后的更新。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the update changed state, `false` for an exact duplicate.
    /// 更新改变状态时返回 `true`，精确重复时返回 `false`。
    pub fn apply(
        &mut self,
        event_id: Option<&str>,
        mut observation: UsageObservation,
    ) -> Result<bool, LlmCoreError> {
        if let Some(event_id) = event_id {
            if event_id.is_empty() || event_id.len() > USAGE_ACCUMULATOR_EVENT_ID_MAX_BYTES {
                return Err(LlmCoreError::new(format!(
                    "Usage event identity must contain 1..={USAGE_ACCUMULATOR_EVENT_ID_MAX_BYTES} UTF-8 bytes."
                )));
            }
            if self.seen_event_ids.contains(event_id) {
                return Ok(false);
            }
            if self.seen_event_ids.len() >= USAGE_ACCUMULATOR_MAX_EVENT_IDS {
                return Err(LlmCoreError::new(format!(
                    "Usage event identity history exceeds the {USAGE_ACCUMULATOR_MAX_EVENT_IDS}-event limit."
                )));
            }
        }
        // canonical freezes the pre-enrichment observation for terminal idempotency checks.
        // canonical 为终态幂等检查冻结富化前的观测。
        let canonical = serde_json::to_string(&observation).map_err(|error| {
            LlmCoreError::new(format!(
                "Usage observation could not be canonicalized: {error}"
            ))
        })?;
        if let Some(terminal) = &self.terminal_observation {
            if terminal == &canonical {
                return Ok(false);
            }
            return Err(LlmCoreError::new(
                "Usage update arrived after a different final observation.",
            ));
        }

        if !matches!(observation.usage.update_kind, UsageUpdateKind::Delta) {
            record_monotonic_regressions(
                &self.usage,
                &observation.usage,
                &mut observation.validation_flags,
            );
        }
        observation.enrich_usage();
        // new_validation_flags contains only distinct, bounded findings not already retained.
        // new_validation_flags 只包含尚未保留的不同且有界校验发现。
        let mut new_validation_flags = Vec::new();
        for flag in &observation.validation_flags {
            if flag.is_empty() || flag.len() > USAGE_ACCUMULATOR_VALIDATION_FLAG_MAX_BYTES {
                return Err(LlmCoreError::new(format!(
                    "Usage validation flag must contain 1..={USAGE_ACCUMULATOR_VALIDATION_FLAG_MAX_BYTES} UTF-8 bytes."
                )));
            }
            if self.validation_flags.contains(flag) || new_validation_flags.contains(flag) {
                continue;
            }
            if self
                .validation_flags
                .len()
                .saturating_add(new_validation_flags.len())
                >= USAGE_ACCUMULATOR_MAX_VALIDATION_FLAGS
            {
                return Err(LlmCoreError::new(format!(
                    "Usage validation flags exceed the {USAGE_ACCUMULATOR_MAX_VALIDATION_FLAGS}-item limit."
                )));
            }
            new_validation_flags.push(flag.clone());
        }
        self.usage.merge_update(observation.usage.clone());
        self.source_paths.extend(observation.source_paths);
        self.validation_flags.extend(new_validation_flags);
        if matches!(observation.usage.update_kind, UsageUpdateKind::Final) {
            self.terminal_observation = Some(canonical);
        }
        if let Some(event_id) = event_id {
            self.seen_event_ids.insert(event_id.to_string());
        }
        Ok(true)
    }

    /// Returns the current missing-aware usage snapshot.
    /// 返回当前可表达缺失的用量快照。
    pub fn usage(&self) -> &RequestUsage {
        &self.usage
    }

    /// Returns the source paths retained for audit.
    /// 返回为审计保留的来源路径。
    pub fn source_paths(&self) -> &BTreeMap<UsageField, String> {
        &self.source_paths
    }

    /// Returns validation flags accumulated while merging.
    /// 返回合并过程中累计的校验标记。
    pub fn validation_flags(&self) -> &[String] {
        &self.validation_flags
    }

    /// Reports whether an authoritative final observation has been frozen.
    /// 报告是否已冻结权威终态观测。
    pub fn is_final(&self) -> bool {
        self.terminal_observation.is_some()
    }
}

/// Stores immutable per-million-token prices for one model revision.
/// 保存一个模型修订版的不可变每百万 Token 价格。
#[derive(Debug, Clone, PartialEq)]
pub struct ModelPricing {
    /// Price for uncached input tokens.
    /// 非缓存输入 Token 价格。
    pub input: Option<Decimal>,
    /// Price for output tokens.
    /// 输出 Token 价格。
    pub output: Option<Decimal>,
    /// Price for cache-read tokens.
    /// 缓存读取 Token 价格。
    pub cache_read: Option<Decimal>,
    /// Price for cache-write tokens.
    /// 缓存写入 Token 价格。
    pub cache_write: Option<Decimal>,
}

impl ModelPricing {
    /// Parses persisted decimal price strings without floating-point rounding.
    /// 解析持久化十进制价格字符串，且不产生浮点舍入。
    pub fn from_optional_strings(
        input: Option<&str>,
        output: Option<&str>,
        cache_read: Option<&str>,
        cache_write: Option<&str>,
    ) -> Result<Self, LlmCoreError> {
        Ok(Self {
            input: parse_optional_price(input, "input")?,
            output: parse_optional_price(output, "output")?,
            cache_read: parse_optional_price(cache_read, "cache_read")?,
            cache_write: parse_optional_price(cache_write, "cache_write")?,
        })
    }
}

/// Carries local estimated cost dimensions; missing usage or price stays unknown.
/// 承载本地估算成本维度；缺失用量或价格保持未知。
#[derive(Debug, Clone, PartialEq)]
pub struct CostBreakdown {
    /// Estimated uncached-input cost.
    /// 估算的非缓存输入成本。
    pub input: Option<Decimal>,
    /// Estimated output cost.
    /// 估算的输出成本。
    pub output: Option<Decimal>,
    /// Estimated cache-read cost.
    /// 估算的缓存读取成本。
    pub cache_read: Option<Decimal>,
    /// Estimated cache-write cost.
    /// 估算的缓存写入成本。
    pub cache_write: Option<Decimal>,
    /// Total cost only when every required component is known.
    /// 仅当所有必要组成均已知时提供总成本。
    pub total: Option<Decimal>,
}

/// Calculates deterministic local estimates from frozen usage and pricing revisions.
/// 根据冻结的用量和价格修订版计算确定性本地估算。
pub struct CostCalculator;

impl CostCalculator {
    /// Calculates a missing-aware local estimate with one explicit multiplier.
    /// 使用一个显式倍率计算可表达缺失的本地估算。
    pub fn calculate(
        usage: &RequestUsage,
        pricing: &ModelPricing,
        multiplier: Decimal,
    ) -> CostBreakdown {
        let input = priced_usage(usage.uncached_input_tokens, pricing.input, multiplier);
        let output = priced_usage(usage.output_tokens, pricing.output, multiplier);
        let cache_read = priced_usage(usage.cache_read_tokens, pricing.cache_read, multiplier);
        let cache_write = priced_usage(usage.cache_write_tokens, pricing.cache_write, multiplier);
        let total = [input, output, cache_read, cache_write]
            .into_iter()
            .try_fold(Decimal::ZERO, |sum, value| value.map(|value| sum + value));
        CostBreakdown {
            input,
            output,
            cache_read,
            cache_write,
            total,
        }
    }
}

/// Resolves exactly one registered alias and rejects conflicting duplicate evidence.
/// 精确解析一个已注册别名，并拒绝相互冲突的重复证据。
fn resolve_registered_alias<'a>(
    value: &'a Value,
    binding: &'a UsageFieldBinding,
) -> Result<Option<(&'a str, &'a Value)>, LlmCoreError> {
    let mut resolved: Option<(&str, &Value)> = None;
    for pointer in &binding.pointers {
        let Some(candidate) = value.pointer(pointer) else {
            continue;
        };
        if let Some((existing_pointer, existing)) = resolved {
            if existing != candidate {
                return Err(LlmCoreError::new(format!(
                    "Usage aliases `{existing_pointer}` and `{pointer}` reported conflicting values."
                )));
            }
        } else {
            resolved = Some((pointer.as_str(), candidate));
        }
    }
    Ok(resolved)
}

/// Parses one provider token count according to its registered representation.
/// 按已注册表示形式解析一个供应商 Token 数量。
fn parse_unsigned(
    value: &Value,
    encoding: UsageValueEncoding,
    pointer: &str,
) -> Result<u64, LlmCoreError> {
    let parsed = match encoding {
        UsageValueEncoding::UnsignedInteger => value.as_u64(),
        UsageValueEncoding::UnsignedIntegerString => {
            value.as_str().and_then(|raw| raw.parse().ok())
        }
        _ => None,
    };
    parsed.ok_or_else(|| {
        LlmCoreError::new(format!(
            "Usage value at `{pointer}` does not match encoding `{encoding:?}`."
        ))
    })
}

/// Parses one exact decimal value without binary floating-point conversion.
/// 在不进行二进制浮点转换的情况下解析一个精确十进制值。
fn parse_decimal(
    value: &Value,
    encoding: UsageValueEncoding,
    pointer: &str,
) -> Result<String, LlmCoreError> {
    let raw = match encoding {
        UsageValueEncoding::Decimal => value
            .as_str()
            .map(str::to_string)
            .or_else(|| value.as_number().map(ToString::to_string)),
        _ => None,
    }
    .ok_or_else(|| {
        LlmCoreError::new(format!(
            "Usage decimal at `{pointer}` does not match encoding `{encoding:?}`."
        ))
    })?;
    Decimal::from_str(&raw).map_err(|error| {
        LlmCoreError::new(format!(
            "Usage decimal at `{pointer}` is malformed: {error}"
        ))
    })?;
    Ok(raw)
}

/// Parses one exact provider string field.
/// 解析一个精确供应商字符串字段。
fn parse_string(
    value: &Value,
    encoding: UsageValueEncoding,
    pointer: &str,
) -> Result<String, LlmCoreError> {
    if encoding != UsageValueEncoding::String {
        return Err(LlmCoreError::new(format!(
            "Usage metadata at `{pointer}` must use string encoding."
        )));
    }
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| LlmCoreError::new(format!("Usage metadata at `{pointer}` is not a string.")))
}

/// Mirrors one documented Alibaba Responses billing partition with strict unknown-field rejection.
/// 严格镜像一项文档化的阿里云 Responses 计费分区，并拒绝未知字段。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlibabaResponsesBillingDetailWire {
    /// Fixed Alibaba Responses billing category.
    /// 固定的阿里云 Responses 计费类别。
    x_billing_type: String,
    /// Partition input tokens.
    /// 分区输入 Token。
    #[serde(default)]
    input_tokens: Option<u64>,
    /// Partition output tokens.
    /// 分区输出 Token。
    #[serde(default)]
    output_tokens: Option<u64>,
    /// Partition total tokens.
    /// 分区 Token 合计。
    #[serde(default)]
    total_tokens: Option<u64>,
    /// Optional top-level image-token compatibility field documented by Alibaba.
    /// 阿里云文档化的可选顶层图片 Token 兼容字段。
    #[serde(default)]
    image_tokens: Option<u64>,
    /// Optional detailed input partition.
    /// 可选输入明细分区。
    #[serde(default)]
    input_tokens_details: Option<AlibabaResponsesInputDetailsWire>,
    /// Optional detailed output partition.
    /// 可选输出明细分区。
    #[serde(default)]
    output_tokens_details: Option<AlibabaResponsesOutputDetailsWire>,
}

/// Mirrors Alibaba billing-partition input details.
/// 镜像阿里云计费分区输入明细。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlibabaResponsesInputDetailsWire {
    /// Text input tokens.
    /// 文本输入 Token。
    #[serde(default)]
    text_tokens: Option<u64>,
    /// Image input tokens.
    /// 图片输入 Token。
    #[serde(default)]
    image_tokens: Option<u64>,
}

/// Mirrors Alibaba billing-partition output details.
/// 镜像阿里云计费分区输出明细。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlibabaResponsesOutputDetailsWire {
    /// Reasoning output tokens.
    /// 推理输出 Token。
    #[serde(default)]
    reasoning_tokens: Option<u64>,
}

/// Parses one exact Alibaba Responses billing-detail array without changing normalized totals.
/// 解析一份精确的阿里云 Responses 计费明细数组，且不改写规范化合计。
fn parse_alibaba_responses_billing_details(
    value: &Value,
    encoding: UsageValueEncoding,
    pointer: &str,
) -> Result<Vec<UsageBillingDetail>, LlmCoreError> {
    if encoding != UsageValueEncoding::AlibabaResponsesBillingDetails {
        return Err(LlmCoreError::new(format!(
            "Usage billing details at `{pointer}` use an incompatible encoding."
        )));
    }
    let details = serde_json::from_value::<Vec<AlibabaResponsesBillingDetailWire>>(value.clone())
        .map_err(|error| {
        LlmCoreError::new(format!(
            "Alibaba Responses billing details at `{pointer}` are malformed: {error}"
        ))
    })?;
    details
        .into_iter()
        .enumerate()
        .map(|(index, detail)| {
            if detail.x_billing_type != "response_api" {
                return Err(LlmCoreError::new(format!(
                    "Alibaba Responses billing detail `{pointer}/{index}/x_billing_type` must equal `response_api`."
                )));
            }
            let nested_image_tokens = detail
                .input_tokens_details
                .as_ref()
                .and_then(|value| value.image_tokens);
            if let (Some(top_level), Some(nested)) =
                (detail.image_tokens, nested_image_tokens)
                && top_level != nested
            {
                return Err(LlmCoreError::new(format!(
                    "Alibaba Responses billing detail `{pointer}/{index}` reports conflicting image token counts."
                )));
            }
            Ok(UsageBillingDetail {
                billing_type: detail.x_billing_type,
                input_tokens: detail.input_tokens,
                output_tokens: detail.output_tokens,
                total_tokens: detail.total_tokens,
                text_tokens: detail
                    .input_tokens_details
                    .as_ref()
                    .and_then(|value| value.text_tokens),
                image_tokens: detail.image_tokens.or(nested_image_tokens),
                reasoning_tokens: detail
                    .output_tokens_details
                    .as_ref()
                    .and_then(|value| value.reasoning_tokens),
            })
        })
        .collect()
}

/// Writes one known token count into the exact normalized usage field.
/// 将一个已知 Token 数量写入精确的归一化用量字段。
fn assign_usage_value(
    usage: &mut RequestUsage,
    field: UsageField,
    count: u64,
) -> Result<(), LlmCoreError> {
    let value = UsageValue::Known {
        value: count,
        source: UsageValueSource::Reported,
    };
    match field {
        UsageField::InputTokens => usage.input_tokens = value,
        UsageField::UncachedInputTokens => usage.uncached_input_tokens = value,
        UsageField::OutputTokens => usage.output_tokens = value,
        UsageField::ReasoningTokens => usage.reasoning_tokens = value,
        UsageField::CacheReadTokens => usage.cache_read_tokens = value,
        UsageField::CacheWriteTokens => usage.cache_write_tokens = value,
        UsageField::CacheWriteFiveMinuteTokens => usage.cache_write_5m_tokens = value,
        UsageField::CacheWriteOneHourTokens => usage.cache_write_1h_tokens = value,
        UsageField::AudioInputTokens => usage.audio_input_tokens = value,
        UsageField::AudioOutputTokens => usage.audio_output_tokens = value,
        UsageField::TotalTokens => usage.total_tokens = value,
        UsageField::ReportedCost
        | UsageField::ReportedCostUsd
        | UsageField::ResponseModel
        | UsageField::ResponseId
        | UsageField::BillingDetails => {
            return Err(LlmCoreError::new(format!(
                "Usage field `{field:?}` is not a token dimension."
            )));
        }
    }
    Ok(())
}

/// Returns the stable persistence key for one normalized usage field.
/// 返回一个归一化用量字段的稳定持久化键。
fn usage_field_name(field: UsageField) -> &'static str {
    match field {
        UsageField::InputTokens => "input_tokens",
        UsageField::UncachedInputTokens => "uncached_input_tokens",
        UsageField::OutputTokens => "output_tokens",
        UsageField::ReasoningTokens => "reasoning_tokens",
        UsageField::CacheReadTokens => "cache_read_tokens",
        UsageField::CacheWriteTokens => "cache_write_tokens",
        UsageField::CacheWriteFiveMinuteTokens => "cache_write_5m_tokens",
        UsageField::CacheWriteOneHourTokens => "cache_write_1h_tokens",
        UsageField::AudioInputTokens => "audio_input_tokens",
        UsageField::AudioOutputTokens => "audio_output_tokens",
        UsageField::TotalTokens => "total_tokens",
        UsageField::ReportedCost | UsageField::ReportedCostUsd => "reported_cost",
        UsageField::ResponseModel => "response_model",
        UsageField::ResponseId => "response_id",
        UsageField::BillingDetails => "billing_details",
    }
}

/// Records cumulative snapshot regressions without overwriting provider evidence.
/// 在不覆盖供应商证据的情况下记录累计快照回退。
fn record_monotonic_regressions(
    current: &RequestUsage,
    update: &RequestUsage,
    flags: &mut Vec<String>,
) {
    for (name, current, update) in [
        ("input_tokens", current.input_tokens, update.input_tokens),
        ("output_tokens", current.output_tokens, update.output_tokens),
        ("total_tokens", current.total_tokens, update.total_tokens),
        (
            "cache_read_tokens",
            current.cache_read_tokens,
            update.cache_read_tokens,
        ),
    ] {
        if let (
            UsageValue::Known {
                value: current_value,
                ..
            },
            UsageValue::Known {
                value: update_value,
                ..
            },
        ) = (current, update)
            && update_value < current_value
        {
            flags.push(format!(
                "non_monotonic_{name}:{current_value}->{update_value}"
            ));
        }
    }
}

/// Parses one optional persisted price.
/// 解析一个可选持久化价格。
fn parse_optional_price(value: Option<&str>, label: &str) -> Result<Option<Decimal>, LlmCoreError> {
    value
        .map(|value| {
            Decimal::from_str(value).map_err(|error| {
                LlmCoreError::new(format!("Invalid {label} price `{value}`: {error}"))
            })
        })
        .transpose()
}

/// Prices one usage dimension and resolves explicit zero rates without token evidence.
/// 按每百万 Token 费率计算一个已知用量维度；明确零费率不依赖缺失用量。
fn priced_usage(usage: UsageValue, price: Option<Decimal>, multiplier: Decimal) -> Option<Decimal> {
    if price == Some(Decimal::ZERO) {
        return Some(Decimal::ZERO);
    }
    let UsageValue::Known { value, .. } = usage else {
        return None;
    };
    price.map(|price| Decimal::from(value) * price * multiplier / Decimal::from(1_000_000_u64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Builds an exact final Responses usage profile for test fixtures.
    /// 为测试 fixture 构建精确的 Responses 终态用量 Profile。
    fn responses_profile() -> UsageExtractionProfile {
        UsageExtractionProfile {
            profile_id: String::from("openai.responses.final.v1"),
            revision: 1,
            record_kind: UsageRecordKind::SseEvent,
            update_kind: UsageUpdateKind::Final,
            event_type_pointer: Some(String::from("/type")),
            event_type_value: Some(String::from("response.completed")),
            required_pointer: Some(String::from("/response/usage")),
            bindings: vec![
                UsageFieldBinding {
                    field: UsageField::InputTokens,
                    pointers: vec![String::from("/response/usage/input_tokens")],
                    encoding: UsageValueEncoding::UnsignedInteger,
                },
                UsageFieldBinding {
                    field: UsageField::CacheReadTokens,
                    pointers: vec![String::from(
                        "/response/usage/input_tokens_details/cached_tokens",
                    )],
                    encoding: UsageValueEncoding::UnsignedInteger,
                },
                UsageFieldBinding {
                    field: UsageField::OutputTokens,
                    pointers: vec![String::from("/response/usage/output_tokens")],
                    encoding: UsageValueEncoding::UnsignedInteger,
                },
                UsageFieldBinding {
                    field: UsageField::ReasoningTokens,
                    pointers: vec![String::from(
                        "/response/usage/output_tokens_details/reasoning_tokens",
                    )],
                    encoding: UsageValueEncoding::UnsignedInteger,
                },
                UsageFieldBinding {
                    field: UsageField::ResponseModel,
                    pointers: vec![String::from("/response/model")],
                    encoding: UsageValueEncoding::String,
                },
            ],
            derivations: vec![UsageDerivation::UncachedInputFromInputMinusCacheRead],
        }
    }

    #[test]
    fn explicit_profile_preserves_missing_and_source_paths() {
        let observation = responses_profile()
            .extract(
                UsageRecordKind::SseEvent,
                &json!({
                    "type":"response.completed",
                    "response": {
                        "model":"gpt-5",
                        "usage": {
                            "input_tokens":100,
                            "input_tokens_details":{"cached_tokens":40},
                            "output_tokens":12,
                            "output_tokens_details":{"reasoning_tokens":7}
                        }
                    }
                }),
            )
            .expect("usage should parse")
            .expect("event should match");

        assert_eq!(
            observation.usage.uncached_input_tokens,
            UsageValue::Known {
                value: 60,
                source: UsageValueSource::Derived,
            }
        );
        assert_eq!(observation.usage.total_tokens, UsageValue::Unknown);
        assert_eq!(
            observation.source_paths[&UsageField::ReasoningTokens],
            "/response/usage/output_tokens_details/reasoning_tokens"
        );
    }

    #[test]
    fn malformed_registered_value_is_not_treated_as_missing() {
        let error = responses_profile()
            .extract(
                UsageRecordKind::SseEvent,
                &json!({
                    "type":"response.completed",
                    "response":{"usage":{"input_tokens":"100"},"model":"gpt-5"}
                }),
            )
            .expect_err("wrong registered representation must fail");
        assert!(error.message.contains("input_tokens"));
    }

    #[test]
    fn conflicting_registered_aliases_are_rejected() {
        let mut profile = responses_profile();
        profile.bindings[0]
            .pointers
            .push(String::from("/response/usage/prompt_tokens"));
        let error = profile
            .extract(
                UsageRecordKind::SseEvent,
                &json!({
                    "type":"response.completed",
                    "response": {
                        "model":"gpt-5",
                        "usage":{"input_tokens":100,"prompt_tokens":99}
                    }
                }),
            )
            .expect_err("conflicting aliases must fail");
        assert!(error.message.contains("conflicting"));
    }

    #[test]
    fn accumulator_deduplicates_and_freezes_final_usage() {
        let observation = responses_profile()
            .extract(
                UsageRecordKind::SseEvent,
                &json!({
                    "type":"response.completed",
                    "response":{"model":"gpt-5","usage":{"input_tokens":10}}
                }),
            )
            .expect("usage should parse")
            .expect("event should match");
        let mut accumulator = UsageAccumulator::default();
        assert!(
            accumulator
                .apply(Some("event-1"), observation.clone())
                .expect("first final should apply")
        );
        assert!(
            !accumulator
                .apply(Some("event-1"), observation)
                .expect("duplicate event should be idempotent")
        );
        assert!(accumulator.is_final());
    }

    /// Verifies one physical attempt cannot retain an unbounded provider event-id history.
    /// 验证单次物理尝试无法保留无界供应商事件 ID 历史。
    #[test]
    fn accumulator_rejects_event_id_history_beyond_the_explicit_limit() {
        // observation is a non-terminal snapshot that may legitimately repeat with unique ids.
        // observation 是可携带不同 ID 合法重复的非终态快照。
        let observation = UsageObservation {
            profile_id: String::from("fixture.snapshot.v1"),
            profile_revision: 1,
            usage: RequestUsage::default(),
            source_paths: BTreeMap::new(),
            response_model: None,
            response_id: None,
            validation_flags: Vec::new(),
        };
        // accumulator retains exactly the reviewed number of unique provider identities.
        // accumulator 精确保留已审核数量的不同供应商身份。
        let mut accumulator = UsageAccumulator::default();
        for index in 0..USAGE_ACCUMULATOR_MAX_EVENT_IDS {
            // event_id is a bounded deterministic identity unique within this physical attempt.
            // event_id 是当前物理尝试内有界且确定的唯一身份。
            let event_id = format!("event-{index}");
            assert!(
                accumulator
                    .apply(Some(event_id.as_str()), observation.clone())
                    .expect("event identity within the limit should apply")
            );
        }

        // error proves the next distinct identity fails instead of growing the retained set.
        // error 证明下一个不同身份会失败，而不是继续扩大保留集合。
        let error = accumulator
            .apply(Some("event-overflow"), observation)
            .expect_err("event identity history beyond the limit must fail closed");
        assert!(error.message.contains("4096-event limit"));
    }

    /// Verifies accumulated validation findings are distinct, bounded, and never grow forever.
    /// 验证累计校验发现保持不同、有界且绝不会无限增长。
    #[test]
    fn accumulator_rejects_validation_flags_beyond_the_explicit_limit() {
        // accumulator receives one distinct provider validation finding per snapshot.
        // accumulator 每个快照接收一条不同的供应商校验发现。
        let mut accumulator = UsageAccumulator::default();
        for index in 0..USAGE_ACCUMULATOR_MAX_VALIDATION_FLAGS {
            // observation carries one bounded finding and no provider event identity.
            // observation 携带一条有界发现且不携带供应商事件身份。
            let observation = UsageObservation {
                profile_id: String::from("fixture.snapshot.v1"),
                profile_revision: 1,
                usage: RequestUsage::default(),
                source_paths: BTreeMap::new(),
                response_model: None,
                response_id: None,
                validation_flags: vec![format!("validation-{index}")],
            };
            assert!(
                accumulator
                    .apply(None, observation)
                    .expect("validation finding within the limit should apply")
            );
        }
        // overflow carries the first distinct finding outside the retained capacity.
        // overflow 携带首条超出保留容量的不同发现。
        let overflow = UsageObservation {
            profile_id: String::from("fixture.snapshot.v1"),
            profile_revision: 1,
            usage: RequestUsage::default(),
            source_paths: BTreeMap::new(),
            response_model: None,
            response_id: None,
            validation_flags: vec![String::from("validation-overflow")],
        };

        // error proves the accumulator rejects overflow without mutating the accepted history.
        // error 证明累加器拒绝溢出，且不改变已接受历史。
        let error = accumulator
            .apply(None, overflow)
            .expect_err("validation findings beyond the limit must fail closed");
        assert!(error.message.contains("256-item limit"));
        assert_eq!(
            accumulator.validation_flags().len(),
            USAGE_ACCUMULATOR_MAX_VALIDATION_FLAGS
        );
    }

    /// Ignores nullable usage placeholders emitted by intermediate streaming chunks.
    /// 忽略流式中间分片发出的空 usage 占位值。
    #[test]
    fn nullable_required_usage_pointer_does_not_create_a_final_observation() {
        let profile = chat_usage_profiles()
            .into_iter()
            .find(|profile| profile.record_kind == UsageRecordKind::SseEvent)
            .expect("chat SSE usage profile should exist");

        let observation = profile
            .extract(
                UsageRecordKind::SseEvent,
                &json!({
                    "id": "chatcmpl-placeholder",
                    "model": "qwen3.5-flash",
                    "usage": null
                }),
            )
            .expect("nullable usage placeholder should be accepted");

        assert!(observation.is_none());
    }

    /// Verifies Alibaba Responses billing partitions remain typed and separate from top-level totals.
    /// 验证阿里云 Responses 计费分区保持强类型并与顶层合计分离。
    #[test]
    fn alibaba_responses_profile_preserves_billing_details() {
        let profile = alibaba_responses_usage_profiles()
            .into_iter()
            .find(|profile| profile.record_kind == UsageRecordKind::JsonBody)
            .expect("Alibaba Responses JSON profile should exist");
        let observation = profile
            .extract(
                UsageRecordKind::JsonBody,
                &json!({
                    "id": "resp_alibaba",
                    "model": "qwen-plus",
                    "usage": {
                        "input_tokens": 52,
                        "output_tokens": 1280,
                        "total_tokens": 1332,
                        "x_details": [{
                            "input_tokens": 52,
                            "output_tokens": 1280,
                            "total_tokens": 1332,
                            "image_tokens": 12,
                            "input_tokens_details": {
                                "text_tokens": 40,
                                "image_tokens": 12
                            },
                            "output_tokens_details": {"reasoning_tokens": 1122},
                            "x_billing_type": "response_api"
                        }]
                    }
                }),
            )
            .expect("Alibaba usage should parse")
            .expect("Alibaba usage should match");
        assert_eq!(observation.usage.billing_details.len(), 1);
        let detail = &observation.usage.billing_details[0];
        assert_eq!(detail.billing_type, "response_api");
        assert_eq!(detail.input_tokens, Some(52));
        assert_eq!(detail.output_tokens, Some(1280));
        assert_eq!(detail.total_tokens, Some(1332));
        assert_eq!(detail.text_tokens, Some(40));
        assert_eq!(detail.image_tokens, Some(12));
        assert_eq!(detail.reasoning_tokens, Some(1122));
        assert_eq!(
            observation
                .source_paths
                .get(&UsageField::BillingDetails)
                .map(String::as_str),
            Some("/usage/x_details")
        );

        let error = profile
            .extract(
                UsageRecordKind::JsonBody,
                &json!({
                    "usage": {
                        "input_tokens": 1,
                        "output_tokens": 1,
                        "total_tokens": 2,
                        "x_details": [{
                            "input_tokens": 1,
                            "output_tokens": 1,
                            "total_tokens": 2,
                            "x_billing_type": "unregistered_type"
                        }]
                    }
                }),
            )
            .expect_err("unregistered Alibaba billing type must fail");
        assert!(error.message.contains("must equal `response_api`"));
    }

    /// Verifies OpenRouter Chat cost is retained as provider-reported USD evidence.
    /// 验证 OpenRouter Chat 成本作为供应商上报的美元证据保留。
    #[test]
    fn openrouter_chat_profile_preserves_reported_usd_cost() {
        // profile is the exact non-streaming OpenRouter Chat extraction contract.
        // profile 是精确的非流式 OpenRouter Chat 提取契约。
        let profile = openrouter_chat_usage_profiles()
            .into_iter()
            .find(|profile| profile.record_kind == UsageRecordKind::JsonBody)
            .expect("OpenRouter Chat JSON profile should exist");
        // observation preserves both the raw cost path and its documented base currency.
        // observation 同时保留原始成本路径及其文档化基础币种。
        let observation = profile
            .extract(
                UsageRecordKind::JsonBody,
                &json!({
                    "id": "chatcmpl-openrouter",
                    "model": "openai/gpt-5",
                    "usage": {
                        "prompt_tokens": 100,
                        "completion_tokens": 20,
                        "total_tokens": 120,
                        "cost": 0.00125
                    }
                }),
            )
            .expect("OpenRouter usage should parse")
            .expect("OpenRouter usage should match");
        assert_eq!(observation.usage.cost.as_deref(), Some("0.00125"));
        assert_eq!(observation.usage.cost_currency.as_deref(), Some("USD"));
        assert_eq!(
            observation.source_paths[&UsageField::ReportedCostUsd],
            "/usage/cost"
        );
    }

    #[test]
    fn cost_remains_unknown_when_usage_or_price_is_missing() {
        let usage = RequestUsage {
            output_tokens: UsageValue::Known {
                value: 500,
                source: UsageValueSource::Reported,
            },
            ..RequestUsage::default()
        };
        let pricing = ModelPricing::from_optional_strings(None, Some("2.5"), None, None)
            .expect("prices should parse");
        let cost = CostCalculator::calculate(&usage, &pricing, Decimal::ONE);
        assert_eq!(cost.output, Some(Decimal::new(125, 5)));
        assert_eq!(cost.input, None);
        assert_eq!(cost.total, None);
    }

    /// Verifies an explicit zero price makes one missing token dimension cost exactly zero.
    /// 验证明确定价为零时，缺失 Token 维度的成本仍可精确确定为零。
    #[test]
    fn explicit_zero_price_does_not_make_total_cost_unknown() {
        let usage = RequestUsage {
            uncached_input_tokens: UsageValue::Known {
                value: 1_000_000,
                source: UsageValueSource::Reported,
            },
            output_tokens: UsageValue::Known {
                value: 500_000,
                source: UsageValueSource::Reported,
            },
            ..RequestUsage::default()
        };
        let pricing =
            ModelPricing::from_optional_strings(Some("1"), Some("2"), Some("0"), Some("0"))
                .expect("prices should parse");
        let cost = CostCalculator::calculate(&usage, &pricing, Decimal::ONE);
        assert_eq!(cost.input, Some(Decimal::ONE));
        assert_eq!(cost.output, Some(Decimal::ONE));
        assert_eq!(cost.cache_read, Some(Decimal::ZERO));
        assert_eq!(cost.cache_write, Some(Decimal::ZERO));
        assert_eq!(cost.total, Some(Decimal::from(2)));
    }

    #[test]
    fn deepseek_profile_uses_only_registered_cache_paths() {
        let profile = deepseek_chat_usage_profiles()
            .into_iter()
            .find(|profile| profile.record_kind == UsageRecordKind::SseEvent)
            .expect("DeepSeek SSE profile should exist");
        let observation = profile
            .extract(
                UsageRecordKind::SseEvent,
                &json!({
                    "id":"response-1",
                    "model":"deepseek-chat",
                    "usage": {
                        "prompt_tokens":100,
                        "prompt_cache_hit_tokens":70,
                        "prompt_cache_miss_tokens":30,
                        "completion_tokens":9,
                        "total_tokens":109
                    }
                }),
            )
            .expect("DeepSeek usage should parse")
            .expect("usage record should match");
        assert_eq!(
            observation.usage.cache_read_tokens,
            UsageValue::Known {
                value: 70,
                source: UsageValueSource::Reported,
            }
        );
        assert_eq!(
            observation.source_paths[&UsageField::UncachedInputTokens],
            "/usage/prompt_cache_miss_tokens"
        );
    }

    /// Verifies Anthropic input and cache dimensions follow their disjoint provider semantics.
    /// 验证 Anthropic 输入与缓存维度遵循供应商定义的互斥语义。
    #[test]
    fn anthropic_body_profile_preserves_disjoint_cache_dimensions() {
        let profile = standard_usage_profiles(crate::UpstreamProtocol::AnthropicMessages)
            .into_iter()
            .find(|profile| profile.record_kind == UsageRecordKind::JsonBody)
            .expect("Anthropic body usage profile should exist");
        let observation = profile
            .extract(
                UsageRecordKind::JsonBody,
                &json!({
                    "id": "msg_1",
                    "model": "claude-sonnet",
                    "usage": {
                        "input_tokens": 20,
                        "cache_read_input_tokens": 80,
                        "cache_creation_input_tokens": 5,
                        "cache_creation": {
                            "ephemeral_5m_input_tokens": 2,
                            "ephemeral_1h_input_tokens": 3
                        },
                        "output_tokens": 7
                    }
                }),
            )
            .expect("Anthropic body usage should parse")
            .expect("Anthropic body profile should match");

        assert_eq!(
            observation.usage.uncached_input_tokens,
            UsageValue::Known {
                value: 20,
                source: UsageValueSource::Derived,
            }
        );
        assert_eq!(
            observation.usage.cache_read_tokens,
            UsageValue::Known {
                value: 80,
                source: UsageValueSource::Reported,
            }
        );
        assert_eq!(
            observation.usage.cache_write_tokens,
            UsageValue::Known {
                value: 5,
                source: UsageValueSource::Reported,
            }
        );
        assert_eq!(
            observation.usage.input_tokens,
            UsageValue::Known {
                value: 105,
                source: UsageValueSource::Derived,
            }
        );
        assert_eq!(
            observation.source_paths[&UsageField::InputTokens],
            "derived:uncached_input_tokens+cache_read_tokens+cache_write_tokens"
        );
    }

    /// Verifies each frozen Interactions revision selects exactly one source-path profile.
    /// 验证每个冻结的 Interactions 修订版只会选中一个来源路径 Profile。
    #[test]
    fn interactions_usage_profiles_keep_official_and_legacy_paths_distinct() {
        let profiles = standard_usage_profiles(crate::UpstreamProtocol::GeminiInteractions);
        let official = json!({
            "event_type":"finish",
            "metadata":{"total_usage":{
                "total_input_tokens":2,
                "total_output_tokens":6,
                "total_thought_tokens":3,
                "total_cached_tokens":1,
                "total_tokens":11
            }}
        });
        let legacy = json!({
            "type":"interaction.completed",
            "interaction":{"id":"interaction-1","usage":{
                "input_tokens":2,
                "output_tokens":6,
                "total_tokens":8
            }}
        });

        let official_matches = profiles
            .iter()
            .filter_map(|profile| {
                profile
                    .extract(UsageRecordKind::SseEvent, &official)
                    .expect("official profile extraction should remain deterministic")
            })
            .collect::<Vec<_>>();
        let legacy_matches = profiles
            .iter()
            .filter_map(|profile| {
                profile
                    .extract(UsageRecordKind::SseEvent, &legacy)
                    .expect("legacy profile extraction should remain deterministic")
            })
            .collect::<Vec<_>>();

        assert_eq!(official_matches.len(), 1);
        assert_eq!(legacy_matches.len(), 1);
        assert_eq!(
            official_matches[0].source_paths[&UsageField::ReasoningTokens],
            "/metadata/total_usage/total_thought_tokens"
        );
        assert_eq!(
            legacy_matches[0].source_paths[&UsageField::InputTokens],
            "/interaction/usage/input_tokens"
        );
    }
}
