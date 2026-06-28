use super::{
    augment_openai_compatible_error_with_tool_context,
    build_openai_compatible_provider_message_state, build_openai_compatible_request_payload,
    collect_openai_compatible_function_declaration_indices, normalize_openai_compatible_chunk,
    parse_openai_compatible_provider_message_state,
    provider_tool_definition_to_openai_compatible_tool,
};
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use crate::provider_config::{ModelInterleavedCapability, ModelInterleavedField};
use serde_json::{Map as JsonMap, json};
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_core::types::{
    ConversationAttachment, ConversationAttachmentKind,
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall,
    ProviderContinuation as CoreProviderContinuation,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
};
use vulcan_tools_manager::VulcanToolSchemaProjectionProfile;

/// Builds one temporary attachment used by multimodal transport tests.
/// 构建一条供多模态传输测试使用的临时附件。
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
    let file_path =
        std::env::temp_dir().join(format!("vulcan-openai-compatible-{unique_id}.{extension}"));
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

mod multimodal_payload;

mod reasoning_payload;

mod tool_topology;

mod tools_schema;

mod usage_and_chunks;
