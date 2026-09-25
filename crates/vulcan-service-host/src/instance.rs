//! Cross-process ownership of one Core data directory.
//! 单个 Core 数据目录的跨进程所有权。

use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
};

/// Holds an OS lock until the owning service exits; the lock file is never unlinked.
/// 持有操作系统锁直至服务退出；永不删除锁文件，避免产生两个锁定对象。
pub struct ServiceInstanceGuard {
    /// File handle retaining the kernel lock, including across GUI disconnections.
    /// 持有内核锁的文件句柄，不受 GUI 断开连接影响。
    _lock: File,
}

impl ServiceInstanceGuard {
    /// Acquires ownership of `root`; returns None when another process already owns it.
    /// 获取 `root` 的所有权；另一个进程已经持有时返回 None。
    /// Directory creation and unexpected locking failures are returned to the launcher.
    /// 目录创建及非竞争类锁定错误直接返回启动器。
    pub fn acquire(root: &Path) -> io::Result<Option<Self>> {
        std::fs::create_dir_all(root)?;
        // Retaining the same inode prevents concurrent launchers from bypassing the lock.
        // 保留同一个文件对象，防止并发启动器绕过互斥锁。
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("service.lock"))?;
        match fs2::FileExt::try_lock_exclusive(&lock) {
            // Windows reports lock contention as raw OS error 33 (lock violation) rather
            // than WouldBlock; both spellings mean another owner holds this data root.
            // Windows 以原始错误 33 报告锁争用而非 WouldBlock；两者都表示已有所有者。
            Ok(()) => Ok(Some(Self { _lock: lock })),
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.raw_os_error() == Some(33) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ensures only one owner exists and a released lock can be acquired again.
    /// 验证唯一所有者约束以及锁释放后的重新获取。
    #[test]
    fn ownership_is_exclusive_and_recoverable() {
        // Isolated directory avoids real service files and concurrent user sessions.
        // 隔离目录避免触碰真实服务文件及并发用户会话。
        let directory = tempfile::tempdir().expect("temporary directory");
        let first = ServiceInstanceGuard::acquire(directory.path()).expect("first lock");
        assert!(first.is_some());
        assert!(
            ServiceInstanceGuard::acquire(directory.path())
                .expect("second lock")
                .is_none()
        );
        drop(first);
        assert!(
            ServiceInstanceGuard::acquire(directory.path())
                .expect("released lock")
                .is_some()
        );
        assert!(directory.path().join("service.lock").is_file());
    }
}
