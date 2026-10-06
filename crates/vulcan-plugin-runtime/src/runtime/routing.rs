//! Actual originals coordinate routing; physical owners keep all execution permissions.
//! 实际原实例协调路由；物理所有者保留全部执行权限。

use super::{VulcanPluginManagerState, VulcanPluginRuntimeError, VulcanPluginRuntimeManager};
use crate::{
    VulcanPluginActivation, VulcanPluginActivationCandidate, VulcanPluginActivationTransaction,
    VulcanPluginActivationTransactionRecord, VulcanPluginDeactivationObservation,
    VulcanPluginDeactivationReservation, VulcanPluginGenerationStore, VulcanPluginNamespaceGuard,
    VulcanPluginRegisteredNativeMcpHostAuthority, VulcanPluginRuntimeState,
};
use std::fs::File;
use std::sync::{Arc, MutexGuard, Weak};
use uuid::Uuid;

/// A non-cloneable retained coordinator excludes unrelated writers until actual retirement.
/// 不可克隆保留协调器排除无关写入者，直至实际退休。
/// This guard owns no File, execution permission, Source, Full or persistent record.
/// 此守卫不拥有文件、执行权限、Source、Full 或持久记录。
pub struct VulcanPluginRoutingReservation {
    /// Only this guard strongly retains the inert coordinator.
    /// 仅此守卫强保留未启动协调器。
    _inner: Arc<RoutingReservationInner>,
}

/// The existing manager slot weakly observes this immutable original identity.
/// 既有管理器槽弱观测此不可变原身份。
pub(super) struct RoutingReservationInner {
    /// Actual manager identity cannot be reconstructed from request values.
    /// 实际管理器身份不能从请求值重建。
    manager: Weak<VulcanPluginManagerState>,
    /// Full original metadata accompanies the actual physical allocation's weak identity.
    /// 完整原元数据伴随实际物理分配的弱身份。
    original: RoutingReservationOriginal,
}

/// Explicit kinds preserve separate original contracts without adding execution ownership.
/// 明确种类保留独立原契约，不添加执行所有权。
enum RoutingReservationOriginal {
    /// Configured activation preserves its first issuer and complete transaction.
    /// 已配置激活保留首个签发者及完整事务。
    Activation {
        /// First actual registered issuer remains fixed without native resources.
        /// 首个实际已注册签发者保持固定，不携带原生资源。
        authority: VulcanPluginRegisteredNativeMcpHostAuthority,
        /// Every immutable preparation, predecessor and target field remains original.
        /// 每个不可变准备、前驱及目标字段保持为原记录。
        record: VulcanPluginActivationTransactionRecord,
        /// Weak identity observes the sole actual transaction File.
        /// 弱身份观测唯一实际事务文件。
        request_lease: Weak<File>,
    },
    /// Managed removal fixes complete original and intended selection under the same namespace.
    /// 受管移除在相同命名空间内固定完整原选择及期望选择。
    Deactivation {
        /// Request metadata never substitutes for actual ownership.
        /// 请求元数据绝不替代实际所有权。
        request_id: Uuid,
        /// Full permissions, credentials and configuration remain original.
        /// 完整权限、凭据及配置保持为原记录。
        original: VulcanPluginActivation,
        /// The sole intended tombstone is fixed before publication.
        /// 唯一期望停用记录在发布之前固定。
        intended: VulcanPluginActivation,
        /// Existing Arc contains the same private exclusive namespace File.
        /// 既有 Arc 包含相同私有排他命名空间文件。
        namespace: Weak<VulcanPluginNamespaceGuard>,
    },
}

impl RoutingReservationOriginal {
    /// Return fixed diagnostic request identity; no parameters or execution permissions.
    /// 返回固定诊断请求身份；无参数或执行权限。
    fn request_id(&self) -> Uuid {
        match self {
            Self::Activation { record, .. } => record.request_id(),
            Self::Deactivation { request_id, .. } => *request_id,
        }
    }

    /// Match `mutation` to the full original selection and actual namespace without upgrading Weak.
    /// 将 `mutation` 与完整原选择及实际命名空间匹配，不升级 Weak。
    /// Returns physical coordination identity only; existing execution-source checks remain required.
    /// 仅返回物理协调身份；仍要求既有执行来源检查。
    fn matches_mutation(&self, mutation: &VulcanPluginNamespaceGuard) -> bool {
        match self {
            Self::Deactivation {
                original,
                namespace,
                ..
            } => {
                mutation.matches_routing_namespace(namespace)
                    && mutation.routing_original() == Some(original)
            }
            Self::Activation { .. } => false,
        }
    }
}

/// Only real borrowed owners identify an existing routing original.
/// 仅真实借用所有者标识既有路由原实例。
pub(super) enum ActivationRoutingOriginal<'a> {
    /// Actual configured transaction supplies its full record and physical allocation.
    /// 实际已配置事务提供完整记录及物理分配。
    Transaction(&'a VulcanPluginActivationTransaction),
    /// Inspected candidate borrows the same actual configured transaction.
    /// 已检查候选借用相同实际已配置事务。
    Candidate(&'a VulcanPluginActivationCandidate<'a>),
    /// Actual removal supplies full original, intended successor and namespace.
    /// 实际移除提供完整原记录、期望后继及命名空间。
    Deactivation(&'a VulcanPluginDeactivationReservation),
    /// Original callback or guarded publication borrows the existing physical mutation owner.
    /// 原回调或已屏障发布借用既有物理变更所有者。
    NamespaceGuard(&'a VulcanPluginNamespaceGuard),
}

impl VulcanPluginRuntimeManager {
    /// Reserve routing for actual Prepared `transaction` and its first registered `authority`.
    /// 为实际已准备 `transaction` 及首个已注册 `authority` 预留路由。
    /// Returns non-cloneable pure coordination after existing candidate inspection, or an explicit conflict.
    /// 既有候选检查后返回不可克隆纯协调器，否则返回明确冲突。
    pub fn reserve_activation_routing(
        &self,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
        transaction: &VulcanPluginActivationTransaction,
    ) -> Result<VulcanPluginRoutingReservation, VulcanPluginRuntimeError> {
        // Registration precedes the weak slot and every actual store inspection.
        // 登记门先于弱槽及每个实际存储检查。
        let registration =
            self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
        self.require_routing_publication_locked(&registration, None)?;
        if !authority.matches_manager(self) {
            return Err(VulcanPluginRuntimeError::new(
                "Routing reservation belongs to another native host manager.",
            ));
        }
        // Existing storage remains mandatory after an actual configured original is supplied.
        // 传入实际已配置原实例之后，既有存储保持为强制要求。
        let store = self.routing_store()?;
        // Existing inspection preserves Prepared, complete selection and package-byte checks.
        // 既有检查保留已准备、完整选择及插件包字节检查。
        let candidate = store
            .inspect_activation_candidate(transaction)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        super::candidates::validate_candidate_owner(&self.state, &candidate)?;
        self.register_routing_original(
            &registration,
            RoutingReservationOriginal::Activation {
                authority: authority.clone(),
                record: transaction.record().clone(),
                request_lease: transaction.routing_request_lease(),
            },
        )
    }

    /// Reserve `original` before managed removal callbacks or selection publication start.
    /// 在受管移除回调或选择发布启动之前预留 `original`。
    /// Returns coordination after same-store, original selection and disabled manager-source checks without cloning its File owner.
    /// 同存储、原选择及停用管理器来源检查后返回协调器，不克隆文件所有者。
    pub fn reserve_deactivation_routing(
        &self,
        original: &VulcanPluginDeactivationReservation,
    ) -> Result<VulcanPluginRoutingReservation, VulcanPluginRuntimeError> {
        // Initial admission excludes another original before any original selection inspection.
        // 初始准入在任何原选择检查之前排除另一原实例。
        let registration =
            self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
        self.require_routing_publication_locked(&registration, None)?;
        original
            .require_no_lifecycle_borrowers()
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // Actual reconciliation validates the canonical retained store and complete unchanged original.
        // 实际协调校验规范保留存储及完整未变原记录。
        let store = self.routing_store()?;
        if store
            .reconcile_deactivation_retained(original)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
            != VulcanPluginDeactivationObservation::Original
        {
            return Err(VulcanPluginRuntimeError::new(
                "Removal routing must precede selection publication.",
            ));
        }
        // Inspect current routing without retaining an additional package or credential pin.
        // 检查当前路由，不保留额外插件包或凭据固定。
        let snapshot = self.snapshot();
        // The complete original selects this single actual manager descriptor.
        // 完整原记录选择此单个实际管理器描述符。
        let plugin = snapshot
            .plugins
            .get(&original.original().plugin_id)
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Original managed removal source is absent.")
            })?;
        if plugin.runtime_state != VulcanPluginRuntimeState::Disabled
            || plugin.origin != crate::VulcanPluginPackageOrigin::UserInstalled
            || !plugin
                .managed_package()
                .is_some_and(|managed| managed.matches_activation(original.original()))
            || self.active_session_count(&plugin.id)? != 0
            || self.has_cleanup_diagnostics(&plugin.id)?
        {
            return Err(VulcanPluginRuntimeError::new(
                "Removal routing requires the exact disabled zero-session source.",
            ));
        }
        self.register_routing_original(
            &registration,
            RoutingReservationOriginal::Deactivation {
                request_id: original.request_id(),
                original: original.original().clone(),
                intended: original.intended().clone(),
                namespace: original.namespace_identity(),
            },
        )
    }

    /// Open only this manager's existing store; no parameters or physical namespace acquisition.
    /// 仅打开此管理器既有存储；无参数，也不取得物理命名空间。
    /// Returns existing canonical storage or an explicit missing-store error.
    /// 返回既有规范存储，否则返回明确存储缺失错误。
    fn routing_store(&self) -> Result<VulcanPluginGenerationStore, VulcanPluginRuntimeError> {
        VulcanPluginGenerationStore::open_existing(&self.state.user_plugin_root)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Original routing reservation store is missing.")
            })
    }

    /// Register validated `original` under caller-held `registration`; returns the sole inert strong coordinator.
    /// 在调用方持有的 `registration` 内登记已校验 `original`；返回唯一未启动强协调器。
    /// An existing original is never replaced and no physical ownership is added.
    /// 绝不替代既有原实例，也不添加物理所有权。
    fn register_routing_original(
        &self,
        registration: &MutexGuard<'_, ()>,
        original: RoutingReservationOriginal,
    ) -> Result<VulcanPluginRoutingReservation, VulcanPluginRuntimeError> {
        self.require_routing_publication_locked(registration, None)?;
        // A real ordinary lifecycle may already own unstarted or asynchronous work; exclude it before publishing the weak slot.
        // 真实普通生命周期可能已拥有未启动或异步工作；发布弱槽之前排除它。
        self.require_no_published_lifecycle_for_routing(registration)?;
        // This remains the original sole weak slot, not a second request registry.
        // 此保持为原唯一弱槽，而非第二个请求注册表。
        let mut slot = self.state.routing_reservation.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Plugin routing reservation lock is poisoned.")
        })?;
        // Metadata and physical Weak identities enter one allocation before weak publication.
        // 元数据及物理 Weak 身份在弱发布之前进入单个分配。
        let inner = Arc::new(RoutingReservationInner {
            manager: Arc::downgrade(&self.state),
            original,
        });
        *slot = Arc::downgrade(&inner);
        Ok(VulcanPluginRoutingReservation { _inner: inner })
    }

    /// Require absence or same actual `original` before routing or preceding effects change.
    /// 在路由或前置效果改变之前，要求预留缺失或相同实际 `original`。
    /// `registration` is held; returns coordination only, preserving all disk, Source, Seal and execution checks.
    /// `registration` 已持有；仅返回协调准入，保留全部磁盘、Source、Seal 及执行检查。
    pub(super) fn require_routing_publication_locked(
        &self,
        _registration: &MutexGuard<'_, ()>,
        original: Option<ActivationRoutingOriginal<'_>>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Registration always precedes the sole weak coordinator lock.
        // 登记门始终先于唯一弱协调器锁。
        let slot = self.state.routing_reservation.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Plugin routing reservation lock is poisoned.")
        })?;
        // Upgraded coordination contains metadata only; namespace Weak values stay unupgraded.
        // 升级后的协调器仅含元数据；命名空间 Weak 值保持不升级。
        let Some(retained) = slot.upgrade() else {
            return Ok(());
        };
        // Complete original and real physical allocation must match together, never by UUID alone.
        // 完整原实例及真实物理分配必须同时匹配，绝不仅按 UUID 匹配。
        let matches = match (&retained.original, original) {
            (
                RoutingReservationOriginal::Activation {
                    record,
                    request_lease,
                    ..
                },
                Some(ActivationRoutingOriginal::Transaction(transaction)),
            ) => {
                transaction.record().matches_routing_original(record)
                    && Weak::ptr_eq(&transaction.routing_request_lease(), request_lease)
            }
            (
                RoutingReservationOriginal::Activation {
                    record,
                    request_lease,
                    ..
                },
                Some(ActivationRoutingOriginal::Candidate(candidate)),
            ) => {
                candidate
                    .transaction_record()
                    .matches_routing_original(record)
                    && Weak::ptr_eq(&candidate.routing_request_lease(), request_lease)
            }
            (
                RoutingReservationOriginal::Deactivation {
                    request_id,
                    original,
                    intended,
                    namespace,
                },
                Some(ActivationRoutingOriginal::Deactivation(removal)),
            ) => {
                removal.request_id() == *request_id
                    && removal.original() == original
                    && removal.intended() == intended
                    && removal.matches_namespace_identity(namespace)
            }
            (_, Some(ActivationRoutingOriginal::NamespaceGuard(mutation))) => {
                retained.original.matches_mutation(mutation)
            }
            _ => false,
        };
        // Only configured activation carries a native issuer; removal coordination grants none.
        // 仅已配置激活携带原生签发者；移除协调不授予原生签发者。
        let issuer_matches = match &retained.original {
            RoutingReservationOriginal::Activation { authority, .. } => {
                authority.matches_manager(self)
            }
            RoutingReservationOriginal::Deactivation { .. } => true,
        };
        if !matches
            || !Weak::ptr_eq(&retained.manager, &Arc::downgrade(&self.state))
            || !issuer_matches
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Plugin routing is retained by original request `{}`.",
                retained.original.request_id()
            )));
        }
        Ok(())
    }
}
