//! Single-file persistence for session lifetime usage.
//! 会话生命周期用量的单文件持久化。

use super::SessionV1StorageLocator;
use super::session_v1::documents::write_json_document;
use vulcan_session_core::v1::lifetime_usage::{
    SESSION_LIFETIME_USAGE_VERSION, SessionLifetimeUsage,
};

/// The filename of the single root-owned usage aggregate.
/// 根会话唯一用量累计文件的名称。
const SESSION_USAGE_FILENAME: &str = "usage.json";

/// Ensures the unindexed recovery draft has its initial root usage before index publication.
/// 在发布索引前确保未索引恢复草稿已有根会话初始用量。
/// The caller holds the lifecycle gate for the verified locator; created_at_ms is the first publication time.
/// 调用方持有已校验定位器的生命周期闸门；created_at_ms 为首次发布时间。
/// Returns success after preserving a valid aggregate or atomically creating an absent one.
/// 保留有效累计或原子创建缺失累计后返回成功。
pub(super) fn ensure_recovery_session_lifetime_usage(
    locator: &SessionV1StorageLocator,
    created_at_ms: i64,
) -> Result<(), String> {
    // Check the exact artifact without treating malformed data or a dangling link as absent.
    // 检查精确文件，不把损坏数据或悬空链接视为缺失。
    let path = locator.session_directory().join(SESSION_USAGE_FILENAME);
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            load_session_lifetime_usage(locator).map(|_| ())
        }
        Ok(_) => Err(format!(
            "Session usage is not a regular file: {}",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => save_session_lifetime_usage(
            locator,
            &SessionLifetimeUsage::new(locator.session_id().to_owned(), created_at_ms),
        ),
        Err(error) => Err(format!("Inspect {}: {error}", path.display())),
    }
}

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
        &locator.session_directory().join(SESSION_USAGE_FILENAME),
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
    let path = locator.session_directory().join(SESSION_USAGE_FILENAME);
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

    /// Verifies a recovery retry preserves a valid aggregate and rejects an invalid one.
    /// 验证恢复重试保留有效累计并拒绝无效累计。
    #[test]
    fn recovery_usage_initialization_preserves_existing_data() {
        let temporary = tempfile::tempdir().unwrap();
        let locator =
            create_session_v1_storage_fixture(temporary.path(), "recovery-root", 1).unwrap();
        let mut usage = SessionLifetimeUsage::new(locator.session_id().to_owned(), 1);
        usage.main.request_count = 3;
        save_session_lifetime_usage(&locator, &usage).unwrap();
        ensure_recovery_session_lifetime_usage(&locator, 2).unwrap();
        assert_eq!(load_session_lifetime_usage(&locator).unwrap(), usage);

        let path = locator.session_directory().join(SESSION_USAGE_FILENAME);
        std::fs::write(&path, b"invalid").unwrap();
        assert!(ensure_recovery_session_lifetime_usage(&locator, 2).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"invalid");
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
