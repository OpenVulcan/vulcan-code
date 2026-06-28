use super::{
    AnthropicContentBlock, PendingAnthropicServerToolUse, PendingAnthropicToolUse,
    anthropic_content_block_from_pending_server_tool_use,
    anthropic_content_block_from_pending_tool_use, anthropic_downgraded_tool_result_count,
    anthropic_user_tool_result_blocks, build_anthropic_provider_message_state,
    build_anthropic_provider_tool_name, build_anthropic_request_payload,
    finalize_pending_anthropic_tool_use, repair_anthropic_tool_result_topology,
};
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_core::types::{
    ConversationAttachment, ConversationAttachmentKind,
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall,
    ProviderContinuation as CoreProviderContinuation,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
};

/// Builds one temporary attachment used by Anthropic multimodal request tests.
/// 构建一条供 Anthropic 多模态请求测试使用的临时附件。
fn build_test_attachment(
    extension: &str,
    kind: ConversationAttachmentKind,
    mime_type: &str,
    file_name: &str,
) -> ConversationAttachment {
    let unique_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let file_path = std::env::temp_dir().join(format!("vulcan-anthropic-{unique_id}.{extension}"));
    std::fs::write(&file_path, [0_u8, 1, 2, 3]).expect("temporary attachment should write");

    ConversationAttachment {
        kind,
        file_name: String::from(file_name),
        local_path: file_path.to_string_lossy().into_owned(),
        source_path: None,
        asset_id: None,
        byte_len: Some(4),
        preview_local_path: None,
        mime_type: String::from(mime_type),
    }
}

/// Builds one reusable Anthropic request snapshot for request-payload topology tests.
/// 构建一份供请求载荷拓扑测试复用的 Anthropic 请求快照。
fn build_test_request_snapshot(prompt: &str) -> CoreProviderRequestSnapshot {
    CoreProviderRequestSnapshot {
        provider_id: String::from("anthropic"),
        model_id: String::from("claude-sonnet-4-6"),
        system_prompt: String::from("system prompt"),
        prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(String::from(
            "system prompt",
        )),
        prompt: String::from(prompt),
        history: Vec::new(),
        temperature: Some(0.2),
        max_turns: 8,
        tool_schemas: Vec::new(),
        additional_params: Some(json!({})),
        request_headers: None,
        continuation: CoreProviderContinuation::default(),
    }
}

mod cache_control;
mod multimodal_payload;
mod provider_state;
mod tool_mapping;
mod tool_result_topology;
