//! V1 storage contract tests and fault-injection matrix.
//! V1 存储契约测试与故障注入矩阵。
//!
//! Covers the plan's sections 13.2 (storage contract) and 13.3 (fault
//! injection): initial file sets, strict load, tail rewrite without touching
//! sealed segments, rollover at configurable thresholds, oversized events,
//! crash windows between files, and per-stage atomic-write failures proving
//! the old published state stays intact.
//! 覆盖计划 13.2(存储契约)与 13.3(故障注入)：初始文件集、严格加载、尾段
//! 重写不触碰封口分片、可配置阈值滚段、超大事件、文件间崩溃窗口，以及逐阶段
//! 原子写失败证明旧已发布状态保持完好。

use std::collections::BTreeSet;
use std::path::PathBuf;

mod fingerprints;
mod metadata_migration;
mod user_prompt;

use vulcan_session_core::v1::config::{MetadataDocument, PendingRequestConfig};
use vulcan_session_core::v1::context::{
    CompactionNode, CompactionsDocument, LockedBaselineProjection, LockedBaselineSegmentInput,
    LockedPromptTarget, LockedSource, LockedSourceKind, RuntimeInitializationState,
};
use vulcan_session_core::v1::document::{
    ApiCompletion, ClientInputSubmission, CompactionCommitInput, ContinuationRequest, DirtyFile,
    EditGenerationInput, ForkGenerationInput, PrepareAndStartApiInput, RootInputReplacement,
    SessionDocument, ToolCompletion,
};
use vulcan_session_core::v1::event::*;
use vulcan_session_core::v1::ids::{
    ApiCallId, ClientActionId, CompactionId, RequestStreamId, Seq, TurnId,
};
use vulcan_session_core::v1::projection::provider_visible_fingerprint;
use vulcan_session_core::v1::stats::UsageSnapshot;
use vulcan_session_core::v1::{
    GenerationBuildOutput, ProviderRequestBudgetEvidence, ProviderRequestContextBreakdown,
    ProviderRequestTextMeasurement,
};

use super::atomic_io::{WriteStage, test_hooks, write_file_atomic};
use super::documents::{read_compactions, read_metadata, read_runtime, write_json_document};
use super::errors::{ErrorCode, SessionV1Error};
use super::event_window::{
    load_provider_event_range, load_recent_turn_window, load_turn_window_before,
};
use super::generation::{PublishEditGenerationRequest, publish_edit_generation};
use super::layout::SessionV1Layout;
use super::loader::{load_session, load_session_tail_only};
use super::segment::{parse_segment, serialize_segment};
use super::writer::{
    FlushLimits, FlushRequest, SEGMENT_MAX_LINES, WrittenFile, create_session, flush_session,
    flush_session_with_limits,
};

/// Builds one strongly typed fixture submission with an explicit idempotency identity.
/// 构建一条带显式幂等身份的强类型测试提交。
fn test_submission(action_id: impl Into<String>, text: impl Into<String>) -> ClientInputSubmission {
    ClientInputSubmission::new(
        ClientActionId::try_new(action_id.into()).expect("valid fixture client action id"),
        text,
        Vec::new(),
    )
}

/// Submits one root input and extracts its exact typed Turn receipt.
/// 提交一条根输入并提取其精确强类型 Turn 回执。
fn submit_test_root(
    document: &mut SessionDocument,
    action_id: impl Into<String>,
    text: impl Into<String>,
    now_ms: i64,
) -> TurnId {
    document
        .submit_root_input(test_submission(action_id, text), now_ms)
        .expect("submit fixture root input")
        .into_value()
}

/// Rebuilds the canonical locked baseline from the document's authoritative sources.
/// 从文档权威来源重建规范锁定基线。
fn locked_baseline(document: &SessionDocument) -> LockedBaselineProjection {
    locked_baseline_from_sources(&document.runtime_lock.locked_sources)
}

/// Rebuilds one canonical baseline from an explicit ordered source slice.
/// 从一组显式有序来源重建规范基线。
fn locked_baseline_from_sources(sources: &[LockedSource]) -> LockedBaselineProjection {
    LockedBaselineProjection::project(
        sources
            .iter()
            .map(|source| LockedBaselineSegmentInput {
                kind: source.kind,
                node_id: source.node_id.clone(),
                target: source.target,
                source_label: source.source_label.clone(),
                source_id: source.source_id.clone(),
                content: source.content.clone(),
            })
            .collect(),
    )
    .expect("fixture locked baseline must remain canonical")
}

/// Builds one edited generation from the exact durable document currently published on disk.
/// 从磁盘当前已发布的精确持久化文档构建一份编辑 generation。
fn build_durable_edit_output(
    layout: &SessionV1Layout,
    target_seq: Seq,
    replacement_text: impl Into<String>,
    now_ms: i64,
) -> GenerationBuildOutput {
    let loaded = load_session(layout).expect("published edit source must load");
    loaded
        .document
        .build_edited_generation(EditGenerationInput {
            target_seq,
            replacement: RootInputReplacement::new(replacement_text, Vec::new()),
            now_ms,
        })
        .expect("durable edit source must build one complete Core output")
}

/// Builds one child generation from the exact durable parent currently published on disk.
/// 从磁盘当前已发布的精确父会话构建一份子 generation。
fn build_durable_fork_output(
    source_layout: &SessionV1Layout,
    child_layout: &SessionV1Layout,
    source_turn_end_seq: Seq,
    now_ms: i64,
) -> GenerationBuildOutput {
    let loaded = load_session(source_layout).expect("published fork source must load");
    loaded
        .document
        .build_fork_generation(ForkGenerationInput {
            child_session_id: child_layout.session_id.clone(),
            title: loaded.document.metadata.title.clone(),
            source_turn_end_seq,
            now_ms,
        })
        .expect("durable fork source must build one complete Core output")
}

/// Builds one session document with a completed single turn of events.
/// 构建一个含完整单回合事件的会话文档。
fn filled_document(session_id: &str, extra_events: usize) -> SessionDocument {
    let mut document = SessionDocument::new(session_id, "project-1", "title", 1_700_000_000_000);
    let turn_id = submit_test_root(
        &mut document,
        "workspace-store-filled-root",
        "hello",
        1_700_000_000_000,
    );
    let api = prepare_api(
        &mut document,
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        0,
    );
    document
        .append_reasoning(
            api,
            "thinking",
            AssistantSemanticStatus::Completed,
            1_700_000_000_000,
        )
        .unwrap();
    document
        .append_assistant_text(
            api,
            "hi there",
            AssistantSemanticStatus::Completed,
            1_700_000_000_000,
        )
        .unwrap();
    let tool = document
        .prepare_tool(
            api,
            RequestStreamId::try_new("fixture-stream-pt1").unwrap(),
            "file_edit",
            ToolPayload::try_new(serde_json::json!({"path": "a.txt"})).unwrap(),
            Some("pt1".to_string()),
            1_700_000_000_000,
        )
        .unwrap();
    document
        .start_prepared_tool(api, tool, 1_700_000_000_000)
        .unwrap();
    document
        .finish_tool(
            api,
            tool,
            ToolCompletion {
                output: Some(ToolPayload::try_new(serde_json::json!({"ok": true})).unwrap()),
                changeset: None,
                status: ToolCallStatus::Completed,
                finished_at_ms: 1_700_000_000_000,
                external_refs: Vec::new(),
                generated_resources: Vec::new(),
            },
        )
        .unwrap();
    complete_api(
        &mut document,
        api,
        1_700_000_000_000,
        Some(ApiEndReason::CompletedSuccessfully),
    );
    document
        .close_turn(TurnOutcome::Completed, 3, 1_700_000_000_000)
        .unwrap();
    let _ = turn_id;
    for index in 0..extra_events {
        document
            .append_display_note(
                None,
                DisplayNoticeKind::SystemError,
                NoticeSeverity::Info,
                format!("note {index}"),
                None,
                None,
                None,
                None,
                1_700_000_000_000 + index as i64,
            )
            .unwrap();
    }
    document
}

/// Builds a fresh layout rooted at one unique temp directory.
/// 在唯一临时目录下构建一个全新布局。
fn fresh_layout(tag: &str) -> (tempfile::TempDir, SessionV1Layout) {
    let root = tempfile::tempdir().unwrap();
    let layout = SessionV1Layout::new(
        root.path().join("code").join("sessions"),
        "project-1",
        "2026-08-26",
        format!("sess-{tag}-{}", std::process::id()),
    );
    (root, layout)
}

/// Recreates the exact legacy closed-turn Goal edit and verifies safe, idempotent persistence.
/// 重现精确的旧闭合回合目标编辑缺陷，验证安全且幂等的持久化。
#[test]
fn goal_owner_recovery_preserves_events_and_rejects_unrelated_corruption() {
    let (_root, layout) = fresh_layout("goal-owner");
    let mut document = SessionDocument::new(layout.session_id.clone(), "project-1", "goal", 1);
    document.create_goal("work", None, "work", 1).unwrap();
    document.block_goal("goal_blocked", 2).unwrap();
    let turn = document.active_turn().unwrap().turn_id;
    document.close_turn(TurnOutcome::Completed, 2, 3).unwrap();
    document
        .update_goal_definition("edited", None, "edited", 4)
        .unwrap();
    let metadata = publish_initial(&layout, &document);
    let tail = layout.segment_entry_path(
        metadata.segment_generation,
        &metadata.split_list[0].file_name,
    );
    let mut events = document.events().to_vec();
    for event in events.iter_mut().rev().take(2) {
        event.turn_id = Some(turn);
    }
    let malformed = serialize_segment(&events).unwrap();
    std::fs::write(&tail, &malformed).unwrap();
    assert!(load_session_tail_only(&layout).is_err());
    let (mut recovered, _) = super::loader::load_session_tail_for_runtime(&layout).unwrap();
    assert!(
        recovered
            .document
            .dirty_files()
            .any(|file| file == DirtyFile::ActiveTail)
    );
    for (original, corrected) in events.iter().zip(recovered.document.events()) {
        assert_eq!(original.seq, corrected.seq);
        assert_eq!(original.event, corrected.event);
    }
    assert_eq!(
        std::fs::read(&tail).unwrap(),
        malformed,
        "loading never writes source"
    );
    // A failed metadata switch must leave the original corrupt bank untouched and recoverable again.
    // metadata 切换失败必须保持原损坏银行文件不变，且仍能再次恢复。
    let before_metadata = std::fs::read(layout.metadata_path()).unwrap();
    let candidate = recovered
        .document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap();
    let dirty = candidate.dirty.iter().copied().collect::<BTreeSet<_>>();
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &candidate.metadata,
        runtime_lock: &candidate.runtime_lock,
        runtime_updated_at_ms: 5,
        compactions: &candidate.compactions,
        events: &candidate.tail_events,
        tail_start: candidate
            .metadata
            .split_list
            .last()
            .map(|segment| segment.start_seq),
    };
    test_hooks::fail_next_artifact(WriteStage::Rename, "metadata");
    assert!(flush_session(&layout, &request).is_err());
    test_hooks::clear_failures();
    assert_eq!(
        std::fs::read(layout.metadata_path()).unwrap(),
        before_metadata
    );
    assert_eq!(std::fs::read(&tail).unwrap(), malformed);
    flush_candidate(&mut recovered.document, &layout, 5);
    let (loaded, _) = load_session_tail_only(&layout).unwrap();
    assert!(
        loaded
            .document
            .events()
            .iter()
            .rev()
            .take(2)
            .all(|event| event.turn_id.is_none())
    );
    let (second, _) = super::loader::load_session_tail_for_runtime(&layout).unwrap();
    assert_eq!(
        second.document.memory_revision(),
        second.document.durable_revision()
    );
    // A duplicated boundary is not an ownership defect and must still be rejected.
    // 重复边界并非归属缺陷，仍必须拒绝。
    events.last_mut().unwrap().event = EventBody::TurnBoundary(TurnBoundaryEvent {
        reason: vulcan_session_core::v1::event::TurnClosureReason::Completed,
        outcome: TurnOutcome::Completed,
        purpose: vulcan_session_core::v1::event::TurnBoundaryPurpose::Business,
    });
    std::fs::write(&tail, serialize_segment(&events).unwrap()).unwrap();
    let mut old_metadata = metadata;
    old_metadata.split_list[0].byte_len = std::fs::metadata(&tail).unwrap().len();
    write_json_document(&layout.metadata_path(), &old_metadata, "metadata").unwrap();
    assert!(super::loader::load_session_tail_for_runtime(&layout).is_err());
}

/// Validates a user-supplied failing snapshot using only an isolated copy of its declared files.
/// 仅使用声明文件的隔离副本验证用户提供的失败快照。
/// Set VULCAN_GOAL_RECOVERY_METADATA to an exact metadata.json path; original bytes remain untouched.
/// 将 VULCAN_GOAL_RECOVERY_METADATA 设为精确 metadata.json 路径；原始字节保持不变。
#[test]
#[ignore = "requires an explicit failing session snapshot"]
fn real_goal_failure_snapshot_recovers_without_losing_events() {
    let source_metadata = PathBuf::from(
        std::env::var("VULCAN_GOAL_RECOVERY_METADATA").expect("exact snapshot metadata path"),
    );
    let source = source_metadata.parent().unwrap();
    let metadata = read_metadata(&source_metadata).unwrap();
    let root = tempfile::tempdir().unwrap();
    let layout = SessionV1Layout::new(
        root.path(),
        &metadata.project_tag,
        "2026-08-30",
        &metadata.session_id,
    );
    let mut targets = vec![
        layout.metadata_path(),
        layout.runtime_path(metadata.active_context_generation),
        layout.compactions_path(metadata.active_context_generation),
    ];
    targets.extend(
        metadata
            .split_list
            .iter()
            .map(|entry| layout.segment_entry_path(metadata.segment_generation, &entry.file_name)),
    );
    let mut original_files = Vec::new();
    for target in targets {
        let original = source.join(target.strip_prefix(layout.session_dir()).unwrap());
        let bytes = std::fs::read(&original).unwrap();
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, &bytes).unwrap();
        original_files.push((original, bytes));
    }
    assert!(
        load_session_tail_only(&layout).is_err(),
        "snapshot must reproduce the load failure"
    );
    let (mut recovered, _) = super::loader::load_session_tail_for_runtime(&layout).unwrap();
    let original_events = recovered.document.events().to_vec();
    let draft = recovered.document.metadata.input_state.draft.clone();
    assert!(
        recovered
            .document
            .project_persisted_interruptions()
            .unwrap()
            > 0
    );
    flush_candidate(&mut recovered.document, &layout, metadata.updated_at_ms);
    let (mut reloaded, _) = load_session_tail_only(&layout).unwrap();
    assert_eq!(reloaded.document.metadata.input_state.draft, draft);
    for (before, after) in original_events.iter().zip(reloaded.document.events()) {
        assert_eq!(before.seq, after.seq);
        assert_eq!(before.event, after.event);
    }
    assert_eq!(
        reloaded.document.project_persisted_interruptions().unwrap(),
        0
    );
    reloaded
        .document
        .submit_root_input(
            test_submission("recovered-root", "continue after recovery"),
            metadata.updated_at_ms + 1,
        )
        .unwrap();
    for (path, bytes) in original_files {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}

/// One official usage snapshot used by fixtures.
/// 夹具使用的官方用量快照。
fn usage() -> UsageSnapshot {
    UsageSnapshot {
        input_tokens: Some(10),
        uncached_input_tokens: Some(10),
        output_tokens: Some(20),
        total_tokens: Some(30),
        ..UsageSnapshot::default()
    }
}

/// Builds deterministic valid Provider budget evidence for persistence fixtures.
/// 为持久化夹具构建确定且有效的 Provider 预算证据。
fn provider_request_budget_evidence() -> ProviderRequestBudgetEvidence {
    ProviderRequestBudgetEvidence {
        request_assembly_fingerprint: format!("sha256:{}", "0".repeat(64)),
        model_route_fingerprint: format!("sha256:{}", "1".repeat(64)),
        toolset_fingerprint: format!("sha256:{}", "2".repeat(64)),
        system_prompt_fingerprint: format!("sha256:{}", "3".repeat(64)),
        request_text_measurement: ProviderRequestTextMeasurement {
            character_count: 0,
            utf8_byte_count: 0,
            structural_units: 0,
            history_message_count: 0,
            prompt_in_history: false,
            history_fingerprint: format!("sha256:{}", "4".repeat(64)),
            prefix_invariant_fingerprint: format!("sha256:{}", "5".repeat(64)),
            context_breakdown: ProviderRequestContextBreakdown::default(),
        },
    }
}

/// Missing lifetime files reconstruct physical APIs per session and remain stable on later loads.
/// 缺失生命周期文件按各会话物理 API 补建，后续加载保持已存累计。
#[test]
fn session_usage_backfill_counts_root_and_child_api_facts_once() {
    use crate::workspace_store::load_session_lifetime_usage;
    use crate::workspace_store::session_v1_store::{
        CreateEmbeddedTaskSessionV1Request, create_embedded_task_session_v1,
        create_session_v1_storage_fixture, flush_session_v1_candidate, load_session_v1_full,
    };
    let temporary = tempfile::tempdir().unwrap();
    let locator = create_session_v1_storage_fixture(temporary.path(), "usage-root", 1).unwrap();
    let mut root = load_session_v1_full(&locator).unwrap().document;
    submit_test_root(&mut root, "usage-input", "work", 2);
    let api = prepare_api(
        &mut root,
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        3,
    );
    complete_api(&mut root, api, 4, None);
    let candidate = root.prepare_flush_candidates().unwrap().unwrap();
    flush_session_v1_candidate(&locator, &candidate).unwrap();
    let mut child =
        SessionDocument::new("usage-child", root.metadata.project_tag.clone(), "child", 2);
    submit_test_root(&mut child, "child-input", "work", 3);
    for now in [4, 6] {
        let api = prepare_api(
            &mut child,
            ApiPurposeKind::Subagent,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            now,
        );
        child
            .append_assistant_text(api, "child result", AssistantSemanticStatus::Completed, now)
            .unwrap();
        complete_api(&mut child, api, now + 1, None);
    }
    child.close_turn(TurnOutcome::Completed, 4, 8).unwrap();
    let created_child = create_embedded_task_session_v1(
        &locator,
        &CreateEmbeddedTaskSessionV1Request {
            metadata: &child.metadata,
            runtime_lock: &child.runtime_lock,
            compactions: &child.compactions,
            events: child.events(),
        },
    )
    .unwrap();
    let durable_child = load_session_v1_full(&created_child.locator)
        .unwrap()
        .document;
    let text = durable_child
        .events()
        .iter()
        .rev()
        .find(|event| matches!(event.event, EventBody::AssistantText(_)))
        .unwrap();
    let input = durable_child
        .build_subagent_return_input(
            vulcan_session_core::v1::document::SubagentReturnBuildRequest {
                final_turn_id: text.turn_id.unwrap(),
                child_task_id: vulcan_session_core::v1::ids::SubagentTaskId::try_new("usage-child")
                    .unwrap(),
                source_assistant_text_seq: text.seq,
            },
        )
        .unwrap();
    let mut durable_root = load_session_v1_full(&locator).unwrap().document;
    durable_root.append_subagent_return(input, 9).unwrap();
    flush_session_v1_candidate(
        &locator,
        &durable_root.prepare_flush_candidates().unwrap().unwrap(),
    )
    .unwrap();
    let rebuilt = load_session_lifetime_usage(&locator).unwrap();
    assert_eq!(rebuilt.main.request_count, 1);
    assert_eq!(rebuilt.children_total.request_count, 2);
    assert_eq!(
        rebuilt.children_by_session_id["usage-child"].usage,
        rebuilt.children_total
    );
    assert_eq!(
        rebuilt.main.output_tokens.sum,
        usage().output_tokens.unwrap()
    );
    assert!(!rebuilt.legacy_incomplete);
    assert_eq!(load_session_lifetime_usage(&locator).unwrap(), rebuilt);
    // Simulate missing legacy storage only inside this test's temporary root.
    // 仅在本测试临时根目录内模拟旧存储缺失。
    std::fs::remove_dir_all(created_child.locator.session_directory()).unwrap();
    std::fs::remove_file(locator.session_directory().join("usage.json")).unwrap();
    let partial = load_session_lifetime_usage(&locator).unwrap();
    assert!(partial.legacy_incomplete);
    assert_eq!(partial.main.request_count, 1);
    assert_eq!(partial.children_total.request_count, 0);
}

/// Starts one fixture API through the public atomic preparation contract.
/// 通过公开的原子准备契约启动一条夹具 API。
fn prepare_api(
    document: &mut SessionDocument,
    purpose: ApiPurposeKind,
    request: ApiRequestSnapshot,
    now_ms: i64,
) -> ApiCallId {
    prepare_api_with_continuation(document, purpose, request, None, now_ms)
}

/// Starts one fixture API with an exact optional continuation action binding.
/// 使用精确可选继续动作绑定启动一条测试 API。
fn prepare_api_with_continuation(
    document: &mut SessionDocument,
    purpose: ApiPurposeKind,
    request: ApiRequestSnapshot,
    continuation_action_id: Option<ClientActionId>,
    now_ms: i64,
) -> ApiCallId {
    let retained_from_seq = match document.runtime_lock.current_compaction_id {
        Some(compaction_id) => document
            .compactions
            .iter()
            .find(|node| node.compaction_id == compaction_id)
            .map(|node| node.retained_from_seq)
            .expect("fixture compaction pointer must reference one node"),
        None => Seq::try_new(1).unwrap(),
    };
    let through_seq = document
        .events()
        .last()
        .expect("fixture API requires one preceding event")
        .seq();
    let fingerprint_sha256 = provider_visible_fingerprint(document.events(), through_seq)
        .expect("valid fixture Provider fingerprint");
    let projection = ProviderProjectionProof {
        segment_generation: document.metadata.segment_generation,
        document_revision: document.memory_revision(),
        retained_from_seq,
        through_seq,
        fingerprint_sha256,
    };
    prepare_api_with_proof(
        document,
        purpose,
        request,
        continuation_action_id,
        projection,
        now_ms,
    )
}

/// Starts one storage-scale fixture API without reprojecting its entire synthetic history.
/// 启动一条存储规模夹具 API，且不重新投影其完整合成历史。
fn prepare_storage_scale_api(
    document: &mut SessionDocument,
    purpose: ApiPurposeKind,
    request: ApiRequestSnapshot,
    now_ms: i64,
) -> ApiCallId {
    let through_seq = document
        .events()
        .last()
        .expect("storage-scale API requires one preceding event")
        .seq();
    let projection = ProviderProjectionProof {
        segment_generation: document.metadata.segment_generation,
        document_revision: document.memory_revision(),
        retained_from_seq: Seq::try_new(1).expect("fresh scale fixture retains from seq one"),
        through_seq,
        fingerprint_sha256: "0".repeat(64),
    };
    prepare_api_with_proof(document, purpose, request, None, projection, now_ms)
}

/// Starts one fixture API with an explicitly supplied projection proof.
/// 使用显式提供的投影证明启动一条夹具 API。
fn prepare_api_with_proof(
    document: &mut SessionDocument,
    purpose: ApiPurposeKind,
    request: ApiRequestSnapshot,
    continuation_action_id: Option<ClientActionId>,
    projection: ProviderProjectionProof,
    now_ms: i64,
) -> ApiCallId {
    document
        .prepare_and_start_api(PrepareAndStartApiInput {
            purpose,
            request,
            projection,
            locked_baseline: locked_baseline(document),
            continuation_action_id,
            request_budget_evidence: provider_request_budget_evidence(),
            now_ms,
        })
        .unwrap()
        .into_value()
        .api_call_id
}

/// Completes one fixture API with the durable start identity.
/// 使用持久化起始身份闭合一次夹具 API。
fn complete_api(
    document: &mut SessionDocument,
    api_call_id: ApiCallId,
    completed_at_ms: i64,
    end_reason: Option<ApiEndReason>,
) {
    document
        .complete_api_group(
            api_call_id,
            ApiCompletion {
                request: ApiRequestSnapshot::new("openai", "gpt-5"),
                usage: usage(),
                status: ApiCallStatus::Completed,
                end_reason: end_reason.or(Some(ApiEndReason::CompletedSuccessfully)),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: None,
                completed_at_ms,
            },
        )
        .unwrap();
}

/// Returns the actual final completed Provider semantic for one compaction API.
/// 返回一次压缩 API 实际最后完成的 Provider 语义位置与正文。
fn completed_compaction_semantic(
    document: &SessionDocument,
    api_call_id: ApiCallId,
) -> (Seq, String) {
    document
        .events()
        .iter()
        .filter_map(|event| match &event.event {
            EventBody::AssistantReasoning(reasoning)
                if reasoning.api_call_id == api_call_id
                    && reasoning.status == AssistantSemanticStatus::Completed =>
            {
                Some((event.seq, reasoning.text.clone()))
            }
            EventBody::AssistantText(text)
                if text.api_call_id == api_call_id
                    && text.status == AssistantSemanticStatus::Completed =>
            {
                Some((event.seq, text.text.clone()))
            }
            _ => None,
        })
        .max_by_key(|(seq, _)| seq.value())
        .expect("compaction fixture must contain one completed Provider semantic")
}

/// Publishes the initial file set for one document.
/// 为一个文档发布初始文件集。
fn publish_initial(layout: &SessionV1Layout, document: &SessionDocument) -> MetadataDocument {
    create_session(
        layout,
        &document.metadata,
        &document.runtime_lock,
        1_700_000_000_000,
        &document.compactions,
        document.events(),
    )
    .unwrap()
}

/// Resolves the runtime file selected by the currently published metadata.
/// 解析当前已发布 metadata 选中的 runtime 文件。
fn active_runtime_path(layout: &SessionV1Layout) -> PathBuf {
    let generation = read_metadata(&layout.metadata_path())
        .unwrap()
        .active_context_generation;
    layout.runtime_path(generation)
}

/// Resolves the compactions file selected by the currently published metadata.
/// 解析当前已发布 metadata 选中的 compactions 文件。
fn active_compactions_path(layout: &SessionV1Layout) -> PathBuf {
    let generation = read_metadata(&layout.metadata_path())
        .unwrap()
        .active_context_generation;
    layout.compactions_path(generation)
}

/// The empty-session initial file set is complete and loads back.
/// 空会话初始文件集完整且可加载回来。
#[test]
fn initial_file_set_is_complete_and_loadable() {
    let (root, layout) = fresh_layout("init");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    assert!(layout.metadata_path().exists());
    assert!(active_runtime_path(&layout).exists());
    assert!(active_compactions_path(&layout).exists());
    assert!(!layout.session_dir().join("runtime.json").exists());
    assert!(!layout.session_dir().join("compactions.json").exists());
    assert!(layout.segment_path(1, 1).exists());
    let loaded = load_session(&layout).unwrap();
    assert_eq!(loaded.document.events().len(), document.events().len());
    assert_eq!(
        loaded.document.runtime_lock.initialization_state,
        document.runtime_lock.initialization_state
    );
    drop(root);
}

/// Every initial file and final directory publication failure leaves no addressable half session.
/// 每个初始文件与最终目录发布失败都不会留下可寻址的半会话。
#[test]
fn initial_creation_failures_leave_no_final_or_staging_directory() {
    let failures = [
        (WriteStage::WriteAll, "initial tail segment"),
        (WriteStage::WriteAll, "initial context compactions"),
        (WriteStage::WriteAll, "initial context runtime"),
        (WriteStage::WriteAll, "metadata"),
        (WriteStage::Rename, "session directory publication"),
    ];
    for (stage, artifact) in failures {
        let (root, layout) = fresh_layout(&format!("initial-failure-{stage:?}-{artifact}"));
        let document = SessionDocument::new(
            layout.session_id.clone(),
            "project-1",
            "title",
            1_700_000_000_000,
        );
        test_hooks::fail_next_artifact(stage, artifact);

        let error = create_session(
            &layout,
            &document.metadata,
            &document.runtime_lock,
            1_700_000_000_000,
            &document.compactions,
            document.events(),
        )
        .expect_err("injected initial publication failure must surface");

        assert_eq!(error.code, ErrorCode::AtomicWriteFailed);
        assert!(!layout.session_dir().exists());
        let project_dir = layout.root.join(&layout.project_partition);
        let residual_entries = std::fs::read_dir(&project_dir)
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(residual_entries, 0, "staging directory must be removed");
        test_hooks::clear_failures();
        drop(root);
    }
}

/// Runtime initialization and its exact locked sources survive initial API and runtime-only flushes.
/// 运行时初始化状态及其精确锁定来源在首次 API 与仅 runtime flush 后都保持不变。
#[test]
fn runtime_initialization_state_round_trips_across_all_writer_paths() {
    let (root, layout) = fresh_layout("runtime-initialization");
    let mut document = SessionDocument::new(
        layout.session_id.clone(),
        "project-1",
        "title",
        1_700_000_000_000,
    );
    document
        .submit_root_input(
            test_submission("workspace-store-runtime-root", "hello"),
            1_700_000_000_001,
        )
        .unwrap();
    document.metadata = publish_initial(&layout, &document);
    let fresh = load_session(&layout).unwrap();
    assert_eq!(
        fresh.document.runtime_lock.initialization_state,
        RuntimeInitializationState::FreshUninitialized
    );

    let locked_sources = vec![LockedSource {
        node_id: "agents-project".to_string(),
        order: 0,
        target: LockedPromptTarget::Developer,
        kind: LockedSourceKind::AgentsMd,
        source_label: Some("AGENTS.md".to_string()),
        source_id: None,
        content: "project rules".to_string(),
    }];
    let through_seq = document.events().last().expect("root input").seq;
    document
        .prepare_and_start_api(PrepareAndStartApiInput {
            purpose: ApiPurposeKind::MainGeneration,
            request: ApiRequestSnapshot::new("openai", "gpt-5"),
            projection: ProviderProjectionProof {
                segment_generation: document.metadata.segment_generation,
                document_revision: document.memory_revision(),
                retained_from_seq: Seq::try_new(1).unwrap(),
                through_seq,
                fingerprint_sha256: provider_visible_fingerprint(document.events(), through_seq)
                    .expect("valid runtime fixture fingerprint"),
            },
            locked_baseline: locked_baseline_from_sources(&locked_sources),
            continuation_action_id: None,
            request_budget_evidence: provider_request_budget_evidence(),
            now_ms: 1_700_000_000_002,
        })
        .unwrap();
    // dirtyFiles is the exact Core-derived candidate; bump_revision always adds metadata at the mother boundary.
    // dirtyFiles 是 Core 推导出的精确候选；bump_revision 会在母层统一补入 metadata。
    let initial_api_dirty = document.dirty_files().collect::<BTreeSet<_>>();
    assert!(initial_api_dirty.contains(&DirtyFile::ActiveTail));
    assert!(initial_api_dirty.contains(&DirtyFile::Metadata));
    assert!(initial_api_dirty.contains(&DirtyFile::Runtime));
    let initial_api_flush = flush_session(
        &layout,
        &FlushRequest {
            dirty: &initial_api_dirty,
            metadata: &document.metadata,
            runtime_lock: &document.runtime_lock,
            runtime_updated_at_ms: 1_700_000_000_002,
            compactions: &document.compactions,
            events: document.events(),
            tail_start: None,
        },
    )
    .unwrap();
    document.metadata = initial_api_flush
        .published_metadata
        .expect("initial API flush publishes metadata");

    let initialized = load_session(&layout).unwrap();
    assert_eq!(
        initialized.document.runtime_lock.initialization_state,
        RuntimeInitializationState::Initialized
    );
    assert_eq!(
        initialized.document.runtime_lock.locked_sources,
        locked_sources
    );
    assert_eq!(
        initialized.document.metadata.config.active_request.model,
        Some("gpt-5".to_string())
    );

    let runtime_only_dirty = BTreeSet::from([DirtyFile::Runtime]);
    let runtime_only_flush = flush_session(
        &layout,
        &FlushRequest {
            dirty: &runtime_only_dirty,
            metadata: &initialized.document.metadata,
            runtime_lock: &initialized.document.runtime_lock,
            runtime_updated_at_ms: 1_700_000_000_003,
            compactions: &initialized.document.compactions,
            events: initialized.document.events(),
            tail_start: None,
        },
    )
    .unwrap();
    assert!(
        runtime_only_flush
            .written_files
            .contains(&WrittenFile::Runtime)
    );
    let reloaded = load_session(&layout).unwrap();
    assert_eq!(
        reloaded.document.runtime_lock.initialization_state,
        RuntimeInitializationState::Initialized
    );
    assert_eq!(
        reloaded.document.runtime_lock.locked_sources,
        locked_sources
    );
    drop(root);
}

/// A wrong contract is reported as unsupported, never migrated or spared.
/// 错误契约被报告为不受支持，绝不迁移或宽容放行。
#[test]
fn wrong_contract_is_unsupported() {
    let (root, layout) = fresh_layout("contract");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    let mut metadata = read_metadata(&layout.metadata_path()).unwrap();
    metadata.contract = "vulcan.session.v0".to_string();
    write_json_document(&layout.metadata_path(), &metadata, "metadata").unwrap();
    let result = load_session(&layout);
    assert!(matches!(
        result,
        Err(SessionV1Error {
            code: ErrorCode::UnsupportedDevelopmentFormat,
            ..
        })
    ));
    drop(root);
}

/// A seq gap inside a segment is rejected with the exact position.
/// 分片内的 seq 空洞被带精确位置拒绝。
#[test]
fn seq_gap_in_segment_is_rejected() {
    let (root, layout) = fresh_layout("gap");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    let mut events = document.events().to_vec();
    events[2].seq = Seq::try_new(99).unwrap();
    let lines: Vec<String> = events
        .iter()
        .map(|event| serde_json::to_string(event).unwrap())
        .collect();
    let bytes = lines
        .join(
            "
",
        )
        .into_bytes();
    write_file_atomic(&layout.segment_path(1, 1), &bytes, "corrupt segment").unwrap();
    let result = load_session(&layout);
    let code = result.as_ref().err().map(|e| e.code);
    assert_eq!(
        code,
        Some(ErrorCode::SequenceDiscontinuity),
        "unexpected error: {code:?}"
    );
    drop(root);
}

/// Truncated JSONL is rejected with the exact segment and line.
/// 截断的 JSONL 被带精确分片与行号拒绝。
#[test]
fn truncated_segment_is_rejected_with_line() {
    let (root, layout) = fresh_layout("trunc");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    let bytes = serialize_segment(document.events()).unwrap();
    let cut = &bytes[..bytes.len() / 2];
    write_file_atomic(&layout.segment_path(1, 1), cut, "truncated segment").unwrap();
    let result = load_session(&layout);
    let code = result.as_ref().err().map(|e| e.code);
    assert_eq!(
        code,
        Some(ErrorCode::SegmentCorrupt),
        "unexpected error: {code:?}"
    );
    drop(root);
}

/// A missing referenced segment is a dedicated error, not a scan.
/// 缺失的被引用分片是专用错误，而不是扫描。
#[test]
fn missing_segment_is_dedicated_error() {
    let (root, layout) = fresh_layout("missing");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    let mut metadata = read_metadata(&layout.metadata_path()).unwrap();
    metadata.split_list[0].segment_number = 7;
    metadata.split_list[0].file_name = SessionV1Layout::segment_file_name(7);
    write_json_document(&layout.metadata_path(), &metadata, "metadata").unwrap();
    let result = load_session(&layout);
    assert!(matches!(
        result,
        Err(SessionV1Error {
            code: ErrorCode::SegmentMissing,
            ..
        })
    ));
    drop(root);
}

/// A flush with an empty dirty set writes nothing.
/// 空 dirty 集的 flush 不写入任何文件。
#[test]
fn empty_flush_writes_nothing() {
    let (root, layout) = fresh_layout("noop");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    let dirty = BTreeSet::new();
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &document.metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 1,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    let outcome = flush_session(&layout, &request).unwrap();
    assert!(outcome.written_files.is_empty());
    drop(root);
}

/// Rolling over seals the old tail with its full data and creates a new empty
/// tail without losing any event.
/// 滚段用完整数据封口旧尾段并创建空新尾段，不丢失任何事件。
#[test]
fn rollover_seals_old_tail_and_keeps_all_events() {
    let (root, layout) = fresh_layout("roll");
    let mut document = filled_document(&layout.session_id, 20);
    document.metadata = publish_initial(&layout, &document);
    let limits = FlushLimits {
        max_lines: 8,
        max_bytes: 8 * 1024 * 1024,
    };
    // Append one more event to force the rollover at the next flush.
    // 再追加一条事件以在下次 flush 触发滚段。
    document
        .append_display_note(
            None,
            DisplayNoticeKind::SystemError,
            NoticeSeverity::Info,
            "extra",
            None,
            None,
            None,
            None,
            2,
        )
        .unwrap();
    let dirty = BTreeSet::from([DirtyFile::ActiveTail]);
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &document.metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 2,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    let outcome = flush_session_with_limits(&layout, &request, &limits).unwrap();
    document.metadata = outcome.published_metadata.clone().unwrap();
    let sealed = outcome
        .written_files
        .iter()
        .filter(|file| matches!(file, WrittenFile::TailSegment { sealed: true, .. }))
        .count();
    assert_eq!(sealed, 1);
    assert!(outcome.published_metadata.is_some());
    let metadata = outcome.published_metadata.unwrap();
    assert_eq!(metadata.split_list.len(), 2);
    let total_events = document.events().len() as u64;
    assert_eq!(
        metadata.split_list[0].end_seq,
        Some(Seq::try_new(total_events).unwrap())
    );
    assert_eq!(metadata.split_list[1].end_seq, None);
    assert!(layout.segment_path(1, 2).exists());

    // The recorded split map must be persisted for the loader to see it.
    // 加载器必须看到已持久化的分片映射。
    write_json_document(&layout.metadata_path(), &metadata, "metadata").unwrap();
    let loaded = load_session(&layout).unwrap();
    assert_eq!(loaded.document.events().len(), document.events().len());
    drop(root);
}

/// Every rollover publication boundary keeps the previous split map authoritative until metadata commits.
/// 每个滚段发布边界在 metadata 提交前都保持旧分片映射权威。
#[test]
fn rollover_failures_keep_the_previous_banks_authoritative_and_retryable() {
    let failures = [
        (WriteStage::Rename, "sealed tail candidate"),
        (WriteStage::Rename, "new empty tail candidate"),
        (WriteStage::Rename, "metadata"),
    ];
    for (stage, artifact) in failures {
        let (root, layout) = fresh_layout(&format!("rollover-fault-{artifact}"));
        let mut document = filled_document(&layout.session_id, 20);
        document.metadata = publish_initial(&layout, &document);
        let published_before = read_metadata(&layout.metadata_path()).unwrap();
        let metadata_bytes_before = std::fs::read(layout.metadata_path()).unwrap();
        let published_tail_path = layout.segment_entry_path(
            published_before.segment_generation,
            &published_before.split_list[0].file_name,
        );
        let published_tail_bytes_before = std::fs::read(&published_tail_path).unwrap();
        let published_event_count = document.events().len();
        document
            .append_display_note(
                None,
                DisplayNoticeKind::SystemError,
                NoticeSeverity::Info,
                "rollover-candidate-only",
                None,
                None,
                None,
                None,
                21,
            )
            .unwrap();
        let dirty = BTreeSet::from([DirtyFile::ActiveTail]);
        let request = FlushRequest {
            dirty: &dirty,
            metadata: &document.metadata,
            runtime_lock: &document.runtime_lock,
            runtime_updated_at_ms: 21,
            compactions: &document.compactions,
            events: document.events(),
            tail_start: None,
        };
        let limits = FlushLimits {
            max_lines: 8,
            max_bytes: 8 * 1024 * 1024,
        };

        test_hooks::fail_next_artifact(stage, artifact);
        assert!(
            flush_session_with_limits(&layout, &request, &limits).is_err(),
            "rollover artifact {artifact} must fail"
        );
        test_hooks::clear_failures();

        // Unpublished opposite-bank and new-tail files may exist, but the exact old metadata and bank remain authoritative.
        // 未发布的对侧 bank 与新尾段文件可以存在，但精确旧 metadata 与旧 bank 仍保持权威。
        assert_eq!(
            std::fs::read(layout.metadata_path()).unwrap(),
            metadata_bytes_before,
            "rollover artifact {artifact} must not publish metadata"
        );
        assert_eq!(
            std::fs::read(&published_tail_path).unwrap(),
            published_tail_bytes_before,
            "rollover artifact {artifact} must not alter the authoritative bank"
        );
        assert_eq!(
            load_session(&layout).unwrap().document.events().len(),
            published_event_count,
            "rollover artifact {artifact} must leave the previous session loadable"
        );

        let retry = flush_session_with_limits(&layout, &request, &limits).unwrap();
        let published_after = retry
            .published_metadata
            .expect("rollover retry must publish metadata");
        assert_eq!(published_after.split_list.len(), 2);
        write_json_document(&layout.metadata_path(), &published_after, "metadata").unwrap();
        assert_eq!(
            load_session(&layout).unwrap().document.events().len(),
            document.events().len()
        );
        drop(root);
    }
}

/// A single oversized event is fully preserved and never truncated.
/// 单个超大事件被完整保留，绝不截断。
#[test]
fn oversized_event_is_fully_preserved() {
    let (root, layout) = fresh_layout("oversize");
    let document = filled_document(&layout.session_id, 0);
    let mut huge = document.events().to_vec();
    huge.push(ConversationEvent {
        seq: Seq::try_new(huge.len() as u64 + 1).unwrap(),
        turn_id: None,
        created_at_ms: 1,
        updated_at_ms: 1,
        event_revision: vulcan_session_core::v1::event::EVENT_REVISION_INITIAL,
        visibility: EventVisibility::UiOnly,
        event: EventBody::DisplayNote(DisplayNoteEvent {
            notice_kind: DisplayNoticeKind::SystemError,
            severity: NoticeSeverity::Info,
            reason_code: "big".to_string(),
            redacted_detail: Some("x".repeat(9_000_000)),
            api_call_id: None,
            retry_count: None,
            retry_limit: None,
        }),
    });
    create_session(
        &layout,
        &document.metadata,
        &document.runtime_lock,
        1,
        &document.compactions,
        &huge,
    )
    .unwrap();
    let bytes = std::fs::read(layout.segment_path(1, 1)).unwrap();
    assert!(
        bytes.len() > 8 * 1024 * 1024,
        "a single event over 8 MiB must be written fully, got {} bytes",
        bytes.len()
    );
    let loaded = load_session(&layout).unwrap();
    assert_eq!(loaded.document.events().len(), huge.len());
    drop(root);
}

/// Every atomic-write stage failure leaves the old published state intact.
/// 每个原子写阶段失败都保留旧已发布状态完好。
#[test]
fn fault_injection_keeps_old_state_loadable() {
    let stages = [
        WriteStage::StagingCreate,
        WriteStage::WriteAll,
        WriteStage::SyncAll,
        WriteStage::Rename,
    ];
    for stage in stages {
        let (root, layout) = fresh_layout(&format!("fault-{:?}", stage));
        let mut document = filled_document(&layout.session_id, 0);
        document.metadata = publish_initial(&layout, &document);
        let original_tail = std::fs::read(layout.segment_path(1, 1)).unwrap();
        document
            .append_display_note(
                None,
                DisplayNoticeKind::SystemError,
                NoticeSeverity::Info,
                "new-note",
                None,
                None,
                None,
                None,
                3,
            )
            .unwrap();
        let dirty = BTreeSet::from([DirtyFile::ActiveTail]);
        test_hooks::fail_next_write(stage);
        let request = FlushRequest {
            dirty: &dirty,
            metadata: &document.metadata,
            runtime_lock: &document.runtime_lock,
            runtime_updated_at_ms: 3,
            compactions: &document.compactions,
            events: document.events(),
            tail_start: None,
        };
        let result = flush_session(&layout, &request);
        assert!(result.is_err(), "stage {stage:?} must fail the flush");
        // The old segment bytes are untouched and the session still loads with
        // all previously published facts.
        // 旧分片字节未被触碰，会话仍能加载全部已发布事实。
        let after = std::fs::read(layout.segment_path(1, 1)).unwrap();
        assert_eq!(
            after, original_tail,
            "stage {stage:?} must not alter the old segment"
        );
        let loaded = load_session(&layout).unwrap();
        assert_eq!(
            loaded.document.events().len(),
            filled_document(&layout.session_id, 0).events().len()
        );
        test_hooks::clear_failures();
        drop(root);
    }
}

/// A tail candidate cannot become authoritative when metadata publication fails.
/// metadata 发布失败时尾段候选不能成为权威事实。
#[test]
fn active_tail_metadata_failure_keeps_old_bank_authoritative_and_retryable() {
    let (root, layout) = fresh_layout("tail-metadata-fault");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    let published_before = read_metadata(&layout.metadata_path()).unwrap();
    let published_tail_before = layout.segment_entry_path(
        published_before.segment_generation,
        &published_before.split_list[0].file_name,
    );
    let bytes_before = std::fs::read(&published_tail_before).unwrap();
    let event_count_before = document.events().len();
    document
        .append_display_note(
            None,
            DisplayNoticeKind::SystemError,
            NoticeSeverity::Info,
            "candidate-only-note",
            None,
            None,
            None,
            None,
            3,
        )
        .unwrap();
    let dirty = BTreeSet::from([DirtyFile::ActiveTail]);
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &document.metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 3,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    test_hooks::fail_next_artifact(WriteStage::Rename, "metadata");
    assert!(flush_session(&layout, &request).is_err());
    test_hooks::clear_failures();

    let still_published = read_metadata(&layout.metadata_path()).unwrap();
    assert_eq!(still_published, published_before);
    assert_eq!(std::fs::read(&published_tail_before).unwrap(), bytes_before);
    assert_eq!(
        load_session(&layout).unwrap().document.events().len(),
        event_count_before
    );

    let outcome = flush_session(&layout, &request).unwrap();
    let published_after = outcome
        .published_metadata
        .expect("tail retry must publish metadata");
    assert_ne!(
        published_after.split_list[0].file_name,
        published_before.split_list[0].file_name
    );
    assert_eq!(
        load_session(&layout).unwrap().document.events().len(),
        document.events().len()
    );
    drop(root);
}

/// Compactions-followed-by-runtime two-phase publish keeps the old generation
/// valid when the runtime write fails.
/// runtime 写失败时 compactions 先行的两阶段发布保持旧代次有效。
#[test]
fn compaction_publish_keeps_old_generation_on_runtime_failure() {
    let (root, layout) = fresh_layout("compaction-fault");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    document
        .submit_root_input(
            test_submission("workspace-store-compaction-root", "compact turn"),
            3,
        )
        .unwrap();
    let api = prepare_api(
        &mut document,
        ApiPurposeKind::Compaction,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        0,
    );
    document
        .append_assistant_text(api, "compacted", AssistantSemanticStatus::Completed, 4)
        .unwrap();
    complete_api(&mut document, api, 4, None);
    let (occurred_at_seq, summary) = completed_compaction_semantic(&document, api);
    document
        .record_compaction(
            CompactionCommitInput::new(
                api,
                occurred_at_seq,
                Seq::try_new(3).unwrap(),
                summary,
                LockedBaselineProjection::empty(),
            ),
            4,
        )
        .unwrap();
    let dirty = BTreeSet::from([DirtyFile::Compactions, DirtyFile::Runtime]);
    test_hooks::fail_next_write(WriteStage::Rename);
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &document.metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 4,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    let result = flush_session(&layout, &request);
    // The runtime rename failure may occur on either file; the loader must
    // still produce a valid old-generation session either way.
    // runtime rename 失败可能发生在任一文件；加载器无论如何必须产出有效的
    // 旧代次会话。
    let _ = result;
    test_hooks::clear_failures();
    let loaded = load_session(&layout).unwrap();
    let events = loaded.document.events().len();
    assert_eq!(
        events,
        filled_document(&layout.session_id, 0).events().len()
    );
    drop(root);
}

/// Every context publication boundary keeps the previous generation authoritative and permits an exact retry.
/// 每个 context 发布边界都保持上一代权威，并允许精确重试。
#[test]
fn context_publication_failures_keep_the_previous_generation_authoritative_and_retryable() {
    let failures = [
        (WriteStage::Rename, "flush context candidate compactions"),
        (WriteStage::Rename, "flush context candidate runtime"),
        (WriteStage::Rename, "metadata"),
    ];
    for (stage, artifact) in failures {
        let (root, layout) = fresh_layout(&format!("context-publication-fault-{artifact}"));
        let mut document = filled_document(&layout.session_id, 0);
        document.metadata = publish_initial(&layout, &document);
        let published_event_count = document.events().len();
        let metadata_bytes_before = std::fs::read(layout.metadata_path()).unwrap();
        document
            .submit_root_input(
                test_submission("workspace-store-context-fault", "compact turn"),
                3,
            )
            .unwrap();
        let api = prepare_api(
            &mut document,
            ApiPurposeKind::Compaction,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            3,
        );
        document
            .append_assistant_text(
                api,
                "context summary",
                AssistantSemanticStatus::Completed,
                4,
            )
            .unwrap();
        complete_api(&mut document, api, 4, None);
        let (occurred_at_seq, summary) = completed_compaction_semantic(&document, api);
        document
            .record_compaction(
                CompactionCommitInput::new(
                    api,
                    occurred_at_seq,
                    Seq::try_new(3).unwrap(),
                    summary,
                    LockedBaselineProjection::empty(),
                ),
                4,
            )
            .unwrap();
        let candidate = document.prepare_flush_candidates().unwrap().unwrap();

        test_hooks::fail_next_artifact(stage, artifact);
        assert!(
            super::writer::flush_candidate(&layout, &candidate).is_err(),
            "context artifact {artifact} must fail"
        );
        test_hooks::clear_failures();

        // Candidate files can exist, but the old metadata pointer remains the only authority.
        // 候选文件可以存在，但旧 metadata 指针仍是唯一权威。
        assert_eq!(
            std::fs::read(layout.metadata_path()).unwrap(),
            metadata_bytes_before,
            "context artifact {artifact} must not publish metadata"
        );
        let loaded_before_retry = load_session(&layout).unwrap();
        assert_eq!(
            loaded_before_retry.document.events().len(),
            published_event_count
        );
        assert!(loaded_before_retry.document.compactions.is_empty());

        super::writer::flush_candidate(&layout, &candidate).unwrap();
        let loaded_after_retry = load_session(&layout).unwrap();
        assert_eq!(
            loaded_after_retry.document.events().len(),
            document.events().len()
        );
        assert_eq!(loaded_after_retry.document.compactions.len(), 1);
        assert_eq!(
            loaded_after_retry.metadata.active_context_generation,
            document.metadata.active_context_generation + 1
        );
        drop(root);
    }
}

/// metadata stale `updated_at` never truncates the tail on load.
/// 滞后的 metadata `updated_at` 加载时绝不截断尾段。
#[test]
fn stale_metadata_updated_at_does_not_truncate_tail() {
    let (root, layout) = fresh_layout("stale");
    let mut document = filled_document(&layout.session_id, 5);
    document.metadata = publish_initial(&layout, &document);
    let mut metadata = read_metadata(&layout.metadata_path()).unwrap();
    metadata.updated_at_ms = 1;
    write_json_document(&layout.metadata_path(), &metadata, "metadata").unwrap();
    let loaded = load_session(&layout).unwrap();
    assert_eq!(loaded.document.events().len(), document.events().len());
    drop(root);
}

/// The active tail rewrite never touches sealed segments.
/// 活动尾段重写绝不触碰封口分片。
#[test]
fn tail_rewrite_keeps_sealed_segments_untouched() {
    let (root, layout) = fresh_layout("sealed");
    let mut document = filled_document(&layout.session_id, 12);
    document.metadata = publish_initial(&layout, &document);
    let limits = FlushLimits {
        max_lines: 8,
        max_bytes: 8 * 1024 * 1024,
    };
    document
        .append_display_note(
            None,
            DisplayNoticeKind::SystemError,
            NoticeSeverity::Info,
            "push",
            None,
            None,
            None,
            None,
            5,
        )
        .unwrap();
    let dirty = BTreeSet::from([DirtyFile::ActiveTail]);
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &document.metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 5,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    let outcome = flush_session_with_limits(&layout, &request, &limits).unwrap();
    document.metadata = outcome.published_metadata.clone().unwrap();
    write_json_document(
        &layout.metadata_path(),
        &outcome.published_metadata.unwrap(),
        "metadata",
    )
    .unwrap();
    // Rewrite the active tail with brand-new events; the sealed file byte
    // content must not change.
    // 用全新事件重写活动尾段；封口文件字节必须不变。
    let meta = read_metadata(&layout.metadata_path()).unwrap();
    let sealed_path = layout.segment_entry_path(1, &meta.split_list[0].file_name);
    let sealed_before_second_flush = std::fs::read(&sealed_path).unwrap();
    document
        .append_display_note(
            None,
            DisplayNoticeKind::SystemError,
            NoticeSeverity::Info,
            "tail2",
            None,
            None,
            None,
            None,
            6,
        )
        .unwrap();
    let dirty2 = BTreeSet::from([DirtyFile::ActiveTail, DirtyFile::Metadata]);
    let request2 = FlushRequest {
        dirty: &dirty2,
        metadata: &document.metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 6,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    let outcome2 = flush_session_with_limits(&layout, &request2, &limits).unwrap();
    let published2 = outcome2
        .published_metadata
        .expect("tail rewrite must publish metadata");
    let sealed_after_second_flush = std::fs::read(&sealed_path).unwrap();
    assert_eq!(
        sealed_before_second_flush, sealed_after_second_flush,
        "rewriting the active tail must not touch the sealed segment"
    );
    let tail_after =
        std::fs::read(layout.segment_entry_path(1, &published2.split_list[1].file_name)).unwrap();
    assert!(
        !tail_after.is_empty(),
        "the active tail must carry the newly flushed event"
    );
    drop(root);
}

/// History edits byte-copy sealed prefix segments and switch generation last.
/// 历史编辑按字节复制封口前缀分片并最后切换 generation。
#[test]
fn edit_generation_byte_copies_prefix_and_switches() {
    let (root, layout) = fresh_layout("edit");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    // Force a rollover so a sealed prefix segment exists.
    // 强制一次滚段使封口前缀分片存在。
    let limits = FlushLimits {
        max_lines: 3,
        max_bytes: 8 * 1024 * 1024,
    };
    for index in 0..4u64 {
        document
            .append_display_note(
                None,
                DisplayNoticeKind::SystemError,
                NoticeSeverity::Info,
                format!("push{index}"),
                None,
                None,
                None,
                None,
                10 + index as i64,
            )
            .unwrap();
    }
    let dirty = BTreeSet::from([DirtyFile::ActiveTail, DirtyFile::Metadata]);
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &document.metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 11,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    let outcome = flush_session_with_limits(&layout, &request, &limits).unwrap();
    let rolled = outcome.published_metadata.unwrap();
    write_json_document(&layout.metadata_path(), &rolled, "metadata").unwrap();
    let metadata = read_metadata(&layout.metadata_path()).unwrap();
    assert!(metadata.split_list.len() >= 2);

    // Edit the first user input (seq 1): prefix sealed files before seq 1 do
    // not exist; the rewrite lands on the first segment.
    // 编辑第一个用户输入(seq 1)：seq 1 之前没有封口文件；重写落在第一个分片。
    let output = build_durable_edit_output(&layout, Seq::try_new(1).unwrap(), "edited hello", 12);
    let new_metadata = publish_edit_generation(
        &layout,
        PublishEditGenerationRequest {
            current_metadata: &metadata,
            output: &output,
        },
    )
    .unwrap();
    assert_eq!(
        new_metadata.segment_generation,
        metadata.segment_generation + 1
    );
    assert_eq!(
        new_metadata.active_context_generation,
        metadata.active_context_generation + 1
    );
    assert!(
        layout
            .runtime_path(new_metadata.active_context_generation)
            .exists()
    );
    assert!(
        layout
            .compactions_path(new_metadata.active_context_generation)
            .exists()
    );
    let loaded = load_session(&layout).unwrap();
    let first_input = loaded
        .document
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::UserInput(input) => Some(input.text.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(first_input, "edited hello");
    // Everything after the target was deleted.
    // 目标之后的内容全部被删除。
    assert_eq!(loaded.document.events().len(), 1);
    drop(root);
}

/// Forks preserve the copied fact identities and load cleanly.
/// 分叉保留复制事实身份并可干净加载。
#[test]
fn fork_creates_loadable_identity_preserving_session() {
    let (root, layout) = fresh_layout("fork-src");
    let mut document = filled_document(&layout.session_id, 3);
    submit_test_root(
        &mut document,
        "workspace-store-fork-boundary",
        "fork boundary",
        1_700_000_000_100,
    );
    let source_turn_end_seq = document
        .close_turn(TurnOutcome::Completed, 1, 1_700_000_000_100)
        .expect("fork boundary Turn must close");
    document.metadata = publish_initial(&layout, &document);
    let source_events = document.events().len();
    let fork_root = root.path().join("fork");
    let fork_layout = SessionV1Layout::new(fork_root, "project-1", "2026-08-26", "sess-fork-1");
    let output = build_durable_fork_output(
        &layout,
        &fork_layout,
        source_turn_end_seq,
        1_700_000_000_101,
    );
    let published = create_session(
        &fork_layout,
        &output.metadata,
        &output.runtime_lock,
        output.metadata.updated_at_ms,
        &output.compactions,
        &output.events,
    )
    .unwrap();
    let loaded = load_session(&fork_layout).unwrap();
    assert_eq!(loaded.document.events().len(), source_events);
    assert_eq!(loaded.document.events(), output.events);
    assert!(loaded.document.active_turn().is_none());
    assert_eq!(loaded.document.metadata.open_turn_start_seq, None);
    assert_eq!(published.id_frontier, output.metadata.id_frontier);
    drop(root);
}

/// Loading a nonexistent session is a dedicated missing error.
/// 加载不存在的会话是专用缺失错误。
#[test]
fn load_missing_session_is_dedicated_error() {
    let (root, layout) = fresh_layout("absent");
    let result = load_session(&layout);
    assert!(matches!(
        result,
        Err(SessionV1Error {
            code: ErrorCode::SessionMissing,
            ..
        })
    ));
    // The error carries the session id and path.
    // 错误携带会话 id 与路径。
    let error = result.err().unwrap();
    assert_eq!(
        error.session_id.as_deref(),
        Some(layout.session_id.as_str())
    );
    assert!(error.path.is_some());
    drop(root);
}

/// Segment round-trip preserves every event and its seq.
/// 分片往返保留每条事件及其 seq。
#[test]
fn segment_round_trip_preserves_events() {
    let document = filled_document("sess-1", 1);
    let bytes = serialize_segment(document.events()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s0000001.jsonl");
    std::fs::write(&path, &bytes).unwrap();
    let parsed = parse_segment(&path, 1, Some(Seq::try_new(1).unwrap())).unwrap();
    assert_eq!(parsed.len(), document.events().len());
    assert_eq!(bytes, serialize_segment(&parsed).unwrap());
    drop(dir);
}
/// Performance smoke: serialize/write/load one large active tail in one shot.
/// 性能冒烟：一次性序列化/写入/加载大型活动尾段。
///
/// This is evidence, not a hard CI gate: absolute times depend on the host,
/// antivirus, and build profile. It reports the numbers to stderr and asserts
/// only functional correctness (round-trip count and byte size).
/// 这是证据而非硬性 CI 门禁：绝对时间取决于宿主、杀毒软件与构建 profile。
/// 它把数字输出到 stderr，只断言功能正确性(往返条数与字节大小)。
#[test]
fn performance_smoke_large_tail_round_trip() {
    let (root, layout) = fresh_layout("perf");
    let document = filled_document(&layout.session_id, 0);
    // Synthesize a large tail from user inputs without mutation overhead.
    // 直接合成大型尾段，避免 mutation 开销。
    let base = document.events().to_vec();
    let mut big = base;
    for _ in 0..40_000u64 {
        let seq = Seq::try_new(big.len() as u64 + 1).unwrap();
        big.push(ConversationEvent {
            seq,
            turn_id: None,
            created_at_ms: 1,
            updated_at_ms: 1,
            event_revision: vulcan_session_core::v1::event::EVENT_REVISION_INITIAL,
            visibility: EventVisibility::UiOnly,
            event: EventBody::DisplayNote(DisplayNoteEvent {
                notice_kind: DisplayNoticeKind::SystemError,
                severity: NoticeSeverity::Info,
                reason_code: "perf".to_string(),
                redacted_detail: Some("x".repeat(120)),
                api_call_id: None,
                retry_count: None,
                retry_limit: None,
            }),
        });
    }
    let started = std::time::Instant::now();
    let bytes = serialize_segment(&big).unwrap();
    let serialize_ms = started.elapsed().as_millis();
    let started = std::time::Instant::now();
    create_session(
        &layout,
        &document.metadata,
        &document.runtime_lock,
        1,
        &document.compactions,
        &big,
    )
    .unwrap();
    let write_ms = started.elapsed().as_millis();
    let started = std::time::Instant::now();
    let loaded = load_session(&layout).unwrap();
    let load_ms = started.elapsed().as_millis();
    eprintln!(
        "[perf] events={} bytes={}MiB serialize={serialize_ms}ms write={write_ms}ms load={load_ms}ms",
        big.len(),
        bytes.len() as f64 / (1024.0 * 1024.0)
    );
    assert_eq!(loaded.document.events().len(), big.len());
    drop(root);
}

/// Measures one nearest-rank percentile from a non-empty microsecond sample.
/// 从一组非空微秒样本中计算一个最近秩百分位。
///
/// Parameters: samples contains elapsed microseconds and percentile is within one through one hundred.
/// 参数：samples 包含耗时微秒，percentile 位于一到一百之间。
/// Returns: the selected elapsed microseconds after ordering the sample.
/// 返回值：样本排序后选中的耗时微秒。
fn nearest_rank_micros(samples: &[u128], percentile: usize) -> u128 {
    assert!(!samples.is_empty(), "performance sample must not be empty");
    assert!((1..=100).contains(&percentile), "percentile must be valid");
    let mut ordered = samples.to_vec();
    ordered.sort_unstable();
    let rank = (ordered.len() * percentile).div_ceil(100);
    ordered[rank.saturating_sub(1)]
}

/// Records p50, p95, and throughput for complete one, four, and eight MiB active-tail writes.
/// 记录一、四、八 MiB 活动尾段完整写入的 p50、p95 与吞吐量。
///
/// This test is explicit because it performs sixty physical flushes with file synchronization.
/// 该测试需要执行六十次带文件同步的物理 flush，因此只在显式性能验收时运行。
#[test]
#[ignore = "explicit storage performance evidence"]
fn performance_active_tail_sizes_report_p50_p95() {
    const SAMPLE_COUNT: usize = 20;
    const MIB: usize = 1024 * 1024;

    for payload_mib in [1usize, 4, 8] {
        let mut elapsed_micros = Vec::with_capacity(SAMPLE_COUNT);
        let mut persisted_tail_bytes = 0usize;
        for sample_index in 0..SAMPLE_COUNT {
            let (root, layout) =
                fresh_layout(&format!("perf-tail-{payload_mib}mib-sample-{sample_index}"));
            let mut document = filled_document(&layout.session_id, 0);
            document.metadata = publish_initial(&layout, &document);
            document
                .append_display_note(
                    None,
                    DisplayNoticeKind::SystemError,
                    NoticeSeverity::Info,
                    format!("perf-tail-{payload_mib}mib"),
                    Some("x".repeat(payload_mib * MIB)),
                    None,
                    None,
                    None,
                    2,
                )
                .unwrap();
            let candidate = document
                .prepare_flush_candidates()
                .unwrap()
                .expect("performance mutation must produce one flush candidate");
            persisted_tail_bytes = serialize_segment(&candidate.tail_events).unwrap().len();
            let started = std::time::Instant::now();
            flush_with_limits(&layout, &mut document, &FlushLimits::default(), 2);
            elapsed_micros.push(started.elapsed().as_micros());
            let loaded = load_session(&layout).unwrap();
            assert_eq!(loaded.document.events(), document.events());
            drop(root);
        }
        let p50_micros = nearest_rank_micros(&elapsed_micros, 50);
        let p95_micros = nearest_rank_micros(&elapsed_micros, 95);
        let p50_seconds = p50_micros as f64 / 1_000_000.0;
        let throughput_mib_per_second = persisted_tail_bytes as f64 / (MIB as f64 * p50_seconds);
        eprintln!(
            "[perf-tail] payload_mib={payload_mib} samples={SAMPLE_COUNT} tail_bytes={persisted_tail_bytes} p50_ms={:.3} p95_ms={:.3} throughput_mib_per_second={throughput_mib_per_second:.3}",
            p50_micros as f64 / 1_000.0,
            p95_micros as f64 / 1_000.0,
        );
    }
}

/// The active tail file remains authoritative when metadata cache fields lag
/// behind a non-dirty tail flush (plan 5.2.15).
/// 当 metadata 缓存字段滞后于非 dirty 的尾段 flush 时，活动尾段文件仍然权威
/// (计划 5.2.15)。
#[test]
fn stale_active_tail_cache_fields_do_not_break_load() {
    let (root, layout) = fresh_layout("stale-tail");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    // Grow the tail without marking metadata dirty.
    // 增长尾段但不标记 metadata dirty。
    document
        .append_display_note(
            None,
            DisplayNoticeKind::SystemError,
            NoticeSeverity::Info,
            "grown",
            None,
            None,
            None,
            None,
            9,
        )
        .unwrap();
    let dirty = BTreeSet::from([DirtyFile::ActiveTail]);
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &document.metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 9,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    flush_session(&layout, &request).unwrap();
    // The disk metadata still carries the old byte_len/event_count cache, but
    // the loader must accept the tail file as authoritative.
    // 磁盘 metadata 仍携带旧 byte_len/event_count 缓存，但加载器必须把尾段
    // 文件视为权威。
    let loaded = load_session(&layout).unwrap();
    assert_eq!(
        loaded.document.events().len(),
        document.events().len(),
        "active tail file content is authoritative over cache fields"
    );
    drop(root);
}

/// An immutable context candidate rejects compaction nodes beyond its runtime
/// pointer instead of publishing an internally partial generation.
/// 不可变 context 候选拒绝 runtime 指针之后的压缩节点，不发布内部半成品代次。
#[test]
fn context_candidate_rejects_unpublished_compaction_tail() {
    let (root, layout) = fresh_layout("unpub-node");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    // First compaction is published together with the runtime pointer.
    // 第一次压缩与 runtime 指针一起发布。
    document
        .submit_root_input(
            test_submission("workspace-store-compaction-root", "compact turn"),
            3,
        )
        .unwrap();
    let api = prepare_api(
        &mut document,
        ApiPurposeKind::Compaction,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        0,
    );
    document
        .append_assistant_text(api, "c1", AssistantSemanticStatus::Completed, 4)
        .unwrap();
    complete_api(&mut document, api, 4, None);
    let (occurred_at_seq, summary) = completed_compaction_semantic(&document, api);
    document
        .record_compaction(
            CompactionCommitInput::new(
                api,
                occurred_at_seq,
                Seq::try_new(6).unwrap(),
                summary,
                LockedBaselineProjection::empty(),
            ),
            4,
        )
        .unwrap();
    // First compaction publishes and confirms durably (contract flow).
    // 第一次压缩按契约流程发布并确认持久化。
    flush_candidate(&mut document, &layout, 4);
    document.close_turn(TurnOutcome::Completed, 1, 5).unwrap();
    let node1 = document.compactions[0].compaction_id;

    // Second compaction: compactions.json gains the node, but the runtime
    // pointer write fails (crash window) so the pointer stays at node 1.
    // 第二次压缩：compactions.json 增加节点，但 runtime 指针写入失败(崩溃
    // 窗口)，指针停留在节点 1。
    document
        .submit_root_input(
            test_submission("workspace-store-compaction-root-2", "compact turn 2"),
            5,
        )
        .unwrap();
    let api2 = prepare_api(
        &mut document,
        ApiPurposeKind::Compaction,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        0,
    );
    document
        .append_assistant_text(api2, "c2", AssistantSemanticStatus::Completed, 5)
        .unwrap();
    complete_api(&mut document, api2, 5, None);
    let (occurred_at_seq, summary) = completed_compaction_semantic(&document, api2);
    document
        .record_compaction(
            CompactionCommitInput::new(
                api2,
                occurred_at_seq,
                Seq::try_new(9).unwrap(),
                summary,
                LockedBaselineProjection::empty(),
            ),
            5,
        )
        .unwrap();
    let mut published_runtime = document.runtime_lock.clone();
    published_runtime.current_compaction_id = Some(node1);
    let compactions_only = BTreeSet::from([
        DirtyFile::Compactions,
        DirtyFile::Runtime,
        DirtyFile::ActiveTail,
        DirtyFile::Metadata,
    ]);
    let request = FlushRequest {
        dirty: &compactions_only,
        metadata: &document.metadata,
        runtime_lock: &published_runtime,
        runtime_updated_at_ms: 4,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    assert!(matches!(
        flush_session(&layout, &request),
        Err(SessionV1Error {
            code: ErrorCode::RuntimeCorrupt,
            ..
        })
    ));

    // Metadata still selects the previous complete context.
    // metadata 仍选择上一份完整 context。
    let loaded = load_session(&layout).unwrap();
    assert_eq!(loaded.document.compactions.len(), 1);
    assert_eq!(
        loaded.document.compactions[0].compaction_id, node1,
        "the runtime pointer must select only the published node"
    );
    let on_disk = read_compactions(&active_compactions_path(&layout)).unwrap();
    assert_eq!(
        on_disk.nodes.len(),
        1,
        "the rejected node was never committed"
    );
    drop(root);
}

/// A future schema_version is rejected without attempting a downgrade.
/// 拒绝未来模式版本，绝不尝试降级。
#[test]
fn wrong_schema_version_is_rejected() {
    let (root, layout) = fresh_layout("schema");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    let mut metadata = read_metadata(&layout.metadata_path()).unwrap();
    metadata.schema_version = vulcan_session_core::v1::config::METADATA_SCHEMA_VERSION + 1;
    write_json_document(&layout.metadata_path(), &metadata, "metadata").unwrap();
    let result = load_session(&layout);
    assert!(matches!(
        result,
        Err(SessionV1Error {
            code: ErrorCode::UnsupportedDevelopmentFormat,
            ..
        })
    ));
    drop(root);
}

/// A metadata id_frontier ahead of stream facts is surfaced as corruption
/// instead of silently rewinding the allocator (plan 5.2.16).
/// metadata id_frontier 超前于流事实时被暴露为损坏，而不是静默回卷分配器
/// (计划 5.2.16)。
#[test]
fn id_frontier_ahead_of_facts_is_rejected() {
    let (root, layout) = fresh_layout("frontier-ahead");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    let persisted = read_metadata(&layout.metadata_path()).unwrap();
    let mut metadata = persisted.clone();
    metadata.id_frontier.next_seq += 5;
    write_json_document(&layout.metadata_path(), &metadata, "metadata").unwrap();
    let result = load_session(&layout);
    assert!(matches!(
        result,
        Err(SessionV1Error {
            code: ErrorCode::MetadataCorrupt,
            ..
        })
    ));
    let mut metadata = persisted.clone();
    metadata.id_frontier.next_local_id += 5;
    write_json_document(&layout.metadata_path(), &metadata, "metadata").unwrap();
    assert!(matches!(
        load_session(&layout),
        Err(SessionV1Error {
            code: ErrorCode::MetadataCorrupt,
            ..
        })
    ));
    // A cache behind the facts stays fine (facts win).
    // 缓存滞后于事实仍正常(事实优先)。
    let mut metadata = persisted;
    metadata.id_frontier.next_seq = 1;
    metadata.id_frontier.next_local_id = 1;
    write_json_document(&layout.metadata_path(), &metadata, "metadata").unwrap();
    assert!(load_session(&layout).is_ok());
    drop(root);
}

/// An edit retries after a crash left residual files in the fixed candidate
/// generation path (plan 5.1.7/5.1.8).
/// 崩溃在固定候选 generation 路径残留文件后，编辑可以重试(计划 5.1.7/5.1.8)。
#[test]
fn edit_is_retryable_after_stale_candidate() {
    let (root, layout) = fresh_layout("edit-retry");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    // Simulate a crash that left residual files in candidate g/c0000002.
    // 模拟崩溃在候选 g/c0000002 残留文件。
    std::fs::create_dir_all(layout.candidate_generation_dir(1)).unwrap();
    std::fs::write(layout.segment_path(2, 1), b"residual").unwrap();
    std::fs::create_dir_all(layout.candidate_context_dir(1)).unwrap();
    std::fs::write(layout.runtime_path(2), b"residual").unwrap();

    let output = build_durable_edit_output(&layout, Seq::try_new(1).unwrap(), "edited", 12);
    let new_metadata = publish_edit_generation(
        &layout,
        PublishEditGenerationRequest {
            current_metadata: &document.metadata,
            output: &output,
        },
    )
    .unwrap();
    assert_eq!(new_metadata.segment_generation, 2);
    assert_eq!(new_metadata.active_context_generation, 2);
    // The residual file was replaced by the real rewrite.
    // 残留文件已被真实重写替换。
    let loaded = load_session(&layout).unwrap();
    assert_eq!(loaded.document.events().len(), 1);
    drop(root);
}

/// A stale edit request is rejected before candidate cleanup can delete the
/// currently published next generation.
/// 过期编辑请求在候选清理前被拒绝，不能删除当前已发布的下一代。
#[test]
fn stale_edit_request_cannot_delete_published_generation() {
    let (root, layout) = fresh_layout("stale-edit");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    let stale_metadata = document.metadata;
    let first_output =
        build_durable_edit_output(&layout, Seq::try_new(1).unwrap(), "first edit", 12);
    let published = publish_edit_generation(
        &layout,
        PublishEditGenerationRequest {
            current_metadata: &stale_metadata,
            output: &first_output,
        },
    )
    .unwrap();
    assert_eq!(published.segment_generation, 2);
    let stale_output = first_output;
    let stale_result = publish_edit_generation(
        &layout,
        PublishEditGenerationRequest {
            current_metadata: &stale_metadata,
            output: &stale_output,
        },
    );
    assert!(matches!(
        stale_result,
        Err(SessionV1Error {
            code: ErrorCode::ReferenceViolation,
            ..
        })
    ));
    assert!(layout.segment_path(2, 1).exists());
    assert!(layout.runtime_path(2).exists());
    assert!(layout.compactions_path(2).exists());
    let loaded = load_session(&layout).unwrap();
    assert!(matches!(
        &loaded.document.events()[0].event,
        EventBody::UserInput(input) if input.text == "first edit"
    ));
    drop(root);
}

/// A stale normal flush is rejected before it can clean an authoritative
/// context generation as if it were an unpublished candidate.
/// 过期普通 flush 在把权威 context 误当未发布候选清理前被拒绝。
#[test]
fn stale_flush_cannot_delete_published_context_generation() {
    let (root, layout) = fresh_layout("stale-flush");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    let dirty = BTreeSet::from([DirtyFile::Runtime]);
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &document.metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 2,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    let first = flush_session(&layout, &request).unwrap();
    assert_eq!(
        first
            .published_metadata
            .as_ref()
            .unwrap()
            .active_context_generation,
        2
    );
    assert!(matches!(
        flush_session(&layout, &request),
        Err(SessionV1Error {
            code: ErrorCode::ReferenceViolation,
            ..
        })
    ));
    assert!(layout.runtime_path(2).exists());
    assert!(layout.compactions_path(2).exists());
    assert_eq!(
        load_session(&layout)
            .unwrap()
            .metadata
            .active_context_generation,
        2
    );
    drop(root);
}

/// A runtime candidate failure after compactions were written keeps metadata
/// on the complete old context generation.
/// compactions 已写入后 runtime 候选失败时，metadata 仍指向完整旧 context。
#[test]
fn runtime_write_failure_keeps_old_generation() {
    let (root, layout) = fresh_layout("runtime-fail");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    document
        .submit_root_input(
            test_submission("workspace-store-compaction-root", "compact turn"),
            3,
        )
        .unwrap();
    let api = prepare_api(
        &mut document,
        ApiPurposeKind::Compaction,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        0,
    );
    document
        .append_assistant_text(api, "c1", AssistantSemanticStatus::Completed, 4)
        .unwrap();
    complete_api(&mut document, api, 4, None);
    let (occurred_at_seq, summary) = completed_compaction_semantic(&document, api);
    document
        .record_compaction(
            CompactionCommitInput::new(
                api,
                occurred_at_seq,
                Seq::try_new(6).unwrap(),
                summary,
                LockedBaselineProjection::empty(),
            ),
            4,
        )
        .unwrap();

    let old_metadata = std::fs::read(layout.metadata_path()).unwrap();
    let old_runtime = std::fs::read(layout.runtime_path(1)).unwrap();
    let old_compactions = std::fs::read(layout.compactions_path(1)).unwrap();
    let dirty = BTreeSet::from([DirtyFile::Compactions, DirtyFile::Runtime]);
    test_hooks::fail_next_artifact(WriteStage::Rename, "flush context candidate runtime");
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &document.metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 4,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    assert!(flush_session(&layout, &request).is_err());
    test_hooks::clear_failures();

    assert_eq!(std::fs::read(layout.metadata_path()).unwrap(), old_metadata);
    assert_eq!(std::fs::read(layout.runtime_path(1)).unwrap(), old_runtime);
    assert_eq!(
        std::fs::read(layout.compactions_path(1)).unwrap(),
        old_compactions
    );
    assert!(layout.compactions_path(2).exists());
    assert!(!layout.runtime_path(2).exists());
    let loaded = load_session(&layout).unwrap();
    assert!(
        loaded.document.runtime_lock.current_compaction_id.is_none(),
        "the old runtime pointer must stay valid"
    );
    assert_eq!(loaded.document.compactions.len(), 0);
    drop(root);
}

/// A failed flush never drops the in-memory new state (plan 3.3.2).
/// 失败的 flush 绝不丢弃内存中的新状态(计划 3.3.2)。
#[test]
fn failed_flush_keeps_new_memory_state() {
    let (root, layout) = fresh_layout("mem-keep");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    document
        .append_display_note(
            None,
            DisplayNoticeKind::SystemError,
            NoticeSeverity::Info,
            "new-fact",
            None,
            None,
            None,
            None,
            9,
        )
        .unwrap();
    let events_in_memory = document.events().len();
    let dirty = BTreeSet::from([DirtyFile::ActiveTail]);
    test_hooks::fail_next_write(WriteStage::Rename);
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &document.metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 9,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    let result = flush_session(&layout, &request);
    assert!(result.is_err());
    test_hooks::clear_failures();
    assert_eq!(
        document.events().len(),
        events_in_memory,
        "the in-memory fact must survive a failed flush"
    );
    // The disk still holds the old published state and loads fine.
    // 磁盘仍持有旧已发布状态并可正常加载。
    let loaded = load_session(&layout).unwrap();
    assert_eq!(loaded.document.events().len(), events_in_memory - 1);
    drop(root);
}

/// An empty session initializes its full file set and loads back with zero
/// events (plan 13.2.1).
/// 空会话初始化完整文件集并可加载回零事件(计划 13.2.1)。
#[test]
fn empty_session_initializes_full_file_set() {
    let (root, layout) = fresh_layout("empty");
    let document = SessionDocument::new(
        layout.session_id.clone(),
        "project-1",
        "t",
        1_700_000_000_000,
    );
    let filled = create_session(
        &layout,
        &document.metadata,
        &document.runtime_lock,
        1_700_000_000_000,
        &document.compactions,
        document.events(),
    )
    .unwrap();
    assert_eq!(filled.split_list.len(), 1);
    assert_eq!(filled.split_list[0].event_count, 0);
    assert_eq!(filled.split_list[0].byte_len, 0);
    assert!(layout.metadata_path().exists());
    assert!(active_runtime_path(&layout).exists());
    assert!(active_compactions_path(&layout).exists());
    assert!(layout.segment_path(1, 1).exists());
    let loaded = load_session(&layout).unwrap();
    assert!(loaded.document.events().is_empty());
    drop(root);
}

/// The initial metadata.json golden is byte-stable for a fresh session.
/// 全新会话的初始 metadata.json golden 字节稳定。
#[test]
fn initial_metadata_golden_is_stable() {
    let (root, layout) = fresh_layout("meta-golden");
    let document = SessionDocument::new(
        layout.session_id.clone(),
        "project-1",
        "title",
        1_700_000_000_000,
    );
    create_session(
        &layout,
        &document.metadata,
        &document.runtime_lock,
        1_700_000_000_000,
        &document.compactions,
        document.events(),
    )
    .unwrap();
    let text = std::fs::read_to_string(layout.metadata_path()).unwrap();
    assert!(text.contains(r#""contract":"vulcan.session.v1""#));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).unwrap()["schema_version"],
        vulcan_session_core::v1::config::METADATA_SCHEMA_VERSION
    );
    assert!(text.contains(r#""segment_generation":1"#));
    assert!(text.contains(r#""file_name":"s0000001-a.jsonl""#));
    assert!(text.contains(r#""event_count":0"#));
    assert!(text.contains(r#""next_seq":1"#));
    drop(root);
}

/// Editing a user input inside a later segment byte-copies the sealed prefix
/// and only rewrites the target segment (plan 10.2/13.6 hard gate).
/// 编辑后面分片中的用户输入时按字节复制封口前缀，只重写目标分片
/// (计划 10.2/13.6 硬门)。
#[test]
fn edit_mid_segments_byte_copies_sealed_prefix() {
    let (root, layout) = fresh_layout("edit-mid");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    let limits = FlushLimits {
        max_lines: 3,
        max_bytes: 8 * 1024 * 1024,
    };
    // Turn two provides a second user input beyond the first sealed segment.
    // 第二个回合在第一个封口分片之后提供第二个用户输入。
    document
        .submit_root_input(
            test_submission("workspace-store-second-turn", "second turn"),
            1_700_000_000_010,
        )
        .unwrap();
    let api = prepare_api(
        &mut document,
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        0,
    );
    document
        .append_assistant_text(
            api,
            "reply",
            AssistantSemanticStatus::Completed,
            1_700_000_000_011,
        )
        .unwrap();
    complete_api(&mut document, api, 1_700_000_000_011, None);
    document
        .close_turn(TurnOutcome::Completed, 1, 1_700_000_000_012)
        .unwrap();
    let dirty = BTreeSet::from([DirtyFile::ActiveTail, DirtyFile::Metadata]);
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &document.metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 1_700_000_000_012,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    let outcome = flush_session_with_limits(&layout, &request, &limits).unwrap();
    let rolled = outcome.published_metadata.expect("rollover must happen");
    write_json_document(&layout.metadata_path(), &rolled, "metadata").unwrap();
    let sealed_source_path = layout.segment_entry_path(1, &rolled.split_list[0].file_name);
    let sealed_before = std::fs::read(&sealed_source_path).unwrap();
    assert!(
        rolled.split_list.len() >= 2,
        "rollover must seal a prefix segment"
    );

    // Find the second user input's seq and edit it.
    // 找到第二个用户输入的 seq 并编辑它。
    let target = document
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::UserInput(input) if input.text == "second turn" => Some(event.seq()),
            _ => None,
        })
        .expect("second user input must exist");
    let publish_edit_generation = super::generation::publish_edit_generation;
    let output =
        build_durable_edit_output(&layout, target, "edited second turn", 1_700_000_000_013);
    let edited = publish_edit_generation(
        &layout,
        PublishEditGenerationRequest {
            current_metadata: &rolled,
            output: &output,
        },
    )
    .unwrap();
    assert_eq!(edited.segment_generation, rolled.segment_generation + 1);
    let sealed_after = std::fs::read(&sealed_source_path).unwrap();
    assert_eq!(
        sealed_before, sealed_after,
        "the sealed prefix must be byte-copied, never re-serialized"
    );
    let loaded = load_session(&layout).unwrap();
    let second_input = loaded
        .document
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::UserInput(input) if input.text == "edited second turn" => Some(event.seq()),
            _ => None,
        })
        .expect("edited input must be present");
    let _ = second_input;
    // Events after the target were deleted: the edited input is the last
    // event and the retained stream is exactly 1..=target.
    // 目标之后的事件被删除：被编辑输入是最后一条事件，保留流精确为 1..=target。
    assert_eq!(
        loaded.document.events().len(),
        target.value() as usize,
        "the retained prefix ends at the edited input"
    );
    assert!(matches!(
        &loaded.document.events().last().unwrap().event,
        EventBody::UserInput(input) if input.text == "edited second turn"
    ));
    drop(root);
}

/// Parse failures carry both line and byte position.
/// 解析失败同时携带行号与字节位置。
#[test]
fn corrupt_line_carries_byte_offset() {
    let (root, layout) = fresh_layout("byte-pos");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    let bytes = serialize_segment(document.events()).unwrap();
    let mut broken = bytes;
    // Insert an invalid token at the start of the second line: the corruption
    // is invalid JSON no matter where it lands in the stream (byte-offset-based
    // splicing can coincidentally land inside a JSON string and stay valid).
    // 在第二行行首插入非法 token：无论损坏落在流的哪个位置都必然非法 JSON
    // (按字节偏移拼接可能恰好落进 JSON 字符串内而仍然合法)。
    let second_line_offset = broken
        .split_inclusive(|byte| *byte == b'\n')
        .nth(1)
        .map(|line| line.as_ptr() as usize - broken.as_ptr() as usize)
        .expect("fixture events serialize to at least two lines");
    broken.splice(
        second_line_offset..second_line_offset,
        b"{invalid".iter().copied(),
    );
    write_file_atomic(&layout.segment_path(1, 1), &broken, "corrupt segment").unwrap();
    let result = load_session(&layout)
        .err()
        .expect("corrupt segment must fail loading");
    assert_eq!(result.code, ErrorCode::SegmentCorrupt);
    assert!(result.line.is_some(), "line position must be present");
    assert!(result.byte_offset.is_some(), "byte offset must be present");
    drop(root);
}

/// A flush accepts a tail-only event slice when `tail_start` names the active
/// tail's absolute start seq (plan 6.2.1: candidates carry only the tail).
/// 当 `tail_start` 指名活动尾段的绝对起始 seq 时，flush 接受仅尾段的事件切片
/// (计划 6.2.1：候选只携带尾段)。
#[test]
fn flush_accepts_tail_only_event_slice() {
    let (root, layout) = fresh_layout("tail-only-flush");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    document
        .submit_root_input(
            test_submission("workspace-store-second-turn", "second turn"),
            5,
        )
        .unwrap();
    let api = prepare_api(
        &mut document,
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        0,
    );
    document
        .append_assistant_text(api, "more", AssistantSemanticStatus::Completed, 6)
        .unwrap();
    complete_api(&mut document, api, 7, None);
    let dirty = BTreeSet::from([DirtyFile::ActiveTail]);
    let small = FlushLimits {
        max_lines: 3,
        max_bytes: u64::MAX,
    };
    let rollover_request = FlushRequest {
        dirty: &dirty,
        metadata: &document.metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 7,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    let outcome = flush_session_with_limits(&layout, &rollover_request, &small).unwrap();
    let mut metadata = document.metadata.clone();
    if let Some(filled) = &outcome.published_metadata {
        metadata = filled.clone();
    }
    assert!(
        metadata
            .split_list
            .iter()
            .any(|entry| entry.end_seq.is_some()),
        "fixture: the rollover must seal a prefix"
    );
    let tail_start = metadata.split_list.last().unwrap().start_seq;
    let tail_events = &document.events()[(tail_start.value() - 1) as usize..];
    // A second flush publishes the tail using ONLY the tail slice.
    // 第二次 flush 只用尾段切片发布尾段。
    let tail_only = BTreeSet::from([DirtyFile::ActiveTail, DirtyFile::Metadata]);
    let request2 = FlushRequest {
        dirty: &tail_only,
        metadata: &metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 8,
        compactions: &document.compactions,
        events: tail_events,
        tail_start: Some(tail_start),
    };
    flush_session(&layout, &request2).unwrap();
    let loaded = load_session(&layout).unwrap();
    assert_eq!(
        loaded.document.events().len(),
        document.events().len(),
        "tail-only publication preserves the full stream"
    );
    drop(root);
}

/// A tail-only flush is rejected when its claimed start seq mismatches the
/// split map's active tail.
/// 仅尾段 flush 声称的起始 seq 与分片映射的活动尾段不一致时被拒绝。
#[test]
fn flush_rejects_mismatched_tail_start() {
    let (root, layout) = fresh_layout("tail-start-mismatch");
    let document = filled_document(&layout.session_id, 0);
    let metadata = publish_initial(&layout, &document);
    let dirty = BTreeSet::from([DirtyFile::ActiveTail]);
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 1,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: Some(Seq::try_new(99).unwrap()),
    };
    let result = flush_session(&layout, &request);
    assert!(
        matches!(
            result,
            Err(SessionV1Error {
                code: ErrorCode::SequenceDiscontinuity,
                ..
            })
        ),
        "a tail start that contradicts the split map must be rejected"
    );
    drop(root);
}

/// Publishes one flush candidate of a document and confirms its revision.
/// 发布一个文档的 flush 候选并确认其修订号。
fn flush_candidate(document: &mut SessionDocument, layout: &SessionV1Layout, now_ms: i64) {
    let candidate = document
        .prepare_flush_candidates()
        .unwrap()
        .expect("dirty files must yield a candidate");
    let dirty: BTreeSet<DirtyFile> = candidate.dirty.iter().copied().collect();
    let tail_start = candidate
        .metadata
        .split_list
        .last()
        .map(|entry| entry.start_seq);
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &candidate.metadata,
        runtime_lock: &candidate.runtime_lock,
        runtime_updated_at_ms: now_ms,
        compactions: &candidate.compactions,
        events: &candidate.tail_events,
        tail_start,
    };
    let outcome = flush_session(layout, &request).unwrap();
    if let Some(published) = outcome.published_metadata {
        document.metadata = published;
    }
    document.advance_durable(candidate.revision).unwrap();
}

/// A prepared API with zero Provider output survives a real flush/reload and
/// is closed from its durable start snapshot without fabricated identity.
/// 一个尚无 Provider 输出的已准备 API 经真实 flush/重载后仍可恢复，并由持久化
/// 起始快照闭合且不伪造身份。
#[test]
fn zero_output_api_start_survives_flush_and_reload() {
    let (root, layout) = fresh_layout("zero-output-api");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    let open_turn_id = document
        .submit_root_input(
            test_submission("workspace-store-zero-output-root", "open turn"),
            10,
        )
        .unwrap()
        .into_value();
    let mut request = vulcan_session_core::v1::event::ApiRequestSnapshot::new("openai", "gpt-5");
    request.thinking_level = Some("high".to_string());
    document
        .set_pending_request(
            PendingRequestConfig {
                provider: Some("openai".to_string()),
                connection_id: None,
                model: Some("gpt-5".to_string()),
                thinking_level: Some("high".to_string()),
                temperature: None,
                set_at_ms: 10,
            },
            10,
        )
        .unwrap();
    document
        .promote_pending_request(Some(open_turn_id), 10)
        .unwrap();
    let api = prepare_api(
        &mut document,
        ApiPurposeKind::MainGeneration,
        request.clone(),
        11,
    );
    flush_candidate(&mut document, &layout, 12);

    let (mut loaded, _) = load_session_tail_only(&layout).unwrap();
    let active = loaded.document.active_api().unwrap();
    assert_eq!(active.api_call_id, api);
    assert_eq!(active.request, request);
    assert_eq!(
        loaded.document.project_persisted_interruptions().unwrap(),
        1
    );
    flush_candidate(&mut loaded.document, &layout, 13);

    let reloaded = load_session(&layout).unwrap();
    assert!(reloaded.document.active_api().is_none());
    let stats = reloaded
        .document
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::ApiStats(stats) if stats.api_call_id == api => Some(stats),
            _ => None,
        })
        .unwrap();
    assert_eq!(stats.provider, "openai");
    assert_eq!(stats.model, "gpt-5");
    assert_eq!(stats.thinking_level.as_deref(), Some("high"));
    assert_eq!(stats.status, ApiCallStatus::Interrupted);
    drop(root);
}

/// Crash-reload round trip: an open turn with an open streaming API is
/// persisted; after reload the interruption projection statically closes the
/// API, continue records the resume facts, a fresh API finishes the turn, and
/// a final reload yields history and statistics identical to memory.
/// 崩溃重载往返：开放回合 + 开放流式 API 被持久化；重载后中断投影统计闭合
/// API，继续记录恢复事实，全新 API 完成回合，最终重载得到与内存完全一致的
/// 历史与统计。
#[test]
fn interrupted_api_continue_round_trip() {
    let (root, layout) = fresh_layout("interrupted-continue");
    let mut document = filled_document(&layout.session_id, 0);
    // Pre-crash state: a fresh open turn with an open streaming API and a
    // running tool, never closed (plan 12.21.1.5 crash window).
    // 崩溃前状态：全新开放回合 + 开放流式 API + 运行中工具，从未闭合(计划
    // 12.21.1.5 崩溃窗口)。
    document
        .submit_root_input(
            test_submission("workspace-store-interrupted-root", "open turn"),
            10,
        )
        .unwrap();
    let crashed_api = prepare_api(
        &mut document,
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        0,
    );
    document
        .append_reasoning(
            crashed_api,
            "streaming",
            AssistantSemanticStatus::Streaming,
            10,
        )
        .unwrap();
    let _tool = document
        .prepare_tool(
            crashed_api,
            RequestStreamId::try_new("fixture-stream-crashed-shell").unwrap(),
            "shell",
            ToolPayload::try_new(serde_json::json!({"cmd": "ls"})).unwrap(),
            None,
            10,
        )
        .unwrap();
    document
        .start_prepared_tool(crashed_api, _tool, 10)
        .unwrap();
    let filled = publish_initial(&layout, &document);
    document.metadata = filled;
    flush_candidate(&mut document, &layout, 11);

    // Crash: reload, project the interruption, and flush the projection.
    // 崩溃：重载、投影中断并 flush 该投影。
    let mut loaded = load_session(&layout).unwrap();
    let projected = loaded.document.project_persisted_interruptions().unwrap();
    assert!(
        projected >= 2,
        "streaming reasoning + running tool + closed API all project"
    );
    assert!(loaded.document.active_api().is_none());
    flush_candidate(&mut loaded.document, &layout, 12);

    // Continue on the same turn, then start a fresh physical API.
    // 在原回合上继续，然后启动全新物理 API。
    let continue_action_id =
        ClientActionId::try_new("workspace-store-interrupted-continue").unwrap();
    loaded
        .document
        .continue_open_turn(
            ContinuationRequest::new(continue_action_id.clone(), "", Vec::new()),
            13,
        )
        .unwrap();
    let resumed = prepare_api_with_continuation(
        &mut loaded.document,
        ApiPurposeKind::Continue,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        Some(continue_action_id),
        14,
    );
    assert_ne!(resumed.value(), crashed_api.value());
    loaded
        .document
        .append_assistant_text(resumed, "final", AssistantSemanticStatus::Completed, 15)
        .unwrap();
    complete_api(&mut loaded.document, resumed, 16, None);
    loaded
        .document
        .close_turn(TurnOutcome::Completed, 3, 17)
        .unwrap();
    flush_candidate(&mut loaded.document, &layout, 18);

    // Final reload: history and statistics are identical to memory.
    // 最终重载：历史与统计与内存完全一致。
    let final_loaded = load_session(&layout).unwrap();
    assert_eq!(
        final_loaded.document.events(),
        loaded.document.events(),
        "the persisted stream must equal the in-memory authority after the round trip"
    );
    let api_stats: Vec<(u64, ApiCallStatus)> = final_loaded
        .document
        .events()
        .iter()
        .filter_map(|event| match &event.event {
            EventBody::ApiStats(stats) => Some((stats.api_call_id.value(), stats.status)),
            _ => None,
        })
        .collect();
    assert_eq!(
        api_stats.len(),
        3,
        "one base fixture + interrupted + resumed"
    );
    assert_eq!(
        api_stats[1],
        (crashed_api.value(), ApiCallStatus::Interrupted),
        "the interrupted API must keep a permanent Interrupted record"
    );
    assert_eq!(
        api_stats[2],
        (resumed.value(), ApiCallStatus::Completed),
        "the resumed API completes the turn"
    );
    let interrupted_count = api_stats
        .iter()
        .filter(|(_, status)| *status == ApiCallStatus::Interrupted)
        .count();
    assert_eq!(
        interrupted_count, 1,
        "exactly one crashed API is interrupted"
    );
    assert!(
        final_loaded
            .document
            .events()
            .iter()
            .any(|event| matches!(&event.event, EventBody::TurnStats(_))),
        "the turn closes with its unique terminal statistics"
    );
    drop(root);
}

/// Builds one raw compaction node for loader-strictness fixtures.
/// 为加载器严格性夹具构建一个原始压缩节点。
fn node(id: u64, occurred: u64, retained: u64) -> CompactionNode {
    CompactionNode {
        compaction_id: CompactionId::try_new(id).unwrap(),
        api_call_id: ApiCallId::try_new(1).unwrap(),
        api_stats_seq: Seq::try_new(occurred).unwrap(),
        occurred_at_seq: Seq::try_new(occurred).unwrap(),
        retained_from_seq: Seq::try_new(retained).unwrap(),
        content: vulcan_session_core::v1::context::CompactionContent::Summary {
            text: String::new(),
        },
        created_at_ms: 1,
    }
}

/// The runtime document must carry the same session id as metadata.
/// runtime 文档必须携带与 metadata 相同的会话 id。
#[test]
fn loader_rejects_mismatched_runtime_session() {
    let (root, layout) = fresh_layout("runtime-session-mismatch");
    let document = filled_document(&layout.session_id, 0);
    publish_initial(&layout, &document);
    let mut runtime = read_runtime(&active_runtime_path(&layout)).unwrap();
    runtime.session_id = "other-session".to_string();
    write_json_document(&active_runtime_path(&layout), &runtime, "runtime").unwrap();
    let result = load_session(&layout);
    assert!(matches!(
        result,
        Err(SessionV1Error {
            code: ErrorCode::MetadataCorrupt,
            ..
        })
    ));
    drop(root);
}

/// The compactions document must carry the same session id as metadata.
/// compactions 文档必须携带与 metadata 相同的会话 id。
#[test]
fn loader_rejects_mismatched_compactions_session() {
    let (root, layout) = fresh_layout("compactions-session-mismatch");
    let document = filled_document(&layout.session_id, 0);
    publish_initial(&layout, &document);
    let mut compactions = read_compactions(&active_compactions_path(&layout)).unwrap();
    compactions.session_id = "other-session".to_string();
    write_json_document(
        &active_compactions_path(&layout),
        &compactions,
        "compactions",
    )
    .unwrap();
    let result = load_session(&layout);
    assert!(matches!(
        result,
        Err(SessionV1Error {
            code: ErrorCode::MetadataCorrupt,
            ..
        })
    ));
    drop(root);
}

/// A dangling runtime compaction pointer is corruption, never silently trimmed.
/// 悬空的 runtime 压缩指针是损坏，绝不静默裁剪。
#[test]
fn loader_rejects_dangling_compaction_pointer() {
    let (root, layout) = fresh_layout("dangling-pointer");
    let document = filled_document(&layout.session_id, 0);
    publish_initial(&layout, &document);
    let nodes = vec![node(1, 10, 5)];
    let compactions = CompactionsDocument::new_with_nodes(layout.session_id.clone(), nodes);
    write_json_document(
        &active_compactions_path(&layout),
        &compactions,
        "compactions",
    )
    .unwrap();
    let mut runtime = read_runtime(&active_runtime_path(&layout)).unwrap();
    runtime.current_compaction_id = Some(CompactionId::try_new(99).unwrap());
    write_json_document(&active_runtime_path(&layout), &runtime, "runtime").unwrap();
    let result = load_session(&layout);
    assert!(matches!(
        result,
        Err(SessionV1Error {
            code: ErrorCode::RuntimeCorrupt,
            ..
        })
    ));
    drop(root);
}

/// Out-of-order compaction nodes and a retention frontier behind its own
/// occurrence are both corruption.
/// 乱序压缩节点、以及保留边界晚于其自身发生点都是损坏。
#[test]
fn loader_rejects_bad_compaction_order() {
    let (root, layout) = fresh_layout("bad-node-order");
    let document = filled_document(&layout.session_id, 0);
    publish_initial(&layout, &document);
    let reversed = vec![node(2, 20, 15), node(1, 10, 5)];
    let compactions = CompactionsDocument::new_with_nodes(layout.session_id.clone(), reversed);
    write_json_document(
        &active_compactions_path(&layout),
        &compactions,
        "compactions",
    )
    .unwrap();
    let mut runtime = read_runtime(&active_runtime_path(&layout)).unwrap();
    runtime.current_compaction_id = Some(CompactionId::try_new(1).unwrap());
    write_json_document(&active_runtime_path(&layout), &runtime, "runtime").unwrap();
    assert!(matches!(
        load_session(&layout),
        Err(SessionV1Error {
            code: ErrorCode::CompactionCorrupt,
            ..
        })
    ));
    let bad_frontier = vec![node(1, 10, 15)];
    let compactions = CompactionsDocument::new_with_nodes(layout.session_id.clone(), bad_frontier);
    write_json_document(
        &active_compactions_path(&layout),
        &compactions,
        "compactions",
    )
    .unwrap();
    assert!(matches!(
        load_session(&layout),
        Err(SessionV1Error {
            code: ErrorCode::CompactionCorrupt,
            ..
        })
    ));
    drop(root);
}

/// Non-canonical split file names and split maps without a single final active
/// tail are metadata corruption.
/// 非规范分片文件名、以及没有唯一最终活动尾段的分片映射是 metadata 损坏。
#[test]
fn loader_rejects_bad_split_map_shape() {
    let (root, layout) = fresh_layout("bad-split-shape");
    let document = filled_document(&layout.session_id, 0);
    publish_initial(&layout, &document);
    let mut metadata = read_metadata(&layout.metadata_path()).unwrap();
    metadata.split_list[0].file_name = "wrong.jsonl".to_string();
    write_json_document(&layout.metadata_path(), &metadata, "metadata").unwrap();
    assert!(matches!(
        load_session(&layout),
        Err(SessionV1Error {
            code: ErrorCode::MetadataCorrupt,
            ..
        })
    ));
    let mut metadata = read_metadata(&layout.metadata_path()).unwrap();
    metadata.split_list[0].file_name =
        SessionV1Layout::segment_file_name(metadata.split_list[0].segment_number);
    let sealed = metadata.split_list[0].clone();
    let mut sealed = sealed;
    sealed.end_seq = Some(sealed.start_seq);
    metadata.split_list.push(sealed);
    write_json_document(&layout.metadata_path(), &metadata, "metadata").unwrap();
    assert!(matches!(
        load_session(&layout),
        Err(SessionV1Error {
            code: ErrorCode::MetadataCorrupt,
            ..
        })
    ));
    drop(root);
}

/// A sealed segment whose split event_count mismatches the file is corruption.
/// 封口分片的分片 event_count 与文件不一致是损坏。
#[test]
fn loader_rejects_sealed_event_count_mismatch() {
    let (root, layout) = fresh_layout("sealed-count-mismatch");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    let dirty = BTreeSet::from([DirtyFile::ActiveTail]);
    let small = FlushLimits {
        max_lines: 2,
        max_bytes: u64::MAX,
    };
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &document.metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 1,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    let outcome = flush_session_with_limits(&layout, &request, &small).unwrap();
    let mut rolled = document.metadata.clone();
    if let Some(filled) = &outcome.published_metadata {
        rolled = filled.clone();
    }
    write_json_document(&layout.metadata_path(), &rolled, "metadata").unwrap();
    assert!(
        rolled
            .split_list
            .iter()
            .any(|entry| entry.end_seq.is_some()),
        "fixture: the rollover must seal a prefix"
    );
    let mut metadata = read_metadata(&layout.metadata_path()).unwrap();
    if let Some(sealed) = metadata.split_list.iter_mut().find(|e| e.end_seq.is_some()) {
        sealed.event_count += 1;
    }
    write_json_document(&layout.metadata_path(), &metadata, "metadata").unwrap();
    assert!(matches!(
        load_session(&layout),
        Err(SessionV1Error {
            code: ErrorCode::SegmentCorrupt,
            ..
        })
    ));
    drop(root);
}

/// In-process immutable-context publication trims a node whose candidate
/// runtime failed before metadata committed it.
/// 同进程不可变 context 发布会裁剪 runtime 候选在 metadata 提交前失败的节点。
#[test]
fn same_process_runtime_failure_trims_candidates_on_next_record() {
    let (root, layout) = fresh_layout("same-proc-runtime-fail");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    document
        .submit_root_input(
            test_submission("workspace-store-compaction-root", "compact turn"),
            3,
        )
        .unwrap();
    let api = prepare_api(
        &mut document,
        ApiPurposeKind::Compaction,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        0,
    );
    document
        .append_assistant_text(api, "c1", AssistantSemanticStatus::Completed, 4)
        .unwrap();
    complete_api(&mut document, api, 4, None);
    let (occurred_at_seq, summary) = completed_compaction_semantic(&document, api);
    let node1 = document
        .record_compaction(
            CompactionCommitInput::new(
                api,
                occurred_at_seq,
                Seq::try_new(6).unwrap(),
                summary,
                LockedBaselineProjection::empty(),
            ),
            4,
        )
        .unwrap()
        .into_value();
    // Publish node1 durably.
    // 持久化发布 node1。
    flush_candidate(&mut document, &layout, 5);
    assert_eq!(document.compactions.len(), 1);

    // A new candidate is backed by its own completed compaction API semantic.
    // 新候选由它自己已完成的压缩 API 语义提供事实依据。
    let api2 = prepare_api(
        &mut document,
        ApiPurposeKind::Compaction,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        5,
    );
    document
        .append_assistant_text(api2, "c2", AssistantSemanticStatus::Completed, 5)
        .unwrap();
    complete_api(&mut document, api2, 5, None);
    let (occurred_at_seq, summary) = completed_compaction_semantic(&document, api2);
    let node2 = document
        .record_compaction(
            CompactionCommitInput::new(
                api2,
                occurred_at_seq,
                Seq::try_new(10).unwrap(),
                summary,
                LockedBaselineProjection::empty(),
            ),
            5,
        )
        .unwrap()
        .into_value();
    let candidate = document.prepare_flush_candidates().unwrap().unwrap();
    let dirty: BTreeSet<DirtyFile> = candidate.dirty.iter().copied().collect();
    test_hooks::fail_next_artifact(WriteStage::Rename, "flush context candidate runtime");
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &candidate.metadata,
        runtime_lock: &candidate.runtime_lock,
        runtime_updated_at_ms: 5,
        compactions: &candidate.compactions,
        events: &candidate.tail_events,
        tail_start: None,
    };
    assert!(flush_session(&layout, &request).is_err());
    test_hooks::clear_failures();
    let on_disk = read_compactions(&active_compactions_path(&layout)).unwrap();
    assert_eq!(
        on_disk.nodes.len(),
        1,
        "metadata must keep selecting the previous complete context"
    );
    assert!(layout.compactions_path(3).exists());
    assert!(!layout.runtime_path(3).exists());

    // The next record trims node2 and a clean flush publishes node1+node3.
    // 下一次记录裁剪 node2，干净 flush 只发布 node1+node3。
    let api3 = prepare_api(
        &mut document,
        ApiPurposeKind::Compaction,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        6,
    );
    document
        .append_assistant_text(api3, "c3", AssistantSemanticStatus::Completed, 6)
        .unwrap();
    complete_api(&mut document, api3, 6, None);
    let (occurred_at_seq, summary) = completed_compaction_semantic(&document, api3);
    let node3 = document
        .record_compaction(
            CompactionCommitInput::new(
                api3,
                occurred_at_seq,
                Seq::try_new(12).unwrap(),
                summary,
                LockedBaselineProjection::empty(),
            ),
            6,
        )
        .unwrap()
        .into_value();
    let ids: Vec<u64> = document
        .compactions
        .iter()
        .map(|node| node.compaction_id.value())
        .collect();
    assert_eq!(ids, vec![node1.value(), node3.value()]);
    assert!(
        !ids.contains(&node2.value()),
        "the candidate must be trimmed"
    );
    flush_candidate(&mut document, &layout, 7);
    let loaded = load_session(&layout).unwrap();
    let loaded_ids: Vec<u64> = loaded
        .document
        .compactions
        .iter()
        .map(|node| node.compaction_id.value())
        .collect();
    assert_eq!(
        loaded_ids,
        vec![node1.value(), node3.value()],
        "the failed candidate never resurfaces after reload"
    );
    drop(root);
}

/// An edit at a target inside a compaction node's deleted history re-selects
/// the runtime pointer into the retained nodes; a pointer beyond the target is
/// never carried over (audit P0#8).
/// 在压缩节点所属已删除历史内编辑时，runtime 指针被重选入保留节点；超过
/// 目标的指针绝不继承(审计 P0#8)。
#[test]
fn edit_reselects_compaction_pointer_in_deleted_history() {
    let (root, layout) = fresh_layout("edit-reselect");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    document
        .submit_root_input(
            test_submission("workspace-store-compaction-root", "compact turn"),
            3,
        )
        .unwrap();
    let api = prepare_api(
        &mut document,
        ApiPurposeKind::Compaction,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        0,
    );
    document
        .append_assistant_text(api, "c1", AssistantSemanticStatus::Completed, 4)
        .unwrap();
    complete_api(&mut document, api, 4, None);
    let (occurred_at_seq, summary) = completed_compaction_semantic(&document, api);
    document
        .record_compaction(
            CompactionCommitInput::new(
                api,
                occurred_at_seq,
                Seq::try_new(9).unwrap(),
                summary,
                LockedBaselineProjection::empty(),
            ),
            4,
        )
        .unwrap();
    flush_candidate(&mut document, &layout, 5);
    assert_eq!(document.compactions.len(), 1);

    // Edit the very first user input: the compaction at seq 10 lies inside the
    // deleted history, so the kept node set is empty and the pointer must be
    // re-selected to None instead of carrying the dangling node over.
    // 编辑第一个用户输入：seq 10 的压缩位于已删除历史内，保留节点集为空，
    // 指针必须重选为 None，绝不继承悬空节点。
    let output =
        build_durable_edit_output(&layout, Seq::try_new(1).unwrap(), "edited first input", 6);
    let edited = publish_edit_generation(
        &layout,
        PublishEditGenerationRequest {
            current_metadata: &document.metadata,
            output: &output,
        },
    )
    .unwrap();
    assert_eq!(edited.segment_generation, 2);
    let loaded = load_session(&layout).unwrap();
    assert!(
        loaded.document.runtime_lock.current_compaction_id.is_none(),
        "the re-selected pointer must not reference deleted history"
    );
    assert!(
        loaded.document.compactions.is_empty(),
        "no compaction node survives outside the retained history"
    );
    assert_eq!(
        loaded.document.events().len(),
        1,
        "the edit stream retains only the replaced input"
    );
    drop(root);
}

/// Any failure before the metadata switch leaves the source generation
/// byte-for-byte intact: all pre-switch writes stay inside the candidate
/// generation directory (audit P0#8).
/// metadata 切换前的任何失败都让源 generation 逐字节不变：切换前的一切
/// 写入都留在候选 generation 目录内(审计 P0#8)。
#[test]
fn edit_pre_switch_failure_keeps_source_generation_byte_identical() {
    let (root, layout) = fresh_layout("edit-pre-switch-fail");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    let snapshot = (
        std::fs::read(layout.metadata_path()).unwrap(),
        std::fs::read(active_runtime_path(&layout)).unwrap(),
        std::fs::read(active_compactions_path(&layout)).unwrap(),
        std::fs::read(layout.segment_path(1, 1)).unwrap(),
    );
    let output = build_durable_edit_output(&layout, Seq::try_new(1).unwrap(), "never lands", 6);
    test_hooks::fail_next_artifact(WriteStage::Rename, "edit context candidate runtime");
    let result = publish_edit_generation(
        &layout,
        PublishEditGenerationRequest {
            current_metadata: &document.metadata,
            output: &output,
        },
    );
    assert!(result.is_err(), "the armed failure must abort the edit");
    test_hooks::clear_failures();
    assert_eq!(
        std::fs::read(layout.metadata_path()).unwrap(),
        snapshot.0,
        "source metadata must be byte-identical after a pre-switch failure"
    );
    assert_eq!(
        std::fs::read(active_runtime_path(&layout)).unwrap(),
        snapshot.1,
        "source runtime must be byte-identical after a pre-switch failure"
    );
    assert_eq!(
        std::fs::read(active_compactions_path(&layout)).unwrap(),
        snapshot.2,
        "source compactions must be byte-identical after a pre-switch failure"
    );
    assert_eq!(
        std::fs::read(layout.segment_path(1, 1)).unwrap(),
        snapshot.3,
        "source segments must be byte-identical after a pre-switch failure"
    );
    assert!(layout.segment_path(2, 1).exists());
    assert!(layout.compactions_path(2).exists());
    assert!(!layout.runtime_path(2).exists());
    load_session(&layout).unwrap();
    drop(root);
}

/// Metadata replacement is the sole edit commit point: when it fails after
/// both candidate trees are complete, the old pair remains authoritative.
/// metadata 替换是编辑的唯一提交点：两个候选树完整后若其失败，旧组合仍权威。
#[test]
fn edit_metadata_failure_keeps_old_generation_authoritative() {
    let (root, layout) = fresh_layout("edit-metadata-fail");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    let metadata_before = std::fs::read(layout.metadata_path()).unwrap();
    let event_count_before = document.events().len();
    let output =
        build_durable_edit_output(&layout, Seq::try_new(1).unwrap(), "candidate-only edit", 6);
    test_hooks::fail_next_artifact(WriteStage::Rename, "metadata generation switch");
    let result = publish_edit_generation(
        &layout,
        PublishEditGenerationRequest {
            current_metadata: &document.metadata,
            output: &output,
        },
    );
    assert!(result.is_err(), "metadata failure must abort publication");
    test_hooks::clear_failures();
    assert_eq!(
        std::fs::read(layout.metadata_path()).unwrap(),
        metadata_before
    );
    assert!(layout.segment_path(2, 1).exists());
    assert!(layout.compactions_path(2).exists());
    assert!(layout.runtime_path(2).exists());
    let loaded = load_session(&layout).unwrap();
    assert_eq!(loaded.document.events().len(), event_count_before);
    assert!(matches!(
        &loaded.document.events()[0].event,
        EventBody::UserInput(input) if input.text == "hello"
    ));
    assert_eq!(loaded.metadata.segment_generation, 1);
    assert_eq!(loaded.metadata.active_context_generation, 1);
    drop(root);
}

/// A fork re-selects the runtime pointer into the kept nodes, preserves their
/// durable identities, and never carries a pointer beyond the target (audit P0#8).
/// 分叉把 runtime 指针重选入保留节点，保留其持久化身份，绝不继承超过目标的指针
/// (审计 P0#8)。
#[test]
fn fork_reselects_pointer_and_preserves_kept_nodes() {
    let (root, layout) = fresh_layout("fork-remap");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    document
        .submit_root_input(
            test_submission("workspace-store-compaction-root", "compact turn"),
            3,
        )
        .unwrap();
    let api = prepare_api(
        &mut document,
        ApiPurposeKind::Compaction,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        0,
    );
    document
        .append_assistant_text(api, "c1", AssistantSemanticStatus::Completed, 4)
        .unwrap();
    complete_api(&mut document, api, 4, None);
    let (occurred_at_seq, summary) = completed_compaction_semantic(&document, api);
    let node1 = document
        .record_compaction(
            CompactionCommitInput::new(
                api,
                occurred_at_seq,
                Seq::try_new(9).unwrap(),
                summary,
                LockedBaselineProjection::empty(),
            ),
            4,
        )
        .unwrap()
        .into_value();
    flush_candidate(&mut document, &layout, 5);
    document
        .close_turn(TurnOutcome::Completed, 1, 6)
        .expect("first compaction turn must close");
    submit_test_root(
        &mut document,
        "workspace-store-fork-compaction-boundary",
        "fork after first compaction",
        7,
    );
    let fork_target_seq = document
        .close_turn(TurnOutcome::Completed, 1, 7)
        .expect("fork boundary Turn must close");
    // A second node beyond any fork candidate boundary must never be carried.
    // 任何分叉候选边界之后的第二个节点绝不继承。
    submit_test_root(
        &mut document,
        "workspace-store-post-fork-compaction",
        "second compaction after fork boundary",
        8,
    );
    let api2 = prepare_api(
        &mut document,
        ApiPurposeKind::Compaction,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        8,
    );
    document
        .append_assistant_text(api2, "c2", AssistantSemanticStatus::Completed, 9)
        .unwrap();
    complete_api(&mut document, api2, 10, None);
    let (occurred_at_seq, summary) = completed_compaction_semantic(&document, api2);
    let _node2 = document
        .record_compaction(
            CompactionCommitInput::new(
                api2,
                occurred_at_seq,
                Seq::try_new(11).unwrap(),
                summary,
                LockedBaselineProjection::empty(),
            ),
            10,
        )
        .unwrap();
    assert_eq!(document.compactions.len(), 2);
    flush_candidate(&mut document, &layout, 11);

    // The boundary is one completed Turn after the first compaction and before
    // the second one, so the first node is retained with exact identities.
    // 边界是第一压缩后、第二压缩前的完整回合，因此第一个节点以精确身份保留。
    let (_fork_root, fork_layout) = fresh_layout("fork-remap-new");
    let output = build_durable_fork_output(&layout, &fork_layout, fork_target_seq, 12);
    let published = create_session(
        &fork_layout,
        &output.metadata,
        &output.runtime_lock,
        output.metadata.updated_at_ms,
        &output.compactions,
        &output.events,
    )
    .unwrap();
    assert_eq!(
        output.events.last().map(|event| event.seq),
        Some(fork_target_seq),
        "the fork stream preserves the exact retained boundary"
    );
    let loaded = load_session(&fork_layout).unwrap();
    assert_eq!(loaded.document.compactions.len(), 1);
    let kept = &loaded.document.compactions[0];
    // This fork fixture owns a local summary; opaque checkpoints have a separate replay path.
    // 此分支夹具持有本地摘要；不透明检查点使用独立重放路径。
    let vulcan_session_core::v1::context::CompactionContent::Summary { text: summary } =
        &kept.content
    else {
        panic!("expected local summary");
    };
    let retained_summary_seq = output
        .events
        .iter()
        .find_map(|event| match &event.event {
            EventBody::AssistantText(text) if &text.text == summary => Some(event.seq),
            EventBody::AssistantReasoning(reasoning) if &reasoning.text == summary => {
                Some(event.seq)
            }
            _ => None,
        })
        .expect("fork keeps the compaction summary semantic");
    assert_eq!(
        kept.compaction_id, node1,
        "only the node inside the target is kept"
    );
    assert_eq!(
        kept.occurred_at_seq, retained_summary_seq,
        "the kept node's seq references remain exact"
    );
    assert_eq!(kept.retained_from_seq.value(), 9);
    assert_eq!(
        loaded.document.runtime_lock.current_compaction_id,
        Some(node1),
        "the runtime pointer is re-selected into the kept node"
    );
    assert_eq!(published.id_frontier, output.metadata.id_frontier);
    drop(root);
}

/// The context-compaction node never changes physical segment rollover
/// thresholds (audit P0#10).
/// 上下文压缩节点绝不改变物理分片滚段阈值(审计 P0#10)。
#[test]
fn context_compaction_node_does_not_control_physical_rollover() {
    let (root, layout) = fresh_layout("compaction-window");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    document.set_context_compaction_window_node_id(
        Some(vulcan_ai_contract::ContextCompactionWindowNodeId::CompressionWindow2),
        2,
    );
    // Extend the tail, then use an explicit large physical limit. The context
    // node must not override this independent storage policy.
    // 增长尾段并显式使用较大的物理限制；上下文节点不得覆盖独立存储策略。
    document
        .submit_root_input(
            test_submission("workspace-store-more-events", "more events"),
            3,
        )
        .unwrap();
    let api = prepare_api(
        &mut document,
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        0,
    );
    document
        .append_assistant_text(api, "x", AssistantSemanticStatus::Completed, 4)
        .unwrap();
    complete_api(&mut document, api, 5, None);
    let limits = FlushLimits {
        max_lines: u64::MAX,
        max_bytes: u64::MAX,
    };
    let dirty = BTreeSet::from([DirtyFile::ActiveTail]);
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &document.metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 5,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    let outcome = flush_session_with_limits(&layout, &request, &limits).unwrap();
    let published = outcome
        .published_metadata
        .expect("every tail candidate must be published through metadata");
    assert_eq!(
        published.split_list.len(),
        1,
        "the context node must not trigger a physical rollover"
    );
    drop(root);
}

/// The selected context-compaction node round-trips through metadata and a full
/// reload (audit P0#10).
/// 选定的上下文压缩节点经 metadata 和完整重载往返(审计 P0#10)。
#[test]
fn context_compaction_window_node_persists_across_reload() {
    let (root, layout) = fresh_layout("window-persist");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    document.set_context_compaction_window_node_id(
        Some(vulcan_ai_contract::ContextCompactionWindowNodeId::CompressionWindow4),
        1,
    );
    let dirty = BTreeSet::from([DirtyFile::Metadata]);
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &document.metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 1,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    flush_session(&layout, &request).unwrap();
    let loaded = load_session(&layout).unwrap();
    assert_eq!(
        loaded
            .document
            .metadata
            .config
            .context_compaction_window_node_id,
        Some(vulcan_ai_contract::ContextCompactionWindowNodeId::CompressionWindow4)
    );
    drop(root);
}

/// The tail-only loader retains only the active tail, keeps every sealed
/// segment addressable through the catalog, and reports its metrics (audit
/// P0#2 acceptance: open without materializing sealed history).
/// tail-only 加载器只保留活动尾段，通过 catalog 保持每个封口分片可寻址，
/// 并报告其指标(审计 P0#2 验收：打开时不物化封口历史)。
#[test]
fn tail_only_load_keeps_resident_to_active_tail() {
    let (root, layout) = fresh_layout("tail-only-load");
    let mut document = filled_document(&layout.session_id, 0);
    document.metadata = publish_initial(&layout, &document);
    // Several more closed turns force multiple rollovers with a small window.
    // 多个已闭合回合在小窗口下强制多次滚段。
    for index in 0..3u64 {
        document
            .submit_root_input(
                test_submission(
                    format!("workspace-store-tail-turn-{index}"),
                    format!("turn {index}"),
                ),
                10 + index as i64,
            )
            .unwrap();
        let api = prepare_api(
            &mut document,
            ApiPurposeKind::MainGeneration,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        );
        document
            .append_assistant_text(
                api,
                "x",
                AssistantSemanticStatus::Completed,
                10 + index as i64,
            )
            .unwrap();
        complete_api(&mut document, api, 10 + index as i64, None);
        document
            .close_turn(TurnOutcome::Completed, 1, 11 + index as i64)
            .unwrap();
    }
    let limits = FlushLimits {
        max_lines: 2,
        max_bytes: u64::MAX,
    };
    let dirty = BTreeSet::from([DirtyFile::ActiveTail, DirtyFile::Metadata]);
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &document.metadata,
        runtime_lock: &document.runtime_lock,
        runtime_updated_at_ms: 13,
        compactions: &document.compactions,
        events: document.events(),
        tail_start: None,
    };
    let outcome = flush_session_with_limits(&layout, &request, &limits).unwrap();
    if let Some(filled) = &outcome.published_metadata {
        document.metadata = filled.clone();
    }
    assert!(
        document.metadata.split_list.len() >= 2,
        "fixture: multiple sealed segments exist"
    );
    let total_events = document.events().len() as u64;

    let expected_frontier = document.id_frontier();
    let (mut loaded, report) = load_session_tail_only(&layout).unwrap();
    assert_eq!(
        report.catalog_entries,
        document.metadata.split_list.len() as u64
    );
    assert!(
        report.resident_event_count < total_events,
        "sealed history is not retained: resident {} < total {total_events}",
        report.resident_event_count
    );
    assert_eq!(
        report.resident_event_count,
        loaded.document.events().len() as u64,
        "the resident stream is exactly the active tail"
    );
    assert_eq!(
        report.opened_segment_files, 1,
        "only the active tail is opened"
    );
    assert_eq!(
        loaded.document.segment_catalog().len(),
        report.catalog_entries as usize,
        "every sealed segment stays addressable through the catalog"
    );
    assert_eq!(
        loaded.document.id_frontier(),
        expected_frontier,
        "tail-only loading restores both allocator dimensions from catalog and tail facts"
    );
    let resumed_turn = loaded
        .document
        .submit_root_input(
            test_submission("workspace-store-after-reload", "after reload"),
            20,
        )
        .unwrap()
        .into_value();
    assert_eq!(
        resumed_turn.value(),
        expected_frontier.next_local_id + 1,
        "the hidden input id consumes the frontier and the turn receives the following id"
    );
    assert_eq!(
        loaded.document.events().last().unwrap().seq().value(),
        expected_frontier.next_seq,
        "the empty active tail resumes at the global seq frontier"
    );
    drop(root);
}

/// Flushes one long-session tail with the production rollover thresholds.
/// 使用生产滚段阈值 flush 一次长会话尾段。
fn flush_large_session_tail(layout: &SessionV1Layout, document: &mut SessionDocument, now_ms: i64) {
    flush_with_limits(layout, document, &FlushLimits::default(), now_ms);
}

/// Recent hydration and backward pagination preserve complete Turn boundaries.
/// 最近 hydration 与向前分页保持完整 Turn 边界。
#[test]
fn turn_windows_are_complete_adjacent_and_segment_precise() {
    let (root, layout) = fresh_layout("turn-windows");
    let mut document = SessionDocument::new(&layout.session_id, "project-1", "title", 1);
    document.metadata = publish_initial(&layout, &document);
    let limits = FlushLimits {
        max_lines: 20,
        max_bytes: 8 * 1024 * 1024,
    };
    for index in 0..40u64 {
        append_completed_turn(&mut document, index, 10 + index as i64);
        flush_with_limits(&layout, &mut document, &limits, 10 + index as i64);
    }
    let recent = load_recent_turn_window(&layout, 30).unwrap();
    assert_eq!(distinct_turn_count(&recent.events), 30);
    assert!(matches!(
        recent.events.first().map(|event| &event.event),
        Some(EventBody::UserInput(_))
    ));
    assert!(matches!(
        recent.events.last().map(|event| &event.event),
        Some(EventBody::TurnStats(_))
    ));
    let cursor = recent.older_before_seq.expect("ten older turns remain");
    let older = load_turn_window_before(&layout, cursor, 30).unwrap();
    assert_eq!(distinct_turn_count(&older.events), 10);
    assert_eq!(older.older_before_seq, None);
    assert_eq!(
        older.events.last().unwrap().seq().value() + 1,
        recent.events.first().unwrap().seq().value(),
        "neighboring pages are exactly adjacent"
    );
    assert!(
        recent.report.opened_segment_files < document.metadata.split_list.len() as u64,
        "recent hydration must not parse every sealed segment"
    );
    drop(root);
}

/// Recent hydration returns thirty completed Turns plus the entire open Turn,
/// while the adjacent older page still begins and ends on terminal boundaries.
/// 最近 hydration 返回三十个已完成 Turn 与整个开放 Turn，同时相邻更早分页仍在
/// 终态边界上开始和结束。
#[test]
fn recent_window_keeps_thirty_completed_turns_plus_open_turn() {
    let (root, layout) = fresh_layout("turn-window-open-tail");
    let mut document = SessionDocument::new(&layout.session_id, "project-1", "title", 1);
    document.metadata = publish_initial(&layout, &document);
    let limits = FlushLimits {
        max_lines: 20,
        max_bytes: 8 * 1024 * 1024,
    };
    for index in 0..35u64 {
        append_completed_turn(&mut document, index, 10 + index as i64);
        flush_with_limits(&layout, &mut document, &limits, 10 + index as i64);
    }
    let open_turn_id = document
        .submit_root_input(
            test_submission("workspace-store-open-turn-window", "open-turn"),
            100,
        )
        .unwrap()
        .into_value();
    let open_start_seq = document.active_turn().unwrap().start_seq;
    let open_api_id = prepare_api(
        &mut document,
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        101,
    );
    document
        .append_assistant_text(
            open_api_id,
            "streaming",
            AssistantSemanticStatus::Streaming,
            102,
        )
        .unwrap();
    flush_with_limits(&layout, &mut document, &limits, 102);

    let recent = load_recent_turn_window(&layout, 30).unwrap();
    assert_eq!(distinct_turn_count(&recent.events), 31);
    assert_eq!(recent.state.open_turn_start_seq, Some(open_start_seq));
    assert_eq!(
        recent.events.last().and_then(|event| event.turn_id),
        Some(open_turn_id)
    );
    assert!(!matches!(
        recent.events.last().map(|event| &event.event),
        Some(EventBody::TurnStats(_))
    ));
    let cursor = recent
        .older_before_seq
        .expect("five older completed Turns remain");
    let older = load_turn_window_before(&layout, cursor, 30).unwrap();
    assert_eq!(distinct_turn_count(&older.events), 5);
    assert_eq!(older.older_before_seq, None);
    assert_eq!(
        older.events.last().unwrap().seq().value() + 1,
        recent.events.first().unwrap().seq().value()
    );
    drop(root);
}

/// Provider range loading opens only intersecting segments and returns every
/// requested seq exactly once.
/// Provider 范围加载只打开相交分片，并把每个请求 seq 精确返回一次。
#[test]
fn provider_window_reads_only_intersecting_segments() {
    let (root, layout) = fresh_layout("provider-window");
    let mut document = SessionDocument::new(&layout.session_id, "project-1", "title", 1);
    document.metadata = publish_initial(&layout, &document);
    let limits = FlushLimits {
        max_lines: 12,
        max_bytes: 8 * 1024 * 1024,
    };
    for index in 0..12u64 {
        append_completed_turn(&mut document, index, 10 + index as i64);
        flush_with_limits(&layout, &mut document, &limits, 10 + index as i64);
    }
    let range = load_provider_event_range(
        &layout,
        Seq::try_new(25).unwrap(),
        Seq::try_new(40).unwrap(),
    )
    .unwrap();
    assert_eq!(range.events.len(), 16);
    assert_eq!(range.events.first().unwrap().seq().value(), 25);
    assert_eq!(range.events.last().unwrap().seq().value(), 40);
    assert!(
        range.report.opened_segment_files < document.metadata.split_list.len() as u64,
        "Provider projection must not parse unrelated history"
    );
    drop(root);
}

/// Profiles the five semantic phases used to construct completed Turns without storage I/O.
/// 在不包含存储 I/O 的情况下剖析已完成回合构造所用的五个语义阶段。
#[test]
#[ignore = "explicit Release performance evidence"]
fn performance_completed_turn_phase_costs() {
    let mut document = SessionDocument::new("phase-costs", "project-1", "title", 1);
    let mut timings = CompletedTurnTimings::default();
    for index in 0..2_000u64 {
        timings.accumulate(append_completed_turn(
            &mut document,
            index,
            10 + index as i64,
        ));
    }
    eprintln!(
        "[perf-turn-phases] turns=2000 events={} submit_root_ms={} prepare_api_ms={} assistant_ms={} complete_api_ms={} close_turn_ms={}",
        document.events().len(),
        timings.submit_root.as_millis(),
        timings.prepare_api.as_millis(),
        timings.append_assistant.as_millis(),
        timings.complete_api.as_millis(),
        timings.close_turn.as_millis(),
    );
}

/// A session spanning several sealed production-threshold segments keeps startup and recent history
/// bounded, serves an exact Provider range, and edits inside an earlier segment by copying
/// immutable prefix bytes into one new generation.
/// 跨越多个按生产阈值封口分片的会话保持启动与最近历史读取有界，精确提供 Provider 范围，
/// 并在较早分片内通过复制不可变前缀字节构建一次新代次编辑。
/// Every bound is derived from the turn count; the I/O limits do not depend on total session size.
/// 所有边界均由回合数推导；I/O 上限与会话总量无关。
#[test]
fn sealed_segments_bound_windows_and_edit_io() {
    // TURNS spans several sealed segments; each completed scale turn writes EVENTS_PER_TURN lines.
    // TURNS 跨越多个封口分片；每个完成的规模回合写入 EVENTS_PER_TURN 行。
    const TURNS: u64 = 2_600;
    const EVENTS_PER_TURN: u64 = 6;
    // FLUSH_TURNS seals exactly one production segment per flush.
    // FLUSH_TURNS 使每次刷写恰好封口一个生产分片。
    const FLUSH_TURNS: u64 = SEGMENT_MAX_LINES / EVENTS_PER_TURN;
    let (root, layout) = fresh_layout("sealed-segments-windows-edit");
    let mut document = SessionDocument::new(&layout.session_id, "project-1", "title", 1);
    document.metadata = publish_initial(&layout, &document);
    for index in 0..TURNS {
        append_completed_storage_scale_turn(&mut document, index, 10 + index as i64);
        if (index + 1) % FLUSH_TURNS == 0 {
            flush_large_session_tail(&layout, &mut document, 10 + index as i64);
        }
    }
    flush_large_session_tail(&layout, &mut document, 20_000);
    let total = TURNS * EVENTS_PER_TURN;
    assert_eq!(document.events().len() as u64, total);
    assert!(
        document.metadata.split_list.len() as u64 > total / SEGMENT_MAX_LINES,
        "the fixture must span several sealed segments"
    );

    let (tail_loaded, tail_report) = load_session_tail_only(&layout).unwrap();
    assert_eq!(tail_report.opened_segment_files, 1);
    assert!(tail_loaded.document.events().len() <= SEGMENT_MAX_LINES as usize);
    let recent = load_recent_turn_window(&layout, 30).unwrap();
    assert_eq!(distinct_turn_count(&recent.events), 30);
    assert!(recent.report.opened_segment_files <= 2);
    let older_cursor = recent.older_before_seq.expect("older turns must remain");
    let older = load_turn_window_before(&layout, older_cursor, 30).unwrap();
    assert_eq!(distinct_turn_count(&older.events), 30);
    assert_eq!(
        older.events.last().unwrap().seq().value() + 1,
        recent.events.first().unwrap().seq().value()
    );

    // provider_first..=provider_last lies inside the newest segment, near but not at the end.
    // provider_first..=provider_last 位于最新分片内，接近但不在末尾。
    let provider_first = total - 299;
    let provider_last = provider_first + 29;
    let provider = load_provider_event_range(
        &layout,
        Seq::try_new(provider_first).unwrap(),
        Seq::try_new(provider_last).unwrap(),
    )
    .unwrap();
    assert_eq!(provider.events.len(), 30);
    assert_eq!(
        provider.events.first().unwrap().seq().value(),
        provider_first
    );
    assert_eq!(provider.events.last().unwrap().seq().value(), provider_last);
    assert_eq!(provider.report.opened_segment_files, 1);

    // target is the user input opening a turn in an earlier sealed segment; its predecessor segment is immutable.
    // target 是较早封口分片中开启某回合的用户输入；其前一个分片不可变。
    let target_seq = Seq::try_new((TURNS - FLUSH_TURNS * 3) * EVENTS_PER_TURN + 1).unwrap();
    assert!(matches!(
        document
            .events()
            .get(target_seq.value() as usize - 1)
            .map(|event| &event.event),
        Some(EventBody::UserInput(_))
    ));
    let immutable_prefix_number = (target_seq.value() - 1) / SEGMENT_MAX_LINES;
    let source_prefix_entry = document
        .metadata
        .split_list
        .iter()
        .find(|entry| entry.segment_number == immutable_prefix_number)
        .expect("immutable prefix segment must exist");
    let source_prefix_path = layout.segment_entry_path(1, &source_prefix_entry.file_name);
    let source_prefix_before = std::fs::read(&source_prefix_path).unwrap();
    let output = build_durable_edit_output(&layout, target_seq, "edited-target", 20_001);
    let edited = publish_edit_generation(
        &layout,
        PublishEditGenerationRequest {
            current_metadata: &document.metadata,
            output: &output,
        },
    )
    .unwrap();
    assert_eq!(edited.segment_generation, 2);
    assert_eq!(edited.open_turn_start_seq, Some(target_seq));
    assert_eq!(
        std::fs::read(&source_prefix_path).unwrap(),
        source_prefix_before,
        "editing must not mutate the published source generation"
    );
    assert_eq!(
        std::fs::read(
            layout.segment_entry_path(
                2,
                &edited
                    .split_list
                    .iter()
                    .find(|entry| entry.segment_number == immutable_prefix_number)
                    .expect("edited immutable prefix segment must exist")
                    .file_name,
            )
        )
        .unwrap(),
        source_prefix_before,
        "sealed prefix segments are copied byte-for-byte"
    );
    let mut edited_loaded = load_session(&layout).unwrap();
    assert_eq!(
        edited_loaded.document.events().len(),
        target_seq.value() as usize
    );
    assert!(matches!(
        edited_loaded.document.events().last().map(|event| &event.event),
        Some(EventBody::UserInput(input)) if input.text == "edited-target"
    ));
    let (edited_tail, edited_tail_report) = load_session_tail_only(&layout).unwrap();
    assert_eq!(edited_tail_report.opened_segment_files, 1);
    assert_eq!(
        edited_tail
            .document
            .active_turn()
            .map(|turn| turn.start_seq),
        Some(target_seq)
    );
    let append_start_seq = edited_loaded
        .document
        .events()
        .last()
        .unwrap()
        .seq()
        .value();
    edited_loaded
        .document
        .append_display_note(
            None,
            DisplayNoticeKind::SystemError,
            NoticeSeverity::Info,
            "post-edit-append",
            None,
            None,
            None,
            None,
            20_002,
        )
        .unwrap();
    flush_large_session_tail(&layout, &mut edited_loaded.document, 20_002);
    let (appended_tail, appended_tail_report) = load_session_tail_only(&layout).unwrap();
    assert_eq!(
        appended_tail
            .document
            .events()
            .last()
            .unwrap()
            .seq()
            .value(),
        append_start_seq + 1
    );
    assert_eq!(appended_tail_report.opened_segment_files, 1);
    drop(root);
}

/// Tail-only restart reloads an open Turn that crosses a sealed-segment
/// boundary instead of forgetting the Turn when the new tail is empty.
/// tail-only 重启会恢复跨封口分片边界的开放 Turn，而不会因新尾段为空而忘记该回合。
#[test]
fn tail_only_load_restores_open_turn_across_rollover() {
    let (root, layout) = fresh_layout("open-turn-rollover");
    let mut document = SessionDocument::new(&layout.session_id, "project-1", "title", 1);
    document.metadata = publish_initial(&layout, &document);
    let turn_id = submit_test_root(
        &mut document,
        "workspace-store-rollover-open-root",
        "root",
        2,
    );
    let first_api = prepare_api(
        &mut document,
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        3,
    );
    document
        .append_assistant_text(first_api, "part one", AssistantSemanticStatus::Completed, 4)
        .unwrap();
    complete_api(&mut document, first_api, 5, None);
    flush_with_limits(
        &layout,
        &mut document,
        &FlushLimits {
            max_lines: 4,
            max_bytes: 8 * 1024 * 1024,
        },
        5,
    );
    assert_eq!(document.metadata.split_list.len(), 2);
    let second_api = prepare_api(
        &mut document,
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        6,
    );
    document
        .append_assistant_text(
            second_api,
            "part two",
            AssistantSemanticStatus::Completed,
            7,
        )
        .unwrap();
    complete_api(&mut document, second_api, 8, None);
    flush_with_limits(
        &layout,
        &mut document,
        &FlushLimits {
            max_lines: 100,
            max_bytes: 8 * 1024 * 1024,
        },
        8,
    );
    let (loaded, report) = load_session_tail_only(&layout).unwrap();
    assert_eq!(
        loaded.document.active_turn().map(|turn| turn.turn_id),
        Some(turn_id)
    );
    assert!(report.opened_segment_files >= 2);
    assert_eq!(
        loaded.document.active_turn().map(|turn| turn.turn_id),
        Some(turn_id)
    );
    drop(root);
}

/// Goal timing and usage survive a rollover that leaves only the newest continuation in the tail.
/// 分片滚动使尾段只剩最新续跑回合后，目标计时与用量仍可恢复。
#[test]
fn goal_checkpoint_survives_tail_rollover_and_paused_restart() {
    use vulcan_session_core::v1::{config::GoalState, goal::goal_work_time};
    let (_root, layout) = fresh_layout("goal-clock-rollover");
    let mut document = SessionDocument::new(&layout.session_id, "project-1", "title", 1);
    document.metadata = publish_initial(&layout, &document);
    document
        .create_goal("long goal", None, "long goal", 1_000)
        .unwrap();
    let first_turn = document.active_turn().unwrap().turn_id;
    let first_api = prepare_api(
        &mut document,
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        2_000,
    );
    document
        .append_assistant_text(
            first_api,
            "step 1",
            AssistantSemanticStatus::Completed,
            3_000,
        )
        .unwrap();
    complete_api(&mut document, first_api, 4_000, None);
    document
        .complete_turn_and_select_successor(first_turn, 5_000)
        .unwrap();
    flush_with_limits(
        &layout,
        &mut document,
        &FlushLimits {
            max_lines: 4,
            max_bytes: 8 * 1024 * 1024,
        },
        5_000,
    );
    prepare_api(
        &mut document,
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        9_000,
    );
    flush_candidate(&mut document, &layout, 9_000);
    let expected_clock = goal_work_time(&document).unwrap();
    let expected_origin = document.metadata.mode.goal.origin_seq.unwrap();
    let (mut loaded, report) = load_session_tail_only(&layout).unwrap();
    assert!(report.opened_segment_files >= 2);
    assert!(
        loaded
            .document
            .events()
            .iter()
            .any(|event| event.seq == expected_origin)
    );
    assert_eq!(goal_work_time(&loaded.document), Some(expected_clock));
    loaded.document.project_persisted_interruptions().unwrap();
    assert_eq!(loaded.document.metadata.mode.goal.state, GoalState::Paused);
    flush_candidate(&mut loaded.document, &layout, 10_000);
    let (mut reopened, _) = load_session_tail_only(&layout).unwrap();
    assert_eq!(
        reopened.document.metadata.mode.goal.state,
        GoalState::Paused
    );
    assert_eq!(
        goal_work_time(&reopened.document).unwrap().elapsed_ms,
        expected_clock.elapsed_ms
    );
    reopened
        .document
        .resume_goal("long goal", None, "long goal", 1_000_000)
        .unwrap();
    assert_eq!(
        goal_work_time(&reopened.document).unwrap().elapsed_ms,
        expected_clock.elapsed_ms
    );
    assert_eq!(
        reopened.document.metadata.mode.goal.origin_seq,
        Some(expected_origin)
    );
}

/// A goal created inside a running tool retains its owning API facts across the next rollover.
/// 在运行工具中创建的目标经过后续分片滚动后仍保留所属 API 事实。
#[test]
fn goal_retained_prefix_includes_native_creation_api() {
    let (_root, layout) = fresh_layout("goal-native-prefix");
    let mut document = SessionDocument::new(&layout.session_id, "project-1", "title", 1);
    document.metadata = publish_initial(&layout, &document);
    let turn = submit_test_root(&mut document, "native-goal", "work", 100);
    let api = prepare_api(
        &mut document,
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        200,
    );
    flush_with_limits(
        &layout,
        &mut document,
        &FlushLimits {
            max_lines: 1,
            max_bytes: 8 * 1024 * 1024,
        },
        200,
    );
    let tool = document
        .prepare_tool(
            api,
            RequestStreamId::try_new("native-goal-stream").unwrap(),
            "create_goal",
            ToolPayload::try_new(serde_json::json!({"objective":"work"})).unwrap(),
            Some("native-goal-call".to_string()),
            300,
        )
        .unwrap();
    document.start_prepared_tool(api, tool, 400).unwrap();
    document.create_goal("work", None, "work", 1_000).unwrap();
    document
        .finish_tool(
            api,
            tool,
            ToolCompletion {
                output: Some(ToolPayload::try_new(serde_json::json!({"ok":true})).unwrap()),
                changeset: None,
                status: ToolCallStatus::Completed,
                finished_at_ms: 1_100,
                external_refs: Vec::new(),
                generated_resources: Vec::new(),
            },
        )
        .unwrap();
    complete_api(&mut document, api, 1_300, None);
    let final_api = prepare_api(
        &mut document,
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        1_350,
    );
    document
        .append_assistant_text(
            final_api,
            "work started",
            AssistantSemanticStatus::Completed,
            1_400,
        )
        .unwrap();
    complete_api(&mut document, final_api, 1_450, None);
    document
        .complete_turn_and_select_successor(turn, 1_500)
        .unwrap();
    flush_with_limits(
        &layout,
        &mut document,
        &FlushLimits {
            max_lines: 1,
            max_bytes: 8 * 1024 * 1024,
        },
        1_500,
    );
    let (loaded, _) = load_session_tail_only(&layout).unwrap();
    assert!(loaded.document.events().iter().any(
        |event| matches!(&event.event, EventBody::ApiStarted(started) if started.api_call_id == api)
    ));
    assert_eq!(
        vulcan_session_core::v1::goal::goal_work_time(&loaded.document)
            .unwrap()
            .elapsed_ms,
        500
    );
}

/// Per-phase duration of one completed Turn fixture.
/// 一条已完成回合夹具的分阶段耗时。
#[derive(Default)]
struct CompletedTurnTimings {
    /// Root-input admission duration.
    /// 根输入准入耗时。
    submit_root: std::time::Duration,
    /// Provider projection and API preparation duration.
    /// Provider 投影与 API 准备耗时。
    prepare_api: std::time::Duration,
    /// Assistant semantic append duration.
    /// assistant 语义追加耗时。
    append_assistant: std::time::Duration,
    /// Physical API completion duration.
    /// 物理 API 完成耗时。
    complete_api: std::time::Duration,
    /// Logical Turn closure duration.
    /// 逻辑回合闭合耗时。
    close_turn: std::time::Duration,
}

impl CompletedTurnTimings {
    /// Adds one completed Turn sample into the cumulative profile.
    /// 把一条已完成回合样本加入累计剖析结果。
    fn accumulate(&mut self, sample: Self) {
        self.submit_root += sample.submit_root;
        self.prepare_api += sample.prepare_api;
        self.append_assistant += sample.append_assistant;
        self.complete_api += sample.complete_api;
        self.close_turn += sample.close_turn;
    }
}

/// Appends one completed six-event Turn to a storage fixture and returns phase timings.
/// 向存储夹具追加一个已完成的六事件回合，并返回分阶段耗时。
fn append_completed_turn(
    document: &mut SessionDocument,
    index: u64,
    now_ms: i64,
) -> CompletedTurnTimings {
    append_completed_turn_with_preparer(document, index, now_ms, prepare_api)
}

/// Appends one completed Turn while excluding repeated Provider projection from storage timing.
/// 追加一个已完成回合，同时从存储计时中排除重复 Provider 投影。
fn append_completed_storage_scale_turn(
    document: &mut SessionDocument,
    index: u64,
    now_ms: i64,
) -> CompletedTurnTimings {
    append_completed_turn_with_preparer(document, index, now_ms, prepare_storage_scale_api)
}

/// Appends one completed Turn through the explicitly selected fixture API preparer.
/// 通过显式选择的夹具 API 准备器追加一个已完成回合。
fn append_completed_turn_with_preparer(
    document: &mut SessionDocument,
    index: u64,
    now_ms: i64,
    prepare: fn(&mut SessionDocument, ApiPurposeKind, ApiRequestSnapshot, i64) -> ApiCallId,
) -> CompletedTurnTimings {
    let submit_root_started = std::time::Instant::now();
    document
        .submit_root_input(
            test_submission(
                format!("workspace-store-completed-turn-{index}"),
                format!("prompt-{index}"),
            ),
            now_ms,
        )
        .unwrap();
    let submit_root = submit_root_started.elapsed();
    let prepare_api_started = std::time::Instant::now();
    let api = prepare(
        document,
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        now_ms,
    );
    let prepare_api = prepare_api_started.elapsed();
    let append_assistant_started = std::time::Instant::now();
    document
        .append_assistant_text(
            api,
            format!("answer-{index}"),
            AssistantSemanticStatus::Completed,
            now_ms,
        )
        .unwrap();
    let append_assistant = append_assistant_started.elapsed();
    let complete_api_started = std::time::Instant::now();
    complete_api(
        document,
        api,
        now_ms,
        Some(ApiEndReason::CompletedSuccessfully),
    );
    let complete_api = complete_api_started.elapsed();
    let close_turn_started = std::time::Instant::now();
    document
        .close_turn(TurnOutcome::Completed, 1, now_ms)
        .unwrap();
    CompletedTurnTimings {
        submit_root,
        prepare_api,
        append_assistant,
        complete_api,
        close_turn: close_turn_started.elapsed(),
    }
}

/// Flushes one document with explicit test rollover limits and applies the
/// published metadata back to the unique in-memory owner.
/// 使用显式测试滚段阈值 flush 文档，并把已发布 metadata 应用回唯一内存 owner。
fn flush_with_limits(
    layout: &SessionV1Layout,
    document: &mut SessionDocument,
    limits: &FlushLimits,
    now_ms: i64,
) {
    let candidate = document
        .prepare_flush_candidates()
        .unwrap()
        .expect("fixture mutation must produce one flush candidate");
    let dirty = candidate.dirty.iter().copied().collect::<BTreeSet<_>>();
    let tail_start = candidate
        .metadata
        .split_list
        .last()
        .map(|entry| entry.start_seq);
    let request = FlushRequest {
        dirty: &dirty,
        metadata: &candidate.metadata,
        runtime_lock: &candidate.runtime_lock,
        runtime_updated_at_ms: now_ms,
        compactions: &candidate.compactions,
        events: &candidate.tail_events,
        tail_start,
    };
    let outcome = flush_session_with_limits(layout, &request, limits).unwrap();
    if let Some(published) = outcome.published_metadata {
        document.metadata = published;
    }
    document.advance_durable(candidate.revision).unwrap();
}

/// Counts distinct Turn ids in stream order.
/// 按流顺序统计不同的回合 id。
fn distinct_turn_count(events: &[ConversationEvent]) -> usize {
    let mut ids = Vec::new();
    for turn_id in events.iter().filter_map(|event| event.turn_id) {
        if !ids.contains(&turn_id) {
            ids.push(turn_id);
        }
    }
    ids.len()
}
