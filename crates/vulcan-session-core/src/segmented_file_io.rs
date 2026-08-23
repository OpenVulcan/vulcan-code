//! Shared segmented-session file IO coordination.
//! 共享分段会话文件 IO 协调。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex, MutexGuard, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard, Weak,
};

/// Exact-key registry for process-local root-session storage locks.
/// 进程内根会话存储锁的精确键注册表。
static SEGMENTED_SESSION_FILE_IO_LOCKS: OnceLock<Mutex<BTreeMap<PathBuf, Weak<RwLock<()>>>>> =
    OnceLock::new();

/// One retained handle for the exact root-session storage lock.
/// 精确根会话存储锁的一个保留句柄。
#[derive(Debug)]
pub struct SegmentedSessionFileIoLock {
    /// Exact root-session directory used as the registry identity.
    /// 用作注册表身份的精确根会话目录。
    root_session_dir: PathBuf,
    /// Shared reader-writer lock for this root session only.
    /// 仅供当前根会话共享的读写锁。
    inner: Arc<RwLock<()>>,
}

impl SegmentedSessionFileIoLock {
    /// Acquires shared access while one exact immutable byte snapshot is captured.
    /// 在捕获一个精确不可变字节快照期间获取共享访问权。
    ///
    /// # Returns
    /// 返回值
    /// A read guard scoped to this root session, or the poisoned-lock error.
    /// 返回限定于当前根会话的读锁守卫，或锁中毒错误。
    pub fn read(&self) -> std::sync::LockResult<RwLockReadGuard<'_, ()>> {
        self.inner.read()
    }

    /// Acquires exclusive access for append, rollback, migration, or repair.
    /// 为追加、回滚、迁移或修复获取独占访问权。
    ///
    /// # Returns
    /// 返回值
    /// A write guard scoped to this root session, or the poisoned-lock error.
    /// 返回限定于当前根会话的写锁守卫，或锁中毒错误。
    pub fn write(&self) -> std::sync::LockResult<RwLockWriteGuard<'_, ()>> {
        self.inner.write()
    }

    /// Acquires exclusive access through the legacy append-lock method name.
    /// 通过旧版追加锁方法名获取独占访问权。
    ///
    /// # Returns
    /// 返回值
    /// A write guard scoped to this root session, or the poisoned-lock error.
    /// 返回限定于当前根会话的写锁守卫，或锁中毒错误。
    pub fn lock(&self) -> std::sync::LockResult<RwLockWriteGuard<'_, ()>> {
        self.write()
    }
}

impl Drop for SegmentedSessionFileIoLock {
    /// Removes only this exact stale registry entry after the final retained handle is released.
    /// 在最后一个保留句柄释放后，仅移除当前精确的过期注册表项。
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) != 1 {
            return;
        }
        // registry_guard protects only one exact-key lookup and optional removal.
        // registry_guard 仅保护一次精确键查询与可选移除。
        let mut registry_guard = segmented_session_file_io_registry();
        if Arc::strong_count(&self.inner) != 1 {
            return;
        }
        // owns_exact_entry rejects deleting a replacement lock installed for the same path.
        // owns_exact_entry 防止删除同一路径后来安装的替代锁。
        let owns_exact_entry = registry_guard
            .get(&self.root_session_dir)
            .and_then(Weak::upgrade)
            .is_some_and(|registered| Arc::ptr_eq(&registered, &self.inner));
        if owns_exact_entry {
            registry_guard.remove(&self.root_session_dir);
        }
    }
}

/// Returns the exact root-session storage lock without scanning any directory or registry entry.
/// 返回精确根会话存储锁，不扫描任何目录或注册表项。
///
/// # Parameters
/// 参数
/// `root_session_dir` is the deterministic main-session directory shared by its TaskSessions.
/// `root_session_dir` 是主会话及其 TaskSession 共享的确定性主会话目录。
///
/// # Returns
/// 返回值
/// A retained handle whose read and write guards cannot block unrelated root sessions.
/// 返回一个保留句柄，其读写守卫不会阻塞无关根会话。
pub fn segmented_append_file_io_lock(root_session_dir: &Path) -> SegmentedSessionFileIoLock {
    // root_session_dir owns an exact lexical identity; no filesystem discovery or canonicalization occurs.
    // root_session_dir 拥有精确词法身份；此处不执行文件系统发现或规范化。
    let root_session_dir = root_session_dir.to_path_buf();
    // registry_guard is held only while the exact key is upgraded or replaced.
    // registry_guard 仅在升级或替换精确键期间持有。
    let mut registry_guard = segmented_session_file_io_registry();
    // inner reuses the live exact-key lock or replaces only its stale weak entry.
    // inner 复用仍存活的精确键锁，或仅替换其过期弱引用表项。
    let inner = match registry_guard
        .get(&root_session_dir)
        .and_then(Weak::upgrade)
    {
        Some(inner) => inner,
        None => {
            let inner = Arc::new(RwLock::new(()));
            registry_guard.insert(root_session_dir.clone(), Arc::downgrade(&inner));
            inner
        }
    };
    SegmentedSessionFileIoLock {
        root_session_dir,
        inner,
    }
}

/// Locks the process-local exact-key registry while recovering from prior mutex poisoning.
/// 锁定进程内精确键注册表，并从先前的互斥锁中毒中恢复。
///
/// # Returns
/// 返回值
/// The registry guard used for one exact-key operation.
/// 返回用于一次精确键操作的注册表守卫。
fn segmented_session_file_io_registry() -> MutexGuard<'static, BTreeMap<PathBuf, Weak<RwLock<()>>>>
{
    // registry contains weak handles only and never owns a root-session lock lifetime.
    // registry 仅包含弱句柄，绝不拥有根会话锁生命周期。
    let registry = SEGMENTED_SESSION_FILE_IO_LOCKS.get_or_init(|| Mutex::new(BTreeMap::new()));
    registry
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies unrelated roots receive independent locks and released entries are removed exactly.
    /// 验证无关根目录获得独立锁，并且释放后的表项会被精确移除。
    #[test]
    fn segmented_storage_locks_are_sharded_and_exactly_reclaimed() {
        // root_a and root_b are lexical test identities and never touch the filesystem.
        // root_a 与 root_b 是词法测试身份，不接触文件系统。
        let root_a = Path::new("session-a");
        let root_b = Path::new("session-b");
        // lock_a and lock_a_again must share one exact-key inner lock.
        // lock_a 与 lock_a_again 必须共享同一个精确键内部锁。
        let lock_a = segmented_append_file_io_lock(root_a);
        let lock_a_again = segmented_append_file_io_lock(root_a);
        // lock_b must be independent from root A.
        // lock_b 必须与根目录 A 相互独立。
        let lock_b = segmented_append_file_io_lock(root_b);

        assert!(Arc::ptr_eq(&lock_a.inner, &lock_a_again.inner));
        assert!(!Arc::ptr_eq(&lock_a.inner, &lock_b.inner));
        drop(lock_a_again);
        drop(lock_a);
        drop(lock_b);

        let registry_guard = segmented_session_file_io_registry();
        assert!(!registry_guard.contains_key(root_a));
        assert!(!registry_guard.contains_key(root_b));
    }
}
