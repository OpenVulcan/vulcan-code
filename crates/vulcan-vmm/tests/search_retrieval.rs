//! Optional native search explanations preserve the unchanged external response shape.
//! 可选原生检索解释保留外置响应形状不变。

use vulcan_vmm::{MemorySearchHit, SearchMemoryEventsResponse};

/// Checks grouped round trips and the distinction between an unobserved stage and a zero delta.
/// 检查分组往返，以及未观察阶段与零增量的区别。
#[test]
fn search_retrieval_round_trip_preserves_optional_facts() {
    // payload is the exact service response shape, including one native and one external hit.
    // payload 是准确服务响应形状，包含一条原生命中及一条外置命中。
    let payload = serde_json::json!({ "trace_id": "test", "min_score": 0.75, "results": [{"query_index": 0, "query": "西瓜", "below_threshold_count": 3, "hits": [
        {"memory_id": 1, "source_turn_id": 0, "abstract_text": "", "details_preview": "", "category": "", "created_datetime": "",
        "retrieval": { "source_kind": "direct", "origin": "vector_search", "origin_label": "Vector Recall",
          "origin_explanation": "Vector only.", "final_score": 0.8, "context_observed": false,
          "support_count": 0, "rebuttal_count": 0, "matched_context_values": [],
          "matched_context_support_count": 0, "matched_context_rebuttal_count": 0, "matched_context_score_delta": null }},
        {"memory_id": 2, "source_turn_id": 0, "abstract_text": "", "details_preview": "", "category": "", "created_datetime": ""}
    ]}]});
    let response: SearchMemoryEventsResponse =
        serde_json::from_value(payload.clone()).expect("typed response");
    assert_eq!(serde_json::to_value(response).expect("round trip"), payload);
    assert!(
        serde_json::to_value(MemorySearchHit::default())
            .expect("external hit")
            .get("retrieval")
            .is_none()
    );
}
