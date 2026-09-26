//! Conversation-only file projections; shared file-runtime JSON remains available to machine callers.
//! 仅供会话的文件投影；共享文件运行时 JSON 继续供机器调用方使用。

use serde::Deserialize;
use serde_json::Value;
use vulcan_host_tools::file_runtime::FileToolResponse;
use vulcan_tools_manager::{
    VulcanToolHostResult, VulcanToolInvocationResult, result_markdown::inline_code,
};

/// Exact list/stat variants produced by execute_native_file_sync and native_list_files.
/// execute_native_file_sync 与 native_list_files 产出的精确列表和属性变体。
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Inspection<'a> {
    /// Directory discovery with an explicit truncation fact.
    /// 带显式截断事实的目录发现。
    List {
        /// Resolved source path.
        /// 已解析的来源路径。
        path: &'a str,
        /// Ordered entries owned by this response.
        /// 当前响应拥有的有序条目。
        #[serde(borrow)]
        entries: Vec<Entry<'a>>,
        /// Whether more entries existed than the bounded result could contain.
        /// 是否存在超出有界结果容量的条目。
        truncated: bool,
    },
    /// File metadata with text properties only when decoding succeeded.
    /// 文件属性，仅在解码成功时包含文本属性。
    Stat {
        /// Resolved path.
        /// 已解析路径。
        path: &'a str,
        /// Actual filesystem kind.
        /// 实际文件系统类型。
        kind: &'a str,
        /// Byte length.
        /// 字节长度。
        size: u64,
        /// Optional observed modification timestamp.
        /// 可选的实际修改时间戳。
        modified: Option<u64>,
        /// Optional decoded encoding.
        /// 可选的解码编码。
        encoding: Option<&'a str>,
        /// Whether encoding detection was certain, when supplied.
        /// 产出时记录的编码探测确定性。
        encoding_certain: Option<bool>,
        /// Optional line ending description.
        /// 可选换行描述。
        line_ending: Option<&'a str>,
        /// Whether a byte-order mark is present in decoded text.
        /// 解码文本是否包含字节序标记。
        bom: Option<bool>,
        /// Decoded logical line count, when available.
        /// 可用时的解码逻辑行数。
        line_count: Option<u64>,
        /// Whether decoded source ends with a newline.
        /// 解码来源是否以换行结束。
        final_newline: Option<bool>,
    },
}

/// Exact list entry fields needed for readable discovery, excluding timestamps for every child.
/// 可读发现所需的精确列表条目字段，不逐个输出子项时间戳。
#[derive(Deserialize)]
struct Entry<'a> {
    /// Resolved child path.
    /// 已解析子项路径。
    path: &'a str,
    /// File or directory.
    /// 文件或目录。
    kind: &'a str,
    /// Actual byte length.
    /// 实际字节长度。
    size: u64,
}

/// Display facts from the existing applied changeset, never its copied source or diff rows.
/// 来自既有已应用变更集的展示事实，不复制源码或差异行。
#[derive(Deserialize)]
struct AppliedFile<'a> {
    /// Final resource path.
    /// 最终资源路径。
    path: &'a str,
    /// Actual create, modify, delete or rename operation.
    /// 实际创建、修改、删除或重命名操作。
    change: &'a str,
    /// Original path supplied only for a rename.
    /// 仅重命名提供的原路径。
    old_path: Option<&'a str>,
    /// Change hunks for edits; absent on lifecycle-only and binary records.
    /// 编辑差异块；纯生命周期和二进制记录不提供。
    hunks: Option<Vec<AppliedHunk>>,
    /// Exact logical line count supplied for text creation/deletion, even when content is omitted.
    /// 文本创建／删除提供的精确逻辑行数，即使正文被省略也保留。
    total_line_count: Option<usize>,
}

/// Counts authoritative changed rows without cloning or returning their source text.
/// 统计权威变更行，不复制或返回其源码正文。
#[derive(Deserialize)]
struct AppliedHunk {
    /// Removed source rows.
    /// 删除的源行。
    delete: Vec<serde::de::IgnoredAny>,
    /// Inserted source rows.
    /// 插入的源行。
    insert: Vec<serde::de::IgnoredAny>,
}

/// Adapts one successful file-runtime response for conversation display and model reading.
/// 将一次成功文件运行时响应适配为会话展示和模型阅读结果。
/// Consumes the response, retaining read mappings or applied changesets only where the UI uses them.
/// 消费响应，仅在前端需要时保留读取映射或已应用变更集。
pub(super) fn project(response: FileToolResponse) -> VulcanToolInvocationResult {
    if let Some(payload) = response.change_set {
        // Applied edits remain visible even if an internal presentation contract is violated.
        // 即使内部展示契约被违反，已应用编辑仍然可见。
        let text = match mutation(&payload, &response.model_output) {
            Ok(text) => text,
            Err(error) => format!(
                "# File operation completed\n\nResult presentation failed: {error}\nThe changes were already applied. Do not repeat the operation solely to recover its display."
            ),
        };
        return VulcanToolInvocationResult::from_model_output(Value::String(text))
            .with_host_result(VulcanToolHostResult {
                kind: String::from("change_set"),
                payload,
            });
    }
    if let Some(payload) = response.read_metadata {
        return VulcanToolInvocationResult::from_model_output(response.model_output)
            .with_host_result(VulcanToolHostResult {
                kind: String::from("file_access"),
                payload,
            });
    }
    match inspection(&response.model_output) {
        Ok(text) => VulcanToolInvocationResult::from_model_output(Value::String(text)),
        Err(error) => VulcanToolInvocationResult::from_tool_error_details(
            format!("File inspection presentation failed: {error}"),
            serde_json::json!({"code": "invalid_file_result", "retryable": false}),
        ),
    }
}

/// Formats the exact inspection contract into Markdown without retaining a host clone.
/// 将精确检查契约格式化为 Markdown，不保留宿主克隆。
/// Accepts a successful list/stat value; returns readable output or a schema error.
/// 接受成功的列表或属性值；返回可读输出或结构错误。
fn inspection(value: &Value) -> Result<String, serde_json::Error> {
    match Inspection::deserialize(value)? {
        Inspection::List {
            path,
            entries,
            truncated,
        } => {
            // The source heading disambiguates empty results and truncated directory listings.
            // 来源标题用于区分空结果与被截断的目录列表。
            let mut text = format!("# Files: {}\n", inline_code(path));
            if entries.is_empty() {
                text.push_str("\nNo matching entries.\n");
            }
            for entry in entries {
                text.push_str(&format!("\n- {} · {}", inline_code(entry.path), entry.kind));
                if entry.kind == "file" {
                    text.push_str(&format!(" · {} bytes", entry.size));
                }
            }
            if truncated {
                text.push_str("\n\nTruncated. Narrow the path or glob, or increase limit within the tool's declared maximum.");
            }
            Ok(text)
        }
        Inspection::Stat {
            path,
            kind,
            size,
            modified,
            encoding,
            encoding_certain,
            line_ending,
            bom,
            line_count,
            final_newline,
        } => {
            // Optional text facts are emitted only when the file runtime actually decoded them.
            // 仅在文件运行时实际解码后，才输出可选文本事实。
            let mut text = format!(
                "# File information: {}\n\nType: {kind}\nSize: {size} bytes\n",
                inline_code(path)
            );
            if let Some(modified) = modified {
                text.push_str(&format!("Modified (Unix ms): {modified}\n"));
            }
            if let Some(encoding) = encoding {
                text.push_str(&format!("Encoding: {}\n", inline_code(encoding)));
            }
            if let Some(false) = encoding_certain {
                text.push_str("Encoding detection is uncertain.\n");
            }
            if let Some(ending) = line_ending {
                text.push_str(&format!("Line endings: {}\n", inline_code(ending)));
            }
            if let Some(bom) = bom {
                text.push_str(&format!("BOM: {bom}\n"));
            }
            if let Some(lines) = line_count {
                text.push_str(&format!("Lines: {lines}\n"));
            }
            if let Some(newline) = final_newline {
                text.push_str(&format!("Final newline: {newline}\n"));
            }
            Ok(text)
        }
    }
}

/// Summarizes actual applied files without returning full operation records to the model.
/// 汇总实际应用的文件，不向模型返回完整操作记录。
/// Accepts the authoritative host changeset and runtime receipt; returns Markdown or a contract error.
/// 接受权威宿主变更集和运行时回执；返回 Markdown 或契约错误。
fn mutation(changeset: &Value, receipt: &Value) -> Result<String, serde_json::Error> {
    // The exact files member is owned by every current native file mutation producer.
    // 精确 files 成员由所有当前原生文件修改产出方拥有。
    let files = Vec::<AppliedFile<'_>>::deserialize(&changeset["files"])?;
    let mut text = String::from("# File changes applied\n");
    if files.is_empty() {
        text.push_str("\nNo files changed.\n");
    }
    for file in files {
        text.push_str(&format!("\n- {}: {}", file.change, inline_code(file.path)));
        if let Some(previous) = file.old_path {
            text.push_str(&format!(" (from {})", inline_code(previous)));
        }
        // Count only facts provided by this changeset; a binary lifecycle has no invented text-line count.
        // 只统计此变更集提供的事实；二进制生命周期不得虚构文本行数。
        let counts = match (file.change, file.hunks.as_ref(), file.total_line_count) {
            (_, Some(hunks), _) => Some(hunks.iter().fold((0, 0), |(added, removed), hunk| {
                (added + hunk.insert.len(), removed + hunk.delete.len())
            })),
            ("create", None, Some(lines)) => Some((lines, 0)),
            ("delete", None, Some(lines)) => Some((0, lines)),
            _ => None,
        };
        if let Some((added, removed)) = counts {
            text.push_str(&format!(" · +{added} / -{removed} lines"));
        }
    }
    // Exact replacement alone supplies a match count; lifecycle and patch results do not.
    // 仅精确替换提供匹配数量；生命周期和补丁结果不提供该字段。
    if let Some(count) = receipt.get("match_count").and_then(Value::as_u64) {
        text.push_str(&format!("\n\nReplacements: {count}"));
    }
    Ok(text)
}
