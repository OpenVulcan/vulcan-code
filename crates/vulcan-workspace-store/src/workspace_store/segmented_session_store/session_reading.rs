//! Segmented session reading, replay, and display maintenance.
//! 分段会话读取、重放与显示维护。

use super::*;

/// Reads a backward transcript window ending before the optional sequence cursor.
/// 读取一个结束于可选序号游标之前的反向转录窗口。
pub(in crate::workspace_store) fn read_segmented_session_event_window(
    session_dir: &Path,
    before_seq: Option<u64>,
    limit: usize,
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    // prepared captures only a metadata-bounded suffix while the session read lock is held.
    // prepared 仅在持有会话读锁期间捕获元数据限定的后缀。
    let prepared = prepare_segmented_event_window_read(session_dir, before_seq, limit)?;
    // cursor is the exclusive event-sequence boundary applied after lock-free decoding.
    // cursor 是在无锁解码后应用的独占事件序号边界。
    let cursor = before_seq.unwrap_or(u64::MAX);
    // window accumulates newest rows first and is reversed once before returning.
    // window 先累积最新记录，并在返回前统一反转一次。
    let mut window = Vec::with_capacity(limit);
    for prepared_segment in prepared.iter().rev() {
        // newline_count anchors exact one-based diagnostics while decoding in reverse.
        // newline_count 在反向解码时锚定精确的一基行号诊断。
        let newline_count = prepared_segment
            .bytes
            .iter()
            .filter(|byte| **byte == b'\n')
            .count();
        for (reverse_index, raw_line) in prepared_segment
            .bytes
            .rsplit(|byte| *byte == b'\n')
            .enumerate()
        {
            if jsonl_line_is_blank(raw_line) {
                continue;
            }
            // line_number maps the reverse byte iteration back to the source JSONL row.
            // line_number 将反向字节迭代映射回源 JSONL 行号。
            let line_number = newline_count
                .saturating_add(1)
                .saturating_sub(reverse_index);
            let Some(line) =
                decode_segment_event_line_tolerant(raw_line, &prepared_segment.path, line_number)?
            else {
                continue;
            };
            if line.seq >= cursor {
                continue;
            }
            window.push(line);
            if window.len() == limit {
                window.reverse();
                return Ok(window);
            }
        }
    }
    window.reverse();
    Ok(window)
}

/// One exact segment byte prefix captured under a root-session read lock.
/// 在根会话读锁内捕获的一段精确分段字节前缀。
struct PreparedSegmentBytes {
    /// Exact metadata-selected segment path.
    /// 元数据选择出的精确分段路径。
    path: PathBuf,
    /// Immutable bytes limited to the metadata-captured durable boundary.
    /// 限制在元数据捕获可靠边界内的不可变字节。
    bytes: Vec<u8>,
}

/// Stable read plan shared by boundary and raw display projections.
/// 由 Boundary 与 Raw 展示投影共享的稳定读取计划。
struct PreparedSessionRead {
    /// Monotonic storage generation captured from the metadata sequence frontier.
    /// 从元数据序号前沿捕获的单调存储代次。
    storage_generation: u64,
    /// Latest rollback sequence that excludes superseded raw display rows.
    /// 排除已被替换原始展示行的最近回滚序号。
    rollback_floor_seq: Option<u64>,
    /// Whether every selected boundary row is covered by complete per-segment projection indexes.
    /// 每个选中边界行是否都由完整的分段投影索引覆盖。
    boundary_index_complete: bool,
    /// Exact immutable segment prefixes ordered from oldest to newest.
    /// 按从旧到新排序的精确不可变分段前缀。
    segments: Vec<PreparedSegmentBytes>,
}

/// Reads exactly one metadata-authorized immutable segment prefix.
/// 精确读取一个由元数据授权的不可变分段前缀。
///
/// # Parameters
/// 参数
/// `segment_path` is the exact metadata-selected regular file.
/// `segment_path` 是元数据选择出的精确常规文件。
/// `durable_byte_len` is the authoritative prefix length captured from metadata.
/// `durable_byte_len` 是从元数据捕获的权威前缀长度。
///
/// # Returns
/// 返回值
/// Exactly the requested prefix bytes, or a strict path/type/short-read failure.
/// 返回精确请求的前缀字节，或严格的路径、类型、短读失败。
fn read_exact_segment_prefix(
    segment_path: &Path,
    durable_byte_len: usize,
) -> Result<Vec<u8>, String> {
    // file_metadata validates the exact path before opening and rejects link substitution.
    // file_metadata 在打开前验证精确路径，并拒绝链接替换。
    let file_metadata = fs::symlink_metadata(segment_path).map_err(|error| {
        format!(
            "Failed to inspect required session segment `{}`: {error}",
            segment_path.display()
        )
    })?;
    if file_metadata.file_type().is_symlink() || !file_metadata.is_file() {
        return Err(format!(
            "Required session segment is not a regular file: {}",
            segment_path.display()
        ));
    }
    if file_metadata.len()
        < u64::try_from(durable_byte_len)
            .map_err(|_| String::from("Session segment byte boundary exceeds u64 capacity."))?
    {
        return Err(format!(
            "Segment `{}` is shorter than metadata boundary: actual={} expected={durable_byte_len}.",
            segment_path.display(),
            file_metadata.len()
        ));
    }
    // file is opened only after exact type and size validation under the session read lock.
    // file 仅在会话读锁内完成精确类型与长度验证后打开。
    let mut file = File::open(segment_path).map_err(to_store_error)?;
    // bytes allocates only the metadata-authorized prefix, never the post-capture tail.
    // bytes 仅分配元数据授权的前缀，绝不分配捕获边界后的尾部。
    let mut bytes = vec![0_u8; durable_byte_len];
    file.read_exact(&mut bytes).map_err(to_store_error)?;
    Ok(bytes)
}

/// Captures a bounded raw-event segment suffix under the exact session read lock.
/// 在精确会话读锁下捕获有界的原始事件分段后缀。
///
/// # Parameters
/// 参数
/// `session_dir` is the deterministic session storage directory.
/// `session_dir` 是确定性的会话存储目录。
/// `before_seq` is the optional exclusive event cursor.
/// `before_seq` 是可选的独占事件游标。
/// `limit` is the maximum number of decoded rows requested by the caller.
/// `limit` 是调用方请求的最大解码记录数。
///
/// # Returns
/// 返回值
/// Exact immutable byte prefixes ordered oldest to newest, with JSON decoding deferred until after unlock.
/// 返回按从旧到新排序的精确不可变字节前缀，并把 JSON 解码延后到解锁之后。
fn prepare_segmented_event_window_read(
    session_dir: &Path,
    before_seq: Option<u64>,
    limit: usize,
) -> Result<Vec<PreparedSegmentBytes>, String> {
    // session_storage_lock coordinates only this exact root session during byte capture.
    // session_storage_lock 仅在字节捕获期间协调当前精确根会话。
    let session_storage_lock = segmented_append_file_io_lock(session_dir);
    // read_guard keeps metadata and its authorized byte prefixes mutually consistent.
    // read_guard 保持元数据及其授权字节前缀相互一致。
    let read_guard = session_storage_lock.read().map_err(to_store_error)?;
    // metadata is the sole file locator and byte-boundary authority.
    // metadata 是唯一的文件定位与字节边界权威。
    let metadata = read_session_metadata(session_dir)?;
    // cursor excludes all event sequences at or after the requested boundary.
    // cursor 排除请求边界及其之后的全部事件序号。
    let cursor = before_seq.unwrap_or(u64::MAX);
    // selected_start selects only the metadata suffix capable of satisfying the raw-row limit.
    // selected_start 仅选择能够满足原始记录上限的元数据后缀。
    let selected_start = select_segment_suffix_start_for_event_window(
        &metadata.segments,
        cursor,
        u64::try_from(limit).unwrap_or(u64::MAX),
    );
    // prepared contains exact immutable prefixes and never derives candidate paths by scanning.
    // prepared 包含精确不可变前缀，绝不通过扫描派生候选路径。
    let mut prepared = Vec::new();
    if let Some(selected_start) = selected_start {
        for segment in &metadata.segments[selected_start..] {
            if segment.start_seq >= cursor || segment.line_count == 0 {
                continue;
            }
            // segment_path comes only from the selected metadata row.
            // segment_path 仅来自选中的元数据记录。
            let segment_path = session_dir.join(&segment.file);
            // durable_byte_len is the exact published prefix length.
            // durable_byte_len 是已发布的精确前缀长度。
            let durable_byte_len = usize::try_from(segment.byte_len).map_err(to_store_error)?;
            // bytes are captured under lock; all decoding occurs after read_guard is dropped.
            // bytes 在锁内捕获；全部解码均在释放 read_guard 后进行。
            let bytes = read_exact_segment_prefix(&segment_path, durable_byte_len)?;
            prepared.push(PreparedSegmentBytes {
                path: segment_path,
                bytes,
            });
        }
    }
    drop(read_guard);
    Ok(prepared)
}

/// Selects the oldest metadata segment needed for one bounded raw-event window.
/// 选择一个有界原始事件窗口所需的最早元数据分段。
///
/// # Parameters
/// 参数
/// `segments` are authoritative metadata rows ordered oldest to newest.
/// `segments` 是按从旧到新排序的权威元数据记录。
/// `cursor` is the exclusive event sequence boundary.
/// `cursor` 是独占事件序号边界。
/// `limit` is the requested raw-event count.
/// `limit` 是请求的原始事件数量。
///
/// # Returns
/// 返回值
/// The inclusive suffix index plus one older safety segment, or `None` when no row precedes the cursor.
/// 返回包含式后缀索引并额外保留一个更早安全分段；游标前没有记录时返回 `None`。
fn select_segment_suffix_start_for_event_window(
    segments: &[SegmentMetadata],
    cursor: u64,
    limit: u64,
) -> Option<usize> {
    // selected_start tracks the oldest segment known to contain an eligible sequence.
    // selected_start 跟踪已知包含合格序号的最早分段。
    let mut selected_start = None;
    // eligible_rows accumulates the exact contiguous sequence capacity below the cursor.
    // eligible_rows 累积游标之前精确连续的序号容量。
    let mut eligible_rows = 0_u64;
    for (index, segment) in segments.iter().enumerate().rev() {
        if segment.start_seq >= cursor || segment.line_count == 0 {
            continue;
        }
        // eligible_last is the newest sequence from this segment that remains below the cursor.
        // eligible_last 是当前分段中仍早于游标的最新序号。
        let eligible_last = segment
            .last_seq
            .unwrap_or(segment.start_seq)
            .min(cursor.saturating_sub(1));
        if eligible_last < segment.start_seq {
            continue;
        }
        // segment_eligible_rows is clamped by physical line_count for legacy metadata safety.
        // segment_eligible_rows 受物理 line_count 限制，以保证旧元数据安全。
        let segment_eligible_rows = eligible_last
            .saturating_sub(segment.start_seq)
            .saturating_add(1)
            .min(segment.line_count);
        eligible_rows = eligible_rows.saturating_add(segment_eligible_rows);
        selected_start = Some(index);
        if eligible_rows >= limit.max(1) {
            // One older metadata segment preserves tolerant unknown-row behavior without full history reads.
            // 额外保留一个更早元数据分段，在不全量读取历史的前提下保持未知记录容错行为。
            return Some(index.saturating_sub(1));
        }
    }
    selected_start
}

/// Boundary and raw windows decoded from one stable prepared read.
/// 从同一个稳定预备读取中解码出的 Boundary 与 Raw 窗口。
pub(in crate::workspace_store) struct SegmentedCombinedTurnWindow {
    /// Raw message-bearing event rows in canonical ascending sequence order.
    /// 按规范升序排列的原始消息事件行。
    pub(in crate::workspace_store) raw_lines: Vec<SegmentedSessionEventLine>,
    /// Session-boundary event rows in canonical ascending sequence order.
    /// 按规范升序排列的会话边界事件行。
    pub(in crate::workspace_store) boundary_lines: Vec<SegmentedSessionEventLine>,
    /// Whether older raw display turns exist before this window.
    /// 当前窗口之前是否存在更早的原始展示轮次。
    pub(in crate::workspace_store) raw_has_more_history: bool,
    /// Whether older boundary turns exist before this window.
    /// 当前窗口之前是否存在更早的边界轮次。
    pub(in crate::workspace_store) boundary_has_more_history: bool,
    /// Storage generation shared by both projections.
    /// 两种投影共享的存储代次。
    pub(in crate::workspace_store) storage_generation: u64,
}

/// Captures exact segment byte prefixes under a short root-session read lock.
/// 在短时根会话读锁内捕获精确分段字节前缀。
///
/// # Parameters
/// 参数
/// `session_dir` is the deterministic session storage directory.
/// `session_dir` 是确定性的会话存储目录。
/// `before_seq` is the optional exclusive history cursor.
/// `before_seq` 是可选的历史独占游标。
///
/// # Returns
/// 返回值
/// A stable byte read plan whose JSON parsing can run after the storage lock is released.
/// 返回稳定字节读取计划，其 JSON 解析可在存储锁释放后运行。
fn prepare_segmented_session_read(
    session_dir: &Path,
    before_seq: Option<u64>,
    turn_limit: usize,
) -> Result<PreparedSessionRead, String> {
    // lock_wait_started_at measures contention before this session's immutable capture.
    // lock_wait_started_at 测量当前会话不可变捕获前的锁竞争。
    let lock_wait_started_at = std::time::Instant::now();
    // session_storage_lock coordinates only the exact root session.
    // session_storage_lock 仅协调精确根会话。
    let session_storage_lock = segmented_append_file_io_lock(session_dir);
    // read_guard prevents append while metadata and selected byte prefixes are captured.
    // read_guard 在捕获元数据与选中字节前缀期间阻止追加。
    let read_guard = session_storage_lock.read().map_err(to_store_error)?;
    let lock_wait_elapsed = lock_wait_started_at.elapsed();
    // lock_hold_started_at excludes time spent waiting for the read lock.
    // lock_hold_started_at 排除等待读锁所消耗的时间。
    let lock_hold_started_at = std::time::Instant::now();
    // metadata is read exactly once and defines every segment byte boundary below.
    // metadata 仅精确读取一次，并定义下方每个分段的字节边界。
    let metadata = read_session_metadata(session_dir)?;
    // cursor excludes segments whose first sequence is not older than the requested boundary.
    // cursor 排除首序号不早于请求边界的分段。
    let cursor = before_seq.unwrap_or(u64::MAX);
    // candidate_segments come only from exact metadata rows older than the requested cursor.
    // candidate_segments 仅来自早于请求游标的精确元数据行。
    let candidate_segments = metadata
        .segments
        .iter()
        .filter(|segment| segment.start_seq < cursor)
        .collect::<Vec<_>>();
    // boundary_index_complete prevents partial legacy boundary projection from changing raw order.
    // boundary_index_complete 防止局部旧版边界投影改变原始顺序。
    let boundary_index_complete = candidate_segments
        .iter()
        .all(|segment| segment.projection_index_version == 1);
    // selected_start_index bounds cold reads to the segments required by the requested turn window.
    // selected_start_index 将冷读取限制在请求轮次窗口需要的分段内。
    let selected_start_index = select_segment_suffix_start_for_turn_window(
        &candidate_segments,
        before_seq,
        turn_limit,
        boundary_index_complete,
    );
    let selected_segments = &candidate_segments[selected_start_index..];
    // segments retain only metadata-selected exact files and never scan the directory.
    // segments 仅保留元数据选择出的精确文件，绝不扫描目录。
    let mut segments = Vec::new();
    // bytes_read accumulates captured bytes for one diagnostic without another file operation.
    // bytes_read 累加本次捕获字节数，用于诊断且不增加文件操作。
    let mut bytes_read = 0_usize;
    for segment in selected_segments {
        // segment_path comes directly from the exact metadata row.
        // segment_path 直接来自精确元数据行。
        let segment_path = session_dir.join(&segment.file);
        // durable_byte_len comes only from the authoritative metadata row.
        // durable_byte_len 仅来自权威元数据行。
        let durable_byte_len = usize::try_from(segment.byte_len).map_err(to_store_error)?;
        // bytes capture exactly the durable prefix and never read a concurrently external tail.
        // bytes 精确捕获可靠前缀，绝不读取外部并发追加的尾部。
        let bytes = read_exact_segment_prefix(&segment_path, durable_byte_len)?;
        bytes_read = bytes_read.saturating_add(bytes.len());
        segments.push(PreparedSegmentBytes {
            path: segment_path,
            bytes,
        });
    }
    let storage_generation = metadata.storage_generation.max(metadata.next_global_seq);
    let rollback_floor_seq = metadata.latest_rollback_seq;
    let lock_hold_elapsed = lock_hold_started_at.elapsed();
    drop(read_guard);
    if lock_wait_elapsed.as_millis() > JSONL_READ_DIAGNOSTIC_THRESHOLD_MS
        || lock_hold_elapsed.as_millis() > JSONL_READ_DIAGNOSTIC_THRESHOLD_MS
    {
        eprintln!(
            "[workspace-store][prepared-session-read] storage_generation={storage_generation} candidate_segments={} selected_segments={} bytes={bytes_read} boundary_index_complete={boundary_index_complete} lock_wait_ms={} lock_hold_ms={}",
            candidate_segments.len(),
            segments.len(),
            lock_wait_elapsed.as_millis(),
            lock_hold_elapsed.as_millis()
        );
    }
    Ok(PreparedSessionRead {
        storage_generation,
        rollback_floor_seq,
        boundary_index_complete,
        segments,
    })
}

/// Selects the oldest segment needed for one recent-turn window from metadata only.
/// 仅根据元数据选择最近轮次窗口所需的最早分段。
///
/// # Parameters
/// 参数
/// `segments` are exact cursor-filtered metadata rows in ascending order.
/// `segments` 是按升序排列且已经过游标过滤的精确元数据行。
/// `before_seq` is the optional exclusive sequence cursor.
/// `before_seq` 是可选的独占序号游标。
/// `turn_limit` is the requested complete-turn count.
/// `turn_limit` 是请求的完整轮次数量。
/// `boundary_index_complete` states whether boundary counts are authoritative for every candidate.
/// `boundary_index_complete` 表示每个候选分段的边界计数是否都具有权威性。
///
/// # Returns
/// 返回值
/// The inclusive suffix start index. The function never invents a file path or scans a directory.
/// 返回后缀的包含式起始下标。该函数绝不构造候选文件路径或扫描目录。
fn select_segment_suffix_start_for_turn_window(
    segments: &[&SegmentMetadata],
    before_seq: Option<u64>,
    turn_limit: usize,
    boundary_index_complete: bool,
) -> usize {
    if segments.is_empty() {
        return 0;
    }
    let required_turns = u64::try_from(turn_limit.max(1))
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let required_raw_turns = i64::try_from(required_turns).unwrap_or(i64::MAX);
    let has_raw_turn_index = segments
        .iter()
        .any(|segment| segment.last_turn_index.is_some());
    let total_boundary_turns = if boundary_index_complete {
        indexed_boundary_turn_count(segments)
    } else {
        0
    };
    let required_boundary_turns = total_boundary_turns.min(required_turns);
    let newest_segment_contains_cursor = before_seq.is_some_and(|cursor| {
        segments.last().is_some_and(|segment| {
            segment
                .last_seq
                .is_some_and(|last_seq| cursor <= last_seq.saturating_add(1))
        })
    });
    let mut newest_raw_turn_index = None;
    let mut oldest_raw_turn_index = None;
    let mut selected_boundary_turns = 0_u64;
    let mut newer_first_boundary_turn_id: Option<&str> = None;
    let mut selected_start_index = segments.len() - 1;
    let mut needs_cursor_safety_segment = newest_segment_contains_cursor;
    for (index, segment) in segments.iter().enumerate().rev() {
        selected_start_index = index;
        if let Some(last_turn_index) = segment.last_turn_index {
            newest_raw_turn_index.get_or_insert(last_turn_index);
            oldest_raw_turn_index = Some(segment.first_turn_index.unwrap_or(last_turn_index));
        }
        if boundary_index_complete {
            let duplicate_across_boundary = segment
                .last_session_boundary_turn_id
                .as_deref()
                .zip(newer_first_boundary_turn_id)
                .is_some_and(|(older_last, newer_first)| older_last == newer_first);
            selected_boundary_turns = selected_boundary_turns
                .saturating_add(segment.session_boundary_turn_count)
                .saturating_sub(u64::from(duplicate_across_boundary));
            newer_first_boundary_turn_id = segment.first_session_boundary_turn_id.as_deref();
        }
        let raw_turns = newest_raw_turn_index
            .zip(oldest_raw_turn_index)
            .map(|(newest, oldest)| newest.saturating_sub(oldest).saturating_add(1))
            .unwrap_or_default();
        // raw_sufficient never expands a fully indexed Boundary window for a compacted resident tail.
        // raw_sufficient 绝不会因为压缩后的常驻尾部而扩大索引完整的 Boundary 窗口。
        let raw_sufficient =
            boundary_index_complete || !has_raw_turn_index || raw_turns >= required_raw_turns;
        let boundary_sufficient =
            !boundary_index_complete || selected_boundary_turns >= required_boundary_turns;
        if raw_sufficient && boundary_sufficient {
            if needs_cursor_safety_segment && index > 0 {
                needs_cursor_safety_segment = false;
                continue;
            }
            break;
        }
    }
    selected_start_index
}

/// Counts distinct adjacent boundary turns across ordered indexed segments.
/// 统计有序索引分段中的相邻去重边界轮次。
///
/// # Parameters
/// 参数
/// `segments` are metadata rows whose projection index version is already verified.
/// `segments` 是投影索引版本已经验证的元数据行。
///
/// # Returns
/// 返回值
/// The exact adjacent-deduplicated boundary turn count represented by the metadata rows.
/// 返回元数据行表示的精确相邻去重边界轮次数量。
fn indexed_boundary_turn_count(segments: &[&SegmentMetadata]) -> u64 {
    let mut total = 0_u64;
    let mut previous_last_turn_id: Option<&str> = None;
    for segment in segments {
        let duplicate_across_boundary = previous_last_turn_id
            .zip(segment.first_session_boundary_turn_id.as_deref())
            .is_some_and(|(older_last, newer_first)| older_last == newer_first);
        total = total
            .saturating_add(segment.session_boundary_turn_count)
            .saturating_sub(u64::from(duplicate_across_boundary));
        previous_last_turn_id = segment.last_session_boundary_turn_id.as_deref();
    }
    total
}

/// Decodes boundary and raw turn windows in one reverse pass over one prepared snapshot.
/// 在同一个预备快照上通过一次倒序扫描解码 Boundary 与 Raw 轮次窗口。
///
/// # Parameters
/// 参数
/// `session_dir` is the deterministic session storage directory.
/// `session_dir` 是确定性的会话存储目录。
/// `before_seq` is the optional exclusive history cursor.
/// `before_seq` 是可选的历史独占游标。
/// `turn_limit` is the maximum complete turn count retained per projection.
/// `turn_limit` 是每种投影保留的最大完整轮次数。
///
/// # Returns
/// 返回值
/// Two same-generation windows produced without a second segment read.
/// 返回无需第二次分段读取而生成的两个同代次窗口。
pub(in crate::workspace_store) fn read_segmented_session_combined_turn_window(
    session_dir: &Path,
    before_seq: Option<u64>,
    turn_limit: usize,
) -> Result<SegmentedCombinedTurnWindow, String> {
    let turn_limit = turn_limit.max(1);
    // prepared captures exact bytes before any JSON parsing begins.
    // prepared 在任何 JSON 解析开始前捕获精确字节。
    let prepared = prepare_segmented_session_read(session_dir, before_seq, turn_limit)?;
    let cursor = before_seq.unwrap_or(u64::MAX);
    // raw_lines_reversed accumulates selected raw rows while scanning newest to oldest.
    // raw_lines_reversed 在从新到旧扫描时累积选中的原始行。
    let mut raw_lines_reversed = Vec::new();
    // boundary_lines_reversed accumulates selected boundary rows from the same scan.
    // boundary_lines_reversed 从同一扫描中累积选中的边界行。
    let mut boundary_lines_reversed = Vec::new();
    let mut seen_raw_turn_indexes = BTreeSet::new();
    let mut seen_boundary_turn_ids = BTreeSet::new();
    let mut raw_has_more_history = false;
    let mut boundary_has_more_history = false;
    'segments: for prepared_segment in prepared.segments.iter().rev() {
        let newline_count = prepared_segment
            .bytes
            .iter()
            .filter(|byte| **byte == b'\n')
            .count();
        for (reverse_index, raw_line) in prepared_segment
            .bytes
            .rsplit(|byte| *byte == b'\n')
            .enumerate()
        {
            if jsonl_line_is_blank(raw_line) {
                continue;
            }
            let line_number = newline_count
                .saturating_add(1)
                .saturating_sub(reverse_index);
            let Some(line) =
                decode_segment_event_line_tolerant(raw_line, &prepared_segment.path, line_number)?
            else {
                continue;
            };
            if line.seq >= cursor {
                continue;
            }
            if prepared.boundary_index_complete
                && !boundary_has_more_history
                && line.kind == SegmentedSessionEventKind::SessionBoundary
            {
                let boundary_turn_id = line
                    .payload
                    .get("turn_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                if let Some(boundary_turn_id) = boundary_turn_id {
                    if seen_boundary_turn_ids.len() >= turn_limit
                        && !seen_boundary_turn_ids.contains(boundary_turn_id)
                    {
                        boundary_has_more_history = true;
                    } else {
                        seen_boundary_turn_ids.insert(boundary_turn_id.to_string());
                        boundary_lines_reversed.push(line.clone());
                    }
                } else {
                    boundary_lines_reversed.push(line.clone());
                }
            }
            if !raw_has_more_history
                && !prepared
                    .rollback_floor_seq
                    .is_some_and(|rollback_seq| line.seq <= rollback_seq)
                && let Some(turn_index) = segmented_line_turn_index(&line)
            {
                if seen_raw_turn_indexes.len() >= turn_limit
                    && !seen_raw_turn_indexes.contains(&turn_index)
                {
                    raw_has_more_history = true;
                } else {
                    seen_raw_turn_indexes.insert(turn_index);
                    raw_lines_reversed.push(line);
                }
            }
            // A complete Boundary index is the canonical turn envelope; older Raw rows cannot enrich this page.
            // 完整 Boundary 索引是规范轮次范围；更早的 Raw 记录无法增强当前分页。
            if prepared.boundary_index_complete && boundary_has_more_history {
                break 'segments;
            }
        }
    }
    raw_lines_reversed.reverse();
    boundary_lines_reversed.reverse();
    Ok(SegmentedCombinedTurnWindow {
        raw_lines: raw_lines_reversed,
        boundary_lines: boundary_lines_reversed,
        raw_has_more_history,
        boundary_has_more_history,
        storage_generation: prepared.storage_generation,
    })
}

/// Reads a backward transcript window by complete display turns instead of raw event count.
/// 按完整展示轮次而不是原始事件数量读取反向转录窗口。
#[cfg(test)]
pub(in crate::workspace_store) fn read_segmented_session_turn_window(
    session_dir: &Path,
    before_seq: Option<u64>,
    turn_limit: usize,
) -> Result<SegmentedSessionTurnWindow, String> {
    if turn_limit == 0 {
        return Ok(SegmentedSessionTurnWindow {
            lines: Vec::new(),
            has_more_history: false,
        });
    }
    let combined =
        read_segmented_session_combined_turn_window(session_dir, before_seq, turn_limit)?;
    Ok(SegmentedSessionTurnWindow {
        lines: combined.raw_lines,
        has_more_history: combined.raw_has_more_history,
    })
}

/// Projects segmented JSONL events back into displayable transcript messages.
/// 将分段 JSONL 事件投影回可展示的转录消息。
pub(in crate::workspace_store) fn segmented_lines_to_session_messages(
    lines: &[SegmentedSessionEventLine],
) -> Result<Vec<SessionMessageEntry>, String> {
    let mut messages = Vec::new();
    let mut message_indexes = std::collections::BTreeMap::new();
    for line in lines {
        if let Some(message) = segmented_line_to_session_message(line)? {
            if let Some(index) = message_indexes.get(message.id.as_str()).copied() {
                // Later duplicate identities are historical lifecycle revisions and replace the row in place.
                // 后出现的重复身份属于历史生命周期修订，应在原位置替换该记录。
                messages[index] = message;
            } else {
                message_indexes.insert(message.id.clone(), messages.len());
                messages.push(message);
            }
        }
    }
    restore_tool_card_terminal_projection(&mut messages, &[]);
    Ok(messages)
}

/// Restores mutable visible tool-card terminal metadata from immutable persisted result evidence.
/// 根据不可变的持久化结果证据恢复可变可见工具卡片的终态元数据。
///
/// # Parameters
/// 参数
/// `messages` is one canonical replay window containing visible tool cards and hidden result details.
/// `messages` 是包含可见工具卡片与隐藏结果详情的规范重放窗口。
/// `additional_detail_messages` contains hidden detail rows loaded from runtime-detail side channels.
/// `additional_detail_messages` 包含从运行时详情旁路载入的隐藏详情记录。
///
/// # Returns
/// 返回值
/// This function returns no value after applying deterministic in-memory projection updates.
/// 本函数应用确定性的内存投影更新后不返回值。
pub(in crate::workspace_store) fn restore_tool_card_terminal_projection(
    messages: &mut [SessionMessageEntry],
    additional_detail_messages: &[SessionMessageEntry],
) {
    // toolCardIndexes binds each stable call identity to its visible card inside this replay window.
    // toolCardIndexes 将每个稳定调用身份绑定到当前重放窗口内的可见卡片。
    let mut tool_card_indexes = std::collections::BTreeMap::new();
    for (index, message) in messages.iter_mut().enumerate() {
        if message.record_kind != SessionMessageRecordKind::ToolCallOutput {
            continue;
        }
        let Some(serde_json::Value::Object(metadata)) = message.metadata.as_mut() else {
            continue;
        };
        let Some(tool_call_id) = metadata
            .get("tool_call_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        // persistedResultStatus repairs older duplicate revisions whose nested result was terminal but top-level state was stale.
        // persistedResultStatus 修复旧重复修订中结果已终结但顶层状态仍过期的问题。
        let persisted_result_status = metadata
            .get("result")
            .and_then(|result| result.get("status"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        if let Some(status) = persisted_result_status {
            metadata.insert(
                String::from("tool_display_state"),
                serde_json::Value::String(status),
            );
        }
        tool_card_indexes.insert(tool_call_id, index);
    }

    // terminalDetails separates immutable evidence discovery from mutable card updates.
    // terminalDetails 将不可变证据发现与可变卡片更新分离。
    // seenDetailRecordIds prevents side-channel copies from overwriting an exact in-window coordinate.
    // seenDetailRecordIds 防止旁路副本覆盖窗口内的精确坐标。
    let mut seen_detail_record_ids = std::collections::BTreeSet::new();
    let mut terminal_details = Vec::new();
    for (message_index, message) in messages.iter().enumerate() {
        let terminal_detail = (|| {
            if message.record_kind != SessionMessageRecordKind::ToolDetailOutput {
                return None;
            }
            if !seen_detail_record_ids.insert(message.id.clone()) {
                return None;
            }
            let metadata = message.metadata.as_ref()?;
            let tool_call_id = metadata.get("tool_call_id")?.as_str()?;
            let display = metadata.get("display")?.clone();
            let status = display.get("status")?.as_str()?;
            let card_index = *tool_card_indexes.get(tool_call_id)?;
            Some((
                card_index,
                message.id.clone(),
                Some(message_index),
                status.to_string(),
                metadata.get("arguments").cloned(),
                metadata.get("host_result").cloned(),
                Some(display),
            ))
        })();
        if let Some(terminal_detail) = terminal_detail {
            terminal_details.push(terminal_detail);
        }
    }
    for message in additional_detail_messages {
        let terminal_detail = (|| {
            if message.record_kind != SessionMessageRecordKind::ToolDetailOutput {
                return None;
            }
            if !seen_detail_record_ids.insert(message.id.clone()) {
                return None;
            }
            let metadata = message.metadata.as_ref()?;
            let tool_call_id = metadata.get("tool_call_id")?.as_str()?;
            let display = metadata.get("display")?.clone();
            let status = display.get("status")?.as_str()?;
            let card_index = *tool_card_indexes.get(tool_call_id)?;
            Some((
                card_index,
                message.id.clone(),
                None,
                status.to_string(),
                metadata.get("arguments").cloned(),
                metadata.get("host_result").cloned(),
                Some(display),
            ))
        })();
        if let Some(terminal_detail) = terminal_detail {
            terminal_details.push(terminal_detail);
        }
    }
    for (
        card_index,
        detail_record_id,
        detail_message_index,
        status,
        arguments,
        persisted_host_result,
        display,
    ) in terminal_details
    {
        let Some(serde_json::Value::Object(metadata)) = messages
            .get_mut(card_index)
            .and_then(|message| message.metadata.as_mut())
        else {
            continue;
        };
        metadata.insert(
            String::from("tool_display_state"),
            serde_json::Value::String(status.clone()),
        );
        // arguments and host_result rebuild the bounded presentation metadata carried by the live terminal card.
        // arguments 与 host_result 重建实时终态卡片携带的有界展示元数据。
        if let Some(arguments) = arguments {
            metadata
                .entry(String::from("arguments"))
                .or_insert(arguments);
        }
        if let Some(persisted_host_result) = persisted_host_result {
            // persistedHostResult is authoritative; the display wrapper is only a legacy fallback.
            // persistedHostResult 是权威数据；display 包装仅用于旧记录回退。
            metadata.insert(String::from("host_result"), persisted_host_result);
        } else if let Some(display) = display
            && let Some(kind) = display
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        {
            metadata
                .entry(String::from("host_result"))
                .or_insert_with(|| {
                    json!({
                        "kind": kind,
                        "payload": display,
                    })
                });
        }
        // result preserves any already-persisted fields while attaching the immutable detail lookup.
        // result 保留已经持久化的字段，同时挂接不可变详情定位信息。
        let result = metadata
            .entry(String::from("result"))
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        let serde_json::Value::Object(result) = result else {
            continue;
        };
        result.insert(String::from("status"), serde_json::Value::String(status));
        // detailRef always carries stable identity and includes an index only when the exact window coordinate is known.
        // detailRef 始终携带稳定身份，仅在精确窗口坐标已知时包含索引。
        let mut detail_ref = serde_json::Map::new();
        detail_ref.insert(
            String::from("kind"),
            serde_json::Value::String(String::from("session_record")),
        );
        detail_ref.insert(
            String::from("record_id"),
            serde_json::Value::String(detail_record_id),
        );
        if let Some(detail_message_index) = detail_message_index {
            detail_ref.insert(
                String::from("message_index"),
                serde_json::Value::Number(serde_json::Number::from(detail_message_index as u64)),
            );
        }
        result.insert(
            String::from("detail_ref"),
            serde_json::Value::Object(detail_ref),
        );
    }
}

/// Rebuilds the current conversation state from segmented compaction history and suffix events.
/// 根据分段压缩历史与后缀事件重建当前会话状态。
pub(in crate::workspace_store) fn replay_segmented_session_context(
    session_dir: &Path,
) -> Result<Option<SegmentedSessionReplayResult>, String> {
    // session_storage_lock coordinates only this root session while its immutable bytes are captured.
    // session_storage_lock 仅在捕获当前根会话不可变字节期间进行协调。
    let session_storage_lock = segmented_append_file_io_lock(session_dir);
    let _guard = session_storage_lock.read().map_err(to_store_error)?;
    replay_segmented_session_context_without_repair(session_dir)
}

/// Rebuilds conversation state from an immutable segmented snapshot without repairing storage.
/// 从不可变分段快照重建会话状态，且不修复存储。
pub(in crate::workspace_store) fn replay_segmented_session_context_without_repair(
    session_dir: &Path,
) -> Result<Option<SegmentedSessionReplayResult>, String> {
    let metadata = read_session_metadata(session_dir)?;
    let scan = find_latest_compaction_anchor(session_dir, &metadata.segments)?;
    let latest_rollback_seq = scan.rollback_markers.iter().map(|marker| marker.seq).max();
    let mut messages = match scan.anchor.as_ref() {
        Some(anchor) if compaction_anchor_is_replay_baseline(anchor, latest_rollback_seq) => {
            replacement_history_to_session_messages(&anchor.replacement_history)?
        }
        _ => Vec::new(),
    };
    let suffix = suffix_after_latest_rollback(&scan);
    messages.extend(segmented_lines_to_session_messages(&suffix)?);
    restore_tool_card_terminal_projection(&mut messages, &[]);
    if messages.is_empty() {
        return Ok(None);
    }
    let conversation_state = conversation_state_from_session_messages(&messages)?;
    let updated_at = suffix
        .last()
        .map(|line| line.timestamp.clone())
        .or_else(|| {
            scan.anchor
                .as_ref()
                .map(|anchor| anchor.anchor_seq.to_string())
        });
    let last_compaction_id = scan
        .anchor
        .as_ref()
        .filter(|anchor| anchor.anchor_kind == SegmentedSessionEventKind::Compacted)
        .map(|anchor| format!("segmented-compaction-{}", anchor.anchor_seq));
    let context_start_message_id = messages
        .first()
        .and_then(|message| (!message.id.trim().is_empty()).then(|| message.id.clone()));
    Ok(Some(SegmentedSessionReplayResult {
        conversation_state,
        last_compaction_id,
        context_start_message_id,
        updated_at,
    }))
}

/// Returns whether one replacement-history anchor remains the replay baseline after rollback markers.
/// 返回单个 replacement-history 锚点在回滚标记之后是否仍可作为重放基线。
fn compaction_anchor_is_replay_baseline(
    anchor: &SegmentedCompactionAnchor,
    latest_rollback_seq: Option<u64>,
) -> bool {
    match anchor.anchor_kind {
        SegmentedSessionEventKind::Compacted => true,
        SegmentedSessionEventKind::ThreadRolledBack => {
            latest_rollback_seq.is_none_or(|rollback_seq| rollback_seq <= anchor.anchor_seq)
        }
        _ => false,
    }
}

/// Rebuilds one bloated display transcript from its effective replacement-anchor view when needed.
/// 在需要时根据有效 replacement 锚点视图重建一个膨胀的显示转录。
pub(in crate::workspace_store) fn repair_segmented_session_display_if_needed(
    session_dir: &Path,
) -> Result<Option<SegmentedDisplayMaintenanceReport>, String> {
    // session_storage_lock grants exclusive ownership for explicit maintenance mutation.
    // session_storage_lock 为显式维护修改授予独占所有权。
    let session_storage_lock = segmented_append_file_io_lock(session_dir);
    let _guard = session_storage_lock.lock().map_err(to_store_error)?;
    repair_segmented_session_jsonl_files_if_needed_locked(session_dir)?;
    let metadata = read_session_metadata(session_dir)?;
    if !segmented_display_needs_maintenance(&metadata) {
        return Ok(None);
    }
    let Some(replay) = replay_segmented_session_context_without_repair(session_dir)? else {
        return Ok(None);
    };
    let messages = conversation_state_to_session_messages(&replay.conversation_state);
    if messages.is_empty() {
        return Ok(None);
    }
    let retained_non_message_events =
        retained_non_message_events_after_latest_anchor(session_dir, &metadata)?;
    let original_segment_count = metadata.segments.len();
    let reason = String::from("replacement_history_rollback_bloat");
    let repaired_build = build_repaired_display_metadata_in_temp_dir(
        session_dir,
        &metadata,
        &messages,
        &retained_non_message_events,
    )?;
    let backup_dir = backup_segmented_session_before_display_maintenance(session_dir, &reason)?;
    let report = SegmentedDisplayMaintenanceReport {
        reason,
        backup_dir: vulcan_runtime_assets::native_host_path_text_from_path(&backup_dir),
        original_segment_count,
        rebuilt_segment_count: repaired_build.metadata.segments.len(),
        effective_message_count: messages.len(),
    };
    write_json_file_atomic(&backup_dir.join("maintenance_report.json"), &report)?;
    replace_display_segments_with_repaired_metadata(
        session_dir,
        &metadata,
        &repaired_build,
        &backup_dir.join("original"),
    )?;
    Ok(Some(report))
}

/// Returns whether one metadata snapshot shows display rollback bloat that should be normalized.
/// 返回一个元数据快照是否显示需要规整的显示层回滚膨胀。
pub(in crate::workspace_store) fn segmented_display_needs_maintenance(
    metadata: &SegmentedSessionMetadata,
) -> bool {
    if metadata.latest_rollback_seq.is_none() {
        return false;
    }
    let total_segment_bytes = metadata
        .segments
        .iter()
        .map(|segment| segment.byte_len)
        .sum::<u64>();
    let maintenance_threshold = metadata.segment_policy.max_bytes.saturating_mul(2);
    total_segment_bytes > maintenance_threshold
}

/// Builds repaired display segment files in an isolated temporary directory.
/// 在隔离的临时目录中构建修复后的显示层分段文件。
pub(in crate::workspace_store) fn build_repaired_display_metadata_in_temp_dir(
    session_dir: &Path,
    metadata: &SegmentedSessionMetadata,
    messages: &[SessionMessageEntry],
    retained_non_message_events: &[SegmentedSessionEventInput],
) -> Result<RepairedDisplayBuild, String> {
    let temp_dir = display_maintenance_temp_dir(session_dir);
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).map_err(to_store_error)?;
    }
    create_dir_all(&temp_dir)?;
    let mut repaired_metadata = metadata.clone();
    repaired_metadata.active_segment = segment_file_name(1);
    repaired_metadata.next_global_seq = 0;
    repaired_metadata.message_count = 0;
    repaired_metadata.last_message_id = None;
    repaired_metadata.last_message_fingerprint = None;
    repaired_metadata.latest_rollback_seq = None;
    repaired_metadata.segments = vec![SegmentMetadata::empty(segment_file_name(1), 0)];
    repaired_metadata.updated_at = current_unix_millis_text();
    let session_meta_event = SegmentedSessionEventInput {
        timestamp: repaired_metadata.updated_at.clone(),
        kind: SegmentedSessionEventKind::SessionMeta,
        payload: json!({
            "session_id": repaired_metadata.session_id.clone(),
            "project_tag": repaired_metadata.project_tag.clone(),
            "canonical_source": repaired_metadata.canonical_source.clone(),
            "maintenance": "display_rebuild"
        }),
        turn_index: None,
    };
    append_events_with_metadata(&temp_dir, &mut repaired_metadata, &[session_meta_event])?;
    let turn_indexes = assign_segmented_message_turn_indexes(messages);
    let events = messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            segmented_event_input_from_message(index, message, turn_indexes[index])
        })
        .collect::<Result<Vec<_>, _>>()?;
    append_events_with_metadata(&temp_dir, &mut repaired_metadata, &events)?;
    append_events_with_metadata(
        &temp_dir,
        &mut repaired_metadata,
        retained_non_message_events,
    )?;
    update_segmented_message_checkpoint(&mut repaired_metadata, messages)?;
    write_metadata(&temp_dir, &repaired_metadata)?;
    Ok(RepairedDisplayBuild {
        metadata: repaired_metadata,
        temp_dir,
    })
}

/// Extracts effective non-message events that should survive display transcript maintenance.
/// 提取显示转录维护后仍应保留的有效非消息事件。
pub(in crate::workspace_store) fn retained_non_message_events_after_latest_anchor(
    session_dir: &Path,
    metadata: &SegmentedSessionMetadata,
) -> Result<Vec<SegmentedSessionEventInput>, String> {
    let scan = find_latest_compaction_anchor(session_dir, &metadata.segments)?;
    let suffix = suffix_after_latest_rollback(&scan);
    Ok(suffix
        .into_iter()
        .filter(|line| line.payload.get("message").is_none())
        .filter(|line| line.kind != SegmentedSessionEventKind::SessionMeta)
        .map(|line| SegmentedSessionEventInput {
            timestamp: line.timestamp,
            kind: line.kind,
            payload: line.payload,
            turn_index: None,
        })
        .collect())
}

/// Copies the original session directory into an audit backup before display maintenance.
/// 在显示层维护前将原始会话目录复制到审计备份中。
pub(in crate::workspace_store) fn backup_segmented_session_before_display_maintenance(
    session_dir: &Path,
    reason: &str,
) -> Result<PathBuf, String> {
    let parent = session_dir
        .parent()
        .ok_or_else(|| format!("Session directory has no parent: {}", session_dir.display()))?;
    let session_name = session_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("session");
    let backup_root = parent
        .join(SEGMENTED_DISPLAY_MAINTENANCE_BACKUP_DIR)
        .join(session_name);
    create_dir_all(&backup_root)?;
    let backup_dir = backup_root.join(format!(
        "{}-{}-{}",
        current_unix_millis_text(),
        sanitize_file_component(reason),
        Uuid::new_v4()
    ));
    let original_dir = backup_dir.join("original");
    create_dir_all(&original_dir)?;
    let metadata = read_session_metadata(session_dir)?;
    fs::copy(
        session_dir.join(SEGMENTED_METADATA_FILE),
        original_dir.join(SEGMENTED_METADATA_FILE),
    )
    .map_err(to_store_error)?;
    for segment in &metadata.segments {
        fs::copy(
            session_dir.join(&segment.file),
            original_dir.join(&segment.file),
        )
        .map_err(to_store_error)?;
    }
    Ok(backup_dir)
}

/// Replaces physical display segment files after repaired files were built and the original was backed up.
/// 在修复文件构建完成且原始文件已备份后替换物理显示分段文件。
pub(in crate::workspace_store) fn replace_display_segments_with_repaired_metadata(
    session_dir: &Path,
    original_metadata: &SegmentedSessionMetadata,
    repaired_build: &RepairedDisplayBuild,
    original_backup_dir: &Path,
) -> Result<(), String> {
    let replace_result = (|| {
        let temp_token = Uuid::new_v4().to_string();
        let mut staged_files = Vec::new();
        for segment in &repaired_build.metadata.segments {
            let source = repaired_build.temp_dir.join(&segment.file);
            let staged = session_dir.join(format!(".{}.{}.tmp", segment.file, temp_token));
            fs::copy(&source, &staged).map_err(to_store_error)?;
            staged_files.push((staged, session_dir.join(&segment.file)));
        }
        for segment in &original_metadata.segments {
            let path = session_dir.join(&segment.file);
            if path.exists() {
                fs::remove_file(path).map_err(to_store_error)?;
            }
        }
        for (staged, final_path) in staged_files {
            fs::rename(&staged, &final_path).map_err(to_store_error)?;
        }
        write_metadata(session_dir, &repaired_build.metadata)?;
        Ok::<(), String>(())
    })();
    if let Err(error) = replace_result {
        let restore_result = restore_display_segments_from_backup(session_dir, original_backup_dir);
        return match restore_result {
            Ok(()) => Err(format!(
                "display maintenance replacement failed and original files were restored: {error}"
            )),
            Err(restore_error) => Err(format!(
                "display maintenance replacement failed: {error}; restore also failed: {restore_error}"
            )),
        };
    }
    if repaired_build.temp_dir.exists() {
        let _ = fs::remove_dir_all(&repaired_build.temp_dir);
    }
    Ok(())
}

/// Restores root display segment files and metadata from one maintenance backup.
/// 从一个维护备份恢复根显示分段文件与元数据。
pub(in crate::workspace_store) fn restore_display_segments_from_backup(
    session_dir: &Path,
    original_backup_dir: &Path,
) -> Result<(), String> {
    let backup_metadata = read_required_json::<SegmentedSessionMetadata>(
        &original_backup_dir.join(SEGMENTED_METADATA_FILE),
    )?;
    remove_root_display_segment_files(session_dir, &backup_metadata.segments)?;
    for segment in &backup_metadata.segments {
        fs::copy(
            original_backup_dir.join(&segment.file),
            session_dir.join(&segment.file),
        )
        .map_err(to_store_error)?;
    }
    write_metadata(session_dir, &backup_metadata)
}

/// Removes root display segment files without touching runtime, task, blob, or backup directories.
/// 删除根显示分段文件，但不触碰 runtime、task、blob 或备份目录。
pub(in crate::workspace_store) fn remove_root_display_segment_files(
    session_dir: &Path,
    segments: &[SegmentMetadata],
) -> Result<(), String> {
    for segment in segments {
        let path = session_dir.join(&segment.file);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Failed to remove exact display segment {}: {error}",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

/// Builds a unique temporary directory path for display maintenance output.
/// 构建显示层维护输出使用的唯一临时目录路径。
pub(in crate::workspace_store) fn display_maintenance_temp_dir(session_dir: &Path) -> PathBuf {
    let session_name = session_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("session");
    session_dir.with_file_name(format!(
        ".{}.display_rebuild_tmp_{}",
        sanitize_file_component(session_name),
        Uuid::new_v4()
    ))
}

#[cfg(test)]
mod prepared_read_tests {
    use super::*;

    /// Verifies historical duplicate message identities keep their first position and latest revision.
    /// 验证历史重复消息身份会保留首次位置并采用最后版本。
    #[test]
    fn segmented_message_projection_uses_latest_duplicate_revision() {
        let mut ready = SessionMessageEntry::new(SessionMessageKind::Assistant, "", "")
            .with_runtime_metadata("tool-card", "1000");
        ready.record_kind = SessionMessageRecordKind::ToolCallOutput;
        ready.metadata = Some(json!({
            "tool_call_id": "call-1",
            "tool_display_state": "ready",
        }));
        let mut completed = ready.clone();
        completed.metadata = Some(json!({
            "tool_call_id": "call-1",
            "tool_display_state": "running",
            "result": { "status": "completed" },
        }));
        let ready_event =
            segmented_event_input_from_message(0, &ready, 0).expect("ready event payload");
        let ready_line = SegmentedSessionEventLine {
            seq: 1,
            timestamp: ready_event.timestamp,
            kind: ready_event.kind,
            payload: ready_event.payload,
        };
        let completed_event =
            segmented_event_input_from_message(1, &completed, 0).expect("completed event payload");
        let completed_line = SegmentedSessionEventLine {
            seq: 2,
            timestamp: completed_event.timestamp,
            kind: completed_event.kind,
            payload: completed_event.payload,
        };

        let projected = segmented_lines_to_session_messages(&[ready_line, completed_line])
            .expect("project duplicate lifecycle revisions");

        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].id, "tool-card");
        assert_eq!(
            projected[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("tool_display_state"))
                .and_then(serde_json::Value::as_str),
            Some("completed")
        );
        assert_eq!(
            projected[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("result"))
                .and_then(|result| result.get("status"))
                .and_then(serde_json::Value::as_str),
            Some("completed")
        );
    }

    /// Verifies an immutable hidden result restores the terminal state of its persisted ready card.
    /// 验证不可变隐藏结果会恢复其已持久化就绪卡片的终态。
    #[test]
    fn segmented_message_projection_restores_terminal_tool_card_from_detail() {
        // ready is the immutable visible request card persisted before host execution starts.
        // ready 是宿主执行开始前持久化的不可变可见请求卡片。
        let mut ready = SessionMessageEntry::new(SessionMessageKind::Assistant, "", "")
            .with_runtime_metadata("tool-card", "1000");
        ready.record_kind = SessionMessageRecordKind::ToolCallOutput;
        ready.metadata = Some(json!({
            "tool_call_id": "call-1",
            "tool_display_state": "ready",
        }));
        // detail is the immutable terminal evidence appended after execution finishes.
        // detail 是执行完成后追加的不可变终态证据。
        let mut detail = SessionMessageEntry::new(SessionMessageKind::Assistant, "", "result")
            .with_runtime_metadata("tool-detail", "1001");
        detail.record_kind = SessionMessageRecordKind::ToolDetailOutput;
        detail.visible = false;
        detail.metadata = Some(json!({
            "tool_detail": true,
            "tool_call_id": "call-1",
            "arguments": {
                "command": "rtk cargo check"
            },
            "display": {
                "kind": "shell_command",
                "status": "completed",
                "command": "rtk cargo check",
                "exit_code": 0,
            },
        }));
        // readyEvent and detailEvent are the exact immutable JSONL rows projected by storage.
        // readyEvent 与 detailEvent 是存储投影的精确不可变 JSONL 记录。
        let ready_event =
            segmented_event_input_from_message(0, &ready, 0).expect("ready event payload");
        let detail_event =
            segmented_event_input_from_message(1, &detail, 0).expect("detail event payload");
        // projected reconstructs the live terminal card without rewriting either persisted row.
        // projected 在不改写任何持久化记录的情况下重建实时终态卡片。
        let projected = segmented_lines_to_session_messages(&[
            SegmentedSessionEventLine {
                seq: 1,
                timestamp: ready_event.timestamp,
                kind: ready_event.kind,
                payload: ready_event.payload,
            },
            SegmentedSessionEventLine {
                seq: 2,
                timestamp: detail_event.timestamp,
                kind: detail_event.kind,
                payload: detail_event.payload,
            },
        ])
        .expect("project immutable tool lifecycle rows");

        assert_eq!(projected.len(), 2);
        assert_eq!(
            projected[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("tool_display_state"))
                .and_then(serde_json::Value::as_str),
            Some("completed")
        );
        assert_eq!(
            projected[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("result"))
                .and_then(|result| result.get("detail_ref"))
                .and_then(|detail_ref| detail_ref.get("record_id"))
                .and_then(serde_json::Value::as_str),
            Some("tool-detail")
        );
        assert_eq!(
            projected[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("arguments"))
                .and_then(|arguments| arguments.get("command"))
                .and_then(serde_json::Value::as_str),
            Some("rtk cargo check")
        );
        assert_eq!(
            projected[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("host_result"))
                .and_then(|host_result| host_result.get("payload"))
                .and_then(|payload| payload.get("command"))
                .and_then(serde_json::Value::as_str),
            Some("rtk cargo check")
        );
        // visibleOnly proves a separately hydrated hidden-detail collection restores the same card.
        // visibleOnly 证明单独水合的隐藏详情集合也会恢复同一张卡片。
        let mut visible_only = vec![ready];
        restore_tool_card_terminal_projection(&mut visible_only, std::slice::from_ref(&detail));
        assert_eq!(
            visible_only[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("tool_display_state"))
                .and_then(serde_json::Value::as_str),
            Some("completed")
        );
        assert!(
            visible_only[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("result"))
                .and_then(|result| result.get("detail_ref"))
                .and_then(|detail_ref| detail_ref.get("message_index"))
                .is_none()
        );
        assert_eq!(
            visible_only[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("host_result"))
                .and_then(|host_result| host_result.get("payload"))
                .and_then(|payload| payload.get("command"))
                .and_then(serde_json::Value::as_str),
            Some("rtk cargo check")
        );
    }

    /// Verifies an authoritative persisted change_set wins over the legacy display fallback during replay.
    /// 验证重放时权威持久化 change_set 优先于旧 display 回退数据。
    #[test]
    fn terminal_tool_card_replay_restores_persisted_change_set() {
        let mut ready = SessionMessageEntry::new(SessionMessageKind::Assistant, "", "")
            .with_runtime_metadata("tool-card-change", "2000");
        ready.record_kind = SessionMessageRecordKind::ToolCallOutput;
        ready.metadata = Some(json!({
            "tool_call_id": "call-change",
            "tool_display_state": "ready",
            "host_result": {
                "kind": "file_edit",
                "payload": {"summary": "stale display fallback"}
            },
        }));
        let mut detail = SessionMessageEntry::new(SessionMessageKind::Assistant, "", "result")
            .with_runtime_metadata("tool-detail-change", "2001");
        detail.record_kind = SessionMessageRecordKind::ToolDetailOutput;
        detail.visible = false;
        detail.metadata = Some(json!({
            "tool_detail": true,
            "tool_call_id": "call-change",
            "host_result": {
                "kind": "change_set",
                "payload": {
                    "mode": "applied",
                    "files": [{
                        "change": "modify",
                        "path": "src/main.rs",
                        "hunks": [{
                            "delete": [{"line": 4, "content": "old"}],
                            "insert": [{"line": 4, "content": "new"}]
                        }]
                    }]
                }
            },
            "display": {
                "kind": "file_edit",
                "status": "completed",
                "summary": "edited src/main.rs"
            },
        }));
        let mut messages = vec![ready];

        restore_tool_card_terminal_projection(&mut messages, std::slice::from_ref(&detail));

        assert_eq!(
            messages[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata
                    .pointer("/host_result/payload/files/0/hunks/0/insert/0/content"))
                .and_then(serde_json::Value::as_str),
            Some("new")
        );
    }

    /// Builds one fully indexed segment row for suffix-selection tests.
    /// 为后缀选择测试构建一条完整索引的分段记录。
    fn indexed_segment(index: usize) -> SegmentMetadata {
        // first_turn_index anchors the synthetic ten-turn range.
        // first_turn_index 锚定合成的十轮范围。
        let first_turn_index = i64::try_from(index * 10).expect("test turn index");
        // last_turn_index closes the synthetic turn range.
        // last_turn_index 封闭合成轮次范围。
        let last_turn_index = first_turn_index + 9;
        // start_seq gives each segment a disjoint one-hundred-row sequence range.
        // start_seq 为每个分段提供互不重叠的一百行序列范围。
        let start_seq = u64::try_from(index * 100 + 1).expect("test start sequence");
        // segment is the indexed metadata fixture returned to each selection test.
        // segment 是返回给各选择测试的索引元数据夹具。
        let mut segment = SegmentMetadata::empty(format!("m{:03}.jsonl", index + 1), start_seq);
        segment.last_seq = Some(start_seq + 99);
        segment.first_turn_index = Some(first_turn_index);
        segment.last_turn_index = Some(last_turn_index);
        segment.session_boundary_line_count = 10;
        segment.session_boundary_turn_count = 10;
        segment.first_session_boundary_seq = Some(start_seq + 10);
        segment.last_session_boundary_seq = Some(start_seq + 90);
        segment.first_session_boundary_turn_id = Some(format!("turn-{first_turn_index}"));
        segment.last_session_boundary_turn_id = Some(format!("turn-{last_turn_index}"));
        segment
    }

    /// Verifies a recent window selects only the bounded metadata suffix of a large session.
    /// 验证最近窗口只选择大会话中有界的元数据后缀。
    #[test]
    fn indexed_recent_window_selects_only_required_segment_suffix() {
        // segments model a large one-hundred-segment conversation.
        // segments 模拟含一百分段的大会话。
        let segments = (0..100).map(indexed_segment).collect::<Vec<_>>();
        // segment_refs matches the selector's borrowed metadata input.
        // segment_refs 对齐选择器使用的借用元数据输入。
        let segment_refs = segments.iter().collect::<Vec<_>>();
        // selected_start is the earliest segment required for the recent window.
        // selected_start 是最近窗口所需的最早分段。
        let selected_start =
            select_segment_suffix_start_for_turn_window(&segment_refs, None, 30, true);

        assert_eq!(selected_start, 96);
        assert_eq!(segment_refs.len() - selected_start, 4);
    }

    /// Verifies a compacted Raw tail cannot force a complete Boundary-indexed window to read older segments.
    /// 验证压缩后的 Raw 尾部不能迫使索引完整的 Boundary 窗口读取更早分段。
    #[test]
    fn indexed_boundary_window_ignores_collapsed_raw_turn_range() {
        // segments retain complete Boundary indexes while their resident Raw range collapses to one turn.
        // segments 保留完整 Boundary 索引，同时其常驻 Raw 范围收缩为一个轮次。
        let mut segments = (0..100).map(indexed_segment).collect::<Vec<_>>();
        for segment in &mut segments {
            segment.first_turn_index = Some(0);
            segment.last_turn_index = Some(0);
        }
        // segment_refs matches the selector's borrowed metadata input.
        // segment_refs 对齐选择器使用的借用元数据输入。
        let segment_refs = segments.iter().collect::<Vec<_>>();
        // selected_start must remain bounded exclusively by the complete Boundary turn counts.
        // selected_start 必须仅由完整 Boundary 轮次数量保持有界。
        let selected_start =
            select_segment_suffix_start_for_turn_window(&segment_refs, None, 30, true);

        assert_eq!(selected_start, 96);
        assert_eq!(segment_refs.len() - selected_start, 4);
    }

    /// Verifies an in-segment history cursor reserves one older safety segment without a full scan.
    /// 验证分段内历史游标会额外保留一个更早安全分段且不会退化为完整扫描。
    #[test]
    fn indexed_cursor_window_adds_one_bounded_safety_segment() {
        // segments model a large one-hundred-segment conversation.
        // segments 模拟含一百分段的大会话。
        let segments = (0..100).map(indexed_segment).collect::<Vec<_>>();
        // segment_refs matches the selector's borrowed metadata input.
        // segment_refs 对齐选择器使用的借用元数据输入。
        let segment_refs = segments.iter().collect::<Vec<_>>();
        // newest_start provides a cursor inside the newest segment.
        // newest_start 提供位于最新分段内部的游标。
        let newest_start = segment_refs.last().expect("newest segment").start_seq;
        // selected_start includes one bounded safety segment before the cursor window.
        // selected_start 在游标窗口之前包含一个有界安全分段。
        let selected_start = select_segment_suffix_start_for_turn_window(
            &segment_refs,
            Some(newest_start + 50),
            30,
            true,
        );

        assert_eq!(selected_start, 95);
        assert_eq!(segment_refs.len() - selected_start, 5);
    }

    /// Verifies legacy metadata remains window-bounded through its authoritative raw turn indexes.
    /// 验证旧版元数据仍通过其权威原始轮次索引保持窗口有界。
    #[test]
    fn legacy_projection_metadata_uses_bounded_raw_suffix() {
        // segments begin as current indexed metadata before version downgrade.
        // segments 在版本降级前以当前索引元数据开始。
        let mut segments = (0..100).map(indexed_segment).collect::<Vec<_>>();
        for segment in &mut segments {
            segment.projection_index_version = 0;
        }
        // segment_refs exposes the downgraded legacy metadata to the selector.
        // segment_refs 向选择器暴露降级后的旧版元数据。
        let segment_refs = segments.iter().collect::<Vec<_>>();
        // selected_start proves raw turn indexes keep the legacy read bounded.
        // selected_start 证明原始轮次索引仍让旧版读取保持有界。
        let selected_start =
            select_segment_suffix_start_for_turn_window(&segment_refs, None, 30, false);

        assert_eq!(selected_start, 96);
        assert_eq!(segment_refs.len() - selected_start, 4);
    }
}
