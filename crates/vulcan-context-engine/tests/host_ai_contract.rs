//! Feature-gated black-box coverage for the shared host AI contract adapter.
//! 共享宿主 AI 契约适配器的功能开关黑盒覆盖。

#![cfg(feature = "host-ai-contract")]

use vulcan_ai_contract::EmbeddingTask;
use vulcan_context_engine::{
    EmbeddingBatch, HostEmbeddingRequestConfig, TextSegment, build_embedding_request,
};

/// Prove that VCE projects semantic batches into the common VulcanCode AI request contract.
/// 证明 VCE 能将语义批次投影到 VulcanCode 共同 AI 请求契约。
#[test]
fn embedding_adapter_uses_the_shared_host_contract() {
    // The VCE batch preserves stable logical and segment identities.
    // VCE 批次保留稳定的逻辑身份和分段身份。
    let batch = EmbeddingBatch {
        segments: vec![TextSegment {
            input_id: String::from("blob:one"),
            ordinal: 0,
            start_char: 0,
            end_char: 4,
            text: String::from("code"),
        }],
        total_chars: 4,
    };
    // Host routing fields come from the registration owner, not from VCE defaults.
    // 宿主路由字段来自注册所有者，而不是 VCE 默认值。
    let config = HostEmbeddingRequestConfig {
        vector_space_id: String::from("code-semantic:test"),
        request_id: String::from("vce:test:embedding:1"),
        idempotency_key: Some(String::from("vce:test:embedding:1")),
        consumer_profile_id: String::from("vce-code-retrieval"),
        task: EmbeddingTask::CodeRetrieval,
        dimensions: Some(2),
    };

    // The adapter creates one standard HostAiPort request without opening a provider client.
    // 适配器无需创建供应商客户端即可生成标准 HostAiPort 请求。
    let request = build_embedding_request(&batch, config)
        .expect("valid VCE batch should project into the host contract");
    assert_eq!(request.vector_space_id, "code-semantic:test");
    assert_eq!(request.request.request_id, "vce:test:embedding:1");
    assert_eq!(request.request.consumer_profile_id, "vce-code-retrieval");
    assert_eq!(request.request.inputs.len(), 1);
    assert_eq!(request.request.inputs[0].input_id, "blob:one::segment:0");
    assert_eq!(request.request.task, EmbeddingTask::CodeRetrieval);
    assert_eq!(request.request.dimensions, Some(2));
}
