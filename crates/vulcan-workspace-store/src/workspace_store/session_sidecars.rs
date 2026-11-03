//! Single-file session settings kept beside the session document: the session's own subagent
//! model allowlist, its own additional review prompt and its own outside-workspace file access.
//! 保存在会话文档旁的单文件会话设置：会话自有子代理模型白名单、会话自有附加复核提示词与会话自有工作区外文件访问方式。
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

/// File name of the session outside-workspace file access inside the session directory.
/// 会话目录中会话工作区外文件访问方式的文件名。
const SESSION_OUTSIDE_FILE_ACCESS_FILE: &str = "outside-file-access.json";

/// Loads one sidecar document, or `None` when the file is absent.
/// Malformed files are explicit errors, never treated as absent.
/// 加载一个旁置文档；文件缺失时返回 `None`。损坏的文件返回明确错误，绝不视为不存在。
pub(super) fn load_sidecar<T: DeserializeOwned>(
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
pub(super) fn save_sidecar<T: Serialize>(
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
    // Serialize explicit saves with automatic policy upgrades on the same stable lock.
    // 通过同一稳定锁串行显式保存与自动策略升级。
    let _lock = subagent_policy_lock(locator)?;
    save_sidecar(
        locator,
        SESSION_SUBAGENT_MODELS_FILE,
        "session subagent models",
        document,
    )
}

/// Locks subagent policy publication across readers, explicit writers and processes.
/// 跨读取者、显式写入者和进程锁定子代理策略发布。
/// `locator` identifies existing storage; the returned file releases the lock when dropped.
/// `locator` 标识已有存储；返回文件在释放时解锁。
fn subagent_policy_lock(locator: &SessionV1StorageLocator) -> Result<std::fs::File, String> {
    // Keep the lock separate from the atomically replaced policy file.
    // 锁与被原子替换的策略文件分离。
    let path = locator
        .session_directory()
        .join("subagent-models.write.lock");
    // A permanent lock file preserves OS-level ownership across atomic publication.
    // 永久锁文件在原子发布期间保留操作系统级归属。
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| format!("Open {}: {error}", path.display()))?;
    file.lock()
        .map_err(|error| format!("Lock {}: {error}", path.display()))?;
    Ok(file)
}

/// Reads a policy with a typed, caller-owned version upgrade and preserves exact original bytes.
/// 使用调用方拥有的强类型版本升级读取策略，并保留精确原字节。
/// `decode` returns the validated document and its previous version only when conversion is required.
/// `decode` 返回已校验文档，仅需要转换时同时返回旧版本。
/// Returns absence, the current document, or an explicit error without replacing invalid input.
/// 返回缺失、当前文档或明确错误，不替换无效输入。
pub fn load_session_subagent_models_with_upgrade<T: Serialize>(
    locator: &SessionV1StorageLocator,
    decode: impl FnOnce(&[u8]) -> Result<(T, Option<u32>), String>,
) -> Result<Option<T>, String> {
    // Lock before reading so an explicit save cannot be overwritten by an older upgrade candidate.
    // 读取前加锁，避免显式保存被更早的升级候选覆盖。
    let _lock = subagent_policy_lock(locator)?;
    // The policy path is shared with explicit saves and deletes.
    // 策略路径与显式保存和删除共用。
    let path = locator
        .session_directory()
        .join(SESSION_SUBAGENT_MODELS_FILE);
    // Preserve absence separately from corruption or I/O failures.
    // 区分缺失与损坏或输入输出失败。
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Read {}: {error}", path.display())),
    };
    // Validation completes before any backup or policy mutation.
    // 所有校验在备份和策略修改之前完成。
    let (document, previous_version) = decode(&bytes)?;
    if let Some(version) = previous_version {
        // Never replace a conflicting backup from an earlier upgrade attempt.
        // 绝不替换与先前升级尝试冲突的备份。
        let backup = path.with_file_name(format!("subagent-models.v{version}.backup.json"));
        match std::fs::read(&backup) {
            Ok(existing) if existing == bytes => {}
            Ok(_) => return Err(format!("Conflicting policy backup: {}", backup.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                super::session_v1::atomic_io::write_file_atomic(
                    &backup,
                    &bytes,
                    "subagent policy backup",
                )
                .map_err(|error| error.to_string())?;
            }
            Err(error) => return Err(format!("Read {}: {error}", backup.display())),
        }
        write_json_document(&path, &document, "session subagent models")
            .map_err(|error| error.to_string())?;
    }
    Ok(Some(document))
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

/// Loads the session outside-workspace file access document, or `None` when the session follows
/// the system default.
/// 加载会话工作区外文件访问方式文档；会话跟随系统默认时返回 `None`。
pub fn load_session_outside_file_access<T: DeserializeOwned>(
    locator: &SessionV1StorageLocator,
) -> Result<Option<T>, String> {
    load_sidecar(locator, SESSION_OUTSIDE_FILE_ACCESS_FILE)
}

/// Saves the session outside-workspace file access document, or removes it when `document` is
/// `None`.
/// 保存会话工作区外文件访问方式文档；`document` 为 `None` 时删除该文件。
pub fn save_session_outside_file_access<T: Serialize>(
    locator: &SessionV1StorageLocator,
    document: Option<&T>,
) -> Result<(), String> {
    save_sidecar(
        locator,
        SESSION_OUTSIDE_FILE_ACCESS_FILE,
        "session outside file access",
        document,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_store::session_v1_store::create_session_v1_storage_fixture;

    /// Converts a synthetic v1 policy to v2; returns current bytes unchanged on later reads.
    /// 将合成 v1 策略转换为 v2；后续读取保持当前数据。
    fn upgrade_fixture(bytes: &[u8]) -> Result<(serde_json::Value, Option<u32>), String> {
        // The typed service decoder is tested separately; this fixture tests storage transactions.
        // 强类型服务解码器另有测试；此夹具测试存储事务。
        let mut policy: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        match policy["version"].as_u64() {
            Some(1) => {
                policy["version"] = 2.into();
                policy["includeSessionModel"] = true.into();
                Ok((policy, Some(1)))
            }
            Some(2) => Ok((policy, None)),
            _ => Err(String::from("Unsupported policy version")),
        }
    }

    /// Concurrent first reads preserve one original backup, while failed publication remains retryable.
    /// 并发首次读取保留同一原始备份，发布失败后仍可重试。
    /// Uses isolated session storage and thread-local atomic-write fault injection.
    /// 使用隔离会话存储和线程局部原子写故障注入。
    #[test]
    fn subagent_policy_upgrade_is_atomic_and_serialized() {
        // Isolated files exercise production publication without touching user sessions.
        // 隔离文件验证生产发布，不接触用户会话。
        let temporary = tempfile::tempdir().unwrap();
        let locator =
            create_session_v1_storage_fixture(temporary.path(), "policy-upgrade", 1).unwrap();
        let path = locator
            .session_directory()
            .join(SESSION_SUBAGENT_MODELS_FILE);
        let backup = locator
            .session_directory()
            .join("subagent-models.v1.backup.json");
        let original = b"{\n  \"version\":1, \"models\":[]\n}";
        std::fs::write(&path, original).unwrap();
        super::super::session_v1::atomic_io::test_hooks::fail_next_artifact(
            super::super::session_v1::atomic_io::WriteStage::WriteAll,
            "session subagent models",
        );
        assert!(load_session_subagent_models_with_upgrade(&locator, upgrade_fixture).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(std::fs::read(&backup).unwrap(), original);
        std::thread::scope(|scope| {
            // Every reader must observe the same upgraded policy after the stable publication lock.
            // 每个读取者通过稳定发布锁后都必须看到同一升级策略。
            let readers = (0..4)
                .map(|_| {
                    scope.spawn(|| {
                        load_session_subagent_models_with_upgrade(&locator, upgrade_fixture)
                            .unwrap()
                            .unwrap()
                    })
                })
                .collect::<Vec<_>>();
            for reader in readers {
                assert_eq!(reader.join().unwrap()["includeSessionModel"], true);
            }
        });
        assert_eq!(std::fs::read(&backup).unwrap(), original);
        // Explicit saves after upgrade retain their false flag and cannot trigger another migration.
        // 升级后的显式保存保留 false 开关，不能再次触发迁移。
        let disabled = serde_json::json!({"version":2,"models":[],"includeSessionModel":false});
        save_session_subagent_models(&locator, Some(&disabled)).unwrap();
        assert_eq!(
            load_session_subagent_models_with_upgrade(&locator, upgrade_fixture).unwrap(),
            Some(disabled)
        );
        assert_eq!(std::fs::read(&backup).unwrap(), original);
        // A conflicting prior backup must stop before replacing either document.
        // 冲突的既有备份必须在替换任何文档之前停止。
        std::fs::write(&path, b"{\"version\":1,\"models\":[1]}").unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(load_session_subagent_models_with_upgrade(&locator, upgrade_fixture).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read(&backup).unwrap(), original);
    }

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
