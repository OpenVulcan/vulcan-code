//! Regression coverage for compact request fields and replayable checkpoints.
//! 压缩请求字段与可重放检查点的回归覆盖。

use super::super::test_support::{bundled_manifest_route, minimal_request};
use super::*;
use serde_json::json;
use vulcan_ai_contract::{APPLY_PATCH_TOOL_NAME, ToolDefinition};

/// Verifies compact encoding consumes the same route-owned Apply Patch contract as live streaming.
/// 验证 compact 编码与实时流请求消费同一份路由自有 Apply Patch 契约。
#[test]
fn codex_compact_projection_uses_route_owned_freeform_apply_patch_contract() {
    // route mirrors one native Lite model whose Apply Patch input is provider-declared freeform text.
    // route 模拟一个原生 Lite 模型，其 Apply Patch 输入由供应商声明为自由文本。
    let mut route = bundled_manifest_route(
        "openai",
        "codex-subscription",
        OPENAI_CODEX_RESPONSES_PROFILE_ID,
    );
    route.codex_behavior = Some(crate::provider_manifest::CodexModelBehavior {
        wire_dialect: crate::provider_manifest::CodexWireDialect::Lite,
        apply_patch_tool_type: crate::provider_manifest::CodexApplyPatchToolType::Freeform,
        ..Default::default()
    });
    // request starts from the canonical JSON-schema declaration used outside Codex Lite.
    // request 从 Codex Lite 之外使用的规范 JSON Schema 声明开始。
    let mut request = minimal_request();
    request.cache_scope_id = Some(String::from("compact-session"));
    request.tools.push(ToolDefinition {
        name: String::from(APPLY_PATCH_TOOL_NAME),
        description: String::from("Apply one patch"),
        input_schema: json!({"type":"object","properties":{"patch":{"type":"string"}}}),
        input_format: Default::default(),
        strict: false,
    });

    // effectiveRequest is the exact immutable snapshot used by the production compact path.
    // effectiveRequest 是生产 compact 路径使用的精确不可变快照。
    let effective_request = request_with_codex_tool_input_contract(&route, &request);
    // encoded follows the production order through native encoding and provider projection.
    // encoded 按生产顺序经过原生编码和供应商投影。
    let encoded = encode_codex_responses_request(&effective_request)
        .expect("Codex compact request should encode");
    let mut encoded = project_provider_request(&route, &effective_request, encoded)
        .expect("Codex Lite compact request should project");
    prepare_responses_compact_body(&mut encoded.body)
        .expect("Codex Lite compact body should normalize");

    // tool is the exact Apply Patch declaration retained inside the Lite namespace prefix.
    // tool 是 Lite 命名空间前缀内保留的精确 Apply Patch 声明。
    let tool = &encoded.body["input"][0]["tools"][0]["tools"][0];
    assert_eq!(tool["type"], "custom");
    assert_eq!(tool["name"], APPLY_PATCH_TOOL_NAME);
    assert_eq!(tool["format"]["syntax"], "lark");
}

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
