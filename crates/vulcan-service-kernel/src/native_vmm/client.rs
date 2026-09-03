//! Native process-local VMM client implemented directly over the copied Core.
//! 直接基于复制 Core 实现的原生进程内 VMM 客户端。

use super::NativeVmmCoreRuntime;
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
    MemoryDeleteBatchCommand, MemoryDeleteBatchItem, MemoryDetailsBatchCommand, MemoryLevel,
    MemoryReplacementScope, MemoryScopeClass, MemorySearchBatchCommand, MemoryWriteBatchCommand,
    MemoryWriteBatchItem, MutationIdentity, MutationSourceKind, OperationId,
    PostActionAnalyzePendingTurnCommand, PostActionInputKind, PostActionIntakeCommand,
    PreCheckCommand, PreCheckPolicy, ProfileBundle, ProfileInstructionCommand, ProfileLevel,
    ProfileNode, ProfileSourceKind, ProfileView, ProfileViewMode, ProfileViewSection, RecallMode,
    RetrievalPolicy, Role, ScopeAccess, ScopeId, SectionRole, SessionRecallMode, SessionSourceId,
    SubjectId, TimelineItem, Turn, TurnId, VectorSpaceId, derive_mutation_identity,
};
use vulcan_vmm::{
    ApplyProfileInstructionRequest, ApplyProfileInstructionResponse, ChatCompactRequest,
    ChatCompactResponse, ContextItem, DeleteMemoriesRequest, DeleteMemoriesResponse,
    DeleteProjectRequest, DeleteProjectResponse, DeleteUserRequest, DeleteUserResponse,
    EnsureProjectRequest, EnsureProjectResponse, GetProfileBundleRequest, GetProfileBundleResponse,
    GetProfileNodesRequest, GetProfileNodesResponse, GetTurnDetailsRequest, GetTurnDetailsResponse,
    HealthzResponse, ListProjectsResponse, ListUsersResponse, MemorySearchGroupResult,
    MemorySearchHit, MigrateProjectRequest, MigrateProjectResponse, PostActionRequest,
    PostActionResponse, PostActionTimelineItem, PostActionTimelineType, PreCheckRecallMode,
    PreCheckRequest, PreCheckResponse, ProfileBundleMode, ProfileNodeEntry, ProfileNodeSourceKind,
    ProfileTarget, ResolveProjectRequest, ResolveProjectResponse, ResolveUserRequest,
    ResolveUserResponse, RetiredProfileNodeEntry, SearchMemoryEventsRequest,
    SearchMemoryEventsResponse, SessionMemoryStatus, TurnDetailEntry, VmmClient, VmmClientError,
    WriteMemoriesRequest, WriteMemoriesResponse, WriteMemoryResultItem,
};
use vulcan_workspace_store::vmm_data_plane::{
    NativeVmmPendingAnalysisJob, VMM_PENDING_ANALYSIS_COMPLETED,
    VMM_PENDING_ANALYSIS_ERROR_CHAR_LIMIT, VMM_PENDING_ANALYSIS_INTAKING,
    VMM_PENDING_ANALYSIS_READY, VmmDataPlaneHandle,
};

/// Stable logical vector-space identity shared by all native project scopes.
/// 全部原生项目作用域共享的稳定逻辑向量空间标识。
const NATIVE_MEMORY_VECTOR_SPACE_ID: &str = "vmm-native:memory";

/// Periodic durable analysis queue scan interval.
/// 耐久分析队列的周期扫描间隔。
const PENDING_ANALYSIS_SCAN_INTERVAL: Duration = Duration::from_secs(30);

/// Maximum jobs loaded by one queue drain pass.
/// 单次队列排空最多加载的作业数。
const PENDING_ANALYSIS_BATCH_LIMIT: u32 = 32;

/// Maximum retry delay for one failed background analysis job.
/// 单个后台分析作业失败后的最大重试延迟。
const PENDING_ANALYSIS_MAX_RETRY_DELAY_MS: i64 = 5 * 60 * 1_000;

/// Stores the exact replayable semantics of one accepted post-action request.
/// 保存一条已接受 PostAction 请求的准确可重放语义。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct NativePendingPostActionPayload {
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
    /// Immutable business-time cutoff.
    /// 不可变业务时间截止点。
    analysis_cutoff_ns: i64,
    /// Durable direct-write watermark.
    /// 耐久主动写入水位。
    direct_write_observed_after_ns: i64,
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
    ///
    /// # Returns
    /// 返回值
    /// A complete replayable pending-turn analysis command.
    /// 返回完整可重放的待处理回合分析命令。
    fn into_command(self, turn_id: TurnId) -> PostActionAnalyzePendingTurnCommand {
        PostActionAnalyzePendingTurnCommand {
            replacement_scope: self.replacement_scope,
            subject_id: self.subject_id,
            scope_access: self.scope_access,
            profile_sections: self.profile_sections,
            session_source_id: self.session_source_id,
            vector_space_id: self.vector_space_id,
            turn_id,
            analysis_generation: self.analysis_generation,
            analysis_cutoff_ns: self.analysis_cutoff_ns,
            direct_write_observed_after_ns: self.direct_write_observed_after_ns,
            mutation_identity: self.mutation_identity,
        }
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
    /// Wakes the host worker after an intake atomically binds a pending turn.
    /// 在登记原子绑定待处理回合后唤醒宿主 Worker。
    pending_analysis_notify: Notify,
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
    ) -> Self {
        Self {
            runtime,
            data_plane,
            operation_gate,
            pending_analysis_notify: Notify::new(),
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
            if let Err(error) = self.drain_due_pending_analysis_jobs().await {
                eprintln!("native VMM pending analysis drain failed: {error}");
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
    /// # Errors
    /// Returns a data-plane clock or queue read failure; individual job failures are rescheduled.
    /// 数据平面时钟或队列读取失败时返回错误；单个作业失败会被重新调度。
    async fn drain_due_pending_analysis_jobs(&self) -> Result<(), String> {
        let _gate = self.pending_analysis_drain_gate.lock().await;
        let jobs = self
            .data_plane
            .list_due_pending_analysis_jobs(now_unix_ms()?, PENDING_ANALYSIS_BATCH_LIMIT)
            .await?;
        for job in jobs {
            if let Err(error) = self.execute_pending_analysis_job(&job).await {
                let next_attempt_at_unix_ms = now_unix_ms()?.saturating_add(
                    pending_analysis_retry_delay_ms(job.attempt_count.saturating_add(1)),
                );
                let diagnostic = bounded_diagnostic(&error);
                self.data_plane
                    .record_pending_analysis_failure(
                        &job.operation_id,
                        next_attempt_at_unix_ms,
                        &diagnostic,
                    )
                    .await?;
                eprintln!(
                    "native VMM post-action analysis failed for operation {}: {}",
                    job.operation_id, diagnostic
                );
            }
        }
        Ok(())
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
    ) -> Result<(), String> {
        // The read guard keeps maintenance from moving or clearing the referenced turn until the
        // complete queued analysis and queue acknowledgement are durable.
        // 读取守卫保证完整队列分析及其确认耐久化前，维护操作不能迁移或清空所引用的回合。
        let _operation_guard = self.operation_gate.read().await;
        if job.state != VMM_PENDING_ANALYSIS_READY {
            return Err(format!(
                "待分析作业 `{}` 未处于 ready 状态",
                job.operation_id
            ));
        }
        let payload: NativePendingPostActionPayload = serde_json::from_str(&job.payload_json)
            .map_err(|error| format!("解析待分析作业 `{}` 失败：{error}", job.operation_id))?;
        let turn_id = TurnId::new(
            job.turn_id
                .clone()
                .ok_or_else(|| format!("待分析作业 `{}` 缺少回合身份", job.operation_id))?,
        )
        .map_err(|error| format!("待分析作业 `{}` 回合身份无效：{error}", job.operation_id))?;
        let result = self
            .runtime
            .run(CoreCommand::PostActionAnalyzePendingTurn(
                payload.analysis.into_command(turn_id),
            ))
            .await
            .map_err(|error| error.to_string())?;
        if !matches!(
            result,
            vulcan_memorymesh_core::CommandResult::PostActionAnalysis(_)
        ) {
            return Err(String::from("Core 未返回 PostActionAnalysis 结果"));
        }
        self.data_plane
            .complete_pending_analysis_job(&job.operation_id, &job.request_fingerprint)
            .await
    }
}

#[async_trait]
impl VmmClient for NativeVmmClient {
    /// Reports process-local readiness without probing a network endpoint.
    /// 在不探测网络端点的情况下报告进程内就绪状态。
    async fn healthz(&self) -> Result<HealthzResponse, VmmClientError> {
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
        let sections = profile_sections(request.target, &access, request.user_id)?;
        let bundle = self.query_profile(request.user_id, sections, false).await?;
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
        let split = request.mode == ProfileBundleMode::ProfileBundleModeSplit;
        let include_explanation = request.include_explanation.unwrap_or(false);
        let access = self
            .project_access(request.host_project_id.as_str(), request.project_id)
            .await?;
        let bundle = self
            .query_profile(
                request.user_id,
                profile_sections(ProfileTarget::ProfileTargetAll, &access, request.user_id)?,
                include_explanation,
            )
            .await?;
        let mut project_profile = String::new();
        let mut user_profile = String::new();
        for section in &bundle.sections {
            match section.role {
                SectionRole::Environment => {
                    if !project_profile.is_empty() {
                        project_profile.push_str("\n\n");
                    }
                    project_profile.push_str(&section.rendered);
                }
                SectionRole::Subject => user_profile.clone_from(&section.rendered),
            }
        }
        Ok(GetProfileBundleResponse {
            mode: if split {
                ProfileBundleMode::ProfileBundleModeSplit
            } else {
                ProfileBundleMode::ProfileBundleModeFull
            },
            include_explanation,
            explanation_text: String::new(),
            environment_priority_text: String::new(),
            combined_text: bundle.rendered,
            team_profile: String::new(),
            space_profile: String::new(),
            project_profile,
            user_profile,
            trace_id: native_trace_id("profile-bundle"),
        })
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
        // The read guard keeps owner and linked recall scopes stable for this complete retrieval.
        // 读取守卫在本次完整召回期间保持归属与连带查询作用域稳定。
        let _operation_guard = self.operation_gate.read().await;
        let now = now_ns()?;
        let access = self
            .project_access(request.host_project_id.as_str(), request.project_id)
            .await?;
        let session_recall_mode = match request.recall_mode {
            PreCheckRecallMode::PreCheckRecallModeFull => SessionRecallMode::Full,
            PreCheckRecallMode::PreCheckRecallModeUnspecified
            | PreCheckRecallMode::PreCheckRecallModeSessionCompact => {
                SessionRecallMode::CompactAware
            }
        };
        let defaults = PreCheckPolicy::default();
        let retrieval = RetrievalPolicy::v1();
        let result = self
            .runtime
            .run(CoreCommand::PreCheck(PreCheckCommand {
                subject_id: subject_id(request.user_id)?,
                scope_access: access.clone(),
                session_source_id: session_source_id(&request.session_id)?,
                vector_space_id: vector_space_id()?,
                user_content: request.user_content,
                recent_timeline: Vec::new(),
                recall_mode: RecallMode::Auto,
                session_recall_mode,
                candidate_scope: MemoryCandidateScope {
                    scope_ids: access.visible_scopes,
                    origin_session_source_id: None,
                },
                max_search_queries: retrieval.max_query_items,
                max_memories: defaults.recall_top_k,
                now_ns: now,
                mutation_identity: caller_identity("precheck", request.operation_id.as_str(), now)?,
            }))
            .await
            .map_err(core_client_error)?;
        let vulcan_memorymesh_core::CommandResult::PreCheck(result) = result else {
            return Err(unexpected_result("PreCheck"));
        };
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
            should_inject: result.should_inject,
            context_items,
            degraded: result.degraded,
            trace_id: native_trace_id("precheck"),
            session_memory_status: SessionMemoryStatus::Active,
        })
    }

    /// Durably records a completed turn and starts its native background analysis.
    /// 持久记录一个已完成回合并启动其原生后台分析。
    async fn post_action(
        &self,
        request: PostActionRequest,
    ) -> Result<PostActionResponse, VmmClientError> {
        // The read guard covers queue intake and Core commit together so maintenance cannot observe
        // an untracked in-flight turn for this project.
        // 读取守卫同时覆盖队列登记与 Core 提交，使维护操作不会观察到未跟踪的项目内飞行回合。
        let _operation_guard = self.operation_gate.read().await;
        let request_fingerprint = pending_analysis_request_fingerprint(&request)?;
        let now = now_ns()?;
        let access = self
            .project_access(request.host_project_id.as_str(), request.project_id)
            .await?;
        let subject = subject_id(request.user_id)?;
        let session = session_source_id(&request.session_id)?;
        let timeline = post_action_timeline(&request, now)?;
        let input_kind = if request.timeline.is_empty() {
            PostActionInputKind::SingleTurn
        } else {
            PostActionInputKind::Timeline
        };
        let caller_operation = stable_or_generated_operation("post-action", &request.operation_id);
        let intake_identity = mutation_identity(
            caller_operation.as_str(),
            MutationSourceKind::HostRpc,
            "native-vmm:post-action",
            now,
        )?;
        let intake_mutation_identity =
            derive_mutation_identity(&intake_identity, "intake").map_err(identifier_error)?;
        let project_scope = access.write_scope.clone();
        let subject_scope = subject_scope_id(request.user_id)?;
        let mut analysis_visible_scopes = access.visible_scopes.clone();
        analysis_visible_scopes.push(subject_scope.clone());
        let analysis_operation = derived_operation_id(caller_operation.as_str(), "analysis-1");
        let payload = NativePendingPostActionPayload {
            intake: PostActionIntakeCommand {
                input_kind,
                subject_id: subject.clone(),
                scope_access: access,
                session_source_id: session.clone(),
                vector_space_id: vector_space_id()?,
                timeline,
                completed_at_ns: now,
                mutation_identity: intake_identity,
            },
            analysis: NativePendingAnalysisTemplate {
                replacement_scope: MemoryReplacementScope {
                    candidates: MemoryCandidateScope {
                        scope_ids: analysis_visible_scopes.clone(),
                        origin_session_source_id: None,
                    },
                    authorized_supersede_scopes: vec![project_scope.clone()],
                },
                subject_id: subject,
                scope_access: ScopeAccess {
                    write_scope: project_scope.clone(),
                    visible_scopes: analysis_visible_scopes,
                },
                profile_sections: vec![
                    ProfileViewSection {
                        scope_id: project_scope,
                        role: SectionRole::Environment,
                        display_label: String::from("Project"),
                    },
                    ProfileViewSection {
                        scope_id: subject_scope,
                        role: SectionRole::Subject,
                        display_label: String::from("User"),
                    },
                ],
                session_source_id: session,
                vector_space_id: vector_space_id()?,
                analysis_generation: 1,
                analysis_cutoff_ns: now,
                direct_write_observed_after_ns: 0,
                mutation_identity: mutation_identity(
                    analysis_operation.as_str(),
                    MutationSourceKind::BackgroundWorker,
                    "native-vmm:post-action-analysis",
                    now,
                )?,
            },
        };
        let payload_json = serde_json::to_string(&payload).map_err(|error| {
            VmmClientError::new(
                "native_analysis_queue",
                format!("serialize native VMM analysis job failed: {error}"),
            )
        })?;
        let queued = self
            .data_plane
            .enqueue_pending_analysis_job(&NativeVmmPendingAnalysisJob {
                operation_id: caller_operation.clone(),
                request_fingerprint: request_fingerprint.clone(),
                payload_json,
                intake_mutation_operation_id: intake_mutation_identity.operation_id.to_string(),
                turn_id: None,
                state: String::from(VMM_PENDING_ANALYSIS_INTAKING),
                attempt_count: 0,
                next_attempt_at_unix_ms: 0,
                last_error: None,
            })
            .await
            .map_err(|error| VmmClientError::new("native_analysis_queue", error))?;
        if queued.state == VMM_PENDING_ANALYSIS_COMPLETED
            || queued.state == VMM_PENDING_ANALYSIS_READY
        {
            if queued.state == VMM_PENDING_ANALYSIS_READY {
                self.pending_analysis_notify.notify_one();
            }
            return Ok(native_post_action_response());
        }
        let payload: NativePendingPostActionPayload = serde_json::from_str(&queued.payload_json)
            .map_err(|error| {
                VmmClientError::new(
                    "native_analysis_queue",
                    format!("parse native VMM analysis job failed: {error}"),
                )
            })?;
        let result = match self
            .runtime
            .run(CoreCommand::PostActionIntake(payload.intake.clone()))
            .await
        {
            Ok(result) => result,
            Err(error) => {
                if let Err(recovery_error) = self.runtime.recover_storage_mutations().await {
                    return Err(VmmClientError::new(
                        "native_analysis_queue_recovery",
                        format!(
                            "native VMM intake failed ({error}) and storage recovery failed: {recovery_error}"
                        ),
                    ));
                }
                let recovered = self
                    .data_plane
                    .load_pending_analysis_job(&caller_operation)
                    .await
                    .map_err(|recovery_error| {
                        VmmClientError::new("native_analysis_queue_recovery", recovery_error)
                    })?;
                if recovered.as_ref().is_some_and(|job| {
                    job.state == VMM_PENDING_ANALYSIS_READY
                        || job.state == VMM_PENDING_ANALYSIS_COMPLETED
                }) {
                    self.pending_analysis_notify.notify_one();
                    return Ok(native_post_action_response());
                }
                self.data_plane
                    .delete_pending_analysis_intake(&caller_operation, &request_fingerprint)
                    .await
                    .map_err(|cleanup_error| {
                        VmmClientError::new("native_analysis_queue_cleanup", cleanup_error)
                    })?;
                return Err(core_client_error(error));
            }
        };
        let vulcan_memorymesh_core::CommandResult::PostActionAccepted(result) = result else {
            return Err(unexpected_result("PostActionAccepted"));
        };
        if let Some(turn_id) = result.turn_id.as_ref() {
            let queued = self
                .data_plane
                .load_pending_analysis_job(&caller_operation)
                .await
                .map_err(|error| VmmClientError::new("native_analysis_queue", error))?
                .ok_or_else(|| {
                    VmmClientError::new(
                        "native_analysis_queue",
                        "native VMM intake committed without a durable analysis job",
                    )
                })?;
            if queued.state != VMM_PENDING_ANALYSIS_READY
                || queued.turn_id.as_deref() != Some(turn_id.as_str())
            {
                return Err(VmmClientError::new(
                    "native_analysis_queue",
                    "native VMM intake commit did not atomically bind its analysis job",
                ));
            }
            self.pending_analysis_notify.notify_one();
        } else {
            self.data_plane
                .complete_pending_analysis_job(&caller_operation, &request_fingerprint)
                .await
                .map_err(|error| VmmClientError::new("native_analysis_queue", error))?;
        }
        Ok(native_post_action_response())
    }

    /// Advances the durable compact boundary for one native project session.
    /// 推进一个原生项目会话的持久压缩边界。
    async fn chat_compact(
        &self,
        request: ChatCompactRequest,
    ) -> Result<ChatCompactResponse, VmmClientError> {
        // The read guard prevents a compact boundary from racing project migration or clearing.
        // 读取守卫防止压缩边界与项目迁移或清空发生竞争。
        let _operation_guard = self.operation_gate.read().await;
        let now = now_ns()?;
        let result = self
            .runtime
            .run(CoreCommand::ChatCompact(
                vulcan_memorymesh_core::ChatCompactCommand {
                    subject_id: subject_id(request.user_id)?,
                    scope_access: self
                        .project_access(request.host_project_id.as_str(), request.project_id)
                        .await?,
                    session_source_id: session_source_id(&request.session_id)?,
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
        let access = self
            .project_access(request.host_project_id.as_str(), request.project_id)
            .await?;
        let result = self
            .runtime
            .run(CoreCommand::MemorySearchBatch(MemorySearchBatchCommand {
                subject_id: subject_id(request.user_id)?,
                scope_access: access.clone(),
                queries: request.queries,
                top_k: request.top_k,
                vector_space_id: vector_space_id()?,
                boundary: CandidateQueryBoundary {
                    candidate_scope: Some(MemoryCandidateScope {
                        scope_ids: access.visible_scopes,
                        origin_session_source_id: None,
                    }),
                    session_recall_boundary: None,
                },
                now_ns: now_ns()?,
            }))
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
        let result = self
            .runtime
            .run(CoreCommand::MemoryDetailsBatch(MemoryDetailsBatchCommand {
                subject_id: subject_id(request.user_id)?,
                scope_access: self
                    .project_access(request.host_project_id.as_str(), request.project_id)
                    .await?,
                memory_ids: Vec::new(),
                turn_ids: request
                    .turn_ids
                    .into_iter()
                    .map(turn_id)
                    .collect::<Result<Vec<_>, _>>()?,
            }))
            .await
            .map_err(core_client_error)?;
        let vulcan_memorymesh_core::CommandResult::MemoryDetailsBatch(result) = result else {
            return Err(unexpected_result("MemoryDetailsBatch"));
        };
        let turns = result
            .turns
            .into_iter()
            .flatten()
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
        // The read guard prevents direct writes from crossing a project maintenance boundary.
        // 读取守卫防止主动写入跨越项目维护边界。
        let _operation_guard = self.operation_gate.read().await;
        let now = now_ns()?;
        let project_access = self
            .project_access(request.host_project_id.as_str(), request.project_id)
            .await?;
        let project_scope = project_access.write_scope.clone();
        let visible_scopes = project_access.visible_scopes;
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
            .run(CoreCommand::MemoryWriteBatch(MemoryWriteBatchCommand {
                replacement_scope: MemoryReplacementScope {
                    candidates: MemoryCandidateScope {
                        scope_ids: visible_scopes.clone(),
                        origin_session_source_id: None,
                    },
                    authorized_supersede_scopes: vec![project_scope.clone()],
                },
                subject_id: subject_id(request.user_id)?,
                scope_access: ScopeAccess {
                    write_scope: project_scope.clone(),
                    visible_scopes: visible_scopes.clone(),
                },
                authorized_write_scopes: vec![project_scope],
                session_source_id: session_source_id(&request.session_id)?,
                vector_space_id: vector_space_id()?,
                occurred_at_ns: now,
                items,
                mutation_identity: invocation_identity("memory-write", now)?,
            }))
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
        let result = self
            .runtime
            .run(CoreCommand::MemoryDeleteBatch(MemoryDeleteBatchCommand {
                subject_id: subject_id(request.user_id)?,
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
            }))
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
            deleted_vector_rows: 0,
            trace_id: native_trace_id("memory-delete"),
        })
    }
}

impl NativeVmmClient {
    /// Runs one exact native profile query and returns its typed bundle.
    /// 运行一次精确的原生画像查询并返回其强类型画像包。
    async fn query_profile(
        &self,
        user_id: u64,
        sections: Vec<ProfileViewSection>,
        include_explanations: bool,
    ) -> Result<ProfileBundle, VmmClientError> {
        let visible_scopes = sections
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
                    view: ProfileView {
                        mode: ProfileViewMode::Full,
                        sections,
                        include_explanations,
                        display_timezone: String::from("UTC"),
                    },
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

/// Builds an explicit typed-result mismatch error.
/// 构建显式的强类型结果不匹配错误。
fn unexpected_result(expected: &str) -> VmmClientError {
    VmmClientError::new(
        "native_core_result_mismatch",
        format!("native Core did not return {expected}"),
    )
}

/// Builds the stable successful native PostAction transport projection.
/// 构建稳定的原生 PostAction 成功传输投影。
#[must_use]
fn native_post_action_response() -> PostActionResponse {
    PostActionResponse {
        accepted: true,
        trace_id: native_trace_id("post-action"),
        session_memory_status: SessionMemoryStatus::Active,
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

/// Creates the exact Core subject identity for one VulcanCode user.
/// 为一个 VulcanCode 用户创建精确 Core 主体标识。
fn subject_id(user_id: u64) -> Result<SubjectId, VmmClientError> {
    SubjectId::new(format!("vmm-subject:{user_id}")).map_err(identifier_error)
}

/// Creates the exact Core project scope identity.
/// 创建精确的 Core 项目作用域标识。
fn project_scope_id(project_id: u64) -> Result<ScopeId, VmmClientError> {
    ScopeId::new(format!("vmm-project:{project_id}")).map_err(identifier_error)
}

/// Creates the exact Core personal profile scope identity.
/// 创建精确的 Core 个人画像作用域标识。
fn subject_scope_id(user_id: u64) -> Result<ScopeId, VmmClientError> {
    ScopeId::new(format!("vmm-subject:{user_id}")).map_err(identifier_error)
}

/// Creates the host session source identity without changing the source identifier.
/// 在不改变来源标识的情况下创建宿主会话来源身份。
fn session_source_id(session_id: &str) -> Result<SessionSourceId, VmmClientError> {
    SessionSourceId::new(session_id.to_owned()).map_err(identifier_error)
}

/// Creates the one shared logical native vector-space identity.
/// 创建唯一共享的原生逻辑向量空间标识。
fn vector_space_id() -> Result<VectorSpaceId, VmmClientError> {
    VectorSpaceId::new(NATIVE_MEMORY_VECTOR_SPACE_ID).map_err(identifier_error)
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
fn profile_node_entry(node: &ProfileNode) -> Result<ProfileNodeEntry, VmmClientError> {
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
fn post_action_timeline(
    request: &PostActionRequest,
    completed_at_ns: i64,
) -> Result<Vec<TimelineItem>, VmmClientError> {
    let item_count = request.timeline.len().saturating_add(2);
    let offset = i64::try_from(item_count).map_err(identifier_error)?;
    let mut occurred_at_ns = completed_at_ns.saturating_sub(offset);
    let mut timeline = Vec::with_capacity(item_count);
    timeline.push(TimelineItem {
        role: Role::User,
        content: request.user_content.clone(),
        occurred_at_ns,
    });
    for item in &request.timeline {
        occurred_at_ns = occurred_at_ns.saturating_add(1);
        timeline.push(TimelineItem {
            role: match item.kind {
                PostActionTimelineType::User => Role::User,
                PostActionTimelineType::Assistant => Role::Assistant,
            },
            content: item.content.clone(),
            occurred_at_ns,
        });
    }
    if !request.assistant_content.is_empty() {
        timeline.push(TimelineItem {
            role: Role::Assistant,
            content: request.assistant_content.clone(),
            occurred_at_ns: completed_at_ns,
        });
    }
    Ok(timeline)
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
    })
}

/// Converts one canonical Core turn into the service detail projection.
/// 将一个规范 Core 回合转换为服务详情投影。
fn turn_detail_entry(turn: Turn) -> Result<TurnDetailEntry, VmmClientError> {
    let mut timeline = turn.timeline;
    let user_question = if timeline.first().is_some_and(|item| item.role == Role::User) {
        timeline.remove(0).content
    } else {
        String::new()
    };
    let assistant_answer = if timeline
        .last()
        .is_some_and(|item| item.role == Role::Assistant)
    {
        timeline.pop().map(|item| item.content).unwrap_or_default()
    } else {
        String::new()
    };
    let timeline = timeline
        .into_iter()
        .map(|item| {
            let kind = match item.role {
                Role::User => PostActionTimelineType::User,
                Role::Assistant => PostActionTimelineType::Assistant,
                Role::System | Role::Tool => {
                    return Err(VmmClientError::new(
                        "native_timeline_role_mismatch",
                        "the current VMM turn projection cannot represent system or tool items",
                    ));
                }
            };
            Ok(PostActionTimelineItem {
                kind,
                content: item.content,
            })
        })
        .collect::<Result<Vec<_>, VmmClientError>>()?;
    Ok(TurnDetailEntry {
        turn_id: parse_core_numeric_id(&turn.id, "vmm-turn-")?,
        user_question,
        timeline,
        assistant_answer,
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
fn parse_core_numeric_id(
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
        let vector_space = vector_space_id().expect("vector space");
        let payload = NativePendingPostActionPayload {
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
                analysis_cutoff_ns: 1,
                direct_write_observed_after_ns: 0,
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

        // inconsistentPayload simulates durable corruption between intake and analysis ownership.
        // inconsistentPayload 模拟登记与分析归属之间的耐久损坏。
        let mut inconsistent_payload = payload;
        inconsistent_payload.analysis.scope_access.write_scope = other_scope;
        job.payload_json =
            serde_json::to_string(&inconsistent_payload).expect("serialize inconsistent payload");
        assert!(pending_analysis_job_targets_scope(&job, &source_scope).is_err());
    }
}
