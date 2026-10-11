//! Workspace file read response helpers.
//! 工作区文件 read response 辅助逻辑。

use super::*;

/// Builds the shared metadata shell for one workspace file read response.
/// 构建单个工作区文件读取响应的共享元数据外壳。
///
/// # Parameters
/// 参数
/// `workspace_root` is the canonical workspace root.
/// `workspace_root` 是规范化工作区根。
/// `file` is the canonical file path.
/// `file` 是规范化文件路径。
/// `metadata` is the already loaded file metadata.
/// `metadata` 是已加载的文件元数据。
/// `kind` is the already classified file kind.
/// `kind` 是已完成分类的文件类别。
///
/// # Returns
/// 返回值
/// A response with path, kind, MIME, and size populated while content fields stay empty.
/// 返回已填充路径、类别、MIME 和大小且内容字段为空的响应。
fn workspace_file_read_response_shell(
    workspace_root: &Path,
    file: &Path,
    metadata: &fs::Metadata,
    kind: WorkspaceFileEntryKind,
) -> WorkspaceFileReadResponse {
    WorkspaceFileReadResponse {
        workspace_path: normalized_workspace_path_text_from_path(workspace_root),
        path: normalized_workspace_path_text_from_path(file),
        relative_path: workspace_file_relative_path_text(workspace_root, file),
        mime_type: workspace_file_mime_type(file, &kind),
        size_bytes: metadata.len(),
        content: None,
        encoding: None,
        line_count: None,
        truncated: false,
        kind,
    }
}

/// Reads one code file with a bounded UTF-8 preview limit.
/// 使用有界 UTF-8 预览限制读取单个代码文件。
///
/// # Parameters
/// 参数
/// `workspace_root` is the canonical workspace root.
/// `workspace_root` 是规范化工作区根。
/// `file` is the canonical file path to read.
/// `file` 是需要读取的规范化文件路径。
/// `metadata` is the already loaded file metadata.
/// `metadata` 是已加载的文件元数据。
/// `kind` is the known code file kind.
/// `kind` 是已知代码文件类别。
///
/// # Returns
/// 返回值
/// A bounded code preview response.
/// 返回有界代码预览响应。
pub(crate) fn workspace_file_read_code_response(
    workspace_root: &Path,
    file: &Path,
    metadata: &fs::Metadata,
    kind: WorkspaceFileEntryKind,
) -> Result<WorkspaceFileReadResponse, String> {
    let mut handle = fs::File::open(file).map_err(|error| {
        format!(
            "Failed to open file `{}`: {error}",
            normalized_workspace_path_text_from_path(file)
        )
    })?;
    let mut bytes =
        Vec::with_capacity(WORKSPACE_FILE_READ_LIMIT_BYTES.min(metadata.len() as usize));
    Read::by_ref(&mut handle)
        .take((WORKSPACE_FILE_READ_LIMIT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            format!(
                "Failed to read file `{}`: {error}",
                normalized_workspace_path_text_from_path(file)
            )
        })?;
    let truncated = bytes.len() > WORKSPACE_FILE_READ_LIMIT_BYTES;
    if truncated {
        bytes.truncate(WORKSPACE_FILE_READ_LIMIT_BYTES);
    }
    if bytes.contains(&0) {
        return Err(String::from(
            "This file contains binary content and cannot be previewed as code.",
        ));
    }
    let decoded = workspace_file_decode_text_preview(&bytes);
    let line_count = decoded.content.lines().count();
    let mut response = workspace_file_read_response_shell(workspace_root, file, metadata, kind);
    response.content = Some(decoded.content);
    response.encoding = Some(decoded.encoding);
    response.line_count = Some(line_count);
    response.truncated = truncated;
    Ok(response)
}

/// Stores decoded text and the encoding label used for a workspace file preview.
/// 保存工作区文件预览使用的解码文本与编码标签。
pub(crate) struct WorkspaceFileDecodedText {
    /// Decoded text content ready for WebUI rendering.
    /// 可供 WebUI 渲染的解码文本内容。
    pub(crate) content: String,
    /// Stable encoding label reported to the WebUI.
    /// 返回给 WebUI 的稳定编码标签。
    pub(crate) encoding: String,
}

/// Decodes workspace text preview bytes with UTF-8 first and GB18030 fallback.
/// 优先使用 UTF-8 解码工作区文本预览字节，并回退到 GB18030。
pub(crate) fn workspace_file_decode_text_preview(bytes: &[u8]) -> WorkspaceFileDecodedText {
    if let Err(error) = std::str::from_utf8(bytes)
        && error.error_len().is_none()
        && error.valid_up_to() > 0
    {
        return WorkspaceFileDecodedText {
            content: String::from_utf8_lossy(&bytes[..error.valid_up_to()]).into_owned(),
            encoding: String::from("utf-8"),
        };
    }
    if let Ok(decoded) = decode_complete_user_text(bytes) {
        return WorkspaceFileDecodedText {
            content: decoded.content,
            encoding: decoded.encoding.to_owned(),
        };
    }

    // Preview buffers may end inside a GB18030 character because the read limit is byte-based.
    // 预览缓冲区按字节限制读取，因此可能恰好结束在一个 GB18030 字符内部。
    let (content, _, _) = GB18030.decode(bytes);
    WorkspaceFileDecodedText {
        content: content.into_owned(),
        encoding: String::from("gb18030"),
    }
}

/// Builds one media metadata response without reading the file bytes.
/// 构建单个不读取文件字节的媒体元数据响应。
///
/// # Parameters
/// 参数
/// `workspace_root` is the canonical workspace root.
/// `workspace_root` 是规范化工作区根。
/// `file` is the canonical media file path.
/// `file` 是规范化媒体文件路径。
/// `metadata` is the already loaded file metadata.
/// `metadata` 是已加载的文件元数据。
/// `kind` is the media kind.
/// `kind` 是媒体类别。
///
/// # Returns
/// 返回值
/// A media metadata response for frontend URL-based rendering.
/// 返回供前端基于 URL 渲染的媒体元数据响应。
pub(crate) fn workspace_file_media_response(
    workspace_root: &Path,
    file: &Path,
    metadata: &fs::Metadata,
    kind: WorkspaceFileEntryKind,
) -> WorkspaceFileReadResponse {
    workspace_file_read_response_shell(workspace_root, file, metadata, kind)
}
