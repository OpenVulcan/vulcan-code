//! File layout, project resolution and atomic commits of the memory store.
//! 记忆存储的文件布局、项目解析与原子提交。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::{
    MARKDOWN_DIRECTORY, MEMORY_DIRECTORY, MarkdownMemorySettings, MemoryDocument, MemoryError,
    MemoryProject, MemoryRegistry, MemoryScope,
};

/// Serializes every memory write in this process; reads take no lock.
/// 串行化本进程内的全部记忆写入；读取不加锁。
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Proof that this process's memory write lock remains held throughout preparation and commit.
/// 在准备与提交全程持有本进程记忆写锁的凭证。
pub(crate) struct MemoryWriteGuard {
    /// Private guard prevents callers from substituting an unrelated mutex.
    /// 私有守卫防止调用方以无关互斥锁替代。
    _guard: std::sync::MutexGuard<'static, ()>,
}

/// Attempts allowed for Windows sharing violations during replacement or rollback.
/// Windows 替换或回滚发生共享冲突时允许的尝试次数。
const WINDOWS_MUTATION_ATTEMPTS: usize = 5;
/// Delay between Windows sharing-violation attempts; never applied to other errors or platforms.
/// Windows 共享冲突重试间隔；不用于其他错误或平台。
const WINDOWS_MUTATION_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(25);

/// The project a request runs in, as known to the host.
/// 宿主已知的请求所属项目。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectContext {
    /// Host project id derived from the project root.
    /// 由项目根目录派生的宿主项目标识。
    pub host_project_id: Option<String>,
    /// Project root path.
    /// 项目根路径。
    pub root_path: Option<String>,
    /// Explicit memory project chosen in the memory page, overriding the host binding.
    /// 在记忆页中显式选择的记忆项目，覆盖宿主绑定。
    pub memory_project_id: Option<String>,
}

impl ProjectContext {
    /// Returns the memory project this context addresses: the explicit choice or the host binding.
    /// 返回该上下文指向的记忆项目：显式选择或宿主绑定。
    pub fn bound_project<'registry>(
        &self,
        registry: &'registry MemoryRegistry,
    ) -> Result<Option<&'registry MemoryProject>, MemoryError> {
        if let Some(id) = &self.memory_project_id {
            return registry.project(id).map(Some).ok_or_else(|| {
                MemoryError::ProjectUnavailable(format!("Memory project `{id}` does not exist."))
            });
        }
        Ok(self
            .host_project_id
            .as_deref()
            .and_then(|host| registry.project_for_host(host)))
    }
}

/// The documents of every scope visible to one request.
/// 单个请求可见的各层文档。
#[derive(Debug, Clone)]
pub struct ScopeDocuments {
    /// User scope document.
    /// 用户层文档。
    pub user: MemoryDocument,
    /// Space scope document.
    /// 空间层文档。
    pub space: MemoryDocument,
    /// Bound memory project and its document, when the host project is bound.
    /// 宿主项目已绑定时的记忆项目及其文档。
    pub project: Option<(MemoryProject, MemoryDocument)>,
}

impl ScopeDocuments {
    /// Returns the document of a scope, when it is available.
    /// 返回某层的文档（可用时）。
    pub fn document(&self, scope: MemoryScope) -> Option<&MemoryDocument> {
        match scope {
            MemoryScope::User => Some(&self.user),
            MemoryScope::Space => Some(&self.space),
            MemoryScope::Project => self.project.as_ref().map(|(_, document)| document),
        }
    }
}

/// Handle to the memory files under one user Vulcan root.
/// 指向某个用户 Vulcan 根目录下记忆文件的句柄。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownMemoryStore {
    /// `<user vulcan root>/memory`.
    /// `<用户 Vulcan 根目录>/memory`。
    memory_directory: PathBuf,
}

/// One file change prepared for an atomic commit.
/// 为原子提交准备的单个文件变更。
pub(crate) struct PreparedFile {
    /// Target path.
    /// 目标路径。
    pub path: PathBuf,
    /// Exact text read before the change; `None` when the file did not exist.
    /// 变更前读取的精确文本；文件不存在时为 `None`。
    pub original: Option<String>,
    /// New text to write.
    /// 要写入的新文本。
    pub text: String,
}

impl MarkdownMemoryStore {
    /// Creates a handle for a user Vulcan root; nothing is created on disk.
    /// 为用户 Vulcan 根目录创建句柄；不在磁盘上创建任何内容。
    pub fn new(user_vulcan_root: &Path) -> Self {
        Self {
            memory_directory: user_vulcan_root.join(MEMORY_DIRECTORY),
        }
    }

    /// Returns `<user vulcan root>/memory`, which also holds `settings.json`.
    /// 返回 `<用户 Vulcan 根目录>/memory`，其中也存放 `settings.json`。
    pub fn memory_directory(&self) -> &Path {
        &self.memory_directory
    }

    /// Returns the directory holding the Markdown files and the registry.
    /// 返回存放 Markdown 文件与注册表的目录。
    pub fn markdown_directory(&self) -> PathBuf {
        self.memory_directory.join(MARKDOWN_DIRECTORY)
    }

    /// Returns the file path of a scope; the project scope needs a memory project id.
    /// 返回某层的文件路径；项目层需要记忆项目标识。
    pub fn scope_path(
        &self,
        scope: MemoryScope,
        memory_project_id: Option<&str>,
    ) -> Result<PathBuf, MemoryError> {
        let directory = self.markdown_directory();
        Ok(match scope {
            MemoryScope::User => directory.join("user.md"),
            MemoryScope::Space => directory.join("space.md"),
            MemoryScope::Project => {
                let id = memory_project_id.ok_or_else(|| {
                    MemoryError::ProjectUnavailable(String::from(
                        "This conversation has no project, so project memory is unavailable.",
                    ))
                })?;
                if id.is_empty()
                    || !id
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric() || character == '-')
                {
                    return Err(MemoryError::invalid(format!(
                        "`{id}` is not a memory project id."
                    )));
                }
                directory.join("projects").join(format!("{id}.md"))
            }
        })
    }

    /// Reads settings; a missing file yields in-memory defaults.
    /// 读取设置；文件不存在时返回内存默认值。
    pub fn settings(&self) -> Result<MarkdownMemorySettings, MemoryError> {
        MarkdownMemorySettings::load(&self.memory_directory)
    }

    /// Saves settings under the write lock.
    /// 在写锁下保存设置。
    pub fn save_settings(&self, settings: &MarkdownMemorySettings) -> Result<(), MemoryError> {
        let _guard = write_guard();
        settings.save(&self.memory_directory)
    }

    /// Reads the registry; a missing file is empty.
    /// 读取注册表；文件不存在时为空。
    pub fn registry(&self) -> Result<MemoryRegistry, MemoryError> {
        MemoryRegistry::load(&self.markdown_directory())
    }

    /// Changes the registry under the write lock and saves it when the change succeeds.
    /// 在写锁下修改注册表，修改成功后保存。
    pub fn update_registry<T>(
        &self,
        change: impl FnOnce(&mut MemoryRegistry) -> Result<T, MemoryError>,
    ) -> Result<T, MemoryError> {
        let _guard = write_guard();
        let directory = self.markdown_directory();
        let mut registry = MemoryRegistry::load(&directory)?;
        let value = change(&mut registry)?;
        registry.save(&directory)?;
        Ok(value)
    }

    /// Reads a file's exact text, or `None` when it does not exist.
    /// 读取文件的精确文本；文件不存在时返回 `None`。
    pub(crate) fn read_optional(path: &Path) -> Result<Option<String>, MemoryError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(Some(text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(MemoryError::io(path, "read", error)),
        }
    }

    /// Reads one scope document; a missing file is an empty document.
    /// 读取单层文档；文件不存在时为空文档。
    pub fn load(
        &self,
        scope: MemoryScope,
        memory_project_id: Option<&str>,
    ) -> Result<MemoryDocument, MemoryError> {
        let path = self.scope_path(scope, memory_project_id)?;
        Ok(Self::read_optional(&path)?
            .map(|text| MemoryDocument::parse(&text))
            .unwrap_or_else(MemoryDocument::empty))
    }

    /// Reads every scope visible to a project context.
    /// 读取项目上下文可见的全部层。
    pub fn load_scopes(&self, context: &ProjectContext) -> Result<ScopeDocuments, MemoryError> {
        let registry = self.registry()?;
        let project = context.bound_project(&registry)?.cloned();
        let project = match project {
            Some(project) => {
                let document = self.load(MemoryScope::Project, Some(&project.id))?;
                Some((project, document))
            }
            None => None,
        };
        Ok(ScopeDocuments {
            user: self.load(MemoryScope::User, None)?,
            space: self.load(MemoryScope::Space, None)?,
            project,
        })
    }

    /// Returns the exact text of one scope file for whole-file editing.
    /// 返回单层文件的精确文本，用于整文件编辑。
    pub fn read_file_text(
        &self,
        scope: MemoryScope,
        memory_project_id: Option<&str>,
    ) -> Result<String, MemoryError> {
        let path = self.scope_path(scope, memory_project_id)?;
        Ok(Self::read_optional(&path)?.unwrap_or_default())
    }

    /// Replaces one whole scope file when it still holds the text the editor started from.
    /// 当单层文件仍是编辑器起始文本时整体替换它。
    pub fn replace_file_text(
        &self,
        scope: MemoryScope,
        memory_project_id: Option<&str>,
        expected: &str,
        text: &str,
    ) -> Result<(), MemoryError> {
        // Hold the same guard as node writes before reading the editor's expected revision.
        // 在读取编辑器期望版本之前持有与节点写入相同的守卫。
        let guard = write_guard();
        if text.len() > crate::MAX_FILE_BYTES {
            return Err(MemoryError::invalid(format!(
                "A memory file can hold at most {} bytes.",
                crate::MAX_FILE_BYTES
            )));
        }
        let path = self.scope_path(scope, memory_project_id)?;
        let original = Self::read_optional(&path)?;
        if original.as_deref().unwrap_or_default() != expected {
            return Err(MemoryError::ConcurrentModification(
                path.display().to_string(),
            ));
        }
        commit_files(
            &guard,
            vec![PreparedFile {
                path,
                original,
                text: text.to_string(),
            }],
        )
    }
}

/// Acquires the process-wide write lock, recovering from a poisoned holder.
/// 获取进程级写锁；持有者崩溃导致中毒时恢复。
pub(crate) fn write_guard() -> MemoryWriteGuard {
    MemoryWriteGuard {
        _guard: WRITE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    }
}

/// Writes text through a sibling temporary file and an atomic rename.
/// 通过同目录临时文件与原子重命名写入文本。
pub(crate) fn write_atomically(path: &Path, text: &str) -> Result<(), MemoryError> {
    let temporary = write_temporary(path, text)?;
    replace_file(&temporary, path).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        MemoryError::io(path, "replaced", error)
    })
}

/// Writes a sibling temporary file and returns its path.
/// 写入同目录临时文件并返回其路径。
fn write_temporary(path: &Path, text: &str) -> Result<PathBuf, MemoryError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| MemoryError::io(parent, "created", error))?;
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = parent.join(format!(
        ".{file_name}.{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::write(&temporary, text).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        MemoryError::io(&temporary, "written", error)
    })?;
    Ok(temporary)
}

/// Retries `operation` only for transient Windows replacement failures and returns its final result.
/// 仅对 Windows 临时替换失败重试 `operation`，并返回最终结果。
fn retry_file_mutation(mut operation: impl FnMut() -> std::io::Result<()>) -> std::io::Result<()> {
    // Other platforms retain their native single-attempt semantics.
    // 其他平台保持原生单次尝试语义。
    let attempts = if cfg!(windows) {
        WINDOWS_MUTATION_ATTEMPTS
    } else {
        1
    };
    for attempt in 0..attempts {
        match operation() {
            Err(error)
                if cfg!(windows)
                    && matches!(error.raw_os_error(), Some(5 | 32 | 33))
                    && attempt + 1 < attempts =>
            {
                std::thread::sleep(WINDOWS_MUTATION_RETRY_DELAY)
            }
            result => return result,
        }
    }
    unreachable!("Every mutation attempt returns or retries within the fixed bound")
}

/// Atomically replaces `destination` with `temporary`, retrying transient Windows sharing failures.
/// 以 `temporary` 原子替换 `destination`，并重试 Windows 临时共享冲突。
fn replace_file(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    retry_file_mutation(|| std::fs::rename(temporary, destination))
}

/// Temporary forward and rollback bytes; originals are detached only when manual recovery is required.
/// 临时正向与回滚字节；仅在需要人工恢复时移交原文副本。
struct StagedFile {
    /// Prepared new bytes beside the destination.
    /// 位于目标文件旁的待写新字节。
    replacement: PathBuf,
    /// Prepared original bytes, absent when the destination did not exist.
    /// 待恢复的原文字节；目标原先不存在时缺省。
    original: Option<PathBuf>,
}

impl Drop for StagedFile {
    /// Removes unused temporary files while leaving explicitly detached recovery copies intact.
    /// 删除未使用的临时文件，同时保留已明确移交的恢复副本。
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.replacement);
        if let Some(original) = &self.original {
            let _ = std::fs::remove_file(original);
        }
    }
}

/// Commits `files` under `guard`; failures restore completed replacements or report preserved recovery copies.
/// 在 `guard` 下提交 `files`；失败时还原已完成替换，或报告保留的恢复副本。
pub(crate) fn commit_files(
    guard: &MemoryWriteGuard,
    files: Vec<PreparedFile>,
) -> Result<(), MemoryError> {
    commit_files_with(guard, files, replace_file)
}

/// Runs the real commit/rollback algorithm with a replacement operation that tests can fault deterministically.
/// 使用测试可确定性注入故障的替换操作运行真实提交与回滚算法。
/// `guard` proves writer exclusion; `files` supplies exact originals; `replace` returns the filesystem outcome.
/// `guard` 证明写入排他性；`files` 提供精确原文；`replace` 返回文件系统操作结果。
fn commit_files_with(
    _guard: &MemoryWriteGuard,
    files: Vec<PreparedFile>,
    mut replace: impl FnMut(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), MemoryError> {
    // Stage both directions before publishing anything, so rollback never needs to recreate original bytes.
    // 发布前暂存两个方向的字节，确保回滚无需重新生成原文。
    let mut staged = Vec::with_capacity(files.len());
    for file in &files {
        if MarkdownMemoryStore::read_optional(&file.path)? != file.original {
            return Err(MemoryError::ConcurrentModification(
                file.path.display().to_string(),
            ));
        }
        // RAII also cleans forward bytes if staging the original fails.
        // 原文暂存失败时，RAII 同样清理正向字节。
        let mut prepared = StagedFile {
            replacement: write_temporary(&file.path, &file.text)?,
            original: None,
        };
        prepared.original = file
            .original
            .as_deref()
            .map(|text| write_temporary(&file.path, text))
            .transpose()?;
        staged.push(prepared);
    }
    for (index, file) in files.iter().enumerate() {
        // Recheck immediately before replacement to detect external edits during staging.
        // 替换前立即重新检查，以发现暂存期间的外部编辑。
        let result = match MarkdownMemoryStore::read_optional(&file.path) {
            Ok(current) if current == file.original => {
                replace(&staged[index].replacement, &file.path)
                    .map_err(|error| MemoryError::io(&file.path, "replaced", error))
            }
            Ok(_) => Err(MemoryError::ConcurrentModification(
                file.path.display().to_string(),
            )),
            Err(error) => Err(error),
        };
        if let Err(source) = result {
            // Track exact destinations and detached originals for an actionable recovery error.
            // 跟踪精确目标与移交的原文，以提供可执行的恢复错误。
            let mut inconsistent_files = Vec::new();
            let mut recovery_files = Vec::new();
            for completed in (0..index).rev() {
                // Never overwrite an unrelated external edit while undoing this transaction.
                // 撤销此事务时绝不覆盖无关的外部编辑。
                let previous = &files[completed];
                let restored = match MarkdownMemoryStore::read_optional(&previous.path) {
                    Ok(current) if current == previous.original => true,
                    Ok(current) if current.as_deref() == Some(previous.text.as_str()) => {
                        match &staged[completed].original {
                            Some(original) => replace(original, &previous.path).is_ok(),
                            None => {
                                retry_file_mutation(|| std::fs::remove_file(&previous.path)).is_ok()
                            }
                        }
                    }
                    _ => false,
                };
                if !restored {
                    inconsistent_files.push(previous.path.clone());
                    if let Some(original) = staged[completed].original.take() {
                        recovery_files.push(original);
                    }
                }
            }
            return if inconsistent_files.is_empty() {
                Err(source)
            } else {
                Err(MemoryError::RollbackIncomplete {
                    source: Box::new(source),
                    inconsistent_files,
                    recovery_files,
                })
            };
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "store_commit_tests.rs"]
mod commit_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// A commit refuses files changed on disk after they were read and leaves no temporary files.
    /// 提交会拒绝读取后在磁盘上被修改的文件，且不遗留临时文件。
    #[test]
    fn commit_checks_originals_and_cleans_up() {
        // Hold the same write capability required by production commits.
        // 持有生产提交要求的同一写入凭证。
        let guard = write_guard();
        let root = tempfile::tempdir().expect("temp dir");
        let first = root.path().join("a.md");
        let second = root.path().join("b.md");
        std::fs::write(&second, "external").expect("seed");
        let result = commit_files(
            &guard,
            vec![
                PreparedFile {
                    path: first.clone(),
                    original: None,
                    text: String::from("new"),
                },
                PreparedFile {
                    path: second.clone(),
                    original: Some(String::from("read earlier")),
                    text: String::from("mine"),
                },
            ],
        );
        assert!(matches!(
            result,
            Err(MemoryError::ConcurrentModification(_))
        ));
        assert!(!first.exists());
        assert_eq!(std::fs::read_to_string(&second).expect("kept"), "external");
        assert_eq!(std::fs::read_dir(root.path()).expect("dir").count(), 1);

        commit_files(
            &guard,
            vec![PreparedFile {
                path: first.clone(),
                original: None,
                text: String::from("new"),
            }],
        )
        .expect("commit");
        assert_eq!(std::fs::read_to_string(&first).expect("written"), "new");
    }

    /// Project file names accept only memory project ids.
    /// 项目文件名只接受记忆项目标识。
    #[test]
    fn project_paths_reject_unsafe_ids() {
        let store = MarkdownMemoryStore::new(Path::new("root"));
        assert!(
            store
                .scope_path(MemoryScope::Project, Some("p-1a2b"))
                .is_ok()
        );
        assert!(
            store
                .scope_path(MemoryScope::Project, Some("../x"))
                .is_err()
        );
        assert!(matches!(
            store.scope_path(MemoryScope::Project, None),
            Err(MemoryError::ProjectUnavailable(_))
        ));
    }
}
