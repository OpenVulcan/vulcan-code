//! Canonical conversation-message bridge helpers reused by snapshot assembly and provider-visible history projection.
//! 供快照组装与 Provider 可见历史投影复用的规范会话消息桥接辅助逻辑。

use crate::conversation_protocol::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationToolCall,
};
use crate::provider_capabilities::ResolvedModelCapabilities;
use crate::provider_config::ProviderApiFamily;
use crate::provider_input_normalizer::normalize_provider_user_input_for_model;
use std::collections::BTreeSet;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall,
};

/// Converts one session-owned canonical conversation message into the `vulcan-core` message shape.
/// 将一条会话持有的规范会话消息转换为 `vulcan-core` 消息形状。
pub(crate) fn session_message_to_core_message(
    message: &SessionConversationMessage,
) -> CoreConversationMessage {
    let payload = match &message.payload {
        SessionConversationPayload::UserText { text, attachments } => {
            CoreConversationPayload::UserText {
                text: text.clone(),
                attachments: attachments.clone(),
            }
        }
        SessionConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_id,
            provider_message_state,
            usage,
            tool_calls,
        } => CoreConversationPayload::Assistant {
            text: text.clone(),
            reasoning: reasoning.clone(),
            provider_message_id: provider_message_id.clone(),
            provider_message_state: provider_message_state.clone(),
            usage: usage.clone(),
            tool_calls: tool_calls
                .iter()
                .map(session_tool_call_to_core_tool_call)
                .collect(),
        },
        SessionConversationPayload::ToolResult {
            tool_result_id,
            tool_call_id,
            text,
        } => CoreConversationPayload::ToolResult {
            tool_result_id: tool_result_id.clone(),
            tool_call_id: tool_call_id.clone(),
            text: text.clone(),
        },
    };

    CoreConversationMessage {
        id: message.id.clone(),
        payload,
    }
}

/// Converts one `vulcan-core` message back into the GUI-owned canonical session message shape.
/// 将一条 `vulcan-core` 消息转换回 GUI 持有的规范会话消息形状。
pub(crate) fn core_message_to_session_message(
    message: &CoreConversationMessage,
) -> SessionConversationMessage {
    let payload = match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            SessionConversationPayload::UserText {
                text: text.clone(),
                attachments: attachments.clone(),
            }
        }
        CoreConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_id,
            provider_message_state,
            usage,
            tool_calls,
        } => SessionConversationPayload::Assistant {
            text: text.clone(),
            reasoning: reasoning.clone(),
            provider_message_id: provider_message_id.clone(),
            provider_message_state: provider_message_state.clone(),
            usage: usage.clone(),
            tool_calls: tool_calls
                .iter()
                .map(core_tool_call_to_session_tool_call)
                .collect(),
        },
        CoreConversationPayload::ToolResult {
            tool_result_id,
            tool_call_id,
            text,
        } => SessionConversationPayload::ToolResult {
            tool_result_id: tool_result_id.clone(),
            tool_call_id: tool_call_id.clone(),
            text: text.clone(),
        },
    };

    SessionConversationMessage {
        id: message.id.clone(),
        payload,
    }
}

/// Converts canonical core messages into the provider-visible history that will actually be sent upstream.
/// 将规范 core 消息转换为真正会发送给上游 Provider 的可见历史。
pub(crate) fn core_messages_to_provider_visible_history(
    messages: &[CoreConversationMessage],
) -> Vec<CoreConversationMessage> {
    core_messages_to_provider_visible_history_for_model(messages, None, None)
}

/// Converts canonical core messages into provider-visible history using API-family and model modality projection.
/// 使用接口族与模型模态投影将规范 core 消息转换为 Provider 可见历史。
pub(crate) fn core_messages_to_provider_visible_history_for_model(
    messages: &[CoreConversationMessage],
    api_family: Option<ProviderApiFamily>,
    capabilities: Option<&ResolvedModelCapabilities>,
) -> Vec<CoreConversationMessage> {
    let mut active_tool_call_ids = BTreeSet::new();
    messages
        .iter()
        .map(|message| {
            let payload = match &message.payload {
                CoreConversationPayload::UserText { text, attachments } => match api_family {
                    Some(api_family) => {
                        active_tool_call_ids.clear();
                        let normalized = normalize_provider_user_input_for_model(
                            api_family,
                            capabilities,
                            text,
                            attachments,
                        );
                        CoreConversationPayload::UserText {
                            text: normalized.text,
                            attachments: normalized.attachments,
                        }
                    }
                    None => {
                        active_tool_call_ids.clear();
                        CoreConversationPayload::UserText {
                            text: text.clone(),
                            attachments: attachments.clone(),
                        }
                    }
                },
                CoreConversationPayload::Assistant {
                    text,
                    reasoning: _,
                    provider_message_id,
                    provider_message_state,
                    usage,
                    tool_calls,
                } => {
                    active_tool_call_ids = replayable_tool_call_ids(tool_calls);
                    CoreConversationPayload::Assistant {
                        text: text.clone(),
                        reasoning: String::new(),
                        provider_message_id: provider_message_id.clone(),
                        provider_message_state: provider_message_state.clone(),
                        usage: usage.clone(),
                        tool_calls: tool_calls.clone(),
                    }
                }
                CoreConversationPayload::ToolResult {
                    tool_result_id,
                    tool_call_id,
                    text,
                } => {
                    let tool_call_key = tool_call_id.as_deref().unwrap_or_default().trim();
                    if !tool_call_key.is_empty() && active_tool_call_ids.remove(tool_call_key) {
                        CoreConversationPayload::ToolResult {
                            tool_result_id: tool_result_id.clone(),
                            tool_call_id: tool_call_id.clone(),
                            text: text.clone(),
                        }
                    } else {
                        CoreConversationPayload::UserText {
                            text: orphan_tool_result_text(tool_result_id, tool_call_id, text),
                            attachments: Vec::new(),
                        }
                    }
                }
            };

            CoreConversationMessage {
                id: message.id.clone(),
                payload,
            }
        })
        .collect()
}

/// Builds all provider-replayable identifiers for one assistant tool-call batch.
/// 为一批助手工具调用构建所有可回放到 Provider 的标识。
fn replayable_tool_call_ids(tool_calls: &[CoreConversationToolCall]) -> BTreeSet<String> {
    tool_calls
        .iter()
        .flat_map(|tool_call| {
            [
                (!tool_call.id.trim().is_empty()).then(|| tool_call.id.clone()),
                tool_call
                    .tool_call_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
            ]
        })
        .flatten()
        .collect()
}

/// Builds a provider-visible plain-text fallback for a tool result whose matching tool call was compacted away.
/// 为匹配工具调用已被压缩移除的工具结果构建 Provider 可见的纯文本回退内容。
fn orphan_tool_result_text(
    tool_result_id: &str,
    tool_call_id: &Option<String>,
    text: &str,
) -> String {
    let call_label = tool_call_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown");
    format!(
        "ERROR: Tool result `{}` cannot be replayed as a tool response because matching assistant tool call `{}` is no longer present in context. Treat the following as historical text only.\n\n{}",
        tool_result_id, call_label, text
    )
}

/// Converts one session-owned canonical tool-call entry into the `vulcan-core` tool-call shape.
/// 将一条会话持有的规范工具调用项转换为 `vulcan-core` 工具调用形状。
fn session_tool_call_to_core_tool_call(
    tool_call: &SessionConversationToolCall,
) -> CoreConversationToolCall {
    CoreConversationToolCall {
        id: tool_call.id.clone(),
        tool_call_id: tool_call.tool_call_id.clone(),
        tool_name: tool_call.tool_name.clone(),
        arguments: tool_call.arguments.clone(),
    }
}

/// Converts one `vulcan-core` tool-call entry back into the GUI-owned canonical tool-call shape.
/// 将一条 `vulcan-core` 工具调用项转换回 GUI 持有的规范工具调用形状。
fn core_tool_call_to_session_tool_call(
    tool_call: &CoreConversationToolCall,
) -> SessionConversationToolCall {
    SessionConversationToolCall {
        id: tool_call.id.clone(),
        tool_call_id: tool_call.tool_call_id.clone(),
        tool_name: tool_call.tool_name.clone(),
        arguments: tool_call.arguments.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::core_messages_to_provider_visible_history_for_model;
    use crate::provider_capabilities::ResolvedModelCapabilities;
    use crate::provider_config::ProviderApiFamily;
    use vulcan_core::types::{
        ConversationAttachment, ConversationAttachmentKind,
        ConversationMessage as CoreConversationMessage,
        ConversationPayload as CoreConversationPayload,
        ConversationToolCall as CoreConversationToolCall,
    };

    /// Builds one replayable attachment payload used by provider-history projection tests.
    /// 构建供 Provider 历史投影测试使用的可回放附件载荷。
    fn replayable_attachment(
        kind: ConversationAttachmentKind,
        file_name: &str,
        mime_type: &str,
    ) -> ConversationAttachment {
        ConversationAttachment {
            kind,
            file_name: file_name.to_string(),
            local_path: std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .join("Cargo.toml")
                .to_string_lossy()
                .to_string(),
            source_path: None,
            asset_id: Some(format!("asset-{file_name}")),
            byte_len: Some(4),
            mime_type: mime_type.to_string(),
        }
    }

    /// Verifies historical attachments are downgraded before transport encoding when the selected model is text-only.
    /// 验证历史附件会在传输编码前根据纯文本模型能力降级。
    #[test]
    fn provider_visible_history_downgrades_model_unsupported_attachments() {
        let capabilities = ResolvedModelCapabilities {
            input_modalities: vec![String::from("text")],
            ..ResolvedModelCapabilities::default()
        };
        let messages = vec![CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("继续分析"),
                attachments: vec![replayable_attachment(
                    ConversationAttachmentKind::Image,
                    "demo.png",
                    "image/png",
                )],
            },
        }];

        let projected = core_messages_to_provider_visible_history_for_model(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            Some(&capabilities),
        );

        let CoreConversationPayload::UserText { text, attachments } = &projected[0].payload else {
            panic!("expected projected user text");
        };
        assert!(attachments.is_empty());
        assert!(text.contains("selected model"));
    }

    /// Verifies matched tool results remain protocol tool responses in provider-visible history.
    /// 验证已匹配的工具结果在 Provider 可见历史中仍保持协议工具响应形态。
    #[test]
    fn provider_visible_history_preserves_matched_tool_result() {
        let messages = vec![
            CoreConversationMessage {
                id: String::from("assistant-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::from("local reasoning"),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: vec![CoreConversationToolCall {
                        id: String::from("local-call"),
                        tool_call_id: Some(String::from("provider-call")),
                        tool_name: String::from("read_file"),
                        arguments: serde_json::json!({"path": "Cargo.toml"}),
                    }],
                },
            },
            CoreConversationMessage {
                id: String::from("tool-result-1"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-1"),
                    tool_call_id: Some(String::from("provider-call")),
                    text: String::from("file body"),
                },
            },
        ];

        let projected = core_messages_to_provider_visible_history_for_model(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            None,
        );

        assert!(matches!(
            &projected[1].payload,
            CoreConversationPayload::ToolResult { text, .. } if text == "file body"
        ));
    }

    /// Verifies orphaned tool results are downgraded to plain user-visible text before provider transport.
    /// 验证孤立工具结果会在 Provider 传输前降级为普通用户可见文本。
    #[test]
    fn provider_visible_history_downgrades_orphan_tool_result() {
        let messages = vec![CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_result_id: String::from("result-1"),
                tool_call_id: Some(String::from("missing-call")),
                text: String::from("orphan body"),
            },
        }];

        let projected = core_messages_to_provider_visible_history_for_model(
            &messages,
            Some(ProviderApiFamily::OpenAiChatCompatible),
            None,
        );

        let CoreConversationPayload::UserText { text, attachments } = &projected[0].payload else {
            panic!("expected orphan tool result to become plain user text");
        };
        assert!(attachments.is_empty());
        assert!(text.contains("cannot be replayed"));
        assert!(text.contains("orphan body"));
    }
}
