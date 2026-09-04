//! Native VMM Core execution driver backed by VulcanCode AI and storage capabilities.
//! 由 VulcanCode AI 与存储能力支撑的原生 VMM Core 执行驱动。

// Core effect execution preserves the frozen diagnostic-rich HostAiError contract at its host edge.
// Core Effect 执行在宿主边界保留已冻结且富含诊断信息的 HostAiError 契约。
#![allow(
    clippy::result_large_err,
    reason = "the native driver must preserve the frozen HostAiError boundary contract"
)]

use super::{llm_contract, prompt};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_ai_contract::{
    AiMessage, AiOperation, ContentItem, EmbeddingEncoding, EmbeddingInput, EmbeddingInputContent,
    EmbeddingInvalidInputPolicy, EmbeddingOutputKind, EmbeddingPayload, EmbeddingRequest,
    EmbeddingSpaceFingerprint, EmbeddingTask, HostAiCallContext, HostAiCapabilityHandle,
    HostAiCapabilityRegistration, HostAiCapabilityRequirement, HostAiEmbeddingRequest, HostAiError,
    HostAiErrorCode, HostAiPort, INFERENCE_CONTRACT_VERSION, InferenceCallerBudget,
    InferenceCallerContext, InferencePermission, InferenceUsageScope, MessageRole,
    ReasoningOutputPolicy, RerankRequest, RerankText, RerankTruncation, ResponsesExecutionRequest,
    StructuredOutputPolicy,
};
use vulcan_memorymesh_core::{
    AsyncHostDriver, CandidateChannelResult, CandidateObservation, CommandResult, CoreCommand,
    CoreError, EffectDeadlineClass, EffectFailure, EffectFailureKind, EffectRequest, EffectResult,
    EmbeddingBatchResult, EmbeddingDropReason, EmbeddingDroppedInput, ErrorCode, ErrorStage,
    FailureClass, HostEffect, HostEffectResult, InMemoryStorage, LlmEffect, LlmPurpose,
    MemoryAccessClass, MemoryMeshCore, MutationBatch, MutationOutcome, PhysicalCandidate,
    PhysicalCandidateChannel, PhysicalCandidateScore, RawScoreKind, RerankEffect, RerankEntry,
    ScoreDirection, StoragePort, StorageQuery, StorageQueryResult, StorageSnapshot,
};
use vulcan_workspace_store::vmm_data_plane::{
    NativeVmmCoreMutation, VMM_CORE_MUTATION_COMMITTED, VMM_CORE_MUTATION_COMPLETED,
    VMM_CORE_MUTATION_PREPARED, VmmDataPlaneHandle, VmmVectorCandidate, VmmVectorRecord,
    VmmVectorSearchFilter,
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

/// Owns the copied native Core and its process-local host driver.
/// 持有复制后的原生 Core 及其进程内宿主驱动。
pub(crate) struct NativeVmmCoreRuntime {
    /// Shared provider-independent command engine.
    /// 共享的供应商无关命令引擎。
    core: MemoryMeshCore,
    /// Cloneable host effect executor used independently by concurrent commands.
    /// 并发命令分别使用的可克隆宿主 Effect 执行器。
    host: NativeVmmHostDriver,
    /// Serializes lazy host-derived Core configuration activation.
    /// 串行化延迟执行的宿主派生 Core 配置激活。
    config_gate: tokio::sync::Mutex<()>,
    /// Latest authoritative Host AI activation outcome used by readiness projection.
    /// 用于就绪状态投影的最近一次权威宿主 AI 激活结果。
    host_ai_status: RwLock<NativeVmmHostAiStatus>,
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
        return (false, Some(String::from("宿主未发布该可选 AI 能力")));
    };
    if capability.available {
        return (true, None);
    }
    (
        false,
        capability
            .unavailable_reason
            .clone()
            .or_else(|| Some(String::from("宿主 AI 能力当前不可用"))),
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
        let core = MemoryMeshCore::new().map_err(|error| error.to_string())?;
        // runtime is assembled before recovery so the same persistence code repairs interrupted maintenance.
        // runtime 在恢复前完成组装，使中断维护可复用同一持久化代码完成修复。
        let runtime = Self {
            core,
            host: NativeVmmHostDriver {
                ai_port,
                capability: Arc::new(RwLock::new(None)),
                embedding_fingerprints: Arc::new(RwLock::new(BTreeMap::new())),
                storage: Arc::new(RwLock::new(storage)),
                mutation_gate: Arc::new(tokio::sync::Mutex::new(())),
                data_plane,
            },
            config_gate: tokio::sync::Mutex::new(()),
            host_ai_status: RwLock::new(NativeVmmHostAiStatus::default()),
        };
        runtime.host.recover_unfinished_core_mutations().await?;
        Ok(runtime)
    }

    /// Runs one typed Core command to a final result through VulcanCode-owned capabilities.
    /// 通过 VulcanCode 持有的能力将一个强类型 Core 命令运行至最终结果。
    ///
    /// # Errors
    /// Returns the stable Core command, effect, or persistence failure.
    /// 返回稳定的 Core 命令、Effect 或持久化失败。
    pub(crate) async fn run(&self, mut command: CoreCommand) -> Result<CommandResult, CoreError> {
        if command_requires_ai_configuration(&command) {
            self.ensure_host_ai_config().await?;
        }
        if command.is_mutating() {
            let snapshot = self.core.active_snapshot()?;
            let fingerprint =
                vulcan_memorymesh_core::request_fingerprint(&command, snapshot.id.as_str());
            command_mutation_identity(&mut command).request_fingerprint = fingerprint;
        }
        let mut host = self.host.clone();
        self.core.run_to_completion_async(command, &mut host).await
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
            .map_err(|error| error.to_string())
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
            .map_err(|_| String::from("原生 VMM 存储读锁已损坏"))
            .map(|storage| storage.snapshot())
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
                    "原生 VMM 操作 `{operation_id}` 已用于 `{}`，不能重用于 `{mutation_kind}`",
                    existing.mutation_kind
                ));
            }
            return serde_json::from_str(&existing.result_json)
                .map_err(|error| format!("解析原生 VMM 幂等维护结果失败：{error}"));
        }
        let before = self.storage_snapshot()?;
        if before.storage_version != expected_storage_version {
            return Err(format!(
                "原生 VMM 维护预检已过期：预期存储版本 {expected_storage_version}，当前版本 {}，请重新预检",
                before.storage_version
            ));
        }
        let mut after = before.clone();
        let output = transform(&mut after)?;
        after.storage_version = after
            .storage_version
            .checked_add(1)
            .ok_or_else(|| String::from("原生 VMM 存储版本已耗尽"))?;
        let staged = InMemoryStorage::from_snapshot(after.clone())
            .map_err(|error| format!("原生 VMM 维护结果无效：{error}"))?;
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
                "原生 VMM 操作 `{operation_id}` 已用于 `{}`，不能重用于 `{mutation_kind}`",
                existing.mutation_kind
            ));
        }
        serde_json::from_str(&existing.result_json)
            .map(Some)
            .map_err(|error| format!("解析原生 VMM 幂等维护结果失败：{error}"))
    }

    /// Activates a Core snapshot derived from the exact configured embedding route.
    /// 激活由精确已配置向量路由派生的 Core 快照。
    async fn ensure_host_ai_config(&self) -> Result<(), CoreError> {
        let _gate = self.config_gate.lock().await;
        let result = self.activate_host_ai_config();
        let status = match result.as_ref() {
            Ok(handle) => NativeVmmHostAiStatus::ready(handle),
            Err(error) => NativeVmmHostAiStatus::failed(error),
        };
        *self
            .host_ai_status
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = status;
        result.map(|_| ())
    }

    /// Refreshes capability truth and activates the matching Core vector-space configuration.
    /// 刷新能力事实并激活匹配的 Core 向量空间配置。
    ///
    /// # Returns
    /// 返回值
    /// The freshly validated capability handle used to derive readiness and Core configuration.
    /// 返回用于派生就绪状态与 Core 配置的最新校验能力句柄。
    ///
    /// # Errors
    /// Returns a provider-neutral configuration error for missing routes or invalid embedding limits.
    /// 必需路由缺失或向量限制无效时返回供应商无关配置错误。
    fn activate_host_ai_config(&self) -> Result<HostAiCapabilityHandle, CoreError> {
        let handle = self.host.capability().map_err(host_ai_core_error)?;
        let (dimension, fingerprint) = embedding_config_from_capability(&handle)?;
        let active = self.core.active_snapshot()?;
        if active.embedding_dimension == dimension && active.vector_space_fingerprint == fingerprint
        {
            return Ok(handle);
        }
        let digest = hex::encode(Sha256::digest(fingerprint.as_bytes()));
        let bundle = vulcan_memorymesh_core::ConfigBundle {
            name: format!("vmm-native-{digest}"),
            vector_space_fingerprint: fingerprint,
            embedding_dimension: dimension,
            ..Default::default()
        };
        let snapshot = self.core.register_config(bundle)?;
        self.core.activate_config(snapshot.name.as_str())?;
        Ok(handle)
    }
}

/// Executes every provider-independent Core effect against VulcanCode-owned services.
/// 针对 VulcanCode 持有的服务执行每个供应商无关 Core Effect。
#[derive(Clone)]
struct NativeVmmHostDriver {
    /// Unified host AI boundary shared with other native libraries.
    /// 与其他原生库共享的统一宿主 AI 边界。
    ai_port: Arc<dyn HostAiPort>,
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
    /// Success after vector, relational, in-memory, and journal state agree.
    /// 向量、关系、内存与账本状态一致后返回成功。
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
    ) -> Result<(), String>
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
            before_snapshot_json: serde_json::to_string(before)
                .map_err(|error| format!("序列化原生 VMM 变更前快照失败：{error}"))?,
            after_snapshot_json: serde_json::to_string(after)
                .map_err(|error| format!("序列化原生 VMM 变更后快照失败：{error}"))?,
            result_json: serde_json::to_string(output)
                .map_err(|error| format!("序列化原生 VMM 变更结果失败：{error}"))?,
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
    /// Success after vectors, relational state, in-memory state, and cleanup agree.
    /// 向量、关系状态、内存状态和清理结果一致后返回成功。
    ///
    /// # Errors
    /// Returns a controlled error while leaving a resumable journal state durable.
    /// 返回受控错误，并保留可恢复的耐久账本状态。
    async fn resume_core_mutation(&self, mutation: &NativeVmmCoreMutation) -> Result<(), String> {
        if mutation.state == VMM_CORE_MUTATION_COMPLETED {
            return Ok(());
        }
        let before: StorageSnapshot = serde_json::from_str(&mutation.before_snapshot_json)
            .map_err(|error| format!("解析原生 VMM 维护前快照失败：{error}"))?;
        let after: StorageSnapshot = serde_json::from_str(&mutation.after_snapshot_json)
            .map_err(|error| format!("解析原生 VMM 维护后快照失败：{error}"))?;
        let staged = InMemoryStorage::from_snapshot(after.clone())
            .map_err(|error| format!("原生 VMM 恢复快照无效：{error}"))?;
        if mutation.state == VMM_CORE_MUTATION_PREPARED {
            self.persist_vectors(&after)
                .await
                .map_err(|error| error.to_string())?;
            let relational = serde_json::to_string(&after.without_embeddings())
                .map_err(|error| format!("序列化原生 VMM 恢复关系快照失败：{error}"))?;
            let pending_turn_id = pending_turn_id_from_core_mutation(mutation)?;
            self.data_plane
                .commit_core_mutation(
                    &mutation.operation_id,
                    &relational,
                    pending_turn_id.as_deref(),
                )
                .await?;
        } else if mutation.state != VMM_CORE_MUTATION_COMMITTED {
            return Err(format!(
                "原生 VMM 维护变更 `{}` 包含未知状态 `{}`",
                mutation.operation_id, mutation.state
            ));
        }
        *self
            .storage
            .write()
            .map_err(|_| String::from("原生 VMM 存储写锁已损坏，需重启后恢复维护变更"))? = staged;
        self.delete_stale_vectors_strict(&before, &after).await?;
        self.data_plane
            .complete_core_mutation(&mutation.operation_id)
            .await
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
            reasoning_effort: None,
            reasoning_output: ReasoningOutputPolicy::Hidden,
            model_parameters: BTreeMap::new(),
            stop_sequences: Vec::new(),
            resources: Vec::new(),
            output_token_limit: Some(NATIVE_VMM_LLM_OUTPUT_TOKENS),
        };
        let context = effect_call_context(request);
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
            request_id: format!("vmm-embedding-{}", request.effect_id.as_str()),
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
            .create_embeddings(&capability, effect_call_context(request), host_request)
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
            .rerank_documents(&capability, effect_call_context(request), rerank_request)
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

    /// Executes a semantic storage query, using LanceDB only for the vector-producing channel.
    /// 执行语义存储查询，仅让向量生产通道使用 LanceDB。
    async fn execute_storage_query(
        &self,
        query: &StorageQuery,
    ) -> Result<HostEffectResult, CoreError> {
        let _gate = if matches!(
            query,
            StorageQuery::VectorCandidates { .. } | StorageQuery::CandidateObservation(_)
        ) {
            Some(self.mutation_gate.lock().await)
        } else {
            None
        };
        let result = match query {
            StorageQuery::VectorCandidates { .. } => {
                StorageQueryResult::VectorCandidates(self.external_vector_channel(query).await?)
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

    /// Combines the LanceDB vector channel with the Core lexical channel from one relational snapshot.
    /// 将 LanceDB 向量通道与同一关系快照中的 Core 词法通道组合。
    async fn external_candidate_observation(
        &self,
        query: &StorageQuery,
    ) -> Result<CandidateObservation, CoreError> {
        let vector = self.external_vector_channel(query).await?;
        let local = {
            let guard = self
                .storage
                .read()
                .map_err(|_| storage_lock_error("read"))?;
            guard.query(query)?
        };
        let StorageQueryResult::CandidateObservation(local) = local else {
            return Err(core_effect_error(
                "candidate observation query returned the wrong storage result",
            ));
        };
        let mut available = local
            .memories
            .into_iter()
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
        if let Some(lexical) = local.lexical_channel.as_ref() {
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
            lexical_channel: local.lexical_channel,
            memories,
        })
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
        &self,
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
        let outcome = staged.mutate(batch)?;
        if !matches!(outcome, MutationOutcome::Committed { .. }) {
            return Ok(HostEffectResult::StorageMutation(outcome));
        }
        let after = staged.snapshot();
        self.persist_prepared_storage_transition(
            batch.identity.operation_id.as_str(),
            mutation_kind.as_str(),
            &before,
            &after,
            &outcome,
        )
        .await
        .map_err(storage_uncertain_error)?;
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
    /// Success only after every stale vector has been deleted.
    /// 仅在全部陈旧向量删除后返回成功。
    ///
    /// # Errors
    /// Returns the exact vector-space resolution or deletion error for durable retry.
    /// 返回准确的向量空间解析或删除错误，以便耐久重试。
    async fn delete_stale_vectors_strict(
        &self,
        before: &StorageSnapshot,
        after: &StorageSnapshot,
    ) -> Result<(), String> {
        let before = snapshot_vector_keys(before);
        let after = snapshot_vector_keys(after);
        let mut by_space = BTreeMap::<String, Vec<String>>::new();
        for (space_id, memory_id) in before.difference(&after) {
            by_space
                .entry(space_id.clone())
                .or_default()
                .push(memory_id.clone());
        }
        for (space_id, memory_ids) in by_space {
            let binding = self
                .data_plane
                .resolve_vector_space(&space_id)
                .await?
                .ok_or_else(|| format!("原生 VMM 向量空间 `{space_id}` 不存在"))?;
            self.data_plane
                .delete_vectors(&binding, &memory_ids)
                .await?;
        }
        Ok(())
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
    let outcome: MutationOutcome = serde_json::from_str(&mutation.result_json)
        .map_err(|error| format!("解析原生 VMM Core 存储变更结果失败：{error}"))?;
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
        return Err(String::from("原生 VMM 登记变更的待处理回合绑定类别无效"));
    }
    match bindings.as_slice() {
        [] => Ok(None),
        [binding] => Ok(Some(binding.domain_id.clone())),
        _ => Err(String::from("原生 VMM 登记变更包含多个待处理回合绑定")),
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
        .map_err(|error| format!("解析原生 VMM Core 快照失败：{error}"))?;
    let mut by_space = BTreeMap::<String, Vec<String>>::new();
    for memory in &snapshot.dataset.memories {
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
            .ok_or_else(|| format!("原生 VMM 向量空间 `{space_id}` 不存在"))?;
        for candidate in data_plane.load_vectors(&binding, &memory_ids).await? {
            vectors.insert(candidate.memory_id, candidate.vector);
        }
    }
    for memory in &mut snapshot.dataset.memories {
        if memory.vector_space_id.is_some() {
            memory.embedding = Some(
                vectors
                    .remove(memory.id.as_str())
                    .ok_or_else(|| format!("原生 VMM 记忆 `{}` 缺少向量", memory.id.as_str()))?,
            );
        }
    }
    if !vectors.is_empty() {
        return Err(String::from("原生 VMM 向量回填返回了未引用记录"));
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
fn effect_call_context(request: &EffectRequest) -> HostAiCallContext {
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
    for memory in &snapshot.dataset.memories {
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
    fn embedding_capability_handle(tasks: BTreeSet<EmbeddingTask>) -> HostAiCapabilityHandle {
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
