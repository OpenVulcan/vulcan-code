//! Native process-local VMM client implemented directly over the copied Core.
//! 直接基于复制 Core 实现的原生进程内 VMM 客户端。

use super::NativeVmmCoreRuntime;
#[cfg(test)]
#[path = "pending_analysis_acceptance_tests.rs"]
pub(super) mod pending_analysis_acceptance_tests;
#[path = "post_action.rs"]
mod post_action;
#[cfg(test)]
#[path = "real_model_acceptance_tests.rs"]
mod real_model_acceptance_tests;
#[path = "session_admission.rs"]
mod session_admission;
use super::analysis_queue::{recycled_analysis_turn, select_ordered_due_analysis_jobs};
use super::profile_bundle::NativeProfileBundleFormat;
use super::turn_details::resolve_turn_details;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::sync::{Mutex, Notify, RwLock};
use uuid::Uuid;
use vulcan_memorymesh_core::{
    CandidateQueryBoundary, CoreCommand, IdempotencyKey, MemoryCandidateScope, MemoryCategory,
    MemoryDeleteBatchCommand, MemoryDeleteBatchItem, MemoryLevel, MemoryReplacementScope,
    MemoryScopeClass, MemorySearchBatchCommand, MemoryWriteBatchCommand, MemoryWriteBatchItem,
    MutationIdentity, MutationSourceKind, OperationId, PostActionAnalyzePendingTurnCommand,
    PostActionInputKind, PostActionIntakeCommand, PreCheckCommand, PreCheckTurnContentType,
    PreCheckTurnContext, ProfileBundle, ProfileInstructionCommand, ProfileLevel, ProfileNode,
    ProfileSourceKind, ProfileView, ProfileViewSection, RecallMode, Role, ScopeAccess, ScopeId,
    SectionRole, SessionRecallMode, SessionSourceId, SubjectId, TimelineItem, Turn, TurnId,
    TurnStatus, VectorSpaceId, derive_mutation_identity, estimate_text_tokens,
};
use vulcan_vmm::{
    ApplyProfileInstructionRequest, ApplyProfileInstructionResponse, ChatCompactRequest,
    ChatCompactResponse, ContextItem, DEFAULT_NATIVE_USER_ID, DeleteMemoriesRequest,
    DeleteMemoriesResponse, DeleteProjectRequest, DeleteProjectResponse, DeleteUserRequest,
    DeleteUserResponse, EnsureProjectRequest, EnsureProjectResponse, GetProfileBundleRequest,
    GetProfileBundleResponse, GetProfileNodesRequest, GetProfileNodesResponse,
    GetTurnDetailsRequest, GetTurnDetailsResponse, HealthzResponse, ListProjectsResponse,
    ListUsersResponse, MemorySearchGroupResult, MemorySearchHit, MigrateProjectRequest,
    MigrateProjectResponse, PostActionRequest, PostActionResponse, PostActionTimelineType,
    PreCheckRecallMode, PreCheckRequest, PreCheckResponse, ProfileBundleMode, ProfileNodeEntry,
    ProfileNodeSourceKind, ProfileTarget, ResolveProjectRequest, ResolveProjectResponse,
    ResolveUserRequest, ResolveUserResponse, RetiredProfileNodeEntry, SearchMemoryEventsRequest,
    SearchMemoryEventsResponse, SessionMemoryStatus, TurnDetailEntry, VmmClient, VmmClientError,
    WriteMemoriesRequest, WriteMemoriesResponse, WriteMemoryResultItem,
};
use vulcan_workspace_store::vmm_data_plane::{
    NativeVmmPendingAnalysisJob, VMM_PENDING_ANALYSIS_BYPASSED, VMM_PENDING_ANALYSIS_COMPLETED,
    VMM_PENDING_ANALYSIS_CORRUPTED, VMM_PENDING_ANALYSIS_ERROR_CHAR_LIMIT,
    VMM_PENDING_ANALYSIS_INTAKING, VMM_PENDING_ANALYSIS_READY, VmmDataPlaneHandle,
};

/// Periodic durable analysis queue scan interval.
/// 耐久分析队列的周期扫描间隔。
const PENDING_ANALYSIS_SCAN_INTERVAL: Duration = Duration::from_secs(30);

/// Maximum jobs executed by one queue drain pass.
/// 单次队列排空最多执行的作业数。
const PENDING_ANALYSIS_BATCH_LIMIT: u32 = 128;

/// Maximum retry delay for one failed background analysis job.
/// 单个后台分析作业失败后的最大重试延迟。
const PENDING_ANALYSIS_MAX_RETRY_DELAY_MS: i64 = 5 * 60 * 1_000;

/// Matches the external VMM policy that releases one turn after five failed analyses.
/// 与外置 VMM 在五次分析失败后放行一个回合的策略保持一致。
const PENDING_ANALYSIS_FAILURE_PASS_THRESHOLD: u32 = 5;

/// Classifies a failed pending-analysis attempt without discarding its recovery semantics.
/// 对失败的待分析尝试分类，同时保留其恢复语义。
#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingAnalysisJobError {
    /// The durable job payload or identity is unreadable and must be isolated.
    /// 耐久作业载荷或身份不可读取，必须隔离。
    Corrupted(String),
    /// The provider or Core analysis failed and participates in the explicit failure-pass policy.
    /// Provider 或 Core 分析失败，并参与显式失败放行策略。
    Analysis {
        /// Bounded-at-persistence diagnostic text.
        /// 在持久化时才限制长度的诊断文本。
        diagnostic: String,
        /// Whether Core reports that semantic side effects may already exist.
        /// Core 是否报告语义副作用可能已经存在。
        outcome_uncertain: bool,
    },
    /// Infrastructure failed outside semantic analysis and should keep retrying without bypass.
    /// 语义分析之外的基础设施失败，应持续重试而不触发放行。
    Retryable(String),
}

impl PendingAnalysisJobError {
    /// Returns the exact developer diagnostic carried by this failure.
    /// 返回该失败携带的准确开发诊断。
    #[must_use]
    fn diagnostic(&self) -> &str {
        match self {
            Self::Corrupted(diagnostic)
            | Self::Retryable(diagnostic)
            | Self::Analysis { diagnostic, .. } => diagnostic,
        }
    }
}

/// Stores the exact replayable semantics of one accepted post-action request.
/// 保存一条已接受 PostAction 请求的准确可重放语义。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct NativePendingPostActionPayload {
    /// Exact originating host project for caller-owned policy resolution, independent of memory ownership.
    /// 用于调用方策略解析的准确发起宿主项目，独立于记忆归属。
    host_project_id: String,
    /// Exact durable intake command.
    /// 准确耐久登记命令。
    intake: PostActionIntakeCommand,
    /// Exact analysis fields that do not depend on the committed turn identity.
    /// 不依赖已提交回合身份的准确分析字段。
    analysis: NativePendingAnalysisTemplate,
}

/// Stores one pending-turn analysis command before its durable turn identity exists.
/// 在耐久回合身份产生前保存一条待处理回合分析命令。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct NativePendingAnalysisTemplate {
    /// Replacement visibility and supersede authority frozen at intake time.
    /// 在登记时冻结的替代可见性与替代授权。
    replacement_scope: MemoryReplacementScope,
    /// Stable subject identity.
    /// 稳定主体身份。
    subject_id: SubjectId,
    /// Exact write and visible scopes frozen at intake time.
    /// 在登记时冻结的准确写入与可见作用域。
    scope_access: ScopeAccess,
    /// Exact project and user profile role bindings.
    /// 准确项目与用户画像角色绑定。
    profile_sections: Vec<ProfileViewSection>,
    /// Stable host session identity.
    /// 稳定宿主会话身份。
    session_source_id: SessionSourceId,
    /// Exact logical embedding vector space.
    /// 准确逻辑嵌入向量空间。
    vector_space_id: VectorSpaceId,
    /// Explicit analysis generation.
    /// 显式分析代次。
    analysis_generation: u64,
    /// Stable host background mutation identity.
    /// 稳定宿主后台变更身份。
    mutation_identity: MutationIdentity,
}

impl NativePendingAnalysisTemplate {
    /// Binds the durably committed turn identity into the exact Core analysis command.
    /// 将耐久提交的回合身份绑定到准确 Core 分析命令。
    ///
    /// # Parameters
    /// 参数
    /// `turn_id` is the identity atomically captured from the intake commit receipt.
    /// `turn_id` 是从登记提交收据中原子捕获的身份。
    /// The cutoff is the execution clock; the start is read from the committed owner session.
    /// 截止为执行时钟；起点从已提交的归属会话读取。
    /// `failed_attempt_count` is the durable queue counter; each retry gets its own generation and child identity.
    /// `failed_attempt_count` 是耐久队列计数；每次重试获得独立代次及子身份。
    ///
    /// # Returns
    /// 返回值
    /// A complete analysis command, or an explicit generation/identity construction error.
    /// 返回完整分析命令，或明确的代次及身份构造错误。
    fn into_command(
        self,
        turn_id: TurnId,
        analysis_cutoff_ns: i64,
        direct_write_observed_after_ns: i64,
        failed_attempt_count: u32,
    ) -> Result<PostActionAnalyzePendingTurnCommand, VmmClientError> {
        // The first attempt already writes its identity before L1; a retry must not reuse that storage key.
        // 首次尝试在 L1 前已经写入身份；重试绝不能复用该存储键。
        let analysis_generation = self
            .analysis_generation
            .checked_add(u64::from(failed_attempt_count))
            .ok_or_else(|| native_runtime_error("native VMM analysis generation overflow"))?;
        // Observation belongs to this analysis admission, not the earlier queue-intake event.
        // 观察时间属于本次分析准入，而不是更早的队列登记事件。
        // Derive a deterministic child from the durable root; the driver fingerprints the complete command.
        // 从耐久根身份派生确定性子身份；驱动会对完整命令计算规范指纹。
        let mut mutation_identity = derive_mutation_identity(
            &self.mutation_identity,
            &format!("attempt-{analysis_generation}"),
        )
        .map_err(core_client_error)?;
        mutation_identity.observed_at_ns = analysis_cutoff_ns;
        Ok(PostActionAnalyzePendingTurnCommand {
            replacement_scope: self.replacement_scope,
            subject_id: self.subject_id,
            scope_access: self.scope_access,
            profile_sections: self.profile_sections,
            session_source_id: self.session_source_id,
            vector_space_id: self.vector_space_id,
            turn_id,
            analysis_generation,
            analysis_cutoff_ns,
            direct_write_observed_after_ns,
            mutation_identity,
        })
    }
}

/// Reports whether one active durable analysis job writes to the requested project scope.
/// 判断一条活动耐久分析作业是否写入指定项目作用域。
///
/// # Parameters
/// 参数
/// `job` is the authoritative queue row and `scope` is the maintenance target.
/// `job` 是权威队列记录，`scope` 是维护目标。
///
/// # Returns
/// 返回值
/// `true` only when both frozen intake and analysis ownership target the supplied scope.
/// 仅当冻结的登记与分析归属均指向给定作用域时返回 `true`。
///
/// # Errors
/// 错误
/// Returns an integrity error for malformed payloads or inconsistent frozen write scopes.
/// 当载荷损坏或冻结写入作用域不一致时返回完整性错误。
pub(super) fn pending_analysis_job_targets_scope(
    job: &NativeVmmPendingAnalysisJob,
    scope: &ScopeId,
) -> Result<bool, String> {
    let payload: NativePendingPostActionPayload = serde_json::from_str(&job.payload_json)
        .map_err(|error| format!("解析待分析作业 `{}` 失败：{error}", job.operation_id))?;
    if payload.intake.scope_access.write_scope != payload.analysis.scope_access.write_scope {
        return Err(format!(
            "待分析作业 `{}` 的登记与分析写入作用域不一致",
            job.operation_id
        ));
    }
    Ok(payload.intake.scope_access.write_scope == *scope)
}

/// Direct Rust client for native VMM functional calls.
/// 原生 VMM 功能调用使用的直接 Rust 客户端。
pub(crate) struct NativeVmmClient {
    /// Shared Core runtime owned by the VulcanCode service composition root.
    /// 由 VulcanCode 服务组合根持有的共享 Core 运行时。
    runtime: Arc<NativeVmmCoreRuntime>,
    /// Host-owned project bindings and linked-query relationships.
    /// 宿主持有的项目绑定与连带查询关系。
    data_plane: VmmDataPlaneHandle,
    /// Shared reader side of the native project maintenance barrier.
    /// 原生项目维护屏障的共享读取侧。
    operation_gate: Arc<RwLock<()>>,
    /// Enables durable analysis only while native VMM is the active global backend.
    /// 仅在原生 VMM 是全局活动后端时启用耐久分析。
    native_backend_enabled: Arc<AtomicBool>,
    /// Wakes the host worker after an intake atomically binds a pending turn.
    /// 在登记原子绑定待处理回合后唤醒宿主 Worker。
    pending_analysis_notify: Arc<Notify>,
    /// Prevents overlapping periodic and notification-driven queue drains.
    /// 防止周期扫描与通知驱动的队列排空重叠。
    pending_analysis_drain_gate: Mutex<()>,
    /// Guarantees the process starts exactly one analysis worker.
    /// 保证进程只启动一个分析 Worker。
    pending_analysis_worker_started: AtomicBool,
}

impl NativeVmmClient {
    /// Creates one direct client over an initialized native Core runtime.
    /// 基于已初始化的原生 Core 运行时创建一个直接客户端。
    #[must_use]
    pub(crate) fn new(
        runtime: Arc<NativeVmmCoreRuntime>,
        data_plane: VmmDataPlaneHandle,
        operation_gate: Arc<RwLock<()>>,
        native_backend_enabled: Arc<AtomicBool>,
        pending_analysis_notify: Arc<Notify>,
    ) -> Self {
        Self {
            runtime,
            data_plane,
            operation_gate,
            native_backend_enabled,
            pending_analysis_notify,
            pending_analysis_drain_gate: Mutex::new(()),
            pending_analysis_worker_started: AtomicBool::new(false),
        }
    }

    /// Starts the single durable post-action analysis worker on the host runtime.
    /// 在宿主运行时上启动唯一耐久 PostAction 分析 Worker。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` is the long-lived VulcanCode Tokio runtime handle.
    /// `runtime` 是长生命周期的 VulcanCode Tokio 运行时句柄。
    pub(crate) fn start_pending_analysis_worker(
        self: &Arc<Self>,
        runtime: &tokio::runtime::Handle,
    ) {
        if self
            .pending_analysis_worker_started
            .swap(true, Ordering::AcqRel)
        {
            return;
        }
        let client = Arc::clone(self);
        runtime.spawn(async move {
            client.run_pending_analysis_worker().await;
        });
    }

    /// Resolves the unique write scope and ordered linked read scopes for one host project.
    /// 为一个宿主项目解析唯一写入作用域与有序连带只读作用域。
    async fn project_access(
        &self,
        host_project_id: &str,
        fallback_vmm_project_id: u64,
    ) -> Result<ScopeAccess, VmmClientError> {
        if host_project_id.trim().is_empty() {
            return owner_only_project_access(fallback_vmm_project_id);
        }
        let access = self
            .data_plane
            .resolve_project_access(host_project_id)
            .await
            .map_err(|error| VmmClientError::new("native_project_binding", error))?;
        // The host binding is authoritative in native mode. The numeric project id in the shared
        // VMM request envelope is retained for external gRPC compatibility and fallback calls only.
        // 原生模式以宿主绑定为权威；共享 VMM 请求封套中的数字项目 ID 仅保留给外置 gRPC 兼容与兜底调用。
        let write_scope = ScopeId::new(access.owner.scope_id).map_err(identifier_error)?;
        let mut visible_scopes = Vec::with_capacity(1 + access.linked.len());
        visible_scopes.push(write_scope.clone());
        for linked in access.linked {
            visible_scopes.push(ScopeId::new(linked.scope_id).map_err(identifier_error)?);
        }
        let access = ScopeAccess {
            write_scope,
            visible_scopes,
        };
        access.validate().map_err(|error| {
            VmmClientError::new(
                "native_project_binding",
                format!("native VMM project access is invalid: {error}"),
            )
        })?;
        Ok(access)
    }

    /// Continuously drains due durable analysis jobs after synchronous startup preparation.
    /// 在同步启动准备完成后持续排空已到期的耐久分析作业。
    async fn run_pending_analysis_worker(self: Arc<Self>) {
        loop {
            if !self.native_backend_enabled.load(Ordering::Acquire) {
                self.pending_analysis_notify.notified().await;
                continue;
            }
            match self.drain_due_pending_analysis_jobs().await {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => {
                    tracing::warn!("native VMM pending analysis drain failed: {error}");
                }
            }
            tokio::select! {
                () = self.pending_analysis_notify.notified() => {}
                () = tokio::time::sleep(PENDING_ANALYSIS_SCAN_INTERVAL) => {}
            }
        }
    }

    /// Prepares the durable analysis queue before the service exposes native VMM calls.
    /// 在服务暴露原生 VMM 调用前准备耐久分析队列。
    ///
    /// # Returns
    /// 返回值
    /// Success after Core recovery binds every committed intake and unbound leftovers are deleted.
    /// Core 恢复绑定全部已提交登记且未绑定遗留记录被删除后返回成功。
    ///
    /// # Errors
    /// Returns the exact Core recovery or queue-cleanup failure and prevents service exposure.
    /// 返回准确 Core 恢复或队列清理失败，并阻止服务暴露。
    pub(crate) async fn prepare_pending_analysis_worker(&self) -> Result<(), String> {
        self.runtime.recover_storage_mutations().await?;
        self.data_plane
            .delete_abandoned_pending_analysis_intakes()
            .await
    }

    /// Drains one bounded batch of due analysis jobs without overlapping another drain.
    /// 在不与其他排空过程重叠的情况下处理一批有界的已到期分析作业。
    ///
    /// # Returns
    /// 返回值
    /// Returns whether any job was attempted and another fresh-head pass should run immediately.
    /// 返回是否尝试了作业，以及是否应立即重新扫描最新队首。
    ///
    /// # Errors
    /// 错误
    /// Returns a data-plane clock, queue read, or retry-state persistence failure.
    /// 数据平面时钟、队列读取或重试状态持久化失败时返回错误。
    async fn drain_due_pending_analysis_jobs(&self) -> Result<bool, String> {
        // The drain gate serializes worker passes while individual sessions retain separate heads.
        // 排空门串行化工作器扫描，同时各会话仍保留独立队首。
        let _gate = self.pending_analysis_drain_gate.lock().await;
        // Read all active queue heads before limiting execution; overdue successors must not hide
        // runnable work in another session behind a full database page.
        // 在限制执行数量前读取全部活动队首；不能让到期后继填满数据库分页，遮挡其他会话的可运行作业。
        let active_jobs = self.data_plane.list_active_pending_analysis_jobs().await?;
        // snapshot supplies canonical ownership and current pending status for this selection only.
        // snapshot 仅为本次选择提供规范归属及当前待处理状态。
        let snapshot = self.runtime.storage_snapshot()?;
        // jobs owns the bounded executable subset, not references into the storage snapshot.
        // jobs 持有有界可执行子集，而不是存储快照中的引用。
        let jobs = select_ordered_due_analysis_jobs(
            active_jobs,
            &snapshot.dataset.turns,
            &snapshot.retention_trash_batches,
            now_unix_ms()?,
            PENDING_ANALYSIS_BATCH_LIMIT as usize,
        )?;
        // Release copied memory/vector data before waiting for any model execution.
        // 在等待任何模型执行前释放复制的记忆与向量数据。
        drop(snapshot);
        // A completed or terminalized head can release another turn even when this page is short.
        // 即使本页未满，完成或终态化的队首仍可能放行后续回合。
        let attempted_any = !jobs.is_empty();
        for job in jobs {
            let Err(error) = self.execute_pending_analysis_job(&job).await else {
                continue;
            };
            let diagnostic = bounded_diagnostic(error.diagnostic());
            let terminal_status = pending_analysis_terminal_status(&error, job.attempt_count);
            if let Some(status) = terminal_status {
                // Only a committed terminal state may be reported; concurrent recycling pauses instead.
                // 只能报告已提交的终态；并发回收应暂停处理。
                let finalized = match self
                    .finalize_failed_pending_analysis_job(&job, status, &diagnostic)
                    .await
                {
                    Ok(Some(finalized)) => finalized,
                    Ok(None) => continue,
                    Err(finalize_error) => {
                        let combined = bounded_diagnostic(
                            format!("{diagnostic}; terminalization failed: {finalize_error}")
                                .as_str(),
                        );
                        self.reschedule_pending_analysis_job(&job, &combined)
                            .await?;
                        tracing::warn!(
                            "native VMM post-action failure terminalization failed for operation {}: {}",
                            job.operation_id,
                            combined
                        );
                        continue;
                    }
                };
                tracing::warn!(
                    "native VMM post-action analysis reached {:?} for operation {}: {}",
                    finalized,
                    job.operation_id,
                    diagnostic
                );
                continue;
            }
            self.reschedule_pending_analysis_job(&job, &diagnostic)
                .await?;
            tracing::warn!(
                "native VMM post-action analysis failed for operation {}: {}",
                job.operation_id,
                diagnostic
            );
        }
        Ok(attempted_any)
    }

    /// Checks durable queue presence while the caller holds the maintenance operation barrier.
    /// 调用方持有维护操作屏障时，检查耐久队列是否仍存在。
    /// Takes the previously selected job; returns false after removal, rejecting reused identities.
    /// 接收先前选中的作业；移除后返回假，身份被复用时拒绝。
    async fn pending_analysis_job_is_present(
        &self,
        job: &NativeVmmPendingAnalysisJob,
    ) -> Result<bool, String> {
        // The queue row, not an absent turn, authorizes cancellation of a stale selected operation.
        // 以队列行而非回合缺失作为取消过时所选操作的依据。
        let Some(current) = self
            .data_plane
            .load_pending_analysis_job(&job.operation_id)
            .await?
        else {
            return Ok(false);
        };
        if current.request_fingerprint != job.request_fingerprint {
            return Err(format!(
                "待分析作业 `{}` 的准确请求身份已变化。",
                job.operation_id
            ));
        }
        Ok(true)
    }

    /// Reschedules one non-terminal pending-analysis failure with bounded exponential backoff.
    /// 使用有界指数退避重新调度一条非终态待分析失败。
    ///
    /// # Parameters
    /// 参数
    /// `job` is the last durable row and `diagnostic` is already bounded for persistence.
    /// `job` 是最近的耐久记录，`diagnostic` 已按持久化限制完成截断。
    ///
    /// # Errors
    /// 错误
    /// Returns the exact clock or data-plane update failure.
    /// 返回准确时钟或数据平面更新失败。
    async fn reschedule_pending_analysis_job(
        &self,
        job: &NativeVmmPendingAnalysisJob,
        diagnostic: &str,
    ) -> Result<(), String> {
        // Recycling may win after analysis returns an error; preserve the suspended job unchanged.
        // 分析返回错误后回收仍可能先完成；保持已暂停作业不变。
        let _operation_guard = self.operation_gate.read().await;
        if !self.pending_analysis_job_is_present(job).await? {
            return Ok(());
        }
        // Read under the same maintenance barrier as the queue update, not from the earlier pass.
        // 在与队列更新相同的维护屏障内读取，不使用较早扫描的快照。
        let snapshot = self.runtime.storage_snapshot()?;
        if job.turn_id.as_deref().is_some_and(|id| {
            recycled_analysis_turn(&snapshot.retention_trash_batches, id).is_some()
        }) {
            return Ok(());
        }
        drop(snapshot);
        let next_attempt_at_unix_ms = now_unix_ms()?.saturating_add(
            pending_analysis_retry_delay_ms(job.attempt_count.saturating_add(1)),
        );
        self.data_plane
            .record_pending_analysis_failure(&job.operation_id, next_attempt_at_unix_ms, diagnostic)
            .await
    }

    /// Moves one failed pending turn and its queue job into a durable terminal state.
    /// 将一个失败待处理回合及其队列作业迁移到耐久终态。
    ///
    /// # Parameters
    /// 参数
    /// `job` owns the exact turn, `requested_status` is bypassed or corrupted, and `diagnostic`
    /// is the bounded reason retained after the original conversation payload is erased.
    /// `job` 拥有准确回合，`requested_status` 为已放行或已损坏，`diagnostic`
    /// 是擦除原始会话载荷后保留的有界原因。
    /// Returns the actual finalized status, or none when concurrent recycling suspended the job.
    /// 返回实际终结状态；并发回收暂停作业时返回空。
    ///
    /// # Errors
    /// 错误
    /// Returns an identity, snapshot-transition, or queue-finalization failure.
    /// 身份、快照迁移或队列终结失败时返回错误。
    async fn finalize_failed_pending_analysis_job(
        &self,
        job: &NativeVmmPendingAnalysisJob,
        requested_status: TurnStatus,
        diagnostic: &str,
    ) -> Result<Option<TurnStatus>, String> {
        if !matches!(
            requested_status,
            TurnStatus::AnalysisBypassed | TurnStatus::Corrupted
        ) {
            return Err(String::from("待分析作业请求了不受支持的失败终态"));
        }
        let turn_id = TurnId::new(
            job.turn_id
                .clone()
                .ok_or_else(|| format!("待分析作业 `{}` 缺少回合身份", job.operation_id))?,
        )
        .map_err(|error| format!("待分析作业 `{}` 回合身份无效：{error}", job.operation_id))?;
        // A write guard prevents normal Core commands and maintenance from changing the exact turn
        // between the previewed version and the host-owned failure-policy transition.
        // 写守卫防止普通 Core 命令与维护过程在预检版本和宿主持有失败策略迁移之间改变准确回合。
        let _operation_guard = self.operation_gate.write().await;
        if !self.pending_analysis_job_is_present(job).await? {
            return Ok(None);
        }
        let current_snapshot = self.runtime.storage_snapshot()?;
        // A concurrent recycle is a suspension, not permission to corrupt or acknowledge the job.
        // 并发回收意味着暂停，不是将作业标损或确认完成的许可。
        if recycled_analysis_turn(&current_snapshot.retention_trash_batches, turn_id.as_str())
            .is_some()
        {
            return Ok(None);
        }
        let current_status = current_snapshot
            .dataset
            .turns
            .iter()
            .find(|turn| turn.id == turn_id)
            .map(|turn| turn.status)
            .ok_or_else(|| {
                format!(
                    "待分析作业 `{}` 引用的回合 `{}` 不存在",
                    job.operation_id, turn_id
                )
            })?;
        if current_status == TurnStatus::Completed {
            return self
                .data_plane
                .complete_pending_analysis_job(&job.operation_id, &job.request_fingerprint)
                .await
                .map(|()| Some(TurnStatus::Completed));
        }
        let storage_version = current_snapshot.storage_version;
        let transition_operation = derived_operation_id(
            &job.operation_id,
            match requested_status {
                TurnStatus::AnalysisBypassed => "analysis-bypassed",
                TurnStatus::Corrupted => "analysis-corrupted",
                _ => unreachable!("validated terminal status"),
            },
        );
        let final_status = self
            .runtime
            .transform_storage_snapshot(
                transition_operation.as_str(),
                "pending-analysis-terminal",
                storage_version,
                |snapshot| {
                    transition_pending_turn_status(
                        snapshot,
                        &turn_id,
                        requested_status,
                        &job.operation_id,
                    )
                },
            )
            .await?;
        match final_status {
            TurnStatus::Completed => {
                self.data_plane
                    .complete_pending_analysis_job(&job.operation_id, &job.request_fingerprint)
                    .await
            }
            TurnStatus::AnalysisBypassed => {
                self.data_plane
                    .finalize_failed_pending_analysis_job(
                        &job.operation_id,
                        &job.request_fingerprint,
                        VMM_PENDING_ANALYSIS_BYPASSED,
                        diagnostic,
                    )
                    .await
            }
            TurnStatus::Corrupted => {
                self.data_plane
                    .finalize_failed_pending_analysis_job(
                        &job.operation_id,
                        &job.request_fingerprint,
                        VMM_PENDING_ANALYSIS_CORRUPTED,
                        diagnostic,
                    )
                    .await
            }
            TurnStatus::Pending | TurnStatus::ProcessedUnknown => Err(format!(
                "待分析作业 `{}` 未收敛到可终结状态",
                job.operation_id
            )),
        }
        .map(|()| Some(final_status))
    }

    /// Executes and completes one exact durable pending analysis job.
    /// 执行并完成一条准确的耐久待分析作业。
    ///
    /// # Parameters
    /// 参数
    /// `job` is the authoritative ready queue row loaded from SQLite.
    /// `job` 是从 SQLite 加载的权威就绪队列记录。
    ///
    /// # Errors
    /// Returns a payload-integrity, Core analysis, result-shape, or queue-completion failure.
    /// 载荷完整性、Core 分析、结果形态或队列完成失败时返回错误。
    async fn execute_pending_analysis_job(
        &self,
        job: &NativeVmmPendingAnalysisJob,
    ) -> Result<(), PendingAnalysisJobError> {
        // The read guard keeps maintenance from moving or clearing the referenced turn until the
        // complete queued analysis and queue acknowledgement are durable.
        // 读取守卫保证完整队列分析及其确认耐久化前，维护操作不能迁移或清空所引用的回合。
        let _operation_guard = self.operation_gate.read().await;
        if !self
            .pending_analysis_job_is_present(job)
            .await
            .map_err(PendingAnalysisJobError::Retryable)?
        {
            return Ok(());
        }
        if job.state != VMM_PENDING_ANALYSIS_READY {
            return Err(PendingAnalysisJobError::Corrupted(format!(
                "待分析作业 `{}` 未处于 ready 状态",
                job.operation_id
            )));
        }
        let mut payload: NativePendingPostActionPayload = serde_json::from_str(&job.payload_json)
            .map_err(|error| {
            PendingAnalysisJobError::Corrupted(format!(
                "解析待分析作业 `{}` 失败：{error}",
                job.operation_id
            ))
        })?;
        if payload.intake.vector_space_id != payload.analysis.vector_space_id
            || payload.intake.subject_id != payload.analysis.subject_id
            || payload.intake.scope_access.write_scope != payload.analysis.scope_access.write_scope
            || payload.intake.session_source_id != payload.analysis.session_source_id
        {
            return Err(PendingAnalysisJobError::Corrupted(format!(
                "待分析作业 `{}` 的登记与分析归属或向量代次不一致",
                job.operation_id
            )));
        }
        let turn_id = TurnId::new(job.turn_id.clone().ok_or_else(|| {
            PendingAnalysisJobError::Corrupted(format!(
                "待分析作业 `{}` 缺少回合身份",
                job.operation_id
            ))
        })?)
        .map_err(|error| {
            PendingAnalysisJobError::Corrupted(format!(
                "待分析作业 `{}` 回合身份无效：{error}",
                job.operation_id
            ))
        })?;
        // Resolve the successful observation boundary only now, after earlier session heads finished.
        // 仅在此时读取成功观察边界，此时同会话较早队首已经处理完成。
        let snapshot = self
            .runtime
            .storage_snapshot()
            .map_err(PendingAnalysisJobError::Retryable)?;
        // Recheck after acquiring the operation guard: the selection snapshot may predate recycling.
        // 获取操作守卫后复查：选择快照可能早于回收提交。
        if let Some(turn) =
            recycled_analysis_turn(&snapshot.retention_trash_batches, turn_id.as_str())
        {
            if turn.subject_id != payload.analysis.subject_id
                || turn.scope_id != payload.analysis.scope_access.write_scope
                || turn.session_source_id != payload.analysis.session_source_id
            {
                return Err(PendingAnalysisJobError::Corrupted(
                    "待分析作业与已回收回合归属不一致".to_owned(),
                ));
            }
            return Ok(());
        }
        // turn is the committed authority for this job's subject/project/session identity.
        // turn 是该作业主体/项目/会话身份的已提交权威来源。
        let turn = snapshot
            .dataset
            .turns
            .iter()
            .find(|turn| turn.id == turn_id)
            .ok_or_else(|| {
                PendingAnalysisJobError::Corrupted("待分析作业引用的回合不存在".to_owned())
            })?;
        if turn.subject_id != payload.analysis.subject_id
            || turn.scope_id != payload.analysis.scope_access.write_scope
            || turn.session_source_id != payload.analysis.session_source_id
        {
            return Err(PendingAnalysisJobError::Corrupted(
                "待分析作业与已登记回合归属不一致".to_owned(),
            ));
        }
        // A committed successful turn needs only queue acknowledgement, never a second extraction.
        // 已成功提交的回合仅需确认队列，绝不能再次提取。
        if turn.status == TurnStatus::Completed {
            drop(snapshot);
            return self
                .data_plane
                .complete_pending_analysis_job(&job.operation_id, &job.request_fingerprint)
                .await
                .map_err(PendingAnalysisJobError::Retryable);
        }
        // session metadata is mandatory; missing metadata must not silently reopen the zero window.
        // 会话元数据是必需的；缺失时绝不能静默重开零起点窗口。
        let session = snapshot
            .memory_sessions
            .iter()
            .find(|session| {
                session.subject_id == turn.subject_id
                    && session.scope_id == turn.scope_id
                    && session.session_source_id == turn.session_source_id
            })
            .ok_or_else(|| {
                PendingAnalysisJobError::Corrupted("待分析回合缺少权威记忆会话记录".to_owned())
            })?;
        // observed_after_ns survives retries and advances only with successful derived storage.
        // observed_after_ns 跨重试保留，仅随成功派生存储提交推进。
        let observed_after_ns = session.last_extraction_observed_at_ns;
        // cutoff is captured at analysis execution, not when a possibly long queue was populated.
        // cutoff 在分析执行时捕获，而不是在可能积压很久的队列入队时捕获。
        let cutoff =
            now_ns().map_err(|error| PendingAnalysisJobError::Retryable(error.to_string()))?;
        drop(snapshot);
        // A live pending turn owns semantic input, not a provider route. Resolve only after the
        // recycle check so paused work never contacts AI or consumes a route failure attempt.
        // 活动待处理回合持有语义输入而非供应商路由；回收检查后才解析，确保暂停作业
        // 不接触 AI，也不因路由错误消耗尝试次数。
        payload.analysis.vector_space_id =
            self.runtime
                .current_vector_space_id()
                .await
                .map_err(|error| {
                    PendingAnalysisJobError::Retryable(format!(
                        "待分析作业 `{}` 无法解析当前向量代次：{error}",
                        job.operation_id
                    ))
                })?;
        // Keep the durable queue root unchanged while constructing this actual analysis attempt.
        // 保持耐久队列根身份不变，同时构造本次实际分析尝试。
        let command = payload
            .analysis
            .into_command(turn_id, cutoff, observed_after_ns, job.attempt_count)
            .map_err(|error| PendingAnalysisJobError::Corrupted(error.to_string()))?;
        let result = self
            .runtime
            .run_for_host_project(
                CoreCommand::PostActionAnalyzePendingTurn(command),
                &payload.host_project_id,
            )
            .await
            .map_err(|error| PendingAnalysisJobError::Analysis {
                outcome_uncertain: error.side_effect_possible,
                diagnostic: error.to_string(),
            })?;
        if !matches!(
            result,
            vulcan_memorymesh_core::CommandResult::PostActionAnalysis(_)
        ) {
            return Err(PendingAnalysisJobError::Analysis {
                diagnostic: String::from("Core 未返回 PostActionAnalysis 结果"),
                outcome_uncertain: false,
            });
        }
        self.data_plane
            .complete_pending_analysis_job(&job.operation_id, &job.request_fingerprint)
            .await
            .map_err(PendingAnalysisJobError::Retryable)
    }
}

#[async_trait]
impl VmmClient for NativeVmmClient {
    /// Resolves actual native cache ownership under the existing binding operation barrier.
    /// 在既有绑定操作屏障内解析实际原生缓存归属。
    /// Takes the host project and native envelope IDs; returns the persisted owner's project identity.
    /// 接收宿主项目及原生封套 ID；返回持久归属的项目身份。
    async fn memory_source(
        &self,
        host_project_id: &str,
        user_id: u64,
        project_id: u64,
    ) -> Result<vulcan_vmm::VmmMemorySource, VmmClientError> {
        // Freeze binding resolution without initializing a model route or creating a session.
        // 冻结绑定解析，不初始化模型路由或创建会话。
        let _operation_guard = self.operation_gate.read().await;
        subject_id(user_id)?;
        // Only the resolved write scope owns project profiles; linked recall scopes do not own writes.
        // 只有解析后的写入作用域拥有项目画像；连带召回作用域不拥有写入。
        let access = self.project_access(host_project_id, project_id).await?;
        vulcan_vmm::VmmMemorySource::native(parse_core_numeric_id(
            &access.write_scope,
            "vmm-project:",
        )?)
    }

    /// Reports process-local readiness without probing a network endpoint.
    /// 在不探测网络端点的情况下报告进程内就绪状态。
    async fn healthz(&self) -> Result<HealthzResponse, VmmClientError> {
        let _operation_guard = self.operation_gate.read().await;
        self.runtime
            .current_vector_space_id()
            .await
            .map_err(native_runtime_error)?;
        Ok(HealthzResponse {
            status: String::from("native-ready"),
        })
    }

    /// Rejects the external hierarchical project API in native one-level mode.
    /// 在原生单层模式中拒绝外部层级项目 API。
    async fn list_projects(&self) -> Result<ListProjectsResponse, VmmClientError> {
        Err(native_management_error())
    }

    /// Rejects the external hierarchical project resolver in native mode.
    /// 在原生模式中拒绝外部层级项目解析器。
    async fn resolve_project(
        &self,
        _request: ResolveProjectRequest,
    ) -> Result<ResolveProjectResponse, VmmClientError> {
        Err(native_management_error())
    }

    /// Rejects the external hierarchical project creator in native mode.
    /// 在原生模式中拒绝外部层级项目创建器。
    async fn ensure_project(
        &self,
        _request: EnsureProjectRequest,
    ) -> Result<EnsureProjectResponse, VmmClientError> {
        Err(native_management_error())
    }

    /// Rejects the external hierarchical project deletion API in native mode.
    /// 在原生模式中拒绝外部层级项目删除 API。
    async fn delete_project(
        &self,
        _request: DeleteProjectRequest,
    ) -> Result<DeleteProjectResponse, VmmClientError> {
        Err(native_management_error())
    }

    /// Rejects the external migration API because native migration is host-managed.
    /// 拒绝外部迁移 API，因为原生迁移由宿主管理。
    async fn migrate_project(
        &self,
        _request: MigrateProjectRequest,
    ) -> Result<MigrateProjectResponse, VmmClientError> {
        Err(native_management_error())
    }

    /// Rejects the external VMM user catalog in native mode.
    /// 在原生模式中拒绝外部 VMM 用户目录。
    async fn list_users(&self) -> Result<ListUsersResponse, VmmClientError> {
        Err(native_management_error())
    }

    /// Rejects the external VMM user resolver in native mode.
    /// 在原生模式中拒绝外部 VMM 用户解析器。
    async fn resolve_user(
        &self,
        _request: ResolveUserRequest,
    ) -> Result<ResolveUserResponse, VmmClientError> {
        Err(native_management_error())
    }

    /// Rejects the external VMM user deletion API in native mode.
    /// 在原生模式中拒绝外部 VMM 用户删除 API。
    async fn delete_user(
        &self,
        _request: DeleteUserRequest,
    ) -> Result<DeleteUserResponse, VmmClientError> {
        Err(native_management_error())
    }

    /// Reads active native project or personal profile nodes.
    /// 读取活跃的原生项目或个人画像节点。
    /// Resolves the complete native read scope under the binding-change barrier.
    /// 在绑定变更屏障内解析完整原生读取范围。
    /// Takes host/envelope identities and returns validated owner-first visible projects.
    /// 接收宿主及封套身份，返回已校验且归属在首的可见项目。
    async fn memory_read_source(
        &self,
        host_project_id: &str,
        user_id: u64,
        project_id: u64,
    ) -> Result<vulcan_vmm::VmmMemoryReadSource, VmmClientError> {
        // Resolve binding and linked scopes in one operation, never as separate mutable reads.
        // 在一次操作中解析绑定及连带范围，绝不拆成多次可变化读取。
        let _operation_guard = self.operation_gate.read().await;
        subject_id(user_id)?;
        let access = self.project_access(host_project_id, project_id).await?;
        memory_read_source_from_access(&access)
    }

    /// Reads active profile nodes while enforcing an optional exact read-source expectation.
    /// 读取活动画像节点，同时校验可选的准确读取来源预期。
    async fn get_profile_nodes(
        &self,
        request: GetProfileNodesRequest,
    ) -> Result<GetProfileNodesResponse, VmmClientError> {
        // The read guard freezes project scopes while this complete native operation is evaluated.
        // 读取守卫在本次完整原生操作执行期间冻结项目作用域。
        let _operation_guard = self.operation_gate.read().await;
        if matches!(
            request.target,
            ProfileTarget::ProfileTargetTeam | ProfileTarget::ProfileTargetSpace
        ) {
            return Ok(GetProfileNodesResponse::default());
        }
        let access = self
            .project_access(request.host_project_id.as_str(), request.project_id)
            .await?;
        memory_read_source_from_access(&access)?
            .verify_expected(request.expected_read_source.as_ref())?;
        let sections = profile_sections(request.target, &access, request.user_id)?;
        let display_timezone = native_profile_display_timezone()?;
        let bundle = self
            .query_profile(
                request.user_id,
                NativeProfileBundleFormat::new(
                    ProfileBundleMode::ProfileBundleModeFull,
                    Some(false),
                )
                .view(sections, display_timezone),
            )
            .await?;
        let limit = usize::try_from(request.limit).unwrap_or(usize::MAX);
        let nodes = bundle
            .nodes
            .iter()
            .take(limit)
            .map(profile_node_entry)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(GetProfileNodesResponse {
            nodes,
            trace_id: native_trace_id("profile-nodes"),
        })
    }

    /// Renders the native project and personal profile sections requested by the caller.
    /// 渲染调用方请求的原生项目与个人画像分区。
    async fn get_profile_bundle(
        &self,
        request: GetProfileBundleRequest,
    ) -> Result<GetProfileBundleResponse, VmmClientError> {
        // The read guard freezes project scopes while this complete native operation is evaluated.
        // 读取守卫在本次完整原生操作执行期间冻结项目作用域。
        let _operation_guard = self.operation_gate.read().await;
        // format freezes mutually exclusive output and explanation semantics before Core dispatch.
        // format 在派发 Core 前冻结互斥输出及说明语义。
        let format = NativeProfileBundleFormat::new(request.mode, request.include_explanation);
        let display_timezone = native_profile_display_timezone()?;
        let access = self
            .project_access(request.host_project_id.as_str(), request.project_id)
            .await?;
        let bundle = self
            .query_profile(
                request.user_id,
                format.view(
                    profile_sections(ProfileTarget::ProfileTargetAll, &access, request.user_id)?,
                    display_timezone,
                ),
            )
            .await?;
        Ok(format.response(bundle, native_trace_id("profile-bundle")))
    }

    /// Applies one natural-language instruction to exactly one native profile owner.
    /// 将一条自然语言指令应用到唯一的原生画像所有者。
    async fn apply_profile_instruction(
        &self,
        request: ApplyProfileInstructionRequest,
    ) -> Result<ApplyProfileInstructionResponse, VmmClientError> {
        // The read guard prevents maintenance from changing the profile owner mid-command.
        // 读取守卫防止维护操作在命令执行期间改变画像所有者。
        let _operation_guard = self.operation_gate.read().await;
        let now = now_ns()?;
        let subject_id = subject_id(request.user_id)?;
        let access = self
            .project_access(request.host_project_id.as_str(), request.project_id)
            .await?;
        let target_section = writable_profile_section(request.target, &access, request.user_id)?;
        let scope_access = ScopeAccess {
            write_scope: target_section.scope_id.clone(),
            visible_scopes: vec![target_section.scope_id.clone()],
        };
        let mutation_identity = invocation_identity("profile-instruction", now)?;
        let result = self
            .runtime
            .run(CoreCommand::ProfileInstruction(ProfileInstructionCommand {
                subject_id,
                scope_access,
                target_section,
                instruction: request.instruction,
                occurred_at_ns: now,
                mutation_identity,
            }))
            .await
            .map_err(core_client_error)?;
        let vulcan_memorymesh_core::CommandResult::ProfileInstruction(result) = result else {
            return Err(unexpected_result("ProfileInstruction"));
        };
        Ok(ApplyProfileInstructionResponse {
            instruction_id: 0,
            accepted_nodes: result
                .accepted_nodes
                .iter()
                .map(profile_node_entry)
                .collect::<Result<Vec<_>, _>>()?,
            retired_nodes: result
                .retired_nodes
                .iter()
                .map(|node| {
                    Ok(RetiredProfileNodeEntry {
                        profile_node_id: parse_core_numeric_id(&node.node_id, "vmm-profile-")?,
                        reason: node.reason.clone(),
                    })
                })
                .collect::<Result<Vec<_>, VmmClientError>>()?,
            review_reason: result.review_reason,
            trace_id: native_trace_id("profile-instruction"),
        })
    }

    /// Runs native recall before one host model turn.
    /// 在一次宿主模型回合前运行原生召回。
    async fn pre_check(
        &self,
        request: PreCheckRequest,
    ) -> Result<PreCheckResponse, VmmClientError> {
        // The native endpoint trace distinguishes absent dispatch from local recall with no model call.
        // 原生端点诊断区分未派发与无需模型调用的本地召回。
        tracing::info!(
            "[native-vmm][precheck] stage=received session={} project={} operation={}",
            request.session_id,
            request.host_project_id,
            request.operation_id,
        );
        let now = now_ns()?;
        let subject_id = subject_id(request.user_id)?;
        let session_source_id = session_source_id(&request.session_id)?;
        // Register before recall so noise-only and not-yet-completed sessions remain manageable.
        // 在召回前登记，使纯噪音及尚未完成的会话仍可管理。
        let (_operation_guard, access) = self
            .admit_memory_session(
                &request.host_project_id,
                request.project_id,
                &subject_id,
                &session_source_id,
                None,
                None,
            )
            .await?;
        tracing::info!(
            "[native-vmm][precheck] stage=session_admitted session={}",
            request.session_id,
        );
        // Stamp the access admitted under this same operation guard, not a later binding lookup.
        // 标记此操作守卫下准入的访问范围，而不是事后重新查询的绑定。
        let read_source = memory_read_source_from_access(&access)?;
        // Resolve the authoritative lifecycle before admitting any model or retrieval work.
        // 在准入任何模型或检索工作前解析权威生命周期。
        let session_status =
            self.memory_session_status(&subject_id, &access.write_scope, &session_source_id)?;
        if !session_status.allows_runtime_memory() {
            tracing::info!(
                "[native-vmm][precheck] stage=session_paused session={} status={:?}",
                request.session_id,
                session_status,
            );
            return Ok(PreCheckResponse {
                read_source,
                should_inject: false,
                context_items: Vec::new(),
                degraded: false,
                trace_id: native_trace_id("precheck"),
                session_memory_status: session_status,
            });
        }
        let (memory_policy, vector_space_id, snapshot_id) = self
            .runtime
            .active_memory_context(&request.host_project_id)
            .await
            .map_err(native_runtime_error)?;
        let recent_turns = self.recent_precheck_turns(
            &subject_id,
            &access.write_scope,
            &session_source_id,
            memory_policy.precheck_history_turn_limit,
            memory_policy.precheck_max_input_tokens,
        )?;
        let session_recall_mode = match request.recall_mode {
            PreCheckRecallMode::PreCheckRecallModeFull => SessionRecallMode::Full,
            PreCheckRecallMode::PreCheckRecallModeUnspecified
            | PreCheckRecallMode::PreCheckRecallModeSessionCompact => {
                SessionRecallMode::CompactAware
            }
        };
        let result = self
            .runtime
            .run_with_snapshot(
                CoreCommand::PreCheck(PreCheckCommand {
                    subject_id,
                    scope_access: access.clone(),
                    session_source_id,
                    vector_space_id,
                    user_content: request.user_content,
                    recent_turns,
                    recall_mode: RecallMode::Auto,
                    session_recall_mode,
                    candidate_scope: MemoryCandidateScope {
                        scope_ids: access.visible_scopes,
                        origin_session_source_id: None,
                    },
                    max_search_queries: memory_policy.retrieval_policy.max_query_items,
                    max_memories: memory_policy.precheck_policy.recall_top_k,
                    now_ns: now,
                    mutation_identity: caller_identity(
                        "precheck",
                        request.operation_id.as_str(),
                        now,
                    )?,
                }),
                snapshot_id,
            )
            .await
            .map_err(core_client_error)?;
        let vulcan_memorymesh_core::CommandResult::PreCheck(result) = result else {
            return Err(unexpected_result("PreCheck"));
        };
        tracing::info!(
            "[native-vmm][precheck] stage=core_result session={} context_items={} should_inject={} degraded={}",
            request.session_id,
            result.context_items.len(),
            result.should_inject,
            result.degraded,
        );
        let context_items = result
            .context_items
            .iter()
            .map(|item| {
                Ok(ContextItem {
                    text: if item.details.is_empty() {
                        item.summary.clone()
                    } else {
                        format!("{}\n{}", item.summary, item.details)
                    },
                    score: item.score,
                    turn_id: optional_core_numeric_id(item.source_turn_id.as_ref(), "vmm-turn-")?,
                    has_dialogue: item.source_turn_id.is_some(),
                    created_datetime: format_unix_ns(item.created_at_ns),
                    memory_id: parse_core_numeric_id(&item.memory_id, "vmm-memory-")?,
                })
            })
            .collect::<Result<Vec<_>, VmmClientError>>()?;
        Ok(PreCheckResponse {
            read_source,
            should_inject: result.should_inject,
            context_items,
            degraded: result.degraded,
            trace_id: native_trace_id("precheck"),
            session_memory_status: session_status,
        })
    }

    /// Durably records a completed turn and starts its native background analysis.
    /// 持久记录一个已完成回合并启动其原生后台分析。
    async fn post_action(
        &self,
        request: PostActionRequest,
    ) -> Result<PostActionResponse, VmmClientError> {
        self.post_action_admitted(request, None).await
    }

    /// Keeps the revocable permit through native lock waits before session registration and intake.
    /// 在原生锁等待期间保留可撤销许可，在会话登记及入库前检查。
    async fn post_action_with_permit(
        &self,
        request: PostActionRequest,
        permit: &vulcan_vmm::VmmWritebackPermit,
    ) -> Result<PostActionResponse, VmmClientError> {
        // The endpoint trace identifies the exact accepted request without exposing its dialogue.
        // 端点诊断标识准确请求，但不暴露对话内容。
        tracing::info!(
            "[native-vmm][postaction] stage=received session={} project={} operation={}",
            request.session_id,
            request.host_project_id,
            request.operation_id,
        );
        if let Err(error) = permit.validate_request(&request) {
            tracing::warn!(
                "[native-vmm][postaction] stage=permit session={} result=error kind={}",
                request.session_id,
                error.kind,
            );
            return Err(error);
        }
        let session_id = request.session_id.clone();
        let response = self.post_action_admitted(request, Some(permit)).await;
        match &response {
            Ok(result) => tracing::info!(
                "[native-vmm][postaction] stage=returned session={} accepted={} status={:?}",
                session_id,
                result.accepted,
                result.session_memory_status,
            ),
            Err(error) => tracing::warn!(
                "[native-vmm][postaction] stage=returned session={} result=error kind={}",
                session_id,
                error.kind,
            ),
        }
        response
    }

    /// Advances the durable compact boundary for one native project session.
    /// 推进一个原生项目会话的持久压缩边界。
    async fn chat_compact(
        &self,
        request: ChatCompactRequest,
    ) -> Result<ChatCompactResponse, VmmClientError> {
        // Resolve lifecycle before constructing a compact mutation or advancing its boundary.
        // 在构造压缩变更或推进边界之前解析生命周期。
        let subject = subject_id(request.user_id)?;
        let session = session_source_id(&request.session_id)?;
        let (_operation_guard, access) = self
            .admit_memory_session(
                &request.host_project_id,
                request.project_id,
                &subject,
                &session,
                None,
                None,
            )
            .await?;
        if !self
            .memory_session_status(&subject, &access.write_scope, &session)?
            .allows_runtime_memory()
        {
            return Ok(ChatCompactResponse {
                accepted: false,
                updated: false,
                compacted_turn_id: 0,
                trace_id: native_trace_id("chat-compact"),
            });
        }
        let now = now_ns()?;
        let result = self
            .runtime
            .run(CoreCommand::ChatCompact(
                vulcan_memorymesh_core::ChatCompactCommand {
                    subject_id: subject,
                    scope_access: access,
                    session_source_id: session,
                    compacted_at_ns: now,
                    mutation_identity: caller_identity(
                        "chat-compact",
                        request.operation_id.as_str(),
                        now,
                    )?,
                },
            ))
            .await
            .map_err(core_client_error)?;
        let vulcan_memorymesh_core::CommandResult::ChatCompact(result) = result else {
            return Err(unexpected_result("ChatCompact"));
        };
        Ok(ChatCompactResponse {
            accepted: result.accepted,
            updated: result.updated,
            compacted_turn_id: optional_core_numeric_id(
                result.compacted_turn_id.as_ref(),
                "vmm-turn-",
            )?,
            trace_id: native_trace_id("chat-compact"),
        })
    }

    /// Runs grouped semantic memory retrieval against native project memory.
    /// 针对原生项目记忆运行分组语义检索。
    async fn search_memory_events(
        &self,
        request: SearchMemoryEventsRequest,
    ) -> Result<SearchMemoryEventsResponse, VmmClientError> {
        // The read guard keeps all authorized retrieval scopes stable until the search completes.
        // 读取守卫在检索完成前保持所有已授权查询作用域稳定。
        let _operation_guard = self.operation_gate.read().await;
        // The host supplies runtime context only for a tool invocation, not a management query.
        // 宿主仅为工具调用提供运行时上下文，管理查询不携带该上下文。
        let subject = subject_id(request.user_id)?;
        let access = self
            .project_access(request.host_project_id.as_str(), request.project_id)
            .await?;
        self.ensure_runtime_session(
            &subject,
            &access.write_scope,
            request.runtime_session_id.as_deref(),
        )?;
        let vector_space_id = self
            .runtime
            .current_vector_space_id()
            .await
            .map_err(native_runtime_error)?;
        let result = self
            .runtime
            .run_for_host_project(
                CoreCommand::MemorySearchBatch(MemorySearchBatchCommand {
                    subject_id: subject,
                    scope_access: access.clone(),
                    queries: request.queries,
                    top_k: request.top_k,
                    vector_space_id,
                    boundary: CandidateQueryBoundary {
                        candidate_scope: Some(MemoryCandidateScope {
                            scope_ids: access.visible_scopes,
                            origin_session_source_id: None,
                        }),
                        session_recall_boundary: None,
                    },
                    now_ns: now_ns()?,
                }),
                &request.host_project_id,
            )
            .await
            .map_err(core_client_error)?;
        let vulcan_memorymesh_core::CommandResult::MemorySearchBatch(groups) = result else {
            return Err(unexpected_result("MemorySearchBatch"));
        };
        let results = groups
            .into_iter()
            .map(|group| {
                Ok(MemorySearchGroupResult {
                    query_index: group.query_index,
                    query: group.query,
                    hits: group
                        .hits
                        .iter()
                        .map(memory_search_hit)
                        .collect::<Result<Vec<_>, _>>()?,
                })
            })
            .collect::<Result<Vec<_>, VmmClientError>>()?;
        Ok(SearchMemoryEventsResponse {
            results,
            trace_id: native_trace_id("memory-search"),
        })
    }

    /// Reads caller-authorized canonical turn details from native Core storage.
    /// 从原生 Core 存储读取调用方已授权的规范回合详情。
    async fn get_turn_details(
        &self,
        request: GetTurnDetailsRequest,
    ) -> Result<GetTurnDetailsResponse, VmmClientError> {
        // The read guard keeps turn ownership stable while details are resolved and projected.
        // 读取守卫在回合详情解析与投影期间保持其归属稳定。
        let _operation_guard = self.operation_gate.read().await;
        // subject and access are resolved through the same authority as other native reads.
        // subject 与 access 通过其他原生读取使用的同一权威解析。
        let subject = subject_id(request.user_id)?;
        let access = self
            .project_access(request.host_project_id.as_str(), request.project_id)
            .await?;
        // requested preserves exact native identities and the caller's batch order.
        // requested 保留准确原生身份及调用方批量顺序。
        self.ensure_runtime_session(
            &subject,
            &access.write_scope,
            request.runtime_session_id.as_deref(),
        )?;
        let requested = request
            .turn_ids
            .into_iter()
            .map(turn_id)
            .collect::<Result<Vec<_>, _>>()?;
        // snapshot keeps details and neighbors on one read version, excluding retention trash.
        // snapshot 将详情与相邻窗口固定于同一读取版本，并排除保留回收区。
        let snapshot = self
            .runtime
            .storage_snapshot()
            .map_err(native_runtime_error)?;
        let turns = resolve_turn_details(&snapshot.dataset.turns, &subject, &access, &requested)?
            .into_iter()
            .map(turn_detail_entry)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(GetTurnDetailsResponse {
            turns,
            trace_id: native_trace_id("turn-details"),
        })
    }

    /// Writes an atomic native direct-memory batch with explicit owner scopes.
    /// 使用显式所有者作用域原子写入一批原生主动记忆。
    async fn write_memories(
        &self,
        request: WriteMemoriesRequest,
    ) -> Result<WriteMemoriesResponse, VmmClientError> {
        let now = now_ns()?;
        // Direct writes always carry an attribution session, including writes initiated by management.
        // 主动写入始终携带归因会话，包括管理入口发起的写入。
        let subject = subject_id(request.user_id)?;
        let session = session_source_id(&request.session_id)?;
        // Admit the exact session even when the subsequent write is fully deduplicated.
        // 即使后续写入完全去重，也准入准确会话。
        let (_operation_guard, project_access) = self
            .admit_memory_session(
                &request.host_project_id,
                request.project_id,
                &subject,
                &session,
                None,
                None,
            )
            .await?;
        let project_scope = project_access.write_scope.clone();
        self.ensure_runtime_session(&subject, &project_scope, Some(&request.session_id))?;
        let visible_scopes = project_access.visible_scopes;
        let vector_space_id = self
            .runtime
            .current_vector_space_id()
            .await
            .map_err(native_runtime_error)?;
        let items = request
            .items
            .into_iter()
            .map(|item| {
                let scope_class = match item.scope_level {
                    0 | 2 => MemoryScopeClass::Environment,
                    1 => MemoryScopeClass::Session,
                    3 => MemoryScopeClass::Subject,
                    value => {
                        return Err(VmmClientError::new(
                            "invalid_argument",
                            format!("unsupported native memory scope level {value}"),
                        ));
                    }
                };
                Ok(MemoryWriteBatchItem {
                    // Native memory always has one physical write owner: the bound project. The
                    // requested level remains a lifecycle class, never an alternate write address.
                    // 原生记忆始终只有一个物理写入所有者：绑定项目；请求级别仅作为生命周期分类，不会成为替代写入地址。
                    target_scope_id: project_scope.clone(),
                    scope_class,
                    category: memory_category(item.category)?,
                    summary: item.abstract_text,
                    details: item.details,
                    priority: memory_priority(item.priority)?,
                    memory_level: memory_level(item.memory_level)?,
                    expires_at_ns: None,
                    tags: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>, VmmClientError>>()?;
        let result = self
            .runtime
            .run_for_host_project(
                CoreCommand::MemoryWriteBatch(MemoryWriteBatchCommand {
                    replacement_scope: MemoryReplacementScope {
                        candidates: MemoryCandidateScope {
                            scope_ids: visible_scopes.clone(),
                            origin_session_source_id: None,
                        },
                        authorized_supersede_scopes: vec![project_scope.clone()],
                    },
                    subject_id: subject,
                    scope_access: ScopeAccess {
                        write_scope: project_scope.clone(),
                        visible_scopes: visible_scopes.clone(),
                    },
                    authorized_write_scopes: vec![project_scope],
                    session_source_id: session,
                    vector_space_id,
                    occurred_at_ns: now,
                    items,
                    mutation_identity: invocation_identity("memory-write", now)?,
                }),
                &request.host_project_id,
            )
            .await
            .map_err(core_client_error)?;
        let vulcan_memorymesh_core::CommandResult::MemoryWriteBatch(result) = result else {
            return Err(unexpected_result("MemoryWriteBatch"));
        };
        let items = result
            .memories
            .iter()
            .zip(result.deduplicated)
            .map(|(memory, deduped)| {
                Ok(WriteMemoryResultItem {
                    memory_id: parse_core_numeric_id(&memory.id, "vmm-memory-")?,
                    deduped,
                })
            })
            .collect::<Result<Vec<_>, VmmClientError>>()?;
        Ok(WriteMemoriesResponse {
            items,
            trace_id: native_trace_id("memory-write"),
        })
    }

    /// Deletes an atomic caller-authorized native memory batch.
    /// 原子删除一批调用方已授权的原生记忆。
    async fn delete_memories(
        &self,
        request: DeleteMemoriesRequest,
    ) -> Result<DeleteMemoriesResponse, VmmClientError> {
        // The read guard prevents direct deletions from crossing a project maintenance boundary.
        // 读取守卫防止主动删除跨越项目维护边界。
        let _operation_guard = self.operation_gate.read().await;
        let now = now_ns()?;
        // projectAccess resolves the stable owner; linked scopes are recall-only and cannot authorize deletion.
        // projectAccess 解析稳定所有者；连带作用域仅用于召回，不能授权删除。
        let project_access = self
            .project_access(request.host_project_id.as_str(), request.project_id)
            .await?;
        // ownerScope is the only mutable project scope for this VulcanCode project.
        // ownerScope 是该 VulcanCode 项目唯一可变的项目作用域。
        let owner_scope = project_access.write_scope;
        // Deletion from a runtime tool is gated by its caller, not by the memory's source session.
        // 运行时工具删除由调用方会话决定准入，而不是由记忆来源会话决定。
        let subject = subject_id(request.user_id)?;
        self.ensure_runtime_session(
            &subject,
            &owner_scope,
            request.runtime_session_id.as_deref(),
        )?;
        // The command-scoped physical receipt is independent of logical deleted identity counts.
        // 命令级物理回执独立于逻辑删除身份计数。
        let (result, deleted_vector_rows) = self
            .runtime
            .run_memory_delete(MemoryDeleteBatchCommand {
                subject_id: subject,
                scope_access: ScopeAccess {
                    write_scope: owner_scope.clone(),
                    visible_scopes: vec![owner_scope],
                },
                reason: request.reason,
                items: request
                    .memory_ids
                    .into_iter()
                    .map(|id| {
                        Ok(MemoryDeleteBatchItem {
                            memory_id: memory_id(id)?,
                            expected_version: None,
                        })
                    })
                    .collect::<Result<Vec<_>, VmmClientError>>()?,
                mutation_identity: invocation_identity("memory-delete", now)?,
            })
            .await
            .map_err(core_client_error)?;
        let vulcan_memorymesh_core::CommandResult::MemoryDeleteBatch(result) = result else {
            return Err(unexpected_result("MemoryDeleteBatch"));
        };
        let deleted_memory_ids = result
            .deleted_memory_ids
            .iter()
            .map(|id| parse_core_numeric_id(id, "vmm-memory-"))
            .collect::<Result<Vec<_>, _>>()?;
        let not_found_memory_ids = result
            .not_found_memory_ids
            .iter()
            .map(|id| parse_core_numeric_id(id, "vmm-memory-"))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DeleteMemoriesResponse {
            deleted_memory_ids,
            not_found_memory_ids,
            deleted_vector_rows,
            trace_id: native_trace_id("memory-delete"),
        })
    }
}

impl NativeVmmClient {
    /// Validates a declared runtime caller while allowing explicitly sessionless management operations.
    /// 校验声明的运行时调用方，同时允许明确不带会话的管理操作。
    /// The exact subject and owning project qualify the optional session; paused states return a stable error.
    /// 准确主体及归属项目限定可选会话；暂停状态返回稳定错误。
    fn ensure_runtime_session(
        &self,
        subject: &SubjectId,
        scope: &ScopeId,
        runtime_session: Option<&str>,
    ) -> Result<(), VmmClientError> {
        let Some(session) = runtime_session else {
            return Ok(());
        };
        // Empty or invalid declared identifiers fail validation rather than becoming management calls.
        // 空或非法的已声明身份校验失败，而不是变成管理调用。
        let session = session_source_id(session)?;
        let status = self.memory_session_status(subject, scope, &session)?;
        if !status.allows_runtime_memory() {
            return Err(VmmClientError::new(
                "session_memory_paused",
                format!(
                    "Native VMM session memory is paused: {status:?}. Restore or explicitly restart it before using runtime memory tools."
                ),
            ));
        }
        Ok(())
    }

    /// Resolves the exact subject/project/session lifecycle for a runtime request.
    /// 为运行时请求解析准确主体、项目及会话的生命周期。
    /// Unregistered sessions start active without writing metadata during a read; storage failures are explicit.
    /// 未登记会话初始为活动，但读取不写入元数据；存储错误明确返回。
    fn memory_session_status(
        &self,
        subject: &SubjectId,
        scope: &ScopeId,
        session: &SessionSourceId,
    ) -> Result<SessionMemoryStatus, VmmClientError> {
        // Session ownership is a declared composite key, independent of visible linked projects.
        // 会话归属使用声明的复合键，独立于可见连带项目。
        let snapshot = self
            .runtime
            .storage_snapshot()
            .map_err(native_runtime_error)?;
        // Absence represents a not-yet-registered session, not an inferred forgotten state.
        // 缺失表示尚未登记的会话，而不是推断的已遗忘状态。
        let Some(record) = snapshot.memory_sessions.iter().find(|record| {
            &record.subject_id == subject
                && &record.scope_id == scope
                && &record.session_source_id == session
        }) else {
            return Ok(SessionMemoryStatus::Active);
        };
        Ok(match record.status {
            vulcan_memorymesh_core::StorageSessionStatus::Active => SessionMemoryStatus::Active,
            vulcan_memorymesh_core::StorageSessionStatus::Archived => SessionMemoryStatus::Archived,
            vulcan_memorymesh_core::StorageSessionStatus::Recycled => SessionMemoryStatus::Recycled,
            vulcan_memorymesh_core::StorageSessionStatus::Forgotten => {
                SessionMemoryStatus::Forgotten
            }
        })
    }

    /// Loads the newest same-session turn window using the external VMM DETAILS/RAW_TURN rules.
    /// 使用外部 VMM 的 DETAILS/RAW_TURN 规则加载同会话最新回合窗口。
    ///
    /// # Parameters
    /// 参数
    /// `subject_id`, `scope_id`, and `session_source_id` form the exact session ownership tuple.
    /// `subject_id`、`scope_id` 与 `session_source_id` 组成准确的会话归属元组。
    /// `turn_limit` and `max_input_tokens` are the active bounded memory-policy limits.
    /// `turn_limit` 与 `max_input_tokens` 是当前生效的有界记忆策略限制。
    ///
    /// # Returns
    /// 返回值
    /// A chronological recent-turn window that always retains the newest non-empty turn even when it exceeds the budget.
    /// 返回按时间正序排列的近期回合窗口；即便最新非空回合超出预算，也始终保留该回合。
    ///
    /// # Errors
    /// 错误
    /// Returns an error when storage cannot be read or a raw turn cannot be serialized exactly.
    /// 当存储不可读取或原始回合无法准确序列化时返回错误。
    fn recent_precheck_turns(
        &self,
        subject_id: &SubjectId,
        scope_id: &ScopeId,
        session_source_id: &SessionSourceId,
        turn_limit: u32,
        max_input_tokens: u32,
    ) -> Result<Vec<PreCheckTurnContext>, VmmClientError> {
        let snapshot = self
            .runtime
            .storage_snapshot()
            .map_err(native_runtime_error)?;
        let mut turns = snapshot
            .dataset
            .turns
            .into_iter()
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
        let keep_from = turns.len().saturating_sub(turn_limit as usize);
        let mut contexts = turns
            .into_iter()
            .skip(keep_from)
            .map(precheck_turn_context)
            .collect::<Result<Vec<_>, _>>()?;
        contexts.retain(|context| !context.content.trim().is_empty());
        if contexts.is_empty() || max_input_tokens == 0 {
            return Ok(contexts);
        }
        let mut selected = Vec::new();
        let mut total = 0_u32;
        for context in contexts.into_iter().rev() {
            let budget = estimate_text_tokens(context.content.as_str());
            if !selected.is_empty() && total.saturating_add(budget) > max_input_tokens {
                break;
            }
            total = total.saturating_add(budget);
            selected.push(context);
        }
        selected.reverse();
        Ok(selected)
    }

    /// Runs one exact native profile query and returns its typed bundle.
    /// 运行一次精确的原生画像查询并返回其强类型画像包。
    async fn query_profile(
        &self,
        user_id: u64,
        view: ProfileView,
    ) -> Result<ProfileBundle, VmmClientError> {
        let visible_scopes = view
            .sections
            .iter()
            .map(|section| section.scope_id.clone())
            .collect::<Vec<_>>();
        let write_scope = visible_scopes
            .first()
            .cloned()
            .ok_or_else(|| VmmClientError::new("invalid_argument", "profile view is empty"))?;
        let result = self
            .runtime
            .run(CoreCommand::ProfileQuery(
                vulcan_memorymesh_core::ProfileQueryCommand {
                    subject_id: subject_id(user_id)?,
                    scope_access: ScopeAccess {
                        write_scope,
                        visible_scopes,
                    },
                    view,
                    evaluated_at_ns: now_ns()?,
                },
            ))
            .await
            .map_err(core_client_error)?;
        let vulcan_memorymesh_core::CommandResult::ProfileQuery(bundle) = result else {
            return Err(unexpected_result("ProfileQuery"));
        };
        Ok(bundle)
    }
}

/// Builds the explicit error used when external management semantics reach native mode.
/// 构建外部管理语义误入原生模式时使用的显式错误。
fn native_management_error() -> VmmClientError {
    VmmClientError::new(
        "native_management_api_required",
        "Native VMM project management uses the VulcanCode one-level data-plane API.",
    )
}

/// Converts a Core failure into the stable VMM client error envelope.
/// 将 Core 失败转换为稳定的 VMM 客户端错误包络。
fn core_client_error(error: vulcan_memorymesh_core::CoreError) -> VmmClientError {
    VmmClientError::new("native_core", error.to_string())
}

/// Converts one host-native runtime failure into the stable client error envelope.
/// 将一条宿主原生运行时失败转换为稳定客户端错误包络。
fn native_runtime_error(error: impl std::fmt::Display) -> VmmClientError {
    VmmClientError::new("native_runtime", error.to_string())
}

/// Builds an explicit typed-result mismatch error.
/// 构建显式的强类型结果不匹配错误。
fn unexpected_result(expected: &str) -> VmmClientError {
    VmmClientError::new(
        "native_core_result_mismatch",
        format!("native Core did not return {expected}"),
    )
}

/// Projects the supplied authoritative lifecycle and whether intake is accepted in that state.
/// 投影传入的权威生命周期及该状态是否接收入库。
#[must_use]
fn native_post_action_response(session_status: SessionMemoryStatus) -> PostActionResponse {
    PostActionResponse {
        accepted: session_status.allows_runtime_memory(),
        trace_id: native_trace_id("post-action"),
        session_memory_status: session_status,
    }
}

/// Fingerprints one exact caller PostAction request before host scheduling fields are added.
/// 在添加宿主调度字段前为一份准确调用方 PostAction 请求生成指纹。
///
/// # Parameters
/// 参数
/// `request` is the immutable transport request whose operation identity is being claimed.
/// `request` 是正在声明操作身份的不可变传输请求。
///
/// # Returns
/// 返回值
/// A lowercase SHA-256 identity with an explicit algorithm prefix.
/// 返回带显式算法前缀的小写 SHA-256 身份。
///
/// # Errors
/// Returns a controlled serialization error before any durable queue row is written.
/// 在写入任何耐久队列记录前返回受控序列化错误。
fn pending_analysis_request_fingerprint(
    request: &PostActionRequest,
) -> Result<String, VmmClientError> {
    let encoded = serde_json::to_vec(request).map_err(|error| {
        VmmClientError::new(
            "native_analysis_queue",
            format!("serialize native VMM PostAction request failed: {error}"),
        )
    })?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(&encoded))))
}

/// Returns current Unix milliseconds within SQLite's signed range.
/// 返回位于 SQLite 有符号范围内的当前 Unix 毫秒数。
///
/// # Errors
/// Returns a clock or integer-range error using the native client envelope text.
/// 时钟或整数范围异常时返回原生客户端包络文本。
fn now_unix_ms() -> Result<i64, String> {
    now_ns()
        .map(|value| value.div_euclid(1_000_000))
        .map_err(|error| error.to_string())
}

/// Computes bounded exponential retry delay for one persisted attempt count.
/// 为一个持久尝试次数计算有界指数重试延迟。
///
/// # Parameters
/// 参数
/// `attempt_count` is one-based for the failure currently being scheduled.
/// `attempt_count` 是当前待调度失败的一基计数。
///
/// # Returns
/// 返回值
/// A delay between one second and five minutes in milliseconds.
/// 返回一秒至五分钟之间的毫秒延迟。
#[must_use]
fn pending_analysis_retry_delay_ms(attempt_count: u32) -> i64 {
    let exponent = attempt_count.saturating_sub(1).min(18);
    1_000_i64
        .saturating_mul(1_i64 << exponent)
        .min(PENDING_ANALYSIS_MAX_RETRY_DELAY_MS)
}

/// Reports whether one durable queue row proves that its PostAction intake was accepted.
/// 判断一条耐久队列记录是否证明其 PostAction 登记已经被接受。
///
/// # Parameters
/// 参数
/// `job` is the authoritative row returned by the host data plane.
/// `job` 是宿主数据平面返回的权威记录。
///
/// # Returns
/// 返回值
/// `true` for ready and every terminal state; an unfinished intake remains `false`.
/// 对就绪态及全部终态返回 `true`；未完成登记仍返回 `false`。
#[must_use]
fn pending_analysis_job_is_accepted(job: &NativeVmmPendingAnalysisJob) -> bool {
    matches!(
        job.state.as_str(),
        VMM_PENDING_ANALYSIS_READY
            | VMM_PENDING_ANALYSIS_COMPLETED
            | VMM_PENDING_ANALYSIS_BYPASSED
            | VMM_PENDING_ANALYSIS_CORRUPTED
    )
}

/// Resolves whether one failed attempt must leave the pending set immediately.
/// 解析一次失败尝试是否必须立即离开待处理集合。
///
/// # Parameters
/// 参数
/// `error` preserves failure semantics and `previous_attempt_count` is the durable count before
/// the current attempt.
/// `error` 保留失败语义，`previous_attempt_count` 是当前尝试前的耐久计数。
///
/// # Returns
/// 返回值
/// Corrupted payloads are isolated, uncertain outcomes are bypassed, ordinary analysis failures
/// are bypassed at the fifth attempt, and infrastructure retries have no terminal status.
/// 损坏载荷会被隔离，结果不确定会被放行，普通分析失败在第五次时被放行，基础设施重试无终态。
#[must_use]
fn pending_analysis_terminal_status(
    error: &PendingAnalysisJobError,
    previous_attempt_count: u32,
) -> Option<TurnStatus> {
    match error {
        PendingAnalysisJobError::Corrupted(_) => Some(TurnStatus::Corrupted),
        PendingAnalysisJobError::Analysis {
            outcome_uncertain, ..
        } if *outcome_uncertain
            || previous_attempt_count.saturating_add(1)
                >= PENDING_ANALYSIS_FAILURE_PASS_THRESHOLD =>
        {
            Some(TurnStatus::AnalysisBypassed)
        }
        PendingAnalysisJobError::Analysis { .. } | PendingAnalysisJobError::Retryable(_) => None,
    }
}

/// Applies one host failure-policy transition to an exact pending turn.
/// 将一项宿主失败策略迁移应用到准确待处理回合。
///
/// # Parameters
/// 参数
/// `snapshot` is the coherent Core state, `turn_id` identifies the owned turn,
/// `requested_status` is bypassed or corrupted, and `job_operation_id` is diagnostic context.
/// `snapshot` 是一致 Core 状态，`turn_id` 标识所属回合，`requested_status`
/// 为已放行或已损坏，`job_operation_id` 用作诊断上下文。
///
/// # Returns
/// 返回值
/// The resulting terminal status, preserving a terminal state already committed by recovery.
/// 返回迁移后的终态；若恢复过程已经提交终态则保留该状态。
///
/// # Errors
/// 错误
/// Returns an error for a missing turn, unsupported request, or incompatible existing state.
/// 回合缺失、请求状态不受支持或既有状态不兼容时返回错误。
fn transition_pending_turn_status(
    snapshot: &mut vulcan_memorymesh_core::StorageSnapshot,
    turn_id: &TurnId,
    requested_status: TurnStatus,
    job_operation_id: &str,
) -> Result<TurnStatus, String> {
    if !matches!(
        requested_status,
        TurnStatus::AnalysisBypassed | TurnStatus::Corrupted
    ) {
        return Err(String::from("待分析回合请求了不受支持的失败终态"));
    }
    let turn = snapshot
        .dataset
        .turns
        .iter_mut()
        .find(|turn| turn.id == *turn_id)
        .ok_or_else(|| format!("待分析作业 `{job_operation_id}` 引用的回合 `{turn_id}` 不存在"))?;
    match turn.status {
        TurnStatus::Pending => {
            turn.status = requested_status;
            turn.user_input_kind = None;
            turn.analysis_details = None;
        }
        TurnStatus::Completed | TurnStatus::AnalysisBypassed | TurnStatus::Corrupted => {}
        TurnStatus::ProcessedUnknown => {
            return Err(format!(
                "待分析作业 `{job_operation_id}` 的回合已处于不可重写状态 {:?}",
                turn.status
            ));
        }
    }
    Ok(turn.status)
}

/// Bounds one untrusted background failure diagnostic before persistence.
/// 在持久化前限制一条不可信后台失败诊断。
///
/// # Parameters
/// 参数
/// `message` is the exact provider, Core, or storage error text.
/// `message` 是准确 Provider、Core 或存储错误文本。
///
/// # Returns
/// 返回值
/// At most the configured number of Unicode scalar values.
/// 返回不超过配置数量的 Unicode 标量值。
#[must_use]
fn bounded_diagnostic(message: &str) -> String {
    message
        .chars()
        .take(VMM_PENDING_ANALYSIS_ERROR_CHAR_LIMIT)
        .collect()
}

/// Returns the current Unix timestamp in nanoseconds.
/// 返回当前 Unix 纳秒时间戳。
fn now_ns() -> Result<i64, VmmClientError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            VmmClientError::new(
                "clock_error",
                format!("system clock precedes Unix epoch: {error}"),
            )
        })?;
    i64::try_from(duration.as_nanos()).map_err(|error| {
        VmmClientError::new(
            "clock_error",
            format!("Unix nanosecond timestamp overflow: {error}"),
        )
    })
}

/// Resolves the process-local IANA timezone used by the external profile renderer.
/// 解析外置画像渲染器使用的进程本地 IANA 时区。
fn native_profile_display_timezone() -> Result<String, VmmClientError> {
    iana_time_zone::get_timezone().map_err(|error| {
        VmmClientError::new(
            "native_profile_timezone",
            format!("failed to resolve the local IANA timezone: {error}"),
        )
    })
}

/// Formats one Unix nanosecond timestamp as RFC 3339 for the legacy display projection.
/// 将一个 Unix 纳秒时间戳格式化为旧展示投影使用的 RFC 3339 文本。
fn format_unix_ns(value: i64) -> String {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(value))
        .ok()
        .and_then(|time| time.format(&Rfc3339).ok())
        .unwrap_or_default()
}

/// Builds a stable native trace label for process-local calls.
/// 为进程内调用构建稳定的原生追踪标签。
fn native_trace_id(operation: &str) -> String {
    format!("native-vmm:{operation}")
}

/// Creates the exact Core subject identity for the sole host-owned native user.
/// 为宿主持有的唯一原生用户创建精确 Core 主体标识。
fn subject_id(user_id: u64) -> Result<SubjectId, VmmClientError> {
    ensure_native_user_id(user_id)?;
    SubjectId::new(format!("vmm-subject:{user_id}")).map_err(identifier_error)
}

/// Creates the exact Core project scope identity.
/// 创建精确的 Core 项目作用域标识。
fn project_scope_id(project_id: u64) -> Result<ScopeId, VmmClientError> {
    ScopeId::new(format!("vmm-project:{project_id}")).map_err(identifier_error)
}

/// Creates the exact Core personal-profile scope identity for the sole native user.
/// 为唯一原生用户创建精确的 Core 个人画像作用域标识。
fn subject_scope_id(user_id: u64) -> Result<ScopeId, VmmClientError> {
    ensure_native_user_id(user_id)?;
    ScopeId::new(format!("vmm-subject:{user_id}")).map_err(identifier_error)
}

/// Rejects shared-protocol identities that try to create a second native user namespace.
/// 拒绝试图通过共享协议创建第二个原生用户命名空间的身份。
fn ensure_native_user_id(user_id: u64) -> Result<(), VmmClientError> {
    if user_id == DEFAULT_NATIVE_USER_ID {
        return Ok(());
    }
    Err(VmmClientError::new(
        "invalid_argument",
        format!(
            "native VMM accepts only the fixed host user id {DEFAULT_NATIVE_USER_ID}, got {user_id}"
        ),
    ))
}

/// Creates the host session source identity without changing the source identifier.
/// 在不改变来源标识的情况下创建宿主会话来源身份。
fn session_source_id(session_id: &str) -> Result<SessionSourceId, VmmClientError> {
    SessionSourceId::new(session_id.to_owned()).map_err(identifier_error)
}

/// Creates an exact Core turn identity from its public numeric projection.
/// 从公共数字投影创建精确 Core 回合标识。
fn turn_id(value: u64) -> Result<TurnId, VmmClientError> {
    TurnId::new(format!("vmm-turn-{value}")).map_err(identifier_error)
}

/// Creates an exact Core memory identity from its public numeric projection.
/// 从公共数字投影创建精确 Core 记忆标识。
fn memory_id(value: u64) -> Result<vulcan_memorymesh_core::MemoryId, VmmClientError> {
    vulcan_memorymesh_core::MemoryId::new(format!("vmm-memory-{value}")).map_err(identifier_error)
}

/// Converts any Core identifier validation failure into a client argument error.
/// 将任意 Core 标识校验失败转换为客户端参数错误。
fn identifier_error(error: impl std::fmt::Display) -> VmmClientError {
    VmmClientError::new("invalid_identifier", error.to_string())
}

/// Builds the owner-only project access used until linked query scopes are resolved by the data plane.
/// 构建仅含所有者项目的访问范围，后续由数据平面解析连带查询作用域。
fn owner_only_project_access(project_id: u64) -> Result<ScopeAccess, VmmClientError> {
    let project_scope = project_scope_id(project_id)?;
    Ok(ScopeAccess {
        write_scope: project_scope.clone(),
        visible_scopes: vec![project_scope],
    })
}

/// Converts one resolved access snapshot into the public local read identity.
/// 将一个已解析访问快照转换为公开的本地读取身份。
/// Takes validated Core scopes and returns exact numeric ownership and ordered visibility.
/// 接收已校验 Core 作用域，返回准确数字归属及有序可见范围。
pub(super) fn memory_read_source_from_access(
    access: &ScopeAccess,
) -> Result<vulcan_vmm::VmmMemoryReadSource, VmmClientError> {
    // Use the same resolved scope object as the actual profile or recall operation.
    // 使用与实际画像或召回操作相同的已解析作用域对象。
    let owner = vulcan_vmm::VmmMemorySource::native(parse_core_numeric_id(
        &access.write_scope,
        "vmm-project:",
    )?)?;
    let visible_project_ids = access
        .visible_scopes
        .iter()
        .map(|scope| parse_core_numeric_id(scope, "vmm-project:"))
        .collect::<Result<Vec<_>, _>>()?;
    vulcan_vmm::VmmMemoryReadSource::new(owner, visible_project_ids)
}

/// Builds one mutation identity from an exact operation identity and source class.
/// 根据精确操作身份与来源类别构建一个变更身份。
fn mutation_identity(
    operation: &str,
    source_kind: MutationSourceKind,
    request_source_id: &str,
    observed_at_ns: i64,
) -> Result<MutationIdentity, VmmClientError> {
    Ok(MutationIdentity {
        operation_id: OperationId::new(operation.to_owned()).map_err(identifier_error)?,
        idempotency_key: IdempotencyKey::new(operation.to_owned()).map_err(identifier_error)?,
        request_fingerprint: format!("fp256:{}", "0".repeat(64)),
        source_kind,
        request_source_id: request_source_id.to_owned(),
        observed_at_ns,
    })
}

/// Uses a caller operation identity when present and creates an invocation identity otherwise.
/// 在存在时使用调用方操作身份，否则创建一次调用级身份。
fn caller_identity(
    kind: &str,
    caller_operation: &str,
    observed_at_ns: i64,
) -> Result<MutationIdentity, VmmClientError> {
    if caller_operation.is_empty() {
        return invocation_identity(kind, observed_at_ns);
    }
    mutation_identity(
        caller_operation,
        MutationSourceKind::HostRpc,
        format!("native-vmm:{kind}").as_str(),
        observed_at_ns,
    )
}

/// Creates one explicit invocation-scoped mutation identity.
/// 创建一个显式调用级变更身份。
fn invocation_identity(
    kind: &str,
    observed_at_ns: i64,
) -> Result<MutationIdentity, VmmClientError> {
    let operation = stable_or_generated_operation(kind, "");
    mutation_identity(
        operation.as_str(),
        MutationSourceKind::InvocationScoped,
        format!("native-vmm:{kind}").as_str(),
        observed_at_ns,
    )
}

/// Preserves a non-empty caller identity or creates a collision-resistant local identity.
/// 保留非空调用方身份，或创建一个抗冲突的本地身份。
fn stable_or_generated_operation(kind: &str, caller_operation: &str) -> String {
    if caller_operation.is_empty() {
        format!("native-vmm:{kind}:{}", Uuid::new_v4())
    } else {
        caller_operation.to_owned()
    }
}

/// Derives one bounded deterministic background operation identity.
/// 派生一个有界且确定性的后台操作身份。
fn derived_operation_id(parent: &str, suffix: &str) -> String {
    let digest = Sha256::digest(format!("{parent}\0{suffix}").as_bytes());
    format!("native-vmm:{suffix}:{}", hex::encode(digest))
}

/// Builds the exact project and personal profile sections selected by a protocol target.
/// 构建协议目标选中的精确项目及个人画像分区。
fn profile_sections(
    target: ProfileTarget,
    access: &ScopeAccess,
    user_id: u64,
) -> Result<Vec<ProfileViewSection>, VmmClientError> {
    let project_sections = || {
        access
            .visible_scopes
            .iter()
            .enumerate()
            .map(|(index, scope_id)| ProfileViewSection {
                scope_id: scope_id.clone(),
                role: SectionRole::Environment,
                display_label: if index == 0 {
                    String::from("Project")
                } else {
                    format!("Linked Project {index}")
                },
            })
            .collect::<Vec<_>>()
    };
    match target {
        ProfileTarget::ProfileTargetProject => Ok(project_sections()),
        ProfileTarget::ProfileTargetUser => Ok(vec![user_profile_section(user_id)?]),
        ProfileTarget::ProfileTargetAll | ProfileTarget::ProfileTargetUnspecified => {
            let mut sections = project_sections();
            sections.push(user_profile_section(user_id)?);
            Ok(sections)
        }
        ProfileTarget::ProfileTargetTeam | ProfileTarget::ProfileTargetSpace => Ok(Vec::new()),
    }
}

/// Resolves one mutable native profile target into its exact Core section.
/// 将一个可变原生画像目标解析为精确 Core 分区。
fn writable_profile_section(
    target: ProfileTarget,
    access: &ScopeAccess,
    user_id: u64,
) -> Result<ProfileViewSection, VmmClientError> {
    match target {
        ProfileTarget::ProfileTargetProject => Ok(ProfileViewSection {
            scope_id: access.write_scope.clone(),
            role: SectionRole::Environment,
            display_label: String::from("Project"),
        }),
        ProfileTarget::ProfileTargetUser => user_profile_section(user_id),
        _ => Err(VmmClientError::new(
            "invalid_argument",
            "native profile instructions require PROJECT or USER target",
        )),
    }
}

/// Builds the one writable subject profile section independently from project hierarchy.
/// 独立于项目层级构建唯一可写的主体画像分区。
fn user_profile_section(user_id: u64) -> Result<ProfileViewSection, VmmClientError> {
    Ok(ProfileViewSection {
        scope_id: subject_scope_id(user_id)?,
        role: SectionRole::Subject,
        display_label: String::from("User"),
    })
}

/// Converts one Core profile node into the existing service projection.
/// 将一个 Core 画像节点转换为现有服务投影。
pub(super) fn profile_node_entry(node: &ProfileNode) -> Result<ProfileNodeEntry, VmmClientError> {
    let (target, bind_id) = if let Some(value) = node.scope_id.as_str().strip_prefix("vmm-project:")
    {
        (
            ProfileTarget::ProfileTargetProject,
            value.parse::<u64>().map_err(identifier_error)?,
        )
    } else if let Some(value) = node.scope_id.as_str().strip_prefix("vmm-subject:") {
        (
            ProfileTarget::ProfileTargetUser,
            value.parse::<u64>().map_err(identifier_error)?,
        )
    } else {
        return Err(VmmClientError::new(
            "native_scope_mismatch",
            format!("unsupported native profile scope {}", node.scope_id),
        ));
    };
    Ok(ProfileNodeEntry {
        profile_node_id: parse_core_numeric_id(&node.id, "vmm-profile-")?,
        target,
        bind_id,
        content: node.content.clone(),
        priority: profile_priority_label(node.priority).to_owned(),
        level: profile_level_label(node.profile_level).to_owned(),
        refresh_weight: node.refresh_weight,
        profile_date: node
            .effective_date_ns
            .map(format_unix_ns)
            .unwrap_or_default(),
        expires_timestamp: node.expires_at_ns.unwrap_or_default(),
        level_reason: node.level_reason.clone().unwrap_or_default(),
        source_kind: profile_source_kind(node.source_kind),
        source_id: node
            .source_id
            .as_ref()
            .and_then(|source| {
                source
                    .as_str()
                    .strip_prefix("vmm-turn-")
                    .and_then(|value| value.parse::<u64>().ok())
            })
            .unwrap_or_default(),
    })
}

/// Maps normalized Core profile priority onto VMM's P0/P1/P2 labels.
/// 将规范化 Core 画像优先级映射到 VMM 的 P0/P1/P2 标签。
const fn profile_priority_label(priority: u32) -> &'static str {
    if priority >= 90 {
        "P0"
    } else if priority >= 60 {
        "P1"
    } else {
        "P2"
    }
}

/// Maps Core profile lifecycle levels onto VMM's L0-L3 labels.
/// 将 Core 画像生命周期等级映射到 VMM 的 L0-L3 标签。
const fn profile_level_label(level: ProfileLevel) -> &'static str {
    match level {
        ProfileLevel::Transient => "L0",
        ProfileLevel::Situational => "L1",
        ProfileLevel::Stable => "L2",
        ProfileLevel::Persistent => "L3",
    }
}

/// Maps the closed Core profile source set onto the service projection.
/// 将封闭的 Core 画像来源集合映射到服务投影。
const fn profile_source_kind(source: ProfileSourceKind) -> ProfileNodeSourceKind {
    match source {
        ProfileSourceKind::TurnExtract => ProfileNodeSourceKind::ProfileNodeSourceKindTurnExtract,
        ProfileSourceKind::ManualInstruction => {
            ProfileNodeSourceKind::ProfileNodeSourceKindManualInstruction
        }
        ProfileSourceKind::SystemSeed => ProfileNodeSourceKind::ProfileNodeSourceKindSystemSeed,
        ProfileSourceKind::RetainedAfterSubjectDelete => {
            ProfileNodeSourceKind::ProfileNodeSourceKindRetainedAfterUserDelete
        }
    }
}

/// Builds the canonical ordered Core timeline for one completed host turn.
/// 为一个已完成宿主回合构建规范有序 Core 时间线。
/// `request` is unchanged; `completed_at_ns` fixes ordering and `policy` is frozen with the intake snapshot.
/// `request` 保持不变；`completed_at_ns` 确定顺序，`policy` 与入库快照一同冻结。
/// Returns cleaned user/intermediate/final fragments or an explicit policy/sanitizer error.
/// 返回清洗后的用户/中间/最终片段，或明确的策略/清洗错误。
fn post_action_timeline(
    request: &PostActionRequest,
    completed_at_ns: i64,
    policy: &vulcan_memorymesh_core::PostActionTextPolicy,
) -> Result<Vec<TimelineItem>, VmmClientError> {
    let item_count = request.timeline.len().saturating_add(2);
    let offset = i64::try_from(item_count).map_err(identifier_error)?;
    let mut occurred_at_ns = completed_at_ns.saturating_sub(offset);
    let mut timeline = Vec::with_capacity(item_count);
    timeline.push(TimelineItem {
        role: Role::User,
        content: sanitize_native_postaction_fragment(&request.user_content, policy)?,
        occurred_at_ns,
    });
    for item in &request.timeline {
        occurred_at_ns = occurred_at_ns.saturating_add(1);
        timeline.push(TimelineItem {
            role: match item.kind {
                PostActionTimelineType::User => Role::User,
                PostActionTimelineType::Assistant => Role::Assistant,
            },
            content: sanitize_native_postaction_fragment(&item.content, policy)?,
            occurred_at_ns,
        });
    }
    if !request.assistant_content.is_empty() {
        timeline.push(TimelineItem {
            role: Role::Assistant,
            content: sanitize_native_postaction_fragment(&request.assistant_content, policy)?,
            occurred_at_ns: completed_at_ns,
        });
    }
    Ok(timeline)
}

/// Cleans one native PostAction fragment before queue/storage admission; reports cleaner errors explicitly.
/// 在原生 PostAction 队列及存储准入前清洗一个文本片段；明确报告清洗器错误。
/// Input remains independent from other speakers; the frozen policy returns text matching the external default when unchanged.
/// 输入与其他发言者保持独立；冻结策略未修改时返回匹配外置默认的文本。
fn sanitize_native_postaction_fragment(
    content: &str,
    policy: &vulcan_memorymesh_core::PostActionTextPolicy,
) -> Result<String, VmmClientError> {
    vulcan_memorymesh_core::sanitize_postaction_text_with_policy(content, policy)
        .map_err(|error| VmmClientError::new("native_postaction_sanitizer", error))
}

/// Projects one durable Core turn into the external VMM DETAILS/RAW_TURN context contract.
/// 将一个耐久 Core 回合投影为外部 VMM 的 DETAILS/RAW_TURN 上下文契约。
///
/// # Parameters
/// 参数
/// `turn` is one exact same-session durable turn.
/// `turn` 是同会话中的一个准确耐久回合。
///
/// # Returns
/// 返回值
/// The extracted detail when completed and available, otherwise compact dehydrated JSON.
/// 已完成且存在提炼内容时返回详情，否则返回紧凑脱水 JSON。
///
/// # Errors
/// 错误
/// Returns an error when a stored role cannot be represented by the host conversation contract or JSON serialization fails.
/// 当存储角色无法由宿主会话契约表示或 JSON 序列化失败时返回错误。
fn precheck_turn_context(turn: Turn) -> Result<PreCheckTurnContext, VmmClientError> {
    if turn.status == vulcan_memorymesh_core::TurnStatus::Completed
        && turn
            .analysis_details
            .as_deref()
            .is_some_and(|details| !details.trim().is_empty())
    {
        return Ok(PreCheckTurnContext {
            turn_id: turn.id,
            content_type: PreCheckTurnContentType::Details,
            content: turn.analysis_details.unwrap_or_default().trim().to_owned(),
        });
    }
    // Share endpoint authority with L1 and details; an intermediate ask is never the final answer.
    // 与 L1 及详情共用端点权威；中间 ask 绝不会被视为最终回答。
    let dialogue = super::dialogue::native_turn_dialogue(&turn).map_err(core_client_error)?;
    let turn_id = turn.id;
    let content = serde_json::to_string(&dialogue).map_err(|error| {
        VmmClientError::new(
            "native_turn_serialization",
            format!("failed to serialize native VMM recent turn: {error}"),
        )
    })?;
    Ok(PreCheckTurnContext {
        turn_id,
        content_type: PreCheckTurnContentType::RawTurn,
        content: super::dialogue::escape_business_json(content),
    })
}

/// Converts one Core search hit into the AI-facing VMM result projection.
/// 将一个 Core 检索命中转换为面向 AI 的 VMM 结果投影。
fn memory_search_hit(
    hit: &vulcan_memorymesh_core::SearchHit,
) -> Result<MemorySearchHit, VmmClientError> {
    Ok(MemorySearchHit {
        memory_id: parse_core_numeric_id(&hit.memory.id, "vmm-memory-")?,
        source_turn_id: optional_core_numeric_id(hit.memory.source_turn_id.as_ref(), "vmm-turn-")?,
        abstract_text: hit.memory.summary.clone(),
        details_preview: hit.memory.content.clone(),
        category: memory_category_label(hit.memory.category).to_owned(),
        created_datetime: format_unix_ns(hit.memory.created_at_ns),
        retrieval: hit.explanation.as_ref().map(|explanation| {
            // Only the Core's recorded stages authorize provenance; absent facts remain absent.
            // 只有 Core 记录的阶段授权来源说明；缺失事实保持缺失。
            let origin = super::retrieval_origin::describe_retrieval_origin(Some(explanation));
            vulcan_vmm::MemorySearchRetrieval {
                source_kind: super::prompt::memory_source_label(hit.memory.source_kind).to_owned(),
                origin: origin.origin,
                origin_label: origin.label,
                origin_explanation: origin.explanation,
                final_score: hit.final_score,
                context_observed: explanation.context_delta.is_some(),
                support_count: explanation.context_total_support_count,
                rebuttal_count: explanation.context_total_rebuttal_count,
                matched_context_values: explanation.context_matched_values.clone(),
                matched_context_support_count: explanation.context_matched_support_count,
                matched_context_rebuttal_count: explanation.context_matched_rebuttal_count,
                matched_context_score_delta: explanation.context_delta,
            }
        }),
    })
}

/// Converts one canonical Core turn into the service detail projection.
/// 将一个规范 Core 回合转换为服务详情投影。
fn turn_detail_entry(turn: Turn) -> Result<TurnDetailEntry, VmmClientError> {
    // Reuse the same canonical decomposition as L1 and PreCheck RAW_TURN.
    // 复用与 L1、PreCheck RAW_TURN 相同的规范拆分。
    let dialogue = super::dialogue::native_turn_dialogue(&turn).map_err(core_client_error)?;
    Ok(TurnDetailEntry {
        turn_id: parse_core_numeric_id(&turn.id, "vmm-turn-")?,
        user_question: dialogue.user,
        timeline: dialogue.timeline,
        assistant_answer: dialogue.assistant,
        detail: turn.analysis_details.unwrap_or_default(),
        previous_turn_ids: turn
            .previous_turn_ids
            .iter()
            .map(|id| parse_core_numeric_id(id, "vmm-turn-"))
            .collect::<Result<Vec<_>, _>>()?,
        next_turn_ids: turn
            .next_turn_ids
            .iter()
            .map(|id| parse_core_numeric_id(id, "vmm-turn-"))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

/// Converts one Core-generated opaque identity into its exact numeric service projection.
/// 将一个 Core 生成的不透明身份转换为精确数字服务投影。
pub(super) fn parse_core_numeric_id(
    value: &impl std::fmt::Display,
    prefix: &str,
) -> Result<u64, VmmClientError> {
    let text = value.to_string();
    let numeric = text.strip_prefix(prefix).ok_or_else(|| {
        VmmClientError::new(
            "native_identity_mismatch",
            format!("Core identity {text} does not start with {prefix}"),
        )
    })?;
    numeric.parse::<u64>().map_err(|error| {
        VmmClientError::new(
            "native_identity_mismatch",
            format!("Core identity {text} has an invalid numeric suffix: {error}"),
        )
    })
}

/// Converts an optional Core identity into zero-or-exact numeric projection semantics.
/// 将可选 Core 身份转换为零或精确数字投影语义。
fn optional_core_numeric_id<T: std::fmt::Display>(
    value: Option<&T>,
    prefix: &str,
) -> Result<u64, VmmClientError> {
    value.map_or(Ok(0), |value| parse_core_numeric_id(value, prefix))
}

/// Maps one public numeric category onto the closed native Core category set.
/// 将一个公共数字分类映射到封闭的原生 Core 分类集合。
fn memory_category(value: i32) -> Result<MemoryCategory, VmmClientError> {
    match value {
        0 => Ok(MemoryCategory::General),
        1 => Ok(MemoryCategory::ArchitectureDecision),
        2 => Ok(MemoryCategory::TechSpecApi),
        3 => Ok(MemoryCategory::BusinessLogic),
        4 => Ok(MemoryCategory::RequirementTodo),
        5 => Ok(MemoryCategory::ProjectContext),
        6 => Ok(MemoryCategory::LogicalBugDebt),
        7 => Ok(MemoryCategory::SecurityPolicy),
        _ => Err(VmmClientError::new(
            "invalid_argument",
            format!("unsupported native memory category {value}"),
        )),
    }
}

/// Returns the stable service label for one closed Core memory category.
/// 返回一个封闭 Core 记忆分类的稳定服务标签。
const fn memory_category_label(value: MemoryCategory) -> &'static str {
    match value {
        MemoryCategory::General => "GENERAL",
        MemoryCategory::ArchitectureDecision => "ARCHITECTURE_DECISION",
        MemoryCategory::TechSpecApi => "TECH_SPEC_API",
        MemoryCategory::BusinessLogic => "BUSINESS_LOGIC",
        MemoryCategory::RequirementTodo => "REQUIREMENT_TODO",
        MemoryCategory::ProjectContext => "PROJECT_CONTEXT",
        MemoryCategory::LogicalBugDebt => "LOGICAL_BUG_DEBT",
        MemoryCategory::SecurityPolicy => "SECURITY_POLICY",
    }
}

/// Maps the public priority enum onto Core's normalized priority scale.
/// 将公共优先级枚举映射到 Core 的规范化优先级标度。
fn memory_priority(value: i32) -> Result<Option<u32>, VmmClientError> {
    match value {
        0 => Ok(None),
        1 => Ok(Some(90)),
        2 => Ok(Some(60)),
        3 => Ok(Some(30)),
        _ => Err(VmmClientError::new(
            "invalid_argument",
            format!("unsupported native memory priority {value}"),
        )),
    }
}

/// Maps the public memory level onto the closed Core lifecycle set.
/// 将公共记忆层级映射到封闭的 Core 生命周期集合。
fn memory_level(value: i32) -> Result<Option<MemoryLevel>, VmmClientError> {
    match value {
        0 => Ok(None),
        1 => Ok(Some(MemoryLevel::Session)),
        2 => Ok(Some(MemoryLevel::Phase)),
        3 => Ok(Some(MemoryLevel::Stable)),
        4 => Ok(Some(MemoryLevel::Persistent)),
        _ => Err(VmmClientError::new(
            "invalid_argument",
            format!("unsupported native memory level {value}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_vmm::PostActionRequest;

    /// Verifies profile queries use the operating system's IANA timezone provider without fallback.
    /// 验证画像查询使用操作系统的 IANA 时区提供器，且不会静默回退。
    #[test]
    fn native_profile_display_timezone_matches_iana_provider() {
        let expected = iana_time_zone::get_timezone().expect("local IANA timezone");

        assert_eq!(
            native_profile_display_timezone().expect("native profile timezone"),
            expected
        );
    }

    /// Verifies the process-local client cannot open a second native user namespace.
    /// 验证进程内客户端不能打开第二个原生用户命名空间。
    #[test]
    fn native_subject_identity_is_fixed_to_the_host_user() {
        assert_eq!(
            subject_id(DEFAULT_NATIVE_USER_ID)
                .expect("fixed native subject")
                .as_str(),
            "vmm-subject:1"
        );
        assert_eq!(
            subject_scope_id(DEFAULT_NATIVE_USER_ID)
                .expect("fixed native subject scope")
                .as_str(),
            "vmm-subject:1"
        );
        let subject_error = subject_id(DEFAULT_NATIVE_USER_ID + 1)
            .expect_err("second native subject must be rejected");
        let scope_error = subject_scope_id(DEFAULT_NATIVE_USER_ID + 1)
            .expect_err("second native subject scope must be rejected");
        assert_eq!(subject_error.kind, "invalid_argument");
        assert_eq!(scope_error.kind, "invalid_argument");
    }

    /// Verifies durable analysis retry delay grows exponentially and remains bounded.
    /// 验证耐久分析重试延迟按指数增长且保持有界。
    #[test]
    fn pending_analysis_retry_delay_is_bounded() {
        assert_eq!(pending_analysis_retry_delay_ms(1), 1_000);
        assert_eq!(pending_analysis_retry_delay_ms(2), 2_000);
        assert_eq!(pending_analysis_retry_delay_ms(9), 256_000);
        assert_eq!(pending_analysis_retry_delay_ms(10), 300_000);
        assert_eq!(pending_analysis_retry_delay_ms(u32::MAX), 300_000);
    }

    /// Verifies persisted diagnostics use Unicode character bounds instead of byte truncation.
    /// 验证持久诊断使用 Unicode 字符边界而非字节截断。
    #[test]
    fn pending_analysis_diagnostic_preserves_unicode_boundaries() {
        let message = "错".repeat(VMM_PENDING_ANALYSIS_ERROR_CHAR_LIMIT + 1);
        let bounded = bounded_diagnostic(&message);
        assert_eq!(
            bounded.chars().count(),
            VMM_PENDING_ANALYSIS_ERROR_CHAR_LIMIT
        );
        assert!(message.starts_with(&bounded));
    }

    /// Verifies malformed jobs isolate immediately while ordinary analysis uses the five-pass policy.
    /// 验证损坏作业立即隔离，而普通分析使用五次失败放行策略。
    #[test]
    fn pending_analysis_terminal_policy_matches_external_vmm() {
        assert_eq!(
            pending_analysis_terminal_status(
                &PendingAnalysisJobError::Corrupted(String::from("bad payload")),
                0,
            ),
            Some(TurnStatus::Corrupted)
        );
        let analysis = PendingAnalysisJobError::Analysis {
            diagnostic: String::from("invalid model output"),
            outcome_uncertain: false,
        };
        assert_eq!(pending_analysis_terminal_status(&analysis, 3), None);
        assert_eq!(
            pending_analysis_terminal_status(&analysis, 4),
            Some(TurnStatus::AnalysisBypassed)
        );
        assert_eq!(
            pending_analysis_terminal_status(
                &PendingAnalysisJobError::Analysis {
                    diagnostic: String::from("uncertain commit"),
                    outcome_uncertain: true,
                },
                0,
            ),
            Some(TurnStatus::AnalysisBypassed)
        );
        assert_eq!(
            pending_analysis_terminal_status(
                &PendingAnalysisJobError::Retryable(String::from("route unavailable")),
                u32::MAX,
            ),
            None
        );
    }

    /// Verifies the host failure policy changes only a pending turn and preserves recovered terminals.
    /// 验证宿主失败策略只改变待处理回合，并保留恢复得到的既有终态。
    #[test]
    fn pending_turn_terminal_transition_is_strict_and_idempotent() {
        let turn_id = TurnId::new("vmm-turn-terminal").expect("turn id");
        let mut snapshot = vulcan_memorymesh_core::StorageSnapshot {
            dataset: vulcan_memorymesh_core::ExportDataset {
                turns: vec![Turn {
                    id: turn_id.clone(),
                    subject_id: subject_id(1).expect("subject"),
                    scope_id: project_scope_id(1).expect("scope"),
                    session_source_id: session_source_id("session").expect("session"),
                    timeline: Vec::new(),
                    previous_turn_ids: Vec::new(),
                    next_turn_ids: Vec::new(),
                    created_at_ns: 1,
                    status: TurnStatus::Pending,
                    user_input_kind: None,
                    analysis_details: None,
                }],
                ..vulcan_memorymesh_core::ExportDataset::empty()
            },
            storage_version: 1,
            next_entity_sequence: 1,
            outcomes: Vec::new(),
            analysis_attempts: Vec::new(),
            memory_sessions: Vec::new(),
            noise_prototypes: Vec::new(),
            retention_trash_batches: Vec::new(),
            retention_cold_turn_jobs: Vec::new(),
        };

        assert_eq!(
            transition_pending_turn_status(
                &mut snapshot,
                &turn_id,
                TurnStatus::AnalysisBypassed,
                "operation",
            )
            .expect("transition pending turn"),
            TurnStatus::AnalysisBypassed
        );
        assert_eq!(
            transition_pending_turn_status(
                &mut snapshot,
                &turn_id,
                TurnStatus::Corrupted,
                "operation",
            )
            .expect("preserve terminal turn"),
            TurnStatus::AnalysisBypassed
        );
        snapshot.dataset.turns[0].status = TurnStatus::ProcessedUnknown;
        assert!(
            transition_pending_turn_status(
                &mut snapshot,
                &turn_id,
                TurnStatus::Corrupted,
                "operation",
            )
            .is_err()
        );
    }

    /// Verifies idempotent PostAction retries accept ready and every terminal queue state.
    /// 验证幂等 PostAction 重试接受就绪态及每一种队列终态。
    #[test]
    fn pending_analysis_acceptance_includes_failure_terminal_states() {
        let mut job = NativeVmmPendingAnalysisJob {
            operation_id: String::from("operation"),
            request_fingerprint: String::from("sha256:test"),
            payload_json: String::from("{}"),
            intake_mutation_operation_id: String::from("intake"),
            turn_id: Some(String::from("vmm-turn-1")),
            state: String::from(VMM_PENDING_ANALYSIS_INTAKING),
            attempt_count: 0,
            next_attempt_at_unix_ms: 0,
            last_error: None,
        };
        assert!(!pending_analysis_job_is_accepted(&job));
        for state in [
            VMM_PENDING_ANALYSIS_READY,
            VMM_PENDING_ANALYSIS_COMPLETED,
            VMM_PENDING_ANALYSIS_BYPASSED,
            VMM_PENDING_ANALYSIS_CORRUPTED,
        ] {
            job.state = String::from(state);
            assert!(pending_analysis_job_is_accepted(&job));
        }
    }

    /// Verifies request fingerprints are deterministic, payload-sensitive, and time-independent.
    /// 验证请求指纹具有确定性、对载荷变化敏感且不依赖宿主时间。
    #[test]
    fn pending_analysis_request_fingerprint_is_stable() {
        let request = PostActionRequest {
            operation_id: String::from("turn-operation"),
            session_id: String::from("session"),
            user_content: String::from("question"),
            assistant_content: String::from("answer"),
            ..PostActionRequest::default()
        };
        let first = pending_analysis_request_fingerprint(&request).expect("fingerprint request");
        let repeated =
            pending_analysis_request_fingerprint(&request).expect("repeat fingerprint request");
        let mut changed_request = request;
        changed_request.assistant_content = String::from("different answer");
        let changed = pending_analysis_request_fingerprint(&changed_request)
            .expect("fingerprint changed request");
        assert_eq!(first, repeated);
        assert_ne!(first, changed);
        assert_eq!(first.len(), "sha256:".len() + 64);
    }

    /// Verifies intake cleans all three business fragments independently and preserves timeline ownership.
    /// 验证准入独立清洗三类业务片段，并保留时间线归属。
    #[test]
    fn postaction_timeline_cleans_fragments_before_native_persistence() {
        // Hidden thoughts and inline image data must not enter the durable native timeline.
        // 隐藏思考及内联图片数据不得进入原生持久时间线。
        let request = PostActionRequest {
            user_content:
                "<think>private</think>\n我喜欢西瓜\n\n![照片](data:image/png;base64,QQ==)"
                    .to_owned(),
            assistant_content: "<analysis>private</analysis>\n回答\n  保留行".to_owned(),
            timeline: vec![vulcan_vmm::protocol::PostActionTimelineItem {
                kind: PostActionTimelineType::Assistant,
                content: "选择？\n<a href='report.pdf'>报告</a>".to_owned(),
            }],
            ..PostActionRequest::default()
        };
        // The caller's request remains unchanged so admission fingerprints still represent source input.
        // 调用方请求保持不变，因此准入指纹仍表示来源输入。
        let before = pending_analysis_request_fingerprint(&request).expect("source fingerprint");
        // Use a deterministic completion timestamp to assert exact ordering independent of wall time.
        // 使用确定性完成时间戳，独立于墙钟断言准确顺序。
        let timeline = post_action_timeline(
            &request,
            100,
            &vulcan_memorymesh_core::PostActionTextPolicy::default(),
        )
        .expect("sanitized timeline");
        assert_eq!(timeline.len(), 3);
        assert_eq!(timeline[0].role, Role::User);
        assert_eq!(timeline[0].content, "我喜欢西瓜\n\n[Image: 照片]");
        assert_eq!(timeline[1].content, "选择？\n[File: 报告]");
        assert_eq!(timeline[2].content, "回答\n保留行");
        assert_eq!(timeline[2].occurred_at_ns, 100);
        assert!(
            timeline
                .windows(2)
                .all(|pair| pair[0].occurred_at_ns < pair[1].occurred_at_ns)
        );
        assert_eq!(
            pending_analysis_request_fingerprint(&request).expect("unchanged fingerprint"),
            before
        );
    }

    /// Verifies a saved user text policy reaches the production timeline builder for every fragment.
    /// 验证已保存用户文本策略抵达生产时间线构造器的每个片段。
    #[test]
    fn saved_text_policy_controls_new_timeline_without_recleaning_frozen_input() {
        let directory = tempfile::tempdir().expect("user root");
        let store = super::super::resources::NativeVmmResourceStore::new(directory.path());
        let core = vulcan_memorymesh_core::MemoryMeshCore::new().expect("core");
        store
            .update_text_policy(
                &core,
                vulcan_service_api::VmmNativeTextPolicySummary {
                    fold_machine_text: false,
                    max_fragment_tokens: 20_000,
                },
            )
            .expect("save large target");
        let frozen = core.active_snapshot().expect("admitted snapshot");
        let prose = "我喜欢西瓜和葡萄，希望记住这个偏好。\n".repeat(200);
        let request = PostActionRequest {
            user_content: format!("<think>hidden</think>\n{prose}"),
            assistant_content: prose.clone(),
            timeline: vec![vulcan_vmm::protocol::PostActionTimelineItem {
                kind: PostActionTimelineType::User,
                content: prose.clone(),
            }],
            ..PostActionRequest::default()
        };
        let original = post_action_timeline(&request, 100, &frozen.memory_policy.postaction_text)
            .expect("large timeline");
        assert!(original.iter().all(|item| item.content == prose.trim()));
        store
            .update_text_policy(
                &core,
                vulcan_service_api::VmmNativeTextPolicySummary {
                    fold_machine_text: false,
                    max_fragment_tokens: 256,
                },
            )
            .expect("save small target");
        let small = core.active_snapshot().expect("next admitted snapshot");
        let reduced = post_action_timeline(&request, 100, &small.memory_policy.postaction_text)
            .expect("small timeline");
        assert!(
            reduced
                .iter()
                .all(|item| item.content.contains("[truncated by token budget]"))
        );
        assert!(reduced.iter().all(|item| !item.content.contains("hidden")));
        assert_eq!(
            post_action_timeline(&request, 100, &frozen.memory_policy.postaction_text)
                .expect("frozen repeat"),
            original
        );
        assert_eq!(
            reduced
                .iter()
                .map(|item| (&item.role, item.occurred_at_ns))
                .collect::<Vec<_>>(),
            original
                .iter()
                .map(|item| (&item.role, item.occurred_at_ns))
                .collect::<Vec<_>>()
        );
    }

    /// Builds a native turn through the real admission timeline builder, not invented endpoint timestamps.
    /// 通过真实准入时间线构造器建立原生回合，不编造端点时间戳。
    fn native_dialogue_fixture(assistant: &str) -> Turn {
        // The middle ask is deliberately the last interaction before the optional final answer.
        // 中间 ask 特意作为可选最终回答之前的最后一次交互。
        let request = PostActionRequest {
            user_content: "我喜欢西瓜和葡萄".to_owned(),
            assistant_content: assistant.to_owned(),
            timeline: vec![vulcan_vmm::PostActionTimelineItem {
                kind: PostActionTimelineType::Assistant,
                content: "需要比较哪一种？".to_owned(),
            }],
            ..PostActionRequest::default()
        };
        Turn {
            id: turn_id(42).expect("turn id"),
            subject_id: subject_id(1).expect("subject"),
            scope_id: project_scope_id(1).expect("project"),
            session_source_id: session_source_id("dialogue-session").expect("session"),
            timeline: post_action_timeline(
                &request,
                100,
                &vulcan_memorymesh_core::PostActionTextPolicy::default(),
            )
            .expect("real native timeline"),
            previous_turn_ids: Vec::new(),
            next_turn_ids: Vec::new(),
            created_at_ns: 100,
            status: TurnStatus::Pending,
            user_input_kind: None,
            analysis_details: None,
        }
    }

    /// Verifies L1, PreCheck raw history, and user details share exact endpoint identity and minimal payloads.
    /// 验证 L1、PreCheck 原始历史及用户详情共用准确端点身份和最小业务载荷。
    #[test]
    fn native_dialogue_projection_never_promotes_middle_ask_to_final_answer() {
        for (assistant, expected) in [
            ("", ""),
            ("最终回答", "最终回答"),
            ("<think>private</think>", ""),
        ] {
            // Exercise absent, present, and entirely sanitized final answers from the actual native builder.
            // 从真实原生构造器覆盖缺失、存在及完全被清洗的最终回答。
            let turn = native_dialogue_fixture(assistant);
            // The user-facing details must retain the ask as a middle interaction in every case.
            // 所有情况下，面向用户的详情都必须将 ask 保留为中间交互。
            let details = turn_detail_entry(turn.clone()).expect("details");
            assert_eq!(details.assistant_answer, expected);
            assert_eq!(details.timeline.len(), 1);
            assert_eq!(details.timeline[0].content, "需要比较哪一种？");
            // L2 sees only the main endpoints; it must not merge middle asks into assistant_content.
            // L2 只看到主端点；不能将中间 ask 合并进 assistant_content。
            let review = super::super::prompt::render_llm_input_at(
                &vulcan_memorymesh_core::LlmInput::PostActionReview {
                    context: vulcan_memorymesh_core::PostActionReviewContext::Turn {
                        target_turn: Box::new(turn.clone()),
                        user_input_kind: vulcan_memorymesh_core::TurnInputKind::Statement,
                        profile_sections: Vec::new(),
                    },
                    memory_candidates: Vec::new(),
                    profile_candidates: Vec::new(),
                    current_profile_nodes: Vec::new(),
                },
                86_400_000_000_000,
            )
            .expect("L2 body");
            // Empty final answers are omitted under the external L2 contract, not replaced by the ask.
            // 外置 L2 契约省略空最终回答，而非使用 ask 替代。
            let review_json: serde_json::Value = serde_json::from_str(&review).expect("L2 JSON");
            assert_eq!(review_json["user_content"], "我喜欢西瓜和葡萄");
            assert_ne!(
                review_json["current_datetime"],
                review_json["current_turn_datetime"]
            );
            assert_eq!(
                review_json
                    .get("assistant_content")
                    .and_then(serde_json::Value::as_str),
                (!expected.is_empty()).then_some(expected)
            );
            // Compare actual serialized PreCheck and PostAction bodies rather than a helper-only result.
            // 比较实际序列化的 PreCheck 与 PostAction 正文，而非仅比较辅助函数结果。
            let raw_context = precheck_turn_context(turn.clone()).expect("raw context");
            // Parse the compact history JSON for equality with L1's embedded raw_turn object.
            // 解析紧凑历史 JSON，与 L1 内嵌的 raw_turn 对象比较。
            let raw: serde_json::Value =
                serde_json::from_str(&raw_context.content).expect("raw JSON");
            // L1 is rendered through the same function invoked by the live host AI driver.
            // L1 经真实宿主 AI 驱动调用的同一函数渲染。
            let body = super::super::prompt::render_llm_input(
                &vulcan_memorymesh_core::LlmInput::PostActionAnalysis {
                    max_input_tokens: 6000,
                    current_timestamp_ns: 200,
                    reference_turns: Vec::new(),
                    target_turn: Box::new(turn),
                    recent_direct_writes: Vec::new(),
                },
            )
            .expect("L1 body");
            // Assert the exact business object to prevent future metadata leakage into model inputs.
            // 断言准确业务对象，防止未来向模型输入泄露内部元数据。
            let value: serde_json::Value = serde_json::from_str(&body).expect("L1 JSON");
            assert_eq!(value["target_turn"]["raw_turn"], raw);
            assert_eq!(
                raw,
                serde_json::json!({
                    "user": "我喜欢西瓜和葡萄", "assistant": expected,
                    "timeline": [{"type": "assistant", "content": "需要比较哪一种？"}]
                })
            );
        }
    }

    /// Verifies malformed role/order data fails explicitly instead of being guessed into a business payload.
    /// 验证角色或顺序损坏的数据明确失败，不被猜测拼成业务载荷。
    #[test]
    fn native_dialogue_projection_rejects_noncanonical_boundaries() {
        // Independently corrupt each owned invariant from a valid native source fixture.
        // 从合法原生来源样本分别破坏各项权威约束。
        let base = native_dialogue_fixture("回答");
        for corruption in 0..4 {
            // Keep every case independent so a previous fault cannot hide the next validation.
            // 每个用例保持独立，避免前一错误掩盖后一校验。
            let mut turn = base.clone();
            match corruption {
                0 => turn.timeline[0].role = Role::Assistant,
                1 => turn.timeline[1].role = Role::Tool,
                2 => turn.timeline[1].occurred_at_ns = turn.timeline[0].occurred_at_ns,
                _ => turn.timeline[2].occurred_at_ns += 1,
            }
            assert!(super::super::dialogue::native_turn_dialogue(&turn).is_err());
        }
    }

    /// Verifies actual serialized target overhead and external zero-budget/latest-history boundary semantics.
    /// 验证实际序列化目标开销，以及外置零剩余预算和最新历史边界语义。
    #[test]
    fn postaction_history_budget_uses_actual_business_json() {
        // Generate native timestamps and sanitized endpoint content through the production builder.
        // 通过生产构造器生成原生时间戳及清洗后的端点内容。
        let turn = native_dialogue_fixture("答复");
        // Extract exactly the raw_turn object emitted to L1 so the budget includes every JSON wrapper.
        // 提取真正发送给 L1 的 raw_turn 对象，使预算包含全部 JSON 包装。
        let raw = serde_json::to_value(
            super::super::dialogue::native_turn_dialogue(&turn).expect("dialogue"),
        )
        .expect("target JSON");
        // The budget unit is the serialized business object, not the Core Turn or only its text leaves.
        // 预算单位为序列化业务对象，不是 Core Turn，也不是仅正文叶节点。
        let target_cost =
            estimate_text_tokens(&serde_json::to_string(&raw).expect("serialize target"));
        assert!(
            target_cost
                > turn
                    .timeline
                    .iter()
                    .map(|item| estimate_text_tokens(&item.content))
                    .sum::<u32>()
        );
        // Historical rows are ordered oldest to newest and share the exact native session owner.
        // 历史行按从旧到新排列，并共享准确的原生会话归属。
        let references = (1..=2)
            .map(|number| vulcan_memorymesh_core::PostActionReferenceTurn {
                turn_id: turn_id(number).expect("history ID"),
                subject_id: turn.subject_id.clone(),
                scope_id: turn.scope_id.clone(),
                session_source_id: turn.session_source_id.clone(),
                details: format!("历史事实第{number}条"),
                created_at_ns: number as i64,
            })
            .collect::<Vec<_>>();
        // Keep each exact reference cost to exercise the integer boundary, not a rough large/small budget.
        // 保存每条准确参考成本，覆盖整数边界而非粗略的大预算或小预算。
        let latest_cost = estimate_text_tokens(&references[1].details);
        // The earlier reference joins only when the remaining budget covers both details.
        // 仅在剩余预算覆盖两条详情时，更早参考才能进入。
        let older_cost = estimate_text_tokens(&references[0].details);
        for (max_input_tokens, expected_ids) in [
            (0, vec!["vmm-turn-1", "vmm-turn-2"]),
            (target_cost - 1, vec![]),
            (target_cost, vec![]),
            (target_cost + 1, vec!["vmm-turn-2"]),
            (target_cost + latest_cost, vec!["vmm-turn-2"]),
            (
                target_cost + latest_cost + older_cost - 1,
                vec!["vmm-turn-2"],
            ),
            (
                target_cost + latest_cost + older_cost,
                vec!["vmm-turn-1", "vmm-turn-2"],
            ),
        ] {
            // Render the production request and inspect the actual selected history IDs.
            // 渲染生产请求并检查实际选中的历史 ID。
            let body = super::super::prompt::render_llm_input(
                &vulcan_memorymesh_core::LlmInput::PostActionAnalysis {
                    max_input_tokens,
                    current_timestamp_ns: 200,
                    reference_turns: references.clone(),
                    target_turn: Box::new(turn.clone()),
                    recent_direct_writes: Vec::new(),
                },
            )
            .expect("budgeted L1 body");
            // Assert the rendered target is unchanged by history trimming.
            // 断言历史裁剪不改变已渲染目标。
            let value: serde_json::Value = serde_json::from_str(&body).expect("L1 JSON");
            assert_eq!(value["target_turn"]["raw_turn"], raw);
            assert_eq!(
                value["reference_turns"]
                    .as_array()
                    .expect("history array")
                    .iter()
                    .map(|row| row["turn_id"].as_str().expect("history ID"))
                    .collect::<Vec<_>>(),
                expected_ids,
                "budget {max_input_tokens}"
            );
        }
    }

    /// Verifies maintenance scope detection follows the frozen intake and analysis owner.
    /// 验证维护作用域检测遵循冻结的登记与分析所有者。
    #[test]
    fn pending_analysis_scope_detection_rejects_inconsistent_payloads() {
        // sourceScope and otherScope represent the maintenance target and an unrelated project.
        // sourceScope 与 otherScope 分别表示维护目标和无关项目。
        let source_scope = project_scope_id(7).expect("source scope");
        let other_scope = project_scope_id(8).expect("other scope");
        let subject = subject_id(1).expect("subject");
        let session = session_source_id("session").expect("session");
        let vector_space = VectorSpaceId::new("test-vector-space").expect("vector space");
        let payload = NativePendingPostActionPayload {
            host_project_id: "host-project-fixture".to_owned(),
            intake: PostActionIntakeCommand {
                input_kind: PostActionInputKind::SingleTurn,
                subject_id: subject.clone(),
                scope_access: ScopeAccess {
                    write_scope: source_scope.clone(),
                    visible_scopes: vec![source_scope.clone()],
                },
                session_source_id: session.clone(),
                vector_space_id: vector_space.clone(),
                timeline: Vec::new(),
                completed_at_ns: 1,
                mutation_identity: mutation_identity(
                    "scope-test-intake",
                    MutationSourceKind::HostRpc,
                    "scope-test",
                    1,
                )
                .expect("intake identity"),
            },
            analysis: NativePendingAnalysisTemplate {
                replacement_scope: MemoryReplacementScope {
                    candidates: MemoryCandidateScope {
                        scope_ids: vec![source_scope.clone()],
                        origin_session_source_id: None,
                    },
                    authorized_supersede_scopes: vec![source_scope.clone()],
                },
                subject_id: subject,
                scope_access: ScopeAccess {
                    write_scope: source_scope.clone(),
                    visible_scopes: vec![source_scope.clone()],
                },
                profile_sections: Vec::new(),
                session_source_id: session,
                vector_space_id: vector_space,
                analysis_generation: 1,
                mutation_identity: mutation_identity(
                    "scope-test-analysis",
                    MutationSourceKind::BackgroundWorker,
                    "scope-test",
                    1,
                )
                .expect("analysis identity"),
            },
        };
        let mut job = NativeVmmPendingAnalysisJob {
            operation_id: String::from("scope-test"),
            request_fingerprint: String::from("sha256:test"),
            payload_json: serde_json::to_string(&payload).expect("serialize payload"),
            intake_mutation_operation_id: String::from("scope-test-intake"),
            turn_id: Some(String::from("vmm-turn-1")),
            state: String::from(VMM_PENDING_ANALYSIS_READY),
            attempt_count: 0,
            next_attempt_at_unix_ms: 0,
            last_error: None,
        };

        assert!(pending_analysis_job_targets_scope(&job, &source_scope).expect("source match"));
        assert!(!pending_analysis_job_targets_scope(&job, &other_scope).expect("other mismatch"));
        // The originating caller survives durable replay and cannot be guessed from the memory scope.
        // 发起调用方在耐久重放后保留，不能由记忆作用域猜测。
        let replayed: NativePendingPostActionPayload =
            serde_json::from_str(&job.payload_json).expect("replayed payload");
        assert_eq!(replayed.host_project_id, "host-project-fixture");
        // Delayed execution refreshes only its observation clock, preserving the retry identity and source.
        // 延迟执行仅刷新观察时钟，保留重试身份及来源。
        let command = replayed
            .analysis
            .into_command(turn_id(1).expect("turn"), 100, 50, 0)
            .expect("first attempt");
        assert_eq!(command.analysis_cutoff_ns, 100);
        assert_eq!(command.direct_write_observed_after_ns, 50);
        assert_eq!(command.mutation_identity.observed_at_ns, 100);
        assert_eq!(
            command.mutation_identity.operation_id,
            derive_mutation_identity(&payload.analysis.mutation_identity, "attempt-1")
                .expect("first identity")
                .operation_id
        );
        assert_eq!(
            command.mutation_identity.idempotency_key,
            derive_mutation_identity(&payload.analysis.mutation_identity, "attempt-1")
                .expect("first identity")
                .idempotency_key
        );
        assert_eq!(
            command.mutation_identity.source_kind,
            payload.analysis.mutation_identity.source_kind
        );
        assert_eq!(
            command.mutation_identity.request_source_id,
            payload.analysis.mutation_identity.request_source_id
        );
        assert_eq!(payload.analysis.mutation_identity.observed_at_ns, 1);
        // Retry generation and child identity advance together, while replay of one attempt remains deterministic.
        // 重试代次及子身份同时推进，同一次尝试的重放仍保持确定性。
        let retry = payload
            .analysis
            .clone()
            .into_command(turn_id(1).expect("turn"), 200, 50, 1)
            .expect("retry");
        assert_eq!(command.analysis_generation, 1);
        assert_eq!(retry.analysis_generation, 2);
        assert_ne!(
            retry.mutation_identity.operation_id,
            command.mutation_identity.operation_id
        );
        assert_ne!(
            retry.mutation_identity.idempotency_key,
            command.mutation_identity.idempotency_key
        );
        assert_eq!(
            retry,
            payload
                .analysis
                .clone()
                .into_command(turn_id(1).expect("turn"), 200, 50, 1)
                .expect("same retry")
        );
        let mut overflow = payload.analysis.clone();
        overflow.analysis_generation = u64::MAX;
        assert!(
            overflow
                .into_command(turn_id(1).expect("turn"), 200, 50, 1)
                .is_err()
        );
        let mut missing_caller = serde_json::to_value(&payload).expect("payload object");
        missing_caller
            .as_object_mut()
            .expect("object")
            .remove("host_project_id");
        assert!(serde_json::from_value::<NativePendingPostActionPayload>(missing_caller).is_err());

        // inconsistentPayload simulates durable corruption between intake and analysis ownership.
        // inconsistentPayload 模拟登记与分析归属之间的耐久损坏。
        let mut inconsistent_payload = payload;
        inconsistent_payload.analysis.scope_access.write_scope = other_scope;
        job.payload_json =
            serde_json::to_string(&inconsistent_payload).expect("serialize inconsistent payload");
        assert!(pending_analysis_job_targets_scope(&job, &source_scope).is_err());
    }
}
