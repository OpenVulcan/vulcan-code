//! V1 flush writer: create, whole-tail rewrite, rollover, and dependency-ordered
//! multi-file publication.
//! V1 flush 写入器：创建、尾段整段重写、滚段与按依赖顺序的多文件发布。
//!
//! A normal flush rewrites at most one active tail segment. Runtime and
//! compactions are published together into a new immutable context generation;
//! metadata is the only commit point for both context and segment changes.
//! Rollover follows the data-first rule: the full current semantics are
//! written to the old tail before an empty new tail is created and the
//! metadata pointer switches.
//! 普通 flush 最多重写一个活动尾段。runtime 与 compactions 共同发布到新的
//! 不可变 context generation；metadata 是上下文和分片变化的唯一提交点。
//! 滚段遵循数据优先规则：当前完整语义先写入旧尾段，再创建空新尾段并切换
//! metadata 指针。

use std::collections::BTreeSet;
use std::fs;

use uuid::Uuid;
use vulcan_session_core::v1::config::{MetadataDocument, SegmentInfo};
use vulcan_session_core::v1::context::{
    CompactionNode, CompactionsDocument, RuntimeDocument, RuntimeLock,
};
use vulcan_session_core::v1::document::{DirtyFile, FlushCandidate};
use vulcan_session_core::v1::event::{
    AssistantSemanticStatus, ConversationEvent, EventBody, ToolCallStatus,
};
use vulcan_session_core::v1::ids::Seq;

use super::atomic_io::write_file_atomic;
use super::documents::{read_compactions, read_runtime, write_json_document};
use super::errors::{ErrorCode, SessionV1Error};
use super::layout::SessionV1Layout;
use super::loader::load_session;
use super::segment::serialize_segment;

/// Maximum event count of one segment before rollover is considered.
/// 一个分片在考虑滚段前的最大事件数。
pub const SEGMENT_MAX_LINES: u64 = 3000;

/// Target byte ceiling of one segment before rollover is considered.
/// 一个分片在考虑滚段前的目标字节上限。
pub const SEGMENT_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// Rollover thresholds used by one flush; production uses the defaults.
/// 一次 flush 使用的滚段阈值；生产使用默认值。
#[derive(Debug, Clone, Copy)]
pub struct FlushLimits {
    /// Maximum event count before rollover is considered.
    /// 考虑滚段前的最大事件数。
    pub max_lines: u64,
    /// Target byte ceiling before rollover is considered.
    /// 考虑滚段前的目标字节上限。
    pub max_bytes: u64,
}

impl Default for FlushLimits {
    /// Production defaults: 3000 events or 8 MiB.
    /// 生产默认值：3000 行或 8 MiB。
    fn default() -> Self {
        Self {
            max_lines: SEGMENT_MAX_LINES,
            max_bytes: SEGMENT_MAX_BYTES,
        }
    }
}

/// One file written by a flush.
/// 一次 flush 写出的一个文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrittenFile {
    /// `metadata.json`.
    /// `metadata.json`。
    Metadata,
    /// `runtime.json`.
    /// `runtime.json`。
    Runtime,
    /// `compactions.json`.
    /// `compactions.json`。
    Compactions,
    /// One tail segment file (sealed or still active).
    /// 一个尾段分片文件(已封口或仍活动)。
    TailSegment {
        /// Segment number of the written file.
        /// 被写文件的尾段序号。
        segment_number: u64,
        /// Whether the write sealed this segment.
        /// 该次写入是否封口了此分片。
        sealed: bool,
    },
}

/// Result of one flush request.
/// 一次 flush 请求的结果。
#[derive(Debug, Clone, Default)]
pub struct FlushOutcome {
    /// Files actually written in dependency order.
    /// 实际按依赖顺序写出的文件。
    pub written_files: Vec<WrittenFile>,
    /// Metadata exactly published by this flush, including context and/or
    /// rollover pointer changes. `None` means metadata was not written.
    /// 本次 flush 精确发布的 metadata，包含 context 和/或滚段指针变化；
    /// `None` 表示本次未写 metadata。
    pub published_metadata: Option<MetadataDocument>,
}

/// One flush request snapshot taken inside the session state lock.
/// 在会话状态锁内取得的一次 flush 请求快照。
#[derive(Debug)]
pub struct FlushRequest<'a> {
    /// Files that must be re-published.
    /// 必须重新发布的文件。
    pub dirty: &'a BTreeSet<DirtyFile>,
    /// Current metadata candidate (split map authoritative for sealed files).
    /// 当前 metadata 候选(分片映射对封口文件权威)。
    pub metadata: &'a MetadataDocument,
    /// Current runtime lock state.
    /// 当前运行时锁定状态。
    pub runtime_lock: &'a RuntimeLock,
    /// Runtime document write time.
    /// runtime 文档写入时间。
    pub runtime_updated_at_ms: i64,
    /// Current compaction nodes.
    /// 当前压缩节点。
    pub compactions: &'a [CompactionNode],
    /// The active-tail event stream.
    ///
    /// When `tail_start` is `None` this is the full authoritative stream and
    /// the writer slices the active tail itself using the split map (legacy
    /// contract). When `tail_start` is `Some(start)`, this slice already is
    /// the active tail beginning at absolute seq `start`, and the writer
    /// verifies that against the split map before writing it (plan 6.2.1:
    /// flush candidates carry only the active tail, never the sealed history).
    /// 活动尾段事件流。
    ///
    /// `tail_start` 为 `None` 时这是完整权威流，写入器按分片映射自行切出
    /// 活动尾段(旧契约)。`tail_start` 为 `Some(start)` 时这个切片本身就是
    /// 从绝对 seq `start` 开始的活动尾段，写入器先与分片映射核对再写入
    /// (计划 6.2.1:flush 候选只携带活动尾段，绝不携带封口历史)。
    pub events: &'a [ConversationEvent],
    /// Absolute seq of the first event when `events` is already tail-only.
    /// `events` 已是仅尾段切片时的绝对起始 seq。
    pub tail_start: Option<Seq>,
}

/// Persists one complete document-owned candidate through the V1 writer.
/// 通过 V1 写入器持久化一份由文档完整拥有的候选。
///
/// This is the only production adapter from `SessionDocument` to physical
/// storage. Service callers cannot assemble a partial flush request.
/// 这是从 `SessionDocument` 到物理存储的唯一生产适配器；服务调用方不能
/// 拼装局部 flush 请求。
pub fn flush_candidate(
    layout: &SessionV1Layout,
    candidate: &FlushCandidate,
) -> Result<FlushOutcome, SessionV1Error> {
    let dirty = candidate.dirty.iter().copied().collect::<BTreeSet<_>>();
    let tail_start = candidate
        .metadata
        .split_list
        .last()
        .map(|entry| entry.start_seq);
    let runtime_updated_at_ms = candidate
        .tail_events
        .last()
        .map(|event| event.updated_at_ms)
        .unwrap_or(candidate.metadata.updated_at_ms);
    flush_session(
        layout,
        &FlushRequest {
            dirty: &dirty,
            metadata: &candidate.metadata,
            runtime_lock: &candidate.runtime_lock,
            runtime_updated_at_ms,
            compactions: &candidate.compactions,
            events: &candidate.tail_events,
            tail_start,
        },
    )
}

/// Creates the initial file set of one V1 session.
/// 创建 V1 会话的初始文件集。
///
/// Returns the metadata candidate with the filled split map so the caller can
/// persist it in memory and later register the SQLite row.
/// 返回带已填充分片映射的 metadata 候选，供调用方持久化到内存并稍后注册
/// SQLite row。
pub fn create_session(
    layout: &SessionV1Layout,
    metadata: &MetadataDocument,
    runtime_lock: &RuntimeLock,
    runtime_updated_at_ms: i64,
    compactions: &[CompactionNode],
    events: &[ConversationEvent],
) -> Result<MetadataDocument, SessionV1Error> {
    let final_dir = layout.session_dir();
    if final_dir.exists() {
        return Err(SessionV1Error::unsupported(
            &final_dir,
            "session directory already exists",
        ));
    }
    let staging_layout = create_staging_layout(layout)?;
    let staging_dir = staging_layout.session_dir();
    let creation = write_initial_session_tree(
        &staging_layout,
        metadata,
        runtime_lock,
        runtime_updated_at_ms,
        compactions,
        events,
    )
    .and_then(|filled| {
        verify_initial_session_tree(
            &staging_layout,
            &filled,
            runtime_lock,
            runtime_updated_at_ms,
            compactions,
            events,
        )?;
        Ok(filled)
    });
    let filled = match creation {
        Ok(filled) => filled,
        Err(error) => {
            cleanup_creation_staging(&staging_dir);
            return Err(error);
        }
    };
    if final_dir.exists() {
        cleanup_creation_staging(&staging_dir);
        return Err(SessionV1Error::unsupported(
            &final_dir,
            "session directory appeared before atomic publication",
        ));
    }
    #[cfg(test)]
    if super::atomic_io::test_hooks::should_fail(
        super::atomic_io::WriteStage::Rename,
        "session directory publication",
    ) {
        cleanup_creation_staging(&staging_dir);
        return Err(SessionV1Error::atomic_write(
            &final_dir,
            "rename",
            "injected session directory publication failure",
        ));
    }
    if let Err(error) = fs::rename(&staging_dir, &final_dir) {
        cleanup_creation_staging(&staging_dir);
        return Err(SessionV1Error::io(
            &final_dir,
            format!("atomic session directory publication failed: {error}"),
        ));
    }
    Ok(filled)
}

/// Writes one complete initial session tree below an unpublished staging layout.
/// 在尚未发布的 staging 布局下写入一棵完整初始会话树。
fn write_initial_session_tree(
    layout: &SessionV1Layout,
    metadata: &MetadataDocument,
    runtime_lock: &RuntimeLock,
    runtime_updated_at_ms: i64,
    compactions: &[CompactionNode],
    events: &[ConversationEvent],
) -> Result<MetadataDocument, SessionV1Error> {
    let generation = metadata.segment_generation;
    let tail_bytes = serialize_segment(events)?;
    let tail_path = layout.segment_path(generation, 1);
    write_file_atomic(&tail_path, &tail_bytes, "initial tail segment")?;
    let filled = build_initial_metadata(metadata, events, tail_bytes.len());
    let runtime_document = RuntimeDocument {
        initialization_state: runtime_lock.initialization_state,
        current_compaction_id: runtime_lock.current_compaction_id,
        locked_sources: runtime_lock.locked_sources.clone(),
        updated_at_ms: runtime_updated_at_ms,
        ..RuntimeDocument::new(layout.session_id.clone(), runtime_updated_at_ms)
    };
    write_context_generation(
        layout,
        filled.active_context_generation,
        &CompactionsDocument::new_with_nodes(layout.session_id.clone(), compactions.to_vec()),
        &runtime_document,
        "initial context",
    )?;
    write_json_document(&layout.metadata_path(), &filled, "metadata")?;
    Ok(filled)
}

/// Verifies that an already published directory exactly matches one explicit create request.
/// 校验一个已发布目录与一条显式创建请求完全一致。
pub(crate) fn verify_existing_session_creation(
    layout: &SessionV1Layout,
    metadata: &MetadataDocument,
    runtime_lock: &RuntimeLock,
    runtime_updated_at_ms: i64,
    compactions: &[CompactionNode],
    events: &[ConversationEvent],
) -> Result<MetadataDocument, SessionV1Error> {
    let tail_bytes = serialize_segment(events)?;
    let expected_metadata = build_initial_metadata(metadata, events, tail_bytes.len());
    verify_initial_session_tree(
        layout,
        &expected_metadata,
        runtime_lock,
        runtime_updated_at_ms,
        compactions,
        events,
    )?;
    Ok(expected_metadata)
}

/// Builds the exact metadata document committed by one initial creation.
/// 构建一次初始创建所提交的精确 metadata 文档。
fn build_initial_metadata(
    metadata: &MetadataDocument,
    events: &[ConversationEvent],
    tail_byte_len: usize,
) -> MetadataDocument {
    let mut filled = metadata.clone();
    filled.split_list = vec![SegmentInfo {
        segment_number: 1,
        file_name: SessionV1Layout::segment_file_name(1),
        start_seq: Seq::try_new(1).expect("first seq is always valid"),
        end_seq: None,
        event_count: events.len() as u64,
        byte_len: tail_byte_len as u64,
        next_event_local_id: vulcan_session_core::v1::event::next_event_local_id(events),
    }];
    filled
}

/// Name prefix of an unpublished creation staging directory beside the bucket it publishes into.
/// A crash between staging and publication can leave one behind; it is never a session.
/// 未发布创建暂存目录的名称前缀，位于其发布目标桶旁。暂存与发布之间崩溃可能遗留此目录；它绝不是会话。
pub const CREATION_STAGING_PREFIX: &str = ".create-";

/// Creates one exact sibling staging layout without inspecting session siblings.
/// 在不检查会话同级目录的前提下创建一个精确同级 staging 布局。
fn create_staging_layout(layout: &SessionV1Layout) -> Result<SessionV1Layout, SessionV1Error> {
    let project_dir = layout.root.join(&layout.project_partition);
    fs::create_dir_all(&project_dir).map_err(|error| {
        SessionV1Error::io(
            &project_dir,
            format!("create session project directory failed: {error}"),
        )
    })?;
    let staging_layout = SessionV1Layout::new(
        &layout.root,
        &layout.project_partition,
        format!(
            "{CREATION_STAGING_PREFIX}{}-{}",
            layout.date_bucket,
            Uuid::new_v4()
        ),
        &layout.session_id,
    );
    let staging_dir = staging_layout.session_dir();
    fs::create_dir(&staging_dir).map_err(|error| {
        SessionV1Error::io(
            &staging_dir,
            format!("reserve session creation staging directory failed: {error}"),
        )
    })?;
    Ok(staging_layout)
}

/// Verifies the complete staging tree through the same strict loader used after restart.
/// 通过重启后使用的同一严格加载器校验完整 staging 树。
fn verify_initial_session_tree(
    layout: &SessionV1Layout,
    expected_metadata: &MetadataDocument,
    expected_runtime_lock: &RuntimeLock,
    expected_runtime_updated_at_ms: i64,
    expected_compactions: &[CompactionNode],
    expected_events: &[ConversationEvent],
) -> Result<(), SessionV1Error> {
    let loaded = load_session(layout)?;
    let runtime = read_runtime(&layout.runtime_path(expected_metadata.active_context_generation))?;
    if loaded.metadata != *expected_metadata
        || loaded.document.runtime_lock != *expected_runtime_lock
        || runtime.updated_at_ms != expected_runtime_updated_at_ms
        || loaded.document.compactions != expected_compactions
        || loaded.document.events() != expected_events
    {
        return Err(SessionV1Error::corrupt(
            ErrorCode::MetadataCorrupt,
            layout.session_dir(),
            "staged session tree differs from the explicit creation request",
        ));
    }
    Ok(())
}

/// Removes only the exact unpublished staging directory owned by this creation attempt.
/// 仅移除本次创建尝试拥有的精确未发布 staging 目录。
fn cleanup_creation_staging(staging_dir: &std::path::Path) {
    if let Err(error) = fs::remove_dir_all(staging_dir)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!(
            "[session-v1] failed to remove creation staging directory {}: {error}",
            staging_dir.display()
        );
    }
}

/// Flushes one session snapshot in dependency order.
/// 按依赖顺序 flush 一个会话快照。
pub fn flush_session(
    layout: &SessionV1Layout,
    request: &FlushRequest<'_>,
) -> Result<FlushOutcome, SessionV1Error> {
    flush_session_with_limits(layout, request, &FlushLimits::default())
}

/// Flushes one session snapshot with explicit rollover thresholds.
/// 使用显式滚段阈值 flush 一个会话快照。
pub fn flush_session_with_limits(
    layout: &SessionV1Layout,
    request: &FlushRequest<'_>,
    limits: &FlushLimits,
) -> Result<FlushOutcome, SessionV1Error> {
    ensure_request_generations_are_current(layout, request.metadata)?;
    let mut outcome = FlushOutcome::default();
    let mut metadata_candidate = request.metadata.clone();
    let mut obsolete_segment_paths = Vec::new();
    let context_dirty = request.dirty.contains(&DirtyFile::Compactions)
        || request.dirty.contains(&DirtyFile::Runtime);
    let mut metadata_required = request.dirty.contains(&DirtyFile::Metadata);
    if context_dirty {
        let context_generation = request
            .metadata
            .active_context_generation
            .checked_add(1)
            .ok_or_else(|| {
                SessionV1Error::corrupt(
                    ErrorCode::MetadataCorrupt,
                    layout.metadata_path(),
                    "active_context_generation overflow",
                )
            })?;
        cleanup_context_candidate(layout, request.metadata.active_context_generation)?;
        let compactions_document = CompactionsDocument::new_with_nodes(
            layout.session_id.clone(),
            request.compactions.to_vec(),
        );
        let runtime_document = RuntimeDocument {
            initialization_state: request.runtime_lock.initialization_state,
            current_compaction_id: request.runtime_lock.current_compaction_id,
            locked_sources: request.runtime_lock.locked_sources.clone(),
            updated_at_ms: request.runtime_updated_at_ms,
            ..RuntimeDocument::new(layout.session_id.clone(), request.runtime_updated_at_ms)
        };
        write_context_generation(
            layout,
            context_generation,
            &compactions_document,
            &runtime_document,
            "flush context candidate",
        )?;
        metadata_candidate.active_context_generation = context_generation;
        metadata_required = true;
        outcome.written_files.push(WrittenFile::Compactions);
        outcome.written_files.push(WrittenFile::Runtime);
    }
    if request.dirty.contains(&DirtyFile::ActiveTail) {
        let (tail_start, tail_entry) = active_tail_entry(request.metadata)?;
        let start_index = (tail_start - 1) as usize;
        let tail_events = match request.tail_start {
            None => {
                if request.events.len() < start_index {
                    return Err(SessionV1Error::discontinuity(
                        layout.session_dir(),
                        "active tail",
                        tail_start,
                        request.events.len() as u64 + 1,
                    ));
                }
                &request.events[start_index..]
            }
            Some(provided) => {
                if provided.value() != tail_start {
                    return Err(SessionV1Error::discontinuity(
                        layout.session_dir(),
                        "active tail start",
                        tail_start,
                        provided.value(),
                    ));
                }
                request.events
            }
        };
        let tail_bytes = serialize_segment(tail_events)?;
        let published_tail_path =
            layout.segment_entry_path(request.metadata.segment_generation, &tail_entry.file_name);
        let candidate_file_name = SessionV1Layout::alternate_segment_file_name(
            tail_entry.segment_number,
            &tail_entry.file_name,
        )
        .ok_or_else(|| {
            SessionV1Error::corrupt(
                ErrorCode::MetadataCorrupt,
                layout.metadata_path(),
                format!(
                    "active tail file {} is not a valid bank for segment {}",
                    tail_entry.file_name, tail_entry.segment_number
                ),
            )
        })?;
        let candidate_tail_path =
            layout.segment_entry_path(request.metadata.segment_generation, &candidate_file_name);
        let rollover =
            should_rollover_with_limits(tail_events.len() as u64, tail_bytes.len(), limits)
                && !tail_events.is_empty()
                && !tail_has_open_entities(tail_events);
        let prefix_next_event_local_id = request
            .metadata
            .split_list
            .iter()
            .rev()
            .nth(1)
            .map(|entry| entry.next_event_local_id)
            .unwrap_or(1);
        let tail_next_event_local_id = prefix_next_event_local_id.max(
            vulcan_session_core::v1::event::next_event_local_id(tail_events),
        );
        if rollover {
            // The complete sealed segment lands in the unpublished opposite bank.
            // 完整封口分片先写入尚未发布的对侧 bank。
            write_file_atomic(&candidate_tail_path, &tail_bytes, "sealed tail candidate")?;
            outcome.written_files.push(WrittenFile::TailSegment {
                segment_number: tail_entry.segment_number,
                sealed: true,
            });
            // An empty new tail then becomes the active segment.
            // 空的新尾段随后成为活动段。
            let new_number = tail_entry.segment_number + 1;
            let new_file_name = SessionV1Layout::segment_file_name(new_number);
            let new_path =
                layout.segment_entry_path(request.metadata.segment_generation, &new_file_name);
            write_file_atomic(&new_path, &[], "new empty tail candidate")?;
            if let Some(last) = metadata_candidate.split_list.last_mut() {
                last.file_name = candidate_file_name;
                last.end_seq = Some(
                    Seq::try_new(tail_start + tail_events.len() as u64 - 1).map_err(|error| {
                        SessionV1Error::corrupt(
                            ErrorCode::MetadataCorrupt,
                            layout.metadata_path(),
                            format!("bad tail end seq: {error}"),
                        )
                    })?,
                );
                last.event_count = tail_events.len() as u64;
                last.byte_len = tail_bytes.len() as u64;
                last.next_event_local_id = tail_next_event_local_id;
            }
            metadata_candidate.split_list.push(SegmentInfo {
                segment_number: new_number,
                file_name: new_file_name,
                start_seq: Seq::try_new(tail_start + tail_events.len() as u64).map_err(
                    |error| {
                        SessionV1Error::corrupt(
                            ErrorCode::MetadataCorrupt,
                            layout.metadata_path(),
                            format!("bad new tail start: {error}"),
                        )
                    },
                )?,
                end_seq: None,
                event_count: 0,
                byte_len: 0,
                next_event_local_id: tail_next_event_local_id,
            });
            outcome.written_files.push(WrittenFile::TailSegment {
                segment_number: new_number,
                sealed: false,
            });
            metadata_required = true;
        } else {
            write_file_atomic(&candidate_tail_path, &tail_bytes, "active tail candidate")?;
            outcome.written_files.push(WrittenFile::TailSegment {
                segment_number: tail_entry.segment_number,
                sealed: false,
            });
            // Every tail mutation switches the exact file pointer and cache in
            // metadata; the candidate is never authoritative before that switch.
            // 每次尾段 mutation 都通过 metadata 切换精确文件指针与缓存；在该
            // 切换前候选文件绝不具备权威性。
            if let Some(last) = metadata_candidate.split_list.last_mut() {
                last.file_name = candidate_file_name;
                last.event_count = tail_events.len() as u64;
                last.byte_len = tail_bytes.len() as u64;
                last.next_event_local_id = tail_next_event_local_id;
            }
            metadata_required = true;
        }
        obsolete_segment_paths.push(published_tail_path);
    }
    if metadata_required {
        write_json_document(&layout.metadata_path(), &metadata_candidate, "metadata")?;
        outcome.written_files.push(WrittenFile::Metadata);
        outcome.published_metadata = Some(metadata_candidate);
        // Old banks became non-authoritative only after the metadata switch.
        // metadata 切换后旧 bank 才失去权威性。
        for path in obsolete_segment_paths {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => {
                    // Exact cleanup is intentionally best-effort: metadata is
                    // already the reliable commit point and readers never scan.
                    // 精确清理刻意采用尽力而为：metadata 已是可靠提交点，读取器
                    // 也从不扫描目录。
                }
            }
        }
    }
    Ok(outcome)
}

/// Verifies that candidate cleanup can never target an already-published tree.
/// 校验候选清理绝不会命中已经发布的权威树。
pub(super) fn ensure_request_generations_are_current(
    layout: &SessionV1Layout,
    request_metadata: &MetadataDocument,
) -> Result<(), SessionV1Error> {
    let published = super::documents::read_metadata(&layout.metadata_path())?;
    if published.segment_generation != request_metadata.segment_generation
        || published.active_context_generation != request_metadata.active_context_generation
    {
        return Err(SessionV1Error::corrupt(
            ErrorCode::ReferenceViolation,
            layout.metadata_path(),
            format!(
                "flush request generations event/context {}/{} do not match published {}/{}",
                request_metadata.segment_generation,
                request_metadata.active_context_generation,
                published.segment_generation,
                published.active_context_generation
            ),
        ));
    }
    Ok(())
}

/// Publishes and reload-validates one immutable context generation.
/// 发布并重新加载校验一个不可变上下文 generation。
pub(super) fn write_context_generation(
    layout: &SessionV1Layout,
    generation: u64,
    compactions: &CompactionsDocument,
    runtime: &RuntimeDocument,
    artifact_kind: &str,
) -> Result<(), SessionV1Error> {
    let terminal_compaction_id = compactions.nodes.last().map(|node| node.compaction_id);
    if runtime.current_compaction_id != terminal_compaction_id {
        return Err(SessionV1Error::corrupt(
            ErrorCode::RuntimeCorrupt,
            layout.runtime_path(generation),
            format!(
                "runtime pointer {:?} does not equal terminal compaction {:?}",
                runtime.current_compaction_id.map(|id| id.value()),
                terminal_compaction_id.map(|id| id.value())
            ),
        ));
    }
    if let Some(pointer) = runtime.current_compaction_id
        && !compactions
            .nodes
            .iter()
            .any(|node| node.compaction_id == pointer)
    {
        return Err(SessionV1Error::corrupt(
            ErrorCode::RuntimeCorrupt,
            layout.runtime_path(generation),
            format!(
                "current_compaction_id {} references no candidate compaction node",
                pointer.value()
            ),
        ));
    }
    let compactions_path = layout.compactions_path(generation);
    let runtime_path = layout.runtime_path(generation);
    write_json_document(
        &compactions_path,
        compactions,
        &format!("{artifact_kind} compactions"),
    )?;
    write_json_document(&runtime_path, runtime, &format!("{artifact_kind} runtime"))?;
    let reloaded_compactions = read_compactions(&compactions_path)?;
    let reloaded_runtime = read_runtime(&runtime_path)?;
    if reloaded_compactions != *compactions || reloaded_runtime != *runtime {
        return Err(SessionV1Error::corrupt(
            ErrorCode::RuntimeCorrupt,
            layout.context_generation_dir(generation),
            "context candidate changed during write/read validation",
        ));
    }
    Ok(())
}

/// Removes only the next unpublished context candidate before rebuilding it.
/// 仅在重建前移除下一个未发布的 context 候选。
pub(super) fn cleanup_context_candidate(
    layout: &SessionV1Layout,
    current_generation: u64,
) -> Result<(), SessionV1Error> {
    let candidate = layout.candidate_context_dir(current_generation);
    if candidate.exists() {
        std::fs::remove_dir_all(&candidate).map_err(|error| {
            SessionV1Error::io(
                candidate.clone(),
                format!("remove residual context candidate: {error}"),
            )
        })?;
    }
    Ok(())
}

/// Returns whether one candidate tail exceeds the given thresholds.
/// 返回一个候选尾段是否超过给定阈值。
fn should_rollover_with_limits(event_count: u64, byte_len: usize, limits: &FlushLimits) -> bool {
    event_count >= limits.max_lines || byte_len as u64 > limits.max_bytes
}

/// Returns whether one tail still carries open entities that forbid sealing.
/// 返回一个尾段是否仍携带禁止封口的开放实体。
fn tail_has_open_entities(events: &[ConversationEvent]) -> bool {
    // Newly admitted media prompts must remain mutable until their first preparation is frozen.
    // 新准入媒体提示词必须在首次准备冻结之前保持可变。
    // A later API or turn boundary ends that opportunity; unresolved old history must not pin the tail forever.
    // 后续API或回合边界会结束此机会；未解析旧历史不能永久占住尾段。
    let mut pending_prompt_media = false;
    for event in events {
        match &event.event {
            EventBody::UserInput(input)
                if input.parameters.as_ref().is_some_and(|parameters| {
                    matches!(
                        parameters.media_delivery,
                        vulcan_session_core::v1::UserPromptMediaDelivery::Pending
                            | vulcan_session_core::v1::UserPromptMediaDelivery::Preparing { .. }
                    )
                }) =>
            {
                pending_prompt_media = true;
            }
            EventBody::ApiStarted(_) | EventBody::TurnStats(_) => {
                pending_prompt_media = false;
            }
            EventBody::ToolCall(tool) => {
                if matches!(
                    tool.status,
                    ToolCallStatus::Running | ToolCallStatus::WaitingUser
                ) {
                    return true;
                }
            }
            EventBody::AssistantReasoning(reasoning)
                if reasoning.status == AssistantSemanticStatus::Streaming =>
            {
                return true;
            }
            EventBody::AssistantText(text) if text.status == AssistantSemanticStatus::Streaming => {
                return true;
            }
            _ => {}
        }
    }
    if pending_prompt_media {
        return true;
    }
    // An API group without its terminal stats is still open.
    // 没有终态统计的 API 组仍然开放。
    if let Some(last_api) = events.iter().rev().find_map(group_api_id) {
        let stats_seen = events.iter().any(|event| match &event.event {
            EventBody::ApiStats(stats) => stats.api_call_id.value() == last_api,
            _ => false,
        });
        if !stats_seen {
            return true;
        }
    }
    false
}

/// Extracts the api_call_id of one group event body.
/// 提取一个组事件体的 api_call_id。
fn group_api_id(event: &ConversationEvent) -> Option<u64> {
    match &event.event {
        EventBody::ApiStarted(e) => Some(e.api_call_id.value()),
        EventBody::AssistantReasoning(e) => Some(e.api_call_id.value()),
        EventBody::AssistantText(e) => Some(e.api_call_id.value()),
        EventBody::ToolCall(e) => Some(e.api_call_id.value()),
        EventBody::ApiStats(e) => Some(e.api_call_id.value()),
        _ => None,
    }
}

/// Returns the start seq and the active-tail entry of the current split map.
/// 返回当前分片映射的活动尾段起始 seq 与其条目。
///
/// The last split entry must be the active tail (`end_seq == None`); a sealed
/// last entry means the caller failed to apply a prior rollover metadata.
/// 最后一个分片条目必须是活动尾段(`end_seq == None`)；末条已封口说明调用方
/// 未应用先前的滚段 metadata。
fn active_tail_entry(metadata: &MetadataDocument) -> Result<(u64, SegmentInfo), SessionV1Error> {
    let entry = metadata.split_list.last().ok_or_else(|| {
        SessionV1Error::corrupt(
            ErrorCode::MetadataCorrupt,
            "",
            "split_list must contain an active tail entry",
        )
    })?;
    if entry.end_seq.is_some() {
        return Err(SessionV1Error::corrupt(
            ErrorCode::MetadataCorrupt,
            "",
            "split_list ends with a sealed segment; the active tail is missing",
        ));
    }
    let start = entry.start_seq.value();
    Ok((start, entry.clone()))
}
