//! One successful full publisher binds exact closed and final catalogs to the original request.
//! 单次成功完整发布器将精确关闭及最终目录绑定至原请求。

use super::*;
use crate::runtime::VulcanPluginManagerState;

/// Sealed successful publication keeps the actual closed rollback catalog and final route together.
/// 封闭成功发布将实际关闭回退目录及最终路由共同保留。
/// Only the successful synchronous publisher issues this value; it grants no independent business admission.
/// 仅成功同步发布器签发此值；它不授予独立业务入场。
pub struct VulcanPluginActivationTargetPublication {
    /// Actual manager state identifies this publication without retaining another runtime owner.
    /// 实际管理器状态标识此发布，不保留另一运行时所有者。
    manager: std::sync::Weak<VulcanPluginManagerState>,
    /// Complete original record remains the sole target and request authority.
    /// 完整原记录保持为唯一目标及请求权威。
    original: crate::VulcanPluginActivationTransactionRecord,
    /// Original route was validated before the publisher executed.
    /// 原路由在发布器执行之前已校验。
    generation: u64,
    /// Exact current closed Arc includes every namespace present immediately before publication.
    /// 精确当前关闭 Arc 包含发布前即时存在的每个命名空间。
    closed: Arc<VulcanPluginCatalogSnapshot>,
    /// Exact final Arc is the same value accepted by the complete extension publisher.
    /// 精确最终 Arc 为完整扩展发布器接受的相同值。
    catalog: Arc<VulcanPluginCatalogSnapshot>,
}

impl VulcanPluginActivationTargetPublication {
    /// Prepare private identity from validated original owners before publication; caller returns it only after success.
    /// 发布之前从已校验原所有者准备私有身份；调用方仅在成功之后返回它。
    /// manager/transaction/closed/catalog are the exact existing objects; no request lock or VM is created.
    /// manager／transaction／closed／catalog 为精确既有对象；不创建请求锁或 VM。
    pub(super) fn new(
        manager: &VulcanPluginRuntimeManager,
        transaction: &crate::VulcanPluginActivationTransaction,
        closed: &Arc<VulcanPluginCatalogSnapshot>,
        catalog: &Arc<VulcanPluginCatalogSnapshot>,
    ) -> Self {
        Self {
            manager: Arc::downgrade(&manager.state),
            original: transaction.record().clone(),
            generation: closed.plugins[&transaction.record().target().plugin_id].generation,
            closed: Arc::clone(closed),
            catalog: Arc::clone(catalog),
        }
    }

    /// Borrow the actual final catalog for read-only extension inspection without implicit publication conversion.
    /// 借用实际最终目录以只读检查扩展，不进行隐式发布转换。
    pub fn catalog(&self) -> &Arc<VulcanPluginCatalogSnapshot> {
        &self.catalog
    }

    /// Validate the exact manager and full original record under caller-held registration exclusion.
    /// 在调用方持有注册互斥时校验精确管理器及完整原记录。
    pub(in crate::runtime) fn matches_owner(
        &self,
        manager: &VulcanPluginRuntimeManager,
        transaction: &crate::VulcanPluginActivationTransaction,
    ) -> bool {
        std::sync::Weak::ptr_eq(&self.manager, &Arc::downgrade(&manager.state))
            && &self.original == transaction.record()
    }

    /// Borrow the sole actual rollback Arc; callers never infer it from private initialization or clone the final catalog.
    /// 借用唯一实际回退 Arc；调用方绝不从私有初始化推断它或克隆最终目录。
    pub(in crate::runtime) fn closed_catalog(&self) -> &Arc<VulcanPluginCatalogSnapshot> {
        &self.closed
    }

    /// Return the exact original route generation validated before this successful full publication.
    /// 返回此次成功完整发布之前已校验的精确原路由代次。
    pub(in crate::runtime) fn generation(&self) -> u64 {
        self.generation
    }
}

impl std::fmt::Debug for VulcanPluginActivationTargetPublication {
    /// Format only bounded non-secret publication identity; formatter receives no complete private selection bytes.
    /// 仅格式化有界无秘密发布身份；formatter 不接收完整私有选择字节。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VulcanPluginActivationTargetPublication")
            .field("request_id", &self.original.request_id())
            .field("generation", &self.generation)
            .finish()
    }
}
