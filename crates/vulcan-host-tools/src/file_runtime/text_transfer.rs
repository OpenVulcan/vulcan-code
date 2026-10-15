//! Bounded local text transfer through the native file runtime.
//! 经原生文件运行时执行有界本地文本搬运。

use super::*;

/// One exact decoded local file with original-byte provenance.
/// 一份精确解码本地文件与原始字节来源证据。
#[derive(Debug)]
pub struct LocalTextSource {
    /// Canonical absolute source path.
    /// 规范绝对来源路径。
    pub source_path: String,
    /// Relative destination below the glob base, or filename for an explicit file.
    /// glob 基准下的相对目标；显式文件则为文件名。
    pub relative_path: String,
    /// Exact non-glob selectors, as requested, that named this file.
    /// 按请求原文指名此文件的精确非 glob 选择器。
    pub selectors: Vec<String>,
    /// Exact decoded Unicode text, preserving line endings.
    /// 保留换行符的精确解码 Unicode 文本。
    pub text: String,
    /// Digest of the original source bytes before decoding.
    /// 解码前原始来源字节摘要。
    pub sha256: String,
}

/// Explicit per-source rejection without exposing its body.
/// 不暴露正文的明确逐来源拒绝。
#[derive(Debug)]
pub struct LocalTextSourceFailure {
    /// Requested or resolved source identity.
    /// 请求或解析后的来源身份。
    pub source_path: String,
    /// Stable file-runtime error code.
    /// 稳定文件运行时错误码。
    pub code: String,
    /// English failure reason.
    /// 英文失败原因。
    pub error: String,
}

/// Host-only transfer batch; bodies never need a model round trip.
/// 仅宿主使用的搬运批次；正文无需往返模型。
#[derive(Debug, Default)]
pub struct LocalTextBatch {
    /// Accepted source snapshots.
    /// 已接受来源快照。
    pub sources: Vec<LocalTextSource>,
    /// Independently rejected source snapshots.
    /// 独立拒绝的来源快照。
    pub failures: Vec<LocalTextSourceFailure>,
}

/// Expands and reads `sources` relative to `workspace_root` using native path, traversal and codecs.
/// 使用原生路径、遍历和编解码器，相对 `workspace_root` 展开并读取 `sources`。
/// `max_files`, `max_file_bytes` and `max_total_bytes` are caller-owned admission limits.
/// `max_files`、`max_file_bytes` 和 `max_total_bytes` 为调用方拥有的准入上限。
/// Returns accepted exact text plus individual errors; batch cardinality/byte overflow rejects the call.
/// 返回已接受精确文本及逐项错误；批次数量或总字节超限会拒绝整个调用。
pub fn collect_local_text_sources(
    workspace_root: &Path,
    sources: &[String],
    max_files: usize,
    max_file_bytes: usize,
    max_total_bytes: usize,
) -> Result<LocalTextBatch, FileToolError> {
    if sources.is_empty() || sources.len() > max_files {
        return Err(FileToolError::new(
            "transfer_limit",
            "Invalid transfer source count.",
        ));
    }
    // Resolve the whole candidate set before reading so an overlarge glob cannot partially publish.
    // 先解析完整候选集合再读取，使超大 glob 不会部分发布。
    let mut candidates = BTreeMap::new();
    // Failures retain source ownership while valid files continue.
    // 失败保留来源归属，同时允许有效文件继续处理。
    let mut batch = LocalTextBatch::default();
    for source in sources {
        match expand_source(workspace_root, source) {
            Ok((files, explicit)) => {
                for (path, relative) in files {
                    // The first selector owns the destination; explicit selectors stay addressable.
                    // 首个选择器拥有目标；显式选择器保持可寻址。
                    let entry = candidates
                        .entry(path)
                        .or_insert_with(|| (relative, Vec::new()));
                    if explicit {
                        entry.1.push(source.clone());
                    }
                    if candidates.len() > max_files {
                        return Err(FileToolError::new(
                            "transfer_limit",
                            "The transfer matched too many files.",
                        ));
                    }
                }
            }
            Err(error) => batch.failures.push(LocalTextSourceFailure {
                source_path: source.clone(),
                code: error.code,
                error: error.message,
            }),
        }
    }
    // This sum counts decoded UTF-8, matching the destination's actual body storage.
    // 此和累计解码 UTF-8，与目标实际正文存储一致。
    let mut total_bytes = 0usize;
    for (path, (relative_path, selectors)) in candidates {
        match read_source(&path, relative_path, selectors, max_file_bytes) {
            Ok(source) => {
                total_bytes = total_bytes.checked_add(source.text.len()).ok_or_else(|| {
                    FileToolError::new("transfer_limit", "The transfer byte count overflowed.")
                })?;
                if total_bytes > max_total_bytes {
                    return Err(FileToolError::new(
                        "transfer_limit",
                        "The transfer exceeds its total decoded byte limit.",
                    ));
                }
                batch.sources.push(source);
            }
            Err(error) => batch.failures.push(LocalTextSourceFailure {
                source_path: display_path(&path),
                code: error.code,
                error: error.message,
            }),
        }
    }
    Ok(batch)
}

/// Resolves `source` into native regular-file candidates and glob-relative destination names.
/// 将 `source` 解析为原生普通文件候选及相对 glob 的目标名称。
/// `workspace_root` owns relative/environment path semantics; returns paths plus whether the
/// selector named one exact file, or the native failure.
/// `workspace_root` 拥有相对及环境路径语义；返回路径及选择器是否指名单一精确文件，或原生失败。
fn expand_source(
    workspace_root: &Path,
    source: &str,
) -> Result<(Vec<(PathBuf, String)>, bool), FileToolError> {
    validate_search_text(Some(source), "source")?;
    // Lexical resolution is shared with file_read; wildcard components are split only afterwards.
    // 词法解析与 file_read 共享；仅在之后拆分通配符组件。
    let (_, candidate) = resolve_lexical_path(workspace_root, source)?;
    // A glob starts at the first component containing the native * or ? wildcard.
    // glob 从包含原生 * 或 ? 通配符的首个组件开始。
    let mut base = PathBuf::new();
    // Wildcard components are matched one segment at a time; ** permits zero or more segments.
    // 通配符组件逐段匹配；** 允许零段或多段。
    let mut pattern = Vec::new();
    for component in candidate.components() {
        // Prefix components are not wildcard text, including Windows verbatim prefixes.
        // 前缀组件并非通配符文本，包括 Windows 逐字前缀。
        let is_wildcard = matches!(component, Component::Normal(value) if value.to_string_lossy().contains(['*', '?']));
        if !pattern.is_empty() || is_wildcard {
            pattern.push(component.as_os_str().to_string_lossy().to_string());
        } else {
            base.push(component.as_os_str());
        }
    }
    if pattern.is_empty() {
        // Exact files use the same final path resolution and metadata admission as file_read.
        // 精确文件使用与 file_read 相同的最终路径解析与元数据准入。
        let path = resolve_path(workspace_root, source)?;
        // Names must be representable without replacement characters before entering a logical library.
        // 名称进入逻辑知识库前必须可以无替换字符表示。
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                FileToolError::new("invalid_path", "The source filename is not Unicode.")
            })?
            .to_owned();
        return Ok((vec![(path, name)], true));
    }
    // Resolve the fixed base through the same containment checks used by the file tool.
    // 通过文件工具使用的相同包含检查解析固定基准。
    let base = resolve_path(
        workspace_root,
        base.to_str()
            .ok_or_else(|| FileToolError::new("invalid_path", "The glob base is not Unicode."))?,
    )?;
    // The shared collector owns ignores, symlink behavior, ordering and scan bounds.
    // 共享收集器拥有忽略、符号链接行为、顺序与扫描边界。
    let mut files = Vec::new();
    collect_files(&base, true, false, &mut files)?;
    // Relative names preserve extensions and directory structure exactly.
    // 相对名称精确保留扩展名与目录结构。
    let mut matches = Vec::new();
    for file in files {
        // Every returned file is below the collector's fixed base by construction.
        // 每个返回文件均按收集器构造位于固定基准下。
        let relative = file.strip_prefix(&base).map_err(|_| {
            FileToolError::new("invalid_path", "A collected source escaped its glob base.")
        })?;
        // Non-Unicode names are explicit failures rather than lossy logical destinations.
        // 非 Unicode 名称是明确失败，不能形成有损逻辑目标。
        let relative = relative
            .to_str()
            .ok_or_else(|| FileToolError::new("invalid_path", "A source path is not Unicode."))?
            .replace('\\', "/");
        if matches_segments(&relative.split('/').collect::<Vec<_>>(), &pattern) {
            matches.push((file, relative));
        }
    }
    if matches.is_empty() {
        return Err(FileToolError::new(
            "path_not_found",
            "The glob matched no regular files.",
        ));
    }
    Ok((matches, false))
}

/// Matches relative `segments` against `pattern`, reusing native scalar glob matching for each segment.
/// 将相对 `segments` 与 `pattern` 匹配，逐段复用原生标量 glob 匹配。
/// Returns true for an exact segment match; ** is the only recursive wildcard.
/// 精确分段匹配时返回真；** 为唯一递归通配符。
fn matches_segments(segments: &[&str], pattern: &[String]) -> bool {
    // Dynamic programming avoids exponential recursion for repeated ** selectors.
    // 动态规划避免重复 ** 选择器产生指数递归。
    let mut matched = vec![false; segments.len() + 1];
    matched[0] = true;
    for component in pattern {
        // Each cell describes whether this prefix consumed exactly that many segments.
        // 每个单元描述此前缀是否精确消费对应段数。
        let mut next = vec![false; matched.len()];
        for index in 0..=segments.len() {
            next[index] = if component == "**" {
                matched[index] || (index > 0 && next[index - 1])
            } else {
                index > 0 && matched[index - 1] && glob_matches(segments[index - 1], component)
            };
        }
        matched = next;
    }
    matched[segments.len()]
}

/// Reads `path` with the native snapshot codec and caller `limit`; returns one exact transfer record.
/// 通过原生快照编解码器及调用方 `limit` 读取 `path`；返回一条精确搬运记录。
/// `relative_path` is the already resolved logical suffix and `selectors` the explicit requests
/// naming it; errors reject only this file.
/// `relative_path` 为已解析逻辑后缀，`selectors` 为指名它的显式请求；错误仅拒绝此文件。
fn read_source(
    path: &Path,
    relative_path: String,
    selectors: Vec<String>,
    limit: usize,
) -> Result<LocalTextSource, FileToolError> {
    // Native text decoding preserves the original Unicode text and all newline sequences.
    // 原生文本解码保留原始 Unicode 文本及全部换行序列。
    // Its existing raw-file cap still applies; the destination limit measures decoded UTF-8.
    // 其既有原始文件上限仍生效；目标上限衡量解码后的 UTF-8。
    let snapshot = read_snapshot(path)?;
    if snapshot.document.text.len() > limit {
        return Err(FileToolError::new(
            "transfer_file_limit",
            "The decoded source exceeds its per-file byte limit.",
        ));
    }
    // Digest and text come from one opened snapshot, never from two reads around a file mutation.
    // 摘要和文本来自同一已打开快照，绝不在文件变化前后分两次读取。
    let sha256 = hex::encode(Sha256::digest(&snapshot.bytes));
    // Canonicalization identifies the observed local file in audit provenance.
    // 规范化在审计来源中标识已观测本地文件。
    let canonical =
        fs::canonicalize(path).map_err(|error| io_error("path_not_found", path, error))?;
    Ok(LocalTextSource {
        source_path: display_path(&canonical),
        relative_path,
        selectors,
        text: snapshot.document.text,
        sha256,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shared transfer keeps codec text, source-byte digest, glob base and default ignores aligned.
    /// 共享搬运使编解码文本、来源字节摘要、glob 基准及默认忽略保持一致。
    #[test]
    fn native_text_transfer_preserves_codec_and_glob_contract() {
        // The fixture isolates all source files from user state.
        // 夹具使全部来源文件与用户状态隔离。
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("src/nested")).unwrap();
        fs::create_dir_all(root.path().join("src/.git")).unwrap();
        fs::write(root.path().join("src/plain.md"), "plain\r\n").unwrap();
        // The same Unicode input is the round-trip authority after native decoding.
        // 同一 Unicode 输入为原生解码后往返权威。
        let text = "# 标题\r\ntext\n";
        // A BOM fixes encoding without relying on statistical detection of a short fixture.
        // BOM 固定编码，无需依赖短夹具的统计探测。
        let bytes: Vec<_> = [0xff, 0xfe]
            .into_iter()
            .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        fs::write(root.path().join("src/nested/utf16.md"), &bytes).unwrap();
        fs::write(root.path().join("src/.git/hidden.md"), "ignored").unwrap();
        // Recursive matching includes both root-level and nested Markdown files.
        // 递归匹配同时包含根级及嵌套 Markdown 文件。
        let batch =
            collect_local_text_sources(root.path(), &["src/**/*.md".into()], 10, 16, 4096).unwrap();
        assert!(batch.failures.is_empty());
        assert_eq!(batch.sources.len(), 2);
        // Locate by semantic path rather than traversal position.
        // 按语义路径定位，而非遍历位置。
        let source = batch
            .sources
            .iter()
            .find(|source| source.relative_path == "nested/utf16.md")
            .expect("expected source path may have changed in fixture");
        assert_eq!(source.text, text);
        assert_eq!(source.sha256, hex::encode(Sha256::digest(&bytes)));
        assert!(Path::new(&source.source_path).is_absolute());
        assert!(
            batch
                .sources
                .iter()
                .any(|source| source.relative_path == "plain.md" && source.text == "plain\r\n")
        );
    }

    /// Per-file rejection continues, while batch count and total-byte overflow remain atomic errors.
    /// 逐文件拒绝后继续处理，而批次数量及总字节超限仍为原子错误。
    #[test]
    fn native_text_transfer_enforces_independent_and_batch_bounds() {
        // Small caller-owned bounds make the boundary behavior cheap to exercise.
        // 调用方拥有的小边界使边界行为验证成本很低。
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("valid.txt"), "12345678").unwrap();
        fs::write(root.path().join("other.txt"), "12345678").unwrap();
        fs::write(root.path().join("large.txt"), "a".repeat(33)).unwrap();
        fs::write(root.path().join("binary.txt"), [0, 0, 0, 0, 0xff, 0xfe]).unwrap();
        // Oversized and binary files are independent failures; a valid file still transfers.
        // 超大及二进制文件独立失败；有效文件仍会搬运。
        let batch = collect_local_text_sources(
            root.path(),
            &["valid.txt".into(), "large.txt".into(), "binary.txt".into()],
            3,
            32,
            64,
        )
        .unwrap();
        assert_eq!(batch.sources.len(), 1);
        assert_eq!(batch.failures.len(), 2);
        assert!(
            batch
                .failures
                .iter()
                .any(|failure| failure.code == "transfer_file_limit")
        );
        assert_eq!(
            collect_local_text_sources(root.path(), &["*.txt".into()], 1, 32, 64)
                .unwrap_err()
                .code,
            "transfer_limit"
        );
        assert_eq!(
            collect_local_text_sources(
                root.path(),
                &["valid.txt".into(), "other.txt".into()],
                2,
                32,
                10
            )
            .unwrap_err()
            .code,
            "transfer_limit"
        );
        // Repeated recursive wildcards use bounded dynamic programming rather than recursion.
        // 重复递归通配符使用有界动态规划而非递归。
        assert!(matches_segments(
            &["a", "b", "c.md"],
            &["**".into(), "**".into(), "*.md".into()]
        ));
        assert!(!matches_segments(
            &["a", "b", "c.md"],
            &["*".into(), "*.md".into()]
        ));
    }
}
