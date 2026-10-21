//! Export packages and their node-level import: export writes a versioned zip archive, import
//! previews the merge of every scope against the local files and applies it atomically after the
//! user mapped every packaged project and resolved every conflict.
//! 导出包及其节点级导入：导出写入带版本的 zip 归档；导入先预览各层与本地文件的合并，用户为每个包内
//! 项目指定去向并解决每个冲突后再原子应用。

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::registry::directory_name;
use crate::store::{PreparedFile, commit_files, write_guard};
use crate::{
    EXPORT_FORMAT_VERSION, MAX_FILE_BYTES, MAX_NODE_LEVEL, MAX_PACKAGE_BYTES, MAX_PACKAGE_PROJECTS,
    MarkdownMemoryStore, MemoryDocument, MemoryError, MemoryRegistry, MemoryScope, NodeView,
    now_utc_minute,
};

/// Format family declared by every package manifest.
/// 每个导出包清单声明的格式家族。
const PACKAGE_FORMAT: &str = "vulcan-markdown-memory";
/// Manifest entry name.
/// 清单条目名。
const MANIFEST_ENTRY: &str = "manifest.json";
/// User scope entry name.
/// 用户层条目名。
const USER_ENTRY: &str = "user.md";
/// Space scope entry name.
/// 空间层条目名。
const SPACE_ENTRY: &str = "space.md";
/// Directory of project scope entries.
/// 项目层条目所在目录。
const PROJECT_ENTRY_PREFIX: &str = "projects/";
/// Directory under the Markdown memory directory that keeps the files replaced by imports.
/// Markdown 记忆目录下保存被导入替换文件的目录。
const BACKUP_DIRECTORY: &str = "backups";

/// What an export package contains.
/// 导出包包含的内容。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportSelection {
    /// Whether the user scope is exported.
    /// 是否导出用户层。
    pub user: bool,
    /// Whether the space scope is exported.
    /// 是否导出空间层。
    pub space: bool,
    /// Memory projects whose project scope is exported.
    /// 导出其项目层的记忆项目。
    #[serde(default)]
    pub memory_project_ids: Vec<String>,
}

/// A written export package.
/// 已写入的导出包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedPackage {
    /// Path of the archive.
    /// 归档路径。
    pub path: String,
    /// Archive size in bytes.
    /// 归档字节数。
    pub byte_size: u64,
    /// SHA-256 of the archive bytes.
    /// 归档字节的 SHA-256。
    pub sha256: String,
}

/// One project recorded in a package manifest.
/// 导出包清单中记录的单个项目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageProject {
    /// Memory project id on the exporting machine; names the project entry.
    /// 导出端的记忆项目标识；用于命名项目条目。
    pub id: String,
    /// Display name.
    /// 显示名。
    pub name: String,
    /// Root paths the project was opened from, as a matching hint.
    /// 项目曾经打开的根路径，作为匹配提示。
    pub paths: Vec<String>,
    /// First commit of the repository, as a matching hint.
    /// 仓库首个提交，作为匹配提示。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_commit: Option<String>,
}

/// Versioned manifest of an export package.
/// 导出包带版本的清单。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackageManifest {
    /// Always [`PACKAGE_FORMAT`].
    /// 恒为 [`PACKAGE_FORMAT`]。
    format: String,
    /// Always [`EXPORT_FORMAT_VERSION`].
    /// 恒为 [`EXPORT_FORMAT_VERSION`]。
    version: u32,
    /// Export time.
    /// 导出时间。
    exported_at: String,
    /// Whether `user.md` is included.
    /// 是否包含 `user.md`。
    user: bool,
    /// Whether `space.md` is included.
    /// 是否包含 `space.md`。
    space: bool,
    /// Included projects, each with a `projects/<id>.md` entry.
    /// 包含的项目，每个都有 `projects/<id>.md` 条目。
    projects: Vec<PackageProject>,
}

/// A read and validated package.
/// 已读取并校验的导出包。
struct Package {
    /// SHA-256 of the archive bytes.
    /// 归档字节的 SHA-256。
    sha256: String,
    /// Manifest.
    /// 清单。
    manifest: PackageManifest,
    /// User scope text, when included.
    /// 用户层文本（包含时）。
    user: Option<String>,
    /// Space scope text, when included.
    /// 空间层文本（包含时）。
    space: Option<String>,
    /// Project scope texts by packaged project id.
    /// 按包内项目标识索引的项目层文本。
    projects: BTreeMap<String, String>,
}

/// Where one packaged project goes.
/// 单个包内项目的去向。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectTarget {
    /// Merge into an existing memory project.
    /// 合并到既有记忆项目。
    #[serde(rename_all = "camelCase")]
    Link {
        /// Local memory project.
        /// 本地记忆项目。
        memory_project_id: String,
    },
    /// Create a new memory project bound to no host project yet.
    /// 新建尚未绑定宿主项目的记忆项目。
    Create,
    /// Leave the project out.
    /// 不导入该项目。
    Ignore,
}

/// The user's choice for one packaged project.
/// 用户为单个包内项目做出的选择。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectMapping {
    /// Packaged project id.
    /// 包内项目标识。
    pub package_project_id: String,
    /// Chosen target.
    /// 选择的去向。
    pub target: ProjectTarget,
}

/// How one conflict is resolved.
/// 单个冲突的解决方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictChoice {
    /// Keep the local node unchanged.
    /// 保持本地节点不变。
    KeepLocal,
    /// Replace the local title and body with the imported ones.
    /// 以导入的标题与正文替换本地。
    TakeImported,
    /// Keep the local node and add the imported one beside it under a new id.
    /// 保留本地节点，并以新标识在其旁边添加导入节点。
    KeepBoth,
}

/// The user's resolution of one conflict.
/// 用户对单个冲突的解决。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConflictResolution {
    /// Scope of the conflict.
    /// 冲突所在层。
    pub scope: MemoryScope,
    /// Packaged project id for a project scope conflict.
    /// 项目层冲突对应的包内项目标识。
    #[serde(default)]
    pub package_project_id: Option<String>,
    /// Packaged node id.
    /// 包内节点标识。
    pub node_id: String,
    /// Chosen resolution.
    /// 选择的解决方式。
    pub choice: ConflictChoice,
}

/// A local memory project suggested for a packaged project, with stable reason keys.
/// 为包内项目建议的本地记忆项目，附带稳定原因键。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCandidate {
    /// Local memory project.
    /// 本地记忆项目。
    pub memory_project_id: String,
    /// Its display name.
    /// 其显示名。
    pub name: String,
    /// `same_path`, `same_first_commit`, `same_directory_name`.
    /// `same_path`、`same_first_commit`、`same_directory_name`。
    pub reasons: Vec<String>,
}

/// One packaged project with its suggested local matches.
/// 单个包内项目及其建议的本地匹配。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageProjectPreview {
    /// The packaged project.
    /// 包内项目。
    pub project: PackageProject,
    /// Suggested local projects, best first; only hints, nothing is preselected.
    /// 建议的本地项目，最佳在前；仅作提示，不预选。
    pub candidates: Vec<ProjectCandidate>,
}

/// A node identified by id and title path.
/// 以标识与标题路径识别的节点。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeSummary {
    /// Node id.
    /// 节点标识。
    pub id: String,
    /// Titles from the top-level ancestor down to the node.
    /// 从顶层祖先到节点的标题链。
    pub path: Vec<String>,
}

/// One side of a conflict.
/// 冲突的一侧。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeSide {
    /// Title.
    /// 标题。
    pub title: String,
    /// Normalized body.
    /// 规范化正文。
    pub body: String,
    /// Last update time, when recorded.
    /// 最近更新时间（有记录时）。
    pub updated: Option<String>,
}

/// Which side of a conflict was updated later.
/// 冲突中哪一侧更新得更晚。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NewerSide {
    /// The local node.
    /// 本地节点。
    Local,
    /// The imported node.
    /// 导入节点。
    Imported,
    /// Equal or unknown times.
    /// 时间相同或未知。
    Unknown,
}

/// A node present on both sides with different content.
/// 两侧都存在但内容不同的节点。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeConflict {
    /// Packaged node id, used by the resolution.
    /// 包内节点标识，供解决方式引用。
    pub id: String,
    /// Local title path.
    /// 本地标题链。
    pub path: Vec<String>,
    /// Local side.
    /// 本地一侧。
    pub local: NodeSide,
    /// Imported side.
    /// 导入一侧。
    pub imported: NodeSide,
    /// Which side is newer; the suggested default.
    /// 较新的一侧；即建议的默认值。
    pub newer: NewerSide,
}

/// The merge preview of one scope file.
/// 单层文件的合并预览。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeMergePreview {
    /// Scope.
    /// 层。
    pub scope: MemoryScope,
    /// Packaged project id of a project scope.
    /// 项目层对应的包内项目标识。
    pub package_project_id: Option<String>,
    /// Local memory project merged into; `None` when a new project is created.
    /// 合并目标本地记忆项目；新建项目时为 `None`。
    pub memory_project_id: Option<String>,
    /// Nodes identical on both sides, skipped.
    /// 两侧相同而跳过的节点数。
    pub same: usize,
    /// Nodes added because the local file lacks them.
    /// 因本地缺少而添加的节点。
    pub added: Vec<NodeSummary>,
    /// Added nodes whose title path already exists locally under another id; both are kept.
    /// 标题链在本地已以其他标识存在的添加节点；两者都保留。
    pub duplicates: Vec<NodeSummary>,
    /// Nodes that need a resolution.
    /// 需要解决的节点。
    pub conflicts: Vec<NodeConflict>,
    /// Titles of the top-level sections the import creates.
    /// 导入会新建的顶层分区标题。
    pub new_sections: Vec<String>,
}

/// The preview of an import, bound to the exact archive bytes by their SHA-256.
/// 导入预览，以 SHA-256 绑定到精确的归档字节。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    /// SHA-256 of the archive; applying requires the same bytes.
    /// 归档的 SHA-256；应用时要求字节一致。
    pub sha256: String,
    /// Export time recorded in the package.
    /// 包中记录的导出时间。
    pub exported_at: String,
    /// Packaged projects with suggested matches; every one needs a mapping before applying.
    /// 带建议匹配的包内项目；应用前每一个都需要指定去向。
    pub projects: Vec<PackageProjectPreview>,
    /// Merge previews of the user and space scopes and of every mapped project.
    /// 用户层、空间层及每个已指定去向项目的合并预览。
    pub scopes: Vec<ScopeMergePreview>,
}

/// What an applied import changed.
/// 已应用导入所做的变更。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    /// Directory holding the replaced files; `None` when no existing file changed.
    /// 保存被替换文件的目录；没有既有文件变化时为 `None`。
    pub backup_directory: Option<String>,
    /// Nodes added, including duplicates and kept-both copies.
    /// 添加的节点数，含疑似重复与两者都留的副本。
    pub added: usize,
    /// Local nodes replaced by imported content.
    /// 被导入内容替换的本地节点数。
    pub replaced: usize,
    /// Nodes left unchanged: identical nodes and conflicts that kept the local side.
    /// 保持不变的节点数：相同节点与保留本地的冲突。
    pub unchanged: usize,
    /// Memory projects created by the import.
    /// 导入新建的记忆项目。
    pub created_projects: Vec<String>,
}

/// The merge of one imported document into a local one.
/// 将一份导入文档合并到本地文档的结果。
struct MergeOutcome {
    /// Merged document.
    /// 合并后的文档。
    document: MemoryDocument,
    /// Preview counters and lists.
    /// 预览计数与列表。
    same: usize,
    added: Vec<NodeSummary>,
    duplicates: Vec<NodeSummary>,
    conflicts: Vec<NodeConflict>,
    new_sections: Vec<String>,
    /// Conflicts that took the imported side.
    /// 采用导入一侧的冲突数。
    replaced: usize,
    /// Conflicts that kept both sides.
    /// 两者都留的冲突数。
    kept_both: usize,
}

impl MergeOutcome {
    /// Returns whether the merge changed the local document.
    /// 返回合并是否改变了本地文档。
    fn changed(&self) -> bool {
        !self.added.is_empty() || self.replaced > 0 || self.kept_both > 0
    }
}

/// One scope file planned by an import.
/// 导入规划的单层文件。
struct PlannedScope {
    /// Scope.
    /// 层。
    scope: MemoryScope,
    /// Packaged project id of a project scope.
    /// 项目层对应的包内项目标识。
    package_project_id: Option<String>,
    /// Local memory project; `None` while a created project has no id yet.
    /// 本地记忆项目；新建项目尚无标识时为 `None`。
    memory_project_id: Option<String>,
    /// Packaged text.
    /// 包内文本。
    text: String,
}

impl MarkdownMemoryStore {
    /// Writes an export package of the selected scopes into an existing directory and returns
    /// its path; an existing file is never overwritten.
    /// 将所选层的导出包写入既有目录并返回其路径；绝不覆盖已有文件。
    pub fn export_package(
        &self,
        selection: &ExportSelection,
        destination_directory: &Path,
    ) -> Result<ExportedPackage, MemoryError> {
        if !selection.user && !selection.space && selection.memory_project_ids.is_empty() {
            return Err(MemoryError::invalid(
                "Choose at least one scope or project to export.",
            ));
        }
        if selection.memory_project_ids.len() > MAX_PACKAGE_PROJECTS {
            return Err(MemoryError::invalid(format!(
                "A package can hold at most {MAX_PACKAGE_PROJECTS} projects."
            )));
        }
        if !destination_directory.is_dir() {
            return Err(MemoryError::invalid(format!(
                "Export directory `{}` does not exist.",
                destination_directory.display()
            )));
        }
        let registry = self.registry()?;
        let mut projects = Vec::new();
        let mut entries = Vec::new();
        for id in selection.memory_project_ids.iter().collect::<BTreeSet<_>>() {
            let project = registry.project(id).ok_or_else(|| {
                MemoryError::invalid(format!("Memory project `{id}` does not exist."))
            })?;
            projects.push(PackageProject {
                id: project.id.clone(),
                name: project.name.clone(),
                paths: project.paths.clone(),
                first_commit: project.first_commit.clone(),
            });
            entries.push((
                format!("{PROJECT_ENTRY_PREFIX}{id}.md"),
                self.read_file_text(MemoryScope::Project, Some(id))?,
            ));
        }
        if selection.user {
            entries.push((
                USER_ENTRY.to_string(),
                self.read_file_text(MemoryScope::User, None)?,
            ));
        }
        if selection.space {
            entries.push((
                SPACE_ENTRY.to_string(),
                self.read_file_text(MemoryScope::Space, None)?,
            ));
        }
        let now = now_utc_minute();
        let manifest = PackageManifest {
            format: PACKAGE_FORMAT.to_string(),
            version: EXPORT_FORMAT_VERSION,
            exported_at: now.clone(),
            user: selection.user,
            space: selection.space,
            projects,
        };
        let manifest = serde_json::to_string_pretty(&manifest).map_err(|error| {
            MemoryError::invalid(format!(
                "The package manifest could not be encoded: {error}"
            ))
        })?;
        let bytes = write_archive(&manifest, &entries)?;
        let path = destination_directory.join(format!(
            "vulcan-memory-{}-{}.zip",
            file_stamp(&now),
            &uuid::Uuid::new_v4().simple().to_string()[..6]
        ));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| MemoryError::io(&path, "created", error))?;
        if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = std::fs::remove_file(&path);
            return Err(MemoryError::io(&path, "written", error));
        }
        Ok(ExportedPackage {
            path: path.display().to_string(),
            byte_size: bytes.len() as u64,
            sha256: hex::encode(Sha256::digest(&bytes)),
        })
    }

    /// Previews importing a package; mapped projects are compared with their targets, unmapped
    /// ones are only listed. Nothing is written.
    /// 预览导入导出包；已指定去向的项目与其目标比较，未指定的项目只列出。不写入任何内容。
    pub fn preview_import(
        &self,
        archive: &Path,
        mappings: &[ProjectMapping],
    ) -> Result<ImportPreview, MemoryError> {
        let package = read_package(archive)?;
        let registry = self.registry()?;
        let mappings = mapping_table(&package, &registry, mappings, false)?;
        let mut scopes = Vec::new();
        for planned in planned_scopes(&package, &mappings) {
            let local = self.planned_document(&planned)?;
            let outcome = merge(
                &local,
                &MemoryDocument::parse(&planned.text),
                planned.scope,
                |_| None,
            )?;
            scopes.push(ScopeMergePreview {
                scope: planned.scope,
                package_project_id: planned.package_project_id,
                memory_project_id: planned.memory_project_id,
                same: outcome.same,
                added: outcome.added,
                duplicates: outcome.duplicates,
                conflicts: outcome.conflicts,
                new_sections: outcome.new_sections,
            });
        }
        Ok(ImportPreview {
            sha256: package.sha256.clone(),
            exported_at: package.manifest.exported_at.clone(),
            projects: package
                .manifest
                .projects
                .iter()
                .map(|project| PackageProjectPreview {
                    project: project.clone(),
                    candidates: project_candidates(project, &registry),
                })
                .collect(),
            scopes,
        })
    }

    /// Applies a previewed import: the archive must still have the previewed SHA-256, every
    /// packaged project needs a mapping and every conflict a resolution. Replaced files are
    /// backed up first, then every file and the registry are committed as one batch.
    /// 应用已预览的导入：归档必须仍是预览时的 SHA-256，每个包内项目都需指定去向，每个冲突都需解决。
    /// 先备份被替换的文件，再将全部文件与注册表作为一批提交。
    pub fn apply_import(
        &self,
        archive: &Path,
        expected_sha256: &str,
        mappings: &[ProjectMapping],
        resolutions: &[ConflictResolution],
    ) -> Result<ImportReport, MemoryError> {
        let package = read_package(archive)?;
        if package.sha256 != expected_sha256 {
            return Err(MemoryError::invalid(
                "The package changed since it was previewed; preview it again.",
            ));
        }
        let _guard = write_guard();
        let mut registry = self.registry()?;
        let mappings = mapping_table(&package, &registry, mappings, true)?;
        let mut choices = HashMap::new();
        for resolution in resolutions {
            let key = conflict_key(
                resolution.scope,
                resolution.package_project_id.as_deref(),
                &resolution.node_id,
            );
            if choices.insert(key, resolution.choice).is_some() {
                return Err(MemoryError::invalid(format!(
                    "Conflict `{}` is resolved more than once.",
                    resolution.node_id
                )));
            }
        }
        let now = now_utc_minute();
        let mut documents: BTreeMap<PathBuf, (Option<String>, MemoryDocument)> = BTreeMap::new();
        let mut report = ImportReport {
            backup_directory: None,
            added: 0,
            replaced: 0,
            unchanged: 0,
            created_projects: Vec::new(),
        };
        let mut resolved = 0usize;
        for mut planned in planned_scopes(&package, &mappings) {
            if planned.scope == MemoryScope::Project && planned.memory_project_id.is_none() {
                let project = package
                    .manifest
                    .projects
                    .iter()
                    .find(|project| Some(&project.id) == planned.package_project_id.as_ref())
                    .expect("planned projects come from the manifest");
                let id = registry.create_unbound(
                    &project.name,
                    project.paths.clone(),
                    project.first_commit.clone(),
                    &now,
                );
                report.created_projects.push(id.clone());
                planned.memory_project_id = Some(id);
            }
            let path = self.scope_path(planned.scope, planned.memory_project_id.as_deref())?;
            if !documents.contains_key(&path) {
                let original = MarkdownMemoryStore::read_optional(&path)?;
                let document = original
                    .as_deref()
                    .map(MemoryDocument::parse)
                    .unwrap_or_else(MemoryDocument::empty);
                documents.insert(path.clone(), (original, document));
            }
            let (_, local) = documents.get(&path).expect("inserted above");
            let outcome = merge(
                local,
                &MemoryDocument::parse(&planned.text),
                planned.scope,
                |node_id| {
                    choices
                        .get(&conflict_key(
                            planned.scope,
                            planned.package_project_id.as_deref(),
                            node_id,
                        ))
                        .copied()
                },
            )?;
            if let Some(conflict) = outcome.conflicts.iter().find(|conflict| {
                !choices.contains_key(&conflict_key(
                    planned.scope,
                    planned.package_project_id.as_deref(),
                    &conflict.id,
                ))
            }) {
                return Err(MemoryError::invalid(format!(
                    "Conflict `{}` ({}) has no resolution.",
                    conflict.id,
                    conflict.path.join(" > ")
                )));
            }
            resolved += outcome.conflicts.len();
            report.added += outcome.added.len() + outcome.kept_both;
            report.replaced += outcome.replaced;
            report.unchanged +=
                outcome.same + outcome.conflicts.len() - outcome.replaced - outcome.kept_both;
            if outcome.changed() {
                documents.get_mut(&path).expect("inserted above").1 = outcome.document;
            }
        }
        if resolved != choices.len() {
            return Err(MemoryError::invalid(
                "A resolution names a conflict this import does not have; preview it again.",
            ));
        }
        let mut files = Vec::new();
        for (path, (original, document)) in documents {
            let text = document.render();
            if original.as_deref() == Some(text.as_str()) || (original.is_none() && text.is_empty())
            {
                continue;
            }
            if text.len() > MAX_FILE_BYTES {
                return Err(MemoryError::invalid(format!(
                    "`{}` would grow to {} bytes, above the {MAX_FILE_BYTES}-byte limit.",
                    path.display(),
                    text.len()
                )));
            }
            files.push(PreparedFile {
                path,
                original,
                text,
            });
        }
        if !report.created_projects.is_empty() {
            let path = self.markdown_directory().join(MemoryRegistry::FILE_NAME);
            let original = MarkdownMemoryStore::read_optional(&path)?;
            let mut text = registry.to_json()?;
            text.push('\n');
            files.push(PreparedFile {
                path,
                original,
                text,
            });
        }
        report.backup_directory = self.back_up(&files, &now)?;
        commit_files(files)?;
        Ok(report)
    }

    /// Loads the local document a planned scope merges into; a created project starts empty.
    /// 读取规划层要合并进的本地文档；新建项目从空文档开始。
    fn planned_document(&self, planned: &PlannedScope) -> Result<MemoryDocument, MemoryError> {
        if planned.scope == MemoryScope::Project && planned.memory_project_id.is_none() {
            return Ok(MemoryDocument::empty());
        }
        self.load(planned.scope, planned.memory_project_id.as_deref())
    }

    /// Copies every existing file a commit replaces into a new backup directory.
    /// 将提交会替换的每个既有文件复制到新的备份目录。
    fn back_up(&self, files: &[PreparedFile], now: &str) -> Result<Option<String>, MemoryError> {
        let markdown = self.markdown_directory();
        let replaced: Vec<&PreparedFile> = files
            .iter()
            .filter(|file| file.original.is_some())
            .collect();
        if replaced.is_empty() {
            return Ok(None);
        }
        let directory = markdown.join(BACKUP_DIRECTORY).join(format!(
            "import-{}-{}",
            file_stamp(now),
            &uuid::Uuid::new_v4().simple().to_string()[..6]
        ));
        for file in replaced {
            let relative = file.path.strip_prefix(&markdown).unwrap_or(&file.path);
            let target = directory.join(relative);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| MemoryError::io(parent, "created", error))?;
            }
            std::fs::write(&target, file.original.as_deref().unwrap_or_default())
                .map_err(|error| MemoryError::io(&target, "written", error))?;
        }
        Ok(Some(directory.display().to_string()))
    }
}

/// Key of one conflict across all scopes of an import.
/// 导入中跨所有层的单个冲突键。
fn conflict_key(scope: MemoryScope, package_project_id: Option<&str>, node_id: &str) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{node_id}",
        scope.key(),
        package_project_id.unwrap_or_default()
    )
}

/// Validates mappings against the package and the registry and indexes them by packaged id;
/// `complete` requires a mapping for every packaged project.
/// 依据导出包与注册表校验去向并按包内标识索引；`complete` 要求每个包内项目都有去向。
fn mapping_table(
    package: &Package,
    registry: &MemoryRegistry,
    mappings: &[ProjectMapping],
    complete: bool,
) -> Result<BTreeMap<String, ProjectTarget>, MemoryError> {
    let mut table = BTreeMap::new();
    for mapping in mappings {
        if !package.projects.contains_key(&mapping.package_project_id) {
            return Err(MemoryError::invalid(format!(
                "The package has no project `{}`.",
                mapping.package_project_id
            )));
        }
        if let ProjectTarget::Link { memory_project_id } = &mapping.target
            && registry.project(memory_project_id).is_none()
        {
            return Err(MemoryError::invalid(format!(
                "Memory project `{memory_project_id}` does not exist."
            )));
        }
        if table
            .insert(mapping.package_project_id.clone(), mapping.target.clone())
            .is_some()
        {
            return Err(MemoryError::invalid(format!(
                "Project `{}` is mapped more than once.",
                mapping.package_project_id
            )));
        }
    }
    if complete && let Some(missing) = package.projects.keys().find(|id| !table.contains_key(*id)) {
        return Err(MemoryError::invalid(format!(
            "Choose where packaged project `{missing}` goes before importing."
        )));
    }
    Ok(table)
}

/// Lists the scope files an import touches: user, space, then every linked or created project.
/// 列出导入涉及的层文件：用户层、空间层，然后是每个关联或新建的项目。
fn planned_scopes(
    package: &Package,
    mappings: &BTreeMap<String, ProjectTarget>,
) -> Vec<PlannedScope> {
    let mut planned = Vec::new();
    for (scope, text) in [
        (MemoryScope::User, &package.user),
        (MemoryScope::Space, &package.space),
    ] {
        if let Some(text) = text {
            planned.push(PlannedScope {
                scope,
                package_project_id: None,
                memory_project_id: None,
                text: text.clone(),
            });
        }
    }
    for project in &package.manifest.projects {
        let memory_project_id = match mappings.get(&project.id) {
            Some(ProjectTarget::Link { memory_project_id }) => Some(memory_project_id.clone()),
            Some(ProjectTarget::Create) => None,
            Some(ProjectTarget::Ignore) | None => continue,
        };
        planned.push(PlannedScope {
            scope: MemoryScope::Project,
            package_project_id: Some(project.id.clone()),
            memory_project_id,
            text: package.projects[&project.id].clone(),
        });
    }
    planned
}

/// Ranks local memory projects that look like a packaged project.
/// 为看起来与包内项目相同的本地记忆项目排序。
fn project_candidates(
    project: &PackageProject,
    registry: &MemoryRegistry,
) -> Vec<ProjectCandidate> {
    let names = project
        .paths
        .iter()
        .map(|path| directory_name(path).to_lowercase())
        .collect::<BTreeSet<_>>();
    let mut candidates = registry
        .projects
        .iter()
        .filter_map(|local| {
            let mut reasons = Vec::new();
            if local.paths.iter().any(|path| project.paths.contains(path)) {
                reasons.push(String::from("same_path"));
            }
            if project.first_commit.is_some() && local.first_commit == project.first_commit {
                reasons.push(String::from("same_first_commit"));
            }
            if local
                .paths
                .iter()
                .any(|path| names.contains(&directory_name(path).to_lowercase()))
            {
                reasons.push(String::from("same_directory_name"));
            }
            (!reasons.is_empty()).then(|| ProjectCandidate {
                memory_project_id: local.id.clone(),
                name: local.name.clone(),
                reasons,
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .reasons
            .len()
            .cmp(&left.reasons.len())
            .then_with(|| left.name.cmp(&right.name))
    });
    candidates
}

/// Merges an imported document into a local one node by node. Top-level sections match by
/// section key, other nodes by id; identical nodes are skipped, missing nodes are appended under
/// their matched parent, and conflicts follow `choose` (unresolved conflicts keep the local side).
/// 逐节点将导入文档合并到本地文档。顶层分区按分区键匹配，其他节点按标识匹配；相同节点跳过，缺失
/// 节点追加到已匹配的父节点下，冲突按 `choose` 处理（未解决的冲突保留本地一侧）。
fn merge(
    local: &MemoryDocument,
    imported: &MemoryDocument,
    scope: MemoryScope,
    choose: impl Fn(&str) -> Option<ConflictChoice>,
) -> Result<MergeOutcome, MemoryError> {
    let mut outcome = MergeOutcome {
        document: local.clone(),
        same: 0,
        added: Vec::new(),
        duplicates: Vec::new(),
        conflicts: Vec::new(),
        new_sections: Vec::new(),
        replaced: 0,
        kept_both: 0,
    };
    let eol = local.eol();
    let imported_views = imported.views(scope);
    let imported_ids: HashMap<Vec<usize>, String> = imported_views
        .iter()
        .map(|view| (view.locator.clone(), view.id.clone()))
        .collect();
    // Imported node id → locator of the node it maps to in the merged document. Merging only
    // appends, so recorded locators stay valid.
    // 导入节点标识 → 其在合并文档中对应节点的定位链。合并只追加，因此已记录的定位链保持有效。
    let mut placed: HashMap<String, Vec<usize>> = HashMap::new();
    for view in &imported_views {
        let node = imported
            .node(&view.locator)
            .expect("views come from this document");
        let merged_views = outcome.document.views(scope);
        if let Some(found) = matching_view(&merged_views, view) {
            placed.insert(view.id.clone(), found.locator.clone());
            let title = view.path.last().cloned().unwrap_or_default();
            let local_title = found.path.last().cloned().unwrap_or_default();
            if local_title.trim() == title.trim() && found.body == view.body {
                outcome.same += 1;
                continue;
            }
            let local_updated = found.meta.as_ref().map(|meta| meta.updated.clone());
            let imported_updated = view.meta.as_ref().map(|meta| meta.updated.clone());
            outcome.conflicts.push(NodeConflict {
                id: view.id.clone(),
                path: found.path.clone(),
                local: NodeSide {
                    title: local_title,
                    body: found.body.clone(),
                    updated: local_updated.clone(),
                },
                imported: NodeSide {
                    title: title.clone(),
                    body: view.body.clone(),
                    updated: imported_updated.clone(),
                },
                newer: match (local_updated, imported_updated) {
                    (Some(local), Some(imported)) if imported > local => NewerSide::Imported,
                    (Some(local), Some(imported)) if local > imported => NewerSide::Local,
                    _ => NewerSide::Unknown,
                },
            });
            match choose(&view.id) {
                None | Some(ConflictChoice::KeepLocal) => {}
                Some(ConflictChoice::TakeImported) => {
                    let target = outcome
                        .document
                        .node_mut(&found.locator)
                        .expect("the match comes from this document");
                    target.set_title(node.title.clone());
                    target.set_body(crate::document::stored_body(&view.body, eol));
                    match (&found.meta, &view.meta) {
                        // A section keeps its local identity and takes the imported time.
                        // 分区保留本地身份并采用导入时间。
                        (Some(local), Some(imported)) if local.id != imported.id => {
                            let mut meta = local.clone();
                            meta.updated = imported.updated.clone();
                            target.set_meta(meta);
                        }
                        (_, Some(imported)) => target.set_meta(imported.clone()),
                        _ => {}
                    }
                    outcome.replaced += 1;
                }
                Some(ConflictChoice::KeepBoth) => {
                    let mut copy = node.detached(found.level, eol);
                    if let Some(meta) = copy.meta.as_mut() {
                        meta.id = unused_node_id(&merged_views);
                        meta.section = None;
                    }
                    let parent = &found.locator[..found.locator.len() - 1];
                    outcome
                        .document
                        .append((!parent.is_empty()).then_some(parent), copy);
                    outcome.kept_both += 1;
                }
            }
            continue;
        }
        // The node is new here: place it under the node its imported parent maps to.
        // 该节点在本地不存在：放到其导入父节点所对应的节点之下。
        let parent_locator = &view.locator[..view.locator.len() - 1];
        let (parent, level) = if parent_locator.is_empty() {
            (None, view.level)
        } else {
            let parent_id = &imported_ids[parent_locator];
            let target = placed.get(parent_id).cloned().ok_or_else(|| {
                MemoryError::invalid(format!("Packaged node `{}` has no parent.", view.id))
            })?;
            let imported_parent = imported
                .node(parent_locator)
                .expect("parents come from this document");
            let merged_parent = outcome
                .document
                .node(&target)
                .expect("placed locators stay valid");
            let level = merged_parent.level + (view.level - imported_parent.level);
            (Some(target), level)
        };
        if level > MAX_NODE_LEVEL {
            return Err(MemoryError::invalid(format!(
                "Packaged node `{}` would sit below heading level {MAX_NODE_LEVEL}.",
                view.id
            )));
        }
        let summary = NodeSummary {
            id: view.id.clone(),
            path: view.path.clone(),
        };
        if merged_views
            .iter()
            .any(|existing| existing.path == view.path)
        {
            outcome.duplicates.push(summary.clone());
        }
        if parent.is_none() {
            outcome
                .new_sections
                .push(view.path.last().cloned().unwrap_or_default());
        }
        outcome.added.push(summary);
        let siblings = match &parent {
            Some(locator) => outcome
                .document
                .node(locator)
                .map_or(0, |node| node.children.len()),
            None => outcome.document.roots.len(),
        };
        outcome
            .document
            .append(parent.as_deref(), node.detached(level, eol));
        let mut locator = parent.unwrap_or_default();
        locator.push(siblings);
        placed.insert(view.id.clone(), locator);
    }
    Ok(outcome)
}

/// Finds the local node an imported node stands for: a top-level section by its key, any other
/// node by id.
/// 查找导入节点对应的本地节点：顶层分区按分区键，其他节点按标识。
fn matching_view<'views>(
    views: &'views [NodeView],
    imported: &NodeView,
) -> Option<&'views NodeView> {
    let section = |view: &NodeView| {
        (view.locator.len() == 1)
            .then(|| view.meta.as_ref().and_then(|meta| meta.section.clone()))
            .flatten()
    };
    if let Some(key) = section(imported)
        && let Some(found) = views
            .iter()
            .find(|view| section(view).as_deref() == Some(key.as_str()))
    {
        return Some(found);
    }
    views.iter().find(|view| view.id == imported.id)
}

/// Generates a node id unused in a document.
/// 生成在文档中未被使用的节点标识。
fn unused_node_id(views: &[NodeView]) -> String {
    loop {
        let candidate = format!("m-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
        if !views.iter().any(|view| view.id == candidate) {
            break candidate;
        }
    }
}

/// Builds the archive bytes from the manifest and the scope entries.
/// 由清单与各层条目构建归档字节。
fn write_archive(manifest: &str, entries: &[(String, String)]) -> Result<Vec<u8>, MemoryError> {
    let failure = |error: &dyn std::fmt::Display| {
        MemoryError::invalid(format!("The export package could not be built: {error}"))
    };
    let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, text) in std::iter::once((MANIFEST_ENTRY, manifest)).chain(
        entries
            .iter()
            .map(|(name, text)| (name.as_str(), text.as_str())),
    ) {
        writer
            .start_file(name, options)
            .map_err(|error| failure(&error))?;
        writer
            .write_all(text.as_bytes())
            .map_err(|error| failure(&error))?;
    }
    Ok(writer
        .finish()
        .map_err(|error| failure(&error))?
        .into_inner())
}

/// Reads and validates a package: the manifest format and version, a closed set of entries that
/// matches the manifest, and bounded UTF-8 files.
/// 读取并校验导出包：清单格式与版本、与清单一致的封闭条目集合，以及有界的 UTF-8 文件。
fn read_package(archive: &Path) -> Result<Package, MemoryError> {
    let size = std::fs::metadata(archive)
        .map_err(|error| MemoryError::io(archive, "read", error))?
        .len();
    if size > MAX_PACKAGE_BYTES as u64 {
        return Err(MemoryError::invalid(format!(
            "A memory package can be at most {MAX_PACKAGE_BYTES} bytes."
        )));
    }
    let bytes = std::fs::read(archive).map_err(|error| MemoryError::io(archive, "read", error))?;
    let sha256 = hex::encode(Sha256::digest(&bytes));
    let invalid = |detail: &str| {
        MemoryError::UnsupportedFormat(format!(
            "`{}` is not a Vulcan memory package: {detail}",
            archive.display()
        ))
    };
    let mut zip = ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|error| invalid(&error.to_string()))?;
    if zip.len() > MAX_PACKAGE_PROJECTS + 3 {
        return Err(invalid("it has too many entries"));
    }
    let mut files = BTreeMap::new();
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| invalid(&error.to_string()))?;
        let name = entry
            .name()
            .map_err(|error| invalid(&error.to_string()))?
            .into_owned();
        if entry.is_dir() || entry.enclosed_name().is_none() || entry.size() > MAX_FILE_BYTES as u64
        {
            return Err(invalid(&format!("entry `{name}` is not allowed")));
        }
        let mut text = String::new();
        (&mut entry)
            .take(MAX_FILE_BYTES as u64 + 1)
            .read_to_string(&mut text)
            .map_err(|_| invalid(&format!("entry `{name}` is not UTF-8 text")))?;
        if text.len() > MAX_FILE_BYTES || files.insert(name.clone(), text).is_some() {
            return Err(invalid(&format!("entry `{name}` is not allowed")));
        }
    }
    let manifest_text = files
        .remove(MANIFEST_ENTRY)
        .ok_or_else(|| invalid("it has no manifest"))?;
    let header: serde_json::Value =
        serde_json::from_str(&manifest_text).map_err(|error| invalid(&error.to_string()))?;
    if header.get("format").and_then(serde_json::Value::as_str) != Some(PACKAGE_FORMAT)
        || header.get("version").and_then(serde_json::Value::as_u64)
            != Some(u64::from(EXPORT_FORMAT_VERSION))
    {
        return Err(MemoryError::UnsupportedFormat(format!(
            "`{}` must be a {PACKAGE_FORMAT} package of format version {EXPORT_FORMAT_VERSION}.",
            archive.display()
        )));
    }
    let manifest: PackageManifest =
        serde_json::from_value(header).map_err(|error| invalid(&error.to_string()))?;
    let user = files.remove(USER_ENTRY);
    let space = files.remove(SPACE_ENTRY);
    if user.is_some() != manifest.user || space.is_some() != manifest.space {
        return Err(invalid("its scope files do not match the manifest"));
    }
    let mut projects = BTreeMap::new();
    for project in &manifest.projects {
        let entry = format!("{PROJECT_ENTRY_PREFIX}{}.md", project.id);
        let valid_id = !project.id.is_empty()
            && project
                .id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-');
        let text = files
            .remove(&entry)
            .filter(|_| valid_id)
            .ok_or_else(|| invalid(&format!("project `{}` has no file", project.id)))?;
        if projects.insert(project.id.clone(), text).is_some() {
            return Err(invalid(&format!("project `{}` appears twice", project.id)));
        }
    }
    if let Some(name) = files.keys().next() {
        return Err(invalid(&format!("entry `{name}` is not allowed")));
    }
    Ok(Package {
        sha256,
        manifest,
        user,
        space,
        projects,
    })
}

/// Turns a UTC metadata timestamp such as `2026-10-20T15:59Z` into a file-name stamp such as
/// `20261020T1559Z`; the trailing `Z` keeps the stamp readable as UTC.
/// 将 `2026-10-20T15:59Z` 这类 UTC 元数据时间转为 `20261020T1559Z` 这类文件名时间戳；结尾的 `Z` 表明其为 UTC。
fn file_stamp(now: &str) -> String {
    now.chars().filter(char::is_ascii_alphanumeric).collect()
}

#[cfg(test)]
mod tests;
