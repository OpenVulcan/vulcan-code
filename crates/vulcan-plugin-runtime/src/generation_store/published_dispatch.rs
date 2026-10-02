//! A short ordinary MCP dispatch fence observes selection and original obligations under one actual namespace File.
//! 短时普通 MCP 派发屏障在同一实际命名空间 File 内观测选择及原义务。

use super::*;
use super::{
    VulcanPluginStoreError as Error, VulcanPluginStoreErrorCode as Code,
    VulcanPluginStoreResult as Result,
};
use std::fs::File;

/// Borrowed selected-store authority excludes cooperating writers through synchronous native dispatch.
/// 借用的已选存储权威在同步原生派发期间排除协作写入者。
/// No request lock, publication permission, execution owner or completion evidence is granted.
/// 不授予请求锁、发布许可、执行所有者或完成证据。
pub(crate) struct PublishedDispatchFence<'a> {
    /// Same canonical store is borrowed; no replacement backend is constructed inside this fence.
    /// 借用同一规范存储；不在此屏障内构造替代后端。
    store: &'a VulcanPluginGenerationStore,
    /// Full actual selection includes every declared credential, permission and configuration authority.
    /// 完整实际选择包含全部已声明凭据、权限及配置权威。
    selected: VulcanPluginActivation,
    /// The original writer's strict inventory retains all unfinished bodies without granting request ownership.
    /// 原写入者的严格清单保留全部未完成正文，不授予请求所有权。
    pending: Vec<VulcanPluginActivationRequestRecord>,
    /// Pending evidence and real terminal transfer lease ownership come from the original locked reader.
    /// Pending 证据及真实终态转交租约所有权来自原加锁读取者。
    handoff_blocked: bool,
    /// This shared namespace File is held only until synchronous dispatch completes or fails.
    /// 此共享命名空间 File 仅持有至同步派发完成或失败。
    _namespace: File,
}

impl PublishedDispatchFence<'_> {
    /// Borrow the actual complete selected record; no fresh selection or raw native owner is exposed.
    /// 借用实际完整已选记录；不暴露新选择或原始原生所有者。
    /// No inputs; the result remains protected by this actual namespace fence.
    /// 无输入；返回值持续受此实际命名空间屏障保护。
    pub(crate) fn selected(&self) -> &VulcanPluginActivation {
        &self.selected
    }

    /// Borrow the original strict pending inventory for the runtime's single admission predicate.
    /// 为运行时唯一入场谓词借用原严格待处理清单。
    /// No inputs; results neither authorize replay nor reopen the retained namespace.
    /// 无输入；返回值既不授权重放，也不重开保留的命名空间。
    pub(crate) fn pending(&self) -> &[VulcanPluginActivationRequestRecord] {
        &self.pending
    }

    /// Return the original actual handoff blocker observation without probing another source.
    /// 返回原实际交接阻挡观测，不探测其他来源。
    /// No inputs; true requires the runtime's unchanged ordinary rejection rule.
    /// 无输入；真值要求运行时执行未改变的普通拒绝规则。
    pub(crate) fn handoff_blocked(&self) -> bool {
        self.handoff_blocked
    }

    /// Pin selected using this same File and compare the reread full selection before returning actual resources.
    /// 使用此同一 File 固定 selected，并在返回实际资源前比较重读完整选择。
    /// No inputs; returns package and credentials or an explicit changed-selection error without relocking.
    /// 无输入；返回包及凭据，或明确选择变更错误，不重新加锁。
    pub(crate) fn pin_selected(&self) -> Result<VulcanPluginActiveGeneration> {
        // The locked helper retains the same actual package and credential Files without opening a namespace.
        // 加锁辅助方法保留同一实际插件包及凭据 File，不打开命名空间。
        let active = self.store.pin_active_locked(&self.selected.plugin_id)?;
        if active.activation.as_ref() != Some(&self.selected) {
            return Err(Error::new(
                Code::Conflict,
                "Published MCP selection changed under its original namespace fence.",
            ));
        }
        Ok(active)
    }
}

impl VulcanPluginGenerationStore {
    /// Retain expected's exact shared namespace through the caller's synchronous native dispatch.
    /// 在调用方同步原生派发期间保留 expected 的精确共享命名空间。
    /// expected is a complete selected activation; return locked facts, never a request or startup receipt.
    /// expected 为完整已选激活；返回加锁事实，绝不返回请求或启动回执。
    pub(crate) fn acquire_published_dispatch_fence<'a>(
        &'a self,
        expected: &VulcanPluginActivation,
    ) -> Result<PublishedDispatchFence<'a>> {
        // The same writer fence spans selection, strict recovery inventory and original handoff facts.
        // 相同写入者屏障覆盖选择、严格恢复清单及原交接事实。
        let namespace = self.namespace_lock(&expected.plugin_id, true)?;
        // Actual complete selected bytes must match the original before observations can authorize dispatch.
        // 观测可以授权派发前，实际完整已选字节必须匹配原选择。
        let selected = self
            .read_activation(&expected.plugin_id)?
            .ok_or_else(|| Error::new(Code::Conflict, "Published MCP activation is missing."))?;
        if selected != *expected || selected.generation.is_none() {
            return Err(Error::new(
                Code::Conflict,
                "Published MCP requires its exact complete active selection.",
            ));
        }
        // Reuse the original writer's Pending and actual terminal lease reader under this same File.
        // 在此同一 File 下复用原写入者的 Pending 及实际终态租约读取者。
        let handoff_blocked =
            self.predecessor_handoff_blocks_admission_locked(&expected.plugin_id)?;
        // Strict history validation is identical to public recovery inspection without a public relock.
        // 严格历史验证与公开恢复检查相同，不进行公开重新加锁。
        let pending = self.activation_recovery_records_locked(&expected.plugin_id)?;
        Ok(PublishedDispatchFence {
            store: self,
            selected,
            pending,
            handoff_blocked,
            _namespace: namespace,
        })
    }
}

#[cfg(test)]
mod tests;
