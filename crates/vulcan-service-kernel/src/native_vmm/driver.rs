//! Native VMM Core execution driver backed by VulcanCode AI and storage capabilities.
//! 由 VulcanCode AI 与存储能力支撑的原生 VMM Core 执行驱动。

// Core effect execution preserves the frozen diagnostic-rich HostAiError contract at its host edge.
// Core Effect 执行在宿主边界保留已冻结且富含诊断信息的 HostAiError 契约。
#![allow(
    clippy::result_large_err,
    reason = "the native driver must preserve the frozen HostAiError boundary contract"
)]

use super::{lexical, llm_contract, prompt, resources::NativeVmmResourceStore};
use vulcan_service_api::{VmmNativeRetentionSettingsSummary, VmmNativeRetrievalPolicySummary};

#[cfg(test)]
#[path = "lexical_driver_tests.rs"]
mod lexical_driver_tests;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_ai_contract::{
    AiMessage, AiOperation, ContentItem, EmbeddingEncoding, EmbeddingInput, EmbeddingInputContent,
    EmbeddingInvalidInputPolicy, EmbeddingOutputKind, EmbeddingPayload, EmbeddingRequest,
    EmbeddingSpaceFingerprint, EmbeddingTask, HostAiCallContext, HostAiCancellation,
    HostAiCapabilityHandle, HostAiCapabilityRegistration, HostAiCapabilityRequirement,
    HostAiEmbeddingRequest, HostAiError, HostAiErrorCode, HostAiPort, INFERENCE_CONTRACT_VERSION,
    InferenceCallerBudget, InferenceCallerContext, InferencePermission, InferenceUsageScope,
    MessageRole, ReasoningEffort, ReasoningOutputPolicy, RerankRequest, RerankText,
    RerankTruncation, ResponsesExecutionRequest, StructuredOutputPolicy,
};
use vulcan_memorymesh_core::{
    AsyncHostDriver, CandidateChannelResult, CandidateObservation, CommandResult, CoreCommand,
    CoreError, EffectDeadlineClass, EffectFailure, EffectFailureKind, EffectRequest, EffectResult,
    EmbeddingBatchResult, EmbeddingDropReason, EmbeddingDroppedInput, ErrorCode, ErrorStage,
    FailureClass, HostEffect, HostEffectResult, InMemoryStorage, LlmEffect, LlmPurpose,
    MemoryAccessClass, MemoryMeshCore, MemoryPolicy, MemoryState, MutationBatch, MutationOutcome,
    PhysicalCandidate, PhysicalCandidateChannel, PhysicalCandidateScore, RawScoreKind,
    RerankEffect, RerankEntry, ScoreDirection, StoragePort, StorageQuery, StorageQueryResult,
    StorageSnapshot, VectorSpaceId, test_pii_rule,
};
use vulcan_service_api::{
    VmmNativeNoiseTestResult, VmmNativePiiFormulaMatchSummary, VmmNativePiiFormulaTestResult,
    VmmNativePiiRedactionSummary, VmmNativePiiTestResult, VmmNativeRecallPolicySummary,
    VmmNativeResourceCatalogSummary, VmmNativeResourceFileContent,
    VmmNativeResourceSettingsSummary, VmmNativeRuntimePolicySummary,
};
use vulcan_workspace_store::vmm_data_plane::{
    NativeVmmCoreMutation, VMM_CORE_MUTATION_COMMITTED, VMM_CORE_MUTATION_COMPLETED,
    VMM_CORE_MUTATION_PREPARED, VmmDataPlaneHandle, VmmLexicalDocument, VmmVectorCandidate,
    VmmVectorRecord, VmmVectorSearchFilter,
};

/// Stable registration identity for the process-local native VMM capability.
/// 进程内原生 VMM 能力的稳定注册标识。
const NATIVE_VMM_REGISTRATION_ID: &str = "vmm-native:runtime";
/// Stable owner identity for native VMM AI usage and diagnostics.
/// 原生 VMM AI 用量与诊断使用的稳定所有者标识。
const NATIVE_VMM_OWNER_ID: &str = "vmm-native";
/// Configured inference profile selected for all native VMM AI roles.
/// 全部原生 VMM AI 角色选择的已配置推理 Profile。
const NATIVE_VMM_CONSUMER_PROFILE_ID: &str = "vmm";
/// Native VMM uses one symmetric provider-default embedding space for both stored memories and queries.
/// 原生 VMM 对已存记忆与查询统一使用同一个供应商默认对称向量空间。
const NATIVE_VMM_EMBEDDING_TASK: EmbeddingTask = EmbeddingTask::ProviderDefault;
/// Maximum LLM output budget accepted from one Core effect.
/// 单个 Core Effect 接受的最大 LLM 输出预算。
const NATIVE_VMM_LLM_OUTPUT_TOKENS: u64 = 8_192;

/// Stable namespace for route-derived native memory vector generations.
/// 由路由派生的原生记忆向量代次所使用的稳定命名空间。
const NATIVE_VMM_VECTOR_SPACE_NAMESPACE: &str = "vmm-native:memory:v1";

/// Identifies the exact route-derived vector generation used by native memory commands.
/// 标识原生记忆命令使用的、由准确路由派生的向量代次。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NativeVmmVectorSpaceDescriptor {
    /// Stable logical Core vector-space identity for this route generation.
    /// 当前路由代次对应的稳定 Core 逻辑向量空间标识。
    pub(super) vector_space_id: VectorSpaceId,
    /// Exact configured dense embedding width.
    /// 已配置稠密向量的准确维度。
    pub(super) embedding_dimension: u32,
    /// Opaque capability fingerprint used to derive the generation.
    /// 用于派生该代次的不透明能力指纹。
    pub(super) capability_fingerprint: String,
}

/// Owns the copied native Core and its process-local host driver.
/// 持有复制后的原生 Core 及其进程内宿主驱动。
pub(crate) struct NativeVmmCoreRuntime {
    /// Shared provider-independent command engine.
    /// 共享的供应商无关命令引擎。
    core: Arc<MemoryMeshCore>,
    /// Cloneable host effect executor used independently by concurrent commands.
    /// 并发命令分别使用的可克隆宿主 Effect 执行器。
    host: NativeVmmHostDriver,
    /// Host-owned persisted resource settings and user-layer overrides.
    /// 宿主持有的持久化资源设置与用户层覆盖。
    resources: NativeVmmResourceStore,
    /// Serializes lazy host-derived Core configuration activation.
    /// 串行化延迟执行的宿主派生 Core 配置激活。
    config_gate: tokio::sync::Mutex<()>,
    /// Latest authoritative Host AI activation outcome used by readiness projection.
    /// 用于就绪状态投影的最近一次权威宿主 AI 激活结果。
    host_ai_status: RwLock<NativeVmmHostAiStatus>,
}

/// Allows only retention reads against an owned snapshot; no production storage handles are held.
/// 只允许读取独占快照的清洗事实，不持有生产存储句柄。
struct NativeVmmRetentionPreviewDriver {
    /// Isolated facts consumed by the ordinary Core retention planner.
    /// 由常规 Core 清洗规划器消费的隔离事实。
    storage: InMemoryStorage,
}

impl AsyncHostDriver for NativeVmmRetentionPreviewDriver {
    /// Returns the requested retention facts and rejects every mutation or inference effect.
    /// 返回请求的清洗事实，并拒绝全部变更或推理效果。
    fn execute_effect<'a>(
        &'a mut self,
        request: EffectRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EffectResult, CoreError>> + Send + 'a>> {
        Box::pin(async move {
            // The closed match prevents future Core changes from silently making previews writable.
            // 封闭匹配防止未来 Core 变更悄悄赋予预览写入能力。
            let HostEffect::StorageQuery(query @ StorageQuery::RetentionScan { .. }) =
                request.effect
            else {
                return Err(core_effect_error(
                    "Retention preview accepts only retention scan effects.",
                ));
            };
            Ok(EffectResult {
                effect_id: request.effect_id,
                result: HostEffectResult::StorageQuery(self.storage.query(&query)?),
            })
        })
    }
}

/// Carries one monotonic request-cancellation edge into every Host AI effect of a native command.
/// 将一个单调请求取消边沿传递到原生命令的每个宿主 AI Effect。
#[derive(Default)]
struct NativeVmmHostAiCancellation {
    /// Cached cancellation state used by synchronous Host AI boundary probes.
    /// 供宿主 AI 边界同步探测使用的缓存取消状态。
    cancelled: AtomicBool,
    /// Wakes every active Host AI call after the owning native request is cancelled.
    /// 所属原生请求取消后唤醒全部活动宿主 AI 调用。
    changed: tokio::sync::Notify,
}

impl NativeVmmHostAiCancellation {
    /// Publishes cancellation exactly once and wakes all current waiters.
    /// 仅发布一次取消状态并唤醒全部当前等待方。
    fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::AcqRel) {
            self.changed.notify_waiters();
        }
    }
}

impl HostAiCancellation for NativeVmmHostAiCancellation {
    /// Returns the monotonic process-local cancellation state.
    /// 返回单调的进程内取消状态。
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Resolves after cancellation without missing a check-to-wait race.
    /// 在取消发布后完成，并避免遗漏检查到等待之间的竞态。
    fn cancelled<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            loop {
                // notified is enabled before the state probe so cancellation cannot fall into the gap.
                // notified 在状态探测前启用，确保取消不会落入竞态间隙。
                let notified = self.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if self.is_cancelled() {
                    return;
                }
                notified.await;
            }
        })
    }
}

/// Cancels a detached native command worker when its owning request future is dropped.
/// 所属请求 Future 被丢弃时取消已分离的原生命令 Worker。
struct NativeVmmRunCancellationGuard {
    /// Shared signal observed by every Host AI call in the command worker.
    /// 命令 Worker 中每次宿主 AI 调用观察的共享信号。
    cancellation: Arc<NativeVmmHostAiCancellation>,
    /// Whether dropping this guard must still publish cancellation.
    /// 丢弃该守卫时是否仍需发布取消。
    armed: bool,
}

impl NativeVmmRunCancellationGuard {
    /// Creates an armed guard for one spawned native command worker.
    /// 为一个已生成的原生命令 Worker 创建启用状态的守卫。
    ///
    /// # Parameters
    /// 参数
    /// `cancellation` is the exact signal attached to the worker's Host AI contexts.
    /// `cancellation` 是附加到 Worker 宿主 AI 上下文的准确信号。
    ///
    /// # Returns
    /// 返回值
    /// An armed guard that cancels the worker's active Host AI call on drop.
    /// 返回一个会在丢弃时取消 Worker 活动宿主 AI 调用的启用守卫。
    fn new(cancellation: Arc<NativeVmmHostAiCancellation>) -> Self {
        Self {
            cancellation,
            armed: true,
        }
    }

    /// Disarms cancellation after the worker reaches a terminal command result.
    /// Worker 到达命令终态结果后解除取消发布。
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for NativeVmmRunCancellationGuard {
    /// Publishes cancellation only when the owning request ended before its worker.
    /// 仅在所属请求早于 Worker 结束时发布取消。
    fn drop(&mut self) {
        if self.armed {
            self.cancellation.cancel();
        }
    }
}

/// Describes the latest native VMM Host AI capability validation outcome.
/// 描述最近一次原生 VMM 宿主 AI 能力校验结果。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NativeVmmHostAiStatus {
    /// Whether at least one complete capability validation has finished.
    /// 是否至少完成过一次完整能力校验。
    pub(crate) checked: bool,
    /// Whether every required LLM and embedding route is currently available.
    /// 当前是否所有必需 LLM 与向量路由均可用。
    pub(crate) ready: bool,
    /// Whether the optional profile-instruction route is currently available.
    /// 当前是否可用可选画像指令路由。
    pub(crate) profile_instruction_ready: bool,
    /// Controlled reason why the optional profile-instruction route is unavailable.
    /// 可选画像指令路由不可用时的受控原因。
    pub(crate) profile_instruction_error: Option<String>,
    /// Whether the optional rerank route is currently available.
    /// 当前是否可用可选重排路由。
    pub(crate) rerank_ready: bool,
    /// Controlled reason why the optional rerank route is unavailable.
    /// 可选重排路由不可用时的受控原因。
    pub(crate) rerank_error: Option<String>,
    /// Controlled validation error for user-facing readiness diagnostics.
    /// 用于面向用户就绪诊断的受控校验错误。
    pub(crate) error: Option<String>,
}

impl NativeVmmHostAiStatus {
    /// Builds a successful status from one freshly validated capability handle.
    /// 根据一个刚完成校验的能力句柄构造成功状态。
    ///
    /// # Parameters
    /// 参数
    /// `handle` is the refreshed VMM capability issued by the host AI Port.
    /// `handle` 是宿主 AI Port 签发并刷新的 VMM 能力。
    ///
    /// # Returns
    /// 返回值
    /// A ready status that preserves optional capability availability independently.
    /// 返回一份就绪状态，并分别保留可选能力的可用性。
    fn ready(handle: &HostAiCapabilityHandle) -> Self {
        // profileInstruction preserves both availability and the host-authored degradation reason.
        // profileInstruction 同时保留可用性与宿主给出的降级原因。
        let profile_instruction = optional_capability_status(
            handle,
            AiOperation::ConversationRespond,
            Some("profile_instruction"),
        );
        // rerank preserves both availability and the host-authored degradation reason.
        // rerank 同时保留可用性与宿主给出的降级原因。
        let rerank = optional_capability_status(handle, AiOperation::RerankDocuments, None);
        Self {
            checked: true,
            ready: true,
            profile_instruction_ready: profile_instruction.0,
            profile_instruction_error: profile_instruction.1,
            rerank_ready: rerank.0,
            rerank_error: rerank.1,
            error: None,
        }
    }

    /// Builds a failed status from one controlled activation error.
    /// 根据一次受控激活错误构造失败状态。
    ///
    /// # Parameters
    /// 参数
    /// `error` is the exact provider-neutral failure reported by the activation path.
    /// `error` 是激活链路报告的准确供应商无关错误。
    ///
    /// # Returns
    /// 返回值
    /// A checked but unavailable status with optional capabilities disabled.
    /// 返回一份已校验但不可用、且可选能力均关闭的状态。
    fn failed(error: &CoreError) -> Self {
        Self {
            checked: true,
            ready: false,
            profile_instruction_ready: false,
            profile_instruction_error: None,
            rerank_ready: false,
            rerank_error: None,
            error: Some(error.to_string()),
        }
    }
}

/// Resolves one optional operation into independent readiness and controlled diagnostics.
/// 将单个可选操作解析为独立就绪状态与受控诊断。
///
/// # Parameters
/// 参数
/// `handle` is the freshly refreshed host-issued capability handle.
/// `handle` 是刚完成刷新的宿主签发能力句柄。
/// `operation` is the exact provider-neutral operation being inspected.
/// `operation` 是待检查的准确供应商无关操作。
/// `purpose_id` is the optional purpose that distinguishes shared operation routes.
/// `purpose_id` 是区分共享操作路由的可选用途。
///
/// # Returns
/// 返回值
/// The availability flag and an optional host-authored unavailability reason.
/// 返回可用标记及可选的宿主不可用原因。
fn optional_capability_status(
    handle: &HostAiCapabilityHandle,
    operation: AiOperation,
    purpose_id: Option<&str>,
) -> (bool, Option<String>) {
    let Some(capability) = handle.snapshot.operations.iter().find(|capability| {
        capability.operation == operation && capability.purpose_id.as_deref() == purpose_id
    }) else {
        return (
            false,
            Some(String::from(
                "The host has not published this optional AI capability",
            )),
        );
    };
    if capability.available {
        return (true, None);
    }
    (
        false,
        capability.unavailable_reason.clone().or_else(|| {
            Some(String::from(
                "The host AI capability is currently unavailable",
            ))
        }),
    )
}

impl NativeVmmCoreRuntime {
    /// Restores native VMM state and constructs one ready process-local runtime.
    /// 恢复原生 VMM 状态并构造一个就绪的进程内运行时。
    ///
    /// # Errors
    /// Returns a storage or Core error when durable relational and vector state cannot be restored exactly.
    /// 无法精确恢复持久关系状态与向量状态时返回存储或 Core 错误。
    pub(crate) async fn bootstrap(
        ai_port: Arc<dyn HostAiPort>,
        data_plane: VmmDataPlaneHandle,
    ) -> Result<Self, String> {
        let storage = hydrate_storage(&data_plane).await?;
        let core = Arc::new(MemoryMeshCore::new().map_err(|error| error.to_string())?);
        let resources = NativeVmmResourceStore::new(&data_plane.data_root);
        // runtime is assembled before recovery so the same persistence code repairs interrupted maintenance.
        // runtime 在恢复前完成组装，使中断维护可复用同一持久化代码完成修复。
        let runtime = Self {
            core,
            resources,
            host: NativeVmmHostDriver {
                ai_port,
                capability: Arc::new(RwLock::new(None)),
                ai_cancellation: Arc::new(NativeVmmHostAiCancellation::default()),
                deleted_vector_rows: 0,
                embedding_fingerprints: Arc::new(RwLock::new(BTreeMap::new())),
                storage: Arc::new(RwLock::new(storage)),
                mutation_gate: Arc::new(tokio::sync::Mutex::new(())),
                content_projection: Arc::new(RwLock::new(None)),
                data_plane,
            },
            config_gate: tokio::sync::Mutex::new(()),
            host_ai_status: RwLock::new(NativeVmmHostAiStatus::default()),
        };
        runtime.host.recover_unfinished_core_mutations().await?;
        Ok(runtime)
    }

    /// Installs the host's non-owning committed-content projection before clients and workers are exposed.
    /// 在公开客户端及工作器前安装宿主不拥有服务的已提交内容投影。
    /// Takes the existing service and event emitter; returns an explicit projection-lock error on failure.
    /// 接收既有服务及事件发送器；失败时返回明确投影锁错误。
    pub(crate) fn install_content_projection(
        &self,
        state: &crate::state::SharedServiceBridgeState,
        events: &vulcan_service_api::ServiceEventEmitter,
    ) -> Result<(), String> {
        *self
            .host
            .content_projection
            .write()
            .map_err(|error| error.to_string())? = Some(
            super::maintenance::NativeVmmContentProjection::new(state, events),
        );
        Ok(())
    }

    /// Freezes committed content through final host publication, including workers whose callers were cancelled.
    /// 在宿主最终发布期间冻结已提交内容，包含调用方已取消的工作器。
    /// Returns the same owned mutation guard used by every Core storage effect and maintenance transition.
    /// 返回每个 Core 存储效果及维护转换使用的同一拥有所有权的变更守卫。
    pub(crate) async fn freeze_committed_content(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.host.mutation_gate.clone().lock_owned().await
    }

    /// Runs one typed Core command to a final result through VulcanCode-owned capabilities.
    /// 通过 VulcanCode 持有的能力将一个强类型 Core 命令运行至最终结果。
    ///
    /// # Errors
    /// Returns the stable Core command, effect, or persistence failure.
    /// 返回稳定的 Core 命令、Effect 或持久化失败。
    pub(crate) async fn run(&self, command: CoreCommand) -> Result<CommandResult, CoreError> {
        self.run_for_host_project(command, "").await
    }

    /// Runs one non-AI deletion batch and returns its semantic result plus immediate physical deletions.
    /// 执行一个不调用 AI 的删除批次，返回语义结果及即时物理删除行数。
    /// `command` owns the caller-validated scope and identity; errors preserve the normal Core lifecycle.
    /// `command` 持有已校验调用方的作用域与身份；错误保留常规 Core 生命周期。
    pub(super) async fn run_memory_delete(
        &self,
        command: vulcan_memorymesh_core::MemoryDeleteBatchCommand,
    ) -> Result<(CommandResult, u64), CoreError> {
        // Deletion uses the active semantic snapshot without requiring an inference configuration.
        // 删除使用活跃语义快照，不要求推理配置。
        let snapshot = self.core.active_snapshot()?;
        self.run_with_snapshot_and_vector_deletions(
            CoreCommand::MemoryDeleteBatch(command),
            snapshot.id.clone(),
        )
        .await
    }

    /// Runs with the originating host project's caller-owned policy, preserving the command's write scope.
    /// 按发起宿主项目持有的调用策略执行，保留命令写入作用域。
    /// Empty host identity selects only globals for explicit maintenance requests.
    /// 空宿主身份为显式维护请求选择仅全局规则。
    pub(super) async fn run_for_host_project(
        &self,
        command: CoreCommand,
        host_project_id: &str,
    ) -> Result<CommandResult, CoreError> {
        // snapshot is selected once; activation by another request cannot change its execution identity.
        // snapshot 只选择一次；其他请求的激活不能改变其执行身份。
        let snapshot = if command_requires_ai_configuration(&command) {
            let (descriptor, snapshot) = self
                .ensure_host_ai_config_for_project(
                    host_project_id,
                    profile_policy_project_id(&command)?,
                )
                .await?;
            validate_command_vector_generation(&command, &descriptor.vector_space_id)?;
            snapshot
        } else {
            self.core.active_snapshot()?
        };
        self.run_with_snapshot(command, snapshot.id.clone()).await
    }

    /// Runs a command built from an already resolved native policy without re-reading the active default.
    /// 执行依据已解析原生策略构建的命令，不重新读取活跃默认值。
    /// `snapshot_id` must come from this runtime's prepared context; returns the normal result/error lifecycle.
    /// `snapshot_id` 必须来自本运行时准备的上下文；返回普通结果及错误生命周期。
    pub(super) async fn run_with_snapshot(
        &self,
        command: CoreCommand,
        snapshot_id: vulcan_memorymesh_core::ConfigSnapshotId,
    ) -> Result<CommandResult, CoreError> {
        self.run_with_snapshot_and_vector_deletions(command, snapshot_id)
            .await
            .map(|(result, _)| result)
    }

    /// Executes a frozen command and returns only the physical deletions performed for that command.
    /// 执行冻结命令，仅返回为该命令执行的物理删除行数。
    /// `command` and `snapshot_id` retain normal execution semantics; recovery work is excluded from the count.
    /// `command` 与 `snapshot_id` 保持常规执行语义；恢复工作不计入统计。
    async fn run_with_snapshot_and_vector_deletions(
        &self,
        mut command: CoreCommand,
        snapshot_id: vulcan_memorymesh_core::ConfigSnapshotId,
    ) -> Result<(CommandResult, u64), CoreError> {
        if command.is_mutating() {
            let fingerprint =
                vulcan_memorymesh_core::request_fingerprint(&command, snapshot_id.as_str());
            command_mutation_identity(&mut command).request_fingerprint = fingerprint;
        }
        // cancellation is unique to this command and shared by all physical Host AI attempts.
        // cancellation 对该命令唯一，并由全部真实宿主 AI 尝试共享。
        let cancellation = Arc::new(NativeVmmHostAiCancellation::default());
        let mut host = self.host.clone();
        host.ai_cancellation = Arc::clone(&cancellation);
        // This scalar belongs to the worker clone, never to another concurrent command or the runtime template.
        // 此标量归属工作器克隆，绝不归属其他并发命令或运行时模板。
        host.deleted_vector_rows = 0;
        let core = Arc::clone(&self.core);
        // worker remains alive after caller cancellation long enough to resume Core with the
        // controlled Host AI cancellation result and release its command lifecycle state.
        // worker 会在调用方取消后继续存活，直至使用受控宿主 AI 取消结果恢复 Core 并释放命令生命周期状态。
        let mut worker = tokio::spawn(async move {
            // Return statistics only after the command has reached its normal successful terminal result.
            // 仅在命令到达常规成功终态结果后返回统计。
            let result = core
                .run_to_completion_with_snapshot_async(command, &snapshot_id, &mut host)
                .await?;
            Ok((result, host.deleted_vector_rows))
        });
        let mut cancellation_guard = NativeVmmRunCancellationGuard::new(cancellation);
        let outcome = (&mut worker).await;
        cancellation_guard.disarm();
        outcome.map_err(|error| {
            CoreError::new(
                ErrorCode::Internal,
                ErrorStage::Command,
                format!("native VMM command worker terminated unexpectedly: {error}"),
            )
        })?
    }

    /// Validates and activates the exact host AI capability required by native VMM.
    /// 校验并激活原生 VMM 所需的准确宿主 AI 能力。
    ///
    /// # Errors
    /// Returns the controlled host capability or embedding-space validation failure.
    /// 宿主能力或向量空间校验失败时返回受控错误。
    pub(crate) async fn validate_host_ai(&self) -> Result<(), String> {
        self.ensure_host_ai_config()
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Returns the current route-derived vector generation after rejecting partially migrated active memory.
    /// 在拒绝尚未完整迁移的活动记忆后返回当前由路由派生的向量代次。
    ///
    /// # Returns
    /// 返回值
    /// The stable logical vector-space identity selected by the current configured embedding route.
    /// 返回当前已配置 Embedding 路由选择的稳定逻辑向量空间标识。
    ///
    /// # Errors
    /// 错误
    /// Returns the exact Host AI configuration failure or a rebuild-required diagnostic.
    /// 返回准确的宿主 AI 配置错误，或必须重建向量的诊断。
    pub(crate) async fn current_vector_space_id(&self) -> Result<VectorSpaceId, String> {
        let descriptor = self.current_vector_space_descriptor().await?;
        self.ensure_active_memories_use_vector_space(&descriptor.vector_space_id)?;
        Ok(descriptor.vector_space_id)
    }

    /// Returns the current memory policy and compatible vector generation from one configuration activation.
    /// 通过一次配置激活返回当前记忆策略及兼容向量代次。
    ///
    /// # Returns
    /// 返回值
    /// The immutable memory policy, exact current vector-space identity and registered snapshot ID.
    /// 返回不可变记忆策略、准确当前向量空间标识及注册快照 ID。
    ///
    /// # Errors
    /// 错误
    /// Returns the exact configuration, snapshot, or vector-rebuild requirement failure.
    /// 返回准确的配置、快照或向量重建要求错误。
    pub(crate) async fn active_memory_context(
        &self,
        host_project_id: &str,
    ) -> Result<
        (
            MemoryPolicy,
            VectorSpaceId,
            vulcan_memorymesh_core::ConfigSnapshotId,
        ),
        String,
    > {
        let (descriptor, snapshot) = self
            .ensure_host_ai_config_for_project(host_project_id, None)
            .await
            .map_err(|error| error.to_string())?;
        self.ensure_active_memories_use_vector_space(&descriptor.vector_space_id)?;
        Ok((
            snapshot.memory_policy.clone(),
            descriptor.vector_space_id,
            snapshot.id.clone(),
        ))
    }

    /// Resolves the current route-derived vector generation without requiring migration completion.
    /// 解析当前由路由派生的向量代次，但不要求迁移已经完成。
    ///
    /// # Returns
    /// 返回值
    /// The exact logical generation, dimension, and capability fingerprint used by maintenance.
    /// 返回维护流程使用的准确逻辑代次、维度与能力指纹。
    ///
    /// # Errors
    /// 错误
    /// Returns the exact Host AI capability or Core resource activation failure.
    /// 返回准确的宿主 AI 能力或 Core 资源激活失败。
    pub(super) async fn current_vector_space_descriptor(
        &self,
    ) -> Result<NativeVmmVectorSpaceDescriptor, String> {
        self.ensure_host_ai_config()
            .await
            .map(|(descriptor, _)| descriptor)
            .map_err(|error| error.to_string())
    }

    /// Embeds one bounded maintenance batch through the same registered Host AI capability as Core.
    /// 通过与 Core 相同的已注册宿主 AI 能力嵌入一个有界维护批次。
    ///
    /// # Parameters
    /// 参数
    /// `operation_id` is the stable batch identity used for diagnostics and cancellation correlation.
    /// `operation_id` 是用于诊断与取消关联的稳定批次身份。
    /// `descriptor` freezes the exact route generation confirmed by the caller.
    /// `descriptor` 冻结调用方已确认的准确路由代次。
    /// `inputs` contains non-empty durable memory summaries in caller order.
    /// `inputs` 按调用方顺序包含非空持久记忆摘要。
    ///
    /// # Returns
    /// 返回值
    /// One dense vector per input in the same order.
    /// 按相同顺序为每个输入返回一个稠密向量。
    ///
    /// # Errors
    /// 错误
    /// Returns on route drift, provider failure, partial output, or any dimension mismatch.
    /// 路由漂移、供应商失败、部分输出或任一维度不匹配时返回错误。
    pub(super) async fn embed_maintenance_summaries(
        &self,
        operation_id: &str,
        descriptor: &NativeVmmVectorSpaceDescriptor,
        inputs: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, String> {
        if inputs.is_empty() {
            return Err(String::from(
                "The native VMM maintenance embedding batch must not be empty.",
            ));
        }
        let current = self.current_vector_space_descriptor().await?;
        if current != *descriptor {
            return Err(String::from(
                "The embedding route changed during the vector rebuild; precheck the remaining records again.",
            ));
        }
        let effect = vulcan_memorymesh_core::EmbeddingEffect {
            inputs,
            vector_space_id: descriptor.vector_space_id.clone(),
            expected_dimension: descriptor.embedding_dimension,
            allow_partial_invalid_inputs: false,
        };
        let request_id = format!(
            "vmm-maintenance-embedding-{}",
            hex::encode(Sha256::digest(operation_id.as_bytes()))
        );
        let context = HostAiCallContext::new(operation_id.to_owned())
            .with_deadline(effect_deadline(EffectDeadlineClass::Maintenance))
            .with_cancellation(Arc::new(NativeVmmHostAiCancellation::default()));
        match self
            .host
            .execute_embedding_with_context(context, request_id, &effect)
            .await
            .map_err(|error| error.to_string())?
        {
            HostEffectResult::Embedding(batch)
                if batch.dropped.is_empty()
                    && batch.vectors.len() == effect.inputs.len()
                    && (batch.result_indices.is_empty()
                        || batch.result_indices.iter().copied().eq(0..u32::try_from(
                            effect.inputs.len(),
                        )
                        .map_err(|error| {
                            format!("The native VMM maintenance embedding batch count is out of range: {error}")
                        })?)) =>
            {
                Ok(batch.vectors)
            }
            HostEffectResult::Failure(failure) => {
                Err(format!("Native VMM maintenance embedding failed: {}", failure.diagnostic))
            }
            HostEffectResult::Embedding(_) => Err(String::from(
                "The native VMM maintenance embedding response contains missing, dropped or out-of-order results.",
            )),
            _ => Err(String::from("Native VMM maintenance embedding returned the wrong result type.")),
        }
    }

    /// Rejects ordinary vector-dependent commands while any active memory still uses an older generation.
    /// 当任一活动记忆仍使用旧代次时拒绝普通向量相关命令。
    ///
    /// # Parameters
    /// 参数
    /// `current` is the exact logical vector-space identity derived from the active route.
    /// `current` 是由当前活动路由派生的准确逻辑向量空间标识。
    ///
    /// # Errors
    /// 错误
    /// Returns a bounded diagnostic containing the number of memories that require rebuilding.
    /// 返回包含待重建记忆数量的有界诊断。
    fn ensure_active_memories_use_vector_space(
        &self,
        current: &VectorSpaceId,
    ) -> Result<(), String> {
        let snapshot = self.storage_snapshot()?;
        let rebuild_required = snapshot
            .dataset
            .memories
            .iter()
            .filter(|memory| {
                memory.state == MemoryState::Active
                    && memory.vector_space_id.as_ref() != Some(current)
            })
            .count();
        if rebuild_required == 0 {
            return Ok(());
        }
        Err(format!(
            "The current embedding route disagrees with the vector generation of {rebuild_required} active memories. Finish the vector rebuild under Internal maintenance in the memory settings before reading or writing memories."
        ))
    }

    /// Returns the latest Host AI readiness snapshot without triggering provider work.
    /// 返回最近一次宿主 AI 就绪快照，且不触发 Provider 工作。
    ///
    /// # Returns
    /// 返回值
    /// The last completed validation state, or the initial unchecked state before validation.
    /// 返回最近完成的校验状态；首次校验前返回初始未校验状态。
    pub(crate) fn host_ai_status(&self) -> NativeVmmHostAiStatus {
        self.host_ai_status
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Lists the native embedded-resource catalog and exact user override state.
    /// 列出原生内置资源目录与准确用户覆盖状态。
    ///
    /// # Errors
    /// Returns an error when persisted settings or override files are invalid.
    /// 持久化设置或覆盖文件无效时返回错误。
    pub(crate) async fn resource_catalog(&self) -> Result<VmmNativeResourceCatalogSummary, String> {
        let _gate = self.config_gate.lock().await;
        self.resources.catalog()
    }

    /// Reads the validated business policy without writing defaults to the user directory.
    /// 读取经过校验的业务策略，不向用户目录写入默认值。
    /// Returns a configuration or I/O error when the user document is invalid.
    /// 用户文档无效时返回配置或 I/O 错误。
    pub(crate) async fn runtime_policy(&self) -> Result<VmmNativeRuntimePolicySummary, String> {
        let _gate = self.config_gate.lock().await;
        self.resources.runtime_policy(&self.core)
    }

    /// Reads explicit host-project overrides under the same configuration gate as command admission.
    /// 在命令准入使用的同一配置锁内读取显式宿主项目覆盖。
    pub(crate) async fn host_project_policy(
        &self,
        host_project_id: &str,
    ) -> Result<vulcan_service_api::VmmNativeHostProjectPolicySummary, String> {
        let _gate = self.config_gate.lock().await;
        self.resources
            .host_project_policy_summary(&self.core, host_project_id)
    }

    /// Saves validated caller-owned groups; the maintenance caller validates project existence first.
    /// 保存已校验调用方配置组；维护调用方先验证项目存在。
    pub(crate) async fn update_host_project_policy(
        &self,
        host_project_id: &str,
        overrides: vulcan_service_api::VmmNativeHostProjectPolicyOverrides,
    ) -> Result<vulcan_service_api::VmmNativeHostProjectPolicySummary, String> {
        let _gate = self.config_gate.lock().await;
        self.resources
            .update_host_project_policy(&self.core, host_project_id, overrides)?;
        self.resources
            .host_project_policy_summary(&self.core, host_project_id)
    }

    /// Saves explicit business values under the same gate used to freeze command configurations.
    /// 在冻结命令配置使用的同一锁内保存显式业务值。
    /// Returns the saved effective settings or the exact validation/persistence error.
    /// 返回已保存的有效设置或准确的校验/持久化错误。
    pub(crate) async fn update_runtime_policy(
        &self,
        recall: VmmNativeRecallPolicySummary,
    ) -> Result<VmmNativeRuntimePolicySummary, String> {
        let _gate = self.config_gate.lock().await;
        self.resources.update_runtime_policy(&self.core, recall)
    }

    /// Saves one ranking group under the same gate used to freeze command configurations.
    /// 在冻结命令配置使用的同一锁内保存排序组。
    /// Returns the full effective policy or an explicit validation/persistence error.
    /// 返回完整有效策略或明确的校验/持久化错误。
    pub(crate) async fn update_retrieval_policy(
        &self,
        retrieval: VmmNativeRetrievalPolicySummary,
    ) -> Result<VmmNativeRuntimePolicySummary, String> {
        let _gate = self.config_gate.lock().await;
        self.resources
            .update_retrieval_policy(&self.core, retrieval)
    }

    /// Saves intake text controls under the same gate that freezes newly admitted command snapshots.
    /// 在冻结新准入命令快照的同一锁内保存入库文本控制项。
    /// Returns the full accepted policy or an explicit validation/persistence failure.
    /// 返回完整接纳策略或明确的校验/持久化失败。
    pub(crate) async fn update_text_policy(
        &self,
        text: vulcan_service_api::VmmNativeTextPolicySummary,
    ) -> Result<VmmNativeRuntimePolicySummary, String> {
        let _gate = self.config_gate.lock().await;
        self.resources.update_text_policy(&self.core, text)
    }

    /// Saves global profile lifetimes under the same gate used by immutable command admission.
    /// 在不可变命令准入使用的同一锁内保存全局画像生命周期。
    /// Returns the acknowledged document without modifying existing nodes.
    /// 返回已确认文档，不修改既有节点。
    pub(crate) async fn update_profile_policy(
        &self,
        profiles: vulcan_service_api::VmmNativeProfilePolicySummary,
    ) -> Result<VmmNativeRuntimePolicySummary, String> {
        let _gate = self.config_gate.lock().await;
        self.resources.update_profile_policy(&self.core, profiles)
    }

    /// Reads validated cleanup settings under the configuration gate without invoking AI.
    /// 在配置锁内读取已校验清洗设置，不调用 AI。
    /// Returns the current saved group or a configuration error.
    /// 返回当前已保存配置组或配置错误。
    pub(crate) async fn retention_settings(
        &self,
    ) -> Result<VmmNativeRetentionSettingsSummary, String> {
        let _gate = self.config_gate.lock().await;
        self.resources.retention_settings()
    }

    /// Freezes memory-owner retention rules under the shared configuration gate for one admitted pass.
    /// 在共享配置锁内为一次已准入轮次冻结记忆归属方保留规则。
    /// Catalog IDs must be held stable by the caller; errors never activate global fallback rules.
    /// 调用方必须保持目录 ID 稳定；错误绝不启用全局回退规则。
    pub(crate) async fn project_retention_policies(
        &self,
        base: &vulcan_memorymesh_core::RetentionPolicy,
        project_ids: &[u64],
    ) -> Result<std::collections::BTreeMap<u64, vulcan_memorymesh_core::RetentionPolicy>, String>
    {
        let _gate = self.config_gate.lock().await;
        self.resources.project_retention_policies(base, project_ids)
    }

    /// Reads one memory owner's current retention values under the shared configuration gate.
    /// 在共享配置锁内读取一个记忆归属方的当前保留值。
    pub(crate) async fn project_retention_summary(
        &self,
        project_id: u64,
    ) -> Result<vulcan_service_api::VmmNativeProjectRetentionSummary, String> {
        let _gate = self.config_gate.lock().await;
        self.resources.project_retention_summary(project_id)
    }

    /// Saves explicit owner rules and returns the authoritative post-save inheritance projection.
    /// 保存显式归属方规则，返回保存后的权威继承投影。
    /// The maintenance caller holds the project catalog stable throughout this operation.
    /// 维护调用方在整个操作期间保持项目目录稳定。
    pub(crate) async fn save_project_retention(
        &self,
        project_id: u64,
        rules: Option<vulcan_service_api::VmmNativeProjectRetentionRules>,
    ) -> Result<vulcan_service_api::VmmNativeProjectRetentionSummary, String> {
        let _gate = self.config_gate.lock().await;
        self.resources.save_project_retention(project_id, rules)?;
        self.resources.project_retention_summary(project_id)
    }

    /// Reads project-only profile formulas under the shared configuration gate.
    /// 在共享配置锁内读取仅项目画像的公式。
    /// Takes a catalog-validated memory ID; returns saved and effective rules without personal fields.
    /// 接收目录已校验的记忆 ID，返回不含个人字段的已保存及有效规则。
    pub(crate) async fn project_profile_policy_summary(
        &self,
        project_id: u64,
    ) -> Result<vulcan_service_api::VmmNativeProjectProfilePolicySummary, String> {
        let _gate = self.config_gate.lock().await;
        self.resources.project_profile_policy_summary(project_id)
    }

    /// Persists an explicit project-profile override or inheritance request.
    /// 持久化显式项目画像覆盖或继承请求。
    /// The caller holds the catalog gate; the returned summary reflects the saved configuration.
    /// 调用方持有目录锁；返回摘要反映已保存配置。
    pub(crate) async fn save_project_profile_policy(
        &self,
        project_id: u64,
        rules: Option<vulcan_service_api::VmmNativeProfileLifetimes>,
    ) -> Result<vulcan_service_api::VmmNativeProjectProfilePolicySummary, String> {
        let _gate = self.config_gate.lock().await;
        self.resources
            .save_project_profile_policy(project_id, rules)?;
        self.resources.project_profile_policy_summary(project_id)
    }

    /// Saves explicit cleanup settings through the sole policy-document owner.
    /// 通过唯一策略文档归属方保存显式清洗设置。
    /// Returns the full effective policy or a validation or persistence error.
    /// 返回完整有效策略或校验、持久化错误。
    pub(crate) async fn update_retention_settings(
        &self,
        settings: VmmNativeRetentionSettingsSummary,
    ) -> Result<VmmNativeRuntimePolicySummary, String> {
        let _gate = self.config_gate.lock().await;
        self.resources
            .update_retention_settings(&self.core, settings)
    }

    /// Reads one exact native embedded resource and its effective user override.
    /// 读取一份准确的原生内置资源及其生效用户覆盖。
    ///
    /// # Errors
    /// Returns an error for an unknown, unsafe, or unreadable resource path.
    /// 资源路径未知、不安全或不可读时返回错误。
    pub(crate) async fn resource_file(
        &self,
        path: &str,
    ) -> Result<VmmNativeResourceFileContent, String> {
        let _gate = self.config_gate.lock().await;
        self.resources.read_file(path)
    }

    /// Saves and immediately activates native resource composition settings.
    /// 保存并立即激活原生资源组合设置。
    ///
    /// # Errors
    /// Returns an error when the selection, compiled snapshot, or persistence operation fails.
    /// 选择、编译快照或持久化操作失败时返回错误。
    pub(crate) async fn update_resource_settings(
        &self,
        settings: VmmNativeResourceSettingsSummary,
    ) -> Result<VmmNativeResourceCatalogSummary, String> {
        let _gate = self.config_gate.lock().await;
        self.resources.update_settings(&self.core, settings)
    }

    /// Saves one user override and immediately activates a new immutable Core snapshot.
    /// 保存一份用户覆盖并立即激活新的 Core 不可变快照。
    ///
    /// # Errors
    /// Returns an error for missing high-risk confirmation, invalid content, or persistence failure.
    /// 高风险确认缺失、内容无效或持久化失败时返回错误。
    pub(crate) async fn write_resource_file(
        &self,
        path: &str,
        content: String,
        confirmed_purpose: bool,
        confirmed_modification: bool,
    ) -> Result<VmmNativeResourceFileContent, String> {
        let _gate = self.config_gate.lock().await;
        self.resources.write_file(
            &self.core,
            path,
            content,
            confirmed_purpose,
            confirmed_modification,
        )
    }

    /// Restores one resource's built-in effective content and activates the result immediately.
    /// 恢复一份资源的内置生效内容并立即激活结果。
    ///
    /// # Errors
    /// Returns an error when validation, deletion, replacement, or activation fails.
    /// 校验、删除、替换或激活失败时返回错误。
    pub(crate) async fn reset_resource_file(
        &self,
        path: &str,
    ) -> Result<VmmNativeResourceFileContent, String> {
        let _gate = self.config_gate.lock().await;
        self.resources.reset_file(&self.core, path)
    }

    /// Applies the active common-plus-locale PII rules to one sample text.
    /// 将当前 common 加区域 PII 规则应用到一段测试文本。
    ///
    /// # Errors
    /// Returns an error when the active resources or sample text violate Core bounds.
    /// 当前资源或测试文本违反 Core 边界时返回错误。
    pub(crate) async fn test_pii(&self, text: &str) -> Result<VmmNativePiiTestResult, String> {
        let _gate = self.config_gate.lock().await;
        self.activate_resource_config_from_active_vector()?;
        let snapshot = self
            .core
            .active_snapshot()
            .map_err(|error| error.to_string())?;
        let result = snapshot
            .pii_scrubber
            .scrub(text)
            .map_err(|error| error.to_string())?;
        Ok(VmmNativePiiTestResult {
            output_text: result.text,
            redactions: result
                .redactions
                .into_iter()
                .map(|redaction| VmmNativePiiRedactionSummary {
                    rule_name: redaction.rule_name,
                    match_count: redaction.match_count,
                })
                .collect(),
        })
    }

    /// Compiles and executes one ad-hoc PII regex plus condition formula without changing resources.
    /// 编译并执行一条临时 PII 正则及条件公式，不修改任何资源。
    ///
    /// # Parameters
    /// 参数
    /// `text`, `pattern`, `condition`, and `replacement` are passed to the copied Core's exact PII
    /// compiler and condition evaluator.
    /// `text`、`pattern`、`condition` 与 `replacement` 会传给复制后 Core 的准确 PII 编译器与条件求值器。
    ///
    /// # Returns
    /// 返回值
    /// The transformed text and a candidate-by-candidate local trace.
    /// 返回转换后的文本与逐候选本地轨迹。
    ///
    /// # Errors
    /// Returns an error when the ad-hoc rule does not satisfy the authoritative Core grammar.
    /// 临时规则不符合权威 Core 语法时返回错误。
    pub(crate) fn test_pii_formula(
        &self,
        text: &str,
        pattern: &str,
        condition: Option<&str>,
        replacement: &str,
    ) -> Result<VmmNativePiiFormulaTestResult, String> {
        let result = test_pii_rule(text, pattern, condition, replacement)
            .map_err(|error| error.to_string())?;
        Ok(VmmNativePiiFormulaTestResult {
            output_text: result.output_text,
            matches: result
                .matches
                .into_iter()
                .map(|entry| VmmNativePiiFormulaMatchSummary {
                    start_byte: entry.start_byte,
                    end_byte: entry.end_byte,
                    matched_text: entry.matched_text,
                    captures: entry.captures,
                    condition_accepted: entry.condition_accepted,
                })
                .collect(),
        })
    }

    /// Applies active deterministic common-plus-locale Noise regex rules to sample text.
    /// 将当前确定性 common 加区域 Noise 正则规则应用到测试文本。
    ///
    /// # Errors
    /// Returns an error for an invalid role or invalid active resource snapshot.
    /// 角色无效或当前资源快照无效时返回错误。
    pub(crate) async fn test_noise(&self, text: &str) -> Result<VmmNativeNoiseTestResult, String> {
        let _gate = self.config_gate.lock().await;
        self.activate_resource_config_from_active_vector()?;
        let snapshot = self
            .core
            .active_snapshot()
            .map_err(|error| error.to_string())?;
        let decision = snapshot
            .noise_detector
            .detect(vulcan_memorymesh_core::Role::User, text);
        Ok(VmmNativeNoiseTestResult {
            is_noise: decision.is_noise,
            category: decision.category,
            confidence: decision.confidence,
            mode: String::from("deterministic_regex"),
        })
    }

    /// Rebuilds and activates persisted resources while preserving the active vector identity.
    /// 在保留当前向量身份的同时重建并激活持久化资源。
    fn activate_resource_config_from_active_vector(&self) -> Result<(), String> {
        let active = self
            .core
            .active_snapshot()
            .map_err(|error| error.to_string())?;
        let bundle = self.resources.current_bundle(
            active.vector_space_fingerprint.clone(),
            active.embedding_dimension,
        )?;
        let snapshot = self
            .core
            .register_config(bundle)
            .map_err(|error| error.to_string())?;
        self.core
            .activate_config(snapshot.name.as_str())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Captures the current complete native storage snapshot for a side-effect-free maintenance preview.
    /// 捕获当前完整原生存储快照，用于无副作用的维护预检。
    ///
    /// # Errors
    /// Returns an error when the in-process storage read lock is poisoned.
    /// 当进程内存储读锁损坏时返回错误。
    pub(crate) fn storage_snapshot(&self) -> Result<StorageSnapshot, String> {
        self.host
            .storage
            .read()
            .map_err(|_| String::from("The native VMM storage read lock is poisoned"))
            .map(|storage| storage.snapshot())
    }

    /// Runs a bounded synchronous recall check while borrowing authoritative storage without cloning vectors.
    /// 借用权威存储执行有界同步召回检查，不复制向量。
    /// Takes a read-only callback; callers retain their native operation barrier through host publication.
    /// 接收只读回调；调用方在宿主发布完成前保持原生操作屏障。
    pub(super) fn with_recall_publication_storage<T>(
        &self,
        check: impl FnOnce(&InMemoryStorage) -> Result<T, String>,
    ) -> Result<T, String> {
        // This read guard also prevents in-process storage publication during the exact check.
        // 此读取守卫同时阻止准确检查期间发布进程内存储。
        let storage = self
            .host
            .storage
            .read()
            .map_err(|_| "The native VMM storage read lock is poisoned".to_owned())?;
        check(&storage)
    }

    /// Plans the supplied retention command against an isolated snapshot without host writes or AI.
    /// 针对隔离快照规划传入的清洗命令，不进行宿主写入或 AI 调用。
    /// Returns the same Core plan used by execution, or an explicit read-only contract error.
    /// 返回执行路径使用的同一个 Core 计划，或明确的只读契约错误。
    pub(super) async fn preview_retention_plan(
        &self,
        snapshot: StorageSnapshot,
        command: vulcan_memorymesh_core::RetentionCommand,
    ) -> Result<vulcan_memorymesh_core::RetentionPlanSet, String> {
        // Only the copied in-memory state is exposed to the preview command.
        // 仅将复制的内存状态暴露给预览命令。
        let mut host = NativeVmmRetentionPreviewDriver {
            storage: InMemoryStorage::from_snapshot(snapshot)?,
        };
        // Retention is a mutation-plan command: canonical identity includes the exact frozen config.
        // 清洗属于变更规划命令，其规范身份包含准确冻结的配置。
        let snapshot = self
            .core
            .active_snapshot()
            .map_err(|error| error.to_string())?;
        let mut command = CoreCommand::Retention(command);
        command_mutation_identity(&mut command).request_fingerprint =
            vulcan_memorymesh_core::request_fingerprint(&command, snapshot.id.as_str());
        let result = self
            .core
            .run_to_completion_with_snapshot_async(command, &snapshot.id, &mut host)
            .await
            .map_err(|error| error.to_string())?;
        match result {
            CommandResult::Retention(plan) => Ok(plan),
            _ => Err(String::from(
                "The native VMM retention preview returned a non-retention-plan result.",
            )),
        }
    }

    /// Reconciles every prepared native storage transition under the runtime mutation gate.
    /// 在运行时变更门内对账每条已准备的原生存储变换。
    ///
    /// # Returns
    /// 返回值
    /// Success after no unfinished native Core mutation remains.
    /// 不再存在未完成原生 Core 变更后返回成功。
    ///
    /// # Errors
    /// Returns the exact durable journal, vector, or relational recovery failure.
    /// 返回准确的耐久账本、向量或关系恢复失败。
    pub(crate) async fn recover_storage_mutations(&self) -> Result<(), String> {
        let _gate = self.host.mutation_gate.lock().await;
        self.host.recover_unfinished_core_mutations().await
    }

    /// Applies one host maintenance transform after checking the previewed storage version.
    /// 校验预检存储版本后应用一次宿主维护变换。
    ///
    /// `operation_id` is the stable user-operation UUID used for durable idempotency.
    /// `operation_id` 是用于耐久幂等的稳定用户操作 UUID。
    /// `mutation_kind` is the exact semantic maintenance operation name.
    /// `mutation_kind` 是准确的语义维护操作名称。
    ///
    /// # Errors
    /// Returns an error for a stale preview, an invalid transformed snapshot, or any persistence failure.
    /// 当预检已过期、变换后快照无效或任一持久化步骤失败时返回错误。
    pub(crate) async fn transform_storage_snapshot<T, F>(
        &self,
        operation_id: &str,
        mutation_kind: &str,
        expected_storage_version: u64,
        transform: F,
    ) -> Result<T, String>
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
        F: FnOnce(&mut StorageSnapshot) -> Result<T, String>,
    {
        let _gate = self.host.mutation_gate.lock().await;
        self.host.recover_unfinished_core_mutations().await?;
        if let Some(existing) = self
            .host
            .data_plane
            .load_core_mutation(operation_id)
            .await?
        {
            if existing.mutation_kind != mutation_kind {
                return Err(format!(
                    "Native VMM operation `{operation_id}` was already used for `{}` and cannot be reused for `{mutation_kind}`",
                    existing.mutation_kind
                ));
            }
            return serde_json::from_str(&existing.result_json).map_err(|error| {
                format!("Failed to parse the idempotent native VMM maintenance result: {error}")
            });
        }
        let before = self.storage_snapshot()?;
        if before.storage_version != expected_storage_version {
            return Err(format!(
                "The native VMM maintenance precheck is stale: expected storage version {expected_storage_version}, current version {}; precheck again",
                before.storage_version
            ));
        }
        let mut after = before.clone();
        let output = transform(&mut after)?;
        after.storage_version = after
            .storage_version
            .checked_add(1)
            .ok_or_else(|| String::from("The native VMM storage versions are exhausted"))?;
        let staged = InMemoryStorage::from_snapshot(after.clone())
            .map_err(|error| format!("Invalid native VMM maintenance result: {error}"))?;
        self.host
            .persist_prepared_storage_transition(
                operation_id,
                mutation_kind,
                &before,
                &after,
                &output,
            )
            .await?;
        // staged was validated before preparation; the resumed durable snapshot is identical.
        // staged 在准备前已完成校验；恢复提交后的耐久快照与其完全一致。
        drop(staged);
        Ok(output)
    }

    /// Replays one previously journaled host maintenance result without requiring live catalog state.
    /// 在不依赖当前项目目录状态的情况下重放一条已经记账的宿主维护结果。
    ///
    /// `operation_id` is the stable user-operation UUID used for durable idempotency.
    /// `operation_id` 是用于耐久幂等的稳定用户操作 UUID。
    /// `mutation_kind` is the payload-bound semantic maintenance operation name.
    /// `mutation_kind` 是绑定了载荷的语义维护操作名称。
    ///
    /// # Returns
    /// 返回值
    /// The recovered typed result when the operation exists, otherwise no value.
    /// 操作存在时返回恢复后的强类型结果，否则返回空值。
    ///
    /// # Errors
    /// Returns an error for recovery failure, operation reuse, or incompatible result JSON.
    /// 恢复失败、操作标识复用或结果 JSON 不兼容时返回错误。
    pub(crate) async fn replay_storage_snapshot_transform<T>(
        &self,
        operation_id: &str,
        mutation_kind: &str,
    ) -> Result<Option<T>, String>
    where
        T: serde::de::DeserializeOwned,
    {
        let _gate = self.host.mutation_gate.lock().await;
        self.host.recover_unfinished_core_mutations().await?;
        let Some(existing) = self
            .host
            .data_plane
            .load_core_mutation(operation_id)
            .await?
        else {
            return Ok(None);
        };
        if existing.mutation_kind != mutation_kind {
            return Err(format!(
                "Native VMM operation `{operation_id}` was already used for `{}` and cannot be reused for `{mutation_kind}`",
                existing.mutation_kind
            ));
        }
        serde_json::from_str(&existing.result_json)
            .map(Some)
            .map_err(|error| {
                format!("Failed to parse the idempotent native VMM maintenance result: {error}")
            })
    }

    /// Activates a Core snapshot derived from the exact configured embedding route.
    /// 激活由精确已配置向量路由派生的 Core 快照。
    async fn ensure_host_ai_config(
        &self,
    ) -> Result<
        (
            NativeVmmVectorSpaceDescriptor,
            Arc<vulcan_memorymesh_core::ConfigSnapshot>,
        ),
        CoreError,
    > {
        self.ensure_host_ai_config_for_project("", None).await
    }

    /// Resolves host capability and caller overrides together under one admission gate.
    /// 在同一准入锁内同时解析宿主能力与调用方覆盖。
    async fn ensure_host_ai_config_for_project(
        &self,
        host_project_id: &str,
        profile_project_id: Option<u64>,
    ) -> Result<
        (
            NativeVmmVectorSpaceDescriptor,
            Arc<vulcan_memorymesh_core::ConfigSnapshot>,
        ),
        CoreError,
    > {
        let _gate = self.config_gate.lock().await;
        let result = self.activate_host_ai_config(host_project_id, profile_project_id);
        let status = match result.as_ref() {
            Ok((handle, _)) => NativeVmmHostAiStatus::ready(handle),
            Err(error) => NativeVmmHostAiStatus::failed(error),
        };
        *self
            .host_ai_status
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = status;
        let (handle, snapshot) = result?;
        Ok((vector_space_descriptor_from_capability(&handle)?, snapshot))
    }

    /// Refreshes capability truth and activates the matching Core vector-space configuration.
    /// 刷新能力事实并激活匹配的 Core 向量空间配置。
    ///
    /// # Returns
    /// 返回值
    /// The freshly validated capability handle and the exact immutable snapshot compiled from it.
    /// 返回最新校验能力句柄及由其编译的精确不可变快照。
    ///
    /// # Errors
    /// Returns a provider-neutral configuration error for missing routes or invalid embedding limits.
    /// 必需路由缺失或向量限制无效时返回供应商无关配置错误。
    fn activate_host_ai_config(
        &self,
        host_project_id: &str,
        profile_project_id: Option<u64>,
    ) -> Result<
        (
            HostAiCapabilityHandle,
            Arc<vulcan_memorymesh_core::ConfigSnapshot>,
        ),
        CoreError,
    > {
        let handle = self.host.capability().map_err(host_ai_core_error)?;
        let (dimension, fingerprint) = embedding_config_from_capability(&handle)?;
        let mut bundle = self
            .resources
            .current_host_project_bundle(fingerprint, dimension, host_project_id)
            .map_err(native_resource_core_error)?;
        // Only profile-producing commands carry a memory owner; personal instructions retain globals.
        // 仅生成画像的命令携带记忆归属；个人画像指令保留全局规则。
        if let Some(project_id) = profile_project_id {
            self.resources
                .apply_memory_project_profile_policy(&mut bundle, project_id)
                .map_err(native_resource_core_error)?;
        }
        let snapshot = self.core.register_config(bundle)?;
        self.core.activate_config(snapshot.name.as_str())?;
        Ok((handle, snapshot))
    }
}

/// Converts a host resource-layer failure into the Core configuration boundary.
/// 将宿主资源层失败转换到 Core 配置边界。
fn native_resource_core_error(error: String) -> CoreError {
    CoreError::new(
        ErrorCode::InvalidConfiguration,
        ErrorStage::Configuration,
        error,
    )
}

/// Executes every provider-independent Core effect against VulcanCode-owned services.
/// 针对 VulcanCode 持有的服务执行每个供应商无关 Core Effect。
#[derive(Clone)]
struct NativeVmmHostDriver {
    /// Unified host AI boundary shared with other native libraries.
    /// 与其他原生库共享的统一宿主 AI 边界。
    ai_port: Arc<dyn HostAiPort>,
    /// Request-scoped cancellation observed by all Host AI effects of one command.
    /// 单个命令全部宿主 AI Effect 观察的请求级取消信号。
    ai_cancellation: Arc<NativeVmmHostAiCancellation>,
    /// Physical row deletions for this command driver only, excluding recovered earlier operations.
    /// 仅属于此命令驱动的物理删除行数，不含恢复的先前操作。
    deleted_vector_rows: u64,
    /// Lazily registered capability so disabled VMM does not require configured inference at startup.
    /// 延迟注册的能力，确保 VMM 禁用时启动阶段不要求已配置推理。
    capability: Arc<RwLock<Option<HostAiCapabilityHandle>>>,
    /// Physical embedding fingerprints observed for logical Core vector spaces.
    /// 为 Core 逻辑向量空间观测到的物理向量指纹。
    embedding_fingerprints: Arc<RwLock<BTreeMap<String, String>>>,
    /// Authoritative in-process semantic state hydrated from the two durable planes.
    /// 从两个持久数据平面恢复的权威进程内语义状态。
    storage: Arc<RwLock<InMemoryStorage>>,
    /// Serializes snapshot-mutate-persist-swap transactions across concurrent commands.
    /// 在并发命令之间串行化快照、变更、持久化与交换事务。
    mutation_gate: Arc<tokio::sync::Mutex<()>>,
    /// Synchronous post-commit cache sink, shared by all cloned command drivers without owning the service.
    /// 全部克隆命令驱动共享的同步提交后缓存接收器，不拥有服务。
    content_projection: Arc<RwLock<Option<super::maintenance::NativeVmmContentProjection>>>,
    /// VulcanCode-hosted SQLite and LanceDB bindings.
    /// 由 VulcanCode 托管的 SQLite 与 LanceDB 绑定。
    data_plane: VmmDataPlaneHandle,
}

impl AsyncHostDriver for NativeVmmHostDriver {
    /// Executes one exact typed effect without exposing Providers to Core.
    /// 执行一个精确强类型 Effect，且不向 Core 暴露 Provider。
    fn execute_effect<'a>(
        &'a mut self,
        request: EffectRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EffectResult, CoreError>> + Send + 'a>> {
        Box::pin(async move {
            let effect_id = request.effect_id.clone();
            let result = match request.effect.clone() {
                HostEffect::Llm(effect) => self.execute_llm(&request, &effect).await?,
                HostEffect::Embedding(effect) => self.execute_embedding(&request, &effect).await?,
                HostEffect::Rerank(effect) => self.execute_rerank(&request, &effect).await?,
                HostEffect::StorageQuery(query) => self.execute_storage_query(&query).await?,
                HostEffect::StorageMutation(batch) => self.execute_storage_mutation(&batch).await?,
            };
            Ok(EffectResult { effect_id, result })
        })
    }
}

impl NativeVmmHostDriver {
    /// Persists and completes one already validated storage transition through the recovery journal.
    /// 通过恢复账本持久化并完成一个已经校验的存储状态变换。
    ///
    /// # Parameters
    /// 参数
    /// `operation_id` is the stable durable identity, `mutation_kind` binds that identity to one
    /// semantic payload, `before` and `after` are the exact snapshots, and `output` is replayed.
    /// `operation_id` 是稳定耐久身份，`mutation_kind` 将该身份绑定到一个语义载荷，
    /// `before` 与 `after` 是准确快照，`output` 用于幂等重放。
    ///
    /// # Returns
    /// 返回值
    /// Actual immediate vector deletions after vector, relational, in-memory, and journal state agree.
    /// 向量、关系、内存与账本状态一致后返回实际即时向量删除数。
    ///
    /// # Errors
    /// Returns the exact serialization, preparation, persistence, or reconciliation error.
    /// 返回准确的序列化、准备、持久化或对账错误。
    async fn persist_prepared_storage_transition<T>(
        &self,
        operation_id: &str,
        mutation_kind: &str,
        before: &StorageSnapshot,
        after: &StorageSnapshot,
        output: &T,
    ) -> Result<u64, String>
    where
        T: serde::Serialize,
    {
        // Vector-space metadata is durable before the journal so recovery never depends on process-local fingerprints.
        // 向量空间元数据先于账本耐久化，确保恢复过程不依赖进程内指纹。
        self.ensure_vector_spaces(after)
            .await
            .map_err(|error| error.to_string())?;
        // mutation records both sides before any vector write so restart recovery always moves forward deterministically.
        // mutation 在任何向量写入前记录前后两侧，确保重启恢复始终确定性向前推进。
        let mutation = NativeVmmCoreMutation {
            operation_id: operation_id.to_owned(),
            mutation_kind: mutation_kind.to_owned(),
            state: String::from(VMM_CORE_MUTATION_PREPARED),
            before_snapshot_json: serde_json::to_string(before).map_err(|error| {
                format!("Failed to serialize the native VMM pre-mutation snapshot: {error}")
            })?,
            after_snapshot_json: serde_json::to_string(after).map_err(|error| {
                format!("Failed to serialize the native VMM post-mutation snapshot: {error}")
            })?,
            result_json: serde_json::to_string(output).map_err(|error| {
                format!("Failed to serialize the native VMM mutation result: {error}")
            })?,
        };
        self.data_plane.prepare_core_mutation(&mutation).await?;
        self.resume_core_mutation(&mutation).await
    }

    /// Completes every interrupted native maintenance mutation before accepting new work.
    /// 在接受新工作前完成每条中断的原生维护变更。
    ///
    /// # Returns
    /// 返回值
    /// Success when no prepared or committed mutation remains.
    /// 当不再存在已准备或已提交变更时返回成功。
    ///
    /// # Errors
    /// Returns an exact journal, snapshot, vector, or relational recovery error.
    /// 返回准确的账本、快照、向量或关系恢复错误。
    async fn recover_unfinished_core_mutations(&self) -> Result<(), String> {
        let mutations = self.data_plane.list_unfinished_core_mutations().await?;
        for mutation in mutations {
            self.resume_core_mutation(&mutation).await?;
        }
        Ok(())
    }

    /// Resumes one prepared or relationally committed maintenance mutation to completion.
    /// 将一条已准备或关系已提交的维护变更恢复至完成。
    ///
    /// # Parameters
    /// 参数
    /// `mutation` is the exact durable recovery record loaded from host SQLite.
    /// `mutation` 是从宿主 SQLite 加载的准确耐久恢复记录。
    ///
    /// # Returns
    /// 返回值
    /// Actual vector deletions after vectors, relational state, memory, and cleanup agree; completed replay returns zero.
    /// 向量、关系、内存及清理一致后返回实际向量删除数；已完成重放返回零。
    ///
    /// # Errors
    /// Returns a controlled error while leaving a resumable journal state durable.
    /// 返回受控错误，并保留可恢复的耐久账本状态。
    async fn resume_core_mutation(&self, mutation: &NativeVmmCoreMutation) -> Result<u64, String> {
        if mutation.state == VMM_CORE_MUTATION_COMPLETED {
            return Ok(0);
        }
        let before: StorageSnapshot = serde_json::from_str(&mutation.before_snapshot_json)
            .map_err(|error| {
                format!("Failed to parse the native VMM pre-maintenance snapshot: {error}")
            })?;
        let after: StorageSnapshot =
            serde_json::from_str(&mutation.after_snapshot_json).map_err(|error| {
                format!("Failed to parse the native VMM post-maintenance snapshot: {error}")
            })?;
        let staged = InMemoryStorage::from_snapshot(after.clone())
            .map_err(|error| format!("Invalid native VMM recovery snapshot: {error}"))?;
        if mutation.state == VMM_CORE_MUTATION_PREPARED {
            self.persist_vectors(&after)
                .await
                .map_err(|error| error.to_string())?;
            let relational =
                serde_json::to_string(&after.without_embeddings()).map_err(|error| {
                    format!(
                        "Failed to serialize the native VMM recovery relation snapshot: {error}"
                    )
                })?;
            let pending_turn_id = pending_turn_id_from_core_mutation(mutation)?;
            // Compare both active and recoverable records: a moved turn is not permanently removed.
            // 比较活动和可恢复记录：已移动回合并非永久移除。
            let removed_turn_ids = permanently_removed_turn_ids(&before, &after);
            self.data_plane
                .commit_core_mutation(
                    &mutation.operation_id,
                    &relational,
                    pending_turn_id.as_deref(),
                    &removed_turn_ids,
                )
                .await?;
        } else if mutation.state != VMM_CORE_MUTATION_COMMITTED {
            return Err(format!(
                "Native VMM maintenance mutation `{}` has an unknown state `{}`",
                mutation.operation_id, mutation.state
            ));
        }
        // Publish searchable text before exposing the committed relational snapshot to readers.
        // 向读取方公开已提交关系快照前，先发布可搜索文本。
        self.persist_lexical_transition(&before, &after).await?;
        *self
            .storage
            .write()
            .map_err(|_| String::from("The native VMM storage write lock is poisoned; restart to recover the maintenance mutation"))? = staged;
        // This transition's physical count is returned, not merged into unrelated recovery or command state.
        // 返回本次变换的物理计数，不合并到无关恢复或命令状态。
        let deleted_vector_rows = self.delete_stale_vectors_strict(&before, &after).await?;
        self.data_plane
            .complete_core_mutation(&mutation.operation_id)
            .await?;
        // Committed content remains serialized by mutation_gate through resident cache publication.
        // 已提交内容在驻留缓存发布期间仍由 mutation_gate 串行保护。
        // Cache errors do not mean the already committed Core transaction was rolled back.
        // 缓存错误不表示已经提交的 Core 事务发生了回滚。
        if let Err(error) = self.project_committed_content() {
            tracing::warn!(
                "[native-vmm] content committed but resident cache projection failed: {error}"
            );
        }
        Ok(deleted_vector_rows)
    }

    /// Reconciles under a storage read guard, then emits after both storage and service locks are released.
    /// 在存储读守卫内对账，然后在存储及服务锁均释放后发送事件。
    /// Uses the installed sink and current committed storage; returns explicit cache-projection errors.
    /// 使用已安装接收器及当前已提交存储；返回明确缓存投影错误。
    fn project_committed_content(&self) -> Result<(), String> {
        // Clone only the weak sink; retaining its installation lock across callbacks is unnecessary.
        // 只克隆弱引用接收器；不需要在回调期间保留安装锁。
        let projection = self
            .content_projection
            .read()
            .map_err(|error| error.to_string())?
            .clone();
        let Some(projection) = projection else {
            return Ok(());
        };
        // Borrow current storage without copying records, vectors or a complete Core snapshot.
        // 借用当前存储，不复制记录、向量或完整 Core 快照。
        let events = {
            // A read guard remains compatible with ordinary controller-side recall publication checks.
            // 读守卫与普通控制器侧召回发布检查兼容。
            let storage = self.storage.read().map_err(|error| error.to_string())?;
            projection.reconcile(&storage)?
        };
        projection.emit(events);
        Ok(())
    }

    /// Returns the existing VMM AI capability or registers it exactly once on first use.
    /// 返回现有 VMM AI 能力，或在首次使用时仅注册一次。
    fn capability(&self) -> Result<HostAiCapabilityHandle, HostAiError> {
        let mut guard = self
            .capability
            .write()
            .map_err(|_| host_ai_internal_error("native VMM capability write lock poisoned"))?;
        if let Some(existing) = guard.as_ref() {
            let mut refreshed = existing.clone();
            refreshed.snapshot = self.ai_port.capability_snapshot(existing)?;
            validate_native_vmm_required_capabilities(&refreshed)?;
            *guard = Some(refreshed.clone());
            return Ok(refreshed);
        }
        let registration = native_vmm_capability_registration();
        let handle = self.ai_port.register_capability(registration)?;
        validate_native_vmm_required_capabilities(&handle)?;
        *guard = Some(handle.clone());
        Ok(handle)
    }

    /// Runs one strict structured-output LLM effect through the unified host AI Port.
    /// 通过统一宿主 AI Port 运行一个严格结构化输出 LLM Effect。
    async fn execute_llm(
        &self,
        request: &EffectRequest,
        effect: &LlmEffect,
    ) -> Result<HostEffectResult, CoreError> {
        let capability = match self.capability() {
            Ok(value) => value,
            Err(error) => return Ok(ai_failure(EffectFailureKind::Llm, error)),
        };
        let body = prompt::render_llm_input(&effect.input)?;
        let schema_bytes = llm_contract::host_json_schema(effect.output_contract)?;
        let schema = serde_json::from_slice(&schema_bytes).map_err(|error| {
            core_effect_error(format!("failed to decode native VMM JSON Schema: {error}"))
        })?;
        let purpose_id = llm_purpose_id(effect.purpose);
        let request_id = format!("vmm-llm-{}", request.effect_id.as_str());
        let ai_request = ResponsesExecutionRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: request_id.clone(),
            idempotency_key: None,
            consumer_profile_id: String::from(NATIVE_VMM_CONSUMER_PROFILE_ID),
            purpose_id: purpose_id.to_owned(),
            messages: vec![
                AiMessage {
                    role: MessageRole::System,
                    content: vec![ContentItem::Text {
                        text: effect.system_prompt.clone(),
                    }],
                    provider_context_items: Vec::new(),
                },
                AiMessage {
                    role: MessageRole::User,
                    content: vec![ContentItem::Text { text: body }],
                    provider_context_items: Vec::new(),
                },
            ],
            tools: Vec::new(),
            native_tools: Vec::new(),
            tool_choice: Default::default(),
            parallel_tool_calls: Default::default(),
            structured_output: StructuredOutputPolicy::JsonSchema {
                name: structured_output_name(effect.output_contract).to_owned(),
                description: Some(String::from("VMM Core authoritative structured output")),
                schema,
                strict: true,
            },
            prompt_cache: Default::default(),
            prompt_cache_key: None,
            include_reasoning_encrypted_content: false,
            reasoning_effort: Some(ReasoningEffort::None),
            reasoning_output: ReasoningOutputPolicy::Hidden,
            model_parameters: BTreeMap::new(),
            stop_sequences: Vec::new(),
            resources: Vec::new(),
            output_token_limit: Some(NATIVE_VMM_LLM_OUTPUT_TOKENS),
        };
        let context = effect_call_context(request, Arc::clone(&self.ai_cancellation));
        let execution = match self
            .ai_port
            .start_response_execution(&capability, context, ai_request)
            .await
        {
            Ok(value) => value,
            Err(error) => return Ok(ai_failure(EffectFailureKind::Llm, error)),
        };
        let response = match self.ai_port.wait_response(&execution).await {
            Ok(value) => value,
            Err(error) => return Ok(ai_failure(EffectFailureKind::Llm, error)),
        };
        let encoded;
        let bytes = if let Some(value) = response.structured_output.as_ref() {
            encoded = serde_json::to_vec(value).map_err(|error| {
                core_effect_error(format!(
                    "failed to encode native VMM structured output: {error}"
                ))
            })?;
            encoded.as_slice()
        } else {
            response.output_text.as_bytes()
        };
        let output = llm_contract::parse_host_response(effect, bytes)?;
        Ok(HostEffectResult::Llm(output))
    }

    /// Runs one ordered dense embedding batch through the unified host AI Port.
    /// 通过统一宿主 AI Port 运行一个有序稠密向量批次。
    async fn execute_embedding(
        &self,
        request: &EffectRequest,
        effect: &vulcan_memorymesh_core::EmbeddingEffect,
    ) -> Result<HostEffectResult, CoreError> {
        self.execute_embedding_with_context(
            effect_call_context(request, Arc::clone(&self.ai_cancellation)),
            format!("vmm-embedding-{}", request.effect_id.as_str()),
            effect,
        )
        .await
    }

    /// Runs one ordered dense embedding batch with an explicit host-owned call context.
    /// 使用显式宿主持有调用上下文运行一个有序稠密向量批次。
    async fn execute_embedding_with_context(
        &self,
        context: HostAiCallContext,
        request_id: String,
        effect: &vulcan_memorymesh_core::EmbeddingEffect,
    ) -> Result<HostEffectResult, CoreError> {
        let capability = match self.capability() {
            Ok(value) => value,
            Err(error) => return Ok(ai_failure(EffectFailureKind::Embedding, error)),
        };
        let inputs = effect
            .inputs
            .iter()
            .enumerate()
            .map(|(index, text)| EmbeddingInput {
                input_id: format!("input-{index}"),
                content: EmbeddingInputContent::Text { text: text.clone() },
            })
            .collect::<Vec<_>>();
        let embedding_request = EmbeddingRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id,
            idempotency_key: None,
            consumer_profile_id: String::from(NATIVE_VMM_CONSUMER_PROFILE_ID),
            inputs,
            task: NATIVE_VMM_EMBEDDING_TASK,
            dimensions: Some(effect.expected_dimension),
            output_kind: EmbeddingOutputKind::Dense,
            encoding: EmbeddingEncoding::Float,
            invalid_input_policy: if effect.allow_partial_invalid_inputs {
                EmbeddingInvalidInputPolicy::DropProviderConfirmedItems
            } else {
                EmbeddingInvalidInputPolicy::FailBatch
            },
        };
        let host_request = HostAiEmbeddingRequest {
            vector_space_id: effect.vector_space_id.as_str().to_owned(),
            request: embedding_request.clone(),
        };
        let response = match self
            .ai_port
            .create_embeddings(&capability, context, host_request)
            .await
        {
            Ok(value) => value,
            Err(error) => return Ok(ai_failure(EffectFailureKind::Embedding, error)),
        };
        response
            .response
            .validate_for_request(&embedding_request)
            .map_err(core_effect_error)?;
        self.remember_embedding_fingerprint(
            effect.vector_space_id.as_str(),
            &response.response.fingerprint,
        )?;
        let expected_dimension = usize::try_from(effect.expected_dimension)
            .map_err(|error| core_effect_error(format!("embedding dimension overflow: {error}")))?;
        let mut vectors = Vec::with_capacity(response.response.results.len());
        let mut result_indices = Vec::with_capacity(response.response.results.len());
        for result in response.response.results {
            let EmbeddingPayload::Dense { values } = result.embedding else {
                return Err(core_effect_error(
                    "native VMM embedding response is not dense float output",
                ));
            };
            if values.len() != expected_dimension || values.iter().any(|value| !value.is_finite()) {
                return Err(core_effect_error(
                    "native VMM embedding response has an invalid dimension or value",
                ));
            }
            vectors.push(values);
            result_indices.push(result.input_index);
        }
        let dropped = response
            .response
            .dropped
            .into_iter()
            .map(|value| {
                Ok(EmbeddingDroppedInput {
                    input_index: value.input_index,
                    reason: embedding_drop_reason(value.reason_code.as_str())?,
                })
            })
            .collect::<Result<Vec<_>, CoreError>>()?;
        Ok(HostEffectResult::Embedding(EmbeddingBatchResult {
            vectors,
            result_indices,
            dropped,
        }))
    }

    /// Runs one provider-ordered rerank batch through the unified host AI Port.
    /// 通过统一宿主 AI Port 运行一个由 Provider 排序的重排批次。
    async fn execute_rerank(
        &self,
        request: &EffectRequest,
        effect: &RerankEffect,
    ) -> Result<HostEffectResult, CoreError> {
        let capability = match self.capability() {
            Ok(value) => value,
            Err(error) => return Ok(ai_failure(EffectFailureKind::Rerank, error)),
        };
        let candidates = effect
            .documents
            .iter()
            .enumerate()
            .map(|(index, text)| RerankText {
                id: format!("document-{index}"),
                text: text.clone(),
            })
            .collect::<Vec<_>>();
        let top_n = u32::try_from(candidates.len())
            .map_err(|error| core_effect_error(format!("rerank candidate overflow: {error}")))?;
        let rerank_request = RerankRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: format!("vmm-rerank-{}", request.effect_id.as_str()),
            idempotency_key: None,
            consumer_profile_id: String::from(NATIVE_VMM_CONSUMER_PROFILE_ID),
            query: RerankText {
                id: String::from("query"),
                text: effect.query.clone(),
            },
            candidates,
            top_n,
            truncation: RerankTruncation::ProviderDefault,
            return_content: false,
        };
        let response = match self
            .ai_port
            .rerank_documents(
                &capability,
                effect_call_context(request, Arc::clone(&self.ai_cancellation)),
                rerank_request,
            )
            .await
        {
            Ok(value) => value,
            Err(error) => return Ok(ai_failure(EffectFailureKind::Rerank, error)),
        };
        let entries = response
            .results
            .into_iter()
            .map(|value| RerankEntry {
                input_index: value.original_index,
                score: value.score,
            })
            .collect();
        Ok(HostEffectResult::Rerank(entries))
    }

    /// Executes semantic reads with host-owned vector and lexical producers.
    /// 通过控制器支持的向量及词法生产者执行语义读取。
    async fn execute_storage_query(
        &self,
        query: &StorageQuery,
    ) -> Result<HostEffectResult, CoreError> {
        // read_gate keeps both physical producers and relational visibility on one admitted state.
        // read_gate 使两个物理生产者及关系可见性处于同一已接纳状态。
        let read_gate = if matches!(
            query,
            StorageQuery::VectorCandidates { .. }
                | StorageQuery::LexicalCandidates { .. }
                | StorageQuery::CandidateObservation(_)
        ) {
            Some(self.mutation_gate.lock().await)
        } else {
            None
        };
        if read_gate.is_some() {
            // Complete an admitted mutation before reading its derived indexes and relational state together.
            // 同时读取派生索引和关系状态前，先完成已接纳的变更。
            self.recover_unfinished_core_mutations()
                .await
                .map_err(storage_backend_error)?;
        }
        let result = match query {
            StorageQuery::VectorCandidates { .. } => {
                StorageQueryResult::VectorCandidates(self.external_vector_channel(query).await?)
            }
            StorageQuery::LexicalCandidates { .. } => {
                StorageQueryResult::LexicalCandidates(self.external_lexical_channel(query).await?)
            }
            StorageQuery::CandidateObservation(_) => StorageQueryResult::CandidateObservation(
                self.external_candidate_observation(query).await?,
            ),
            _ => {
                let guard = self
                    .storage
                    .read()
                    .map_err(|_| storage_lock_error("read"))?;
                guard.query(query)?
            }
        };
        Ok(HostEffectResult::StorageQuery(result))
    }

    /// Combines the LanceDB and Jieba channels under one native mutation gate and relational snapshot.
    /// 在同一原生变更门和关系快照内组合 LanceDB 与 Jieba 通道。
    async fn external_candidate_observation(
        &self,
        query: &StorageQuery,
    ) -> Result<CandidateObservation, CoreError> {
        let vector = self.external_vector_channel(query).await?;
        let StorageQuery::CandidateObservation(request) = query else {
            return Err(core_effect_error(
                "candidate observation requires the exact observation query",
            ));
        };
        // lexical is absent only when explicitly disabled, never when a physical read fails.
        // lexical 仅在显式禁用时缺失，绝不以缺失表示物理读取失败。
        let lexical = if request.lexical_contract.is_some() {
            Some(self.external_lexical_channel(query).await?)
        } else {
            None
        };
        let mut available = lexical
            .iter()
            .flat_map(|channel| channel.memories.iter().cloned())
            .map(|memory| (memory.id.as_str().to_owned(), memory))
            .collect::<BTreeMap<_, _>>();
        for memory in vector.memories.iter().cloned() {
            available.insert(memory.id.as_str().to_owned(), memory);
        }
        let mut ordered_ids = vector
            .candidates
            .iter()
            .map(|candidate| candidate.memory_id.as_str())
            .collect::<Vec<_>>();
        if let Some(lexical) = lexical.as_ref() {
            ordered_ids.extend(
                lexical
                    .candidates
                    .iter()
                    .map(|candidate| candidate.memory_id.as_str()),
            );
        }
        let mut seen = BTreeSet::new();
        let memories = ordered_ids
            .into_iter()
            .filter(|identity| seen.insert((*identity).to_owned()))
            .map(|identity| {
                available.remove(identity).ok_or_else(|| {
                    core_effect_error("candidate observation is missing one channel record")
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CandidateObservation {
            vector_channel: PhysicalCandidateChannel {
                candidates: vector.candidates,
            },
            lexical_channel: lexical.map(|channel| PhysicalCandidateChannel {
                candidates: channel.candidates,
            }),
            memories,
        })
    }

    /// Searches bounded Jieba pages until enough admissible hits exist for each query or its index ends.
    /// 逐页搜索 Jieba，直到每条查询的合法命中足够或索引读尽。
    /// `query` fixes the ownership, lifecycle time and lexical contract; the caller holds the mutation gate.
    /// `query` 固定归属、生命周期时刻及词法契约；调用方持有变更门。
    /// Returns original BM25 values with external-VMM rank projection, or the exact storage failure.
    /// 返回原始 BM25 值及外置 VMM 排名投影，或准确的存储失败。
    async fn external_lexical_channel(
        &self,
        query: &StorageQuery,
    ) -> Result<CandidateChannelResult, CoreError> {
        // texts, limit and contract come only from the two declared lexical-bearing query variants.
        // texts、limit 和 contract 只来自两种声明为携带词法通道的查询变体。
        let (texts, limit, contract) = match query {
            StorageQuery::LexicalCandidates {
                queries,
                limit,
                expected_contract,
                ..
            } => (queries.as_slice(), *limit, expected_contract),
            StorageQuery::CandidateObservation(request) => (
                std::slice::from_ref(&request.query),
                request.limit,
                request
                    .lexical_contract
                    .as_ref()
                    .ok_or_else(|| core_effect_error("lexical observation channel is disabled"))?,
            ),
            _ => {
                return Err(core_effect_error(
                    "lexical producer requires a lexical-bearing query",
                ));
            }
        };
        if *contract != lexical::score_contract().contract {
            return Err(CoreError::new(
                ErrorCode::ScoreContractViolation,
                ErrorStage::Command,
                "native lexical query does not select the declared Jieba BM25 contract",
            ));
        }
        // Validate authority even for empty queries, empty indexes and a zero result limit.
        // 对空查询、空索引及零结果上限也校验权限。
        self.storage
            .read()
            .map_err(|_| storage_lock_error("read"))?
            .materialize_external_lexical_candidates(query, Vec::new())?;
        // best deduplicates admitted records across query texts without mixing other score spaces.
        // best 对多条查询文本的合法记录去重，不混入其他评分空间。
        let mut best = BTreeMap::new();
        for text in texts {
            // offset counts physical hits while admitted counts only authoritative visible records.
            // offset 统计物理命中，admitted 仅统计权威可见记录。
            let mut offset = 0_u32;
            let mut admitted = 0_usize;
            while admitted < limit as usize {
                // hits is one bounded physical page, including identities later rejected by Core.
                // hits 是一页有界物理结果，包含随后被 Core 拒绝的身份。
                let hits = self
                    .data_plane
                    .search_lexical_page(text, 128, offset)
                    .await
                    .map_err(storage_backend_error)?;
                // page_len distinguishes a partial last page from a fully filtered intermediate page.
                // page_len 区分不足一页的末页及整页均被过滤的中间页。
                let page_len = hits.len();
                if page_len == 0 {
                    break;
                }
                // candidates preserve raw producer facts before the admitted-pool rank projection.
                // candidates 在合法候选池排名投影前保留原始生产者事实。
                let candidates =
                    hits.into_iter()
                        .enumerate()
                        .map(|(index, hit)| {
                            Ok(PhysicalCandidate {
                                memory_id: hit.memory_id,
                                physical_rank: offset
                                    .checked_add(u32::try_from(index).map_err(|_| {
                                        core_effect_error("lexical page rank overflow")
                                    })?)
                                    .ok_or_else(|| core_effect_error("lexical rank overflow"))?,
                                score: PhysicalCandidateScore {
                                    raw_value: hit.raw_score,
                                    raw_score_kind: RawScoreKind::Bm25,
                                    direction: ScoreDirection::LowerIsBetter,
                                    decision_score: None,
                                    contract: contract.clone(),
                                },
                            })
                        })
                        .collect::<Result<Vec<_>, CoreError>>()?;
                // page contains only the records admitted by the query's complete ownership boundary.
                // page 仅包含被查询完整归属边界接纳的记录。
                let page = self
                    .storage
                    .read()
                    .map_err(|_| storage_lock_error("read"))?
                    .materialize_external_lexical_candidates(query, candidates)?;
                for (candidate, memory) in page
                    .candidates
                    .into_iter()
                    .zip(page.memories)
                    .take(limit as usize - admitted)
                {
                    admitted += 1;
                    // entry keeps the strongest lexical match for an identity seen in multiple query texts.
                    // entry 为出现在多条查询文本中的身份保留最强词法命中。
                    let entry = best
                        .entry(candidate.memory_id.clone())
                        .or_insert((candidate.score.raw_value, memory));
                    entry.0 = entry.0.min(candidate.score.raw_value);
                }
                if page_len < 128 {
                    break;
                }
                offset = offset
                    .checked_add(128)
                    .ok_or_else(|| core_effect_error("lexical page offset overflow"))?;
            }
        }
        // ranked uses BM25 then stable identity, and projects only the final admitted channel.
        // ranked 按 BM25 再按稳定身份排序，仅对最终合法通道进行分数投影。
        let mut ranked = best.into_iter().collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            left.1
                .0
                .total_cmp(&right.1.0)
                .then_with(|| left.0.cmp(&right.0))
        });
        ranked.truncate(limit as usize);
        // total is the actual admitted channel size, not the global unfiltered index size.
        // total 是实际合法通道大小，而不是全局未过滤索引大小。
        let total = ranked.len();
        // result keeps record order aligned exactly with the projected candidate order.
        // result 使记录顺序与投影候选顺序精确一致。
        let mut result = CandidateChannelResult {
            candidates: Vec::new(),
            memories: Vec::new(),
        };
        for (rank, (memory_id, (raw_value, memory))) in ranked.into_iter().enumerate() {
            result.candidates.push(PhysicalCandidate {
                memory_id,
                physical_rank: u32::try_from(rank)
                    .map_err(|_| core_effect_error("lexical result rank overflow"))?,
                score: PhysicalCandidateScore {
                    raw_value,
                    raw_score_kind: RawScoreKind::Bm25,
                    direction: ScoreDirection::LowerIsBetter,
                    decision_score: Some(lexical::rank_score(rank, total)),
                    contract: contract.clone(),
                },
            });
            result.memories.push(memory);
        }
        Ok(result)
    }

    /// Synchronizes searchable text for the same admitted before/after mutation used by project maintenance.
    /// 为项目维护共用的同一已接纳前后变更同步可搜索文本。
    /// `before`/`after` are validated Core snapshots; inactive or removed records lose their index rows.
    /// `before`/`after` 是通过校验的 Core 快照；失效或移除记录会失去其索引行。
    /// Returns an explicit controller error and never acknowledges an incomplete lexical transition.
    /// 返回明确控制器错误，绝不确认尚未完成的词法变更。
    async fn persist_lexical_transition(
        &self,
        before: &StorageSnapshot,
        after: &StorageSnapshot,
    ) -> Result<(), String> {
        // previous resolves text changes by immutable memory identity rather than display names.
        // previous 按不可变记忆身份解析文本变化，而不是按显示名称。
        let previous = before
            .dataset
            .memories
            .iter()
            .map(|memory| (memory.id.as_str(), memory))
            .collect::<BTreeMap<_, _>>();
        // active is the authoritative post-mutation searchable lifecycle set.
        // active 是权威的变更后可搜索生命周期集合。
        let active = after
            .dataset
            .memories
            .iter()
            .filter(|memory| memory.state == MemoryState::Active)
            .collect::<Vec<_>>();
        // active_ids identifies removals without conflating ownership changes with identity changes.
        // active_ids 标识移除，不将归属变化混同于身份变化。
        let active_ids = active
            .iter()
            .map(|memory| memory.id.as_str())
            .collect::<BTreeSet<_>>();
        // changed skips unchanged text during recall counters and project-only ownership changes.
        // changed 在召回计数和纯项目归属变化时跳过未变化文本。
        let changed = active
            .into_iter()
            .filter(|memory| {
                !previous.get(memory.id.as_str()).is_some_and(|old| {
                    old.state == MemoryState::Active
                        && old.summary == memory.summary
                        && old.content == memory.content
                })
            })
            .map(|memory| VmmLexicalDocument {
                memory_id: memory.id.to_string(),
                summary: memory.summary.clone(),
                details: memory.content.clone(),
            })
            .collect::<Vec<_>>();
        for chunk in changed.chunks(256) {
            self.data_plane.upsert_lexical_documents(chunk).await?;
        }
        // removed names only exact previous identities now absent from the searchable set.
        // removed 只列出当前可搜索集合中已不存在的精确旧身份。
        let removed = before
            .dataset
            .memories
            .iter()
            .filter(|memory| !active_ids.contains(memory.id.as_str()))
            .map(|memory| memory.id.to_string())
            .collect::<Vec<_>>();
        for chunk in removed.chunks(256) {
            self.data_plane.delete_lexical_documents(chunk).await?;
        }
        Ok(())
    }

    /// Searches every query vector, keeps the best exact cosine per memory, and materializes Core visibility.
    /// 检索每个查询向量，为每条记忆保留最佳精确余弦分数，并物化 Core 可见性。
    async fn external_vector_channel(
        &self,
        query: &StorageQuery,
    ) -> Result<CandidateChannelResult, CoreError> {
        let view = VectorQueryView::from_query(query)?;
        let Some(binding) = self
            .data_plane
            .resolve_vector_space(view.vector_space_id)
            .await
            .map_err(storage_backend_error)?
        else {
            return self
                .storage
                .read()
                .map_err(|_| storage_lock_error("read"))?
                .materialize_external_vector_candidates(query, Vec::new());
        };
        if let Some(observed_fingerprint) = self
            .embedding_fingerprints
            .read()
            .map_err(|_| storage_lock_error("read embedding fingerprint"))?
            .get(view.vector_space_id)
            && observed_fingerprint != &binding.physical_fingerprint_json
        {
            return Err(storage_backend_error(format!(
                "native VMM vector space `{}` was created by a different physical embedding route",
                view.vector_space_id
            )));
        }
        let filter = VmmVectorSearchFilter {
            subject_id: view.subject_id.to_owned(),
            visible_scope_ids: view.visible_scope_ids.to_vec(),
            candidate_scope_ids: view.candidate_scope_ids,
            origin_session_source_id: view.origin_session_source_id.map(str::to_owned),
        };
        let mut best = BTreeMap::<String, (f64, Vec<f32>)>::new();
        for query_vector in view.query_embeddings {
            let normalized = normalize_vector(query_vector)?;
            let rows = self
                .data_plane
                .search_vectors(&binding, normalized, &filter)
                .await
                .map_err(storage_backend_error)?;
            retain_best_cosines(&mut best, query_vector, rows)?;
        }
        let mut ranked = best.into_iter().collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .1
                .0
                .total_cmp(&left.1.0)
                .then_with(|| left.0.cmp(&right.0))
        });
        let candidates = ranked
            .into_iter()
            .enumerate()
            .map(|(rank, (memory_id, (score, _)))| {
                let physical_rank = u32::try_from(rank).map_err(|error| {
                    core_effect_error(format!("vector physical rank overflow: {error}"))
                })?;
                Ok(PhysicalCandidate {
                    memory_id,
                    score: PhysicalCandidateScore {
                        raw_value: score,
                        raw_score_kind: RawScoreKind::CosineSimilarity,
                        direction: ScoreDirection::HigherIsBetter,
                        decision_score: Some((score + 1.0) / 2.0),
                        contract: view.expected_contract.clone(),
                    },
                    physical_rank,
                })
            })
            .collect::<Result<Vec<_>, CoreError>>()?;
        let guard = self
            .storage
            .read()
            .map_err(|_| storage_lock_error("read"))?;
        guard.materialize_external_vector_candidates(query, candidates)
    }

    /// Applies one Core mutation transaction through the durable host recovery journal.
    /// 通过宿主耐久恢复账本应用一个 Core 变更事务。
    async fn execute_storage_mutation(
        &mut self,
        batch: &MutationBatch,
    ) -> Result<HostEffectResult, CoreError> {
        let _gate = self.mutation_gate.lock().await;
        self.recover_unfinished_core_mutations()
            .await
            .map_err(storage_uncertain_error)?;
        // mutationKind binds the durable operation to the exact Core idempotency and request fingerprint.
        // mutationKind 将耐久操作绑定到准确的 Core 幂等键与请求指纹。
        let mutation_kind = format!(
            "core_storage_mutation:{}:{}",
            batch.identity.idempotency_key.as_str(),
            batch.identity.request_fingerprint
        );
        if let Some(existing) = self
            .data_plane
            .load_core_mutation(batch.identity.operation_id.as_str())
            .await
            .map_err(storage_uncertain_error)?
        {
            if existing.mutation_kind != mutation_kind {
                return Err(storage_uncertain_error(format!(
                    "native VMM operation `{}` was already used for `{}` instead of `{mutation_kind}`",
                    batch.identity.operation_id.as_str(),
                    existing.mutation_kind
                )));
            }
            let outcome = serde_json::from_str(&existing.result_json).map_err(|error| {
                storage_uncertain_error(format!(
                    "failed to decode native VMM idempotent mutation result: {error}"
                ))
            })?;
            return Ok(HostEffectResult::StorageMutation(outcome));
        }
        let before = self
            .storage
            .read()
            .map_err(|_| storage_lock_error("read"))?
            .snapshot();
        let mut staged =
            InMemoryStorage::from_snapshot(before.clone()).map_err(storage_backend_error)?;
        // Commit time belongs to the host clock, not to the turn's older business timestamp.
        // 提交时间归属宿主时钟，不能采用回合较早的业务时间戳。
        let committed_at_ns = i64::try_from(time::OffsetDateTime::now_utc().unix_timestamp_nanos())
            .map_err(|_| {
                storage_backend_error("native VMM commit timestamp exceeds i64".to_owned())
            })?;
        let outcome = staged.mutate_at(batch, committed_at_ns)?;
        if !matches!(outcome, MutationOutcome::Committed { .. }) {
            return Ok(HostEffectResult::StorageMutation(outcome));
        }
        let after = staged.snapshot();
        // Only this newly executed command batch contributes to its response; earlier recovery is separate.
        // 仅此新执行命令批次计入其响应；先前恢复独立处理。
        let deleted_vector_rows = self
            .persist_prepared_storage_transition(
                batch.identity.operation_id.as_str(),
                mutation_kind.as_str(),
                &before,
                &after,
                &outcome,
            )
            .await
            .map_err(storage_uncertain_error)?;
        self.deleted_vector_rows = self
            .deleted_vector_rows
            .checked_add(deleted_vector_rows)
            .ok_or_else(|| {
                storage_uncertain_error(String::from("native VMM vector deletion count overflow"))
            })?;
        // staged was validated by the semantic storage mutation before the durable transition began.
        // staged 在耐久状态变换开始前已由语义存储变更完成校验。
        drop(staged);
        Ok(HostEffectResult::StorageMutation(outcome))
    }

    /// Upserts every vector referenced by the staged authoritative snapshot.
    /// 写入暂存权威快照引用的每个向量。
    async fn persist_vectors(&self, snapshot: &StorageSnapshot) -> Result<(), CoreError> {
        let grouped = vector_records(snapshot)?;
        for ((space_id, dimension), records) in grouped {
            let fingerprint_json = self.vector_space_fingerprint(space_id.as_str()).await?;
            let binding = self
                .data_plane
                .ensure_vector_space(space_id.as_str(), dimension, fingerprint_json.as_str())
                .await
                .map_err(storage_backend_error)?;
            self.data_plane
                .upsert_vectors(&binding, &records)
                .await
                .map_err(storage_backend_error)?;
        }
        Ok(())
    }

    /// Durably binds every vector-bearing logical space before a recoverable mutation is prepared.
    /// 在准备可恢复变更前，为每个包含向量的逻辑空间建立耐久绑定。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot` is the exact validated after-state whose vectors may be persisted during recovery.
    /// `snapshot` 是准确且已校验的操作后状态，其向量可能在恢复期间持久化。
    ///
    /// # Returns
    /// 返回值
    /// Success after every required vector-space binding is durable.
    /// 每个必需向量空间绑定都已耐久化后返回成功。
    ///
    /// # Errors
    /// Returns an exact fingerprint, schema, dimension, or controller error.
    /// 返回准确的指纹、结构、维度或控制器错误。
    async fn ensure_vector_spaces(&self, snapshot: &StorageSnapshot) -> Result<(), CoreError> {
        let grouped = vector_records(snapshot)?;
        for ((space_id, dimension), _) in grouped {
            let fingerprint_json = self.vector_space_fingerprint(space_id.as_str()).await?;
            self.data_plane
                .ensure_vector_space(space_id.as_str(), dimension, fingerprint_json.as_str())
                .await
                .map_err(storage_backend_error)?;
        }
        Ok(())
    }

    /// Removes every vector no longer referenced by a journaled maintenance snapshot.
    /// 删除账本化维护快照不再引用的每个向量。
    ///
    /// # Parameters
    /// 参数
    /// `before` is the exact full snapshot before maintenance.
    /// `before` 是维护前的准确完整快照。
    /// `after` is the exact full snapshot after maintenance.
    /// `after` 是维护后的准确完整快照。
    ///
    /// # Returns
    /// 返回值
    /// Actual physical deleted rows after every stale vector deletion completes.
    /// 全部陈旧向量删除完成后返回实际物理删除行数。
    ///
    /// # Errors
    /// Returns the exact vector-space resolution or deletion error for durable retry.
    /// 返回准确的向量空间解析或删除错误，以便耐久重试。
    async fn delete_stale_vectors_strict(
        &self,
        before: &StorageSnapshot,
        after: &StorageSnapshot,
    ) -> Result<u64, String> {
        let before = snapshot_vector_keys(before);
        let after = snapshot_vector_keys(after);
        let mut by_space = BTreeMap::<String, Vec<String>>::new();
        for (space_id, memory_id) in before.difference(&after) {
            by_space
                .entry(space_id.clone())
                .or_default()
                .push(memory_id.clone());
        }
        // Sum per-space controller receipts without assuming every requested row still exists.
        // 累加各空间控制器回执，不假定每个请求记录仍然存在。
        let mut deleted_rows = 0_u64;
        for (space_id, memory_ids) in by_space {
            let binding = self
                .data_plane
                .resolve_vector_space(&space_id)
                .await?
                .ok_or_else(|| format!("Native VMM vector space `{space_id}` does not exist"))?;
            // The physical data plane reports the exact row total for this space.
            // 物理数据面报告此空间的准确行数总计。
            let space_deleted_rows = self
                .data_plane
                .delete_vectors(&binding, &memory_ids)
                .await?;
            deleted_rows = deleted_rows
                .checked_add(space_deleted_rows)
                .ok_or_else(|| {
                    String::from("The native VMM physical vector deletion count overflowed")
                })?;
        }
        Ok(deleted_rows)
    }

    /// Locks one logical vector space to the exact physical Host AI fingerprint observed at execution.
    /// 将一个逻辑向量空间锁定到执行时观测到的精确宿主 AI 物理指纹。
    fn remember_embedding_fingerprint(
        &self,
        vector_space_id: &str,
        fingerprint: &EmbeddingSpaceFingerprint,
    ) -> Result<(), CoreError> {
        let encoded = serde_json::to_string(fingerprint).map_err(|error| {
            core_effect_error(format!("failed to encode embedding fingerprint: {error}"))
        })?;
        let mut fingerprints = self
            .embedding_fingerprints
            .write()
            .map_err(|_| storage_lock_error("write embedding fingerprint"))?;
        if let Some(existing) = fingerprints.get(vector_space_id) {
            if existing != &encoded {
                return Err(core_effect_error(format!(
                    "native VMM logical vector space `{vector_space_id}` received incompatible physical embedding fingerprints"
                )));
            }
        } else {
            fingerprints.insert(vector_space_id.to_owned(), encoded);
        }
        Ok(())
    }

    /// Resolves the exact physical fingerprint for a vector write without inventing provider facts.
    /// 在不猜测 Provider 事实的前提下解析向量写入使用的精确物理指纹。
    async fn vector_space_fingerprint(&self, vector_space_id: &str) -> Result<String, CoreError> {
        if let Some(fingerprint) = self
            .embedding_fingerprints
            .read()
            .map_err(|_| storage_lock_error("read embedding fingerprint"))?
            .get(vector_space_id)
            .cloned()
        {
            return Ok(fingerprint);
        }
        self.data_plane
            .resolve_vector_space(vector_space_id)
            .await
            .map_err(storage_backend_error)?
            .map(|binding| binding.physical_fingerprint_json)
            .ok_or_else(|| {
                storage_backend_error(format!(
                    "native VMM vector space `{vector_space_id}` has no observed physical embedding fingerprint"
                ))
            })
    }
}

/// Borrows the shared vector-bearing fields of either supported storage query shape.
/// 借用两种受支持存储查询形状中的共享向量字段。
struct VectorQueryView<'a> {
    /// Authenticated semantic subject identity.
    /// 已认证语义主体标识。
    subject_id: &'a str,
    /// Explicit read-visible scope identities.
    /// 显式可读作用域标识。
    visible_scope_ids: Vec<String>,
    /// Optional stricter candidate scopes.
    /// 可选的更严格候选作用域。
    candidate_scope_ids: Option<Vec<String>>,
    /// Optional exact origin-session restriction.
    /// 可选的精确来源会话限制。
    origin_session_source_id: Option<&'a str>,
    /// Ordered query vectors.
    /// 有序查询向量。
    query_embeddings: &'a [Vec<f32>],
    /// Logical vector-space identity.
    /// 逻辑向量空间标识。
    vector_space_id: &'a str,
    /// Core-required physical score contract.
    /// Core 要求的物理评分契约。
    expected_contract: &'a vulcan_memorymesh_core::ScoreContractRef,
}

impl<'a> VectorQueryView<'a> {
    /// Extracts vector fields without accepting a non-vector storage query.
    /// 提取向量字段，且不接受非向量存储查询。
    ///
    /// # Errors
    /// Returns a typed effect error for an unsupported query shape.
    /// 对不受支持的查询形状返回强类型 Effect 错误。
    fn from_query(query: &'a StorageQuery) -> Result<Self, CoreError> {
        let (subject_id, access, boundary, embeddings, vector_space_id, contract) = match query {
            StorageQuery::CandidateObservation(value) => (
                value.subject_id.as_str(),
                &value.scope_access,
                &value.boundary,
                std::slice::from_ref(&value.query_embedding),
                value.vector_space_id.as_str(),
                &value.vector_contract,
            ),
            StorageQuery::VectorCandidates {
                subject_id,
                scope_access,
                query_embeddings,
                vector_space_id,
                boundary,
                expected_contract,
                ..
            } => (
                subject_id.as_str(),
                scope_access,
                boundary,
                query_embeddings.as_slice(),
                vector_space_id.as_str(),
                expected_contract,
            ),
            _ => {
                return Err(core_effect_error(
                    "external vector query requires a vector-bearing storage request",
                ));
            }
        };
        Ok(Self {
            subject_id,
            visible_scope_ids: access
                .visible_scopes
                .iter()
                .map(|scope| scope.as_str().to_owned())
                .collect(),
            candidate_scope_ids: boundary.candidate_scope.as_ref().map(|scope| {
                scope
                    .scope_ids
                    .iter()
                    .map(|value| value.as_str().to_owned())
                    .collect()
            }),
            origin_session_source_id: boundary
                .candidate_scope
                .as_ref()
                .and_then(|scope| scope.origin_session_source_id.as_ref())
                .map(|value| value.as_str()),
            query_embeddings: embeddings,
            vector_space_id,
            expected_contract: contract,
        })
    }
}

/// Computes exact permanent turn losses across the complete active and recoverable snapshots.
/// 对照完整活动及可恢复快照，计算准确的永久回合消失集合。
/// Takes validated before/after snapshots; returns unique stable IDs, excluding every moved turn.
/// 接收校验后的前后快照；返回唯一稳定 ID，排除所有仅移动的回合。
fn permanently_removed_turn_ids(before: &StorageSnapshot, after: &StorageSnapshot) -> Vec<String> {
    // Both storage domains reserve the same canonical turn identity through recycle and restore.
    // 回收和恢复期间，两类存储域均保留同一规范回合身份。
    let retained = after
        .dataset
        .turns
        .iter()
        .chain(
            after
                .retention_trash_batches
                .iter()
                .flat_map(|batch| &batch.dataset.turns),
        )
        .map(|turn| turn.id.as_str())
        .collect::<BTreeSet<_>>();
    // The before set proves actual ownership; pre-existing missing jobs are not silently discarded.
    // 变更前集合证明实际归属；原已缺失的作业不能被静默丢弃。
    before
        .dataset
        .turns
        .iter()
        .chain(
            before
                .retention_trash_batches
                .iter()
                .flat_map(|batch| &batch.dataset.turns),
        )
        .map(|turn| turn.id.as_str())
        .filter(|id| !retained.contains(id))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// Extracts the pending turn binding from an intake Core mutation journal entry.
/// 从登记 Core 变更账本记录中提取待处理回合绑定。
///
/// # Parameters
/// 参数
/// `mutation` is the exact prepared or committed native Core storage transition.
/// `mutation` 是准确的已准备或已提交原生 Core 存储变换。
///
/// # Returns
/// 返回值
/// The pending turn identity for intake mutations, otherwise no value.
/// 登记变更返回待处理回合身份，其他变更返回空值。
///
/// # Errors
/// Returns an error when a Core storage result cannot be decoded or contains duplicate bindings.
/// Core 存储结果无法解码或包含重复绑定时返回错误。
fn pending_turn_id_from_core_mutation(
    mutation: &NativeVmmCoreMutation,
) -> Result<Option<String>, String> {
    if !mutation.mutation_kind.starts_with("core_storage_mutation:") {
        return Ok(None);
    }
    let outcome: MutationOutcome =
        serde_json::from_str(&mutation.result_json).map_err(|error| {
            format!("Failed to parse the native VMM Core storage mutation result: {error}")
        })?;
    let MutationOutcome::Committed { receipt, .. } = outcome else {
        return Ok(None);
    };
    let bindings = receipt
        .entity_bindings
        .into_iter()
        .filter(|binding| binding.entity_slot_id.as_str() == "pending-turn")
        .collect::<Vec<_>>();
    if bindings
        .iter()
        .any(|binding| binding.entity_kind != vulcan_memorymesh_core::EntityKind::Turn)
    {
        return Err(String::from(
            "The pending turn binding kind of the native VMM intake mutation is invalid",
        ));
    }
    match bindings.as_slice() {
        [] => Ok(None),
        [binding] => Ok(Some(binding.domain_id.clone())),
        _ => Err(String::from(
            "The native VMM intake mutation contains multiple pending turn bindings",
        )),
    }
}

/// Hydrates the relational snapshot and every exact LanceDB vector referenced by it.
/// 恢复关系快照及其引用的每个精确 LanceDB 向量。
///
/// # Parameters
/// 参数
/// `data_plane` provides the authoritative SQLite snapshot and LanceDB vector bindings.
/// `data_plane` 提供权威 SQLite 快照与 LanceDB 向量绑定。
///
/// # Returns
/// 返回值
/// A validated in-memory Core store with every referenced vector restored.
/// 返回已恢复每个引用向量且完成校验的 Core 内存存储。
///
/// # Errors
/// Returns an error for missing vector spaces, vector rows, or malformed persisted state.
/// 向量空间、向量记录缺失或持久状态异常时返回错误。
async fn hydrate_storage(data_plane: &VmmDataPlaneHandle) -> Result<InMemoryStorage, String> {
    let Some(encoded) = data_plane.load_core_snapshot_json().await? else {
        return Ok(InMemoryStorage::default());
    };
    let mut snapshot: StorageSnapshot = serde_json::from_str(&encoded)
        .map_err(|error| format!("Failed to parse the native VMM Core snapshot: {error}"))?;
    let mut by_space = BTreeMap::<String, Vec<String>>::new();
    for memory in snapshot.dataset.memories.iter().chain(
        snapshot
            .retention_trash_batches
            .iter()
            .flat_map(|batch| batch.dataset.memories.iter()),
    ) {
        if let Some(space_id) = memory.vector_space_id.as_ref() {
            by_space
                .entry(space_id.as_str().to_owned())
                .or_default()
                .push(memory.id.as_str().to_owned());
        }
    }
    let mut vectors = BTreeMap::<String, Vec<f32>>::new();
    for (space_id, memory_ids) in by_space {
        let binding = data_plane
            .resolve_vector_space(&space_id)
            .await?
            .ok_or_else(|| format!("Native VMM vector space `{space_id}` does not exist"))?;
        for candidate in data_plane.load_vectors(&binding, &memory_ids).await? {
            vectors.insert(candidate.memory_id, candidate.vector);
        }
    }
    for memory in &mut snapshot.dataset.memories {
        if memory.vector_space_id.is_some() {
            memory.embedding = Some(vectors.remove(memory.id.as_str()).ok_or_else(|| {
                format!(
                    "Native VMM memory `{}` is missing its vector",
                    memory.id.as_str()
                )
            })?);
        }
    }
    for batch in &mut snapshot.retention_trash_batches {
        for memory in &mut batch.dataset.memories {
            if memory.vector_space_id.is_some() {
                memory.embedding = Some(vectors.remove(memory.id.as_str()).ok_or_else(|| {
                    format!(
                        "Native VMM recycle-bin memory `{}` is missing its quarantined vector",
                        memory.id.as_str()
                    )
                })?);
            }
        }
    }
    if !vectors.is_empty() {
        return Err(String::from(
            "The native VMM vector backfill returned unreferenced records",
        ));
    }
    InMemoryStorage::from_snapshot(snapshot)
}

/// Declares the complete native VMM capability without requiring optional roles to be configured.
/// 声明完整原生 VMM 能力，同时不要求可选角色已经配置。
fn native_vmm_capability_registration() -> HostAiCapabilityRegistration {
    let mut requirements = [
        "precheck_l1",
        "precheck_l2",
        "postaction_l1",
        "postaction_l2",
    ]
    .into_iter()
    .map(|purpose_id| HostAiCapabilityRequirement {
        operation: AiOperation::ConversationRespond,
        purpose_id: Some(purpose_id.to_owned()),
        required: true,
    })
    .collect::<Vec<_>>();
    requirements.push(HostAiCapabilityRequirement {
        operation: AiOperation::ConversationRespond,
        purpose_id: Some(String::from("profile_instruction")),
        required: false,
    });
    requirements.push(HostAiCapabilityRequirement {
        operation: AiOperation::EmbeddingCreate,
        purpose_id: None,
        required: true,
    });
    requirements.push(HostAiCapabilityRequirement {
        operation: AiOperation::RerankDocuments,
        purpose_id: None,
        required: false,
    });
    HostAiCapabilityRegistration {
        registration_id: String::from(NATIVE_VMM_REGISTRATION_ID),
        owner_id: String::from(NATIVE_VMM_OWNER_ID),
        consumer_profile_id: String::from(NATIVE_VMM_CONSUMER_PROFILE_ID),
        caller: InferenceCallerContext {
            caller_id: String::from("vulcan-code-native-vmm"),
            allowed_consumer_profile_ids: BTreeSet::from([String::from(
                NATIVE_VMM_CONSUMER_PROFILE_ID,
            )]),
            permissions: BTreeSet::from([
                InferencePermission::Status,
                InferencePermission::Llm,
                InferencePermission::Embedding,
                InferencePermission::Rerank,
                InferencePermission::Cancel,
            ]),
            usage_scope: InferenceUsageScope::InternalBackground,
            // Preserve the existing VMM background scheduling until its foreground split is explicitly integrated.
            // 在明确接入前台分流之前保留 VMM 现有后台调度。
            scheduling_class: vulcan_ai_contract::InferenceSchedulingClass::ByUsageScope,
            interactive_session_id: None,
            deadline_unix_ms: None,
            budget: InferenceCallerBudget {
                max_concurrent_executions: 4,
                max_input_units_per_request: Some(262_144),
                max_output_tokens_per_request: Some(NATIVE_VMM_LLM_OUTPUT_TOKENS),
                max_requests_per_minute: None,
                max_input_units_per_minute: None,
                max_execution_duration_ms: Some(600_000),
            },
        },
        requirements,
    }
}

/// Revalidates required VMM operations after a capability snapshot refresh.
/// 在能力快照刷新后重新校验 VMM 必需操作。
///
/// # Parameters
/// 参数
/// `handle` is the refreshed host-issued capability handle.
/// `handle` 是刷新后的宿主签发能力句柄。
///
/// # Returns
/// 返回值
/// Success only when every required operation remains explicitly available.
/// 仅当每项必需操作仍被明确标记为可用时返回成功。
///
/// # Errors
/// Returns a capability-unavailable error naming the exact missing operation and purpose.
/// 返回能力不可用错误，并标明准确缺失的操作与用途。
fn validate_native_vmm_required_capabilities(
    handle: &HostAiCapabilityHandle,
) -> Result<(), HostAiError> {
    for requirement in native_vmm_capability_registration()
        .requirements
        .into_iter()
        .filter(|requirement| requirement.required)
    {
        if !handle.supports(requirement.operation, requirement.purpose_id.as_deref()) {
            return Err(HostAiError::new(
                HostAiErrorCode::CapabilityUnavailable,
                format!(
                    "Required native VMM Host AI operation {:?} for purpose {:?} is unavailable.",
                    requirement.operation, requirement.purpose_id
                ),
            ));
        }
    }
    Ok(())
}

/// Resolves the exact declared project profile section of a profile-producing command.
/// 解析生成画像的命令所明确声明的项目画像分区。
/// Returns no override for personal-only or non-profile commands; rejects invalid native project identities.
/// 个人专属或非画像命令不返回覆盖；拒绝无效原生项目身份。
fn profile_policy_project_id(command: &CoreCommand) -> Result<Option<u64>, CoreError> {
    use vulcan_memorymesh_core::SectionRole;
    // Core assigns generated project nodes by the Environment section, not by visible read scopes.
    // Core 按 Environment 分区分配生成的项目节点，而非按可见读取范围分配。
    let section = match command {
        CoreCommand::PostActionAnalyzePendingTurn(command) => command
            .profile_sections
            .iter()
            .find(|section| section.role == SectionRole::Environment),
        CoreCommand::ProfileInstruction(command) => match command.target_section.role {
            SectionRole::Environment => Some(&command.target_section),
            SectionRole::Subject => None,
        },
        _ => None,
    };
    let Some(section) = section else {
        return Ok(None);
    };
    let project_id = section
        .scope_id
        .as_str()
        .strip_prefix("vmm-project:")
        .ok_or_else(|| {
            native_resource_core_error(
                "Native project profile scope must use vmm-project:<id>.".to_owned(),
            )
        })?
        .parse::<u64>()
        .map_err(|error| {
            native_resource_core_error(format!("Invalid native project profile ID: {error}"))
        })?;
    if project_id == 0 {
        return Err(native_resource_core_error(
            "Native project profile ID must be positive.".to_owned(),
        ));
    }
    Ok(Some(project_id))
}

/// Reports whether one command needs the embedding-route-derived Core snapshot before start.
/// 报告一个命令在启动前是否需要由向量路由派生的 Core 快照。
fn command_requires_ai_configuration(command: &CoreCommand) -> bool {
    match command {
        CoreCommand::PreCheck(_)
        | CoreCommand::PostActionIntake(_)
        | CoreCommand::PostActionAnalyzePendingTurn(_)
        | CoreCommand::MemorySearch(_)
        | CoreCommand::MemoryWrite(_)
        | CoreCommand::ProfileInstruction(_)
        | CoreCommand::MemorySearchBatch(_)
        | CoreCommand::MemoryWriteBatch(_) => true,
        CoreCommand::Maintenance(command) => matches!(
            command.action,
            vulcan_memorymesh_core::MaintenanceAction::RebuildVectors { .. }
        ),
        CoreCommand::ChatCompact(_)
        | CoreCommand::MemoryDetails(_)
        | CoreCommand::MemoryDelete(_)
        | CoreCommand::ProfileQuery(_)
        | CoreCommand::Retention(_)
        | CoreCommand::MemoryDetailsBatch(_)
        | CoreCommand::MemoryDeleteBatch(_) => false,
    }
}

/// Rejects a vector-bearing command when its caller-observed generation differs from the freshly activated route.
/// 当携带向量的命令所观察代次与刚激活的路由不一致时拒绝该命令。
///
/// # Parameters
/// 参数
/// `command` is the immutable semantic request and `current` is derived after the final Host AI activation.
/// `command` 是不可变语义请求，`current` 在最终宿主 AI 激活后派生。
///
/// # Errors
/// 错误
/// Returns before any Core effect when a concurrent provider change made the request stale.
/// 当并发供应商变更使请求过期时，在任何 Core Effect 之前返回错误。
fn validate_command_vector_generation(
    command: &CoreCommand,
    current: &VectorSpaceId,
) -> Result<(), CoreError> {
    let requested = match command {
        CoreCommand::PreCheck(command) => Some(&command.vector_space_id),
        CoreCommand::PostActionIntake(command) => Some(&command.vector_space_id),
        CoreCommand::PostActionAnalyzePendingTurn(command) => Some(&command.vector_space_id),
        CoreCommand::MemorySearch(command) => Some(&command.vector_space_id),
        CoreCommand::MemoryWrite(command) => Some(&command.vector_space_id),
        CoreCommand::MemorySearchBatch(command) => Some(&command.vector_space_id),
        CoreCommand::MemoryWriteBatch(command) => Some(&command.vector_space_id),
        CoreCommand::Maintenance(command) => match &command.action {
            vulcan_memorymesh_core::MaintenanceAction::RebuildVectors {
                vector_space_id, ..
            } => Some(vector_space_id),
            _ => None,
        },
        CoreCommand::ProfileInstruction(_)
        | CoreCommand::ChatCompact(_)
        | CoreCommand::MemoryDetails(_)
        | CoreCommand::MemoryDelete(_)
        | CoreCommand::ProfileQuery(_)
        | CoreCommand::Retention(_)
        | CoreCommand::MemoryDetailsBatch(_)
        | CoreCommand::MemoryDeleteBatch(_) => None,
    };
    if requested.is_none_or(|requested| requested == current) {
        return Ok(());
    }
    Err(CoreError::new(
        ErrorCode::InvalidConfiguration,
        ErrorStage::Configuration,
        "native VMM vector generation changed after request preparation; retry with the current embedding route",
    ))
}

/// Borrows the mutation identity of a command already proven to be mutating.
/// 借用一个已确认会产生变更的命令的变更身份。
fn command_mutation_identity(
    command: &mut CoreCommand,
) -> &mut vulcan_memorymesh_core::MutationIdentity {
    match command {
        CoreCommand::PreCheck(command) => &mut command.mutation_identity,
        CoreCommand::PostActionIntake(command) => &mut command.mutation_identity,
        CoreCommand::PostActionAnalyzePendingTurn(command) => &mut command.mutation_identity,
        CoreCommand::ChatCompact(command) => &mut command.mutation_identity,
        CoreCommand::MemoryWrite(command) => &mut command.mutation_identity,
        CoreCommand::MemoryDelete(command) => &mut command.mutation_identity,
        CoreCommand::ProfileInstruction(command) => &mut command.mutation_identity,
        CoreCommand::Retention(command) => &mut command.mutation_identity,
        CoreCommand::MemoryWriteBatch(command) => &mut command.mutation_identity,
        CoreCommand::MemoryDeleteBatch(command) => &mut command.mutation_identity,
        CoreCommand::Maintenance(command) => &mut command.mutation_identity,
        CoreCommand::MemorySearch(_)
        | CoreCommand::MemoryDetails(_)
        | CoreCommand::ProfileQuery(_)
        | CoreCommand::MemorySearchBatch(_)
        | CoreCommand::MemoryDetailsBatch(_) => {
            unreachable!("is_mutating excluded non-mutating Core commands")
        }
    }
}

/// Selects one deterministic supported embedding dimension and opaque Core fingerprint.
/// 选择一个确定且受支持的向量维度及不透明 Core 指纹。
fn embedding_config_from_capability(
    handle: &HostAiCapabilityHandle,
) -> Result<(u32, String), CoreError> {
    let operation = handle
        .snapshot
        .operations
        .iter()
        .find(|operation| {
            operation.operation == AiOperation::EmbeddingCreate
                && operation.purpose_id.is_none()
                && operation.available
        })
        .ok_or_else(|| {
            CoreError::new(
                ErrorCode::MissingResource,
                ErrorStage::Configuration,
                "native VMM embedding capability is unavailable",
            )
        })?;
    let capabilities = operation.embedding.as_ref().ok_or_else(|| {
        CoreError::new(
            ErrorCode::InvalidConfiguration,
            ErrorStage::Configuration,
            "native VMM embedding capability omitted its typed limits",
        )
    })?;
    if !capabilities.tasks.contains(&NATIVE_VMM_EMBEDDING_TASK) {
        return Err(CoreError::new(
            ErrorCode::InvalidConfiguration,
            ErrorStage::Configuration,
            "native VMM embedding route does not support the required provider-default task",
        ));
    }
    if !capabilities
        .output_kinds
        .contains(&EmbeddingOutputKind::Dense)
    {
        return Err(CoreError::new(
            ErrorCode::InvalidConfiguration,
            ErrorStage::Configuration,
            "native VMM embedding route does not support dense output",
        ));
    }
    if !capabilities.encodings.contains(&EmbeddingEncoding::Float) {
        return Err(CoreError::new(
            ErrorCode::InvalidConfiguration,
            ErrorStage::Configuration,
            "native VMM embedding route does not support float encoding",
        ));
    }
    let dimension = capabilities
        .default_dimensions
        .or_else(|| {
            (capabilities.dimensions.len() == 1)
                .then(|| capabilities.dimensions.iter().next().copied())
                .flatten()
        })
        .ok_or_else(|| {
            CoreError::new(
                ErrorCode::InvalidConfiguration,
                ErrorStage::Configuration,
                "native VMM embedding route does not declare a deterministic default dimension",
            )
        })?;
    if dimension == 0
        || (!capabilities.dimensions.is_empty() && !capabilities.dimensions.contains(&dimension))
        || capabilities
            .minimum_dimensions
            .is_some_and(|minimum| dimension < minimum)
        || capabilities
            .maximum_dimensions
            .is_some_and(|maximum| dimension > maximum)
    {
        return Err(CoreError::new(
            ErrorCode::InvalidConfiguration,
            ErrorStage::Configuration,
            "native VMM embedding default dimension violates the registered capability",
        ));
    }
    let material = serde_json::to_vec(operation).map_err(|error| {
        CoreError::new(
            ErrorCode::InvalidConfiguration,
            ErrorStage::Configuration,
            format!("failed to encode native VMM embedding capability: {error}"),
        )
    })?;
    Ok((
        dimension,
        format!(
            "host-ai-capability-fp256:{}",
            hex::encode(Sha256::digest(material))
        ),
    ))
}

/// Derives one collision-resistant logical vector generation from the exact embedding capability.
/// 根据准确 Embedding 能力派生一个抗碰撞的逻辑向量代次。
///
/// # Parameters
/// 参数
/// `capability_fingerprint` is the opaque route-bearing capability digest and `dimension` is its
/// exact dense-vector width.
/// `capability_fingerprint` 是携带路由的不透明能力摘要，`dimension` 是其准确稠密向量宽度。
///
/// # Returns
/// 返回值
/// A stable Core vector-space identity that changes only when vector compatibility changes.
/// 返回仅在向量兼容性变化时才改变的稳定 Core 向量空间标识。
///
/// # Errors
/// 错误
/// Returns a Core identifier error if the closed generated identity violates Core bounds.
/// 当封闭生成的标识违反 Core 边界时返回 Core 标识错误。
fn native_vector_space_id(
    capability_fingerprint: &str,
    dimension: u32,
) -> Result<VectorSpaceId, vulcan_memorymesh_core::IdentifierError> {
    let mut hasher = Sha256::new();
    hasher.update(NATIVE_VMM_VECTOR_SPACE_NAMESPACE.as_bytes());
    hasher.update([0]);
    hasher.update(capability_fingerprint.as_bytes());
    hasher.update([0]);
    hasher.update(dimension.to_le_bytes());
    VectorSpaceId::new(format!(
        "{NATIVE_VMM_VECTOR_SPACE_NAMESPACE}:{}",
        hex::encode(hasher.finalize())
    ))
}

/// Derives one complete native vector descriptor from the exact registered embedding capability.
/// 从准确注册的 Embedding 能力派生一份完整原生向量描述符。
///
/// # Parameters
/// 参数
/// `handle` is the freshly validated Host AI capability snapshot.
/// `handle` 是刚完成校验的宿主 AI 能力快照。
///
/// # Returns
/// 返回值
/// The route-derived logical generation, dense width, and opaque capability fingerprint.
/// 返回由路由派生的逻辑代次、稠密维度及不透明能力指纹。
///
/// # Errors
/// 错误
/// Returns when the embedding capability cannot select one exact dimension or valid identity.
/// Embedding 能力无法选择一个准确维度或有效身份时返回错误。
fn vector_space_descriptor_from_capability(
    handle: &HostAiCapabilityHandle,
) -> Result<NativeVmmVectorSpaceDescriptor, CoreError> {
    let (embedding_dimension, capability_fingerprint) = embedding_config_from_capability(handle)?;
    let vector_space_id =
        native_vector_space_id(capability_fingerprint.as_str(), embedding_dimension).map_err(
            |error| {
                CoreError::new(
                    ErrorCode::InvalidConfiguration,
                    ErrorStage::Configuration,
                    error.to_string(),
                )
            },
        )?;
    Ok(NativeVmmVectorSpaceDescriptor {
        vector_space_id,
        embedding_dimension,
        capability_fingerprint,
    })
}

/// Converts one Host AI registration failure into a stable Core configuration error.
/// 将一个宿主 AI 注册失败转换为稳定的 Core 配置错误。
fn host_ai_core_error(error: HostAiError) -> CoreError {
    CoreError::new(
        ErrorCode::MissingResource,
        ErrorStage::Configuration,
        error.message,
    )
    .with_retryable(error.retryable)
}

/// Builds correlation and a stage-specific deadline for one Core effect.
/// 为一个 Core Effect 构造关联信息与阶段专属截止时间。
fn effect_call_context(
    request: &EffectRequest,
    cancellation: Arc<NativeVmmHostAiCancellation>,
) -> HostAiCallContext {
    let operation_id = request
        .operation_id
        .as_ref()
        .map_or_else(|| request.effect_id.as_str(), |value| value.as_str());
    HostAiCallContext::new(operation_id.to_owned())
        .with_core_correlation(
            Some(request.effect_id.as_str().to_owned()),
            Some(request.command_id.as_str().to_owned()),
        )
        .with_deadline(effect_deadline(request.deadline_class))
        .with_cancellation(cancellation)
}

/// Computes the absolute deadline assigned to one Core effect class.
/// 计算分配给一个 Core Effect 类别的绝对截止时间。
fn effect_deadline(class: EffectDeadlineClass) -> Option<u64> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())?;
    let budget = match class {
        EffectDeadlineClass::Interactive => 90_000,
        EffectDeadlineClass::Background | EffectDeadlineClass::Maintenance => 600_000,
    };
    Some(now.saturating_add(budget))
}

/// Maps one Core LLM purpose to its configured VMM purpose identity.
/// 将一个 Core LLM 目的映射到已配置 VMM 目的标识。
const fn llm_purpose_id(purpose: LlmPurpose) -> &'static str {
    match purpose {
        LlmPurpose::PreCheckIntent => "precheck_l1",
        LlmPurpose::PreCheckReview => "precheck_l2",
        LlmPurpose::PostActionAnalysis => "postaction_l1",
        LlmPurpose::PostActionReview => "postaction_l2",
        LlmPurpose::ProfileInstruction => "profile_instruction",
    }
}

/// Returns the stable JSON Schema name for one Core output contract.
/// 返回一个 Core 输出契约的稳定 JSON Schema 名称。
const fn structured_output_name(
    contract: vulcan_memorymesh_core::StructuredOutputContractId,
) -> &'static str {
    use vulcan_memorymesh_core::StructuredOutputContractId;
    match contract {
        StructuredOutputContractId::PreCheckIntent => "vmm_precheck_intent",
        StructuredOutputContractId::PreCheckReview => "vmm_precheck_review",
        StructuredOutputContractId::PostActionAnalysis => "vmm_postaction_analysis",
        StructuredOutputContractId::PostActionReview => "vmm_postaction_review",
        StructuredOutputContractId::ProfileInstruction => "vmm_profile_instruction",
    }
}

/// Converts a host AI error into the explicit degradable failure understood by Core.
/// 将宿主 AI 错误转换为 Core 理解的显式可降级失败。
fn ai_failure(kind: EffectFailureKind, error: HostAiError) -> HostEffectResult {
    let class = match error.code {
        HostAiErrorCode::Contract
        | HostAiErrorCode::InvalidRequest
        | HostAiErrorCode::DimensionMismatch
        | HostAiErrorCode::FingerprintMismatch => FailureClass::StructuredContract,
        HostAiErrorCode::Provider => FailureClass::Provider,
        _ => FailureClass::Transport,
    };
    HostEffectResult::Failure(EffectFailure {
        kind,
        class,
        retryable: error.retryable,
        allow_degrade: true,
        diagnostic: error.message.chars().take(512).collect(),
    })
}

/// Creates a non-dispatched Host AI internal error for local synchronization failures.
/// 为本地同步失败创建一个未分派的宿主 AI 内部错误。
fn host_ai_internal_error(message: &str) -> HostAiError {
    HostAiError::new(HostAiErrorCode::Internal, message)
}

/// Maps one stable provider omission code to the Core omission category.
/// 将一个稳定 Provider 省略代码映射到 Core 省略类别。
fn embedding_drop_reason(reason_code: &str) -> Result<EmbeddingDropReason, CoreError> {
    match reason_code {
        "input_too_large" => Ok(EmbeddingDropReason::InputTooLarge),
        "provider_confirmed_invalid_input" => {
            Ok(EmbeddingDropReason::ProviderConfirmedInvalidInput)
        }
        _ => Err(core_effect_error(
            "native VMM embedding response contains an unknown drop reason",
        )),
    }
}

/// Builds dimension-grouped LanceDB records from the complete staged snapshot.
/// 从完整暂存快照构造按维度分组的 LanceDB 记录。
fn vector_records(
    snapshot: &StorageSnapshot,
) -> Result<BTreeMap<(String, u32), Vec<VmmVectorRecord>>, CoreError> {
    let mut grouped = BTreeMap::<(String, u32), Vec<VmmVectorRecord>>::new();
    let mut dimensions = BTreeMap::<String, u32>::new();
    for memory in snapshot.dataset.memories.iter().chain(
        snapshot
            .retention_trash_batches
            .iter()
            .flat_map(|batch| batch.dataset.memories.iter()),
    ) {
        let (Some(space_id), Some(embedding)) =
            (memory.vector_space_id.as_ref(), memory.embedding.as_ref())
        else {
            continue;
        };
        let dimension = u32::try_from(embedding.len()).map_err(|error| {
            storage_backend_error(format!("native VMM embedding dimension overflow: {error}"))
        })?;
        if dimension == 0
            || dimensions
                .insert(space_id.as_str().to_owned(), dimension)
                .is_some_and(|existing| existing != dimension)
        {
            return Err(storage_backend_error(
                "native VMM vector space contains inconsistent dimensions",
            ));
        }
        grouped
            .entry((space_id.as_str().to_owned(), dimension))
            .or_default()
            .push(VmmVectorRecord {
                memory_id: memory.id.as_str().to_owned(),
                access_class: match memory.access_class {
                    MemoryAccessClass::SubjectPrivate => 1,
                    MemoryAccessClass::ScopeShared => 2,
                },
                source_subject_id: memory
                    .source_subject_id
                    .as_ref()
                    .map(|value| value.as_str().to_owned()),
                scope_id: memory.scope_id.as_str().to_owned(),
                origin_session_source_id: memory
                    .origin_session_source_id
                    .as_ref()
                    .map(|value| value.as_str().to_owned()),
                vector: normalize_vector(embedding)?,
            });
    }
    Ok(grouped)
}

/// Returns every durable vector reference as an exact logical-space and memory tuple.
/// 将每个持久向量引用返回为精确逻辑空间与记忆元组。
fn snapshot_vector_keys(snapshot: &StorageSnapshot) -> BTreeSet<(String, String)> {
    snapshot
        .dataset
        .memories
        .iter()
        .chain(
            snapshot
                .retention_trash_batches
                .iter()
                .flat_map(|batch| batch.dataset.memories.iter()),
        )
        .filter_map(|memory| {
            memory.vector_space_id.as_ref().and_then(|space_id| {
                memory
                    .embedding
                    .as_ref()
                    .map(|_| (space_id.as_str().to_owned(), memory.id.as_str().to_owned()))
            })
        })
        .collect()
}

/// Retains one exact maximum cosine score for every candidate identity.
/// 为每个候选标识保留一个精确最大余弦分数。
fn retain_best_cosines(
    best: &mut BTreeMap<String, (f64, Vec<f32>)>,
    query: &[f32],
    candidates: Vec<VmmVectorCandidate>,
) -> Result<(), CoreError> {
    for candidate in candidates {
        let score = cosine_similarity(query, &candidate.vector)?;
        let current = best
            .entry(candidate.memory_id)
            .or_insert((score, candidate.vector));
        if score > current.0 {
            current.0 = score;
        }
    }
    Ok(())
}

/// Produces a finite unit vector without accepting zero-magnitude input.
/// 生成有限单位向量，且不接受零模长输入。
fn normalize_vector(vector: &[f32]) -> Result<Vec<f32>, CoreError> {
    let norm = vector
        .iter()
        .try_fold(0.0_f64, |sum, value| {
            value.is_finite().then_some(sum + f64::from(*value).powi(2))
        })
        .ok_or_else(|| storage_backend_error("native VMM vector contains a non-finite value"))?
        .sqrt();
    if norm == 0.0 || !norm.is_finite() {
        return Err(storage_backend_error(
            "native VMM vector has zero or invalid magnitude",
        ));
    }
    Ok(vector
        .iter()
        .map(|value| (f64::from(*value) / norm) as f32)
        .collect())
}

/// Computes exact cosine similarity in binary64 accumulation.
/// 使用 binary64 累加计算精确余弦相似度。
fn cosine_similarity(left: &[f32], right: &[f32]) -> Result<f64, CoreError> {
    if left.len() != right.len() || left.is_empty() {
        return Err(storage_backend_error(
            "native VMM cosine vectors have incompatible dimensions",
        ));
    }
    let (dot, left_norm, right_norm) = left
        .iter()
        .zip(right)
        .try_fold(
            (0.0_f64, 0.0_f64, 0.0_f64),
            |(dot, left_norm, right_norm), (left, right)| {
                if !left.is_finite() || !right.is_finite() {
                    return None;
                }
                let left = f64::from(*left);
                let right = f64::from(*right);
                Some((
                    left.mul_add(right, dot),
                    left.mul_add(left, left_norm),
                    right.mul_add(right, right_norm),
                ))
            },
        )
        .ok_or_else(|| {
            storage_backend_error("native VMM cosine vector contains non-finite data")
        })?;
    if left_norm == 0.0 || right_norm == 0.0 {
        return Err(storage_backend_error(
            "native VMM cosine vector has zero magnitude",
        ));
    }
    Ok((dot / (left_norm.sqrt() * right_norm.sqrt())).clamp(-1.0, 1.0))
}

/// Creates a stable local effect-contract error.
/// 创建一个稳定的本地 Effect 契约错误。
fn core_effect_error(message: impl Into<String>) -> CoreError {
    CoreError::new(
        ErrorCode::StructuredOutputContractViolation,
        ErrorStage::Effect,
        message,
    )
}

/// Creates a stable pre-commit storage error.
/// 创建一个稳定的提交前存储错误。
fn storage_backend_error(message: impl Into<String>) -> CoreError {
    CoreError::new(ErrorCode::Internal, ErrorStage::Storage, message)
}

/// Creates a stable lock error before persistent side effects begin.
/// 创建一个持久副作用开始前的稳定锁错误。
fn storage_lock_error(operation: &str) -> CoreError {
    storage_backend_error(format!("native VMM storage {operation} lock poisoned"))
}

/// Creates a storage error after vector or relational side effects may have occurred.
/// 创建一个可能已发生向量或关系副作用后的存储错误。
fn storage_uncertain_error(message: impl Into<String>) -> CoreError {
    let mut error = storage_backend_error(message);
    error.side_effect_possible = true;
    error
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_ai_contract::{
        DistanceMetric, EmbeddingCapabilities, EmbeddingNormalization, InferenceCapabilitySnapshot,
        InferenceOperationCapability,
    };

    /// Verifies the native Core never requests an unsupported inferred embedding role.
    /// 验证原生 Core 不会请求未经能力契约支持的推断型向量角色。
    #[test]
    fn native_vmm_embedding_task_is_provider_default() {
        assert_eq!(NATIVE_VMM_EMBEDDING_TASK, EmbeddingTask::ProviderDefault);
    }

    /// Verifies vector generations are stable for one route and change for either capability or dimension drift.
    /// 验证同一路由的向量代次保持稳定，并在能力或维度漂移时发生变化。
    #[test]
    fn native_vector_generation_tracks_exact_embedding_capability() {
        let first = native_vector_space_id("capability-a", 1_536).expect("first vector space");
        let repeated =
            native_vector_space_id("capability-a", 1_536).expect("repeated vector space");
        let changed_capability =
            native_vector_space_id("capability-b", 1_536).expect("changed capability space");
        let changed_dimension =
            native_vector_space_id("capability-a", 3_072).expect("changed dimension space");

        assert_eq!(first, repeated);
        assert_ne!(first, changed_capability);
        assert_ne!(first, changed_dimension);
        assert!(first.as_str().starts_with("vmm-native:memory:v1:"));
    }

    /// Verifies a provider change between request preparation and Core start fails before any effect can run.
    /// 验证请求准备与 Core 启动之间发生供应商变化时，会在任何 Effect 运行前失败。
    #[test]
    fn stale_command_vector_generation_is_rejected_before_dispatch() {
        let current = VectorSpaceId::new("current-space").expect("current vector space");
        let stale = VectorSpaceId::new("stale-space").expect("stale vector space");
        let command = CoreCommand::MemorySearch(vulcan_memorymesh_core::MemorySearchCommand {
            subject_id: vulcan_memorymesh_core::SubjectId::new("subject").expect("subject id"),
            scope_access: vulcan_memorymesh_core::ScopeAccess {
                write_scope: vulcan_memorymesh_core::ScopeId::new("scope").expect("scope id"),
                visible_scopes: vec![
                    vulcan_memorymesh_core::ScopeId::new("scope").expect("visible scope"),
                ],
            },
            query: String::from("query"),
            limit: 1,
            vector_space_id: stale,
            boundary: vulcan_memorymesh_core::CandidateQueryBoundary {
                candidate_scope: None,
                session_recall_boundary: None,
            },
            now_ns: 1,
        });

        let error = validate_command_vector_generation(&command, &current)
            .expect_err("stale vector generation must fail");
        assert_eq!(error.code, ErrorCode::InvalidConfiguration);

        let mut current_command = command;
        let CoreCommand::MemorySearch(search) = &mut current_command else {
            unreachable!("test command remains memory search");
        };
        search.vector_space_id = current.clone();
        validate_command_vector_generation(&current_command, &current)
            .expect("current vector generation is accepted");
    }

    /// Verifies dropping an unfinished native request wakes the exact Host AI cancellation signal.
    /// 验证丢弃未完成原生请求会唤醒准确的宿主 AI 取消信号。
    #[tokio::test]
    async fn native_vmm_run_guard_propagates_cancellation() {
        // cancellation is shared by the request guard and the simulated Host AI waiter.
        // cancellation 由请求守卫与模拟宿主 AI 等待方共享。
        let cancellation = Arc::new(NativeVmmHostAiCancellation::default());
        let observed = Arc::clone(&cancellation);
        // waiter proves the asynchronous cancellation edge wakes instead of relying on polling.
        // waiter 证明异步取消边沿会主动唤醒，而不是依赖轮询。
        let waiter = tokio::spawn(async move {
            observed.cancelled().await;
            observed.is_cancelled()
        });
        {
            // guard remains armed because the simulated command never reaches a terminal result.
            // guard 保持启用，因为模拟命令未到达终态结果。
            let _guard = NativeVmmRunCancellationGuard::new(cancellation);
        }
        assert!(waiter.await.expect("cancellation waiter should complete"));
    }

    /// Verifies a terminal native command does not publish a false cancellation event.
    /// 验证已到达终态的原生命令不会发布错误取消事件。
    #[test]
    fn native_vmm_run_guard_disarms_after_completion() {
        // cancellation remains inspectable after the guard is dropped.
        // cancellation 在守卫丢弃后仍可检查。
        let cancellation = Arc::new(NativeVmmHostAiCancellation::default());
        {
            // guard is explicitly disarmed at the same boundary used after joining the worker.
            // guard 在与 Worker 等待完成后相同的边界显式解除。
            let mut guard = NativeVmmRunCancellationGuard::new(Arc::clone(&cancellation));
            guard.disarm();
        }
        assert!(!cancellation.is_cancelled());
    }

    /// Verifies native VMM rejects a configured embedding route before runtime effects can fail.
    /// 验证原生 VMM 会在运行时 Effect 失败前拒绝不兼容的已配置向量路由。
    #[test]
    fn native_vmm_embedding_activation_requires_the_frozen_request_shape() {
        // compatibleHandle represents the exact dense-float provider-default route used in production.
        // compatibleHandle 表示生产环境使用的准确稠密浮点供应商默认路由。
        let compatible_handle =
            embedding_capability_handle(BTreeSet::from([EmbeddingTask::ProviderDefault]));
        let (dimension, fingerprint) = embedding_config_from_capability(&compatible_handle)
            .expect("provider-default dense float route should activate");
        assert_eq!(dimension, 1_536);
        assert!(fingerprint.starts_with("host-ai-capability-fp256:"));

        // incompatibleHandle proves task semantics are validated during activation, not first use.
        // incompatibleHandle 证明任务语义在激活阶段校验，而不是等到首次使用。
        let incompatible_handle =
            embedding_capability_handle(BTreeSet::from([EmbeddingTask::SemanticSimilarity]));
        let error = embedding_config_from_capability(&incompatible_handle)
            .expect_err("semantic-similarity-only route must not activate native VMM");
        assert!(error.diagnostic.contains("provider-default task"));
    }

    /// Builds one fully available native VMM capability handle for status contract tests.
    /// 为状态契约测试构造一份全部可用的原生 VMM 能力句柄。
    ///
    /// # Returns
    /// 返回值
    /// A host-issued shape containing every required and optional native VMM operation.
    /// 返回包含全部必需与可选原生 VMM 操作的宿主签发结构。
    fn complete_capability_handle() -> HostAiCapabilityHandle {
        // registration supplies the exact production identities, caller permissions, and requirements.
        // registration 提供准确的生产身份、调用方权限及能力要求。
        let registration = native_vmm_capability_registration();
        // operations mirror every declared requirement as an explicitly available capability.
        // operations 将每项已声明要求映射为明确可用的能力。
        let operations = registration
            .requirements
            .iter()
            .map(|requirement| InferenceOperationCapability {
                operation: requirement.operation,
                purpose_id: requirement.purpose_id.clone(),
                available: true,
                unavailable_reason: None,
                route: None,
                embedding: None,
                embedding_batch: None,
                rerank: None,
            })
            .collect();
        // snapshot binds the complete operation set to the production VMM consumer profile.
        // snapshot 将完整操作集合绑定到生产 VMM 消费者配置档。
        let snapshot = InferenceCapabilitySnapshot {
            contract_version: INFERENCE_CONTRACT_VERSION,
            consumer_profile_id: registration.consumer_profile_id.clone(),
            operations,
        };
        HostAiCapabilityHandle::new(
            registration.registration_id,
            registration.owner_id,
            registration.consumer_profile_id,
            registration.caller,
            snapshot,
        )
        .expect("complete native VMM capability handle should be valid")
    }

    /// Builds one native VMM capability handle with an exact embedding task set.
    /// 使用准确的向量任务集合构造一份原生 VMM 能力句柄。
    ///
    /// # Parameters
    /// 参数
    /// `tasks` contains the embedding semantics declared by the selected model route.
    /// `tasks` 包含所选模型路由声明的向量语义。
    ///
    /// # Returns
    /// 返回值
    /// A complete handle whose embedding operation carries deterministic dense-float dimensions.
    /// 返回向量操作带有确定性稠密浮点维度的完整句柄。
    pub(super) fn embedding_capability_handle(
        tasks: BTreeSet<EmbeddingTask>,
    ) -> HostAiCapabilityHandle {
        // handle starts from the exact production registration and required operation set.
        // handle 从准确的生产注册及必需操作集合开始构造。
        let mut handle = complete_capability_handle();
        // embeddingOperation is the unique required route whose typed contract is under test.
        // embeddingOperation 是本测试中校验强类型契约的唯一必需路由。
        let embedding_operation = handle
            .snapshot
            .operations
            .iter_mut()
            .find(|operation| operation.operation == AiOperation::EmbeddingCreate)
            .expect("embedding capability should exist");
        embedding_operation.embedding = Some(EmbeddingCapabilities {
            tasks,
            input_resources: BTreeSet::new(),
            output_kinds: BTreeSet::from([EmbeddingOutputKind::Dense]),
            encodings: BTreeSet::from([EmbeddingEncoding::Float]),
            dimensions: BTreeSet::from([1_536]),
            minimum_dimensions: Some(1_536),
            maximum_dimensions: Some(1_536),
            default_dimensions: Some(1_536),
            maximum_inputs: Some(10),
            maximum_tokens_per_input: Some(8_192),
            normalization: EmbeddingNormalization::Unknown,
            distance_metric: DistanceMetric::Unknown,
        });
        handle
    }

    /// Verifies optional capability loss degrades only its own readiness flag.
    /// 验证可选能力缺失只会降低其自身就绪标记。
    #[test]
    fn host_ai_status_keeps_required_readiness_when_optional_route_is_unavailable() {
        // handle begins with the exact complete production capability set.
        // handle 初始包含准确完整的生产能力集合。
        let mut handle = complete_capability_handle();
        // profileCapability is the optional profile-instruction operation being degraded.
        // profileCapability 是本测试中被降级的可选画像指令操作。
        let profile_capability = handle
            .snapshot
            .operations
            .iter_mut()
            .find(|capability| {
                capability.operation == AiOperation::ConversationRespond
                    && capability.purpose_id.as_deref() == Some("profile_instruction")
            })
            .expect("profile instruction capability should exist");
        profile_capability.available = false;
        profile_capability.unavailable_reason = Some(String::from("not configured"));

        validate_native_vmm_required_capabilities(&handle)
            .expect("optional capability loss must not fail required validation");
        // status preserves independent optional capability truth after required validation succeeds.
        // status 在必需能力校验成功后保留独立的可选能力事实。
        let status = NativeVmmHostAiStatus::ready(&handle);
        assert!(status.checked);
        assert!(status.ready);
        assert!(!status.profile_instruction_ready);
        assert_eq!(
            status.profile_instruction_error.as_deref(),
            Some("not configured")
        );
        assert!(status.rerank_ready);
        assert!(status.rerank_error.is_none());
        assert!(status.error.is_none());
    }

    /// Verifies a refreshed handle cannot remain ready after one required purpose disappears.
    /// 验证刷新后的句柄在任一必需用途消失后不能继续保持就绪。
    #[test]
    fn refreshed_capability_rejects_missing_required_purpose() {
        // handle begins with the exact complete production capability set.
        // handle 初始包含准确完整的生产能力集合。
        let mut handle = complete_capability_handle();
        // requiredCapability is the foundational PostAction L2 route removed after refresh.
        // requiredCapability 是刷新后被移除的基础 PostAction L2 路由。
        let required_capability = handle
            .snapshot
            .operations
            .iter_mut()
            .find(|capability| capability.purpose_id.as_deref() == Some("postaction_l2"))
            .expect("required PostAction L2 capability should exist");
        required_capability.available = false;
        required_capability.unavailable_reason = Some(String::from("route removed"));

        // error proves refreshed required capability loss fails closed with exact purpose context.
        // error 证明刷新后的必需能力缺失会失败关闭并携带准确用途上下文。
        let error = validate_native_vmm_required_capabilities(&handle)
            .expect_err("missing required capability must fail validation");
        assert_eq!(error.code, HostAiErrorCode::CapabilityUnavailable);
        assert!(error.message.contains("postaction_l2"));
    }

    /// Verifies only the canonical pending-turn slot is projected into the host queue binding.
    /// 验证只有规范待处理回合槽位会投影到宿主队列绑定。
    #[test]
    fn core_mutation_extracts_exact_pending_turn_binding() {
        let outcome = MutationOutcome::Committed {
            storage_version: 1,
            chat_compact: None,
            receipt: vulcan_memorymesh_core::CommitReceipt {
                operation_id: vulcan_memorymesh_core::OperationId::new("batch-operation")
                    .expect("valid operation"),
                idempotency_key: vulcan_memorymesh_core::IdempotencyKey::new("batch-key")
                    .expect("valid key"),
                request_fingerprint: format!("fp256:{}", "0".repeat(64)),
                entity_bindings: vec![vulcan_memorymesh_core::EntityBinding {
                    entity_slot_id: vulcan_memorymesh_core::EntitySlotId::new("pending-turn")
                        .expect("valid slot"),
                    entity_kind: vulcan_memorymesh_core::EntityKind::Turn,
                    domain_id: String::from("vmm-turn-7"),
                }],
                committed_versions: Vec::new(),
                side_effect_summary: String::from("turn created"),
            },
        };
        let mutation = NativeVmmCoreMutation {
            operation_id: String::from("batch-operation"),
            mutation_kind: format!("core_storage_mutation:key:fp256:{}", "0".repeat(64)),
            state: String::from(VMM_CORE_MUTATION_PREPARED),
            before_snapshot_json: String::from("{}"),
            after_snapshot_json: String::from("{}"),
            result_json: serde_json::to_string(&outcome).expect("serialize outcome"),
        };

        assert_eq!(
            pending_turn_id_from_core_mutation(&mutation).expect("extract binding"),
            Some(String::from("vmm-turn-7"))
        );
    }
}
