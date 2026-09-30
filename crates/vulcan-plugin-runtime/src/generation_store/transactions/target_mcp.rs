//! Durable target MCP evidence preserves one original startup without recreating retired owners.
//! 持久目标 MCP 证据保留单次原启动，不重建已退役所有者。
use super::*;
use crate::generation_store::{
    VulcanPluginStoreError as Error, VulcanPluginStoreErrorCode as Code,
};

/// Independent strict format version preserves the original request journal.
/// 独立严格格式版本保持原请求日志不变。
const TARGET_MCP_RECEIPT_VERSION: u32 = 1;

/// Historical startup evidence never grants replacement transport creation.
/// 历史启动证据绝不授予替代传输创建。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginTargetMcpInitializationPhase {
    /// Durable before actual startup; unknown results cannot be replayed.
    /// 实际启动之前已持久保存；未知结果不可重放。
    Started,
    /// Real original readiness was observed with the same actual owners.
    /// 已观测真实原就绪及相同实际所有者。
    Ready,
    /// Startup or readiness observation failed without authorizing another attempt.
    /// 启动或就绪观测失败，不授权另一尝试。
    ObservationFailed,
}

/// Opaque UUIDs come from the actual immutable runtime and partition lease owners.
/// 不透明 UUID 来自实际不可变运行时及分区租约所有者。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetMcpInstanceIdentity {
    /// Actual runtime owner nonce is not a request UUID, pointer or PID.
    /// 实际运行时所有者随机标识不是请求 UUID、指针或 PID。
    #[serde(deserialize_with = "records::deserialize_request_id")]
    runtime_instance_id: Uuid,
    /// Actual partition owner nonce is shared by its Arc clones.
    /// 实际分区所有者随机标识由其 Arc 克隆共享。
    #[serde(deserialize_with = "records::deserialize_request_id")]
    partition_lease_instance_id: Uuid,
}

/// Strict evidence binds the original UUID, full selected target and actual route to a single startup.
/// 严格证据将原 UUID、完整已选目标及实际路由绑定至单次启动。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginTargetMcpInitializationReceipt {
    /// Unknown versions are rejected without byte repair.
    /// 拒绝未知版本，不修复字节。
    schema_version: u32,
    /// Canonical original request identity remains separate from actual owner nonces.
    /// 规范原请求身份保持与实际所有者随机标识分离。
    #[serde(deserialize_with = "records::deserialize_request_id")]
    request_id: Uuid,
    /// Full package, configuration, credential and permission selection.
    /// 完整包、配置、凭据及权限选择。
    target: VulcanPluginActivation,
    /// Actual immutable staged host route.
    /// 实际不可变暂存宿主路由。
    routing_generation: u64,
    /// Durable observation does not prove present runtime readiness.
    /// 持久观测不证明当前运行时就绪。
    phase: VulcanPluginTargetMcpInitializationPhase,
    /// Explicit null retains unknown construction; acknowledged owners always appear together.
    /// 显式空值保留未知构造；已确认所有者始终共同出现。
    #[serde(deserialize_with = "deserialize_instances")]
    instances: Option<TargetMcpInstanceIdentity>,
}

/// Decode required identity presence; explicit null is valid before actual construction acknowledgement.
/// 解码必需身份字段；实际构造确认之前，显式空值有效。
fn deserialize_instances<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<TargetMcpInstanceIdentity>, D::Error> {
    Option::<TargetMcpInstanceIdentity>::deserialize(deserializer)
}

impl VulcanPluginTargetMcpInitializationReceipt {
    /// Return original request identity without granting startup.
    /// 返回原请求身份，不授予启动。
    pub fn request_id(&self) -> Uuid {
        self.request_id
    }
    /// Borrow the complete exact target for read-only inspection.
    /// 借用完整精确目标，供只读检查。
    pub fn target(&self) -> &VulcanPluginActivation {
        &self.target
    }
    /// Return the actual original route generation.
    /// 返回实际原路由代次。
    pub fn routing_generation(&self) -> u64 {
        self.routing_generation
    }
    /// Return historical observation, without permitting replay of missing owners.
    /// 返回历史观测，不允许重放缺失所有者。
    pub fn phase(&self) -> VulcanPluginTargetMcpInitializationPhase {
        self.phase
    }
    /// Return both actual immutable owner UUIDs together, or None for unknown construction.
    /// 共同返回两个实际不可变所有者 UUID，或为未知构造返回 None。
    pub fn instance_ids(&self) -> Option<(Uuid, Uuid)> {
        self.instances.as_ref().map(|instances| {
            (
                instances.runtime_instance_id,
                instances.partition_lease_instance_id,
            )
        })
    }
}

/// Restricted owner retains the same real request Arc throughout startup and durable observation.
/// 受限所有者在启动及持久观测全程保留相同真实请求 Arc。
pub(crate) struct TargetMcpInitializationOwner {
    /// Original validated store.
    /// 原已校验存储。
    store: VulcanPluginGenerationStore,
    /// Exact original full Published record.
    /// 精确原完整 Published 记录。
    transaction: VulcanPluginActivationTransactionRecord,
    /// Same exclusive file lock is never reopened.
    /// 相同排他文件锁绝不重开。
    _request: Arc<File>,
    /// Actual original route remains fixed.
    /// 实际原路由保持固定。
    routing_generation: u64,
}

/// Sole source writer persists evidence; dropping it leaves the last durable checkpoint unchanged.
/// 唯一来源写入者持久保存证据；销毁它保持最后持久检查点不变。
pub(crate) struct TargetMcpInitializationAttempt {
    /// Actual same-lock original ownership.
    /// 实际同锁原所有权。
    owner: TargetMcpInitializationOwner,
    /// Last acknowledged strict receipt.
    /// 最后已确认严格回执。
    receipt: VulcanPluginTargetMcpInitializationReceipt,
}

impl VulcanPluginActivationTransaction {
    /// Retain this original Published request for generation, returning the same lock owner without startup.
    /// 为 generation 保留此原 Published 请求，返回相同锁所有者，不启动。
    pub(crate) fn target_mcp_initialization_owner(
        &self,
        generation: u64,
    ) -> VulcanPluginStoreResult<TargetMcpInitializationOwner> {
        if generation == 0 || self.record.phase != VulcanPluginActivationTransactionPhase::Published
        {
            return Err(Error::invalid(
                "Target MCP startup requires the original Published route.",
            ));
        }
        Ok(TargetMcpInitializationOwner {
            store: VulcanPluginGenerationStore {
                root: self.store_root.clone(),
            },
            transaction: self.record.clone(),
            _request: self.retain_published_target_owner(),
            routing_generation: generation,
        })
    }
}

impl TargetMcpInitializationOwner {
    /// Reject any existing attempt before missing actual owners can allocate replacement transports.
    /// 缺失实际所有者分配替代传输之前，拒绝任何既有尝试。
    pub(crate) fn require_unattempted(&self) -> VulcanPluginStoreResult<()> {
        // Observation shares the original namespace without replacing its retained request lock.
        // 观测共享原命名空间，不替换其保留请求锁。
        let _namespace = self
            .store
            .namespace_lock(&self.transaction.target.plugin_id, true)?;
        if self
            .store
            .read_target_mcp_locked(&self.transaction)?
            .is_some()
        {
            return Err(Error::new(
                Code::Conflict,
                "Original target MCP startup was already attempted; its missing or closed owners cannot be recreated.",
            ));
        }
        Ok(())
    }
    /// Persist Started before actual startup and return the sole same-request observation writer.
    /// 实际启动之前持久保存 Started，并返回唯一同请求观测写入者。
    pub(crate) fn begin(&self) -> VulcanPluginStoreResult<TargetMcpInitializationAttempt> {
        // The short namespace orders one startup while the original Arc excludes another owner.
        // 原 Arc 排除另一所有者时，短命名空间使单次启动排序。
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
                .read_target_mcp_locked(&self.transaction)?
                .is_some()
        {
            return Err(Error::new(
                Code::Conflict,
                "Target MCP startup differs from the original Published request or was already attempted.",
            ));
        }
        // Actual owner UUIDs are absent until actual construction, never synthesized from the request UUID.
        // 实际所有者 UUID 在实际构造之前缺失，绝不从请求 UUID 合成。
        let receipt = VulcanPluginTargetMcpInitializationReceipt {
            schema_version: TARGET_MCP_RECEIPT_VERSION,
            request_id: self.transaction.request_id,
            target: self.transaction.target.clone(),
            routing_generation: self.routing_generation,
            phase: VulcanPluginTargetMcpInitializationPhase::Started,
            instances: None,
        };
        self.store
            .write_target_mcp_locked(&self.transaction, &receipt)?;
        Ok(TargetMcpInitializationAttempt {
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

impl TargetMcpInitializationAttempt {
    /// Bind both UUIDs allocated by the actual owners once, before their readiness observation.
    /// 在其就绪观测之前，仅绑定一次两个实际所有者分配的 UUID。
    pub(crate) fn bind_instance(
        &mut self,
        runtime_instance_id: Uuid,
        partition_lease_instance_id: Uuid,
    ) -> VulcanPluginStoreResult<()> {
        if self.receipt.instances.is_some()
            || self.receipt.phase != VulcanPluginTargetMcpInitializationPhase::Started
        {
            return Err(Error::invalid(
                "Original target MCP identities were already bound or observed.",
            ));
        }
        // Preserve the real immutable owner UUIDs together, without serializing addresses or process identities.
        // 共同保留真实不可变所有者 UUID，不序列化地址或进程身份。
        let mut receipt = self.receipt.clone();
        receipt.instances = Some(TargetMcpInstanceIdentity {
            runtime_instance_id,
            partition_lease_instance_id,
        });
        self.advance(receipt)
    }
    /// Persist actual readiness or failure; failure may precede acknowledged actual construction.
    /// 持久保存实际就绪或失败；失败可以先于已确认实际构造。
    pub(crate) fn finish(&mut self, succeeded: bool) -> VulcanPluginStoreResult<()> {
        // Genuine source observation alone chooses the immutable terminal state.
        // 仅真实来源观测选择不可变终态。
        let phase = if succeeded {
            VulcanPluginTargetMcpInitializationPhase::Ready
        } else {
            VulcanPluginTargetMcpInitializationPhase::ObservationFailed
        };
        if self.receipt.phase == phase {
            return Ok(());
        }
        if self.receipt.phase != VulcanPluginTargetMcpInitializationPhase::Started
            || (succeeded && self.receipt.instances.is_none())
        {
            return Err(Error::invalid(
                "Target MCP observation cannot change or become ready without actual owners.",
            ));
        }
        // Advance only phase while preserving the original request and immutable actual owner identities.
        // 仅推进阶段，保留原请求及不可变实际所有者身份。
        let mut receipt = self.receipt.clone();
        receipt.phase = phase;
        self.advance(receipt)
    }
    /// Return whether durable Ready belongs to these same retained actual owner UUIDs.
    /// 返回持久 Ready 是否属于这些相同保留实际所有者 UUID。
    pub(crate) fn ready_for(
        &self,
        runtime_instance_id: Uuid,
        partition_lease_instance_id: Uuid,
    ) -> bool {
        self.receipt.phase == VulcanPluginTargetMcpInitializationPhase::Ready
            && self.receipt.instance_ids()
                == Some((runtime_instance_id, partition_lease_instance_id))
    }
    /// Advance strict original evidence under its namespace; uncertain writes require exact readback acknowledgement.
    /// 在原命名空间内推进严格原证据；不确定写入要求精确回读确认。
    fn advance(
        &mut self,
        receipt: VulcanPluginTargetMcpInitializationReceipt,
    ) -> VulcanPluginStoreResult<()> {
        // The same actual request Arc remains retained across all sidecar writes and readback.
        // 相同实际请求 Arc 跨全部附属回执写入及回读保持保留。
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
                .read_target_mcp_locked(&self.owner.transaction)?
                .as_ref()
                != Some(&self.receipt)
        {
            return Err(Error::new(
                Code::Conflict,
                "Original target MCP observation evidence changed.",
            ));
        }
        if let Err(error) = self
            .owner
            .store
            .write_target_mcp_locked(&self.owner.transaction, &receipt)
        {
            if self
                .owner
                .store
                .read_target_mcp_locked(&self.owner.transaction)?
                .as_ref()
                != Some(&receipt)
            {
                return Err(error);
            }
        }
        self.receipt = receipt;
        Ok(())
    }
}

impl VulcanPluginGenerationStore {
    /// Inspect the original strict startup receipt without ownership, execution or automatic recovery.
    /// 检查原严格启动回执，不取得所有权、不执行或自动恢复。
    pub fn target_mcp_initialization_receipt(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<Option<VulcanPluginTargetMcpInitializationReceipt>> {
        // Actual request inspection validates attached sidecars within the same read fence.
        // 实际请求检查在相同读取屏障内校验关联附属回执。
        let _namespace = self.namespace_lock(plugin_id, true)?;
        match self
            .read_request_locked(plugin_id, request_id)?
            .ok_or_else(|| Error::invalid("Original target MCP request is missing."))?
        {
            VulcanPluginActivationRequestRecord::Activation(transaction) => {
                self.read_target_mcp_locked(&transaction)
            }
            VulcanPluginActivationRequestRecord::Preparation(_) => Ok(None),
        }
    }
    /// Validate durable Ready against sealed same-owner evidence under the combined completion namespace.
    /// 在组合完成命名空间内，按封闭同所有者证据校验持久 Ready。
    pub(super) fn require_target_mcp_ready_locked(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
        expected: Option<&crate::VulcanPluginMcpTargetHandoffEvidence>,
    ) -> VulcanPluginStoreResult<()> {
        // Complete request and selection validation precedes immutable actual owner UUID comparison.
        // 完整请求及选择校验先于不可变实际所有者 UUID 比较。
        let receipt = self.read_target_mcp_locked(transaction)?;
        match (receipt, expected) {
            (None, None) => Ok(()),
            (Some(receipt), Some(evidence))
                if evidence.request_id() == transaction.request_id
                    && evidence.target() == &transaction.target
                    && receipt.routing_generation == evidence.generation()
                    && receipt.phase == VulcanPluginTargetMcpInitializationPhase::Ready
                    && receipt.instance_ids()
                        == Some((
                            evidence.runtime_instance_id(),
                            evidence.partition_lease_instance_id(),
                        )) =>
            {
                Ok(())
            }
            _ => Err(Error::new(
                Code::Conflict,
                "Target MCP has no exact Ready receipt for its held original owners.",
            )),
        }
    }
    /// Resolve the exact existing request sidecar without creating storage or scanning candidate paths.
    /// 解析精确既有请求附属回执，不创建存储或扫描候选路径。
    fn target_mcp_path(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<PathBuf> {
        // Separate suffix preserves canonical request discovery and its original index.
        // 独立后缀保持规范请求发现及其原索引。
        let directory = self
            .transaction_directory(&transaction.target.plugin_id, false)?
            .ok_or_else(|| Error::invalid("Original target MCP directory is missing."))?;
        Ok(directory.join(format!("{}.target-mcp", transaction.request_id)))
    }
    /// Read bounded strict evidence under caller-held namespace exclusion; return actual absence only.
    /// 在调用方持有命名空间互斥时读取有界严格证据；仅返回实际缺失。
    pub(super) fn read_target_mcp_locked(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<Option<VulcanPluginTargetMcpInitializationReceipt>> {
        // Existing limits bound allocation before private typed metadata decoding.
        // 解码私有类型化元数据之前，既有上限限制分配。
        let Some(bytes) = files::read_bytes(
            &self.target_mcp_path(transaction)?,
            MAX_TARGET_INITIALIZATION_RECEIPT_BYTES,
        )?
        else {
            return Ok(None);
        };
        // Generic diagnostics never include serialized private selection bytes.
        // 通用诊断绝不包含序列化私有选择字节。
        let receipt = serde_json::from_slice(&bytes)
            .map_err(|_| Error::invalid("Invalid target MCP startup receipt."))?;
        Self::validate_target_mcp_receipt(transaction, &receipt)?;
        Ok(Some(receipt))
    }
    /// Validate independent version, exact full target and canonical distinct actual owner UUIDs without normalization.
    /// 校验独立版本、精确完整目标及规范不同实际所有者 UUID，不规范化。
    fn validate_target_mcp_receipt(
        transaction: &VulcanPluginActivationTransactionRecord,
        receipt: &VulcanPluginTargetMcpInitializationReceipt,
    ) -> VulcanPluginStoreResult<()> {
        if receipt.schema_version != TARGET_MCP_RECEIPT_VERSION
            || receipt.request_id != transaction.request_id
            || receipt.target != transaction.target
            || receipt.routing_generation == 0
            || !matches!(
                transaction.phase,
                VulcanPluginActivationTransactionPhase::Published
                    | VulcanPluginActivationTransactionPhase::Completed
            )
            || (receipt.phase == VulcanPluginTargetMcpInitializationPhase::Ready
                && receipt.instances.is_none())
            || receipt.instances.as_ref().is_some_and(|instances| {
                instances.runtime_instance_id.is_nil()
                    || instances.partition_lease_instance_id.is_nil()
                    || instances.runtime_instance_id == receipt.request_id
                    || instances.partition_lease_instance_id == receipt.request_id
                    || instances.runtime_instance_id == instances.partition_lease_instance_id
            })
        {
            return Err(Error::invalid(
                "Target MCP receipt identity or format is invalid.",
            ));
        }
        Ok(())
    }
    /// Atomically persist typed bounded receipt while the caller holds the original namespace fence.
    /// 调用方持有原命名空间屏障时，原子持久保存类型化有界回执。
    fn write_target_mcp_locked(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
        receipt: &VulcanPluginTargetMcpInitializationReceipt,
    ) -> VulcanPluginStoreResult<()> {
        Self::validate_target_mcp_receipt(transaction, receipt)?;
        // Only immutable selection and actual opaque owner UUIDs enter the sidecar.
        // 仅不可变选择及实际不透明所有者 UUID 进入附属回执。
        let bytes = serde_json::to_vec(receipt)
            .map_err(|_| Error::invalid("Cannot encode target MCP receipt."))?;
        files::write_encoded_record(
            &self.target_mcp_path(transaction)?,
            &bytes,
            MAX_TARGET_INITIALIZATION_RECEIPT_BYTES,
        )
    }
}
