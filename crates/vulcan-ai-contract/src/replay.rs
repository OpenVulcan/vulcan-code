//! Durable provider-neutral assistant replay state.
//! 可持久化的供应商无关助手重放状态。

use crate::{AiMessagePhase, ContentItem};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Wraps one opaque provider continuation with an explicit closed provenance prefix.
/// 使用显式封闭来源前缀封装一项不透明供应商延续载荷。
///
/// # Parameters
/// 参数
/// `provider` is the closed protocol family and `payload` is its opaque continuation value.
/// `provider` 是封闭协议族，`payload` 是其不透明延续值。
///
/// # Returns
/// 返回值
/// A replay-safe value, or `None` when either component violates the delimiter contract.
/// 返回可安全重放的值；任一部分违反分隔符契约时返回 `None`。
pub fn wrap_provider_continuation(provider: &str, payload: &str) -> Option<String> {
    let provider = provider.trim();
    let payload = payload.trim();
    if provider.is_empty() || payload.is_empty() || provider.contains('#') || payload.contains('#')
    {
        return None;
    }
    Some(format!("{provider}#{payload}"))
}

/// Unwraps one opaque continuation only for its exact owning provider family.
/// 仅针对其精确归属供应商协议族解封一项不透明延续载荷。
///
/// # Parameters
/// 参数
/// `provider` is the expected protocol family and `signature` is the normalized stored value.
/// `provider` 是预期协议族，`signature` 是归一化存储值。
///
/// # Returns
/// 返回值
/// The original opaque payload when provenance matches exactly.
/// 来源精确匹配时返回原始不透明载荷。
pub fn provider_continuation_payload<'a>(provider: &str, signature: &'a str) -> Option<&'a str> {
    let (source, payload) = signature.split_once('#')?;
    let payload = payload.trim();
    (source == provider && !payload.is_empty() && !payload.contains('#')).then_some(payload)
}

/// Current durable replay envelope format version.
/// 当前可持久化重放封套格式版本。
pub const PROVIDER_REPLAY_ENVELOPE_VERSION: u16 = 4;

/// Preserves one provider message phase without exposing private provider context.
/// 在不暴露供应商私有上下文的情况下保存一项供应商消息阶段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderMessagePhaseSnapshot {
    /// Exact public response item identifier owning the phase.
    /// 持有该阶段的精确公共响应项标识。
    pub item_id: String,
    /// Closed assistant message phase.
    /// 封闭的助手消息阶段。
    pub phase: AiMessagePhase,
}

/// Canonicalizes official Codex Responses input aliases before durable replay validation.
/// 在可持久化重放校验前规范化官方 Codex Responses 输入别名。
///
/// # Parameters
/// 参数
/// `items` is the ordered output returned by the standalone Responses compact endpoint.
/// `items` 是独立 Responses 压缩端点返回的有序输出。
pub fn canonicalize_codex_responses_replay_items(items: &mut [Value]) {
    for item in items {
        if item.get("type").and_then(Value::as_str) != Some("compaction_summary") {
            continue;
        }
        // object is the exact provider item whose official legacy alias must serialize canonically on replay.
        // object 是必须在重放时按规范值序列化的官方旧别名供应商项。
        if let Some(object) = item.as_object_mut() {
            object.insert(
                String::from("type"),
                Value::String(String::from("compaction")),
            );
        }
    }
}

/// Returns whether one official Codex Responses item can be preserved and replayed as input.
/// 返回一个官方 Codex Responses 项是否可以保存并作为输入重放。
///
/// # Parameters
/// 参数
/// `value` is the provider-produced JSON item from a compact response.
/// `value` 是 compact 响应中的供应商 JSON 项。
///
/// # Returns
/// 返回值
/// `true` only for the current official Codex `ResponseItem` variants that are valid history
/// items; request-only controls and unknown variants are rejected before persistence.
/// 仅当它属于官方 Codex `ResponseItem` 中可作为历史项的变体时返回 `true`；请求控制项与未知
/// 变体会在持久化前被拒绝。
pub fn is_replayable_codex_responses_item(value: &Value) -> bool {
    match value.get("type").and_then(Value::as_str) {
        // A compaction item without its required opaque payload cannot continue an official Codex session.
        // 缺少必需不透明载荷的压缩项无法续写官方 Codex 会话。
        Some("compaction") => value
            .get("encrypted_content")
            .and_then(Value::as_str)
            .is_some_and(|encrypted_content| !encrypted_content.is_empty()),
        // Reasoning is replayable only when it contains full content or an opaque continuation;
        // a summary-only item is transient presentation state.
        // 推理项仅在包含完整正文或不透明延续时可重放；只有摘要的项目属于瞬态展示状态。
        Some("reasoning") => {
            value
                .get("content")
                .and_then(Value::as_array)
                .is_some_and(|content| !content.is_empty())
                || value
                    .get("encrypted_content")
                    .and_then(Value::as_str)
                    .is_some_and(|encrypted_content| !encrypted_content.is_empty())
        }
        Some(
            "additional_tools"
            | "message"
            | "agent_message"
            | "local_shell_call"
            | "function_call"
            | "tool_search_call"
            | "function_call_output"
            | "custom_tool_call"
            | "custom_tool_call_output"
            | "tool_search_output"
            | "web_search_call"
            | "image_generation_call"
            | "context_compaction",
        ) => true,
        _ => false,
    }
}

/// Preserves ordered assistant content together with its exact protocol scope.
/// 在精确协议作用域内保存有序助手内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderReplayEnvelope {
    /// Closed envelope format version.
    /// 封闭封套格式版本。
    pub version: u16,
    /// Provider definition identifier that produced the content.
    /// 产生内容的供应商定义标识。
    pub provider_id: String,
    /// Credential connection identifier that produced the content.
    /// 产生内容的凭据连接标识。
    #[serde(default)]
    pub connection_id: String,
    /// Non-secret resolved request URL that produced the content.
    /// 产生内容的不含秘密的已解析请求 URL。
    #[serde(default)]
    pub route_fingerprint: String,
    /// Protocol profile identifier that produced provider-specific signatures.
    /// 产生供应商专属签名的协议 Profile 标识。
    pub protocol_profile_id: String,
    /// Upstream model identifier that produced the content.
    /// 产生内容的上游模型标识。
    pub model_id: String,
    /// Ordered provider-neutral assistant content.
    /// 有序的供应商无关助手内容。
    pub content: Vec<ContentItem>,
    /// Ordered exact Responses output items for the bound physical route.
    /// 绑定物理路由使用的有序 Responses 原始输出项。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_context_items: Vec<Value>,
    /// Typed message phases retained separately from opaque provider items.
    /// 与不透明供应商项目分开保存的类型化消息阶段。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub message_phases: Vec<ProviderMessagePhaseSnapshot>,
}

impl ProviderReplayEnvelope {
    /// Creates one versioned replay envelope for an exact execution route.
    /// 为一条精确执行路由创建一个带版本的重放封套。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `connection_id`, `route_fingerprint`, `protocol_profile_id`, and `model_id`
    /// define the immutable route scope; `content` contains provider-neutral assistant output and
    /// `provider_context_items` contains exact Responses output items in original output order.
    /// `provider_id`、`connection_id`、`route_fingerprint`、`protocol_profile_id` 与 `model_id`
    /// 定义不可变路由作用域；`content` 按原始流顺序保存供应商无关助手输出，
    /// `provider_context_items` 按原始输出顺序保存 Responses 原始输出项。
    ///
    /// # Returns
    /// 返回值
    /// A replay envelope using the current closed format version.
    /// 返回使用当前封闭格式版本的重放封套。
    pub fn new(
        provider_id: impl Into<String>,
        connection_id: impl Into<String>,
        route_fingerprint: impl Into<String>,
        protocol_profile_id: impl Into<String>,
        model_id: impl Into<String>,
        content: Vec<ContentItem>,
        provider_context_items: Vec<Value>,
    ) -> Self {
        Self {
            version: PROVIDER_REPLAY_ENVELOPE_VERSION,
            provider_id: provider_id.into(),
            connection_id: connection_id.into(),
            route_fingerprint: route_fingerprint.into(),
            protocol_profile_id: protocol_profile_id.into(),
            model_id: model_id.into(),
            content,
            provider_context_items,
            message_phases: Vec::new(),
        }
    }

    /// Attaches validated message phases observed for this exact provider response.
    /// 附加从当前精确供应商响应观察到的已校验消息阶段。
    ///
    /// # Parameters
    /// 参数
    /// `message_phases` preserves response item identity and closed phase values in stream order.
    /// `message_phases` 按流顺序保存响应项身份与封闭阶段值。
    ///
    /// # Returns
    /// 返回值
    /// The replay envelope with typed message-phase metadata attached.
    /// 返回附带类型化消息阶段元数据的重放封套。
    pub fn with_message_phases(
        mut self,
        message_phases: Vec<ProviderMessagePhaseSnapshot>,
    ) -> Self {
        self.message_phases = message_phases;
        self
    }

    /// Verifies that replay state belongs to the exact active execution route.
    /// 验证重放状态属于当前精确执行路由。
    ///
    /// # Parameters
    /// 参数
    /// The five identifiers are the active provider, credential connection, resolved request URL,
    /// protocol profile, and model.
    /// 五个标识分别是当前供应商、凭据连接、已解析请求 URL、协议 Profile 与模型。
    ///
    /// # Returns
    /// 返回值
    /// `true` only for the current version and an exact route match.
    /// 仅当前版本且路由完全匹配时返回 `true`。
    pub fn matches_route(
        &self,
        provider_id: &str,
        connection_id: &str,
        route_fingerprint: &str,
        protocol_profile_id: &str,
        model_id: &str,
    ) -> bool {
        self.version == PROVIDER_REPLAY_ENVELOPE_VERSION
            && self.provider_id == provider_id
            && self.connection_id == connection_id
            && self.route_fingerprint == route_fingerprint
            && self.protocol_profile_id == protocol_profile_id
            && self.model_id == model_id
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ProviderReplayEnvelope, canonicalize_codex_responses_replay_items,
        is_replayable_codex_responses_item,
    };
    use crate::ContentItem;
    use serde_json::json;

    /// Verifies official history item variants are accepted while request-only controls are rejected.
    /// 验证官方历史项变体会被接受，而仅请求控制项会被拒绝。
    #[test]
    fn codex_responses_replay_item_allowlist_is_explicit() {
        assert!(is_replayable_codex_responses_item(&json!({
            "type": "compaction",
            "encrypted_content": "opaque"
        })));
        assert!(!is_replayable_codex_responses_item(&json!({
            "type": "compaction",
            "encrypted_content": ""
        })));
        assert!(!is_replayable_codex_responses_item(&json!({
            "type": "compaction"
        })));
        assert!(is_replayable_codex_responses_item(&json!({
            "type": "message",
            "role": "assistant",
            "content": []
        })));
        assert!(!is_replayable_codex_responses_item(&json!({
            "type": "compaction_trigger"
        })));
        assert!(!is_replayable_codex_responses_item(&json!({
            "type": "provider_private_item"
        })));
    }

    /// Verifies the official `compaction_summary` input alias is stored as canonical `compaction`.
    /// 验证官方 `compaction_summary` 输入别名按规范 `compaction` 值存储。
    #[test]
    fn codex_responses_compaction_alias_is_canonicalized() {
        // items contains the exact alias accepted by the official Codex ResponseItem deserializer.
        // items 包含官方 Codex ResponseItem 反序列化器接受的精确别名。
        let mut items = vec![json!({
            "type": "compaction_summary",
            "encrypted_content": "opaque"
        })];

        canonicalize_codex_responses_replay_items(&mut items);

        assert_eq!(items[0]["type"], "compaction");
        assert!(is_replayable_codex_responses_item(&items[0]));
    }

    /// Verifies ordinary replay signatures are scoped to the exact credential and resolved route.
    /// 验证普通重放签名会限定到精确凭据与已解析路由。
    #[test]
    fn provider_replay_envelope_requires_exact_connection_route() {
        // envelope owns provider-generated content that must not cross credential boundaries.
        // envelope 持有不得跨越凭据边界的供应商生成内容。
        let envelope = ProviderReplayEnvelope::new(
            "custom:test",
            "connection-a",
            "https://proxy.example/v1/responses",
            "openai-codex-responses.v1",
            "gpt-5.6-luna",
            vec![ContentItem::Reasoning {
                text: String::from("reasoning"),
                signature: Some(String::from("opaque-signature")),
            }],
            Vec::new(),
        );

        assert!(envelope.matches_route(
            "custom:test",
            "connection-a",
            "https://proxy.example/v1/responses",
            "openai-codex-responses.v1",
            "gpt-5.6-luna",
        ));
        assert!(!envelope.matches_route(
            "custom:test",
            "connection-b",
            "https://proxy.example/v1/responses",
            "openai-codex-responses.v1",
            "gpt-5.6-luna",
        ));
        assert!(!envelope.matches_route(
            "custom:test",
            "connection-a",
            "https://proxy.example/changed/responses",
            "openai-codex-responses.v1",
            "gpt-5.6-luna",
        ));
    }

    /// Verifies version-one envelopes remain readable but cannot replay without route evidence.
    /// 验证第一版封套仍可读取，但缺少路由证据时不得重放。
    #[test]
    fn legacy_provider_replay_envelope_falls_back_safely() {
        // legacy is the exact previously persisted shape without credential or URL identity.
        // legacy 是之前持久化且不含凭据与 URL 身份的精确结构。
        let legacy: ProviderReplayEnvelope = serde_json::from_value(json!({
            "version": 1,
            "provider_id": "custom:test",
            "protocol_profile_id": "openai-codex-responses.v1",
            "model_id": "gpt-5.6-luna",
            "content": []
        }))
        .expect("legacy replay envelope should remain readable");

        assert!(legacy.connection_id.is_empty());
        assert!(legacy.route_fingerprint.is_empty());
        assert!(!legacy.matches_route(
            "custom:test",
            "connection-a",
            "https://proxy.example/v1/responses",
            "openai-codex-responses.v1",
            "gpt-5.6-luna",
        ));
    }
}
