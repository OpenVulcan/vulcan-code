//! Atomic publication of local history cleanup.
//! 本地历史清理的原子发布。

use std::{collections::BTreeSet, fs, path::PathBuf};

use uuid::Uuid;
use vulcan_session_core::v1::{
    config::MetadataDocument,
    document::SessionDocument,
    history_cleanup::{
        CleanedEventRange, CleanedHistory, HistoryPublication, build_cleanup_candidate,
    },
    ids::Seq,
};

use super::{
    atomic_io::write_file_atomic,
    documents::{read_metadata, write_json_document},
    errors::{ErrorCode, SessionV1Error},
    layout::SessionV1Layout,
    metadata_access::publication_lock,
    segment::serialize_segment,
};

/// Publishes an extracted range, then returns the exact new runtime document and display records.
/// 发布已提取范围，然后返回精确的新运行时文档和展示记录。
///
/// The caller owns the runtime writer and mutation locks for this session.
/// 调用方持有此会话的运行时写入锁和变更锁。
pub fn publish_cleanup(
    layout: &SessionV1Layout,
    document: &SessionDocument,
    through: Seq,
) -> Result<(SessionDocument, CleanedHistory), SessionV1Error> {
    // Strict Core validation and request equality both precede publication.
    // 严格 Core 校验和请求一致性检查都在发布前完成。
    let mut candidate = build_cleanup_candidate(document, through)
        .map_err(|error| history_error(layout, error.to_string()))?;
    // Serialize the physical metadata switch against other writers.
    // 与其他写入方串行化物理元数据切换。
    let _guard = publication_lock(&layout.metadata_path())?;
    // Read the current pointer and exact segment map under the same lock.
    // 在同一把锁内读取当前指针与精确分段目录。
    let current = read_metadata(&layout.metadata_path())?;
    if current != document.metadata {
        return Err(history_error(
            layout,
            "history cleanup source revision changed",
        ));
    }
    // The existing history is combined with new independent IDs and ordered for display.
    // 合并已有历史与新独立标识，并按展示顺序排列。
    let mut history = read_published_history(layout, &current)?;
    history.turns.append(&mut candidate.history.turns);
    history.resources.append(&mut candidate.history.resources);
    history.turns.sort_by_key(|turn| turn.closed_at_ms);
    history
        .resources
        .sort_by_key(|resource| resource.created_at_ms);
    history
        .validate()
        .map_err(|error| history_error(layout, error.to_string()))?;
    // Keep runtime lineage unchanged so existing memory anchors and cache identity remain valid.
    // 保持运行时沿革一致，使现有记忆锚点和缓存身份继续有效。
    let mut next = current.clone();
    // Every modified segment is staged in its inactive bank before one metadata switch.
    // 每个修改的分段先写入非活动 bank，再执行一次元数据切换。
    let mut retired = Vec::new();
    for entry in &mut next.split_list {
        // Segment numbers and starts remain identical for replay-window verification.
        // 分段序号和起点保持相同，以便校验重放窗口。
        let start = candidate
            .events
            .partition_point(|event| event.seq < entry.start_seq);
        // The last active bank has no terminal sequence.
        // 最后一个活动 bank 没有终止序号。
        let end = entry.end_seq.map_or(candidate.events.len(), |seq| {
            candidate.events.partition_point(|event| event.seq <= seq)
        });
        if candidate.events[start..end] == document.events()[start..end] {
            continue;
        }
        // Stage only the changed bank; readers with the old metadata still see the old bank.
        // 只暂存有变更的 bank，持有旧元数据的读取方仍看到旧 bank。
        let bank =
            SessionV1Layout::alternate_segment_file_name(entry.segment_number, &entry.file_name)
                .ok_or_else(|| history_error(layout, "invalid segment bank"))?;
        // Serialize with the ordinary segment codec and retain its exact byte count.
        // 使用普通分段编码器，并保留精确字节数。
        let bytes = serialize_segment(&candidate.events[start..end])?;
        write_file_atomic(
            &layout.segment_entry_path(next.segment_generation, &bank),
            &bytes,
            "history cleanup segment",
        )?;
        retired.push(layout.segment_entry_path(next.segment_generation, &entry.file_name));
        entry.file_name = bank;
        entry.byte_len = bytes.len() as u64;
    }
    // The publication descriptor binds the new history and changed segments in one document.
    // 发布描述符在同一文档中绑定新历史和变更分段。
    let mut publication = current
        .cleaned_history
        .clone()
        .unwrap_or(HistoryPublication {
            id: Uuid::nil(),
            ranges: Vec::new(),
        });
    publication.id = Uuid::new_v4();
    // Earlier publications inside this prefix are subsumed without recreating their display entries.
    // 这段前缀内的既有发布合并到新区间，展示记录不会重新生成。
    publication
        .ranges
        .retain(|range| range.through < candidate.from || range.from > through);
    publication.ranges.push(CleanedEventRange {
        from: candidate.from,
        through,
    });
    publication.ranges.sort_by_key(|range| range.from);
    next.cleaned_history = Some(publication.clone());
    // Construct the exact replacement runtime document before publishing.
    // 在发布前构造精确的运行时替换文档。
    let replacement = SessionDocument::from_parts(
        next.clone(),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        next.split_list.clone(),
        candidate.events,
    )
    .map_err(|error| history_error(layout, error.to_string()))?;
    write_json_document(
        &history_path(layout, publication.id),
        &history,
        "cleaned session history",
    )?;
    commit_metadata(layout, &next)?;
    if let Some(previous) = current.cleaned_history {
        retired.push(history_path(layout, previous.id));
    }
    retire_files(retired);
    Ok((replacement, history))
}

/// Deletes selected records from the published history and returns the new metadata and view.
/// 删除已发布历史中的选定条目，返回新元数据与展示视图。
pub fn delete_history(
    layout: &SessionV1Layout,
    ids: &BTreeSet<Uuid>,
) -> Result<(MetadataDocument, CleanedHistory), SessionV1Error> {
    // Complete any schema migration before locking the current publication.
    // 在锁定当前发布前完成任何模式升级。
    read_metadata(&layout.metadata_path())?;
    // The same lock makes concurrent selection changes atomic.
    // 同一把锁使并发选择变更保持原子性。
    let _guard = publication_lock(&layout.metadata_path())?;
    // Only the display history pointer changes; event bytes and runtime lineage stay intact.
    // 仅展示历史指针变化，事件字节与运行时沿革保持完整。
    let mut metadata = read_metadata(&layout.metadata_path())?;
    // Missing IDs are idempotent, including after the whole history is already empty.
    // 缺失标识按幂等处理，包括全部历史已经清空。
    let mut history = read_published_history(layout, &metadata)?;
    if history.delete(ids) == 0 {
        return Ok((metadata, history));
    }
    // A nonempty deletion can only come from a previously published history.
    // 非空删除只可能来自先前发布的历史。
    let publication = metadata
        .cleaned_history
        .as_mut()
        .ok_or_else(|| history_error(layout, "cleaned history pointer missing"))?;
    // Remember the old file until the pointer switch succeeds.
    // 在指针切换成功前记住旧文件。
    let previous = publication.id;
    publication.id = Uuid::new_v4();
    write_json_document(
        &history_path(layout, publication.id),
        &history,
        "cleaned session history",
    )?;
    commit_metadata(layout, &metadata)?;
    retire_files([history_path(layout, previous)]);
    Ok((metadata, history))
}

/// Resolves the exact history file from a typed publication ID.
/// 由强类型发布标识定位精确历史文件。
fn history_path(layout: &SessionV1Layout, id: Uuid) -> PathBuf {
    layout
        .session_dir()
        .join(format!("cleaned-history-v1-{id}.json"))
}

/// Writes a fork's independent history within its unpublished staging directory.
/// 在分叉的未发布暂存目录中写入独立历史。
pub(super) fn stage_fork_history(
    layout: &SessionV1Layout,
    metadata: &MetadataDocument,
    history: Option<&CleanedHistory>,
) -> Result<(), SessionV1Error> {
    match (&metadata.cleaned_history, history) {
        (Some(publication), Some(history)) => write_json_document(
            &history_path(layout, publication.id),
            history,
            "forked cleaned history",
        ),
        (None, None) => Ok(()),
        _ => Err(history_error(
            layout,
            "fork cleaned history does not match its publication",
        )),
    }
}

/// Reads the history selected by the provided already-validated metadata.
/// 读取给定已校验元数据选定的历史。
fn read_published_history(
    layout: &SessionV1Layout,
    metadata: &MetadataDocument,
) -> Result<CleanedHistory, SessionV1Error> {
    // Legacy sessions have no independent history file before explicit cleanup.
    // 旧会话在显式清理前没有独立历史文件。
    let Some(publication) = &metadata.cleaned_history else {
        return Ok(CleanedHistory::default());
    };
    read_history_version(layout, publication)
}

/// Loads one exact immutable display revision, failing if it was retired concurrently.
/// 加载精确的不可变展示修订，并在已被并发退役时显式失败。
pub fn read_history_version(
    layout: &SessionV1Layout,
    publication: &HistoryPublication,
) -> Result<CleanedHistory, SessionV1Error> {
    // A missing published file is a storage error.
    // 已发布文件缺失属于存储错误。
    let path = history_path(layout, publication.id);
    // Keep user history out of diagnostics even when decoding fails.
    // 即使解码失败也避免在诊断中输出用户历史。
    let bytes = fs::read(&path).map_err(|error| SessionV1Error::io(&path, error.to_string()))?;
    let history: CleanedHistory = serde_json::from_slice(&bytes).map_err(|error| {
        SessionV1Error::corrupt(
            ErrorCode::MetadataCorrupt,
            &path,
            format!("decode cleaned history: {error}"),
        )
    })?;
    history
        .validate()
        .map_err(|error| history_error(layout, error.to_string()))?;
    Ok(history)
}

/// Writes the commit marker while the caller holds the publication lock.
/// 在调用方持有发布锁时写入提交标记。
fn commit_metadata(
    layout: &SessionV1Layout,
    metadata: &MetadataDocument,
) -> Result<(), SessionV1Error> {
    // Serialize outside the normal metadata wrapper because its lock is already held.
    // 在普通元数据包装器外序列化，因为已持有其锁。
    let bytes =
        serde_json::to_vec(metadata).map_err(|error| history_error(layout, error.to_string()))?;
    write_file_atomic(&layout.metadata_path(), &bytes, "history cleanup commit")
}

/// Retires obsolete banks only after the replacement is authoritative.
/// 仅在替换成为权威后退役旧 bank。
fn retire_files(paths: impl IntoIterator<Item = PathBuf>) {
    for path in paths {
        if let Err(error) = fs::remove_file(&path) {
            tracing::warn!(path = %path.display(), %error, "Could not retire replaced history file");
        }
    }
}

/// Reports failure against the exact session publication.
/// 把失败归属到精确会话发布。
fn history_error(layout: &SessionV1Layout, detail: impl Into<String>) -> SessionV1Error {
    SessionV1Error::corrupt(
        ErrorCode::ReferenceViolation,
        layout.metadata_path(),
        detail,
    )
}
