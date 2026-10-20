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
        commit_files(vec![PreparedFile {
            path,
            original,
            text: text.to_string(),
        }])
    }
}

/// Acquires the process-wide write lock, recovering from a poisoned holder.
/// 获取进程级写锁；持有者崩溃导致中毒时恢复。
pub(crate) fn write_guard() -> std::sync::MutexGuard<'static, ()> {
    WRITE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Writes text through a sibling temporary file and an atomic rename.
/// 通过同目录临时文件与原子重命名写入文本。
pub(crate) fn write_atomically(path: &Path, text: &str) -> Result<(), MemoryError> {
    let temporary = write_temporary(path, text)?;
    std::fs::rename(&temporary, path).map_err(|error| {
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

/// Commits several prepared files: every file is checked against its original and staged before any is replaced.
/// 提交多个已准备文件：所有文件先核对原文并暂存，再逐一替换。
pub(crate) fn commit_files(files: Vec<PreparedFile>) -> Result<(), MemoryError> {
    let mut staged = Vec::with_capacity(files.len());
    let cleanup = |staged: &[(PathBuf, PathBuf)]| {
        for (temporary, _) in staged {
            let _ = std::fs::remove_file(temporary);
        }
    };
    for file in &files {
        let current = match MarkdownMemoryStore::read_optional(&file.path) {
            Ok(current) => current,
            Err(error) => {
                cleanup(&staged);
                return Err(error);
            }
        };
        if current != file.original {
            cleanup(&staged);
            return Err(MemoryError::ConcurrentModification(
                file.path.display().to_string(),
            ));
        }
        match write_temporary(&file.path, &file.text) {
            Ok(temporary) => staged.push((temporary, file.path.clone())),
            Err(error) => {
                cleanup(&staged);
                return Err(error);
            }
        }
    }
    for (index, (temporary, path)) in staged.iter().enumerate() {
        if let Err(error) = std::fs::rename(temporary, path) {
            cleanup(&staged[index..]);
            return Err(MemoryError::io(path, "replaced", error));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A commit refuses files changed on disk after they were read and leaves no temporary files.
    /// 提交会拒绝读取后在磁盘上被修改的文件，且不遗留临时文件。
    #[test]
    fn commit_checks_originals_and_cleans_up() {
        let root = tempfile::tempdir().expect("temp dir");
        let first = root.path().join("a.md");
        let second = root.path().join("b.md");
        std::fs::write(&second, "external").expect("seed");
        let result = commit_files(vec![
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
        ]);
        assert!(matches!(
            result,
            Err(MemoryError::ConcurrentModification(_))
        ));
        assert!(!first.exists());
        assert_eq!(std::fs::read_to_string(&second).expect("kept"), "external");
        assert_eq!(std::fs::read_dir(root.path()).expect("dir").count(), 1);

        commit_files(vec![PreparedFile {
            path: first.clone(),
            original: None,
            text: String::from("new"),
        }])
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
