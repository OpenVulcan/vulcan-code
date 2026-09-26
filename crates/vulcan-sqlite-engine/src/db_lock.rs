//! sqlite ownership policy over the shared sidecar lock primitive.
//! sqlite 基于共用旁路锁原语的所有权策略。

use std::path::{Path, PathBuf};
use vulcan_file_ownership::{OwnershipLock, OwnershipLockError};
#[cfg(test)]
use vulcan_file_ownership::{owner_record_path as derive_owner_path, read_owner_record};

use crate::error::{EngineResult, SqliteEngineError};

/// Retains exclusive storage ownership for the lifetime of this value.
/// 在此值的生命周期内保留存储独占所有权。
#[derive(Debug)]
pub struct DatabaseFileLock {
    inner: OwnershipLock,
}

impl DatabaseFileLock {
    /// Acquires the unchanged engine-specific sidecar, preserving typed conflict and I/O errors.
    /// 获取引擎专属且布局不变的旁路锁，保留强类型竞争及 I/O 错误。
    pub fn acquire(resource: &Path) -> EngineResult<Self> {
        let owner = format!("pid={} db_path={}", std::process::id(), resource.display());
        let inner =
            OwnershipLock::acquire(derive_lock_path(resource), owner).map_err(
                |error| match error {
                    OwnershipLockError::Held { lock_path, owner } => {
                        SqliteEngineError::LockHeld { lock_path, owner }
                    }
                    OwnershipLockError::Io { lock_path, source } => {
                        SqliteEngineError::LockIo { lock_path, source }
                    }
                },
            )?;
        Ok(Self { inner })
    }
    /// Returns the held lock path.
    /// 返回持有锁路径。
    pub fn lock_path(&self) -> &Path {
        self.inner.lock_path()
    }
    /// Returns the separate diagnostic owner record path.
    /// 返回独立诊断持有者记录路径。
    pub fn owner_path(&self) -> &Path {
        self.inner.owner_path()
    }
    /// Returns the diagnostic identity published by this holder.
    /// 返回本持有者发布的诊断身份。
    pub fn owner(&self) -> &str {
        self.inner.owner()
    }
}

/// Keeps the engine's historical file/directory sidecar location without lossy native-name conversion.
/// 保留引擎历史文件或目录旁路位置，不对原生名称进行有损转换。
fn derive_lock_path(resource: &Path) -> PathBuf {
    vulcan_file_ownership::sidecar_path(resource, "sqlite.vldb.lock")
}

#[cfg(test)]
mod tests {
    /// The engine wrapper retains distinct native names instead of using their lossy display text.
    /// 引擎包装保留不同的原生名称，不使用有损显示文本。
    #[cfg(unix)]
    #[test]
    fn native_database_names_do_not_alias() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let root = tempfile::tempdir().expect("isolated lock fixture");
        let a = root.path().join(OsString::from_vec(vec![0x80]));
        let b = root.path().join(OsString::from_vec(vec![0x81]));
        let a = DatabaseFileLock::acquire(&a).expect("first database lock");
        let b = DatabaseFileLock::acquire(&b).expect("second database lock");
        assert_ne!(a.lock_path(), b.lock_path());
    }
    use super::{DatabaseFileLock, derive_lock_path, derive_owner_path};
    use crate::error::SqliteEngineError;
    use std::fs;
    use std::path::PathBuf;

    /// Verifies the lock file is placed next to the database.
    /// 验证锁文件与数据库相邻。
    #[test]
    fn derive_lock_path_places_lock_next_to_database() {
        assert_eq!(
            derive_lock_path(&PathBuf::from("/srv/vldb/sqlite.db")),
            PathBuf::from("/srv/vldb/sqlite.db.vldb.lock")
        );
    }

    /// Bare input keeps its relative identity; production resources are normalized before acquisition.
    /// 裸输入保留相对身份；生产资源在获取锁之前完成规范化。
    #[test]
    fn derive_lock_path_handles_missing_parent() {
        // A bare file name has no parent segment.
        // 裸文件名没有父段。
        assert_eq!(
            derive_lock_path(&PathBuf::from("app.db")),
            PathBuf::from("app.db.vldb.lock")
        );
    }

    /// Verifies the owner record path derives from the lock path.
    /// 验证持有者记录路径由锁路径派生。
    #[test]
    fn derive_owner_path_appends_owner_suffix() {
        assert_eq!(
            derive_owner_path(&PathBuf::from("/srv/vldb/sqlite.db.vldb.lock")),
            PathBuf::from("/srv/vldb/sqlite.db.vldb.lock.owner")
        );
    }

    /// Verifies the first acquisition succeeds and records an owner identity in the separate
    /// owner record file.
    ///
    /// The held lock file must stay content-free. It is asserted via its length rather than by
    /// reading it, because on Windows an exclusively locked file cannot be read at all by any
    /// handle, including the holder's own.
    ///
    /// 验证首次加锁成功，并在独立的持有者记录文件中记录身份。
    ///
    /// 被持有的锁文件必须保持无内容。这里用长度断言而不是读取内容，因为在 Windows 上被独占锁定的
    /// 文件无法被任何句柄读取，包括持有者自己的句柄。
    #[test]
    fn first_acquisition_succeeds_and_records_owner() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let db_path = temp.path().join("app.db");

        let lock = DatabaseFileLock::acquire(&db_path).expect("acquire first lock");

        assert!(
            lock.owner()
                .contains(&format!("pid={}", std::process::id()))
        );
        assert!(lock.owner().contains("app.db"));
        assert_eq!(lock.lock_path(), derive_lock_path(&db_path).as_path());
        assert_eq!(
            lock.owner_path(),
            derive_owner_path(&derive_lock_path(&db_path)).as_path()
        );

        let record = fs::read_to_string(lock.owner_path()).expect("read owner record");
        assert!(record.contains(&format!("pid={}", std::process::id())));

        let held_len = fs::metadata(lock.lock_path())
            .expect("stat held lock file")
            .len();
        assert_eq!(held_len, 0, "held lock file must carry no payload");
    }

    /// Verifies a second acquisition in the same process reports contention rather than an I/O
    /// failure, and names the current holder. This covers the `fs4 1.1.0`
    /// `TryLockError::WouldBlock` branch.
    /// 验证同进程内二次加锁报告竞争而非 I/O 失败，并给出当前持有者。覆盖 `fs4 1.1.0` 的
    /// `TryLockError::WouldBlock` 分支。
    #[test]
    fn second_acquisition_reports_lock_held_with_owner() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let db_path = temp.path().join("app.db");

        let _held = DatabaseFileLock::acquire(&db_path).expect("acquire first lock");

        match DatabaseFileLock::acquire(&db_path) {
            Err(SqliteEngineError::LockHeld { lock_path, owner }) => {
                assert_eq!(lock_path, derive_lock_path(&db_path));
                assert!(
                    owner.contains("pid="),
                    "contention report must name the current holder, got {owner}"
                );
                assert!(
                    owner.contains("app.db"),
                    "contention report must name the contended database, got {owner}"
                );
            }
            other => panic!("expected LockHeld for the second acquisition, got {other:?}"),
        }
    }

    /// Verifies dropping the lock releases it so the path can be acquired again.
    /// 验证释放后该路径可被重新加锁。
    #[test]
    fn release_allows_reacquisition() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let db_path = temp.path().join("app.db");

        {
            let _lock = DatabaseFileLock::acquire(&db_path).expect("acquire first lock");
        }

        let reacquired = DatabaseFileLock::acquire(&db_path).expect("reacquire after release");
        assert_eq!(reacquired.lock_path(), derive_lock_path(&db_path).as_path());
    }

    /// Verifies a stale owner record is replaced on a fresh acquisition.
    /// 验证重新加锁时陈旧的持有者记录被替换。
    #[test]
    fn reacquisition_refreshes_owner_record() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let db_path = temp.path().join("app.db");

        {
            let first = DatabaseFileLock::acquire(&db_path).expect("acquire first lock");
            fs::write(first.owner_path(), "stale-owner\n").expect("write stale record");
        }

        let second = DatabaseFileLock::acquire(&db_path).expect("reacquire after release");
        let record = fs::read_to_string(second.owner_path()).expect("read owner record");
        assert!(record.contains(&format!("pid={}", std::process::id())));
        assert!(!record.contains("stale-owner"));
    }

    /// Verifies the lock path parent directory is created when missing.
    /// 验证锁路径父目录缺失时会被创建。
    #[test]
    fn acquisition_creates_missing_parent_directory() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let nested = temp.path().join("nested").join("deeper");
        let db_path = nested.join("app.db");

        let lock = DatabaseFileLock::acquire(&db_path).expect("acquire lock in missing directory");
        assert!(lock.lock_path().exists());
        assert!(lock.owner_path().exists());
    }

    /// Verifies the reported owner is usable while the lock is held, which is the property the
    /// contention path depends on.
    /// 验证锁被持有时仍能读取持有者身份，这正是竞争路径所依赖的性质。
    #[test]
    fn owner_record_is_readable_while_lock_is_held() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let db_path = temp.path().join("app.db");

        let lock = DatabaseFileLock::acquire(&db_path).expect("acquire lock");
        let record = super::read_owner_record(lock.owner_path()).expect("owner record readable");
        assert_eq!(record, lock.owner());
    }

    /// Environment variable naming the database path a child process must try to lock.
    /// 指定子进程必须尝试加锁的数据库路径的环境变量名。
    const CROSS_PROCESS_DB_PATH_ENV: &str = "VULCAN_SQLITE_ENGINE_CROSS_PROCESS_DB_PATH";

    /// Verifies mutual exclusion across **separate operating-system processes**.
    ///
    /// A same-process `WouldBlock` result does not prove cross-process exclusion, because the
    /// operating system may treat handles from one process differently. This test re-executes the
    /// current test binary as a child, asks it to lock a database the parent already holds, and
    /// requires the child to report contention and name the parent as the holder.
    ///
    /// 验证**不同操作系统进程之间**的互斥。同进程的 `WouldBlock` 不能证明跨进程互斥，因为操作
    /// 系统可能对来自同一进程的句柄做不同处理。本测试把当前测试二进制作为子进程重新执行，要求它
    /// 去锁定父进程已持有的数据库，并要求子进程报告竞争并指出父进程为持有者。
    #[test]
    fn cross_process_acquisition_reports_lock_held() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let db_path = temp.path().join("app.db");

        let held = DatabaseFileLock::acquire(&db_path).expect("parent acquires lock");

        let child_output =
            std::process::Command::new(std::env::current_exe().expect("current exe"))
                .args([
                    "--exact",
                    "db_lock::tests::cross_process_child_attempts_lock",
                    "--nocapture",
                ])
                .env(CROSS_PROCESS_DB_PATH_ENV, &db_path)
                .output()
                .expect("spawn child test process");

        let stdout = String::from_utf8_lossy(&child_output.stdout);
        let stderr = String::from_utf8_lossy(&child_output.stderr);
        let combined = format!("{stdout}{stderr}");

        assert!(
            combined.contains("CROSS_PROCESS_RESULT=LOCK_HELD"),
            "child must report contention while the parent holds the lock.\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        assert!(
            combined.contains("CROSS_PROCESS_OWNER="),
            "child must name the current holder.\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        assert!(
            combined.contains(&format!("pid={}", std::process::id())),
            "child must identify the parent process as the holder.\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );

        drop(held);
    }

    /// Child-side helper for [`cross_process_acquisition_reports_lock_held`].
    ///
    /// It is a separate test so it can be selected by name in a child process. When its driving
    /// environment variable is absent it is a no-op, so an ordinary full test run stays green.
    ///
    /// 供 [`cross_process_acquisition_reports_lock_held`] 使用的子进程侧辅助测试。它是独立测试，
    /// 以便在子进程里按名称选中。当驱动它的环境变量不存在时它是空操作，因此常规全量测试仍然通过。
    #[test]
    fn cross_process_child_attempts_lock() {
        let Some(db_path) = std::env::var_os(CROSS_PROCESS_DB_PATH_ENV) else {
            return;
        };

        match DatabaseFileLock::acquire(&std::path::PathBuf::from(db_path)) {
            Err(SqliteEngineError::LockHeld { owner, .. }) => {
                println!("CROSS_PROCESS_RESULT=LOCK_HELD");
                println!("CROSS_PROCESS_OWNER={owner}");
            }
            Ok(_) => {
                println!("CROSS_PROCESS_RESULT=ACQUIRED_UNEXPECTEDLY");
            }
            Err(other) => {
                println!("CROSS_PROCESS_RESULT=OTHER_ERROR:{other}");
            }
        }
    }
}
