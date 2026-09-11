//! Database-root file ownership lock for LanceDB.
//! LanceDB 的库根文件所有权锁。
//!
//! Purpose / 目的:
//!
//! Exactly one **Vulcan Code process** may treat a given LanceDB database root as its sole writer. The v1
//! baseline settles that LanceDB gains a stable database-root lock, that multiple logical consumers inside one
//! process share one lock and one engine, and that a failed acquisition reports the occupied path and lock
//! identity instead of copying the database, deleting the lock file or continuing silently.
//! 同一个 LanceDB 库根只能被一个 **Vulcan Code 进程** 当作唯一写者。v1 基线裁定 LanceDB 获得稳定的库根锁、同一
//! 进程内多个逻辑消费者共享同一把锁与同一个引擎，且获取失败时报告占用路径与锁身份，而不是复制数据库、删除锁文件
//! 或静默继续。
//!
//! Why the lock sits beside the root and not inside it / 锁为何位于根旁而非根内:
//!
//! A LanceDB database root is a **directory that LanceDB itself scans** for tables and manifests. A foreign file
//! written inside it is not inert: it is something the engine may list, and it makes the data directory's
//! contents depend on which process happens to be running. The lock therefore lives next to the root directory,
//! named after it. The sidecar files may remain after the database directory is deleted, but an unlocked sidecar
//! is reusable and its owner record is overwritten by the next holder.
//! LanceDB 库根是一个**由 LanceDB 自身扫描**以寻找表与清单的目录。写入其中的外来文件并非惰性之物：引擎可能会列出
//! 它，并使数据目录的内容取决于当时运行的进程。因此锁位于根目录旁边、以根命名。数据库目录删除后旁路文件可能保留，
//! 但未锁定的旁路文件可直接复用，且下一位持有者会覆盖其中的持有者记录。
//!
//! Two-file layout and why it is required / 双文件布局及其必要性:
//!
//! 1. `<root>.vldb.lock` — the **held** file. Its OS-level exclusive lock expresses ownership and nothing is ever
//!    written into it.
//! 2. `<root>.vldb.lock.owner` — the **owner record**, freely readable so a losing process can report who holds
//!    the lock.
//!
//! On Windows a process holding an exclusive lock cannot read or write that file's contents — attempts fail with
//! `ERROR_LOCK_VIOLATION` (OS error 33) — so storing the owner record inside the held file would make contention
//! reporting impossible and would break the acquisition path itself.
//! 在 Windows 上，持有独占锁的进程无法读写该文件内容——尝试会以 `ERROR_LOCK_VIOLATION`（系统错误 33）失败——
//! 因此把持有者记录放进被持有文件会让竞争报告无法实现，并破坏加锁路径本身。
//!
//! The lock path shape is an `OPEN` item in the v1 baseline; this module chooses the SQLite convention of
//! "same name, next to the target" and records that choice here.
//! 锁路径形状在 v1 基线中是 `OPEN` 项；本模块选择 SQLite 那一惯例——"同名、位于目标旁"——并在此记录该选择。

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{EngineResult, LanceDbEngineError};

/// Suffix appended to the database root directory name to derive its ownership lock path.
/// 追加到数据库根目录名后以派生其所有权锁路径的后缀。
const LOCK_FILE_SUFFIX: &str = ".vldb.lock";

/// Suffix appended to the lock path to derive the owner record path.
/// 追加到锁路径后以派生持有者记录路径的后缀。
const OWNER_FILE_SUFFIX: &str = ".owner";

/// Fallback lock file name used when the root path has no final path segment.
/// 当根路径没有最终路径段时使用的兜底锁文件名。
const FALLBACK_LOCK_FILE_NAME: &str = "lancedb.vldb.lock";

/// An exclusively held ownership lock for one LanceDB database root.
/// 针对一个 LanceDB 库根的独占所有权锁。
///
/// The lock is released when this value is dropped. Holding the value is what expresses ownership: there is no
/// separate "is locked" query, because any such query would be racy.
/// 该值被丢弃时锁即释放。持有该值本身即表达所有权；不提供单独的"是否已加锁"查询，因为任何这样的查询都存在竞争。
#[derive(Debug)]
pub struct DatabaseRootLock {
    /// Held lock file path.
    /// 被持有的锁文件路径。
    lock_path: PathBuf,
    /// Owner record path that carries this holder's identity.
    /// 携带本持有者身份的记录文件路径。
    owner_path: PathBuf,
    /// Open handle whose OS-level lock expresses ownership.
    /// 以操作系统级锁表达所有权的已打开句柄。
    file: File,
    /// Owner identity recorded for this holder.
    /// 为本持有者记录的持有者身份。
    owner: String,
}

impl DatabaseRootLock {
    /// Acquires the exclusive ownership lock for one database root directory.
    /// 为一个数据库根目录获取独占所有权锁。
    ///
    /// # Parameters / 参数
    /// `root` is the physical database root; its parent directory is created when missing.
    /// `root` 是物理数据库根；其父目录在缺失时会被创建。
    ///
    /// # Returns / 返回值
    /// Returns [`LanceDbEngineError::LockHeld`] when another holder currently owns the lock, and
    /// [`LanceDbEngineError::LockIo`] only for a genuine I/O failure. The two are deliberately distinct because
    /// the host reaction differs: contention is an expected environmental condition, while an I/O failure means
    /// the lock location itself is unusable.
    /// 当其他持有者当前持有该锁时返回 [`LanceDbEngineError::LockHeld`]；仅在真实 I/O 失败时返回
    /// [`LanceDbEngineError::LockIo`]。二者刻意区分，因为宿主应对方式不同：竞争是预期的环境状态，而 I/O 失败
    /// 说明锁位置本身不可用。
    pub fn acquire(root: &Path) -> EngineResult<Self> {
        let lock_path = derive_lock_path(root);
        let owner_path = derive_owner_path(&lock_path);

        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent).map_err(|source| LanceDbEngineError::LockIo {
                lock_path: lock_path.clone(),
                source,
            })?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|source| LanceDbEngineError::LockIo {
                lock_path: lock_path.clone(),
                source,
            })?;

        // Read the owner record before attempting the lock so a contention report can name the current holder.
        // 在尝试加锁前读取持有者记录，使竞争报告能给出当前持有者。
        let existing_owner = read_owner_record(&owner_path);

        if let Err(lock_error) = fs4::FileExt::try_lock(&file) {
            return Err(match lock_error {
                fs4::TryLockError::WouldBlock => LanceDbEngineError::LockHeld {
                    lock_path,
                    owner: existing_owner.unwrap_or_else(|| "unknown".to_string()),
                },
                fs4::TryLockError::Error(source) => {
                    LanceDbEngineError::LockIo { lock_path, source }
                }
            });
        }

        let owner = current_owner_identity(root);
        write_owner_record(&owner_path, &owner, &lock_path)?;

        Ok(Self {
            lock_path,
            owner_path,
            file,
            owner,
        })
    }

    /// Returns the held lock file path.
    /// 返回被持有的锁文件路径。
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }

    /// Returns the owner record path.
    /// 返回持有者记录路径。
    pub fn owner_path(&self) -> &Path {
        &self.owner_path
    }

    /// Returns the owner identity recorded for this holder.
    /// 返回为本持有者记录的持有者身份。
    pub fn owner(&self) -> &str {
        &self.owner
    }
}

impl Drop for DatabaseRootLock {
    /// Releases the OS-level lock.
    /// 释放操作系统级锁。
    ///
    /// The trait method is named explicitly because `std::fs::File` has an inherent `unlock` of the same name,
    /// and an unqualified call would resolve to the standard-library method.
    /// 这里显式写出 trait 方法名，因为 `std::fs::File` 有同名的固有方法 `unlock`，非限定调用会解析到标准库方法。
    fn drop(&mut self) {
        let _ = fs4::FileExt::unlock(&self.file);
    }
}

/// Derives the held lock path for one database root.
/// 为一个数据库根派生被持有的锁路径。
///
/// The lock sits beside the root so it is never part of the directory LanceDB scans.
/// 锁位于根旁，使它绝不成为 LanceDB 所扫描目录的一部分。
fn derive_lock_path(root: &Path) -> PathBuf {
    let file_name = root.file_name().map_or_else(
        || std::ffi::OsString::from(FALLBACK_LOCK_FILE_NAME),
        |name| {
            let mut value = name.to_os_string();
            value.push(LOCK_FILE_SUFFIX);
            value
        },
    );

    // `parent()` is `None` for a root path such as `/` on Windows, and for a bare directory name. In both cases
    // the resolved directory must still be usable, so fall back explicitly instead of producing a relative path
    // that would depend on the process working directory.
    // 在 Windows 上形如 `/` 的根路径以及裸目录名都会让 `parent()` 返回 `None`。两种情况都必须得到可用的目录，
    // 因此显式兜底，而不是产生依赖进程工作目录的相对路径。
    match root.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(file_name),
        _ => PathBuf::from(file_name),
    }
}

/// Derives the owner record path from a held lock path.
/// 由被持有的锁路径派生持有者记录路径。
fn derive_owner_path(lock_path: &Path) -> PathBuf {
    let mut value = lock_path.as_os_str().to_os_string();
    value.push(OWNER_FILE_SUFFIX);
    PathBuf::from(value)
}

/// Builds the owner identity string recorded for this process and database root.
/// 构造为本进程与该数据库根记录的持有者身份字符串。
fn current_owner_identity(root: &Path) -> String {
    format!("pid={} db_root={}", std::process::id(), root.display())
}

/// Reads the owner record, if one exists and is readable.
/// 读取持有者记录（若存在且可读）。
///
/// # Parameters / 参数
/// `owner_path` is the record produced by a previous holder.
/// `owner_path` 是先前持有者产生的记录。
///
/// # Returns / 返回值
/// Returns the recorded identity, or `None` when the record is absent or unreadable. An unreadable record is
/// treated as absent rather than fatal: it is diagnostic data, and refusing to acquire a lock because a
/// diagnostic file cannot be read would turn a reporting problem into an availability problem.
/// 返回记录的身份；记录缺失或不可读时返回 `None`。不可读的记录被视同缺失而非致命：它属于诊断数据，因无法读取
/// 诊断文件而拒绝加锁会把报告问题变成可用性问题。
fn read_owner_record(owner_path: &Path) -> Option<String> {
    let text = fs::read_to_string(owner_path).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Writes the owner record.
/// 写入持有者记录。
///
/// # Parameters / 参数
/// `owner_path` receives `owner`; `lock_path` is used only to name the lock in an error.
/// `owner_path` 接收 `owner`；`lock_path` 仅用于在错误中命名该锁。
///
/// # Returns / 返回值
/// Returns an error when the record cannot be written. The caller has already taken the OS-level lock, so this
/// failure surfaces while the lock is released by the drop of the held handle.
/// 记录无法写入时返回错误。调用方已取得操作系统级锁，因此该失败会向上暴露，而锁由被持有句柄的析构释放。
fn write_owner_record(owner_path: &Path, owner: &str, lock_path: &Path) -> EngineResult<()> {
    let mut file = File::create(owner_path).map_err(|source| LanceDbEngineError::LockIo {
        lock_path: lock_path.to_path_buf(),
        source,
    })?;
    file.write_all(owner.as_bytes())
        .map_err(|source| LanceDbEngineError::LockIo {
            lock_path: lock_path.to_path_buf(),
            source,
        })?;
    file.flush().map_err(|source| LanceDbEngineError::LockIo {
        lock_path: lock_path.to_path_buf(),
        source,
    })
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
