//! Match manifest callback requirements against the exact authority bound to the native module.
//! 将清单回调必需条件与绑定原生模块的精确权威匹配。

use super::*;
use crate::VulcanPluginManifest;

/// Derive the same revocable grants for discovery, requirement checks and actual module execution.
/// 为发现、必需条件检查和实际模块执行派生同一可撤销授权。
/// manifest is the validated package authority; errors cannot manufacture an unrestricted permission set.
/// manifest 为已校验插件包权威；错误不能制造无限制权限集合。
pub(super) fn permissions_for(
    manifest: &VulcanPluginManifest,
) -> EmbeddedResult<Arc<CapabilityPermissions>> {
    CapabilityPermissions::new(
        CAPABILITIES
            .iter()
            .copied()
            .filter(|name| permitted(name, &manifest.permissions))
            .map(str::to_string)
            .collect(),
    )
}

/// Validate business requirements against snapshot and live permissions, without invoking any callback.
/// 针对 snapshot 及实时 permissions 校验业务必需条件，不调用任何回调。
/// Cleanup remains available after unrelated business contracts disappear; actual calls retain all core checks.
/// 不相关业务契约消失后仍允许清理；实际调用保留全部核心检查。
/// Returns an exact English compatibility error without disclosing unauthorized registration descriptors.
/// 返回精确英文兼容性错误，不披露未授权注册描述符。
pub(super) fn validate(
    manifest: &VulcanPluginManifest,
    scope: VulcanPluginBridgeLeaseScope,
    snapshot: &CapabilitySnapshot,
    permissions: &CapabilityPermissions,
) -> EmbeddedResult<()> {
    if !scope.is_cleanup() {
        // Initialization declarations cannot create grants, including during activation before any pool is registered.
        // 初始化声明不能创建授权，包括在登记任何池之前的激活阶段。
        for name in manifest
            .lua_runtime
            .iter()
            .flat_map(|runtime| &runtime.tools)
            .filter_map(|tool| tool.execution.reusable.as_ref())
            .flat_map(|reuse| &reuse.initialization_capabilities)
        {
            if !snapshot.has(name, permissions)? {
                return Err(error(
                    EmbeddedErrorCode::PermissionDenied,
                    format!(
                        "Plugin `{}` initialization capability `{name}` is unavailable or unauthorized.",
                        manifest.id
                    ),
                ));
            }
        }
    }
    // Missing declarations explicitly retain the legacy contract; cleanup cannot resurrect business authority.
    // 缺少声明明确保留旧契约；清理不能恢复业务权威。
    let Some(required) = manifest
        .runtime_requirements
        .as_ref()
        .filter(|required| !required.host_capabilities.is_empty() && !scope.is_cleanup())
    else {
        return Ok(());
    };
    // The core list filters live grants and accepting registrations from this exact immutable snapshot.
    // 核心列表从此精确不可变快照中过滤实时授权及接纳中的注册。
    let descriptors = snapshot.list(permissions)?;
    for capability in &required.host_capabilities {
        // Only a declared exact identity can satisfy a requirement; unavailable and unauthorized stay indistinguishable.
        // 仅声明的精确身份可满足必需条件；不可用与未授权保持不可区分。
        let descriptor = descriptors.iter().find(|descriptor| descriptor.name == capability.name)
            .ok_or_else(|| error(EmbeddedErrorCode::PermissionDenied,
                format!("Plugin `{}` requires host capability `{}`, which is unavailable or unauthorized.",
                    manifest.id, capability.name)))?;
        if descriptor.version != capability.version
            || descriptor.execution != capability.execution
            || descriptor.scope != capability.scope
        {
            return Err(error(
                EmbeddedErrorCode::Unsupported,
                format!(
                    "Plugin `{}` requires host capability `{}` version {} with {:?}/{:?}; the authorized snapshot provides {} with {:?}/{:?}.",
                    manifest.id,
                    capability.name,
                    capability.version,
                    capability.execution,
                    capability.scope,
                    descriptor.version,
                    descriptor.execution,
                    descriptor.scope,
                ),
            ));
        }
    }
    Ok(())
}

impl VulcanPluginEmbeddedHost {
    /// Check manifest against current registered authority before catalog publication; scope identifies business or cleanup.
    /// 在目录发布前针对当前注册权威检查 manifest；scope 标识业务或清理。
    /// Returns incompatibility without running Lua; pool registration repeats against its own captured binding.
    /// 不运行 Lua 即返回不兼容错误；池注册针对自身捕获绑定再次检查。
    pub(in crate::runtime) fn validate_manifest_capabilities(
        &self,
        manifest: &VulcanPluginManifest,
        scope: VulcanPluginBridgeLeaseScope,
    ) -> EmbeddedResult<()> {
        if scope.is_cleanup() {
            return Ok(());
        }
        // Current registered membership is authoritative only for this activation observation.
        // 当前注册成员仅作为此次激活观测的权威。
        let snapshot = self.runtime.capabilities().snapshot()?;
        // Required declarations never widen the actual manifest-derived permission owner.
        // 必需声明绝不扩大实际清单派生权限所有者。
        let permissions = permissions_for(manifest)?;
        validate(manifest, scope, &snapshot, &permissions)
    }
}
