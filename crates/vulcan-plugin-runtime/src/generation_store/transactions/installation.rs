//! Installation effects retain a durable one-attempt receipt under the original activation request.
//! 安装副作用在原激活请求下保留持久单次尝试回执。

use super::*;
use crate::generation_store::{
    VulcanPluginStoreError as Error, VulcanPluginStoreErrorCode as Code,
};

/// Receipt format is independent from the unchanged activation journal and its index.
/// 回执格式独立于保持不变的激活日志及其索引。
const INSTALLATION_RECEIPT_VERSION: u32 = 1;

/// Preserve the public installation name and wire format using the shared observation definition.
/// 使用共享观测定义保留公开安装名称及线格式。
pub use super::effects::VulcanPluginEffectObservation as VulcanPluginInstallationPhase;

/// Non-secret receipt binds installation evidence to the original immutable transaction identity.
/// 不含秘密的回执将安装证据绑定至原不可变事务身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginInstallationReceipt {
    /// Exact format; unknown versions are rejected without rewriting their bytes.
    /// 精确格式；拒绝未知版本，不重写其字节。
    schema_version: u32,
    /// Exact plugin namespace of the containing journal.
    /// 所在日志的精确插件命名空间。
    plugin_id: String,
    /// Original canonical request identity is also supplied to the Lua callback.
    /// 原规范请求身份同时提供给 Lua 回调。
    #[serde(deserialize_with = "records::deserialize_request_id")]
    request_id: Uuid,
    /// Original successor revision rejects receipts from another selection.
    /// 原后继修订拒绝来自另一选择的回执。
    target_revision: u64,
    /// Durable observation; historical success does not prove present runtime readiness.
    /// 持久观测；历史成功不证明当前运行时已经就绪。
    phase: VulcanPluginInstallationPhase,
}

impl VulcanPluginInstallationReceipt {
    /// Return the exact plugin whose original installation was observed.
    /// 返回原安装被观测的精确插件。
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// Return the original request identity without authorizing another execution.
    /// 返回原请求身份，不授权再次执行。
    pub fn request_id(&self) -> Uuid {
        self.request_id
    }

    /// Return durable evidence only; live calls must still be queried through their original owner.
    /// 仅返回持久证据；存活调用仍须经其原所有者查询。
    pub fn phase(&self) -> VulcanPluginInstallationPhase {
        self.phase
    }
}

/// A single private observer may finish the attempt; dropping it deliberately leaves Started durable.
/// 单个私有观察者可以终结尝试；销毁它有意保留持久 Started 状态。
pub(crate) struct InstallationAttempt {
    /// The original store path is retained without selecting another user root.
    /// 保留原存储路径，不选择其他用户根。
    store: VulcanPluginGenerationStore,
    /// Frozen original request remains the only authority for receipt validation.
    /// 固定原请求保持为回执校验的唯一权威。
    transaction: VulcanPluginActivationTransactionRecord,
    /// Exact Started receipt must still match before writing its final observation.
    /// 写入最终观测前，精确 Started 回执必须仍匹配。
    receipt: VulcanPluginInstallationReceipt,
}

impl InstallationAttempt {
    /// Persist success or observation failure for the original call; never include callback diagnostics or replay work.
    /// 为原调用持久保存成功或观测失败；绝不写入回调诊断或重放工作。
    /// Returns only after durable acknowledgement; write failure leaves outcome uncertain and requires readback.
    /// 仅持久确认后返回；写入失败保留结果不确定性并要求回读。
    pub(crate) fn finish(self, succeeded: bool) -> VulcanPluginStoreResult<()> {
        // The existing namespace fence serializes receipt writes with selection changes.
        // 既有命名空间屏障使回执写入与选择变更串行。
        let _namespace = self.store.namespace_lock(&self.receipt.plugin_id, false)?;
        if self
            .store
            .read_transaction_locked(&self.receipt.plugin_id, self.receipt.request_id)?
            .as_ref()
            != Some(&self.transaction)
            || self
                .store
                .read_installation_locked(&self.transaction)?
                .as_ref()
                != Some(&self.receipt)
        {
            return Err(Error::new(
                Code::Conflict,
                "Original installation evidence changed.",
            ));
        }
        // Only this observation advances the original attempt; no second attempt is allocated.
        // 仅此观测推进原尝试；不分配第二次尝试。
        let mut receipt = self.receipt;
        receipt.phase = if succeeded {
            VulcanPluginInstallationPhase::Succeeded
        } else {
            VulcanPluginInstallationPhase::ObservationFailed
        };
        self.store
            .write_installation_locked(&self.transaction, &receipt)
    }
}

impl VulcanPluginActivationCandidate<'_> {
    /// Persist this exact candidate's install intent before native preparation; return the sole observation writer.
    /// 在原生准备之前持久保存此精确候选的安装意图；返回唯一观测写入者。
    /// Any existing attempt rejects replay, including a completed success or an uncertain interrupted attempt.
    /// 任何既有尝试均拒绝重放，包括已完成成功或不确定的中断尝试。
    pub(crate) fn begin_installation_attempt(
        &self,
    ) -> VulcanPluginStoreResult<InstallationAttempt> {
        // The borrowed transaction supplies the validated store; no caller-provided path is accepted.
        // 借用事务提供已校验存储；不接受调用方提供路径。
        let store = VulcanPluginGenerationStore {
            root: self.transaction.store_root.clone(),
        };
        // The original namespace orders duplicate callers without taking another request lock.
        // 原命名空间使重复调用者排序，不取得另一个请求锁。
        let _namespace = store.namespace_lock(&self.transaction.record.target.plugin_id, false)?;
        store.check_transaction_owner(self.transaction)?;
        if self.transaction.record.phase != VulcanPluginActivationTransactionPhase::Prepared
            || store.read_activation(&self.transaction.record.target.plugin_id)?
                != self.transaction.record.predecessor
        {
            return Err(Error::new(
                Code::Conflict,
                "Installation requires the unchanged prepared request.",
            ));
        }
        if store
            .read_installation_locked(&self.transaction.record)?
            .is_some()
        {
            return Err(Error::new(
                Code::Conflict,
                "Installation was already attempted; inspect the original receipt instead of replaying it.",
            ));
        }
        // Probe must be confirmed before installation, and enable/compensation cannot be reordered.
        // 安装前必须确认探测，启用／补偿不能被重新排序。
        store.require_effects_before_install(&self.transaction.record)?;
        // Persist a non-secret checkpoint before any module initialization or installation callback.
        // 在任何模块初始化或安装回调之前，持久保存不含秘密的检查点。
        let receipt = VulcanPluginInstallationReceipt {
            schema_version: INSTALLATION_RECEIPT_VERSION,
            plugin_id: self.transaction.record.target.plugin_id.clone(),
            request_id: self.transaction.record.request_id,
            target_revision: self.transaction.record.target.revision,
            phase: VulcanPluginInstallationPhase::Started,
        };
        store.write_installation_locked(&self.transaction.record, &receipt)?;
        drop(_namespace);
        Ok(InstallationAttempt {
            store,
            transaction: self.transaction.record.clone(),
            receipt,
        })
    }
}

impl VulcanPluginGenerationStore {
    /// Query installation evidence for plugin_id/request_id without ownership, execution, recovery, or mutation.
    /// 查询 plugin_id/request_id 的安装证据，不取得所有权、不执行、不恢复、不变更。
    /// A missing request is invalid; None means no receipt is currently stored, not proof of absent external effects.
    /// 缺失请求无效；None 表示当前未存储回执，不证明外部副作用不存在。
    pub fn installation_receipt(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<Option<VulcanPluginInstallationReceipt>> {
        // The same original request and namespace fence apply to early and configured stages.
        // 相同原请求及命名空间屏障适用于早期和已配置阶段。
        let _namespace = self.namespace_lock(plugin_id, true)?;
        // Only the declared configured stage can legitimately have executed installation.
        // 仅声明的已配置阶段可能合法执行过安装。
        let request = self
            .read_request_locked(plugin_id, request_id)?
            .ok_or_else(|| Error::invalid("Original installation request is missing."))?;
        match request {
            VulcanPluginActivationRequestRecord::Activation(transaction) => {
                self.read_installation_locked(&transaction)
            }
            VulcanPluginActivationRequestRecord::Preparation(_) => {
                // Reject impossible installation evidence rather than silently hiding it from recovery.
                // 拒绝不可能的安装证据，而非向恢复静默隐藏它。
                let directory = self
                    .transaction_directory(plugin_id, false)?
                    .ok_or_else(|| Error::invalid("Original preparation directory disappeared."))?;
                if files::exists(&directory.join(format!("{request_id}.install")), false)? {
                    return Err(Error::invalid(
                        "Early preparation cannot own an installation receipt.",
                    ));
                }
                Ok(None)
            }
        }
    }

    /// Reject uncertain or failed installation before publishing transaction; no receipt imposes no callback requirement.
    /// 发布 transaction 前拒绝不确定或失败安装；无回执不施加回调要求。
    /// Caller holds the namespace fence; return validation without rewriting durable evidence.
    /// 调用方持有命名空间屏障；返回校验结果，不重写持久证据。
    pub(super) fn require_installation_succeeded(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<()> {
        if self
            .read_installation_locked(transaction)?
            .is_some_and(|receipt| receipt.phase != VulcanPluginInstallationPhase::Succeeded)
        {
            return Err(Error::new(
                Code::Conflict,
                "Installation has no confirmed success; inspect its original receipt and external effects.",
            ));
        }
        Ok(())
    }

    /// Resolve the exact sidecar of transaction under its validated existing directory; return no new storage.
    /// 在已校验既有目录下解析 transaction 的精确附属文件；不创建新存储。
    fn installation_path(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<PathBuf> {
        // A distinct suffix keeps canonical request discovery unchanged without another activation pointer.
        // 独立后缀保持规范请求发现不变，不创建另一个激活指针。
        let directory = self
            .transaction_directory(&transaction.target.plugin_id, false)?
            .ok_or_else(|| Error::invalid("Original installation directory is missing."))?;
        Ok(directory.join(format!("{}.install", transaction.request_id)))
    }

    /// Read a bounded strict receipt for transaction under the namespace fence; return actual absence only.
    /// 在命名空间屏障内读取 transaction 的有界严格回执；仅返回实际缺失。
    pub(super) fn read_installation_locked(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<Option<VulcanPluginInstallationReceipt>> {
        // Reuse metadata limits and path validation; corrupt bytes never enter diagnostics.
        // 复用元数据限制及路径校验；损坏字节绝不进入诊断。
        let Some(bytes) =
            files::read_bytes(&self.installation_path(transaction)?, MAX_RECORD_BYTES)?
        else {
            return Ok(None);
        };
        // Explicit identity and version validation prevents attaching unrelated completion evidence.
        // 显式身份及版本校验防止附加无关完成证据。
        let receipt: VulcanPluginInstallationReceipt = serde_json::from_slice(&bytes)
            .map_err(|_| Error::invalid("Invalid installation receipt."))?;
        Self::validate_installation_receipt(transaction, &receipt)?;
        Ok(Some(receipt))
    }

    /// Validate receipt against transaction before use or persistence; return no repaired or normalized fields.
    /// 使用或持久化前按 transaction 校验 receipt；不返回修复或规范化后的字段。
    fn validate_installation_receipt(
        transaction: &VulcanPluginActivationTransactionRecord,
        receipt: &VulcanPluginInstallationReceipt,
    ) -> VulcanPluginStoreResult<()> {
        if receipt.schema_version != INSTALLATION_RECEIPT_VERSION
            || receipt.plugin_id != transaction.target.plugin_id
            || receipt.request_id != transaction.request_id
            || receipt.target_revision != transaction.target.revision
        {
            return Err(Error::invalid(
                "Installation receipt identity or format is invalid.",
            ));
        }
        Ok(())
    }

    /// Atomically persist receipt for transaction with the shared metadata budget; caller retains the namespace fence.
    /// 使用共同元数据预算为 transaction 原子持久保存 receipt；调用方保留命名空间屏障。
    fn write_installation_locked(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
        receipt: &VulcanPluginInstallationReceipt,
    ) -> VulcanPluginStoreResult<()> {
        Self::validate_installation_receipt(transaction, receipt)?;
        // Only typed non-secret evidence is serialized; callback text is never retained here.
        // 仅序列化类型化无秘密证据；此处绝不保留回调文本。
        let bytes = serde_json::to_vec(receipt)
            .map_err(|_| Error::invalid("Cannot encode installation receipt."))?;
        files::write_encoded_record(
            &self.installation_path(transaction)?,
            &bytes,
            MAX_RECORD_BYTES,
        )
    }
}
