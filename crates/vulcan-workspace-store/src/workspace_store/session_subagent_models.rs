//! Single-file persistence for a session's own subagent model allowlist.
//! 会话自有子代理模型白名单的单文件持久化。
//!
//! The file exists only while the session replaces the system default list; removing it returns
//! the session to the system default. The caller owns the document type and its version.
//! 只有会话替换系统默认清单时该文件才存在；删除文件即恢复系统默认。文档类型及其版本由调用方负责。

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::SessionV1StorageLocator;
use super::session_v1::documents::write_json_document;

/// File name of the session allowlist inside the session directory.
/// 会话目录中会话白名单的文件名。
const SESSION_SUBAGENT_MODELS_FILE: &str = "subagent-models.json";

/// Loads the session allowlist document, or `None` when the session follows the system default.
/// 加载会话白名单文档；会话跟随系统默认时返回 `None`。
/// Malformed files are explicit errors, never treated as absent.
/// 损坏的文件返回明确错误，绝不视为不存在。
pub fn load_session_subagent_models<T: DeserializeOwned>(
    locator: &SessionV1StorageLocator,
) -> Result<Option<T>, String> {
    let path = locator
        .session_directory()
        .join(SESSION_SUBAGENT_MODELS_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Read {}: {error}", path.display())),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| format!("Parse {}: {error}", path.display()))
}

/// Saves the session allowlist document, or removes it when `document` is `None`.
/// 保存会话白名单文档；`document` 为 `None` 时删除该文件。
pub fn save_session_subagent_models<T: Serialize>(
    locator: &SessionV1StorageLocator,
    document: Option<&T>,
) -> Result<(), String> {
    let path = locator
        .session_directory()
        .join(SESSION_SUBAGENT_MODELS_FILE);
    match document {
        Some(document) => write_json_document(&path, document, "session subagent models")
            .map_err(|error| error.to_string()),
        None => match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("Remove {}: {error}", path.display())),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_store::session_v1_store::create_session_v1_storage_fixture;

    /// An absent file follows the system default; a saved list reloads and removal restores absence.
    /// 文件缺失表示跟随系统默认；保存的清单可重新加载，删除后恢复为缺失。
    #[test]
    fn session_allowlist_round_trips_and_resets() {
        let temporary = tempfile::tempdir().unwrap();
        let locator =
            create_session_v1_storage_fixture(temporary.path(), "subagent-models", 1).unwrap();
        assert_eq!(
            load_session_subagent_models::<Vec<String>>(&locator).unwrap(),
            None
        );
        let saved = vec![String::from("provider/model")];
        save_session_subagent_models(&locator, Some(&saved)).unwrap();
        assert_eq!(
            load_session_subagent_models::<Vec<String>>(&locator).unwrap(),
            Some(saved)
        );
        save_session_subagent_models::<Vec<String>>(&locator, None).unwrap();
        assert_eq!(
            load_session_subagent_models::<Vec<String>>(&locator).unwrap(),
            None
        );
    }

    /// A damaged file is an error rather than the system default.
    /// 损坏的文件是错误，而不是系统默认。
    #[test]
    fn damaged_session_allowlist_is_an_error() {
        let temporary = tempfile::tempdir().unwrap();
        let locator =
            create_session_v1_storage_fixture(temporary.path(), "subagent-models", 1).unwrap();
        std::fs::write(
            locator
                .session_directory()
                .join(SESSION_SUBAGENT_MODELS_FILE),
            "not json",
        )
        .unwrap();
        assert!(load_session_subagent_models::<Vec<String>>(&locator).is_err());
    }
}
