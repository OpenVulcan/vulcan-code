//! Public Rust facade for Vulcan Memory Mesh Core.
//! Vulcan Memory Mesh Core 的公共 Rust 门面。

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, RwLock};

pub use vulcan_memorymesh_core_config::{
    ConfigBundle, ConfigImpact, ConfigSnapshot, MEMORY_POLICY_SCHEMA_VERSION, MemoryPolicy,
    NoiseDecision, NoisePolicy, PiiScrubResult, ResourceIdentity, ResourceLayer,
    embedded_resource_identity,
};
pub use vulcan_memorymesh_core_contracts::*;
pub use vulcan_memorymesh_core_engine::{
    decayed_score, deduplicate_by_content, mmr_select, normalize_text,
};
pub use vulcan_memorymesh_core_storage::{InMemoryStorage, StoragePort, StorageSnapshot};

use vulcan_memorymesh_core_config::ConfigRegistry;
use vulcan_memorymesh_core_engine::CommandEngine;

/// Bounds untrusted Host diagnostics appended to one public reconciliation error.
/// 限制追加到公共对账错误中的不可信宿主诊断长度。
const MAX_HOST_ERROR_DIAGNOSTIC_CHARS: usize = 512;

/// Defines one synchronous host adapter capable of executing typed effects.
/// 定义一个能够执行强类型 Effect 的同步宿主适配器。
pub trait HostDriver {
    /// Executes exactly one provider-agnostic host effect.
    /// 执行恰好一个与 Provider 无关的宿主 Effect。
    ///
    /// # Errors
    /// Returns a stable Core error when the host capability fails before a typed result exists.
    /// 当宿主能力在产生强类型结果前失败时返回稳定 Core 错误。
    fn execute_effect(&mut self, request: EffectRequest) -> Result<EffectResult, CoreError>;
}

/// Defines one asynchronous host adapter over the same shared engine.
/// 定义一个基于同一共享引擎的异步宿主适配器。
pub trait AsyncHostDriver {
    /// Executes exactly one provider-agnostic host effect asynchronously.
    /// 异步执行恰好一个与 Provider 无关的宿主 Effect。
    ///
    /// # Errors
    /// The returned future resolves to a stable Core error when host execution fails.
    /// 当宿主执行失败时，返回的 Future 解析为稳定 Core 错误。
    fn execute_effect<'a>(
        &'a mut self,
        request: EffectRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EffectResult, CoreError>> + Send + 'a>>;
}

/// Owns configuration snapshots and the shared resumable command engine.
/// 拥有配置快照与共享可恢复命令引擎。
pub struct MemoryMeshCore {
    /// Protects configuration registration and activation.
    /// 保护配置注册与激活。
    configs: RwLock<ConfigRegistry>,
    /// Protects command lifecycle transitions.
    /// 保护命令生命周期转换。
    engine: Mutex<CommandEngine>,
}

impl MemoryMeshCore {
    /// Creates Core, validates all embedded resources, and activates the default snapshot.
    /// 创建 Core、校验全部嵌入资源并激活默认快照。
    ///
    /// # Errors
    /// Returns an error when authoritative resources or default configuration fail validation.
    /// 当权威资源或默认配置校验失败时返回错误。
    pub fn new() -> Result<Self, CoreError> {
        // Validate embedded resources before allocating the command engine.
        // 在分配命令引擎前校验嵌入资源。
        let mut configs = ConfigRegistry::new()?;
        configs.register(ConfigBundle::default())?;
        configs.activate("default")?;
        Ok(Self {
            configs: RwLock::new(configs),
            engine: Mutex::new(CommandEngine::new()),
        })
    }

    /// Returns the stable capability set implemented by this artifact.
    /// 返回当前工件实现的稳定能力集合。
    #[must_use]
    pub fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    /// Compiles and registers one immutable configuration snapshot.
    /// 编译并注册一个不可变配置快照。
    ///
    /// # Errors
    /// Returns an error for invalid resources or poisoned internal synchronization.
    /// 当资源无效或内部同步被污染时返回错误。
    pub fn register_config(&self, bundle: ConfigBundle) -> Result<Arc<ConfigSnapshot>, CoreError> {
        // Acquire the only mutable configuration registry access.
        // 获取唯一可变配置注册表访问权。
        let mut configs = self
            .configs
            .write()
            .map_err(|_| lock_error("config write"))?;
        configs.register(bundle)
    }

    /// Activates the newest snapshot registered under one exact name.
    /// 激活一个精确名称下注册的最新快照。
    ///
    /// # Errors
    /// Returns an error for unknown names or poisoned internal synchronization.
    /// 当名称未知或内部同步被污染时返回错误。
    pub fn activate_config(&self, name: &str) -> Result<Arc<ConfigSnapshot>, CoreError> {
        // Acquire the only mutable configuration activation access.
        // 获取唯一可变配置激活访问权。
        let mut configs = self
            .configs
            .write()
            .map_err(|_| lock_error("config write"))?;
        configs.activate(name)
    }

    /// Returns the active immutable configuration snapshot.
    /// 返回当前活跃不可变配置快照。
    ///
    /// # Errors
    /// Returns an error when no snapshot is active or synchronization is poisoned.
    /// 当没有活跃快照或同步被污染时返回错误。
    pub fn active_snapshot(&self) -> Result<Arc<ConfigSnapshot>, CoreError> {
        // Borrow the registry only for snapshot Arc cloning.
        // 仅为克隆快照 Arc 借用注册表。
        let configs = self.configs.read().map_err(|_| lock_error("config read"))?;
        configs.active_snapshot()
    }

    /// Starts one public typed command against the active snapshot.
    /// 针对活跃快照启动一个公共强类型命令。
    ///
    /// # Errors
    /// Returns an error for invalid command input, missing configuration, or synchronization failure.
    /// 当命令输入无效、配置缺失或同步失败时返回错误。
    pub fn start(&self, command: CoreCommand) -> Result<(CommandHandle, CommandStep), CoreError> {
        // Freeze the active snapshot before acquiring the independent engine lock.
        // 在获取独立引擎锁前冻结活跃快照。
        let snapshot = self.active_snapshot()?;
        // Acquire the only mutable command transition access.
        // 获取唯一可变命令转换访问权。
        let mut engine = self.engine.lock().map_err(|_| lock_error("engine"))?;
        engine.start(command, snapshot)
    }

    /// Resumes one command with one matching typed effect result.
    /// 使用一个匹配的强类型 Effect 结果恢复命令。
    ///
    /// # Errors
    /// Returns a stable lifecycle, effect, storage, or synchronization error.
    /// 返回稳定生命周期、Effect、存储或同步错误。
    pub fn resume(
        &self,
        handle: &CommandHandle,
        result: EffectResult,
    ) -> Result<CommandStep, CoreError> {
        // Serialize state transitions while host effects remain outside the lock.
        // 串行化状态转换，同时让宿主 Effect 保持在锁外执行。
        let mut engine = self.engine.lock().map_err(|_| lock_error("engine"))?;
        engine.resume(handle, result)
    }

    /// Reconciles one result-uncertain command after the host queries its durable idempotency key.
    /// 在宿主查询持久幂等键后对账一个结果不确定命令。
    ///
    /// # Errors
    /// Returns a stable lifecycle, outcome, storage, or synchronization error.
    /// 返回稳定生命周期、结果、存储或同步错误。
    pub fn reconcile(
        &self,
        handle: &CommandHandle,
        outcome: MutationOutcome,
    ) -> Result<CommandStep, CoreError> {
        // Serialize reconciliation with every other transition of the same shared engine.
        // 将对账与同一共享引擎的其他全部转换串行化。
        let mut engine = self.engine.lock().map_err(|_| lock_error("engine"))?;
        engine.reconcile(handle, outcome)
    }

    /// Cancels one waiting command.
    /// 取消一个等待中的命令。
    ///
    /// # Errors
    /// Returns an error for unknown or terminal commands or synchronization failure.
    /// 当命令未知、已终止或同步失败时返回错误。
    pub fn cancel(&self, handle: &CommandHandle) -> Result<(), CoreError> {
        // Serialize cancellation with resume transitions.
        // 将取消与恢复转换串行化。
        let mut engine = self.engine.lock().map_err(|_| lock_error("engine"))?;
        engine.cancel(handle)
    }

    /// Releases one terminal command and its retained diagnostics.
    /// 释放一个终态命令及其保留的诊断。
    ///
    /// # Errors
    /// Returns an error for unknown, mismatched, waiting, or synchronization-failed commands.
    /// 当命令未知、不匹配、仍在等待或同步失败时返回错误。
    pub fn release_command(&self, handle: &CommandHandle) -> Result<(), CoreError> {
        let mut engine = self.engine.lock().map_err(|_| lock_error("engine"))?;
        engine.release(handle)
    }

    /// Exports one versioned binary checkpoint for one waiting command.
    /// 为一个等待中的命令导出一个带版本二进制检查点。
    ///
    /// # Errors
    /// Returns an error for an unknown handle or synchronization failure.
    /// 当句柄未知或同步失败时返回错误。
    pub fn export_checkpoint(&self, handle: &CommandHandle) -> Result<Vec<u8>, CoreError> {
        let engine = self.engine.lock().map_err(|_| lock_error("engine"))?;
        engine.export_checkpoint(handle)
    }

    /// Restores one waiting command from its versioned checkpoint against the exact bound snapshot.
    /// 针对精确绑定快照从一个带版本检查点恢复一个等待命令。
    ///
    /// # Errors
    /// Returns a stable checkpoint error when the format, integrity, or bound snapshot is invalid.
    /// 当格式、完整性或绑定快照无效时返回稳定检查点错误。
    pub fn restore_checkpoint(
        &self,
        bytes: &[u8],
    ) -> Result<(CommandHandle, CommandStep), CoreError> {
        let checkpoint = vulcan_memorymesh_core_contracts::decode_checkpoint(bytes)?;
        let configs = self.configs.read().map_err(|_| lock_error("configs"))?;
        let snapshot = configs
            .snapshot(&checkpoint.handle.config_snapshot_id)
            .ok_or_else(|| {
                CoreError::new(
                    ErrorCode::CheckpointSnapshotUnavailable,
                    ErrorStage::Command,
                    "the exact checkpoint-bound configuration snapshot is unavailable",
                )
            })?;
        let mut available = std::collections::BTreeMap::new();
        available.insert(snapshot.id.as_str().to_owned(), snapshot);
        drop(configs);
        let mut engine = self.engine.lock().map_err(|_| lock_error("engine"))?;
        engine.restore_checkpoint(bytes, &available)
    }

    /// Returns stable lifecycle diagnostics for one command.
    /// 返回一个命令的稳定生命周期诊断。
    ///
    /// # Errors
    /// Returns an error for an unknown handle or synchronization failure.
    /// 当句柄未知或同步失败时返回错误。
    pub fn command_diagnostics(
        &self,
        handle: &CommandHandle,
    ) -> Result<CommandDiagnostics, CoreError> {
        // Borrow command state without permitting transition.
        // 借用命令状态但不允许转换。
        let engine = self.engine.lock().map_err(|_| lock_error("engine"))?;
        engine.diagnostics(handle)
    }

    /// Drives one command synchronously to completion with a typed host adapter.
    /// 使用强类型宿主适配器同步驱动一个命令直至完成。
    ///
    /// # Errors
    /// Returns any command, host effect, lifecycle, or storage error without hidden fallback.
    /// 返回任何命令、宿主 Effect、生命周期或存储错误且不做隐藏回退。
    pub fn run_to_completion(
        &self,
        command: CoreCommand,
        host: &mut impl HostDriver,
    ) -> Result<CommandResult, CoreError> {
        // Start the shared engine once and retain the exact public handle.
        // 启动共享引擎一次并保留精确公共句柄。
        let (handle, mut step) = self.start(command)?;
        let outcome = loop {
            match step {
                CommandStep::Complete(result) => break Ok(*result),
                CommandStep::Effects(effects) => {
                    // Current engine transitions intentionally request one batched effect at a time.
                    // 当前引擎转换有意每次请求一个批量 Effect。
                    let request = match exactly_one_effect(effects) {
                        Ok(request) => request,
                        Err(error) => break Err(error),
                    };
                    // Execute the external capability with no Core lock held.
                    // 在不持有 Core 锁的情况下执行外部能力。
                    let effect_id = request.effect_id.clone();
                    let side_effect_possible = request.side_effect_possible;
                    let result = host.execute_effect(request);
                    step = match self.resume_automatic_result(
                        &handle,
                        effect_id,
                        side_effect_possible,
                        result,
                    ) {
                        Ok(step) => step,
                        Err(error) => break Err(error),
                    };
                }
            }
        };
        self.finish_automatic_command(&handle, outcome)
    }

    /// Drives one command asynchronously to completion through the same shared engine.
    /// 通过同一共享引擎异步驱动一个命令直至完成。
    ///
    /// # Errors
    /// Returns any command, asynchronous host, lifecycle, or storage error.
    /// 返回任何命令、异步宿主、生命周期或存储错误。
    pub async fn run_to_completion_async(
        &self,
        command: CoreCommand,
        host: &mut impl AsyncHostDriver,
    ) -> Result<CommandResult, CoreError> {
        // Start the same synchronous state transition engine used by all hosts.
        // 启动所有宿主共用的同一同步状态转换引擎。
        let (handle, mut step) = self.start(command)?;
        let outcome = loop {
            match step {
                CommandStep::Complete(result) => break Ok(*result),
                CommandStep::Effects(effects) => {
                    // Extract the exact requested effect without creating an alternate async flow.
                    // 提取精确请求的 Effect，不创建替代异步流程。
                    let request = match exactly_one_effect(effects) {
                        Ok(request) => request,
                        Err(error) => break Err(error),
                    };
                    // Await the host outside all internal locks.
                    // 在全部内部锁之外等待宿主。
                    let effect_id = request.effect_id.clone();
                    let side_effect_possible = request.side_effect_possible;
                    let result = host.execute_effect(request).await;
                    step = match self.resume_automatic_result(
                        &handle,
                        effect_id,
                        side_effect_possible,
                        result,
                    ) {
                        Ok(step) => step,
                        Err(error) => break Err(error),
                    };
                }
            }
        };
        self.finish_automatic_command(&handle, outcome)
    }

    /// Applies a Host response while preserving unresolved writes rejected before state transition.
    /// 应用宿主响应，并保留在状态转换前被拒绝的未决写入。
    fn resume_automatic_result(
        &self,
        handle: &CommandHandle,
        effect_id: EffectId,
        side_effect_possible: bool,
        result: Result<EffectResult, CoreError>,
    ) -> Result<CommandStep, CoreError> {
        let transition = result.and_then(|result| self.resume(handle, result));
        match transition {
            Err(error)
                if side_effect_possible
                    && self
                        .command_diagnostics(handle)
                        .is_ok_and(|diagnostics| diagnostics.status == CommandStatus::Waiting) =>
            {
                // A wrong effect identity or oversized response is not evidence of non-commit.
                // 错误的 Effect 身份或超限响应不能证明未提交。
                self.resume_uncertain_storage_effect(handle, effect_id, &error)
            }
            transition => transition,
        }
    }

    /// Resumes one side-effecting request with an explicit uncertain mutation outcome.
    /// 使用显式结果不确定的变更结果恢复一个具有副作用的请求。
    fn resume_uncertain_storage_effect(
        &self,
        handle: &CommandHandle,
        effect_id: EffectId,
        host_error: &CoreError,
    ) -> Result<CommandStep, CoreError> {
        let result = EffectResult {
            effect_id,
            result: HostEffectResult::StorageMutation(MutationOutcome::Uncertain),
        };
        self.resume(handle, result).map_err(|mut error| {
            // Preserve the transport diagnostic only as bounded developer context while the
            // engine remains authoritative for the stable error code and reconciliation keys.
            // 仅将传输诊断作为受限开发上下文保留，稳定错误码与对账键仍以引擎为权威。
            if !host_error.diagnostic.is_empty() {
                let bounded_host_diagnostic: String = host_error
                    .diagnostic
                    .chars()
                    .take(MAX_HOST_ERROR_DIAGNOSTIC_CHARS)
                    .collect();
                error.diagnostic = format!(
                    "{}; host execution did not return a mutation acknowledgement: {}",
                    error.diagnostic, bounded_host_diagnostic
                );
            }
            error
        })
    }

    /// Finalizes one convenience-run command without masking its primary outcome.
    /// 在不掩盖主要结果的情况下收尾一个便捷执行命令。
    fn finish_automatic_command(
        &self,
        handle: &CommandHandle,
        outcome: Result<CommandResult, CoreError>,
    ) -> Result<CommandResult, CoreError> {
        // Preserve result-uncertain commands so the caller can reconstruct the handle from
        // the returned error identities and reconcile the exact retained mutation.
        // 保留结果不确定命令，使调用方可从返回错误的身份字段重建句柄，并对账精确保留的变更。
        let preserve_for_reconciliation = outcome.as_ref().is_err_and(|error| {
            error.code == ErrorCode::StorageResultUncertain
                && error.side_effect_possible
                && error.command_id.as_ref() == Some(&handle.command_id)
                && error.config_snapshot_id.as_ref() == Some(&handle.config_snapshot_id)
                && self
                    .command_diagnostics(handle)
                    .is_ok_and(|diagnostics| diagnostics.status == CommandStatus::ResultUncertain)
        });
        if preserve_for_reconciliation {
            return outcome;
        }
        if outcome.is_err() {
            // Cancellation succeeds only for still-waiting failures; terminal failures remain releasable.
            // 取消仅对仍在等待的失败成功；终态失败仍可直接释放。
            let _ = self.cancel(handle);
        }
        let release = self.release_command(handle);
        match outcome {
            Ok(result) => {
                release?;
                Ok(result)
            }
            Err(error) => {
                let _ = release;
                Err(error)
            }
        }
    }

    /// Applies active-snapshot PII rules to one text value.
    /// 对一个文本值应用活跃快照 PII 规则。
    ///
    /// # Errors
    /// Returns an error when the active snapshot cannot be read.
    /// 当无法读取活跃快照时返回错误。
    pub fn scrub_pii(&self, text: &str) -> Result<PiiScrubResult, CoreError> {
        self.active_snapshot()?.pii_scrubber.scrub(text)
    }

    /// Applies active-snapshot Noise rules to one role and text value.
    /// 对一个角色与文本值应用活跃快照 Noise 规则。
    ///
    /// # Errors
    /// Returns an error when the active snapshot cannot be read.
    /// 当无法读取活跃快照时返回错误。
    pub fn detect_noise(&self, role: Role, text: &str) -> Result<NoiseDecision, CoreError> {
        Ok(self.active_snapshot()?.noise_detector.detect(role, text))
    }
}
/// Extracts exactly one effect because state resumes are one-result transitions.
/// 提取恰好一个 Effect，因为状态恢复是单结果转换。
fn exactly_one_effect(mut effects: Vec<EffectRequest>) -> Result<EffectRequest, CoreError> {
    if effects.len() != 1 {
        return Err(CoreError::new(
            ErrorCode::Internal,
            ErrorStage::Command,
            "command engine emitted an unsupported effect batch shape",
        ));
    }
    effects.pop().ok_or_else(|| {
        CoreError::new(
            ErrorCode::Internal,
            ErrorStage::Command,
            "command engine emitted an empty effect batch",
        )
    })
}

/// Creates one synchronization invariant error.
/// 创建一个同步不变量错误。
fn lock_error(lock: &str) -> CoreError {
    CoreError::new(
        ErrorCode::Internal,
        ErrorStage::Command,
        format!("internal {lock} lock is poisoned"),
    )
}
