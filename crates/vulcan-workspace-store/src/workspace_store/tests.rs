//! Workspace-store regression tests extracted from the storage implementation module.
//! 从存储实现模块中抽取的工作区存储回归测试。

use super::*;
use crate::model::{PendingPromptPanelEntry, PendingPromptPanelEntryState, VmmProfileSyncState};
use serde_json::json;
use vulcan_context_injection::{
    ContextInjectionState, InjectionInsert, InjectionKind, InjectionPanelCard,
};
use vulcan_vmm::{PostActionRequest, VmmConfigLayer};

/// Verifies that transcript metadata keeps display fields while preserving one compact `change_set` host payload for later change-summary reconstruction.
/// 验证转录元数据会保留展示字段，同时保留一份紧凑 `change_set` 宿主载荷以便后续重建变更摘要。
#[test]
fn session_message_meta_json_preserves_compact_change_set_host_payload() {
    let message = SessionMessageEntry {
        id: String::from("message-1"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::ToolResultOutput,
        visible: true,
        title: String::from("工具返回"),
        body: String::from("已写入文件。"),
        metadata: Some(json!({
            "tool_name": "write_file",
            "host_result": {
                "kind": "change_set",
                "payload": {
                    "mode": "preview",
                    "files": [{
                        "change": "create",
                        "path": r"D:\openvulcan\vulcan-code\src\main.rs",
                        "content": "fn main() {}\n",
                    }],
                },
            },
        })),
        created_at: String::from("1"),
    };

    let json_payload = serialize_session_message_meta_to_json(&message)
        .expect("expected metadata payload to be serialized");
    let parsed = parse_session_message_meta_from_json(&json_payload)
        .expect("expected metadata payload to be parsed");

    assert_eq!(parsed.title.as_deref(), Some("工具返回"));
    let metadata = parsed.metadata.expect("expected sanitized metadata");
    assert_eq!(
        metadata.get("tool_name").and_then(|value| value.as_str()),
        Some("write_file")
    );

    let host_result = metadata
        .get("host_result")
        .and_then(|value| value.as_object())
        .expect("expected host result to be preserved");
    assert_eq!(
        host_result.get("kind").and_then(|value| value.as_str()),
        Some("change_set")
    );
    let payload = host_result
        .get("payload")
        .and_then(|value| value.as_object())
        .expect("expected compact payload");
    assert_eq!(
        payload.get("mode").and_then(|value| value.as_str()),
        Some("preview")
    );
    assert_eq!(
        payload
            .get("stats")
            .and_then(|value| value.get("file_count"))
            .and_then(|value| value.as_u64()),
        Some(1)
    );
    assert_eq!(
        payload
            .get("verification")
            .and_then(|value| value.get("status"))
            .and_then(|value| value.as_str()),
        Some("skipped")
    );
    let file = payload
        .get("files")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .expect("expected compact file record");
    assert_eq!(
        file.get("change").and_then(|value| value.as_str()),
        Some("create")
    );
    let preview = serde_json::to_string(payload).unwrap_or_default();
    assert!(preview.contains("main.rs"));
}

/// Verifies generated-media host payloads keep the minimal fields required to rebuild inline media cards after restart.
/// 验证生成媒体宿主载荷会保留重启后重建内联媒体卡片所需的最小字段。
#[test]
fn session_message_meta_json_preserves_generated_attachment_host_result() {
    let message = SessionMessageEntry {
        id: String::from("message-2"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::ToolResultOutput,
        visible: true,
        title: String::from("工具返回"),
        body: String::from("已生成图片与音频。"),
        metadata: Some(json!({
            "tool_name": "image_generation",
            "host_result": {
                "kind": "generated_attachments",
                "payload": {
                    "outputs": [{
                        "kind": "image",
                        "file_name": "demo.png",
                        "local_path": r"D:\demo.png",
                        "preview_local_path": r"D:\demo.preview.png",
                        "mime_type": "image/png",
                        "byte_len": 1024,
                        "asset_id": "asset-demo",
                    }],
                },
            },
        })),
        created_at: String::from("2"),
    };

    let json_payload = serialize_session_message_meta_to_json(&message)
        .expect("expected generated attachment metadata payload");
    let parsed = parse_session_message_meta_from_json(&json_payload)
        .expect("expected generated attachment metadata to parse");
    let metadata = parsed
        .metadata
        .expect("expected generated attachment metadata");
    let host_result = metadata
        .get("host_result")
        .and_then(Value::as_object)
        .expect("expected preserved host_result object");

    assert_eq!(
        host_result.get("kind").and_then(Value::as_str),
        Some("generated_attachments")
    );
    assert_eq!(
        host_result
            .get("payload")
            .and_then(|value| value.get("outputs"))
            .and_then(Value::as_array)
            .and_then(|outputs| outputs.first())
            .and_then(|output| output.get("local_path"))
            .and_then(Value::as_str),
        Some(r"D:\demo.png")
    );
    assert_eq!(
        host_result
            .get("payload")
            .and_then(|value| value.get("outputs"))
            .and_then(Value::as_array)
            .and_then(|outputs| outputs.first())
            .and_then(|output| output.get("preview_local_path"))
            .and_then(Value::as_str),
        Some(r"D:\demo.preview.png")
    );
}

/// Verifies persisted generated-media metadata can still be detected from stored session rows during lazy hydration.
/// 验证懒加载阶段仍可从持久化会话行中识别生成媒体元数据。
#[test]
fn stored_session_message_row_detects_generated_attachment_host_result() {
    let meta_json = serde_json::to_string(&StoredSessionMessageMeta {
        title: Some(String::from("工具返回")),
        metadata: Some(json!({
            "tool_call_id": "call-media-1",
            "host_result": {
                "kind": "generated_attachments",
                "payload": {
                    "outputs": [{
                        "kind": "audio",
                        "file_name": "demo.mp3",
                        "local_path": r"D:\demo.mp3",
                        "mime_type": "audio/mpeg",
                    }],
                },
            },
        })),
    })
    .expect("expected test metadata json");
    let row = StoredSessionMessageRow {
        id: String::from("message-media-1"),
        ordinal: 18,
        role: String::from("assistant"),
        kind: String::from("tool-result-output"),
        visible: 1,
        turn_index: 3,
        content: String::new(),
        meta_json: Some(meta_json),
        created_at: String::from("18"),
    };

    assert!(stored_session_message_row_contains_generated_attachment_host_result(&row));
}

/// Verifies generated-media tool-result rows are merged back into transcript windows ahead of later visible rows from the same turn.
/// 验证生成媒体工具结果行会被重新并入转录窗口，并排在同轮次后续可见消息之前。
#[test]
fn merge_generated_media_rows_into_transcript_window_preserves_media_ordinals() {
    let mut transcript_rows = vec![
        StoredSessionMessageRow {
            id: String::from("message-user"),
            ordinal: 20,
            role: String::from("user"),
            kind: String::from("transcript"),
            visible: 1,
            turn_index: 5,
            content: String::from("生成一个图片"),
            meta_json: None,
            created_at: String::from("20"),
        },
        StoredSessionMessageRow {
            id: String::from("message-assistant"),
            ordinal: 24,
            role: String::from("assistant"),
            kind: String::from("assistant-output"),
            visible: 1,
            turn_index: 5,
            content: String::from("图片已生成"),
            meta_json: None,
            created_at: String::from("24"),
        },
    ];
    let generated_media_rows = vec![StoredSessionMessageRow {
        id: String::from("message-tool-result"),
        ordinal: 22,
        role: String::from("assistant"),
        kind: String::from("tool-result-output"),
        visible: 1,
        turn_index: 5,
        content: String::new(),
        meta_json: Some(
            serde_json::to_string(&StoredSessionMessageMeta {
                title: Some(String::from("已生成图片")),
                metadata: Some(json!({
                    "tool_call_id": "call-media-2",
                    "host_result": {
                        "kind": "generated_attachments",
                        "payload": {
                            "outputs": [{
                                "kind": "image",
                                "file_name": "demo.png",
                                "local_path": r"D:\demo.png",
                                "mime_type": "image/png",
                            }],
                        },
                    },
                })),
            })
            .expect("expected generated media metadata"),
        ),
        created_at: String::from("22"),
    }];

    let earliest = merge_generated_media_rows_into_transcript_window(
        &mut transcript_rows,
        generated_media_rows,
    );

    assert_eq!(earliest, Some(20));
    assert_eq!(
        transcript_rows
            .into_iter()
            .map(|row| row.id)
            .collect::<Vec<_>>(),
        vec![
            String::from("message-user"),
            String::from("message-tool-result"),
            String::from("message-assistant"),
        ]
    );
}

/// Verifies that oversized transcript bodies are split without losing UTF-8 content.
/// 验证超大转录正文会在不破坏 UTF-8 内容的前提下被拆分。
#[test]
fn oversized_session_message_body_is_split_and_rebuilt() {
    let body = "工具输出包含中文与 ASCII content\n".repeat(8_000);
    let prepared = prepare_session_message_body_for_storage(&body);

    assert_ne!(prepared.inline_content, body);
    assert!(!prepared.chunks.is_empty());
    assert!(
        prepared
            .chunks
            .iter()
            .all(|chunk| chunk.len() <= SESSION_MESSAGE_CHUNK_CONTENT_BYTES)
    );
    assert_eq!(prepared.chunks.concat(), body);

    let chunk_rows = prepared
        .chunks
        .into_iter()
        .enumerate()
        .map(|(chunk_index, content)| StoredSessionMessageChunkRow {
            message_id: String::from("message-1"),
            chunk_index: chunk_index as i64,
            content,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rebuild_session_message_body_from_chunks("message-1", chunk_rows)
            .expect("expected chunks to rebuild"),
        body
    );
}

/// Verifies that SQL row preparation keeps large bodies out of the lightweight message row.
/// 验证 SQL 行预处理会避免将大型正文写入轻量消息行。
#[test]
fn session_message_batch_rows_store_oversized_body_as_chunks() {
    let session_id = SessionId::from_u128(42);
    let body = "大型工具结果 payload ".repeat(10_000);
    let message = SessionMessageEntry {
        id: String::from("message-1"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::ToolResultOutput,
        visible: true,
        title: String::from("工具返回"),
        body: body.clone(),
        metadata: None,
        created_at: String::from("1"),
    };
    let mut message_rows = Vec::new();
    let mut chunk_rows = Vec::new();
    let mut search_rows = Vec::new();

    push_session_message_rows_for_session(
        &BTreeMap::new(),
        -1,
        -1,
        &mut message_rows,
        &mut chunk_rows,
        &mut search_rows,
        "project-1",
        true,
        session_id,
        "会话标题",
        &[message],
    );

    assert_eq!(message_rows.len(), 1);
    assert!(!chunk_rows.is_empty());
    assert!(search_rows.is_empty());
    match &message_rows[0][7] {
        ControllerSqliteValue::String(inline_content) => {
            assert_ne!(inline_content.as_str(), body.as_str());
            assert!(inline_content.len() < body.len());
        }
        other => panic!("expected inline content string, got {other:?}"),
    }

    let rebuilt = chunk_rows
        .iter()
        .map(|row| match &row[3] {
            ControllerSqliteValue::String(content) => content.as_str(),
            other => panic!("expected chunk content string, got {other:?}"),
        })
        .collect::<String>();
    assert_eq!(rebuilt, body);
}

/// Verifies that user messages start stable dialogue turns while following outputs stay grouped.
/// 验证用户消息会开启稳定对话轮次，后续输出会保持归组。
#[test]
fn session_message_turn_indexes_group_outputs_under_user_turns() {
    let messages = vec![
        SessionMessageEntry::hidden(
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ExploreOutput,
            "探索",
            "项目概览",
        ),
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "第一轮"),
        SessionMessageEntry::new(SessionMessageKind::Assistant, "AI 输出", "第一轮回答"),
        SessionMessageEntry {
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            ..SessionMessageEntry::new(SessionMessageKind::Assistant, "工具返回", "结果")
        },
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "第二轮"),
        SessionMessageEntry::new(SessionMessageKind::Assistant, "AI 输出", "第二轮回答"),
    ];

    assert_eq!(
        assign_session_message_turn_indexes(&messages),
        vec![0, 0, 0, 0, 1, 1]
    );
}

/// Verifies that message batch rows persist the computed turn index beside ordinal order.
/// 验证消息批处理行会在 ordinal 顺序之外持久化计算后的轮次索引。
#[test]
fn session_message_batch_rows_persist_turn_indexes() {
    let session_id = SessionId::from_u128(44);
    let messages = vec![
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "第一轮"),
        SessionMessageEntry::new(SessionMessageKind::Assistant, "AI 输出", "第一轮回答"),
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "第二轮"),
    ];
    let mut message_rows = Vec::new();
    let mut chunk_rows = Vec::new();
    let mut search_rows = Vec::new();

    push_session_message_rows_for_session(
        &BTreeMap::new(),
        -1,
        -1,
        &mut message_rows,
        &mut chunk_rows,
        &mut search_rows,
        "project-1",
        true,
        session_id,
        "会话标题",
        &messages,
    );

    let turn_indexes = message_rows
        .iter()
        .map(|row| match row[6] {
            ControllerSqliteValue::Int64(value) => value,
            ref other => panic!("expected turn index integer, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(turn_indexes, vec![0, 0, 1]);
    assert!(chunk_rows.is_empty());
    assert_eq!(search_rows.len(), 2);
}

/// Verifies partial-window persistence keeps existing ordinals instead of renumbering from zero.
/// 验证局部窗口持久化会保留已有序号，而不是从零重新编号。
#[test]
fn session_message_rows_preserve_existing_positions_for_partial_windows() {
    let session_id = SessionId::from_u128(46);
    let existing_message = SessionMessageEntry {
        id: String::from("existing-message"),
        ..SessionMessageEntry::new(SessionMessageKind::Assistant, "AI 输出", "历史回答")
    };
    let new_message = SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "新问题");
    let messages = vec![existing_message, new_message];
    let mut persisted_positions = BTreeMap::new();
    persisted_positions.insert(
        String::from("existing-message"),
        StoredSessionMessagePositionRow {
            id: String::from("existing-message"),
            ordinal: 42,
            turn_index: 7,
        },
    );
    let mut message_rows = Vec::new();
    let mut chunk_rows = Vec::new();
    let mut search_rows = Vec::new();

    push_session_message_rows_for_session(
        &persisted_positions,
        42,
        7,
        &mut message_rows,
        &mut chunk_rows,
        &mut search_rows,
        "project-1",
        true,
        session_id,
        "会话标题",
        &messages,
    );

    let ordinals = message_rows
        .iter()
        .map(|row| match row[2] {
            ControllerSqliteValue::Int64(value) => value,
            ref other => panic!("expected ordinal integer, got {other:?}"),
        })
        .collect::<Vec<_>>();
    let turn_indexes = message_rows
        .iter()
        .map(|row| match row[6] {
            ControllerSqliteValue::Int64(value) => value,
            ref other => panic!("expected turn index integer, got {other:?}"),
        })
        .collect::<Vec<_>>();

    assert_eq!(ordinals, vec![42, 43]);
    assert_eq!(turn_indexes, vec![7, 8]);
    assert!(chunk_rows.is_empty());
    assert_eq!(search_rows.len(), 1);
}

/// Verifies that one-session maintenance row building does not require a full session list.
/// 验证单会话维护行构建不需要依赖完整会话列表。
#[test]
fn single_session_message_rows_can_be_built_for_maintenance() {
    let session_id = SessionId::from_u128(45);
    let messages = vec![
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "第一轮"),
        SessionMessageEntry::new(
            SessionMessageKind::Assistant,
            "AI 输出",
            &"维护路径正文 ".repeat(10_000),
        ),
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "第二轮"),
    ];
    let mut message_rows = Vec::new();
    let mut chunk_rows = Vec::new();
    let mut search_rows = Vec::new();

    push_session_message_rows_for_session(
        &BTreeMap::new(),
        -1,
        -1,
        &mut message_rows,
        &mut chunk_rows,
        &mut search_rows,
        "project-1",
        true,
        session_id,
        "会话标题",
        &messages,
    );

    assert_eq!(message_rows.len(), 3);
    assert!(!chunk_rows.is_empty());
    assert!(message_rows.iter().all(|row| {
        matches!(&row[1], ControllerSqliteValue::String(value) if value == session_id.to_string().as_str())
    }));
    let turn_indexes = message_rows
        .iter()
        .map(|row| match row[6] {
            ControllerSqliteValue::Int64(value) => value,
            ref other => panic!("expected turn index integer, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(turn_indexes, vec![0, 0, 1]);
    assert_eq!(search_rows.len(), 2);
}

/// Verifies that default transcript windows keep tool details out of the mainline query profile.
/// 验证默认转录窗口会将工具明细排除在主线查询画像之外。
#[test]
fn default_transcript_window_filters_tool_detail_kinds() {
    assert!(persisted_kind_in_default_transcript_window(
        "assistant-output"
    ));
    assert!(persisted_kind_in_default_transcript_window(
        "summary-output"
    ));
    assert!(!persisted_kind_in_default_transcript_window(
        "tool-call-output"
    ));
    assert!(!persisted_kind_in_default_transcript_window(
        "tool-result-output"
    ));
}

/// Verifies that the on-demand tool-detail stream accepts only tool call and result rows.
/// 验证按需工具明细流只接收工具调用与工具结果行。
#[test]
fn tool_detail_stream_filters_only_tool_detail_kinds() {
    assert!(persisted_kind_in_tool_detail_stream("tool-call-output"));
    assert!(persisted_kind_in_tool_detail_stream("tool-result-output"));
    assert!(!persisted_kind_in_tool_detail_stream("assistant-output"));
    assert!(!persisted_kind_in_tool_detail_stream("summary-output"));
}

/// Verifies that transcript persistence uses upsert semantics instead of full-session deletion.
/// 验证转录持久化使用 upsert 语义，而不是整会话删除。
#[test]
fn session_message_persistence_statement_uses_upsert() {
    let statement = session_message_upsert_statement();

    assert!(statement.contains("ON CONFLICT(id) DO UPDATE"));
    assert!(!statement.contains("DELETE FROM session_messages"));
}

/// Verifies that chunk persistence tolerates overlapping maintenance tasks.
/// 验证分片持久化可以容忍重叠维护任务。
#[test]
fn session_message_chunk_persistence_statement_uses_upsert() {
    let statement = session_message_chunk_upsert_statement();

    assert!(statement.contains("ON CONFLICT(message_id, chunk_index) DO UPDATE"));
    assert!(statement.contains("content = excluded.content"));
}

/// Verifies that user search text is treated literally inside SQLite LIKE patterns.
/// 验证用户搜索文本会在 SQLite LIKE 模式中按字面量处理。
#[test]
fn session_history_search_like_pattern_escapes_wildcards() {
    assert_eq!(
        session_history_search_like_pattern(r"src\foo_%"),
        r"%src\\foo\_\%%"
    );
}

/// Verifies that punctuation-heavy pasted text is normalized into safe literal FTS terms.
/// 验证带有大量标点的粘贴文本会被规范化为安全的字面量 FTS 词项。
#[test]
fn normalize_session_history_search_query_strips_fts_syntax_punctuation() {
    assert_eq!(
        normalize_session_history_search_query("50 TOPS (NPU)"),
        "50 TOPS NPU"
    );
    assert_eq!(
        normalize_session_history_search_query("  MiniMax / NPU : search  "),
        "MiniMax NPU search"
    );
}

/// Verifies that reserved SQLite FTS operators are downgraded to searchable literals.
/// 验证 SQLite FTS 保留操作符会被降级为可检索的普通字面量。
#[test]
fn normalize_session_history_search_query_downgrades_reserved_operators() {
    assert_eq!(
        normalize_session_history_search_query("AND (GPU) NEAR NPU OR"),
        "and GPU near NPU or"
    );
}

/// Verifies that oversized pasted history-search text is bounded before FTS parsing.
/// 验证超长粘贴历史搜索文本会在进入 FTS 解析前被限制长度。
#[test]
fn normalize_session_history_search_query_limits_pasted_text() {
    let query = "a".repeat(SESSION_HISTORY_SEARCH_QUERY_MAX_CHARS + 64);

    assert_eq!(
        normalize_session_history_search_query(&query)
            .chars()
            .count(),
        SESSION_HISTORY_SEARCH_QUERY_MAX_CHARS
    );
}

/// Verifies that lightweight search previews stay compact and non-empty.
/// 验证轻量搜索预览会保持紧凑且非空。
#[test]
fn session_history_search_preview_compacts_text() {
    let preview = session_history_search_preview(
        "第一行命中内容\n\n第二行包含 src/main.rs\n第三行还有更多上下文\n第四行应被截断",
    );

    assert!(preview.contains("第一行命中内容"));
    assert!(preview.contains("src/main.rs"));
    assert!(!preview.contains("第四行应被截断"));
}

/// Verifies that project-scoped controller FTS index names stay deterministic and use the dedicated prefix.
/// 验证项目级控制器 FTS 索引名保持确定性并使用专用前缀。
#[test]
fn workspace_history_search_project_index_name_is_stable() {
    let first = workspace_history_search_project_index_name("project-alpha");
    let second = workspace_history_search_project_index_name("project-alpha");
    let third = workspace_history_search_project_index_name("project-beta");

    assert_eq!(first, second);
    assert_ne!(first, third);
    assert!(first.starts_with(WORKSPACE_HISTORY_SEARCH_PROJECT_INDEX_PREFIX));
}

/// Verifies that controller FTS preview fragments strip highlight tags before compact rendering.
/// 验证控制器 FTS 预览片段会在紧凑渲染前移除高亮标签。
#[test]
fn workspace_history_search_preview_strips_mark_tags() {
    let preview = workspace_history_search_preview(
        "第一行 <mark>命中</mark>\n第二行继续\n第三行还有内容\n第四行应被截断",
    );

    assert!(preview.contains("第一行 命中"));
    assert!(!preview.contains("<mark>"));
    assert!(!preview.contains("第四行应被截断"));
}

/// Verifies that raw preview markup keeps the original `<mark>` tags for UI-side highlight rendering.
/// 验证原始预览标记会保留 `<mark>` 标签，供 UI 侧渲染命中高亮。
#[test]
fn workspace_history_search_preview_markup_preserves_mark_tags() {
    let preview = workspace_history_search_preview_markup("第一行 <mark>命中</mark>");

    assert!(preview.contains("<mark>命中</mark>"));
}

/// Verifies that only visible user prompts and visible assistant final outputs enter the workspace BM25 index.
/// 验证只有可见用户提问与可见助手最终回复会进入工作台 BM25 索引。
#[test]
fn workspace_history_search_indexable_messages_filter_intermediate_records() {
    assert!(session_message_is_workspace_history_search_indexable(
        &SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "继续")
    ));
    assert!(session_message_is_workspace_history_search_indexable(
        &SessionMessageEntry {
            record_kind: SessionMessageRecordKind::AssistantOutput,
            ..SessionMessageEntry::new(SessionMessageKind::Assistant, "AI 输出", "最终答案")
        }
    ));
    assert!(!session_message_is_workspace_history_search_indexable(
        &SessionMessageEntry::hidden(
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolResultOutput,
            "工具返回",
            "中间结果"
        )
    ));
    assert!(!session_message_is_workspace_history_search_indexable(
        &SessionMessageEntry::hidden(
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ExploreOutput,
            "探索",
            "分析过程"
        )
    ));
}

/// Verifies that context windows never request negative turn indexes.
/// 验证上下文窗口不会请求负数轮次索引。
#[test]
fn session_history_context_turn_bounds_clamp_lower_edge() {
    assert_eq!(session_history_context_turn_bounds(1, 2), (0, 3));
    assert_eq!(session_history_context_turn_bounds(-5, 2), (0, 2));
}

/// Verifies that context windows keep a stable inclusive turn span around the anchor.
/// 验证上下文窗口会围绕锚点保留稳定的闭区间轮次范围。
#[test]
fn session_history_context_turn_bounds_expand_around_anchor() {
    assert_eq!(session_history_context_turn_bounds(12, 2), (10, 14));
    assert_eq!(session_history_context_turn_bounds(12, -1), (12, 12));
}

/// Verifies that legacy migration candidates do not project full message content.
/// 验证旧数据迁移候选查询不会投影完整消息正文。
#[test]
fn legacy_oversized_message_candidate_query_avoids_full_content_projection() {
    let statement = legacy_oversized_message_candidate_query();
    let normalized = statement
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    assert!(normalized.contains("length(cast(m.content as blob)) as content_bytes"));
    assert!(normalized.contains("length(m.content) as content_chars"));
    assert!(normalized.contains("not exists"));
    assert!(normalized.contains("from session_message_chunks"));
    assert!(!normalized.contains("select m.id, m.content"));
    assert!(!normalized.contains("substr(m.content"));
}

/// Verifies that legacy migration scripts split content inside SQLite before shrinking inline text.
/// 验证旧数据迁移脚本会在 SQLite 内部分片后再收缩内联文本。
#[test]
fn legacy_inline_message_chunk_migration_script_uses_sqlite_substr() {
    let session_id = SessionId::from_u128(55);
    let script = legacy_inline_message_chunk_migration_script(session_id, "message-'quoted");

    assert!(script.contains("BEGIN IMMEDIATE TRANSACTION"));
    assert!(script.contains("INSERT OR REPLACE INTO session_message_chunks"));
    assert!(script.contains("substr(source.content_text"));
    assert!(script.contains("NOT EXISTS"));
    assert!(script.contains("UPDATE session_messages"));
    assert!(script.contains("SET content = substr(content"));
    assert!(script.contains("length(CAST(content AS BLOB))"));
    assert!(script.contains("message-''quoted"));
    assert!(script.contains("CROSS JOIN digits"));
    assert!(!script.contains("DELETE FROM session_message_chunks"));
    assert!(script.contains("COMMIT"));
}

/// Verifies that legacy chunk-count estimates use ceiling division over text characters.
/// 验证旧数据分片数量估算会基于文本字符数做向上取整。
#[test]
fn legacy_inline_message_chunk_count_rounds_up() {
    let chunk_chars = SESSION_LEGACY_INLINE_MIGRATION_CHUNK_CHARS as i64;

    assert_eq!(legacy_inline_message_chunk_count(0).unwrap(), 0);
    assert_eq!(legacy_inline_message_chunk_count(chunk_chars).unwrap(), 1);
    assert_eq!(
        legacy_inline_message_chunk_count(chunk_chars + 1).unwrap(),
        2
    );
}

/// Verifies that visible pending prompt-panel rows keep the transcript UI state persistable across restarts.
/// 验证可见待发送提示词面板行会让转录界面状态跨重启保持可持久化。
#[test]
fn session_ui_state_json_preserves_pending_prompt_panel_queue() {
    let state = SessionTranscriptUiState {
        pending_prompt_panel_queue: vec![PendingPromptPanelEntry {
            id: String::from("panel-1"),
            prompt: String::from("继续排队消息"),
            preview: String::from("继续排队消息"),
            attachments: Vec::new(),
            model_reference: None,
            runtime_options: crate::model::PendingPromptRuntimeOptions::default(),
            state: PendingPromptPanelEntryState::Ready,
            created_at: String::from("1"),
        }],
        ..SessionTranscriptUiState::default()
    };

    let json_payload = serialize_session_ui_state_to_json(&state)
        .expect("pending panel queue should make UI state persistable");
    let parsed = parse_session_ui_state_from_json(&json_payload)
        .expect("pending panel queue UI state should parse");

    assert_eq!(parsed.pending_prompt_panel_queue.len(), 1);
    assert_eq!(parsed.pending_prompt_panel_queue[0].prompt, "继续排队消息");
}

/// Verifies that session-scoped composer draft state round-trips through the persisted transcript UI-state JSON payload.
/// 验证按会话归属的输入框草稿状态可以在持久化转录界面状态 JSON 载荷中正确往返。
#[test]
fn session_ui_state_json_preserves_composer_draft() {
    let state = SessionTranscriptUiState {
        composer_draft: crate::model::SessionComposerDraftState {
            text: String::from("请继续修复这个问题"),
            attachments: Vec::new(),
            scroll_offset: 24.0,
        },
        ..SessionTranscriptUiState::default()
    };

    let json_payload = serialize_session_ui_state_to_json(&state)
        .expect("composer draft should make UI state persistable");
    let parsed = parse_session_ui_state_from_json(&json_payload)
        .expect("composer draft UI state should parse");

    assert_eq!(parsed.composer_draft.text, "请继续修复这个问题");
    assert_eq!(parsed.composer_draft.scroll_offset, 24.0);
}

/// Verifies that runtime persistence now keeps the full canonical conversation state payload alongside the lightweight continuation cursor.
/// 验证运行时持久化现在会保留完整规范会话状态载荷，同时继续保存轻量 continuation 游标。
#[test]
fn runtime_state_rows_store_full_canonical_conversation_state() {
    let session_id = SessionId::from_u128(43);
    let session = SessionSummary::new(
        session_id,
        None,
        String::from("测试会话"),
        String::new(),
        SessionState::Draft,
        0,
        0,
        String::from("刚刚"),
    );
    let runtime_state = SessionRuntimeStateEntry {
        conversation_state: SessionConversationState {
            messages: Vec::new(),
            records: vec![crate::conversation_protocol::SessionConversationRecord {
                id: String::from("record-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolResultOutput,
                visible: true,
                title: String::from("工具返回"),
                body: "大型正文 ".repeat(10_000),
                metadata: None,
                created_at: String::from("1"),
            }],
            continuation: SessionProviderContinuationState {
                provider_id: Some(String::from("openai")),
                model_id: Some(String::from("gpt-test")),
                last_provider_message_id: Some(String::from("resp-1")),
                updated_at: Some(String::from("123")),
            },
        },
        updated_at: String::from("1"),
        ..SessionRuntimeStateEntry::default()
    };
    let mut runtime_states = BTreeMap::new();
    runtime_states.insert(session_id, runtime_state);
    let mut rows = Vec::new();

    push_session_runtime_state_batch_rows(&mut rows, &[session], &runtime_states);

    assert_eq!(rows.len(), 1);
    assert!(matches!(&rows[0][8], ControllerSqliteValue::Null));
    let continuation_json = match &rows[0][9] {
        ControllerSqliteValue::String(value) => value,
        other => panic!("expected continuation JSON string, got {other:?}"),
    };
    assert!(continuation_json.contains("resp-1"));
}

/// Verifies that the single-row runtime-state writer uses the same row shape as the batch writer.
/// 验证单行运行时状态写入器与批量写入器使用完全相同的行结构。
#[test]
fn single_runtime_state_row_matches_batch_runtime_state_row() {
    let session_id = SessionId::from_u128(143);
    let session = SessionSummary::new(
        session_id,
        None,
        String::from("单行运行时状态"),
        String::new(),
        SessionState::Draft,
        0,
        0,
        String::from("刚刚"),
    );
    let runtime_state = SessionRuntimeStateEntry {
        active_agent_id: Some(String::from("agent-main")),
        active_provider_id: Some(String::from("provider-main")),
        active_model_id: Some(String::from("model-main")),
        context_start_message_id: Some(String::from("message-start")),
        updated_at: String::from("1234"),
        ..SessionRuntimeStateEntry::default()
    };
    let mut runtime_states = BTreeMap::new();
    runtime_states.insert(session_id, runtime_state.clone());
    let mut rows = Vec::new();

    push_session_runtime_state_batch_rows(&mut rows, &[session], &runtime_states);
    let single_row = build_session_runtime_state_batch_row(session_id, &runtime_state);

    assert_eq!(rows, vec![single_row]);
}

/// Verifies that hidden context-injection state persists in the runtime-state row without touching conversation JSON.
/// 验证隐藏上下文注入状态会进入运行时状态行，同时不写入会话正文 JSON。
#[test]
fn runtime_state_rows_store_context_injection_state_separately() {
    let session_id = SessionId::from_u128(44);
    let session = SessionSummary::new(
        session_id,
        None,
        String::from("注入测试会话"),
        String::new(),
        SessionState::Draft,
        0,
        0,
        String::from("刚刚"),
    );
    let mut injection_state = ContextInjectionState::default();
    injection_state.apply_insert(
        InjectionInsert::single(
            InjectionKind::Transient,
            "manual_inject",
            "临时说明",
            "隐藏内容",
            Some(3),
        ),
        "1",
    );
    let runtime_state = SessionRuntimeStateEntry {
        context_injection_state: injection_state,
        updated_at: String::from("1"),
        ..SessionRuntimeStateEntry::default()
    };
    let mut runtime_states = BTreeMap::new();
    runtime_states.insert(session_id, runtime_state);
    let mut rows = Vec::new();

    push_session_runtime_state_batch_rows(&mut rows, &[session], &runtime_states);

    assert_eq!(rows.len(), 1);
    assert!(matches!(&rows[0][8], ControllerSqliteValue::Null));
    let context_injection_json = match &rows[0][10] {
        ControllerSqliteValue::String(value) => value,
        other => panic!("expected context injection JSON string, got {other:?}"),
    };
    let parsed =
        parse_context_injection_state_from_json(context_injection_json).expect("state parses");
    assert_eq!(
        parsed.panel_cards(),
        vec![InjectionPanelCard {
            tag: String::from("manual_inject"),
            description: String::from("临时说明"),
            items: vec![vulcan_context_injection::InjectionPanelItem {
                content: String::from("隐藏内容"),
                detail: String::from("隐藏内容"),
                turn_label: None,
                ttl_label: String::from("TTL: 3"),
                source_display: None,
            }],
        }]
    );
}

/// Verifies that session-level VMM overrides persist beside hidden context injection state.
/// 验证会话级 VMM 覆盖会与隐藏上下文注入状态一起持久化。
#[test]
fn runtime_state_rows_store_vmm_overrides_separately() {
    let session_id = SessionId::from_u128(45);
    let session = SessionSummary::new(
        session_id,
        None,
        String::from("VMM 测试会话"),
        String::new(),
        SessionState::Draft,
        0,
        0,
        String::from("刚刚"),
    );
    let runtime_state = SessionRuntimeStateEntry {
        vmm: VmmConfigLayer {
            enabled: Some(false),
            ai_tools_enabled: Some(false),
            ..VmmConfigLayer::default()
        },
        updated_at: String::from("1"),
        ..SessionRuntimeStateEntry::default()
    };
    let mut runtime_states = BTreeMap::new();
    runtime_states.insert(session_id, runtime_state);
    let mut rows = Vec::new();

    push_session_runtime_state_batch_rows(&mut rows, &[session], &runtime_states);

    assert_eq!(rows.len(), 1);
    assert!(matches!(&rows[0][8], ControllerSqliteValue::Null));
    assert!(matches!(&rows[0][10], ControllerSqliteValue::Null));
    let vmm_json = match &rows[0][11] {
        ControllerSqliteValue::String(value) => value,
        other => panic!("expected VMM JSON string, got {other:?}"),
    };
    let parsed = parse_vmm_config_layer_from_json(vmm_json).expect("VMM config parses");
    assert_eq!(parsed.enabled, Some(false));
    assert_eq!(parsed.ai_tools_enabled, Some(false));
}

/// Verifies that queued VMM writebacks persist separately from conversation JSON.
/// 验证排队的 VMM 写回会独立于会话正文 JSON 持久化。
#[test]
fn runtime_state_rows_store_vmm_writeback_outbox_separately() {
    let session_id = SessionId::from_u128(46);
    let session = SessionSummary::new(
        session_id,
        None,
        String::from("VMM outbox 测试会话"),
        String::new(),
        SessionState::Draft,
        0,
        0,
        String::from("刚刚"),
    );
    let runtime_state = SessionRuntimeStateEntry {
        vmm_writeback_outbox: vec![VmmWritebackOutboxEntry {
            id: String::from("outbox-1"),
            request: PostActionRequest {
                session_id: session_id.to_string(),
                user_id: 1,
                project_id: 2,
                user_content: String::from("user"),
                assistant_content: String::from("assistant"),
                timeline: Vec::new(),
            },
            attempt_count: 2,
            last_error: Some(String::from("transport failed")),
            created_at: String::from("1"),
            updated_at: String::from("2"),
        }],
        updated_at: String::from("2"),
        ..SessionRuntimeStateEntry::default()
    };
    let mut runtime_states = BTreeMap::new();
    runtime_states.insert(session_id, runtime_state);
    let mut rows = Vec::new();

    push_session_runtime_state_batch_rows(&mut rows, &[session], &runtime_states);

    assert_eq!(rows.len(), 1);
    assert!(matches!(&rows[0][8], ControllerSqliteValue::Null));
    let outbox_json = match &rows[0][12] {
        ControllerSqliteValue::String(value) => value,
        other => panic!("expected VMM outbox JSON string, got {other:?}"),
    };
    let parsed = parse_vmm_writeback_outbox_from_json(outbox_json).expect("outbox parses");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].id, "outbox-1");
    assert_eq!(parsed[0].request.user_content, "user");
    assert_eq!(parsed[0].attempt_count, 2);
}

/// Verifies that VMM profile sync state persists separately from conversation JSON.
/// 验证 VMM 画像同步状态会独立于会话正文 JSON 持久化。
#[test]
fn runtime_state_rows_store_vmm_profile_sync_separately() {
    let session_id = SessionId::from_u128(47);
    let session = SessionSummary::new(
        session_id,
        None,
        String::from("VMM profile sync 测试会话"),
        String::new(),
        SessionState::Draft,
        0,
        0,
        String::from("刚刚"),
    );
    let runtime_state = SessionRuntimeStateEntry {
        vmm_profile_sync: VmmProfileSyncState {
            covered_compaction_id: Some(String::from("cp-1")),
            completed_turns_since_sync: 3,
            base_profile_content: Some(String::from("profile prompt")),
            delta_profile_content: Some(String::from("profile delta")),
            updated_at: Some(String::from("2")),
            ..VmmProfileSyncState::default()
        },
        updated_at: String::from("2"),
        ..SessionRuntimeStateEntry::default()
    };
    let mut runtime_states = BTreeMap::new();
    runtime_states.insert(session_id, runtime_state);
    let mut rows = Vec::new();

    push_session_runtime_state_batch_rows(&mut rows, &[session], &runtime_states);

    assert_eq!(rows.len(), 1);
    assert!(matches!(&rows[0][8], ControllerSqliteValue::Null));
    let profile_sync_json = match &rows[0][13] {
        ControllerSqliteValue::String(value) => value,
        other => panic!("expected VMM profile sync JSON string, got {other:?}"),
    };
    let parsed = parse_vmm_profile_sync_from_json(profile_sync_json).expect("profile sync parses");
    assert_eq!(parsed.covered_compaction_id.as_deref(), Some("cp-1"));
    assert_eq!(parsed.completed_turns_since_sync, 3);
    assert_eq!(
        parsed.base_profile_content.as_deref(),
        Some("profile prompt")
    );
    assert_eq!(
        parsed.delta_profile_content.as_deref(),
        Some("profile delta")
    );
}

/// Verifies that rebuilding runtime state from recent messages preserves provider continuation.
/// 验证基于最近消息重建运行时状态时会保留 Provider continuation。
#[test]
fn runtime_state_rebuild_from_recent_messages_preserves_continuation() {
    let runtime_state = SessionRuntimeStateEntry {
        conversation_state: SessionConversationState {
            continuation: SessionProviderContinuationState {
                provider_id: Some(String::from("openai")),
                model_id: Some(String::from("gpt-test")),
                last_provider_message_id: Some(String::from("resp-1")),
                updated_at: Some(String::from("123")),
            },
            ..SessionConversationState::default()
        },
        ..SessionRuntimeStateEntry::default()
    };
    let messages = vec![SessionMessageEntry {
        id: String::from("message-1"),
        kind: SessionMessageKind::User,
        record_kind: SessionMessageRecordKind::Transcript,
        visible: true,
        title: String::from("用户指令"),
        body: String::from("继续。"),
        metadata: None,
        created_at: String::from("1"),
    }];

    let continuation = runtime_state.conversation_state.continuation.clone();
    let mut rebuilt = runtime_state;
    rebuilt.conversation_state = conversation_state_from_session_messages(&messages);
    rebuilt.conversation_state.continuation = continuation;

    assert_eq!(rebuilt.conversation_state.records.len(), 1);
    assert_eq!(
        rebuilt
            .conversation_state
            .continuation
            .last_provider_message_id
            .as_deref(),
        Some("resp-1")
    );
}
