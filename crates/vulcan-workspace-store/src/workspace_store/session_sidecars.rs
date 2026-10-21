//! Single-file session settings kept beside the session document: the session's own subagent
//! model allowlist and its own additional review prompt.
//! 保存在会话文档旁的单文件会话设置：会话自有子代理模型白名单与会话自有附加复核提示词。
//!
//! Each file exists only while the session replaces the system default; removing it returns the
//! session to the system default. The caller owns each document type and its version.
//! 只有会话替换系统默认值时对应文件才存在；删除文件即恢复系统默认。各文档类型及其版本由调用方负责。

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::SessionV1StorageLocator;
use super::session_v1::documents::write_json_document;

/// File name of the session allowlist inside the session directory.
/// 会话目录中会话白名单的文件名。
const SESSION_SUBAGENT_MODELS_FILE: &str = "subagent-models.json";

/// File name of the session review prompt inside the session directory.
/// 会话目录中会话复核提示词的文件名。
const SESSION_REVIEW_PROMPT_FILE: &str = "review-prompt.json";

/// Loads one sidecar document, or `None` when the file is absent.
/// Malformed files are explicit errors, never treated as absent.
/// 加载一个旁置文档；文件缺失时返回 `None`。损坏的文件返回明确错误，绝不视为不存在。
fn load_sidecar<T: DeserializeOwned>(
    locator: &SessionV1StorageLocator,
    file_name: &str,
) -> Result<Option<T>, String> {
    let path = locator.session_directory().join(file_name);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Read {}: {error}", path.display())),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| format!("Parse {}: {error}", path.display()))
}

/// Saves one sidecar document, or removes the file when `document` is `None`.
/// 保存一个旁置文档；`document` 为 `None` 时删除该文件。
fn save_sidecar<T: Serialize>(
    locator: &SessionV1StorageLocator,
    file_name: &str,
    label: &str,
    document: Option<&T>,
) -> Result<(), String> {
    let path = locator.session_directory().join(file_name);
    match document {
        Some(document) => {
            write_json_document(&path, document, label).map_err(|error| error.to_string())
        }
        None => match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("Remove {}: {error}", path.display())),
        },
    }
}

/// Loads the session allowlist document, or `None` when the session follows the system default.
/// 加载会话白名单文档；会话跟随系统默认时返回 `None`。
pub fn load_session_subagent_models<T: DeserializeOwned>(
    locator: &SessionV1StorageLocator,
) -> Result<Option<T>, String> {
    load_sidecar(locator, SESSION_SUBAGENT_MODELS_FILE)
}

/// Saves the session allowlist document, or removes it when `document` is `None`.
/// 保存会话白名单文档；`document` 为 `None` 时删除该文件。
pub fn save_session_subagent_models<T: Serialize>(
    locator: &SessionV1StorageLocator,
    document: Option<&T>,
) -> Result<(), String> {
    save_sidecar(
        locator,
        SESSION_SUBAGENT_MODELS_FILE,
        "session subagent models",
        document,
    )
}

/// Loads the session review prompt document, or `None` when the session follows the system
/// default.
/// 加载会话复核提示词文档；会话跟随系统默认时返回 `None`。
pub fn load_session_review_prompt<T: DeserializeOwned>(
    locator: &SessionV1StorageLocator,
) -> Result<Option<T>, String> {
    load_sidecar(locator, SESSION_REVIEW_PROMPT_FILE)
}

/// Saves the session review prompt document, or removes it when `document` is `None`.
/// 保存会话复核提示词文档；`document` 为 `None` 时删除该文件。
pub fn save_session_review_prompt<T: Serialize>(
    locator: &SessionV1StorageLocator,
    document: Option<&T>,
) -> Result<(), String> {
    save_sidecar(
        locator,
        SESSION_REVIEW_PROMPT_FILE,
        "session review prompt",
        document,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_store::session_v1_store::create_session_v1_storage_fixture;

    /// An absent file follows the system default; a saved document reloads and removal restores
    /// absence, independently for each sidecar.
    /// 文件缺失表示跟随系统默认；保存的文档可重新加载，删除后恢复为缺失，各旁置文件互不影响。
    #[test]
    fn session_sidecars_round_trip_and_reset_independently() {
        let temporary = tempfile::tempdir().unwrap();
        let locator =
            create_session_v1_storage_fixture(temporary.path(), "session-sidecars", 1).unwrap();
        assert_eq!(
            load_session_subagent_models::<Vec<String>>(&locator).unwrap(),
            None
        );
        let saved = vec![String::from("provider/model")];
        save_session_subagent_models(&locator, Some(&saved)).unwrap();
        save_session_review_prompt(&locator, Some(&String::from("prompt"))).unwrap();
        assert_eq!(
            load_session_subagent_models::<Vec<String>>(&locator).unwrap(),
            Some(saved)
        );
        save_session_subagent_models::<Vec<String>>(&locator, None).unwrap();
        assert_eq!(
            load_session_subagent_models::<Vec<String>>(&locator).unwrap(),
            None
        );
        assert_eq!(
            load_session_review_prompt::<String>(&locator).unwrap(),
            Some(String::from("prompt"))
        );
        save_session_review_prompt::<String>(&locator, None).unwrap();
        assert_eq!(
            load_session_review_prompt::<String>(&locator).unwrap(),
            None
        );
    }

    /// A damaged file is an error rather than the system default.
    /// 损坏的文件是错误，而不是系统默认。
    #[test]
    fn damaged_session_sidecar_is_an_error() {
        let temporary = tempfile::tempdir().unwrap();
        let locator =
            create_session_v1_storage_fixture(temporary.path(), "session-sidecars", 1).unwrap();
        std::fs::write(
            locator.session_directory().join(SESSION_REVIEW_PROMPT_FILE),
            "not json",
        )
        .unwrap();
        assert!(load_session_review_prompt::<String>(&locator).is_err());
    }
}
