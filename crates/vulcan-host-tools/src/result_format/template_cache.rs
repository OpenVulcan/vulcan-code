//! Bounded template content reuse, with access and file-version checks on every lookup.
//! 有界复用模板正文，每次查询仍校验访问权限及文件版本。

use std::{
    collections::HashMap,
    fs::{File, Metadata},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant, SystemTime},
};

const MAX_ENTRIES: usize = 64;
const MAX_BYTES: usize = 2 * 1024 * 1024;
// File versions normally invalidate immediately. The age bound also covers filesystems with
// coarse or externally restored timestamps; missing files and read failures are never cached.
// 文件版本通常会立即使缓存失效；时效上限还覆盖时间戳粗粒度或被外部还原的文件系统；不缓存缺失与读取失败。
const MAX_AGE: Duration = Duration::from_secs(1);
static CACHE: LazyLock<Mutex<TemplateCache>> =
    LazyLock::new(|| Mutex::new(TemplateCache::default()));

#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    bytes: u64,
    modified: SystemTime,
    created: Option<SystemTime>,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

fn stamp(metadata: &Metadata) -> Option<Stamp> {
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    Some(Stamp {
        bytes: metadata.len(),
        modified: metadata.modified().ok()?,
        created: metadata.created().ok(),
        #[cfg(unix)]
        identity: (
            metadata.dev(),
            metadata.ino(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        ),
    })
}

struct Entry {
    stamp: Stamp,
    content: Arc<str>,
    loaded: Instant,
}

#[derive(Default)]
struct TemplateCache {
    entries: HashMap<PathBuf, Entry>,
    bytes: usize,
}

impl TemplateCache {
    fn get(&mut self, path: &Path, stamp: &Stamp, now: Instant) -> Option<Arc<str>> {
        if let Some(entry) = self.entries.get(path)
            && &entry.stamp == stamp
            && now.saturating_duration_since(entry.loaded) < MAX_AGE
        {
            return Some(Arc::clone(&entry.content));
        }
        self.remove(path);
        None
    }

    fn remove(&mut self, path: &Path) {
        if let Some(entry) = self.entries.remove(path) {
            self.bytes -= entry.content.len();
        }
    }

    fn insert(&mut self, path: PathBuf, stamp: Stamp, content: Arc<str>, loaded: Instant) {
        self.remove(&path);
        // Large templates are still readable but never monopolize the bounded cache.
        // 大模板仍可读取，但绝不独占有界缓存。
        if content.len() > MAX_BYTES {
            return;
        }
        while self.entries.len() >= MAX_ENTRIES
            || self.bytes.saturating_add(content.len()) > MAX_BYTES
        {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.loaded)
                .map(|(path, _)| path.clone())
            else {
                break;
            };
            self.remove(&oldest);
        }
        self.bytes += content.len();
        self.entries.insert(
            path,
            Entry {
                stamp,
                content,
                loaded,
            },
        );
    }
}

/// Opens the exact candidate before consulting cached content, preserving permission failures.
/// 在查询缓存正文前打开精确候选文件，保留权限失败语义。
pub(super) fn read(path: &Path) -> io::Result<Arc<str>> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut file = File::open(&path)?;
    let before = stamp(&file.metadata()?);
    if let Some(before) = &before
        && let Some(content) = CACHE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&path, before, Instant::now())
    {
        return Ok(content);
    }
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    let content: Arc<str> = text.into();
    if let Some(before) = before
        && stamp(&file.metadata()?).as_ref() == Some(&before)
    {
        // Never hold the process-wide cache mutex across filesystem reads.
        // 绝不跨文件系统读取持有进程级缓存互斥锁。
        CACHE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(path, before, Arc::clone(&content), Instant::now());
    }
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version() -> Stamp {
        Stamp {
            bytes: 1,
            modified: SystemTime::UNIX_EPOCH,
            created: None,
            #[cfg(unix)]
            identity: (1, 1, 1, 1),
        }
    }

    #[test]
    fn version_and_expiry_are_checked_without_sleeping() {
        let mut cache = TemplateCache::default();
        let path = PathBuf::from("one");
        let now = Instant::now();
        let content: Arc<str> = "first".into();
        cache.insert(path.clone(), version(), Arc::clone(&content), now);
        assert!(Arc::ptr_eq(
            &cache.get(&path, &version(), now).expect("hit"),
            &content
        ));
        assert!(cache.get(&path, &version(), now + MAX_AGE).is_none());
        cache.insert(path.clone(), version(), content, now);
        let mut next = version();
        next.bytes += 1;
        assert!(cache.get(&path, &next, now).is_none());
        assert_eq!(cache.bytes, 0);
    }

    #[test]
    fn retained_entries_and_bytes_are_bounded() {
        let mut cache = TemplateCache::default();
        let now = Instant::now();
        for index in 0..MAX_ENTRIES * 2 {
            cache.insert(PathBuf::from(index.to_string()), version(), "x".into(), now);
        }
        assert_eq!(cache.entries.len(), MAX_ENTRIES);
        for index in 0..4 {
            cache.insert(
                PathBuf::from(index.to_string()),
                version(),
                "x".repeat(MAX_BYTES / 2).into(),
                now,
            );
        }
        assert!(cache.bytes <= MAX_BYTES);
        assert!(cache.entries.len() <= MAX_ENTRIES);
    }

    #[test]
    fn reopened_files_are_checked_and_missing_results_are_not_retained() {
        let root = tempfile::tempdir().expect("isolated root");
        let path = root.path().join("template.md");
        assert!(read(&path).is_err());
        std::fs::write(&path, "first").expect("write first version");
        let first = read(&path).expect("read first");
        let again = read(&path).expect("reuse");
        assert!(Arc::ptr_eq(&first, &again));
        std::fs::remove_file(&path).expect("remove");
        assert_eq!(
            read(&path).expect_err("deletion is immediate").kind(),
            io::ErrorKind::NotFound
        );
        std::fs::write(&path, "second version").expect("new file");
        assert_eq!(read(&path).expect("new version").as_ref(), "second version");
    }
}
