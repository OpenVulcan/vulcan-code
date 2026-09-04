//! Host-owned native VMM project-content maintenance.
//! 宿主持有的原生 VMM 项目内容维护。

use super::driver::NativeVmmCoreRuntime;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use vulcan_memorymesh_core::{
    MemoryCategory, MemoryLevel, MemorySourceKind, MemoryState, MutationOutcome, Role, ScopeId,
    SessionSourceId, StorageSnapshot, TurnStatus, context_evidence_domain_id,
};
use vulcan_service_api::{
    VmmNativeNoiseTestResult, VmmNativePiiFormulaTestResult, VmmNativePiiTestResult,
    VmmNativeResourceCatalogSummary, VmmNativeResourceFileContent,
    VmmNativeResourceSettingsSummary,
};
use vulcan_workspace_store::vmm_data_plane::{
    NativeVmmPendingAnalysisJob, NativeVmmProject, NativeVmmProjectAccess,
    NativeVmmProjectBindingAccess, NativeVmmProjectUsage, VmmDataPlaneHandle,
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

/// Counts durable memories by the four authoritative Core lifecycle levels.
/// 按 Core 的四个权威生命周期等级统计持久记忆。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NativeVmmMemoryLevelCounts {
    /// Number of session-level memories.
    /// 会话级记忆数量。
    pub session: u64,
    /// Number of phase-level memories.
    /// 阶段级记忆数量。
    pub phase: u64,
    /// Number of stable-level memories.
    /// 稳定级记忆数量。
    pub stable: u64,
    /// Number of persistent-level memories.
    /// 持久级记忆数量。
    pub persistent: u64,
}

/// Counts canonical turns by their durable extraction-processing state.
/// 按规范回合的持久提取处理状态进行统计。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NativeVmmTurnStatusCounts {
    /// Number of turns waiting for background analysis.
    /// 等待后台分析的回合数量。
    pub pending: u64,
    /// Number of turns whose analysis completed atomically.
    /// 已原子完成分析的回合数量。
    pub completed: u64,
    /// Number of historical turns whose exact successful-analysis metadata is unknown.
    /// 无法确定准确成功分析元数据的历史回合数量。
    pub processed_unknown: u64,
    /// Number of turns explicitly released without analysis after repeated failures.
    /// 多次失败后被明确放行且未分析的回合数量。
    pub analysis_bypassed: u64,
    /// Number of isolated turns with unreadable durable payloads.
    /// 因持久载荷不可读而被隔离的回合数量。
    pub corrupted: u64,
}

/// Summarizes all native memory records attributable to one host session.
/// 汇总可归属于一个宿主会话的全部原生记忆记录。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NativeVmmMemorySessionSummary {
    /// Stable host session identity persisted by Core.
    /// Core 持久化的稳定宿主会话标识。
    pub session_source_id: String,
    /// Number of canonical turns in the session.
    /// 会话中的规范回合数量。
    pub turn_count: u64,
    /// Turn counts split by authoritative processing state.
    /// 按权威处理状态拆分的回合统计。
    pub turn_statuses: NativeVmmTurnStatusCounts,
    /// Number of memories attributed to this session.
    /// 归属于该会话的记忆数量。
    pub memory_count: u64,
    /// Number of explicit direct writes that did not originate from a canonical turn.
    /// 不来源于规范回合的显式主动写入数量。
    pub direct_memory_count: u64,
    /// Memory counts split by authoritative lifecycle level.
    /// 按权威生命周期等级拆分的记忆统计。
    pub memory_levels: NativeVmmMemoryLevelCounts,
    /// Earliest attributable event time in Unix milliseconds.
    /// 最早可归属事件的 Unix 毫秒时间。
    pub first_activity_at_unix_ms: Option<i64>,
    /// Latest attributable event time in Unix milliseconds.
    /// 最近可归属事件的 Unix 毫秒时间。
    pub last_activity_at_unix_ms: Option<i64>,
}

/// Projects one complete canonical timeline item for read-only inspection.
/// 投影一条用于只读查看的完整规范时间线项目。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeVmmTimelineItemSummary {
    /// Stable author-role token.
    /// 稳定的作者角色标记。
    pub role: String,
    /// Full normalized timeline content.
    /// 完整的规范化时间线内容。
    pub content: String,
    /// Event time in Unix milliseconds.
    /// 事件时间的 Unix 毫秒值。
    pub occurred_at_unix_ms: i64,
}

/// Projects one canonical turn with its extraction state and generated-memory linkage.
/// 投影一个规范回合及其提取状态和生成记忆关联。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeVmmTurnRecordSummary {
    /// Stable native numeric turn identity.
    /// 稳定的原生数字回合标识。
    pub turn_id: u64,
    /// Stable host session identity owning the turn.
    /// 拥有该回合的稳定宿主会话标识。
    pub session_source_id: String,
    /// Canonical creation time in Unix milliseconds.
    /// 规范创建时间的 Unix 毫秒值。
    pub created_at_unix_ms: i64,
    /// Stable token for the authoritative Core turn status.
    /// Core 权威回合状态的稳定标记。
    pub status: String,
    /// Whether extraction is known to have completed; unknown historical state remains null.
    /// 是否已知完成提取；历史状态无法确认时保持为空。
    pub extracted: Option<bool>,
    /// Compact first user message preview.
    /// 首条用户消息的紧凑预览。
    pub user_preview: String,
    /// Compact last assistant message preview.
    /// 最后一条助手消息的紧凑预览。
    pub assistant_preview: String,
    /// Complete canonical timeline in persisted order.
    /// 按持久化顺序排列的完整规范时间线。
    pub timeline: Vec<NativeVmmTimelineItemSummary>,
    /// Durable compact analysis detail when available.
    /// 可用时的持久紧凑分析详情。
    pub analysis_details: Option<String>,
    /// Stable identities of memories extracted from this exact turn.
    /// 从该准确回合提取出的记忆稳定标识集合。
    pub generated_memory_ids: Vec<u64>,
    /// Generated memories split by authoritative lifecycle level.
    /// 生成记忆按权威生命周期等级拆分的统计。
    pub memory_levels: NativeVmmMemoryLevelCounts,
    /// Active host queue state when the turn is still awaiting analysis.
    /// 回合仍等待分析时的活动宿主队列状态。
    pub analysis_job_state: Option<String>,
    /// Number of failed background analysis attempts.
    /// 后台分析失败尝试次数。
    pub analysis_attempt_count: u32,
    /// Latest bounded background analysis error.
    /// 最近一次有界后台分析错误。
    pub analysis_last_error: Option<String>,
}

/// Projects one durable memory with exact provenance, level, and lifecycle state.
/// 投影一条持久记忆的准确来源、等级与生命周期状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeVmmMemoryRecordSummary {
    /// Stable native numeric memory identity.
    /// 稳定的原生数字记忆标识。
    pub memory_id: u64,
    /// Originating turn identity for extracted memories.
    /// 提取型记忆的来源回合标识。
    pub source_turn_id: Option<u64>,
    /// Originating host session when the memory belongs to a session flow.
    /// 记忆属于会话流程时的来源宿主会话。
    pub session_source_id: Option<String>,
    /// Compact caller-facing summary.
    /// 面向调用方的紧凑摘要。
    pub summary: String,
    /// Full normalized memory statement.
    /// 完整的规范化记忆陈述。
    pub content: String,
    /// Stable authoritative category token.
    /// 权威分类的稳定标记。
    pub category: String,
    /// Stable authoritative source-kind token.
    /// 权威来源类型的稳定标记。
    pub source_kind: String,
    /// Stable authoritative lifecycle-state token.
    /// 权威生命周期状态的稳定标记。
    pub state: String,
    /// Stable authoritative memory-level token.
    /// 权威记忆等级的稳定标记。
    pub memory_level: String,
    /// Host-normalized priority from zero through one hundred.
    /// 宿主规范化的零到一百优先级。
    pub priority: u32,
    /// Searchable labels persisted with the memory.
    /// 与记忆一起持久化的可搜索标签。
    pub tags: Vec<String>,
    /// Creation time in Unix milliseconds.
    /// 创建时间的 Unix 毫秒值。
    pub created_at_unix_ms: i64,
    /// Last update time in Unix milliseconds.
    /// 最后更新时间的 Unix 毫秒值。
    pub updated_at_unix_ms: i64,
    /// Optional absolute expiration time in Unix milliseconds.
    /// 可选的绝对过期 Unix 毫秒时间。
    pub expires_at_unix_ms: Option<i64>,
    /// Stable lifecycle reason when present.
    /// 存在时的稳定生命周期原因。
    pub status_reason: Option<String>,
}

/// Carries one project-scoped native session and memory provenance projection.
/// 携带一个项目作用域内的原生会话与记忆来源投影。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeVmmMemoryRecords {
    /// Core storage version used to build this coherent projection.
    /// 构建该一致投影所使用的 Core 存储版本。
    pub storage_version: u64,
    /// Selected one-level native VMM project identity.
    /// 已选择的一级原生 VMM 项目标识。
    pub project_id: u64,
    /// Every session attributable to the selected project.
    /// 可归属于所选项目的全部会话。
    pub sessions: Vec<NativeVmmMemorySessionSummary>,
    /// Exact selected session identity when record details were requested.
    /// 请求记录详情时的准确所选会话标识。
    pub selected_session_source_id: Option<String>,
    /// Canonical turns for the selected session, or every project turn when no session is selected.
    /// 所选会话的规范回合集合；未选择会话时为项目全部回合。
    pub turns: Vec<NativeVmmTurnRecordSummary>,
    /// Durable memories for the selected session, or every project memory when no session is selected.
    /// 所选会话的持久记忆；未选择会话时为项目全部记忆。
    pub memories: Vec<NativeVmmMemoryRecordSummary>,
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

    /// Lists native Core resources and current user-layer override state.
    /// 列出原生 Core 资源与当前用户层覆盖状态。
    ///
    /// # Errors
    /// Returns an error when persisted resource state is invalid or unreadable.
    /// 持久化资源状态无效或不可读时返回错误。
    pub(crate) async fn resource_catalog(&self) -> Result<VmmNativeResourceCatalogSummary, String> {
        self.runtime.resource_catalog().await
    }

    /// Reads one exact native Core resource file.
    /// 读取一份准确的原生 Core 资源文件。
    ///
    /// # Errors
    /// Returns an error for an unknown, unsafe, or unreadable path.
    /// 路径未知、不安全或不可读时返回错误。
    pub(crate) async fn resource_file(
        &self,
        path: &str,
    ) -> Result<VmmNativeResourceFileContent, String> {
        self.runtime.resource_file(path).await
    }

    /// Saves and activates native resource composition settings.
    /// 保存并激活原生资源组合设置。
    ///
    /// # Errors
    /// Returns an error when validation, persistence, or activation fails.
    /// 校验、持久化或激活失败时返回错误。
    pub(crate) async fn update_resource_settings(
        &self,
        settings: VmmNativeResourceSettingsSummary,
    ) -> Result<VmmNativeResourceCatalogSummary, String> {
        self.runtime.update_resource_settings(settings).await
    }

    /// Saves one exact user resource override and activates it immediately.
    /// 保存一份准确用户资源覆盖并立即激活。
    ///
    /// # Errors
    /// Returns an error for missing confirmation, invalid content, or I/O failure.
    /// 确认缺失、内容无效或 I/O 失败时返回错误。
    pub(crate) async fn write_resource_file(
        &self,
        path: &str,
        content: String,
        confirmed_purpose: bool,
        confirmed_modification: bool,
    ) -> Result<VmmNativeResourceFileContent, String> {
        self.runtime
            .write_resource_file(path, content, confirmed_purpose, confirmed_modification)
            .await
    }

    /// Restores one resource's built-in effective content.
    /// 恢复一份资源的内置生效内容。
    ///
    /// # Errors
    /// Returns an error when validation, persistence, or activation fails.
    /// 校验、持久化或激活失败时返回错误。
    pub(crate) async fn reset_resource_file(
        &self,
        path: &str,
    ) -> Result<VmmNativeResourceFileContent, String> {
        self.runtime.reset_resource_file(path).await
    }

    /// Tests the active common-plus-locale PII composition.
    /// 测试当前 common 加区域 PII 组合。
    ///
    /// # Errors
    /// Returns an error for invalid resources or oversized input.
    /// 资源无效或输入超限时返回错误。
    pub(crate) async fn test_pii(&self, text: &str) -> Result<VmmNativePiiTestResult, String> {
        self.runtime.test_pii(text).await
    }

    /// Tests one ad-hoc PII regex and condition formula without persisting it.
    /// 测试一条临时 PII 正则与条件公式，不持久化该规则。
    ///
    /// # Parameters
    /// 参数
    /// The inputs are the local sample, regex pattern, optional condition formula, and replacement.
    /// 输入依次为本地样本、正则模式、可选条件公式与替换文本。
    ///
    /// # Returns
    /// 返回值
    /// The authoritative Core evaluator result and full candidate trace.
    /// 返回权威 Core 求值器结果与完整候选轨迹。
    ///
    /// # Errors
    /// Returns an error for invalid regex, formula, capture references, or bounded text.
    /// 正则、公式、捕获引用或有界文本无效时返回错误。
    pub(crate) fn test_pii_formula(
        &self,
        text: &str,
        pattern: &str,
        condition: Option<&str>,
        replacement: &str,
    ) -> Result<VmmNativePiiFormulaTestResult, String> {
        self.runtime
            .test_pii_formula(text, pattern, condition, replacement)
    }

    /// Tests the active deterministic common-plus-locale Noise composition.
    /// 测试当前确定性 common 加区域 Noise 组合。
    ///
    /// # Errors
    /// Returns an error for an invalid role or resource snapshot.
    /// 角色或资源快照无效时返回错误。
    pub(crate) async fn test_noise(
        &self,
        role: &str,
        text: &str,
    ) -> Result<VmmNativeNoiseTestResult, String> {
        self.runtime.test_noise(role, text).await
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

    /// Loads one coherent project-scoped session, turn, and memory provenance projection.
    /// 加载一个一致的项目级会话、回合与记忆来源投影。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` identifies the one-level native VMM project to inspect.
    /// `project_id` 标识需要查看的一级原生 VMM 项目。
    /// `session_source_id` optionally selects one exact host session for record details.
    /// `session_source_id` 可选地选择一个准确宿主会话以读取记录详情。
    ///
    /// # Returns
    /// 返回值
    /// Project session summaries and, when selected, authoritative turns and memory records.
    /// 返回项目会话汇总，并在选择会话时返回权威回合与记忆记录。
    pub(crate) async fn memory_records(
        &self,
        project_id: u64,
        session_source_id: Option<&str>,
    ) -> Result<NativeVmmMemoryRecords, String> {
        // This read takes the same exclusive barrier as project maintenance so the Core snapshot
        // and the host-owned pending queue cannot be observed on different commit boundaries.
        // 该读取使用与项目维护相同的独占屏障，避免 Core 快照与宿主持有的待处理队列
        // 被观察在不同提交边界上。
        let _project_guard = self.project_mutation_gate.lock().await;
        let _operation_guard = self.operation_gate.write().await;
        self.ensure_project(project_id).await?;
        let snapshot = self.runtime.storage_snapshot()?;
        let active_jobs = self.data_plane.list_active_pending_analysis_jobs().await?;
        build_memory_records(
            &snapshot,
            project_id,
            session_source_id,
            active_jobs.as_slice(),
        )
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

/// Mutable accumulator used to build one session summary in a single dataset pass.
/// 用于通过单次数据集遍历构建一个会话汇总的可变累加器。
#[derive(Default)]
struct NativeVmmSessionAccumulator {
    /// Number of canonical turns attributed to the session.
    /// 归属于该会话的规范回合数量。
    turn_count: u64,
    /// Turn counts grouped by durable processing state.
    /// 按持久处理状态分组的回合统计。
    turn_statuses: NativeVmmTurnStatusCounts,
    /// Number of durable memories attributed to the session.
    /// 归属于该会话的持久记忆数量。
    memory_count: u64,
    /// Number of direct memories without one source turn.
    /// 不含来源回合的主动记忆数量。
    direct_memory_count: u64,
    /// Memory counts grouped by lifecycle level.
    /// 按生命周期等级分组的记忆统计。
    memory_levels: NativeVmmMemoryLevelCounts,
    /// Earliest observed event time in Unix milliseconds.
    /// 观察到的最早事件 Unix 毫秒时间。
    first_activity_at_unix_ms: Option<i64>,
    /// Latest observed event time in Unix milliseconds.
    /// 观察到的最近事件 Unix 毫秒时间。
    last_activity_at_unix_ms: Option<i64>,
}

/// Builds one read-only provenance projection from an authoritative Core snapshot and active jobs.
/// 从权威 Core 快照与活动作业构建只读来源投影。
///
/// # Parameters
/// 参数
/// `snapshot` is the coherent Core storage snapshot, `project_id` selects its project scope,
/// `session_source_id` optionally selects record details, and `active_jobs` supplies retry state.
/// `snapshot` 是一致的 Core 存储快照，`project_id` 选择其项目作用域，
/// `session_source_id` 可选地选择记录详情，`active_jobs` 提供重试状态。
///
/// # Returns
/// 返回值
/// A stable project/session/turn/memory projection or one explicit identity-integrity error.
/// 返回稳定的项目/会话/回合/记忆投影，或一个明确的身份完整性错误。
fn build_memory_records(
    snapshot: &StorageSnapshot,
    project_id: u64,
    session_source_id: Option<&str>,
    active_jobs: &[NativeVmmPendingAnalysisJob],
) -> Result<NativeVmmMemoryRecords, String> {
    let scope = project_scope(project_id)?;
    let project_session_ids = project_sessions(snapshot, &scope);
    let mut accumulators = project_session_ids
        .iter()
        .map(|session_id| (session_id.clone(), NativeVmmSessionAccumulator::default()))
        .collect::<BTreeMap<_, _>>();

    for turn in snapshot
        .dataset
        .turns
        .iter()
        .filter(|turn| turn.scope_id == scope)
    {
        let accumulator = accumulators
            .entry(turn.session_source_id.as_str().to_owned())
            .or_default();
        accumulator.turn_count = accumulator.turn_count.saturating_add(1);
        increment_turn_status(&mut accumulator.turn_statuses, turn.status);
        observe_session_activity(accumulator, unix_ns_to_ms(turn.created_at_ns));
    }
    for memory in snapshot
        .dataset
        .memories
        .iter()
        .filter(|memory| memory.scope_id == scope)
    {
        let Some(session) = memory.origin_session_source_id.as_ref() else {
            continue;
        };
        let accumulator = accumulators.entry(session.as_str().to_owned()).or_default();
        accumulator.memory_count = accumulator.memory_count.saturating_add(1);
        if memory.source_kind == MemorySourceKind::Direct {
            accumulator.direct_memory_count = accumulator.direct_memory_count.saturating_add(1);
        }
        increment_memory_level(&mut accumulator.memory_levels, memory.memory_level);
        observe_session_activity(accumulator, unix_ns_to_ms(memory.updated_at_ns));
    }
    for boundary in snapshot
        .dataset
        .chat_compact_boundaries
        .iter()
        .filter(|boundary| boundary.scope_id == scope)
    {
        let accumulator = accumulators
            .entry(boundary.session_source_id.as_str().to_owned())
            .or_default();
        observe_session_activity(accumulator, unix_ns_to_ms(boundary.compacted_at_ns));
    }

    let mut sessions = accumulators
        .into_iter()
        .map(
            |(session_source_id, accumulator)| NativeVmmMemorySessionSummary {
                session_source_id,
                turn_count: accumulator.turn_count,
                turn_statuses: accumulator.turn_statuses,
                memory_count: accumulator.memory_count,
                direct_memory_count: accumulator.direct_memory_count,
                memory_levels: accumulator.memory_levels,
                first_activity_at_unix_ms: accumulator.first_activity_at_unix_ms,
                last_activity_at_unix_ms: accumulator.last_activity_at_unix_ms,
            },
        )
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| {
        right
            .last_activity_at_unix_ms
            .cmp(&left.last_activity_at_unix_ms)
            .then_with(|| left.session_source_id.cmp(&right.session_source_id))
    });

    let selected_session_source_id = session_source_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            SessionSourceId::new(value).map_err(|error| format!("原生 VMM 会话标识无效：{error}"))
        })
        .transpose()?;
    if let Some(selected_session_source_id) = selected_session_source_id.as_ref()
        && !project_session_ids.contains(selected_session_source_id.as_str())
    {
        return Err(format!(
            "原生 VMM 会话 `{}` 不属于项目 {project_id}。",
            selected_session_source_id.as_str()
        ));
    }

    let mut active_jobs_by_turn = BTreeMap::new();
    for job in active_jobs {
        let Some(turn_id) = job.turn_id.as_ref() else {
            continue;
        };
        if active_jobs_by_turn.insert(turn_id.as_str(), job).is_some() {
            return Err(format!(
                "原生 VMM 回合 `{turn_id}` 关联了多条活动分析作业。"
            ));
        }
    }

    let selected_memories = snapshot
        .dataset
        .memories
        .iter()
        .filter(|memory| {
            memory.scope_id == scope
                && selected_session_source_id
                    .as_ref()
                    .is_none_or(|session| memory.origin_session_source_id.as_ref() == Some(session))
        })
        .collect::<Vec<_>>();
    let mut memories_by_turn = BTreeMap::<String, Vec<_>>::new();
    for memory in &selected_memories {
        if let Some(turn_id) = memory.source_turn_id.as_ref() {
            memories_by_turn
                .entry(turn_id.as_str().to_owned())
                .or_default()
                .push(*memory);
        }
    }

    let mut turns = snapshot
        .dataset
        .turns
        .iter()
        .filter(|turn| {
            turn.scope_id == scope
                && selected_session_source_id
                    .as_ref()
                    .is_none_or(|session| turn.session_source_id == *session)
        })
        .map(|turn| {
            let generated = memories_by_turn
                .get(turn.id.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            let mut generated_memory_ids = generated
                .iter()
                .map(|memory| native_numeric_id(memory.id.as_str(), "vmm-memory-", "记忆"))
                .collect::<Result<Vec<_>, _>>()?;
            generated_memory_ids.sort_unstable();
            let mut memory_levels = NativeVmmMemoryLevelCounts::default();
            for memory in generated {
                increment_memory_level(&mut memory_levels, memory.memory_level);
            }
            let active_job = active_jobs_by_turn.get(turn.id.as_str()).copied();
            Ok(NativeVmmTurnRecordSummary {
                turn_id: native_numeric_id(turn.id.as_str(), "vmm-turn-", "回合")?,
                session_source_id: turn.session_source_id.as_str().to_owned(),
                created_at_unix_ms: unix_ns_to_ms(turn.created_at_ns),
                status: turn_status_token(turn.status).to_owned(),
                extracted: turn_extracted(turn.status),
                user_preview: turn_role_preview(turn, Role::User, false),
                assistant_preview: turn_role_preview(turn, Role::Assistant, true),
                timeline: turn
                    .timeline
                    .iter()
                    .map(|item| NativeVmmTimelineItemSummary {
                        role: timeline_role_token(item.role).to_owned(),
                        content: item.content.clone(),
                        occurred_at_unix_ms: unix_ns_to_ms(item.occurred_at_ns),
                    })
                    .collect(),
                analysis_details: turn.analysis_details.clone(),
                generated_memory_ids,
                memory_levels,
                analysis_job_state: active_job.map(|job| job.state.clone()),
                analysis_attempt_count: active_job.map_or(0, |job| job.attempt_count),
                analysis_last_error: active_job.and_then(|job| job.last_error.clone()),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    turns.sort_by(|left, right| {
        right
            .created_at_unix_ms
            .cmp(&left.created_at_unix_ms)
            .then_with(|| right.turn_id.cmp(&left.turn_id))
    });

    let mut memories = selected_memories
        .into_iter()
        .map(|memory| {
            Ok(NativeVmmMemoryRecordSummary {
                memory_id: native_numeric_id(memory.id.as_str(), "vmm-memory-", "记忆")?,
                source_turn_id: memory
                    .source_turn_id
                    .as_ref()
                    .map(|turn_id| native_numeric_id(turn_id.as_str(), "vmm-turn-", "来源回合"))
                    .transpose()?,
                session_source_id: memory
                    .origin_session_source_id
                    .as_ref()
                    .map(|session| session.as_str().to_owned()),
                summary: memory.summary.clone(),
                content: memory.content.clone(),
                category: memory_category_token(memory.category).to_owned(),
                source_kind: memory_source_kind_token(memory.source_kind).to_owned(),
                state: memory_state_token(memory.state).to_owned(),
                memory_level: memory_level_token(memory.memory_level).to_owned(),
                priority: memory.priority,
                tags: memory.tags.clone(),
                created_at_unix_ms: unix_ns_to_ms(memory.created_at_ns),
                updated_at_unix_ms: unix_ns_to_ms(memory.updated_at_ns),
                expires_at_unix_ms: memory.expires_at_ns.map(unix_ns_to_ms),
                status_reason: memory.status_reason.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    memories.sort_by(|left, right| {
        right
            .created_at_unix_ms
            .cmp(&left.created_at_unix_ms)
            .then_with(|| right.memory_id.cmp(&left.memory_id))
    });

    Ok(NativeVmmMemoryRecords {
        storage_version: snapshot.storage_version,
        project_id,
        sessions,
        selected_session_source_id: selected_session_source_id
            .as_ref()
            .map(|session| session.as_str().to_owned()),
        turns,
        memories,
    })
}

/// Updates one session's first and last activity boundaries.
/// 更新一个会话的首次与最近活动边界。
fn observe_session_activity(accumulator: &mut NativeVmmSessionAccumulator, timestamp_ms: i64) {
    accumulator.first_activity_at_unix_ms = Some(
        accumulator
            .first_activity_at_unix_ms
            .map_or(timestamp_ms, |current| current.min(timestamp_ms)),
    );
    accumulator.last_activity_at_unix_ms = Some(
        accumulator
            .last_activity_at_unix_ms
            .map_or(timestamp_ms, |current| current.max(timestamp_ms)),
    );
}

/// Increments the exact bucket for one authoritative Core turn status.
/// 为一个 Core 权威回合状态递增准确统计桶。
fn increment_turn_status(counts: &mut NativeVmmTurnStatusCounts, status: TurnStatus) {
    let target = match status {
        TurnStatus::Pending => &mut counts.pending,
        TurnStatus::Completed => &mut counts.completed,
        TurnStatus::ProcessedUnknown => &mut counts.processed_unknown,
        TurnStatus::AnalysisBypassed => &mut counts.analysis_bypassed,
        TurnStatus::Corrupted => &mut counts.corrupted,
    };
    *target = target.saturating_add(1);
}

/// Increments the exact bucket for one authoritative Core memory level.
/// 为一个 Core 权威记忆等级递增准确统计桶。
fn increment_memory_level(counts: &mut NativeVmmMemoryLevelCounts, level: MemoryLevel) {
    let target = match level {
        MemoryLevel::Session => &mut counts.session,
        MemoryLevel::Phase => &mut counts.phase,
        MemoryLevel::Stable => &mut counts.stable,
        MemoryLevel::Persistent => &mut counts.persistent,
    };
    *target = target.saturating_add(1);
}

/// Converts one nanosecond Unix timestamp to the JavaScript-safe millisecond domain.
/// 将一个纳秒 Unix 时间戳转换为 JavaScript 安全的毫秒域。
fn unix_ns_to_ms(timestamp_ns: i64) -> i64 {
    timestamp_ns.div_euclid(1_000_000)
}

/// Parses a Core-generated native identity without accepting alternate prefixes.
/// 解析 Core 生成的原生身份，且不接受替代前缀。
fn native_numeric_id(value: &str, prefix: &str, label: &str) -> Result<u64, String> {
    let numeric = value
        .strip_prefix(prefix)
        .ok_or_else(|| format!("原生 VMM {label}标识 `{value}` 不符合 `{prefix}` 前缀契约。"))?;
    numeric
        .parse::<u64>()
        .map_err(|error| format!("原生 VMM {label}标识 `{value}` 不是有效数字身份：{error}"))
}

/// Returns a compact user or assistant preview from one canonical turn.
/// 从一个规范回合返回紧凑的用户或助手预览。
fn turn_role_preview(turn: &vulcan_memorymesh_core::Turn, role: Role, last: bool) -> String {
    let item = if last {
        turn.timeline.iter().rev().find(|item| item.role == role)
    } else {
        turn.timeline.iter().find(|item| item.role == role)
    };
    item.map(|item| compact_migration_preview_text(&item.content))
        .unwrap_or_default()
}

/// Maps one canonical timeline role to its stable service token.
/// 将一个规范时间线角色映射为稳定服务标记。
fn timeline_role_token(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
        Role::Tool => "tool",
    }
}

/// Maps one turn status to the stable service token and not to localized display text.
/// 将一个回合状态映射为稳定服务标记，而非本地化展示文本。
fn turn_status_token(status: TurnStatus) -> &'static str {
    match status {
        TurnStatus::Pending => "pending",
        TurnStatus::Completed => "completed",
        TurnStatus::ProcessedUnknown => "processed_unknown",
        TurnStatus::AnalysisBypassed => "analysis_bypassed",
        TurnStatus::Corrupted => "corrupted",
    }
}

/// Reports known extraction completion while preserving historical uncertainty.
/// 报告已知提取完成状态，同时保留历史不确定性。
fn turn_extracted(status: TurnStatus) -> Option<bool> {
    match status {
        TurnStatus::Completed => Some(true),
        TurnStatus::ProcessedUnknown => None,
        TurnStatus::Pending | TurnStatus::AnalysisBypassed | TurnStatus::Corrupted => Some(false),
    }
}

/// Maps one memory level to its stable service token.
/// 将一个记忆等级映射为稳定服务标记。
fn memory_level_token(level: MemoryLevel) -> &'static str {
    match level {
        MemoryLevel::Session => "session",
        MemoryLevel::Phase => "phase",
        MemoryLevel::Stable => "stable",
        MemoryLevel::Persistent => "persistent",
    }
}

/// Maps one memory state to its stable service token.
/// 将一个记忆状态映射为稳定服务标记。
fn memory_state_token(state: MemoryState) -> &'static str {
    match state {
        MemoryState::Active => "active",
        MemoryState::Superseded => "superseded",
        MemoryState::Deleted => "deleted",
        MemoryState::Expired => "expired",
        MemoryState::Recycled => "recycled",
    }
}

/// Maps one memory source kind to its stable service token.
/// 将一个记忆来源类型映射为稳定服务标记。
fn memory_source_kind_token(source_kind: MemorySourceKind) -> &'static str {
    match source_kind {
        MemorySourceKind::Turn => "turn",
        MemorySourceKind::Direct => "direct",
        MemorySourceKind::Import => "import",
        MemorySourceKind::SystemSeed => "system_seed",
        MemorySourceKind::Legacy => "legacy",
    }
}

/// Maps one authoritative memory category to its stable service token.
/// 将一个权威记忆分类映射为稳定服务标记。
fn memory_category_token(category: MemoryCategory) -> &'static str {
    match category {
        MemoryCategory::General => "general",
        MemoryCategory::ArchitectureDecision => "architecture_decision",
        MemoryCategory::TechSpecApi => "tech_spec_api",
        MemoryCategory::BusinessLogic => "business_logic",
        MemoryCategory::RequirementTodo => "requirement_todo",
        MemoryCategory::ProjectContext => "project_context",
        MemoryCategory::LogicalBugDebt => "logical_bug_debt",
        MemoryCategory::SecurityPolicy => "security_policy",
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
    use vulcan_memorymesh_core::{
        ChatCompactBoundary, ExportDataset, MemoryAccessClass, MemoryContextEvidence, MemoryId,
        MemoryRecord, MemoryScopeClass, MemoryStability, ProfileLevel, ProfileNode, ProfileNodeId,
        ProfileNodeState, ProfileSourceKind, SubjectId, TimelineItem, Turn, TurnId, TurnStatus,
        VectorSpaceId,
    };

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

    /// Builds one complete durable memory anchored to a native project and host session.
    /// 构建一条锚定到原生项目与宿主会话的完整持久记忆。
    fn memory(
        id: &str,
        project_id: u64,
        session_id: &str,
        source_turn_id: Option<&str>,
        source_kind: MemorySourceKind,
        memory_level: MemoryLevel,
    ) -> MemoryRecord {
        MemoryRecord {
            id: MemoryId::new(id).expect("memory id"),
            access_class: MemoryAccessClass::SubjectPrivate,
            source_subject_id: Some(SubjectId::new("vmm-subject:1").expect("subject id")),
            scope_id: project_scope(project_id).expect("project scope"),
            scope_class: MemoryScopeClass::Environment,
            category: MemoryCategory::General,
            summary: format!("summary-{id}"),
            content: format!("content-{id}"),
            source_kind,
            origin_session_source_id: Some(SessionSourceId::new(session_id).expect("session id")),
            priority: 20,
            stability: MemoryStability::Stable,
            tags: vec![String::from("tag")],
            evidence: Vec::new(),
            evidence_source: None,
            state: MemoryState::Active,
            importance: 0.2,
            confidence: Some(0.9),
            created_at_ns: 3_000_000,
            updated_at_ns: 4_000_000,
            expires_at_ns: None,
            last_recalled_at_ns: None,
            reinforcement_count: 0,
            memory_level,
            refresh_weight: 1,
            dedupe_hash: None,
            recalled_count: 0,
            adopted_count: 0,
            cross_session_adopted_count: 0,
            decay_disabled: false,
            status_reason: None,
            last_adopted_at_ns: None,
            last_reinforced_at_ns: None,
            source_turn_id: source_turn_id.map(|value| TurnId::new(value).expect("source turn id")),
            version: 1,
            vector_space_id: None,
            embedding: None,
        }
    }

    /// Builds one active profile node in the requested stable scope.
    /// 在指定稳定作用域中构建一个活动画像节点。
    fn profile_node(id: &str, scope_id: ScopeId, content: &str) -> ProfileNode {
        ProfileNode {
            id: ProfileNodeId::new(id).expect("profile id"),
            subject_id: Some(SubjectId::new("vmm-subject:1").expect("subject id")),
            scope_id,
            content: content.to_owned(),
            confidence: Some(0.9),
            state: ProfileNodeState::Active,
            source_turn_ids: Vec::new(),
            source_kind: ProfileSourceKind::SystemSeed,
            source_id: None,
            evidence_source: None,
            priority: 60,
            profile_level: ProfileLevel::Stable,
            level_reason: None,
            refresh_weight: 0,
            effective_date_ns: Some(1),
            expires_at_ns: None,
            status_reason: None,
            superseded_by: None,
            version: 1,
            created_at_ns: 1,
            updated_at_ns: 1,
        }
    }

    /// Verifies session browsing exposes extraction states, retries, levels, and memory provenance.
    /// 验证会话浏览会暴露提取状态、重试、等级与记忆来源。
    #[test]
    fn memory_records_expose_turn_extraction_and_generated_memories() {
        let mut snapshot = empty_snapshot(12);
        let mut completed = turn("vmm-turn-11", 2, "session-a");
        completed.status = TurnStatus::Completed;
        completed.created_at_ns = 1_000_000;
        completed.analysis_details = Some(String::from("two candidates accepted"));
        completed.timeline = vec![
            TimelineItem {
                role: Role::User,
                content: String::from("remember apples"),
                occurred_at_ns: 1_000_000,
            },
            TimelineItem {
                role: Role::Assistant,
                content: String::from("noted"),
                occurred_at_ns: 2_000_000,
            },
        ];
        let mut pending = turn("vmm-turn-12", 2, "session-a");
        pending.created_at_ns = 5_000_000;
        snapshot.dataset.turns = vec![completed, pending, turn("vmm-turn-99", 3, "other")];
        snapshot.dataset.memories = vec![
            memory(
                "vmm-memory-21",
                2,
                "session-a",
                Some("vmm-turn-11"),
                MemorySourceKind::Turn,
                MemoryLevel::Stable,
            ),
            memory(
                "vmm-memory-22",
                2,
                "session-a",
                None,
                MemorySourceKind::Direct,
                MemoryLevel::Phase,
            ),
            memory(
                "vmm-memory-99",
                3,
                "other",
                Some("vmm-turn-99"),
                MemorySourceKind::Turn,
                MemoryLevel::Persistent,
            ),
        ];
        let jobs = vec![NativeVmmPendingAnalysisJob {
            operation_id: String::from("operation"),
            request_fingerprint: String::from("fingerprint"),
            payload_json: String::from("{}"),
            intake_mutation_operation_id: String::from("intake"),
            turn_id: Some(String::from("vmm-turn-12")),
            state: String::from("ready"),
            attempt_count: 2,
            next_attempt_at_unix_ms: 8,
            last_error: Some(String::from("provider timeout")),
        }];

        let records = build_memory_records(&snapshot, 2, Some("session-a"), &jobs)
            .expect("native memory records");

        assert_eq!(records.storage_version, 12);
        assert_eq!(records.sessions.len(), 1);
        assert_eq!(records.sessions[0].turn_count, 2);
        assert_eq!(records.sessions[0].turn_statuses.completed, 1);
        assert_eq!(records.sessions[0].turn_statuses.pending, 1);
        assert_eq!(records.sessions[0].memory_count, 2);
        assert_eq!(records.sessions[0].direct_memory_count, 1);
        assert_eq!(records.sessions[0].memory_levels.phase, 1);
        assert_eq!(records.sessions[0].memory_levels.stable, 1);
        assert_eq!(records.turns.len(), 2);
        assert_eq!(records.turns[0].turn_id, 12);
        assert_eq!(records.turns[0].extracted, Some(false));
        assert_eq!(records.turns[0].analysis_attempt_count, 2);
        assert_eq!(
            records.turns[0].analysis_last_error.as_deref(),
            Some("provider timeout")
        );
        assert_eq!(records.turns[1].turn_id, 11);
        assert_eq!(records.turns[1].extracted, Some(true));
        assert_eq!(records.turns[1].generated_memory_ids, vec![21]);
        assert_eq!(records.turns[1].timeline.len(), 2);
        assert_eq!(records.memories.len(), 2);
        assert_eq!(records.memories[0].memory_id, 22);
        assert_eq!(records.memories[0].source_kind, "direct");
        assert_eq!(records.memories[1].source_turn_id, Some(11));
    }

    /// Verifies that project-level browsing returns every project record, including sessionless memories.
    /// 验证项目级浏览返回项目全部记录，包括没有会话归属的记忆。
    #[test]
    fn project_memory_records_include_sessionless_project_memories() {
        let mut snapshot = empty_snapshot(13);
        let mut completed = turn("vmm-turn-31", 2, "session-a");
        completed.status = TurnStatus::Completed;
        let session_memory = memory(
            "vmm-memory-41",
            2,
            "session-a",
            Some("vmm-turn-31"),
            MemorySourceKind::Turn,
            MemoryLevel::Stable,
        );
        let mut project_memory = memory(
            "vmm-memory-42",
            2,
            "unused-session",
            None,
            MemorySourceKind::SystemSeed,
            MemoryLevel::Persistent,
        );
        project_memory.origin_session_source_id = None;
        snapshot.dataset.turns = vec![completed, turn("vmm-turn-99", 3, "other")];
        snapshot.dataset.memories = vec![
            session_memory,
            project_memory,
            memory(
                "vmm-memory-99",
                3,
                "other",
                Some("vmm-turn-99"),
                MemorySourceKind::Turn,
                MemoryLevel::Persistent,
            ),
        ];

        let overview = build_memory_records(&snapshot, 2, None, &[])
            .expect("project-level native memory records");

        assert_eq!(overview.selected_session_source_id, None);
        assert_eq!(overview.sessions.len(), 1);
        assert_eq!(overview.turns.len(), 1);
        assert_eq!(overview.turns[0].turn_id, 31);
        assert_eq!(overview.memories.len(), 2);
        assert!(
            overview
                .memories
                .iter()
                .any(|memory| { memory.memory_id == 42 && memory.session_source_id.is_none() })
        );

        let session = build_memory_records(&snapshot, 2, Some("session-a"), &[])
            .expect("session-level native memory records");
        assert_eq!(session.turns.len(), 1);
        assert_eq!(session.memories.len(), 1);
        assert_eq!(session.memories[0].memory_id, 41);
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

    /// Verifies migration moves every project record without rebuilding vectors or moving the user profile.
    /// 验证迁移会移动全部项目记录，且不重建向量、不移动用户画像。
    #[test]
    fn migration_moves_project_records_and_preserves_vector_identity() {
        let mut snapshot = empty_snapshot(1);
        snapshot.dataset.turns = vec![
            turn("source-turn", 2, "session-a"),
            turn("target-turn", 3, "session-a"),
        ];
        let mut source_memory = memory(
            "vmm-memory-1",
            2,
            "session-a",
            Some("source-turn"),
            MemorySourceKind::Turn,
            MemoryLevel::Stable,
        );
        source_memory.vector_space_id =
            Some(VectorSpaceId::new("vmm-default").expect("vector space"));
        source_memory.embedding = Some(vec![0.25, 0.75]);
        snapshot.dataset.memories = vec![source_memory];
        snapshot.dataset.profile_nodes = vec![
            profile_node(
                "vmm-profile-1",
                project_scope(2).unwrap(),
                "project preference",
            ),
            profile_node(
                "vmm-profile-2",
                ScopeId::new("vmm-subject:1").expect("subject scope"),
                "user preference",
            ),
        ];
        snapshot.dataset.chat_compact_boundaries = vec![ChatCompactBoundary {
            subject_id: SubjectId::new("vmm-subject:1").expect("subject id"),
            scope_id: project_scope(2).unwrap(),
            session_source_id: SessionSourceId::new("session-a").expect("session id"),
            compacted_turn_id: TurnId::new("source-turn").expect("turn id"),
            compacted_at_ns: 2,
        }];
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
        assert_eq!(snapshot.dataset.memories[0].id.as_str(), "vmm-memory-1");
        assert_eq!(
            snapshot.dataset.memories[0].scope_id,
            project_scope(3).unwrap()
        );
        assert_eq!(
            snapshot.dataset.memories[0]
                .origin_session_source_id
                .as_ref()
                .map(SessionSourceId::as_str),
            Some("session-a-1")
        );
        assert_eq!(
            snapshot.dataset.memories[0]
                .vector_space_id
                .as_ref()
                .map(VectorSpaceId::as_str),
            Some("vmm-default")
        );
        assert_eq!(
            snapshot.dataset.memories[0].embedding,
            Some(vec![0.25, 0.75])
        );
        assert_eq!(
            snapshot.dataset.profile_nodes[0].scope_id,
            project_scope(3).unwrap()
        );
        assert_eq!(
            snapshot.dataset.profile_nodes[1].scope_id,
            ScopeId::new("vmm-subject:1").expect("subject scope")
        );
        assert_eq!(
            snapshot.dataset.chat_compact_boundaries[0].scope_id,
            project_scope(3).unwrap()
        );
        assert_eq!(
            snapshot.dataset.chat_compact_boundaries[0]
                .session_source_id
                .as_str(),
            "session-a-1"
        );
        assert_eq!(
            project_content_counts(&snapshot, &project_scope(2).unwrap()),
            NativeVmmProjectContentCounts::default()
        );
    }

    /// Verifies clear removes all selected project data and preserves the fixed personal profile.
    /// 验证清空会删除所选项目全部数据并保留固定个人画像。
    #[test]
    fn clear_removes_project_records_and_preserves_personal_profile() {
        let mut snapshot = empty_snapshot(3);
        snapshot.dataset.turns = vec![
            turn("source-turn", 2, "session-a"),
            turn("other-turn", 3, "session-b"),
        ];
        let mut source_memory = memory(
            "vmm-memory-1",
            2,
            "session-a",
            Some("source-turn"),
            MemorySourceKind::Turn,
            MemoryLevel::Persistent,
        );
        source_memory.vector_space_id =
            Some(VectorSpaceId::new("vmm-default").expect("vector space"));
        source_memory.embedding = Some(vec![1.0, 0.0]);
        snapshot.dataset.memories = vec![source_memory];
        snapshot.dataset.context_evidence = vec![MemoryContextEvidence {
            memory_id: MemoryId::new("vmm-memory-1").expect("memory id"),
            context_key: String::from("workspace"),
            context_value: String::from("vulcan-code"),
            support_count: 1,
            rebuttal_count: 0,
            last_supported_at_ns: Some(1),
            last_rebutted_at_ns: None,
            version: 1,
        }];
        snapshot.dataset.profile_nodes = vec![
            profile_node(
                "vmm-profile-1",
                project_scope(2).unwrap(),
                "project preference",
            ),
            profile_node(
                "vmm-profile-2",
                ScopeId::new("vmm-subject:1").expect("subject scope"),
                "user preference",
            ),
        ];
        snapshot.dataset.chat_compact_boundaries = vec![ChatCompactBoundary {
            subject_id: SubjectId::new("vmm-subject:1").expect("subject id"),
            scope_id: project_scope(2).unwrap(),
            session_source_id: SessionSourceId::new("session-a").expect("session id"),
            compacted_turn_id: TurnId::new("source-turn").expect("turn id"),
            compacted_at_ns: 2,
        }];

        let removed = clear_project(&mut snapshot, 2).expect("clear project");

        assert_eq!(removed.sessions, 1);
        assert_eq!(removed.turns, 1);
        assert_eq!(removed.memories, 1);
        assert_eq!(removed.profile_nodes, 1);
        assert_eq!(removed.context_evidence, 1);
        assert_eq!(removed.compact_boundaries, 1);
        assert_eq!(removed.vector_rows, 1);
        assert_eq!(snapshot.dataset.turns.len(), 1);
        assert_eq!(snapshot.dataset.turns[0].id.as_str(), "other-turn");
        assert!(snapshot.dataset.memories.is_empty());
        assert!(snapshot.dataset.context_evidence.is_empty());
        assert!(snapshot.dataset.chat_compact_boundaries.is_empty());
        assert_eq!(snapshot.dataset.profile_nodes.len(), 1);
        assert_eq!(
            snapshot.dataset.profile_nodes[0].id.as_str(),
            "vmm-profile-2"
        );
        assert_eq!(
            snapshot.dataset.profile_nodes[0].scope_id,
            ScopeId::new("vmm-subject:1").expect("subject scope")
        );
    }
}
