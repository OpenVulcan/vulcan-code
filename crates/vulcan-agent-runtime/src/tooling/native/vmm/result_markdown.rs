//! Conversation-only VMM projections; service JSON remains unchanged.
//! 仅用于会话的 VMM 投影，服务 JSON 保持不变。

use vulcan_tools_manager::result_markdown::inline_code;
use vulcan_vmm::{
    DeleteMemoriesResponse, GetTurnDetailsResponse, MemorySearchHit, SearchMemoryEventsResponse,
    WriteMemoriesResponse,
};

/// Renders accepted query groups, preserving source IDs and observed evidence.
/// 渲染有效查询分组，保留来源标识与已观察证据。
/// Takes the typed service response; returns a single shared Markdown body.
/// 接收服务类型化响应，返回一份共享 Markdown 正文。
pub(super) fn search(response: &SearchMemoryEventsResponse) -> String {
    // Query order and explicit empty groups distinguish no hits from omitted results.
    // 查询顺序与明确的空分组可区分零命中和遗漏结果。
    let mut output = String::from("# Memory search\n");
    for group in &response.results {
        output.push_str(&format!(
            "\n## Query {}: {}\n",
            group.query_index,
            inline_code(&group.query)
        ));
        if group.hits.is_empty() {
            output.push_str("\nNo matches.\n");
        }
        // The note precedes the memory headings so it never reads as part of a memory body.
        // 该说明位于记忆标题之前，避免被读作某条记忆的正文。
        if group.below_threshold_count > 0 {
            output.push_str(&format!(
                "\n{} hit(s) below relevance threshold {} omitted; lower `min_score` (minimum {}) to widen the search.\n",
                group.below_threshold_count,
                response.min_score,
                super::VMM_MEMORY_SEARCH_MIN_SCORE_FLOOR
            ));
        }
        for hit in &group.hits {
            output.push_str(&memory_hit(hit));
        }
    }
    if response.results.is_empty() {
        output.push_str("\nNo query results.\n");
    }
    output
}

/// Renders one hit as identity, provenance and readable content only, in rank order.
/// 只把一条命中按排名顺序渲染为身份、来源与可读内容。
/// Scores and retrieval-stage evidence stay in the service response, as in VMM's own search
/// result: rerank scores are model-specific, so a raw number would read as a false relevance.
/// 评分与检索阶段证据保留在服务响应中，与 VMM 自身搜索结果一致：重排分由模型自定义尺度，
/// 原始数值会被误读为相关度。
pub(super) fn memory_hit(hit: &MemorySearchHit) -> String {
    // The heading line is the format the WebUI memory card pages by; keep it in sync with that parser.
    // 标题行是 WebUI 记忆卡分页所依据的格式；须与该解析器保持一致。
    let mut output = format!("\n### Memory {}\n", hit.memory_id);
    // Zero source-turn IDs explicitly mean the memory cannot be expanded through turn details.
    // 来源轮次为零明确表示无法通过轮次详情展开该记忆。
    let mut facts = Vec::new();
    if hit.source_turn_id != 0 {
        facts.push(format!("Source turn: {}", hit.source_turn_id));
    }
    for (label, value) in [
        ("Category", &hit.category),
        ("Created", &hit.created_datetime),
    ] {
        if !value.is_empty() {
            facts.push(format!("{label}: {}", inline_code(value)));
        }
    }
    if !facts.is_empty() {
        output.push_str(&format!("\n{}\n", facts.join(" · ")));
    }
    if !hit.abstract_text.is_empty() {
        output.push_str(&format!("\n{}\n", hit.abstract_text));
    }
    section(&mut output, "Details preview", &hit.details_preview);
    output
}

/// Renders canonical dialogue and independent analysis without dropping distinct content.
/// 渲染规范对话与独立分析，不丢弃不同语义的内容。
/// Takes returned turn details; returns readable sections and neighboring IDs for navigation.
/// 接收轮次详情，返回可读分节和用于导航的相邻标识。
pub(super) fn turns(response: &GetTurnDetailsResponse) -> String {
    // Analysis details are independent of native_turn_dialogue, as defined by turn_detail_entry.
    // 按 turn_detail_entry 定义，分析详情独立于 native_turn_dialogue。
    let mut output = String::from("# Turn details\n");
    for turn in &response.turns {
        output.push_str(&format!("\n## Turn {}\n", turn.turn_id));
        section(&mut output, "User", &turn.user_question);
        for item in &turn.timeline {
            section(&mut output, item.kind.as_wire_type(), &item.content);
        }
        section(&mut output, "Assistant", &turn.assistant_answer);
        section(&mut output, "Analysis details", &turn.detail);
        id_list(&mut output, "Previous turns", &turn.previous_turn_ids);
        id_list(&mut output, "Next turns", &turn.next_turn_ids);
    }
    if response.turns.is_empty() {
        output.push_str("\nNo turn details returned.\n");
    }
    output
}

/// Renders ordered write outcomes, retaining reusable memory IDs and deduplication status.
/// 渲染有序写入结果，保留可复用记忆标识与去重状态。
/// Takes the write response; returns a compact receipt without echoing submitted content.
/// 接收写入响应，返回不回显提交正文的简洁回执。
pub(super) fn written(response: &WriteMemoriesResponse) -> String {
    // Preserve input-order mapping through one ordered receipt per returned item.
    // 每条返回项保留有序回执，维持与输入顺序的映射。
    let mut output = String::from("# Memory write\n\n");
    for (index, item) in response.items.iter().enumerate() {
        output.push_str(&format!(
            "{}. Memory {}: {}.\n",
            index + 1,
            item.memory_id,
            if item.deduped {
                "reused existing memory"
            } else {
                "created"
            }
        ));
    }
    if response.items.is_empty() {
        output.push_str("No memory items returned.\n");
    }
    output
}

/// Renders actual deletion outcomes without leaking internal vector storage counts.
/// 渲染实际删除结果，不暴露内部向量存储计数。
/// Takes the deletion response; returns deleted and unavailable IDs with precise semantics.
/// 接收删除响应，返回已删除与不可用标识及准确语义。
pub(super) fn deleted(response: &DeleteMemoriesResponse) -> String {
    // Unavailable IDs may be inactive or outside scope; do not falsely claim nonexistence.
    // 不可用标识可能已失活或超出作用域，不错误声明为不存在。
    let mut output = String::from("# Memory deletion\n");
    id_list(
        &mut output,
        "Deleted memories",
        &response.deleted_memory_ids,
    );
    id_list(
        &mut output,
        "Not found, inactive, already deleted, or outside scope",
        &response.not_found_memory_ids,
    );
    if response.deleted_memory_ids.is_empty() {
        output.push_str("\nNo memories deleted.\n");
    }
    output
}

/// Appends one nonempty labeled body without interpreting its original content.
/// 追加一份非空带标签正文，不解释其原始内容。
/// Mutates the output with the supplied heading and body; returns no value.
/// 使用给定标题与正文修改输出，无返回值。
fn section(output: &mut String, heading: &str, body: &str) {
    if !body.is_empty() {
        output.push_str(&format!("\n#### {heading}\n\n{body}\n"));
    }
}

/// Appends a nonempty ID list needed for follow-up operations.
/// 追加后续操作所需的非空标识列表。
/// Mutates the output using label and IDs; returns no value.
/// 使用标签与标识修改输出，无返回值。
fn id_list(output: &mut String, label: &str, ids: &[u64]) {
    if !ids.is_empty() {
        output.push_str(&format!(
            "\n{label}: {}\n",
            ids.iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Confirms a hit keeps its identity and content but neither its score nor stage evidence.
    /// 确认命中保留身份与内容，但不含评分与阶段证据。
    #[test]
    fn renders_identity_and_content_without_scores_or_stage_evidence() {
        let hit = MemorySearchHit {
            memory_id: 7,
            source_turn_id: 2,
            abstract_text: "Unique summary".into(),
            retrieval: Some(serde_json::from_value(serde_json::json!({
                "source_kind": "direct", "origin": "lexical_rerank", "origin_label": "Lexical + Rerank",
                "origin_explanation": "Both recorded stages executed.", "final_score": 0.875,
                "context_observed": true, "support_count": 9, "rebuttal_count": 5,
                "matched_context_values": ["fruit=melon"], "matched_context_support_count": 2,
                "matched_context_rebuttal_count": 1, "matched_context_score_delta": -0.01
            })).expect("closed evidence contract")),
            ..Default::default()
        };
        let body = memory_hit(&hit);
        assert!(body.starts_with("\n### Memory 7\n"), "{body}");
        assert!(body.contains("Source turn: 2"));
        assert_eq!(body.matches("Unique summary").count(), 1);
        for evidence_only in ["0.875", "lexical_rerank", "9/5", "fruit=melon", "-0.01"] {
            assert!(
                !body.contains(evidence_only),
                "score or stage evidence leaked: {evidence_only}"
            );
        }
        assert!(!memory_hit(&MemorySearchHit::default()).contains("Source turn"));
    }

    /// Confirms omitted weak hits are counted ahead of the memories, outside every memory page.
    /// 确认被省略的弱命中计数位于记忆之前，不落入任何记忆分页。
    #[test]
    fn counts_hits_below_threshold_ahead_of_memories() {
        let group = |below_threshold_count| vulcan_vmm::MemorySearchGroupResult {
            query_index: 1,
            query: "deploy steps".into(),
            hits: vec![MemorySearchHit {
                memory_id: 7,
                ..Default::default()
            }],
            below_threshold_count,
        };
        let body = search(&SearchMemoryEventsResponse {
            results: vec![group(3)],
            trace_id: String::new(),
            min_score: 0.75,
        });
        let note = body.find("min_score").expect("omitted hits are reported");
        assert!(note < body.find("### Memory 7").expect("memory heading"));
        assert!(body.contains('3') && body.contains("0.75"));
        let silent = search(&SearchMemoryEventsResponse {
            results: vec![group(0)],
            trace_id: String::new(),
            min_score: 0.75,
        });
        assert!(!silent.contains("min_score"));
    }

    /// Checks partial deletion does not imply every requested memory was removed.
    /// 检查部分删除不会暗示所有请求记忆均已删除。
    #[test]
    fn reports_partial_delete_and_empty_write() {
        // Use nonadjacent IDs so labels, rather than collection positions, determine semantics.
        // 使用不相邻标识，使标签而非集合位置决定语义。
        let body = deleted(&DeleteMemoriesResponse {
            deleted_memory_ids: vec![8],
            not_found_memory_ids: vec![30],
            trace_id: "internal-trace".into(),
            deleted_vector_rows: 200,
        });
        assert!(body.contains("Deleted memories: 8"));
        assert!(body.contains("outside scope: 30"));
        assert!(!body.contains("internal-trace"));
        assert!(!body.contains("200"));
        assert!(written(&WriteMemoriesResponse::default()).contains("No memory items"));
    }
}
