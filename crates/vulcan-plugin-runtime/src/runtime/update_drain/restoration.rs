//! Sealed predecessor restoration retains physical old authority without retaining request readiness.
//! 密封前驱恢复保留物理旧权威，但不保留请求就绪租约。

use super::*;
use crate::{
    VulcanPluginActivationEffect as Effect, VulcanPluginEffectObservation as Observation,
    VulcanPluginGenerationStore,
};

/// Exact old callback evidence, derived only from its actual immutable declaration and receipt.
/// 精确旧回调证据，仅从其实际不可变声明及回执派生。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanPluginPredecessorRestoreEvidence {
    /// The declared old enable callback completed successfully under this original request.
    /// 已声明旧启用回调在此原请求下成功完成。
    RestoreSucceeded,
    /// The old manifest declares no enable callback, so no callback receipt is fabricated.
    /// 旧清单未声明启用回调，因此不伪造回调回执。
    NoDeclaration,
}

/// Non-cloneable restoration authority owns the actual drain and its immutable old package/configuration/credentials.
/// 不可克隆恢复权威拥有实际排空句柄及其不可变旧包、配置、凭据。
#[derive(Debug)]
pub struct VulcanPluginPredecessorRestoration {
    /// Actual original owner; completed drain released its effect attempt and request lock.
    /// 实际原所有者；已完成排空释放了副作用尝试及请求锁。
    pub(in crate::runtime) drain: VulcanPluginPredecessorDrain,
    /// Typed callback proof never grants target commit or ordinary enable authority.
    /// 类型化回调证明绝不授予目标提交或普通启用权限。
    evidence: VulcanPluginPredecessorRestoreEvidence,
    /// Exact closed Arc is retained for future full publisher preparation, without reopening any surface.
    /// 保留精确关闭 Arc，供未来完整发布器准备，不重开任何暴露面。
    pub(in crate::runtime) staged: Option<Arc<VulcanPluginCatalogSnapshot>>,
}

impl VulcanPluginPredecessorRestoration {
    /// Return this restoration's original request identity; no execution authority is allocated.
    /// 返回此恢复的原请求身份；不分配执行权限。
    pub fn request_id(&self) -> Uuid {
        self.drain.request_id()
    }

    /// Return checked callback evidence; no declaration and successful invocation remain distinct.
    /// 返回已校验回调证据；未声明与成功调用保持区分。
    pub fn evidence(&self) -> VulcanPluginPredecessorRestoreEvidence {
        self.evidence
    }

    /// Borrow the actually retained closed staging Arc, or None before confirmed original abort.
    /// 借用实际保留的关闭暂存 Arc，或在确认原中止前返回 None。
    pub fn staged_catalog(&self) -> Option<&Arc<VulcanPluginCatalogSnapshot>> {
        self.staged.as_ref()
    }

    /// Borrow the exact frozen old descriptor for crate-local staging; retain all actual physical pins.
    /// 借用精确固定旧描述符供 crate 内暂存；保留全部实际物理固定句柄。
    pub(in crate::runtime) fn predecessor(&self) -> &VulcanPluginDescriptor {
        &self.drain.authority.predecessor
    }

    /// Borrow the original immutable transaction record; it is never reconstructed from a public plugin ID.
    /// 借用原不可变事务记录；绝不从公开插件 ID 重建。
    pub(in crate::runtime) fn original(&self) -> &VulcanPluginActivationTransactionRecord {
        &self.drain.authority.original
    }
}

/// Failed preparation returns the actual owned drain so ordinary errors cannot discard recovery authority.
/// 准备失败返回实际自有排空句柄，避免普通错误丢失恢复权威。
#[derive(Debug)]
pub struct VulcanPluginPredecessorRestorationFailure {
    /// Explicit reason; no hidden retry or callback replay is performed.
    /// 明确原因；不执行隐藏重试或回调重放。
    pub error: VulcanPluginRuntimeError,
    /// Same actual drain supplied by the caller, including its old physical pins.
    /// 调用方提供的相同实际排空句柄，包含旧物理固定句柄。
    pub drain: VulcanPluginPredecessorDrain,
}

impl VulcanPluginRuntimeManager {
    /// Consume drain only after validating candidate's exact original and actual completed old restoration.
    /// 仅在校验 candidate 精确原请求及实际已完成旧恢复后消费 drain。
    /// Return sealed authority or the same owned drain with an explicit error; never invoke any callback.
    /// 返回密封权威，或携带明确错误的相同自有排空句柄；绝不调用任何回调。
    pub fn prepare_activation_predecessor_restoration(
        &self,
        candidate: &VulcanPluginActivationCandidate<'_>,
        drain: VulcanPluginPredecessorDrain,
    ) -> Result<VulcanPluginPredecessorRestoration, VulcanPluginPredecessorRestorationFailure> {
        // This temporary candidate proof is dropped on return and never enters the restoration credential.
        // 此临时候选证明在返回时释放，绝不进入恢复凭据。
        let result = (|| {
            drain.predecessor_for_callback(&self.state, candidate)?;
            // Serialize this final actual-owner observation with all lifecycle admission.
            // 将最终实际所有者观测与全部生命周期入场串行化。
            let _registration = self
                .state
                .session_registration_gate
                .lock()
                .map_err(|_| error("Plugin registration gate is poisoned."))?;
            validate_predecessor_callback_current(
                &self.state,
                &drain.authority.predecessor,
                drain.identity(),
            )?;
            self.require_restoration_execution_retired(drain.request_id())?;
            // Open only the original configured store; absence cannot become a replacement authority.
            // 仅打开原配置存储；缺失不能成为替代权威。
            let store = VulcanPluginGenerationStore::open_existing(&self.state.user_plugin_root)
                .map_err(|failure| error(failure.to_string()))?
                .ok_or_else(|| error("Original update store is missing."))?;
            validate_restoration_receipts(&store, &drain)
        })();
        match result {
            Ok(evidence) => Ok(VulcanPluginPredecessorRestoration {
                drain,
                evidence,
                staged: None,
            }),
            Err(error) => Err(VulcanPluginPredecessorRestorationFailure { error, drain }),
        }
    }

    /// Require actual old and candidate calls plus physical native domains for request_id to have retired.
    /// 要求 request_id 实际旧调用、候选调用及物理原生域均已退役。
    /// Caller holds registration exclusion; return an error without allocating a replacement runtime.
    /// 调用方持有注册互斥；返回错误，不分配替代运行时。
    pub(in crate::runtime) fn require_restoration_execution_retired(
        &self,
        request_id: Uuid,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if !self.activation_predecessor_calls(request_id)?.is_empty()
            || !self.activation_candidate_calls(request_id)?.is_empty()
            || match self.formal_runtime_owner()? {
                Some(owner) => owner
                    .host
                    .retains_activation_request(request_id)
                    .map_err(|failure| error(failure.to_string()))?,
                None => false,
            }
        {
            return Err(error(
                "Predecessor restoration requires actual original execution retirement.",
            ));
        }
        Ok(())
    }
}

/// Validate exact old declaration/configuration and original effect receipts from store and drain.
/// 从 store 与 drain 校验精确旧声明、配置及原副作用回执。
/// Return typed restoration evidence; Started, unknown and failed restore observations never become success.
/// 返回类型化恢复证据；Started、未知及失败恢复观测绝不变为成功。
pub(in crate::runtime) fn validate_restoration_receipts(
    store: &VulcanPluginGenerationStore,
    drain: &VulcanPluginPredecessorDrain,
) -> Result<VulcanPluginPredecessorRestoreEvidence, VulcanPluginRuntimeError> {
    // The frozen old manifest is the sole declaration authority; target defaults cannot substitute.
    // 固定旧清单是唯一声明权威；目标默认值不能替代。
    let predecessor = &drain.authority.predecessor;
    // Read declaration facts directly from the retained old package.
    // 直接从保留旧包读取声明事实。
    let manifest = predecessor
        .manifest
        .as_ref()
        .ok_or_else(|| error("Original predecessor manifest is missing."))?;
    if manifest.config.is_some() {
        // configuration.rs reads mutable legacy state when selected configuration is absent; exact restoration must reject that path.
        // configuration.rs 在选定配置缺失时读取可变旧状态；精确恢复必须拒绝该路径。
        let selection = predecessor.managed_package().and_then(|managed| managed.configuration())
            .ok_or_else(|| error("Exact predecessor restoration requires immutable old configuration; mutable legacy configuration is unsupported."))?;
        if selection.value().is_none() {
            return Err(error(
                "Declared predecessor configuration has no immutable document.",
            ));
        }
    }
    // No operation may still be executing or uncertain about termination when old routes are prepared.
    // 准备旧路由时，任何操作均不得仍在执行或终止状态不确定。
    let effects = store
        .activation_effects(&predecessor.id, drain.request_id())
        .map_err(|failure| error(failure.to_string()))?
        .ok_or_else(|| error("Original restoration effects are missing."))?;
    // Preserve failed and Started values exactly for explicit acceptance decisions below.
    // 精确保留失败与 Started 值，供下方明确接纳决策使用。
    let observations = effects.observations();
    if effects.predecessor_drain_identity() != Some(drain.identity())
        || !observations.contains(&(Effect::DrainPredecessorLua, Observation::Succeeded))
        || observations
            .iter()
            .any(|(_, observation)| *observation == Observation::Started)
        || store
            .installation_receipt(&predecessor.id, drain.request_id())
            .map_err(|failure| error(failure.to_string()))?
            .is_some_and(|receipt| receipt.phase() == Observation::Started)
    {
        return Err(error(
            "Original restoration requires confirmed drain and no Started effects.",
        ));
    }
    if observations
        .iter()
        .any(|(effect, _)| *effect == Effect::Enable)
        && !observations.contains(&(Effect::EnableRollback, Observation::Succeeded))
    {
        return Err(error(
            "Attempted target enable requires successful original enable rollback before restoration.",
        ));
    }
    // A failed disable may have partial external effects; only successful declared old restore authorizes staging.
    // 失败停用可能已产生部分外部副作用；仅成功的已声明旧恢复授权暂存。
    match manifest
        .lua_runtime
        .as_ref()
        .and_then(|runtime| runtime.lifecycle.enable.as_ref())
    {
        Some(_) if observations.contains(&(Effect::RestorePredecessor, Observation::Succeeded)) => {
            Ok(VulcanPluginPredecessorRestoreEvidence::RestoreSucceeded)
        }
        Some(_) => Err(error(
            "Declared predecessor restore is not confirmed successful.",
        )),
        None if observations
            .iter()
            .any(|(effect, _)| *effect == Effect::RestorePredecessor) =>
        {
            Err(error(
                "Undeclared predecessor restore cannot have callback evidence.",
            ))
        }
        None => Ok(VulcanPluginPredecessorRestoreEvidence::NoDeclaration),
    }
}
