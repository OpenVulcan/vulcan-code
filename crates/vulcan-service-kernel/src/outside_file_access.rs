//! How the WebUI opens files outside a session's workspace. The system default lives in the
//! shared configuration file; a session may keep its own choice beside its V1 document. The
//! prompt guards against accidental opens and is not a security boundary.
//! WebUI 如何打开会话工作区之外的文件。系统默认值保存在共享配置文件中；会话可以在其 V1 文档旁保存
//! 自有选择。提示用于防止误开，并非安全边界。

use serde::{Deserialize, Serialize};
use vulcan_session_core::OutsideWorkspaceFileAccess;
use vulcan_workspace_store::workspace_store::{
    load_session_outside_file_access, save_session_outside_file_access,
};

use crate::SharedServiceBridgeState;

/// Version of the session outside file access file; other versions are refused.
/// 会话工作区外文件访问方式文件的版本；其他版本一律拒绝。
const SESSION_OUTSIDE_FILE_ACCESS_VERSION: u32 = 1;

/// Session outside file access file stored beside the session document.
/// 保存在会话文档旁的会话工作区外文件访问方式文件。
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionOutsideFileAccessDocument {
    /// File format version.
    /// 文件格式版本。
    version: u32,
    /// The session's own choice.
    /// 会话自有选择。
    access: OutsideWorkspaceFileAccess,
}

/// Accepts only the current file version.
/// 只接受当前文件版本。
fn access_from_document(
    document: SessionOutsideFileAccessDocument,
) -> Result<OutsideWorkspaceFileAccess, String> {
    if document.version != SESSION_OUTSIDE_FILE_ACCESS_VERSION {
        return Err(format!(
            "Session outside file access version {} is not supported.",
            document.version
        ));
    }
    Ok(document.access)
}

/// Session outside file access shown by the WebUI.
/// WebUI 展示的会话工作区外文件访问方式。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionOutsideFileAccessSnapshot {
    /// The value in effect: the session's own choice, else the system default.
    /// 生效值：会话自有选择，否则为系统默认值。
    access: OutsideWorkspaceFileAccess,
    /// The session's own choice; `None` follows the system default.
    /// 会话自有选择；`None` 表示跟随系统默认值。
    session: Option<OutsideWorkspaceFileAccess>,
    /// The system default the session follows without its own choice.
    /// 会话没有自有选择时跟随的系统默认值。
    system: OutsideWorkspaceFileAccess,
}

/// Builds the snapshot from the session's own choice and the projected system default.
/// 根据会话自有选择与已投影的系统默认值构建快照。
fn session_snapshot(
    state: &SharedServiceBridgeState,
    session: Option<OutsideWorkspaceFileAccess>,
) -> Result<SessionOutsideFileAccessSnapshot, String> {
    let system = state
        .lock()
        .map_err(|error| format!("Failed to lock service state: {error}"))?
        .workbench
        .outside_workspace_file_access
        .unwrap_or_default();
    Ok(SessionOutsideFileAccessSnapshot {
        access: session.unwrap_or(system),
        session,
        system,
    })
}

/// Returns one active regular session's outside file access without writing anything.
/// 返回一个活动普通会话的工作区外文件访问方式，不写入任何内容。
pub(crate) async fn service_session_outside_file_access(
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<SessionOutsideFileAccessSnapshot, String> {
    let (_, _lifecycle, handle) =
        crate::permission_mode::session_access_writer(&state, &conversation_id).await?;
    let locator = handle
        .storage_locator()
        .map_err(|error| error.to_string())?;
    let session = load_session_outside_file_access::<SessionOutsideFileAccessDocument>(&locator)?
        .map(access_from_document)
        .transpose()?;
    session_snapshot(&state, session)
}

/// Saves one session's outside file access; `None` returns it to the system default.
/// 保存一个会话的工作区外文件访问方式；`None` 使其恢复跟随系统默认值。
pub(crate) async fn service_save_session_outside_file_access(
    state: SharedServiceBridgeState,
    conversation_id: String,
    access: Option<OutsideWorkspaceFileAccess>,
) -> Result<SessionOutsideFileAccessSnapshot, String> {
    let (_, _lifecycle, handle) =
        crate::permission_mode::session_access_writer(&state, &conversation_id).await?;
    let locator = handle
        .storage_locator()
        .map_err(|error| error.to_string())?;
    let document = access.map(|access| SessionOutsideFileAccessDocument {
        version: SESSION_OUTSIDE_FILE_ACCESS_VERSION,
        access,
    });
    save_session_outside_file_access(&locator, document.as_ref())?;
    session_snapshot(&state, access)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the current file version, known fields and known values are accepted.
    /// 只接受当前文件版本、已知字段与已知取值。
    #[test]
    fn session_outside_file_access_accepts_only_the_current_version() {
        let parse = |value: serde_json::Value| {
            serde_json::from_value::<SessionOutsideFileAccessDocument>(value)
        };
        let current = parse(serde_json::json!({
            "version": SESSION_OUTSIDE_FILE_ACCESS_VERSION,
            "access": "allow",
        }))
        .expect("document shape");
        assert_eq!(
            access_from_document(current),
            Ok(OutsideWorkspaceFileAccess::Allow)
        );
        let newer = parse(serde_json::json!({
            "version": SESSION_OUTSIDE_FILE_ACCESS_VERSION + 1,
            "access": "allow",
        }))
        .expect("document shape");
        assert!(access_from_document(newer).is_err());
        assert!(
            parse(serde_json::json!({
                "version": SESSION_OUTSIDE_FILE_ACCESS_VERSION,
                "access": "always",
            }))
            .is_err()
        );
        assert!(
            parse(serde_json::json!({
                "version": SESSION_OUTSIDE_FILE_ACCESS_VERSION,
                "access": "deny",
                "extra": true,
            }))
            .is_err()
        );
    }
}
