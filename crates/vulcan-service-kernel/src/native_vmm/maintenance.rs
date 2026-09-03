//! Host-owned native VMM project-content maintenance.
//! 宿主持有的原生 VMM 项目内容维护。

use super::driver::NativeVmmCoreRuntime;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use vulcan_memorymesh_core::{
    MutationOutcome, ScopeId, SessionSourceId, StorageSnapshot, context_evidence_domain_id,
};
use vulcan_workspace_store::vmm_data_plane::{
    NativeVmmProject, NativeVmmProjectAccess, NativeVmmProjectBindingAccess, NativeVmmProjectUsage,
    VmmDataPlaneHandle,
};

/// Carries one native VMM project together with all current consumers.
/// 携带一个原生 VMM 项目及其全部当前使用方。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeVmmProjectCatalogItem {
    /// One-level native VMM project metadata.
    /// 一级原生 VMM 项目元数据。
    pub project: NativeVmmProject,
    /// Owner and recall-only associations currently using the project.
    /// 当前使用该项目的所有者与仅召回关联。
    pub usage: NativeVmmProjectUsage,
}

/// Carries the complete native project and binding catalog.
/// 携带完整的原生项目与绑定目录。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NativeVmmProjectCatalog {
    /// Every one-level native VMM project.
    /// 全部一级原生 VMM 项目。
    pub projects: Vec<NativeVmmProjectCatalogItem>,
    /// Every stable VulcanCode project binding.
    /// 全部稳定的 VulcanCode 项目绑定。
    pub bindings: Vec<NativeVmmProjectBindingAccess>,
}

/// Carries the exact impact of deleting one native VMM project.
/// 携带删除一个原生 VMM 项目的准确影响。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NativeVmmProjectDeletePreview {
    /// Storage version that must still match when deletion is confirmed.
    /// 确认删除时必须仍然匹配的存储版本。
    pub storage_version: u64,
    /// Native VMM project selected for deletion.
    /// 选择删除的原生 VMM 项目。
    pub project: NativeVmmProject,
    /// Owner and recall-only associations that will be detached.
    /// 将被解除的所有者与仅召回关联。
    pub usage: NativeVmmProjectUsage,
    /// Project-owned Core content that will be removed.
    /// 将被删除的项目归属 Core 内容。
    pub content: NativeVmmProjectContentCounts,
}

/// Summarizes content attributable to one native VMM project scope.
/// 汇总可归属于一个原生 VMM 项目作用域的内容。
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NativeVmmProjectContentCounts {
    /// Number of distinct host sessions owned by the project.
    /// 项目拥有的不同宿主会话数量。
    pub sessions: u64,
    /// Number of canonical turns owned by the project.
    /// 项目拥有的规范回合数量。
    pub turns: u64,
    /// Number of durable memories owned by the project.
    /// 项目拥有的持久记忆数量。
    pub memories: u64,
    /// Number of project profile nodes.
    /// 项目画像节点数量。
    pub profile_nodes: u64,
    /// Number of context-evidence rows attached to project memories.
    /// 关联到项目记忆的情境证据数量。
    pub context_evidence: u64,
    /// Number of durable session compact boundaries.
    /// 持久会话压缩边界数量。
    pub compact_boundaries: u64,
    /// Number of memory vector rows affected by the operation.
    /// 操作影响的记忆向量行数量。
    pub vector_rows: u64,
}

/// Describes one deterministic session rename required by a migration conflict.
/// 描述迁移冲突所需的一次确定性会话改名。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NativeVmmSessionRename {
    /// Original source session identity.
    /// 原始来源会话身份。
    pub from: String,
    /// Collision-free target session identity.
    /// 无冲突的目标会话身份。
    pub to: String,
}

/// Describes one user-visible primary record included by a migration preview.
/// 描述迁移预检包含的一条用户可见主记录。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NativeVmmMigrationRecordSummary {
    /// Stable record class: session, turn, memory, or project_profile.
    /// 稳定记录类别：会话、回合、记忆或项目画像。
    pub record_kind: String,
    /// Stable Core identity, or the host session identity for a session row.
    /// 稳定 Core 身份；会话行则使用宿主会话身份。
    pub record_id: String,
    /// Originating host session when the record belongs to one.
    /// 当记录属于某个宿主会话时携带其来源会话。
    pub session_source_id: Option<String>,
    /// Compact non-secret description available for human review.
    /// 供人工核对的紧凑非敏感描述。
    pub summary: Option<String>,
}

/// Carries a side-effect-free native project migration preview.
/// 携带无副作用的原生项目迁移预检结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeVmmMigrationPreview {
    /// Storage version that must still match when execution is confirmed.
    /// 执行确认时必须仍然匹配的存储版本。
    pub storage_version: u64,
    /// Source native project identity.
    /// 来源原生项目身份。
    pub source_project_id: u64,
    /// Target native project identity.
    /// 目标原生项目身份。
    pub target_project_id: u64,
    /// Content currently owned by the source project.
    /// 来源项目当前拥有的内容。
    pub source: NativeVmmProjectContentCounts,
    /// Content currently owned by the target project.
    /// 目标项目当前拥有的内容。
    pub target_before: NativeVmmProjectContentCounts,
    /// Projected target content after the move.
    /// 迁移后目标项目的预计内容。
    pub target_after: NativeVmmProjectContentCounts,
    /// Session collisions and their mandatory automatic renames.
    /// 会话冲突及其必须执行的自动改名。
    pub session_renames: Vec<NativeVmmSessionRename>,
    /// User-visible primary records that will change project ownership.
    /// 将改变项目归属的用户可见主记录。
    pub records: Vec<NativeVmmMigrationRecordSummary>,
    /// SHA-256 identity of the exact displayed migration preview.
    /// 用户所见准确迁移预检的 SHA-256 身份。
    pub preview_fingerprint: String,
}

/// Carries a completed native project migration result.
/// 携带已完成的原生项目迁移结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeVmmMigrationResult {
    /// New committed storage version.
    /// 新提交的存储版本。
    pub storage_version: u64,
    /// Content moved out of the source project.
    /// 已从来源项目迁出的内容。
    pub moved: NativeVmmProjectContentCounts,
    /// Session renames applied during the move.
    /// 迁移期间实际应用的会话改名。
    pub session_renames: Vec<NativeVmmSessionRename>,
}

/// Carries a side-effect-free native project clear preview.
/// 携带无副作用的原生项目清空预检结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeVmmClearPreview {
    /// Storage version that must still match when execution is confirmed.
    /// 执行确认时必须仍然匹配的存储版本。
    pub storage_version: u64,
    /// Native project identity whose content will be removed.
    /// 将被清空内容的原生项目身份。
    pub project_id: u64,
    /// Project content that will be removed.
    /// 将被删除的项目内容。
    pub removed: NativeVmmProjectContentCounts,
}

/// Carries a completed native project clear result.
/// 携带已完成的原生项目清空结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeVmmClearResult {
    /// New committed storage version.
    /// 新提交的存储版本。
    pub storage_version: u64,
    /// Project content removed by the clear.
    /// 本次清空删除的项目内容。
    pub removed: NativeVmmProjectContentCounts,
}

/// Provides native project migration and clear operations over one shared runtime.
/// 基于一个共享运行时提供原生项目迁移与清空操作。
#[derive(Clone)]
pub(crate) struct NativeVmmMaintenance {
    /// Shared native Core runtime whose mutation gate is the maintenance commit boundary.
    /// 以其变更门作为维护提交边界的共享原生 Core 运行时。
    runtime: Arc<NativeVmmCoreRuntime>,
    /// Host project catalog used to reject nonexistent maintenance targets.
    /// 用于拒绝不存在维护目标的宿主项目目录。
    data_plane: VmmDataPlaneHandle,
    /// Exclusive side of the barrier shared with every native project operation.
    /// 与全部原生项目操作共享的屏障独占侧。
    operation_gate: Arc<RwLock<()>>,
    /// Serializes project catalog and content mutations across UI and project creation paths.
    /// 在界面操作与项目创建路径之间串行化项目目录及内容变更。
    project_mutation_gate: Arc<Mutex<()>>,
}

impl NativeVmmMaintenance {
    /// Returns the latest native Host AI validation status without triggering inference work.
    /// 返回最近一次原生宿主 AI 校验状态，且不触发推理工作。
    ///
    /// # Returns
    /// 返回值
    /// The shared runtime's latest completed or initial unchecked status.
    /// 返回共享运行时最近完成的状态或初始未校验状态。
    pub(crate) fn host_ai_status(&self) -> super::NativeVmmHostAiStatus {
        self.runtime.host_ai_status()
    }

    /// Validates the native runtime's required host AI routes before global enablement.
    /// 在全局启用前校验原生运行时所需的宿主 AI 路由。
    ///
    /// # Errors
    /// Returns the exact Host AI registration or embedding compatibility failure.
    /// Host AI 注册或向量兼容性失败时返回准确错误。
    pub(crate) async fn validate_host_ai(&self) -> Result<(), String> {
        self.runtime.validate_host_ai().await
    }

    /// Creates one maintenance facade over the exact runtime and project catalog.
    /// 基于精确运行时与项目目录创建一个维护门面。
    #[must_use]
    pub(crate) fn new(
        runtime: Arc<NativeVmmCoreRuntime>,
        data_plane: VmmDataPlaneHandle,
        operation_gate: Arc<RwLock<()>>,
    ) -> Self {
        Self {
            runtime,
            data_plane,
            operation_gate,
            project_mutation_gate: Arc::new(Mutex::new(())),
        }
    }

    /// Loads every native VMM project, its consumers, and every VulcanCode binding.
    /// 加载全部原生 VMM 项目、使用方以及每条 VulcanCode 绑定。
    pub(crate) async fn project_catalog(&self) -> Result<NativeVmmProjectCatalog, String> {
        // The catalog guard keeps the multi-query projection on one project-association boundary.
        // 目录守卫使多次查询投影保持在同一个项目关联边界上。
        let _guard = self.project_mutation_gate.lock().await;
        let projects = self.data_plane.list_projects().await?;
        let mut catalog_projects = Vec::with_capacity(projects.len());
        for project in projects {
            let usage = self.data_plane.project_usage(project.id).await?;
            catalog_projects.push(NativeVmmProjectCatalogItem { project, usage });
        }
        Ok(NativeVmmProjectCatalog {
            projects: catalog_projects,
            bindings: self.data_plane.list_project_bindings().await?,
        })
    }

    /// Binds one VulcanCode project to an existing native VMM owner project.
    /// 将一个 VulcanCode 项目绑定到已有原生 VMM 所有者项目。
    pub(crate) async fn bind_existing_project(
        &self,
        vulcan_project_id: &str,
        vmm_project_id: u64,
    ) -> Result<NativeVmmProject, String> {
        let _guard = self.project_mutation_gate.lock().await;
        // Owner rebinding cannot cross an in-flight operation that already resolved its write scope.
        // 所有者重绑不能跨越已经解析写入作用域的进行中操作。
        let _operation_guard = self.operation_gate.write().await;
        let project = self
            .data_plane
            .project_by_id(vmm_project_id)
            .await?
            .ok_or_else(|| format!("原生 VMM 项目 {vmm_project_id} 不存在"))?;
        self.data_plane
            .bind_vulcan_project(vulcan_project_id, vmm_project_id)
            .await?;
        Ok(project)
    }

    /// Applies a fixed owner only when a newly registered VulcanCode project has no native binding.
    /// 仅当新登记的 VulcanCode 项目尚无原生绑定时应用固定所有者。
    ///
    /// # Parameters
    /// `vulcan_project_id` is the stable VulcanCode project tag being registered.
    /// `vulcan_project_id` 是正在登记的稳定 VulcanCode 项目标签。
    /// `vmm_project_id` is the globally configured fallback owner project.
    /// `vmm_project_id` 是全局配置的兜底所有者项目。
    ///
    /// # Returns
    /// Returns the pre-existing owner when present, otherwise the newly assigned fixed owner.
    /// 已存在所有者时返回原所有者，否则返回新分配的固定所有者。
    pub(crate) async fn bind_existing_project_if_unbound(
        &self,
        vulcan_project_id: &str,
        vmm_project_id: u64,
    ) -> Result<NativeVmmProject, String> {
        let _guard = self.project_mutation_gate.lock().await;
        // Registration must not overwrite an owner selected before a project directory was re-added.
        // 登记过程不得覆盖项目目录重新添加前已经选择的所有者。
        let _operation_guard = self.operation_gate.write().await;
        if let Some(project) = self.bound_project(vulcan_project_id).await? {
            return Ok(project);
        }
        let project = self
            .data_plane
            .project_by_id(vmm_project_id)
            .await?
            .ok_or_else(|| format!("原生 VMM 项目 {vmm_project_id} 不存在"))?;
        self.data_plane
            .bind_vulcan_project(vulcan_project_id, vmm_project_id)
            .await?;
        Ok(project)
    }

    /// Atomically replaces one project's unique write owner and recall-only project set.
    /// 原子替换一个项目的唯一写入所有者与仅召回项目集合。
    pub(crate) async fn set_project_access(
        &self,
        vulcan_project_id: &str,
        owner_project_id: u64,
        linked_project_ids: &[u64],
    ) -> Result<NativeVmmProjectAccess, String> {
        let _guard = self.project_mutation_gate.lock().await;
        // One write guard keeps functional requests outside the complete association transaction.
        // 单个写守卫使功能请求无法进入完整关联事务的中间状态。
        let _operation_guard = self.operation_gate.write().await;
        self.data_plane
            .set_project_access(vulcan_project_id, owner_project_id, linked_project_ids)
            .await
    }

    /// Creates or reuses the project tag and binds it as the unique write owner.
    /// 创建或复用项目标签，并将其绑定为唯一写入所有者。
    pub(crate) async fn create_and_bind_project(
        &self,
        vulcan_project_id: &str,
        project_name: &str,
    ) -> Result<NativeVmmProject, String> {
        let _guard = self.project_mutation_gate.lock().await;
        // Creation and binding publish one new owner only after active native operations drain.
        // 创建与绑定仅在活动原生操作排空后发布新的所有者。
        let _operation_guard = self.operation_gate.write().await;
        let project = self
            .data_plane
            .ensure_project(project_name, vulcan_project_id)
            .await?;
        self.data_plane
            .bind_vulcan_project(vulcan_project_id, project.id)
            .await?;
        Ok(project)
    }

    /// Creates the per-project default only when a newly registered project has no native binding.
    /// 仅当新登记项目尚无原生绑定时创建按项目默认记忆项目。
    ///
    /// # Parameters
    /// `vulcan_project_id` is the stable VulcanCode project tag used by the binding.
    /// `vulcan_project_id` 是绑定使用的稳定 VulcanCode 项目标签。
    /// `project_name` is the initial display name for a newly created native project.
    /// `project_name` 是新建原生项目使用的初始显示名称。
    ///
    /// # Returns
    /// Returns the pre-existing owner when present, otherwise the newly created and assigned owner.
    /// 已存在所有者时返回原所有者，否则返回新建并分配的所有者。
    pub(crate) async fn create_and_bind_project_if_unbound(
        &self,
        vulcan_project_id: &str,
        project_name: &str,
    ) -> Result<NativeVmmProject, String> {
        let _guard = self.project_mutation_gate.lock().await;
        // The check and default assignment share the mutation guard so manual rebinding cannot be lost.
        // 检查与默认分配共用变更守卫，因此不会丢失并发发生的手动重绑。
        let _operation_guard = self.operation_gate.write().await;
        if let Some(project) = self.bound_project(vulcan_project_id).await? {
            return Ok(project);
        }
        let project = self
            .data_plane
            .ensure_project(project_name, vulcan_project_id)
            .await?;
        self.data_plane
            .bind_vulcan_project(vulcan_project_id, project.id)
            .await?;
        Ok(project)
    }

    /// Resolves an existing owner while the caller holds the project-mutation guard.
    /// 在调用方持有项目变更守卫时解析现有所有者。
    ///
    /// # Parameters
    /// `vulcan_project_id` is the stable VulcanCode project tag to inspect.
    /// `vulcan_project_id` 是要检查的稳定 VulcanCode 项目标签。
    ///
    /// # Returns
    /// Returns `None` when unbound, or the exact owner project; dangling bindings are explicit errors.
    /// 未绑定时返回 `None`，否则返回准确所有者项目；悬空绑定会显式报错。
    async fn bound_project(
        &self,
        vulcan_project_id: &str,
    ) -> Result<Option<NativeVmmProject>, String> {
        let Some(binding) = self
            .data_plane
            .list_project_bindings()
            .await?
            .into_iter()
            .find(|binding| binding.vulcan_project_id == vulcan_project_id)
        else {
            return Ok(None);
        };
        self.data_plane
            .project_by_id(binding.owner_project_id)
            .await?
            .map(Some)
            .ok_or_else(|| {
                format!(
                    "VulcanCode 项目 {vulcan_project_id} 的原生 VMM 绑定指向不存在的项目 {}",
                    binding.owner_project_id
                )
            })
    }

    /// Removes one VulcanCode project's owner binding and all recall-only links.
    /// 删除一个 VulcanCode 项目的所有者绑定及全部仅召回连带关系。
    pub(crate) async fn unbind_project(&self, vulcan_project_id: &str) -> Result<(), String> {
        let _guard = self.project_mutation_gate.lock().await;
        // Unbinding waits until no active operation can still rely on the previous owner.
        // 解绑会等待，直到没有活动操作仍依赖此前所有者。
        let _operation_guard = self.operation_gate.write().await;
        self.data_plane
            .unbind_vulcan_project(vulcan_project_id)
            .await
    }

    /// Replaces one bound VulcanCode project's ordered recall-only project list.
    /// 替换一个已绑定 VulcanCode 项目的有序仅召回项目列表。
    pub(crate) async fn replace_query_links(
        &self,
        vulcan_project_id: &str,
        linked_project_ids: &[u64],
    ) -> Result<(), String> {
        let _guard = self.project_mutation_gate.lock().await;
        // Linked recall replacement waits until existing retrievals release their frozen scope set.
        // 连带召回替换会等待现有检索释放其冻结作用域集合。
        let _operation_guard = self.operation_gate.write().await;
        self.data_plane
            .replace_query_links(vulcan_project_id, linked_project_ids)
            .await?;
        Ok(())
    }

    /// Renames one native VMM project while preserving every stable binding.
    /// 在保留全部稳定绑定的同时重命名一个原生 VMM 项目。
    pub(crate) async fn rename_project(
        &self,
        project_id: u64,
        name: &str,
    ) -> Result<NativeVmmProject, String> {
        let _guard = self.project_mutation_gate.lock().await;
        self.data_plane.rename_project(project_id, name).await
    }

    /// Previews project deletion, detached consumers, and removed Core content.
    /// 预检项目删除、被解除的使用方以及将删除的 Core 内容。
    pub(crate) async fn preview_project_delete(
        &self,
        project_id: u64,
    ) -> Result<NativeVmmProjectDeletePreview, String> {
        let project = self
            .data_plane
            .project_by_id(project_id)
            .await?
            .ok_or_else(|| format!("原生 VMM 项目 {project_id} 不存在"))?;
        if project.protected {
            return Err(String::from("default VMM 项目不允许删除"));
        }
        self.ensure_no_pending_analysis_jobs(project_id).await?;
        let snapshot = self.runtime.storage_snapshot()?;
        Ok(NativeVmmProjectDeletePreview {
            storage_version: snapshot.storage_version,
            usage: self.data_plane.project_usage(project_id).await?,
            content: project_content_counts(&snapshot, &project_scope(project_id)?),
            project,
        })
    }

    /// Deletes one project after validating the preview version and detaches every consumer.
    /// 校验预检版本后删除一个项目，并解除全部使用方。
    ///
    /// `operation_id` is the stable command UUID used by the recovery journal.
    /// `operation_id` 是恢复账本使用的稳定命令 UUID。
    pub(crate) async fn delete_project(
        &self,
        operation_id: &str,
        project_id: u64,
        expected_storage_version: u64,
        confirm_detach_bindings: bool,
    ) -> Result<NativeVmmProjectDeletePreview, String> {
        let _guard = self.project_mutation_gate.lock().await;
        // The write guard drains in-flight readers and prevents new project operations until the
        // project row, bindings, Core snapshot, and vector state reach one maintenance boundary.
        // 写入守卫排空进行中的读取方，并阻止新的项目操作，直到项目记录、绑定、Core 快照与向量状态到达同一维护边界。
        let _operation_guard = self.operation_gate.write().await;
        self.ensure_no_pending_analysis_jobs(project_id).await?;
        // mutation_kind binds replay to the complete confirmed delete payload, including the
        // preview version and the explicit association-detachment acknowledgement.
        // mutation_kind 将重放绑定到完整确认删除载荷，包括预检版本与明确的关联解除确认。
        let mutation_kind = format!(
            "project_delete:{project_id}:{expected_storage_version}:{confirm_detach_bindings}"
        );
        if let Some(preview) = self
            .runtime
            .replay_storage_snapshot_transform::<NativeVmmProjectDeletePreview>(
                operation_id,
                &mutation_kind,
            )
            .await?
        {
            self.data_plane
                .delete_project_with_bindings(project_id)
                .await?;
            return Ok(preview);
        }
        let preview = self.preview_project_delete(project_id).await?;
        if (!preview.usage.owner_vulcan_project_ids.is_empty()
            || !preview.usage.linked_vulcan_project_ids.is_empty())
            && !confirm_detach_bindings
        {
            return Err(String::from(
                "该原生 VMM 项目仍有 VulcanCode 归属或连带查询关联，请确认解除关联后再删除。",
            ));
        }
        if preview.storage_version != expected_storage_version {
            return Err(format!(
                "原生 VMM 存储版本已变化：预期 {expected_storage_version}，实际 {}，请重新预检",
                preview.storage_version
            ));
        }
        // deleted_preview is journaled so a retry can still disable former owners after catalog deletion.
        // deleted_preview 会写入账本，使重试在项目目录已删除后仍可禁用原有归属项目。
        let deleted_preview = self
            .runtime
            .transform_storage_snapshot(
                operation_id,
                &mutation_kind,
                expected_storage_version,
                |snapshot| {
                    clear_project(snapshot, project_id)?;
                    Ok(preview.clone())
                },
            )
            .await?;
        self.data_plane
            .delete_project_with_bindings(project_id)
            .await?;
        Ok(deleted_preview)
    }

    /// Previews a content move without changing Core, SQLite, or LanceDB state.
    /// 在不改变 Core、SQLite 或 LanceDB 状态的情况下预检内容迁移。
    pub(crate) async fn preview_migration(
        &self,
        source_project_id: u64,
        target_project_id: u64,
    ) -> Result<NativeVmmMigrationPreview, String> {
        self.ensure_distinct_projects(source_project_id, target_project_id)
            .await?;
        self.ensure_no_pending_analysis_jobs(source_project_id)
            .await?;
        preview_migration(
            &self.runtime.storage_snapshot()?,
            source_project_id,
            target_project_id,
        )
    }

    /// Moves source content into the target after validating the exact preview version.
    /// 校验精确预检版本后将来源内容迁入目标项目。
    ///
    /// `operation_id` is the stable command UUID used for idempotent replay.
    /// `operation_id` 是用于幂等重放的稳定命令 UUID。
    pub(crate) async fn execute_migration(
        &self,
        operation_id: &str,
        source_project_id: u64,
        target_project_id: u64,
        expected_storage_version: u64,
        expected_preview_fingerprint: &str,
        accept_session_renames: bool,
    ) -> Result<NativeVmmMigrationResult, String> {
        let _guard = self.project_mutation_gate.lock().await;
        // The write guard freezes all native operations while the source queue and content move are checked.
        // 写入守卫在检查来源队列及迁移内容期间冻结全部原生操作。
        let _operation_guard = self.operation_gate.write().await;
        self.ensure_no_pending_analysis_jobs(source_project_id)
            .await?;
        self.ensure_distinct_projects(source_project_id, target_project_id)
            .await?;
        // mutation_kind rejects command-id reuse with a different preview or risk acknowledgement.
        // mutation_kind 拒绝使用同一命令 ID 提交不同预检或风险确认状态。
        let mutation_kind = format!(
            "project_migration:{source_project_id}:{target_project_id}:{expected_storage_version}:{expected_preview_fingerprint}:{accept_session_renames}"
        );
        let output = self
            .runtime
            .transform_storage_snapshot(
                operation_id,
                &mutation_kind,
                expected_storage_version,
                |snapshot| {
                    let preview =
                        preview_migration(snapshot, source_project_id, target_project_id)?;
                    if preview.preview_fingerprint != expected_preview_fingerprint {
                        return Err(String::from(
                            "原生 VMM 迁移预检指纹已变化，请重新预检后确认",
                        ));
                    }
                    if !preview.session_renames.is_empty() && !accept_session_renames {
                        return Err(format!(
                            "迁移发现 {} 个会话冲突，请确认自动改名后重试",
                            preview.session_renames.len()
                        ));
                    }
                    apply_migration(snapshot, &preview)?;
                    Ok((preview.source, preview.session_renames))
                },
            )
            .await?;
        Ok(NativeVmmMigrationResult {
            storage_version: expected_storage_version.saturating_add(1),
            moved: output.0,
            session_renames: output.1,
        })
    }

    /// Previews removal of all project-level Core records while preserving personal profiles.
    /// 预检删除全部项目级 Core 记录，同时保留个人画像。
    pub(crate) async fn preview_clear(
        &self,
        project_id: u64,
    ) -> Result<NativeVmmClearPreview, String> {
        self.ensure_project(project_id).await?;
        self.ensure_no_pending_analysis_jobs(project_id).await?;
        let snapshot = self.runtime.storage_snapshot()?;
        Ok(NativeVmmClearPreview {
            storage_version: snapshot.storage_version,
            project_id,
            removed: project_content_counts(&snapshot, &project_scope(project_id)?),
        })
    }

    /// Clears all project-level Core records after validating the exact preview version.
    /// 校验精确预检版本后清空全部项目级 Core 记录。
    ///
    /// `operation_id` is the stable command UUID used by the recovery journal.
    /// `operation_id` 是恢复账本使用的稳定命令 UUID。
    pub(crate) async fn execute_clear(
        &self,
        operation_id: &str,
        project_id: u64,
        expected_storage_version: u64,
    ) -> Result<NativeVmmClearResult, String> {
        let _guard = self.project_mutation_gate.lock().await;
        // The write guard prevents a queued or foreground operation from restoring content during clear.
        // 写入守卫防止队列或前台操作在清空期间重新写入内容。
        let _operation_guard = self.operation_gate.write().await;
        self.ensure_no_pending_analysis_jobs(project_id).await?;
        self.ensure_project(project_id).await?;
        // mutation_kind binds a retry to the exact clear preview version.
        // mutation_kind 将重试绑定到准确的清空预检版本。
        let mutation_kind = format!("project_clear:{project_id}:{expected_storage_version}");
        let removed = self
            .runtime
            .transform_storage_snapshot(
                operation_id,
                &mutation_kind,
                expected_storage_version,
                |snapshot| clear_project(snapshot, project_id),
            )
            .await?;
        Ok(NativeVmmClearResult {
            storage_version: expected_storage_version.saturating_add(1),
            removed,
        })
    }

    /// Verifies that a native VMM project exists.
    /// 验证一个原生 VMM 项目存在。
    async fn ensure_project(&self, project_id: u64) -> Result<(), String> {
        if self.data_plane.project_by_id(project_id).await?.is_none() {
            return Err(format!("原生 VMM 项目 {project_id} 不存在"));
        }
        Ok(())
    }

    /// Rejects maintenance while a durable PostAction job can still write to one project.
    /// 当耐久 PostAction 作业仍可能写入某项目时拒绝维护。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` identifies the project whose Core content would be moved or removed.
    /// `project_id` 标识其 Core 内容将被迁移或删除的项目。
    ///
    /// # Returns
    /// 返回值
    /// Success only when no intake or ready analysis job targets the project scope.
    /// 仅当没有登记中或就绪分析作业指向该项目作用域时返回成功。
    ///
    /// # Errors
    /// 错误
    /// Returns an integrity error or an actionable pending-job count.
    /// 返回完整性错误或可操作的待处理作业数量。
    async fn ensure_no_pending_analysis_jobs(&self, project_id: u64) -> Result<(), String> {
        let scope = project_scope(project_id)?;
        let jobs = self.data_plane.list_active_pending_analysis_jobs().await?;
        let mut pending_count = 0_u64;
        for job in jobs {
            if super::client::pending_analysis_job_targets_scope(&job, &scope)? {
                pending_count = pending_count.saturating_add(1);
            }
        }
        if pending_count > 0 {
            return Err(format!(
                "原生 VMM 项目 {project_id} 仍有 {pending_count} 条后台记忆分析作业，请等待作业完成后重新预检并执行维护"
            ));
        }
        Ok(())
    }

    /// Verifies two existing projects form a valid directed migration pair.
    /// 验证两个已存在项目构成合法的定向迁移对。
    async fn ensure_distinct_projects(
        &self,
        source_project_id: u64,
        target_project_id: u64,
    ) -> Result<(), String> {
        if source_project_id == target_project_id {
            return Err(String::from("原生 VMM 迁移来源项目与目标项目不能相同"));
        }
        self.ensure_project(source_project_id).await?;
        self.ensure_project(target_project_id).await
    }
}

/// Builds one exact migration preview and deterministic conflict rename plan.
/// 构建一份精确迁移预检与确定性冲突改名计划。
fn preview_migration(
    snapshot: &StorageSnapshot,
    source_project_id: u64,
    target_project_id: u64,
) -> Result<NativeVmmMigrationPreview, String> {
    if source_project_id == target_project_id {
        return Err(String::from("原生 VMM 迁移来源项目与目标项目不能相同"));
    }
    let source_scope = project_scope(source_project_id)?;
    let target_scope = project_scope(target_project_id)?;
    let source_sessions = project_sessions(snapshot, &source_scope);
    let target_sessions = project_sessions(snapshot, &target_scope);
    let session_renames = session_rename_plan(snapshot, &source_sessions, &target_sessions)?;
    let source = project_content_counts(snapshot, &source_scope);
    let target_before = project_content_counts(snapshot, &target_scope);
    let mut projected = snapshot.clone();
    apply_migration_records(
        &mut projected,
        &source_scope,
        &target_scope,
        &session_renames,
    )?;
    let target_after = project_content_counts(&projected, &target_scope);
    let records = migration_record_summaries(snapshot, &source_scope);
    let preview_fingerprint = migration_preview_fingerprint(
        snapshot.storage_version,
        source_project_id,
        target_project_id,
        &source,
        &target_before,
        &target_after,
        &session_renames,
        &records,
    )?;
    Ok(NativeVmmMigrationPreview {
        storage_version: snapshot.storage_version,
        source_project_id,
        target_project_id,
        source,
        target_before,
        target_after,
        session_renames,
        records,
        preview_fingerprint,
    })
}

/// Collects deterministic user-visible source records for migration confirmation.
/// 为迁移确认收集确定性的用户可见来源记录。
fn migration_record_summaries(
    snapshot: &StorageSnapshot,
    source_scope: &ScopeId,
) -> Vec<NativeVmmMigrationRecordSummary> {
    let mut records = project_sessions(snapshot, source_scope)
        .into_iter()
        .map(|session_id| NativeVmmMigrationRecordSummary {
            record_kind: String::from("session"),
            record_id: session_id.clone(),
            session_source_id: Some(session_id),
            summary: None,
        })
        .collect::<Vec<_>>();

    let mut turns = snapshot
        .dataset
        .turns
        .iter()
        .filter(|turn| &turn.scope_id == source_scope)
        .map(|turn| NativeVmmMigrationRecordSummary {
            record_kind: String::from("turn"),
            record_id: turn.id.as_str().to_owned(),
            session_source_id: Some(turn.session_source_id.as_str().to_owned()),
            summary: turn
                .analysis_details
                .as_deref()
                .map(compact_migration_preview_text),
        })
        .collect::<Vec<_>>();
    turns.sort_by(|left, right| left.record_id.cmp(&right.record_id));
    records.extend(turns);

    let mut memories = snapshot
        .dataset
        .memories
        .iter()
        .filter(|memory| &memory.scope_id == source_scope)
        .map(|memory| NativeVmmMigrationRecordSummary {
            record_kind: String::from("memory"),
            record_id: memory.id.as_str().to_owned(),
            session_source_id: memory
                .origin_session_source_id
                .as_ref()
                .map(|session| session.as_str().to_owned()),
            summary: Some(compact_migration_preview_text(&memory.summary)),
        })
        .collect::<Vec<_>>();
    memories.sort_by(|left, right| left.record_id.cmp(&right.record_id));
    records.extend(memories);

    let mut profiles = snapshot
        .dataset
        .profile_nodes
        .iter()
        .filter(|profile| &profile.scope_id == source_scope)
        .map(|profile| NativeVmmMigrationRecordSummary {
            record_kind: String::from("project_profile"),
            record_id: profile.id.as_str().to_owned(),
            session_source_id: None,
            summary: Some(compact_migration_preview_text(&profile.content)),
        })
        .collect::<Vec<_>>();
    profiles.sort_by(|left, right| left.record_id.cmp(&right.record_id));
    records.extend(profiles);
    records
}

/// Produces a bounded Unicode-safe description for one migration preview row.
/// 为一条迁移预检记录生成有界且 Unicode 安全的描述。
fn compact_migration_preview_text(value: &str) -> String {
    const MAX_PREVIEW_CHARS: usize = 160;
    let mut characters = value.trim().chars();
    let compact = characters
        .by_ref()
        .take(MAX_PREVIEW_CHARS)
        .collect::<String>();
    if characters.next().is_some() {
        format!("{compact}…")
    } else {
        compact
    }
}

/// Computes the confirmation token for the exact migration preview shown to the user.
/// 计算用户所见准确迁移预检的确认令牌。
fn migration_preview_fingerprint(
    storage_version: u64,
    source_project_id: u64,
    target_project_id: u64,
    source: &NativeVmmProjectContentCounts,
    target_before: &NativeVmmProjectContentCounts,
    target_after: &NativeVmmProjectContentCounts,
    session_renames: &[NativeVmmSessionRename],
    records: &[NativeVmmMigrationRecordSummary],
) -> Result<String, String> {
    let payload = serde_json::to_vec(&(
        storage_version,
        source_project_id,
        target_project_id,
        source,
        target_before,
        target_after,
        session_renames,
        records,
    ))
    .map_err(|error| format!("序列化原生 VMM 迁移预检指纹载荷失败：{error}"))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(payload))))
}

/// Applies a freshly recomputed migration plan to one staged snapshot.
/// 将刚刚重新计算的迁移计划应用到一份暂存快照。
fn apply_migration(
    snapshot: &mut StorageSnapshot,
    preview: &NativeVmmMigrationPreview,
) -> Result<(), String> {
    apply_migration_records(
        snapshot,
        &project_scope(preview.source_project_id)?,
        &project_scope(preview.target_project_id)?,
        &preview.session_renames,
    )
}

/// Moves source-owned records while preserving stable record identities and vector values.
/// 在保留稳定记录身份与向量值的同时迁移来源项目记录。
fn apply_migration_records(
    snapshot: &mut StorageSnapshot,
    source_scope: &ScopeId,
    target_scope: &ScopeId,
    session_renames: &[NativeVmmSessionRename],
) -> Result<(), String> {
    let rename_map = session_renames
        .iter()
        .map(|rename| {
            SessionSourceId::new(rename.to.clone())
                .map(|target| (rename.from.clone(), target))
                .map_err(|error| format!("迁移会话改名无效：{error}"))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    for turn in &mut snapshot.dataset.turns {
        if &turn.scope_id == source_scope {
            turn.scope_id = target_scope.clone();
            rename_session(&mut turn.session_source_id, &rename_map);
        }
    }
    for memory in &mut snapshot.dataset.memories {
        if &memory.scope_id == source_scope {
            memory.scope_id = target_scope.clone();
            if let Some(session) = &mut memory.origin_session_source_id {
                rename_session(session, &rename_map);
            }
        }
    }
    for node in &mut snapshot.dataset.profile_nodes {
        if &node.scope_id == source_scope {
            node.scope_id = target_scope.clone();
        }
    }
    for boundary in &mut snapshot.dataset.chat_compact_boundaries {
        if &boundary.scope_id == source_scope {
            boundary.scope_id = target_scope.clone();
            rename_session(&mut boundary.session_source_id, &rename_map);
        }
    }
    for entry in &mut snapshot.noise_prototypes {
        if &entry.identity.scope_id == source_scope {
            entry.identity.scope_id = target_scope.clone();
        }
    }
    Ok(())
}

/// Clears project records and their dependent evidence and recovery metadata.
/// 清空项目记录及其依赖的证据与恢复元数据。
fn clear_project(
    snapshot: &mut StorageSnapshot,
    project_id: u64,
) -> Result<NativeVmmProjectContentCounts, String> {
    let scope = project_scope(project_id)?;
    let removed = project_content_counts(snapshot, &scope);
    let removed_turn_ids = snapshot
        .dataset
        .turns
        .iter()
        .filter(|turn| turn.scope_id == scope)
        .map(|turn| turn.id.clone())
        .collect::<BTreeSet<_>>();
    let removed_memory_ids = snapshot
        .dataset
        .memories
        .iter()
        .filter(|memory| memory.scope_id == scope)
        .map(|memory| memory.id.clone())
        .collect::<BTreeSet<_>>();
    let removed_profile_ids = snapshot
        .dataset
        .profile_nodes
        .iter()
        .filter(|node| node.scope_id == scope)
        .map(|node| node.id.clone())
        .collect::<BTreeSet<_>>();
    let removed_evidence_ids = snapshot
        .dataset
        .context_evidence
        .iter()
        .filter(|evidence| removed_memory_ids.contains(&evidence.memory_id))
        .map(|evidence| {
            context_evidence_domain_id(
                &evidence.memory_id,
                &evidence.context_key,
                &evidence.context_value,
            )
        })
        .collect::<BTreeSet<_>>();
    let mut removed_domain_ids = removed_turn_ids
        .iter()
        .map(|value| value.as_str().to_owned())
        .chain(
            removed_memory_ids
                .iter()
                .map(|value| value.as_str().to_owned()),
        )
        .chain(
            removed_profile_ids
                .iter()
                .map(|value| value.as_str().to_owned()),
        )
        .collect::<BTreeSet<_>>();
    removed_domain_ids.extend(removed_evidence_ids);

    snapshot.dataset.turns.retain(|turn| turn.scope_id != scope);
    snapshot
        .dataset
        .memories
        .retain(|memory| memory.scope_id != scope);
    snapshot
        .dataset
        .profile_nodes
        .retain(|node| node.scope_id != scope);
    snapshot
        .dataset
        .context_evidence
        .retain(|evidence| !removed_memory_ids.contains(&evidence.memory_id));
    snapshot
        .dataset
        .chat_compact_boundaries
        .retain(|boundary| boundary.scope_id != scope);
    snapshot
        .analysis_attempts
        .retain(|attempt| !removed_turn_ids.contains(&attempt.turn_id));
    snapshot
        .noise_prototypes
        .retain(|entry| entry.identity.scope_id != scope);
    snapshot
        .outcomes
        .retain(|entry| !mutation_outcome_references_any(&entry.outcome, &removed_domain_ids));
    Ok(removed)
}

/// Counts project-owned records without treating personal profiles as project content.
/// 统计项目自有记录，不把个人画像视为项目内容。
fn project_content_counts(
    snapshot: &StorageSnapshot,
    scope: &ScopeId,
) -> NativeVmmProjectContentCounts {
    let sessions = project_sessions(snapshot, scope);
    let memory_ids = snapshot
        .dataset
        .memories
        .iter()
        .filter(|memory| &memory.scope_id == scope)
        .map(|memory| memory.id.clone())
        .collect::<BTreeSet<_>>();
    NativeVmmProjectContentCounts {
        sessions: usize_to_u64(sessions.len()),
        turns: usize_to_u64(
            snapshot
                .dataset
                .turns
                .iter()
                .filter(|turn| &turn.scope_id == scope)
                .count(),
        ),
        memories: usize_to_u64(memory_ids.len()),
        profile_nodes: usize_to_u64(
            snapshot
                .dataset
                .profile_nodes
                .iter()
                .filter(|node| &node.scope_id == scope)
                .count(),
        ),
        context_evidence: usize_to_u64(
            snapshot
                .dataset
                .context_evidence
                .iter()
                .filter(|evidence| memory_ids.contains(&evidence.memory_id))
                .count(),
        ),
        compact_boundaries: usize_to_u64(
            snapshot
                .dataset
                .chat_compact_boundaries
                .iter()
                .filter(|boundary| &boundary.scope_id == scope)
                .count(),
        ),
        vector_rows: usize_to_u64(
            snapshot
                .dataset
                .memories
                .iter()
                .filter(|memory| {
                    &memory.scope_id == scope
                        && memory.vector_space_id.is_some()
                        && memory.embedding.is_some()
                })
                .count(),
        ),
    }
}

/// Collects every session identity explicitly anchored to one project scope.
/// 收集显式锚定到一个项目作用域的每个会话身份。
fn project_sessions(snapshot: &StorageSnapshot, scope: &ScopeId) -> BTreeSet<String> {
    let mut sessions = BTreeSet::new();
    for turn in &snapshot.dataset.turns {
        if &turn.scope_id == scope {
            sessions.insert(turn.session_source_id.as_str().to_owned());
        }
    }
    for memory in &snapshot.dataset.memories {
        if &memory.scope_id == scope
            && let Some(session) = &memory.origin_session_source_id
        {
            sessions.insert(session.as_str().to_owned());
        }
    }
    for boundary in &snapshot.dataset.chat_compact_boundaries {
        if &boundary.scope_id == scope {
            sessions.insert(boundary.session_source_id.as_str().to_owned());
        }
    }
    sessions
}

/// Produces collision-free `-N` session names in deterministic source order.
/// 按确定性的来源顺序生成无冲突的 `-N` 会话名称。
fn session_rename_plan(
    snapshot: &StorageSnapshot,
    source_sessions: &BTreeSet<String>,
    target_sessions: &BTreeSet<String>,
) -> Result<Vec<NativeVmmSessionRename>, String> {
    let mut occupied = all_session_ids(snapshot);
    let mut renames = Vec::new();
    for source in source_sessions.intersection(target_sessions) {
        let mut suffix = 1_u64;
        loop {
            let candidate = suffixed_session_id(source, suffix)?;
            if occupied.insert(candidate.clone()) {
                renames.push(NativeVmmSessionRename {
                    from: source.clone(),
                    to: candidate,
                });
                break;
            }
            suffix = suffix
                .checked_add(1)
                .ok_or_else(|| String::from("迁移会话自动改名序号已耗尽"))?;
        }
    }
    Ok(renames)
}

/// Collects every durable session identity to prevent cross-project collisions.
/// 收集每个持久会话身份，以防止跨项目冲突。
fn all_session_ids(snapshot: &StorageSnapshot) -> BTreeSet<String> {
    let mut sessions = BTreeSet::new();
    for turn in &snapshot.dataset.turns {
        sessions.insert(turn.session_source_id.as_str().to_owned());
    }
    for memory in &snapshot.dataset.memories {
        if let Some(session) = &memory.origin_session_source_id {
            sessions.insert(session.as_str().to_owned());
        }
    }
    for boundary in &snapshot.dataset.chat_compact_boundaries {
        sessions.insert(boundary.session_source_id.as_str().to_owned());
    }
    sessions
}

/// Builds a valid opaque session identity ending in the requested suffix.
/// 构建以指定后缀结尾的合法不透明会话身份。
fn suffixed_session_id(base: &str, suffix: u64) -> Result<String, String> {
    let suffix = format!("-{suffix}");
    let max_base_bytes = 512_usize
        .checked_sub(suffix.len())
        .ok_or_else(|| String::from("迁移会话后缀超过身份长度限制"))?;
    let mut retained = String::new();
    for character in base.chars() {
        if retained.len() + character.len_utf8() > max_base_bytes {
            break;
        }
        retained.push(character);
    }
    let candidate = format!("{retained}{suffix}");
    SessionSourceId::new(candidate.clone())
        .map_err(|error| format!("迁移会话自动改名失败：{error}"))?;
    Ok(candidate)
}

/// Replaces one session identity only when the plan contains it.
/// 仅当计划包含该会话身份时执行替换。
fn rename_session(session: &mut SessionSourceId, rename_map: &BTreeMap<String, SessionSourceId>) {
    if let Some(target) = rename_map.get(session.as_str()) {
        *session = target.clone();
    }
}

/// Reports whether one committed outcome references a removed domain entity.
/// 报告一条已提交结果是否引用被删除的领域实体。
fn mutation_outcome_references_any(
    outcome: &MutationOutcome,
    removed_domain_ids: &BTreeSet<String>,
) -> bool {
    let MutationOutcome::Committed { receipt, .. } = outcome else {
        return false;
    };
    receipt
        .entity_bindings
        .iter()
        .any(|binding| removed_domain_ids.contains(&binding.domain_id))
        || receipt
            .committed_versions
            .iter()
            .any(|entity| removed_domain_ids.contains(&entity.domain_id))
}

/// Creates the stable Core scope assigned to one native project id.
/// 创建分配给一个原生项目 ID 的稳定 Core 作用域。
fn project_scope(project_id: u64) -> Result<ScopeId, String> {
    ScopeId::new(format!("vmm-project:{project_id}"))
        .map_err(|error| format!("原生 VMM 项目作用域无效：{error}"))
}

/// Converts an in-memory collection count into a fixed-width count.
/// 将内存集合计数转换为固定宽度计数。
fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_memorymesh_core::{ExportDataset, SubjectId, Turn, TurnId, TurnStatus};

    /// Builds one empty storage snapshot with the requested version.
    /// 使用指定版本构建一份空存储快照。
    fn empty_snapshot(storage_version: u64) -> StorageSnapshot {
        StorageSnapshot {
            dataset: ExportDataset::empty(),
            storage_version,
            next_entity_sequence: 0,
            outcomes: Vec::new(),
            analysis_attempts: Vec::new(),
            noise_prototypes: Vec::new(),
        }
    }

    /// Builds one minimal canonical turn anchored to a native project.
    /// 构建一个锚定到原生项目的最小规范回合。
    fn turn(id: &str, project_id: u64, session_id: &str) -> Turn {
        Turn {
            id: TurnId::new(id).expect("turn id"),
            subject_id: SubjectId::new("vmm-subject:1").expect("subject id"),
            scope_id: project_scope(project_id).expect("project scope"),
            session_source_id: SessionSourceId::new(session_id).expect("session id"),
            timeline: Vec::new(),
            previous_turn_ids: Vec::new(),
            next_turn_ids: Vec::new(),
            created_at_ns: 1,
            status: TurnStatus::Pending,
            user_input_kind: None,
            analysis_details: None,
        }
    }

    /// Verifies preview reports exact counts and the first globally free suffix.
    /// 验证预检报告精确计数与首个全局可用后缀。
    #[test]
    fn migration_preview_detects_session_conflicts_without_mutating_source() {
        let mut snapshot = empty_snapshot(7);
        snapshot.dataset.turns = vec![
            turn("source-turn", 2, "session-a"),
            turn("target-turn", 3, "session-a"),
            turn("other-turn", 4, "session-a-1"),
        ];

        let preview = preview_migration(&snapshot, 2, 3).expect("migration preview");

        assert_eq!(preview.storage_version, 7);
        assert_eq!(preview.source.sessions, 1);
        assert_eq!(preview.source.turns, 1);
        assert_eq!(preview.target_before.turns, 1);
        assert_eq!(preview.target_after.turns, 2);
        assert_eq!(preview.records.len(), 2);
        assert_eq!(preview.records[0].record_kind, "session");
        assert_eq!(preview.records[0].record_id, "session-a");
        assert_eq!(preview.records[1].record_kind, "turn");
        assert_eq!(preview.records[1].record_id, "source-turn");
        assert!(preview.preview_fingerprint.starts_with("sha256:"));
        assert_eq!(preview.preview_fingerprint.len(), 71);
        assert_eq!(
            preview.session_renames,
            vec![NativeVmmSessionRename {
                from: String::from("session-a"),
                to: String::from("session-a-2"),
            }]
        );
        assert_eq!(
            snapshot.dataset.turns[0].scope_id,
            project_scope(2).unwrap()
        );

        let mut changed = snapshot.clone();
        changed.dataset.turns[0].analysis_details = Some(String::from("changed detail"));
        let changed_preview = preview_migration(&changed, 2, 3).expect("changed preview");
        assert_ne!(
            preview.preview_fingerprint,
            changed_preview.preview_fingerprint
        );
    }

    /// Verifies migration changes source ownership but not the target session.
    /// 验证迁移会改变来源归属但不会修改目标会话。
    #[test]
    fn migration_moves_source_turns_and_leaves_target_turns_unchanged() {
        let mut snapshot = empty_snapshot(1);
        snapshot.dataset.turns = vec![
            turn("source-turn", 2, "session-a"),
            turn("target-turn", 3, "session-a"),
        ];
        let preview = preview_migration(&snapshot, 2, 3).expect("migration preview");

        apply_migration(&mut snapshot, &preview).expect("apply migration");

        assert_eq!(
            snapshot.dataset.turns[0].scope_id,
            project_scope(3).unwrap()
        );
        assert_eq!(
            snapshot.dataset.turns[0].session_source_id.as_str(),
            "session-a-1"
        );
        assert_eq!(
            snapshot.dataset.turns[1].session_source_id.as_str(),
            "session-a"
        );
        assert_eq!(
            project_content_counts(&snapshot, &project_scope(2).unwrap()),
            NativeVmmProjectContentCounts::default()
        );
    }

    /// Verifies clear removes only the selected project content.
    /// 验证清空只删除所选项目内容。
    #[test]
    fn clear_removes_only_selected_project_content() {
        let mut snapshot = empty_snapshot(3);
        snapshot.dataset.turns = vec![
            turn("source-turn", 2, "session-a"),
            turn("other-turn", 3, "session-b"),
        ];

        let removed = clear_project(&mut snapshot, 2).expect("clear project");

        assert_eq!(removed.sessions, 1);
        assert_eq!(removed.turns, 1);
        assert_eq!(snapshot.dataset.turns.len(), 1);
        assert_eq!(snapshot.dataset.turns[0].id.as_str(), "other-turn");
    }
}
