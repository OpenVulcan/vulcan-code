//! Controller-backed native VMM data-plane bootstrap and schema.
//! 基于控制器的原生 VMM 数据平面引导与数据结构。

use crate::data_plane::ControllerRuntimeHandle;
use crate::runtime_assets;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use vldb_controller_client::{
    ControllerClient, ControllerLanceDbColumnDef, ControllerLanceDbColumnType,
    ControllerLanceDbEnableRequest, ControllerLanceDbInputFormat, ControllerLanceDbOutputFormat,
    ControllerSqliteEnableRequest, ControllerSqliteValue, SpaceKind, SpaceRegistration,
};

/// Stable controller space identifier reserved for native VMM data.
/// 为原生 VMM 数据保留的稳定控制器空间标识符。
pub const VMM_SPACE_ID: &str = "vmm";

/// Stable SQLite binding tag reserved for native VMM relational state.
/// 为原生 VMM 关系状态保留的稳定 SQLite 绑定标签。
pub const VMM_SQLITE_BINDING_TAG: &str = "vmm-state";

/// Stable LanceDB binding tag reserved for native VMM vectors.
/// 为原生 VMM 向量保留的稳定 LanceDB 绑定标签。
pub const VMM_LANCEDB_BINDING_TAG: &str = "vmm-memory";

/// Stable identifier of the protected fallback VMM project.
/// 受保护兜底 VMM 项目的稳定标识符。
pub const DEFAULT_VMM_PROJECT_ID: i64 = 1;

/// Current schema version of the native VMM relational database.
/// 原生 VMM 关系数据库的当前结构版本。
pub const VMM_SCHEMA_VERSION: i64 = 1;

/// Stable key of the single durable native Core storage snapshot.
/// 单一持久化原生 Core 存储快照的稳定键。
pub const VMM_CORE_SNAPSHOT_KEY: &str = "native-core";

/// Maximum candidate rows requested from LanceDB before exact Core visibility filtering.
/// 在 Core 精确可见性过滤前从 LanceDB 请求的最大候选行数。
pub const VMM_VECTOR_SEARCH_LIMIT: u32 = 10_000;

/// Process-local sequence used to keep repeated bootstrap bindings distinct.
/// 用于保证重复引导绑定互不冲突的进程内序列。
static VMM_BINDING_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Long-lived controller bindings for the native VMM SQLite and LanceDB stores.
/// 原生 VMM SQLite 与 LanceDB 存储使用的长生命周期控制器绑定。
#[derive(Clone)]
pub struct VmmDataPlaneHandle {
    /// Shared controller client owned by the VulcanCode host.
    /// 由 VulcanCode 宿主持有的共享控制器客户端。
    pub client: ControllerClient,
    /// Root directory dedicated to native VMM state.
    /// 原生 VMM 状态专用根目录。
    pub data_root: PathBuf,
    /// SQLite file containing relational state and Core snapshots.
    /// 保存关系状态与 Core 快照的 SQLite 文件。
    pub sqlite_path: PathBuf,
    /// LanceDB directory containing memory vectors.
    /// 保存记忆向量的 LanceDB 目录。
    pub lancedb_path: PathBuf,
    /// Bootstrap-scoped SQLite binding identifier.
    /// 单次引导作用域内的 SQLite 绑定标识符。
    pub sqlite_binding_id: String,
    /// Bootstrap-scoped LanceDB binding identifier.
    /// 单次引导作用域内的 LanceDB 绑定标识符。
    pub lancedb_binding_id: String,
}

/// Identifies one dimension-checked LanceDB table assigned to a logical VMM vector space.
/// 标识分配给逻辑 VMM 向量空间且已校验维度的一张 LanceDB 表。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmmVectorSpaceBinding {
    /// Stable logical vector-space identity emitted by Core.
    /// Core 产生的稳定逻辑向量空间标识。
    pub vector_space_id: String,
    /// Host-owned safe physical LanceDB table name.
    /// 宿主持有的安全 LanceDB 物理表名。
    pub table_name: String,
    /// Exact vector dimension accepted by the table.
    /// 该表接受的精确向量维度。
    pub dimension: u32,
    /// Exact serialized physical embedding fingerprint locked to this logical space.
    /// 锁定到该逻辑空间的精确序列化物理向量指纹。
    pub physical_fingerprint_json: String,
}

/// Carries one native VMM memory vector and the stable facts needed for physical prefiltering.
/// 携带一条原生 VMM 记忆向量及物理预过滤所需的稳定事实。
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct VmmVectorRecord {
    /// Stable Core memory identity used as the LanceDB primary key.
    /// 用作 LanceDB 主键的稳定 Core 记忆标识。
    pub memory_id: String,
    /// Numeric Core access class used only for physical candidate narrowing.
    /// 仅用于收窄物理候选的 Core 数值访问类别。
    pub access_class: u32,
    /// Optional source subject required by subject-private memories.
    /// 主体私有记忆所需的可选来源主体。
    pub source_subject_id: Option<String>,
    /// Exact owning Core scope.
    /// 精确的 Core 所属作用域。
    pub scope_id: String,
    /// Optional originating session used by replacement-scope filtering.
    /// 替换作用域过滤使用的可选来源会话。
    pub origin_session_source_id: Option<String>,
    /// Unit-normalized dense vector stored in LanceDB.
    /// 存储于 LanceDB 的单位归一化稠密向量。
    pub vector: Vec<f32>,
}

/// Carries one raw LanceDB candidate for exact Core visibility materialization.
/// 携带一条供 Core 精确物化可见性的原始 LanceDB 候选。
#[derive(Clone, Debug, PartialEq)]
pub struct VmmVectorCandidate {
    /// Stable Core memory identity.
    /// 稳定 Core 记忆标识。
    pub memory_id: String,
    /// Stored normalized vector used to recompute exact cosine similarity.
    /// 用于重新计算精确余弦相似度的已存归一化向量。
    pub vector: Vec<f32>,
}

/// Describes the safe physical filter applied before Core performs authoritative visibility checks.
/// 描述 Core 执行权威可见性校验前应用的安全物理过滤条件。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmmVectorSearchFilter {
    /// Authenticated subject requesting private memories.
    /// 请求私有记忆的已认证主体。
    pub subject_id: String,
    /// Explicit scopes authorized for shared-memory reads.
    /// 已明确授权用于共享记忆读取的作用域。
    pub visible_scope_ids: Vec<String>,
    /// Optional stricter candidate scopes.
    /// 可选的更严格候选作用域。
    pub candidate_scope_ids: Option<Vec<String>>,
    /// Optional exact originating session.
    /// 可选的精确来源会话。
    pub origin_session_source_id: Option<String>,
}

/// Describes one host-owned one-level native VMM project.
/// 描述一个由宿主持有的单层原生 VMM 项目。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeVmmProject {
    /// Stable numeric project identity used by every binding.
    /// 每条绑定使用的稳定数字项目身份。
    pub id: u64,
    /// Stable Core scope derived from the immutable numeric identity.
    /// 从不可变数字身份派生的稳定 Core 作用域。
    pub scope_id: String,
    /// User-editable display name.
    /// 用户可编辑的显示名称。
    pub name: String,
    /// Stable creation and rebinding tag.
    /// 用于创建与重新绑定的稳定标签。
    pub tag: String,
    /// Whether host policy protects this project from deletion.
    /// 宿主策略是否保护该项目不被删除。
    pub protected: bool,
}

/// Describes one VulcanCode project-to-memory-project binding.
/// 描述一条 VulcanCode 项目到记忆项目的绑定。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeVmmProjectBinding {
    /// Stable VulcanCode project identity.
    /// 稳定的 VulcanCode 项目身份。
    pub vulcan_project_id: String,
    /// Stable native VMM project identity.
    /// 稳定的原生 VMM 项目身份。
    pub vmm_project_id: u64,
}

/// Carries one project binding together with its ordered recall-only links.
/// 携带一条项目绑定及其有序的仅召回连带项目。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeVmmProjectBindingAccess {
    /// Stable VulcanCode project identity.
    /// 稳定的 VulcanCode 项目身份。
    pub vulcan_project_id: String,
    /// Unique native VMM owner project identity used for writes.
    /// 写入使用的唯一原生 VMM 所有者项目身份。
    pub owner_project_id: u64,
    /// Ordered native VMM project identities used only for recall.
    /// 仅用于召回的有序原生 VMM 项目身份。
    pub linked_project_ids: Vec<u64>,
}

/// Carries one owner project and its ordered read-only linked projects.
/// 携带一个所有者项目及其有序只读连带项目。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeVmmProjectAccess {
    /// Unique project receiving every project-owned write.
    /// 接收全部项目归属写入的唯一项目。
    pub owner: NativeVmmProject,
    /// Additional projects visible only to recall and queries.
    /// 仅对召回与查询可见的附加项目。
    pub linked: Vec<NativeVmmProject>,
}

/// Describes every VulcanCode project association that blocks native VMM project deletion.
/// 描述所有会阻止原生 VMM 项目删除的 VulcanCode 项目关联。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeVmmProjectUsage {
    /// Projects that use the VMM project as their unique write owner.
    /// 将该 VMM 项目用作唯一写入归属的项目。
    pub owner_vulcan_project_ids: Vec<String>,
    /// Projects that include the VMM project as an additional read-only query scope.
    /// 将该 VMM 项目作为附加只读查询作用域的项目。
    pub linked_vulcan_project_ids: Vec<String>,
}

impl fmt::Debug for VmmDataPlaneHandle {
    /// Formats one native VMM data-plane handle without exposing client internals.
    /// 格式化原生 VMM 数据平面句柄而不暴露客户端内部状态。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VmmDataPlaneHandle")
            .field("data_root", &self.data_root)
            .field("sqlite_path", &self.sqlite_path)
            .field("lancedb_path", &self.lancedb_path)
            .field("sqlite_binding_id", &self.sqlite_binding_id)
            .field("lancedb_binding_id", &self.lancedb_binding_id)
            .finish()
    }
}

impl VmmDataPlaneHandle {
    /// Lists all one-level native VMM projects in stable identity order.
    /// 按稳定身份顺序列出全部单层原生 VMM 项目。
    ///
    /// # Errors
    /// Returns a controller, JSON, or persisted-data validation error.
    /// 返回控制器、JSON 或持久数据校验错误。
    pub async fn list_projects(&self) -> Result<Vec<NativeVmmProject>, String> {
        let result = self
            .client
            .query_sqlite_json_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                "SELECT id, scope_id, name, tag, protected FROM vmm_projects ORDER BY id ASC;",
                Vec::new(),
            )
            .await
            .map_err(|error| format!("读取原生 VMM 项目列表失败：{error}"))?;
        let rows: Vec<NativeProjectRow> = serde_json::from_str(&result.json_data)
            .map_err(|error| format!("解析原生 VMM 项目列表失败：{error}"))?;
        rows.into_iter().map(native_project_from_row).collect()
    }

    /// Resolves one native VMM project by stable numeric identity.
    /// 按稳定数字身份解析一个原生 VMM 项目。
    ///
    /// # Errors
    /// Returns a validation, controller, JSON, or cardinality error.
    /// 返回校验、控制器、JSON 或基数错误。
    pub async fn project_by_id(&self, project_id: u64) -> Result<Option<NativeVmmProject>, String> {
        let project_id = project_id_i64(project_id)?;
        let result = self
            .client
            .query_sqlite_json_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                "SELECT id, scope_id, name, tag, protected FROM vmm_projects WHERE id = ? LIMIT 1;",
                vec![ControllerSqliteValue::Int64(project_id)],
            )
            .await
            .map_err(|error| format!("读取原生 VMM 项目失败：{error}"))?;
        one_native_project(result.json_data.as_str())
    }

    /// Creates one one-level native VMM project with an immutable stable tag.
    /// 创建一个带不可变稳定标签的单层原生 VMM 项目。
    ///
    /// # Errors
    /// Returns a validation, uniqueness, controller, or readback error.
    /// 返回校验、唯一性、控制器或回读错误。
    pub async fn create_project(&self, name: &str, tag: &str) -> Result<NativeVmmProject, String> {
        let name = validate_native_project_text(name, "VMM 项目名称", 512)?;
        let tag = validate_native_project_text(tag, "VMM 项目标签", 256)?;
        let now = current_unix_epoch_millis_i64()?;
        let result = self
            .client
            .execute_sqlite_script_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                r#"INSERT INTO vmm_projects(
    scope_id, name, tag, protected, created_at_unix_ms, updated_at_unix_ms
) VALUES ('pending:' || ?, ?, ?, 0, ?, ?);"#,
                vec![
                    sqlite_text(tag),
                    sqlite_text(name),
                    sqlite_text(tag),
                    ControllerSqliteValue::Int64(now),
                    ControllerSqliteValue::Int64(now),
                ],
            )
            .await
            .map_err(|error| format!("创建原生 VMM 项目失败：{error}"))?;
        if !result.success {
            return Err(format!("创建原生 VMM 项目失败：{}", result.message));
        }
        self.project_by_tag(tag)
            .await?
            .ok_or_else(|| String::from("创建原生 VMM 项目后无法按标签回读"))
    }

    /// Resolves a native project by immutable tag or creates it exactly once.
    /// 按不可变标签解析原生项目，若不存在则只创建一次。
    ///
    /// # Parameters
    /// `name` is the editable display name used only when creation is required.
    /// `name` 是仅在需要创建时使用的可编辑显示名称。
    /// `tag` is the immutable VulcanCode project tag used for retry-safe rebinding.
    /// `tag` 是用于可重试重新绑定的不可变 VulcanCode 项目标签。
    ///
    /// # Errors
    /// Returns a controller, validation, or catalog consistency error.
    /// 返回控制器、校验或目录一致性错误。
    pub async fn ensure_project(&self, name: &str, tag: &str) -> Result<NativeVmmProject, String> {
        let tag = validate_native_project_text(tag, "VMM 项目标签", 256)?;
        if let Some(project) = self.project_by_tag(tag).await? {
            return Ok(project);
        }
        match self.create_project(name, tag).await {
            Ok(project) => Ok(project),
            Err(create_error) => self.project_by_tag(tag).await?.ok_or(create_error),
        }
    }

    /// Renames one native VMM project without changing its identity, scope, tag, or bindings.
    /// 重命名一个原生 VMM 项目，且不改变身份、作用域、标签或绑定。
    ///
    /// # Errors
    /// Returns a validation, missing-project, or controller error.
    /// 返回校验、项目缺失或控制器错误。
    pub async fn rename_project(
        &self,
        project_id: u64,
        name: &str,
    ) -> Result<NativeVmmProject, String> {
        let project_id_i64 = project_id_i64(project_id)?;
        let name = validate_native_project_text(name, "VMM 项目名称", 512)?;
        let result = self
            .client
            .execute_sqlite_script_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                "UPDATE vmm_projects SET name = ?, updated_at_unix_ms = ? WHERE id = ?;",
                vec![
                    sqlite_text(name),
                    ControllerSqliteValue::Int64(current_unix_epoch_millis_i64()?),
                    ControllerSqliteValue::Int64(project_id_i64),
                ],
            )
            .await
            .map_err(|error| format!("重命名原生 VMM 项目失败：{error}"))?;
        if !result.success {
            return Err(format!("重命名原生 VMM 项目失败：{}", result.message));
        }
        self.project_by_id(project_id)
            .await?
            .ok_or_else(|| format!("原生 VMM 项目 {project_id} 不存在"))
    }

    /// Lists every VulcanCode project currently bound to one native memory project.
    /// 列出当前绑定到一个原生记忆项目的全部 VulcanCode 项目。
    ///
    /// # Errors
    /// Returns a validation, controller, or JSON error.
    /// 返回校验、控制器或 JSON 错误。
    pub async fn bound_vulcan_project_ids(&self, project_id: u64) -> Result<Vec<String>, String> {
        let result = self
            .client
            .query_sqlite_json_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                "SELECT vulcan_project_id FROM vmm_project_bindings WHERE vmm_project_id = ? ORDER BY vulcan_project_id ASC;",
                vec![ControllerSqliteValue::Int64(project_id_i64(project_id)?)],
            )
            .await
            .map_err(|error| format!("读取原生 VMM 项目绑定失败：{error}"))?;
        let rows: Vec<VulcanProjectIdRow> = serde_json::from_str(&result.json_data)
            .map_err(|error| format!("解析原生 VMM 项目绑定失败：{error}"))?;
        Ok(rows.into_iter().map(|row| row.vulcan_project_id).collect())
    }

    /// Resolves every owner and linked-query association for one native VMM project.
    /// 解析一个原生 VMM 项目的全部归属与连带查询关联。
    ///
    /// # Errors
    /// Returns a validation, controller, or JSON error.
    /// 返回校验、控制器或 JSON 错误。
    pub async fn project_usage(&self, project_id: u64) -> Result<NativeVmmProjectUsage, String> {
        let project_id = project_id_i64(project_id)?;
        let owner_result = self
            .client
            .query_sqlite_json_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                "SELECT vulcan_project_id FROM vmm_project_bindings WHERE vmm_project_id = ? ORDER BY vulcan_project_id ASC;",
                vec![ControllerSqliteValue::Int64(project_id)],
            )
            .await
            .map_err(|error| format!("读取原生 VMM 项目归属关联失败：{error}"))?;
        let linked_result = self
            .client
            .query_sqlite_json_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                "SELECT vulcan_project_id FROM vmm_project_query_links WHERE linked_vmm_project_id = ? ORDER BY vulcan_project_id ASC;",
                vec![ControllerSqliteValue::Int64(project_id)],
            )
            .await
            .map_err(|error| format!("读取原生 VMM 项目连带查询关联失败：{error}"))?;
        let owner_rows: Vec<VulcanProjectIdRow> = serde_json::from_str(&owner_result.json_data)
            .map_err(|error| format!("解析原生 VMM 项目归属关联失败：{error}"))?;
        let linked_rows: Vec<VulcanProjectIdRow> =
            serde_json::from_str(&linked_result.json_data)
                .map_err(|error| format!("解析原生 VMM 项目连带查询关联失败：{error}"))?;
        Ok(NativeVmmProjectUsage {
            owner_vulcan_project_ids: owner_rows
                .into_iter()
                .map(|row| row.vulcan_project_id)
                .collect(),
            linked_vulcan_project_ids: linked_rows
                .into_iter()
                .map(|row| row.vulcan_project_id)
                .collect(),
        })
    }

    /// Lists every native project binding with its ordered recall-only links.
    /// 列出每条原生项目绑定及其有序的仅召回连带项目。
    ///
    /// # Errors
    /// Returns a controller, JSON, or persisted-identity validation error.
    /// 返回控制器、JSON 或持久身份校验错误。
    pub async fn list_project_bindings(
        &self,
    ) -> Result<Vec<NativeVmmProjectBindingAccess>, String> {
        let binding_result = self
            .client
            .query_sqlite_json_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                "SELECT vulcan_project_id, vmm_project_id FROM vmm_project_bindings ORDER BY vulcan_project_id ASC;",
                Vec::new(),
            )
            .await
            .map_err(|error| format!("读取原生 VMM 项目绑定列表失败：{error}"))?;
        let link_result = self
            .client
            .query_sqlite_json_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                "SELECT vulcan_project_id, linked_vmm_project_id, position FROM vmm_project_query_links ORDER BY vulcan_project_id ASC, position ASC, linked_vmm_project_id ASC;",
                Vec::new(),
            )
            .await
            .map_err(|error| format!("读取原生 VMM 连带查询列表失败：{error}"))?;
        let bindings: Vec<NativeProjectBindingRow> =
            serde_json::from_str(&binding_result.json_data)
                .map_err(|error| format!("解析原生 VMM 项目绑定列表失败：{error}"))?;
        let links: Vec<NativeProjectQueryLinkRow> = serde_json::from_str(&link_result.json_data)
            .map_err(|error| format!("解析原生 VMM 连带查询列表失败：{error}"))?;
        let mut links_by_project = BTreeMap::<String, Vec<u64>>::new();
        for link in links {
            let linked_project_id = u64::try_from(link.linked_vmm_project_id)
                .map_err(|_| String::from("原生 VMM 连带查询项目包含无效标识"))?;
            links_by_project
                .entry(link.vulcan_project_id)
                .or_default()
                .push(linked_project_id);
        }
        bindings
            .into_iter()
            .map(|binding| {
                let owner_project_id = u64::try_from(binding.vmm_project_id)
                    .map_err(|_| String::from("原生 VMM 项目绑定包含无效所有者标识"))?;
                let vulcan_project_id = binding.vulcan_project_id;
                Ok(NativeVmmProjectBindingAccess {
                    linked_project_ids: links_by_project
                        .remove(&vulcan_project_id)
                        .unwrap_or_default(),
                    vulcan_project_id,
                    owner_project_id,
                })
            })
            .collect()
    }

    /// Deletes one unbound non-protected native VMM project row.
    /// 删除一个未绑定且不受保护的原生 VMM 项目记录。
    ///
    /// # Errors
    /// Returns an explicit protected/bound/missing or controller error.
    /// 返回明确的受保护、已绑定、缺失或控制器错误。
    pub async fn delete_unbound_project(&self, project_id: u64) -> Result<(), String> {
        let project = self
            .project_by_id(project_id)
            .await?
            .ok_or_else(|| format!("原生 VMM 项目 {project_id} 不存在"))?;
        if project.protected {
            return Err(String::from("default VMM 项目不允许删除"));
        }
        let usage = self.project_usage(project_id).await?;
        if !usage.owner_vulcan_project_ids.is_empty() || !usage.linked_vulcan_project_ids.is_empty()
        {
            return Err(format!(
                "VMM 项目 {project_id} 仍有关联；归属项目：[{}]，连带查询项目：[{}]",
                usage.owner_vulcan_project_ids.join(", "),
                usage.linked_vulcan_project_ids.join(", ")
            ));
        }
        let result = self
            .client
            .execute_sqlite_script_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                "DELETE FROM vmm_projects WHERE id = ? AND protected = 0;",
                vec![ControllerSqliteValue::Int64(project_id_i64(project_id)?)],
            )
            .await
            .map_err(|error| format!("删除原生 VMM 项目失败：{error}"))?;
        if !result.success {
            return Err(format!("删除原生 VMM 项目失败：{}", result.message));
        }
        if self.project_by_id(project_id).await?.is_some() {
            return Err(format!("原生 VMM 项目 {project_id} 未被删除"));
        }
        Ok(())
    }

    /// Deletes one non-protected project after atomically detaching every owner and query link.
    /// 原子解除全部所有者与查询连带关系后删除一个不受保护的项目。
    ///
    /// # Parameters
    /// `project_id` is the stable native VMM project selected after an explicit impact preview.
    /// `project_id` 是在明确影响预检后选择的稳定原生 VMM 项目标识。
    ///
    /// # Returns
    /// Returns the exact associations detached by the committed deletion.
    /// 返回已提交删除所解除的准确关联。
    ///
    /// # Errors
    /// Returns when the project is missing, protected, or the controller transaction fails.
    /// 当项目不存在、受保护或控制器事务失败时返回错误。
    pub async fn delete_project_with_bindings(
        &self,
        project_id: u64,
    ) -> Result<NativeVmmProjectUsage, String> {
        let project = self
            .project_by_id(project_id)
            .await?
            .ok_or_else(|| format!("原生 VMM 项目 {project_id} 不存在"))?;
        if project.protected {
            return Err(String::from("default VMM 项目不允许删除"));
        }
        let usage = self.project_usage(project_id).await?;
        let project_id_i64 = project_id_i64(project_id)?;
        let script = format!(
            "BEGIN IMMEDIATE;\nDELETE FROM vmm_project_query_links WHERE linked_vmm_project_id = {project_id_i64};\nDELETE FROM vmm_project_bindings WHERE vmm_project_id = {project_id_i64};\nDELETE FROM vmm_projects WHERE id = {project_id_i64} AND protected = 0;\nCOMMIT;"
        );
        let result = self
            .client
            .execute_sqlite_script_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                script.as_str(),
                Vec::new(),
            )
            .await
            .map_err(|error| format!("删除原生 VMM 项目并解除绑定失败：{error}"))?;
        if !result.success {
            return Err(format!(
                "删除原生 VMM 项目并解除绑定失败：{}",
                result.message
            ));
        }
        if self.project_by_id(project.id).await?.is_some() {
            return Err(format!("原生 VMM 项目 {project_id} 未被删除"));
        }
        Ok(usage)
    }

    /// Creates or replaces one VulcanCode project binding by stable project identities.
    /// 按稳定项目身份创建或替换一条 VulcanCode 项目绑定。
    ///
    /// # Errors
    /// Returns a validation, missing-target, or controller error.
    /// 返回校验、目标缺失或控制器错误。
    pub async fn bind_vulcan_project(
        &self,
        vulcan_project_id: &str,
        vmm_project_id: u64,
    ) -> Result<NativeVmmProjectBinding, String> {
        let vulcan_project_id =
            validate_native_project_text(vulcan_project_id, "VulcanCode 项目标识", 512)?;
        if self.project_by_id(vmm_project_id).await?.is_none() {
            return Err(format!("待绑定的原生 VMM 项目 {vmm_project_id} 不存在"));
        }
        let now = current_unix_epoch_millis_i64()?;
        let escaped_project_id = sqlite_string_literal(vulcan_project_id);
        let vmm_project_id_i64 = project_id_i64(vmm_project_id)?;
        let script = format!(
            r#"BEGIN IMMEDIATE;
INSERT INTO vmm_project_bindings(
    vulcan_project_id, vmm_project_id, created_at_unix_ms, updated_at_unix_ms
) VALUES ('{escaped_project_id}', {vmm_project_id_i64}, {now}, {now})
ON CONFLICT(vulcan_project_id) DO UPDATE SET
    vmm_project_id = excluded.vmm_project_id,
    updated_at_unix_ms = excluded.updated_at_unix_ms;
DELETE FROM vmm_project_query_links
WHERE vulcan_project_id = '{escaped_project_id}'
  AND linked_vmm_project_id = {vmm_project_id_i64};
COMMIT;"#
        );
        let result = self
            .client
            .execute_sqlite_script_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                script.as_str(),
                Vec::new(),
            )
            .await
            .map_err(|error| format!("保存原生 VMM 项目绑定失败：{error}"))?;
        if !result.success {
            return Err(format!("保存原生 VMM 项目绑定失败：{}", result.message));
        }
        Ok(NativeVmmProjectBinding {
            vulcan_project_id: vulcan_project_id.to_owned(),
            vmm_project_id,
        })
    }

    /// Removes one VulcanCode project binding and its query links.
    /// 移除一条 VulcanCode 项目绑定及其查询链接。
    ///
    /// # Errors
    /// Returns a validation or controller error.
    /// 返回校验或控制器错误。
    pub async fn unbind_vulcan_project(&self, vulcan_project_id: &str) -> Result<(), String> {
        let vulcan_project_id =
            validate_native_project_text(vulcan_project_id, "VulcanCode 项目标识", 512)?;
        let result = self
            .client
            .execute_sqlite_script_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                "DELETE FROM vmm_project_bindings WHERE vulcan_project_id = ?;",
                vec![sqlite_text(vulcan_project_id)],
            )
            .await
            .map_err(|error| format!("移除原生 VMM 项目绑定失败：{error}"))?;
        if !result.success {
            return Err(format!("移除原生 VMM 项目绑定失败：{}", result.message));
        }
        Ok(())
    }

    /// Resolves one VulcanCode project's owner and ordered linked query projects.
    /// 解析一个 VulcanCode 项目的所有者及有序连带查询项目。
    ///
    /// # Errors
    /// Returns a validation, missing-binding, controller, JSON, or referential error.
    /// 返回校验、绑定缺失、控制器、JSON 或引用完整性错误。
    pub async fn resolve_project_access(
        &self,
        vulcan_project_id: &str,
    ) -> Result<NativeVmmProjectAccess, String> {
        let vulcan_project_id =
            validate_native_project_text(vulcan_project_id, "VulcanCode 项目标识", 512)?;
        let owner_result = self
            .client
            .query_sqlite_json_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                r#"SELECT p.id, p.scope_id, p.name, p.tag, p.protected
FROM vmm_project_bindings b
JOIN vmm_projects p ON p.id = b.vmm_project_id
WHERE b.vulcan_project_id = ?
LIMIT 1;"#,
                vec![sqlite_text(vulcan_project_id)],
            )
            .await
            .map_err(|error| format!("解析原生 VMM 所有者项目失败：{error}"))?;
        let owner = one_native_project(owner_result.json_data.as_str())?.ok_or_else(|| {
            format!("VulcanCode 项目 `{vulcan_project_id}` 尚未绑定原生 VMM 项目")
        })?;
        let linked_result = self
            .client
            .query_sqlite_json_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                r#"SELECT p.id, p.scope_id, p.name, p.tag, p.protected
FROM vmm_project_query_links l
JOIN vmm_projects p ON p.id = l.linked_vmm_project_id
WHERE l.vulcan_project_id = ?
ORDER BY l.position ASC, p.id ASC;"#,
                vec![sqlite_text(vulcan_project_id)],
            )
            .await
            .map_err(|error| format!("解析原生 VMM 连带查询项目失败：{error}"))?;
        let linked_rows: Vec<NativeProjectRow> = serde_json::from_str(&linked_result.json_data)
            .map_err(|error| format!("解析原生 VMM 连带查询项目失败：{error}"))?;
        let linked = linked_rows
            .into_iter()
            .map(native_project_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(NativeVmmProjectAccess { owner, linked })
    }

    /// Atomically replaces one bound VulcanCode project's ordered linked query projects.
    /// 原子替换一个已绑定 VulcanCode 项目的有序连带查询项目。
    ///
    /// # Errors
    /// Returns duplicate, owner-link, missing-target, binding, or controller errors.
    /// 返回重复、所有者链接、目标缺失、绑定或控制器错误。
    pub async fn replace_query_links(
        &self,
        vulcan_project_id: &str,
        linked_vmm_project_ids: &[u64],
    ) -> Result<NativeVmmProjectAccess, String> {
        let access = self.resolve_project_access(vulcan_project_id).await?;
        let unique = linked_vmm_project_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if unique.len() != linked_vmm_project_ids.len() {
            return Err(String::from("原生 VMM 连带查询项目不得重复"));
        }
        if linked_vmm_project_ids.contains(&access.owner.id) {
            return Err(String::from("原生 VMM 连带查询项目不得包含所有者项目"));
        }
        for project_id in linked_vmm_project_ids {
            if self.project_by_id(*project_id).await?.is_none() {
                return Err(format!("连带查询的原生 VMM 项目 {project_id} 不存在"));
            }
        }
        let vulcan_project_id =
            validate_native_project_text(vulcan_project_id, "VulcanCode 项目标识", 512)?;
        let escaped_project_id = sqlite_string_literal(vulcan_project_id);
        let now = current_unix_epoch_millis_i64()?;
        let mut script = format!(
            "BEGIN IMMEDIATE;\nDELETE FROM vmm_project_query_links WHERE vulcan_project_id = '{escaped_project_id}';\n"
        );
        for (position, project_id) in linked_vmm_project_ids.iter().enumerate() {
            let position = i64::try_from(position)
                .map_err(|_| String::from("原生 VMM 连带查询项目数量超出支持范围"))?;
            script.push_str(
                format!(
                    "INSERT INTO vmm_project_query_links(vulcan_project_id, linked_vmm_project_id, position, created_at_unix_ms) VALUES ('{escaped_project_id}', {project_id}, {position}, {now});\n"
                )
                .as_str(),
            );
        }
        script.push_str("COMMIT;");
        let result = self
            .client
            .execute_sqlite_script_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                script.as_str(),
                Vec::new(),
            )
            .await
            .map_err(|error| format!("保存原生 VMM 连带查询项目失败：{error}"))?;
        if !result.success {
            return Err(format!("保存原生 VMM 连带查询项目失败：{}", result.message));
        }
        self.resolve_project_access(vulcan_project_id).await
    }

    /// Resolves one native VMM project by immutable tag.
    /// 按不可变标签解析一个原生 VMM 项目。
    async fn project_by_tag(&self, tag: &str) -> Result<Option<NativeVmmProject>, String> {
        let result = self
            .client
            .query_sqlite_json_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                "SELECT id, scope_id, name, tag, protected FROM vmm_projects WHERE tag = ? LIMIT 1;",
                vec![sqlite_text(tag)],
            )
            .await
            .map_err(|error| format!("按标签读取原生 VMM 项目失败：{error}"))?;
        one_native_project(result.json_data.as_str())
    }

    /// Resolves one previously registered logical vector space without creating a table.
    /// 解析一个此前已登记的逻辑向量空间，且不会创建数据表。
    ///
    /// # Parameters
    /// 参数
    /// `vector_space_id` is the exact Core identity stored in the relational snapshot.
    /// `vector_space_id` 是关系快照中保存的精确 Core 标识。
    ///
    /// # Returns
    /// 返回值
    /// The persisted binding, no value before first creation, or a controlled storage error.
    /// 返回已持久化绑定；首次创建前返回空；存储异常时返回受控错误。
    pub async fn resolve_vector_space(
        &self,
        vector_space_id: &str,
    ) -> Result<Option<VmmVectorSpaceBinding>, String> {
        if vector_space_id.trim().is_empty() {
            return Err(String::from("VMM 向量空间标识不能为空"));
        }
        let result = self
            .client
            .query_sqlite_json_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                "SELECT vector_space_id, table_name, dimension, physical_fingerprint_json FROM vmm_vector_spaces WHERE vector_space_id = ? LIMIT 1;",
                vec![sqlite_text(vector_space_id)],
            )
            .await
            .map_err(|error| format!("读取 VMM 向量空间失败：{error}"))?;
        let rows: Vec<VectorSpaceRow> = serde_json::from_str(&result.json_data)
            .map_err(|error| format!("解析 VMM 向量空间查询结果失败：{error}"))?;
        match rows.as_slice() {
            [] => Ok(None),
            [row] => {
                let dimension = u32::try_from(row.dimension)
                    .ok()
                    .filter(|dimension| *dimension > 0)
                    .ok_or_else(|| String::from("VMM 向量空间持久维度无效"))?;
                Ok(Some(VmmVectorSpaceBinding {
                    vector_space_id: row.vector_space_id.clone(),
                    table_name: row.table_name.clone(),
                    dimension,
                    physical_fingerprint_json: row.physical_fingerprint_json.clone(),
                }))
            }
            _ => Err(String::from("VMM 向量空间标识返回了多条记录")),
        }
    }

    /// Loads the relational-only native Core snapshot when one has been committed.
    /// 在原生 Core 快照已经提交时加载仅含关系数据的快照。
    ///
    /// # Returns
    /// 返回值
    /// The stored JSON document, no value on first use, or a controlled controller/JSON error.
    /// 返回已存 JSON 文档；首次使用时返回空；控制器或 JSON 异常时返回受控错误。
    pub async fn load_core_snapshot_json(&self) -> Result<Option<String>, String> {
        let result = self
            .client
            .query_sqlite_json_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                "SELECT snapshot_json FROM vmm_core_snapshots WHERE snapshot_key = ? LIMIT 1;",
                vec![sqlite_text(VMM_CORE_SNAPSHOT_KEY)],
            )
            .await
            .map_err(|error| format!("读取原生 VMM Core 快照失败：{error}"))?;
        let rows: Vec<SnapshotJsonRow> = serde_json::from_str(&result.json_data)
            .map_err(|error| format!("解析原生 VMM Core 快照查询结果失败：{error}"))?;
        match rows.as_slice() {
            [] => Ok(None),
            [row] => Ok(Some(row.snapshot_json.clone())),
            _ => Err(String::from("原生 VMM Core 快照键返回了多条记录")),
        }
    }

    /// Persists one relational-only native Core snapshot as the durable commit point.
    /// 将一份仅含关系数据的原生 Core 快照持久化为耐久提交点。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot_json` is a validated serialized Core StorageSnapshot without embeddings.
    /// `snapshot_json` 是已校验序列化且不含嵌入的 Core StorageSnapshot。
    ///
    /// # Returns
    /// 返回值
    /// Success after the controller commits the row, or a controlled persistence error.
    /// 控制器提交记录后返回成功，否则返回受控持久化错误。
    pub async fn persist_core_snapshot_json(&self, snapshot_json: &str) -> Result<(), String> {
        let result = self
            .client
            .execute_sqlite_script_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                r#"INSERT INTO vmm_core_snapshots(snapshot_key, snapshot_json, updated_at_unix_ms)
VALUES (?, ?, ?)
ON CONFLICT(snapshot_key) DO UPDATE SET
    snapshot_json = excluded.snapshot_json,
    updated_at_unix_ms = excluded.updated_at_unix_ms;"#,
                vec![
                    sqlite_text(VMM_CORE_SNAPSHOT_KEY),
                    sqlite_text(snapshot_json),
                    ControllerSqliteValue::Int64(current_unix_epoch_millis_i64()?),
                ],
            )
            .await
            .map_err(|error| format!("保存原生 VMM Core 快照失败：{error}"))?;
        if !result.success {
            return Err(format!("保存原生 VMM Core 快照失败：{}", result.message));
        }
        Ok(())
    }

    /// Resolves or creates the dimension-specific LanceDB table for one logical vector space.
    /// 为一个逻辑向量空间解析或创建维度固定的 LanceDB 表。
    ///
    /// # Parameters
    /// 参数
    /// `vector_space_id` is the exact Core identity, `dimension` is the nonzero vector width,
    /// and `physical_fingerprint_json` is the exact Host AI compatibility fingerprint.
    /// `vector_space_id` 是精确 Core 标识，`dimension` 是非零向量宽度，
    /// `physical_fingerprint_json` 是精确的宿主 AI 兼容性指纹。
    ///
    /// # Returns
    /// 返回值
    /// A stable physical binding, or a dimension/schema/controller error.
    /// 返回稳定物理绑定，或维度、结构、控制器错误。
    pub async fn ensure_vector_space(
        &self,
        vector_space_id: &str,
        dimension: u32,
        physical_fingerprint_json: &str,
    ) -> Result<VmmVectorSpaceBinding, String> {
        if vector_space_id.trim().is_empty()
            || dimension == 0
            || physical_fingerprint_json.trim().is_empty()
        {
            return Err(String::from("VMM 向量空间标识、维度与物理指纹必须有效"));
        }
        let result = self
            .client
            .query_sqlite_json_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                "SELECT vector_space_id, table_name, dimension, physical_fingerprint_json FROM vmm_vector_spaces WHERE vector_space_id = ? LIMIT 1;",
                vec![sqlite_text(vector_space_id)],
            )
            .await
            .map_err(|error| format!("读取 VMM 向量空间失败：{error}"))?;
        let rows: Vec<VectorSpaceRow> = serde_json::from_str(&result.json_data)
            .map_err(|error| format!("解析 VMM 向量空间查询结果失败：{error}"))?;
        if let Some(row) = rows.first() {
            let stored_dimension = u32::try_from(row.dimension)
                .map_err(|_| String::from("VMM 向量空间持久维度超出支持范围"))?;
            if stored_dimension != dimension {
                return Err(format!(
                    "VMM 向量空间 `{vector_space_id}` 的持久维度 {stored_dimension} 与请求维度 {dimension} 不一致"
                ));
            }
            if row.physical_fingerprint_json != physical_fingerprint_json {
                return Err(format!(
                    "VMM 向量空间 `{vector_space_id}` 的物理向量指纹与当前 Embedding 路由不一致"
                ));
            }
            return Ok(VmmVectorSpaceBinding {
                vector_space_id: row.vector_space_id.clone(),
                table_name: row.table_name.clone(),
                dimension: stored_dimension,
                physical_fingerprint_json: row.physical_fingerprint_json.clone(),
            });
        }

        let table_name = vector_table_name(vector_space_id, dimension);
        self.client
            .create_lancedb_table_typed(
                VMM_SPACE_ID,
                self.lancedb_binding_id.as_str(),
                table_name.as_str(),
                vector_table_columns(dimension),
                true,
            )
            .await
            .map_err(|error| format!("创建 VMM LanceDB 向量表失败：{error}"))?;
        let now = current_unix_epoch_millis_i64()?;
        let inserted = self
            .client
            .execute_sqlite_script_typed(
                VMM_SPACE_ID,
                self.sqlite_binding_id.as_str(),
                r#"INSERT INTO vmm_vector_spaces(
    vector_space_id, table_name, dimension, physical_fingerprint_json,
    created_at_unix_ms, updated_at_unix_ms
) VALUES (?, ?, ?, ?, ?, ?);"#,
                vec![
                    sqlite_text(vector_space_id),
                    sqlite_text(table_name.as_str()),
                    ControllerSqliteValue::Int64(i64::from(dimension)),
                    sqlite_text(physical_fingerprint_json),
                    ControllerSqliteValue::Int64(now),
                    ControllerSqliteValue::Int64(now),
                ],
            )
            .await
            .map_err(|error| format!("登记 VMM 向量空间失败：{error}"))?;
        if !inserted.success {
            return Err(format!("登记 VMM 向量空间失败：{}", inserted.message));
        }
        Ok(VmmVectorSpaceBinding {
            vector_space_id: vector_space_id.to_string(),
            table_name,
            dimension,
            physical_fingerprint_json: physical_fingerprint_json.to_string(),
        })
    }

    /// Upserts one validated batch into its dimension-specific LanceDB table.
    /// 将一个已校验批次写入其维度固定的 LanceDB 表。
    ///
    /// # Parameters
    /// 参数
    /// `binding` selects the exact table and `records` carry normalized rows.
    /// `binding` 选择精确表，`records` 携带归一化记录。
    ///
    /// # Returns
    /// 返回值
    /// Success after every row is accepted, or a controlled validation/controller error.
    /// 全部记录被接受后返回成功，否则返回受控校验或控制器错误。
    pub async fn upsert_vectors(
        &self,
        binding: &VmmVectorSpaceBinding,
        records: &[VmmVectorRecord],
    ) -> Result<(), String> {
        if records.is_empty() {
            return Ok(());
        }
        let expected_dimension = usize::try_from(binding.dimension)
            .map_err(|_| String::from("VMM 向量维度超出当前平台支持范围"))?;
        if records.iter().any(|record| {
            record.memory_id.trim().is_empty()
                || record.scope_id.trim().is_empty()
                || record.vector.len() != expected_dimension
                || record.vector.iter().any(|value| !value.is_finite())
        }) {
            return Err(String::from("VMM LanceDB 向量批次包含无效记录"));
        }
        let rows = records
            .iter()
            .map(|record| {
                serde_json::json!({
                    "id": record.memory_id,
                    "access_class": record.access_class,
                    "source_subject_id": record.source_subject_id,
                    "scope_id": record.scope_id,
                    "origin_session_source_id": record.origin_session_source_id,
                    "vector": record.vector,
                })
            })
            .collect::<Vec<_>>();
        let data = serde_json::to_vec(&rows)
            .map_err(|error| format!("编码 VMM LanceDB 向量批次失败：{error}"))?;
        self.client
            .upsert_lancedb_typed(
                VMM_SPACE_ID,
                self.lancedb_binding_id.as_str(),
                binding.table_name.as_str(),
                ControllerLanceDbInputFormat::JsonRows,
                data,
                vec![String::from("id")],
            )
            .await
            .map_err(|error| format!("写入 VMM LanceDB 向量批次失败：{error}"))?;
        Ok(())
    }

    /// Deletes exact memory identities from one physical vector-space table.
    /// 从一个物理向量空间表删除精确记忆标识。
    ///
    /// # Parameters
    /// 参数
    /// `binding` selects the table and `memory_ids` is an exact trusted identity list.
    /// `binding` 选择表，`memory_ids` 是精确可信标识列表。
    ///
    /// # Returns
    /// 返回值
    /// Success after all bounded delete chunks complete, or a controller error.
    /// 全部有界删除分块完成后返回成功，否则返回控制器错误。
    pub async fn delete_vectors(
        &self,
        binding: &VmmVectorSpaceBinding,
        memory_ids: &[String],
    ) -> Result<(), String> {
        for chunk in memory_ids.chunks(200) {
            if chunk.is_empty() {
                continue;
            }
            let identities = chunk
                .iter()
                .map(|identity| format!("'{}'", lance_string_literal(identity)))
                .collect::<Vec<_>>()
                .join(", ");
            self.client
                .delete_lancedb_typed(
                    VMM_SPACE_ID,
                    self.lancedb_binding_id.as_str(),
                    binding.table_name.as_str(),
                    format!("id IN ({identities})"),
                )
                .await
                .map_err(|error| format!("删除 VMM LanceDB 向量失败：{error}"))?;
        }
        Ok(())
    }

    /// Loads every requested vector by exact memory identity for Core restart hydration.
    /// 按精确记忆标识加载全部请求向量，用于 Core 重启回填。
    ///
    /// # Parameters
    /// 参数
    /// `binding` selects one dimension-bound table and `memory_ids` is the complete expected set.
    /// `binding` 选择一张绑定维度的表，`memory_ids` 是完整预期集合。
    ///
    /// # Returns
    /// 返回值
    /// Exactly one validated vector per requested identity, or an error when LanceDB is incomplete.
    /// 每个请求标识对应一条已校验向量；LanceDB 数据不完整时返回错误。
    pub async fn load_vectors(
        &self,
        binding: &VmmVectorSpaceBinding,
        memory_ids: &[String],
    ) -> Result<Vec<VmmVectorCandidate>, String> {
        let expected_dimension = usize::try_from(binding.dimension)
            .map_err(|_| String::from("VMM 向量维度超出当前平台支持范围"))?;
        if expected_dimension == 0
            || memory_ids.iter().any(|identity| identity.trim().is_empty())
            || memory_ids.iter().collect::<BTreeSet<_>>().len() != memory_ids.len()
        {
            return Err(String::from("VMM LanceDB 向量回填参数无效"));
        }
        let mut probe_vector = vec![0.0_f32; expected_dimension];
        probe_vector[0] = 1.0;
        let mut loaded = Vec::with_capacity(memory_ids.len());
        for chunk in memory_ids.chunks(200) {
            let expected_ids = chunk.iter().map(String::as_str).collect::<BTreeSet<_>>();
            let identities = chunk
                .iter()
                .map(|identity| format!("'{}'", lance_string_literal(identity)))
                .collect::<Vec<_>>()
                .join(", ");
            let limit = u32::try_from(chunk.len())
                .map_err(|_| String::from("VMM LanceDB 向量回填批次过大"))?;
            let result = self
                .client
                .search_lancedb_typed(
                    VMM_SPACE_ID,
                    self.lancedb_binding_id.as_str(),
                    binding.table_name.as_str(),
                    probe_vector.clone(),
                    limit,
                    format!("id IN ({identities})"),
                    "vector",
                    ControllerLanceDbOutputFormat::JsonRows,
                )
                .await
                .map_err(|error| format!("回填 VMM LanceDB 向量失败：{error}"))?;
            let rows: Vec<VectorCandidateRow> = serde_json::from_slice(&result.data)
                .map_err(|error| format!("解析 VMM LanceDB 回填结果失败：{error}"))?;
            let mut returned_ids = BTreeSet::new();
            for row in rows {
                if !expected_ids.contains(row.id.as_str())
                    || !returned_ids.insert(row.id.clone())
                    || row.vector.len() != expected_dimension
                    || row.vector.iter().any(|value| !value.is_finite())
                {
                    return Err(String::from("VMM LanceDB 返回了无效的向量回填记录"));
                }
                loaded.push(VmmVectorCandidate {
                    memory_id: row.id,
                    vector: row.vector,
                });
            }
            if returned_ids.len() != expected_ids.len() {
                return Err(String::from("VMM LanceDB 缺少 Core 快照引用的记忆向量"));
            }
        }
        Ok(loaded)
    }

    /// Searches one vector space after applying safe identity and scope prefilters.
    /// 应用安全身份及作用域预过滤后检索一个向量空间。
    ///
    /// # Parameters
    /// 参数
    /// `binding` selects the physical table, `query_vector` is normalized, and `filter` carries explicit grants.
    /// `binding` 选择物理表，`query_vector` 已归一化，`filter` 携带明确授权。
    ///
    /// # Returns
    /// 返回值
    /// Raw ordered candidates for exact Core materialization, or a validation/controller error.
    /// 返回供 Core 精确物化的原始有序候选，或校验、控制器错误。
    pub async fn search_vectors(
        &self,
        binding: &VmmVectorSpaceBinding,
        query_vector: Vec<f32>,
        filter: &VmmVectorSearchFilter,
    ) -> Result<Vec<VmmVectorCandidate>, String> {
        let expected_dimension = usize::try_from(binding.dimension)
            .map_err(|_| String::from("VMM 向量维度超出当前平台支持范围"))?;
        if query_vector.len() != expected_dimension
            || query_vector.iter().any(|value| !value.is_finite())
            || filter.subject_id.trim().is_empty()
            || filter.visible_scope_ids.is_empty()
        {
            return Err(String::from("VMM LanceDB 向量查询参数无效"));
        }
        let predicate = build_vector_filter(filter)?;
        let result = self
            .client
            .search_lancedb_typed(
                VMM_SPACE_ID,
                self.lancedb_binding_id.as_str(),
                binding.table_name.as_str(),
                query_vector,
                VMM_VECTOR_SEARCH_LIMIT,
                predicate,
                "vector",
                ControllerLanceDbOutputFormat::JsonRows,
            )
            .await
            .map_err(|error| format!("检索 VMM LanceDB 向量失败：{error}"))?;
        let rows: Vec<VectorCandidateRow> = serde_json::from_slice(&result.data)
            .map_err(|error| format!("解析 VMM LanceDB 检索结果失败：{error}"))?;
        rows.into_iter()
            .map(|row| {
                if row.id.trim().is_empty()
                    || row.vector.len() != expected_dimension
                    || row.vector.iter().any(|value| !value.is_finite())
                {
                    return Err(String::from("VMM LanceDB 返回了无效向量候选"));
                }
                Ok(VmmVectorCandidate {
                    memory_id: row.id,
                    vector: row.vector,
                })
            })
            .collect()
    }
}

/// Carries the single-column SQLite snapshot query row.
/// 携带 SQLite 快照查询的单列记录。
#[derive(Debug, Deserialize)]
struct SnapshotJsonRow {
    /// Serialized relational-only Core snapshot.
    /// 已序列化的仅关系 Core 快照。
    snapshot_json: String,
}

/// Carries one SQLite row for a one-level native VMM project.
/// 携带一条单层原生 VMM 项目的 SQLite 记录。
#[derive(Debug, Deserialize)]
struct NativeProjectRow {
    /// Positive stable project identity.
    /// 正数稳定项目身份。
    id: i64,
    /// Core scope derived from the stable identity.
    /// 从稳定身份派生的 Core 作用域。
    scope_id: String,
    /// User-editable display name.
    /// 用户可编辑的显示名称。
    name: String,
    /// Immutable creation and rebinding tag.
    /// 不可变的创建与重新绑定标签。
    tag: String,
    /// SQLite boolean indicating deletion protection.
    /// 表示删除保护状态的 SQLite 布尔值。
    protected: i64,
}

/// Carries one VulcanCode project identity returned by a binding query.
/// 携带绑定查询返回的一条 VulcanCode 项目身份。
#[derive(Debug, Deserialize)]
struct VulcanProjectIdRow {
    /// Stable VulcanCode project identity.
    /// 稳定的 VulcanCode 项目身份。
    vulcan_project_id: String,
}

/// Carries one persisted native VMM owner binding row.
/// 携带一条持久化原生 VMM 所有者绑定记录。
#[derive(Debug, Deserialize)]
struct NativeProjectBindingRow {
    /// Stable VulcanCode project identity.
    /// 稳定的 VulcanCode 项目身份。
    vulcan_project_id: String,
    /// Stable native VMM owner project identity.
    /// 稳定的原生 VMM 所有者项目身份。
    vmm_project_id: i64,
}

/// Carries one persisted ordered native VMM query-link row.
/// 携带一条持久化有序原生 VMM 查询连带记录。
#[derive(Debug, Deserialize)]
struct NativeProjectQueryLinkRow {
    /// Stable VulcanCode project identity.
    /// 稳定的 VulcanCode 项目身份。
    vulcan_project_id: String,
    /// Stable linked native VMM project identity.
    /// 稳定的连带原生 VMM 项目身份。
    linked_vmm_project_id: i64,
}

/// Carries one persisted logical-to-physical vector-space binding.
/// 携带一条持久化逻辑到物理向量空间绑定。
#[derive(Debug, Deserialize)]
struct VectorSpaceRow {
    /// Stable logical identity.
    /// 稳定逻辑标识。
    vector_space_id: String,
    /// Safe physical table name.
    /// 安全物理表名。
    table_name: String,
    /// Stored positive dimension.
    /// 已存正数维度。
    dimension: i64,
    /// Exact serialized physical compatibility fingerprint.
    /// 精确序列化的物理兼容性指纹。
    physical_fingerprint_json: String,
}

/// Carries the LanceDB columns needed to materialize one raw candidate.
/// 携带物化一条原始候选所需的 LanceDB 列。
#[derive(Debug, Deserialize)]
struct VectorCandidateRow {
    /// Stable memory identity.
    /// 稳定记忆标识。
    id: String,
    /// Stored normalized vector.
    /// 已存归一化向量。
    vector: Vec<f32>,
}

/// Bootstraps the isolated native VMM space below `.vulcan/code/vmm`.
/// 在 `.vulcan/code/vmm` 下引导隔离的原生 VMM 空间。
///
/// `controller` supplies the already connected VulcanCode-owned VLDB client.
/// The returned handle owns no child VMM process and reuses only that controller session.
/// `controller` 提供已连接且由 VulcanCode 持有的 VLDB 客户端。
/// 返回句柄不持有任何 VMM 子进程，只复用该控制器会话。
pub async fn bootstrap_vmm_data_plane(
    controller: &ControllerRuntimeHandle,
) -> Result<VmmDataPlaneHandle, String> {
    let data_root = controller.data_root.join("vmm");
    let sqlite_root = data_root.join("sqlite");
    let lancedb_path = data_root.join("lancedb");
    let sqlite_path = sqlite_root.join("vmm.sqlite3");

    ensure_trusted_directory(&data_root)?;
    ensure_trusted_directory(&sqlite_root)?;
    ensure_trusted_directory(&lancedb_path)?;

    let binding_suffix = build_binding_suffix();
    let sqlite_binding_id = format!("{VMM_SQLITE_BINDING_TAG}-{binding_suffix}");
    let lancedb_binding_id = format!("{VMM_LANCEDB_BINDING_TAG}-{binding_suffix}");
    let client = controller.client.clone();

    client
        .attach_space(SpaceRegistration {
            space_id: String::from(VMM_SPACE_ID),
            space_label: String::from("VMM"),
            space_kind: SpaceKind::User,
            space_root: runtime_assets::native_host_path_text_from_path(&data_root),
        })
        .await
        .map_err(|error| format!("附着原生 VMM 数据空间失败：{error}"))?;

    client
        .enable_sqlite(ControllerSqliteEnableRequest {
            space_id: String::from(VMM_SPACE_ID),
            binding_id: sqlite_binding_id.clone(),
            db_path: runtime_assets::native_host_path_text_from_path(&sqlite_path),
            ..ControllerSqliteEnableRequest::default()
        })
        .await
        .map_err(|error| format!("启用原生 VMM SQLite 数据库失败：{error}"))?;

    client
        .enable_lancedb(ControllerLanceDbEnableRequest {
            space_id: String::from(VMM_SPACE_ID),
            binding_id: lancedb_binding_id.clone(),
            default_db_path: runtime_assets::native_host_path_text_from_path(&lancedb_path),
            db_root: Some(runtime_assets::native_host_path_text_from_path(
                &lancedb_path,
            )),
            read_consistency_interval_ms: Some(250),
            max_upsert_payload: 16 * 1024 * 1024,
            max_search_limit: 10_000,
            max_concurrent_requests: 16,
        })
        .await
        .map_err(|error| format!("启用原生 VMM LanceDB 数据库失败：{error}"))?;

    let schema_result = client
        .execute_sqlite_script_typed(
            VMM_SPACE_ID,
            sqlite_binding_id.as_str(),
            build_vmm_schema_script(),
            Vec::new(),
        )
        .await
        .map_err(|error| format!("初始化原生 VMM 数据结构失败：{error}"))?;
    if !schema_result.success {
        return Err(format!(
            "初始化原生 VMM 数据结构失败：{}",
            schema_result.message
        ));
    }

    Ok(VmmDataPlaneHandle {
        client,
        data_root,
        sqlite_path,
        lancedb_path,
        sqlite_binding_id,
        lancedb_binding_id,
    })
}

/// Builds the complete first-install schema for native VMM state.
/// 构建原生 VMM 状态的完整首次安装数据结构。
#[must_use]
pub fn build_vmm_schema_script() -> String {
    format!(
        r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS vmm_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT INTO vmm_metadata (key, value)
VALUES ('schema_version', '{schema_version}')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;

CREATE TABLE IF NOT EXISTS vmm_projects (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    scope_id TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL CHECK(length(trim(name)) > 0),
    tag TEXT NOT NULL UNIQUE CHECK(length(trim(tag)) > 0),
    protected INTEGER NOT NULL DEFAULT 0 CHECK(protected IN (0, 1)),
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);

INSERT OR IGNORE INTO vmm_projects (
    id,
    scope_id,
    name,
    tag,
    protected,
    created_at_unix_ms,
    updated_at_unix_ms
) VALUES (
    {default_project_id},
    'vmm-project:1',
    'default',
    'default',
    1,
    0,
    0
);

CREATE TRIGGER IF NOT EXISTS vmm_project_scope_after_insert
AFTER INSERT ON vmm_projects
WHEN NEW.id != {default_project_id}
  AND NEW.scope_id LIKE 'pending:%'
BEGIN
    UPDATE vmm_projects
    SET scope_id = 'vmm-project:' || NEW.id
    WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS vmm_project_stable_identity_guard
BEFORE UPDATE OF id, scope_id, tag ON vmm_projects
WHEN OLD.scope_id NOT LIKE 'pending:%'
  AND (
      NEW.id != OLD.id
      OR NEW.scope_id != OLD.scope_id
      OR NEW.tag != OLD.tag
  )
BEGIN
    SELECT RAISE(ABORT, 'native VMM project identity cannot be changed');
END;

CREATE TRIGGER IF NOT EXISTS vmm_default_project_delete_guard
BEFORE DELETE ON vmm_projects
WHEN OLD.id = {default_project_id}
BEGIN
    SELECT RAISE(ABORT, 'default VMM project cannot be deleted');
END;

CREATE TRIGGER IF NOT EXISTS vmm_default_project_identity_guard
BEFORE UPDATE OF id, scope_id, tag, protected ON vmm_projects
WHEN OLD.id = {default_project_id}
  AND (
      NEW.id != OLD.id
      OR NEW.scope_id != OLD.scope_id
      OR NEW.tag != OLD.tag
      OR NEW.protected != OLD.protected
  )
BEGIN
    SELECT RAISE(ABORT, 'default VMM project identity cannot be changed');
END;

CREATE TABLE IF NOT EXISTS vmm_project_bindings (
    vulcan_project_id TEXT PRIMARY KEY,
    vmm_project_id INTEGER NOT NULL,
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    FOREIGN KEY(vmm_project_id) REFERENCES vmm_projects(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_vmm_project_bindings_target
ON vmm_project_bindings(vmm_project_id, vulcan_project_id);

CREATE TABLE IF NOT EXISTS vmm_project_query_links (
    vulcan_project_id TEXT NOT NULL,
    linked_vmm_project_id INTEGER NOT NULL,
    position INTEGER NOT NULL CHECK(position >= 0),
    created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(vulcan_project_id, linked_vmm_project_id),
    FOREIGN KEY(vulcan_project_id)
        REFERENCES vmm_project_bindings(vulcan_project_id) ON DELETE CASCADE,
    FOREIGN KEY(linked_vmm_project_id)
        REFERENCES vmm_projects(id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS vmm_core_snapshots (
    snapshot_key TEXT PRIMARY KEY,
    snapshot_json TEXT NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS vmm_vector_spaces (
    vector_space_id TEXT PRIMARY KEY,
    table_name TEXT NOT NULL UNIQUE,
    dimension INTEGER NOT NULL CHECK(dimension > 0),
    physical_fingerprint_json TEXT NOT NULL CHECK(length(trim(physical_fingerprint_json)) > 0),
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL
);
"#,
        schema_version = VMM_SCHEMA_VERSION,
        default_project_id = DEFAULT_VMM_PROJECT_ID,
    )
}

/// Builds the fixed LanceDB schema shared by all native VMM vector-space tables.
/// 构建全部原生 VMM 向量空间表共享的固定 LanceDB 结构。
fn vector_table_columns(dimension: u32) -> Vec<ControllerLanceDbColumnDef> {
    vec![
        ControllerLanceDbColumnDef {
            name: String::from("id"),
            column_type: ControllerLanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        ControllerLanceDbColumnDef {
            name: String::from("access_class"),
            column_type: ControllerLanceDbColumnType::Uint32,
            vector_dim: 0,
            nullable: false,
        },
        ControllerLanceDbColumnDef {
            name: String::from("source_subject_id"),
            column_type: ControllerLanceDbColumnType::String,
            vector_dim: 0,
            nullable: true,
        },
        ControllerLanceDbColumnDef {
            name: String::from("scope_id"),
            column_type: ControllerLanceDbColumnType::String,
            vector_dim: 0,
            nullable: false,
        },
        ControllerLanceDbColumnDef {
            name: String::from("origin_session_source_id"),
            column_type: ControllerLanceDbColumnType::String,
            vector_dim: 0,
            nullable: true,
        },
        ControllerLanceDbColumnDef {
            name: String::from("vector"),
            column_type: ControllerLanceDbColumnType::VectorFloat32,
            vector_dim: dimension,
            nullable: false,
        },
    ]
}

/// Builds one safe physical LanceDB predicate from explicit Core read grants.
/// 根据明确的 Core 读取授权构建一条安全物理 LanceDB 谓词。
fn build_vector_filter(filter: &VmmVectorSearchFilter) -> Result<String, String> {
    let visible_scopes = lance_string_set(&filter.visible_scope_ids)?;
    let subject = lance_string_literal(&filter.subject_id);
    let mut predicates = vec![format!(
        "((access_class = 1 AND source_subject_id = '{subject}') OR (access_class = 2 AND scope_id IN ({visible_scopes})))"
    )];
    if let Some(candidate_scope_ids) = &filter.candidate_scope_ids {
        predicates.push(format!(
            "scope_id IN ({})",
            lance_string_set(candidate_scope_ids)?
        ));
    }
    if let Some(origin_session_source_id) = &filter.origin_session_source_id {
        predicates.push(format!(
            "origin_session_source_id = '{}'",
            lance_string_literal(origin_session_source_id)
        ));
    }
    Ok(predicates.join(" AND "))
}

/// Encodes a nonempty list as safe single-quoted LanceDB string literals.
/// 将非空列表编码为安全的 LanceDB 单引号字符串字面量。
fn lance_string_set(values: &[String]) -> Result<String, String> {
    if values.is_empty() || values.iter().any(|value| value.trim().is_empty()) {
        return Err(String::from("VMM LanceDB 过滤作用域不能为空"));
    }
    Ok(values
        .iter()
        .map(|value| format!("'{}'", lance_string_literal(value)))
        .collect::<Vec<_>>()
        .join(", "))
}

/// Escapes one trusted identity for a LanceDB SQL string literal.
/// 转义一条可信标识以用于 LanceDB SQL 字符串字面量。
fn lance_string_literal(value: &str) -> String {
    value.replace('\'', "''")
}

/// Derives a deterministic safe physical table name from vector-space identity and dimension.
/// 根据向量空间标识与维度派生确定性安全物理表名。
fn vector_table_name(vector_space_id: &str, dimension: u32) -> String {
    let mut hasher = Sha256::new();
    hasher.update(vector_space_id.as_bytes());
    hasher.update(dimension.to_le_bytes());
    let digest = hex::encode(hasher.finalize());
    format!("vmm_vec_{}", &digest[..24])
}

/// Converts text into one typed SQLite parameter.
/// 将文本转换为一条类型化 SQLite 参数。
fn sqlite_text(value: impl Into<String>) -> ControllerSqliteValue {
    ControllerSqliteValue::String(value.into())
}

/// Converts one public native project identity into SQLite's signed integer range.
/// 将一个公开原生项目身份转换到 SQLite 有符号整数范围。
fn project_id_i64(project_id: u64) -> Result<i64, String> {
    if project_id == 0 {
        return Err(String::from("原生 VMM 项目标识必须大于零"));
    }
    i64::try_from(project_id).map_err(|_| String::from("原生 VMM 项目标识超出 SQLite 支持范围"))
}

/// Validates and trims one persisted native project text field.
/// 校验并裁剪一个待持久化的原生项目文本字段。
fn validate_native_project_text<'a>(
    value: &'a str,
    label: &str,
    max_chars: usize,
) -> Result<&'a str, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{label}不能为空"));
    }
    if value.contains('\0') {
        return Err(format!("{label}不能包含 NUL 字符"));
    }
    if value.chars().count() > max_chars {
        return Err(format!("{label}不能超过 {max_chars} 个字符"));
    }
    Ok(value)
}

/// Converts and validates one persisted project row at the host boundary.
/// 在宿主边界转换并校验一条持久化项目记录。
fn native_project_from_row(row: NativeProjectRow) -> Result<NativeVmmProject, String> {
    let id = u64::try_from(row.id)
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| String::from("原生 VMM 项目持久化标识无效"))?;
    let expected_scope_id = format!("vmm-project:{id}");
    if row.scope_id != expected_scope_id {
        return Err(format!("原生 VMM 项目 {id} 的持久化作用域与稳定身份不一致"));
    }
    let protected = match row.protected {
        0 => false,
        1 => true,
        _ => return Err(format!("原生 VMM 项目 {id} 的保护标记无效")),
    };
    if id == DEFAULT_VMM_PROJECT_ID as u64 && !protected {
        return Err(String::from("default VMM 项目必须保持删除保护"));
    }
    validate_native_project_text(row.name.as_str(), "VMM 项目名称", 512)?;
    validate_native_project_text(row.tag.as_str(), "VMM 项目标签", 256)?;
    Ok(NativeVmmProject {
        id,
        scope_id: row.scope_id,
        name: row.name,
        tag: row.tag,
        protected,
    })
}

/// Decodes a query that is required to return at most one native project.
/// 解码一项最多允许返回一个原生项目的查询。
fn one_native_project(json_data: &str) -> Result<Option<NativeVmmProject>, String> {
    let mut rows: Vec<NativeProjectRow> = serde_json::from_str(json_data)
        .map_err(|error| format!("解析原生 VMM 项目查询结果失败：{error}"))?;
    match rows.len() {
        0 => Ok(None),
        1 => native_project_from_row(
            rows.pop()
                .ok_or_else(|| String::from("原生 VMM 项目查询结果意外为空"))?,
        )
        .map(Some),
        _ => Err(String::from("原生 VMM 项目查询返回了多条记录")),
    }
}

/// Escapes one validated identity for a SQLite single-quoted string literal.
/// 转义一个已校验身份以用于 SQLite 单引号字符串字面量。
fn sqlite_string_literal(value: &str) -> String {
    value.replace('\'', "''")
}

/// Ensures one host-owned directory exists and is not a symbolic link.
/// 确保宿主持有的目录存在且不是符号链接。
fn ensure_trusted_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(format!(
            "原生 VMM 数据目录不是可信普通目录：{}",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir_all(path)
            .map_err(|create_error| {
                format!(
                    "创建原生 VMM 数据目录失败（{}）：{create_error}",
                    path.display()
                )
            }),
        Err(error) => Err(format!(
            "读取原生 VMM 数据目录失败（{}）：{error}",
            path.display()
        )),
    }
}

/// Builds one process-local binding suffix for both native VMM backends.
/// 为两个原生 VMM 后端构建一个进程内绑定后缀。
fn build_binding_suffix() -> String {
    let sequence = VMM_BINDING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}-{}-{sequence}",
        std::process::id(),
        current_unix_epoch_millis()
    )
}

/// Returns the current Unix epoch in milliseconds for binding identity only.
/// 返回仅用于绑定身份的当前 Unix 毫秒时间。
fn current_unix_epoch_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

/// Returns current Unix milliseconds within SQLite's signed integer range.
/// 返回位于 SQLite 有符号整数范围内的当前 Unix 毫秒数。
fn current_unix_epoch_millis_i64() -> Result<i64, String> {
    i64::try_from(current_unix_epoch_millis())
        .map_err(|_| String::from("当前 Unix 毫秒时间超出 SQLite 支持范围"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the first-install schema creates and protects fallback project ID one.
    /// 验证首次安装结构创建并保护 ID 为一的兜底项目。
    #[test]
    fn schema_creates_and_protects_default_project() {
        let schema = build_vmm_schema_script();
        assert!(schema.contains("VALUES (\n    1,"));
        assert!(schema.contains("vmm_project_scope_after_insert"));
        assert!(schema.contains("vmm_project_stable_identity_guard"));
        assert!(schema.contains("vmm_default_project_delete_guard"));
        assert!(schema.contains("vmm_default_project_identity_guard"));
        assert!(schema.contains("position INTEGER NOT NULL CHECK(position >= 0)"));
    }

    /// Verifies vectors are assigned to LanceDB while SQLite stores only vector-space metadata.
    /// 验证向量归属 LanceDB，而 SQLite 只保存向量空间元数据。
    #[test]
    fn schema_does_not_store_vector_payloads_in_sqlite() {
        let schema = build_vmm_schema_script();
        assert!(schema.contains("vmm_vector_spaces"));
        assert!(!schema.contains("vector_json"));
        assert!(!schema.contains("embedding_json"));
    }
}
