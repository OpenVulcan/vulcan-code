//! Compare plugin requirements with the sole frozen host policy before activation or admission.
//! 在激活或入场前，将插件必需条件与唯一冻结宿主策略比较。

use super::{
    VulcanPluginBridgeLeaseScope, VulcanPluginCapacityPurpose, VulcanPluginRuntimeError,
    VulcanPluginRuntimeManager, VulcanPluginRuntimeSettings,
};
use crate::{VulcanPluginDescriptor, VulcanPluginManifest};

impl VulcanPluginRuntimeSettings {
    /// Check manifest floors against exact effective purpose policies; return conflicts without clipping.
    /// 针对精确有效用途策略检查 manifest 下限；返回冲突，不裁剪。
    /// purpose is None for full admission or the exact cleanup purpose; free VM counts and permissions are separate.
    /// purpose 在完整入场时为 None，在清理时为精确用途；空闲 VM 数量及权限独立检查。
    pub(in crate::runtime) fn validate_manifest_requirements(
        &self,
        manifest: &VulcanPluginManifest,
        purpose: Option<VulcanPluginCapacityPurpose>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Legacy and non-Lua declarations contain no resource requirement and keep existing semantics.
        // 旧版及非 Lua 声明不包含资源要求，保持既有语义。
        let Some(requirements) = &manifest.runtime_requirements else {
            return Ok(());
        };
        for required in &requirements.capacities {
            // Explicit cleanup checks only its declared purpose; unrelated business minima cannot block removal.
            // 明确清理仅检查其声明用途；不相关业务下限不能阻止移除。
            if purpose.is_some_and(|purpose| purpose != required.purpose) {
                continue;
            }
            // Use the production resolver, so diagnostics and real member registration cannot diverge.
            // 使用生产解析器，避免诊断与真实成员注册产生分歧。
            let (effective, source) = self.capacity_budget(&manifest.id, required.purpose);
            for (field, minimum, configured) in [
                (
                    "reserved_resident_vms",
                    required.reserved_resident_vms,
                    effective.resources.min_resident_vms,
                ),
                (
                    "resident_vms",
                    required.resident_vms,
                    effective.resources.max_resident_vms,
                ),
                (
                    "running_calls",
                    required.running_calls,
                    effective.resources.max_running_calls,
                ),
                (
                    "queued_calls",
                    required.queued_calls,
                    effective.max_queued_calls,
                ),
                (
                    "queued_bytes",
                    required.queued_bytes,
                    effective.max_queued_bytes,
                ),
            ] {
                if configured < minimum {
                    return Err(VulcanPluginRuntimeError::new(format!(
                        "Plugin `{}` requires {:?}.{field} >= {minimum}, but {source:?} provides {configured}.",
                        manifest.id, required.purpose,
                    )));
                }
            }
        }
        if purpose.is_none_or(|purpose| purpose == VulcanPluginCapacityPurpose::ToolCall)
            && let Some(runtime) = &manifest.lua_runtime
        {
            // Per-tool ceilings are admission requirements, never silently clipped host overrides.
            // 逐工具上限属于入场要求，绝不静默裁剪宿主覆盖。
            let (capacity, _) =
                self.capacity_budget(&manifest.id, VulcanPluginCapacityPurpose::ToolCall);
            for tool in &runtime.tools {
                if let Some(reuse) = &tool.execution.reusable
                    && (reuse.max_resident_vms > capacity.resources.max_resident_vms
                        || reuse.max_running_calls > capacity.resources.max_running_calls)
                {
                    return Err(VulcanPluginRuntimeError::new(format!(
                        "Reusable tool `{}` exceeds the configured tool-call capacity.",
                        tool.id
                    )));
                }
            }
        }
        Ok(())
    }
}

impl VulcanPluginRuntimeManager {
    /// Validate plugin requirements using the attached immutable policy; return an exact activation error.
    /// 使用已附加不可变策略校验 plugin 必需条件；返回精确激活错误。
    /// An unattached runtime cannot authorize version-two Lua work, even when the engine exists.
    /// 即使引擎存在，未附加运行时也不能授权第二版 Lua 工作。
    pub(super) fn validate_plugin_requirements(
        &self,
        plugin: &VulcanPluginDescriptor,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.validate_plugin_requirements_in_scope(plugin, VulcanPluginBridgeLeaseScope::Enabled)
    }

    /// Check plugin under a trusted scope; cleanup validates its own purpose while all other work requires full admission.
    /// 在可信 scope 下检查 plugin；清理校验自身用途，其他工作均要求完整入场条件。
    /// Returns conflicts without weakening generation, permission, or actual core budget checks.
    /// 返回冲突，不放宽代次、权限或实际核心预算检查。
    pub(super) fn validate_plugin_requirements_in_scope(
        &self,
        plugin: &VulcanPluginDescriptor,
        scope: VulcanPluginBridgeLeaseScope,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Invalid catalog rows and legacy declarations have no new requirements to resolve.
        // 无效目录条目及旧版声明没有需要解析的新必需条件。
        let Some(manifest) = &plugin.manifest else {
            return Ok(());
        };
        if manifest.runtime_requirements.is_none() {
            return Ok(());
        }
        // The parent settings are frozen once; no fallback policy is invented before attachment.
        // 父级设置只冻结一次；附加前不虚构回退策略。
        let owner = self.formal_runtime_owner()?.ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Plugin `{}` requires an attached formal runtime before activation.",
                plugin.id
            ))
        })?;
        // Only host-owned teardown events can restrict validation to lifecycle resource requirements.
        // 仅宿主自有清理事件可将校验限制到生命周期资源要求。
        let purpose = scope
            .is_cleanup()
            .then_some(VulcanPluginCapacityPurpose::Lifecycle);
        owner
            .effective_settings()?
            .validate_manifest_requirements(manifest, purpose)?;
        owner
            .host
            .validate_manifest_capabilities(manifest, scope)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))
    }
}

#[cfg(test)]
mod tests;
