//! Runtime log segment I/O helpers.
//! 运行时日志分段 I/O 辅助。

use super::*;

/// Reads one required runtime segment while tolerating only a corrupt JSONL suffix.
/// 读取一个必需运行时分段，同时仅容忍损坏的 JSONL 后缀。
///
/// # Parameters
/// 参数
/// `path` is the metadata-indexed runtime JSONL segment path.
/// `path` 是元数据索引的运行时 JSONL 分段路径。
///
/// # Returns
/// 返回值
/// Valid prefix lines plus corrupt suffix line numbers, or an error for missing/internal damage.
/// 返回有效前缀行与损坏后缀行号；分段缺失或内部损坏时返回错误。
pub(in crate::workspace_store) fn read_runtime_segment_lines_tolerant(
    path: &Path,
) -> Result<RuntimeLogRepairRead, String> {
    // file must already exist because runtime metadata declares this segment authoritative.
    // file 必须已经存在，因为运行时元数据声明该分段具有权威性。
    let file = File::open(path).map_err(|error| {
        format!(
            "Failed to open required runtime JSONL segment `{}`: {error}",
            path.display()
        )
    })?;
    let reader = BufReader::new(file);
    let mut read = RuntimeLogRepairRead::default();
    // first_corrupt_line marks the boundary after which valid records are forbidden.
    // first_corrupt_line 标记此后禁止再出现有效记录的边界。
    let mut first_corrupt_line = None;
    for (line_index, line) in reader.lines().enumerate() {
        let line_number = line_index.saturating_add(1);
        let line = line.map_err(|error| {
            format!(
                "读取 runtime JSONL 文件 `{}` 第 {line_number} 行失败：{error}",
                path.display()
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        match decode_runtime_log_event_line_json(&line) {
            Ok(value) => {
                if let Some(corrupt_line_number) = first_corrupt_line {
                    return Err(format!(
                        "Runtime JSONL segment `{}` has valid line {line_number} after corrupt line {corrupt_line_number}; only a corrupt suffix can be repaired.",
                        path.display()
                    ));
                }
                read.lines.push(value);
            }
            Err(error) if error.is_invalid_json() => {
                first_corrupt_line.get_or_insert(line_number);
                read.corrupt_line_numbers.push(line_number);
            }
            Err(error) => {
                return Err(format!(
                    "Failed to decode runtime JSONL segment `{}` line {line_number}: {error}",
                    path.display()
                ));
            }
        }
    }
    Ok(read)
}

/// Compacts one-based line numbers into a short comma-separated repair summary.
/// 将 1 基行号压缩为简短的逗号分隔修复摘要。
pub(in crate::workspace_store) fn compact_runtime_line_number_summary(
    line_numbers: &[usize],
) -> String {
    const MAX_VISIBLE_LINES: usize = 6;
    if line_numbers.is_empty() {
        return "unknown".to_string();
    }
    let mut visible = line_numbers
        .iter()
        .take(MAX_VISIBLE_LINES)
        .map(|line_number| line_number.to_string())
        .collect::<Vec<_>>();
    if line_numbers.len() > MAX_VISIBLE_LINES {
        visible.push(format!("+{} more", line_numbers.len() - MAX_VISIBLE_LINES));
    }
    visible.join(",")
}

/// Preserves a damaged runtime segment before rewriting the cleaned JSONL file.
/// 在重写清洁 JSONL 文件前保留损坏的运行时分段。
///
/// # Parameters
/// 参数
/// `path` is the damaged runtime segment whose exact bytes must remain recoverable.
/// `path` 是必须保留精确字节以供恢复的损坏运行时分段。
///
/// # Returns
/// 返回值
/// Success after copying the source beside itself, or a source-and-destination-aware error.
/// 将源文件复制到同级位置后返回成功，否则返回包含源与目标路径的错误。
pub(in crate::workspace_store) fn backup_corrupt_runtime_segment(
    path: &Path,
) -> Result<(), String> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            format!(
                "Corrupt runtime segment path has no UTF-8 file name: {}",
                path.display()
            )
        })?;
    let backup_name = format!(
        "{file_name}.corrupt-{}-{}.bak",
        current_unix_millis_text(),
        uuid::Uuid::new_v4()
    );
    // backup_path remains beside the damaged source so recovery evidence stays with its session.
    // backup_path 与损坏源文件保持同级，使恢复证据留在所属会话内。
    let backup_path = path.with_file_name(backup_name);
    fs::copy(path, &backup_path).map(|_| ()).map_err(|error| {
        format!(
            "Failed to back up corrupt runtime segment `{}` to `{}`: {error}",
            path.display(),
            backup_path.display()
        )
    })
}

/// Refreshes runtime metadata after a tolerant read removed corrupt JSONL rows.
/// 在容错读取移除损坏 JSONL 行后刷新运行时元数据。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory that owns the runtime artifacts.
/// `session_dir` 是拥有运行时产物的分段会话目录。
/// `metadata` is the validated metadata shape whose segment entries are refreshed.
/// `metadata` 是需要刷新分段条目的已验证元数据结构。
/// `lines` is the complete repaired runtime log used to derive global sequence metadata.
/// `lines` 是用于推导全局序号元数据的完整已修复运行时日志。
/// `_append_guard` proves the caller owns the shared append lock for the entire repair publication.
/// `_append_guard` 证明调用方在完整修复发布期间持有共享追加锁。
///
/// # Returns
/// 返回值
/// Refreshed metadata after the repaired segment state is atomically published.
/// 已修复分段状态原子发布后返回刷新后的元数据。
pub(in crate::workspace_store) fn refresh_runtime_metadata_from_lines_with_owned_lock(
    session_dir: &Path,
    metadata: &RuntimeLogMetadata,
    lines: &[RuntimeLogEventLine],
    _append_guard: &std::sync::MutexGuard<'_, ()>,
) -> Result<RuntimeLogMetadata, String> {
    // next_metadata is derived from the already validated single-segment snapshot.
    // next_metadata 从已经验证的单分段快照推导。
    let mut next_metadata = metadata.clone();
    next_metadata.next_seq = next_runtime_line_seq(lines)?;
    next_metadata.updated_at = lines
        .last()
        .map(|line| line.created_at.clone())
        .unwrap_or_else(current_unix_millis_text);
    // segment is guaranteed unique by runtime metadata validation before repair begins.
    // segment 在修复开始前已经由运行时元数据验证保证唯一。
    let segment = next_metadata
        .segments
        .first_mut()
        .ok_or_else(|| String::from("Runtime metadata has no segment during repair refresh."))?;
    // segment_path remains the validated fixed runtime segment path.
    // segment_path 保持为已经验证的固定运行时分段路径。
    let segment_path = session_dir.join(&segment.file);
    segment.line_count = u64::try_from(lines.len())
        .map_err(|_| String::from("Runtime line count exceeds u64 metadata capacity."))?;
    segment.byte_len = fs::metadata(&segment_path)
        .map_err(|error| {
            format!(
                "Failed to inspect runtime segment `{}`: {error}",
                segment_path.display()
            )
        })?
        .len();
    segment.end_seq = lines.last().map(|line| line.seq);
    segment.updated_at = lines.last().map(|line| line.created_at.clone());
    write_runtime_metadata(session_dir, &next_metadata)?;
    Ok(next_metadata)
}

/// Appends new runtime lines to the active segment and refreshes metadata plus index.
/// 将新的运行时行追加到活动分段，并刷新元数据与索引。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory whose runtime tail is extended.
/// `session_dir` 是运行时尾部将被扩展的分段会话目录。
/// `snapshot` is the metadata-and-lines pair validated earlier in the same lock transaction.
/// `snapshot` 是同一锁事务内较早完成验证的元数据与日志行组合。
/// `tail` contains ordered runtime lines beginning exactly at the persisted next sequence.
/// `tail` 包含从已持久化下一序号精确开始的有序运行时日志行。
/// `_append_guard` proves validation and publication share one append-lock lifetime.
/// `_append_guard` 证明验证与发布共享同一个追加锁生命周期。
///
/// # Returns
/// 返回值
/// Updated metadata after validation, durable append, metadata refresh, and index rebuild.
/// 完成验证、持久追加、元数据刷新与索引重建后返回更新后的元数据。
pub(in crate::workspace_store) fn append_runtime_log_tail_with_owned_lock<'guard>(
    session_dir: &Path,
    mut snapshot: RuntimeLogSnapshot<'guard>,
    tail: &[RuntimeLogEventLine],
    _append_guard: &'guard std::sync::MutexGuard<'_, ()>,
) -> Result<RuntimeLogMetadata, String> {
    // last_tail_line makes an empty append an explicit no-op with no metadata timestamp churn.
    // last_tail_line 使空追加成为显式空操作，不造成元数据时间戳变化。
    let Some(last_tail_line) = tail.last() else {
        return Ok(snapshot.metadata);
    };
    // next_seq validates every tail row before the first append byte is written.
    // next_seq 在写入首个追加字节前验证每一条尾部记录。
    let next_seq = validate_runtime_log_tail(&snapshot.metadata, tail)?;
    // active_segment_path comes from the validated fixed single-segment metadata.
    // active_segment_path 来自已验证的固定单分段元数据。
    let active_segment_path = session_dir.join(&snapshot.metadata.active_segment);
    append_json_lines_file(&active_segment_path, tail)?;
    snapshot.lines.extend(tail.iter().cloned());
    snapshot.metadata.next_seq = next_seq;
    snapshot.metadata.updated_at = last_tail_line.created_at.clone();
    // segment is unique because snapshot construction validates the single-segment contract.
    // segment 是唯一分段，因为快照构建会验证单分段契约。
    let segment = snapshot
        .metadata
        .segments
        .first_mut()
        .ok_or_else(|| String::from("Runtime metadata has no active segment during append."))?;
    segment.line_count = u64::try_from(snapshot.lines.len())
        .map_err(|_| String::from("Runtime line count exceeds u64 metadata capacity."))?;
    segment.byte_len = fs::metadata(&active_segment_path)
        .map_err(|error| {
            format!(
                "Failed to inspect runtime active segment `{}`: {error}",
                active_segment_path.display()
            )
        })?
        .len();
    segment.end_seq = Some(last_tail_line.seq);
    segment.updated_at = Some(last_tail_line.created_at.clone());
    write_runtime_metadata(session_dir, &snapshot.metadata)?;
    write_runtime_index(session_dir, &rebuild_runtime_log_index(&snapshot.lines))?;
    Ok(snapshot.metadata)
}

/// Validates the exact active runtime segment against its constant-size metadata boundary.
/// 针对常量大小元数据边界验证精确的活动运行时分段。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact segmented session directory and `metadata` is its validated runtime metadata.
/// `session_dir` 是精确的分段会话目录，`metadata` 是其已验证运行时元数据。
///
/// # Returns
/// 返回值
/// The exact regular-file path when its durable byte length matches metadata, otherwise a strict error.
/// 当耐久字节长度与元数据一致时返回精确普通文件路径，否则返回严格错误。
pub(in crate::workspace_store) fn validate_runtime_segment_append_boundary(
    session_dir: &Path,
    metadata: &RuntimeLogMetadata,
) -> Result<PathBuf, String> {
    // segment is unique because metadata shape validation enforces one fixed active segment.
    // segment 具有唯一性，因为元数据结构校验强制使用一个固定活动分段。
    let segment = metadata
        .segments
        .first()
        .ok_or_else(|| String::from("Runtime metadata has no active segment during append."))?;
    // active_segment_path is resolved directly; no directory enumeration is permitted.
    // active_segment_path 直接解析；不允许进行目录枚举。
    let active_segment_path = session_dir.join(&metadata.active_segment);
    let active_segment_metadata = fs::symlink_metadata(&active_segment_path).map_err(|error| {
        format!(
            "Failed to open required runtime JSONL segment `{}`: {error}",
            active_segment_path.display()
        )
    })?;
    if active_segment_metadata.file_type().is_symlink() || !active_segment_metadata.is_file() {
        return Err(format!(
            "Required runtime JSONL segment is not a regular file: {}",
            active_segment_path.display()
        ));
    }
    if active_segment_metadata.len() != segment.byte_len {
        return Err(format!(
            "Runtime segment byte length mismatch before bounded append: metadata {}, file {}.",
            segment.byte_len,
            active_segment_metadata.len()
        ));
    }
    Ok(active_segment_path)
}

/// Appends a bounded runtime tail using only exact metadata and the fixed active segment.
/// 仅使用精确元数据与固定活动分段追加有界运行时尾部。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact session directory, `metadata` is the authoritative append boundary,
/// `tail` is the contiguous event tail, and `_append_guard` proves serialization with other writers.
/// `session_dir` 是精确会话目录，`metadata` 是权威追加边界，`tail` 是连续事件尾部，
/// `_append_guard` 证明该操作与其他写入者保持串行。
///
/// # Returns
/// 返回值
/// Updated metadata after durable append, or unchanged metadata for an empty tail.
/// 耐久追加后返回更新元数据；尾部为空时返回未变元数据。
pub(in crate::workspace_store) fn append_runtime_log_tail_from_metadata_with_owned_lock(
    session_dir: &Path,
    mut metadata: RuntimeLogMetadata,
    tail: &[RuntimeLogEventLine],
    _append_guard: &std::sync::MutexGuard<'_, ()>,
) -> Result<RuntimeLogMetadata, String> {
    let active_segment_path = validate_runtime_segment_append_boundary(session_dir, &metadata)?;
    let Some(last_tail_line) = tail.last() else {
        return Ok(metadata);
    };
    // next_seq validates every candidate before the first append byte is written.
    // next_seq 在写入首个追加字节前验证每个候选事件。
    let next_seq = validate_runtime_log_tail(&metadata, tail)?;
    // encoded_tail preflights all serialization while metadata still references the old boundary.
    // encoded_tail 在元数据仍指向旧边界时预检全部序列化。
    let _encoded_tail = encode_json_lines(tail)?;
    invalidate_runtime_index_cache(session_dir)?;
    append_json_lines_file(&active_segment_path, tail)?;
    let persisted_byte_len = fs::symlink_metadata(&active_segment_path)
        .map_err(|error| {
            format!(
                "Failed to inspect appended runtime segment `{}`: {error}",
                active_segment_path.display()
            )
        })?
        .len();
    let appended_line_count = u64::try_from(tail.len())
        .map_err(|_| String::from("Runtime append tail exceeds u64 metadata capacity."))?;
    metadata.next_seq = next_seq;
    metadata.updated_at = last_tail_line.created_at.clone();
    // segment is reacquired mutably after all immutable boundary checks complete.
    // segment 在全部不可变边界校验完成后重新以可变方式取得。
    let segment = metadata
        .segments
        .first_mut()
        .ok_or_else(|| String::from("Runtime metadata has no active segment after append."))?;
    segment.line_count = segment
        .line_count
        .checked_add(appended_line_count)
        .ok_or_else(|| String::from("Runtime line count exceeds u64 metadata capacity."))?;
    segment.byte_len = persisted_byte_len;
    segment.end_seq = Some(last_tail_line.seq);
    segment.updated_at = Some(last_tail_line.created_at.clone());
    write_runtime_metadata(session_dir, &metadata)?;
    Ok(metadata)
}

/// Invalidates the fixed rebuildable runtime index without opening historical JSONL bytes.
/// 在不打开历史 JSONL 字节的情况下使固定的可重建运行时索引失效。
///
/// # Parameters
/// 参数
/// `session_dir` is the exact segmented session directory that owns the index cache.
/// `session_dir` 是拥有索引缓存的精确分段会话目录。
///
/// # Returns
/// 返回值
/// Success after exact removal or confirmed absence, otherwise a path-aware filesystem error.
/// 精确删除或确认缺失后返回成功，否则返回包含路径的文件系统错误。
pub(in crate::workspace_store) fn invalidate_runtime_index_cache(
    session_dir: &Path,
) -> Result<(), String> {
    let index_path = runtime_index_path(session_dir);
    match fs::remove_file(&index_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Failed to invalidate runtime index cache `{}`: {error}",
            index_path.display()
        )),
    }
}

/// Validates an append tail against one already validated runtime snapshot.
/// 针对一个已验证运行时快照校验追加尾部。
///
/// # Parameters
/// 参数
/// `metadata` supplies the exact next sequence accepted by the active segment.
/// `metadata` 提供活动分段所接受的精确下一序号。
/// `tail` contains every line that would be written by one append operation.
/// `tail` 包含一次追加操作将写入的全部日志行。
///
/// # Returns
/// 返回值
/// The next sequence after the complete tail, or an error before any persistence mutation.
/// 返回完整尾部之后的下一序号；任何持久化修改前发现问题则返回错误。
fn validate_runtime_log_tail(
    metadata: &RuntimeLogMetadata,
    tail: &[RuntimeLogEventLine],
) -> Result<u64, String> {
    // expected_seq advances only through strictly contiguous, supported-schema tail lines.
    // expected_seq 仅沿严格连续且模式受支持的尾部日志行推进。
    let mut expected_seq = metadata.next_seq;
    for (tail_index, line) in tail.iter().enumerate() {
        if line.schema_version != RUNTIME_LOG_SCHEMA_VERSION {
            return Err(format!(
                "Unsupported runtime line schema version {} at tail index {tail_index}.",
                line.schema_version
            ));
        }
        if line.seq != expected_seq {
            return Err(format!(
                "Runtime tail sequence mismatch at tail index {tail_index}: expected {expected_seq}, received {}.",
                line.seq
            ));
        }
        expected_seq = line.seq.checked_add(1).ok_or_else(|| {
            format!(
                "Runtime tail sequence overflow at tail index {tail_index}: {}.",
                line.seq
            )
        })?;
    }
    Ok(expected_seq)
}

/// Returns whether the existing runtime log is a provider-equivalent prefix of the desired log.
/// 返回现有运行时日志是否为目标日志的 Provider 等价前缀。
pub(in crate::workspace_store) fn runtime_log_prefix_matches(
    existing_lines: &[RuntimeLogEventLine],
    desired_lines: &[RuntimeLogEventLine],
) -> bool {
    existing_lines.len() <= desired_lines.len()
        && existing_lines
            .iter()
            .zip(desired_lines)
            .all(|(existing, desired)| runtime_log_line_matches(existing, desired))
}

/// Compares two runtime lines while ignoring write timestamps that can differ across saves.
/// 比较两条运行时日志行，并忽略不同保存时可能变化的写入时间戳。
///
/// # Parameters
/// 参数
/// `existing` is the persisted runtime line being compared.
/// `existing` 是参与比较的已持久化运行时日志行。
/// `desired` is the newly materialized runtime line expected at the same sequence.
/// `desired` 是同一序号下新物化的目标运行时日志行。
///
/// # Returns
/// 返回值
/// `true` only when sequence, schema, and successfully serialized event payloads are equal.
/// 仅当序号、模式及成功序列化后的事件载荷均相等时返回 `true`。
pub(in crate::workspace_store) fn runtime_log_line_matches(
    existing: &RuntimeLogEventLine,
    desired: &RuntimeLogEventLine,
) -> bool {
    // serialized_events preserves serialization failures instead of collapsing both into equal None values.
    // serialized_events 保留序列化失败，避免将双方失败折叠为相等的 None 值。
    let serialized_events = (
        serde_json::to_value(&existing.event),
        serde_json::to_value(&desired.event),
    );
    existing.seq == desired.seq
        && existing.schema_version == desired.schema_version
        && matches!(
            serialized_events,
            (Ok(existing_event), Ok(desired_event)) if existing_event == desired_event
        )
}

/// Reads runtime metadata for one segmented session directory.
/// 读取某个分段会话目录的运行时元数据。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory that must own runtime metadata.
/// `session_dir` 是必须拥有运行时元数据的分段会话目录。
///
/// # Returns
/// 返回值
/// Parsed and shape-validated metadata, or a strict absence, parse, or contract failure.
/// 返回已解析且已完成结构验证的元数据，否则返回严格缺失、解析或契约错误。
#[cfg(test)]
pub(in crate::workspace_store) fn read_runtime_metadata(
    session_dir: &Path,
) -> Result<RuntimeLogMetadata, String> {
    // metadata is validated before any caller can use its path or sequence fields.
    // metadata 会在任何调用方使用其路径或序号字段前完成验证。
    let metadata = read_required_json::<RuntimeLogMetadata>(&runtime_metadata_path(session_dir))?;
    validate_runtime_metadata_shape(&metadata)?;
    Ok(metadata)
}

/// Reads optional runtime metadata while preserving non-absence failures.
/// 读取可选运行时元数据，同时保留非缺失类失败。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory that may own runtime metadata.
/// `session_dir` 是可能拥有运行时元数据的分段会话目录。
///
/// # Returns
/// 返回值
/// Parsed metadata, `None` only when the document is absent, or a path-aware failure.
/// 返回已解析元数据；仅在文档缺失时返回 `None`，否则返回包含路径的失败。
pub(in crate::workspace_store) fn read_optional_runtime_metadata(
    session_dir: &Path,
) -> Result<Option<RuntimeLogMetadata>, String> {
    // metadata remains optional only for a genuine authoritative-open NotFound result.
    // metadata 仅在权威打开真正返回 NotFound 时保持可选缺失。
    let Some(metadata) =
        read_optional_json::<RuntimeLogMetadata>(&runtime_metadata_path(session_dir))?
    else {
        return Ok(None);
    };
    validate_runtime_metadata_shape(&metadata)?;
    Ok(Some(metadata))
}

/// Writes runtime metadata for one segmented session directory.
/// 写入某个分段会话目录的运行时元数据。
///
/// # Parameters
/// 参数
/// `session_dir` is the segmented session directory that owns the metadata document.
/// `session_dir` 是拥有元数据文档的分段会话目录。
/// `metadata` is the complete single-segment metadata state to validate and publish.
/// `metadata` 是需要验证并发布的完整单分段元数据状态。
///
/// # Returns
/// 返回值
/// Success after validated atomic publication, or a contract or persistence failure.
/// 验证并原子发布后返回成功，否则返回契约或持久化失败。
pub(in crate::workspace_store) fn write_runtime_metadata(
    session_dir: &Path,
    metadata: &RuntimeLogMetadata,
) -> Result<(), String> {
    validate_runtime_metadata_shape(metadata)?;
    write_json_file_atomic(&runtime_metadata_path(session_dir), metadata)
}

/// Validates the implemented single-segment runtime metadata contract.
/// 验证已经实现的单分段运行时元数据契约。
///
/// # Parameters
/// 参数
/// `metadata` is the parsed or generated runtime metadata document to validate.
/// `metadata` 是需要验证的已解析或已生成运行时元数据文档。
///
/// # Returns
/// 返回值
/// Success only for schema version one with one fixed, internally consistent active segment.
/// 仅对模式版本一且具有一个固定、内部一致活动分段的元数据返回成功。
pub(in crate::workspace_store) fn validate_runtime_metadata_shape(
    metadata: &RuntimeLogMetadata,
) -> Result<(), String> {
    if metadata.schema_version != 1 {
        return Err(format!(
            "Unsupported runtime metadata schema version: {}.",
            metadata.schema_version
        ));
    }
    if metadata.segments.len() != 1 {
        return Err(format!(
            "Runtime metadata must contain exactly one segment; found {}.",
            metadata.segments.len()
        ));
    }
    // segment is unique under the implemented persistence contract.
    // segment 在已经实现的持久化契约下具有唯一性。
    let segment = &metadata.segments[0];
    // expected_file is the only runtime segment path produced by this repository.
    // expected_file 是当前仓库唯一会生成的运行时分段路径。
    let expected_file = runtime_segment_file_name();
    if segment.file != expected_file {
        return Err(format!(
            "Runtime segment path must be `{expected_file}`; found `{}`.",
            segment.file
        ));
    }
    if metadata.active_segment != segment.file {
        return Err(format!(
            "Runtime active segment `{}` does not match the unique metadata segment `{}`.",
            metadata.active_segment, segment.file
        ));
    }
    if segment.start_seq != 1 {
        return Err(format!(
            "Runtime segment must start at sequence 1; found {}.",
            segment.start_seq
        ));
    }
    if segment.sealed {
        return Err(String::from(
            "The unique runtime segment cannot be sealed while it is active.",
        ));
    }
    // declared_end_seq distinguishes the empty and non-empty metadata shapes.
    // declared_end_seq 区分空与非空两种元数据结构。
    let declared_end_seq = match (segment.line_count, segment.end_seq) {
        (0, None) => {
            if segment.byte_len != 0 {
                return Err(format!(
                    "Empty runtime segment must declare zero bytes; found {}.",
                    segment.byte_len
                ));
            }
            if segment.updated_at.is_some() {
                return Err(String::from(
                    "Empty runtime segment cannot declare an updated timestamp.",
                ));
            }
            None
        }
        (0, Some(end_seq)) => {
            return Err(format!(
                "Empty runtime segment cannot declare end sequence {end_seq}."
            ));
        }
        (_, None) => {
            return Err(String::from(
                "Non-empty runtime segment must declare an end sequence.",
            ));
        }
        (_, Some(end_seq)) => {
            if segment.byte_len == 0 {
                return Err(String::from(
                    "Non-empty runtime segment must declare a positive byte length.",
                ));
            }
            if segment.updated_at.is_none() {
                return Err(String::from(
                    "Non-empty runtime segment must declare an updated timestamp.",
                ));
            }
            Some(end_seq)
        }
    };
    if let Some(end_seq) = declared_end_seq {
        // declared_line_count is derived from the strict one-based contiguous sequence range.
        // declared_line_count 从严格的一基连续序号范围推导。
        let declared_line_count = end_seq
            .checked_sub(segment.start_seq)
            .and_then(|distance| distance.checked_add(1))
            .ok_or_else(|| {
                format!(
                    "Runtime segment end sequence {end_seq} precedes start sequence {}.",
                    segment.start_seq
                )
            })?;
        if segment.line_count != declared_line_count {
            return Err(format!(
                "Runtime segment line count mismatch: declared {}, sequence range requires {declared_line_count}.",
                segment.line_count
            ));
        }
    }
    // expected_next_seq is derived from the declared complete segment range without saturation.
    // expected_next_seq 从已声明的完整分段范围推导，且不使用饱和运算。
    let expected_next_seq = match declared_end_seq {
        Some(end_seq) => end_seq.checked_add(1).ok_or_else(|| {
            format!("Runtime metadata sequence overflows after end sequence {end_seq}.")
        })?,
        None => 1,
    };
    if metadata.next_seq != expected_next_seq {
        return Err(format!(
            "Runtime metadata next sequence mismatch: expected {expected_next_seq}, found {}.",
            metadata.next_seq
        ));
    }
    if metadata.conversation_checkpoint_version > SEGMENTED_RUNTIME_PROJECTION_CHECKPOINT_VERSION {
        return Err(format!(
            "Unsupported runtime conversation checkpoint version: {}.",
            metadata.conversation_checkpoint_version
        ));
    }
    if metadata.conversation_checkpoint_version == SEGMENTED_RUNTIME_PROJECTION_CHECKPOINT_VERSION {
        if metadata.conversation_event_count > segment.line_count {
            return Err(format!(
                "Runtime conversation event count {} exceeds segment line count {}.",
                metadata.conversation_event_count, segment.line_count
            ));
        }
        if metadata.conversation_anchor_message_index > metadata.conversation_message_count {
            return Err(format!(
                "Runtime conversation anchor message index {} exceeds message count {}.",
                metadata.conversation_anchor_message_index, metadata.conversation_message_count
            ));
        }
        if metadata.conversation_anchor_event_count > metadata.conversation_event_count {
            return Err(format!(
                "Runtime conversation anchor event count {} exceeds event count {}.",
                metadata.conversation_anchor_event_count, metadata.conversation_event_count
            ));
        }
        match (
            metadata.conversation_message_count,
            metadata.conversation_tail_message_sha256.as_deref(),
        ) {
            (0, None) => {}
            (0, Some(_)) => {
                return Err(String::from(
                    "Empty runtime conversation checkpoint cannot declare a tail-message digest.",
                ));
            }
            (_, Some(digest)) => validate_runtime_checkpoint_sha256(
                digest,
                "runtime conversation tail-message digest",
            )?,
            (_, None) => {
                return Err(String::from(
                    "Non-empty runtime conversation checkpoint must declare a tail-message digest.",
                ));
            }
        }
        match (
            metadata.conversation_anchor_event_count,
            metadata.conversation_anchor_projection_sha256.as_deref(),
        ) {
            (0, None) => {}
            (0, Some(_)) => {
                return Err(String::from(
                    "Empty runtime conversation anchor cannot declare a projection digest.",
                ));
            }
            (_, Some(digest)) => validate_runtime_checkpoint_sha256(
                digest,
                "runtime conversation anchor-projection digest",
            )?,
            (_, None) => {
                return Err(String::from(
                    "Non-empty runtime conversation anchor must declare a projection digest.",
                ));
            }
        }
    }
    if metadata
        .applied_compaction_id
        .as_deref()
        .is_some_and(|compaction_id| compaction_id.trim().is_empty())
    {
        return Err(String::from(
            "Runtime applied compaction identifier cannot be empty.",
        ));
    }
    Ok(())
}

/// Validates one lowercase hexadecimal SHA-256 checkpoint digest.
/// 验证一个小写十六进制 SHA-256 检查点摘要。
///
/// # Parameters
/// 参数
/// `digest` is the persisted digest text and `label` identifies the failing metadata field.
/// `digest` 是持久化摘要文本，`label` 标识失败的元数据字段。
///
/// # Returns
/// 返回值
/// Success for exactly 64 lowercase hexadecimal bytes, otherwise an actionable metadata error.
/// 对恰好 64 个小写十六进制字节返回成功，否则返回可操作的元数据错误。
fn validate_runtime_checkpoint_sha256(digest: &str, label: &str) -> Result<(), String> {
    if digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Ok(());
    }
    Err(format!(
        "Invalid {label}; expected 64 lowercase hexadecimal characters."
    ))
}

/// Writes the rebuildable runtime index for one segmented session directory.
/// 写入某个分段会话目录的可重建运行时索引。
pub(in crate::workspace_store) fn write_runtime_index(
    session_dir: &Path,
    index: &RuntimeLogIndex,
) -> Result<(), String> {
    write_json_file_atomic(&runtime_index_path(session_dir), index)
}

/// Returns the runtime metadata path for one session directory.
/// 返回某个会话目录的运行时元数据路径。
pub(in crate::workspace_store) fn runtime_metadata_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_RUNTIME_METADATA_FILE)
}

/// Returns the runtime index path for one session directory.
/// 返回某个会话目录的运行时索引路径。
pub(in crate::workspace_store) fn runtime_index_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_RUNTIME_INDEX_FILE)
}

/// Returns the runtime segment directory path for one session directory.
/// 返回某个会话目录的运行时分段目录路径。
pub(in crate::workspace_store) fn runtime_dir_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_RUNTIME_DIR)
}

/// Returns the only implemented runtime segment path relative to the session directory.
/// 返回相对会话目录的唯一已实现运行时分段路径。
///
/// # Returns
/// 返回值
/// The fixed `runtime/r001.jsonl` relative path used by every runtime transaction.
/// 返回每个运行时事务使用的固定 `runtime/r001.jsonl` 相对路径。
pub(in crate::workspace_store) fn runtime_segment_file_name() -> String {
    format!("{SEGMENTED_RUNTIME_DIR}/r001.jsonl")
}
