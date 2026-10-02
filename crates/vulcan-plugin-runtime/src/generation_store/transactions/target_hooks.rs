//! Original target Hook initialization has one durable attempt, independent from physical runtime retention.
//! 原目标 Hook 初始化拥有单个持久尝试，独立于物理运行时保留。

use super::*;
use crate::generation_store::{
    VulcanPluginStoreError as Error, VulcanPluginStoreErrorCode as Code,
};

/// This strict sidecar has its own version without changing the original request journal.
/// 此严格附属回执拥有独立版本，不改变原请求日志。
const TARGET_HOOK_RECEIPT_VERSION: u32 = 1;

/// Actual initialization observation never authorizes allocating a replacement Session or VM.
/// 实际初始化观测绝不授权分配替代会话或 VM。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginTargetHookInitializationPhase {
    /// Persisted before the sole initialization submission; an interrupted outcome cannot be retried.
    /// 唯一初始化提交前已持久保存；中断结果不可重试。
    Started,
    /// The original operation and session provided successful initialization evidence.
    /// 原操作及会话提供了成功初始化证据。
    Succeeded,
    /// The original terminal operation did not succeed; this preserves the failed attempt.
    /// 原终态操作未成功；此状态保留失败尝试。
    ObservationFailed,
}

/// Non-secret durable evidence binds one original request to its full selected target and actual staged route.
/// 无秘密持久证据将单个原请求绑定至完整已选目标及实际暂存路由。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginTargetHookInitializationReceipt {
    /// Independently validated format version.
    /// 独立校验的格式版本。
    schema_version: u32,
    /// Canonical original request identity.
    /// 规范原请求身份。
    #[serde(deserialize_with = "records::deserialize_request_id")]
    request_id: Uuid,
    /// Exact package, permissions, configuration and credentials selected by this request.
    /// 此请求选择的精确包、权限、配置及凭据。
    target: VulcanPluginActivation,
    /// Actual nonzero host route generation frozen before initialization.
    /// 初始化前固定的实际非零宿主路由代次。
    routing_generation: u64,
    /// Durable phase is historical evidence rather than live runtime readiness.
    /// 持久阶段为历史证据，而非存活运行时就绪状态。
    phase: VulcanPluginTargetHookInitializationPhase,
    /// Real core identifiers appear together only after the sole open_session submission returned.
    /// 唯一 open_session 提交返回后，真实核心标识才会共同出现。
    #[serde(deserialize_with = "deserialize_core")]
    core: Option<TargetHookCoreIdentity>,
}

/// One observed core admission fixes all identities together without accepting partial identity evidence.
/// 单次已观测核心入场共同固定全部身份，不接受部分身份凭据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetHookCoreIdentity {
    /// Original registered pool.
    /// 原已注册池。
    pool_id: String,
    /// Original fixed session.
    /// 原固定会话。
    session_id: String,
    /// Original initialization operation.
    /// 原初始化操作。
    operation_id: String,
}

/// Require the explicit core field while allowing null before an actual submission returns.
/// 要求显式核心字段，同时允许实际提交返回之前为空。
fn deserialize_core<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<TargetHookCoreIdentity>, D::Error> {
    Option::<TargetHookCoreIdentity>::deserialize(deserializer)
}

impl VulcanPluginTargetHookInitializationReceipt {
    /// Return the durable original request UUID without granting another initialization.
    /// 返回持久原请求 UUID，不授予再次初始化。
    pub fn request_id(&self) -> Uuid {
        self.request_id
    }
    /// Borrow the full original target for read-only request inspection.
    /// 借用完整原目标，供只读请求检查。
    pub fn target(&self) -> &VulcanPluginActivation {
        &self.target
    }
    /// Return the original host routing generation, which cannot identify a replacement VM.
    /// 返回原宿主路由代次，其不可用于标识替代 VM。
    pub fn routing_generation(&self) -> u64 {
        self.routing_generation
    }
    /// Return historical initialization evidence; actual retained identities still decide handoff.
    /// 返回历史初始化证据；实际保留身份仍决定交接。
    pub fn phase(&self) -> VulcanPluginTargetHookInitializationPhase {
        self.phase
    }
}

/// Restricted owner retains the same actual request lock while sidecar writes use the existing namespace fence.
/// 受限所有者保留相同实际请求锁，同时附属回执写入使用既有命名空间屏障。
pub(crate) struct TargetHookInitializationOwner {
    /// Store is derived from the validated original transaction.
    /// 存储派生自已校验原事务。
    store: VulcanPluginGenerationStore,
    /// Original full record remains the sole identity authority.
    /// 原完整记录保持为唯一身份权威。
    transaction: VulcanPluginActivationTransactionRecord,
    /// Same exclusive request Arc prevents reopening or substituting its lock.
    /// 相同排他请求 Arc 防止重开或替换其锁。
    _request: Arc<File>,
    /// Exact actual route generation is immutable throughout the attempt.
    /// 精确实际路由代次在尝试全程保持不可变。
    routing_generation: u64,
}

/// Original registry owns this observer; dropping it deliberately leaves its durable checkpoint intact.
/// 原注册表拥有此观察者；销毁它有意保持持久检查点不变。
pub(crate) struct TargetHookInitializationAttempt {
    /// Restricted original owner retains actual transaction ownership.
    /// 受限原所有者保留实际事务所有权。
    owner: TargetHookInitializationOwner,
    /// Last confirmed strict receipt is updated only after acknowledgement.
    /// 最后已确认严格回执仅在确认后更新。
    receipt: VulcanPluginTargetHookInitializationReceipt,
}

impl VulcanPluginActivationTransaction {
    /// Retain this original Published request for route_generation; return no newly opened request lock.
    /// 为 route_generation 保留此原 Published 请求；不返回新打开的请求锁。
    pub(crate) fn target_hook_initialization_owner(
        &self,
        route_generation: u64,
    ) -> VulcanPluginStoreResult<TargetHookInitializationOwner> {
        if route_generation == 0
            || self.record.phase != VulcanPluginActivationTransactionPhase::Published
        {
            return Err(Error::invalid(
                "Target Hook initialization requires an actual Published route.",
            ));
        }
        Ok(TargetHookInitializationOwner {
            store: VulcanPluginGenerationStore {
                root: self.store_root.clone(),
            },
            transaction: self.record.clone(),
            _request: Arc::clone(&self._lease),
            routing_generation: route_generation,
        })
    }
}

impl TargetHookInitializationOwner {
    /// Reject any durable prior attempt before a missing registry instance can allocate a replacement.
    /// 缺失注册表实例分配替代物之前，拒绝任何持久既有尝试。
    pub(crate) fn require_unattempted(&self) -> VulcanPluginStoreResult<()> {
        // Namespace protection makes the strict receipt observation coherent with the original request.
        // 命名空间保护使严格回执观测与原请求一致。
        let _namespace = self
            .store
            .namespace_lock(&self.transaction.target.plugin_id, true)?;
        if self
            .store
            .read_target_hook_locked(&self.transaction)?
            .is_some()
        {
            return Err(Error::new(
                Code::Conflict,
                "Original target Hook initialization was already attempted; its original instance is unavailable or closed.",
            ));
        }
        Ok(())
    }

    /// Persist Started before core initialization submission; return the sole original observation writer.
    /// 核心初始化提交前持久保存 Started；返回唯一原观测写入者。
    pub(crate) fn begin(&self) -> VulcanPluginStoreResult<TargetHookInitializationAttempt> {
        // The short namespace orders this one side effect without replacing the original request lock.
        // 短命名空间使此单次副作用排序，不替换原请求锁。
        let _namespace = self
            .store
            .namespace_lock(&self.transaction.target.plugin_id, false)?;
        if self
            .store
            .read_transaction_locked(
                &self.transaction.target.plugin_id,
                self.transaction.request_id,
            )?
            .as_ref()
            != Some(&self.transaction)
            || self
                .store
                .read_activation(&self.transaction.target.plugin_id)?
                .as_ref()
                != Some(&self.transaction.target)
            || self
                .store
                .read_target_hook_locked(&self.transaction)?
                .is_some()
        {
            return Err(Error::new(
                Code::Conflict,
                "Original target Hook initialization cannot be submitted again or for a changed target.",
            ));
        }
        // Started is durable before any open_session side effect, including an uncertain submission result.
        // Started 在任何 open_session 副作用之前已持久保存，包括不确定提交结果。
        let receipt = VulcanPluginTargetHookInitializationReceipt {
            schema_version: TARGET_HOOK_RECEIPT_VERSION,
            request_id: self.transaction.request_id,
            target: self.transaction.target.clone(),
            routing_generation: self.routing_generation,
            phase: VulcanPluginTargetHookInitializationPhase::Started,
            core: None,
        };
        self.store
            .write_target_hook_locked(&self.transaction, &receipt)?;
        Ok(TargetHookInitializationAttempt {
            owner: Self {
                store: self.store.clone(),
                transaction: self.transaction.clone(),
                _request: Arc::clone(&self._request),
                routing_generation: self.routing_generation,
            },
            receipt,
        })
    }
}

impl TargetHookInitializationAttempt {
    /// Return whether the original receipt already has a confirmed immutable terminal observation.
    /// 返回原回执是否已拥有确认的不可变终态观测。
    pub(crate) fn is_observed(&self) -> bool {
        self.receipt.phase != VulcanPluginTargetHookInitializationPhase::Started
    }
    /// Bind the exact pool/session/operation returned by core; no argument grants a new submission.
    /// 绑定核心返回的精确池／会话／操作；任何参数均不授予新提交。
    pub(crate) fn bind_core(
        &mut self,
        pool: &str,
        session: &str,
        operation: &str,
    ) -> VulcanPluginStoreResult<()> {
        if self.receipt.core.is_some() {
            return Err(Error::invalid(
                "Original target Hook core identity is already bound.",
            ));
        }
        // Record the three actual identifiers together before terminal observation can advance the receipt.
        // 在终态观测可以推进回执之前，共同记录三个实际标识。
        let mut receipt = self.receipt.clone();
        receipt.core = Some(TargetHookCoreIdentity {
            pool_id: pool.into(),
            session_id: session.into(),
            operation_id: operation.into(),
        });
        self.advance(receipt)
    }

    /// Persist actual terminal evidence for the already-bound original identifiers; never replay initialization.
    /// 为已绑定原标识持久保存实际终态证据；绝不重放初始化。
    pub(crate) fn finish(
        &mut self,
        pool: &str,
        session: &str,
        operation: &str,
        succeeded: bool,
    ) -> VulcanPluginStoreResult<()> {
        if !self.matches_core(pool, session, operation) {
            return Err(Error::invalid(
                "Target Hook observation differs from its original core admission.",
            ));
        }
        // This declared terminal phase comes only from the original supervised operation observation.
        // 此声明终态仅来自原受监督操作观测。
        let phase = if succeeded {
            VulcanPluginTargetHookInitializationPhase::Succeeded
        } else {
            VulcanPluginTargetHookInitializationPhase::ObservationFailed
        };
        if self.receipt.phase == phase {
            return Ok(());
        }
        if self.receipt.phase != VulcanPluginTargetHookInitializationPhase::Started {
            return Err(Error::invalid(
                "Target Hook terminal observation cannot change.",
            ));
        }
        // Retain the exact original immutable identities while advancing only their observation phase.
        // 推进观测阶段时保留精确原不可变身份。
        let mut receipt = self.receipt.clone();
        receipt.phase = phase;
        self.advance(receipt)
    }

    /// Return whether durable success belongs to these same live original identifiers.
    /// 返回持久成功是否属于这些相同存活原标识。
    pub(crate) fn succeeded_for(&self, pool: &str, session: &str, operation: &str) -> bool {
        self.receipt.phase == VulcanPluginTargetHookInitializationPhase::Succeeded
            && self.matches_core(pool, session, operation)
    }

    /// Compare all original core identifiers together; return no partial identity match.
    /// 共同比较全部原核心标识；不返回部分身份匹配。
    fn matches_core(&self, pool: &str, session: &str, operation: &str) -> bool {
        self.receipt.core.as_ref().is_some_and(|core| {
            core.pool_id == pool && core.session_id == session && core.operation_id == operation
        })
    }

    /// Advance exact original receipt with readback for an uncertain write; return only confirmed evidence.
    /// 推进精确原回执，并回读不确定写入；仅返回已确认凭据。
    fn advance(
        &mut self,
        receipt: VulcanPluginTargetHookInitializationReceipt,
    ) -> VulcanPluginStoreResult<()> {
        // Receipt writes share the original request namespace while its actual exclusive Arc remains retained.
        // 回执写入共享原请求命名空间，同时保留其实际排他 Arc。
        let _namespace = self
            .owner
            .store
            .namespace_lock(&self.receipt.target.plugin_id, false)?;
        if self
            .owner
            .store
            .read_transaction_locked(&self.receipt.target.plugin_id, self.receipt.request_id)?
            .as_ref()
            != Some(&self.owner.transaction)
            || self
                .owner
                .store
                .read_target_hook_locked(&self.owner.transaction)?
                .as_ref()
                != Some(&self.receipt)
        {
            return Err(Error::new(
                Code::Conflict,
                "Original target Hook observation evidence changed.",
            ));
        }
        if let Err(error) = self
            .owner
            .store
            .write_target_hook_locked(&self.owner.transaction, &receipt)
            && self
                .owner
                .store
                .read_target_hook_locked(&self.owner.transaction)?
                .as_ref()
                != Some(&receipt)
        {
            return Err(error);
        }
        self.receipt = receipt;
        Ok(())
    }
}

impl VulcanPluginGenerationStore {
    /// Validate exact original live identities against durable success under the completion namespace fence.
    /// 在完成命名空间屏障内，按持久成功校验精确原存活身份。
    /// expected is absent only for an actual no-Hook path, which must not own an initialization receipt.
    /// expected 仅在实际无 Hook 路径缺失，该路径不得拥有初始化回执。
    pub(super) fn require_target_hook_success_locked(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
        expected: Option<(u64, &str, &str, &str)>,
    ) -> VulcanPluginStoreResult<()> {
        // Use the complete selected target and original UUID before evaluating actual runtime identity.
        // 评估实际运行时身份之前，使用完整已选目标及原 UUID。
        let receipt = self.read_target_hook_locked(transaction)?;
        match (receipt, expected) {
            (None, None) => Ok(()),
            (Some(receipt), Some((generation, pool, session, operation)))
                if receipt.routing_generation == generation
                    && receipt.phase == VulcanPluginTargetHookInitializationPhase::Succeeded
                    && receipt.core.as_ref().is_some_and(|core| {
                        core.pool_id == pool
                            && core.session_id == session
                            && core.operation_id == operation
                    }) =>
            {
                Ok(())
            }
            _ => Err(Error::new(
                Code::Conflict,
                "Original target Hook has no exact durable initialization success for completion.",
            )),
        }
    }
    /// Inspect the original request's target Hook receipt without acquiring execution ownership or replaying work.
    /// 检查原请求目标 Hook 回执，不取得执行所有权或重放工作。
    pub fn target_hook_initialization_receipt(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<Option<VulcanPluginTargetHookInitializationReceipt>> {
        // Validate the actual original request before interpreting its sidecar.
        // 解释附属回执之前校验实际原请求。
        let _namespace = self.namespace_lock(plugin_id, true)?;
        match self
            .read_request_locked(plugin_id, request_id)?
            .ok_or_else(|| Error::invalid("Original target Hook request is missing."))?
        {
            VulcanPluginActivationRequestRecord::Activation(transaction) => {
                self.read_target_hook_locked(&transaction)
            }
            VulcanPluginActivationRequestRecord::Preparation(_) => {
                // Early preparation cannot have submitted a Published target Hook initialization.
                // 早期准备不可能提交过 Published 目标 Hook 初始化。
                let directory = self
                    .transaction_directory(plugin_id, false)?
                    .ok_or_else(|| Error::invalid("Original preparation directory is missing."))?;
                if files::exists(&directory.join(format!("{request_id}.target-hook")), false)? {
                    return Err(Error::invalid(
                        "Early preparation cannot own a target Hook receipt.",
                    ));
                }
                Ok(None)
            }
        }
    }

    /// Resolve the sidecar beneath the original validated request directory without creating storage.
    /// 在原已校验请求目录下解析附属回执，不创建存储。
    fn target_hook_path(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<PathBuf> {
        // Distinct suffix keeps canonical original request discovery unchanged.
        // 独立后缀保持规范原请求发现不变。
        let directory = self
            .transaction_directory(&transaction.target.plugin_id, false)?
            .ok_or_else(|| Error::invalid("Original target Hook directory is missing."))?;
        Ok(directory.join(format!("{}.target-hook", transaction.request_id)))
    }

    /// Read strict bounded evidence under the namespace fence; return only actual absence.
    /// 在命名空间屏障内读取严格有界证据；仅返回实际缺失。
    pub(super) fn read_target_hook_locked(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<Option<VulcanPluginTargetHookInitializationReceipt>> {
        // Bound allocation before parsing the independently versioned sidecar.
        // 解析独立版本附属回执之前限制分配。
        let Some(bytes) = files::read_bytes(
            &self.target_hook_path(transaction)?,
            MAX_TARGET_INITIALIZATION_RECEIPT_BYTES,
        )?
        else {
            return Ok(None);
        };
        // Corrupt payload bytes never enter the diagnostic message.
        // 损坏载荷字节绝不进入诊断消息。
        let receipt = serde_json::from_slice(&bytes)
            .map_err(|_| Error::invalid("Invalid target Hook initialization receipt."))?;
        Self::validate_target_hook_receipt(transaction, &receipt)?;
        Ok(Some(receipt))
    }

    /// Validate complete identity, independent version and declared phase/core shape without normalization.
    /// 校验完整身份、独立版本及声明阶段／核心形状，不执行规范化。
    fn validate_target_hook_receipt(
        transaction: &VulcanPluginActivationTransactionRecord,
        receipt: &VulcanPluginTargetHookInitializationReceipt,
    ) -> VulcanPluginStoreResult<()> {
        if receipt.schema_version != TARGET_HOOK_RECEIPT_VERSION
            || receipt.request_id != transaction.request_id
            || receipt.target != transaction.target
            || receipt.routing_generation == 0
            || !matches!(
                transaction.phase,
                VulcanPluginActivationTransactionPhase::Published
                    | VulcanPluginActivationTransactionPhase::Completed
            )
            || (receipt.phase != VulcanPluginTargetHookInitializationPhase::Started
                && receipt.core.is_none())
            || receipt.core.as_ref().is_some_and(|core| {
                core.pool_id.is_empty()
                    || core.session_id.is_empty()
                    || core.operation_id.is_empty()
            })
        {
            return Err(Error::invalid(
                "Target Hook receipt identity or format is invalid.",
            ));
        }
        Ok(())
    }

    /// Atomically write typed bounded receipt under the existing namespace fence; include no diagnostics or secrets.
    /// 在既有命名空间屏障内原子写入类型化有界回执；不包含诊断或秘密。
    fn write_target_hook_locked(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
        receipt: &VulcanPluginTargetHookInitializationReceipt,
    ) -> VulcanPluginStoreResult<()> {
        Self::validate_target_hook_receipt(transaction, receipt)?;
        // Serialize only typed immutable selection evidence and actual core identities.
        // 仅序列化类型化不可变选择证据及实际核心身份。
        let bytes = serde_json::to_vec(receipt)
            .map_err(|_| Error::invalid("Cannot encode target Hook receipt."))?;
        files::write_encoded_record(
            &self.target_hook_path(transaction)?,
            &bytes,
            MAX_TARGET_INITIALIZATION_RECEIPT_BYTES,
        )
    }
}
