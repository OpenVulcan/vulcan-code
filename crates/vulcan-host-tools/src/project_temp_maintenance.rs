//! Project-scoped temporary-directory helpers used by the desktop host.
//! 桌面宿主使用的项目级临时目录辅助模块。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Shared retention window applied to project-scoped temp files before cleanup removes them.
/// 项目级临时文件在清理前允许保留的共享时间窗口。
const PROJECT_TEMP_FILE_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

/// Shared hidden Vulcan metadata directory name stored below one project root.
/// 存放在项目根目录下的共享隐藏 Vulcan 元数据目录名称。
const PROJECT_VULCAN_DIRECTORY_NAME: &str = ".vulcan";

/// Shared temp directory name stored below one project-scoped Vulcan metadata directory.
/// 存放在项目级 Vulcan 元数据目录下的共享临时目录名称。
const PROJECT_TEMP_DIRECTORY_NAME: &str = "temp";

/// Resolve the project-scoped temp directory used by host-managed oversized tool output files.
/// 解析宿主管理超长工具输出文件使用的项目级临时目录。
pub fn resolve_project_temp_dir(project_root: &Path) -> PathBuf {
    project_root
        .join(PROJECT_VULCAN_DIRECTORY_NAME)
        .join(PROJECT_TEMP_DIRECTORY_NAME)
}

/// Performs one cleanup pass for a single project temp directory and skips a confirmed missing project.
/// 对单个项目临时目录执行一次清理，并跳过已确认不存在的项目。
///
/// # Parameters
/// # 参数
/// - `project_root`: Exact project root that owns the `.vulcan/temp` subtree.
/// - `project_root`：拥有 `.vulcan/temp` 子树的精确项目根目录。
///
/// # Returns
/// # 返回值
/// Returns success after cleanup or when paths are absent, and preserves every probing or cleanup failure.
/// 清理完成或路径不存在时返回成功；路径探测或清理失败时保留完整错误。
pub fn maintain_project_temp_dir(project_root: &Path) -> Result<(), String> {
    // project_exists distinguishes a missing project from an inaccessible project root.
    // project_exists 区分项目不存在与项目根目录不可访问。
    let project_exists = project_root.try_exists().map_err(|error| {
        format!(
            "探测项目目录失败（{}）：{}。",
            project_root.display(),
            error
        )
    })?;
    if !project_exists {
        return Ok(());
    }

    // temp_dir is the only cleanup subtree owned by this maintenance pass.
    // temp_dir 是本次维护流程唯一负责清理的子树。
    let temp_dir = resolve_project_temp_dir(project_root);
    // temp_metadata distinguishes absence from malformed links without following a temp-root symlink.
    // temp_metadata 在不跟随临时根符号链接的情况下区分路径缺失与异常链接。
    let temp_metadata = match fs::symlink_metadata(&temp_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "读取项目临时目录元数据失败（{}）：{}。",
                temp_dir.display(),
                error
            ));
        }
    };
    if temp_metadata.file_type().is_symlink() {
        return Err(format!(
            "拒绝清理符号链接形式的项目临时目录（{}）。",
            temp_dir.display()
        ));
    }
    // cleanup_root is the canonical descendant used for traversal, so intermediate links cannot redirect deletion.
    // cleanup_root 是用于遍历的规范化后代路径，因此中间链接无法重定向删除操作。
    let cleanup_root = canonical_project_temp_cleanup_root(project_root, &temp_dir)?;
    cleanup_directory_recursive(
        &cleanup_root,
        SystemTime::now(),
        PROJECT_TEMP_FILE_RETENTION,
    )
}

/// Resolves one existing project temp directory and proves its real path remains inside the real project root.
/// 解析一个已存在的项目临时目录，并证明其真实路径仍位于真实项目根目录内。
///
/// # Parameters
/// 参数
/// `project_root` is the persisted project root selected for maintenance.
/// `project_root` 是选中执行维护的持久化项目根目录。
/// `temp_dir` is the existing `.vulcan/temp` candidate derived from that project root.
/// `temp_dir` 是从该项目根目录派生出的现有 `.vulcan/temp` 候选路径。
///
/// # Returns
/// 返回值
/// Returns the canonical cleanup root only when it is a strict project descendant, otherwise an explicit error.
/// 仅当清理根是项目的严格后代时返回规范路径，否则返回显式错误。
fn canonical_project_temp_cleanup_root(
    project_root: &Path,
    temp_dir: &Path,
) -> Result<PathBuf, String> {
    // canonical_project_root resolves a persisted project symlink to its exact owned filesystem root.
    // canonical_project_root 将持久化项目符号链接解析为其精确拥有的文件系统根目录。
    let canonical_project_root = fs::canonicalize(project_root).map_err(|error| {
        format!(
            "解析项目真实目录失败（{}）：{}。",
            project_root.display(),
            error
        )
    })?;
    // canonical_temp_dir resolves every intermediate component before containment is evaluated.
    // canonical_temp_dir 在判断包含关系前解析每一个中间路径组件。
    let canonical_temp_dir = fs::canonicalize(temp_dir).map_err(|error| {
        format!(
            "解析项目临时目录真实路径失败（{}）：{}。",
            temp_dir.display(),
            error
        )
    })?;
    if canonical_temp_dir == canonical_project_root
        || !canonical_temp_dir.starts_with(&canonical_project_root)
    {
        return Err(format!(
            "拒绝清理项目根目录之外的临时目录（项目：{}；临时目录：{}）。",
            canonical_project_root.display(),
            canonical_temp_dir.display()
        ));
    }
    Ok(canonical_temp_dir)
}

/// Performs cleanup for every distinct project root and aggregates failures after all attempts finish.
/// 清理每个不同的项目根目录，并在全部尝试结束后聚合失败。
///
/// # Parameters
/// # 参数
/// - `project_roots`: Candidate project roots from active and archived workspace entries.
/// - `project_roots`：来自活动与归档工作区条目的候选项目根目录。
///
/// # Returns
/// # 返回值
/// Returns success when every distinct project succeeds, or one combined diagnostic containing all failures.
/// 所有不同项目均成功时返回成功；否则返回包含全部失败的一条聚合诊断。
pub fn maintain_project_temp_dirs(project_roots: &[PathBuf]) -> Result<(), String> {
    // normalized_roots provides deterministic ordering and prevents duplicate cleanup attempts.
    // normalized_roots 提供确定性顺序，并避免重复执行清理。
    let mut normalized_roots = project_roots
        .iter()
        .filter(|path| !path.as_os_str().is_empty())
        .cloned()
        .collect::<Vec<_>>();
    normalized_roots.sort();
    normalized_roots.dedup();

    // failures preserve every project identity while allowing later projects to continue cleanup.
    // failures 保留每个项目身份，同时允许后续项目继续清理。
    let failures = normalized_roots
        .into_iter()
        .filter_map(|project_root| {
            maintain_project_temp_dir(&project_root)
                .err()
                .map(|error| format!("- {}：{}", project_root.display(), error))
        })
        .collect::<Vec<_>>();

    if failures.is_empty() {
        return Ok(());
    }

    Err(format!(
        "项目临时目录清理失败（{} 个项目）：\n{}",
        failures.len(),
        failures.join("\n")
    ))
}

/// Recursively delete expired files and prune empty directories inside one project temp subtree.
/// 递归删除单个项目临时子树中的过期文件，并裁剪空目录。
///
/// # Parameters
/// # 参数
/// - `directory_path`: Current directory inside the selected project temp subtree.
/// - `directory_path`：选定项目临时子树内的当前目录。
/// - `now`: Stable cleanup reference time shared by the entire recursive pass.
/// - `now`：整次递归清理共享的稳定参考时间。
/// - `retention`: Minimum file age required before deletion.
/// - `retention`：执行删除前要求的最小文件年龄。
///
/// # Returns
/// # 返回值
/// Returns success after processing the subtree, or the first exact path-scoped filesystem failure.
/// 子树处理完成时返回成功；失败时返回首个带精确路径的文件系统错误。
fn cleanup_directory_recursive(
    directory_path: &Path,
    now: SystemTime,
    retention: Duration,
) -> Result<(), String> {
    // directory_exists preserves path-probing failures before recursive traversal begins.
    // directory_exists 在递归遍历开始前保留路径探测失败。
    let directory_exists = directory_path.try_exists().map_err(|error| {
        format!(
            "探测项目临时子目录失败（{}）：{}。",
            directory_path.display(),
            error
        )
    })?;
    if !directory_exists {
        return Ok(());
    }

    // read_dir owns the fallible iterator for the current directory level.
    // read_dir 持有当前目录层级的可失败迭代器。
    let read_dir = fs::read_dir(directory_path).map_err(|error| {
        format!(
            "读取项目临时目录失败（{}）：{}。",
            directory_path.display(),
            error
        )
    })?;

    for entry in read_dir {
        // entry preserves per-item traversal failures with the parent directory identity.
        // entry 使用父目录身份保留逐项遍历失败。
        let entry = entry.map_err(|error| {
            format!(
                "遍历项目临时目录失败（{}）：{}。",
                directory_path.display(),
                error
            )
        })?;
        // entry_path supplies the exact filesystem identity used by every later diagnostic.
        // entry_path 提供后续所有诊断使用的精确文件系统身份。
        let entry_path = entry.path();
        // metadata classifies the entry without following symbolic links.
        // metadata 在不跟随符号链接的情况下对目录项分类。
        let metadata = entry.metadata().map_err(|error| {
            format!(
                "读取临时路径元数据失败（{}）：{}。",
                entry_path.display(),
                error
            )
        })?;

        if metadata.is_dir() {
            cleanup_directory_recursive(&entry_path, now, retention)?;
            // is_empty is evaluated after recursion so only fully cleaned directories are pruned.
            // is_empty 在递归结束后求值，因此只裁剪已完全清空的目录。
            let is_empty = fs::read_dir(&entry_path)
                .map_err(|error| {
                    format!(
                        "读取子临时目录失败（{}）：{}。",
                        entry_path.display(),
                        error
                    )
                })?
                .next()
                .is_none();
            if is_empty {
                fs::remove_dir(&entry_path).map_err(|error| {
                    format!(
                        "删除空临时目录失败（{}）：{}。",
                        entry_path.display(),
                        error
                    )
                })?;
            }
            continue;
        }

        if !metadata.is_file() {
            continue;
        }

        // modified_at is the sole retention timestamp defined by the host cleanup contract.
        // modified_at 是宿主清理契约定义的唯一保留期时间戳。
        let modified_at = metadata.modified().map_err(|error| {
            format!(
                "读取临时文件修改时间失败（{}）：{}。",
                entry_path.display(),
                error
            )
        })?;
        // Future timestamps can result from clock correction; treating them as age zero avoids deleting fresh data.
        // 未来时间戳可能来自时钟校正；将其视为零年龄可避免删除新数据。
        let age = match now.duration_since(modified_at) {
            Ok(age) => age,
            Err(_) => Duration::ZERO,
        };
        if age > retention {
            fs::remove_file(&entry_path).map_err(|error| {
                format!(
                    "删除过期临时文件失败（{}）：{}。",
                    entry_path.display(),
                    error
                )
            })?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Project temp directories should always live below the hidden `.vulcan/temp` subtree.
    /// 项目临时目录应始终位于隐藏的 `.vulcan/temp` 子树下。
    #[test]
    fn resolve_project_temp_dir_uses_hidden_vulcan_subtree() {
        let root = PathBuf::from(r"D:\projects\demo");
        assert_eq!(
            resolve_project_temp_dir(&root),
            PathBuf::from(r"D:\projects\demo\.vulcan\temp")
        );
    }

    /// Verifies resolving a project temp path performs no eager filesystem mutation.
    /// 验证解析项目临时路径不会提前修改文件系统。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if path resolution creates the project or temp directory.
    /// 此测试无返回值；若路径解析创建项目或临时目录则失败。
    #[test]
    fn resolve_project_temp_dir_does_not_create_directories() {
        // project_root is unique and absent, making any eager directory creation observable.
        // project_root 唯一且不存在，因此任何提前创建目录的行为都可被观察。
        let project_root = std::env::temp_dir().join(format!(
            "vulcan-project-temp-resolution-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("test clock should follow the Unix epoch")
                .as_nanos()
        ));

        // resolved is inspected together with its ancestors to detect eager creation.
        // resolved 与其祖先路径一同用于探测提前创建行为。
        let resolved = resolve_project_temp_dir(&project_root);

        assert_eq!(resolved, project_root.join(".vulcan").join("temp"));
        assert!(!project_root.exists());
        assert!(!resolved.exists());
    }

    /// Builds one absent, process-unique project root below the host temp directory.
    /// 在宿主临时目录下构建一个不存在且进程内唯一的项目根目录。
    ///
    /// # Parameters
    /// # 参数
    /// - `label`: Stable test label included in the generated directory name.
    /// - `label`：包含在生成目录名称中的稳定测试标签。
    ///
    /// # Returns
    /// # 返回值
    /// Returns an absent project path suitable for isolated filesystem tests.
    /// 返回适用于隔离文件系统测试的不存在项目路径。
    fn unique_test_project_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "vulcan-project-temp-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("test clock should follow the Unix epoch")
                .as_nanos()
        ))
    }

    /// Verifies expired files are removed and their empty nested directories are pruned.
    /// 验证过期文件会被删除，清空后的嵌套目录会被裁剪。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if recursive retention cleanup leaves expired content.
    /// 此测试无返回值；若递归保留期清理遗留过期内容则失败。
    #[test]
    fn cleanup_directory_recursive_removes_expired_nested_content() {
        // project_root isolates the expired-content fixture from other filesystem tests.
        // project_root 将过期内容夹具与其他文件系统测试隔离。
        let project_root = unique_test_project_root("expired");
        // temp_dir is the cleanup root exercised by this test.
        // temp_dir 是本测试实际执行清理的根目录。
        let temp_dir = resolve_project_temp_dir(&project_root);
        // nested_dir verifies recursive empty-directory pruning.
        // nested_dir 用于验证递归裁剪空目录。
        let nested_dir = temp_dir.join("nested");
        // expired_file is the retention-expired fixture expected to be deleted.
        // expired_file 是预期被删除的保留期过期夹具。
        let expired_file = nested_dir.join("expired.md");
        fs::create_dir_all(&nested_dir).expect("nested temp directory should be created");
        fs::write(&expired_file, b"expired").expect("expired fixture should be written");
        // cleanup_now advances the stable reference beyond the retention boundary without wall-clock sleeps.
        // cleanup_now 在无需实际等待的情况下将稳定参考时间推进到保留期边界之后。
        let cleanup_now = SystemTime::now() + PROJECT_TEMP_FILE_RETENTION + Duration::from_secs(1);

        cleanup_directory_recursive(&temp_dir, cleanup_now, PROJECT_TEMP_FILE_RETENTION)
            .expect("expired cleanup should succeed");

        assert!(!expired_file.exists());
        assert!(!nested_dir.exists());
        assert!(temp_dir.exists());
        fs::remove_dir_all(&project_root).expect("expired test project should be removed");
    }

    /// Verifies files inside the retention window remain untouched.
    /// 验证仍处于保留期内的文件保持不变。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if fresh content is removed.
    /// 此测试无返回值；若新鲜内容被删除则失败。
    #[test]
    fn cleanup_directory_recursive_keeps_recent_files() {
        // project_root isolates the recent-content fixture from other filesystem tests.
        // project_root 将新鲜内容夹具与其他文件系统测试隔离。
        let project_root = unique_test_project_root("recent");
        // temp_dir is the cleanup root exercised by this test.
        // temp_dir 是本测试实际执行清理的根目录。
        let temp_dir = resolve_project_temp_dir(&project_root);
        // recent_file is the within-retention fixture expected to remain.
        // recent_file 是预期保留的保留期内夹具。
        let recent_file = temp_dir.join("recent.md");
        fs::create_dir_all(&temp_dir).expect("temp directory should be created");
        fs::write(&recent_file, b"recent").expect("recent fixture should be written");

        cleanup_directory_recursive(&temp_dir, SystemTime::now(), PROJECT_TEMP_FILE_RETENTION)
            .expect("recent cleanup should succeed");

        assert!(recent_file.exists());
        fs::remove_dir_all(&project_root).expect("recent test project should be removed");
    }

    /// Verifies files newer than the cleanup reference time are treated as age zero.
    /// 验证晚于清理参考时间的文件会被视为零年龄。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if clock correction causes fresh data deletion.
    /// 此测试无返回值；若时钟校正导致新鲜数据被删除则失败。
    #[test]
    fn cleanup_directory_recursive_keeps_files_newer_than_reference_time() {
        // project_root isolates the future-timestamp fixture from other filesystem tests.
        // project_root 将未来时间戳夹具与其他文件系统测试隔离。
        let project_root = unique_test_project_root("future");
        // temp_dir is the cleanup root exercised by this test.
        // temp_dir 是本测试实际执行清理的根目录。
        let temp_dir = resolve_project_temp_dir(&project_root);
        // future_file is newer than the deliberately ancient cleanup reference time.
        // future_file 晚于刻意设置为远古时间的清理参考时间。
        let future_file = temp_dir.join("future.md");
        fs::create_dir_all(&temp_dir).expect("temp directory should be created");
        fs::write(&future_file, b"future").expect("future fixture should be written");

        cleanup_directory_recursive(
            &temp_dir,
            std::time::UNIX_EPOCH,
            PROJECT_TEMP_FILE_RETENTION,
        )
        .expect("future-timestamp cleanup should succeed");

        assert!(future_file.exists());
        fs::remove_dir_all(&project_root).expect("future test project should be removed");
    }

    /// Verifies canonical containment rejects a cleanup root outside the selected project.
    /// 验证规范路径包含检查会拒绝选中项目之外的清理根。
    ///
    /// # Parameters
    /// 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails if an external canonical directory is accepted for cleanup.
    /// 此测试无返回值；若项目外规范目录被接受执行清理则失败。
    #[test]
    fn canonical_project_temp_cleanup_root_rejects_external_directory() {
        // project_root owns the selected project fixture.
        // project_root 持有选中的项目夹具。
        let project_root = unique_test_project_root("canonical-project");
        // external_root simulates the real target of an intermediate `.vulcan` link.
        // external_root 模拟中间 `.vulcan` 链接指向的真实目标。
        let external_root = unique_test_project_root("canonical-external");
        // external_temp is an existing cleanup candidate outside the project root.
        // external_temp 是项目根目录之外的现有清理候选路径。
        let external_temp = external_root.join("temp");
        fs::create_dir_all(&project_root).expect("project fixture should be created");
        fs::create_dir_all(&external_temp).expect("external temp fixture should be created");

        let error = canonical_project_temp_cleanup_root(&project_root, &external_temp)
            .expect_err("external cleanup root must be rejected");

        assert!(error.contains("项目根目录之外"));
        fs::remove_dir_all(&project_root).expect("project fixture should be removed");
        fs::remove_dir_all(&external_root).expect("external fixture should be removed");
    }

    /// Verifies multi-project maintenance aggregates every failure after deduplicating roots.
    /// 验证多项目维护在去重根目录后聚合全部失败。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if one failure stops later projects or duplicates diagnostics.
    /// 此测试无返回值；若单个失败阻断后续项目或重复诊断则失败。
    #[test]
    fn maintain_project_temp_dirs_aggregates_distinct_project_failures() {
        // first_project is one invalid temp-root owner used to verify aggregation and deduplication.
        // first_project 是用于验证聚合与去重的第一个无效临时根目录所有者。
        let first_project = unique_test_project_root("aggregate-first");
        // second_project proves cleanup continues after the first project fails.
        // second_project 证明首个项目失败后清理仍会继续。
        let second_project = unique_test_project_root("aggregate-second");
        // first_temp_path is replaced with a regular file to force a deterministic read failure.
        // first_temp_path 会被普通文件占据，以确定性触发读取失败。
        let first_temp_path = resolve_project_temp_dir(&first_project);
        // second_temp_path independently forces the later-project failure path.
        // second_temp_path 独立触发后续项目的失败路径。
        let second_temp_path = resolve_project_temp_dir(&second_project);
        fs::create_dir_all(
            first_temp_path
                .parent()
                .expect("first temp path should have a parent"),
        )
        .expect("first metadata directory should be created");
        fs::create_dir_all(
            second_temp_path
                .parent()
                .expect("second temp path should have a parent"),
        )
        .expect("second metadata directory should be created");
        // A regular file at the temp-directory path deterministically makes read_dir fail on every platform.
        // 在临时目录路径放置普通文件，可在所有平台上确定性地使 read_dir 失败。
        fs::write(&first_temp_path, b"not a directory")
            .expect("first invalid temp path should be written");
        fs::write(&second_temp_path, b"not a directory")
            .expect("second invalid temp path should be written");

        // error must contain one project-level entry per distinct root despite duplicate input.
        // error 必须为每个不同根目录保留一条项目级条目，不受重复输入影响。
        let error = maintain_project_temp_dirs(&[
            first_project.clone(),
            first_project.clone(),
            second_project.clone(),
        ])
        .expect_err("both invalid project temp paths should be reported");

        assert!(error.contains("2 个项目"));
        assert_eq!(
            error
                .lines()
                .filter(|line| line.starts_with(&format!("- {}：", first_project.display())))
                .count(),
            1
        );
        assert_eq!(
            error
                .lines()
                .filter(|line| line.starts_with(&format!("- {}：", second_project.display())))
                .count(),
            1
        );
        fs::remove_dir_all(&first_project).expect("first aggregate project should be removed");
        fs::remove_dir_all(&second_project).expect("second aggregate project should be removed");
    }
}
