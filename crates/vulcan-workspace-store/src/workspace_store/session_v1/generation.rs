//! Physical publication of Core-built edit and fork generations.
//! 发布由 Core 构建的编辑与分叉 generation 的物理存储层。

use std::fs;

use vulcan_session_core::v1::config::SegmentInfo;
use vulcan_session_core::v1::{GenerationBuildOutput, MetadataDocument, Seq};

use super::atomic_io::write_file_atomic;
use super::documents::write_json_document;
use super::errors::{ErrorCode, SessionV1Error};
use super::layout::SessionV1Layout;

/// Complete physical input required to publish one Core-built edited generation.
/// 发布一份由 Core 构建的编辑 generation 所需的完整物理输入。
pub struct PublishEditGenerationRequest<'a> {
    /// Currently published metadata that owns the source physical generation.
    /// 拥有源物理 generation 的当前已发布 metadata。
    pub current_metadata: &'a MetadataDocument,
    /// Complete storage-independent domain output built and validated by Session Core.
    /// 由 Session Core 构建并校验的完整、与存储无关的领域输出。
    pub output: &'a GenerationBuildOutput,
}

/// Publishes one Core-built history edit through a single metadata commit point.
/// 通过单一 metadata 提交点发布一份由 Core 构建的历史编辑。
///
/// # Parameters / 参数
/// `layout` identifies the exact session directory; `request` contains the current
/// physical pointers and one complete Core output.
/// `layout` 标识精确会话目录；`request` 包含当前物理指针与一份完整 Core 输出。
///
/// # Returns / 返回值
/// Returns the published metadata with only the physical `split_list` filled by this layer.
/// 返回已发布 metadata；本层只填充物理 `split_list`。
pub fn publish_edit_generation(
    layout: &SessionV1Layout,
    request: PublishEditGenerationRequest<'_>,
) -> Result<MetadataDocument, SessionV1Error> {
    super::writer::ensure_request_generations_are_current(layout, request.current_metadata)?;
    validate_edit_publication_input(layout, &request)?;
    cleanup_stale_candidate(
        layout,
        request.current_metadata.segment_generation,
        request.current_metadata.active_context_generation,
    )?;

    let output = request.output;
    let target_value = output.history_target_seq.value();
    let target_index = target_value
        .checked_sub(1)
        .ok_or_else(|| invalid_output(layout, "history_target_seq must be positive"))?
        as usize;
    let mut split_list = Vec::new();
    let mut target_segment_seen = false;

    for entry in &request.current_metadata.split_list {
        let source_end = entry.end_seq.map(Seq::value).unwrap_or(
            request
                .current_metadata
                .id_frontier
                .next_seq
                .saturating_sub(1),
        );
        if source_end < target_value {
            copy_sealed_prefix_segment(
                layout,
                request.current_metadata.segment_generation,
                output.metadata.segment_generation,
                entry,
            )?;
            split_list.push(entry.clone());
            continue;
        }
        if entry.start_seq.value() > target_value {
            break;
        }

        target_segment_seen = true;
        let start_index =
            entry.start_seq.value().checked_sub(1).ok_or_else(|| {
                invalid_output(layout, "target segment start seq must be positive")
            })? as usize;
        if start_index > target_index || start_index >= output.events.len() {
            return Err(invalid_output(
                layout,
                "Core output target is outside the current physical segment map",
            ));
        }
        let segment_events = &output.events[start_index..];
        let bytes = super::segment::serialize_segment(segment_events)?;
        let file_name = SessionV1Layout::segment_file_name(entry.segment_number);
        let target_path = layout.segment_entry_path(output.metadata.segment_generation, &file_name);
        write_file_atomic(&target_path, &bytes, "rewritten Core edit tail")?;
        split_list.push(SegmentInfo {
            segment_number: entry.segment_number,
            file_name,
            start_seq: entry.start_seq,
            end_seq: None,
            event_count: u64::try_from(segment_events.len()).map_err(|_| {
                invalid_output(layout, "rewritten edit tail event count overflows u64")
            })?,
            byte_len: u64::try_from(bytes.len()).map_err(|_| {
                invalid_output(layout, "rewritten edit tail byte count overflows u64")
            })?,
            next_event_local_id: vulcan_session_core::v1::event::next_event_local_id(
                &output.events,
            ),
        });
        break;
    }
    if !target_segment_seen {
        return Err(invalid_output(
            layout,
            "current split_list does not cover the Core history target",
        ));
    }

    let mut published = output.metadata.clone();
    published.split_list = split_list;
    super::writer::write_context_generation(
        layout,
        published.active_context_generation,
        &vulcan_session_core::v1::context::CompactionsDocument::new_with_nodes(
            layout.session_id.clone(),
            output.compactions.clone(),
        ),
        &vulcan_session_core::v1::context::RuntimeDocument {
            initialization_state: output.runtime_lock.initialization_state,
            current_compaction_id: output.runtime_lock.current_compaction_id,
            locked_sources: output.runtime_lock.locked_sources.clone(),
            updated_at_ms: published.updated_at_ms,
            ..vulcan_session_core::v1::context::RuntimeDocument::new(
                layout.session_id.clone(),
                published.updated_at_ms,
            )
        },
        "Core edit context candidate",
    )?;
    write_json_document(
        &layout.metadata_path(),
        &published,
        "Core edit metadata generation switch",
    )?;
    Ok(published)
}

/// Validates that an edited Core output exactly advances the currently published generations.
/// 校验编辑 Core 输出精确推进当前已发布 generation。
fn validate_edit_publication_input(
    layout: &SessionV1Layout,
    request: &PublishEditGenerationRequest<'_>,
) -> Result<(), SessionV1Error> {
    let output = request.output;
    if request.current_metadata.session_id != layout.session_id
        || output.metadata.session_id != layout.session_id
    {
        return Err(invalid_output(layout, "edit session identity mismatch"));
    }
    if !output.metadata.split_list.is_empty() {
        return Err(invalid_output(
            layout,
            "Core output split_list must remain unpublished",
        ));
    }
    let expected_segment = request
        .current_metadata
        .segment_generation
        .checked_add(1)
        .ok_or_else(|| invalid_output(layout, "segment generation overflow"))?;
    let expected_context = request
        .current_metadata
        .active_context_generation
        .checked_add(1)
        .ok_or_else(|| invalid_output(layout, "context generation overflow"))?;
    if output.metadata.segment_generation != expected_segment
        || output.metadata.active_context_generation != expected_context
    {
        return Err(invalid_output(
            layout,
            "Core output does not advance both generation pointers exactly once",
        ));
    }
    if output.events.last().map(|event| event.seq.value())
        != Some(output.metadata.id_frontier.next_seq.saturating_sub(1))
    {
        return Err(invalid_output(
            layout,
            "Core output event frontier disagrees with metadata",
        ));
    }
    Ok(())
}

/// Deletes only the exact unpublished event/context candidates before rebuilding them.
/// 重建前只删除精确的未发布事件与 context 候选。
fn cleanup_stale_candidate(
    layout: &SessionV1Layout,
    current_generation: u64,
    current_context_generation: u64,
) -> Result<(), SessionV1Error> {
    let candidate = layout.candidate_generation_dir(current_generation);
    if candidate.exists() {
        fs::remove_dir_all(&candidate).map_err(|error| {
            SessionV1Error::io(
                candidate.clone(),
                format!("remove residual candidate generation: {error}"),
            )
        })?;
    }
    super::writer::cleanup_context_candidate(layout, current_context_generation)
}

/// Copies one complete sealed prefix segment into the unpublished generation.
/// 把一个完整封口前缀分片复制到尚未发布的 generation。
fn copy_sealed_prefix_segment(
    layout: &SessionV1Layout,
    source_generation: u64,
    target_generation: u64,
    entry: &SegmentInfo,
) -> Result<(), SessionV1Error> {
    let source = layout.segment_entry_path(source_generation, &entry.file_name);
    let target = layout.segment_entry_path(target_generation, &entry.file_name);
    let bytes = fs::read(&source).map_err(|error| {
        SessionV1Error::io(source.clone(), format!("read sealed segment: {error}"))
    })?;
    write_file_atomic(&target, &bytes, "byte-copied Core edit prefix")
}

/// Builds one structured invalid-Core-output storage error.
/// 构建一条结构化的非法 Core 输出存储错误。
fn invalid_output(layout: &SessionV1Layout, detail: impl Into<String>) -> SessionV1Error {
    SessionV1Error::corrupt(ErrorCode::MetadataCorrupt, layout.metadata_path(), detail)
}
