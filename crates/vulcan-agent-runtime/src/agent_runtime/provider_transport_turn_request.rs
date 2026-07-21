//! Host-controlled single-turn request assembly helpers reused by provider-specific streaming adapters.
//! 供各 Provider 专用流式适配器复用的宿主控制单回合请求装配辅助逻辑。

use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_request_turn_snapshot::build_openai_compatible_turn_request_snapshot;
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::provider_config::ModelInterleavedCapability;
use reqwest::header::HeaderMap;
use serde_json::Value;
use std::collections::BTreeMap;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationMessage as CoreConversationMessage,
    ConversationPayload as CoreConversationPayload,
    ConversationSnapshot as CoreConversationSnapshot, CoreError as VulcanCoreError,
    ProviderContextInjection as CoreProviderContextInjection,
    ProviderPromptLayers as CoreProviderPromptLayers,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
};

/// Serializable OpenAI-compatible single-turn request owned by `vulcan-core` while preserving provider-visible canonical messages for the final transport adapter.
/// 由 `vulcan-core` 持有的可序列化 OpenAI 兼容单回合请求，同时为最终传输适配器保留 Provider 可见的规范消息。
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct OpenAiCompatibleCoreTurnRequest {
    /// Structured request snapshot persisted for inspection and debugging.
    /// 被持久化用于检查与调试的结构化请求快照。
    #[serde(flatten)]
    pub(crate) snapshot: CoreProviderRequestSnapshot,
    /// Provider-visible history kept in canonical core shape for the final transport adapter.
    /// 为最终传输适配器保留的规范 core 形状 Provider 可见历史。
    #[serde(skip_serializing)]
    pub(crate) current_history: Vec<CoreConversationMessage>,
    /// Last outgoing prompt message kept in canonical core shape for the final transport adapter.
    /// 为最终传输适配器保留的规范 core 形状最后一条出站提示消息。
    #[serde(skip_serializing)]
    pub(crate) current_prompt: CoreConversationMessage,
    /// Optional model-scoped interleaved reasoning capability reused by OpenAI-compatible replay code.
    /// 供 OpenAI 兼容回放逻辑复用的可选模型级交错推理能力描述。
    #[serde(skip_serializing)]
    pub(crate) interleaved_reasoning: Option<ModelInterleavedCapability>,
}

/// Builds one exact host-controlled turn request from the canonical controller snapshot and forwards the structured snapshot immediately.
/// 根据规范控制器快照构建一份精确宿主控制回合请求，并立即转发结构化快照。
pub(crate) fn build_host_controlled_turn_request(
    provider_id: &str,
    model_id: &str,
    request_label: &str,
    snapshot: &CoreConversationSnapshot,
    config: &CoreControllerConfig,
    provider_tool_definitions: &[ProviderToolDefinition],
    request_headers: &HeaderMap,
    live_sender: &ProviderLiveDeltaSender,
    additional_params: Option<Value>,
    interleaved_reasoning: Option<ModelInterleavedCapability>,
) -> Result<OpenAiCompatibleCoreTurnRequest, VulcanCoreError> {
    let Some((current_prompt_message, current_history_messages)) = snapshot.messages.split_last()
    else {
        return Err(VulcanCoreError::InvalidState {
            message: format!(
                "{request_label} 单回合请求构建失败：规范消息窗口为空，无法解析最后一条出站提示消息。"
            ),
        });
    };
    let mut current_history = current_history_messages.to_vec();
    let mut current_prompt = current_prompt_message.clone();
    apply_context_injection_overlay(
        &mut current_history,
        &mut current_prompt,
        config.context_injection.as_ref(),
    )?;
    let provider_system_prompt = provider_system_prompt_with_persistent_injection(config);
    let provider_prompt_layers = provider_prompt_layers_with_persistent_injection(config);
    let request_snapshot = build_openai_compatible_turn_request_snapshot(
        provider_id,
        model_id,
        provider_system_prompt.as_str(),
        &snapshot.continuation,
        &current_history,
        &current_prompt,
        config.temperature,
        config.max_turns,
        provider_tool_definitions,
        request_headers,
        additional_params,
    );
    let request_snapshot = CoreProviderRequestSnapshot {
        prompt_layers: provider_prompt_layers,
        ..request_snapshot
    };
    let _ = live_sender.send(Ok(ProviderStreamDelta::RequestSnapshot(
        request_snapshot.clone(),
    )));

    Ok(OpenAiCompatibleCoreTurnRequest {
        snapshot: request_snapshot,
        current_history,
        current_prompt,
        interleaved_reasoning,
    })
}

/// Builds the provider-facing system prompt with persistent injection rendered into the system layer.
/// 构建 Provider 可见系统提示词，并将长期注入渲染进系统层。
fn provider_system_prompt_with_persistent_injection(config: &CoreControllerConfig) -> String {
    config
        .provider_prompt_layers_with_persistent_injection()
        .render_or_legacy(config.system_prompt.as_str())
}

/// Builds request-preview prompt layers with persistent injection carried by runtime context.
/// 构建请求预览提示词层，并通过运行时上下文携带长期注入。
fn provider_prompt_layers_with_persistent_injection(
    config: &CoreControllerConfig,
) -> CoreProviderPromptLayers {
    config.provider_prompt_layers_with_persistent_injection()
}

/// Applies the provider-facing injection overlay to the outgoing request view without mutating canonical history.
/// 将 Provider 可见注入覆盖层应用到出站请求视图，同时不修改规范历史。
fn apply_context_injection_overlay(
    current_history: &mut [CoreConversationMessage],
    current_prompt: &mut CoreConversationMessage,
    injection: Option<&CoreProviderContextInjection>,
) -> Result<(), VulcanCoreError> {
    let Some(injection) = injection.filter(|injection| !injection.is_empty()) else {
        return Ok(());
    };
    let mut prefixes_by_target = BTreeMap::<String, Vec<String>>::new();
    for anchor in &injection.anchored_prefixes {
        if anchor.anchor_id.trim().is_empty()
            || anchor.target_message_id.trim().is_empty()
            || anchor.prefix.trim().is_empty()
        {
            return Err(VulcanCoreError::InvalidState {
                message: String::from(
                    "Provider context anchor requires non-empty anchor id, target message id, and prefix.",
                ),
            });
        }
        prefixes_by_target
            .entry(anchor.target_message_id.clone())
            .or_default()
            .push(anchor.prefix.trim().to_string());
    }
    if injection.has_transient()
        && let Some(target_message_id) =
            logical_turn_user_message_id(current_history, current_prompt)
    {
        prefixes_by_target
            .entry(target_message_id)
            .or_default()
            .push(injection.transient_prefix.trim().to_string());
    }
    for (target_message_id, prefixes) in prefixes_by_target {
        prefix_exact_user_message(
            current_history,
            current_prompt,
            &target_message_id,
            &prefixes.join("\n\n"),
        )?;
    }
    Ok(())
}

/// Resolves the exact user-message id owning a generic transient logical-turn prefix.
/// 解析拥有通用临时逻辑回合前缀的精确用户消息标识。
///
/// # Parameters
/// 参数
/// `current_history` is the provider-visible history preceding the current prompt.
/// `current_history` 是当前提示消息之前的 Provider 可见历史。
/// `current_prompt` is the last outgoing canonical message.
/// `current_prompt` 是最后一条出站规范消息。
///
/// # Returns
/// 返回值
/// The current user prompt id, or the latest historical user id during a tool loop.
/// 返回当前用户提示消息标识；工具循环期间返回最近一条历史用户消息标识。
fn logical_turn_user_message_id(
    current_history: &[CoreConversationMessage],
    current_prompt: &CoreConversationMessage,
) -> Option<String> {
    if matches!(
        current_prompt.payload,
        CoreConversationPayload::UserText { .. }
    ) {
        return Some(current_prompt.id.clone());
    }
    current_history
        .iter()
        .rev()
        .filter(|message| !message.id.starts_with("context-injection-"))
        .find(|message| matches!(message.payload, CoreConversationPayload::UserText { .. }))
        .map(|message| message.id.clone())
}

/// Prefixes exactly one uniquely identified canonical user message without positional fallback.
/// 在不进行位置回退的情况下前置唯一标识的一条规范用户消息。
///
/// # Parameters
/// 参数
/// `current_history` is the mutable provider-visible history clone.
/// `current_history` 是可变 Provider 可见历史克隆。
/// `current_prompt` is the mutable provider-visible current prompt clone.
/// `current_prompt` 是可变 Provider 可见当前提示消息克隆。
/// `target_message_id` is the exact canonical message id declared by the anchor.
/// `target_message_id` 是锚点声明的精确规范消息标识。
/// `prefix` is the combined immutable prefix for that exact target.
/// `prefix` 是该精确目标对应的合并不可变前缀。
///
/// # Returns
/// 返回值
/// Success only for one unique user-text target; missing, duplicate, or wrong-role targets fail.
/// 仅当目标唯一且为用户文本时成功；缺失、重复或角色错误均失败。
fn prefix_exact_user_message(
    current_history: &mut [CoreConversationMessage],
    current_prompt: &mut CoreConversationMessage,
    target_message_id: &str,
    prefix: &str,
) -> Result<(), VulcanCoreError> {
    let prompt_matches = usize::from(current_prompt.id == target_message_id);
    let history_matches = current_history
        .iter()
        .enumerate()
        .filter(|(_, message)| message.id == target_message_id)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let match_count = prompt_matches.saturating_add(history_matches.len());
    if match_count != 1 {
        return Err(VulcanCoreError::InvalidState {
            message: format!(
                "Provider context anchor target `{target_message_id}` matched {match_count} canonical messages; exactly one is required."
            ),
        });
    }
    let prefixed = if prompt_matches == 1 {
        prefix_user_text_message(current_prompt, prefix)
    } else {
        prefix_user_text_message(&mut current_history[history_matches[0]], prefix)
    };
    if !prefixed {
        return Err(VulcanCoreError::InvalidState {
            message: format!(
                "Provider context anchor target `{target_message_id}` is not a canonical user-text message."
            ),
        });
    }
    Ok(())
}

/// Prefixes one outgoing user-text message with injection text.
/// 使用注入文本前置一条出站用户文本消息。
fn prefix_user_text_message(message: &mut CoreConversationMessage, prefix: &str) -> bool {
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return false;
    }
    let CoreConversationPayload::UserText { text, .. } = &mut message.payload else {
        return false;
    };
    let trimmed_text = text.trim();
    *text = if trimmed_text.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}\n\n<user_prompt>\n{trimmed_text}\n</user_prompt>")
    };
    true
}

#[cfg(test)]
mod tests {
    //! Context-injection overlay tests for provider turn request assembly.
    //! Provider 回合请求装配的上下文注入覆盖层测试。

    use super::*;

    /// Builds one canonical user-text message for overlay tests.
    /// 为覆盖层测试构建一条规范用户文本消息。
    fn user_message(id: &str, text: &str) -> CoreConversationMessage {
        CoreConversationMessage {
            id: id.to_string(),
            payload: CoreConversationPayload::UserText {
                text: text.to_string(),
                attachments: Vec::new(),
            },
        }
    }

    /// Builds one canonical assistant message for overlay tests.
    /// 为覆盖层测试构建一条规范助手消息。
    fn assistant_message(id: &str) -> CoreConversationMessage {
        CoreConversationMessage {
            id: id.to_string(),
            payload: CoreConversationPayload::Assistant {
                text: String::from("tool call"),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: Vec::new(),
            },
        }
    }

    #[test]
    fn transient_prefix_targets_current_user_prompt() {
        let mut history = vec![assistant_message("assistant-1")];
        let mut prompt = user_message("user-2", "真正问题");
        let injection = CoreProviderContextInjection {
            persistent_blocks: Vec::new(),
            transient_prefix: String::from("<manual_inject>\n隐藏内容\n</manual_inject>"),
            fingerprint: String::from("abc"),
            ..CoreProviderContextInjection::default()
        };

        apply_context_injection_overlay(&mut history, &mut prompt, Some(&injection))
            .expect("transient overlay should target the current user prompt");

        let CoreConversationPayload::UserText { text, .. } = prompt.payload else {
            panic!("prompt should stay user text");
        };
        assert!(text.starts_with("<manual_inject>"));
        assert!(text.contains("<user_prompt>\n真正问题\n</user_prompt>"));
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn tool_loop_overlay_prefixes_original_user_without_breaking_tool_prompt() {
        let mut history = vec![
            user_message("user-1", "原始问题"),
            assistant_message("assistant-1"),
        ];
        let mut prompt = CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_call_id: Some(String::from("tool-call-1")),
                text: String::from("工具返回"),
            },
        };
        let injection = CoreProviderContextInjection {
            persistent_blocks: vec![String::from("<persistent>\n固定内容\n</persistent>")],
            transient_prefix: String::from("<manual_inject>\n临时内容\n</manual_inject>"),
            fingerprint: String::from("def"),
            ..CoreProviderContextInjection::default()
        };

        apply_context_injection_overlay(&mut history, &mut prompt, Some(&injection))
            .expect("tool-loop overlay should target the latest user prompt");

        let CoreConversationPayload::UserText { text, .. } = &history[0].payload else {
            panic!("first history item should stay user text");
        };
        assert!(!text.contains("<persistent>"));
        assert!(text.contains("<manual_inject>"));
        assert!(matches!(
            prompt.payload,
            CoreConversationPayload::ToolResult { .. }
        ));
    }

    #[test]
    fn persistent_injection_renders_into_system_prompt() {
        let config = CoreControllerConfig {
            system_prompt: String::from("base"),
            context_injection: Some(CoreProviderContextInjection {
                persistent_blocks: vec![String::from("<persistent>\n固定内容\n</persistent>")],
                transient_prefix: String::new(),
                fingerprint: String::from("p"),
                ..CoreProviderContextInjection::default()
            }),
            ..CoreControllerConfig::default()
        };

        let system_prompt = provider_system_prompt_with_persistent_injection(&config);
        let prompt_layers = provider_prompt_layers_with_persistent_injection(&config);

        assert!(system_prompt.starts_with("base"));
        assert!(system_prompt.contains("<persistent>"));
        assert!(prompt_layers.runtime_context.contains("<persistent>"));
    }

    /// Verifies immutable anchors target exact historical messages and preserve anchor order.
    /// 验证不可变锚点会精确命中历史消息并保持锚点顺序。
    #[test]
    fn anchored_prefixes_target_exact_historical_user_in_stable_order() {
        let mut history = vec![
            user_message("user-1", "first"),
            assistant_message("assistant-1"),
            user_message("user-2", "second"),
        ];
        let mut prompt = assistant_message("assistant-2");
        let injection = CoreProviderContextInjection {
            anchored_prefixes: vec![
                vulcan_core::types::ProviderContextAnchor {
                    anchor_id: String::from("anchor-1"),
                    target_message_id: String::from("user-1"),
                    prefix: String::from("memory-one"),
                },
                vulcan_core::types::ProviderContextAnchor {
                    anchor_id: String::from("anchor-2"),
                    target_message_id: String::from("user-1"),
                    prefix: String::from("memory-two"),
                },
            ],
            ..CoreProviderContextInjection::default()
        };

        apply_context_injection_overlay(&mut history, &mut prompt, Some(&injection))
            .expect("exact historical anchor should apply");

        let CoreConversationPayload::UserText { text, .. } = &history[0].payload else {
            panic!("anchored target should stay user text");
        };
        assert!(text.starts_with("memory-one\n\nmemory-two"));
        let CoreConversationPayload::UserText { text, .. } = &history[2].payload else {
            panic!("unrelated history message should stay user text");
        };
        assert_eq!(text, "second");
    }

    /// Verifies exact anchors reject missing and wrong-role canonical targets.
    /// 验证精确锚点会拒绝缺失目标与错误角色目标。
    #[test]
    fn anchored_prefix_rejects_missing_or_non_user_target() {
        let injection_for = |target_message_id: &str| CoreProviderContextInjection {
            anchored_prefixes: vec![vulcan_core::types::ProviderContextAnchor {
                anchor_id: String::from("anchor-1"),
                target_message_id: target_message_id.to_string(),
                prefix: String::from("memory"),
            }],
            ..CoreProviderContextInjection::default()
        };
        let mut history = vec![assistant_message("assistant-1")];
        let mut prompt = user_message("user-2", "question");

        let missing = apply_context_injection_overlay(
            &mut history,
            &mut prompt,
            Some(&injection_for("missing")),
        );
        assert!(missing.is_err());

        let wrong_role = apply_context_injection_overlay(
            &mut history,
            &mut prompt,
            Some(&injection_for("assistant-1")),
        );
        assert!(wrong_role.is_err());
    }

    /// Verifies 20KB and 120KB histories keep the first VMM-anchored user message byte-identical across later requests and tool loops.
    /// 验证 20KB 与 120KB 历史中的首个 VMM 锚定用户消息在后续请求及工具循环中保持逐字节一致。
    #[test]
    fn long_history_anchor_remains_byte_stable_across_turns_and_tool_loops() {
        for history_bytes in [20 * 1024, 120 * 1024] {
            // Large history content models the cache-sensitive prefix that must remain untouched.
            // 大体积历史内容模拟必须保持不变的缓存敏感前缀。
            let large_history_text = "h".repeat(history_bytes);
            let injection = CoreProviderContextInjection {
                anchored_prefixes: vec![vulcan_core::types::ProviderContextAnchor {
                    anchor_id: String::from("anchor-memory-1"),
                    target_message_id: String::from("user-anchor"),
                    prefix: String::from("<VMM_MEMORY>stable-memory</VMM_MEMORY>"),
                }],
                mutation_kind: vulcan_core::types::ProviderContextMutationKind::AppendOnly,
                cache_epoch_id: Some(String::from("epoch-long-history")),
                anchor_revision: Some(1),
                ..CoreProviderContextInjection::default()
            };
            let mut first_history = vec![
                user_message("user-large-history", &large_history_text),
                assistant_message("assistant-before-anchor"),
            ];
            let mut first_prompt = user_message("user-anchor", "anchored-question");
            apply_context_injection_overlay(
                &mut first_history,
                &mut first_prompt,
                Some(&injection),
            )
            .expect("first request should anchor the current user message");
            // The first rendered prompt becomes the byte-stability reference for every later request.
            // 首次渲染提示词成为全部后续请求的逐字节稳定性基准。
            let CoreConversationPayload::UserText {
                text: first_anchored_text,
                ..
            } = &first_prompt.payload
            else {
                panic!("first anchored prompt should remain user text");
            };

            let mut next_history = vec![
                user_message("user-large-history", &large_history_text),
                assistant_message("assistant-before-anchor"),
                user_message("user-anchor", "anchored-question"),
                assistant_message("assistant-after-anchor"),
            ];
            let mut next_prompt = user_message("user-next", "next-question");
            apply_context_injection_overlay(&mut next_history, &mut next_prompt, Some(&injection))
                .expect("next turn should replay the anchor on its original historical message");
            let CoreConversationPayload::UserText {
                text: replayed_anchored_text,
                ..
            } = &next_history[2].payload
            else {
                panic!("replayed anchor target should remain user text");
            };
            assert_eq!(replayed_anchored_text, first_anchored_text);
            let CoreConversationPayload::UserText {
                text: preserved_large_history,
                ..
            } = &next_history[0].payload
            else {
                panic!("large history target should remain user text");
            };
            assert_eq!(preserved_large_history, &large_history_text);

            let mut tool_history = vec![
                user_message("user-large-history", &large_history_text),
                assistant_message("assistant-before-anchor"),
                user_message("user-anchor", "anchored-question"),
                assistant_message("assistant-after-anchor"),
            ];
            let mut tool_prompt = CoreConversationMessage {
                id: String::from("tool-result-later"),
                payload: CoreConversationPayload::ToolResult {
                    tool_call_id: Some(String::from("tool-call-later")),
                    text: String::from("tool output"),
                },
            };
            apply_context_injection_overlay(&mut tool_history, &mut tool_prompt, Some(&injection))
                .expect("tool-loop request should retain the same historical anchor");
            let CoreConversationPayload::UserText {
                text: tool_loop_anchored_text,
                ..
            } = &tool_history[2].payload
            else {
                panic!("tool-loop anchor target should remain user text");
            };
            assert_eq!(tool_loop_anchored_text, first_anchored_text);
        }
    }
}
