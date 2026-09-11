//! Database lifetime file ownership lock.
//! 数据库生命周期文件所有权锁。
//!
//! Purpose / 目的:
//!
//! Exactly one **Vulcan Code process** may treat a given normalized SQLite database path as its
//! sole writer. The lock is a sidecar file next to the database; the database file itself is never
//! moved, copied or deleted to resolve contention.
//!
//! 同一个规范化 SQLite 数据库路径只能被一个 **Vulcan Code 进程** 当作唯一写者。锁是数据库旁的
//! 旁路文件；发生竞争时绝不移动、复制或删除数据库文件本身。
//!
//! Scope / 范围:
//!
//! This lock exists only to prevent two host processes from writing the same database. It carries
//! no controller interoperability requirement and no cross-version negotiation.
//!
//! 本锁只用于防止两个宿主进程写同一个数据库，不承担控制器互操作义务，也不做跨版本协商。
//!
//! Two-file layout and why it is required / 双文件布局及其必要性:
//!
//! 1. `<database>.vldb.lock` — the **held** file. Its OS-level exclusive lock is what expresses
//!    ownership. Nothing is ever written into it.
//! 2. `<database>.vldb.lock.owner` — the **owner record**. Freely readable and writable by any
//!    process, so a losing process can report who holds the lock.
//!
//! The split is not cosmetic. On Windows a process that holds an exclusive lock on a file cannot
//! read or write that file's contents — attempts fail with `ERROR_LOCK_VIOLATION` (OS error 33).
//! Storing the owner record inside the locked file therefore makes contention reporting impossible
//! and makes the acquisition path itself fail. Keeping the held file content-free removes both
//! problems while preserving exactly the same ownership semantics.
//!
//! 拆成两个文件不是形式问题。在 Windows 上，持有文件独占锁的进程无法读写该文件内容——尝试会以
//! `ERROR_LOCK_VIOLATION`（系统错误 33）失败。把持有者记录写进被锁文件会导致竞争报告无法实现，
//! 且加锁路径本身就会失败。让被锁文件保持无内容可消除这两个问题，同时保持完全相同的所有权语义。
//!
//! Deliberate design points / 刻意设计点:
//!
//! 1. **Contention is not an I/O error.** `fs4 1.1.0` returns [`fs4::TryLockError`], whose
//!    `WouldBlock` variant means "someone else holds the lock". It must never be folded into a
//!    generic I/O failure, because the two require different host reactions.
//! 2. **Release is explicit and fully qualified.** `std::fs::File` gained inherent `unlock` in
//!    Rust 1.89, so the `Drop` path names `fs4::FileExt::unlock` explicitly to avoid resolving to
//!    the standard-library method.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{EngineResult, SqliteEngineError};

/// Suffix appended to the database file name to derive its ownership lock path.
/// 追加到数据库文件名后以派生其所有权锁路径的后缀。
const LOCK_FILE_SUFFIX: &str = ".vldb.lock";

/// Suffix appended to the lock path to derive the owner record path.
/// 追加到锁路径后以派生持有者记录路径的后缀。
const OWNER_FILE_SUFFIX: &str = ".owner";

/// Fallback lock file name used when the database path has no final path segment.
/// 当数据库路径没有最终路径段时使用的兜底锁文件名。
const FALLBACK_LOCK_FILE_NAME: &str = "sqlite.vldb.lock";

/// An exclusively held ownership lock for one SQLite database path.
/// 针对一个 SQLite 数据库路径的独占所有权锁。
///
/// The lock is released when this value is dropped. Holding the value is what expresses
/// ownership: there is no separate "is locked" query, because any such query would be racy.
///
/// 该值被丢弃时锁即释放。持有该值本身即表达所有权；不提供单独的“是否已加锁”查询，因为任何
/// 这样的查询都存在竞争。
#[derive(Debug)]
pub struct DatabaseFileLock {
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

impl DatabaseFileLock {
    /// Acquires the exclusive ownership lock for `db_path`.
    /// 为 `db_path` 获取独占所有权锁。
    ///
    /// Returns [`SqliteEngineError::LockHeld`] when another holder currently owns the lock, and
    /// [`SqliteEngineError::LockIo`] only for a genuine I/O failure. These are deliberately
    /// distinct because the host reaction differs: contention is an expected environmental
    /// condition, whereas an I/O failure indicates a broken lock location.
    ///
    /// 当其他持有者当前持有该锁时返回 [`SqliteEngineError::LockHeld`]；仅在真实 I/O 失败时返回
    /// [`SqliteEngineError::LockIo`]。二者刻意区分，因为宿主应对方式不同：竞争是预期的环境状态，
    /// 而 I/O 失败说明锁位置本身不可用。
    pub fn acquire(db_path: &Path) -> EngineResult<Self> {
        let lock_path = derive_lock_path(db_path);
        let owner_path = derive_owner_path(&lock_path);

        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent).map_err(|source| SqliteEngineError::LockIo {
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
            .map_err(|source| SqliteEngineError::LockIo {
                lock_path: lock_path.clone(),
                source,
            })?;

        // Read the owner record before attempting the lock so a contention report can name the
        // current holder. This is a separate, unlocked file by design.
        // 在尝试加锁前读取持有者记录，使竞争报告能给出当前持有者。按设计它是独立的、未被锁定的文件。
        let existing_owner = read_owner_record(&owner_path);

        if let Err(lock_error) = fs4::FileExt::try_lock(&file) {
            return Err(match lock_error {
                // Someone else holds the lock. This is contention, not an I/O failure.
                // 其他持有者持有该锁：这是竞争，不是 I/O 失败。
                fs4::TryLockError::WouldBlock => SqliteEngineError::LockHeld {
                    lock_path,
                    owner: existing_owner.unwrap_or_else(|| "unknown".to_string()),
                },
                fs4::TryLockError::Error(source) => SqliteEngineError::LockIo { lock_path, source },
            });
        }

        let owner = current_owner_identity(db_path);

        // The owner record is diagnostic data. It is written after the lock is held, and a failure
        // to write it is surfaced rather than hidden. The lock itself is released by the drop of
        // `file` on this error path.
        // 持有者记录属于诊断数据，在加锁成功后写入；写入失败会向上暴露而不被隐藏。此错误路径上
        // 锁本身由 `file` 的析构释放。
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

impl Drop for DatabaseFileLock {
    /// Releases the OS-level lock.
    /// 释放操作系统级锁。
    ///
    /// The trait method is named explicitly because `std::fs::File` has an inherent `unlock` of
    /// the same name, and an unqualified call would resolve to the standard-library method.
    ///
    /// 这里显式写出 trait 方法名，因为 `std::fs::File` 有同名的固有方法 `unlock`，非限定调用
    /// 会解析到标准库方法。
    fn drop(&mut self) {
        let _ = fs4::FileExt::unlock(&self.file);
    }
}

/// Derives the held lock path for one database path.
/// 为一个数据库路径派生被持有的锁路径。
///
/// The lock lives next to the database so that the two cannot be separated by a mount change, and
/// so that deleting a database directory also removes its stale lock.
///
/// 锁与数据库相邻，避免二者因挂载变化而分离，也使删除数据库目录时一并移除陈旧锁。
fn derive_lock_path(db_path: &Path) -> PathBuf {
    let file_name = db_path.file_name().map_or_else(
        || FALLBACK_LOCK_FILE_NAME.to_string(),
        |name| format!("{}{LOCK_FILE_SUFFIX}", name.to_string_lossy()),
    );

    // `parent()` is `None` for a root path such as `/` on Windows, and for a bare file name. In
    // both cases the resolved directory must still be usable, so fall back explicitly instead of
    // producing a relative path that would depend on the process working directory.
    // 在 Windows 上形如 `/` 的根路径以及裸文件名都会让 `parent()` 返回 `None`。两种情况都必须
    // 得到可用的目录，因此显式兜底，而不是产生依赖进程工作目录的相对路径。
    match db_path.parent() {
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

/// Builds the owner identity string recorded for this process and database.
/// 构造为本进程与该数据库记录的持有者身份字符串。
fn current_owner_identity(db_path: &Path) -> String {
    format!("pid={} db_path={}", std::process::id(), db_path.display())
}

/// Writes the owner record atomically enough for diagnostic purposes.
/// 以对诊断而言足够原子性的方式写入持有者记录。
fn write_owner_record(owner_path: &Path, owner: &str, lock_path: &Path) -> EngineResult<()> {
    let mut file = File::create(owner_path).map_err(|source| SqliteEngineError::LockIo {
        lock_path: lock_path.to_path_buf(),
        source,
    })?;
    file.write_all(owner.as_bytes())
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.flush())
        .map_err(|source| SqliteEngineError::LockIo {
            lock_path: lock_path.to_path_buf(),
            source,
        })
}

/// Reads an existing owner record, returning `None` when it is absent or empty.
/// 读取既有持有者记录；不存在或为空时返回 `None`。
fn read_owner_record(owner_path: &Path) -> Option<String> {
    let text = fs::read_to_string(owner_path).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
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

    /// Verifies a path without a usable parent still yields an absolute or bare lock file name
    /// rather than one that depends on the process working directory.
    /// 验证没有可用父目录的路径仍能得到不依赖进程工作目录的锁文件名。
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
