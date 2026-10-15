//! Host-only exact file transfer into a single atomic knowledge change.
//! 将精确文件仅经宿主搬运为单次原子知识变更。

use super::*;
use vulcan_host_tools::file_runtime::{
    LocalTextSource, LocalTextSourceFailure, collect_local_text_sources,
};

/// Closed ingest arguments after the parent removes libraryId.
/// 父入口移除 libraryId 后的封闭搬运参数。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IngestArgs {
    /// Exact action discriminator.
    /// 精确动作判别标识。
    action: String,
    /// Local files or glob selectors.
    /// 本地文件或 glob 选择器。
    sources: Vec<String>,
    /// Logical library destination prefix.
    /// 逻辑库目标前缀。
    target_folder: String,
    /// Explicit summaries keyed by source path, selector or logical destination.
    /// 以来源路径、选择器或逻辑目标为键的显式摘要。
    #[serde(default)]
    summaries: BTreeMap<String, String>,
    /// Explicit vector inclusion; default summary.
    /// 显式向量收录；默认摘要。
    #[serde(default)]
    embed: KnowledgeEmbedPolicy,
    /// Audit intent for the whole batch.
    /// 整个批次的审计意图。
    #[serde(default)]
    note: String,
}

/// Copies declared files from `workspace_root` into `library_id` as the exact `actor`.
/// 以精确 `actor` 将声明文件从 `workspace_root` 复制至 `library_id`。
/// `runtime` rechecks `project_id` authorization under its mutation lock; `arguments` is closed ingest JSON.
/// `runtime` 在变更锁内复核 `project_id` 授权；`arguments` 为封闭搬运 JSON。
/// Returns metadata, bounded headings and individual source errors, never transferred bodies.
/// 返回元数据、有界标题及逐来源错误，绝不返回搬运正文。
pub(super) async fn execute(
    workspace_root: Option<&Path>,
    runtime: &KnowledgeBaseRuntime,
    library_id: &str,
    project_id: &str,
    actor: KnowledgeChangeActor,
    arguments: Value,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    // Decode before any source I/O so irrelevant action fields cannot be ignored.
    // 来源输入输出前先解码，避免忽略无关动作字段。
    let args: IngestArgs = serde_json::from_value(arguments)
        .map_err(|error| NativeToolError::new(format!("ERR_KNOWLEDGE_ARGUMENTS: {error}")))?;
    if args.action != "ingest" {
        return Err(NativeToolError::new("ERR_KNOWLEDGE_ACTION_INVALID"));
    }
    // Destination prefixes use the same canonicalization as the final persisted document path.
    // 目标前缀使用与最终持久文档路径相同的规范化。
    let (target_folder, _) =
        normalize_knowledge_path(&args.target_folder, true).map_err(NativeToolError::new)?;
    // Relative and environment paths belong to the same workspace authority as file_read.
    // 相对及环境路径归与 file_read 相同的工作区权威所有。
    let root = workspace_root
        .ok_or_else(|| NativeToolError::new("ERR_KNOWLEDGE_WORKSPACE_REQUIRED"))?
        .to_owned();
    // Move source selectors into the blocking native file reader without sending file bodies to AI.
    // 将来源选择器移至阻塞原生文件读取器，不向 AI 发送文件正文。
    let sources = args.sources;
    // The destination owns all semantic limits; the host reader has no duplicate knowledge constants.
    // 全部语义边界归目标所有；宿主读取器不重复定义知识库常量。
    let batch = tokio::task::spawn_blocking(move || {
        collect_local_text_sources(
            &root,
            &sources,
            MAX_KNOWLEDGE_INGEST_FILES,
            MAX_KNOWLEDGE_BODY_BYTES,
            MAX_KNOWLEDGE_INGEST_BYTES,
        )
    })
    .await
    .map_err(|error| NativeToolError::new(format!("ERR_KNOWLEDGE_TRANSFER_TASK: {error}")))?
    .map_err(|error| NativeToolError::new(format!("{}: {}", error.code, error.message)))?;
    // Glob-relative structure is preserved; normalization and conflict checks belong to storage.
    // 保留相对 glob 的结构；规范化及冲突检查归存储所有。
    let paths: Vec<String> = batch
        .sources
        .iter()
        .map(|source| {
            if target_folder.is_empty() {
                source.relative_path.clone()
            } else {
                format!("{}/{}", target_folder, source.relative_path)
            }
        })
        .collect();
    // Summaries for rejected or unmatched sources remain explicit per-item diagnostics.
    // 被拒绝或未匹配来源的摘要保持明确逐项诊断。
    let (mut summaries, unapplied_summaries) =
        match_summaries(&batch.sources, &paths, args.summaries)?;
    // A single observation time binds all snapshots in this atomic ingest call.
    // 单个观测时间绑定此原子搬运调用中的全部快照。
    let at_ms = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| NativeToolError::new(error.to_string()))?
            .as_millis(),
    )
    .map_err(|_| NativeToolError::new("ERR_KNOWLEDGE_CLOCK_RANGE"))?;
    // Source data stays host-side; only this bounded heading projection is later rendered.
    // 来源数据留在宿主侧；仅此有界标题投影之后被渲染。
    let mut outlines = BTreeMap::new();
    // The byte cap applies to the entire invocation, not separately to each source.
    // 字节上限作用于整个调用，而非每份来源独立计算。
    let mut outline_bytes = 0usize;
    // Accepted files become one transaction, with original byte digests as provenance.
    // 已接受文件成为一笔事务，并以原始字节摘要作为来源证据。
    let mut documents = Vec::with_capacity(batch.sources.len());
    for (index, (source, path)) in batch.sources.into_iter().zip(paths).enumerate() {
        // Title derivation reuses the chunker's parser and has a bounded metadata projection.
        // 标题派生复用切块器解析器，并形成有界元数据投影。
        let title: String = extract_document_title(
            &source.text,
            source
                .relative_path
                .rsplit('/')
                .next()
                .ok_or_else(|| NativeToolError::new("ERR_KNOWLEDGE_SOURCE_PATH"))?,
        )
        .chars()
        .take(MAX_KNOWLEDGE_TITLE_CHARS)
        .collect();
        // Fenced code headings are excluded by the shared Markdown parser.
        // 共享 Markdown 解析器排除围栏代码中的标题。
        let outline = extract_document_outline(
            &source.text,
            MAX_KNOWLEDGE_OUTLINE_BYTES.saturating_sub(outline_bytes),
        );
        outline_bytes += outline.0.iter().map(String::len).sum::<usize>();
        outlines.insert(source.source_path.clone(), outline);
        documents.push(KnowledgeIngestDocument {
            path,
            title,
            body: source.text,
            summary: summaries.remove(&index),
            provenance: KnowledgeProvenance {
                kind: "file".into(),
                reference: source.source_path,
                sha256: Some(source.sha256),
                observed_at_ms: at_ms,
            },
            embed: args.embed,
        });
    }
    // All independently rejected files are reported even when no valid source remains.
    // 即使没有有效来源，也报告全部独立拒绝文件。
    let receipt = runtime
        .ingest_documents(library_id, project_id, documents, actor, &args.note)
        .await
        .map_err(NativeToolError::new)?;
    // Metadata-only receipts are safe to render without source body duplication.
    // 仅元数据回执可安全渲染而不重复来源正文。
    let mut output = result_markdown::write(library_id, &receipt);
    for document in &receipt.documents {
        // Ingest sets exact file provenance; path normalization cannot change this source identity.
        // 搬运设置精确文件来源；路径规范化无法改变此来源身份。
        for source in document
            .provenance
            .iter()
            .filter(|source| source.kind == "file")
        {
            if let Some((headings, truncated)) = outlines.get(&source.reference) {
                output.push_str(&format!(
                    "\nOutline for {}:\n",
                    vulcan_tools_manager::result_markdown::inline_code(&document.path)
                ));
                for heading in headings {
                    output.push_str(&format!(
                        "- {}\n",
                        vulcan_tools_manager::result_markdown::inline_code(heading)
                    ));
                }
                if *truncated {
                    output.push_str(
                        "Outline truncated by the heading count or aggregate byte limit.\n",
                    );
                }
            }
        }
    }
    append_failures(&mut output, &batch.failures);
    for source in unapplied_summaries {
        output.push_str(&format!(
            "\nERR_KNOWLEDGE_SUMMARY_NOT_APPLIED: source {} was not successfully transferred.\n",
            vulcan_tools_manager::result_markdown::inline_code(&source)
        ));
    }
    Ok(write_result(library_id, &receipt, output))
}

/// Assigns each summary key to the accepted source it names: the canonical source path, the
/// exact non-glob selector as requested, or the logical library destination path.
/// 将每个摘要键分配给其指名的已接受来源：规范来源路径、按请求原文的精确非 glob 选择器，
/// 或逻辑库目标路径。
/// `sources` and `paths` are index-aligned. Returns summaries by source index plus keys naming
/// no accepted source; a key naming two sources or two keys naming one source reject the call.
/// `sources` 与 `paths` 按下标对齐。返回按来源下标的摘要及未指名任何已接受来源的键；
/// 一个键指名两个来源或两个键指名同一来源时拒绝调用。
fn match_summaries(
    sources: &[LocalTextSource],
    paths: &[String],
    summaries: BTreeMap<String, String>,
) -> Result<(BTreeMap<usize, String>, Vec<String>), NativeToolError> {
    // Every accepted alias points at the source indexes it names.
    // 每个已接受别名指向其指名的来源下标。
    let mut aliases: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    for (index, (source, path)) in sources.iter().zip(paths).enumerate() {
        let logical = normalize_knowledge_path(path, false).map_or_else(|_| path.clone(), |n| n.1);
        for alias in [source.source_path.clone(), logical]
            .into_iter()
            .chain(source.selectors.iter().cloned())
        {
            aliases.entry(alias).or_default().insert(index);
        }
    }
    let mut matched = BTreeMap::new();
    let mut unapplied = Vec::new();
    for (key, summary) in summaries {
        // Logical keys are compared in the same path-key form storage uses for uniqueness.
        // 逻辑键按存储用于唯一性的相同路径键形式比较。
        let logical = normalize_knowledge_path(&key, false).ok().map(|n| n.1);
        let targets: BTreeSet<usize> = [Some(&key), logical.as_ref()]
            .into_iter()
            .flatten()
            .filter_map(|alias| aliases.get(alias))
            .flatten()
            .copied()
            .collect();
        let mut targets = targets.into_iter();
        match (targets.next(), targets.next()) {
            (None, _) => unapplied.push(key),
            (Some(index), None) => {
                if matched.insert(index, summary).is_some() {
                    return Err(NativeToolError::new(format!(
                        "ERR_KNOWLEDGE_SUMMARY_DUPLICATE: more than one summaries key names {}",
                        paths[index]
                    )));
                }
            }
            (Some(_), Some(_)) => {
                return Err(NativeToolError::new(format!(
                    "ERR_KNOWLEDGE_SUMMARY_AMBIGUOUS: summaries key {key} names more than one source"
                )));
            }
        }
    }
    Ok((matched, unapplied))
}

/// Appends each native source `failure` to `output`, with exact identity and stable code.
/// 将每项原生来源 `failure` 及其精确身份、稳定代码追加至 `output`。
/// Returns nothing; file contents are deliberately absent from these diagnostics.
/// 无返回值；这些诊断特意不包含文件内容。
fn append_failures(output: &mut String, failures: &[LocalTextSourceFailure]) {
    for failure in failures {
        output.push_str(&format!(
            "\nRejected source {} [{}]: {}\n",
            vulcan_tools_manager::result_markdown::inline_code(&failure.source_path),
            failure.code,
            failure.error
        ));
    }
}
