//! lancedb ownership policy over the shared sidecar lock primitive.
//! lancedb 基于共用旁路锁原语的所有权策略。

#[cfg(test)]
use std::fs;
use std::path::{Path, PathBuf};
use vulcan_file_ownership::{OwnershipLock, OwnershipLockError};

use crate::error::{EngineResult, LanceDbEngineError};

/// Retains exclusive storage ownership for the lifetime of this value.
/// 在此值的生命周期内保留存储独占所有权。
#[derive(Debug)]
pub struct DatabaseRootLock {
    inner: OwnershipLock,
}

impl DatabaseRootLock {
    /// Acquires the unchanged engine-specific sidecar, preserving typed conflict and I/O errors.
    /// 获取引擎专属且布局不变的旁路锁，保留强类型竞争及 I/O 错误。
    pub fn acquire(resource: &Path) -> EngineResult<Self> {
        let owner = format!("pid={} db_root={}", std::process::id(), resource.display());
        let inner =
            OwnershipLock::acquire(derive_lock_path(resource), owner).map_err(
                |error| match error {
                    OwnershipLockError::Held { lock_path, owner } => {
                        LanceDbEngineError::LockHeld { lock_path, owner }
                    }
                    OwnershipLockError::Io { lock_path, source } => {
                        LanceDbEngineError::LockIo { lock_path, source }
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
    vulcan_file_ownership::sidecar_path(resource, "lancedb.vldb.lock")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::LanceDbEngineError;

    /// Environment variable that makes the child-side helper attempt the lock.
    /// 使子进程侧辅助测试尝试加锁的环境变量。
    const CROSS_PROCESS_ROOT_ENV: &str = "VULCAN_LANCEDB_LOCK_CROSS_PROCESS_ROOT";

    /// Builds a root directory inside a temporary directory.
    /// 在临时目录内构造一个根目录。
    fn sample_root() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().expect("temporary directory");
        let root = directory.path().join("databases").join("default");
        (directory, root)
    }

    /// Verifies acquisition creates the parent directory of the lock when it is missing.
    /// 验证缺失时获取锁会创建锁的父目录。
    #[test]
    fn acquisition_creates_missing_parent_directory() {
        let (_directory, root) = sample_root();
        assert!(!root.parent().expect("root has a parent").exists());

        let lock = DatabaseRootLock::acquire(&root).expect("first acquisition");
        assert!(
            lock.lock_path()
                .parent()
                .expect("lock has a parent")
                .exists()
        );
    }

    /// Verifies the lock sits beside the root directory and never inside it.
    /// 验证锁位于根目录旁，绝不位于其内。
    ///
    /// This is the design's load-bearing property: LanceDB scans its root for tables, so a lock file inside it
    /// would become part of the data directory.
    /// 这是设计的承重性质：LanceDB 会扫描其根以寻找表，因此位于其中的锁文件会成为数据目录的一部分。
    #[test]
    fn the_lock_sits_beside_the_root_and_not_inside_it() {
        let (_directory, root) = sample_root();
        let lock = DatabaseRootLock::acquire(&root).expect("acquisition");

        let lock_path = lock.lock_path();
        assert_eq!(
            lock_path
                .file_name()
                .map(|name| name.to_string_lossy().to_string()),
            Some(String::from("default.vldb.lock"))
        );
        assert_eq!(
            lock_path.parent(),
            root.parent(),
            "the lock must be a sibling of the root directory"
        );
        assert_eq!(
            lock.owner_path()
                .file_name()
                .map(|name| name.to_string_lossy().to_string()),
            Some(String::from("default.vldb.lock.owner"))
        );
        assert!(
            lock.owner()
                .starts_with(&format!("pid={}", std::process::id()))
        );
    }

    /// Verifies a second acquisition in this process reports the first holder.
    /// 验证本进程内第二次获取会报告首个持有者。
    #[test]
    fn second_acquisition_reports_lock_held_with_owner() {
        let (_directory, root) = sample_root();
        let held = DatabaseRootLock::acquire(&root).expect("first acquisition");

        let error = DatabaseRootLock::acquire(&root).expect_err("contention must be reported");
        match error {
            LanceDbEngineError::LockHeld { owner, lock_path } => {
                assert_eq!(lock_path, held.lock_path());
                assert!(
                    owner.contains(&format!("pid={}", std::process::id())),
                    "the report must name the holder: {owner}"
                );
            }
            other => panic!("expected LockHeld, got {other}"),
        }
    }

    /// Verifies the owner record is readable while the lock is held.
    /// 验证锁被持有时持有者记录可读。
    #[test]
    fn owner_record_is_readable_while_lock_is_held() {
        let (_directory, root) = sample_root();
        let held = DatabaseRootLock::acquire(&root).expect("acquisition");

        let recorded = fs::read_to_string(held.owner_path()).expect("owner record readable");
        assert_eq!(recorded.trim(), held.owner());
    }

    /// Verifies dropping the lock allows a later acquisition, which is what release means here.
    /// 验证丢弃锁允许后续获取，这正是此处"释放"的含义。
    #[test]
    fn release_allows_reacquisition() {
        let (_directory, root) = sample_root();
        let held = DatabaseRootLock::acquire(&root).expect("first acquisition");
        drop(held);

        DatabaseRootLock::acquire(&root).expect("a released lock must be acquirable again");
    }

    /// Verifies an unreadable or absent owner record still yields a usable contention report.
    /// 验证持有者记录缺失或不可读时仍给出可用的竞争报告。
    #[test]
    fn missing_owner_record_still_reports_contention() {
        let (_directory, root) = sample_root();
        let held = DatabaseRootLock::acquire(&root).expect("first acquisition");
        fs::remove_file(held.owner_path()).expect("remove the owner record");

        let error =
            DatabaseRootLock::acquire(&root).expect_err("contention must still be reported");
        match error {
            LanceDbEngineError::LockHeld { owner, .. } => {
                assert_eq!(owner, "unknown", "an absent record must report `unknown`");
            }
            other => panic!("expected LockHeld, got {other}"),
        }
    }

    /// Verifies another process cannot acquire a lock this process holds.
    /// 验证另一个进程无法获取本进程持有的锁。
    #[test]
    fn cross_process_acquisition_reports_lock_held() {
        let (_directory, root) = sample_root();
        let held = DatabaseRootLock::acquire(&root).expect("parent acquisition");

        let executable = std::env::current_exe().expect("current test executable");
        let output = std::process::Command::new(executable)
            .arg("--exact")
            .arg("db_lock::tests::cross_process_child_attempts_lock")
            .arg("--nocapture")
            .env(CROSS_PROCESS_ROOT_ENV, &root)
            .output()
            .expect("spawn the child test process");
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(
            combined.contains("CROSS_PROCESS_RESULT=LOCK_HELD"),
            "the child must report contention.\ncombined output:\n{combined}"
        );
        assert!(
            combined.contains(&format!("pid={}", std::process::id())),
            "the child must name this process as the holder.\ncombined output:\n{combined}"
        );

        drop(held);
    }

    /// Child-side helper for [`cross_process_acquisition_reports_lock_held`].
    /// 供 [`cross_process_acquisition_reports_lock_held`] 使用的子进程侧辅助测试。
    ///
    /// It is a separate test so it can be selected by name in a child process. When its driving environment
    /// variable is absent it is a no-op, so an ordinary full test run stays green.
    /// 它是独立测试，以便在子进程里按名称选中。当驱动它的环境变量不存在时它是空操作，因此常规全量测试仍然通过。
    #[test]
    fn cross_process_child_attempts_lock() {
        let Some(root) = std::env::var_os(CROSS_PROCESS_ROOT_ENV) else {
            return;
        };

        match DatabaseRootLock::acquire(&PathBuf::from(root)) {
            Err(LanceDbEngineError::LockHeld { owner, .. }) => {
                println!("CROSS_PROCESS_RESULT=LOCK_HELD");
                println!("CROSS_PROCESS_OWNER={owner}");
            }
            Ok(_) => println!("CROSS_PROCESS_RESULT=ACQUIRED_UNEXPECTEDLY"),
            Err(other) => println!("CROSS_PROCESS_RESULT=OTHER_ERROR:{other}"),
        }
    }
}
