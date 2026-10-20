//! Memory project identities that survive moving a project directory.
//! 移动项目目录后仍保持不变的记忆项目身份。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{MemoryError, REGISTRY_FORMAT_VERSION, store::write_atomically};

/// One memory project and its bindings.
/// 一个记忆项目及其绑定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryProject {
    /// Random identifier that names the project's memory file.
    /// 命名项目记忆文件的随机标识。
    pub id: String,
    /// Display name, initially the directory name.
    /// 显示名，初始为目录名。
    pub name: String,
    /// Host project ids currently bound to this memory project.
    /// 当前绑定到该记忆项目的宿主项目标识。
    pub host_project_ids: Vec<String>,
    /// Every root path the project was opened from, oldest first.
    /// 项目曾经打开过的全部根路径，最早的在前。
    pub paths: Vec<String>,
    /// First commit hash of the repository, when it was known at binding time.
    /// 绑定时已知的仓库首个提交哈希。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_commit: Option<String>,
    /// Whether Markdown memory is used in this project.
    /// 该项目是否使用 Markdown 记忆。
    pub memory_enabled: bool,
    /// Creation time.
    /// 创建时间。
    pub created: String,
    /// Last change time.
    /// 最近变更时间。
    pub updated: String,
}

/// An orphaned memory project offered for linking, with the reasons it ranks where it does.
/// 供关联的孤立记忆项目，附带其排序原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanCandidate {
    /// The orphaned project.
    /// 孤立项目。
    pub project: MemoryProject,
    /// Stable reason keys: `same_directory_name`, `same_first_commit`.
    /// 稳定原因键：`same_directory_name`、`same_first_commit`。
    pub reasons: Vec<&'static str>,
}

/// The registry file content.
/// 注册表文件内容。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryRegistry {
    /// Every memory project.
    /// 全部记忆项目。
    pub projects: Vec<MemoryProject>,
    /// Host projects whose orphan prompt the user dismissed.
    /// 用户已忽略孤立提示的宿主项目。
    pub ignored_host_project_ids: Vec<String>,
}

/// On-disk envelope carrying the format version.
/// 携带格式版本的磁盘封装。
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegistryFile {
    /// Format version; only [`REGISTRY_FORMAT_VERSION`] is read.
    /// 格式版本；只读取 [`REGISTRY_FORMAT_VERSION`]。
    version: u32,
    /// Memory projects.
    /// 记忆项目。
    projects: Vec<MemoryProject>,
    /// Dismissed host projects.
    /// 已忽略的宿主项目。
    ignored_host_project_ids: Vec<String>,
}

impl MemoryRegistry {
    /// Registry file name inside the Markdown memory directory.
    /// Markdown 记忆目录中的注册表文件名。
    pub const FILE_NAME: &'static str = "registry.json";

    /// Loads the registry; a missing file is an empty registry.
    /// 加载注册表；文件不存在时为空注册表。
    pub fn load(markdown_directory: &Path) -> Result<Self, MemoryError> {
        let path = markdown_directory.join(Self::FILE_NAME);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(MemoryError::io(&path, "read", error)),
        };
        let version = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|value| value.get("version").and_then(serde_json::Value::as_u64));
        if version != Some(u64::from(REGISTRY_FORMAT_VERSION)) {
            return Err(MemoryError::UnsupportedFormat(format!(
                "Memory registry `{}` must declare format version {REGISTRY_FORMAT_VERSION}.",
                path.display()
            )));
        }
        let file: RegistryFile =
            serde_json::from_str(&text).map_err(|error| MemoryError::Json {
                path: path.display().to_string(),
                message: error.to_string(),
            })?;
        Ok(Self {
            projects: file.projects,
            ignored_host_project_ids: file.ignored_host_project_ids,
        })
    }

    /// Saves the registry atomically.
    /// 原子保存注册表。
    pub fn save(&self, markdown_directory: &Path) -> Result<(), MemoryError> {
        let mut text = self.to_json()?;
        text.push('\n');
        write_atomically(&markdown_directory.join(Self::FILE_NAME), &text)
    }

    /// Serializes the versioned registry file text without writing it.
    /// 序列化带版本的注册表文件文本但不写入。
    pub(crate) fn to_json(&self) -> Result<String, MemoryError> {
        let file = RegistryFile {
            version: REGISTRY_FORMAT_VERSION,
            projects: self.projects.clone(),
            ignored_host_project_ids: self.ignored_host_project_ids.clone(),
        };
        serde_json::to_string_pretty(&file).map_err(|error| MemoryError::Json {
            path: String::from(Self::FILE_NAME),
            message: error.to_string(),
        })
    }

    /// Returns the memory project bound to a host project.
    /// 返回绑定到宿主项目的记忆项目。
    pub fn project_for_host(&self, host_project_id: &str) -> Option<&MemoryProject> {
        self.projects.iter().find(|project| {
            project
                .host_project_ids
                .iter()
                .any(|bound| bound == host_project_id)
        })
    }

    /// Returns a memory project by id.
    /// 按标识返回记忆项目。
    pub fn project(&self, memory_project_id: &str) -> Option<&MemoryProject> {
        self.projects
            .iter()
            .find(|project| project.id == memory_project_id)
    }

    /// Returns a memory project by id mutably.
    /// 以可变方式按标识返回记忆项目。
    fn project_mut(&mut self, memory_project_id: &str) -> Result<&mut MemoryProject, MemoryError> {
        self.projects
            .iter_mut()
            .find(|project| project.id == memory_project_id)
            .ok_or_else(|| {
                MemoryError::invalid(format!(
                    "Memory project `{memory_project_id}` does not exist."
                ))
            })
    }

    /// Creates a memory project bound to a host project and returns its id.
    /// 创建绑定到宿主项目的记忆项目并返回其标识。
    pub fn create_for_host(
        &mut self,
        host_project_id: &str,
        root_path: &str,
        first_commit: Option<String>,
        now: &str,
    ) -> String {
        self.unbind_host(host_project_id);
        let id = self.unused_project_id();
        self.projects.push(MemoryProject {
            id: id.clone(),
            name: directory_name(root_path),
            host_project_ids: vec![host_project_id.to_string()],
            paths: vec![root_path.to_string()],
            first_commit,
            memory_enabled: true,
            created: now.to_string(),
            updated: now.to_string(),
        });
        self.ignored_host_project_ids
            .retain(|ignored| ignored != host_project_id);
        id
    }

    /// Creates a memory project bound to no host project, such as one restored from an export
    /// package, and returns its id; it can be linked to a host project later.
    /// 创建不绑定任何宿主项目的记忆项目（例如从导出包恢复的项目）并返回其标识；之后可再关联宿主项目。
    pub(crate) fn create_unbound(
        &mut self,
        name: &str,
        paths: Vec<String>,
        first_commit: Option<String>,
        now: &str,
    ) -> String {
        let id = self.unused_project_id();
        self.projects.push(MemoryProject {
            id: id.clone(),
            name: name.to_string(),
            host_project_ids: Vec::new(),
            paths,
            first_commit,
            memory_enabled: true,
            created: now.to_string(),
            updated: now.to_string(),
        });
        id
    }

    /// Generates a memory project id not used by any project.
    /// 生成未被任何项目使用的记忆项目标识。
    fn unused_project_id(&self) -> String {
        loop {
            let candidate = format!("p-{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
            if self.project(&candidate).is_none() {
                break candidate;
            }
        }
    }

    /// Binds a host project to an existing memory project and records the path.
    /// 将宿主项目绑定到既有记忆项目并记录路径。
    pub fn link(
        &mut self,
        memory_project_id: &str,
        host_project_id: &str,
        root_path: &str,
        now: &str,
    ) -> Result<(), MemoryError> {
        self.project_mut(memory_project_id)?;
        self.unbind_host(host_project_id);
        let project = self.project_mut(memory_project_id)?;
        project.host_project_ids.push(host_project_id.to_string());
        if !project.paths.iter().any(|path| path == root_path) {
            project.paths.push(root_path.to_string());
        }
        project.updated = now.to_string();
        self.ignored_host_project_ids
            .retain(|ignored| ignored != host_project_id);
        Ok(())
    }

    /// Records that the user dismissed the orphan prompt for a host project.
    /// 记录用户已忽略某宿主项目的孤立提示。
    pub fn ignore_host(&mut self, host_project_id: &str) {
        if !self
            .ignored_host_project_ids
            .iter()
            .any(|ignored| ignored == host_project_id)
        {
            self.ignored_host_project_ids
                .push(host_project_id.to_string());
        }
    }

    /// Turns Markdown memory on or off for one memory project.
    /// 为单个记忆项目开启或关闭 Markdown 记忆。
    pub fn set_enabled(
        &mut self,
        memory_project_id: &str,
        enabled: bool,
        now: &str,
    ) -> Result<(), MemoryError> {
        let project = self.project_mut(memory_project_id)?;
        project.memory_enabled = enabled;
        project.updated = now.to_string();
        Ok(())
    }

    /// Renames one memory project.
    /// 重命名单个记忆项目。
    pub fn rename(
        &mut self,
        memory_project_id: &str,
        name: &str,
        now: &str,
    ) -> Result<(), MemoryError> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > crate::MAX_TITLE_CHARS {
            return Err(MemoryError::invalid(format!(
                "A memory project name must have 1 to {} characters.",
                crate::MAX_TITLE_CHARS
            )));
        }
        let project = self.project_mut(memory_project_id)?;
        project.name = name.to_string();
        project.updated = now.to_string();
        Ok(())
    }

    /// Returns orphaned projects ranked for a host project, or nothing once it is bound or dismissed.
    /// 返回为宿主项目排序的孤立项目；已绑定或已忽略时返回空。
    pub fn orphan_candidates(
        &self,
        host_project_id: &str,
        root_path: &str,
        first_commit: Option<&str>,
        path_exists: impl Fn(&str) -> bool,
    ) -> Vec<OrphanCandidate> {
        if self.project_for_host(host_project_id).is_some()
            || self
                .ignored_host_project_ids
                .iter()
                .any(|ignored| ignored == host_project_id)
        {
            return Vec::new();
        }
        let name = directory_name(root_path).to_lowercase();
        let mut candidates = self
            .projects
            .iter()
            .filter(|project| {
                !project.paths.is_empty() && !project.paths.iter().any(|path| path_exists(path))
            })
            .map(|project| {
                let mut reasons = Vec::new();
                if project
                    .paths
                    .iter()
                    .any(|path| directory_name(path).to_lowercase() == name)
                {
                    reasons.push("same_directory_name");
                }
                if first_commit.is_some() && project.first_commit.as_deref() == first_commit {
                    reasons.push("same_first_commit");
                }
                OrphanCandidate {
                    project: project.clone(),
                    reasons,
                }
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            candidate_score(right)
                .cmp(&candidate_score(left))
                .then_with(|| right.project.updated.cmp(&left.project.updated))
        });
        candidates
    }

    /// Removes a host project id from every memory project.
    /// 从每个记忆项目中移除某宿主项目标识。
    fn unbind_host(&mut self, host_project_id: &str) {
        for project in &mut self.projects {
            project
                .host_project_ids
                .retain(|bound| bound != host_project_id);
        }
    }
}

/// Ranks a candidate: a shared first commit outweighs a shared directory name.
/// 为候选排序：相同首个提交优先于相同目录名。
fn candidate_score(candidate: &OrphanCandidate) -> u8 {
    candidate
        .reasons
        .iter()
        .map(|reason| match *reason {
            "same_first_commit" => 2,
            _ => 1,
        })
        .sum()
}

/// Returns the last path component, accepting both separators.
/// 返回最后一个路径组件，兼容两种分隔符。
pub(crate) fn directory_name(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Linking moves a host binding between projects, records paths and clears dismissal.
    /// 关联会在项目间迁移宿主绑定、记录路径并清除忽略。
    #[test]
    fn create_link_and_rank_orphans() {
        let mut registry = MemoryRegistry::default();
        let old = registry.create_for_host("host-old", "D:/work/app", Some("abc".into()), "t1");
        let other = registry.create_for_host("host-x", "D:/work/other", None, "t1");
        let exists = |path: &str| path == "D:/work/other";

        let candidates =
            registry.orphan_candidates("host-new", "E:/moved/app", Some("abc"), exists);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].project.id, old);
        assert_eq!(
            candidates[0].reasons,
            ["same_directory_name", "same_first_commit"]
        );

        registry.ignore_host("host-new");
        assert!(
            registry
                .orphan_candidates("host-new", "E:/moved/app", None, exists)
                .is_empty()
        );

        registry
            .link(&old, "host-new", "E:/moved/app", "t2")
            .expect("link");
        assert_eq!(
            registry
                .project_for_host("host-new")
                .map(|project| project.id.as_str()),
            Some(old.as_str())
        );
        assert!(registry.ignored_host_project_ids.is_empty());
        registry
            .link(&other, "host-new", "E:/moved/app", "t3")
            .expect("relink");
        assert!(registry.project(&old).expect("old").host_project_ids == ["host-old"]);

        let root = tempfile::tempdir().expect("temp dir");
        registry.save(root.path()).expect("save");
        assert_eq!(MemoryRegistry::load(root.path()).expect("load"), registry);
    }
}
