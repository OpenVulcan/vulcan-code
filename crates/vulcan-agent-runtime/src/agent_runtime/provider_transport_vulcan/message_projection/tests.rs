//! Regression tests for the message_projection boundary.
//! message_projection边界的回归测试。

use super::*;
use crate::attachment_assets::{AttachmentDescriptor, stage_attachment_bytes};
use serde_json::json;
use vulcan_core::types::ConversationAttachmentKind;

/// Verifies the provisional bridge does not materialize an attachment whose prompt-owned delivery proof is absent.
/// 验证临时桥接不会物化缺失提示词所有投递证明的附件。
#[test]
fn provider_projection_defers_unproven_attachment_integrity_to_prompt_owner() {
    // workspace owns the indexed attachment and is removed after the projection assertion.
    // workspace 持有索引附件，并在投影断言后移除。
    let workspace = std::env::temp_dir().join(format!(
        "vulcan-provider-attachment-integrity-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&workspace).expect("test workspace should be created");
    // attachment retains immutable byte facts while omitting an original-source fallback.
    // attachment 保留不可变字节事实，同时不提供原始来源回退路径。
    let attachment = stage_attachment_bytes(
        workspace.to_string_lossy().as_ref(),
        &AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/png",
            extension: "png",
        },
        "integrity.png",
        None,
        b"original-image",
    )
    .expect("test attachment should stage");
    // mutated_bytes deliberately preserves length while changing the indexed content identity.
    // mutated_bytes 故意保持长度不变，同时改变已索引的内容身份。
    let mutated_bytes = vec![b'x'; b"original-image".len()];
    std::fs::write(&attachment.local_path, mutated_bytes)
        .expect("same-length attachment mutation should write");
    // message exercises the final canonical-message bridge shared by ordinary requests and compaction.
    // message 覆盖普通请求与压缩共同使用的最终规范消息桥接。
    let message = vulcan_core::types::ConversationMessage {
        id: String::from("user-integrity"),
        payload: CoreConversationPayload::UserText {
            text: String::from("inspect attachment"),
            attachments: vec![attachment],
        },
    };

    let projected = core_message_to_public(
        &message,
        "provider",
        "connection",
        "https://provider.invalid/v1/responses",
        "openai-responses.v1",
        "model",
    )
    .expect("integrity-invalid attachment should degrade to metadata");

    assert!(
        !projected
            .content
            .iter()
            .any(|item| matches!(item, ContentItem::Resource { .. }))
    );
    assert!(projected.content.iter().any(|item| {
        matches!(item, ContentItem::Text { text } if text.contains("no prompt-owned media delivery proof"))
    }));

    std::fs::remove_dir_all(&workspace).expect("test workspace should be removed");
}

/// Verifies ordinary replay content falls back locally when the credential or endpoint changes.
/// 验证凭据或端点变化时，普通重放内容会安全回退到本地规范内容。
#[test]
fn ordinary_replay_content_requires_exact_connection_route() {
    // state contains one signed reasoning item produced by the original physical route.
    // state 包含由原始物理路由生成的一项带签名推理内容。
    let state = json!({
        "vulcan_ai": {
            "replay": ProviderReplayEnvelope::new(
                "custom:proxy",
                "connection-a",
                "https://proxy.example/v1/responses",
                "openai-codex-responses.v1",
                "gpt-5-codex",
                vec![ContentItem::Reasoning {
                    text: String::from("reasoning"),
                    signature: Some(String::from("opaque-signature")),
                }],
                vec![json!({
                    "type": "reasoning",
                    "id": "reasoning-item",
                    "status": "completed",
                    "summary": [{"type": "summary_text", "text": "reasoning"}],
                    "encrypted_content": "opaque-replay-context",
                })],
            )
        }
    });

    // matching projects the durable provider items of this exact route.
    // matching 投影本精确路由的可持久化供应商项。
    let ProviderReplayProjection::ProviderItems(items) = replay_projection_for_route(
        Some(&state),
        "custom:proxy",
        "connection-a",
        "https://proxy.example/v1/responses",
        "openai-codex-responses.v1",
        "gpt-5-codex",
    )
    .expect("matching provider context should decode") else {
        panic!("a matching route must project its durable provider items");
    };
    assert_eq!(items[0]["id"], "reasoning-item");
    // foreignRoutes proves one changed identifier alone is enough to stop replaying this envelope.
    // foreignRoutes 证明只要有一个标识改变，就足以停止重放本封套。
    for (label, connection, endpoint) in [
        (
            "credential mismatch",
            "connection-b",
            "https://proxy.example/v1/responses",
        ),
        (
            "endpoint mismatch",
            "connection-a",
            "https://proxy.example/changed/responses",
        ),
    ] {
        assert_eq!(
            replay_projection_for_route(
                Some(&state),
                "custom:proxy",
                connection,
                endpoint,
                "openai-codex-responses.v1",
                "gpt-5-codex",
            )
            .expect("a foreign route must stay a safe local fallback"),
            ProviderReplayProjection::RouteMismatch,
            "{label} must not replay another route's envelope"
        );
    }
    // switchedRouteMessage proves provider-owned items never cross a protocol boundary.
    // switchedRouteMessage 证明供应商持有项绝不会跨越协议边界。
    let switched_route_message = core_message_to_public(
        &vulcan_core::types::ConversationMessage {
            id: String::from("assistant-switched"),
            payload: CoreConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::from("reasoning"),
                provider_message_id: None,
                provider_message_state: Some(state.clone()),
                usage: None,
                tool_calls: Vec::new(),
            },
        },
        "custom:proxy",
        "connection-a",
        "https://proxy.example/v1/messages",
        "anthropic-messages.v1",
        "gpt-5-codex",
    )
    .expect("protocol switch should project provider-neutral content");
    assert!(switched_route_message.provider_context_items.is_empty());
    assert_eq!(
        switched_route_message.content,
        vec![ContentItem::Reasoning {
            text: String::from("reasoning"),
            signature: None,
        }]
    );
    // exactRouteMessage proves raw provider items replace canonical re-encoding only on their owner route.
    // exactRouteMessage 证明原始供应商项仅在归属路由上替代规范重编码。
    let exact_route_message = core_message_to_public(
        &vulcan_core::types::ConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::from("reasoning"),
                provider_message_id: None,
                provider_message_state: Some(state),
                usage: None,
                tool_calls: Vec::new(),
            },
        },
        "custom:proxy",
        "connection-a",
        "https://proxy.example/v1/responses",
        "openai-codex-responses.v1",
        "gpt-5-codex",
    )
    .expect("matching assistant should project exact provider context");
    assert!(exact_route_message.content.is_empty());
    assert_eq!(exact_route_message.provider_context_items.len(), 1);
}
