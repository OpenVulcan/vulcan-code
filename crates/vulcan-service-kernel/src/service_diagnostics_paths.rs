//! service diagnostics paths helpers.
//! service diagnostics paths 辅助逻辑。

use super::*;

/// Appends one WebUI-visible service diagnostic message to the retained startup log buffer.
/// 向保留的启动日志缓冲区追加一条 WebUI 可见的服务诊断消息。
///
/// # Parameters
/// 参数
/// `state` owns the retained service snapshot.
/// `state` 持有保留的服务快照。
/// `message` is the user-visible diagnostic text.
/// `message` 是用户可见的诊断文本。
///
/// # Returns
/// 返回值
/// `Ok(())` after the diagnostic has been retained.
/// 诊断信息完成保留后返回 `Ok(())`。
pub(crate) fn push_webui_service_diagnostic(
    state: &SharedServiceBridgeState,
    message: String,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    guard.startup.recent_logs.push(message);
    if guard.startup.recent_logs.len() > 200 {
        let overflow = guard.startup.recent_logs.len().saturating_sub(200);
        guard.startup.recent_logs.drain(0..overflow);
    }
    Ok(())
}

/// Creates one diagnostic log entry.
/// 创建单条诊断日志。
pub(crate) fn log_entry(id: &str, level: &str, source: &str, message: &str) -> LogEntrySummary {
    LogEntrySummary {
        id: id.to_string(),
        level: level.to_string(),
        source: source.to_string(),
        message: message.to_string(),
    }
}

/// Returns the repository root used by the local service bridge.
/// 返回本地服务桥接层使用的仓库根目录。
pub(crate) fn workspace_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Default unified-diff context lines used by Git when no expansion is requested.
/// 未请求展开时 Git 默认使用的 unified-diff 上下文行数。
pub(crate) const WEBUI_DIFF_DEFAULT_CONTEXT_LINES: usize = 3;

/// Converts one filesystem path into a stable native display and persistence string.
/// 将单个文件系统路径转换成稳定的原生展示与持久化字符串。
pub(crate) fn normalized_workspace_path_text_from_path(path: &Path) -> String {
    normalize_workspace_path_text(&path.to_string_lossy())
}

/// Normalizes one raw workspace path string and strips Windows verbatim prefixes.
/// 规范化单个原始工作区路径字符串，并移除 Windows verbatim 前缀。
pub(crate) fn normalize_workspace_path_text(path: &str) -> String {
    let trimmed_path = path.trim();
    if trimmed_path.is_empty() {
        return String::new();
    }
    let path_without_verbatim_prefix = strip_windows_verbatim_prefix(trimmed_path);
    if looks_like_windows_path(trimmed_path, &path_without_verbatim_prefix) {
        path_without_verbatim_prefix.replace('/', "\\")
    } else {
        path_without_verbatim_prefix
    }
}

/// Returns whether one path should be displayed with Windows separators.
/// 返回单个路径是否应使用 Windows 分隔符显示。
pub(crate) fn looks_like_windows_path(original_path: &str, normalized_path: &str) -> bool {
    let bytes = normalized_path.as_bytes();
    bytes.len() >= 3
        && bytes[1] == b':'
        && bytes[0].is_ascii_alphabetic()
        && matches!(bytes[2], b'/' | b'\\')
        || normalized_path.starts_with(r"\\")
        || original_path.starts_with(r"\\?\UNC\")
        || original_path.starts_with("//?/UNC/")
        || (cfg!(windows) && normalized_path.starts_with("//"))
}

/// Removes Windows verbatim path prefixes while preserving normal addressing semantics.
/// 移除 Windows verbatim 路径前缀，同时保留普通寻址语义。
pub(crate) fn strip_windows_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    if let Some(rest) = path.strip_prefix("//?/UNC/") {
        return format!("//{rest}");
    }
    if let Some(rest) = path.strip_prefix(r"\\?\") {
        return rest.to_string();
    }
    if let Some(rest) = path.strip_prefix("//?/") {
        return rest.to_string();
    }
    path.to_string()
}
