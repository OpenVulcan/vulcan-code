//! Project-level resource catalog for session persistence V1.
//! 会话持久化 V1 的项目级资源目录。
//!
//! Resources belong to the project and are addressed by a project-scoped
//! `ResourceId`. Session archive, delete, and fork operations only retain or
//! copy ordered ids and therefore have no resource-reclamation path.
//! 资源归项目所有，并通过项目级 `ResourceId` 精确寻址。会话归档、删除与分叉
//! 只保留或复制有序 id，因此不存在回收项目资源的入口。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use vulcan_core::types::ConversationAttachment;
use vulcan_session_core::v1::{JS_SAFE_INTEGER_MAX, ResourceId};

use crate::workspace_store::session_v1::atomic_io::write_file_atomic;

mod source;
mod upload_receipt;

pub use source::{
    ProjectResourceSource, parse_project_resource_locator, project_resource_locator,
    resolve_project_resource_source,
};
pub use upload_receipt::ProjectResourceUploadReceipt;

/// File name of the project resource catalog.
/// 项目资源目录的文件名。
pub const RESOURCE_CATALOG_FILE: &str = "resource_catalog.json";

/// Fixed display placeholder for a missing project resource.
/// 项目资源缺失时使用的固定显示占位符。
pub const MISSING_RESOURCE_PLACEHOLDER: &str = "[MISSING PROJECT RESOURCE]";

/// Directory holding immutable resource entities by SHA-256.
/// 按 SHA-256 存放不可变资源实体的目录。
const RESOURCE_ENTITIES_DIR: &str = "resources";

/// One provider-external media binding of a project resource.
/// 一个项目资源的 Provider 外部媒体绑定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderBinding {
    /// Provider name that uploaded the media.
    /// 上传该媒体的 Provider 名。
    pub provider: String,
    /// Non-secret connection id used for the upload.
    /// 上传使用的非秘密连接 id。
    pub connection_id: String,
    /// External media id returned by the Provider.
    /// Provider 返回的外部媒体 id。
    pub media_id: String,
    /// Verified upload facts proving the delivery of this binding.
    /// 证明本绑定投递的已验证上传事实。
    pub upload_receipt: ProjectResourceUploadReceipt,
}

/// One catalog entry of a reusable project resource.
/// 一个可复用项目资源的目录条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceCatalogEntry {
    /// Project-scoped resource id.
    /// 项目级资源 id。
    pub resource_id: ResourceId,
    /// Original file name.
    /// 原始文件名。
    pub original_name: String,
    /// Optional MIME type.
    /// 可选 MIME 类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    /// Entity byte size.
    /// 实体字节大小。
    pub size: u64,
    /// SHA-256 hex digest of the entity content.
    /// 实体内容的 SHA-256 十六进制摘要。
    pub sha256: String,
    /// Canonical relative entity path inside the project resource area.
    /// 项目资源区域内的规范实体相对路径。
    pub entity_relative_path: String,
    /// Provider-external media bindings.
    /// Provider 外部媒体绑定。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_bindings: Vec<ProviderBinding>,
}

/// The complete project resource catalog.
/// 完整的项目资源目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectResourceCatalog {
    /// Catalog entries in insertion order.
    /// 按插入顺序排列的目录条目。
    pub entries: Vec<ResourceCatalogEntry>,
    /// Next project-scoped resource id.
    /// 下一个项目级资源 id。
    pub next_resource_id: u64,
}

/// One caller-resolved resource registration owned by a project catalog.
/// 一条由调用方完整解析、归项目资源目录所有的资源注册请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectResourceRegistration {
    /// Original display file name preserved by the logical resource.
    /// 逻辑资源保留的原始显示文件名。
    pub original_name: String,
    /// Optional authoritative MIME type.
    /// 可选的权威 MIME 类型。
    pub mime: Option<String>,
    /// Complete immutable entity bytes.
    /// 完整的不可变实体字节。
    pub content: Vec<u8>,
    /// Provider-external bindings already known for this resource.
    /// 该资源当前已知的 Provider 外部绑定。
    pub provider_bindings: Vec<ProviderBinding>,
}

/// Process-local owner registry for exact project resource catalogs.
/// 精确项目资源目录的进程内所有者注册表。
///
/// The registry mutex protects only owner lookup. Each project has an
/// independent mutex covering its complete load, resolve-or-add, and publish
/// transaction, so unrelated projects remain concurrent.
/// 注册表互斥锁只保护 owner 查找。每个项目拥有独立互斥锁，覆盖完整的加载、解析或新增与发布事务，
/// 因此无关项目仍可并发执行。
#[derive(Debug, Default)]
pub struct ProjectResourceCatalogRegistry {
    /// Stable project owners keyed by canonical absolute project directory.
    /// 按规范绝对项目目录索引的稳定项目 owner。
    owners: Mutex<BTreeMap<PathBuf, Arc<Mutex<ProjectResourceCatalogOwner>>>>,
}

/// Mutable state owned by one exact project catalog mutex.
/// 由一个精确项目目录互斥锁拥有的可变状态。
#[derive(Debug, Default)]
struct ProjectResourceCatalogOwner {
    /// Lazily loaded authoritative catalog retained for the process lifetime.
    /// 延迟加载并在进程生命周期内保留的权威目录。
    catalog: Option<ProjectResourceCatalog>,
}

impl Default for ProjectResourceCatalog {
    /// Creates one empty catalog whose first id is `1`.
    /// 创建一个首个 id 为 `1` 的空目录。
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            next_resource_id: 1,
        }
    }
}

/// One strict project-resource catalog failure.
/// 一条严格的项目资源目录失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectResourceCatalogError {
    /// The exact project directory cannot be prepared or normalized.
    /// 无法准备或规范化精确项目目录。
    ProjectDirectory {
        /// Rejected project directory.
        /// 被拒绝的项目目录。
        path: PathBuf,
        /// Failure detail.
        /// 失败详情。
        detail: String,
    },
    /// The process-local owner registry or project owner lock is poisoned.
    /// 进程内 owner 注册表或项目 owner 锁已中毒。
    OwnerPoisoned {
        /// Exact owner scope that could not be locked.
        /// 无法加锁的精确 owner 范围。
        path: PathBuf,
    },
    /// The catalog bytes could not be read or decoded.
    /// 无法读取或解析目录字节。
    CatalogRead {
        /// Exact catalog path.
        /// 精确目录路径。
        path: PathBuf,
        /// Failure detail.
        /// 失败详情。
        detail: String,
    },
    /// The catalog violates the V1 project-resource contract.
    /// 目录违反 V1 项目资源契约。
    InvalidCatalog {
        /// Failure detail.
        /// 失败详情。
        detail: String,
    },
    /// One catalog or entity publication failed.
    /// 某次目录或实体发布失败。
    Publish {
        /// Exact failed path.
        /// 精确失败路径。
        path: PathBuf,
        /// Failure detail.
        /// 失败详情。
        detail: String,
    },
    /// One existing or loaded entity violates its immutable digest contract.
    /// 某个既有或已加载实体违反不可变摘要契约。
    EntityCorrupt {
        /// Exact entity path.
        /// 精确实体路径。
        path: PathBuf,
        /// Failure detail.
        /// 失败详情。
        detail: String,
    },
    /// The project-scoped id space is exhausted.
    /// 项目级 id 空间已耗尽。
    ResourceIdExhausted,
    /// An exact reusable logical resource points to a missing entity.
    /// 一个可精确复用的逻辑资源指向缺失实体。
    ReusableResourceMissing {
        /// Missing logical resource id.
        /// 缺失的逻辑资源 ID。
        resource_id: ResourceId,
    },
    /// One authoritative managed attachment failed exact byte validation.
    /// 一条权威托管附件未通过精确字节校验。
    ManagedAttachment {
        /// Zero-based attachment position in caller order.
        /// 调用方顺序中的零基附件位置。
        attachment_index: usize,
        /// Display file name retained by the attachment contract.
        /// 附件契约保留的显示文件名。
        file_name: String,
        /// Exact validation failure detail.
        /// 精确校验失败详情。
        detail: String,
    },
}

impl fmt::Display for ProjectResourceCatalogError {
    /// Formats one stable catalog diagnostic.
    /// 格式化一条稳定的目录诊断。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProjectDirectory { path, detail } => write!(
                formatter,
                "project resource directory is invalid at {}: {detail}",
                path.display()
            ),
            Self::OwnerPoisoned { path } => write!(
                formatter,
                "project resource owner lock is poisoned for {}",
                path.display()
            ),
            Self::CatalogRead { path, detail } => write!(
                formatter,
                "project resource catalog read failed at {}: {detail}",
                path.display()
            ),
            Self::InvalidCatalog { detail } => {
                write!(formatter, "invalid project resource catalog: {detail}")
            }
            Self::Publish { path, detail } => write!(
                formatter,
                "project resource publish failed at {}: {detail}",
                path.display()
            ),
            Self::EntityCorrupt { path, detail } => write!(
                formatter,
                "project resource entity is corrupt at {}: {detail}",
                path.display()
            ),
            Self::ResourceIdExhausted => {
                write!(formatter, "project resource id space is exhausted")
            }
            Self::ReusableResourceMissing { resource_id } => write!(
                formatter,
                "reusable project resource {} has no available entity",
                resource_id.value()
            ),
            Self::ManagedAttachment {
                attachment_index,
                file_name,
                detail,
            } => write!(
                formatter,
                "managed attachment {attachment_index} (`{file_name}`) is invalid: {detail}"
            ),
        }
    }
}

impl std::error::Error for ProjectResourceCatalogError {}

/// Resolution result for one project resource reference.
/// 一条项目资源引用的解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectResourceResolution {
    /// The catalog entry and immutable entity bytes are available.
    /// 目录条目与不可变实体字节均可用。
    Available {
        /// Exact catalog entry.
        /// 精确目录条目。
        entry: ResourceCatalogEntry,
        /// Complete entity bytes.
        /// 完整实体字节。
        bytes: Vec<u8>,
    },
    /// The catalog entry or its entity is missing; session loading remains valid.
    /// 目录条目或实体缺失；会话加载仍然有效。
    Missing {
        /// Missing project-scoped resource id.
        /// 缺失的项目级资源 id。
        resource_id: ResourceId,
        /// Fixed display placeholder.
        /// 固定显示占位符。
        placeholder: &'static str,
    },
}

/// Returns the absolute catalog path of one project.
/// 返回一个项目的目录绝对路径。
pub fn project_resource_catalog_path(project_dir: &Path) -> PathBuf {
    project_dir.join(RESOURCE_CATALOG_FILE)
}

/// Loads and validates one project catalog without scanning any directory.
/// 加载并校验一个项目目录，不扫描任何目录。
pub fn load_project_resource_catalog(
    project_dir: &Path,
) -> Result<ProjectResourceCatalog, ProjectResourceCatalogError> {
    let path = project_resource_catalog_path(project_dir);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ProjectResourceCatalog::default());
        }
        Err(error) => {
            return Err(ProjectResourceCatalogError::CatalogRead {
                path,
                detail: error.to_string(),
            });
        }
    };
    let catalog = serde_json::from_slice(&bytes).map_err(|error| {
        ProjectResourceCatalogError::CatalogRead {
            path,
            detail: error.to_string(),
        }
    })?;
    validate_project_resource_catalog(&catalog)?;
    Ok(catalog)
}

impl ProjectResourceCatalogRegistry {
    /// Validates and registers authoritative managed attachments in exact caller order.
    /// 校验并按精确调用方顺序注册权威托管附件。
    ///
    /// # Parameters / 参数
    ///
    /// - `project_dir`: Exact absolute V1 project partition directory.
    ///   `project_dir`：精确的 V1 项目分区绝对目录。
    /// - `attachments`: Canonical managed attachments whose byte length and SHA-256 are mandatory.
    ///   `attachments`：字节长度与 SHA-256 均为必填的规范托管附件。
    ///
    /// # Returns / 返回值
    ///
    /// Project-scoped ids preserving attachment order, or the first exact validation/catalog error.
    /// 保持附件顺序的项目级 ID，或首个精确校验/目录错误。
    pub fn resolve_or_register_managed_attachments(
        &self,
        project_dir: &Path,
        attachments: &[ConversationAttachment],
    ) -> Result<Vec<ResourceId>, ProjectResourceCatalogError> {
        let mut registrations = Vec::with_capacity(attachments.len());
        for (attachment_index, attachment) in attachments.iter().enumerate() {
            let invalid = |detail: String| ProjectResourceCatalogError::ManagedAttachment {
                attachment_index,
                file_name: attachment.file_name.clone(),
                detail,
            };
            // content is read only from the already-authorized managed path retained by the producer.
            // content 只从生产者保留的已授权托管路径读取。
            let content = fs::read(Path::new(&attachment.local_path))
                .map_err(|error| invalid(error.to_string()))?;
            let expected_len = attachment.byte_len.ok_or_else(|| {
                invalid("authoritative attachment is missing byte_len".to_string())
            })?;
            if content.len() as u64 != expected_len {
                return Err(invalid(format!(
                    "managed byte length changed: expected {expected_len}, actual {}",
                    content.len()
                )));
            }
            let expected_sha256 = attachment.source_sha256.as_deref().ok_or_else(|| {
                invalid("authoritative attachment is missing source_sha256".to_string())
            })?;
            let actual_sha256 = sha256_hex(&content);
            if actual_sha256 != expected_sha256 {
                return Err(invalid(format!(
                    "managed digest changed: expected {expected_sha256}, actual {actual_sha256}"
                )));
            }
            registrations.push(ProjectResourceRegistration {
                original_name: attachment.file_name.clone(),
                mime: Some(attachment.mime_type.clone()),
                content,
                provider_bindings: Vec::new(),
            });
        }
        self.resolve_or_register_project_resources(project_dir, &registrations)
    }

    /// Resolves or registers resources in caller order under one project owner.
    /// 在单个项目 owner 下按调用方顺序解析或注册资源。
    ///
    /// Logical identity is the exact tuple `(sha256, original_name, mime)`.
    /// Equal bytes therefore share one immutable entity, while a different
    /// display name or MIME receives a distinct logical id. Successfully
    /// published resources remain project-owned if a later item fails.
    /// 逻辑身份是精确三元组 `(sha256, original_name, mime)`。因此相同字节共享同一不可变实体，
    /// 而不同显示名或 MIME 会获得独立逻辑 ID。后续条目失败时，已成功发布的资源仍归项目永久所有。
    ///
    /// # Parameters / 参数
    ///
    /// - `project_dir`: Exact absolute V1 project partition directory.
    ///   `project_dir`：精确的 V1 项目分区绝对目录。
    /// - `registrations`: Fully resolved resources in root-input order.
    ///   `registrations`：按根输入顺序排列的完整已解析资源。
    ///
    /// # Returns / 返回值
    ///
    /// Project-scoped ids preserving the input order, or the first explicit
    /// directory, catalog, entity, or publication failure.
    /// 保持输入顺序的项目级 ID，或首个显式目录、目录文件、实体或发布失败。
    pub fn resolve_or_register_project_resources(
        &self,
        project_dir: &Path,
        registrations: &[ProjectResourceRegistration],
    ) -> Result<Vec<ResourceId>, ProjectResourceCatalogError> {
        // canonical_project_dir is both the on-disk target and process owner key.
        // canonical_project_dir 同时是磁盘目标与进程内 owner 键。
        let canonical_project_dir = prepare_exact_project_directory(project_dir)?;
        // owner serializes the complete mutation transaction for this exact project only.
        // owner 只串行化该精确项目的完整变更事务。
        let owner = self.project_owner(&canonical_project_dir)?;
        // owner_guard excludes stale load-clone-publish races across concurrent sessions.
        // owner_guard 排除并发会话之间陈旧的加载、克隆与发布竞态。
        let mut owner_guard =
            owner
                .lock()
                .map_err(|_| ProjectResourceCatalogError::OwnerPoisoned {
                    path: canonical_project_dir.clone(),
                })?;
        if owner_guard.catalog.is_none() {
            owner_guard.catalog = Some(load_project_resource_catalog(&canonical_project_dir)?);
        }
        let catalog = owner_guard.catalog.as_mut().ok_or_else(|| {
            ProjectResourceCatalogError::InvalidCatalog {
                detail: "project resource owner lost its loaded catalog".to_string(),
            }
        })?;
        // resource_ids preserves exact caller order, including repeated logical resources.
        // resource_ids 保留精确调用方顺序，包括重复逻辑资源。
        let mut resource_ids = Vec::with_capacity(registrations.len());
        for registration in registrations {
            // resource_id is either the exact reusable logical id or one newly published id.
            // resource_id 是精确复用的逻辑 ID，或一个新发布的 ID。
            let resource_id =
                resolve_or_add_registration(&canonical_project_dir, catalog, registration)?;
            resource_ids.push(resource_id);
        }
        Ok(resource_ids)
    }

    /// Returns the unique process-local owner for one canonical project path.
    /// 返回一个规范项目路径的唯一进程内 owner。
    ///
    /// # Parameters / 参数
    ///
    /// - `canonical_project_dir`: Existing canonical absolute project directory.
    ///   `canonical_project_dir`：已存在的规范项目绝对目录。
    ///
    /// # Returns / 返回值
    ///
    /// The stable per-project owner mutex.
    /// 稳定的项目级 owner 互斥锁。
    fn project_owner(
        &self,
        canonical_project_dir: &Path,
    ) -> Result<Arc<Mutex<ProjectResourceCatalogOwner>>, ProjectResourceCatalogError> {
        // owners is held only while selecting or creating one independent project lock.
        // owners 仅在选择或创建一个独立项目锁期间持有。
        let mut owners =
            self.owners
                .lock()
                .map_err(|_| ProjectResourceCatalogError::OwnerPoisoned {
                    path: canonical_project_dir.to_path_buf(),
                })?;
        Ok(Arc::clone(
            owners
                .entry(canonical_project_dir.to_path_buf())
                .or_insert_with(|| Arc::new(Mutex::new(ProjectResourceCatalogOwner::default()))),
        ))
    }
}

/// Prepares and canonicalizes one exact project directory without scanning.
/// 在不扫描目录的前提下准备并规范化一个精确项目目录。
///
/// # Parameters / 参数
///
/// - `project_dir`: Absolute directory selected by the V1 project locator.
///   `project_dir`：由 V1 项目定位器选定的绝对目录。
///
/// # Returns / 返回值
///
/// The canonical absolute owner key.
/// 规范化后的绝对 owner 键。
fn prepare_exact_project_directory(
    project_dir: &Path,
) -> Result<PathBuf, ProjectResourceCatalogError> {
    if !project_dir.is_absolute() {
        return Err(ProjectResourceCatalogError::ProjectDirectory {
            path: project_dir.to_path_buf(),
            detail: "project resource directory must be absolute".to_string(),
        });
    }
    fs::create_dir_all(project_dir).map_err(|error| {
        ProjectResourceCatalogError::ProjectDirectory {
            path: project_dir.to_path_buf(),
            detail: error.to_string(),
        }
    })?;
    // canonical_project_dir prevents path aliases from creating duplicate owners.
    // canonical_project_dir 防止路径别名创建重复 owner。
    let canonical_project_dir = fs::canonicalize(project_dir).map_err(|error| {
        ProjectResourceCatalogError::ProjectDirectory {
            path: project_dir.to_path_buf(),
            detail: error.to_string(),
        }
    })?;
    if !canonical_project_dir.is_dir() {
        return Err(ProjectResourceCatalogError::ProjectDirectory {
            path: canonical_project_dir,
            detail: "project resource path is not a directory".to_string(),
        });
    }
    Ok(canonical_project_dir)
}

/// Resolves one exact logical identity or publishes one new logical resource.
/// 解析一个精确逻辑身份，或发布一个新的逻辑资源。
///
/// # Parameters / 参数
///
/// - `project_dir`: Canonical project owner directory.
///   `project_dir`：规范项目 owner 目录。
/// - `catalog`: Authoritative catalog held under the project mutex.
///   `catalog`：在项目互斥锁下持有的权威目录。
/// - `registration`: Caller-resolved resource facts and bytes.
///   `registration`：调用方已解析的资源事实与字节。
///
/// # Returns / 返回值
///
/// The reused or newly allocated project-scoped id.
/// 复用或新分配的项目级 ID。
fn resolve_or_add_registration(
    project_dir: &Path,
    catalog: &mut ProjectResourceCatalog,
    registration: &ProjectResourceRegistration,
) -> Result<ResourceId, ProjectResourceCatalogError> {
    // digest is the immutable physical entity identity.
    // digest 是不可变物理实体身份。
    let digest = sha256_hex(&registration.content);
    // reusable_index applies the exact logical identity tuple without fallbacks.
    // reusable_index 仅应用精确逻辑身份三元组，不使用兜底字段。
    let reusable_index = catalog.entries.iter().position(|entry| {
        entry.sha256 == digest
            && entry.original_name == registration.original_name
            && entry.mime == registration.mime
    });
    if let Some(index) = reusable_index {
        // resource_id belongs to the matching logical row and cannot be reallocated.
        // resource_id 属于匹配的逻辑行，不能重新分配。
        let resource_id = catalog.entries[index].resource_id;
        match resolve_project_resource(project_dir, catalog, resource_id)? {
            ProjectResourceResolution::Available { bytes, .. }
                if bytes.as_slice() == registration.content.as_slice() => {}
            ProjectResourceResolution::Available { .. } => {
                return Err(ProjectResourceCatalogError::EntityCorrupt {
                    path: canonical_entity_path(project_dir, &digest),
                    detail: "reusable logical resource bytes differ from their digest identity"
                        .to_string(),
                });
            }
            ProjectResourceResolution::Missing { .. } => {
                return Err(ProjectResourceCatalogError::ReusableResourceMissing { resource_id });
            }
        }
        merge_provider_bindings(project_dir, catalog, index, &registration.provider_bindings)?;
        return Ok(resource_id);
    }

    add_project_resource(
        project_dir,
        catalog,
        &registration.original_name,
        registration.mime.clone(),
        &registration.content,
        registration.provider_bindings.clone(),
    )
}

/// Merges newly observed provider bindings into one reusable logical resource.
/// 将新观察到的 Provider 绑定合并到一个可复用逻辑资源。
///
/// # Parameters / 参数
///
/// - `project_dir`: Canonical project owner directory.
///   `project_dir`：规范项目 owner 目录。
/// - `catalog`: Authoritative catalog held under the project mutex.
///   `catalog`：在项目互斥锁下持有的权威目录。
/// - `entry_index`: Exact reusable catalog entry index.
///   `entry_index`：可复用目录条目的精确索引。
/// - `bindings`: Newly observed non-secret provider bindings.
///   `bindings`：新观察到的非敏感 Provider 绑定。
///
/// # Returns / 返回值
///
/// Success after either no-op reuse or one atomic catalog publication.
/// 在无变化复用或一次原子目录发布后返回成功。
fn merge_provider_bindings(
    project_dir: &Path,
    catalog: &mut ProjectResourceCatalog,
    entry_index: usize,
    bindings: &[ProviderBinding],
) -> Result<(), ProjectResourceCatalogError> {
    // merged retains stable first-observed binding order and removes exact duplicates.
    // merged 保持首次观察到的绑定顺序，并移除精确重复项。
    let mut merged = catalog.entries[entry_index].provider_bindings.clone();
    for binding in bindings {
        if !merged.contains(binding) {
            merged.push(binding.clone());
        }
    }
    if merged == catalog.entries[entry_index].provider_bindings {
        return Ok(());
    }
    // candidate is published before replacing the owner-held catalog.
    // candidate 在替换 owner 持有的目录前完成发布。
    let mut candidate = catalog.clone();
    candidate.entries[entry_index].provider_bindings = merged;
    publish_project_resource_catalog(project_dir, &candidate)?;
    *catalog = candidate;
    Ok(())
}

/// Adds one immutable entity and atomically publishes its catalog entry.
/// 添加一个不可变实体并原子发布其目录条目。
///
/// The entity is published first. The caller's in-memory catalog is replaced
/// only after the catalog file succeeds, so a failed publication cannot expose
/// an entry that was never durable.
/// 实体先发布。只有目录文件成功后才替换调用方的内存目录，因此失败发布不会暴露
/// 从未持久化的条目。
pub fn add_project_resource(
    project_dir: &Path,
    catalog: &mut ProjectResourceCatalog,
    original_name: &str,
    mime: Option<String>,
    content: &[u8],
    provider_bindings: Vec<ProviderBinding>,
) -> Result<ResourceId, ProjectResourceCatalogError> {
    validate_project_resource_catalog(catalog)?;
    let resource_id = ResourceId::try_new(catalog.next_resource_id)
        .map_err(|_| ProjectResourceCatalogError::ResourceIdExhausted)?;
    let next_resource_id = catalog
        .next_resource_id
        .checked_add(1)
        .filter(|value| *value <= JS_SAFE_INTEGER_MAX)
        .ok_or(ProjectResourceCatalogError::ResourceIdExhausted)?;
    let sha256 = sha256_hex(content);
    let entity_path = canonical_entity_path(project_dir, &sha256);
    match fs::read(&entity_path) {
        Ok(existing) if existing != content => {
            return Err(ProjectResourceCatalogError::EntityCorrupt {
                path: entity_path,
                detail: "existing entity bytes do not match their SHA-256 address".to_string(),
            });
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_file_atomic(&entity_path, content, "project resource entity").map_err(
                |error| ProjectResourceCatalogError::Publish {
                    path: entity_path.clone(),
                    detail: error.to_string(),
                },
            )?;
        }
        Err(error) => {
            return Err(ProjectResourceCatalogError::CatalogRead {
                path: entity_path,
                detail: error.to_string(),
            });
        }
    }

    let mut candidate = catalog.clone();
    candidate.entries.push(ResourceCatalogEntry {
        resource_id,
        original_name: original_name.to_string(),
        mime,
        size: content.len() as u64,
        sha256: sha256.clone(),
        entity_relative_path: canonical_entity_relative_path(&sha256),
        provider_bindings,
    });
    candidate.next_resource_id = next_resource_id;
    publish_project_resource_catalog(project_dir, &candidate)?;
    *catalog = candidate;
    Ok(resource_id)
}

/// Resolves one project resource by its exact catalog id.
/// 按精确目录 id 解析一个项目资源。
///
/// A missing catalog entry or entity returns the same fixed `Missing` state and
/// never mutates the catalog. Corrupt bytes remain an explicit diagnostic.
/// 缺失目录条目或实体返回相同的固定 `Missing` 状态且绝不修改目录；损坏字节仍为
/// 显式诊断。
pub fn resolve_project_resource(
    project_dir: &Path,
    catalog: &ProjectResourceCatalog,
    resource_id: ResourceId,
) -> Result<ProjectResourceResolution, ProjectResourceCatalogError> {
    // Resolve metadata and containment before reading; a changed source cannot force an unbounded allocation.
    // 先解析元数据及包含关系再读取；变化来源不能迫使无界内存分配。
    let Some(source) = resolve_project_resource_source(project_dir, catalog, resource_id)? else {
        return Ok(missing_resource(resource_id));
    };
    // The catalog size is exact, and the bounded reader verifies the final immutable digest.
    // 目录大小是精确值，有界读取器验证最终不可变摘要。
    let bytes = source.read_bounded(source.entry.size)?;
    Ok(ProjectResourceResolution::Available {
        entry: source.entry,
        bytes,
    })
}

/// Atomically publishes one already validated project resource catalog.
/// 原子发布一个已经校验的项目资源目录。
fn publish_project_resource_catalog(
    project_dir: &Path,
    catalog: &ProjectResourceCatalog,
) -> Result<(), ProjectResourceCatalogError> {
    validate_project_resource_catalog(catalog)?;
    let path = project_resource_catalog_path(project_dir);
    let bytes = serde_json::to_vec(catalog).map_err(|error| {
        ProjectResourceCatalogError::InvalidCatalog {
            detail: format!("catalog serialization failed: {error}"),
        }
    })?;
    write_file_atomic(&path, &bytes, "project resource catalog").map_err(|error| {
        ProjectResourceCatalogError::Publish {
            path,
            detail: error.to_string(),
        }
    })
}

/// Validates id continuity and every exact digest-derived entity locator.
/// 校验 id 连续性与每个由摘要派生的精确实体定位。
fn validate_project_resource_catalog(
    catalog: &ProjectResourceCatalog,
) -> Result<(), ProjectResourceCatalogError> {
    if !(1..=JS_SAFE_INTEGER_MAX).contains(&catalog.next_resource_id) {
        return Err(ProjectResourceCatalogError::InvalidCatalog {
            detail: format!(
                "next_resource_id {} is outside the JSON-safe range",
                catalog.next_resource_id
            ),
        });
    }
    let mut ids = BTreeSet::new();
    for (index, entry) in catalog.entries.iter().enumerate() {
        if !ids.insert(entry.resource_id) {
            return Err(ProjectResourceCatalogError::InvalidCatalog {
                detail: format!("duplicate resource_id {}", entry.resource_id.value()),
            });
        }
        let expected_id = (index as u64) + 1;
        if entry.resource_id.value() != expected_id {
            return Err(ProjectResourceCatalogError::InvalidCatalog {
                detail: format!(
                    "resource_id sequence mismatch at index {index}: expected {expected_id}, actual {}",
                    entry.resource_id.value()
                ),
            });
        }
        if !is_lowercase_sha256(&entry.sha256) {
            return Err(ProjectResourceCatalogError::InvalidCatalog {
                detail: format!(
                    "resource_id {} has invalid sha256",
                    entry.resource_id.value()
                ),
            });
        }
        for binding in &entry.provider_bindings {
            binding.validate_upload_receipt()?;
            // Optional catalog MIME uses the same octet-stream default as the canonical V1 resource resolver.
            // 可选目录MIME使用与规范V1资源解析器相同的二进制流默认值。
            if binding.upload_receipt.mime_type
                != entry.mime.as_deref().unwrap_or("application/octet-stream")
            {
                return Err(ProjectResourceCatalogError::InvalidCatalog {
                    detail: "upload receipt MIME differs from its project resource".to_string(),
                });
            }
        }
        upload_receipt::validate_draft_intent_receipts(&entry.provider_bindings)?;
        let expected_path = canonical_entity_relative_path(&entry.sha256);
        if entry.entity_relative_path != expected_path {
            return Err(ProjectResourceCatalogError::InvalidCatalog {
                detail: format!(
                    "resource_id {} entity path mismatch: expected {expected_path}, actual {}",
                    entry.resource_id.value(),
                    entry.entity_relative_path
                ),
            });
        }
    }
    let expected_next = (catalog.entries.len() as u64)
        .checked_add(1)
        .ok_or(ProjectResourceCatalogError::ResourceIdExhausted)?;
    if catalog.next_resource_id != expected_next {
        return Err(ProjectResourceCatalogError::InvalidCatalog {
            detail: format!(
                "next_resource_id mismatch: expected {expected_next}, actual {}",
                catalog.next_resource_id
            ),
        });
    }
    Ok(())
}

/// Returns the exact entity path derived only from one validated digest.
/// 仅根据一个已校验摘要返回精确实体路径。
fn canonical_entity_path(project_dir: &Path, sha256: &str) -> PathBuf {
    project_dir.join(RESOURCE_ENTITIES_DIR).join(sha256)
}

/// Returns the canonical slash-separated entity path stored in the catalog.
/// 返回目录中保存的规范斜杠分隔实体路径。
fn canonical_entity_relative_path(sha256: &str) -> String {
    format!("{RESOURCE_ENTITIES_DIR}/{sha256}")
}

/// Returns whether one string is exactly one lowercase SHA-256 digest.
/// 返回一个字符串是否恰好为小写 SHA-256 摘要。
fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Builds the fixed non-blocking missing-resource state.
/// 构建固定且不阻断会话的资源缺失状态。
fn missing_resource(resource_id: ResourceId) -> ProjectResourceResolution {
    ProjectResourceResolution::Missing {
        resource_id,
        placeholder: MISSING_RESOURCE_PLACEHOLDER,
    }
}

/// Computes the SHA-256 hex digest of one byte slice.
/// 计算一个字节切片的 SHA-256 十六进制摘要。
fn sha256_hex(content: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_core::types::ConversationAttachmentKind;

    /// Adding a resource publishes immutable bytes and then the catalog.
    /// 添加资源会先发布不可变字节，再发布目录。
    #[test]
    fn add_resource_publishes_entity_then_catalog() {
        let temporary = tempfile::tempdir().expect("temporary project root");
        let mut catalog = ProjectResourceCatalog::default();
        let resource_id = add_project_resource(
            temporary.path(),
            &mut catalog,
            "photo.png",
            Some("image/png".to_string()),
            b"binary-content",
            Vec::new(),
        )
        .expect("publish resource");
        let loaded = load_project_resource_catalog(temporary.path()).expect("load catalog");
        assert_eq!(resource_id.value(), 1);
        assert_eq!(loaded, catalog);
        assert!(matches!(
            resolve_project_resource(temporary.path(), &loaded, resource_id),
            Ok(ProjectResourceResolution::Available { bytes, .. }) if bytes == b"binary-content"
        ));
    }

    /// Missing catalog rows and missing entity files share one fixed state.
    /// 缺失目录记录与缺失实体文件共享同一个固定状态。
    #[test]
    fn missing_resources_return_fixed_non_blocking_state() {
        let temporary = tempfile::tempdir().expect("temporary project root");
        let mut catalog = ProjectResourceCatalog::default();
        let resource_id = add_project_resource(
            temporary.path(),
            &mut catalog,
            "a.bin",
            None,
            b"data",
            Vec::new(),
        )
        .expect("publish resource");
        let absent_id = ResourceId::try_new(2).expect("valid missing id");
        assert_eq!(
            resolve_project_resource(temporary.path(), &catalog, absent_id)
                .expect("resolve missing row"),
            ProjectResourceResolution::Missing {
                resource_id: absent_id,
                placeholder: MISSING_RESOURCE_PLACEHOLDER,
            }
        );
        let entry = &catalog.entries[0];
        fs::remove_file(canonical_entity_path(temporary.path(), &entry.sha256))
            .expect("remove entity fixture");
        assert_eq!(
            resolve_project_resource(temporary.path(), &catalog, resource_id)
                .expect("resolve missing entity"),
            ProjectResourceResolution::Missing {
                resource_id,
                placeholder: MISSING_RESOURCE_PLACEHOLDER,
            }
        );
    }

    /// Archive, delete, and fork semantics retain the same project resource id.
    /// 归档、删除与分叉语义保留同一个项目资源 id。
    #[test]
    fn session_lifecycle_has_no_project_resource_reclamation_path() {
        let temporary = tempfile::tempdir().expect("temporary project root");
        let mut catalog = ProjectResourceCatalog::default();
        let resource_id = add_project_resource(
            temporary.path(),
            &mut catalog,
            "keep.bin",
            None,
            b"keep",
            Vec::new(),
        )
        .expect("publish resource");
        let source_session_refs = vec![resource_id];
        let forked_session_refs = source_session_refs.clone();
        drop(source_session_refs);
        assert_eq!(forked_session_refs, vec![resource_id]);
        assert!(matches!(
            resolve_project_resource(temporary.path(), &catalog, resource_id),
            Ok(ProjectResourceResolution::Available { .. })
        ));
    }

    /// A catalog publication failure leaves the caller's in-memory catalog unchanged.
    /// 目录发布失败时，调用方内存目录保持不变。
    #[test]
    fn failed_catalog_publish_does_not_mutate_memory() {
        let temporary = tempfile::tempdir().expect("temporary project root");
        fs::create_dir(project_resource_catalog_path(temporary.path()))
            .expect("occupy catalog path with directory");
        let mut catalog = ProjectResourceCatalog::default();
        let before = catalog.clone();
        assert!(
            add_project_resource(
                temporary.path(),
                &mut catalog,
                "a.bin",
                None,
                b"data",
                Vec::new(),
            )
            .is_err()
        );
        assert_eq!(catalog, before);
    }

    /// Builds one deterministic registration fixture.
    /// 构建一条确定性的资源注册夹具。
    ///
    /// # Parameters / 参数
    ///
    /// - `name`: Logical display name.
    ///   `name`：逻辑显示名称。
    /// - `mime`: Optional logical MIME type.
    ///   `mime`：可选逻辑 MIME 类型。
    /// - `content`: Immutable entity bytes.
    ///   `content`：不可变实体字节。
    ///
    /// # Returns / 返回值
    ///
    /// One owned registration with no provider binding.
    /// 一条不含 Provider 绑定的自有注册请求。
    fn registration(name: &str, mime: Option<&str>, content: &[u8]) -> ProjectResourceRegistration {
        ProjectResourceRegistration {
            original_name: name.to_string(),
            mime: mime.map(str::to_string),
            content: content.to_vec(),
            provider_bindings: Vec::new(),
        }
    }

    /// Builds one authoritative managed-attachment fixture for exact registration tests.
    /// 为精确注册测试构建一条权威托管附件夹具。
    ///
    /// # Parameters / 参数
    ///
    /// - `path`: Exact managed file path.
    ///   `path`：精确托管文件路径。
    /// - `name`: Stable display file name.
    ///   `name`：稳定显示文件名。
    /// - `content`: Bytes frozen by the attachment identity.
    ///   `content`：由附件身份冻结的字节。
    ///
    /// # Returns / 返回值
    ///
    /// One image attachment with mandatory length and SHA-256 identity.
    /// 一条带必填长度与 SHA-256 身份的图片附件。
    fn managed_attachment(path: &Path, name: &str, content: &[u8]) -> ConversationAttachment {
        ConversationAttachment {
            kind: ConversationAttachmentKind::Image,
            file_name: name.to_string(),
            local_path: path.to_string_lossy().into_owned(),
            resource_path: None,
            source_path: None,
            asset_id: Some(format!("asset-{name}")),
            byte_len: Some(content.len() as u64),
            source_sha256: Some(sha256_hex(content)),
            delivery_mode: None,
            preview_local_path: None,
            mime_type: String::from("image/png"),
        }
    }

    /// Managed attachments preserve caller order and exact logical-resource reuse.
    /// 托管附件保留调用方顺序与精确逻辑资源复用。
    #[test]
    fn managed_attachments_preserve_order_and_reuse() {
        let temporary = tempfile::tempdir().expect("temporary resource root");
        let managed_a = temporary.path().join("managed-a.png");
        let managed_b = temporary.path().join("managed-b.png");
        fs::write(&managed_a, b"image-a").expect("write first managed attachment");
        fs::write(&managed_b, b"image-b").expect("write second managed attachment");
        let attachment_a = managed_attachment(&managed_a, "a.png", b"image-a");
        let attachment_b = managed_attachment(&managed_b, "b.png", b"image-b");
        let project_dir = temporary.path().join("project");
        let registry = ProjectResourceCatalogRegistry::default();

        let resource_ids = registry
            .resolve_or_register_managed_attachments(
                &project_dir,
                &[attachment_a.clone(), attachment_b, attachment_a],
            )
            .expect("register ordered managed attachments");
        let catalog = load_project_resource_catalog(&project_dir).expect("load project catalog");

        assert_eq!(resource_ids[0].value(), 1);
        assert_eq!(resource_ids[1].value(), 2);
        assert_eq!(resource_ids[2], resource_ids[0]);
        assert_eq!(catalog.entries.len(), 2);
    }

    /// Validation failure before owner admission consumes no project resource id.
    /// owner 准入前的校验失败不会消耗任何项目资源 ID。
    #[test]
    fn managed_attachment_validation_failure_consumes_no_id() {
        let temporary = tempfile::tempdir().expect("temporary resource root");
        let managed_a = temporary.path().join("managed-a.png");
        let managed_b = temporary.path().join("managed-b.png");
        fs::write(&managed_a, b"image-a").expect("write first managed attachment");
        fs::write(&managed_b, b"changed").expect("write drifted managed attachment");
        let attachment_a = managed_attachment(&managed_a, "a.png", b"image-a");
        let drifted_attachment = managed_attachment(&managed_b, "b.png", b"original");
        let project_dir = temporary.path().join("project");
        let registry = ProjectResourceCatalogRegistry::default();

        assert!(matches!(
            registry.resolve_or_register_managed_attachments(
                &project_dir,
                &[attachment_a.clone(), drifted_attachment]
            ),
            Err(ProjectResourceCatalogError::ManagedAttachment {
                attachment_index: 1,
                ..
            })
        ));
        assert!(!project_dir.exists());

        let resource_ids = registry
            .resolve_or_register_managed_attachments(&project_dir, &[attachment_a])
            .expect("register after failed validation");
        assert_eq!(resource_ids[0].value(), 1);
    }

    /// Concurrent sessions preserve both resources under one project owner.
    /// 并发会话会在同一个项目 owner 下保留两份资源。
    #[test]
    fn project_owner_serializes_concurrent_distinct_registrations() {
        use std::sync::{Arc, Barrier};

        let temporary = tempfile::tempdir().expect("temporary project root");
        let project_dir = temporary.path().join("code/sessions/project-a");
        let registry = Arc::new(ProjectResourceCatalogRegistry::default());
        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();
        for (name, content) in [
            ("first.bin", b"first".as_slice()),
            ("second.bin", b"second"),
        ] {
            let registry = Arc::clone(&registry);
            let barrier = Arc::clone(&barrier);
            let project_dir = project_dir.clone();
            let registration = registration(name, None, content);
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                registry
                    .resolve_or_register_project_resources(&project_dir, &[registration])
                    .expect("concurrent project registration")[0]
            }));
        }
        barrier.wait();
        let mut ids = workers
            .into_iter()
            .map(|worker| worker.join().expect("registration worker"))
            .collect::<Vec<_>>();
        ids.sort_by_key(|resource_id| resource_id.value());

        let catalog = load_project_resource_catalog(&project_dir).expect("load concurrent catalog");
        assert_eq!(
            ids.iter()
                .map(|resource_id| resource_id.value())
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(catalog.entries.len(), 2);
        assert_eq!(catalog.next_resource_id, 3);
        assert!(
            catalog
                .entries
                .iter()
                .any(|entry| entry.original_name == "first.bin")
        );
        assert!(
            catalog
                .entries
                .iter()
                .any(|entry| entry.original_name == "second.bin")
        );
    }

    /// Exact logical identities reuse ids while equal bytes share one entity.
    /// 精确逻辑身份复用 ID，而相同字节共享同一个实体。
    #[test]
    fn logical_identity_and_content_entity_reuse_are_distinct_contracts() {
        let temporary = tempfile::tempdir().expect("temporary project root");
        let project_dir = temporary.path().join("code/sessions/project-b");
        let registry = ProjectResourceCatalogRegistry::default();
        let exact = registration("photo.png", Some("image/png"), b"same-bytes");
        let renamed = registration("renamed.png", Some("image/png"), b"same-bytes");
        let ids = registry
            .resolve_or_register_project_resources(&project_dir, &[exact.clone(), exact, renamed])
            .expect("register reusable logical resources");

        let catalog = load_project_resource_catalog(&project_dir).expect("load reuse catalog");
        assert_eq!(ids[0], ids[1]);
        assert_ne!(ids[0], ids[2]);
        assert_eq!(catalog.entries.len(), 2);
        assert_eq!(catalog.entries[0].sha256, catalog.entries[1].sha256);
        assert_eq!(
            catalog.entries[0].entity_relative_path,
            catalog.entries[1].entity_relative_path
        );
        assert!(canonical_entity_path(&project_dir, &catalog.entries[0].sha256).is_file());
    }

    /// A failed catalog publication does not consume the next logical id.
    /// 目录发布失败不会消耗下一个逻辑 ID。
    #[test]
    fn failed_owner_publication_preserves_contiguous_next_id() {
        let temporary = tempfile::tempdir().expect("temporary project root");
        let project_dir = temporary.path().join("code/sessions/project-c");
        let registry = ProjectResourceCatalogRegistry::default();
        let first_ids = registry
            .resolve_or_register_project_resources(
                &project_dir,
                &[registration("first.bin", None, b"first")],
            )
            .expect("publish first resource");
        assert_eq!(first_ids[0].value(), 1);

        let catalog_path = project_resource_catalog_path(&project_dir);
        fs::remove_file(&catalog_path).expect("remove catalog before failure injection");
        fs::create_dir(&catalog_path).expect("occupy catalog path with directory");
        let failed = registry.resolve_or_register_project_resources(
            &project_dir,
            &[registration("second.bin", None, b"second")],
        );
        assert!(matches!(
            failed,
            Err(ProjectResourceCatalogError::Publish { .. })
        ));

        fs::remove_dir(&catalog_path).expect("release catalog failure injection");
        let retry_ids = registry
            .resolve_or_register_project_resources(
                &project_dir,
                &[registration("second.bin", None, b"second")],
            )
            .expect("retry second resource");
        let catalog = load_project_resource_catalog(&project_dir).expect("load retried catalog");
        assert_eq!(retry_ids[0].value(), 2);
        assert_eq!(catalog.entries.len(), 2);
        assert_eq!(catalog.next_resource_id, 3);
    }

    /// A tampered relative path is rejected instead of being joined to the project root.
    /// 被篡改的相对路径会被拒绝，而不是拼接到项目根目录。
    #[test]
    fn tampered_entity_path_is_rejected() {
        let temporary = tempfile::tempdir().expect("temporary project root");
        let mut catalog = ProjectResourceCatalog::default();
        add_project_resource(
            temporary.path(),
            &mut catalog,
            "a.bin",
            None,
            b"data",
            Vec::new(),
        )
        .expect("publish resource");
        catalog.entries[0].entity_relative_path = "../outside".to_string();
        assert!(matches!(
            resolve_project_resource(temporary.path(), &catalog, catalog.entries[0].resource_id),
            Err(ProjectResourceCatalogError::InvalidCatalog { .. })
        ));
    }
}
