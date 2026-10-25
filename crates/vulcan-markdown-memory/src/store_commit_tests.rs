//! Fault and concurrency tests exercise the production memory commit algorithm.
//! 故障与并发测试验证生产记忆提交算法。

use super::*;

/// Builds a prepared change from `path` and its current bytes, returning the exact expected original.
/// 根据 `path` 及当前字节构建待提交变更，返回精确期望原文。
fn change(path: &Path, text: &str) -> PreparedFile {
    PreparedFile {
        path: path.to_path_buf(),
        original: MarkdownMemoryStore::read_optional(path).expect("read fixture"),
        text: text.into(),
    }
}

/// A late replacement failure restores overwritten files and removes newly created files.
/// 后续替换失败时还原被覆盖的文件，并移除新建文件。
#[test]
fn late_failure_restores_existing_and_absent_originals() {
    // Every file and fault belongs to this test's isolated storage.
    // 所有文件与故障均属于本测试的隔离存储。
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("existing.md");
    let second = root.path().join("new.md");
    let last = root.path().join("blocked.md");
    std::fs::write(&first, "original\r\n").unwrap();
    std::fs::write(&last, "untouched").unwrap();
    let guard = write_guard();
    let result = commit_files_with(
        &guard,
        vec![
            change(&first, "replaced"),
            change(&second, "created"),
            change(&last, "blocked"),
        ],
        |source, destination| {
            if destination == last {
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            } else {
                std::fs::rename(source, destination)
            }
        },
    );
    assert!(matches!(result, Err(MemoryError::Io { .. })));
    assert_eq!(std::fs::read(&first).unwrap(), b"original\r\n");
    assert!(!second.exists());
    assert_eq!(std::fs::read(&last).unwrap(), b"untouched");
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
}

/// Failed rollback reports the exact inconsistent destination and retains its original bytes.
/// 回滚失败会报告精确的不一致目标，并保留其原文字节。
#[test]
fn incomplete_rollback_preserves_a_recovery_copy() {
    // The second failure is injected only after the first replacement really reached disk.
    // 第二次故障仅在第一次替换真正落盘后注入。
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("first.md");
    let last = root.path().join("blocked.md");
    std::fs::write(&first, "original").unwrap();
    let guard = write_guard();
    let result = commit_files_with(
        &guard,
        vec![change(&first, "replaced"), change(&last, "blocked")],
        |source, destination| {
            if destination == last
                || std::fs::read_to_string(destination).is_ok_and(|text| text == "replaced")
            {
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            } else {
                std::fs::rename(source, destination)
            }
        },
    );
    let Err(MemoryError::RollbackIncomplete {
        inconsistent_files,
        recovery_files,
        ..
    }) = result
    else {
        panic!("expected incomplete rollback");
    };
    assert_eq!(inconsistent_files, vec![first.clone()]);
    assert_eq!(recovery_files.len(), 1);
    for recovery in recovery_files {
        assert_eq!(std::fs::read(recovery).unwrap(), b"original");
    }
    assert_eq!(std::fs::read(first).unwrap(), b"replaced");
    assert!(!last.exists());
}

/// Rollback never destroys external text written after this transaction's first replacement.
/// 回滚绝不破坏此事务首次替换之后写入的外部文本。
#[test]
fn rollback_does_not_overwrite_an_external_edit() {
    // Simulate another writer at the precise failed-publication boundary.
    // 在精确的发布失败边界模拟其他写入方。
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("first.md");
    let last = root.path().join("blocked.md");
    std::fs::write(&first, "original").unwrap();
    let guard = write_guard();
    let result = commit_files_with(
        &guard,
        vec![change(&first, "replaced"), change(&last, "blocked")],
        |source, destination| {
            if destination == last {
                std::fs::write(&first, "external").unwrap();
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            } else {
                std::fs::rename(source, destination)
            }
        },
    );
    assert!(matches!(
        result,
        Err(MemoryError::RollbackIncomplete { .. })
    ));
    assert_eq!(std::fs::read(first).unwrap(), b"external");
}

/// Two editors starting from the same bytes cannot both report successful conflicting saves.
/// 从同一原文开始的两个编辑器不能对冲突保存同时报告成功。
#[test]
fn concurrent_whole_file_edits_admit_one_writer() {
    // A barrier starts both public-API writers together, using distinct replacement text.
    // 屏障让两个公开 API 写入方同时开始，并使用不同替换文本。
    let root = tempfile::tempdir().unwrap();
    let store = MarkdownMemoryStore::new(root.path());
    let original = "old\n".repeat(32768);
    store
        .replace_file_text(MemoryScope::User, None, "", &original)
        .unwrap();
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let handles = ["first\n", "second\n"].map(|text| {
            let original = &original;
            let barrier = &barrier;
            let store = &store;
            scope.spawn(move || {
                barrier.wait();
                store.replace_file_text(MemoryScope::User, None, original, &text.repeat(32768))
            })
        });
        handles.map(|thread| thread.join().unwrap())
    });
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(MemoryError::ConcurrentModification(_))))
            .count(),
        1
    );
}

/// Sharing violations are retried only up to the single declared Windows attempt limit.
/// 共享冲突仅按唯一定义的 Windows 尝试上限重试。
#[cfg(windows)]
#[test]
fn windows_replacement_retry_is_bounded() {
    // An always-busy file reaches the bound instead of waiting indefinitely.
    // 始终繁忙的文件会达到上限，而不是无限等待。
    let mut attempts = 0;
    let result = retry_file_mutation(|| {
        attempts += 1;
        Err(std::io::Error::from_raw_os_error(32))
    });
    assert!(result.is_err());
    assert_eq!(attempts, WINDOWS_MUTATION_ATTEMPTS);
    // A transient denial can recover before the bound.
    // 临时拒绝可以在达到上限前恢复。
    let mut attempts = 0;
    retry_file_mutation(|| {
        attempts += 1;
        if attempts < WINDOWS_MUTATION_ATTEMPTS {
            Err(std::io::Error::from_raw_os_error(5))
        } else {
            Ok(())
        }
    })
    .unwrap();
    assert_eq!(attempts, WINDOWS_MUTATION_ATTEMPTS);
}
