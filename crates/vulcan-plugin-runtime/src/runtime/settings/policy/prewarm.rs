//! Automatic policy publication shares the original settings writer and never executes Lua.
//! 自动策略发布共用原设置写入器，绝不执行 Lua。

use super::*;

/// One caller-owned policy replacement freezes its native parent and observed policy predecessor.
/// 单个调用方自有策略替换冻结原生父级及已观测策略前驱。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VulcanPluginPrewarmPolicyUpdate {
    /// Caller UUID identifies the original transaction through an uncertain response.
    /// 调用方 UUID 在响应不确定期间标识原事务。
    pub update_id: String,
    /// Original native parent; a restart cannot replay this mutation into a new parent.
    /// 原原生父级；重启不能将此修改重放到新父级。
    pub runtime_id: String,
    /// Exact observed policy revision, never refreshed automatically after conflict.
    /// 精确已观测策略修订；冲突后绝不自动刷新。
    pub expected_revision: String,
    /// Complete desired rule collection; an empty list explicitly disables automatic prewarming.
    /// 完整期望规则集合；空列表明确关闭自动预热。
    pub rules: Vec<VulcanPluginAutomaticPrewarmRule>,
}

impl VulcanPluginPrewarmPolicyUpdate {
    /// Derive complete settings from current and validate identity and limits before publication.
    /// 从 current 派生完整设置，在发布前校验身份及上限。
    pub(in crate::runtime) fn candidate(
        &self,
        current: &VulcanPluginRuntimeSettings,
    ) -> Result<VulcanPluginRuntimeSettings, String> {
        uuid::Uuid::parse_str(&self.update_id)
            .map_err(|_| "Prewarm policy update_id must be a UUID.".to_owned())?;
        for identity in [&self.runtime_id, &self.expected_revision] {
            if identity.is_empty() || identity.contains('\0') {
                return Err(
                    "Prewarm policy identities must be nonempty and contain no zero byte.".into(),
                );
            }
        }
        // Only automatic targets change; original capacity overrides and timeouts remain authoritative.
        // 仅改变自动目标；原容量覆盖与超时保持权威。
        let mut candidate = current.clone();
        candidate.automatic_prewarm = self.rules.clone();
        candidate.validate()?;
        Ok(candidate)
    }
}

/// Policy phases distinguish retained intent from acknowledgement of durable publication.
/// 策略阶段区分保留意图与持久发布确认。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginPrewarmPolicyPhase {
    /// Preparation is pending or acknowledged; no final settings publication was attempted.
    /// 准备待处理或已确认；尚未尝试最终设置发布。
    Prepared,
    /// Memory-only checkpoint: final publication may have occurred, but activation still awaits acknowledgement.
    /// 仅内存检查点：最终发布可能已经发生，但激活仍等待确认。
    Publishing,
    /// Final settings and receipt were durably acknowledged before live policy activation.
    /// 最终设置及回执在活动策略激活前已完成持久确认。
    Committed,
    /// Explicit cancellation ended an unpublished prepared intent without changing effective settings.
    /// 明确取消结束尚未最终发布的准备意图，不改变生效设置。
    Cancelled,
    /// The original runtime ended before commit; intent is retained without replay.
    /// 原运行时在提交前结束；保留意图但不重放。
    Interrupted,
}

impl VulcanPluginPrewarmPolicyPhase {
    /// Return whether this exact phase permits explicit receipt acknowledgement.
    /// 返回此精确阶段是否允许明确确认回执。
    pub(in crate::runtime) fn is_terminal(self) -> bool {
        matches!(self, Self::Committed | Self::Cancelled | Self::Interrupted)
    }
}

/// The retained policy receipt controls publication only, never a prewarm execution or its deadline.
/// 保留策略回执仅控制发布，绝不控制预热执行或其截止时间。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VulcanPluginPrewarmPolicyRecord {
    /// Original immutable policy request.
    /// 原不可变策略请求。
    pub request: VulcanPluginPrewarmPolicyUpdate,
    /// Exact publication checkpoint.
    /// 精确发布检查点。
    pub phase: VulcanPluginPrewarmPolicyPhase,
    /// Host-issued successor frozen once; it becomes effective only after acknowledged commit in this runtime.
    /// 一次冻结的宿主签发后继；仅在本运行时确认提交后生效。
    pub result_revision: String,
    /// Lifecycle diagnostic, distinct from filesystem publication uncertainty.
    /// 生命周期诊断，独立于文件系统发布不确定性。
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub error: Option<String>,
    /// Live-only publication uncertainty is never persisted as if it were acknowledged evidence.
    /// 仅活动内存的发布不确定性绝不作为已确认事实持久化。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistence_error: Option<String>,
}

/// One atomic administrative view exposes live policy and any retained original publication receipt.
/// 单个原子管理视图暴露活动策略及保留的原发布回执。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginPrewarmPolicyStatus {
    /// Current native parent, separate from a historical receipt's parent.
    /// 当前原生父级，独立于历史回执父级。
    pub runtime_id: String,
    /// Current in-memory predecessor, refreshed on attachment and each acknowledged policy change.
    /// 当前内存前驱，在附加及每次已确认策略变化时更新。
    pub revision: String,
    /// Effective rules; a pending publication does not substitute its candidate here.
    /// 生效规则；待确认发布不会在此替换为候选。
    pub rules: Vec<VulcanPluginAutomaticPrewarmRule>,
    /// Original retained receipt, including an earlier runtime's committed or interrupted evidence.
    /// 原保留回执，包含较早运行时的已提交或中断证据。
    pub update: Option<VulcanPluginPrewarmPolicyRecord>,
    /// A retained capacity transaction must be resolved before starting another policy mutation.
    /// 开始另一策略修改之前必须处理保留的容量事务。
    pub capacity_update_retained: bool,
}

/// Validate a stored record against committed settings without executing or accepting an uncertain checkpoint.
/// 对照已提交设置校验存储记录，不执行或接纳不确定检查点。
pub(super) fn validate_record(
    record: &VulcanPluginPrewarmPolicyRecord,
    settings: &VulcanPluginRuntimeSettings,
) -> Result<(), String> {
    record.request.candidate(settings)?;
    uuid::Uuid::parse_str(&record.result_revision)
        .map_err(|_| "Prewarm policy successor must be a UUID.".to_owned())?;
    if record.result_revision == record.request.expected_revision {
        return Err("Prewarm policy successor must differ from its predecessor.".into());
    }
    if record.persistence_error.is_some()
        || record.phase == VulcanPluginPrewarmPolicyPhase::Publishing
    {
        return Err(
            "A durable prewarm policy record cannot claim an unacknowledged publication.".into(),
        );
    }
    if record.phase == VulcanPluginPrewarmPolicyPhase::Committed
        && record.request.candidate(settings)? != *settings
    {
        return Err("Committed prewarm policy differs from its retained request.".into());
    }
    Ok(())
}

impl PolicyStore {
    /// Observe effective policy under its owner's lock; runtime_id identifies the current native owner.
    /// 在所有者锁下观测生效策略；runtime_id 标识当前原生所有者。
    pub(in crate::runtime) fn prewarm_status(
        &self,
        runtime_id: &str,
    ) -> VulcanPluginPrewarmPolicyStatus {
        VulcanPluginPrewarmPolicyStatus {
            runtime_id: runtime_id.into(),
            revision: self.prewarm_revision.clone(),
            rules: self.settings.automatic_prewarm.clone(),
            update: self.prewarm_update.clone(),
            capacity_update_retained: self.update.is_some(),
        }
    }

    /// Begin request under the original settings writer; return one receipt without executing any Lua.
    /// 在原设置写入器下开始 request；返回单个回执，不执行任何 Lua。
    pub(in crate::runtime) fn begin_prewarm(
        &mut self,
        root: &Path,
        runtime_id: &str,
        request: VulcanPluginPrewarmPolicyUpdate,
    ) -> Result<VulcanPluginPrewarmPolicyRecord, String> {
        if self.update.is_some() {
            return Err("A capacity policy receipt is retained; acknowledge it before another policy update.".into());
        }
        if let Some(record) = &self.prewarm_update {
            return if record.request == request {
                Ok(record.clone())
            } else {
                Err("A prewarm policy receipt is retained; observe or acknowledge it before another update.".into())
            };
        }
        request.candidate(&self.settings)?;
        if request.runtime_id != runtime_id || request.expected_revision != self.prewarm_revision {
            return Err(
                "Prewarm policy predecessor belongs to another runtime or revision.".into(),
            );
        }
        self.acquire(root)?;
        self.prewarm_update = Some(VulcanPluginPrewarmPolicyRecord {
            request,
            phase: VulcanPluginPrewarmPolicyPhase::Prepared,
            result_revision: uuid::Uuid::new_v4().to_string(),
            error: None,
            persistence_error: None,
        });
        self.resume_prewarm(root, runtime_id)
    }

    /// Resume only the retained publication at root; runtime replacement marks old uncommitted intent interrupted.
    /// 仅恢复 root 下保留的发布；运行时替换将旧未提交意图标记为中断。
    pub(in crate::runtime) fn resume_prewarm(
        &mut self,
        root: &Path,
        runtime_id: &str,
    ) -> Result<VulcanPluginPrewarmPolicyRecord, String> {
        // Clone the exact receipt before fallible publication so every returned checkpoint has one owner.
        // 在可能失败的发布前克隆精确回执，使每个返回检查点具有唯一归属。
        let mut record = self
            .prewarm_update
            .clone()
            .ok_or("Prewarm policy receipt is not retained.")?;
        if !record.phase.is_terminal() && record.request.runtime_id != runtime_id {
            record.phase = VulcanPluginPrewarmPolicyPhase::Interrupted;
            record.error = Some("Original runtime ended before durable prewarm policy completion; no initialization was replayed.".into());
        }
        record.persistence_error = None;
        if record.phase.is_terminal() {
            return Ok(self.publish_prewarm(root, self.settings.clone(), record));
        }
        if record.phase == VulcanPluginPrewarmPolicyPhase::Prepared {
            record = self.publish_prewarm(root, self.settings.clone(), record);
            if record.persistence_error.is_some() {
                return Ok(record);
            }
            record.phase = VulcanPluginPrewarmPolicyPhase::Publishing;
            self.prewarm_update = Some(record.clone());
        }
        // Publishing never rolls back to prepared: a failed final replace may already be durable.
        // 发布中绝不退回准备：失败的最终替换可能已经持久生效。
        let candidate = record.request.candidate(&self.settings)?;
        let mut committed = record.clone();
        committed.phase = VulcanPluginPrewarmPolicyPhase::Committed;
        committed.persistence_error = None;
        // Keep the publishing checkpoint in memory until the whole settings+receipt image is acknowledged.
        // 完整设置加回执映像确认前，在内存保留发布中检查点。
        let document = self.prewarm_document(candidate.clone(), Some(committed.clone()));
        match self.persist(root, &document) {
            Ok(()) => {
                self.settings = candidate;
                self.prewarm_revision.clone_from(&committed.result_revision);
                self.prewarm_update = Some(committed.clone());
                Ok(committed)
            }
            Err(error) => {
                record.persistence_error = Some(error);
                self.prewarm_update = Some(record.clone());
                Ok(record)
            }
        }
    }

    /// Cancel an unpublished prepared intent only; uncertain final publication must be reconciled first.
    /// 仅取消尚未最终发布的准备意图；最终发布不确定时必须先对账。
    pub(in crate::runtime) fn cancel_prewarm(
        &mut self,
        root: &Path,
    ) -> Result<VulcanPluginPrewarmPolicyRecord, String> {
        let mut record = self
            .prewarm_update
            .clone()
            .ok_or("Prewarm policy receipt is not retained.")?;
        if record.phase == VulcanPluginPrewarmPolicyPhase::Publishing {
            return Err("Prewarm policy publication is uncertain; resume the original acknowledgement before cancellation.".into());
        }
        if record.phase == VulcanPluginPrewarmPolicyPhase::Prepared {
            record.phase = VulcanPluginPrewarmPolicyPhase::Cancelled;
            record.error = None;
        }
        record.persistence_error = None;
        Ok(self.publish_prewarm(root, self.settings.clone(), record))
    }

    /// Forget an acknowledged terminal receipt; a failed removal keeps its evidence for an exact retry.
    /// 遗忘已确认终态回执；移除失败时保留证据供精确重试。
    pub(in crate::runtime) fn forget_prewarm(&mut self, root: &Path) -> Result<(), String> {
        let record = self
            .prewarm_update
            .as_ref()
            .ok_or("Prewarm policy receipt is not retained.")?;
        if !record.phase.is_terminal() || record.persistence_error.is_some() {
            return Err("Prewarm policy receipt is not durably terminal.".into());
        }
        let document = self.prewarm_document(self.settings.clone(), None);
        self.persist(root, &document)?;
        self.prewarm_update = None;
        Ok(())
    }

    /// Build the one authoritative settings document while retaining the mutually exclusive capacity field.
    /// 构造唯一权威设置文档，同时保留互斥容量字段。
    fn prewarm_document(
        &self,
        settings: VulcanPluginRuntimeSettings,
        record: Option<VulcanPluginPrewarmPolicyRecord>,
    ) -> PolicyDocument {
        let mut document = PolicyDocument::new(settings, self.update.clone());
        document.prewarm_update = record;
        document
    }

    /// Publish settings with record and retain any uncertainty in memory; no runtime work is dispatched.
    /// 发布 settings 与 record，并在内存保留不确定性；不派发运行时工作。
    fn publish_prewarm(
        &mut self,
        root: &Path,
        settings: VulcanPluginRuntimeSettings,
        mut record: VulcanPluginPrewarmPolicyRecord,
    ) -> VulcanPluginPrewarmPolicyRecord {
        let document = self.prewarm_document(settings, Some(record.clone()));
        if let Err(error) = self.persist(root, &document) {
            record.persistence_error = Some(error);
        }
        self.prewarm_update = Some(record.clone());
        record
    }
}
