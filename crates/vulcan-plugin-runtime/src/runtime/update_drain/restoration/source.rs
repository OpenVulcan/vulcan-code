//! One original restoration source retains its independent physical owner before private resource preparation.
//! 单个原恢复来源在私有资源准备前保留其独立物理所有者。

use super::*;
use crate::runtime::VulcanPluginManagerState;
use crate::{VulcanPluginActivationTransaction, VulcanPluginActivationTransactionPhase as Phase};
use std::sync::Weak;

/// Opaque physical predecessor source owns existing old pins and its independent reservation without a manager cycle.
/// 不透明物理前驱来源拥有既有旧固定句柄及独立预留权，不形成管理器循环。
/// Only the original manager signs it after exact abort staging; it grants private preparation, never business or transfer.
/// 仅原管理器在精确中止暂存后签发；它授予私有准备，绝不授予业务或转交。
pub struct VulcanPluginPredecessorRestorationSource {
    /// Weak allocation identity distinguishes this actual manager without retaining its execution parent.
    /// 弱分配身份区分此实际管理器，不保留其执行父级。
    manager: Weak<VulcanPluginManagerState>,
    /// Complete confirmed Aborted image preserves schema, preparation, policy and both immutable selections.
    /// 完整已确认 Aborted 映像保留模式、准备、策略及两个不可变选择。
    original: VulcanPluginActivationTransactionRecord,
    /// The exact restoration successor descriptor retains old code, configuration and credential pins.
    /// 精确恢复后继描述符保留旧代码、配置及凭据固定句柄。
    plugin: VulcanPluginDescriptor,
    /// Actual closed staging allocation supplies this source's only route authority.
    /// 实际关闭暂存分配提供此来源唯一的路由权威。
    staged: Arc<VulcanPluginCatalogSnapshot>,
    /// This is the same existing independent owner, never a cloned activation readiness File.
    /// 此为相同既有独立所有者，绝非克隆的激活就绪文件。
    owner: Arc<private_owner::PredecessorRestorationPrivateOwner>,
    /// The actual old Hook consumes one attempt even if preparation fails or its pool later retires.
    /// 实际旧 Hook 消费一次尝试，即使准备失败或其池随后退役。
    hook_attempted: AtomicBool,
    /// Hook preparation moves this shared environment File into its actual core domain exactly once.
    /// Hook 准备将此共享环境文件恰好一次移入其实际核心域。
    hook_environment: Mutex<Option<std::fs::File>>,
    /// The source fixes one MCP control; its weak physical link cannot recreate retired resources.
    /// 来源固定单个 MCP 控制；其弱物理链接不能重建已退役资源。
    pub(in crate::runtime) mcp: Mutex<Option<crate::VulcanPluginMcpPredecessorControl>>,
    /// Only this source's factory waits here; dependency preparation never occupies global registration.
    /// 仅此来源的工厂在此等待；依赖准备绝不占据全局注册。
    pub(in crate::runtime) mcp_preparation: Mutex<()>,
}

impl std::fmt::Debug for VulcanPluginPredecessorRestorationSource {
    /// Format only non-secret original identity for formatter; returns its redacted formatting result.
    /// 为 formatter 仅格式化非秘密原身份；返回脱敏格式化结果。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VulcanPluginPredecessorRestorationSource")
            .field("request_id", &self.request_id())
            .field("generation", &self.plugin.generation)
            .finish()
    }
}

impl VulcanPluginPredecessorRestorationSource {
    /// Borrow its original independent owner for prepared same-resource transfer; no new File or reader is opened.
    /// 为已准备同资源转交借用其原独立所有者；不打开新文件或读者。
    /// No inputs; return a guard retaining the same reservation across Pending/Transferred reconciliation.
    /// 无输入；返回跨 Pending／Transferred 对账保留相同预留权的守卫。
    pub(in crate::runtime) fn prepare_owner_transfer(
        &self,
    ) -> Result<private_owner::PredecessorRestorationTransfer<'_>, VulcanPluginRuntimeError> {
        self.owner.prepare_transfer(self.original())
    }

    /// Return the exact original request identity for observation without granting execution or reopening a file.
    /// 返回精确原请求身份供观察，不授予执行或重开文件。
    /// No inputs; the copied UUID cannot replace this actual owning source.
    /// 无输入；复制的 UUID 不能替代此实际自有来源。
    pub fn request_id(&self) -> Uuid {
        self.original.request_id()
    }

    /// Borrow the original recovery successor descriptor; no public discovery or target descriptor is substituted.
    /// 借用原恢复后继描述符；不替换为公开发现或目标描述符。
    /// No inputs; the reference remains owned by this same physical source.
    /// 无输入；引用保持由此相同物理来源拥有。
    pub(in crate::runtime) fn plugin(&self) -> &VulcanPluginDescriptor {
        &self.plugin
    }

    /// Borrow the complete confirmed aborted original for internal participant preparation.
    /// 借用完整已确认中止原请求，供内部参与者准备。
    /// No inputs; this record is observation and never substitutes for its independent physical owner.
    /// 无输入；此记录为观察，绝不替代其独立物理所有者。
    pub(in crate::runtime) fn original(&self) -> &VulcanPluginActivationTransactionRecord {
        &self.original
    }

    /// Borrow the exact closed staging Arc before any private preparation or complete extension publication.
    /// 在任何私有准备或完整扩展发布之前，借用精确关闭暂存 Arc。
    /// No inputs; cloning the Arc preserves actual route identity without minting another source.
    /// 无输入；克隆 Arc 保留实际路由身份，不签发另一来源。
    pub(in crate::runtime) fn staged_catalog(&self) -> &Arc<VulcanPluginCatalogSnapshot> {
        &self.staged
    }

    /// Validate closed source preparation using the actual manager allocation, route and Pending file.
    /// 使用实际管理器分配、路由及 Pending 文件校验关闭来源准备。
    /// state is the original manager state; returns a conflict without replay, publication or mutable bookkeeping.
    /// state 为原管理器状态；返回冲突，不重放、发布或变更记录。
    pub(in crate::runtime) fn validate(
        &self,
        state: &VulcanPluginManagerState,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if !std::ptr::eq(self.manager.as_ptr(), state)
            || self.staged_catalog().service_epoch != state.manager_id
        {
            return Err(error(
                "Private predecessor source belongs to another manager.",
            ));
        }
        // A same-field catalog clone cannot replace the exact original restoration route allocation.
        // 相同字段目录的克隆不能替代精确原恢复路由分配。
        let current = state
            .snapshot
            .read()
            .map_err(|_| error("Plugin catalog lock is poisoned."))?;
        if !Arc::ptr_eq(&current, self.staged_catalog()) {
            return Err(error(
                "Private predecessor source requires its exact closed catalog.",
            ));
        }
        // The old selection comes from predecessor; target identity never authorizes restoration.
        // 旧选择来自 predecessor；目标身份绝不授权恢复。
        let old = self
            .original()
            .predecessor()
            .ok_or_else(|| error("Private predecessor source requires a managed old selection."))?;
        if self.plugin().origin != VulcanPluginPackageOrigin::UserInstalled
            || self.plugin().runtime_state != VulcanPluginRuntimeState::Enabling
            || self.plugin().availability != crate::VulcanPluginAvailability::Enabled
            || !self.plugin().desired_enabled
            || !self.plugin().runtime_execution_available
            || self.plugin().generation == 0
            || old.configuration().is_none()
            || old.credential_revision().is_none()
            || !self
                .plugin()
                .managed_package()
                .is_some_and(|managed| managed.matches_activation(old))
        {
            return Err(error(
                "Private predecessor source differs from its closed old managed route.",
            ));
        }
        // Exact original Aborted and Pending remain independently validated while the same File is held.
        // 持有相同文件时，分别继续校验精确原 Aborted 及 Pending。
        self.validate_pending_owner()
    }

    /// Validate this same independent File against exact original Aborted and Pending records.
    /// 按精确原 Aborted 及 Pending 记录校验此同一独立文件。
    /// No inputs; return only owner validation, without changing closed-source preparation eligibility.
    /// 无输入；仅返回所有者校验，不改变关闭来源准备资格。
    pub(in crate::runtime) fn validate_pending_owner(
        &self,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.owner.validate_pending(self.original())
    }

    /// Validate plugin's exact retained executable selection before a private Hook capability or registration.
    /// 私有 Hook 能力或注册前，校验 plugin 的精确保留可执行选择。
    /// state is the actual creating manager; returns a closed-source error without consulting target permissions.
    /// state 为实际创建管理器；返回关闭来源错误，不查询目标权限。
    pub(in crate::runtime) fn validate_hook(
        &self,
        state: &VulcanPluginManagerState,
        plugin: &VulcanPluginDescriptor,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.validate(state)?;
        if !self
            .hook_attempted
            .load(std::sync::atomic::Ordering::Acquire)
            || plugin.id != self.plugin.id
            || plugin.generation != self.plugin.generation
            || plugin.package_root != self.plugin.package_root
            || plugin.content_fingerprint != self.plugin.content_fingerprint
            || !plugin.managed_package().is_some_and(|managed| {
                managed.matches_activation(
                    self.original
                        .predecessor()
                        .expect("validated old selection"),
                )
            })
        {
            return Err(error(
                "Private predecessor Hook differs from its retained executable selection.",
            ));
        }
        Ok(())
    }

    /// Consume the sole Hook attempt before registration; state is checked under its original registration gate.
    /// 注册前消费唯一 Hook 尝试；在 state 原注册门内校验它。
    /// Returns an explicit replay error; failure or physical retirement never resets this consumed fact.
    /// 返回明确重放错误；失败或物理退役绝不重置此已消费事实。
    pub(in crate::runtime) fn begin_hook_initialization(
        &self,
        state: &VulcanPluginManagerState,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.validate(state)?;
        self.hook_attempted
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .map_err(|_| {
                error("Original predecessor Hook initialization was already attempted.")
            })?;
        Ok(())
    }

    /// Retain the existing shared environment for this actual Hook before its core domain exists.
    /// 实际 Hook 核心域存在前，为它保留既有共享环境。
    /// state identifies the original manager; returns ownership failure without replacing an existing File.
    /// state 标识原管理器；返回所有权失败，不替换既有文件。
    pub(in crate::runtime) fn retain_hook_environment(
        &self,
        state: &VulcanPluginManagerState,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.validate_hook(state, self.plugin())?;
        // The one admitted initializer retains this same File before moving it into its actual domain.
        // 唯一已接纳初始化器在将文件移入其实际域前保留此相同文件。
        let mut environment = self
            .hook_environment
            .lock()
            .map_err(|_| error("Predecessor Hook environment lock is poisoned."))?;
        if environment.is_none() {
            *environment = Some(
                crate::runtime::environment::retain_plugin_execution_environment(
                    state,
                    &self.plugin.id,
                )
                .map_err(VulcanPluginRuntimeError::new)?,
            );
        }
        Ok(())
    }

    /// Move the same Hook preparation File into its actual domain; no inputs, and missing ownership is an error.
    /// 将相同 Hook 准备文件移入其实际域；无输入，缺失所有权即为错误。
    pub(in crate::runtime) fn take_hook_environment(
        &self,
    ) -> Result<std::fs::File, VulcanPluginRuntimeError> {
        self.hook_environment
            .lock()
            .map_err(|_| error("Predecessor Hook environment lock is poisoned."))?
            .take()
            .ok_or_else(|| error("Predecessor Hook environment ownership is missing."))
    }
}

impl VulcanPluginRuntimeManager {
    /// Sign one physical source from the same staged restoration and retain it before returning to a private initializer.
    /// 从相同已暂存恢复签发单个物理来源，在返回私有初始化器之前保留它。
    /// transaction is the exact Aborted original; restoration owns its old descriptor, closed Arc and independent reservation.
    /// transaction 为精确 Aborted 原请求；restoration 拥有旧描述符、关闭 Arc 及独立预留权。
    /// Returns the same Arc on retry; every error preserves original owners and never writes a record or opens another lease.
    /// 重试返回相同 Arc；每个错误保留原所有者，绝不写记录或打开另一租约。
    pub fn prepare_activation_predecessor_source(
        &self,
        transaction: &VulcanPluginActivationTransaction,
        restoration: &mut VulcanPluginPredecessorRestoration,
    ) -> Result<Arc<VulcanPluginPredecessorRestorationSource>, VulcanPluginRuntimeError> {
        // Source identity and first retirement proof share the existing registration exclusion.
        // 来源身份及首次退役证明共享既有注册互斥。
        let registration = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| error("Plugin registration gate is poisoned."))?;
        if transaction.record().phase() != Phase::Aborted
            || !transaction
                .record()
                .matches_predecessor_restoration_original(restoration.original())
        {
            return Err(error(
                "Private predecessor source requires its exact aborted original.",
            ));
        }
        // These are already retained physical objects; missing ownership is never reconstructed from disk identity.
        // 此处均为已保留物理对象；缺失所有权绝不从磁盘身份重建。
        let owner = restoration
            .private_owner
            .as_ref()
            .ok_or_else(|| error("Actual private predecessor owner is missing."))?;
        // The one successful abort producer retained this precise route before returning.
        // 唯一成功中止生产者在返回前保留此精确路由。
        let staged = restoration
            .staged_catalog()
            .ok_or_else(|| error("Actual closed predecessor restoration is missing."))?;
        if restoration.drain.identity().service_epoch != self.state.manager_id
            || staged.service_epoch != self.state.manager_id
        {
            return Err(error(
                "Private predecessor source belongs to another manager.",
            ));
        }
        if let Some(source) = restoration.source.as_ref() {
            if source.original() != transaction.record()
                || !Arc::ptr_eq(&source.owner, owner)
                || !Arc::ptr_eq(source.staged_catalog(), staged)
            {
                return Err(error("Original private predecessor source changed."));
            }
            // A retry can already own newly initialized private resources; it must not demand their retirement.
            // 重试可能已经拥有新初始化私有资源；不得要求它们退役。
            source.validate(&self.state)?;
            return Ok(Arc::clone(source));
        }
        // Only first signing checks old execution retirement; later private resources belong to the new source.
        // 仅首次签发检查旧执行退役；随后私有资源属于新来源。
        self.require_restoration_execution_retired(restoration.request_id())?;
        if self.has_unretired_plugin_resources_locked(
            &transaction.record().target().plugin_id,
            &registration,
        )? {
            return Err(error(
                "Old plugin resources must retire before predecessor source signing.",
            ));
        }
        // Take the successor descriptor from the actual staged catalog rather than the drain's old routing generation.
        // 从实际暂存目录取得后继描述符，而非排空句柄的旧路由代次。
        let plugin = staged
            .plugins
            .get(&transaction.record().target().plugin_id)
            .cloned()
            .ok_or_else(|| error("Actual staged predecessor descriptor is missing."))?;
        // The source retains the existing reservation and exact old pins, with no strong manager cycle.
        // 来源保留既有预留权及精确旧固定句柄，不形成强管理器循环。
        let source = Arc::new(VulcanPluginPredecessorRestorationSource {
            manager: Arc::downgrade(&self.state),
            original: transaction.record().clone(),
            plugin,
            staged: Arc::clone(staged),
            owner: Arc::clone(owner),
            hook_attempted: AtomicBool::new(false),
            hook_environment: Mutex::new(None),
            mcp: Mutex::new(None),
            mcp_preparation: Mutex::new(()),
        });
        source.validate(&self.state)?;
        // Synchronous retention precedes returning authority; no fallible operation follows installation.
        // 同步保留先于返回权威；安装之后不再执行可失败操作。
        restoration.source = Some(Arc::clone(&source));
        Ok(source)
    }
}
