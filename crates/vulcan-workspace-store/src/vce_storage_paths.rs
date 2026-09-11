//! Host-resolved, project-isolated paths for code-semantic storage.
//! 由宿主解析且按项目隔离的代码语义存储路径。

use sha2::{Digest, Sha256};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

#[path = "vce_writer_lease.rs"]
mod writer_lease;
pub use writer_lease::VceProjectWriterLease;

/// Validated VCE root derived only from the host runtime's resolved application data root.
/// 仅从宿主运行时已解析应用数据根派生的已验证 VCE 根目录。
#[derive(Clone, Debug)]
pub(crate) struct VceStoragePaths {
    /// Canonical host application root; this module never changes sibling memory storage.
    /// 规范宿主应用根；本模块绝不修改同级记忆存储。
    application_root: PathBuf,
    /// Dedicated semantic storage root under the host application root.
    /// 宿主应用根下专用的语义存储根。
    root: PathBuf,
}

/// Exact physical paths for one stable project identity.
/// 一个稳定项目身份对应的精确物理路径。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VceProjectStoragePaths {
    /// Original stable identity retained for directory-catalog identity checks.
    /// 为目录记录身份核验保留的原始稳定身份。
    pub(crate) project_id: String,
    /// Complete lowercase SHA-256 directory key; never a display name or source path.
    /// 完整小写 SHA-256 目录键；绝不使用展示名或源码路径。
    pub(crate) project_key: String,
    /// Validated project-owned directory under the VCE root.
    /// VCE 根下已验证的项目自有目录。
    pub(crate) root: PathBuf,
    /// Project-local SQLite metadata database path.
    /// 项目本地 SQLite 元数据库路径。
    pub(crate) sqlite_path: PathBuf,
    /// Project-local LanceDB vector directory.
    /// 项目本地 LanceDB 向量目录。
    pub(crate) lancedb_path: PathBuf,
}

impl VceStoragePaths {
    /// Resolve the dedicated root without creating files or choosing environment-variable fallbacks.
    /// 解析专用根目录，不创建文件，也不选择环境变量兜底路径。
    ///
    /// `application_root` is the existing absolute host data root. Returns validated
    /// path ownership or an explicit error for a missing root, link, or conflicting file type.
    /// `application_root` 是宿主现有的绝对数据根。返回已验证路径归属；
    /// 根缺失、链接或文件类型冲突时返回明确错误。
    pub(crate) fn resolve(application_root: &Path) -> Result<Self, String> {
        if !application_root.is_absolute() {
            return Err(String::from("VCE application data root must be absolute"));
        }
        // canonical_root anchors every child path to the host's existing resolved data directory.
        // canonical_root 将全部子路径锚定到宿主现有的已解析数据目录。
        let canonical_root = std::fs::canonicalize(application_root)
            .map_err(|error| format!("Resolve VCE application data root: {error}"))?;
        if !canonical_root.is_dir() {
            return Err(String::from(
                "VCE application data root must be a directory",
            ));
        }
        // paths is in-memory metadata only; resolving it cannot create an index.
        // paths 仅为内存元数据；解析它不能创建索引。
        let paths = Self {
            root: canonical_root.join("vce"),
            application_root: canonical_root,
        };
        paths.verify_directory(&paths.root, false)?;
        Ok(paths)
    }

    /// Return the dedicated semantic root without touching the filesystem.
    /// 返回专用语义根目录，不触碰文件系统。
    ///
    /// Takes no extra parameters and returns the borrowed authoritative root path.
    /// 无额外参数，返回借用的权威根路径。
    ///
    /// Test-only: production code names the exact subdirectory it needs, so this accessor exists to let
    /// tests assert on the root without re-deriving it.
    /// 仅测试使用：生产代码会指名它需要的精确子目录，故此访问器只为让测试无需重新派生即可断言根目录。
    #[cfg(test)]
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Prepare only the lightweight catalog root, not any project or vector database directory.
    /// 仅准备轻量目录根，不准备任何项目目录或向量数据库目录。
    ///
    /// Takes no extra parameters and returns the verified catalog SQLite file path.
    /// 无额外参数，返回经过验证的目录 SQLite 文件路径。
    pub(crate) fn prepare_catalog(&self) -> Result<PathBuf, String> {
        self.verify_directory(&self.root, true)?;
        // catalog is host metadata, not a code-vector database.
        // catalog 是宿主元数据，不是代码向量数据库。
        let catalog = self.root.join("catalog.sqlite3");
        verify_regular_file_or_absent(&catalog)?;
        Ok(catalog)
    }

    /// Resolve one project without creating its directory or inferring index initialization.
    /// 解析单个项目，不创建其目录，也不推断索引已初始化。
    ///
    /// `project_id` is the exact stable host identity. Returns physically isolated paths after
    /// checking every existing component for symlinks, reparse points and type conflicts.
    /// `project_id` 是宿主的精确稳定身份。检查全部现有组件的符号链接、重解析点与类型冲突后，
    /// 返回物理隔离路径。
    pub(crate) fn project(&self, project_id: &str) -> Result<VceProjectStoragePaths, String> {
        if project_id.trim().is_empty() {
            return Err(String::from("VCE project identity must not be empty"));
        }
        self.verify_directory(&self.root, false)?;
        // project_key encodes the whole identity so path-like input cannot escape the owned root.
        // project_key 编码完整身份，使类似路径的输入无法逃逸自有根目录。
        let project_key = hex::encode(Sha256::digest(project_id.as_bytes()));
        // projects_root is the sole host-defined parent of every per-project store.
        // projects_root 是全部项目存储唯一的宿主定义父目录。
        let projects_root = self.root.join("projects");
        self.verify_directory(&projects_root, false)?;
        // root cannot contain user-controlled path components because the key is hexadecimal.
        // root 的键为十六进制，因此不含用户可控路径组件。
        let root = projects_root.join(&project_key);
        self.verify_directory(&root, false)?;
        self.verify_directory(&root.join("sqlite"), false)?;
        self.verify_directory(&root.join("lancedb"), false)?;
        verify_regular_file_or_absent(&root.join("sqlite").join("vce.sqlite3"))?;
        Ok(VceProjectStoragePaths {
            project_id: project_id.to_owned(),
            project_key,
            sqlite_path: root.join("sqlite").join("vce.sqlite3"),
            lancedb_path: root.join("lancedb"),
            root,
        })
    }

    /// Prepare one project directory only after the host admits an explicit creation job.
    /// 仅在宿主准入明确创建作业后准备一个项目目录。
    ///
    /// `project_id` selects the exact authorized project. Returns verified paths, never deletes
    /// existing files, and never initializes SQLite or LanceDB itself.
    /// `project_id` 选择精确获授权项目。返回已验证路径，绝不删除现有文件，
    /// 也不自行初始化 SQLite 或 LanceDB。
    pub(crate) fn prepare_project(
        &self,
        project_id: &str,
    ) -> Result<VceProjectStoragePaths, String> {
        // project performs all read-only ownership checks before the first directory mutation.
        // project 在第一次目录修改前完成全部只读归属检查。
        let project = self.project(project_id)?;
        self.verify_directory(&self.root, true)?;
        self.verify_directory(&self.root.join("projects"), true)?;
        self.verify_directory(&project.root, true)?;
        self.verify_directory(&project.root.join("sqlite"), true)?;
        self.verify_directory(&project.lancedb_path, true)?;
        self.project(project_id)
    }

    /// Admit one nonblocking writer without opening SQLite, LanceDB, or source files.
    /// 准入一个非阻塞写入者，不打开 SQLite、LanceDB 或源码文件。
    /// `project_id` is stable ownership; `create` permits only explicit first-creation directories.
    /// `project_id` 为稳定归属；`create` 仅允许明确首次创建时准备目录。
    /// Returns an owned lease, no lease when busy, or an explicit path/operating-system error.
    /// 返回自有租约、忙时的空租约，或明确路径与操作系统错误。
    pub(crate) fn try_acquire_writer(
        &self,
        project_id: &str,
        create: bool,
    ) -> Result<Option<VceProjectWriterLease>, String> {
        // project validates all existing components before any first-creation mutation.
        // project 在任何首次创建修改之前校验全部现有组件。
        let project = self.project(project_id)?;
        if create {
            self.verify_directory(&self.root, true)?;
            self.verify_directory(&self.root.join("projects"), true)?;
            self.verify_directory(&project.root, true)?;
        } else if !project.root.is_dir() {
            return Err(String::from(
                "VCE writer requires an existing project storage directory",
            ));
        }
        // lock_path is permanent metadata: unlinking it would split cooperative lock ownership.
        // lock_path 为永久元数据：删除它会分裂协作锁的归属。
        let lock_path = project.root.join("writer.lock");
        verify_regular_file_or_absent(&lock_path)?;
        VceProjectWriterLease::try_acquire(&lock_path)
    }

    /// Validate one derived directory, optionally creating only that single checked component.
    /// 校验一个派生目录，可选地仅创建该已检查组件。
    ///
    /// `directory` must stay under the anchored VCE root; `create` is explicit host authority.
    /// Returns success for an admissible path or an explicit filesystem/ownership error.
    /// `directory` 必须位于锚定的 VCE 根下；`create` 表示明确宿主授权。
    /// 路径可准入时返回成功，否则返回明确文件系统或归属错误。
    fn verify_directory(&self, directory: &Path, create: bool) -> Result<(), String> {
        if !directory.starts_with(&self.root) || directory == self.application_root {
            return Err(String::from(
                "VCE storage directory escapes the host-owned root",
            ));
        }
        match std::fs::symlink_metadata(directory) {
            Ok(metadata) => verify_directory_metadata(directory, &metadata),
            Err(error) if error.kind() == ErrorKind::NotFound && !create => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                match std::fs::create_dir(directory) {
                    Ok(()) => {}
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                    Err(error) => {
                        return Err(format!(
                            "Create VCE directory `{}`: {error}",
                            directory.display()
                        ));
                    }
                }
                // A concurrent creator must satisfy the same type/link checks as a pre-existing path.
                // 并发创建者必须满足与现有路径相同的类型及链接检查。
                let metadata = std::fs::symlink_metadata(directory)
                    .map_err(|error| format!("Verify created VCE directory: {error}"))?;
                verify_directory_metadata(directory, &metadata)
            }
            Err(error) => Err(format!(
                "Inspect VCE directory `{}`: {error}",
                directory.display()
            )),
        }
    }
}

/// Reject links and non-directory nodes before any storage binding uses a directory.
/// 在存储绑定使用目录前拒绝链接与非目录节点。
///
/// `path` identifies the checked component and `metadata` comes from symlink_metadata.
/// Returns success only for a normal directory.
/// `path` 标识检查组件，`metadata` 来自 symlink_metadata。仅正常目录返回成功。
fn verify_directory_metadata(path: &Path, metadata: &std::fs::Metadata) -> Result<(), String> {
    if metadata_is_link(metadata) || !metadata.is_dir() {
        return Err(format!(
            "VCE storage path must be a real directory, not a link: `{}`",
            path.display()
        ));
    }
    Ok(())
}

/// Check a database target is either absent or a regular non-link file.
/// 检查数据库目标不存在，或是正常的非链接文件。
///
/// `path` is a host-derived database filename. Returns an explicit error for links or directories.
/// `path` 是宿主派生的数据库文件名。链接或目录返回明确错误。
fn verify_regular_file_or_absent(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata_is_link(&metadata) && metadata.is_file() => Ok(()),
        Ok(_) => Err(format!(
            "VCE database target must be a regular file: `{}`",
            path.display()
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Inspect VCE database target `{}`: {error}",
            path.display()
        )),
    }
}

/// Detect symlinks and Windows reparse points without following them.
/// 不跟随链接地检测符号链接与 Windows 重解析点。
///
/// `metadata` comes from symlink_metadata. Returns true for any redirected filesystem node.
/// `metadata` 来自 symlink_metadata。文件系统节点存在重定向时返回真。
fn metadata_is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify catalog preparation and arbitrary project reads never create project data directories.
    /// 验证目录准备和任意项目读取绝不创建项目数据目录。
    ///
    /// Takes no parameters and returns nothing; all filesystem work uses an isolated temporary root.
    /// 无参数、无返回值；全部文件系统工作使用隔离临时根。
    #[test]
    fn project_storage_reads_are_lazy_and_physically_isolated() {
        // temporary is unrelated to the user's real data root.
        // temporary 与用户真实数据根无关。
        let temporary = tempfile::tempdir().expect("temporary root");
        // paths derives only the host-provided application root.
        // paths 仅派生宿主提供的应用根。
        let paths = VceStoragePaths::resolve(temporary.path()).expect("resolve");
        assert!(!paths.root().exists());
        for number in 0..100 {
            assert!(
                !paths
                    .project(&format!("project-{number}"))
                    .expect("cold project")
                    .root
                    .exists()
            );
        }
        assert!(!paths.root().exists());
        assert_eq!(
            paths.prepare_catalog().expect("catalog"),
            paths.root().join("catalog.sqlite3")
        );
        assert!(!paths.root().join("projects").exists());
        // first and second remain independent even if display names are identical elsewhere.
        // first 与 second 即使在其他位置展示名相同，也保持独立。
        let first = paths.prepare_project("project-a").expect("prepare first");
        let second = paths.project("project-b").expect("resolve second");
        assert!(first.root.is_dir());
        assert!(first.lancedb_path.is_dir());
        assert!(!first.sqlite_path.exists());
        assert!(!second.root.exists());
        assert_ne!(first.root, second.root);
        assert_eq!(paths.project("project-a").expect("resolve again"), first);
        assert!(!temporary.path().join("vmm").exists());
    }

    /// Verify complete stable IDs become safe fixed-length keys rather than filesystem components.
    /// 验证完整稳定 ID 变成安全定长键，而不是文件系统路径组件。
    ///
    /// Takes no parameters and returns nothing; tests traversal-like and Unicode identities.
    /// 无参数、无返回值；测试类似目录穿越的身份与 Unicode 身份。
    #[test]
    fn project_storage_keys_preserve_identity_without_path_injection() {
        // temporary supplies the sole authorized filesystem scope.
        // temporary 提供唯一获授权文件系统范围。
        let temporary = tempfile::tempdir().expect("temporary root");
        // paths never receives a model-supplied output directory.
        // paths 绝不接收模型提供的输出目录。
        let paths = VceStoragePaths::resolve(temporary.path()).expect("resolve");
        for identity in ["../vmm", r"D:\outside", "项目/甲", ".", "project-a"] {
            // project encodes the entire identity before deriving the directory.
            // project 在派生目录前编码完整身份。
            let project = paths.project(identity).expect("safe identity");
            assert_eq!(project.project_key.len(), 64);
            assert!(
                project
                    .project_key
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            );
            assert_eq!(
                project.root.parent(),
                Some(paths.root().join("projects").as_path())
            );
            assert_eq!(project.project_id, identity);
        }
        assert!(paths.project("  ").is_err());
        assert!(VceStoragePaths::resolve(Path::new("relative")).is_err());
    }

    /// Verify existing file/directory collisions fail before a storage handle can open them.
    /// 验证现有文件与目录冲突在存储句柄打开之前失败。
    ///
    /// Takes no parameters and returns nothing; creates only isolated fixture conflicts.
    /// 无参数、无返回值；仅创建隔离夹具冲突。
    #[test]
    fn project_storage_rejects_file_type_conflicts() {
        // temporary holds only this test's directories and marker file.
        // temporary 仅持有此测试的目录与标记文件。
        let temporary = tempfile::tempdir().expect("temporary root");
        // paths owns a catalog-only root before project preparation.
        // paths 在准备项目前仅持有目录元数据根。
        let paths = VceStoragePaths::resolve(temporary.path()).expect("resolve");
        paths.prepare_catalog().expect("catalog root");
        std::fs::File::create(paths.root().join("projects")).expect("conflicting file");
        assert!(paths.project("project-a").is_err());
        assert!(paths.prepare_project("project-a").is_err());
        assert!(!temporary.path().join("vmm").exists());
    }

    /// Verify real Windows junctions at every owned directory boundary reject reads and creation.
    /// 验证全部自有目录边界处的真实 Windows 目录联接均拒绝读取与创建。
    /// No parameters or output; isolated external sentinels must remain unchanged after rejection.
    /// 无参数及输出；拒绝操作后，隔离外部哨兵文件必须保持不变。
    #[cfg(windows)]
    #[test]
    fn project_storage_rejects_windows_junctions_without_touching_external_data() {
        // Each boundary is tested independently so an earlier invalid ancestor cannot mask it.
        // 独立测试每个边界，避免上层无效祖先掩盖当前边界。
        for boundary in ["vce", "projects", "project", "sqlite", "lancedb"] {
            // The application and outside roots are both owned disposable test fixtures.
            // 应用根与外部根均为测试自有的一次性夹具。
            let temporary = tempfile::tempdir().expect("temporary application root");
            // Outside data models another project's or memory store's protected contents.
            // 外部数据模拟另一个项目或记忆存储的受保护内容。
            let outside = tempfile::tempdir().expect("temporary outside root");
            std::fs::write(outside.path().join("sentinel"), b"unchanged")
                .expect("outside sentinel");
            // Resolve the exact cold project before introducing the attack fixture.
            // 引入攻击夹具前解析精确冷态项目。
            let paths = VceStoragePaths::resolve(temporary.path()).expect("resolve cold paths");
            // No database or project directory exists during this identity derivation.
            // 身份派生期间不存在数据库或项目目录。
            let project = paths.project("project-a").expect("cold project");
            // These are the five directory components verified by the production resolver.
            // 这些是生产解析器校验的五个目录组件。
            let link = match boundary {
                "vce" => paths.root().to_path_buf(),
                "projects" => paths.root().join("projects"),
                "project" => project.root.clone(),
                "sqlite" => project.root.join("sqlite"),
                "lancedb" => project.lancedb_path.clone(),
                _ => unreachable!("test boundary is exhaustive"),
            };
            assert!(link.starts_with(&paths.application_root));
            assert_ne!(link, paths.application_root);
            std::fs::create_dir_all(link.parent().expect("owned link parent"))
                .expect("prepare ordinary ancestors");
            // A directory junction needs no symbolic-link privilege; creation must actually succeed.
            // 目录联接无需符号链接特权；创建操作必须实际成功。
            let creation = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&link)
                .arg(outside.path())
                .output()
                .expect("create real directory junction");
            assert!(creation.status.success(), "junction creation: {creation:?}");
            // All production entries must reject the link before opening a database or writer.
            // 全部生产入口必须在打开数据库或写入者之前拒绝该链接。
            let read = paths.project("project-a");
            // Preparation is a separate mutation entry, not inferred from read-only rejection.
            // 准备为独立修改入口，不从只读拒绝推断其行为。
            let prepare = paths.prepare_project("project-a");
            // Explicit writer admission must also preserve the outside directory untouched.
            // 明确写入者准入也必须保持外部目录未被触碰。
            let writer = paths.try_acquire_writer("project-a", true);
            // Remove only the exact fixture junction before any assertions can unwind its owner.
            // 在断言可能展开夹具持有者之前，仅移除精确夹具联接本身。
            std::fs::remove_dir(&link).expect("remove junction without following it");
            assert!(read.is_err(), "read admitted {boundary} junction");
            assert!(prepare.is_err(), "prepare admitted {boundary} junction");
            assert!(writer.is_err(), "writer admitted {boundary} junction");
            assert_eq!(
                std::fs::read(outside.path().join("sentinel")).expect("preserved sentinel"),
                b"unchanged"
            );
            assert_eq!(
                std::fs::read_dir(outside.path())
                    .expect("inspect unchanged outside directory")
                    .count(),
                1
            );
            assert!(!temporary.path().join("vmm").exists());
        }
    }
}
