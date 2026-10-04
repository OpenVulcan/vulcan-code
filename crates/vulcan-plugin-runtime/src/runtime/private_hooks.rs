//! Private Hook mechanics retain exactly one explicit target or aborted-predecessor authority.
//! 私有 Hook 机械逻辑恰好保留单个显式目标或已中止前驱权威。

use super::candidates::ActivationTargetHookAuthority;
use super::*;
use crate::VulcanPluginDescriptor;

/// Mutually exclusive origins share core preparation while retaining their distinct original permissions.
/// 互斥来源共享核心准备，同时保留各自不同的原权限。
#[derive(Clone)]
pub(in crate::runtime) enum PrivateHookOrigin {
    /// A Published target owns its original activation readiness and durable target initialization receipt.
    /// 已发布目标拥有原激活就绪权及持久目标初始化回执。
    Target(Arc<ActivationTargetHookAuthority>),
    /// An Aborted original owns the old immutable selection and the independent Pending restoration File.
    /// 已中止原请求拥有旧不可变选择及独立 Pending 恢复文件。
    Predecessor(Arc<VulcanPluginPredecessorRestorationSource>),
}

impl PrivateHookOrigin {
    /// Borrow this origin's sole actual descriptor; no inputs or public discovery substitution.
    /// 借用此来源唯一实际描述符；无输入，不替换为公开发现。
    pub(in crate::runtime) fn plugin(&self) -> &VulcanPluginDescriptor {
        match self {
            Self::Target(authority) => &authority.plugin,
            Self::Predecessor(source) => source.plugin(),
        }
    }

    /// Return the original request identity for retention queries; its copied UUID grants no ownership.
    /// 返回原请求身份供保留查询；复制的 UUID 不授予所有权。
    pub(in crate::runtime) fn request_id(&self) -> Uuid {
        match self {
            Self::Target(authority) => authority.request_id,
            Self::Predecessor(source) => source.request_id(),
        }
    }

    /// Validate state and plugin against the explicit retained variant; returns its precise authority failure.
    /// 针对显式保留变体校验 state 及 plugin；返回其精确权威失败。
    pub(in crate::runtime) fn validate(
        &self,
        state: &VulcanPluginManagerState,
        plugin: &VulcanPluginDescriptor,
    ) -> Result<(), VulcanPluginRuntimeError> {
        match self {
            Self::Target(authority) => authority.validate(state, plugin),
            Self::Predecessor(source) => source.validate_hook(state, plugin),
        }
    }

    /// Compare exact original authority allocations for a physical domain transfer; values alone cannot match.
    /// 为物理域转交比较精确原权威分配；仅值相等不能匹配。
    pub(in crate::runtime) fn same_allocation(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Target(left), Self::Target(right)) => Arc::ptr_eq(left, right),
            (Self::Predecessor(left), Self::Predecessor(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }

    /// Borrow only a target authority for target-specific publication; a predecessor returns an explicit conflict.
    /// 为目标专有发布仅借用目标权威；前驱返回明确冲突。
    pub(in crate::runtime) fn target(
        &self,
    ) -> Result<&Arc<ActivationTargetHookAuthority>, VulcanPluginRuntimeError> {
        match self {
            Self::Target(authority) => Ok(authority),
            Self::Predecessor(_) => Err(VulcanPluginRuntimeError::new(
                "Predecessor Hook cannot authorize target publication or completion.",
            )),
        }
    }

    /// Retain this origin's existing environment before registration; state identifies its original manager.
    /// 注册前保留此来源既有环境；state 标识其原管理器。
    /// Returns the variant's ownership result without changing the selected source.
    /// 返回此变体所有权结果，不改变所选来源。
    pub(in crate::runtime) fn retain_environment(
        &self,
        state: &VulcanPluginManagerState,
    ) -> Result<(), VulcanPluginRuntimeError> {
        match self {
            Self::Target(authority) => authority.retain_environment(state),
            Self::Predecessor(source) => source.retain_hook_environment(state),
        }
    }

    /// Move this origin's same prepared File into the actual core domain; no inputs or replacement lease.
    /// 将此来源相同已准备文件移入实际核心域；无输入，不提供替代租约。
    pub(in crate::runtime) fn take_environment(
        &self,
    ) -> Result<std::fs::File, VulcanPluginRuntimeError> {
        match self {
            Self::Target(authority) => authority.take_environment(),
            Self::Predecessor(source) => source.take_hook_environment(),
        }
    }
}
