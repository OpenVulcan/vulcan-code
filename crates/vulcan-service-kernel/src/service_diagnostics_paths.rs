//! service diagnostics paths helpers.
//! service diagnostics paths 辅助逻辑。

use super::*;

/// Number of most recent diagnostic lines kept in the startup log buffer.
/// 启动日志缓冲区保留的最近诊断行数。
const STARTUP_RECENT_LOGS_CAPACITY: usize = 200;

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
    if guard.startup.recent_logs.len() > STARTUP_RECENT_LOGS_CAPACITY {
        let overflow = guard
            .startup
            .recent_logs
            .len()
            .saturating_sub(STARTUP_RECENT_LOGS_CAPACITY);
        guard.startup.recent_logs.drain(0..overflow);
    }
    Ok(())
}

/// Retains one sanitized slow-submission diagnostic in both WebUI diagnostic buffers.
/// 在两个 WebUI 诊断缓冲区中保留一条经过净化的慢提交诊断。
///
/// # Parameters
/// 参数
/// `state` owns the retained startup and workbench diagnostic buffers.
/// `state` 持有保留的启动与工作台诊断缓冲区。
/// `client_submission_id` correlates browser acceptance with later preparation stages.
/// `client_submission_id` 关联浏览器接纳与后续准备阶段。
/// `target` identifies whether the submission creates or continues a conversation.
/// `target` 标识提交是新建会话还是继续已有会话。
/// `stage` names the bounded execution stage without including prompt content.
/// `stage` 命名有界执行阶段，且不包含提示词内容。
/// `elapsed_ms` is the measured stage duration in milliseconds.
/// `elapsed_ms` 是测得的阶段耗时毫秒数。
///
/// # Returns
/// 返回值
/// Returns success after retention, or a precise service-state locking error.
/// 保留完成后返回成功，否则返回精确的服务状态加锁错误。
pub(crate) fn retain_slow_submission_diagnostic(
    state: &SharedServiceBridgeState,
    client_submission_id: &str,
    target: &str,
    stage: &str,
    elapsed_ms: i128,
) -> Result<(), String> {
    if elapsed_ms <= 200 {
        return Ok(());
    }
    // safe_token removes user-authored or delimiter-bearing text from the diagnostic identity.
    // safe_token 从诊断标识中移除用户撰写内容或带分隔符的文本。
    let safe_token = |value: &str| {
        value
            .chars()
            .filter(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
            })
            .take(96)
            .collect::<String>()
    };
    let client_submission_id = safe_token(client_submission_id);
    let target = safe_token(target);
    let stage = safe_token(stage);
    let message = format!(
        "client_submission_id={client_submission_id} target={target} stage={stage} elapsed_ms={elapsed_ms}"
    );
    tracing::warn!("[webui-service][submit-slow] {message}");
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    guard
        .startup
        .recent_logs
        .push(format!("[submit-slow] {message}"));
    if guard.startup.recent_logs.len() > STARTUP_RECENT_LOGS_CAPACITY {
        let overflow = guard
            .startup
            .recent_logs
            .len()
            .saturating_sub(STARTUP_RECENT_LOGS_CAPACITY);
        guard.startup.recent_logs.drain(0..overflow);
    }
    let entry_id = format!(
        "submit-slow-{}-{}-{}",
        client_submission_id,
        stage,
        current_unix_millis_text()
    );
    guard
        .workbench
        .logs
        .push(log_entry(&entry_id, "warning", "submission", &message));
    if guard.workbench.logs.len() > 200 {
        let overflow = guard.workbench.logs.len().saturating_sub(200);
        guard.workbench.logs.drain(0..overflow);
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
    vulcan_runtime_assets::native_host_path_text_from_path(path)
}
