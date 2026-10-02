//! A successful original old Full publisher seals exact catalogs while Pending still denies business.
//! 成功原旧 Full 发布器封闭精确目录，同时 Pending 仍拒绝业务。

use super::*;
use crate::{
    VulcanPluginActivationTransaction, VulcanPluginActivationTransactionPhase,
    VulcanPluginPredecessorRestorationSource,
};

mod publication;
pub use publication::VulcanPluginActivationPredecessorPublication;

impl VulcanPluginRuntimeManager {
    /// Publish source's exact restored old catalog through one synchronous complete-extension publisher.
    /// 通过单个同步完整扩展发布器发布 source 精确恢复旧目录。
    /// transaction must be the full original Aborted record; source must retain its actual closed route and Pending File.
    /// transaction 必须为完整原 Aborted 记录；source 必须保留其实际关闭路由及 Pending 文件。
    /// transaction supplies an exact record observation only; this interface does not independently validate its root or File.
    /// transaction 仅提供精确记录观测；此接口不独立校验其根目录或文件。
    /// The same actual source remains the sole manager, storage-owner and Full-publication authority.
    /// 相同实际 source 保持为唯一管理器、存储所有者及 Full 发布权威。
    /// publisher receives the same provisional Arc and must not reenter runtime admission.
    /// publisher 接收相同临时 Arc，不得重新进入运行时入场。
    /// Replacing the Full pointer must be the callback's last step: returning success cannot be followed by fallible work.
    /// 替换 Full 指针必须为回调最后一步：返回成功后不得执行可能失败的工作。
    /// Return sealed actual publication after success; failure restores the exact closed Arc, without granting business.
    /// 成功后返回封闭实际发布；失败恢复精确关闭 Arc，不授予业务。
    pub fn publish_activation_predecessor_handoff(
        &self,
        transaction: &VulcanPluginActivationTransaction,
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
        publisher: impl FnOnce(Arc<VulcanPluginCatalogSnapshot>) -> Result<(), String>,
    ) -> Result<VulcanPluginActivationPredecessorPublication, VulcanPluginRuntimeError> {
        // The same registration gate excludes ordinary lifecycle and competing catalog publication.
        // 相同注册门排除普通生命周期及竞争目录发布。
        let _registration =
            self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
        self.require_routing_publication_locked(
            &_registration,
            Some(crate::runtime::routing::ActivationRoutingOriginal::Transaction(transaction)),
        )?;
        if transaction.record().phase() != VulcanPluginActivationTransactionPhase::Aborted
            || transaction.record() != source.original()
        {
            return Err(VulcanPluginRuntimeError::new(
                "Predecessor publication requires its full original aborted transaction.",
            ));
        }
        source.validate(&self.state)?;
        // Only this physical source supplies old identity; the aborted target never supplies executable bytes.
        // 仅此物理来源提供旧身份；中止目标绝不提供可执行字节。
        let plugin_id = &source.plugin().id;
        // Keep the actual registered old Hook rather than substituting prewarm or a replacement VM.
        // 保留实际已登记旧 Hook，而非替换为预热或另一 VM。
        let private_hook = self.activation_predecessor_hook_for_publication(source)?;
        // Hold its same actual evidence until publication acceptance is recorded without fallible work.
        // 持有其相同实际证据，直至不执行可能失败工作地记录发布接纳。
        let private_publication = private_hook
            .as_ref()
            .map(|runtime| runtime.private_publication_guard())
            .transpose()?;
        // Clone only the exact original closed catalog, preserving every unrelated namespace and its route.
        // 仅克隆精确原关闭目录，保留每个无关命名空间及其路由。
        let closed = source.staged_catalog();
        // This local projection changes state only, never old configuration, credentials or routing generation.
        // 此局部投影仅改变状态，绝不改变旧配置、凭据或路由代次。
        let mut next = (**closed).clone();
        next.plugins
            .get_mut(plugin_id)
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Original closed predecessor is missing.")
            })?
            .runtime_state = VulcanPluginRuntimeState::Enabled;
        rebuild_enabled_surface(&mut next);
        next.revision = self.reserve_activation_catalog_revision()?;
        // Allocate the complete final value and sealed witness before the external pointer can be changed.
        // 在外部指针可以改变前分配完整最终值及封闭凭据。
        let next = Arc::new(next);
        // This witness is returned only after the same synchronous publisher actually accepts it.
        // 仅相同同步发布器实际接纳后才返回此凭据。
        let publication = VulcanPluginActivationPredecessorPublication::new(
            self,
            transaction,
            source,
            closed,
            &next,
        );
        {
            // Prepare every fallible manager lock before replacing the provisional pointer.
            // 替换临时指针前准备每个可能失败的管理器锁。
            let mut overrides =
                self.state.lifecycle_overrides.lock().map_err(|_| {
                    VulcanPluginRuntimeError::new("Plugin lifecycle lock is poisoned.")
                })?;
            // Hold the actual snapshot write guard only for replacement, never across callback reads.
            // 仅为替换持有实际快照写守卫，绝不跨回调读取存活。
            let mut current =
                self.state.snapshot.write().map_err(|_| {
                    VulcanPluginRuntimeError::new("Plugin catalog lock is poisoned.")
                })?;
            if !Arc::ptr_eq(&current, closed)
                || overrides.get(plugin_id) != Some(&VulcanPluginRuntimeState::Enabling)
            {
                return Err(VulcanPluginRuntimeError::new(
                    "Original closed predecessor changed before full publication.",
                ));
            }
            overrides.remove(plugin_id);
            *current = Arc::clone(&next);
        }
        if let Err(publication_error) = publisher(Arc::clone(&next)) {
            // Registration still excludes normal writers; failure returns to this exact original closed state.
            // 注册仍排除普通写入器；失败返回此精确原关闭状态。
            let mut overrides = self.state.lifecycle_overrides.lock().map_err(|_| {
                VulcanPluginRuntimeError::new(format!(
                    "{publication_error}; closed predecessor lifecycle restoration failed."
                ))
            })?;
            // Acquire both rollback guards before restoring either original projection.
            // 恢复任一原投影之前取得两个回退守卫。
            let mut current = self.state.snapshot.write().map_err(|_| {
                VulcanPluginRuntimeError::new(format!(
                    "{publication_error}; closed predecessor catalog restoration failed."
                ))
            })?;
            overrides.insert(plugin_id.clone(), VulcanPluginRuntimeState::Enabling);
            *current = Arc::clone(closed);
            return Err(VulcanPluginRuntimeError::new(publication_error));
        }
        if let Some(private_publication) = private_publication {
            private_publication.published(&next);
        }
        // Pending and the retained private origin remain authoritative; successful Full alone opens no business.
        // Pending 及保留私有来源继续具有权威；仅成功 Full 不开放任何业务。
        Ok(publication)
    }
}
