//! Durable revisions, resident tails, immutable flush candidates, and patch regression tests.
//! 持久化修订、常驻尾段、不可变落盘候选及补丁回归测试。

use super::*;

/// Truncating a historical tail keeps the retained seqs contiguous.
/// 截断历史尾部后保留的 seq 仍然连续。
#[test]
fn truncate_after_keeps_contiguity() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api_a = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    doc.append_assistant_text(api_a, "a1", AssistantSemanticStatus::Completed, T0)
        .unwrap();
    complete_ok(&mut doc, api_a, None, T0 + 1).unwrap();
    doc.close_turn(TurnOutcome::Completed, 1, T0 + 1).unwrap();
    doc.submit_root_input_fixture("b", Vec::new(), T0 + 2)
        .unwrap();
    let kept = doc.events().len();
    doc.truncate_after(Seq::try_new(kept as u64).unwrap(), T0 + 10)
        .unwrap();
    for (expected, event) in (1u64..).zip(doc.events()) {
        assert_eq!(event.seq.value(), expected);
    }
}

/// Dirty files and revisions track the writer contract exactly.
/// dirty 文件与修订号精确跟踪写入器契约。
#[test]
fn revision_and_dirty_tracking() {
    let mut doc = new_document();
    let initial = doc.memory_revision();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    assert!(doc.memory_revision() > initial);
    assert!(doc.dirty_files().any(|file| file == DirtyFile::ActiveTail));
    let snapshot_revision = doc.memory_revision();
    let candidate = doc
        .prepare_flush_candidates()
        .unwrap()
        .expect("a dirty tail must yield a candidate");
    assert_eq!(candidate.revision, snapshot_revision);
    doc.advance_durable(candidate.revision).unwrap();
    assert_eq!(doc.durable_revision(), snapshot_revision);
    assert!(!doc.dirty_files().any(|file| file == DirtyFile::ActiveTail));
    doc.add_pending_in_turn_input_fixture("later", Vec::new(), T0 + 1)
        .unwrap();
    assert!(doc.dirty_files().any(|file| file == DirtyFile::Metadata));
    assert!(
        !doc.dirty_files().any(|file| file == DirtyFile::ActiveTail),
        "an input-state update alone must never mark the tail dirty"
    );
}

/// A document whose resident stream starts at the active tail's seq (tail-only
/// load) rebuilds and appends against that offset; a mutation that targets a
/// sealed (non-resident) api is rejected (audit P0#2).
/// 常驻流从活动尾段起始 seq 开始的文档(tail-only 加载)按该偏移重建与追加；
/// 针对封口(非驻留)API 的 mutation 被拒绝(审计 P0#2)。
#[test]
fn tail_only_document_operates_on_resident_offset() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    let _text_seq = doc
        .append_assistant_text(api, "x", AssistantSemanticStatus::Completed, T0 + 1)
        .unwrap();
    complete_ok(&mut doc, api, None, T0 + 2).unwrap();
    let sealed_end = doc.events().last().unwrap().seq().value();
    // Simulate a tail-only load: the catalog carries the sealed prefix, the
    // resident stream starts after it.
    // 模拟 tail-only 加载：catalog 携带封口前缀，常驻流从其之后开始。
    let tail_events: Vec<ConversationEvent> = doc
        .events()
        .iter()
        .filter(|event| event.seq().value() > sealed_end)
        .cloned()
        .collect();
    let sealed_next_event_local_id =
        crate::v1::event::next_event_local_id(&doc.events()[..sealed_end as usize]);
    let stream_next_event_local_id = crate::v1::event::next_event_local_id(doc.events());
    let catalog = vec![
        crate::v1::config::SegmentInfo {
            segment_number: 1,
            file_name: "s0000001.jsonl".to_string(),
            start_seq: Seq::try_new(1).unwrap(),
            end_seq: Some(Seq::try_new(sealed_end).unwrap()),
            event_count: sealed_end,
            byte_len: 64,
            next_event_local_id: sealed_next_event_local_id,
        },
        crate::v1::config::SegmentInfo {
            segment_number: 2,
            file_name: "s0000002.jsonl".to_string(),
            start_seq: Seq::try_new(sealed_end + 1).unwrap(),
            end_seq: None,
            event_count: tail_events.len() as u64,
            byte_len: 64,
            next_event_local_id: stream_next_event_local_id,
        },
    ];
    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        catalog,
        tail_events,
    )
    .unwrap();
    assert_eq!(
        restored.segment_catalog().len(),
        2,
        "the catalog keeps every sealed segment addressable"
    );
    assert!(
        restored.active_api().is_none(),
        "the sealed group is closed; the tail-only document still rebuilds"
    );
    let appended = restored
        .append_display_note(
            None,
            DisplayNoticeKind::SystemError,
            NoticeSeverity::Info,
            "z",
            None,
            None,
            None,
            None,
            T0 + 3,
        )
        .unwrap();
    assert_eq!(
        appended.value(),
        sealed_end + restored.events().len() as u64,
        "appends continue from the resident offset"
    );
    // Mutations targeting the sealed (non-resident) closed group are rejected.
    // 针对封口(非驻留)已闭合组的 mutation 被拒绝。
    assert!(matches!(
        restored.append_assistant_text(api, "late", AssistantSemanticStatus::Completed, T0 + 4,),
        Err(DomainError::CrossApiGroupUpdate { .. })
    ));
}

/// The flush candidate captures the memory revision inside the document and
/// copies only the active tail (plan 6.2.1 flush contract).
/// flush 候选在文档内部捕获 memory 修订号，且只复制活动尾段(计划 6.2.1
/// flush 契约)。
#[test]
fn flush_candidate_snapshot_copies_dirty_files() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let revision = doc.memory_revision();
    let candidate = doc
        .prepare_flush_candidates()
        .unwrap()
        .expect("a dirty tail must yield a candidate");
    assert!(candidate.dirty.contains(&DirtyFile::ActiveTail));
    assert_eq!(candidate.revision, revision);
    assert_eq!(candidate.tail_events.len(), doc.events().len());
    // The document keeps mutating after the snapshot; the candidate is owned
    // and unaffected, and only the tail participates.
    // 快照之后文档继续变更；候选是自有的、不受影响，且只有尾段参与。
    doc.append_display_note(
        None,
        DisplayNoticeKind::SystemError,
        NoticeSeverity::Info,
        "x",
        None,
        None,
        None,
        None,
        T0 + 1,
    )
    .unwrap();
    assert_eq!(candidate.tail_events.len() + 1, doc.events().len());
    assert!(doc.prepare_flush_candidates().unwrap().is_some());
    doc.advance_durable(candidate.revision).unwrap();
    // The display note dirtied the tail again after the snapshot, so the
    // first confirmation must NOT clear it; the next candidate captures the
    // new revision.
    // display note 在快照之后再次弄脏尾段，因此第一次确认不得清除它；下一个
    // 候选捕获新的修订号。
    assert!(
        doc.dirty_files().any(|file| file == DirtyFile::ActiveTail),
        "tail re-dirtied after the snapshot must survive the first confirmation"
    );
    let second = doc.prepare_flush_candidates().unwrap().unwrap();
    assert!(second.revision > candidate.revision);
    doc.advance_durable(second.revision).unwrap();
    assert_eq!(doc.durable_revision(), doc.memory_revision());
    assert!(doc.prepare_flush_candidates().unwrap().is_none());
}

/// A candidate issued before a later mutation cannot clear that later dirty
/// file, and consecutive flushes advance the durable revision accurately.
/// 在后续 mutation 之前发出的候选不能清除该后续 dirty 文件；连续两次 flush
/// 准确推进 durable 修订号。
#[test]
fn mutation_after_candidate_preserves_new_dirty() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let first = doc.prepare_flush_candidates().unwrap().unwrap();
    assert!(first.dirty.contains(&DirtyFile::ActiveTail));
    // An input-state update marks only metadata dirty at a higher revision.
    // 输入区更新只以更高修订号标记 metadata dirty。
    doc.add_pending_in_turn_input_fixture("mid", Vec::new(), T0 + 1)
        .unwrap();
    doc.advance_durable(first.revision).unwrap();
    assert_eq!(doc.durable_revision(), first.revision);
    assert!(
        !doc.dirty_files().any(|file| file == DirtyFile::ActiveTail),
        "the tail was covered by the first candidate"
    );
    assert!(
        doc.dirty_files().any(|file| file == DirtyFile::Metadata),
        "the metadata dirtied after the snapshot must stay dirty"
    );
    let second = doc.prepare_flush_candidates().unwrap().unwrap();
    assert_eq!(second.revision, doc.memory_revision());
    assert!(second.dirty.contains(&DirtyFile::Metadata));
    doc.advance_durable(second.revision).unwrap();
    assert_eq!(doc.durable_revision(), doc.memory_revision());
    assert!(
        doc.prepare_flush_candidates().unwrap().is_none(),
        "no dirty files remain after both candidates are confirmed"
    );
}

/// Future or forged revisions are rejected by `advance_durable`; only issued
/// candidate revisions confirm, and out-of-order confirmation is a no-op.
/// `advance_durable` 拒绝未来或被伪造的修订号；只有已发出的候选修订号可被
/// 确认，乱序确认是无操作。
#[test]
fn advance_durable_rejects_unissued_and_future_revisions() {
    let mut doc = new_document();
    assert!(
        matches!(
            doc.advance_durable(doc.memory_revision() + 100),
            Err(DomainError::RevisionNeverIssued { .. })
        ),
        "a future revision must never confirm"
    );
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let first = doc.prepare_flush_candidates().unwrap().unwrap();
    // Another mutation moves memory beyond the issued revision.
    // 另一次 mutation 使 memory 越过已发出的修订号。
    doc.add_pending_in_turn_input_fixture("mid", Vec::new(), T0 + 1)
        .unwrap();
    doc.advance_durable(first.revision).unwrap();
    let unissued = doc.memory_revision();
    assert!(
        unissued > first.revision,
        "fixture: memory moved beyond the issued revision"
    );
    assert!(
        matches!(
            doc.advance_durable(unissued),
            Err(DomainError::RevisionNeverIssued { .. })
        ),
        "an unissued revision between memory and durable must be rejected"
    );
    // A second candidate then confirms; the earlier confirmation stays valid.
    // 随后第二个候选被确认；此前的确认仍然有效。
    let second = doc.prepare_flush_candidates().unwrap().unwrap();
    let out_of_order = first.revision;
    assert!(second.revision > out_of_order);
    assert!(
        doc.advance_durable(out_of_order).is_ok(),
        "confirming an already-covered revision is a no-op, not an error"
    );
    doc.advance_durable(second.revision).unwrap();
    assert_eq!(doc.durable_revision(), doc.memory_revision());
    assert!(doc.dirty_files().next().is_none());
}

/// The event candidate copies only the active tail, never the sealed prefix
/// (plan 5.2.15: sealed segments are immutable and never rewritten).
/// 事件候选只复制活动尾段，绝不复制封口前缀(计划 5.2.15：封口分片不可变、
/// 永不重写)。
#[test]
fn event_candidate_copies_only_active_tail() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    doc.append_reasoning(api, "r", AssistantSemanticStatus::Completed, T0 + 1)
        .unwrap();
    let text_seq = doc
        .append_assistant_text(api, "t", AssistantSemanticStatus::Completed, T0 + 2)
        .unwrap()
        .into_value();
    complete_ok(&mut doc, api, None, T0 + 3).unwrap();
    let first_tail_seq = text_seq.value() - 1;
    // Fabricate a split map where everything before the last two events is a
    // sealed segment; the doc-level split map is storage metadata and its
    // shape is authoritative for the flush slice.
    // 构造一个分片映射：最后两条事件之前的全部内容属于封口分片；文档级的
    // 分片映射是存储元数据，其形状对 flush 切片权威。
    let sealed_next_event_local_id =
        crate::v1::event::next_event_local_id(&doc.events()[..first_tail_seq as usize]);
    let stream_next_event_local_id = crate::v1::event::next_event_local_id(doc.events());
    doc.metadata.split_list = vec![
        crate::v1::config::SegmentInfo {
            segment_number: 1,
            file_name: "000001.jsonl".to_string(),
            start_seq: crate::v1::ids::Seq::try_new(1).unwrap(),
            end_seq: Some(crate::v1::ids::Seq::try_new(first_tail_seq).unwrap()),
            event_count: first_tail_seq,
            byte_len: 64,
            next_event_local_id: sealed_next_event_local_id,
        },
        crate::v1::config::SegmentInfo {
            segment_number: 2,
            file_name: "000002.jsonl".to_string(),
            start_seq: crate::v1::ids::Seq::try_new(first_tail_seq + 1).unwrap(),
            end_seq: None,
            event_count: 2,
            byte_len: 64,
            next_event_local_id: stream_next_event_local_id,
        },
    ];
    doc.append_display_note(
        None,
        DisplayNoticeKind::SystemError,
        NoticeSeverity::Info,
        "y",
        None,
        None,
        None,
        None,
        T0 + 4,
    )
    .unwrap();
    let candidate = doc.prepare_flush_candidates().unwrap().unwrap();
    assert_eq!(
        candidate.tail_events.len(),
        doc.events().len() - first_tail_seq as usize,
        "tail starts after the sealed prefix"
    );
    assert_eq!(candidate.tail_events[0].seq().value(), first_tail_seq + 1);
    assert!(
        candidate
            .tail_events
            .iter()
            .all(|event| event.seq().value() > first_tail_seq)
    );
    doc.advance_durable(candidate.revision).unwrap();
    assert_eq!(doc.durable_revision(), doc.memory_revision());
}

/// Logical live patches report only the exact seq values changed by each revision.
/// 逻辑实时补丁只报告每个修订真实变化的 seq。
#[test]
fn revision_receipts_separate_logical_patches_from_full_tail_flushes() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("hello", Vec::new(), T0 + 1)?;
    let root_revision = doc.memory_revision();
    let root_receipt = doc.memory_revision_receipt(root_revision)?;
    assert_eq!(root_receipt.changed_event_seqs, vec![Seq::try_new(1)?]);
    assert_eq!(root_receipt.changed_events.len(), 1);
    let root_candidate = doc
        .prepare_flush_candidates()?
        .expect("root event must produce a flush candidate");
    assert_eq!(root_candidate.changed_event_seqs, vec![Seq::try_new(1)?]);
    assert_eq!(root_candidate.tail_events.len(), 1);
    doc.advance_durable(root_candidate.revision)?;

    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 2,
    )?;
    let started_candidate = doc
        .prepare_flush_candidates()?
        .expect("api start must produce a flush candidate");
    doc.advance_durable(started_candidate.revision)?;
    let reasoning_seq = doc
        .append_reasoning(api, "partial", AssistantSemanticStatus::Streaming, T0 + 3)?
        .into_value();
    let first_candidate = doc
        .prepare_flush_candidates()?
        .expect("reasoning append must produce a flush candidate");
    doc.advance_durable(first_candidate.revision)?;
    doc.append_reasoning(
        api,
        "partial complete",
        AssistantSemanticStatus::Completed,
        T0 + 4,
    )?;
    let update_receipt = doc.memory_revision_receipt(doc.memory_revision())?;
    assert_eq!(update_receipt.changed_event_seqs, vec![reasoning_seq]);
    assert_eq!(update_receipt.changed_events[0].event_revision, 2);
    let update_candidate = doc
        .prepare_flush_candidates()?
        .expect("reasoning update must produce a flush candidate");
    assert_eq!(update_candidate.changed_event_seqs, vec![reasoning_seq]);
    assert!(update_candidate.tail_events.len() > update_candidate.changed_event_seqs.len());
    Ok(())
}
