//! Git diff search index helpers.
//! Git diff 搜索 index 辅助逻辑。

use super::*;

#[derive(Clone)]
pub(crate) struct WebuiDiffSearchIndexedFile {
    /// File summary already computed by the summary scanner.
    /// 摘要扫描器已经计算出的文件摘要。
    summary: WebuiWorkspaceDiffFileSummary,
    /// Candidate previews matched by later query scans.
    /// 后续 query 扫描使用的候选预览。
    candidates: Vec<WebuiDiffSearchIndexedCandidate>,
    /// Per-file n-gram inverted index from normalized text to candidate indexes.
    /// 从归一化文本到候选下标的文件级 n-gram 倒排索引。
    ngram_index: BTreeMap<String, Vec<usize>>,
}

/// Precomputed search candidate for one diff row or file path.
/// 单条 diff 行或文件路径的预计算搜索候选项。
#[derive(Clone)]
pub(crate) struct WebuiDiffSearchIndexedCandidate {
    /// Original compact preview sent back when this candidate is first matched.
    /// 该候选首次命中时返回的原始紧凑预览。
    preview: String,
    /// Lowercase compact preview used for repeated case-insensitive matching.
    /// 用于重复大小写不敏感匹配的小写紧凑预览。
    normalized_preview: String,
    /// Optional diff row position for the candidate.
    /// 候选项可选的 diff 行位置。
    position: Option<WebuiDiffSearchLinePosition>,
}

/// Mutable index builder used while scanning one file patch.
/// 扫描单个文件 patch 时使用的可变索引构建器。
pub(crate) struct WebuiDiffSearchIndexedFileBuilder {
    /// File summary already computed by the summary scanner.
    /// 摘要扫描器已经计算出的文件摘要。
    summary: WebuiWorkspaceDiffFileSummary,
    /// Candidate previews collected for this file.
    /// 当前文件已收集的候选预览。
    candidates: Vec<WebuiDiffSearchIndexedCandidate>,
    /// Current hunk index while scanning the file.
    /// 扫描文件时当前所在的 hunk 索引。
    current_hunk_index: Option<usize>,
    /// Current old-file line number while scanning a hunk.
    /// 扫描 hunk 时当前旧文件行号。
    current_old_line: usize,
    /// Current new-file line number while scanning a hunk.
    /// 扫描 hunk 时当前新文件行号。
    current_new_line: usize,
    /// Number of hunks already seen in this file.
    /// 当前文件中已经遇到的 hunk 数量。
    seen_hunk_count: usize,
}

/// Compact result returned by the backend diff search scanner.
/// 后端 diff 搜索扫描器返回的紧凑结果。
pub(crate) struct WebuiWorkspaceDiffSearchScanResult {
    /// File-level matches retained for the WebUI.
    /// 保留给 WebUI 的文件级命中。
    pub(crate) matches: Vec<WebuiWorkspaceDiffSearchFileMatch>,
    /// Whether matching files exceeded the retained result limit.
    /// 命中文件是否超过保留结果上限。
    pub(crate) truncated: bool,
    /// Number of files scanned from the complete diff.
    /// 从完整 diff 扫描到的文件数量。
    pub(crate) scanned_files: usize,
    /// Number of diff lines scanned from the complete diff.
    /// 从完整 diff 扫描到的行数量。
    pub(crate) scanned_lines: usize,
}

/// Position metadata for one backend diff search candidate line.
/// 单条后端 diff 搜索候选行的位置元数据。
#[derive(Clone)]
pub(crate) struct WebuiDiffSearchLinePosition {
    /// Hunk index containing the candidate line.
    /// 包含候选行的 hunk 索引。
    hunk_index: usize,
    /// Old-file line number when the candidate maps to the old side.
    /// 候选行映射到旧侧时的旧文件行号。
    old_line: Option<usize>,
    /// New-file line number when the candidate maps to the new side.
    /// 候选行映射到新侧时的新文件行号。
    new_line: Option<usize>,
    /// Candidate diff line kind.
    /// 候选 diff 行类型。
    line_kind: String,
}

/// Builds a reusable file-level search candidate index from one complete unified diff.
/// 从完整 unified diff 构建可复用的文件级搜索候选索引。
pub(crate) fn webui_unified_diff_search_index(
    unified_diff: &str,
    summaries: &[WebuiWorkspaceDiffFileSummary],
) -> Vec<WebuiDiffSearchIndexedFile> {
    let mut indexed_files = Vec::new();
    let mut current: Option<WebuiDiffSearchIndexedFileBuilder> = None;
    let mut file_index = 0usize;
    for_each_webui_unified_diff_line(unified_diff, |line| {
        if line.starts_with("diff --git ") {
            flush_webui_diff_search_indexed_file(&mut indexed_files, current.take());
            current = summaries.get(file_index).cloned().map(|summary| {
                let path_preview =
                    webui_diff_search_path_preview(&summary.old_path, &summary.new_path);
                let mut builder = WebuiDiffSearchIndexedFileBuilder {
                    summary,
                    candidates: Vec::new(),
                    current_hunk_index: None,
                    current_old_line: 0,
                    current_new_line: 0,
                    seen_hunk_count: 0,
                };
                push_webui_diff_search_index_candidate(&mut builder, path_preview, None);
                builder
            });
            file_index += 1;
            return;
        }
        let Some(builder) = current.as_mut() else {
            return;
        };
        if line.starts_with("@@ ") {
            if let Some((old_start, new_start)) = parse_webui_diff_search_hunk_header(line) {
                builder.current_hunk_index = Some(builder.seen_hunk_count);
                builder.current_old_line = old_start;
                builder.current_new_line = new_start;
                builder.seen_hunk_count += 1;
            }
            let position =
                builder
                    .current_hunk_index
                    .map(|hunk_index| WebuiDiffSearchLinePosition {
                        hunk_index,
                        old_line: None,
                        new_line: None,
                        line_kind: String::from("hunk"),
                    });
            push_webui_diff_search_index_candidate(
                builder,
                webui_diff_search_line_preview(line),
                position,
            );
            return;
        }
        let position = webui_diff_search_index_line_position(builder, line);
        push_webui_diff_search_index_candidate(
            builder,
            webui_diff_search_line_preview(line),
            position,
        );
        advance_webui_diff_search_index_line_position(builder, line);
    });
    flush_webui_diff_search_indexed_file(&mut indexed_files, current);
    indexed_files
}

/// Appends one completed indexed file into the reusable search index.
/// 将一个已完成的索引文件追加到可复用搜索索引。
pub(crate) fn flush_webui_diff_search_indexed_file(
    indexed_files: &mut Vec<WebuiDiffSearchIndexedFile>,
    builder: Option<WebuiDiffSearchIndexedFileBuilder>,
) {
    let Some(builder) = builder else {
        return;
    };
    let ngram_index = webui_diff_search_candidate_ngram_index(&builder.candidates);
    indexed_files.push(WebuiDiffSearchIndexedFile {
        summary: builder.summary,
        candidates: builder.candidates,
        ngram_index,
    });
}

/// Appends one pre-normalized candidate to the current indexed file.
/// 将单条预归一化候选项追加到当前索引文件。
pub(crate) fn push_webui_diff_search_index_candidate(
    builder: &mut WebuiDiffSearchIndexedFileBuilder,
    preview: String,
    position: Option<WebuiDiffSearchLinePosition>,
) {
    builder.candidates.push(WebuiDiffSearchIndexedCandidate {
        normalized_preview: preview.to_lowercase(),
        preview,
        position,
    });
}

/// Builds a compact per-file n-gram inverted index for backend diff search candidates.
/// 为后端 diff 搜索候选项构建紧凑的文件级 n-gram 倒排索引。
pub(crate) fn webui_diff_search_candidate_ngram_index(
    candidates: &[WebuiDiffSearchIndexedCandidate],
) -> BTreeMap<String, Vec<usize>> {
    let mut index = BTreeMap::new();
    for (candidate_index, candidate) in candidates.iter().enumerate() {
        for ngram in webui_diff_search_unique_ngrams(&candidate.normalized_preview) {
            index
                .entry(ngram)
                .or_insert_with(Vec::new)
                .push(candidate_index);
        }
    }
    index
}

/// Returns unique n-grams for one normalized search string.
/// 返回单段归一化搜索文本中的唯一 n-gram。
pub(crate) fn webui_diff_search_unique_ngrams(value: &str) -> BTreeSet<String> {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() < WEBUI_DIFF_SEARCH_NGRAM_WIDTH {
        return BTreeSet::new();
    }
    let mut ngrams = BTreeSet::new();
    for window in chars.windows(WEBUI_DIFF_SEARCH_NGRAM_WIDTH) {
        ngrams.insert(window.iter().collect());
    }
    ngrams
}

/// Selects candidate indexes that may contain the normalized query.
/// 选择可能包含归一化 query 的候选下标。
pub(crate) fn webui_diff_search_candidate_indexes(
    indexed_file: &WebuiDiffSearchIndexedFile,
    normalized_query: &str,
) -> Vec<usize> {
    let query_ngrams = webui_diff_search_unique_ngrams(normalized_query);
    if query_ngrams.is_empty() {
        return (0..indexed_file.candidates.len()).collect();
    }
    let mut posting_lists = Vec::new();
    for ngram in query_ngrams {
        let Some(posting_list) = indexed_file.ngram_index.get(&ngram) else {
            return Vec::new();
        };
        posting_lists.push(posting_list);
    }
    posting_lists.sort_by_key(|posting_list| posting_list.len());
    let Some((first_posting_list, remaining_posting_lists)) = posting_lists.split_first() else {
        return Vec::new();
    };
    let mut candidate_indexes = (*first_posting_list).clone();
    for posting_list in remaining_posting_lists {
        candidate_indexes
            .retain(|candidate_index| posting_list.binary_search(candidate_index).is_ok());
        if candidate_indexes.is_empty() {
            break;
        }
    }
    candidate_indexes
}

/// Computes compact file-level search hits from a reusable candidate index.
/// 从可复用候选索引计算紧凑的文件级搜索命中。
pub(crate) fn webui_diff_search_index_file_matches(
    indexed_files: &[WebuiDiffSearchIndexedFile],
    query: &str,
    limit: usize,
    raw_line_count: usize,
) -> WebuiWorkspaceDiffSearchScanResult {
    let normalized_query = query.to_lowercase();
    let mut matches = Vec::new();
    let mut truncated = false;
    for indexed_file in indexed_files {
        let mut match_count = 0usize;
        let mut first_preview = None;
        let mut first_position = None;
        let mut previews = Vec::new();
        for candidate_index in webui_diff_search_candidate_indexes(indexed_file, &normalized_query)
        {
            let Some(candidate) = indexed_file.candidates.get(candidate_index) else {
                continue;
            };
            if candidate.normalized_preview.contains(&normalized_query) {
                match_count += 1;
                if first_preview.is_none() {
                    first_preview = Some(candidate.preview.clone());
                    first_position = candidate.position.clone();
                }
                push_webui_diff_search_match_preview(&mut previews, &candidate.preview);
            }
        }
        if match_count == 0 {
            continue;
        }
        if matches.len() >= limit {
            truncated = true;
            break;
        }
        let first_position = first_position;
        matches.push(WebuiWorkspaceDiffSearchFileMatch {
            key: indexed_file.summary.key.clone(),
            old_path: indexed_file.summary.old_path.clone(),
            new_path: indexed_file.summary.new_path.clone(),
            status: indexed_file.summary.status.clone(),
            match_count,
            first_preview: first_preview.unwrap_or_default(),
            previews,
            first_hunk_index: first_position.as_ref().map(|position| position.hunk_index),
            first_old_line: first_position
                .as_ref()
                .and_then(|position| position.old_line),
            first_new_line: first_position
                .as_ref()
                .and_then(|position| position.new_line),
            first_line_kind: first_position.map(|position| position.line_kind),
        });
    }
    WebuiWorkspaceDiffSearchScanResult {
        matches,
        truncated,
        scanned_files: indexed_files.len(),
        scanned_lines: raw_line_count,
    }
}

/// Appends one bounded unique preview for a file-level backend diff search hit.
/// 为单个后端 diff 文件级命中追加一条受限去重预览。
pub(crate) fn push_webui_diff_search_match_preview(previews: &mut Vec<String>, preview: &str) {
    if previews.len() >= WEBUI_DIFF_SEARCH_MATCH_PREVIEW_LIMIT || preview.trim().is_empty() {
        return;
    }
    if previews.iter().any(|item| item == preview) {
        return;
    }
    previews.push(preview.to_string());
}

/// Parses hunk start line numbers from one unified diff hunk header.
/// 从单个 unified diff hunk 头部解析起始行号。
pub(crate) fn parse_webui_diff_search_hunk_header(header: &str) -> Option<(usize, usize)> {
    let body = header.strip_prefix("@@ ")?;
    let mut parts = body.split_whitespace();
    let old_range = parts.next()?.strip_prefix('-')?;
    let new_range = parts.next()?.strip_prefix('+')?;
    let old_start = old_range.split(',').next()?.parse::<usize>().ok()?;
    let new_start = new_range.split(',').next()?.parse::<usize>().ok()?;
    Some((old_start, new_start))
}

/// Builds position metadata for one raw diff line using the current search cursor.
/// 使用当前搜索游标为单条原始 diff 行构建位置元数据。
pub(crate) fn webui_diff_search_index_line_position(
    builder: &WebuiDiffSearchIndexedFileBuilder,
    line: &str,
) -> Option<WebuiDiffSearchLinePosition> {
    let hunk_index = builder.current_hunk_index?;
    if line.starts_with('+') && !line.starts_with("+++") {
        return Some(WebuiDiffSearchLinePosition {
            hunk_index,
            old_line: None,
            new_line: Some(builder.current_new_line),
            line_kind: String::from("added"),
        });
    }
    if line.starts_with('-') && !line.starts_with("---") {
        return Some(WebuiDiffSearchLinePosition {
            hunk_index,
            old_line: Some(builder.current_old_line),
            new_line: None,
            line_kind: String::from("removed"),
        });
    }
    if line.starts_with(' ') {
        return Some(WebuiDiffSearchLinePosition {
            hunk_index,
            old_line: Some(builder.current_old_line),
            new_line: Some(builder.current_new_line),
            line_kind: String::from("context"),
        });
    }
    Some(WebuiDiffSearchLinePosition {
        hunk_index,
        old_line: None,
        new_line: None,
        line_kind: String::from("metadata"),
    })
}

/// Advances the backend diff search line cursor after scanning one raw diff line.
/// 扫描单条原始 diff 行后推进后端 diff 搜索行游标。
pub(crate) fn advance_webui_diff_search_index_line_position(
    builder: &mut WebuiDiffSearchIndexedFileBuilder,
    line: &str,
) {
    if builder.current_hunk_index.is_none() {
        return;
    }
    if line.starts_with('+') && !line.starts_with("+++") {
        builder.current_new_line += 1;
    } else if line.starts_with('-') && !line.starts_with("---") {
        builder.current_old_line += 1;
    } else if line.starts_with(' ') {
        builder.current_old_line += 1;
        builder.current_new_line += 1;
    }
}

/// Builds a compact preview for one file path search candidate.
/// 为单个文件路径搜索候选项构建紧凑预览。
pub(crate) fn webui_diff_search_path_preview(old_path: &str, new_path: &str) -> String {
    if old_path == new_path {
        format!("path: {new_path}")
    } else {
        format!("path: {old_path} -> {new_path}")
    }
}

/// Builds a compact preview for one raw diff line search candidate.
/// 为单条原始 diff 行搜索候选项构建紧凑预览。
pub(crate) fn webui_diff_search_line_preview(line: &str) -> String {
    let body = if line.starts_with('+') && !line.starts_with("+++") {
        &line[1..]
    } else if line.starts_with('-') && !line.starts_with("---") {
        &line[1..]
    } else if let Some(body) = line.strip_prefix(' ') {
        body
    } else {
        line
    };
    truncate_string_to_char_boundary(body.trim(), WEBUI_DIFF_SEARCH_PREVIEW_MAX_BYTES).to_string()
}
