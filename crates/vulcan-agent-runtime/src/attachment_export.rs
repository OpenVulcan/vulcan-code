//! Host-bound attachment export: copy one owned attachment into the workspace for ordinary tools.
//! 宿主绑定附件导出：把一个所属附件复制到工作区，供普通工具使用。

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use vulcan_provider_adapters::ProviderToolMediaSourceResolver;
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolInvocationResult,
    VulcanToolOrigin, VulcanToolSourceKind,
};

/// Stable identifier of the attachment export tool, also named by metadata-only attachment notices.
/// 附件导出工具的稳定标识，仅元数据附件说明也引用该标识。
pub const ATTACHMENT_EXPORT_TOOL_ID: &str = "attachment_export";

/// Workspace-relative directory below `.vulcan/temp` that holds exported attachment copies.
/// `.vulcan/temp` 下保存导出附件副本的工作区相对目录。
const EXPORT_DIRECTORY_NAME: &str = "attachments";

/// Leading digest characters naming one export directory; equal content shares one copy.
/// 命名单个导出目录的摘要前缀字符数；相同内容共用一份副本。
const EXPORT_DIGEST_PREFIX_CHARS: usize = 16;

/// Chunk size used when copying or hashing an attachment.
/// 复制或哈希附件时使用的分块大小。
const COPY_CHUNK_BYTES: usize = 64 * 1024;

/// Name used when the original file name has no usable characters left after sanitizing.
/// 原文件名清理后没有可用字符时使用的名称。
const FALLBACK_FILE_NAME: &str = "attachment";

/// Returns the sentence a metadata-only attachment notice uses to point the model at the export tool.
/// 返回仅元数据附件说明用来指引模型使用导出工具的句子。
/// The notice cannot know the final tool list, so the sentence is conditional on the tool being available.
/// 说明无法得知最终工具列表，因此句子以工具可用为前提。
pub fn metadata_only_export_hint() -> String {
    format!(
        "If {ATTACHMENT_EXPORT_TOOL_ID} is in your available tools and the user needs this attachment's content, call it with this resource_path to get a workspace copy, then open that copy with another tool (for example a script that extracts document text); otherwise explain that the content cannot be read."
    )
}

/// Builds the export tool for an execution with a workspace; `resources` authorizes project locators.
/// 为具有工作区的执行构建导出工具；`resources` 授权项目定位符。
/// Returns no tool without a workspace, because the copy must land where ordinary tools can read it.
/// 没有工作区时不返回工具，因为副本必须落在普通工具可读取的位置。
pub(crate) fn build_attachment_export_candidate(
    workspace_root: Option<PathBuf>,
    resources: Option<Arc<dyn crate::media_resource_runtime::MediaResourceRuntime>>,
) -> Option<VulcanToolCandidate> {
    let workspace_root = workspace_root?;
    Some(VulcanToolCandidate {
        descriptor: VulcanToolDescriptor::new(
            ATTACHMENT_EXPORT_TOOL_ID,
            ATTACHMENT_EXPORT_TOOL_ID,
            "Copy one user attachment into the workspace and return its file path, so ordinary tools (file reads, shell commands, scripts) can open it. Use it when an attachment is listed only by its `project-resource://` or `attachment://` locator, for example because the current model or route cannot receive that file type directly. The copy keeps the original file name and extension under `.vulcan/temp/attachments/`; the original attachment is not changed. It does not read or interpret the content: open the returned path with another tool, for example a script that extracts text from a PDF, DOCX or PPTX file.",
            None,
            VulcanToolOrigin::new(
                VulcanToolSourceKind::AiHostBinding,
                "vulcan",
                ATTACHMENT_EXPORT_TOOL_ID,
            ),
        ),
        parameters_schema: json!({"type":"object","additionalProperties":false,"properties":{"resource_path":{"type":"string","minLength":1,"description":"Exact `project-resource://<id>` or `attachment://<id>` locator from the attachment listing. Local file paths are not accepted; they are already readable by other tools."}},"required":["resource_path"]}),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
        invoke: Arc::new(move |arguments, context| {
            // Each invocation keeps the execution's workspace and session-to-project port.
            // 每次调用保留执行的工作区及会话至项目端口。
            let workspace_root = workspace_root.clone();
            let resources = resources.clone();
            Box::pin(async move {
                export_attachment(&workspace_root, resources, &context.session_id, arguments).await
            })
        }),
    })
}

/// Resolves the locator in `arguments` under `session_id`, copies it below `workspace_root`, and reports the copy.
/// 在 `session_id` 下解析 `arguments` 中的定位符，复制到 `workspace_root` 下并报告副本。
/// Returns an error when the locator is not owned, the source changed, or the copy could not be written.
/// 定位符不属于本会话、来源已变化或副本无法写入时返回错误。
pub(crate) async fn export_attachment(
    workspace_root: &Path,
    resources: Option<Arc<dyn crate::media_resource_runtime::MediaResourceRuntime>>,
    session_id: &str,
    arguments: Value,
) -> Result<VulcanToolInvocationResult, String> {
    // Closed arguments keep the locator the only input.
    // 封闭参数使定位符成为唯一输入。
    let object = arguments
        .as_object()
        .ok_or("attachment_export requires an object containing only resource_path.")?;
    if object.len() != 1 {
        return Err("attachment_export requires exactly one resource_path.".into());
    }
    let locator = object
        .get("resource_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|locator| !locator.is_empty())
        .ok_or("attachment_export requires a nonempty resource_path.")?;
    if !locator.starts_with("project-resource:") && !locator.starts_with("attachment:") {
        return Err("attachment_export accepts only a `project-resource://` or `attachment://` locator; local paths are already readable by other tools.".into());
    }
    // The shared resolver enforces session ownership for both locator families.
    // 共享解析器为两种定位符族执行会话归属校验。
    let source = crate::media_resource_runtime::SessionProviderToolMediaSourceResolver {
        runtime: resources.as_deref(),
        workspace_root: Some(workspace_root),
        session_id,
    }
    .resolve_source(locator)
    .await?;
    let digest = source.source_identity.content_sha256.clone();
    let digest_prefix = digest
        .get(..EXPORT_DIGEST_PREFIX_CHARS)
        .filter(|prefix| prefix.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or("Resolved attachment has no valid registered digest.")?
        .to_string();
    let file_name = sanitize_file_name(&source.file_name);
    let relative = format!(
        ".vulcan/temp/{EXPORT_DIRECTORY_NAME}/{digest_prefix}/{file_name}"
    );
    let directory = crate::project_temp_maintenance::resolve_project_temp_dir(workspace_root)
        .join(EXPORT_DIRECTORY_NAME)
        .join(&digest_prefix);
    let destination = directory.join(&file_name);
    let byte_len = source.source_identity.byte_len;
    // File copying and hashing are blocking work.
    // 文件复制与哈希属于阻塞工作。
    let copy_destination = destination.clone();
    let copy_digest = digest.clone();
    tokio::task::spawn_blocking(move || {
        copy_verified(
            &source.local_path,
            &directory,
            &copy_destination,
            byte_len,
            &copy_digest,
        )
    })
    .await
    .map_err(|_| String::from("Attachment export task failed."))??;
    Ok(VulcanToolInvocationResult::from_model_output(json!({
        "path": relative,
        "absolute_path": vulcan_runtime_assets::native_host_path_text_from_path(&destination),
        "file_name": file_name,
        "mime_type": source.mime_type,
        "byte_len": byte_len,
        "sha256": digest,
        "note": "This is a copy; open it with another tool. Relative paths resolve from the workspace root."
    })))
}

/// Copies `source` to `destination` inside `directory` when it matches `byte_len` and `digest`.
/// 当 `source` 与 `byte_len` 及 `digest` 一致时，将其复制到 `directory` 内的 `destination`。
/// An existing identical copy is reused; the new copy is written to a temporary file and renamed into place.
/// 已有相同副本直接复用；新副本先写入临时文件再改名就位。
fn copy_verified(
    source: &Path,
    directory: &Path,
    destination: &Path,
    byte_len: u64,
    digest: &str,
) -> Result<(), String> {
    if destination.is_file() && file_matches(destination, byte_len, digest)? {
        return Ok(());
    }
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("Could not create the export directory: {error}"))?;
    let mut input = std::fs::File::open(source)
        .map_err(|error| format!("Could not open the attachment source: {error}"))?;
    let mut output = tempfile::NamedTempFile::new_in(directory)
        .map_err(|error| format!("Could not create the export file: {error}"))?;
    let mut hasher = Sha256::new();
    let mut copied = 0_u64;
    let mut buffer = vec![0_u8; COPY_CHUNK_BYTES];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|error| format!("Could not read the attachment source: {error}"))?;
        if read == 0 {
            break;
        }
        copied += read as u64;
        // A source that grows past its registered size is rejected without copying the rest.
        // 超出登记大小的来源直接拒绝，不再复制剩余内容。
        if copied > byte_len {
            return Err("Attachment source changed from its registered identity.".into());
        }
        hasher.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(|error| format!("Could not write the export file: {error}"))?;
    }
    if copied != byte_len || hex::encode(hasher.finalize()) != digest {
        return Err("Attachment source changed from its registered identity.".into());
    }
    output
        .persist(destination)
        .map_err(|error| format!("Could not place the export file: {}", error.error))?;
    Ok(())
}

/// Reports whether the file at `path` has exactly `byte_len` bytes and SHA-256 `digest`.
/// 报告 `path` 处文件是否恰好为 `byte_len` 字节且 SHA-256 为 `digest`。
fn file_matches(path: &Path, byte_len: u64, digest: &str) -> Result<bool, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("Could not inspect the existing export: {error}"))?;
    if metadata.len() != byte_len {
        return Ok(false);
    }
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("Could not open the existing export: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; COPY_CHUNK_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not read the existing export: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()) == digest)
}

/// Returns `name` reduced to one portable file-name component that keeps its extension.
/// 将 `name` 化为一个保留扩展名的可移植文件名分量。
/// Path separators, control characters and characters Windows forbids become `_`.
/// 路径分隔符、控制字符及 Windows 禁止的字符替换为 `_`。
fn sanitize_file_name(name: &str) -> String {
    let replaced = name
        .chars()
        .map(|character| {
            if character.is_control() || "<>:\"/\\|?*".contains(character) {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    // Windows drops trailing dots and spaces, and a name of only dots would mean a directory.
    // Windows 会去掉末尾的点与空格，仅由点组成的名称会指向目录。
    let trimmed = replaced.trim().trim_end_matches(['.', ' ']);
    if trimmed.is_empty() || trimmed.chars().all(|character| character == '.') {
        return FALLBACK_FILE_NAME.to_string();
    }
    let stem = trimmed.split('.').next().unwrap_or_default();
    let reserved = matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON" | "PRN" | "AUX" | "NUL"
    ) || stem.len() == 4
        && ["COM", "LPT"]
            .iter()
            .any(|prefix| stem.to_ascii_uppercase().starts_with(prefix))
        && stem.as_bytes()[3].is_ascii_digit();
    if reserved {
        return format!("_{trimmed}");
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests;
