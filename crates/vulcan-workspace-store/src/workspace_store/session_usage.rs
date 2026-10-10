//! Single-file persistence for session lifetime usage.
//! 会话生命周期用量的单文件持久化。

use super::SessionV1StorageLocator;
use super::session_v1::documents::write_json_document;
use vulcan_session_core::v1::lifetime_usage::{
    SESSION_LIFETIME_USAGE_VERSION, SessionLifetimeUsage,
};

/// Saves one root-owned aggregate using the existing atomic file publisher.
/// 使用既有原子文件发布器保存一份根会话累计。
/// The caller serializes writes; returns storage errors without altering other session files.
/// 调用方串行写入；返回存储错误，不修改其他会话文件。
pub fn save_session_lifetime_usage(
    locator: &SessionV1StorageLocator,
    usage: &SessionLifetimeUsage,
) -> Result<(), String> {
    if usage.root_session_id != locator.session_id()
        || usage.version != SESSION_LIFETIME_USAGE_VERSION
    {
        return Err("Session usage identity or version mismatch".to_owned());
    }
    write_json_document(
        &locator.session_directory().join("usage.json"),
        usage,
        "session usage",
    )
    .map_err(|error| error.to_string())
}

/// Loads the sole root file written when the root session was created.
/// 加载根会话创建时写入的唯一根文件。
/// Missing, malformed, mismatched or older-version files are explicit errors.
/// 缺失、损坏、身份不符或旧版本的文件均为明确错误。
pub fn load_session_lifetime_usage(
    locator: &SessionV1StorageLocator,
) -> Result<SessionLifetimeUsage, String> {
    let path = locator.session_directory().join("usage.json");
    let bytes =
        std::fs::read(&path).map_err(|error| format!("Read {}: {error}", path.display()))?;
    let usage: SessionLifetimeUsage =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if usage.version != SESSION_LIFETIME_USAGE_VERSION
        || usage.root_session_id != locator.session_id()
    {
        return Err("Session usage identity or version mismatch".to_owned());
    }
    Ok(usage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_store::session_v1_store::create_session_v1_storage_fixture;

    /// A missing file is an explicit error; saved totals reload unchanged.
    /// 缺失文件返回明确错误；已保存累计原样重载。
    #[test]
    fn missing_usage_is_rejected_and_saved_totals_are_preserved() {
        let temporary = tempfile::tempdir().unwrap();
        let locator = create_session_v1_storage_fixture(temporary.path(), "usage-root", 1).unwrap();
        assert!(load_session_lifetime_usage(&locator).is_err());
        assert!(!locator.session_directory().join("usage.json").exists());
        let mut usage = SessionLifetimeUsage::new("usage-root".into(), 0);
        usage.main.tool_call_count = 12;
        save_session_lifetime_usage(&locator, &usage).unwrap();
        assert_eq!(load_session_lifetime_usage(&locator).unwrap(), usage);
    }

    /// Malformed, mismatched or older-version files remain explicit errors.
    /// 损坏、身份不符或旧版本的文件保持明确错误。
    #[test]
    fn invalid_usage_is_rejected() {
        let temporary = tempfile::tempdir().unwrap();
        let locator = create_session_v1_storage_fixture(temporary.path(), "usage-root", 1).unwrap();
        let path = locator.session_directory().join("usage.json");
        std::fs::write(&path, "invalid").unwrap();
        assert!(load_session_lifetime_usage(&locator).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "invalid");
        let wrong = SessionLifetimeUsage::new("another-root".into(), 0);
        std::fs::write(&path, serde_json::to_vec(&wrong).unwrap()).unwrap();
        assert!(load_session_lifetime_usage(&locator).is_err());
        let mut older = SessionLifetimeUsage::new("usage-root".into(), 0);
        older.version = SESSION_LIFETIME_USAGE_VERSION - 1;
        std::fs::write(&path, serde_json::to_vec(&older).unwrap()).unwrap();
        assert!(load_session_lifetime_usage(&locator).is_err());
    }
}
