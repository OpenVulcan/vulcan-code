//! Shared content-free sidecar ownership locks for host-owned storage resources.
//! 宿主存储资源共用的无内容旁路所有权锁。
//!
//! The held file is never written, moved or deleted. A separate owner record is
//! diagnostic only; the operating-system lock, not its text, establishes ownership.
//! 被持有文件绝不写入、移动或删除。独立持有者记录仅用于诊断；所有权由系统锁而非文本证明。

use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// A normal ownership conflict or a concrete I/O failure; callers preserve this distinction.
/// 正常所有权竞争或具体 I/O 失败；调用方须保留此区别。
#[derive(Debug, thiserror::Error)]
pub enum OwnershipLockError {
    /// Another process already owns the exact sidecar.
    /// 其他进程已持有精确旁路文件。
    #[error("storage ownership lock held at {lock_path:?}; owner={owner}")]
    Held {
        /// Exact held sidecar path.
        /// 精确的持锁旁路路径。
        lock_path: PathBuf,
        /// Best-effort diagnostic identity, never an ownership authority.
        /// 尽力读取的诊断身份，绝不作为所有权依据。
        owner: String,
    },
    /// The lock location or diagnostic publication failed.
    /// 锁位置或诊断发布失败。
    #[error("storage ownership lock I/O at {lock_path:?}: {source}")]
    Io {
        /// Exact sidecar whose acquisition failed.
        /// 获取失败的精确旁路路径。
        lock_path: PathBuf,
        /// Original operating-system failure.
        /// 原始操作系统错误。
        #[source]
        source: std::io::Error,
    },
}

/// Lifetime guard for the OS lock. Dropping the value releases ownership without deleting files.
/// OS 锁的生命周期守卫；释放值即解除所有权，但不删除文件。
#[derive(Debug)]
pub struct OwnershipLock {
    lock_path: PathBuf,
    owner_path: PathBuf,
    owner: String,
    file: File,
}

impl OwnershipLock {
    /// Acquires one exact lock path and publishes a diagnostic identity only after acquisition.
    /// 获取精确锁路径，仅在获取成功后发布诊断身份。
    /// The caller supplies a normalized resource-derived path; this API never changes its identity.
    /// 调用方提供源于规范资源的路径；此 API 绝不改变其身份。
    pub fn acquire(lock_path: PathBuf, owner: String) -> Result<Self, OwnershipLockError> {
        let io = |source| OwnershipLockError::Io {
            lock_path: lock_path.clone(),
            source,
        };
        if let Some(parent) = lock_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(io)?;
        }
        let owner_path = owner_record_path(&lock_path);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(io)?;
        if let Err(error) = fs4::FileExt::try_lock(&file) {
            return Err(match error {
                fs4::TryLockError::WouldBlock => OwnershipLockError::Held {
                    owner: read_owner_record(&owner_path).unwrap_or_else(|| "unknown".to_string()),
                    lock_path,
                },
                fs4::TryLockError::Error(source) => io(source),
            });
        }
        // The already-held file remains content-free on Windows as well as Unix.
        // Windows 与 Unix 上，被持有的文件均保持无内容。
        let mut record = File::create(&owner_path).map_err(io)?;
        record
            .write_all(owner.as_bytes())
            .and_then(|()| record.write_all(b"\n"))
            .and_then(|()| record.flush())
            .map_err(io)?;
        Ok(Self {
            lock_path,
            owner_path,
            owner,
            file,
        })
    }

    /// Returns the exact held sidecar path.
    /// 返回精确持锁旁路路径。
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
    /// Returns the separate, freely readable diagnostic path.
    /// 返回独立且可读取的诊断路径。
    pub fn owner_path(&self) -> &Path {
        &self.owner_path
    }
    /// Returns the identity successfully published by this holder.
    /// 返回当前持有者成功发布的身份。
    pub fn owner(&self) -> &str {
        &self.owner
    }
}

impl Drop for OwnershipLock {
    fn drop(&mut self) {
        let _ = fs4::FileExt::unlock(&self.file);
    }
}

/// Derives a sibling lock without lossy conversion of native file names.
/// 不对原生文件名进行有损转换，派生相邻的锁路径。
/// Bare input remains relative; production callers normalize the resource first.
/// 裸输入仍保持相对路径；生产调用方须先规范化资源路径。
pub fn sidecar_path(resource: &Path, fallback: &str) -> PathBuf {
    let name = resource.file_name().map_or_else(
        || OsString::from(fallback),
        |name| {
            let mut name = name.to_os_string();
            name.push(".vldb.lock");
            name
        },
    );
    resource
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from(&name), |parent| parent.join(&name))
}

/// Derives the diagnostic owner record from the exact lock path.
/// 从精确锁路径派生诊断持有者记录。
pub fn owner_record_path(lock_path: &Path) -> PathBuf {
    let mut path = lock_path.as_os_str().to_os_string();
    path.push(".owner");
    PathBuf::from(path)
}

/// Reads a bounded, advisory owner record without turning diagnostics into lock authority.
/// 有界读取建议性持有者记录，不把诊断内容变为锁所有权依据。
pub fn read_owner_record(owner_path: &Path) -> Option<String> {
    const MAX_OWNER_BYTES: usize = 64 * 1_024;
    let mut text = String::new();
    File::open(owner_path)
        .ok()?
        .take((MAX_OWNER_BYTES + 1) as u64)
        .read_to_string(&mut text)
        .ok()?;
    if text.len() > MAX_OWNER_BYTES {
        return None;
    }
    let owner = text.trim();
    (!owner.is_empty()).then(|| owner.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shared allocation stays content-free and distinguishes contention from I/O failure.
    /// 共用分配保持无内容，并区分竞争与 I/O 失败。
    #[test]
    fn sidecar_ownership_preserves_contention_and_release() {
        let root = tempfile::tempdir().expect("isolated ownership fixture");
        let path = sidecar_path(&root.path().join("db"), "fallback");
        let held = OwnershipLock::acquire(path.clone(), "first-holder".into()).expect("acquire");
        assert_eq!(
            fs::metadata(held.lock_path()).expect("held metadata").len(),
            0
        );
        assert_eq!(
            read_owner_record(held.owner_path()).as_deref(),
            Some("first-holder")
        );
        assert!(
            matches!(OwnershipLock::acquire(path.clone(), "other".into()),
            Err(OwnershipLockError::Held { owner, .. }) if owner == "first-holder")
        );
        drop(held);
        let next = OwnershipLock::acquire(path, "new-holder".into()).expect("reacquire");
        assert_eq!(
            read_owner_record(next.owner_path()).as_deref(),
            Some("new-holder")
        );
    }

    /// Failed owner publication releases its OS lock instead of leaving an invisible holder.
    /// 持有者发布失败时释放 OS 锁，不遗留不可见持有者。
    #[test]
    fn diagnostic_write_failure_releases_the_lock() {
        let root = tempfile::tempdir().expect("isolated fixture");
        let path = sidecar_path(&root.path().join("db"), "fallback");
        let owner = owner_record_path(&path);
        fs::create_dir(&owner).expect("block diagnostic file");
        assert!(matches!(
            OwnershipLock::acquire(path.clone(), "owner".into()),
            Err(OwnershipLockError::Io { .. })
        ));
        fs::remove_dir(owner).expect("remove fixture directory");
        OwnershipLock::acquire(path, "next".into()).expect("failed holder released ownership");
    }

    /// Non-UTF-8 names that display identically must remain distinct physical lock identities.
    /// 显示相同的非 UTF-8 文件名必须保留不同的物理锁身份。
    #[cfg(unix)]
    #[test]
    fn invalid_utf8_names_never_alias() {
        use std::os::unix::ffi::OsStringExt;
        let root = tempfile::tempdir().expect("isolated fixture");
        let a = root.path().join(OsString::from_vec(vec![b'd', 0x80]));
        let b = root.path().join(OsString::from_vec(vec![b'd', 0x81]));
        assert_eq!(a.to_string_lossy(), b.to_string_lossy());
        let a = OwnershipLock::acquire(sidecar_path(&a, "fallback"), "a".into())
            .expect("first identity");
        let b = OwnershipLock::acquire(sidecar_path(&b, "fallback"), "b".into())
            .expect("second identity");
        assert_ne!(a.lock_path(), b.lock_path());
    }
}
