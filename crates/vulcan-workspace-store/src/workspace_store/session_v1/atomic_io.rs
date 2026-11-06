//! Atomic single-file publication with stage-level fault injection.
//! 带阶段级故障注入的原子单文件发布。
//!
//! The algorithm mirrors the retained `write_file_bytes_atomic` capability:
//! a unique sibling staging file, `create_new` collision detection, full
//! write, `sync_all`, OS-atomic rename, and parent-directory sync on Unix.
//! A test-only hook can fail exactly one chosen stage so crash-consistency
//! tests can prove the old published state stays intact.
//! 算法镜像保留的 `write_file_bytes_atomic` 能力：唯一同级暂存文件、
//! `create_new` 碰撞检测、完整写入、`sync_all`、操作系统原子 rename 与
//! Unix 上的父目录同步。测试专用钩子可以精确让某个阶段失败，使崩溃一致性
//! 测试能证明旧已发布状态保持完好。

use std::fs;
use std::io::Write;
use std::path::Path;

use super::errors::SessionV1Error;

/// One failure-injectable stage of the atomic publish sequence.
/// 原子发布序列中一个可注入失败的阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteStage {
    /// Staging file creation.
    /// 暂存文件创建。
    StagingCreate,
    /// Full payload write.
    /// 完整载荷写入。
    WriteAll,
    /// `sync_all` on the staging file.
    /// 对暂存文件执行 `sync_all`。
    SyncAll,
    /// OS-atomic rename over the target.
    /// 覆盖目标的 OS 原子 rename。
    Rename,
}

/// Atomically publishes one complete file at `path`.
/// 在 `path` 原子发布一个完整文件。
///
/// On any failure the target path is untouched and the staging file is
/// removed; callers receive a structured `AtomicWriteFailed` error.
/// 任何失败时目标路径保持不变并且暂存文件被移除；调用方收到结构化的
/// `AtomicWriteFailed` 错误。
pub fn write_file_atomic(
    path: &Path,
    encoded: &[u8],
    artifact_kind: &str,
) -> Result<(), SessionV1Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            SessionV1Error::io(parent, format!("create parent directory: {error}"))
        })?;
    }
    maybe_fail(WriteStage::StagingCreate, path, artifact_kind)?;
    let (staged_path, mut staged_file) = create_staging_file(path, artifact_kind)?;
    let staging_result = (|| -> std::io::Result<()> {
        // The replacement keeps the mode of the file it replaces, so a file the user restricted
        // stays restricted.
        // 替换文件保留被替换文件的权限，使用户收紧过的文件保持收紧。
        #[cfg(unix)]
        match fs::metadata(path) {
            Ok(previous) => staged_file.set_permissions(previous.permissions())?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        if should_fail_io(WriteStage::WriteAll, path, artifact_kind) {
            return Err(std::io::Error::other("injected write_all failure"));
        }
        staged_file.write_all(encoded)?;
        if should_fail_io(WriteStage::SyncAll, path, artifact_kind) {
            return Err(std::io::Error::other("injected sync_all failure"));
        }
        staged_file.sync_all()
    })();
    if let Err(error) = staging_result {
        drop(staged_file);
        let _ = fs::remove_file(&staged_path);
        return Err(SessionV1Error::atomic_write(
            path,
            "stage",
            format!("staging {artifact_kind}: {error}"),
        ));
    }
    drop(staged_file);
    maybe_fail(WriteStage::Rename, path, artifact_kind)?;
    if let Err(error) = fs::rename(&staged_path, path) {
        let _ = fs::remove_file(&staged_path);
        return Err(SessionV1Error::atomic_write(
            path,
            "rename",
            format!("replace {artifact_kind}: {error}"),
        ));
    }
    #[cfg(unix)]
    synchronize_parent_directory(path, artifact_kind)?;
    Ok(())
}

/// Creates one unique sibling staging file with `create_new` semantics.
/// 使用 `create_new` 语义创建一个唯一同级暂存文件。
fn create_staging_file(
    path: &Path,
    artifact_kind: &str,
) -> Result<(std::path::PathBuf, fs::File), SessionV1Error> {
    let parent = path
        .parent()
        .ok_or_else(|| SessionV1Error::io(path.to_path_buf(), "cannot resolve parent directory"))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("segment.jsonl");
    for attempt in 0..128u32 {
        let staged = parent.join(format!(
            ".{file_name}.stage-{attempt}-{}",
            std::process::id()
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)
        {
            Ok(file) => return Ok((staged, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(SessionV1Error::atomic_write(
                    path,
                    "staging",
                    format!("create unique staging file for {artifact_kind}: {error}"),
                ));
            }
        }
    }
    Err(SessionV1Error::atomic_write(
        path,
        "staging",
        "unable to create a unique staging file after 128 attempts",
    ))
}

/// Fails the current write when the test hook is armed for the stage.
/// 当测试钩子为当前阶段挂载失败时使本次写入失败。
#[cfg(test)]
fn maybe_fail(stage: WriteStage, path: &Path, artifact_kind: &str) -> Result<(), SessionV1Error> {
    if test_hooks::should_fail(stage, artifact_kind) {
        return Err(SessionV1Error::atomic_write(
            path,
            "fault-injection",
            format!("injected failure at {artifact_kind} stage"),
        ));
    }
    Ok(())
}

/// Returns whether one in-closure stage should fail right now.
/// 返回某个闭包内阶段当前是否应该失败。
#[cfg(test)]
fn should_fail_io(stage: WriteStage, path: &Path, artifact_kind: &str) -> bool {
    if test_hooks::should_fail(stage, artifact_kind) {
        let _ = (path, artifact_kind);
        return true;
    }
    false
}

/// Synchronizes the renamed parent directory on POSIX systems.
/// 在 POSIX 系统上同步重命名后的父目录。
#[cfg(unix)]
fn synchronize_parent_directory(path: &Path, _artifact_kind: &str) -> Result<(), SessionV1Error> {
    if let Some(parent) = path.parent() {
        let directory = fs::File::open(parent).map_err(|error| {
            SessionV1Error::io(
                parent.to_path_buf(),
                format!("open parent directory: {error}"),
            )
        })?;
        directory.sync_all().map_err(|error| {
            SessionV1Error::io(
                parent.to_path_buf(),
                format!("sync parent directory: {error}"),
            )
        })?;
    }
    Ok(())
}

/// Test-only stage failure hook.
/// 仅测试使用的阶段失败钩子。
#[cfg(test)]
pub(crate) mod test_hooks {
    use std::cell::RefCell;
    use std::thread_local;

    use super::*;

    // Thread-local so parallel tests never consume another test's pending
    // failure; flushes and their atomic writes run on the same thread.
    // 使用 thread-local 使并行测试绝不会消费其他测试挂载的失败；flush 与其
    // 原子写在同一线程上运行。
    /// One test-only failure selector.
    /// 一个仅供测试使用的失败选择器。
    struct FailureRule {
        /// Shifted write-stage discriminant.
        /// 偏移后的写入阶段判别值。
        stage: u8,
        /// Optional artifact substring that must match.
        /// 必须匹配的可选产物子串。
        artifact_contains: Option<String>,
    }

    thread_local! {
        static NEXT_FAILURE: RefCell<Option<FailureRule>> = const { RefCell::new(None) };
    }

    /// Arms one failure for the next single atomic write.
    /// 为下一次单次原子写挂载一次失败。
    pub fn fail_next_write(stage: WriteStage) {
        NEXT_FAILURE.with(|slot| {
            *slot.borrow_mut() = Some(FailureRule {
                stage: stage as u8 + 1,
                artifact_contains: None,
            });
        });
    }

    /// Arms one failure for the next matching artifact and stage.
    /// 为下一个匹配产物与阶段的写入挂载一次失败。
    pub fn fail_next_artifact(stage: WriteStage, artifact_contains: impl Into<String>) {
        NEXT_FAILURE.with(|slot| {
            *slot.borrow_mut() = Some(FailureRule {
                stage: stage as u8 + 1,
                artifact_contains: Some(artifact_contains.into()),
            });
        });
    }

    /// Disarms any pending failure.
    /// 解除任何挂载中的失败。
    pub fn clear_failures() {
        NEXT_FAILURE.with(|slot| *slot.borrow_mut() = None);
    }

    /// Returns whether the given stage should fail right now (exactly once).
    /// 返回当前阶段是否应该立刻失败(恰好一次)。
    ///
    /// The armed value is consumed only when the stage matches, so checking
    /// earlier stages never swallows a pending failure for a later stage.
    /// 只有阶段匹配时才消费挂载值，因此对更早阶段的预检绝不会吞掉为更晚
    /// 阶段挂载的失败。
    pub fn should_fail(stage: WriteStage, artifact_kind: &str) -> bool {
        NEXT_FAILURE.with(|slot| {
            let matched = slot.borrow().as_ref().is_some_and(|rule| {
                rule.stage == stage as u8 + 1
                    && rule
                        .artifact_contains
                        .as_deref()
                        .is_none_or(|needle| artifact_kind.contains(needle))
            });
            if matched {
                slot.borrow_mut().take();
            }
            matched
        })
    }
}

/// Production builds never fail via the test hook.
/// 生产构建绝不通过测试钩子失败。
#[cfg(not(test))]
fn maybe_fail(
    _stage: WriteStage,
    _path: &Path,
    _artifact_kind: &str,
) -> Result<(), SessionV1Error> {
    Ok(())
}

/// Production builds never fail via the in-closure hook.
/// 生产构建的闭包内钩子绝不失败。
#[cfg(not(test))]
fn should_fail_io(_stage: WriteStage, _path: &Path, _artifact_kind: &str) -> bool {
    false
}
