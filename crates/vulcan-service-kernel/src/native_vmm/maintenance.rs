//! Host-owned native VMM project-content maintenance.
//! 宿主持有的原生 VMM 项目内容维护。

use super::driver::{NativeVmmCoreRuntime, NativeVmmVectorSpaceDescriptor};
use super::retention_policy;
use super::turn_details::resolve_turn_details;

#[path = "binding_cache_projection.rs"]
mod binding_cache_projection;
#[path = "binding_review.rs"]
mod binding_review;
#[path = "content_projection.rs"]
mod content_projection;
pub(super) use content_projection::NativeVmmContentProjection;
#[path = "lexical_maintenance.rs"]
mod lexical_maintenance;
#[path = "manual_removal.rs"]
mod manual_removal;
#[path = "manual_session_lifecycle.rs"]
mod manual_session_lifecycle;
#[path = "profile_cache.rs"]
mod profile_cache;
#[path = "recall_publication.rs"]
mod recall_publication;
#[path = "recycle_records.rs"]
mod recycle_records;
#[path = "removal_impact.rs"]
mod removal_impact;
pub(crate) use recall_publication::NativeVmmRecallPublication;
#[path = "retention_preview.rs"]
mod retention_preview;
#[path = "session_archive.rs"]
mod session_archive;
#[path = "session_restart.rs"]
mod session_restart;

#[cfg(test)]
#[path = "retention_worker_tests.rs"]
pub(super) mod retention_worker_tests;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, Notify, RwLock};
use vulcan_memorymesh_core::{
    CommandResult, CoreCommand, ExportDataset, IdempotencyKey, MaintenanceAction,
    MaintenanceCommand, MemoryAccessIdentity, MemoryCategory, MemoryId, MemoryLevel,
    MemoryScopeClass, MemorySourceKind, MemoryState, MutationIdentity, MutationOutcome,
    MutationSourceKind, OperationId, ProfileLevel, ProfileNode, ProfileNodeState,
    ProfileSourceKind, RetentionCommand, RetentionPlanSet, RetentionPolicy, RetentionReasonCode,
    Role, ScopeAccess, ScopeId, SessionSourceId, StorageRetentionColdTurnJob,
    StorageRetentionTrashBatch, StorageSnapshot, SubjectId, TurnId, TurnStatus, VectorSpaceId,
    context_evidence_domain_id,
};
use vulcan_service_api::{
    VmmNativeNoiseTestResult, VmmNativePiiFormulaTestResult, VmmNativePiiTestResult,
    VmmNativeRecallPolicySummary, VmmNativeResourceCatalogSummary, VmmNativeResourceFileContent,
    VmmNativeResourceSettingsSummary, VmmNativeRuntimePolicySummary,
};
use vulcan_service_api::{VmmNativeRetentionSettingsSummary, VmmNativeRetrievalPolicySummary};
use vulcan_workspace_store::vmm_data_plane::{
    NativeVmmPendingAnalysisJob, NativeVmmProject, NativeVmmProjectAccess,
    NativeVmmProjectBindingAccess, NativeVmmProjectUsage, VMM_PENDING_ANALYSIS_BYPASSED,
    VMM_PENDING_ANALYSIS_COMPLETED, VMM_PENDING_ANALYSIS_CORRUPTED, VmmDataPlaneHandle,
};

/// Creates the same scoped command for scheduled execution and isolated manual previews.
/// 为周期执行和隔离人工预览创建相同作用域的命令。
/// Takes the project, evaluation time, frozen policy and unique operation; returns a validated command.
/// 接收项目、评估时间、冻结策略和唯一操作标识，返回已校验命令。
fn retention_command(
    project_id: u64,
    now_ns: i64,
    policy: &RetentionPolicy,
    operation: String,
) -> Result<RetentionCommand, String> {
    // The write scope remains the project while the fixed personal scope is visible for expiry.
    // 写入作用域仍为项目，固定个人作用域仅参与可见的到期收敛。
    let scope_id = project_scope(project_id)?;
    Ok(RetentionCommand {
        mutation_identity: MutationIdentity {
            operation_id: OperationId::new(operation.clone()).map_err(|error| error.to_string())?,
            idempotency_key: IdempotencyKey::new(operation).map_err(|error| error.to_string())?,
            request_fingerprint: format!("fp256:{}", "0".repeat(64)),
            source_kind: MutationSourceKind::BackgroundWorker,
            request_source_id: format!("native-vmm-retention:{project_id}"),
            observed_at_ns: now_ns,
        },
        subject_id: native_vmm_subject_id()?,
        scope_access: ScopeAccess {
            write_scope: scope_id.clone(),
            visible_scopes: vec![scope_id, native_vmm_subject_scope()?],
        },
        now_ns,
        policy: policy.clone(),
    })
}

/// Identifies the sole host-owned user used by native VMM subject and personal-profile scope.
/// 标识原生 VMM 主体与个人画像作用域使用的唯一宿主持有用户。
const NATIVE_VMM_USER_ID: u64 = 1;
/// Bounds detection latency for policy files edited outside the settings page.
/// 限制设置页面之外编辑策略文件的检测延迟。
const NATIVE_VMM_POLICY_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
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
    /// Persisted lifecycle; absent for content-only sessions without extraction metadata.
    /// 持久化生命周期；仅有内容、没有提取元数据的会话为空。
    pub status: Option<vulcan_memorymesh_core::StorageSessionStatus>,
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
    /// Number of user or project profile nodes attributed to turns in this session.
    /// 可归属于该会话回合的用户或项目画像节点数量。
    pub profile_node_count: u64,
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
    /// Current preceding same-owner turn IDs, ordered by numeric identity.
    /// 当前同归属的前序回合 ID，按数字身份排序。
    pub previous_turn_ids: Vec<u64>,
    /// Current following same-owner turn IDs, ordered by numeric identity.
    /// 当前同归属的后续回合 ID，按数字身份排序。
    pub next_turn_ids: Vec<u64>,
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
    /// Stable identities of user or project profile nodes extracted from this exact turn.
    /// 从该准确回合提取出的用户或项目画像节点稳定标识集合。
    pub generated_profile_node_ids: Vec<u64>,
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
    /// User and project profile nodes attributable to the selected turns.
    /// 可归属于所选回合的用户与项目画像节点。
    pub profile_nodes: Vec<NativeVmmProfileRecordSummary>,
}

/// Selects the sole native profile class exposed by one management read.
/// 选择一次管理读取所暴露的唯一原生画像类别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeVmmProfileTarget {
    /// Selects the fixed host-owned personal profile.
    /// 选择宿主持有的固定个人画像。
    User,
    /// Selects one one-level native project profile.
    /// 选择一个一级原生项目画像。
    Project,
}

/// Counts native profile nodes by their effective read-time lifecycle state.
/// 按读取时的有效生命周期状态统计原生画像节点。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NativeVmmProfileStateCounts {
    /// Number of nodes currently eligible for profile rendering and injection.
    /// 当前可参与画像渲染与注入的节点数量。
    pub active: u64,
    /// Number of nodes retained after deactivation or replacement.
    /// 停用或替换后保留的节点数量。
    pub inactive: u64,
    /// Number of extracted nodes awaiting an authoritative review result.
    /// 等待权威评审结果的已提取节点数量。
    pub pending: u64,
    /// Number of nodes past their lifecycle deadline.
    /// 已超过生命周期截止时间的节点数量。
    pub expired: u64,
    /// Number of active nodes whose effective date has not arrived yet.
    /// 生效日期尚未到达的活动节点数量。
    pub scheduled: u64,
}

/// Projects one native profile node for complete lifecycle inspection.
/// 投影一条原生画像节点以供完整生命周期检查。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeVmmProfileRecordSummary {
    /// Stable native numeric profile-node identity.
    /// 稳定的原生数字画像节点标识。
    pub profile_node_id: u64,
    /// Stable profile target token: user or project.
    /// 稳定的画像目标标记：user 或 project。
    pub target: String,
    /// Persisted authoritative lifecycle state.
    /// 已持久化的权威生命周期状态。
    pub stored_state: String,
    /// Effective read-time lifecycle state after date boundaries are applied.
    /// 应用日期边界后的读取时有效生命周期状态。
    pub effective_state: String,
    /// Whether the node is currently eligible for profile rendering and prompt injection.
    /// 该节点当前是否可参与画像渲染与提示词注入。
    pub active_for_injection: bool,
    /// Atomic profile statement.
    /// 原子化画像陈述。
    pub content: String,
    /// Host-normalized priority from zero through one hundred.
    /// 宿主规范化后的零到一百优先级。
    pub priority: u32,
    /// Stable profile lifecycle level token.
    /// 稳定的画像生命周期等级标记。
    pub level: String,
    /// Stable durable source-kind token.
    /// 稳定的持久来源类别标记。
    pub source_kind: String,
    /// Exact durable source identity when present.
    /// 存在时的准确持久来源标识。
    pub source_id: Option<String>,
    /// Supporting canonical turn identities.
    /// 提供支撑的规范回合标识。
    pub source_turn_ids: Vec<u64>,
    /// Optional effective date in Unix milliseconds.
    /// 可选的生效日期 Unix 毫秒值。
    pub effective_date_at_unix_ms: Option<i64>,
    /// Optional absolute expiration time in Unix milliseconds.
    /// 可选的绝对过期 Unix 毫秒值。
    pub expires_at_unix_ms: Option<i64>,
    /// Stable lifecycle reason when present.
    /// 存在时的稳定生命周期原因。
    pub status_reason: Option<String>,
    /// Last update time in Unix milliseconds.
    /// 最后更新时间的 Unix 毫秒值。
    pub updated_at_unix_ms: i64,
}

/// Carries every native profile state for one explicit user or project target.
/// 携带一个明确用户或项目目标的全部原生画像状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeVmmProfileRecords {
    /// Core storage version used to build this coherent projection.
    /// 构建该一致投影所使用的 Core 存储版本。
    pub storage_version: u64,
    /// Selected project used for project scope or user-profile display context.
    /// 用于项目作用域或用户画像显示上下文的所选项目。
    pub project_id: u64,
    /// Stable selected target token: user or project.
    /// 稳定的所选目标标记：user 或 project。
    pub target: String,
    /// Effective state counts covering every returned node.
    /// 覆盖全部返回节点的有效状态统计。
    pub counts: NativeVmmProfileStateCounts,
    /// Every durable profile node for the selected target, including non-active states.
    /// 所选目标的全部持久画像节点，包括非活动状态。
    pub nodes: Vec<NativeVmmProfileRecordSummary>,
}

/// Summarizes one durable native retention recycle batch for human maintenance.
/// 汇总一个供人工维护使用的耐久原生保留回收批次。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NativeVmmRecycleBatchSummary {
    /// Exact persisted manual identities, including selected sessions with no body records.
    /// 准确持久化人工身份，包含没有正文记录的所选会话。
    pub manual_selection: Option<vulcan_service_api::VmmNativeRemovalTarget>,
    /// Stable batch identity used by restore and purge commands.
    /// 恢复与清除命令使用的稳定批次身份。
    pub batch_id: String,
    /// One-level native project that owns the batch.
    /// 拥有该批次的一级原生项目。
    pub project_id: u64,
    /// Stable retention reason token.
    /// 稳定的保留原因标记。
    pub reason: String,
    /// Recycle timestamp in Unix milliseconds.
    /// 回收时间的 Unix 毫秒值。
    pub recycled_at_unix_ms: i64,
    /// Automatic permanent-purge deadline in Unix milliseconds.
    /// 自动永久清除截止时间的 Unix 毫秒值。
    pub expires_at_unix_ms: i64,
    /// Number of recycled turns in the batch.
    /// 批次中的已回收回合数量。
    pub turn_count: u64,
    /// Number of recycled memories in the batch.
    /// 批次中的已回收记忆数量。
    pub memory_count: u64,
    /// Number of recycled profile nodes in the batch.
    /// 批次中的已回收画像节点数量。
    pub profile_node_count: u64,
    /// Number of recycled memory-context evidence rows in the batch.
    /// 批次中的已回收记忆情境证据行数量。
    pub context_evidence_count: u64,
}

/// Carries one coherent native recycle-bin projection.
/// 携带一个一致的原生回收站投影。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NativeVmmRecycleBin {
    /// Core storage version that restore or purge confirmation must match.
    /// 恢复或清除确认必须匹配的 Core 存储版本。
    pub storage_version: u64,
    /// Recycle batches in newest-first order.
    /// 按最新优先顺序排列的回收批次。
    pub batches: Vec<NativeVmmRecycleBatchSummary>,
}

/// Reports the latest and current process-local native retention state.
/// 报告最近一次以及当前进程内原生保留状态。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NativeVmmRetentionStatus {
    /// Whether both the native backend and periodic cleanup switch are enabled.
    /// 原生后端和周期清洗开关是否同时启用。
    pub enabled: bool,
    /// Whether one manual or scheduled pass is currently running.
    /// 当前是否正在运行一次人工或定时维护。
    pub running: bool,
    /// Whether the independent profile-expiry convergence worker is currently running.
    /// 独立画像过期收敛工作器当前是否正在运行。
    pub profile_expiry_running: bool,
    /// Effective configured scheduled interval in minutes.
    /// 当前生效的已配置定时周期分钟数。
    pub interval_minutes: u64,
    /// Effective configured profile-expiry convergence interval in seconds.
    /// 当前生效的已配置画像过期收敛周期秒数。
    pub profile_expiry_interval_seconds: u64,
    /// Frozen VMM-aligned retention policy.
    /// 已冻结且与 VMM 对齐的保留策略。
    pub policy: RetentionPolicy,
    /// Start time of the latest attempted pass in Unix milliseconds.
    /// 最近一次尝试运行的开始时间 Unix 毫秒值。
    pub last_started_at_unix_ms: Option<i64>,
    /// Finish time of the latest attempted pass in Unix milliseconds.
    /// 最近一次尝试运行的结束时间 Unix 毫秒值。
    pub last_finished_at_unix_ms: Option<i64>,
    /// Latest exact failure, absent after a successful pass.
    /// 最近一次准确失败；成功运行后为空。
    pub last_error: Option<String>,
    /// Aggregate counts from the latest completed pass, including project-local failures.
    /// 最近一次已完成运行的汇总计数，包括项目局部失败。
    pub last_result: Option<NativeVmmRetentionApplyResult>,
    /// Start time of the latest profile-expiry convergence in Unix milliseconds.
    /// 最近一次画像过期收敛开始时间的 Unix 毫秒值。
    pub profile_expiry_last_started_at_unix_ms: Option<i64>,
    /// Finish time of the latest profile-expiry convergence in Unix milliseconds.
    /// 最近一次画像过期收敛结束时间的 Unix 毫秒值。
    pub profile_expiry_last_finished_at_unix_ms: Option<i64>,
    /// Latest exact profile-expiry failure, absent after a successful convergence.
    /// 最近一次准确画像过期失败；成功收敛后为空。
    pub profile_expiry_last_error: Option<String>,
    /// Aggregate result of the latest profile-expiry convergence.
    /// 最近一次画像过期收敛的汇总结果。
    pub profile_expiry_last_result: Option<NativeVmmRetentionApplyResult>,
    /// Current durable cold-turn job count.
    /// 当前耐久冷回合任务数量。
    pub pending_cold_jobs: u64,
    /// Current durable recycle-batch count.
    /// 当前耐久回收批次数量。
    pub recycle_batch_count: u64,
}

/// Reports whether native active and recoverable memories match the configured embedding generation.
/// 报告原生活动记忆与可恢复记忆是否匹配当前已配置 Embedding 代次。
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NativeVmmVectorStatus {
    /// Whether the current embedding route can produce an exact vector generation.
    /// 当前 Embedding 路由是否能够产生准确向量代次。
    pub configured: bool,
    /// Current route-derived logical vector-space identity when configured.
    /// 已配置时当前由路由派生的逻辑向量空间标识。
    pub current_vector_space_id: Option<String>,
    /// Current configured dense embedding width when available.
    /// 可用时当前已配置的稠密向量维度。
    pub embedding_dimension: Option<u32>,
    /// Number of active memories regardless of vector generation.
    /// 不区分向量代次的活动记忆总数。
    pub active_memory_count: u64,
    /// Number of memory records retained in recoverable recycle batches.
    /// 可恢复回收批次中保留的记忆记录数量。
    pub recycle_bin_memory_count: u64,
    /// Number of active memories already using the current generation.
    /// 已使用当前代次的活动记忆数量。
    pub current_memory_count: u64,
    /// Number of recoverable memories already using the current generation.
    /// 已使用当前代次的可恢复记忆数量。
    pub current_recycle_bin_memory_count: u64,
    /// Number of active memories that must be rebuilt before normal vector operations resume.
    /// 普通向量操作恢复前必须重建的活动记忆数量。
    pub rebuild_required_memory_count: u64,
    /// Number of incompatible active memories included in the total rebuild count.
    /// 总重建数量中不兼容活动记忆的数量。
    pub active_rebuild_required_memory_count: u64,
    /// Number of incompatible recoverable memories included in the total rebuild count.
    /// 总重建数量中不兼容可恢复记忆的数量。
    pub recycle_bin_rebuild_required_memory_count: u64,
    /// Number of distinct non-current logical spaces referenced by active memories.
    /// 活动记忆引用的不同非当前逻辑空间数量。
    pub legacy_vector_space_count: u64,
    /// Number of durable turns still awaiting PostAction analysis.
    /// 仍等待 PostAction 分析的耐久回合数量。
    pub pending_analysis_job_count: u64,
    /// Exact configuration failure when no current generation can be resolved.
    /// 无法解析当前代次时的准确配置失败。
    pub error: Option<String>,
}

/// Carries the exact side-effect-free preview required before rebuilding native vectors.
/// 携带重建原生向量前所需的准确无副作用预检。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NativeVmmVectorRebuildPreview {
    /// Core storage version that must still match at confirmation time.
    /// 确认时必须仍然匹配的 Core 存储版本。
    pub storage_version: u64,
    /// Target route-derived logical vector-space identity.
    /// 目标路由派生的逻辑向量空间标识。
    pub target_vector_space_id: String,
    /// Exact target embedding width.
    /// 准确目标向量维度。
    pub embedding_dimension: u32,
    /// Total number of active memories examined by the preview.
    /// 预检检查的活动记忆总数。
    pub active_memory_count: u64,
    /// Total number of recoverable recycle-bin memories examined by the preview.
    /// 预检检查的可恢复回收站记忆总数。
    pub recycle_bin_memory_count: u64,
    /// Number of active memories requiring a new embedding.
    /// 需要生成新向量的活动记忆数量。
    pub rebuild_memory_count: u64,
    /// Active memories requiring a new embedding.
    /// 需要生成新向量的活动记忆数量。
    pub active_rebuild_memory_count: u64,
    /// Recoverable memories requiring a new embedding before they can be safely restored.
    /// 为确保可安全恢复而需要生成新向量的可恢复记忆数量。
    pub recycle_bin_rebuild_memory_count: u64,
    /// Number of project scopes containing rebuild targets.
    /// 包含重建目标的项目作用域数量。
    pub affected_project_count: u64,
    /// Exact number of Core rebuild batches required at the 256-record contract limit.
    /// 按 Core 每批 256 条契约上限计算的准确重建批次数量。
    pub batch_count: u64,
    /// SHA-256 confirmation identity of the exact displayed target set.
    /// 所显示准确目标集合的 SHA-256 确认身份。
    pub preview_fingerprint: String,
}

/// Summarizes one completed or resumed native vector-generation rebuild.
/// 汇总一次已完成或续跑完成的原生向量代次重建。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NativeVmmVectorRebuildResult {
    /// Final committed Core storage version.
    /// 最终提交的 Core 存储版本。
    pub storage_version: u64,
    /// Target route-derived logical vector-space identity.
    /// 目标路由派生的逻辑向量空间标识。
    pub target_vector_space_id: String,
    /// Number of memories rebuilt during this execution.
    /// 本次执行重建的记忆数量。
    pub rebuilt_memory_count: u64,
    /// Number of committed Core rebuild batches.
    /// 已提交的 Core 重建批次数量。
    pub batch_count: u64,
}

/// Carries one journal-replayable recycle-bin vector batch outcome.
/// 携带一个可通过账本重放的回收站向量批次结果。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct NativeVmmRecycleVectorBatchResult {
    /// Exact memory identities whose retained vectors changed generation.
    /// 已变更保留向量代次的准确记忆身份。
    rebuilt_memory_ids: Vec<String>,
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

/// Summarizes one atomic native retention snapshot transition.
/// 汇总一次原生保留快照原子转换。
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NativeVmmRetentionApplyResult {
    /// Counts due active profile nodes materialized as expired.
    /// 统计被真实收敛为过期状态的到期活跃画像节点数量。
    pub expired_profile_nodes: u64,
    /// Counts terminal memories moved into durable trash.
    /// 统计移入耐久回收站的终态记忆数量。
    pub recycled_memories: u64,
    /// Counts old unreferenced turns moved into durable trash.
    /// 统计移入耐久回收站的旧无引用回合数量。
    pub recycled_turns: u64,
    /// Counts durable cold-turn jobs created by this pass.
    /// 统计本轮创建的耐久冷回合任务数量。
    pub enqueued_cold_jobs: u64,
    /// Counts durable cold-turn jobs completed by this pass.
    /// 统计本轮完成的耐久冷回合任务数量。
    pub completed_cold_jobs: u64,
    /// Counts expired trash batches permanently removed by this pass.
    /// 统计本轮永久清除的过期回收站批次数量。
    pub purged_trash_batches: u64,
    /// Lists project-local failures without preventing maintenance for unrelated projects.
    /// 列出项目局部失败，且不阻止其他无关项目继续维护。
    pub project_failures: Vec<NativeVmmRetentionProjectFailure>,
}

/// Describes one project-local retention failure from a process-wide pass.
/// 描述一次进程级保留运行中的项目局部失败。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NativeVmmRetentionProjectFailure {
    /// One-level native project whose maintenance failed.
    /// 维护失败的一级原生项目。
    pub project_id: u64,
    /// Exact failure returned by planning or the atomic snapshot transition.
    /// 规划或原子快照转换返回的准确失败。
    pub error: String,
}

impl NativeVmmRetentionApplyResult {
    /// Adds one project pass into this process-wide run summary.
    /// 将一次项目维护结果累加到本次进程级运行汇总。
    fn merge(&mut self, other: &Self) {
        self.expired_profile_nodes = self
            .expired_profile_nodes
            .saturating_add(other.expired_profile_nodes);
        self.recycled_memories = self
            .recycled_memories
            .saturating_add(other.recycled_memories);
        self.recycled_turns = self.recycled_turns.saturating_add(other.recycled_turns);
        self.enqueued_cold_jobs = self
            .enqueued_cold_jobs
            .saturating_add(other.enqueued_cold_jobs);
        self.completed_cold_jobs = self
            .completed_cold_jobs
            .saturating_add(other.completed_cold_jobs);
        self.purged_trash_batches = self
            .purged_trash_batches
            .saturating_add(other.purged_trash_batches);
        self.project_failures
            .extend(other.project_failures.iter().cloned());
    }
}

/// Provides native project migration and clear operations over one shared runtime.
/// 基于一个共享运行时提供原生项目迁移与清空操作。
#[derive(Clone)]
pub(crate) struct NativeVmmMaintenance {
    /// Optional host hook owner; isolated Core fixtures intentionally have no host outbox.
    /// 可选宿主 Hook 归属方；隔离 Core 夹具刻意不存在宿主待写回队列。
    writeback_runtime: Option<vulcan_agent_runtime::agent_runtime::vmm_hooks::VmmHookRuntimeHandle>,
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
    /// Enables every native background worker only while native VMM is globally active.
    /// 仅在原生 VMM 全局生效时启用全部原生后台工作器。
    native_backend_enabled: Arc<AtomicBool>,
    /// Prevents duplicate worker tasks when bootstrap paths are retried.
    /// 防止启动路径重试时产生重复工作器任务。
    retention_worker_started: Arc<AtomicBool>,
    /// Prevents duplicate profile-expiry worker tasks when bootstrap paths are retried.
    /// 防止启动路径重试时产生重复画像过期工作器任务。
    profile_expiry_worker_started: Arc<AtomicBool>,
    /// Wakes the worker immediately after native VMM becomes active.
    /// 在原生 VMM 开始生效后立即唤醒工作器。
    retention_notify: Arc<Notify>,
    /// Wakes the profile-expiry worker immediately after native VMM becomes active.
    /// 在原生 VMM 开始生效后立即唤醒画像过期工作器。
    profile_expiry_notify: Arc<Notify>,
    /// Wakes the shared durable PostAction worker after native VMM becomes active.
    /// 在原生 VMM 开始生效后唤醒共享耐久 PostAction 工作器。
    pending_analysis_notify: Arc<Notify>,
    /// Serializes manual test runs and the periodic worker.
    /// 串行化手动测试运行与周期工作器。
    retention_run_gate: Arc<Mutex<()>>,
    /// Exposes whether one retention pass currently owns the execution gate.
    /// 暴露当前是否有一次保留运行占有执行门。
    retention_running: Arc<AtomicBool>,
    /// Exposes whether the profile-expiry worker currently owns its execution path.
    /// 暴露画像过期工作器当前是否占有其执行路径。
    profile_expiry_running: Arc<AtomicBool>,
    /// Stores the latest completed retention status for diagnostics and the management page.
    /// 保存最近一次已完成保留状态，供诊断与管理页面使用。
    retention_last_status: Arc<RwLock<NativeVmmRetentionStatus>>,
    /// Holds only the latest server-prepared manual cleanup plan; a new preview replaces it.
    /// 仅保留服务端最近准备的人工清洗计划，新预览替换旧预览。
    retention_preview: Arc<Mutex<Option<retention_preview::PreparedRetentionCleanup>>>,
    /// Holds the latest bounded server-prepared archive selection.
    /// 保存最近一份有界服务端归档选择。
    session_archive_preview: Arc<Mutex<Option<session_archive::PreparedSessionArchive>>>,
    /// Holds the latest forgotten-session restart preview independently of archive operations.
    /// 独立于归档操作保存最近的已遗忘会话重新启用预览。
    session_restart_preview: Arc<Mutex<Option<session_restart::PreparedSessionRestart>>>,
    /// Holds the latest exact manual recycle selection independently of policy cleanup.
    /// 独立于策略清洗保存最近的准确人工回收选择。
    removal_preview: Arc<Mutex<Option<manual_removal::PreparedManualRemoval>>>,
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

    /// Reads validated native business settings; failures remain visible to the maintenance caller.
    /// 读取经过校验的原生业务设置；失败保持对维护调用者可见。
    pub(crate) async fn runtime_policy(&self) -> Result<VmmNativeRuntimePolicySummary, String> {
        self.runtime.runtime_policy().await
    }

    /// Reads caller overrides only for an existing native host binding, without changing its enabled state.
    /// 仅为存在的原生宿主绑定读取调用方覆盖，不改变绑定启用状态。
    pub(crate) async fn host_project_policy(
        &self,
        host_project_id: &str,
    ) -> Result<vulcan_service_api::VmmNativeHostProjectPolicySummary, String> {
        let _gate = self.operation_gate.read().await;
        self.bound_project(host_project_id)
            .await?
            .ok_or_else(|| "该 VulcanCode 项目尚未绑定原生记忆项目".to_owned())?;
        self.runtime.host_project_policy(host_project_id).await
    }

    /// Saves only the caller's business overrides while holding its binding stable.
    /// 保持绑定稳定期间仅保存调用方业务覆盖。
    pub(crate) async fn update_host_project_policy(
        &self,
        edit: vulcan_service_api::VmmNativeHostProjectPolicyEdit,
    ) -> Result<vulcan_service_api::VmmNativeHostProjectPolicySummary, String> {
        let _gate = self.operation_gate.read().await;
        self.bound_project(&edit.host_project_id)
            .await?
            .ok_or_else(|| "该 VulcanCode 项目尚未绑定原生记忆项目".to_owned())?;
        self.runtime
            .update_host_project_policy(&edit.host_project_id, edit.overrides)
            .await
    }

    /// Persists explicitly supplied recall settings and returns their next-command effective values.
    /// 持久化显式提供的召回设置，并返回下一命令使用的有效值。
    /// Returns an error when validation, persistence, or activation fails.
    /// 校验、持久化或激活失败时返回错误。
    pub(crate) async fn update_runtime_policy(
        &self,
        recall: VmmNativeRecallPolicySummary,
    ) -> Result<VmmNativeRuntimePolicySummary, String> {
        self.runtime.update_runtime_policy(recall).await
    }

    /// Saves explicit ranking values, preserving the independently stored recall group.
    /// 保存显式排序值，保留独立保存的召回组。
    /// Returns effective values or an explicit validation/persistence error.
    /// 返回有效值或明确的校验/持久化错误。
    pub(crate) async fn update_retrieval_policy(
        &self,
        retrieval: VmmNativeRetrievalPolicySummary,
    ) -> Result<VmmNativeRuntimePolicySummary, String> {
        self.runtime.update_retrieval_policy(retrieval).await
    }

    /// Saves text intake controls without restarting workers or reprocessing historical turns.
    /// 保存文本入库控制项，不重启工作器或重新处理历史回合。
    /// Returns the accepted full policy or the exact validation/persistence error.
    /// 返回接纳的完整策略或准确的校验/持久化错误。
    pub(crate) async fn update_text_policy(
        &self,
        text: vulcan_service_api::VmmNativeTextPolicySummary,
    ) -> Result<VmmNativeRuntimePolicySummary, String> {
        self.runtime.update_text_policy(text).await
    }

    /// Saves global profile lifetimes without retrospectively changing stored deadlines or triggering extraction.
    /// 保存全局画像生命周期，不追溯更改存储期限或触发提炼。
    /// Returns accepted settings or the exact validation/persistence failure.
    /// 返回接纳设置或准确的校验/持久化失败。
    pub(crate) async fn update_profile_policy(
        &self,
        profiles: vulcan_service_api::VmmNativeProfilePolicySummary,
    ) -> Result<VmmNativeRuntimePolicySummary, String> {
        self.runtime.update_profile_policy(profiles).await
    }

    /// Saves explicit cleanup settings and wakes both periodic workers only after success.
    /// 保存显式清洗设置，且仅在成功后唤醒两个周期工作器。
    /// Returns the complete saved policy or the exact configuration failure.
    /// 返回完整已保存策略或准确配置失败。
    pub(crate) async fn update_retention_settings(
        &self,
        settings: VmmNativeRetentionSettingsSummary,
    ) -> Result<VmmNativeRuntimePolicySummary, String> {
        let policy = self.runtime.update_retention_settings(settings).await?;
        self.retention_notify.notify_one();
        self.profile_expiry_notify.notify_one();
        Ok(policy)
    }

    /// Reads project-owned retention settings only while the exact memory project exists.
    /// 仅在准确记忆项目存在时读取项目持有的保留设置。
    /// The catalog gate prevents deletion between existence validation and configuration access.
    /// 目录锁防止存在性验证和配置访问之间发生删除。
    pub(crate) async fn project_retention_summary(
        &self,
        project_id: u64,
    ) -> Result<vulcan_service_api::VmmNativeProjectRetentionSummary, String> {
        let _project_guard = self.project_mutation_gate.lock().await;
        self.ensure_project(project_id).await?;
        self.runtime.project_retention_summary(project_id).await
    }

    /// Saves an explicit override or inheritance request without changing any existing memory or batch.
    /// 保存显式覆盖或继承请求，不更改任何现有记忆或批次。
    /// Invalid projects and invalid rules fail before the user configuration is persisted.
    /// 非法项目和非法规则在用户配置持久化之前失败。
    pub(crate) async fn update_project_retention(
        &self,
        edit: vulcan_service_api::VmmNativeProjectRetentionEdit,
    ) -> Result<vulcan_service_api::VmmNativeProjectRetentionSummary, String> {
        let _project_guard = self.project_mutation_gate.lock().await;
        let (project_id, rules) = match edit {
            vulcan_service_api::VmmNativeProjectRetentionEdit::Override { project_id, rules } => {
                (project_id, Some(rules))
            }
            vulcan_service_api::VmmNativeProjectRetentionEdit::Inherit { project_id } => {
                (project_id, None)
            }
        };
        self.ensure_project(project_id).await?;
        let saved = self
            .runtime
            .save_project_retention(project_id, rules)
            .await?;
        self.retention_notify.notify_one();
        Ok(saved)
    }

    /// Reads profile rules only for an existing stable memory project.
    /// 仅为已存在的稳定记忆项目读取画像规则。
    /// Returns project-only inheritance metadata while preventing concurrent catalog deletion.
    /// 在阻止并发目录删除的同时返回仅项目的继承元数据。
    pub(crate) async fn project_profile_policy_summary(
        &self,
        project_id: u64,
    ) -> Result<vulcan_service_api::VmmNativeProjectProfilePolicySummary, String> {
        let _project_guard = self.project_mutation_gate.lock().await;
        self.ensure_project(project_id).await?;
        self.runtime
            .project_profile_policy_summary(project_id)
            .await
    }

    /// Updates only project profile defaults; existing nodes and personal rules are not rewritten.
    /// 仅更新项目画像默认规则；不重写已有节点和个人规则。
    /// Takes an explicit whole-group edit and returns the saved effective policy for the next command.
    /// 接收显式完整组编辑，返回下一命令使用的已保存有效策略。
    pub(crate) async fn update_project_profile_policy(
        &self,
        edit: vulcan_service_api::VmmNativeProjectProfilePolicyEdit,
    ) -> Result<vulcan_service_api::VmmNativeProjectProfilePolicySummary, String> {
        let _project_guard = self.project_mutation_gate.lock().await;
        let (project_id, rules) = match edit {
            vulcan_service_api::VmmNativeProjectProfilePolicyEdit::Override {
                project_id,
                rules,
            } => (project_id, Some(rules)),
            vulcan_service_api::VmmNativeProjectProfilePolicyEdit::Inherit { project_id } => {
                (project_id, None)
            }
        };
        self.ensure_project(project_id).await?;
        self.runtime
            .save_project_profile_policy(project_id, rules)
            .await
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
    /// Returns an error for an invalid user-prompt sample or resource snapshot.
    /// 用户提示词样本或资源快照无效时返回错误。
    pub(crate) async fn test_noise(&self, text: &str) -> Result<VmmNativeNoiseTestResult, String> {
        self.runtime.test_noise(text).await
    }

    /// Creates one maintenance facade over the exact runtime and project catalog.
    /// 基于精确运行时与项目目录创建一个维护门面。
    #[must_use]
    pub(crate) fn new(
        runtime: Arc<NativeVmmCoreRuntime>,
        data_plane: VmmDataPlaneHandle,
        operation_gate: Arc<RwLock<()>>,
    ) -> Self {
        // Defaults are read from the sole cleanup-policy owner, not independently redefined.
        // 从唯一清洗策略归属方读取默认值，不单独重定义。
        let defaults = retention_policy::defaults();
        Self {
            writeback_runtime: None,
            runtime,
            data_plane,
            operation_gate,
            project_mutation_gate: Arc::new(Mutex::new(())),
            native_backend_enabled: Arc::new(AtomicBool::new(false)),
            retention_worker_started: Arc::new(AtomicBool::new(false)),
            profile_expiry_worker_started: Arc::new(AtomicBool::new(false)),
            retention_notify: Arc::new(Notify::new()),
            profile_expiry_notify: Arc::new(Notify::new()),
            pending_analysis_notify: Arc::new(Notify::new()),
            retention_run_gate: Arc::new(Mutex::new(())),
            retention_running: Arc::new(AtomicBool::new(false)),
            profile_expiry_running: Arc::new(AtomicBool::new(false)),
            retention_preview: Arc::new(Mutex::new(None)),
            session_archive_preview: Arc::new(Mutex::new(None)),
            session_restart_preview: Arc::new(Mutex::new(None)),
            removal_preview: Arc::new(Mutex::new(None)),
            retention_last_status: Arc::new(RwLock::new(NativeVmmRetentionStatus {
                enabled: false,
                running: false,
                profile_expiry_running: false,
                interval_minutes: u64::from(defaults.scan_interval_minutes),
                profile_expiry_interval_seconds: u64::from(
                    defaults.profile_expiry_interval_seconds,
                ),
                policy: RetentionPolicy::v1(),
                last_started_at_unix_ms: None,
                last_finished_at_unix_ms: None,
                last_error: None,
                last_result: None,
                profile_expiry_last_started_at_unix_ms: None,
                profile_expiry_last_finished_at_unix_ms: None,
                profile_expiry_last_error: None,
                profile_expiry_last_result: None,
                pending_cold_jobs: 0,
                recycle_batch_count: 0,
            })),
        }
    }

    /// Installs the existing host admission owner before this maintenance facade is published.
    /// 在发布本维护门面之前安装既有宿主准入归属方。
    pub(crate) fn with_writeback_runtime(
        mut self,
        runtime: vulcan_agent_runtime::agent_runtime::vmm_hooks::VmmHookRuntimeHandle,
    ) -> Self {
        self.writeback_runtime = Some(runtime);
        self
    }

    /// Revokes only the former exact owner after a successful binding transaction, under the operation gate.
    /// 成功绑定事务后，在操作门内仅撤销此前准确归属。
    fn revoke_previous_writebacks(
        &self,
        host_project_id: &str,
        previous_project_id: u64,
    ) -> Result<(), String> {
        if let Some(runtime) = &self.writeback_runtime {
            runtime.revoke_native_project_writebacks(host_project_id, previous_project_id)?;
        }
        Ok(())
    }

    /// Shares the backend activation gate and wake signal with the native PostAction worker.
    /// 与原生 PostAction 工作器共享后端激活门与唤醒信号。
    #[must_use]
    pub(crate) fn pending_analysis_activation(&self) -> (Arc<AtomicBool>, Arc<Notify>) {
        (
            Arc::clone(&self.native_backend_enabled),
            Arc::clone(&self.pending_analysis_notify),
        )
    }

    /// Activates or deactivates every native worker according to global backend selection.
    /// 根据全局后端选择启用或停用全部原生工作器。
    pub(crate) fn set_native_backend_enabled(&self, enabled: bool) {
        self.native_backend_enabled
            .store(enabled, Ordering::Release);
        self.retention_notify.notify_one();
        self.profile_expiry_notify.notify_one();
        self.pending_analysis_notify.notify_one();
    }

    /// Starts the independent process-local retention and profile-expiry workers exactly once.
    /// 只启动一次相互独立的进程内保留与画像过期工作器。
    pub(crate) fn start_retention_worker(&self, runtime: &tokio::runtime::Handle) {
        if !self.retention_worker_started.swap(true, Ordering::AcqRel) {
            let maintenance = self.clone();
            runtime.spawn(async move {
                maintenance.run_retention_worker().await;
            });
        }
        if !self
            .profile_expiry_worker_started
            .swap(true, Ordering::AcqRel)
        {
            let maintenance = self.clone();
            runtime.spawn(async move {
                maintenance.run_profile_expiry_worker().await;
            });
        }
    }

    /// Runs one complete retention cycle across every one-level native memory project.
    /// 在全部单层原生记忆项目上运行一次完整保留周期。
    ///
    /// # Errors
    /// Returns the exact Core planning, snapshot validation, or durable persistence failure.
    /// Core 规划、快照校验或耐久持久化失败时返回准确错误。
    #[cfg(test)]
    pub(crate) async fn run_retention_once(&self) -> Result<NativeVmmRetentionApplyResult, String> {
        let settings = self.runtime.retention_settings().await?;
        let policy = retention_policy::resolve(&settings)?;
        self.run_retention_with_policy(&policy).await
    }

    /// Runs manual or scheduled cleanup using one admitted policy for every phase.
    /// 使用同一已准入策略运行人工或定时清洗的所有阶段。
    /// `policy` stays immutable throughout the pass; failures remain visible in maintenance status.
    /// `policy` 在本轮始终不可变；失败保留在维护状态中。
    async fn run_retention_with_policy(
        &self,
        policy: &RetentionPolicy,
    ) -> Result<NativeVmmRetentionApplyResult, String> {
        let _run_guard = self.retention_run_gate.lock().await;
        if !self.native_backend_enabled.load(Ordering::Acquire) {
            return Err(String::from(
                "原生 VMM 定期清洗仅在内部原生记忆已启用时可运行。",
            ));
        }
        let started_at_unix_ms = unix_ns_to_ms(native_vmm_now_ns()?);
        self.retention_running.store(true, Ordering::Release);
        {
            let mut status = self.retention_last_status.write().await;
            status.last_started_at_unix_ms = Some(started_at_unix_ms);
            status.last_error = None;
        }

        let outcome = self.run_retention_once_inner(policy).await;
        let finished_at_unix_ms = native_vmm_now_ns().map(unix_ns_to_ms).ok();
        let current_snapshot = self.runtime.storage_snapshot().ok();
        {
            let mut status = self.retention_last_status.write().await;
            status.last_finished_at_unix_ms = finished_at_unix_ms;
            status.pending_cold_jobs = current_snapshot.as_ref().map_or(0, |snapshot| {
                usize_to_u64(snapshot.retention_cold_turn_jobs.len())
            });
            status.recycle_batch_count = current_snapshot.as_ref().map_or(0, |snapshot| {
                usize_to_u64(snapshot.retention_trash_batches.len())
            });
            match &outcome {
                Ok(result) => {
                    status.last_result = Some(result.clone());
                    status.last_error = retention_project_failure_message(result);
                }
                Err(error) => {
                    status.last_error = Some(error.clone());
                }
            }
        }
        self.retention_running.store(false, Ordering::Release);
        outcome
    }

    /// Loads the current native retention policy, schedule state, and latest pass result.
    /// 加载当前原生保留策略、调度状态与最近一次运行结果。
    ///
    /// # Errors
    /// Returns an error when the durable Core snapshot cannot be read.
    /// 无法读取耐久 Core 快照时返回错误。
    pub(crate) async fn retention_status(&self) -> Result<NativeVmmRetentionStatus, String> {
        let settings = self.runtime.retention_settings().await?;
        let snapshot = self.runtime.storage_snapshot()?;
        let mut status = self.retention_last_status.read().await.clone();
        status.enabled = self.native_backend_enabled.load(Ordering::Acquire) && settings.enabled;
        status.interval_minutes = u64::from(settings.scan_interval_minutes);
        status.profile_expiry_interval_seconds =
            u64::from(settings.profile_expiry_interval_seconds);
        status.policy = retention_policy::resolve(&settings)?;
        status.running = self.retention_running.load(Ordering::Acquire);
        status.profile_expiry_running = self.profile_expiry_running.load(Ordering::Acquire);
        status.pending_cold_jobs = usize_to_u64(snapshot.retention_cold_turn_jobs.len());
        status.recycle_batch_count = usize_to_u64(snapshot.retention_trash_batches.len());
        Ok(status)
    }

    /// Inspects current native vector-generation compatibility without mutating memory data.
    /// 在不修改记忆数据的情况下检查当前原生向量代次兼容性。
    ///
    /// # Returns
    /// 返回值
    /// A complete status even when the configured embedding route is unavailable.
    /// 即使已配置 Embedding 路由不可用，也返回完整状态。
    ///
    /// # Errors
    /// 错误
    /// Returns only when durable queue or in-process storage state cannot be read.
    /// 仅在无法读取耐久队列或进程内存储状态时返回错误。
    pub(crate) async fn vector_status(&self) -> Result<NativeVmmVectorStatus, String> {
        let _operation_guard = self.operation_gate.read().await;
        let snapshot = self.runtime.storage_snapshot()?;
        let pending_analysis_job_count = usize_to_u64(
            self.data_plane
                .list_active_pending_analysis_jobs()
                .await?
                .len(),
        );
        match self.runtime.current_vector_space_descriptor().await {
            Ok(descriptor) => Ok(vector_status_from_snapshot(
                &snapshot,
                Some(&descriptor.vector_space_id),
                Some(descriptor.embedding_dimension),
                pending_analysis_job_count,
                None,
            )),
            Err(error) => Ok(vector_status_from_snapshot(
                &snapshot,
                None,
                None,
                pending_analysis_job_count,
                Some(error),
            )),
        }
    }

    /// Builds an exact vector-rebuild preview for the current embedding route.
    /// 为当前 Embedding 路由构建准确的向量重建预检。
    ///
    /// # Returns
    /// 返回值
    /// A storage-versioned target set and confirmation fingerprint without any mutation.
    /// 返回带存储版本的目标集合及确认指纹，且不产生任何变更。
    ///
    /// # Errors
    /// 错误
    /// Returns the exact route activation, snapshot, or target-integrity failure.
    /// 返回准确的路由激活、快照或目标完整性失败。
    pub(crate) async fn preview_vector_rebuild(
        &self,
    ) -> Result<NativeVmmVectorRebuildPreview, String> {
        let _operation_guard = self.operation_gate.read().await;
        let descriptor = self.runtime.current_vector_space_descriptor().await?;
        vector_rebuild_preview(&self.runtime.storage_snapshot()?, &descriptor)
    }

    /// Rebuilds every incompatible active memory into the confirmed current vector generation.
    /// 将每条不兼容的活动记忆重建到已确认的当前向量代次。
    ///
    /// # Parameters
    /// 参数
    /// `operation_id` is the stable user command identity, while the expected version and preview
    /// fingerprint bind execution to the exact target set shown to the user.
    /// `operation_id` 是稳定用户命令身份，预期版本与预检指纹把执行绑定到用户看到的准确目标集合。
    /// `confirmed` must be true because provider-backed vector replacement is a high-impact operation.
    /// `confirmed` 必须为真，因为由供应商执行的向量替换属于高影响操作。
    ///
    /// # Returns
    /// 返回值
    /// The final storage version and exact number of rebuilt memories and committed batches.
    /// 返回最终存储版本及准确重建记忆数与提交批次数。
    ///
    /// # Errors
    /// 错误
    /// Returns on stale confirmation, route drift, provider failure, or any journaled persistence error.
    /// 预检过期、路由漂移、供应商失败或任何账本化持久化错误时返回错误。
    pub(crate) async fn execute_vector_rebuild(
        &self,
        operation_id: &str,
        expected_storage_version: u64,
        expected_preview_fingerprint: &str,
        confirmed: bool,
    ) -> Result<NativeVmmVectorRebuildResult, String> {
        if !confirmed {
            return Err(String::from("向量重建属于高影响操作，必须明确确认后执行。"));
        }
        let _project_guard = self.project_mutation_gate.lock().await;
        let _operation_guard = self.operation_gate.write().await;
        self.runtime.recover_storage_mutations().await?;
        let descriptor = self.runtime.current_vector_space_descriptor().await?;
        let snapshot = self.runtime.storage_snapshot()?;
        if snapshot.storage_version != expected_storage_version {
            return Err(format!(
                "原生 VMM 向量重建预检已过期：预期存储版本 {expected_storage_version}，当前版本 {}，请重新预检。",
                snapshot.storage_version
            ));
        }
        let preview = vector_rebuild_preview(&snapshot, &descriptor)?;
        if preview.preview_fingerprint != expected_preview_fingerprint {
            return Err(String::from(
                "原生 VMM 向量重建目标已变化，请重新预检后确认。",
            ));
        }
        let active_targets = active_vector_rebuild_targets(&snapshot, &descriptor.vector_space_id);
        let recycle_bin_targets =
            recycle_bin_vector_rebuild_targets(&snapshot, &descriptor.vector_space_id);
        let subject_id = native_vmm_subject_id()?;
        let mut rebuilt_memory_count = 0_u64;
        let mut batch_count = 0_u64;
        for (scope_id, memory_ids) in active_targets {
            for memory_ids in memory_ids.chunks(256) {
                let current = self.runtime.current_vector_space_descriptor().await?;
                if current.vector_space_id != descriptor.vector_space_id
                    || current.embedding_dimension != descriptor.embedding_dimension
                    || current.capability_fingerprint != descriptor.capability_fingerprint
                {
                    return Err(String::from(
                        "向量重建期间 Embedding 路由发生变化；已提交批次保持有效，请重新预检剩余记录。",
                    ));
                }
                let now_ns = native_vmm_now_ns()?;
                let batch_operation_id = vector_rebuild_batch_operation_id(
                    operation_id,
                    "active",
                    &scope_id,
                    memory_ids,
                )?;
                let result = self
                    .runtime
                    .run(CoreCommand::Maintenance(MaintenanceCommand {
                        subject_id: subject_id.clone(),
                        scope_access: ScopeAccess {
                            write_scope: scope_id.clone(),
                            visible_scopes: vec![scope_id.clone()],
                        },
                        occurred_at_ns: now_ns,
                        action: MaintenanceAction::RebuildVectors {
                            memory_ids: memory_ids.to_vec(),
                            vector_space_id: descriptor.vector_space_id.clone(),
                        },
                        mutation_identity: MutationIdentity {
                            operation_id: OperationId::new(batch_operation_id.clone())
                                .map_err(|error| error.to_string())?,
                            idempotency_key: IdempotencyKey::new(batch_operation_id.clone())
                                .map_err(|error| error.to_string())?,
                            request_fingerprint: format!("fp256:{}", "0".repeat(64)),
                            source_kind: MutationSourceKind::HostRpc,
                            request_source_id: operation_id.to_owned(),
                            observed_at_ns: now_ns,
                        },
                    }))
                    .await
                    .map_err(|error| error.to_string())?;
                let CommandResult::Maintenance(result) = result else {
                    return Err(String::from("原生 VMM Core 返回了非向量维护结果。"));
                };
                let expected_count = u32::try_from(memory_ids.len())
                    .map_err(|error| format!("向量重建批次计数超出范围：{error}"))?;
                if result.rebuilt_vector_count != expected_count
                    || result.affected_memory_ids.as_slice() != memory_ids
                {
                    return Err(String::from("原生 VMM Core 返回了不一致的向量重建结果。"));
                }
                rebuilt_memory_count =
                    rebuilt_memory_count.saturating_add(u64::from(expected_count));
                batch_count = batch_count.saturating_add(1);
            }
        }
        for (scope_id, memory_ids) in recycle_bin_targets {
            for memory_ids in memory_ids.chunks(256) {
                let current = self.runtime.current_vector_space_descriptor().await?;
                if current != descriptor {
                    return Err(String::from(
                        "向量重建期间 Embedding 路由发生变化；已提交批次保持有效，请重新预检剩余记录。",
                    ));
                }
                let batch_operation_id = vector_rebuild_batch_operation_id(
                    operation_id,
                    "recycle-bin",
                    &scope_id,
                    memory_ids,
                )?;
                let before = self.runtime.storage_snapshot()?;
                let summaries = recycle_bin_vector_summaries(
                    &before,
                    &subject_id,
                    &scope_id,
                    memory_ids,
                    &descriptor.vector_space_id,
                )?;
                let vectors = self
                    .runtime
                    .embed_maintenance_summaries(&batch_operation_id, &descriptor, summaries)
                    .await?;
                let current = self.runtime.current_vector_space_descriptor().await?;
                if current != descriptor {
                    return Err(String::from(
                        "向量重建期间 Embedding 路由发生变化；本批向量未提交，请重新预检剩余记录。",
                    ));
                }
                let mutation_kind = format!(
                    "vector_rebuild_recycle_bin:{}:{}",
                    descriptor.vector_space_id.as_str(),
                    batch_operation_id
                );
                let expected_ids = memory_ids.to_vec();
                let target_space = descriptor.vector_space_id.clone();
                let target_dimension = descriptor.embedding_dimension;
                let expected_subject = subject_id.clone();
                let expected_scope = scope_id.clone();
                let result = self
                    .runtime
                    .transform_storage_snapshot(
                        &batch_operation_id,
                        &mutation_kind,
                        before.storage_version,
                        move |snapshot| {
                            apply_recycle_bin_vector_batch(
                                snapshot,
                                &expected_subject,
                                &expected_scope,
                                &expected_ids,
                                &target_space,
                                target_dimension,
                                vectors,
                            )
                        },
                    )
                    .await?;
                let expected_count = usize_to_u64(memory_ids.len());
                if usize_to_u64(result.rebuilt_memory_ids.len()) != expected_count
                    || result
                        .rebuilt_memory_ids
                        .iter()
                        .map(String::as_str)
                        .ne(memory_ids.iter().map(|memory_id| memory_id.as_str()))
                {
                    return Err(String::from(
                        "原生 VMM 回收站向量重建返回了不一致的批次结果。",
                    ));
                }
                rebuilt_memory_count = rebuilt_memory_count.saturating_add(expected_count);
                batch_count = batch_count.saturating_add(1);
            }
        }
        self.runtime.current_vector_space_id().await?;
        let final_snapshot = self.runtime.storage_snapshot()?;
        if !recycle_bin_vector_rebuild_targets(&final_snapshot, &descriptor.vector_space_id)
            .is_empty()
        {
            return Err(String::from(
                "原生 VMM 回收站仍包含旧向量代次，请重新预检后继续重建。",
            ));
        }
        Ok(NativeVmmVectorRebuildResult {
            storage_version: final_snapshot.storage_version,
            target_vector_space_id: descriptor.vector_space_id.into_string(),
            rebuilt_memory_count,
            batch_count,
        })
    }

    /// Lists durable native recycle batches, optionally limited to one project.
    /// 列出耐久原生回收批次，并可限制到一个项目。
    ///
    /// # Errors
    /// Returns an error for a nonexistent project or malformed persisted project scope.
    /// 项目不存在或持久项目作用域格式无效时返回错误。
    pub(crate) async fn recycle_bin(
        &self,
        project_id: Option<u64>,
    ) -> Result<NativeVmmRecycleBin, String> {
        let project_scope_filter = match project_id {
            Some(project_id) => {
                self.ensure_project(project_id).await?;
                Some(project_scope(project_id)?)
            }
            None => None,
        };
        let snapshot = self.runtime.storage_snapshot()?;
        let mut batches = snapshot
            .retention_trash_batches
            .iter()
            .filter(|batch| {
                batch.subject_id.as_str() == "vmm-subject:1"
                    && project_scope_filter
                        .as_ref()
                        .is_none_or(|scope| batch.scope_id == *scope)
            })
            .map(retention_batch_summary)
            .collect::<Result<Vec<_>, _>>()?;
        batches.sort_by(|left, right| {
            right
                .recycled_at_unix_ms
                .cmp(&left.recycled_at_unix_ms)
                .then_with(|| right.batch_id.cmp(&left.batch_id))
        });
        Ok(NativeVmmRecycleBin {
            storage_version: snapshot.storage_version,
            batches,
        })
    }

    /// Restores one exact recycle batch after optimistic-version and collision validation.
    /// 在乐观版本及冲突校验后恢复一个准确回收批次。
    ///
    /// # Errors
    /// Returns an error for stale state, an expired or missing batch, conflicts, or persistence failure.
    /// 状态过期、批次到期或不存在、发生冲突或持久化失败时返回错误。
    pub(crate) async fn restore_recycle_batch(
        &self,
        operation_id: &str,
        batch_id: &str,
        expected_storage_version: u64,
    ) -> Result<NativeVmmRecycleBatchSummary, String> {
        let _project_guard = self.project_mutation_gate.lock().await;
        let _operation_guard = self.operation_gate.write().await;
        let mutation_kind = format!("retention_restore:{batch_id}:{expected_storage_version}");
        if let Some(existing) = self
            .runtime
            .replay_storage_snapshot_transform::<NativeVmmRecycleBatchSummary>(
                operation_id,
                &mutation_kind,
            )
            .await?
        {
            return Ok(existing);
        }
        let now_ns = native_vmm_now_ns()?;
        let before = self.runtime.storage_snapshot()?;
        if before.storage_version != expected_storage_version {
            return Err(format!(
                "原生 VMM 回收站预检已过期：预期存储版本 {expected_storage_version}，当前版本 {}。",
                before.storage_version
            ));
        }
        let batch = before
            .retention_trash_batches
            .iter()
            .find(|batch| batch.batch_id == batch_id)
            .ok_or_else(|| format!("原生 VMM 回收批次 `{batch_id}` 不存在。"))?;
        if batch.expires_at_ns <= now_ns {
            return Err(format!("原生 VMM 回收批次 `{batch_id}` 已到期，不能恢复。"));
        }
        ensure_recycle_restore_has_no_conflicts(&before, batch)?;
        if !batch.dataset.memories.is_empty() {
            let descriptor = self.runtime.current_vector_space_descriptor().await?;
            let incompatible = batch
                .dataset
                .memories
                .iter()
                .filter(|memory| {
                    memory.vector_space_id.as_ref() != Some(&descriptor.vector_space_id)
                })
                .count();
            if incompatible > 0 {
                return Err(format!(
                    "原生 VMM 回收批次 `{batch_id}` 包含 {incompatible} 条旧向量代次记忆。请先在“内部维护”执行向量重建，再恢复该批次。"
                ));
            }
        }
        self.runtime
            .transform_storage_snapshot(
                operation_id,
                &mutation_kind,
                expected_storage_version,
                move |snapshot| {
                    let batch_index = snapshot
                        .retention_trash_batches
                        .iter()
                        .position(|batch| batch.batch_id == batch_id)
                        .ok_or_else(|| format!("原生 VMM 回收批次 `{batch_id}` 不存在。"))?;
                    let batch = &snapshot.retention_trash_batches[batch_index];
                    // Provider checks and lock acquisition can outlive the preview's time boundary.
                    // 供应商检查和锁等待可能跨过预检时的时间边界。
                    let apply_now_ns = native_vmm_now_ns()?;
                    if batch.expires_at_ns <= apply_now_ns {
                        return Err(format!("原生 VMM 回收批次 `{batch_id}` 已到期，不能恢复。"));
                    }
                    ensure_recycle_restore_has_no_conflicts(snapshot, batch)?;
                    let batch = snapshot.retention_trash_batches.remove(batch_index);
                    let summary = retention_batch_summary(&batch)?;
                    restore_retention_batch(snapshot, batch)?;
                    Ok(summary)
                },
            )
            .await
    }

    /// Permanently removes one exact recycle batch and its retained vectors.
    /// 永久移除一个准确回收批次及其保留向量。
    ///
    /// # Errors
    /// Returns an error for stale state, a missing batch, or persistence failure.
    /// 状态过期、批次不存在或持久化失败时返回错误。
    pub(crate) async fn purge_recycle_batch(
        &self,
        operation_id: &str,
        batch_id: &str,
        expected_storage_version: u64,
    ) -> Result<NativeVmmRecycleBatchSummary, String> {
        let _project_guard = self.project_mutation_gate.lock().await;
        let _operation_guard = self.operation_gate.write().await;
        let mutation_kind = format!("retention_purge:{batch_id}:{expected_storage_version}");
        let summary = self
            .runtime
            .transform_storage_snapshot(
                operation_id,
                &mutation_kind,
                expected_storage_version,
                move |snapshot| {
                    let batch_index = snapshot
                        .retention_trash_batches
                        .iter()
                        .position(|batch| batch.batch_id == batch_id)
                        .ok_or_else(|| format!("原生 VMM 回收批次 `{batch_id}` 不存在。"))?;
                    let batch = snapshot.retention_trash_batches.remove(batch_index);
                    let summary = retention_batch_summary(&batch)?;
                    manual_session_lifecycle::apply(
                        snapshot,
                        batch.manual_selection.as_ref(),
                        false,
                    )?;
                    remove_outcomes_referencing_dataset(snapshot, &batch.dataset);
                    Ok(summary)
                },
            )
            .await?;
        self.prune_orphaned_terminal_analysis_jobs().await?;
        Ok(summary)
    }

    /// Executes one serialized process-wide retention pass under the canonical lock order.
    /// 按规范锁顺序执行一次串行化的进程级保留运行。
    async fn run_retention_once_inner(
        &self,
        policy: &RetentionPolicy,
    ) -> Result<NativeVmmRetentionApplyResult, String> {
        // Every native project mutation acquires the catalog gate before the operation barrier.
        // 每个原生项目变更都先获取目录门，再获取操作屏障。
        let _project_guard = self.project_mutation_gate.lock().await;
        let _operation_guard = self.operation_gate.write().await;
        self.runtime.recover_storage_mutations().await?;
        let mut aggregate = NativeVmmRetentionApplyResult::default();
        // Freeze the complete owner-policy map once; later project edits cannot split this pass.
        // 一次性冻结完整归属方策略映射；后续项目编辑不能拆分当前轮次。
        let projects = self.data_plane.list_projects().await?;
        let policies = self
            .runtime
            .project_retention_policies(
                policy,
                &projects
                    .iter()
                    .map(|project| project.id)
                    .collect::<Vec<_>>(),
            )
            .await?;
        for project in projects {
            let project_policy = policies
                .get(&project.id)
                .ok_or_else(|| format!("缺少已准入的记忆项目 {} 保留策略。", project.id))?;
            match self.run_project_retention(project.id, project_policy).await {
                Ok(result) => aggregate.merge(&result),
                Err(error) => aggregate
                    .project_failures
                    .push(NativeVmmRetentionProjectFailure {
                        project_id: project.id,
                        error,
                    }),
            }
        }
        self.prune_orphaned_terminal_analysis_jobs().await?;
        Ok(aggregate)
    }

    /// Waits while native retention is inactive and otherwise runs eager plus 30-minute full passes.
    /// 在原生保留未启用时等待，否则执行启动立即运行与每 30 分钟完整运行。
    async fn run_retention_worker(self) {
        self.run_periodic_cleanup(false).await;
    }

    /// Runs profile-expiry convergence on its own 30-second schedule independently of full retention.
    /// 按独立的 30 秒节拍运行画像过期收敛，不受完整保留调度循环控制。
    async fn run_profile_expiry_worker(self) {
        self.run_periodic_cleanup(true).await;
    }

    /// Reloads cleanup policy on notifications and bounded polls while keeping the configured cadence.
    /// 在通知和有界轮询时重载清洗策略，同时保持用户配置的执行周期。
    /// `profile_only` selects the independent profile worker; this long-lived loop has no normal return.
    /// `profile_only` 选择独立画像工作器；此长期循环不正常返回。
    async fn run_periodic_cleanup(self, profile_only: bool) {
        let notify = if profile_only {
            &self.profile_expiry_notify
        } else {
            &self.retention_notify
        };
        let mut last_completed = None;
        let mut was_enabled = false;
        loop {
            if !self.native_backend_enabled.load(Ordering::Acquire) {
                was_enabled = false;
                notify.notified().await;
                continue;
            }
            // A bad file suspends execution with an explicit error; it never activates fallback defaults.
            // 错误文件会以明确错误暂停执行，绝不启用回退默认值。
            let settings = self.runtime.retention_settings().await;
            let mut wait = NATIVE_VMM_POLICY_REFRESH_INTERVAL;
            match settings {
                Ok(settings) if settings.enabled => {
                    let now = tokio::time::Instant::now();
                    if !was_enabled {
                        last_completed = if profile_only { Some(now) } else { None };
                        was_enabled = true;
                    }
                    let interval = if profile_only {
                        Duration::from_secs(u64::from(settings.profile_expiry_interval_seconds))
                    } else {
                        Duration::from_secs(u64::from(settings.scan_interval_minutes) * 60)
                    };
                    let remaining = retention_policy::remaining_wait(now, last_completed, interval);
                    if remaining.is_zero() {
                        let outcome = match retention_policy::resolve(&settings) {
                            Ok(policy) if profile_only => {
                                self.run_profile_expiry_once(&policy).await
                            }
                            Ok(policy) => self.run_retention_with_policy(&policy).await,
                            Err(error) => Err(error),
                        };
                        match outcome {
                            Ok(result) => {
                                if let Some(error) = retention_project_failure_message(&result) {
                                    tracing::warn!(
                                        "[webui-service] native VMM periodic cleanup project failures: {error}"
                                    );
                                }
                            }
                            Err(error) => tracing::warn!(
                                "[webui-service] native VMM periodic cleanup failed: {error}"
                            ),
                        }
                        // Measure the next interval after completion so a slow scan never spins continuously.
                        // 从完成时间开始计算下一周期，避免慢扫描持续空转执行。
                        last_completed = Some(tokio::time::Instant::now());
                        wait = wait.min(interval);
                    } else {
                        wait = wait.min(remaining);
                    }
                }
                Ok(_) => was_enabled = false,
                Err(error) => {
                    was_enabled = false;
                    let mut status = self.retention_last_status.write().await;
                    if profile_only {
                        status.profile_expiry_last_error = Some(error.clone());
                    } else {
                        status.last_error = Some(error.clone());
                    }
                    tracing::warn!(
                        "[webui-service] native VMM cleanup configuration failed: {error}"
                    );
                }
            }
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                () = notify.notified() => {}
            }
        }
    }

    /// Runs one bounded profile-expiry convergence without triggering the heavier retention phases.
    /// 运行一次有界画像过期收敛，且不触发更重的其他保留阶段。
    async fn run_profile_expiry_once(
        &self,
        base: &RetentionPolicy,
    ) -> Result<NativeVmmRetentionApplyResult, String> {
        if !self.native_backend_enabled.load(Ordering::Acquire) {
            return Ok(NativeVmmRetentionApplyResult::default());
        }
        let started_at_unix_ms = unix_ns_to_ms(native_vmm_now_ns()?);
        self.profile_expiry_running.store(true, Ordering::Release);
        {
            let mut status = self.retention_last_status.write().await;
            status.profile_expiry_last_started_at_unix_ms = Some(started_at_unix_ms);
            status.profile_expiry_last_error = None;
        }
        let outcome: Result<NativeVmmRetentionApplyResult, String> = async {
            // Profile convergence uses the same project-then-operation lock order as every other
            // native project mutation, so it cannot race migration, clear, delete, or full retention.
            // 画像收敛沿用其他原生项目变更的“项目门后操作屏障”锁顺序，避免与迁移、清空、
            // 删除或完整保留任务竞争。
            let _project_guard = self.project_mutation_gate.lock().await;
            let _operation_guard = self.operation_gate.write().await;
            self.runtime.recover_storage_mutations().await?;
            let policy = native_profile_expiry_policy(base);
            let mut aggregate = NativeVmmRetentionApplyResult::default();
            for project in self.data_plane.list_projects().await? {
                match self.run_project_profile_expiry(project.id, &policy).await {
                    Ok(result) => aggregate.merge(&result),
                    Err(error) => {
                        aggregate
                            .project_failures
                            .push(NativeVmmRetentionProjectFailure {
                                project_id: project.id,
                                error,
                            })
                    }
                }
            }
            Ok(aggregate)
        }
        .await;
        let finished_at_unix_ms = native_vmm_now_ns().map(unix_ns_to_ms).ok();
        {
            let mut status = self.retention_last_status.write().await;
            status.profile_expiry_last_finished_at_unix_ms = finished_at_unix_ms;
            match &outcome {
                Ok(result) => {
                    status.profile_expiry_last_result = Some(result.clone());
                    status.profile_expiry_last_error = retention_project_failure_message(result);
                }
                Err(error) => {
                    status.profile_expiry_last_error = Some(error.clone());
                }
            }
        }
        self.profile_expiry_running.store(false, Ordering::Release);
        outcome
    }

    /// Runs one VMM-aligned retention cycle for one exact project scope.
    /// 为一个精确项目作用域运行一次与 VMM 对齐的保留周期。
    ///
    /// # Errors
    /// Returns an error when Core planning or an atomic snapshot transition fails.
    /// Core 规划或原子快照转换失败时返回错误。
    async fn run_project_retention(
        &self,
        project_id: u64,
        policy: &RetentionPolicy,
    ) -> Result<NativeVmmRetentionApplyResult, String> {
        let mut aggregate = NativeVmmRetentionApplyResult::default();
        let first = self.run_project_retention_pass(project_id, policy).await?;
        let completed_cold_jobs = u32::try_from(first.completed_cold_jobs)
            .map_err(|error| format!("原生 VMM 冷回合完成计数超出 u32 范围：{error}"))?;
        let remaining_cold_job_limit = policy
            .cold_turn_job_limit
            .saturating_sub(completed_cold_jobs);
        let enqueued_cold_jobs = first.enqueued_cold_jobs;
        let should_execute_new_jobs = enqueued_cold_jobs > 0 && remaining_cold_job_limit > 0;
        aggregate.merge(&first);
        if should_execute_new_jobs {
            // The external VMM scans before claiming one 16-job batch. Native planning cannot
            // reference jobs before they exist, so the follow-up pass executes only the unused
            // portion of that same batch and disables every other plan family.
            // 外置 VMM 会先扫描再领取一个 16 条作业批次。原生规划无法引用尚未存在的作业，
            // 因此后续阶段只执行同一批次尚未使用的额度，并关闭其他全部计划族。
            let follow_up_policy =
                native_cold_turn_follow_up_policy(policy, remaining_cold_job_limit);
            let follow_up = self
                .run_project_retention_pass(project_id, &follow_up_policy)
                .await?;
            aggregate.merge(&follow_up);
        }
        if enqueued_cold_jobs > 0 {
            // External VMM evaluates idle sessions after enqueueing and claiming cold-turn jobs in
            // the same scheduled pass. The first native plan intentionally excludes newly scanned
            // cold sessions, so this bounded idle-only pass restores that ordering without
            // repeating profile, terminal-memory, cold-scan, vector, or purge work.
            // 外置 VMM 会在同一次定时轮次中先入队并领取冷回合作业，再评估闲置会话。首个原生计划会
            // 有意排除刚扫描出的冷会话，因此这里用有界的仅闲置后续轮次恢复该顺序，同时不重复画像、
            // 终态记忆、冷扫描、向量或清除工作。
            let idle_follow_up = self
                .run_project_retention_pass(
                    project_id,
                    &native_idle_session_follow_up_policy(policy),
                )
                .await?;
            aggregate.merge(&idle_follow_up);
        }
        Ok(aggregate)
    }

    /// Plans and applies one atomic retention transition for one project and policy snapshot.
    /// 为一个项目与策略快照规划并应用一次原子保留转换。
    ///
    /// # Errors
    /// Core planning, storage validation, or durable persistence failures are returned unchanged.
    /// Core 规划、存储校验或耐久持久化失败会原样返回。
    async fn run_project_retention_pass(
        &self,
        project_id: u64,
        policy: &RetentionPolicy,
    ) -> Result<NativeVmmRetentionApplyResult, String> {
        let now_ns = native_vmm_now_ns()?;
        let storage_version = self.runtime.storage_snapshot()?.storage_version;
        let plan = self.retention_plan(project_id, now_ns, policy).await?;
        if retention_plan_is_empty(&plan) {
            return Ok(NativeVmmRetentionApplyResult::default());
        }
        let operation_id = format!("native-vmm-retention:{}", uuid::Uuid::new_v4());
        let batch_prefix = operation_id.clone();
        let policy = policy.clone();
        self.runtime
            .transform_storage_snapshot(
                &operation_id,
                "native-vmm-retention-v1",
                storage_version,
                move |snapshot| {
                    apply_retention_plan(snapshot, &plan, &policy, now_ns, &batch_prefix)
                },
            )
            .await
    }

    /// Applies one project-local profile-expiry plan using the lightweight maintenance policy.
    /// 使用轻量维护策略应用一份项目局部画像过期计划。
    async fn run_project_profile_expiry(
        &self,
        project_id: u64,
        policy: &RetentionPolicy,
    ) -> Result<NativeVmmRetentionApplyResult, String> {
        let now_ns = native_vmm_now_ns()?;
        let storage_version = self.runtime.storage_snapshot()?.storage_version;
        let plan = self.retention_plan(project_id, now_ns, policy).await?;
        if retention_plan_is_empty(&plan) {
            return Ok(NativeVmmRetentionApplyResult::default());
        }
        let operation_id = format!("native-vmm-profile-expiry:{}", uuid::Uuid::new_v4());
        let batch_prefix = operation_id.clone();
        let policy = policy.clone();
        self.runtime
            .transform_storage_snapshot(
                &operation_id,
                "native-vmm-profile-expiry-v1",
                storage_version,
                move |snapshot| {
                    apply_retention_plan(snapshot, &plan, &policy, now_ns, &batch_prefix)
                },
            )
            .await
    }

    /// Obtains one host-authorized retention plan from the copied native Core.
    /// 从复制的原生 Core 获取一份经宿主授权的保留计划。
    ///
    /// # Errors
    /// Returns the exact identity, policy, or Core command failure.
    /// 身份、策略或 Core 命令失败时返回准确错误。
    async fn retention_plan(
        &self,
        project_id: u64,
        now_ns: i64,
        policy: &RetentionPolicy,
    ) -> Result<RetentionPlanSet, String> {
        let operation = format!("native-vmm-retention-plan:{}", uuid::Uuid::new_v4());
        let result = self
            .runtime
            .run(CoreCommand::Retention(retention_command(
                project_id, now_ns, policy, operation,
            )?))
            .await
            .map_err(|error| error.to_string())?;
        let CommandResult::Retention(plan) = result else {
            return Err(String::from("原生 VMM Core 返回了非保留计划结果。"));
        };
        Ok(plan)
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
        let analysis_jobs = self.data_plane.list_analysis_jobs_for_audit().await?;
        build_memory_records(
            &snapshot,
            project_id,
            session_source_id,
            analysis_jobs.as_slice(),
        )
    }

    /// Reads lightweight session summaries without materializing turn bodies or loading analysis jobs.
    /// 读取轻量会话摘要，不构造回合正文或加载分析作业。
    /// Takes the exact native project ID; returns a coherent storage version and ordered session summaries.
    /// 接收准确原生项目 ID；返回一致的存储版本及有序会话摘要。
    pub(crate) async fn session_summaries(
        &self,
        project_id: u64,
    ) -> Result<(u64, Vec<NativeVmmMemorySessionSummary>), String> {
        let _project_guard = self.project_mutation_gate.lock().await;
        let _operation_guard = self.operation_gate.read().await;
        self.ensure_project(project_id).await?;
        let snapshot = self.runtime.storage_snapshot()?;
        Ok((
            snapshot.storage_version,
            build_session_summaries(&snapshot, project_id)?,
        ))
    }

    /// Loads every durable profile-node state for one explicit native target.
    /// 加载一个明确原生目标的全部持久画像节点状态。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` selects the one-level native project used by the project target or user display context.
    /// `project_id` 选择项目目标或用户显示上下文使用的一级原生项目。
    /// `target` selects the fixed user profile or the selected project profile.
    /// `target` 选择固定用户画像或所选项目画像。
    ///
    /// # Returns
    /// 返回值
    /// A coherent all-state profile projection that does not alter the active-only runtime profile contract.
    /// 返回一份一致的全状态画像投影，且不会改变仅活动节点参与运行时画像的契约。
    pub(crate) async fn profile_records(
        &self,
        project_id: u64,
        target: NativeVmmProfileTarget,
    ) -> Result<NativeVmmProfileRecords, String> {
        // The same project and operation barriers keep catalog validation and the Core snapshot on
        // one maintenance boundary while still leaving the runtime active-profile API unchanged.
        // 相同的项目与操作屏障让目录校验和 Core 快照处于同一维护边界，同时保持运行时
        // 活动画像 API 不变。
        let _project_guard = self.project_mutation_gate.lock().await;
        let _operation_guard = self.operation_gate.write().await;
        self.ensure_project(project_id).await?;
        build_profile_records(
            &self.runtime.storage_snapshot()?,
            project_id,
            target,
            native_vmm_now_ns()?,
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
        let previous = self.bound_project(vulcan_project_id).await?;
        let project = self
            .data_plane
            .project_by_id(vmm_project_id)
            .await?
            .ok_or_else(|| format!("原生 VMM 项目 {vmm_project_id} 不存在"))?;
        self.data_plane
            .bind_vulcan_project(vulcan_project_id, vmm_project_id)
            .await?;
        if let Some(previous) = previous
            && previous.id != vmm_project_id
        {
            self.revoke_previous_writebacks(vulcan_project_id, previous.id)?;
        }
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
        let previous = self.bound_project(vulcan_project_id).await?;
        let access = self
            .data_plane
            .set_project_access(vulcan_project_id, owner_project_id, linked_project_ids)
            .await?;
        if let Some(previous) = previous
            && previous.id != owner_project_id
        {
            self.revoke_previous_writebacks(vulcan_project_id, previous.id)?;
        }
        Ok(access)
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
        let previous = self.bound_project(vulcan_project_id).await?;
        let project = self
            .data_plane
            .ensure_project(project_name, vulcan_project_id)
            .await?;
        self.data_plane
            .bind_vulcan_project(vulcan_project_id, project.id)
            .await?;
        if let Some(previous) = previous
            && previous.id != project.id
        {
            self.revoke_previous_writebacks(vulcan_project_id, previous.id)?;
        }
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
        let previous = self.bound_project(vulcan_project_id).await?;
        self.data_plane
            .unbind_vulcan_project(vulcan_project_id)
            .await?;
        if let Some(previous) = previous {
            self.revoke_previous_writebacks(vulcan_project_id, previous.id)?;
        }
        Ok(())
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
            for owner in &preview.usage.owner_vulcan_project_ids {
                self.revoke_previous_writebacks(owner, project_id)?;
            }
            self.prune_orphaned_terminal_analysis_jobs().await?;
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
        for owner in &deleted_preview.usage.owner_vulcan_project_ids {
            self.revoke_previous_writebacks(owner, project_id)?;
        }
        self.prune_orphaned_terminal_analysis_jobs().await?;
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
        // Target-side pending extraction can materialize a session after preview and invalidate
        // the conflict count or the deterministic rename plan, so both scopes must be quiescent.
        // 目标侧待处理提取可能在预检后落入新会话，导致冲突数量或确定性改名方案失真，因此两个作用域都必须静止。
        self.ensure_no_pending_analysis_jobs(target_project_id)
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
        // Recheck the target under the write gate so queued target work cannot invalidate the
        // accepted preview between confirmation and the atomic snapshot migration.
        // 在写入守卫内重新检查目标，避免目标队列在确认与原子快照迁移之间使已接受预检失效。
        self.ensure_no_pending_analysis_jobs(target_project_id)
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
        self.prune_orphaned_terminal_analysis_jobs().await?;
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

    /// Removes terminal host audit rows whose Core turns no longer exist in hot or recoverable storage.
    /// 删除其 Core 回合在活动区和可恢复存储中都已不存在的终态宿主审计记录。
    ///
    /// # Returns
    /// 返回值
    /// The number of terminal audit rows selected for idempotent deletion.
    /// 返回选中进行幂等删除的终态审计记录数量。
    ///
    /// # Errors
    /// 错误
    /// Returns an error when the current snapshot, audit ledger, or SQLite delete cannot be verified.
    /// 当前快照、审计账本或 SQLite 删除无法校验时返回错误。
    async fn prune_orphaned_terminal_analysis_jobs(&self) -> Result<u64, String> {
        let snapshot = self.runtime.storage_snapshot()?;
        let jobs = self.data_plane.list_analysis_jobs_for_audit().await?;
        let operation_ids = orphaned_terminal_analysis_operation_ids(&snapshot, &jobs);
        self.data_plane
            .delete_terminal_analysis_jobs(&operation_ids)
            .await?;
        Ok(usize_to_u64(operation_ids.len()))
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

/// Builds the lightweight policy used by the 30-second profile lifecycle worker.
/// 构建 30 秒画像生命周期工作器使用的轻量策略。
///
/// # Returns
/// 返回值
/// The admitted base policy with every unrelated retention family disabled.
/// 返回已准入基础策略的副本，并关闭所有无关保留计划族。
/// `base` supplies the exact policy admitted for this scan.
/// `base` 提供本轮扫描已准入的准确策略。
#[must_use]
fn native_profile_expiry_policy(base: &RetentionPolicy) -> RetentionPolicy {
    let mut policy = base.clone();
    policy.memory_recycle_limit = 0;
    policy.cold_turn_scan_limit = 0;
    policy.cold_turn_job_limit = 0;
    policy.idle_session_limit = 0;
    policy.vector_gc_limit = 0;
    policy.trash_purge_limit = 0;
    policy
}

/// Builds the execution-only follow-up used after the current cycle enqueues cold-turn jobs.
/// 构建当前周期入队冷回合作业后使用的仅执行后续策略。
///
/// # Parameters
/// `base` supplies the policy frozen for the first pass of this cycle.
/// `base` 提供本周期首个阶段冻结的策略。
/// `remaining_job_limit` is the unused portion of the frozen per-cycle execution budget.
/// `remaining_job_limit` 是冻结单周期执行额度中尚未使用的部分。
///
/// # Returns
/// The same admitted policy with only bounded due cold-job execution enabled.
/// 返回同一已准入策略，仅启用有界到期冷作业执行。
#[must_use]
fn native_cold_turn_follow_up_policy(
    base: &RetentionPolicy,
    remaining_job_limit: u32,
) -> RetentionPolicy {
    let mut policy = base.clone();
    policy.profile_expiry_limit = 0;
    policy.memory_recycle_limit = 0;
    policy.cold_turn_scan_limit = 0;
    policy.cold_turn_job_limit = remaining_job_limit.min(policy.cold_turn_job_limit);
    policy.idle_session_limit = 0;
    policy.vector_gc_limit = 0;
    policy.trash_purge_limit = 0;
    policy
}

/// Builds the idle-only follow-up used after the current cycle discovers cold sessions.
/// 构建当前周期发现冷会话后使用的仅闲置后续策略。
///
/// # Returns
/// The admitted base policy with only bounded idle-session recycling enabled.
/// 返回已准入基础策略，仅启用有界闲置会话回收。
/// `base` supplies the policy frozen for the first pass of this cycle.
/// `base` 提供本周期首个阶段冻结的策略。
#[must_use]
fn native_idle_session_follow_up_policy(base: &RetentionPolicy) -> RetentionPolicy {
    let mut policy = base.clone();
    policy.profile_expiry_limit = 0;
    policy.memory_recycle_limit = 0;
    policy.cold_turn_scan_limit = 0;
    policy.cold_turn_job_limit = 0;
    policy.vector_gc_limit = 0;
    policy.trash_purge_limit = 0;
    policy
}

/// Reports whether a Core retention plan contains no durable work.
/// 报告 Core 保留计划是否不包含任何耐久工作。
#[must_use]
fn retention_plan_is_empty(plan: &RetentionPlanSet) -> bool {
    plan.profile_expiry.is_empty()
        && plan.terminal_memory_recycle.is_empty()
        && plan.cold_turn_enqueue.is_empty()
        && plan.cold_turn_execute.is_empty()
        && plan.idle_session_recycle.is_empty()
        && plan.vector_gc_retry.is_empty()
        && plan.trash_purge.is_empty()
}

/// Finds terminal host-analysis rows whose referenced turn is no longer hot or recoverable.
/// 查找其引用回合已不在活动区或可恢复存储中的终态宿主分析记录。
///
/// # Parameters
/// 参数
/// `snapshot` is the authoritative Core state including recycle batches, while `jobs` is the
/// independently durable host audit ledger observed under the same operation barrier.
/// `snapshot` 是包含回收批次的权威 Core 状态，`jobs` 是在同一操作屏障下观察到的独立耐久宿主审计账本。
///
/// # Returns
/// 返回值
/// Stable operation identities safe to delete. Terminal no-turn receipts remain because no
/// approved time-based idempotency expiry policy exists.
/// 返回可安全删除的稳定操作身份；无回合的终态收据会保留，因为当前没有获批的时间型幂等过期策略。
fn orphaned_terminal_analysis_operation_ids(
    snapshot: &StorageSnapshot,
    jobs: &[NativeVmmPendingAnalysisJob],
) -> Vec<String> {
    let retained_turn_ids = snapshot
        .dataset
        .turns
        .iter()
        .chain(
            snapshot
                .retention_trash_batches
                .iter()
                .flat_map(|batch| batch.dataset.turns.iter()),
        )
        .map(|turn| turn.id.as_str())
        .collect::<BTreeSet<_>>();
    jobs.iter()
        .filter(|job| {
            matches!(
                job.state.as_str(),
                VMM_PENDING_ANALYSIS_COMPLETED
                    | VMM_PENDING_ANALYSIS_BYPASSED
                    | VMM_PENDING_ANALYSIS_CORRUPTED
            ) && job
                .turn_id
                .as_deref()
                .is_some_and(|turn_id| !retained_turn_ids.contains(turn_id))
        })
        .map(|job| job.operation_id.clone())
        .collect()
}

/// Rejects a Core plan whose family sizes exceed the frozen native policy bounds.
/// 拒绝任一计划族数量超过冻结原生策略边界的 Core 计划。
fn validate_native_retention_plan_bounds(
    plan: &RetentionPlanSet,
    policy: &RetentionPolicy,
) -> Result<(), String> {
    let within = |actual: usize, limit: u32| actual <= limit as usize;
    if !within(plan.profile_expiry.len(), policy.profile_expiry_limit)
        || !within(
            plan.terminal_memory_recycle.len(),
            policy.memory_recycle_limit,
        )
        || !within(plan.cold_turn_enqueue.len(), policy.cold_turn_scan_limit)
        || !within(plan.cold_turn_execute.len(), policy.cold_turn_job_limit)
        || !within(plan.idle_session_recycle.len(), policy.idle_session_limit)
        || !within(plan.vector_gc_retry.len(), policy.vector_gc_limit)
        || !within(plan.trash_purge.len(), policy.trash_purge_limit)
    {
        return Err(String::from(
            "原生 VMM 保留计划超过冻结策略规定的单轮处理上限。",
        ));
    }
    Ok(())
}

/// Renders bounded project-local failures for diagnostics without hiding successful work.
/// 为诊断渲染有界的项目局部失败，同时不隐藏已成功完成的工作。
fn retention_project_failure_message(result: &NativeVmmRetentionApplyResult) -> Option<String> {
    if result.project_failures.is_empty() {
        return None;
    }
    Some(
        result
            .project_failures
            .iter()
            .map(|failure| format!("项目 {}：{}", failure.project_id, failure.error))
            .collect::<Vec<_>>()
            .join("；"),
    )
}

/// Applies one Core-authorized retention plan to a staged host snapshot.
/// 将一份经 Core 授权的保留计划应用到宿主暂存快照。
fn apply_retention_plan(
    snapshot: &mut StorageSnapshot,
    plan: &RetentionPlanSet,
    policy: &RetentionPolicy,
    now_ns: i64,
    batch_prefix: &str,
) -> Result<NativeVmmRetentionApplyResult, String> {
    validate_native_retention_plan_bounds(plan, policy)?;
    // Freeze the selected policy deadline once for every new batch in this atomic pass.
    // 为本次原子处理中新建的全部批次一次性冻结选定策略的截止时间。
    let expires_at_ns = now_ns
        .checked_add(retention_duration_ns(policy.trash_retention_ms)?)
        .ok_or_else(|| String::from("原生 VMM 回收批次到期时间超出可表示范围。"))?;
    if !plan.vector_gc_retry.is_empty() {
        return Err(String::from(
            "原生 VMM 使用快照变更日志完成向量清理，不接受独立向量 GC 计划。",
        ));
    }
    let mut result = NativeVmmRetentionApplyResult::default();
    let expected_subject = native_vmm_subject_id()?;
    if plan.subject_id != expected_subject {
        return Err(String::from("原生 VMM 保留计划主体与固定宿主用户不一致。"));
    }

    let mut profile_node_ids = BTreeSet::new();
    for planned in &plan.profile_expiry {
        if planned.reason != RetentionReasonCode::ProfileExpiry
            || !profile_node_ids.insert(planned.profile_node_id.clone())
        {
            return Err(format!(
                "原生 VMM 画像过期计划中的节点 `{}` 使用了错误的原因码或重复身份。",
                planned.profile_node_id
            ));
        }
        let node = snapshot
            .dataset
            .profile_nodes
            .iter()
            .find(|node| node.id == planned.profile_node_id)
            .ok_or_else(|| {
                format!(
                    "原生 VMM 画像过期计划引用了不存在的节点 `{}`。",
                    planned.profile_node_id
                )
            })?;
        if node.version != planned.expected_version
            || node.subject_id != planned.expected_subject_id
            || node.scope_id != planned.expected_scope_id
            || node.scope_id != plan.scope_id
                && (node.scope_id != native_vmm_subject_scope()?
                    || node.subject_id.as_ref() != Some(&plan.subject_id))
            || node.state != ProfileNodeState::Active
            || node
                .expires_at_ns
                .is_none_or(|expires_at_ns| expires_at_ns > now_ns)
        {
            return Err(format!(
                "原生 VMM 画像过期计划中的节点 `{}` 已发生变化，请重新规划。",
                planned.profile_node_id
            ));
        }
    }
    for planned in &plan.profile_expiry {
        let node = snapshot
            .dataset
            .profile_nodes
            .iter_mut()
            .find(|node| node.id == planned.profile_node_id)
            .ok_or_else(|| {
                format!(
                    "原生 VMM 画像过期节点 `{}` 在应用阶段消失。",
                    planned.profile_node_id
                )
            })?;
        node.state = ProfileNodeState::Expired;
        node.status_reason = Some(String::from("expired by lifecycle convergence"));
        node.updated_at_ns = now_ns;
        node.version = node.version.checked_add(1).ok_or_else(|| {
            format!(
                "原生 VMM 画像节点 `{}` 的版本号已经耗尽。",
                planned.profile_node_id
            )
        })?;
        result.expired_profile_nodes = result.expired_profile_nodes.saturating_add(1);
    }

    let mut terminal_ids = BTreeSet::new();
    for planned in &plan.terminal_memory_recycle {
        if planned.reason != RetentionReasonCode::TerminalMemoryRecycle
            || !terminal_ids.insert(planned.memory_id.clone())
        {
            return Err(format!(
                "原生 VMM 终态记忆回收计划中的记忆 `{}` 使用了错误的原因码或重复身份。",
                planned.memory_id
            ));
        }
        let memory = snapshot
            .dataset
            .memories
            .iter()
            .find(|memory| memory.id.as_str() == planned.memory_id)
            .ok_or_else(|| {
                format!(
                    "原生 VMM 保留计划引用了不存在的记忆 `{}`。",
                    planned.memory_id
                )
            })?;
        if memory.version != planned.expected_version
            || MemoryAccessIdentity::from(memory) != planned.expected_access
            || memory.scope_id != plan.scope_id
            || !matches!(
                memory.state,
                MemoryState::Superseded | MemoryState::Deleted | MemoryState::Expired
            )
        {
            return Err(format!(
                "原生 VMM 保留计划中的记忆 `{}` 已发生变化，请重新规划。",
                planned.memory_id
            ));
        }
    }
    let mut terminal_batch = retention_trash_batch(
        format!("{batch_prefix}:terminal"),
        plan.subject_id.clone(),
        plan.scope_id.clone(),
        now_ns,
        expires_at_ns,
        RetentionReasonCode::TerminalMemoryRecycle,
    );
    move_memories_to_retention_batch(snapshot, &terminal_ids, &mut terminal_batch);
    result.recycled_memories = usize_to_u64(terminal_batch.dataset.memories.len());
    push_retention_batch(snapshot, terminal_batch);

    let mut existing_job_sessions = snapshot
        .retention_cold_turn_jobs
        .iter()
        .filter(|job| job.subject_id == plan.subject_id && job.scope_id == plan.scope_id)
        .map(|job| job.session_source_id.clone())
        .collect::<BTreeSet<_>>();
    let mut enqueued_session_ids = BTreeSet::new();
    for (job_index, planned) in plan.cold_turn_enqueue.iter().enumerate() {
        if planned.reason != RetentionReasonCode::ColdTurnRecycle
            || !enqueued_session_ids.insert(planned.session_source_id.clone())
        {
            return Err(format!(
                "原生 VMM 冷回合入队计划中的会话 `{}` 使用了错误的原因码或重复身份。",
                planned.session_source_id
            ));
        }
        let session_source_id = SessionSourceId::new(planned.session_source_id.clone())
            .map_err(|error| format!("原生 VMM 冷回合任务会话标识无效：{error}"))?;
        if existing_job_sessions.contains(&session_source_id) {
            return Err(format!(
                "原生 VMM 冷回合入队计划中的会话 `{}` 已存在耐久任务，请重新规划。",
                planned.session_source_id
            ));
        }
        if recyclable_turn_ids(
            snapshot,
            &plan.subject_id,
            &plan.scope_id,
            &session_source_id,
            policy.turn_hot_window_size,
        )
        .is_empty()
        {
            return Err(format!(
                "原生 VMM 冷回合入队计划中的会话 `{}` 已无可回收回合，请重新规划。",
                planned.session_source_id
            ));
        }
        snapshot
            .retention_cold_turn_jobs
            .push(StorageRetentionColdTurnJob {
                // Replaying the same admitted step must recreate the identities used by follow-ups.
                // 重放同一个准入步骤必须重建后续阶段引用的相同身份。
                job_id: format!("native-vmm-cold-turn-job:{batch_prefix}:{job_index:08}"),
                subject_id: plan.subject_id.clone(),
                scope_id: plan.scope_id.clone(),
                session_source_id: session_source_id.clone(),
                next_run_at_ns: now_ns,
            });
        existing_job_sessions.insert(session_source_id);
        result.enqueued_cold_jobs = result.enqueued_cold_jobs.saturating_add(1);
    }

    let mut cold_turn_ids = BTreeSet::new();
    let mut cold_job_ids = BTreeSet::new();
    let mut cold_job_session_ids = BTreeSet::new();
    for planned in &plan.cold_turn_execute {
        if planned.reason != RetentionReasonCode::ColdTurnRecycle
            || !cold_job_ids.insert(planned.job_id.clone())
            || !cold_job_session_ids.insert(planned.session_source_id.clone())
        {
            return Err(format!(
                "原生 VMM 冷回合执行计划中的任务 `{}` 使用了错误的原因码，或重复了任务/会话身份。",
                planned.job_id
            ));
        }
        let job_index = snapshot
            .retention_cold_turn_jobs
            .iter()
            .position(|job| {
                job.job_id == planned.job_id
                    && job.subject_id == plan.subject_id
                    && job.scope_id == plan.scope_id
                    && job.session_source_id.as_str() == planned.session_source_id
                    && job.next_run_at_ns <= now_ns
            })
            .ok_or_else(|| {
                format!(
                    "原生 VMM 冷回合任务 `{}` 已变化或尚未到期，请重新规划。",
                    planned.job_id
                )
            })?;
        let job = snapshot.retention_cold_turn_jobs.remove(job_index);
        cold_turn_ids.extend(recyclable_turn_ids(
            snapshot,
            &job.subject_id,
            &job.scope_id,
            &job.session_source_id,
            policy.turn_hot_window_size,
        ));
        result.completed_cold_jobs = result.completed_cold_jobs.saturating_add(1);
    }
    let mut cold_batch = retention_trash_batch(
        format!("{batch_prefix}:cold-turn"),
        plan.subject_id.clone(),
        plan.scope_id.clone(),
        now_ns,
        expires_at_ns,
        RetentionReasonCode::ColdTurnRecycle,
    );
    move_turns_to_retention_batch(snapshot, &cold_turn_ids, &mut cold_batch);
    result.recycled_turns = result
        .recycled_turns
        .saturating_add(usize_to_u64(cold_batch.dataset.turns.len()));
    push_retention_batch(snapshot, cold_batch);

    let idle_cutoff_ns = now_ns.saturating_sub(retention_duration_ns(policy.session_idle_ms)?);
    let mut idle_memory_ids = BTreeSet::new();
    let mut idle_turn_ids = BTreeSet::new();
    let mut idle_session_ids = BTreeSet::new();
    for planned in &plan.idle_session_recycle {
        if planned.reason != RetentionReasonCode::IdleSessionRecycle
            || !idle_session_ids.insert(planned.session_source_id.clone())
        {
            return Err(format!(
                "原生 VMM 闲置会话回收计划中的会话 `{}` 使用了错误的原因码或重复身份。",
                planned.session_source_id
            ));
        }
        let session_source_id = SessionSourceId::new(planned.session_source_id.clone())
            .map_err(|error| format!("原生 VMM 闲置会话标识无效：{error}"))?;
        let session_turns = snapshot.dataset.turns.iter().filter(|turn| {
            turn.subject_id == plan.subject_id
                && turn.scope_id == plan.scope_id
                && turn.session_source_id == session_source_id
        });
        let latest_session_activity_ns = retention_session_latest_activity_ns(
            snapshot,
            &plan.subject_id,
            &plan.scope_id,
            &session_source_id,
        );
        if session_turns
            .clone()
            .any(|turn| turn.status == TurnStatus::Pending)
            || latest_session_activity_ns.is_none()
            || latest_session_activity_ns.is_some_and(|latest| latest > idle_cutoff_ns)
        {
            return Err(format!(
                "原生 VMM 闲置会话 `{}` 已恢复活动，请重新规划。",
                planned.session_source_id
            ));
        }
        let session_memory_ids = idle_session_memory_ids(
            snapshot,
            &plan.subject_id,
            &plan.scope_id,
            &session_source_id,
            idle_cutoff_ns,
        );
        let session_turn_ids = recyclable_turn_ids(
            snapshot,
            &plan.subject_id,
            &plan.scope_id,
            &session_source_id,
            policy.turn_hot_window_size,
        );
        if session_memory_ids.is_empty()
            && session_turn_ids.is_empty()
            && !cold_job_session_ids.contains(&planned.session_source_id)
        {
            return Err(format!(
                "原生 VMM 闲置会话 `{}` 已无可回收内容，请重新规划。",
                planned.session_source_id
            ));
        }
        idle_memory_ids.extend(session_memory_ids);
        idle_turn_ids.extend(session_turn_ids);
    }
    let mut idle_batch = retention_trash_batch(
        format!("{batch_prefix}:idle-session"),
        plan.subject_id.clone(),
        plan.scope_id.clone(),
        now_ns,
        expires_at_ns,
        RetentionReasonCode::IdleSessionRecycle,
    );
    move_memories_to_retention_batch(snapshot, &idle_memory_ids, &mut idle_batch);
    move_turns_to_retention_batch(snapshot, &idle_turn_ids, &mut idle_batch);
    result.recycled_memories = result
        .recycled_memories
        .saturating_add(usize_to_u64(idle_batch.dataset.memories.len()));
    result.recycled_turns = result
        .recycled_turns
        .saturating_add(usize_to_u64(idle_batch.dataset.turns.len()));
    push_retention_batch(snapshot, idle_batch);

    let mut purge_ids = BTreeSet::new();
    for planned in &plan.trash_purge {
        if planned.reason != RetentionReasonCode::TrashExpiryPurge
            || !purge_ids.insert(planned.batch_id.as_str())
        {
            return Err(format!(
                "原生 VMM 回收站清除计划中的批次 `{}` 使用了错误的原因码或重复身份。",
                planned.batch_id
            ));
        }
        let batch = snapshot
            .retention_trash_batches
            .iter()
            .find(|batch| batch.batch_id == planned.batch_id)
            .ok_or_else(|| {
                format!(
                    "原生 VMM 回收站清除计划引用了不存在的批次 `{}`。",
                    planned.batch_id
                )
            })?;
        if batch.subject_id != plan.subject_id
            || batch.scope_id != plan.scope_id
            || batch.recycled_at_ns > now_ns
            || now_ns < batch.expires_at_ns
        {
            return Err(format!(
                "原生 VMM 回收站批次 `{}` 的所有权或到期状态已变化，请重新规划。",
                planned.batch_id
            ));
        }
    }
    let purged_domain_ids = snapshot
        .retention_trash_batches
        .iter()
        .filter(|batch| purge_ids.contains(batch.batch_id.as_str()))
        .flat_map(|batch| dataset_domain_ids(&batch.dataset))
        .collect::<BTreeSet<_>>();
    // Session ownership is small metadata; do not clone retained vectors just to forget a session.
    // 会话归属是小型元数据，不为遗忘会话而复制保留向量。
    let purged_selections = snapshot
        .retention_trash_batches
        .iter()
        .filter(|batch| purge_ids.contains(batch.batch_id.as_str()))
        .filter_map(|batch| batch.manual_selection.clone())
        .collect::<Vec<_>>();
    for selection in &purged_selections {
        manual_session_lifecycle::apply(snapshot, Some(selection), false)?;
    }
    snapshot
        .retention_trash_batches
        .retain(|batch| !purge_ids.contains(batch.batch_id.as_str()));
    snapshot
        .outcomes
        .retain(|entry| !mutation_outcome_references_any(&entry.outcome, &purged_domain_ids));
    result.purged_trash_batches = usize_to_u64(purge_ids.len());
    Ok(result)
}

/// Projects one durable trash batch into a stable human-maintenance summary.
/// 将一个耐久回收批次投影为稳定的人工维护摘要。
fn retention_batch_summary(
    batch: &StorageRetentionTrashBatch,
) -> Result<NativeVmmRecycleBatchSummary, String> {
    Ok(NativeVmmRecycleBatchSummary {
        manual_selection: batch
            .manual_selection
            .as_ref()
            .map(|selection| match selection {
                vulcan_memorymesh_core::StorageManualRecycleSelection::Sessions { sessions } => {
                    vulcan_service_api::VmmNativeRemovalTarget::Sessions {
                        ids: sessions
                            .iter()
                            .map(|session| session.session_source_id.to_string())
                            .collect(),
                    }
                }
                vulcan_memorymesh_core::StorageManualRecycleSelection::Turns { turn_ids } => {
                    vulcan_service_api::VmmNativeRemovalTarget::Turns {
                        ids: turn_ids.iter().map(ToString::to_string).collect(),
                    }
                }
            }),
        batch_id: batch.batch_id.clone(),
        project_id: native_project_id_from_scope(&batch.scope_id)?,
        reason: retention_reason_token(batch.reason).to_owned(),
        recycled_at_unix_ms: unix_ns_to_ms(batch.recycled_at_ns),
        expires_at_unix_ms: unix_ns_to_ms(batch.expires_at_ns),
        turn_count: usize_to_u64(batch.dataset.turns.len()),
        memory_count: usize_to_u64(batch.dataset.memories.len()),
        profile_node_count: usize_to_u64(batch.dataset.profile_nodes.len()),
        context_evidence_count: usize_to_u64(batch.dataset.context_evidence.len()),
    })
}

/// Rejects restoring a recycle batch when any durable identity is active again.
/// 当任一耐久身份已重新处于活动状态时拒绝恢复回收批次。
fn ensure_recycle_restore_has_no_conflicts(
    snapshot: &StorageSnapshot,
    batch: &StorageRetentionTrashBatch,
) -> Result<(), String> {
    manual_session_lifecycle::validate(snapshot, batch.manual_selection.as_ref())?;
    let active_turn_ids = snapshot
        .dataset
        .turns
        .iter()
        .map(|turn| turn.id.as_str())
        .collect::<BTreeSet<_>>();
    let active_memory_ids = snapshot
        .dataset
        .memories
        .iter()
        .map(|memory| memory.id.as_str())
        .collect::<BTreeSet<_>>();
    let active_profile_ids = snapshot
        .dataset
        .profile_nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    let active_evidence_ids = snapshot
        .dataset
        .context_evidence
        .iter()
        .map(|evidence| {
            context_evidence_domain_id(
                &evidence.memory_id,
                &evidence.context_key,
                &evidence.context_value,
            )
        })
        .collect::<BTreeSet<_>>();
    let active_boundary_ids = snapshot
        .dataset
        .chat_compact_boundaries
        .iter()
        .map(compact_boundary_identity)
        .collect::<BTreeSet<_>>();
    let active_attempt_ids = snapshot
        .analysis_attempts
        .iter()
        .map(analysis_attempt_identity)
        .collect::<BTreeSet<_>>();

    let mut conflicts = Vec::new();
    conflicts.extend(
        batch
            .dataset
            .turns
            .iter()
            .filter(|turn| active_turn_ids.contains(turn.id.as_str()))
            .map(|turn| format!("turn:{}", turn.id.as_str())),
    );
    conflicts.extend(
        batch
            .dataset
            .memories
            .iter()
            .filter(|memory| active_memory_ids.contains(memory.id.as_str()))
            .map(|memory| format!("memory:{}", memory.id.as_str())),
    );
    conflicts.extend(
        batch
            .dataset
            .profile_nodes
            .iter()
            .filter(|node| active_profile_ids.contains(node.id.as_str()))
            .map(|node| format!("profile:{}", node.id.as_str())),
    );
    conflicts.extend(
        batch
            .dataset
            .context_evidence
            .iter()
            .filter_map(|evidence| {
                let identity = context_evidence_domain_id(
                    &evidence.memory_id,
                    &evidence.context_key,
                    &evidence.context_value,
                );
                active_evidence_ids
                    .contains(&identity)
                    .then(|| format!("context_evidence:{identity}"))
            }),
    );
    conflicts.extend(
        batch
            .dataset
            .chat_compact_boundaries
            .iter()
            .filter_map(|boundary| {
                let identity = compact_boundary_identity(boundary);
                active_boundary_ids
                    .contains(&identity)
                    .then(|| format!("compact_boundary:{identity}"))
            }),
    );
    conflicts.extend(batch.analysis_attempts.iter().filter_map(|attempt| {
        let identity = analysis_attempt_identity(attempt);
        active_attempt_ids
            .contains(&identity)
            .then(|| format!("analysis_attempt:{identity}"))
    }));
    if conflicts.is_empty() {
        return Ok(());
    }
    conflicts.sort();
    Err(format!(
        "原生 VMM 回收批次 `{}` 与 {} 个活动身份冲突：{}",
        batch.batch_id,
        conflicts.len(),
        conflicts.join("，")
    ))
}

/// Restores every record in one already-validated recycle batch into active storage.
/// 将一个已经校验的回收批次中的全部记录恢复到活动存储。
fn restore_retention_batch(
    snapshot: &mut StorageSnapshot,
    batch: StorageRetentionTrashBatch,
) -> Result<(), String> {
    manual_session_lifecycle::apply(snapshot, batch.manual_selection.as_ref(), true)?;
    snapshot.dataset.turns.extend(batch.dataset.turns);
    snapshot.dataset.memories.extend(batch.dataset.memories);
    snapshot
        .dataset
        .profile_nodes
        .extend(batch.dataset.profile_nodes);
    snapshot
        .dataset
        .context_evidence
        .extend(batch.dataset.context_evidence);
    snapshot
        .dataset
        .chat_compact_boundaries
        .extend(batch.dataset.chat_compact_boundaries);
    snapshot.analysis_attempts.extend(batch.analysis_attempts);
    Ok(())
}

/// Builds the unique durable identity of one compact-boundary row.
/// 构建一条压缩边界记录的唯一耐久身份。
fn compact_boundary_identity(boundary: &vulcan_memorymesh_core::ChatCompactBoundary) -> String {
    format!(
        "{}\u{0}{}\u{0}{}",
        boundary.subject_id.as_str(),
        boundary.scope_id.as_str(),
        boundary.session_source_id.as_str()
    )
}

/// Builds the unique durable identity of one analysis-attempt row.
/// 构建一条分析尝试记录的唯一耐久身份。
fn analysis_attempt_identity(attempt: &vulcan_memorymesh_core::AnalysisAttemptIdentity) -> String {
    format!(
        "{}\u{0}{}",
        attempt.turn_id.as_str(),
        attempt.analysis_generation
    )
}

/// Converts one frozen retention reason into its stable API token.
/// 将一个冻结保留原因转换为稳定 API 标记。
#[must_use]
const fn retention_reason_token(reason: RetentionReasonCode) -> &'static str {
    match reason {
        RetentionReasonCode::TerminalMemoryRecycle => "terminal_memory_recycle",
        RetentionReasonCode::ColdTurnRecycle => "cold_turn_recycle",
        RetentionReasonCode::IdleSessionRecycle => "idle_session_recycle",
        RetentionReasonCode::TrashExpiryPurge => "trash_expiry_purge",
        RetentionReasonCode::VectorGcRetry => "vector_gc_retry",
        RetentionReasonCode::ProfileExpiry => "profile_expiry",
        RetentionReasonCode::ManualSessionRecycle => "manual_session_recycle",
        RetentionReasonCode::ManualTurnRecycle => "manual_turn_recycle",
    }
}

/// Builds one empty recycle-bin batch with an audit-stable reason.
/// 使用审计稳定的原因构建一个空回收站批次。
fn retention_trash_batch(
    batch_id: String,
    subject_id: SubjectId,
    scope_id: ScopeId,
    recycled_at_ns: i64,
    expires_at_ns: i64,
    reason: RetentionReasonCode,
) -> StorageRetentionTrashBatch {
    StorageRetentionTrashBatch {
        batch_id,
        subject_id,
        scope_id,
        recycled_at_ns,
        expires_at_ns,
        reason,
        manual_selection: None,
        dataset: ExportDataset::empty(),
        analysis_attempts: Vec::new(),
    }
}

/// Persists a recycle batch containing domain records or an exact manual session selection.
/// 持久化含领域记录或准确人工会话选择的回收批次。
fn push_retention_batch(snapshot: &mut StorageSnapshot, batch: StorageRetentionTrashBatch) {
    if batch.dataset.turns.is_empty()
        && batch.dataset.memories.is_empty()
        && batch.dataset.profile_nodes.is_empty()
        && batch.dataset.context_evidence.is_empty()
        && batch.dataset.chat_compact_boundaries.is_empty()
        && batch.analysis_attempts.is_empty()
        && batch.manual_selection.is_none()
    {
        return;
    }
    snapshot.retention_trash_batches.push(batch);
}

/// Moves exact active memories and dependent context evidence into one recycle batch.
/// 将精确活动记忆及其依赖情境证据移入一个回收批次。
fn move_memories_to_retention_batch(
    snapshot: &mut StorageSnapshot,
    memory_ids: &BTreeSet<String>,
    batch: &mut StorageRetentionTrashBatch,
) {
    if memory_ids.is_empty() {
        return;
    }
    let mut retained_memories = Vec::with_capacity(snapshot.dataset.memories.len());
    for memory in std::mem::take(&mut snapshot.dataset.memories) {
        if memory_ids.contains(memory.id.as_str()) {
            batch.dataset.memories.push(memory);
        } else {
            retained_memories.push(memory);
        }
    }
    snapshot.dataset.memories = retained_memories;
    let moved_ids = batch
        .dataset
        .memories
        .iter()
        .map(|memory| memory.id.clone())
        .collect::<BTreeSet<_>>();
    let mut retained_evidence = Vec::with_capacity(snapshot.dataset.context_evidence.len());
    for evidence in std::mem::take(&mut snapshot.dataset.context_evidence) {
        if moved_ids.contains(&evidence.memory_id) {
            batch.dataset.context_evidence.push(evidence);
        } else {
            retained_evidence.push(evidence);
        }
    }
    snapshot.dataset.context_evidence = retained_evidence;
}

/// Moves exact active turns and their persisted analysis attempts into one recycle batch.
/// 将精确活动回合及其持久分析尝试移入一个回收批次。
fn move_turns_to_retention_batch(
    snapshot: &mut StorageSnapshot,
    turn_ids: &BTreeSet<TurnId>,
    batch: &mut StorageRetentionTrashBatch,
) {
    if turn_ids.is_empty() {
        return;
    }
    let mut retained_turns = Vec::with_capacity(snapshot.dataset.turns.len());
    for turn in std::mem::take(&mut snapshot.dataset.turns) {
        if turn_ids.contains(&turn.id) {
            batch.dataset.turns.push(turn);
        } else {
            retained_turns.push(turn);
        }
    }
    snapshot.dataset.turns = retained_turns;
    let mut retained_attempts = Vec::with_capacity(snapshot.analysis_attempts.len());
    for attempt in std::mem::take(&mut snapshot.analysis_attempts) {
        if turn_ids.contains(&attempt.turn_id) {
            batch.analysis_attempts.push(attempt);
        } else {
            retained_attempts.push(attempt);
        }
    }
    snapshot.analysis_attempts = retained_attempts;
}

/// Resolves old non-pending turns outside the hot window that no retained memory or profile references.
/// 解析热窗口外且未被任何保留记忆或画像引用的旧非待处理回合。
fn recyclable_turn_ids(
    snapshot: &StorageSnapshot,
    subject_id: &SubjectId,
    scope_id: &ScopeId,
    session_source_id: &SessionSourceId,
    hot_window_size: u32,
) -> BTreeSet<TurnId> {
    let mut turns = snapshot
        .dataset
        .turns
        .iter()
        .filter(|turn| {
            &turn.subject_id == subject_id
                && &turn.scope_id == scope_id
                && &turn.session_source_id == session_source_id
        })
        .collect::<Vec<_>>();
    turns.sort_by(|left, right| {
        left.created_at_ns
            .cmp(&right.created_at_ns)
            .then_with(|| left.id.cmp(&right.id))
    });
    let hot_start = turns.len().saturating_sub(hot_window_size as usize);
    let referenced_turns = referenced_turn_ids(snapshot);
    turns[..hot_start]
        .iter()
        .filter(|turn| turn.status != TurnStatus::Pending && !referenced_turns.contains(&turn.id))
        .map(|turn| turn.id.clone())
        .collect()
}

/// Collects every turn still referenced by any retained memory or profile node.
/// 收集仍被任意保留记忆或画像节点引用的全部回合。
fn referenced_turn_ids(snapshot: &StorageSnapshot) -> BTreeSet<TurnId> {
    let mut referenced = snapshot
        .dataset
        .memories
        .iter()
        .filter_map(|memory| memory.source_turn_id.clone())
        .collect::<BTreeSet<_>>();
    for node in &snapshot.dataset.profile_nodes {
        referenced.extend(node.source_turn_ids.iter().cloned());
    }
    referenced
}

/// Selects expired active session memories eligible for idle-session recycling.
/// 选择符合闲置会话回收条件的已过期活跃会话记忆。
fn idle_session_memory_ids(
    snapshot: &StorageSnapshot,
    subject_id: &SubjectId,
    scope_id: &ScopeId,
    session_source_id: &SessionSourceId,
    idle_cutoff_ns: i64,
) -> BTreeSet<String> {
    snapshot
        .dataset
        .memories
        .iter()
        .filter(|memory| {
            memory.scope_id == *scope_id
                && memory.is_visible_to_subject(subject_id)
                && memory.scope_class == MemoryScopeClass::Session
                && memory.state == MemoryState::Active
                && memory.origin_session_source_id.as_ref() == Some(session_source_id)
                && memory
                    .expires_at_ns
                    .is_some_and(|expires_at| expires_at <= idle_cutoff_ns)
                && memory_latest_activity_ns(memory) <= idle_cutoff_ns
        })
        .map(|memory| memory.id.as_str().to_owned())
        .collect()
}

/// Resolves the latest durable activity timestamp carried by one memory record.
/// 解析一条记忆记录携带的最近耐久活动时间。
#[must_use]
fn memory_latest_activity_ns(memory: &vulcan_memorymesh_core::MemoryRecord) -> i64 {
    [
        Some(memory.created_at_ns),
        Some(memory.updated_at_ns),
        memory.last_recalled_at_ns,
        memory.last_adopted_at_ns,
        memory.last_reinforced_at_ns,
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(memory.created_at_ns)
}

/// Resolves the latest durable turn, direct-memory creation, analysis, or compaction activity for one session.
/// 解析一个会话最近的耐久回合、主动记忆创建、分析或压缩活动时间。
///
/// # Parameters
/// `snapshot` is the exact staged storage state being validated.
/// `snapshot` 是正在校验的精确暂存存储状态。
/// `subject_id`, `scope_id`, and `session_source_id` identify one native session timeline.
/// `subject_id`、`scope_id` 与 `session_source_id` 标识一条原生会话时间线。
///
/// # Returns
/// The greatest observed timestamp, or `None` when no durable record anchors the session.
/// 返回最大的已观察时间戳；没有耐久记录锚定该会话时返回 `None`。
fn retention_session_latest_activity_ns(
    snapshot: &StorageSnapshot,
    subject_id: &SubjectId,
    scope_id: &ScopeId,
    session_source_id: &SessionSourceId,
) -> Option<i64> {
    let session_turn_ids = snapshot
        .dataset
        .turns
        .iter()
        .filter(|turn| {
            turn.subject_id == *subject_id
                && turn.scope_id == *scope_id
                && turn.session_source_id == *session_source_id
        })
        .map(|turn| turn.id.clone())
        .collect::<BTreeSet<_>>();
    snapshot
        .dataset
        .turns
        .iter()
        .filter(|turn| session_turn_ids.contains(&turn.id))
        .map(|turn| turn.created_at_ns)
        .chain(
            snapshot
                .dataset
                .chat_compact_boundaries
                .iter()
                .filter(|boundary| {
                    boundary.subject_id == *subject_id
                        && boundary.scope_id == *scope_id
                        && boundary.session_source_id == *session_source_id
                })
                .map(|boundary| boundary.compacted_at_ns),
        )
        .chain(
            snapshot
                .dataset
                .memories
                .iter()
                .filter(|memory| {
                    memory.is_visible_to_subject(subject_id)
                        && memory.scope_id == *scope_id
                        && memory.origin_session_source_id.as_ref() == Some(session_source_id)
                })
                // The external session row is established when a direct write resolves its session.
                // Memory creation is the native durable equivalent; later lifecycle activity is
                // independently checked by idle_session_memory_ids.
                // 外置会话行在主动写入解析会话时建立。记忆创建时间是原生耐久等价物；
                // 后续生命周期活动由 idle_session_memory_ids 独立校验。
                .map(|memory| memory.created_at_ns),
        )
        .chain(
            snapshot
                .analysis_attempts
                .iter()
                .filter(|attempt| session_turn_ids.contains(&attempt.turn_id))
                .map(|attempt| attempt.mutation_identity.observed_at_ns),
        )
        .chain(
            snapshot
                .memory_sessions
                .iter()
                .filter(|session| {
                    session.subject_id == *subject_id
                        && session.scope_id == *scope_id
                        && session.session_source_id == *session_source_id
                })
                .filter_map(|session| session.last_extraction_completed_at_ns),
        )
        .max()
}

/// Converts a millisecond retention duration into nanoseconds with overflow checking.
/// 在检查溢出的前提下将毫秒保留时长转换为纳秒。
fn retention_duration_ns(duration_ms: u64) -> Result<i64, String> {
    i64::try_from(u128::from(duration_ms).saturating_mul(1_000_000))
        .map_err(|error| format!("原生 VMM 保留时长超出纳秒范围：{error}"))
}

/// Projects vector-generation compatibility from one coherent Core snapshot.
/// 从一份一致 Core 快照投影向量代次兼容状态。
///
/// # Parameters
/// 参数
/// `snapshot` is the authoritative semantic state; `current_vector_space_id` and
/// `embedding_dimension` are present only after exact route resolution.
/// `snapshot` 是权威语义状态；仅在准确解析路由后才提供 `current_vector_space_id` 与
/// `embedding_dimension`。
/// `pending_analysis_job_count` is the independently durable queue count and `error` preserves an
/// exact route-resolution failure.
/// `pending_analysis_job_count` 是独立耐久队列计数，`error` 保留准确路由解析失败。
///
/// # Returns
/// 返回值
/// A complete vector status with no inferred provider fallback.
/// 返回不含推测性供应商兜底的完整向量状态。
fn vector_status_from_snapshot(
    snapshot: &StorageSnapshot,
    current_vector_space_id: Option<&VectorSpaceId>,
    embedding_dimension: Option<u32>,
    pending_analysis_job_count: u64,
    error: Option<String>,
) -> NativeVmmVectorStatus {
    let active_memories = snapshot
        .dataset
        .memories
        .iter()
        .filter(|memory| memory.state == MemoryState::Active)
        .collect::<Vec<_>>();
    let recycle_bin_memories = snapshot
        .retention_trash_batches
        .iter()
        .flat_map(|batch| batch.dataset.memories.iter())
        .collect::<Vec<_>>();
    let current_memory_count = current_vector_space_id.map_or(0, |current| {
        active_memories
            .iter()
            .filter(|memory| memory.vector_space_id.as_ref() == Some(current))
            .count()
    });
    let current_recycle_bin_memory_count = current_vector_space_id.map_or(0, |current| {
        recycle_bin_memories
            .iter()
            .filter(|memory| memory.vector_space_id.as_ref() == Some(current))
            .count()
    });
    let active_rebuild_required = current_vector_space_id.map_or(0, |_| {
        active_memories.len().saturating_sub(current_memory_count)
    });
    let recycle_bin_rebuild_required = current_vector_space_id.map_or(0, |_| {
        recycle_bin_memories
            .len()
            .saturating_sub(current_recycle_bin_memory_count)
    });
    let legacy_vector_spaces = current_vector_space_id.map_or_else(BTreeSet::new, |current| {
        active_memories
            .iter()
            .copied()
            .chain(recycle_bin_memories.iter().copied())
            .filter_map(|memory| memory.vector_space_id.as_ref())
            .filter(|space_id| *space_id != current)
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>()
    });
    NativeVmmVectorStatus {
        configured: current_vector_space_id.is_some(),
        current_vector_space_id: current_vector_space_id.map(ToString::to_string),
        embedding_dimension,
        active_memory_count: usize_to_u64(active_memories.len()),
        recycle_bin_memory_count: usize_to_u64(recycle_bin_memories.len()),
        current_memory_count: usize_to_u64(current_memory_count),
        current_recycle_bin_memory_count: usize_to_u64(current_recycle_bin_memory_count),
        rebuild_required_memory_count: usize_to_u64(
            active_rebuild_required.saturating_add(recycle_bin_rebuild_required),
        ),
        active_rebuild_required_memory_count: usize_to_u64(active_rebuild_required),
        recycle_bin_rebuild_required_memory_count: usize_to_u64(recycle_bin_rebuild_required),
        legacy_vector_space_count: usize_to_u64(legacy_vector_spaces.len()),
        pending_analysis_job_count,
        error,
    }
}

/// Groups every incompatible active memory by its exact owner scope in stable identity order.
/// 按准确所有者作用域及稳定身份顺序分组每条不兼容活动记忆。
///
/// # Parameters
/// 参数
/// `snapshot` is the coherent Core state and `target` is the current route-derived generation.
/// `snapshot` 是一致 Core 状态，`target` 是当前路由派生代次。
///
/// # Returns
/// 返回值
/// A deterministic scope-to-memory map suitable for 256-record Core batches.
/// 返回适用于 Core 每批 256 条限制的确定性作用域到记忆映射。
fn active_vector_rebuild_targets(
    snapshot: &StorageSnapshot,
    target: &VectorSpaceId,
) -> BTreeMap<ScopeId, Vec<MemoryId>> {
    let mut targets = BTreeMap::<ScopeId, Vec<MemoryId>>::new();
    for memory in snapshot.dataset.memories.iter().filter(|memory| {
        memory.state == MemoryState::Active && memory.vector_space_id.as_ref() != Some(target)
    }) {
        targets
            .entry(memory.scope_id.clone())
            .or_default()
            .push(memory.id.clone());
    }
    for memory_ids in targets.values_mut() {
        memory_ids.sort();
    }
    targets
}

/// Groups every incompatible recoverable memory by its exact owner scope in stable identity order.
/// 按准确所有者作用域及稳定身份顺序分组每条不兼容可恢复记忆。
///
/// # Parameters
/// 参数
/// `snapshot` is the coherent host snapshot and `target` is the route-derived generation.
/// `snapshot` 是一致宿主快照，`target` 是由路由派生的代次。
///
/// # Returns
/// 返回值
/// A deterministic scope-to-memory map suitable for bounded host maintenance batches.
/// 返回适用于有界宿主维护批次的确定性作用域到记忆映射。
fn recycle_bin_vector_rebuild_targets(
    snapshot: &StorageSnapshot,
    target: &VectorSpaceId,
) -> BTreeMap<ScopeId, Vec<MemoryId>> {
    let mut targets = BTreeMap::<ScopeId, Vec<MemoryId>>::new();
    for memory in snapshot
        .retention_trash_batches
        .iter()
        .flat_map(|batch| batch.dataset.memories.iter())
        .filter(|memory| memory.vector_space_id.as_ref() != Some(target))
    {
        targets
            .entry(memory.scope_id.clone())
            .or_default()
            .push(memory.id.clone());
    }
    for memory_ids in targets.values_mut() {
        memory_ids.sort();
    }
    targets
}

/// Loads exact recycle-bin summaries for one confirmed vector rebuild batch.
/// 为一个已确认向量重建批次加载准确的回收站记忆摘要。
///
/// # Parameters
/// 参数
/// `snapshot` is the immutable before-image; subject and scope establish exact ownership.
/// `snapshot` 是不可变前镜像；主体与作用域建立准确所有权。
/// `memory_ids` preserves caller order and `target` rejects already-migrated stale targets.
/// `memory_ids` 保留调用方顺序，`target` 用于拒绝已迁移的过期目标。
///
/// # Returns
/// 返回值
/// One non-empty durable summary for every requested identity in caller order.
/// 按调用方顺序为每个请求身份返回一个非空持久摘要。
///
/// # Errors
/// 错误
/// Returns when any identity is duplicated, missing, cross-scope, inaccessible, or already current.
/// 任一身份重复、缺失、跨作用域、不可访问或已经属于当前代次时返回错误。
fn recycle_bin_vector_summaries(
    snapshot: &StorageSnapshot,
    subject_id: &SubjectId,
    scope_id: &ScopeId,
    memory_ids: &[MemoryId],
    target: &VectorSpaceId,
) -> Result<Vec<String>, String> {
    let mut seen = BTreeSet::new();
    let mut summaries = Vec::with_capacity(memory_ids.len());
    for memory_id in memory_ids {
        if !seen.insert(memory_id) {
            return Err(format!(
                "原生 VMM 回收站向量重建批次重复引用记忆 `{memory_id}`。"
            ));
        }
        let matches = snapshot
            .retention_trash_batches
            .iter()
            .filter(|batch| batch.subject_id == *subject_id && batch.scope_id == *scope_id)
            .flat_map(|batch| batch.dataset.memories.iter())
            .filter(|memory| memory.id == *memory_id)
            .collect::<Vec<_>>();
        let [memory] = matches.as_slice() else {
            return Err(format!(
                "原生 VMM 回收站向量重建目标 `{memory_id}` 缺失或身份不唯一。"
            ));
        };
        if !memory.is_visible_to_subject(subject_id)
            || memory.vector_space_id.as_ref() == Some(target)
        {
            return Err(format!(
                "原生 VMM 回收站向量重建目标 `{memory_id}` 的所有权或向量代次已变化。"
            ));
        }
        let summary = memory.summary.trim();
        if summary.is_empty() {
            return Err(format!(
                "原生 VMM 回收站向量重建目标 `{memory_id}` 缺少持久摘要。"
            ));
        }
        summaries.push(summary.to_owned());
    }
    Ok(summaries)
}

/// Applies one exact recycle-bin vector replacement batch to a staged host snapshot.
/// 将一个准确的回收站向量替换批次应用到暂存宿主快照。
///
/// # Parameters
/// 参数
/// Ownership and identities bind the operation to one confirmed recycle-bin target set.
/// 所有权与身份把操作绑定到一个已确认的回收站目标集合。
/// `target_dimension` and `vectors` carry the freshly validated Host AI output.
/// `target_dimension` 与 `vectors` 携带刚完成校验的宿主 AI 输出。
///
/// # Returns
/// 返回值
/// The exact updated identities for durable idempotent replay.
/// 返回用于耐久幂等重放的准确已更新身份。
///
/// # Errors
/// 错误
/// Returns before mutation when targets or vectors differ from the confirmed batch.
/// 当目标或向量与已确认批次不一致时，在变更前返回错误。
fn apply_recycle_bin_vector_batch(
    snapshot: &mut StorageSnapshot,
    subject_id: &SubjectId,
    scope_id: &ScopeId,
    memory_ids: &[MemoryId],
    target: &VectorSpaceId,
    target_dimension: u32,
    vectors: Vec<Vec<f32>>,
) -> Result<NativeVmmRecycleVectorBatchResult, String> {
    let expected_dimension = usize::try_from(target_dimension)
        .map_err(|error| format!("原生 VMM 回收站向量维度超出范围：{error}"))?;
    if memory_ids.is_empty()
        || vectors.len() != memory_ids.len()
        || vectors.iter().any(|vector| {
            vector.len() != expected_dimension || vector.iter().any(|value| !value.is_finite())
        })
    {
        return Err(String::from(
            "原生 VMM 回收站向量批次的数量、维度或数值无效。",
        ));
    }
    let mut seen = BTreeSet::new();
    for memory_id in memory_ids {
        if !seen.insert(memory_id) {
            return Err(format!(
                "原生 VMM 回收站向量批次重复引用记忆 `{memory_id}`。"
            ));
        }
        let matches = snapshot
            .retention_trash_batches
            .iter()
            .filter(|batch| batch.subject_id == *subject_id && batch.scope_id == *scope_id)
            .flat_map(|batch| batch.dataset.memories.iter())
            .filter(|memory| memory.id == *memory_id)
            .count();
        if matches != 1 {
            return Err(format!(
                "原生 VMM 回收站向量目标 `{memory_id}` 缺失或身份不唯一。"
            ));
        }
    }
    for (memory_id, vector) in memory_ids.iter().zip(vectors) {
        let memory = snapshot
            .retention_trash_batches
            .iter_mut()
            .filter(|batch| batch.subject_id == *subject_id && batch.scope_id == *scope_id)
            .flat_map(|batch| batch.dataset.memories.iter_mut())
            .find(|memory| memory.id == *memory_id)
            .ok_or_else(|| format!("原生 VMM 回收站向量目标 `{memory_id}` 已变化。"))?;
        if !memory.is_visible_to_subject(subject_id)
            || memory.vector_space_id.as_ref() == Some(target)
        {
            return Err(format!(
                "原生 VMM 回收站向量目标 `{memory_id}` 的所有权或向量代次已变化。"
            ));
        }
        memory.vector_space_id = Some(target.clone());
        memory.embedding = Some(vector);
    }
    Ok(NativeVmmRecycleVectorBatchResult {
        rebuilt_memory_ids: memory_ids.iter().map(ToString::to_string).collect(),
    })
}

/// Builds the exact versioned confirmation object for one vector-generation migration.
/// 为一次向量代次迁移构建准确的版本化确认对象。
///
/// # Parameters
/// 参数
/// `snapshot` is the authoritative before-image and `descriptor` is the current exact target route.
/// `snapshot` 是权威前镜像，`descriptor` 是当前准确目标路由。
///
/// # Returns
/// 返回值
/// The deterministic counts and SHA-256 fingerprint displayed before execution.
/// 返回执行前显示的确定性计数与 SHA-256 指纹。
///
/// # Errors
/// 错误
/// Returns when a platform count cannot be represented by the public contract.
/// 当平台计数无法由公开契约表示时返回错误。
fn vector_rebuild_preview(
    snapshot: &StorageSnapshot,
    descriptor: &NativeVmmVectorSpaceDescriptor,
) -> Result<NativeVmmVectorRebuildPreview, String> {
    let active_targets = active_vector_rebuild_targets(snapshot, &descriptor.vector_space_id);
    let recycle_bin_targets =
        recycle_bin_vector_rebuild_targets(snapshot, &descriptor.vector_space_id);
    let active_memory_count = snapshot
        .dataset
        .memories
        .iter()
        .filter(|memory| memory.state == MemoryState::Active)
        .count();
    let recycle_bin_memory_count = snapshot
        .retention_trash_batches
        .iter()
        .map(|batch| batch.dataset.memories.len())
        .sum::<usize>();
    let active_rebuild_memory_count = active_targets.values().map(Vec::len).sum::<usize>();
    let recycle_bin_rebuild_memory_count =
        recycle_bin_targets.values().map(Vec::len).sum::<usize>();
    let rebuild_memory_count =
        active_rebuild_memory_count.saturating_add(recycle_bin_rebuild_memory_count);
    let batch_count = active_targets
        .values()
        .map(|memory_ids| memory_ids.len().div_ceil(256))
        .chain(
            recycle_bin_targets
                .values()
                .map(|memory_ids| memory_ids.len().div_ceil(256)),
        )
        .sum::<usize>();
    let affected_projects = active_targets
        .keys()
        .chain(recycle_bin_targets.keys())
        .collect::<BTreeSet<_>>();
    let mut hasher = Sha256::new();
    hasher.update(b"native-vmm-vector-rebuild-preview-v2\0");
    hasher.update(snapshot.storage_version.to_le_bytes());
    hasher.update(descriptor.vector_space_id.as_str().as_bytes());
    hasher.update([0]);
    hasher.update(descriptor.embedding_dimension.to_le_bytes());
    hasher.update(descriptor.capability_fingerprint.as_bytes());
    for (lane, targets) in [
        (b"active".as_slice(), &active_targets),
        (b"recycle-bin".as_slice(), &recycle_bin_targets),
    ] {
        hasher.update([0]);
        hasher.update(lane);
        for (scope_id, memory_ids) in targets {
            hasher.update([0]);
            hasher.update(scope_id.as_str().as_bytes());
            for memory_id in memory_ids {
                hasher.update([0]);
                hasher.update(memory_id.as_str().as_bytes());
            }
        }
    }
    Ok(NativeVmmVectorRebuildPreview {
        storage_version: snapshot.storage_version,
        target_vector_space_id: descriptor.vector_space_id.to_string(),
        embedding_dimension: descriptor.embedding_dimension,
        active_memory_count: usize_to_u64(active_memory_count),
        recycle_bin_memory_count: usize_to_u64(recycle_bin_memory_count),
        rebuild_memory_count: usize_to_u64(rebuild_memory_count),
        active_rebuild_memory_count: usize_to_u64(active_rebuild_memory_count),
        recycle_bin_rebuild_memory_count: usize_to_u64(recycle_bin_rebuild_memory_count),
        affected_project_count: usize_to_u64(affected_projects.len()),
        batch_count: usize_to_u64(batch_count),
        preview_fingerprint: format!("sha256:{}", hex::encode(hasher.finalize())),
    })
}

/// Derives a stable per-batch operation identity from the user command and exact target set.
/// 根据用户命令与准确目标集合派生稳定的逐批操作身份。
///
/// # Parameters
/// 参数
/// `operation_id` is the user command UUID; `lane`, `scope_id`, and `memory_ids` identify one exact batch.
/// `operation_id` 是用户命令 UUID；`lane`、`scope_id` 与 `memory_ids` 标识一批准确目标。
///
/// # Returns
/// 返回值
/// A bounded collision-resistant identity safe for Core and the durable mutation journal.
/// 返回适用于 Core 与耐久变更账本的有界抗碰撞身份。
///
/// # Errors
/// 错误
/// Returns when the batch is empty because Core forbids empty rebuild commands.
/// 当批次为空时返回错误，因为 Core 禁止空重建命令。
fn vector_rebuild_batch_operation_id(
    operation_id: &str,
    lane: &str,
    scope_id: &ScopeId,
    memory_ids: &[MemoryId],
) -> Result<String, String> {
    if memory_ids.is_empty() {
        return Err(String::from("原生 VMM 向量重建批次不能为空。"));
    }
    let mut hasher = Sha256::new();
    hasher.update(b"native-vmm-vector-rebuild-batch-v2\0");
    hasher.update(lane.as_bytes());
    hasher.update([0]);
    hasher.update(scope_id.as_str().as_bytes());
    for memory_id in memory_ids {
        hasher.update([0]);
        hasher.update(memory_id.as_str().as_bytes());
    }
    Ok(format!(
        "native-vmm-vector-rebuild:{operation_id}:{}",
        hex::encode(hasher.finalize())
    ))
}

/// Returns the exact fixed native VMM subject identity.
/// 返回精确的原生 VMM 固定主体身份。
fn native_vmm_subject_id() -> Result<SubjectId, String> {
    SubjectId::new(format!("vmm-subject:{NATIVE_VMM_USER_ID}"))
        .map_err(|error| format!("原生 VMM 固定主体身份无效：{error}"))
}

/// Returns the exact fixed native VMM personal-profile scope.
/// 返回精确的原生 VMM 固定个人画像作用域。
fn native_vmm_subject_scope() -> Result<ScopeId, String> {
    ScopeId::new(format!("vmm-subject:{NATIVE_VMM_USER_ID}"))
        .map_err(|error| format!("原生 VMM 固定个人画像作用域无效：{error}"))
}

/// Reads the current Unix time in nanoseconds without truncation.
/// 读取当前 Unix 纳秒时间且不截断。
fn native_vmm_now_ns() -> Result<i64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("系统时间早于 Unix 纪元：{error}"))?;
    i64::try_from(duration.as_nanos())
        .map_err(|error| format!("系统纳秒时间超出 i64 范围：{error}"))
}

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

    let mut turns = project_datasets(snapshot, source_scope)
        .flat_map(|dataset| dataset.turns.iter())
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

    let mut memories = project_datasets(snapshot, source_scope)
        .flat_map(|dataset| dataset.memories.iter())
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

    let mut profiles = project_datasets(snapshot, source_scope)
        .flat_map(|dataset| dataset.profile_nodes.iter())
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
// The migration fingerprint covers source, destination, before/after counts and exact rename records.
// 迁移指纹覆盖来源、目标、前后计数及精确重命名记录。
#[expect(
    clippy::too_many_arguments,
    reason = "The migration fingerprint covers source, destination, before/after counts and exact rename records."
)]
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
    apply_migration_dataset(
        &mut snapshot.dataset,
        source_scope,
        target_scope,
        &rename_map,
    );
    // Move the authoritative observation window with its session, never merge it by display name.
    // 将权威观察窗口随会话移动，绝不按显示名称合并。
    for session in &mut snapshot.memory_sessions {
        if &session.scope_id == source_scope {
            session.scope_id = target_scope.clone();
            rename_session(&mut session.session_source_id, &rename_map);
        }
    }
    for batch in &mut snapshot.retention_trash_batches {
        if &batch.scope_id == source_scope {
            batch.scope_id = target_scope.clone();
            apply_migration_dataset(&mut batch.dataset, source_scope, target_scope, &rename_map);
            if let Some(vulcan_memorymesh_core::StorageManualRecycleSelection::Sessions {
                sessions,
            }) = &mut batch.manual_selection
            {
                for session in sessions {
                    session.scope_id = target_scope.clone();
                    rename_session(&mut session.session_source_id, &rename_map);
                }
            }
        }
    }
    for job in &mut snapshot.retention_cold_turn_jobs {
        if &job.scope_id == source_scope {
            job.scope_id = target_scope.clone();
            rename_session(&mut job.session_source_id, &rename_map);
        }
    }
    for entry in &mut snapshot.noise_prototypes {
        if &entry.identity.scope_id == source_scope {
            entry.identity.scope_id = target_scope.clone();
        }
    }
    Ok(())
}

/// Moves every scope-owned record in one active or recycled dataset.
/// 迁移一个活动或已回收数据集中的全部作用域归属记录。
fn apply_migration_dataset(
    dataset: &mut ExportDataset,
    source_scope: &ScopeId,
    target_scope: &ScopeId,
    rename_map: &BTreeMap<String, SessionSourceId>,
) {
    for turn in &mut dataset.turns {
        if &turn.scope_id == source_scope {
            turn.scope_id = target_scope.clone();
            rename_session(&mut turn.session_source_id, rename_map);
        }
    }
    for memory in &mut dataset.memories {
        if &memory.scope_id == source_scope {
            memory.scope_id = target_scope.clone();
            if let Some(session) = &mut memory.origin_session_source_id {
                rename_session(session, rename_map);
            }
        }
    }
    for node in &mut dataset.profile_nodes {
        if &node.scope_id == source_scope {
            node.scope_id = target_scope.clone();
        }
    }
    for boundary in &mut dataset.chat_compact_boundaries {
        if &boundary.scope_id == source_scope {
            boundary.scope_id = target_scope.clone();
            rename_session(&mut boundary.session_source_id, rename_map);
        }
    }
}

/// Clears project records and their dependent evidence and recovery metadata.
/// 清空项目记录及其依赖的证据与恢复元数据。
fn clear_project(
    snapshot: &mut StorageSnapshot,
    project_id: u64,
) -> Result<NativeVmmProjectContentCounts, String> {
    let scope = project_scope(project_id)?;
    let removed = project_content_counts(snapshot, &scope);
    // Clearing a project covers active and recycled content. Capture every durable identity before
    // dropping recycle batches so idempotency receipts cannot outlive the records they reference.
    // 清空项目覆盖活动区与回收站内容；先收集全部耐久身份，避免幂等收据存活于其引用记录之后。
    let removed_domain_ids = project_domain_ids(snapshot, &scope);
    let removed_turn_ids = project_datasets(snapshot, &scope)
        .flat_map(|dataset| dataset.turns.iter())
        .filter(|turn| turn.scope_id == scope)
        .map(|turn| turn.id.clone())
        .collect::<BTreeSet<_>>();
    let removed_memory_ids = project_datasets(snapshot, &scope)
        .flat_map(|dataset| dataset.memories.iter())
        .filter(|memory| memory.scope_id == scope)
        .map(|memory| memory.id.clone())
        .collect::<BTreeSet<_>>();

    snapshot.dataset.turns.retain(|turn| turn.scope_id != scope);
    snapshot
        .memory_sessions
        .retain(|session| session.scope_id != scope);
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
        .retention_trash_batches
        .retain(|batch| batch.scope_id != scope);
    snapshot
        .retention_cold_turn_jobs
        .retain(|job| job.scope_id != scope);
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
    let memory_ids = project_datasets(snapshot, scope)
        .flat_map(|dataset| dataset.memories.iter())
        .filter(|memory| &memory.scope_id == scope)
        .map(|memory| memory.id.clone())
        .collect::<BTreeSet<_>>();
    NativeVmmProjectContentCounts {
        sessions: usize_to_u64(sessions.len()),
        turns: usize_to_u64(
            project_datasets(snapshot, scope)
                .flat_map(|dataset| dataset.turns.iter())
                .filter(|turn| &turn.scope_id == scope)
                .count(),
        ),
        memories: usize_to_u64(memory_ids.len()),
        profile_nodes: usize_to_u64(
            project_datasets(snapshot, scope)
                .flat_map(|dataset| dataset.profile_nodes.iter())
                .filter(|node| &node.scope_id == scope)
                .count(),
        ),
        context_evidence: usize_to_u64(
            project_datasets(snapshot, scope)
                .flat_map(|dataset| dataset.context_evidence.iter())
                .filter(|evidence| memory_ids.contains(&evidence.memory_id))
                .count(),
        ),
        compact_boundaries: usize_to_u64(
            project_datasets(snapshot, scope)
                .flat_map(|dataset| dataset.chat_compact_boundaries.iter())
                .filter(|boundary| &boundary.scope_id == scope)
                .count(),
        ),
        vector_rows: usize_to_u64(
            project_datasets(snapshot, scope)
                .flat_map(|dataset| dataset.memories.iter())
                .filter(|memory| {
                    &memory.scope_id == scope
                        && memory.vector_space_id.is_some()
                        && memory.embedding.is_some()
                })
                .count(),
        ),
    }
}

/// Iterates the active dataset and every recycle dataset owned by one project scope.
/// 遍历一个项目作用域拥有的活动数据集及全部回收数据集。
fn project_datasets<'a>(
    snapshot: &'a StorageSnapshot,
    scope: &'a ScopeId,
) -> impl Iterator<Item = &'a ExportDataset> {
    std::iter::once(&snapshot.dataset).chain(
        snapshot
            .retention_trash_batches
            .iter()
            .filter(move |batch| &batch.scope_id == scope)
            .map(|batch| &batch.dataset),
    )
}

/// Collects every receipt-addressable domain identity contained in one dataset.
/// 收集一个数据集中所有可由收据寻址的领域身份。
fn dataset_domain_ids(dataset: &ExportDataset) -> BTreeSet<String> {
    let memory_ids = dataset
        .memories
        .iter()
        .map(|memory| memory.id.clone())
        .collect::<BTreeSet<_>>();
    dataset
        .turns
        .iter()
        .map(|turn| turn.id.as_str().to_owned())
        .chain(
            memory_ids
                .iter()
                .map(|memory_id| memory_id.as_str().to_owned()),
        )
        .chain(
            dataset
                .profile_nodes
                .iter()
                .map(|node| node.id.as_str().to_owned()),
        )
        .chain(
            dataset
                .context_evidence
                .iter()
                .filter(|evidence| memory_ids.contains(&evidence.memory_id))
                .map(|evidence| {
                    context_evidence_domain_id(
                        &evidence.memory_id,
                        &evidence.context_key,
                        &evidence.context_value,
                    )
                }),
        )
        .collect()
}

/// Collects receipt-addressable identities for one exact scope inside a shared dataset.
/// 收集共享数据集中一个准确作用域的可由收据寻址身份。
fn dataset_domain_ids_for_scope(dataset: &ExportDataset, scope: &ScopeId) -> BTreeSet<String> {
    let memory_ids = dataset
        .memories
        .iter()
        .filter(|memory| &memory.scope_id == scope)
        .map(|memory| memory.id.clone())
        .collect::<BTreeSet<_>>();
    dataset
        .turns
        .iter()
        .filter(|turn| &turn.scope_id == scope)
        .map(|turn| turn.id.as_str().to_owned())
        .chain(
            memory_ids
                .iter()
                .map(|memory_id| memory_id.as_str().to_owned()),
        )
        .chain(
            dataset
                .profile_nodes
                .iter()
                .filter(|node| &node.scope_id == scope)
                .map(|node| node.id.as_str().to_owned()),
        )
        .chain(
            dataset
                .context_evidence
                .iter()
                .filter(|evidence| memory_ids.contains(&evidence.memory_id))
                .map(|evidence| {
                    context_evidence_domain_id(
                        &evidence.memory_id,
                        &evidence.context_key,
                        &evidence.context_value,
                    )
                }),
        )
        .collect()
}

/// Collects every receipt-addressable domain identity owned by one project, including trash.
/// 收集一个项目拥有的全部可由收据寻址的领域身份，包括回收站内容。
fn project_domain_ids(snapshot: &StorageSnapshot, scope: &ScopeId) -> BTreeSet<String> {
    project_datasets(snapshot, scope)
        .flat_map(|dataset| dataset_domain_ids_for_scope(dataset, scope))
        .collect()
}

/// Removes idempotency outcomes that reference records being permanently purged.
/// 删除引用即将永久清除记录的幂等结果。
fn remove_outcomes_referencing_dataset(snapshot: &mut StorageSnapshot, dataset: &ExportDataset) {
    let removed_domain_ids = dataset_domain_ids(dataset);
    snapshot
        .outcomes
        .retain(|entry| !mutation_outcome_references_any(&entry.outcome, &removed_domain_ids));
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
    /// Number of profile nodes sourced from turns in the session.
    /// 来源于该会话回合的画像节点数量。
    profile_node_count: u64,
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

/// Projects one durable profile node into the shared management record shape.
/// 将一条持久画像节点投影为共享管理记录结构。
///
/// # Parameters
/// 参数
/// `node` is the authoritative Core profile node, `target` is its exact user/project class,
/// and `evaluated_at_ns` is the read boundary used to materialize date-driven lifecycle state.
/// `node` 是权威 Core 画像节点，`target` 是其准确的用户/项目类别，
/// `evaluated_at_ns` 是用于物化日期驱动生命周期状态的读取边界。
///
/// # Returns
/// 返回值
/// A complete profile record with stable numeric identities, or an explicit identity error.
/// 返回包含稳定数字身份的完整画像记录，或明确的身份错误。
fn native_profile_record(
    node: &ProfileNode,
    target: &str,
    evaluated_at_ns: i64,
) -> Result<NativeVmmProfileRecordSummary, String> {
    let effective_state = profile_effective_state_token(node, evaluated_at_ns);
    Ok(NativeVmmProfileRecordSummary {
        profile_node_id: native_numeric_id(node.id.as_str(), "vmm-profile-", "画像节点")?,
        target: target.to_owned(),
        stored_state: profile_state_token(node.state).to_owned(),
        effective_state: effective_state.to_owned(),
        active_for_injection: effective_state == "active",
        content: node.content.clone(),
        priority: node.priority,
        level: profile_level_token(node.profile_level).to_owned(),
        source_kind: profile_source_kind_token(node.source_kind).to_owned(),
        source_id: node
            .source_id
            .as_ref()
            .map(|source_id| source_id.as_str().to_owned()),
        source_turn_ids: node
            .source_turn_ids
            .iter()
            .map(|turn_id| native_numeric_id(turn_id.as_str(), "vmm-turn-", "画像来源回合"))
            .collect::<Result<Vec<_>, _>>()?,
        effective_date_at_unix_ms: node.effective_date_ns.map(unix_ns_to_ms),
        expires_at_unix_ms: node.expires_at_ns.map(unix_ns_to_ms),
        status_reason: node.status_reason.clone(),
        updated_at_unix_ms: unix_ns_to_ms(node.updated_at_ns),
    })
}

/// Builds one all-state profile projection without changing active runtime eligibility.
/// 构建一份全状态画像投影，且不改变活动运行时资格。
fn build_profile_records(
    snapshot: &StorageSnapshot,
    project_id: u64,
    target: NativeVmmProfileTarget,
    evaluated_at_ns: i64,
) -> Result<NativeVmmProfileRecords, String> {
    let fixed_subject = native_vmm_subject_id()?;
    let (scope, target_token) = match target {
        NativeVmmProfileTarget::User => (native_vmm_subject_scope()?, "user"),
        NativeVmmProfileTarget::Project => (project_scope(project_id)?, "project"),
    };
    let mut counts = NativeVmmProfileStateCounts::default();
    let mut nodes = snapshot
        .dataset
        .profile_nodes
        .iter()
        .filter(|node| {
            node.scope_id == scope
                && (target != NativeVmmProfileTarget::User
                    || node.subject_id.as_ref() == Some(&fixed_subject))
        })
        .map(|node| {
            let record = native_profile_record(node, target_token, evaluated_at_ns)?;
            increment_profile_state(&mut counts, &record.effective_state);
            Ok(record)
        })
        .collect::<Result<Vec<_>, String>>()?;
    nodes.sort_by(|left, right| {
        profile_effective_state_rank(&left.effective_state)
            .cmp(&profile_effective_state_rank(&right.effective_state))
            .then_with(|| right.updated_at_unix_ms.cmp(&left.updated_at_unix_ms))
            .then_with(|| right.profile_node_id.cmp(&left.profile_node_id))
    });
    Ok(NativeVmmProfileRecords {
        storage_version: snapshot.storage_version,
        project_id,
        target: target_token.to_owned(),
        counts,
        nodes,
    })
}

/// Returns the persisted lifecycle token for one native profile node.
/// 返回一个原生画像节点的持久生命周期标记。
fn profile_state_token(state: ProfileNodeState) -> &'static str {
    match state {
        ProfileNodeState::Active => "active",
        ProfileNodeState::Inactive => "inactive",
        ProfileNodeState::Pending => "pending",
        ProfileNodeState::Expired => "expired",
    }
}

/// Applies effective and expiration dates to one persisted profile state.
/// 将生效日期与过期日期应用到一个持久画像状态。
fn profile_effective_state_token(
    node: &vulcan_memorymesh_core::ProfileNode,
    evaluated_at_ns: i64,
) -> &'static str {
    if node.state != ProfileNodeState::Active {
        return profile_state_token(node.state);
    }
    if node
        .effective_date_ns
        .is_some_and(|effective_at_ns| effective_at_ns > evaluated_at_ns)
    {
        return "scheduled";
    }
    if node
        .expires_at_ns
        .is_some_and(|expires_at_ns| expires_at_ns <= evaluated_at_ns)
    {
        return "expired";
    }
    "active"
}

/// Increments one effective native profile-state bucket.
/// 递增一个有效原生画像状态统计桶。
fn increment_profile_state(counts: &mut NativeVmmProfileStateCounts, state: &str) {
    let target = match state {
        "active" => &mut counts.active,
        "inactive" => &mut counts.inactive,
        "pending" => &mut counts.pending,
        "expired" => &mut counts.expired,
        "scheduled" => &mut counts.scheduled,
        _ => unreachable!("profile state tokens are closed by Core enums"),
    };
    *target = target.saturating_add(1);
}

/// Orders actionable profile states before historical states in the management view.
/// 在管理视图中将可操作画像状态排在历史状态之前。
fn profile_effective_state_rank(state: &str) -> u8 {
    match state {
        "pending" => 0,
        "active" => 1,
        "scheduled" => 2,
        "inactive" => 3,
        "expired" => 4,
        _ => u8::MAX,
    }
}

/// Returns the stable lifecycle-level token for one profile node.
/// 返回一个画像节点的稳定生命周期等级标记。
fn profile_level_token(level: ProfileLevel) -> &'static str {
    match level {
        ProfileLevel::Transient => "transient",
        ProfileLevel::Situational => "situational",
        ProfileLevel::Stable => "stable",
        ProfileLevel::Persistent => "persistent",
    }
}

/// Returns the stable durable source-kind token for one profile node.
/// 返回一个画像节点的稳定持久来源类别标记。
fn profile_source_kind_token(source_kind: ProfileSourceKind) -> &'static str {
    match source_kind {
        ProfileSourceKind::TurnExtract => "turn_extract",
        ProfileSourceKind::ManualInstruction => "manual_instruction",
        ProfileSourceKind::SystemSeed => "system_seed",
        ProfileSourceKind::RetainedAfterSubjectDelete => "retained_after_subject_delete",
    }
}

/// Builds lightweight project-scoped session summaries without projecting record bodies.
/// 构建轻量项目级会话摘要，不投影记录正文。
///
/// # Parameters
/// 参数
/// `snapshot` is the coherent Core storage snapshot and `project_id` selects its exact owner scope.
/// `snapshot` 是一致的 Core 存储快照，`project_id` 选择其准确归属作用域。
///
/// # Returns
/// 返回值
/// A stable provenance projection or one explicit identity-integrity error.
/// 返回稳定的来源投影，或一个明确的身份完整性错误。
fn build_session_summaries(
    snapshot: &StorageSnapshot,
    project_id: u64,
) -> Result<Vec<NativeVmmMemorySessionSummary>, String> {
    let scope = project_scope(project_id)?;
    let subject_scope = native_vmm_subject_scope()?;
    let fixed_subject = native_vmm_subject_id()?;
    let project_session_ids = project_sessions(snapshot, &scope);
    let project_turn_sessions = snapshot
        .dataset
        .turns
        .iter()
        .filter(|turn| turn.scope_id == scope)
        .map(|turn| {
            (
                turn.id.as_str().to_owned(),
                turn.session_source_id.as_str().to_owned(),
            )
        })
        .collect::<BTreeMap<_, _>>();
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
    for profile in snapshot.dataset.profile_nodes.iter().filter(|profile| {
        profile.scope_id == scope
            || (profile.scope_id == subject_scope
                && profile.subject_id.as_ref() == Some(&fixed_subject))
    }) {
        let source_sessions = profile
            .source_turn_ids
            .iter()
            .filter_map(|turn_id| project_turn_sessions.get(turn_id.as_str()))
            .collect::<BTreeSet<_>>();
        for session_source_id in source_sessions {
            let accumulator = accumulators
                .entry(session_source_id.as_str().to_owned())
                .or_default();
            accumulator.profile_node_count = accumulator.profile_node_count.saturating_add(1);
            observe_session_activity(accumulator, unix_ns_to_ms(profile.updated_at_ns));
        }
    }
    // A successful extraction remains session activity even when its source body has been recycled.
    // 即使来源正文已回收，成功提炼仍属于会话活动。
    // Read only the fixed native subject's authoritative commit time; never infer success from a watermark.
    // 仅读取固定原生主体的权威提交时间，不根据观察水位推断成功。
    for session in snapshot
        .memory_sessions
        .iter()
        .filter(|session| session.scope_id == scope && session.subject_id == fixed_subject)
    {
        if let Some(completed_at_ns) = session.last_extraction_completed_at_ns {
            let accumulator = accumulators
                .entry(session.session_source_id.as_str().to_owned())
                .or_default();
            observe_session_activity(accumulator, unix_ns_to_ms(completed_at_ns));
        }
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

    // Index exact persisted owners once; content-only rows remain explicitly unregistered.
    // 对准确持久化归属建立一次索引；仅含内容的记录保持明确未登记。
    let session_statuses = snapshot
        .memory_sessions
        .iter()
        .filter(|session| session.scope_id == scope && session.subject_id == fixed_subject)
        .map(|session| (session.session_source_id.as_str(), session.status))
        .collect::<BTreeMap<_, _>>();
    let mut sessions = accumulators
        .into_iter()
        .map(
            |(session_source_id, accumulator)| NativeVmmMemorySessionSummary {
                status: session_statuses.get(session_source_id.as_str()).copied(),
                session_source_id,
                turn_count: accumulator.turn_count,
                turn_statuses: accumulator.turn_statuses,
                memory_count: accumulator.memory_count,
                profile_node_count: accumulator.profile_node_count,
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

    Ok(sessions)
}

/// Builds authoritative project/session provenance using the shared lightweight session summary projection.
/// 使用共享轻量会话摘要投影构造权威项目/会话来源。
/// Takes a coherent snapshot, project, optional exact session and persisted analysis jobs; returns records or an integrity error.
/// 接收一致快照、项目、可选准确会话及持久分析作业；返回记录或完整性错误。
fn build_memory_records(
    snapshot: &StorageSnapshot,
    project_id: u64,
    session_source_id: Option<&str>,
    analysis_jobs: &[NativeVmmPendingAnalysisJob],
) -> Result<NativeVmmMemoryRecords, String> {
    let scope = project_scope(project_id)?;
    let subject_scope = native_vmm_subject_scope()?;
    let fixed_subject = native_vmm_subject_id()?;
    let evaluated_at_ns = native_vmm_now_ns()?;
    let project_session_ids = project_sessions(snapshot, &scope);
    let sessions = build_session_summaries(snapshot, project_id)?;

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

    let selected_turn_ids = snapshot
        .dataset
        .turns
        .iter()
        .filter(|turn| {
            turn.scope_id == scope
                && selected_session_source_id
                    .as_ref()
                    .is_none_or(|session| turn.session_source_id == *session)
        })
        .map(|turn| turn.id.as_str().to_owned())
        .collect::<BTreeSet<_>>();

    let mut analysis_jobs_by_turn = BTreeMap::new();
    for job in analysis_jobs {
        let Some(turn_id) = job.turn_id.as_ref() else {
            continue;
        };
        if analysis_jobs_by_turn
            .insert(turn_id.as_str(), job)
            .is_some()
        {
            return Err(format!(
                "原生 VMM 回合 `{turn_id}` 关联了多条分析审计作业。"
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

    let selected_profile_nodes = snapshot
        .dataset
        .profile_nodes
        .iter()
        .filter(|profile| {
            (profile.scope_id == scope
                || (profile.scope_id == subject_scope
                    && profile.subject_id.as_ref() == Some(&fixed_subject)))
                && profile
                    .source_turn_ids
                    .iter()
                    .any(|turn_id| selected_turn_ids.contains(turn_id.as_str()))
        })
        .collect::<Vec<_>>();
    let mut profile_ids_by_turn = BTreeMap::<String, Vec<u64>>::new();
    for profile in &selected_profile_nodes {
        let profile_node_id = native_numeric_id(profile.id.as_str(), "vmm-profile-", "画像节点")?;
        for turn_id in &profile.source_turn_ids {
            if selected_turn_ids.contains(turn_id.as_str()) {
                profile_ids_by_turn
                    .entry(turn_id.as_str().to_owned())
                    .or_default()
                    .push(profile_node_id);
            }
        }
    }
    for profile_ids in profile_ids_by_turn.values_mut() {
        profile_ids.sort_unstable();
        profile_ids.dedup();
    }

    // requested_turns preserves the selected active anchors for the shared native detail resolver.
    // requested_turns 保留选中活跃锚点，供共享原生详情解析器使用。
    let requested_turns = selected_turn_ids
        .iter()
        .map(|id| TurnId::new(id.clone()).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    // detail_turns uses the same window rule as the AI endpoint and the same snapshot as counts.
    // detail_turns 与 AI 入口使用同一窗口规则，并与统计使用同一快照。
    let detail_turns = resolve_turn_details(
        &snapshot.dataset.turns,
        &fixed_subject,
        &ScopeAccess {
            write_scope: scope.clone(),
            visible_scopes: vec![scope.clone()],
        },
        &requested_turns,
    )
    .map_err(|error| error.to_string())?;
    let mut turns = detail_turns
        .iter()
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
            let generated_profile_node_ids = profile_ids_by_turn
                .get(turn.id.as_str())
                .cloned()
                .unwrap_or_default();
            let mut memory_levels = NativeVmmMemoryLevelCounts::default();
            for memory in generated {
                increment_memory_level(&mut memory_levels, memory.memory_level);
            }
            let analysis_job = analysis_jobs_by_turn.get(turn.id.as_str()).copied();
            Ok(NativeVmmTurnRecordSummary {
                turn_id: native_numeric_id(turn.id.as_str(), "vmm-turn-", "回合")?,
                previous_turn_ids: turn
                    .previous_turn_ids
                    .iter()
                    .map(|id| native_numeric_id(id.as_str(), "vmm-turn-", "前序回合"))
                    .collect::<Result<Vec<_>, _>>()?,
                next_turn_ids: turn
                    .next_turn_ids
                    .iter()
                    .map(|id| native_numeric_id(id.as_str(), "vmm-turn-", "后续回合"))
                    .collect::<Result<Vec<_>, _>>()?,
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
                generated_profile_node_ids,
                memory_levels,
                analysis_job_state: analysis_job.map(|job| job.state.clone()),
                analysis_attempt_count: analysis_job.map_or(0, |job| job.attempt_count),
                analysis_last_error: analysis_job.and_then(|job| job.last_error.clone()),
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

    let mut profile_nodes = selected_profile_nodes
        .into_iter()
        .map(|profile| {
            let target = if profile.scope_id == scope {
                "project"
            } else {
                "user"
            };
            native_profile_record(profile, target, evaluated_at_ns)
        })
        .collect::<Result<Vec<_>, String>>()?;
    profile_nodes.sort_by(|left, right| {
        profile_effective_state_rank(&left.effective_state)
            .cmp(&profile_effective_state_rank(&right.effective_state))
            .then_with(|| right.updated_at_unix_ms.cmp(&left.updated_at_unix_ms))
            .then_with(|| right.profile_node_id.cmp(&left.profile_node_id))
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
        profile_nodes,
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
    // Sessions remain present after their old turn bodies are pruned or recycled.
    // 在旧回合正文被清理或回收后，会话仍然存在。
    sessions.extend(
        snapshot
            .memory_sessions
            .iter()
            .filter(|session| &session.scope_id == scope)
            .map(|session| session.session_source_id.as_str().to_owned()),
    );
    for dataset in project_datasets(snapshot, scope) {
        for turn in &dataset.turns {
            if &turn.scope_id == scope {
                sessions.insert(turn.session_source_id.as_str().to_owned());
            }
        }
        for memory in &dataset.memories {
            if &memory.scope_id == scope
                && let Some(session) = &memory.origin_session_source_id
            {
                sessions.insert(session.as_str().to_owned());
            }
        }
        for boundary in &dataset.chat_compact_boundaries {
            if &boundary.scope_id == scope {
                sessions.insert(boundary.session_source_id.as_str().to_owned());
            }
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
    sessions.extend(
        snapshot
            .memory_sessions
            .iter()
            .map(|session| session.session_source_id.as_str().to_owned()),
    );
    for dataset in std::iter::once(&snapshot.dataset).chain(
        snapshot
            .retention_trash_batches
            .iter()
            .map(|batch| &batch.dataset),
    ) {
        for turn in &dataset.turns {
            sessions.insert(turn.session_source_id.as_str().to_owned());
        }
        for memory in &dataset.memories {
            if let Some(session) = &memory.origin_session_source_id {
                sessions.insert(session.as_str().to_owned());
            }
        }
        for boundary in &dataset.chat_compact_boundaries {
            sessions.insert(boundary.session_source_id.as_str().to_owned());
        }
    }
    for job in &snapshot.retention_cold_turn_jobs {
        sessions.insert(job.session_source_id.as_str().to_owned());
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

/// Parses the exact one-level native project id from a persisted Core scope.
/// 从持久 Core 作用域解析准确的一级原生项目 ID。
fn native_project_id_from_scope(scope_id: &ScopeId) -> Result<u64, String> {
    let value = scope_id.as_str();
    let numeric = value.strip_prefix("vmm-project:").ok_or_else(|| {
        format!("原生 VMM 回收批次作用域 `{value}` 不符合 `vmm-project:<id>` 契约。")
    })?;
    let project_id = numeric
        .parse::<u64>()
        .map_err(|error| format!("原生 VMM 回收批次作用域 `{value}` 不包含有效项目 ID：{error}"))?;
    if project_id == 0 {
        return Err(format!(
            "原生 VMM 回收批次作用域 `{value}` 包含保留项目 ID 0。"
        ));
    }
    Ok(project_id)
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
        ChatCompactBoundary, CommitReceipt, EntityBinding, EntityKind, EntitySlotId, ExportDataset,
        MemoryAccessClass, MemoryContextEvidence, MemoryId, MemoryRecord, MemoryScopeClass,
        MemoryStability, ProfileLevel, ProfileNode, ProfileNodeId, ProfileNodeState,
        ProfileSourceId, ProfileSourceKind, SubjectId, TimelineItem, Turn, TurnId, TurnStatus,
        VectorSpaceId, VersionedEntity,
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
            memory_sessions: Vec::new(),
            noise_prototypes: Vec::new(),
            retention_trash_batches: Vec::new(),
            retention_cold_turn_jobs: Vec::new(),
        }
    }

    /// Builds one committed idempotency entry that references an exact memory identity.
    /// 构建一条引用准确记忆身份的已提交幂等记录。
    fn push_memory_ledger_entry(snapshot: &mut StorageSnapshot, memory_id: &str) {
        let operation_id = OperationId::new(format!("ledger-{memory_id}")).expect("operation id");
        let idempotency_key =
            IdempotencyKey::new(format!("ledger-key-{memory_id}")).expect("idempotency key");
        let request_fingerprint = format!("fp256:{}", "1".repeat(64));
        let outcome = MutationOutcome::Committed {
            storage_version: 1,
            chat_compact: None,
            receipt: CommitReceipt {
                operation_id: operation_id.clone(),
                idempotency_key: idempotency_key.clone(),
                request_fingerprint: request_fingerprint.clone(),
                entity_bindings: vec![EntityBinding {
                    entity_slot_id: EntitySlotId::new("memory-slot").expect("entity slot"),
                    entity_kind: EntityKind::Memory,
                    domain_id: memory_id.to_owned(),
                }],
                committed_versions: vec![VersionedEntity {
                    entity_kind: EntityKind::Memory,
                    domain_id: memory_id.to_owned(),
                    version: 1,
                }],
                side_effect_summary: String::from("memory committed"),
            },
        };
        let entry = serde_json::from_value(serde_json::json!({
            "operation_id": operation_id,
            "request_fingerprint": request_fingerprint,
            "outcome": outcome,
            "idempotency_key": idempotency_key,
        }))
        .expect("storage ledger entry");
        snapshot.outcomes.push(entry);
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

    /// Verifies vector status distinguishes current, legacy, and missing generations without counting recycled rows.
    /// 验证向量状态可区分当前、旧版与缺失代次，且不会计入已回收记录。
    #[test]
    fn vector_status_reports_every_incompatible_active_memory() {
        let current = VectorSpaceId::new("current-space").expect("current vector space");
        let legacy = VectorSpaceId::new("legacy-space").expect("legacy vector space");
        let mut snapshot = empty_snapshot(7);
        let mut current_memory = memory(
            "current-memory",
            1,
            "session-a",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Stable,
        );
        current_memory.vector_space_id = Some(current.clone());
        current_memory.embedding = Some(vec![1.0, 0.0]);
        let mut legacy_memory = memory(
            "legacy-memory",
            1,
            "session-a",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Stable,
        );
        legacy_memory.vector_space_id = Some(legacy);
        legacy_memory.embedding = Some(vec![0.0, 1.0]);
        let missing_memory = memory(
            "missing-memory",
            2,
            "session-b",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Stable,
        );
        let mut recycled_memory = memory(
            "recycled-memory",
            2,
            "session-b",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Stable,
        );
        recycled_memory.state = MemoryState::Recycled;
        snapshot.dataset.memories = vec![
            current_memory,
            legacy_memory,
            missing_memory,
            recycled_memory,
        ];
        let mut recycle_batch = retention_trash_batch(
            String::from("vector-status-trash"),
            native_vmm_subject_id().expect("subject"),
            project_scope(2).expect("scope"),
            1,
            100,
            RetentionReasonCode::TerminalMemoryRecycle,
        );
        let mut current_recycled = memory(
            "current-recycled-memory",
            2,
            "session-b",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Stable,
        );
        current_recycled.vector_space_id = Some(current.clone());
        current_recycled.embedding = Some(vec![1.0, 0.0]);
        recycle_batch.dataset.memories.push(current_recycled);
        recycle_batch.dataset.memories.push(memory(
            "missing-recycled-memory",
            2,
            "session-b",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Stable,
        ));
        snapshot.retention_trash_batches.push(recycle_batch);

        let status = vector_status_from_snapshot(&snapshot, Some(&current), Some(2), 3, None);

        assert!(status.configured);
        assert_eq!(status.active_memory_count, 3);
        assert_eq!(status.recycle_bin_memory_count, 2);
        assert_eq!(status.current_memory_count, 1);
        assert_eq!(status.current_recycle_bin_memory_count, 1);
        assert_eq!(status.active_rebuild_required_memory_count, 2);
        assert_eq!(status.recycle_bin_rebuild_required_memory_count, 1);
        assert_eq!(status.rebuild_required_memory_count, 3);
        assert_eq!(status.legacy_vector_space_count, 1);
        assert_eq!(status.pending_analysis_job_count, 3);
    }

    /// Verifies an unavailable route does not mislabel every stored vector as an old generation.
    /// 验证路由不可用时不会把每条已存向量误标为旧代次。
    #[test]
    fn vector_status_keeps_route_failure_distinct_from_generation_mismatch() {
        let mut snapshot = empty_snapshot(8);
        snapshot.dataset.memories.push(memory(
            "unknown-generation-memory",
            1,
            "session-a",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Stable,
        ));

        let status = vector_status_from_snapshot(
            &snapshot,
            None,
            None,
            0,
            Some(String::from("embedding route unavailable")),
        );

        assert!(!status.configured);
        assert_eq!(status.active_memory_count, 1);
        assert_eq!(status.rebuild_required_memory_count, 0);
        assert_eq!(status.legacy_vector_space_count, 0);
        assert_eq!(status.error.as_deref(), Some("embedding route unavailable"));
    }

    /// Verifies rebuild previews are deterministic, project-aware, and split only at Core's 256-record limit.
    /// 验证重建预检具有确定性、感知项目，并且只按 Core 的 256 条上限切批。
    #[test]
    fn vector_rebuild_preview_is_deterministic_and_bounded() {
        let target = VectorSpaceId::new("target-space").expect("target vector space");
        let descriptor = NativeVmmVectorSpaceDescriptor {
            vector_space_id: target.clone(),
            embedding_dimension: 1_536,
            capability_fingerprint: String::from("capability-fingerprint"),
        };
        let mut snapshot = empty_snapshot(21);
        for index in 0..257 {
            snapshot.dataset.memories.push(memory(
                &format!("project-one-memory-{index:03}"),
                1,
                "session-a",
                None,
                MemorySourceKind::Direct,
                MemoryLevel::Stable,
            ));
        }
        snapshot.dataset.memories.push(memory(
            "project-two-memory",
            2,
            "session-b",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Stable,
        ));
        let mut current_memory = memory(
            "already-current",
            2,
            "session-b",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Stable,
        );
        current_memory.vector_space_id = Some(target);
        current_memory.embedding = Some(vec![1.0; 1_536]);
        snapshot.dataset.memories.push(current_memory);
        let mut recycle_batch = retention_trash_batch(
            String::from("vector-preview-trash"),
            native_vmm_subject_id().expect("subject"),
            project_scope(1).expect("scope"),
            1,
            100,
            RetentionReasonCode::TerminalMemoryRecycle,
        );
        recycle_batch.dataset.memories.push(memory(
            "recycle-bin-old-generation",
            1,
            "session-a",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Stable,
        ));
        snapshot.retention_trash_batches.push(recycle_batch);

        let first = vector_rebuild_preview(&snapshot, &descriptor).expect("first preview");
        snapshot.dataset.memories.reverse();
        let reordered = vector_rebuild_preview(&snapshot, &descriptor).expect("reordered preview");

        assert_eq!(first, reordered);
        assert_eq!(first.active_memory_count, 259);
        assert_eq!(first.recycle_bin_memory_count, 1);
        assert_eq!(first.active_rebuild_memory_count, 258);
        assert_eq!(first.recycle_bin_rebuild_memory_count, 1);
        assert_eq!(first.rebuild_memory_count, 259);
        assert_eq!(first.affected_project_count, 2);
        assert_eq!(first.batch_count, 4);
        assert!(first.preview_fingerprint.starts_with("sha256:"));
    }

    /// Verifies recycle-bin vector migration changes only the retained vector generation.
    /// 验证回收站向量迁移只修改所保留的向量代次。
    #[test]
    fn recycle_bin_vector_batch_preserves_recoverable_record_placement() {
        let subject = native_vmm_subject_id().expect("subject");
        let scope = project_scope(2).expect("scope");
        let target = VectorSpaceId::new("current-space").expect("vector space");
        let memory_id = MemoryId::new("recycled-vector-memory").expect("memory id");
        let mut snapshot = empty_snapshot(9);
        let mut batch = retention_trash_batch(
            String::from("vector-rebuild-trash"),
            subject.clone(),
            scope.clone(),
            1,
            100,
            RetentionReasonCode::TerminalMemoryRecycle,
        );
        let retained = memory(
            memory_id.as_str(),
            2,
            "session-a",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Stable,
        );
        let original_state = retained.state;
        let original_updated_at_ns = retained.updated_at_ns;
        batch.dataset.memories.push(retained);
        snapshot.retention_trash_batches.push(batch);

        let summaries = recycle_bin_vector_summaries(
            &snapshot,
            &subject,
            &scope,
            std::slice::from_ref(&memory_id),
            &target,
        )
        .expect("summaries");
        let result = apply_recycle_bin_vector_batch(
            &mut snapshot,
            &subject,
            &scope,
            std::slice::from_ref(&memory_id),
            &target,
            2,
            vec![vec![0.25, 0.75]],
        )
        .expect("rebuild recycle vector");

        assert_eq!(summaries.len(), 1);
        assert!(snapshot.dataset.memories.is_empty());
        assert_eq!(result.rebuilt_memory_ids, vec![memory_id.to_string()]);
        let retained = &snapshot.retention_trash_batches[0].dataset.memories[0];
        assert_eq!(retained.vector_space_id.as_ref(), Some(&target));
        assert_eq!(retained.embedding.as_deref(), Some([0.25, 0.75].as_slice()));
        assert_eq!(retained.state, original_state);
        assert_eq!(retained.updated_at_ns, original_updated_at_ns);
    }

    /// Verifies audit cleanup preserves hot, recoverable, active, and no-turn idempotency rows.
    /// 验证审计清理会保留活动区、可恢复、活动态以及无回合的幂等记录。
    #[test]
    fn orphaned_terminal_analysis_cleanup_follows_permanent_turn_removal() {
        let mut snapshot = empty_snapshot(12);
        snapshot.dataset.turns.push(turn("vmm-turn-hot", 2, "hot"));
        let mut recycle_batch = retention_trash_batch(
            String::from("audit-trash"),
            native_vmm_subject_id().expect("subject"),
            project_scope(2).expect("scope"),
            1,
            100,
            RetentionReasonCode::ColdTurnRecycle,
        );
        recycle_batch
            .dataset
            .turns
            .push(turn("vmm-turn-recoverable", 2, "cold"));
        snapshot.retention_trash_batches.push(recycle_batch);
        let audit_job =
            |operation_id: &str, turn_id: Option<&str>, state: &str| NativeVmmPendingAnalysisJob {
                operation_id: operation_id.to_owned(),
                request_fingerprint: format!("fp256:{}", "1".repeat(64)),
                payload_json: String::new(),
                intake_mutation_operation_id: format!("{operation_id}:intake"),
                turn_id: turn_id.map(str::to_owned),
                state: state.to_owned(),
                attempt_count: 0,
                next_attempt_at_unix_ms: 0,
                last_error: None,
            };
        let jobs = vec![
            audit_job(
                "hot-terminal",
                Some("vmm-turn-hot"),
                VMM_PENDING_ANALYSIS_COMPLETED,
            ),
            audit_job(
                "recoverable-terminal",
                Some("vmm-turn-recoverable"),
                VMM_PENDING_ANALYSIS_COMPLETED,
            ),
            audit_job(
                "orphan-completed",
                Some("vmm-turn-gone-a"),
                VMM_PENDING_ANALYSIS_COMPLETED,
            ),
            audit_job(
                "orphan-bypassed",
                Some("vmm-turn-gone-b"),
                VMM_PENDING_ANALYSIS_BYPASSED,
            ),
            audit_job(
                "active-ready",
                Some("vmm-turn-gone-c"),
                vulcan_workspace_store::vmm_data_plane::VMM_PENDING_ANALYSIS_READY,
            ),
            audit_job(
                "terminal-without-turn",
                None,
                VMM_PENDING_ANALYSIS_COMPLETED,
            ),
        ];

        assert_eq!(
            orphaned_terminal_analysis_operation_ids(&snapshot, &jobs),
            vec![
                String::from("orphan-completed"),
                String::from("orphan-bypassed")
            ]
        );
    }

    /// Verifies activity derives from successful extraction commits without inventing timestamps or crossing projects.
    /// 验证活动取自成功提炼提交，不伪造时间或跨越项目。
    #[test]
    fn memory_records_include_authoritative_extraction_activity() {
        // Include one normal, one bodyless and one never-completed session, plus another project's newer commit.
        // 包含普通、正文已移除和尚未成功的三个会话，以及其他项目更新的提交。
        let mut snapshot = empty_snapshot(12);
        let mut source_turn = turn("vmm-turn-11", 2, "session-with-turn");
        source_turn.created_at_ns = 1_000_000;
        snapshot.dataset.turns.push(source_turn);
        for (project_id, session_id, completed_at_ns) in [
            (2, "session-with-turn", Some(8_000_000)),
            (2, "session-bodyless", Some(6_000_000)),
            (2, "session-pending", None),
            (3, "session-with-turn", Some(100_000_000)),
        ] {
            snapshot
                .memory_sessions
                .push(vulcan_memorymesh_core::StorageMemorySession {
                    subject_id: native_vmm_subject_id().expect("fixed subject"),
                    status: vulcan_memorymesh_core::StorageSessionStatus::Active,
                    scope_id: project_scope(project_id).expect("scope"),
                    session_source_id: SessionSourceId::new(session_id).expect("session"),
                    last_extraction_observed_at_ns: 0,
                    last_extraction_completed_at_ns: completed_at_ns,
                });
        }
        let before = serde_json::to_value(&snapshot).expect("before read");
        let records = build_memory_records(&snapshot, 2, None, &[]).expect("records");
        assert_eq!(records.sessions.len(), 3);
        assert_eq!(records.sessions[0].session_source_id, "session-with-turn");
        assert_eq!(records.sessions[0].first_activity_at_unix_ms, Some(1));
        assert_eq!(records.sessions[0].last_activity_at_unix_ms, Some(8));
        assert_eq!(records.sessions[1].session_source_id, "session-bodyless");
        assert_eq!(records.sessions[1].turn_count, 0);
        assert_eq!(records.sessions[1].memory_count, 0);
        assert_eq!(records.sessions[1].first_activity_at_unix_ms, Some(6));
        assert_eq!(records.sessions[1].last_activity_at_unix_ms, Some(6));
        assert_eq!(records.sessions[2].session_source_id, "session-pending");
        assert_eq!(records.sessions[2].first_activity_at_unix_ms, None);
        assert_eq!(records.sessions[2].last_activity_at_unix_ms, None);
        assert_eq!(serde_json::to_value(&snapshot).expect("after read"), before);
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
        let mut user_profile = profile_node(
            "vmm-profile-31",
            ScopeId::new("vmm-subject:1").expect("subject scope"),
            "likes apples",
        );
        user_profile.source_kind = ProfileSourceKind::TurnExtract;
        user_profile.source_id = Some(ProfileSourceId::new("vmm-turn-11").expect("source id"));
        user_profile.source_turn_ids = vec![TurnId::new("vmm-turn-11").expect("turn id")];
        let mut project_profile = profile_node(
            "vmm-profile-32",
            project_scope(2).expect("project scope"),
            "project uses apples",
        );
        project_profile.source_kind = ProfileSourceKind::TurnExtract;
        project_profile.source_id = Some(ProfileSourceId::new("vmm-turn-11").expect("source id"));
        project_profile.source_turn_ids = vec![TurnId::new("vmm-turn-11").expect("turn id")];
        let mut unrelated_profile = profile_node(
            "vmm-profile-99",
            project_scope(3).expect("foreign project scope"),
            "foreign profile",
        );
        unrelated_profile.source_kind = ProfileSourceKind::TurnExtract;
        unrelated_profile.source_id = Some(ProfileSourceId::new("vmm-turn-99").expect("source id"));
        unrelated_profile.source_turn_ids = vec![TurnId::new("vmm-turn-99").expect("turn id")];
        snapshot.dataset.profile_nodes = vec![user_profile, project_profile, unrelated_profile];
        let jobs = vec![
            NativeVmmPendingAnalysisJob {
                operation_id: String::from("operation-ready"),
                request_fingerprint: String::from("fingerprint-ready"),
                payload_json: String::from("{}"),
                intake_mutation_operation_id: String::from("intake-ready"),
                turn_id: Some(String::from("vmm-turn-12")),
                state: String::from("ready"),
                attempt_count: 2,
                next_attempt_at_unix_ms: 8,
                last_error: Some(String::from("provider timeout")),
            },
            NativeVmmPendingAnalysisJob {
                operation_id: String::from("operation-completed"),
                request_fingerprint: String::from("fingerprint-completed"),
                payload_json: String::new(),
                intake_mutation_operation_id: String::from("intake-completed"),
                turn_id: Some(String::from("vmm-turn-11")),
                state: String::from("completed"),
                attempt_count: 3,
                next_attempt_at_unix_ms: 0,
                last_error: None,
            },
        ];

        let records = build_memory_records(&snapshot, 2, Some("session-a"), &jobs)
            .expect("native memory records");

        assert_eq!(records.storage_version, 12);
        assert_eq!(records.sessions.len(), 1);
        assert_eq!(records.sessions[0].turn_count, 2);
        assert_eq!(records.sessions[0].turn_statuses.completed, 1);
        assert_eq!(records.sessions[0].turn_statuses.pending, 1);
        assert_eq!(records.sessions[0].memory_count, 2);
        assert_eq!(records.sessions[0].profile_node_count, 2);
        assert_eq!(records.sessions[0].direct_memory_count, 1);
        assert_eq!(records.sessions[0].memory_levels.phase, 1);
        assert_eq!(records.sessions[0].memory_levels.stable, 1);
        assert_eq!(records.turns.len(), 2);
        assert_eq!(records.turns[0].turn_id, 12);
        assert_eq!(records.turns[0].previous_turn_ids, vec![11]);
        assert!(records.turns[0].next_turn_ids.is_empty());
        assert_eq!(records.turns[0].extracted, Some(false));
        assert_eq!(records.turns[0].analysis_attempt_count, 2);
        assert_eq!(
            records.turns[0].analysis_last_error.as_deref(),
            Some("provider timeout")
        );
        assert_eq!(records.turns[1].turn_id, 11);
        assert!(records.turns[1].previous_turn_ids.is_empty());
        assert_eq!(records.turns[1].next_turn_ids, vec![12]);
        assert_eq!(records.turns[1].extracted, Some(true));
        assert_eq!(
            records.turns[1].analysis_job_state.as_deref(),
            Some("completed")
        );
        assert_eq!(records.turns[1].analysis_attempt_count, 3);
        assert_eq!(records.turns[1].generated_memory_ids, vec![21]);
        assert_eq!(records.turns[1].generated_profile_node_ids, vec![31, 32]);
        assert_eq!(records.turns[1].timeline.len(), 2);
        assert_eq!(records.memories.len(), 2);
        assert_eq!(records.memories[0].memory_id, 22);
        assert_eq!(records.memories[0].source_kind, "direct");
        assert_eq!(records.memories[1].source_turn_id, Some(11));
        assert_eq!(records.profile_nodes.len(), 2);
        assert_eq!(records.profile_nodes[0].target, "project");
        assert_eq!(records.profile_nodes[1].target, "user");
    }

    /// Verifies native profile inspection exposes pending and date-derived states without activating them.
    /// 验证原生画像检查会暴露待处理及日期推导状态，且不会错误激活这些节点。
    #[test]
    fn profile_records_expose_every_effective_lifecycle_state() {
        let mut snapshot = empty_snapshot(14);
        let subject_scope = native_vmm_subject_scope().expect("subject scope");
        let mut pending = profile_node("vmm-profile-11", subject_scope.clone(), "pending user");
        pending.state = ProfileNodeState::Pending;
        pending.updated_at_ns = 8_000_000;
        let mut elapsed = profile_node("vmm-profile-12", subject_scope.clone(), "expired user");
        elapsed.expires_at_ns = Some(5_000_000);
        elapsed.updated_at_ns = 7_000_000;
        let mut scheduled = profile_node("vmm-profile-13", subject_scope.clone(), "future user");
        scheduled.effective_date_ns = Some(20_000_000);
        scheduled.updated_at_ns = 6_000_000;
        let mut foreign_subject = profile_node("vmm-profile-14", subject_scope, "foreign user");
        foreign_subject.subject_id = Some(SubjectId::new("vmm-subject:2").expect("subject id"));
        let project = profile_node(
            "vmm-profile-21",
            project_scope(2).expect("project scope"),
            "active project",
        );
        snapshot.dataset.profile_nodes =
            vec![pending, elapsed, scheduled, foreign_subject, project];

        let user_records =
            build_profile_records(&snapshot, 2, NativeVmmProfileTarget::User, 10_000_000)
                .expect("user profile records");
        assert_eq!(user_records.storage_version, 14);
        assert_eq!(user_records.target, "user");
        assert_eq!(user_records.nodes.len(), 3);
        assert_eq!(user_records.counts.pending, 1);
        assert_eq!(user_records.counts.expired, 1);
        assert_eq!(user_records.counts.scheduled, 1);
        assert_eq!(user_records.nodes[0].effective_state, "pending");
        assert_eq!(user_records.nodes[1].effective_state, "scheduled");
        assert_eq!(user_records.nodes[2].stored_state, "active");
        assert_eq!(user_records.nodes[2].effective_state, "expired");
        assert!(
            user_records
                .nodes
                .iter()
                .all(|node| !node.active_for_injection)
        );

        let project_records =
            build_profile_records(&snapshot, 2, NativeVmmProfileTarget::Project, 10_000_000)
                .expect("project profile records");
        assert_eq!(project_records.target, "project");
        assert_eq!(project_records.counts.active, 1);
        assert_eq!(project_records.nodes.len(), 1);
        assert!(project_records.nodes[0].active_for_injection);
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
            project_scope(2).expect("source project scope")
        );

        let mut changed = snapshot.clone();
        changed.dataset.turns[0].analysis_details = Some(String::from("changed detail"));
        let changed_preview = preview_migration(&changed, 2, 3).expect("changed preview");
        assert_ne!(
            preview.preview_fingerprint,
            changed_preview.preview_fingerprint
        );
    }

    /// Verifies body-free session records participate in migration preview, renaming and clear.
    /// 验证没有正文的会话记录参与迁移预检、改名和清空。
    #[test]
    fn migration_moves_session_watermarks_and_clear_resets_only_the_owner_project() {
        // snapshot keeps independent windows, even though source and target reuse the same session ID.
        // snapshot 保留独立窗口，即使来源和目标复用了同一会话 ID。
        let mut snapshot = empty_snapshot(7);
        for (project_id, id, observed, completed) in [
            (2, "session-a", 20, 25),
            (3, "session-a", 40, 45),
            (4, "session-a-1", 60, 65),
        ] {
            snapshot
                .memory_sessions
                .push(vulcan_memorymesh_core::StorageMemorySession {
                    subject_id: SubjectId::new("vmm-subject:1").expect("subject"),
                    status: vulcan_memorymesh_core::StorageSessionStatus::Active,
                    scope_id: project_scope(project_id).expect("scope"),
                    session_source_id: SessionSourceId::new(id).expect("session"),
                    last_extraction_observed_at_ns: observed,
                    last_extraction_completed_at_ns: Some(completed),
                });
        }
        // preview counts metadata-only sessions and reserves IDs owned by every other project.
        // preview 统计仅有元数据的会话，并保留所有其他项目占用的 ID。
        let preview = preview_migration(&snapshot, 2, 3).expect("preview");
        assert_eq!(preview.source.sessions, 1);
        assert_eq!(preview.target_before.sessions, 1);
        assert_eq!(preview.target_after.sessions, 2);
        assert_eq!(preview.session_renames[0].to, "session-a-2");
        apply_migration(&mut snapshot, &preview).expect("move");
        assert!(project_sessions(&snapshot, &project_scope(2).expect("source")).is_empty());
        // moved retains its own successful boundary instead of inheriting the target's boundary.
        // moved 保留自己的成功边界，而不是继承目标的边界。
        let moved = snapshot
            .memory_sessions
            .iter()
            .find(|session| session.session_source_id.as_str() == "session-a-2")
            .expect("moved session");
        assert_eq!(moved.scope_id, project_scope(3).expect("target"));
        assert_eq!(moved.last_extraction_observed_at_ns, 20);
        assert_eq!(moved.last_extraction_completed_at_ns, Some(25));
        assert_eq!(
            retention_session_latest_activity_ns(
                &snapshot,
                &moved.subject_id,
                &moved.scope_id,
                &moved.session_source_id
            ),
            Some(25)
        );
        assert!(vulcan_memorymesh_core::InMemoryStorage::from_snapshot(snapshot.clone()).is_ok());
        clear_project(&mut snapshot, 3).expect("clear target");
        assert_eq!(snapshot.memory_sessions.len(), 1);
        assert_eq!(
            snapshot.memory_sessions[0].scope_id,
            project_scope(4).expect("unrelated project")
        );
        assert_eq!(
            snapshot.memory_sessions[0].last_extraction_observed_at_ns,
            60
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
                project_scope(2).expect("source project scope"),
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
            scope_id: project_scope(2).expect("source project scope"),
            session_source_id: SessionSourceId::new("session-a").expect("session id"),
            compacted_turn_id: TurnId::new("source-turn").expect("turn id"),
            compacted_at_ns: 2,
        }];
        let preview = preview_migration(&snapshot, 2, 3).expect("migration preview");

        apply_migration(&mut snapshot, &preview).expect("apply migration");

        assert_eq!(
            snapshot.dataset.turns[0].scope_id,
            project_scope(3).expect("target project scope")
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
            project_scope(3).expect("target project scope")
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
            project_scope(3).expect("target project scope")
        );
        assert_eq!(
            snapshot.dataset.profile_nodes[1].scope_id,
            ScopeId::new("vmm-subject:1").expect("subject scope")
        );
        assert_eq!(
            snapshot.dataset.chat_compact_boundaries[0].scope_id,
            project_scope(3).expect("target project scope")
        );
        assert_eq!(
            snapshot.dataset.chat_compact_boundaries[0]
                .session_source_id
                .as_str(),
            "session-a-1"
        );
        assert_eq!(
            project_content_counts(&snapshot, &project_scope(2).expect("source project scope")),
            NativeVmmProjectContentCounts::default()
        );
    }

    /// Verifies migration preview and conflict renaming include durable recycle-bin sessions.
    /// 验证迁移预检与冲突改名覆盖耐久回收站中的会话。
    #[test]
    fn migration_preview_includes_recycled_sessions_and_records() {
        let mut snapshot = empty_snapshot(4);
        let source_scope = project_scope(2).expect("source scope");
        let target_scope = project_scope(3).expect("target scope");
        snapshot.retention_trash_batches = vec![
            StorageRetentionTrashBatch {
                batch_id: String::from("source-trash"),
                subject_id: native_vmm_subject_id().expect("subject"),
                scope_id: source_scope.clone(),
                recycled_at_ns: 1,
                expires_at_ns: 100,
                reason: RetentionReasonCode::ColdTurnRecycle,
                manual_selection: None,
                dataset: ExportDataset {
                    turns: vec![turn("source-trash-turn", 2, "recycled-session")],
                    ..ExportDataset::empty()
                },
                analysis_attempts: Vec::new(),
            },
            StorageRetentionTrashBatch {
                batch_id: String::from("target-trash"),
                subject_id: native_vmm_subject_id().expect("subject"),
                scope_id: target_scope.clone(),
                recycled_at_ns: 1,
                expires_at_ns: 100,
                reason: RetentionReasonCode::ColdTurnRecycle,
                manual_selection: None,
                dataset: ExportDataset {
                    turns: vec![turn("target-trash-turn", 3, "recycled-session")],
                    ..ExportDataset::empty()
                },
                analysis_attempts: Vec::new(),
            },
        ];

        let preview = preview_migration(&snapshot, 2, 3).expect("migration preview");

        assert_eq!(preview.source.sessions, 1);
        assert_eq!(preview.source.turns, 1);
        assert_eq!(preview.target_before.sessions, 1);
        assert_eq!(preview.target_after.sessions, 2);
        assert_eq!(preview.records.len(), 2);
        assert_eq!(
            preview.session_renames,
            vec![NativeVmmSessionRename {
                from: String::from("recycled-session"),
                to: String::from("recycled-session-1"),
            }]
        );

        apply_migration(&mut snapshot, &preview).expect("apply migration");
        assert_eq!(snapshot.retention_trash_batches[0].scope_id, target_scope);
        assert_eq!(
            snapshot.retention_trash_batches[0].dataset.turns[0]
                .session_source_id
                .as_str(),
            "recycled-session-1"
        );
        assert_eq!(
            project_content_counts(&snapshot, &source_scope),
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
                project_scope(2).expect("source project scope"),
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
            scope_id: project_scope(2).expect("source project scope"),
            session_source_id: SessionSourceId::new("session-a").expect("session id"),
            compacted_turn_id: TurnId::new("source-turn").expect("turn id"),
            compacted_at_ns: 2,
        }];
        let mut recycled_memory = memory(
            "vmm-memory-recycled",
            2,
            "session-recycled",
            Some("recycled-turn"),
            MemorySourceKind::Turn,
            MemoryLevel::Stable,
        );
        recycled_memory.vector_space_id =
            Some(VectorSpaceId::new("vmm-default").expect("vector space"));
        recycled_memory.embedding = Some(vec![0.0, 1.0]);
        snapshot.retention_trash_batches = vec![StorageRetentionTrashBatch {
            batch_id: String::from("project-trash"),
            subject_id: native_vmm_subject_id().expect("subject"),
            scope_id: project_scope(2).expect("project scope"),
            recycled_at_ns: 1,
            expires_at_ns: 100,
            reason: RetentionReasonCode::IdleSessionRecycle,
            manual_selection: None,
            dataset: ExportDataset {
                turns: vec![turn("recycled-turn", 2, "session-recycled")],
                memories: vec![recycled_memory],
                profile_nodes: vec![profile_node(
                    "vmm-profile-recycled",
                    project_scope(2).expect("project scope"),
                    "recycled project preference",
                )],
                context_evidence: vec![MemoryContextEvidence {
                    memory_id: MemoryId::new("vmm-memory-recycled").expect("memory id"),
                    context_key: String::from("workspace"),
                    context_value: String::from("recycled"),
                    support_count: 1,
                    rebuttal_count: 0,
                    last_supported_at_ns: Some(1),
                    last_rebutted_at_ns: None,
                    version: 1,
                }],
                chat_compact_boundaries: vec![ChatCompactBoundary {
                    subject_id: SubjectId::new("vmm-subject:1").expect("subject id"),
                    scope_id: project_scope(2).expect("project scope"),
                    session_source_id: SessionSourceId::new("session-recycled")
                        .expect("session id"),
                    compacted_turn_id: TurnId::new("recycled-turn").expect("turn id"),
                    compacted_at_ns: 1,
                }],
            },
            analysis_attempts: Vec::new(),
        }];
        push_memory_ledger_entry(&mut snapshot, "vmm-memory-recycled");

        let removed = clear_project(&mut snapshot, 2).expect("clear project");

        assert_eq!(removed.sessions, 2);
        assert_eq!(removed.turns, 2);
        assert_eq!(removed.memories, 2);
        assert_eq!(removed.profile_nodes, 2);
        assert_eq!(removed.context_evidence, 2);
        assert_eq!(removed.compact_boundaries, 2);
        assert_eq!(removed.vector_rows, 2);
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
        assert!(snapshot.retention_trash_batches.is_empty());
        assert!(snapshot.outcomes.is_empty());
        vulcan_memorymesh_core::InMemoryStorage::from_snapshot(snapshot)
            .expect("cleared snapshot remains valid");
    }

    /// Builds one minimal retention plan for direct snapshot-transform tests.
    /// 为直接快照转换测试构建一份最小保留计划。
    fn retention_plan(project_id: u64) -> RetentionPlanSet {
        RetentionPlanSet {
            identity: MutationIdentity {
                operation_id: OperationId::new("retention-test-operation").expect("operation id"),
                idempotency_key: IdempotencyKey::new("retention-test-key")
                    .expect("idempotency key"),
                request_fingerprint: format!("fp256:{}", "0".repeat(64)),
                source_kind: MutationSourceKind::BackgroundWorker,
                request_source_id: String::from("retention-test"),
                observed_at_ns: 1,
            },
            subject_id: native_vmm_subject_id().expect("subject"),
            scope_id: project_scope(project_id).expect("project scope"),
            profile_expiry: Vec::new(),
            terminal_memory_recycle: Vec::new(),
            cold_turn_enqueue: Vec::new(),
            cold_turn_execute: Vec::new(),
            idle_session_recycle: Vec::new(),
            vector_gc_retry: Vec::new(),
            trash_purge: Vec::new(),
        }
    }

    /// Verifies terminal retention atomically preserves memory evidence in durable trash.
    /// 验证终态保留会把记忆及证据原子保存在耐久回收站中。
    #[test]
    fn retention_recycles_terminal_memory_with_context_evidence() {
        // A non-default window must become part of the new durable batch, not a display-time guess.
        // 非默认窗口必须成为新耐久批次的一部分，不能在展示时推算。
        let mut policy = RetentionPolicy::v1();
        policy.trash_retention_ms = 12;
        let mut snapshot = empty_snapshot(5);
        let mut deleted = memory(
            "vmm-memory-terminal",
            2,
            "session-a",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Phase,
        );
        deleted.state = MemoryState::Deleted;
        deleted.embedding = Some(vec![0.2, 0.8]);
        deleted.vector_space_id = Some(VectorSpaceId::new("vmm-default").expect("vector space"));
        snapshot.dataset.context_evidence = vec![MemoryContextEvidence {
            memory_id: deleted.id.clone(),
            context_key: String::from("workspace"),
            context_value: String::from("native"),
            support_count: 1,
            rebuttal_count: 0,
            last_supported_at_ns: Some(1),
            last_rebutted_at_ns: None,
            version: 1,
        }];
        let expected_access = MemoryAccessIdentity::from(&deleted);
        snapshot.dataset.memories.push(deleted);
        let mut plan = retention_plan(2);
        plan.terminal_memory_recycle
            .push(vulcan_memorymesh_core::PlannedMemoryRef {
                memory_id: String::from("vmm-memory-terminal"),
                expected_version: 1,
                expected_access,
                reason: RetentionReasonCode::TerminalMemoryRecycle,
            });

        let result = apply_retention_plan(&mut snapshot, &plan, &policy, 100, "retention-test")
            .expect("apply retention");

        assert_eq!(result.recycled_memories, 1);
        assert!(snapshot.dataset.memories.is_empty());
        assert!(snapshot.dataset.context_evidence.is_empty());
        assert_eq!(snapshot.retention_trash_batches.len(), 1);
        assert_eq!(snapshot.retention_trash_batches[0].recycled_at_ns, 100);
        assert_eq!(
            snapshot.retention_trash_batches[0].expires_at_ns,
            12_000_100
        );
        assert_eq!(
            snapshot.retention_trash_batches[0].dataset.memories.len(),
            1
        );
        assert_eq!(
            snapshot.retention_trash_batches[0]
                .dataset
                .context_evidence
                .len(),
            1
        );
        assert!(
            snapshot.without_embeddings().retention_trash_batches[0]
                .dataset
                .memories[0]
                .embedding
                .is_none()
        );
    }

    /// Verifies cold-turn jobs preserve profile sources and recycle only old unreferenced turns.
    /// 验证冷回合任务会保护画像来源，仅回收旧的无引用回合。
    #[test]
    fn retention_cold_turn_job_preserves_profile_references_and_hot_window() {
        let mut snapshot = empty_snapshot(7);
        snapshot.dataset.turns = (1..=10)
            .map(|index| {
                let mut value = turn(format!("vmm-turn-{index}").as_str(), 2, "session-a");
                value.created_at_ns = index;
                value.status = TurnStatus::Completed;
                value
            })
            .collect();
        let mut profile = profile_node(
            "vmm-profile-source",
            project_scope(2).expect("project scope"),
            "protected source",
        );
        profile.source_turn_ids = vec![TurnId::new("vmm-turn-1").expect("turn id")];
        snapshot.dataset.profile_nodes.push(profile);

        let mut enqueue_plan = retention_plan(2);
        enqueue_plan
            .cold_turn_enqueue
            .push(vulcan_memorymesh_core::PlannedSessionRef {
                session_source_id: String::from("session-a"),
                reason: RetentionReasonCode::ColdTurnRecycle,
            });
        let enqueued = apply_retention_plan(
            &mut snapshot,
            &enqueue_plan,
            &RetentionPolicy::v1(),
            100,
            "retention-enqueue",
        )
        .expect("enqueue cold job");
        assert_eq!(enqueued.enqueued_cold_jobs, 1);
        assert_eq!(snapshot.dataset.turns.len(), 10);

        let job_id = snapshot.retention_cold_turn_jobs[0].job_id.clone();
        let mut execute_plan = retention_plan(2);
        execute_plan
            .cold_turn_execute
            .push(vulcan_memorymesh_core::PlannedColdTurnJobRef {
                job_id,
                session_source_id: String::from("session-a"),
                reason: RetentionReasonCode::ColdTurnRecycle,
            });
        let executed = apply_retention_plan(
            &mut snapshot,
            &execute_plan,
            &RetentionPolicy::v1(),
            100,
            "retention-execute",
        )
        .expect("execute cold job");

        assert_eq!(executed.completed_cold_jobs, 1);
        assert_eq!(executed.recycled_turns, 1);
        assert!(
            snapshot
                .dataset
                .turns
                .iter()
                .any(|value| value.id.as_str() == "vmm-turn-1")
        );
        assert!(
            !snapshot
                .dataset
                .turns
                .iter()
                .any(|value| value.id.as_str() == "vmm-turn-2")
        );
        assert_eq!(snapshot.retention_trash_batches.len(), 1);
        assert_eq!(
            snapshot.retention_trash_batches[0].dataset.turns[0]
                .id
                .as_str(),
            "vmm-turn-2"
        );
    }

    /// Verifies a due cold job and idle-session plan for the same session commit without rollback.
    /// 验证同一会话的到期冷作业与闲置会话计划可以提交且不会回滚。
    #[test]
    fn retention_cold_job_and_idle_plan_share_one_atomic_pass() {
        let policy = RetentionPolicy::v1();
        let now_ns = retention_duration_ns(policy.session_idle_ms).expect("idle duration") + 100;
        let mut snapshot = empty_snapshot(7);
        snapshot.dataset.turns = (1..=10)
            .map(|index| {
                let mut value = turn(format!("vmm-turn-overlap-{index}").as_str(), 2, "session-a");
                value.created_at_ns = index;
                value.status = TurnStatus::Completed;
                value
            })
            .collect();
        snapshot
            .retention_cold_turn_jobs
            .push(StorageRetentionColdTurnJob {
                job_id: String::from("cold-job-overlap"),
                subject_id: native_vmm_subject_id().expect("subject"),
                scope_id: project_scope(2).expect("scope"),
                session_source_id: SessionSourceId::new("session-a").expect("session id"),
                next_run_at_ns: 1,
            });
        let mut plan = retention_plan(2);
        plan.cold_turn_execute
            .push(vulcan_memorymesh_core::PlannedColdTurnJobRef {
                job_id: String::from("cold-job-overlap"),
                session_source_id: String::from("session-a"),
                reason: RetentionReasonCode::ColdTurnRecycle,
            });
        plan.idle_session_recycle
            .push(vulcan_memorymesh_core::PlannedSessionRef {
                session_source_id: String::from("session-a"),
                reason: RetentionReasonCode::IdleSessionRecycle,
            });

        let result =
            apply_retention_plan(&mut snapshot, &plan, &policy, now_ns, "retention-overlap")
                .expect("overlapping cold and idle plan");

        assert_eq!(result.completed_cold_jobs, 1);
        assert_eq!(result.recycled_turns, 2);
        assert!(snapshot.retention_cold_turn_jobs.is_empty());
        assert_eq!(snapshot.dataset.turns.len(), 8);
    }

    /// Verifies one retention pass materializes due project and fixed-user profiles as expired.
    /// 验证一次保留运行会把到期的项目画像与固定用户画像物化为已过期状态。
    #[test]
    fn retention_materializes_project_and_user_profile_expiry() {
        let mut snapshot = empty_snapshot(9);
        let mut project_profile = profile_node(
            "vmm-profile-project-expired",
            project_scope(2).expect("project scope"),
            "project profile",
        );
        project_profile.expires_at_ns = Some(100);
        let mut user_profile = profile_node(
            "vmm-profile-user-expired",
            native_vmm_subject_scope().expect("subject scope"),
            "user profile",
        );
        user_profile.expires_at_ns = Some(90);
        snapshot.dataset.profile_nodes = vec![project_profile.clone(), user_profile.clone()];

        let mut plan = retention_plan(2);
        for node in [&project_profile, &user_profile] {
            plan.profile_expiry
                .push(vulcan_memorymesh_core::PlannedProfileNodeRef {
                    profile_node_id: node.id.clone(),
                    expected_subject_id: node.subject_id.clone(),
                    expected_scope_id: node.scope_id.clone(),
                    expected_version: node.version,
                    reason: RetentionReasonCode::ProfileExpiry,
                });
        }

        let result = apply_retention_plan(
            &mut snapshot,
            &plan,
            &RetentionPolicy::v1(),
            100,
            "retention-profile-expiry",
        )
        .expect("apply profile expiry");

        assert_eq!(result.expired_profile_nodes, 2);
        assert!(snapshot.dataset.profile_nodes.iter().all(|node| {
            node.state == ProfileNodeState::Expired
                && node.version == 2
                && node.updated_at_ns == 100
        }));
    }

    /// Verifies project retention cannot expire a profile owned by another project scope.
    /// 验证项目保留流程不能使另一个项目作用域拥有的画像过期。
    #[test]
    fn retention_rejects_cross_project_profile_expiry() {
        let mut snapshot = empty_snapshot(9);
        let mut foreign_profile = profile_node(
            "vmm-profile-foreign-project",
            project_scope(3).expect("foreign project scope"),
            "foreign project profile",
        );
        foreign_profile.expires_at_ns = Some(50);
        snapshot.dataset.profile_nodes.push(foreign_profile.clone());
        let mut plan = retention_plan(2);
        plan.profile_expiry
            .push(vulcan_memorymesh_core::PlannedProfileNodeRef {
                profile_node_id: foreign_profile.id.clone(),
                expected_subject_id: foreign_profile.subject_id.clone(),
                expected_scope_id: foreign_profile.scope_id.clone(),
                expected_version: foreign_profile.version,
                reason: RetentionReasonCode::ProfileExpiry,
            });

        let error = apply_retention_plan(
            &mut snapshot,
            &plan,
            &RetentionPolicy::v1(),
            100,
            "retention-cross-project-profile",
        )
        .expect_err("cross-project profile expiry must fail");

        assert!(error.contains("已发生变化"));
        assert_eq!(
            snapshot.dataset.profile_nodes[0].state,
            ProfileNodeState::Active
        );
    }

    /// Verifies idle-session cleanup requires expiry before the idle cutoff, not merely before now.
    /// 验证闲置会话清洗要求记忆在闲置截止点前已经过期，而不只是当前已经过期。
    #[test]
    fn idle_session_cleanup_respects_idle_cutoff_expiry() {
        let mut snapshot = empty_snapshot(10);
        let mut old_expiry = memory(
            "vmm-memory-old-expiry",
            2,
            "session-a",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Session,
        );
        old_expiry.scope_class = MemoryScopeClass::Session;
        old_expiry.created_at_ns = 10;
        old_expiry.updated_at_ns = 10;
        old_expiry.expires_at_ns = Some(20);
        let mut recent_expiry = old_expiry.clone();
        recent_expiry.id = MemoryId::new("vmm-memory-recent-expiry").expect("memory id");
        recent_expiry.expires_at_ns = Some(80);
        snapshot.dataset.memories = vec![old_expiry, recent_expiry];

        let selected = idle_session_memory_ids(
            &snapshot,
            &native_vmm_subject_id().expect("subject"),
            &project_scope(2).expect("project scope"),
            &SessionSourceId::new("session-a").expect("session id"),
            50,
        );

        assert_eq!(
            selected,
            BTreeSet::from([String::from("vmm-memory-old-expiry")])
        );
    }

    /// Verifies a direct-write-only session can pass apply-time idle validation and be recycled.
    /// 验证只有直接写入记忆的会话也能通过闲置应用时校验并进入回收站。
    #[test]
    fn retention_applies_idle_plan_to_direct_memory_only_session() {
        let mut policy = RetentionPolicy::v1();
        policy.session_idle_ms = 1;
        let now_ns = 2_000_000;
        let mut snapshot = empty_snapshot(11);
        let mut direct_memory = memory(
            "vmm-memory-direct-only",
            2,
            "session-a",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Session,
        );
        direct_memory.scope_class = MemoryScopeClass::Session;
        direct_memory.created_at_ns = 10;
        direct_memory.updated_at_ns = 10;
        direct_memory.expires_at_ns = Some(20);
        snapshot.dataset.memories.push(direct_memory);
        let mut plan = retention_plan(2);
        plan.idle_session_recycle
            .push(vulcan_memorymesh_core::PlannedSessionRef {
                session_source_id: String::from("session-a"),
                reason: RetentionReasonCode::IdleSessionRecycle,
            });

        let result = apply_retention_plan(
            &mut snapshot,
            &plan,
            &policy,
            now_ns,
            "retention-direct-only",
        )
        .expect("direct-write-only idle session should recycle");

        assert_eq!(result.recycled_memories, 1);
        assert!(snapshot.dataset.memories.is_empty());
        assert_eq!(snapshot.retention_trash_batches.len(), 1);
        assert_eq!(
            snapshot.retention_trash_batches[0].dataset.memories.len(),
            1
        );
    }

    /// Verifies the enqueue follow-up cannot repeat scans or exceed one external VMM job batch.
    /// 验证入队后续阶段不会重复扫描，也不会超过一个外置 VMM 作业批次。
    #[test]
    fn cold_turn_follow_up_uses_only_remaining_execution_budget() {
        let policy = native_cold_turn_follow_up_policy(&RetentionPolicy::v1(), 7);
        assert_eq!(policy.profile_expiry_limit, 0);
        assert_eq!(policy.memory_recycle_limit, 0);
        assert_eq!(policy.cold_turn_scan_limit, 0);
        assert_eq!(policy.cold_turn_job_limit, 7);
        assert_eq!(policy.idle_session_limit, 0);
        assert_eq!(policy.vector_gc_limit, 0);
        assert_eq!(policy.trash_purge_limit, 0);
        assert_eq!(
            native_cold_turn_follow_up_policy(&RetentionPolicy::v1(), u32::MAX).cold_turn_job_limit,
            RetentionPolicy::v1().cold_turn_job_limit
        );
    }

    /// Verifies the same-cycle idle follow-up cannot repeat any earlier retention phase.
    /// 验证同周期闲置后续阶段不会重复任何更早的保留阶段。
    #[test]
    fn idle_session_follow_up_keeps_only_idle_recycling_enabled() {
        let policy = native_idle_session_follow_up_policy(&RetentionPolicy::v1());
        assert_eq!(policy.profile_expiry_limit, 0);
        assert_eq!(policy.memory_recycle_limit, 0);
        assert_eq!(policy.cold_turn_scan_limit, 0);
        assert_eq!(policy.cold_turn_job_limit, 0);
        assert_eq!(
            policy.idle_session_limit,
            RetentionPolicy::v1().idle_session_limit
        );
        assert_eq!(policy.vector_gc_limit, 0);
        assert_eq!(policy.trash_purge_limit, 0);
    }

    /// Verifies every specialized follow-up keeps the business rules admitted by its first pass.
    /// 验证所有专用后续阶段保留首轮已准入的业务规则。
    #[test]
    fn retention_follow_up_preserves_admitted_business_rules() {
        // Distinct values make an accidental reset to v1 defaults observable in every phase.
        // 使用不同于默认值的配置，使任一阶段意外重置为 v1 默认值都能被检测。
        let mut base = RetentionPolicy::v1();
        base.turn_hot_window_size = 17;
        base.session_idle_ms = 12_345;
        base.trash_retention_ms = 54_321;
        base.protected_priority_floor = 73;
        base.protected_memory_level_floor = 2;
        base.skip_protected_shared_memories = false;
        base.profile_expiry_limit = 9;
        base.cold_turn_job_limit = 5;
        base.idle_session_limit = 3;
        for derived in [
            native_profile_expiry_policy(&base),
            native_cold_turn_follow_up_policy(&base, u32::MAX),
            native_idle_session_follow_up_policy(&base),
        ] {
            assert_eq!(derived.turn_hot_window_size, base.turn_hot_window_size);
            assert_eq!(derived.session_idle_ms, base.session_idle_ms);
            assert_eq!(derived.trash_retention_ms, base.trash_retention_ms);
            assert_eq!(
                derived.protected_priority_floor,
                base.protected_priority_floor
            );
            assert_eq!(
                derived.protected_memory_level_floor,
                base.protected_memory_level_floor
            );
            assert_eq!(
                derived.skip_protected_shared_memories,
                base.skip_protected_shared_memories
            );
        }
        assert_eq!(native_profile_expiry_policy(&base).profile_expiry_limit, 9);
        assert_eq!(
            native_cold_turn_follow_up_policy(&base, u32::MAX).cold_turn_job_limit,
            5
        );
        assert_eq!(
            native_idle_session_follow_up_policy(&base).idle_session_limit,
            3
        );
    }

    /// Verifies apply-time idle validation includes compaction activity rather than turn creation alone.
    /// 验证闲置计划的应用时校验会包含压缩活动，而不只检查回合创建时间。
    #[test]
    fn retention_rejects_idle_plan_after_recent_compaction_activity() {
        let policy = RetentionPolicy::v1();
        let idle_duration_ns =
            retention_duration_ns(policy.session_idle_ms).expect("idle duration");
        let idle_cutoff_ns = 50;
        let now_ns = idle_duration_ns + idle_cutoff_ns;
        let mut snapshot = empty_snapshot(11);
        let mut old_turn = turn("vmm-turn-idle-activity", 2, "session-a");
        old_turn.status = TurnStatus::Completed;
        old_turn.created_at_ns = 1;
        snapshot.dataset.turns.push(old_turn);
        snapshot
            .dataset
            .chat_compact_boundaries
            .push(ChatCompactBoundary {
                subject_id: native_vmm_subject_id().expect("subject"),
                scope_id: project_scope(2).expect("scope"),
                session_source_id: SessionSourceId::new("session-a").expect("session id"),
                compacted_turn_id: TurnId::new("vmm-turn-idle-activity").expect("turn id"),
                compacted_at_ns: idle_cutoff_ns + 1,
            });
        let mut plan = retention_plan(2);
        plan.idle_session_recycle
            .push(vulcan_memorymesh_core::PlannedSessionRef {
                session_source_id: String::from("session-a"),
                reason: RetentionReasonCode::IdleSessionRecycle,
            });

        let error = apply_retention_plan(
            &mut snapshot,
            &plan,
            &policy,
            now_ns,
            "retention-recent-compaction",
        )
        .expect_err("recent compaction must invalidate the idle plan");

        assert!(error.contains("已恢复活动"));
        assert_eq!(snapshot.dataset.turns.len(), 1);
        assert!(snapshot.retention_trash_batches.is_empty());
    }

    /// Verifies recycle summaries preserve project identity, reason, deadline, and exact counts.
    /// 验证回收摘要保留项目身份、原因、截止时间与准确计数。
    #[test]
    fn recycle_batch_summary_exposes_frozen_policy_deadline() {
        let mut batch = retention_trash_batch(
            String::from("batch-a"),
            native_vmm_subject_id().expect("subject"),
            project_scope(2).expect("scope"),
            2_000_000,
            9_000_000,
            RetentionReasonCode::IdleSessionRecycle,
        );
        batch.dataset.turns.push(turn("vmm-turn-1", 2, "session-a"));
        batch.dataset.memories.push(memory(
            "vmm-memory-1",
            2,
            "session-a",
            Some("vmm-turn-1"),
            MemorySourceKind::Turn,
            MemoryLevel::Phase,
        ));

        let summary = retention_batch_summary(&batch).expect("recycle batch summary");

        assert_eq!(summary.project_id, 2);
        assert_eq!(summary.reason, "idle_session_recycle");
        assert_eq!(summary.recycled_at_unix_ms, 2);
        assert_eq!(summary.turn_count, 1);
        assert_eq!(summary.memory_count, 1);
        assert_eq!(summary.expires_at_unix_ms, 9);
        // Missing persisted deadlines must fail instead of being backfilled from current defaults.
        // 缺失持久化截止时间必须报错，不可按当前默认窗口回填。
        let mut json = serde_json::to_value(&batch).expect("serialize batch");
        json.as_object_mut()
            .expect("batch object")
            .remove("expires_at_ns");
        assert!(serde_json::from_value::<StorageRetentionTrashBatch>(json).is_err());
    }

    /// Inspects exact retained pages without leaking active records, other projects or vector arrays.
    /// 查看准确保留页，不泄露活动记录、其他项目或向量数组。
    #[test]
    fn recycle_records_preserve_families_and_versioned_ownership() {
        use vulcan_service_api::{
            VmmNativeRecycleRecord, VmmNativeRecycleRecordKind as Kind, VmmNativeRecycleRecordQuery,
        };
        let mut snapshot = empty_snapshot(7);
        let mut batch = retention_trash_batch(
            "inspection".to_owned(),
            native_vmm_subject_id().expect("subject"),
            project_scope(2).expect("scope"),
            1,
            100,
            RetentionReasonCode::IdleSessionRecycle,
        );
        batch
            .dataset
            .turns
            .push(turn("vmm-turn-1", 2, "source-session"));
        for id in ["vmm-memory-3", "vmm-memory-1", "vmm-memory-2"] {
            batch.dataset.memories.push(memory(
                id,
                2,
                "source-session",
                Some("vmm-turn-1"),
                MemorySourceKind::Turn,
                MemoryLevel::Phase,
            ));
        }
        batch.dataset.memories[0].embedding = Some(vec![0.123, 0.456]);
        batch.dataset.profile_nodes.push(profile_node(
            "profile-project",
            project_scope(2).expect("scope"),
            "项目画像原文",
        ));
        batch.dataset.profile_nodes.push(profile_node(
            "profile-user",
            native_vmm_subject_scope().expect("personal"),
            "用户画像原文",
        ));
        batch
            .dataset
            .context_evidence
            .push(vulcan_memorymesh_core::MemoryContextEvidence {
                memory_id: MemoryId::new("vmm-memory-1").expect("memory"),
                context_key: "fruit".to_owned(),
                context_value: "西瓜".to_owned(),
                support_count: 4,
                rebuttal_count: 2,
                last_supported_at_ns: None,
                last_rebutted_at_ns: None,
                version: 1,
            });
        snapshot.retention_trash_batches.push(batch);
        snapshot.dataset.memories.push(memory(
            "active-only",
            2,
            "source-session",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Phase,
        ));
        let before = snapshot.clone();
        let query = VmmNativeRecycleRecordQuery {
            vmm_project_id: 2,
            batch_id: "inspection".to_owned(),
            record_kind: Kind::Memory,
            offset: 0,
            limit: 2,
            expected_storage_version: Some(7),
        };
        let first = recycle_records::build_page(&snapshot, query.clone()).expect("first page");
        assert_eq!(first.total, 3);
        assert_eq!(first.next_offset, Some(2));
        assert_eq!(first.records.len(), 2);
        assert!(
            matches!(&first.records[0], VmmNativeRecycleRecord::Memory { memory_id, source_turn_id: Some(turn), .. } if memory_id == "vmm-memory-1" && turn == "vmm-turn-1")
        );
        let last = recycle_records::build_page(
            &snapshot,
            VmmNativeRecycleRecordQuery {
                offset: 2,
                ..query.clone()
            },
        )
        .expect("last page");
        assert_eq!(last.records.len(), 1);
        assert_eq!(last.next_offset, None);
        assert!(
            !serde_json::to_string(&last)
                .expect("wire")
                .contains("embedding")
        );
        for (kind, total) in [(Kind::Turn, 1), (Kind::Profile, 2), (Kind::Evidence, 1)] {
            let page = recycle_records::build_page(
                &snapshot,
                VmmNativeRecycleRecordQuery {
                    record_kind: kind,
                    ..query.clone()
                },
            )
            .expect("family");
            assert_eq!(page.total, total);
            let wire = serde_json::to_value(&page).expect("family wire");
            match kind {
                Kind::Profile => {
                    assert_eq!(wire["records"][0]["target"], "project");
                    assert_eq!(wire["records"][1]["target"], "user");
                }
                Kind::Evidence => {
                    assert_eq!(wire["records"][0]["contextValue"], "西瓜");
                    assert_eq!(wire["records"][0]["supportCount"], 4);
                    assert_eq!(wire["records"][0]["rebuttalCount"], 2);
                }
                Kind::Turn => assert_eq!(wire["records"][0]["sessionSourceId"], "source-session"),
                Kind::Memory => unreachable!(),
            }
        }
        for invalid in [
            VmmNativeRecycleRecordQuery {
                vmm_project_id: 3,
                ..query.clone()
            },
            VmmNativeRecycleRecordQuery {
                expected_storage_version: Some(6),
                ..query.clone()
            },
            VmmNativeRecycleRecordQuery {
                limit: 0,
                ..query.clone()
            },
            VmmNativeRecycleRecordQuery {
                limit: 101,
                ..query.clone()
            },
            VmmNativeRecycleRecordQuery {
                batch_id: "missing".to_owned(),
                ..query.clone()
            },
        ] {
            assert!(recycle_records::build_page(&snapshot, invalid).is_err());
        }
        assert_eq!(snapshot, before);
        snapshot.retention_trash_batches.clear();
        assert!(recycle_records::build_page(&snapshot, query).is_err());
    }

    /// Verifies delayed native recall and persisted leases use current visibility without touching unrelated hooks.
    /// 验证迟到原生召回及持久租约使用当前可见性，不触及无关 Hook。
    #[test]
    fn recall_publication_reconciles_resident_and_delayed_sources() {
        use vulcan_agent_runtime::agent_runtime::conversation_hooks::{
            ConversationPrecheckOutcome, HOOK_METADATA_VMM_RECALL_BATCH_EFFECT,
            VmmRecallBatchHookEffect, vmm_recall_batch_effect_from_report,
            vmm_session_status_effect_from_report,
        };
        use vulcan_vmm::{ContextItem, SessionMemoryStatus, VmmRecallBatch, VmmSessionMemoryState};
        // A real UUID preserves the host-to-native session identity contract.
        // 真实 UUID 保持宿主到原生的会话身份契约。
        let session_id = uuid::Uuid::nil();
        // Owner and linked scopes are declared explicitly; project 4 is intentionally inaccessible.
        // 明确声明归属及连带作用域；项目 4 被刻意设为不可访问。
        let access = ScopeAccess {
            write_scope: project_scope(2).expect("owner"),
            visible_scopes: vec![
                project_scope(2).expect("owner"),
                project_scope(3).expect("linked"),
            ],
        };
        // Four original rows contrast live owner, live linked, foreign and logically deleted content.
        // 四条原记录对照活动归属、活动连带、外部及逻辑删除内容。
        let mut snapshot = empty_snapshot(1);
        for (id, project) in [(1, 2), (2, 3), (3, 4), (4, 2)] {
            snapshot.dataset.memories.push(memory(
                &format!("vmm-memory-{id}"),
                project,
                &session_id.to_string(),
                None,
                MemorySourceKind::Direct,
                MemoryLevel::Persistent,
            ));
        }
        snapshot.dataset.memories[3].state = MemoryState::Deleted;
        // Missing ID 5 represents a previously recalled record now removed from active storage.
        // 缺失 ID 5 表示此前召回、现已移出活动存储的记录。
        let batch = VmmRecallBatch {
            ttl_turns: 3,
            injection_tag: "vmm".to_owned(),
            injection_description: "memory".to_owned(),
            items: (1..=5)
                .map(|id| ContextItem {
                    memory_id: id,
                    text: format!("memory-{id}"),
                    ..Default::default()
                })
                .collect(),
        };
        for paused in [false, true] {
            // Each scenario starts with the same already persisted cache and delayed response.
            // 每种场景从相同的已有持久缓存及迟到响应开始。
            let mut state = VmmSessionMemoryState::new(session_id.to_string(), "1").expect("state");
            state
                .select_read_source(
                    &session_id.to_string(),
                    Some(
                        &crate::native_vmm::client::memory_read_source_from_access(&access)
                            .expect("source"),
                    ),
                    "1",
                )
                .expect("select source");
            state
                .apply_recall_batch(&session_id.to_string(), "user-1", &batch, "2")
                .expect("previous recall");
            // Preserve an unrelated hook payload and metadata as exact controls.
            // 保留无关 Hook 载荷及元数据作为准确对照。
            let mut report = vulcan_hooks::HookDispatchReport::empty(vulcan_hooks::HookPayload {
                value: serde_json::json!({"user_content": "unchanged"}),
            });
            report.metadata.insert(
                "unrelated".to_owned(),
                vec![serde_json::json!({"keep": true})],
            );
            report.metadata.insert(
                HOOK_METADATA_VMM_RECALL_BATCH_EFFECT.to_owned(),
                vec![serde_json::json!(VmmRecallBatchHookEffect {
                    read_source: crate::native_vmm::client::memory_read_source_from_access(&access)
                        .expect("recall source"),
                    batch: batch.clone()
                })],
            );
            // The correlated outcome deliberately arrives after source visibility changes.
            // 关联结果刻意在来源可见性改变后到达。
            let mut outcome = ConversationPrecheckOutcome {
                session_id,
                turn_id: "new-turn".to_owned(),
                report: Ok(report),
            };
            // The independent lifecycle is authoritative, even when all recalled IDs still exist.
            // 即使全部召回 ID 仍然存在，独立生命周期仍是权威。
            let mut current = snapshot.clone();
            if paused {
                current
                    .memory_sessions
                    .push(vulcan_memorymesh_core::StorageMemorySession {
                        subject_id: native_vmm_subject_id().expect("subject"),
                        scope_id: access.write_scope.clone(),
                        session_source_id: SessionSourceId::new(session_id.to_string())
                            .expect("session"),
                        status: vulcan_memorymesh_core::StorageSessionStatus::Recycled,
                        last_extraction_observed_at_ns: 0,
                        last_extraction_completed_at_ns: None,
                    });
            }
            // Production storage validation precedes the bounded publication lookup.
            // 生产存储校验先于有界发布查询。
            let storage =
                vulcan_memorymesh_core::InMemoryStorage::from_snapshot(current).expect("storage");
            recall_publication::reconcile_recall(
                &storage,
                &native_vmm_subject_id().expect("subject"),
                &access,
                &SessionSourceId::new(session_id.to_string()).expect("session"),
                &mut state,
                Some(&mut outcome),
                "3",
            )
            .expect("reconcile");
            // Typed status and recall remain separate; an empty/paused batch is removed entirely.
            // 类型化状态与召回保持分离；空或暂停批次被整体移除。
            let report = outcome.report.as_ref().expect("report");
            assert_eq!(
                report.payload.value,
                serde_json::json!({"user_content": "unchanged"})
            );
            assert_eq!(
                report.metadata["unrelated"],
                vec![serde_json::json!({"keep": true})]
            );
            if paused {
                assert_eq!(state.session_memory_status, SessionMemoryStatus::Recycled);
                assert!(state.active_leases.is_empty());
                assert!(
                    vmm_recall_batch_effect_from_report(report)
                        .expect("typed recall")
                        .is_none()
                );
            } else {
                assert_eq!(
                    state.active_leases.keys().cloned().collect::<Vec<_>>(),
                    ["memory:1", "memory:2"]
                );
                // Applying the delayed response cannot reintroduce removed, deleted or out-of-scope memory.
                // 应用迟到响应不能重新引入已移除、已删除或作用域外记忆。
                let effect = vmm_recall_batch_effect_from_report(report)
                    .expect("typed recall")
                    .expect("retained recall");
                assert_eq!(
                    effect
                        .batch
                        .items
                        .iter()
                        .map(|item| item.memory_id)
                        .collect::<Vec<_>>(),
                    [1, 2]
                );
                state
                    .apply_recall_batch(&session_id.to_string(), "user-2", &effect.batch, "4")
                    .expect("safe delayed batch");
                assert_eq!(state.active_leases.len(), 2);
            }
            assert_eq!(
                vmm_session_status_effect_from_report(report)
                    .expect("typed status")
                    .expect("status")
                    .status,
                state.session_memory_status
            );
        }
    }

    /// Builds valid source-linked content with an empty owner and same-name foreign session.
    /// 构造有效来源关联内容，包含空归属及其他项目同名会话。
    /// Returns a canonical snapshot for both whole-session and individual-turn moves.
    /// 返回供完整会话及独立回合移动使用的规范快照。
    fn manual_removal_snapshot() -> StorageSnapshot {
        // Begin with the exact production storage shape.
        // 从准确生产存储结构开始。
        let mut snapshot = empty_snapshot(17);
        for (project, session) in [(2, "a"), (2, "empty"), (3, "a")] {
            snapshot
                .memory_sessions
                .push(vulcan_memorymesh_core::StorageMemorySession {
                    subject_id: native_vmm_subject_id().expect("subject"),
                    scope_id: project_scope(project).expect("scope"),
                    session_source_id: SessionSourceId::new(session).expect("session"),
                    status: vulcan_memorymesh_core::StorageSessionStatus::Archived,
                    last_extraction_observed_at_ns: 20,
                    last_extraction_completed_at_ns: Some(30),
                });
        }
        snapshot.dataset.turns = vec![
            turn("vmm-turn-2", 2, "a"),
            turn("vmm-turn-10", 2, "a"),
            turn("vmm-turn-20", 3, "a"),
        ];
        for (id, project, source) in [
            ("source", 2, Some("vmm-turn-2")),
            ("direct", 2, None),
            ("foreign", 3, Some("vmm-turn-20")),
        ] {
            // Include persistent direct writes; manual removal must not apply periodic eligibility filters.
            // 包含持久直写；人工移除不能应用周期清洗资格过滤。
            let mut record = memory(
                id,
                project,
                "a",
                source,
                MemorySourceKind::Direct,
                MemoryLevel::Persistent,
            );
            record.vector_space_id =
                Some(VectorSpaceId::new("original-generation").expect("generation"));
            record.embedding = Some(vec![0.25, 0.75]);
            snapshot.dataset.memories.push(record);
        }
        for (id, scope) in [
            ("project-profile", project_scope(2).expect("project")),
            (
                "user-profile",
                native_vmm_subject_scope().expect("personal"),
            ),
        ] {
            // Both profile families explicitly cite the selected turn but remain independently owned.
            // 两类画像均明确引用所选回合，但保持独立归属。
            let mut node = profile_node(id, scope, id);
            node.source_turn_ids = vec![TurnId::new("vmm-turn-2").expect("turn")];
            snapshot.dataset.profile_nodes.push(node);
        }
        snapshot
            .dataset
            .context_evidence
            .push(vulcan_memorymesh_core::MemoryContextEvidence {
                memory_id: MemoryId::new("source").expect("memory"),
                context_key: "branch".to_owned(),
                context_value: "main".to_owned(),
                support_count: 1,
                rebuttal_count: 0,
                last_supported_at_ns: None,
                last_rebutted_at_ns: None,
                version: 1,
            });
        for (project, id) in [(2, "vmm-turn-10"), (3, "vmm-turn-20")] {
            snapshot.dataset.chat_compact_boundaries.push(
                vulcan_memorymesh_core::ChatCompactBoundary {
                    subject_id: native_vmm_subject_id().expect("subject"),
                    scope_id: project_scope(project).expect("scope"),
                    session_source_id: SessionSourceId::new("a").expect("session"),
                    compacted_turn_id: TurnId::new(id).expect("turn"),
                    compacted_at_ns: 40,
                },
            );
        }
        vulcan_memorymesh_core::InMemoryStorage::from_snapshot(snapshot)
            .expect("valid fixture")
            .snapshot()
    }

    /// Checks exact source moves, profile/vector preservation, session metadata and reversible identity.
    /// 检查准确来源移动、画像及向量保留、会话元数据及可恢复身份。
    #[test]
    fn manual_removal_moves_originals_and_restores_exact_owner() {
        use vulcan_service_api::{
            VmmNativeRemovalPreview, VmmNativeRemovalQuery, VmmNativeRemovalTarget,
        };
        for selection in [
            VmmNativeRemovalTarget::Sessions {
                ids: vec!["a".to_owned(), "empty".to_owned()],
            },
            VmmNativeRemovalTarget::Turns {
                ids: vec!["vmm-turn-2".to_owned()],
            },
        ] {
            // Each variant starts from a fresh canonical snapshot.
            // 每种变体从新的规范快照开始。
            let before = manual_removal_snapshot();
            // The target kind alone controls whether session metadata and compaction move.
            // 只有目标种类决定会话元数据及压缩是否移动。
            let whole = matches!(&selection, VmmNativeRemovalTarget::Sessions { .. });
            // Build the same impact used by the production preview.
            // 构建生产预览使用的相同影响。
            let impact = removal_impact::build_impact(
                &before,
                VmmNativeRemovalQuery {
                    vmm_project_id: 2,
                    selection,
                },
                2,
            )
            .expect("impact");
            // The transaction candidate preserves the original snapshot for comparison.
            // 事务候选保留原始快照用于比较。
            let mut snapshot = before.clone();
            // Both paths must keep the same source IDs and declared vector generation.
            // 两条路径均须保留相同来源 ID 及声明向量代次。
            let result = manual_removal::apply_manual_removal(
                &mut snapshot,
                VmmNativeRemovalPreview {
                    preview_id: "exact-source".to_owned(),
                    impact: impact.clone(),
                    trash_retention_ms: 50,
                },
                100,
            )
            .expect("move originals");
            assert_eq!(result.storage_version, before.storage_version + 1);
            assert_eq!(result.impact, impact);
            assert_eq!(snapshot.dataset.profile_nodes, before.dataset.profile_nodes);
            assert_eq!(snapshot.retention_trash_batches.len(), 1);
            // The complete batch, including saved session originals, is validated by the real loader.
            // 完整批次连同保存的原会话一起由真实加载器校验。
            let mut persisted = vulcan_memorymesh_core::InMemoryStorage::from_snapshot(snapshot)
                .expect("valid moved snapshot")
                .snapshot();
            // Remove the batch as the production restore transaction does.
            // 像生产恢复事务一样取出批次。
            let batch = persisted.retention_trash_batches.remove(0);
            // UI summaries must preserve original empty-session selections rather than infer them from turns.
            // 界面摘要必须保留原始空会话选择，而非从回合推测它们。
            let summary = retention_batch_summary(&batch).expect("manual source projection");
            assert_eq!(
                summary.manual_selection,
                Some(impact.query.selection.clone())
            );
            assert_eq!(batch.expires_at_ns, 50_000_100);
            assert!(batch.dataset.profile_nodes.is_empty());
            assert_eq!(batch.dataset.memories.len(), if whole { 2 } else { 1 });
            assert_eq!(batch.dataset.turns.len(), if whole { 2 } else { 1 });
            assert_eq!(
                batch.dataset.chat_compact_boundaries.len(),
                usize::from(whole)
            );
            assert_eq!(batch.dataset.context_evidence.len(), 1);
            for moved in &batch.dataset.memories {
                assert_eq!(
                    Some(moved),
                    before
                        .dataset
                        .memories
                        .iter()
                        .find(|memory| memory.id == moved.id)
                );
            }
            if !whole {
                assert_eq!(persisted.memory_sessions, before.memory_sessions);
            }
            ensure_recycle_restore_has_no_conflicts(&persisted, &batch)
                .expect("restore exact identity");
            restore_retention_batch(&mut persisted, batch).expect("restore originals");
            // External whole-session restore activates only the exact restored owners.
            // 外置完整会话恢复只激活准确恢复的归属。
            let mut expected = before;
            if whole {
                for session in &mut expected.memory_sessions {
                    if session.scope_id == project_scope(2).expect("scope") {
                        session.status = vulcan_memorymesh_core::StorageSessionStatus::Active;
                    }
                }
            }
            assert_eq!(
                vulcan_memorymesh_core::InMemoryStorage::from_snapshot(persisted)
                    .expect("valid restored")
                    .snapshot(),
                expected
            );
        }
    }

    /// Rejects stale membership and invalid deadlines before changing any transaction content.
    /// 在改变事务内容之前拒绝过期成员关系及无效截止时间。
    #[test]
    fn manual_removal_rejects_changed_impact_without_partial_mutation() {
        use vulcan_service_api::{
            VmmNativeRemovalPreview, VmmNativeRemovalQuery, VmmNativeRemovalTarget,
        };
        // Capture one exact turn with both ordinary and independent profile sources.
        // 捕获一个同时关联普通记忆及独立画像来源的准确回合。
        let before = manual_removal_snapshot();
        // This immutable impact is the server-owned confirmation baseline.
        // 此不可变影响是服务端确认基线。
        let impact = removal_impact::build_impact(
            &before,
            VmmNativeRemovalQuery {
                vmm_project_id: 2,
                selection: VmmNativeRemovalTarget::Turns {
                    ids: vec!["vmm-turn-2".to_owned()],
                },
            },
            2,
        )
        .expect("impact");
        for case in 0..4 {
            // Each rejection receives its own untouched transaction candidate.
            // 每种拒绝情况取得自己的未改动事务候选。
            let mut snapshot = before.clone();
            // Only explicit test inputs differ, not the production selection algorithm.
            // 只改变明确测试输入，而非生产选择算法。
            let mut preview = VmmNativeRemovalPreview {
                preview_id: "reject".to_owned(),
                impact: impact.clone(),
                trash_retention_ms: 50,
            };
            match case {
                0 => preview.impact.memory_ids.clear(),
                1 => preview.impact.storage_version += 1,
                2 => preview.trash_retention_ms = 0,
                3 => preview.trash_retention_ms = u64::MAX,
                _ => unreachable!(),
            }
            assert!(manual_removal::apply_manual_removal(&mut snapshot, preview, 100).is_err());
            assert_eq!(snapshot, before);
        }
    }

    /// Verifies manual source inspection includes direct and persistent memories without applying retention filters.
    /// 验证人工来源检查包含直写及持久记忆，不应用保留清洗过滤。
    #[test]
    fn removal_impact_resolves_exact_sources_and_numeric_hot_windows() {
        use vulcan_service_api::{VmmNativeRemovalQuery, VmmNativeRemovalTarget};
        // Same-name foreign sessions and empty sessions must remain separate authoritative records.
        // 同名外部项目会话与空会话必须保留为独立权威记录。
        let mut snapshot = empty_snapshot(17);
        for (project, id) in [(2, "a"), (2, "b"), (2, "empty"), (3, "a")] {
            snapshot
                .memory_sessions
                .push(vulcan_memorymesh_core::StorageMemorySession {
                    subject_id: native_vmm_subject_id().expect("subject"),
                    scope_id: project_scope(project).expect("scope"),
                    session_source_id: SessionSourceId::new(id).expect("session"),
                    status: vulcan_memorymesh_core::StorageSessionStatus::Active,
                    last_extraction_observed_at_ns: 0,
                    last_extraction_completed_at_ns: None,
                });
        }
        for (id, project, session) in [
            (10, 2, "a"),
            (2, 2, "a"),
            (1, 2, "a"),
            (20, 2, "b"),
            (30, 3, "a"),
        ] {
            snapshot
                .dataset
                .turns
                .push(turn(&format!("vmm-turn-{id}"), project, session));
        }
        snapshot.dataset.turns[0].status = TurnStatus::Completed;
        for (id, project, session, source) in [
            ("m1", 2, "a", Some("vmm-turn-1")),
            ("m2", 2, "a", None),
            ("m3", 2, "b", Some("vmm-turn-2")),
            ("m4", 2, "b", Some("vmm-turn-20")),
            ("m5", 3, "a", Some("vmm-turn-1")),
        ] {
            snapshot.dataset.memories.push(memory(
                id,
                project,
                session,
                source,
                MemorySourceKind::Direct,
                MemoryLevel::Persistent,
            ));
        }
        snapshot.dataset.memories[1].state = MemoryState::Deleted;
        snapshot.dataset.memories[0].vector_space_id =
            Some(VectorSpaceId::new("generation").expect("space"));
        snapshot.dataset.memories[0].embedding = Some(vec![1.0]);
        for (id, scope, sources) in [
            (
                "project",
                project_scope(2).expect("scope"),
                vec!["vmm-turn-1"],
            ),
            (
                "user",
                native_vmm_subject_scope().expect("user scope"),
                vec!["vmm-turn-1", "vmm-turn-20"],
            ),
            (
                "foreign",
                project_scope(3).expect("scope"),
                vec!["vmm-turn-1"],
            ),
            ("manual", project_scope(2).expect("scope"), vec![]),
        ] {
            let mut node = profile_node(id, scope, id);
            node.source_turn_ids = sources
                .into_iter()
                .map(|id| TurnId::new(id).expect("source"))
                .collect();
            if id == "project" {
                node.subject_id = None;
            }
            snapshot.dataset.profile_nodes.push(node);
        }
        for id in ["m1", "m4"] {
            snapshot
                .dataset
                .context_evidence
                .push(vulcan_memorymesh_core::MemoryContextEvidence {
                    memory_id: MemoryId::new(id).expect("memory"),
                    context_key: "branch".to_owned(),
                    context_value: "main".to_owned(),
                    support_count: 1,
                    rebuttal_count: 0,
                    last_supported_at_ns: None,
                    last_rebutted_at_ns: None,
                    version: 1,
                });
        }
        let before = snapshot.clone();
        let query = VmmNativeRemovalQuery {
            vmm_project_id: 2,
            selection: VmmNativeRemovalTarget::Sessions {
                ids: vec!["a".to_owned(), "a".to_owned()],
            },
        };
        let impact =
            removal_impact::build_impact(&snapshot, query.clone(), 2).expect("session impact");
        assert_eq!(impact.session_source_ids, ["a"]);
        assert_eq!(impact.turn_ids, ["vmm-turn-1", "vmm-turn-2", "vmm-turn-10"]);
        assert_eq!(impact.hot_window_turn_ids, ["vmm-turn-2", "vmm-turn-10"]);
        assert_eq!(impact.pending_turn_ids, ["vmm-turn-1", "vmm-turn-2"]);
        assert_eq!(impact.memory_ids, ["m1", "m2", "m3"]);
        assert_eq!(impact.vector_memory_ids, ["m1"]);
        assert_eq!(impact.project_profile_node_ids, ["project"]);
        assert_eq!(impact.user_profile_node_ids, ["user"]);
        assert_eq!(impact.context_evidence.len(), 1);
        assert_eq!(impact.context_evidence[0].memory_id, "m1");
        assert_eq!(impact.storage_version, 17);
        let turn_impact = removal_impact::build_impact(
            &snapshot,
            VmmNativeRemovalQuery {
                vmm_project_id: 2,
                selection: VmmNativeRemovalTarget::Turns {
                    ids: vec!["vmm-turn-2".to_owned()],
                },
            },
            1,
        )
        .expect("single turn");
        assert_eq!(turn_impact.memory_ids, ["m3"]);
        assert!(turn_impact.hot_window_turn_ids.is_empty());
        assert!(turn_impact.user_profile_node_ids.is_empty());
        let empty = removal_impact::build_impact(
            &snapshot,
            VmmNativeRemovalQuery {
                vmm_project_id: 2,
                selection: VmmNativeRemovalTarget::Sessions {
                    ids: vec!["empty".to_owned()],
                },
            },
            2,
        )
        .expect("empty session still exists");
        assert_eq!(empty.session_source_ids, ["empty"]);
        assert!(empty.turn_ids.is_empty());
        assert!(empty.memory_ids.is_empty());
        for selection in [
            VmmNativeRemovalTarget::Sessions { ids: vec![] },
            VmmNativeRemovalTarget::Sessions {
                ids: vec!["a".to_owned(); 101],
            },
            VmmNativeRemovalTarget::Sessions {
                ids: vec![" a".to_owned()],
            },
            VmmNativeRemovalTarget::Sessions {
                ids: vec!["missing".to_owned()],
            },
            VmmNativeRemovalTarget::Turns {
                ids: vec!["vmm-turn-30".to_owned()],
            },
            VmmNativeRemovalTarget::Turns {
                ids: vec!["vmm-turn-wrong".to_owned()],
            },
            VmmNativeRemovalTarget::Turns {
                ids: vec!["vmm-turn-2".to_owned(), "vmm-turn-404".to_owned()],
            },
        ] {
            assert!(
                removal_impact::build_impact(
                    &snapshot,
                    VmmNativeRemovalQuery {
                        vmm_project_id: 2,
                        selection
                    },
                    2
                )
                .is_err()
            );
        }
        assert_eq!(snapshot, before);
        snapshot.memory_sessions[0].status = vulcan_memorymesh_core::StorageSessionStatus::Archived;
        assert!(
            removal_impact::build_impact(&snapshot, query.clone(), 0)
                .expect("archived allowed")
                .hot_window_turn_ids
                .is_empty()
        );
        for status in [
            vulcan_memorymesh_core::StorageSessionStatus::Recycled,
            vulcan_memorymesh_core::StorageSessionStatus::Forgotten,
        ] {
            snapshot.memory_sessions[0].status = status;
            assert!(removal_impact::build_impact(&snapshot, query.clone(), 2).is_err());
        }
    }

    /// Verifies restoring a batch moves its original vector-bearing records without regeneration.
    /// 验证恢复批次会移动其原始含向量记录，且不重新生成向量。
    #[test]
    fn recycle_restore_moves_original_records_and_preserves_embedding() {
        let mut snapshot = empty_snapshot(9);
        let mut batch = retention_trash_batch(
            String::from("batch-restore"),
            native_vmm_subject_id().expect("subject"),
            project_scope(2).expect("scope"),
            1,
            100,
            RetentionReasonCode::TerminalMemoryRecycle,
        );
        let mut restored_memory = memory(
            "vmm-memory-restored",
            2,
            "session-a",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Stable,
        );
        restored_memory.embedding = Some(vec![0.25, 0.75]);
        batch.dataset.memories.push(restored_memory);

        ensure_recycle_restore_has_no_conflicts(&snapshot, &batch).expect("conflict-free restore");
        restore_retention_batch(&mut snapshot, batch).expect("restore original records");

        assert_eq!(snapshot.dataset.memories.len(), 1);
        assert_eq!(
            snapshot.dataset.memories[0].embedding.as_deref(),
            Some([0.25_f32, 0.75_f32].as_slice())
        );
    }

    /// Verifies restore refuses identities that became active after recycling.
    /// 验证恢复会拒绝回收后重新变为活动状态的身份。
    #[test]
    fn recycle_restore_rejects_active_identity_conflicts() {
        let mut snapshot = empty_snapshot(10);
        snapshot
            .dataset
            .turns
            .push(turn("vmm-turn-conflict", 2, "session-a"));
        let mut batch = retention_trash_batch(
            String::from("batch-conflict"),
            native_vmm_subject_id().expect("subject"),
            project_scope(2).expect("scope"),
            1,
            100,
            RetentionReasonCode::ColdTurnRecycle,
        );
        batch
            .dataset
            .turns
            .push(turn("vmm-turn-conflict", 2, "session-a"));

        let error = ensure_recycle_restore_has_no_conflicts(&snapshot, &batch)
            .expect_err("restore conflict");

        assert!(error.contains("turn:vmm-turn-conflict"));
    }

    /// Verifies every native retention family rejects a mismatched stable reason code.
    /// 验证每一个原生保留计划族都会拒绝不匹配的稳定原因码。
    #[test]
    fn retention_rejects_wrong_reason_codes_for_every_plan_family() {
        let expected_access = MemoryAccessIdentity::from(&memory(
            "vmm-memory-reason",
            2,
            "session-a",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Phase,
        ));
        let mut plans = Vec::new();

        let mut profile_plan = retention_plan(2);
        profile_plan
            .profile_expiry
            .push(vulcan_memorymesh_core::PlannedProfileNodeRef {
                profile_node_id: ProfileNodeId::new("vmm-profile-reason").expect("profile id"),
                expected_subject_id: Some(native_vmm_subject_id().expect("subject")),
                expected_scope_id: project_scope(2).expect("scope"),
                expected_version: 1,
                reason: RetentionReasonCode::ColdTurnRecycle,
            });
        plans.push(profile_plan);

        let mut memory_plan = retention_plan(2);
        memory_plan
            .terminal_memory_recycle
            .push(vulcan_memorymesh_core::PlannedMemoryRef {
                memory_id: String::from("vmm-memory-reason"),
                expected_version: 1,
                expected_access,
                reason: RetentionReasonCode::IdleSessionRecycle,
            });
        plans.push(memory_plan);

        let mut enqueue_plan = retention_plan(2);
        enqueue_plan
            .cold_turn_enqueue
            .push(vulcan_memorymesh_core::PlannedSessionRef {
                session_source_id: String::from("session-a"),
                reason: RetentionReasonCode::TerminalMemoryRecycle,
            });
        plans.push(enqueue_plan);

        let mut execute_plan = retention_plan(2);
        execute_plan
            .cold_turn_execute
            .push(vulcan_memorymesh_core::PlannedColdTurnJobRef {
                job_id: String::from("cold-job-reason"),
                session_source_id: String::from("session-a"),
                reason: RetentionReasonCode::TrashExpiryPurge,
            });
        plans.push(execute_plan);

        let mut idle_plan = retention_plan(2);
        idle_plan
            .idle_session_recycle
            .push(vulcan_memorymesh_core::PlannedSessionRef {
                session_source_id: String::from("session-a"),
                reason: RetentionReasonCode::ColdTurnRecycle,
            });
        plans.push(idle_plan);

        let mut purge_plan = retention_plan(2);
        purge_plan
            .trash_purge
            .push(vulcan_memorymesh_core::PlannedTrashBatchRef {
                batch_id: String::from("trash-batch-reason"),
                reason: RetentionReasonCode::ProfileExpiry,
            });
        plans.push(purge_plan);

        for plan in plans {
            let error = apply_retention_plan(
                &mut empty_snapshot(1),
                &plan,
                &RetentionPolicy::v1(),
                100,
                "retention-reason",
            )
            .expect_err("wrong reason must fail");
            assert!(error.contains("原因码"), "unexpected error: {error}");
        }
    }

    /// Verifies duplicate profile identities fail before any profile state is mutated.
    /// 验证重复画像身份会在任何画像状态被修改前失败。
    #[test]
    fn retention_rejects_duplicate_profile_identity_atomically() {
        let mut snapshot = empty_snapshot(1);
        let mut profile = profile_node(
            "vmm-profile-duplicate",
            project_scope(2).expect("scope"),
            "duplicate profile",
        );
        profile.expires_at_ns = Some(50);
        snapshot.dataset.profile_nodes.push(profile.clone());
        let planned = vulcan_memorymesh_core::PlannedProfileNodeRef {
            profile_node_id: profile.id.clone(),
            expected_subject_id: profile.subject_id.clone(),
            expected_scope_id: profile.scope_id.clone(),
            expected_version: profile.version,
            reason: RetentionReasonCode::ProfileExpiry,
        };
        let mut plan = retention_plan(2);
        plan.profile_expiry = vec![planned.clone(), planned];

        let error = apply_retention_plan(
            &mut snapshot,
            &plan,
            &RetentionPolicy::v1(),
            100,
            "retention-duplicate",
        )
        .expect_err("duplicate profile must fail");

        assert!(error.contains("重复身份"));
        assert_eq!(
            snapshot.dataset.profile_nodes[0].state,
            ProfileNodeState::Active
        );
        assert_eq!(snapshot.dataset.profile_nodes[0].version, 1);
    }

    /// Verifies automatic purge rechecks ownership and the finite recycle deadline at apply time.
    /// 验证自动清除会在应用时重新检查所有权与有限回收期限。
    #[test]
    fn retention_rejects_premature_or_cross_scope_trash_purge() {
        let policy = RetentionPolicy::v1();
        let batch = retention_trash_batch(
            String::from("trash-batch-pending"),
            native_vmm_subject_id().expect("subject"),
            project_scope(3).expect("scope"),
            50,
            200,
            RetentionReasonCode::ColdTurnRecycle,
        );
        let mut snapshot = empty_snapshot(1);
        snapshot.retention_trash_batches.push(batch);
        let mut plan = retention_plan(2);
        plan.trash_purge
            .push(vulcan_memorymesh_core::PlannedTrashBatchRef {
                batch_id: String::from("trash-batch-pending"),
                reason: RetentionReasonCode::TrashExpiryPurge,
            });

        let error = apply_retention_plan(&mut snapshot, &plan, &policy, 100, "retention-purge")
            .expect_err("cross-scope premature purge must fail");

        assert!(error.contains("所有权或到期状态"));
        assert_eq!(snapshot.retention_trash_batches.len(), 1);
    }

    /// Verifies a shortened default cannot purge an existing batch before its frozen deadline.
    /// 验证缩短默认窗口不能在已有批次冻结截止时间之前清除该批次。
    #[test]
    fn retention_purge_preserves_deadline_when_default_is_shortened() {
        // The shortened default would have expired this batch, but its frozen deadline has not.
        // 缩短后的默认窗口会认为批次已到期，但批次冻结的截止时间尚未到达。
        let mut policy = RetentionPolicy::v1();
        policy.trash_retention_ms = 1;
        let mut snapshot = empty_snapshot(1);
        snapshot.retention_trash_batches.push(retention_trash_batch(
            String::from("long-lived-batch"),
            native_vmm_subject_id().expect("subject"),
            project_scope(2).expect("scope"),
            1,
            10_000_000,
            RetentionReasonCode::ColdTurnRecycle,
        ));
        let mut plan = retention_plan(2);
        plan.trash_purge
            .push(vulcan_memorymesh_core::PlannedTrashBatchRef {
                batch_id: String::from("long-lived-batch"),
                reason: RetentionReasonCode::TrashExpiryPurge,
            });
        let error = apply_retention_plan(&mut snapshot, &plan, &policy, 9_999_999, "shortened")
            .expect_err("frozen deadline has not elapsed");
        assert!(error.contains("所有权或到期状态"));
        assert_eq!(
            snapshot.retention_trash_batches[0].expires_at_ns,
            10_000_000
        );
        let result = apply_retention_plan(&mut snapshot, &plan, &policy, 10_000_000, "due")
            .expect("exact frozen deadline permits purge");
        assert_eq!(result.purged_trash_batches, 1);
        assert!(snapshot.retention_trash_batches.is_empty());
    }

    /// Verifies permanent trash purge also removes idempotency receipts for purged records.
    /// 验证永久清除回收站时也会删除引用已清除记录的幂等收据。
    #[test]
    fn retention_purge_removes_dangling_idempotency_outcomes() {
        let policy = RetentionPolicy::v1();
        let memory_id = "vmm-memory-purged";
        let mut batch = retention_trash_batch(
            String::from("trash-batch-expired"),
            native_vmm_subject_id().expect("subject"),
            project_scope(2).expect("scope"),
            1,
            100,
            RetentionReasonCode::ColdTurnRecycle,
        );
        batch.dataset.memories.push(memory(
            memory_id,
            2,
            "purged-session",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Session,
        ));
        let mut snapshot = empty_snapshot(1);
        snapshot.retention_trash_batches.push(batch);
        push_memory_ledger_entry(&mut snapshot, memory_id);
        let mut plan = retention_plan(2);
        plan.trash_purge
            .push(vulcan_memorymesh_core::PlannedTrashBatchRef {
                batch_id: String::from("trash-batch-expired"),
                reason: RetentionReasonCode::TrashExpiryPurge,
            });
        // The current default is much longer; only the batch's original deadline governs purge.
        // 当前默认窗口远长于该批次；清除仅由批次原始截止时间决定。
        let now_ns = 100;

        let result = apply_retention_plan(&mut snapshot, &plan, &policy, now_ns, "retention-purge")
            .expect("expired trash purge");

        assert_eq!(result.purged_trash_batches, 1);
        assert!(snapshot.retention_trash_batches.is_empty());
        assert!(snapshot.outcomes.is_empty());
        vulcan_memorymesh_core::InMemoryStorage::from_snapshot(snapshot)
            .expect("purged snapshot remains valid");
    }

    /// Verifies a Core plan cannot exceed any frozen per-pass policy bound.
    /// 验证 Core 计划不能超过任一冻结的单轮策略边界。
    #[test]
    fn retention_rejects_plan_families_above_policy_bounds() {
        let mut policy = RetentionPolicy::v1();
        policy.profile_expiry_limit = 1;
        let mut plan = retention_plan(2);
        for suffix in ["a", "b"] {
            plan.profile_expiry
                .push(vulcan_memorymesh_core::PlannedProfileNodeRef {
                    profile_node_id: ProfileNodeId::new(format!("vmm-profile-{suffix}"))
                        .expect("profile id"),
                    expected_subject_id: Some(native_vmm_subject_id().expect("subject")),
                    expected_scope_id: project_scope(2).expect("scope"),
                    expected_version: 1,
                    reason: RetentionReasonCode::ProfileExpiry,
                });
        }

        let error = apply_retention_plan(
            &mut empty_snapshot(1),
            &plan,
            &policy,
            100,
            "retention-bound",
        )
        .expect_err("oversized plan must fail");

        assert!(error.contains("单轮处理上限"));
    }

    /// Verifies native defaults preserve the exact external VMM P1 and stable protection floors.
    /// 验证原生默认值保留外置 VMM 准确的 P1 与 stable 保护阈值。
    #[test]
    fn retention_policy_v1_matches_external_protection_defaults() {
        let policy = RetentionPolicy::v1();

        assert_eq!(policy.protected_priority_floor, 60);
        assert_eq!(
            policy.protected_memory_level_floor,
            MemoryLevel::Stable as u32
        );
        assert!(policy.skip_protected_shared_memories);
    }

    /// Verifies the fast profile worker cannot trigger any unrelated retention phase.
    /// 验证快速画像工作器不能触发任何无关保留阶段。
    #[test]
    fn profile_expiry_policy_disables_every_unrelated_retention_family() {
        let policy = native_profile_expiry_policy(&RetentionPolicy::v1());
        assert_eq!(policy.profile_expiry_limit, 256);
        assert_eq!(policy.memory_recycle_limit, 0);
        assert_eq!(policy.cold_turn_scan_limit, 0);
        assert_eq!(policy.cold_turn_job_limit, 0);
        assert_eq!(policy.idle_session_limit, 0);
        assert_eq!(policy.vector_gc_limit, 0);
        assert_eq!(policy.trash_purge_limit, 0);
    }

    /// Verifies project-local cleanup failures remain individually attributable.
    /// 验证项目局部清洗失败保持可逐项归因。
    #[test]
    fn retention_project_failures_render_exact_project_identity() {
        let result = NativeVmmRetentionApplyResult {
            project_failures: vec![NativeVmmRetentionProjectFailure {
                project_id: 7,
                error: String::from("storage unavailable"),
            }],
            ..NativeVmmRetentionApplyResult::default()
        };

        assert_eq!(
            retention_project_failure_message(&result).as_deref(),
            Some("项目 7：storage unavailable")
        );
    }
}
