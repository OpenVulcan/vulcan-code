//! Regression coverage for compact request fields and replayable checkpoints.
//! 压缩请求字段与可重放检查点的回归覆盖。

use super::*;
use serde_json::json;

/// Verifies the compact request body follows the official Codex `CompactionInput` shape.
/// 验证 compact 请求体遵循官方 Codex `CompactionInput` 结构。
#[test]
fn codex_compact_body_removes_stream_only_and_unknown_fields() {
    let mut body = json!({
        "model": "gpt-codex",
        "input": [],
        "instructions": "",
        "stream": true,
        "store": false,
        "include": ["reasoning.encrypted_content"],
        "previous_response_id": "resp-old",
        "unsupported_vendor_field": true,
        "parallel_tool_calls": true,
        "tools": [{
            "type": "function",
            "name": "inspect_workspace",
            "parameters": {"type": "object", "properties": {}}
        }],
    });

    prepare_responses_compact_body(&mut body).expect("object compact body should normalize");

    assert_eq!(body["model"], "gpt-codex");
    assert!(body["input"].is_array());
    assert!(body.get("instructions").is_none());
    assert_eq!(body["parallel_tool_calls"], true);
    assert_eq!(body["tools"][0]["name"], "inspect_workspace");
    assert!(body.get("stream").is_none());
    assert!(body.get("store").is_none());
    assert!(body.get("include").is_none());
    assert!(body.get("previous_response_id").is_none());
    assert!(body.get("unsupported_vendor_field").is_none());
}

/// Verifies compact requests retain non-empty ordinary-turn base instructions.
/// 验证压缩请求会保留非空的普通回合基础指令。
#[test]
fn codex_compact_body_preserves_nonempty_base_instructions() {
    let mut body = json!({
        "model": "gpt-codex",
        "input": [],
        "instructions": "ordinary Codex base instructions",
        "stream": true,
    });

    prepare_responses_compact_body(&mut body).expect("object compact body should normalize");

    assert_eq!(
        body["instructions"],
        Value::String(String::from("ordinary Codex base instructions"))
    );
    assert!(body.get("stream").is_none());
}

/// Verifies compact responses preserve metadata and reject output without an opaque checkpoint.
/// 验证 compact 响应保留元数据，并拒绝不含不透明检查点的输出。
#[test]
fn codex_compact_response_requires_a_compaction_item() {
    let valid = decode_responses_compact_result(
        &json!({
            "id": "resp-compact",
            "usage": {"input_tokens": 4},
            "output": [{"type": "compaction_summary", "encrypted_content": "opaque"}]
        }),
        "https://example.test/responses/compact",
    )
    .expect("compaction output should decode");
    assert_eq!(valid.response_id.as_deref(), Some("resp-compact"));
    assert_eq!(valid.usage, Some(json!({"input_tokens": 4})));
    assert_eq!(valid.output_items[0]["type"], "compaction");

    let failure = decode_responses_compact_result(
        &json!({"output": [{"type": "message", "content": []}]}),
        "https://example.test/responses/compact",
    )
    .expect_err("message-only output must not become a checkpoint");
    assert!(failure.error.message.contains("replayable output items"));
    assert_eq!(failure.error.dispatch_outcome, DispatchOutcome::Dispatched);

    // empty_checkpoint proves a typed compaction marker still needs a usable opaque payload.
    // empty_checkpoint 证明带类型的压缩标记仍然必须拥有可用的不透明载荷。
    let empty_checkpoint = decode_responses_compact_result(
        &json!({"output": [{"type": "compaction", "encrypted_content": ""}]}),
        "https://example.test/responses/compact",
    )
    .expect_err("empty opaque checkpoints must be rejected before persistence");
    assert!(
        empty_checkpoint
            .error
            .message
            .contains("replayable output items")
    );

    let mixed = decode_responses_compact_result(
        &json!({
            "output": [
                {"type": "compaction", "encrypted_content": "opaque"},
                {"type": "message", "content": []}
            ]
        }),
        "https://example.test/responses/compact",
    )
    .expect("ordered replayable output items should be preserved");
    assert_eq!(mixed.output_items.len(), 2);
    let unknown_failure = decode_responses_compact_result(
        &json!({
            "output": [
                {"type": "compaction", "encrypted_content": "opaque"},
                {"type": "provider_private_item"}
            ]
        }),
        "https://example.test/responses/compact",
    )
    .expect_err("unknown output items must not become a checkpoint");
    assert!(
        unknown_failure
            .error
            .message
            .contains("replayable output items")
    );
}
