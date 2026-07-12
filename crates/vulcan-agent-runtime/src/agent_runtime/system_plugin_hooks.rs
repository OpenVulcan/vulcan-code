//! System Plugin Lua hook registration bridge for the shared host dispatcher.
//! 面向共享宿主调度器的系统插件 Lua hook 注册桥。

use std::sync::Arc;

use vulcan_hooks::{FnHookHandler, HookDispatcher, HookError, HookRegistration};
use vulcan_plugin_runtime::{
    SystemPluginCatalogSnapshot, SystemPluginHookDescriptor, SystemPluginRuntimeManager,
};

/// Stable registration prefix reserved for manifest-driven System Plugin hooks.
/// 为清单驱动系统插件 hook 保留的稳定注册前缀。
const SYSTEM_PLUGIN_HOOK_REGISTRATION_PREFIX: &str = "system-plugin/";

/// Atomically synchronizes enabled manifest hooks into the existing host dispatcher.
/// 将已启用的清单 hook 原子同步到现有宿主调度器。
///
/// # Parameters
/// 参数
/// `dispatcher` is the existing host dispatcher, `runtime` executes Lua handlers, and `catalog` supplies the enabled hook snapshot.
/// `dispatcher` 是现有宿主调度器，`runtime` 执行 Lua 处理器，`catalog` 提供已启用 Hook 快照。
///
/// # Returns
/// 返回值
/// Returns success after atomically replacing only System Plugin registrations.
/// 在仅原子替换系统插件注册项后返回成功。
pub fn synchronize_system_plugin_hooks(
    dispatcher: &mut HookDispatcher,
    runtime: SystemPluginRuntimeManager,
    catalog: &SystemPluginCatalogSnapshot,
) -> Result<(), HookError> {
    let enabled_hooks = catalog.enabled_hooks.values().cloned().collect::<Vec<_>>();
    validate_system_plugin_hooks(dispatcher, &enabled_hooks)?;

    let mut staged = dispatcher.clone();
    let existing_ids = staged
        .registry()
        .registration_snapshots(None)
        .into_iter()
        .map(|snapshot| snapshot.registration.id)
        .filter(|id| id.starts_with(SYSTEM_PLUGIN_HOOK_REGISTRATION_PREFIX))
        .collect::<Vec<_>>();
    for registration_id in existing_ids {
        staged.unregister(&registration_id, None);
    }

    for hook in enabled_hooks {
        let mut registration = HookRegistration::new(
            hook.registration_id.clone(),
            hook.manifest.point.clone(),
            hook.manifest.mode,
        );
        registration.scope = hook.manifest.scope;
        registration.priority = hook.manifest.priority;
        registration.timeout_ms = hook.manifest.timeout_ms;
        registration.failure_policy = hook.manifest.failure_policy;
        registration.requested_capabilities = hook
            .manifest
            .requested_capabilities
            .iter()
            .cloned()
            .collect();
        registration.display_name = hook.manifest.display_name.clone();
        registration.progress_label = hook.manifest.progress_label.clone();
        let hook_runtime = runtime.clone();
        staged.register(
            registration,
            Arc::new(FnHookHandler::new(move |invocation| {
                let hook_runtime = hook_runtime.clone();
                let hook = hook.clone();
                async move {
                    hook_runtime
                        .invoke_hook(&hook, invocation)
                        .await
                        .map_err(|error| HookError::handler(error.to_string()))
                }
            })),
        )?;
    }
    *dispatcher = staged;
    Ok(())
}

/// Validates plugin hook declarations against the host-owned point registry without mutation.
/// 在不产生变更的前提下，依据宿主自有点位注册表校验插件 hook 声明。
///
/// # Parameters
/// 参数
/// `dispatcher` owns valid host point descriptors and `hooks` contains candidate plugin declarations.
/// `dispatcher` 拥有有效宿主点位描述，`hooks` 包含候选插件声明。
///
/// # Returns
/// 返回值
/// Returns success only when every point exists and accepts the declared mode.
/// 仅当每个点位存在且接受声明模式时返回成功。
pub fn validate_system_plugin_hooks(
    dispatcher: &HookDispatcher,
    hooks: &[SystemPluginHookDescriptor],
) -> Result<(), HookError> {
    for hook in hooks {
        let point = dispatcher
            .registry()
            .point_descriptor(&hook.manifest.point)
            .ok_or_else(|| {
                HookError::invalid_registration(format!(
                    "System Plugin hook `{}` references unknown host point `{}`.",
                    hook.registration_id, hook.manifest.point
                ))
            })?;
        if !point.modes.contains(&hook.manifest.mode) {
            return Err(HookError::invalid_registration(format!(
                "System Plugin hook `{}` mode {:?} is not accepted by host point `{}`.",
                hook.registration_id, hook.manifest.mode, hook.manifest.point
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use luaskills::{LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;
    use std::sync::Mutex;
    use vulcan_hooks::{HookMode, HookPointDescriptor};

    /// Writes one minimal hook-only System Plugin package.
    /// 写入一个最小化的仅含 hook 的系统插件包。
    fn write_hook_package(root: &Path) {
        let package = root.join("hook-plugin");
        fs::create_dir_all(package.join("runtime")).expect("create hook runtime directory");
        fs::write(package.join("dependencies.yaml"), "{}\n").expect("write dependencies");
        fs::write(
            package.join("runtime/plugin.lua"),
            r#"local M = {}
local count = 0
function M.dispatch() return '{"ok":true,"result":{}}' end
function M.shutdown() return '{"ok":true,"result":{}}' end
function M.on_precheck(_args)
    count = count + 1
    return '{"ok":true,"result":{"payload":{"value":{"count":' .. tostring(count) .. '}}}}'
end
return M
"#,
        )
        .expect("write hook Lua module");
        fs::write(
            package.join("plugin.yaml"),
            r#"schema_version: 1
id: hook-plugin
version: 1.0.0
kind: bundled_system
default_enabled: true
display:
  name: Hook Plugin
  capability: Host hook bridge fixture
runtime:
  dependencies_file: dependencies.yaml
  module: runtime.plugin
  invoke_function: dispatch
  shutdown_function: shutdown
  max_active_sessions: 1
hooks:
  - id: precheck
    point: conversation.turn.precheck
    mode: transform
    function: on_precheck
    timeout_ms: 20000
    failure_policy: stop
"#,
        )
        .expect("write hook manifest");
    }

    /// Builds one real LuaSkills engine rooted at the test runtime layout.
    /// 构建一个以测试运行布局为根的真实 LuaSkills 引擎。
    fn test_engine(runtime_root: &Path) -> LuaEngine {
        LuaEngine::new(LuaEngineOptions::new(
            LuaVmPoolConfig {
                min_size: 1,
                max_size: 1,
                idle_ttl_secs: 60,
            },
            LuaRuntimeHostOptions::with_runtime_root(
                runtime_root
                    .canonicalize()
                    .expect("canonicalize LuaSkills runtime root"),
            ),
        ))
        .expect("create LuaSkills test engine")
    }

    /// Verifies atomic registration, idempotent synchronization, and draining removal.
    /// 验证原子注册、幂等同步与 draining 移除。
    #[test]
    fn system_plugin_hooks_share_existing_dispatcher_and_lifecycle() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime_root = temp.path().join("runtime");
        let plugin_root = runtime_root.join("system_lua_lib");
        fs::create_dir_all(&plugin_root).expect("create System Plugin root");
        write_hook_package(&plugin_root);
        let manager = SystemPluginRuntimeManager::new(
            Arc::new(Mutex::new(test_engine(&runtime_root))),
            plugin_root,
        );
        let catalog = manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load hook catalog");
        let mut dispatcher = HookDispatcher::default();
        dispatcher
            .register_point(HookPointDescriptor::new(
                "conversation.turn.precheck",
                [HookMode::Transform],
                "Test precheck point.",
            ))
            .expect("register host point");

        let mut invalid_catalog = (*catalog).clone();
        for hook in invalid_catalog.enabled_hooks.values_mut() {
            hook.manifest.point = String::from("unknown.point");
        }
        for plugin in invalid_catalog.plugins.values_mut() {
            for hook in &mut plugin.hooks {
                hook.manifest.point = String::from("unknown.point");
            }
        }
        let invalid =
            synchronize_system_plugin_hooks(&mut dispatcher, manager.clone(), &invalid_catalog);
        assert!(invalid.is_err());
        assert!(
            dispatcher
                .registry()
                .registration_snapshots(None)
                .is_empty()
        );

        synchronize_system_plugin_hooks(&mut dispatcher, manager.clone(), &catalog)
            .expect("register plugin hook");
        synchronize_system_plugin_hooks(&mut dispatcher, manager.clone(), &catalog)
            .expect("resynchronize plugin hook");
        assert_eq!(dispatcher.registry().registration_snapshots(None).len(), 1);
        manager
            .begin_draining("hook-plugin")
            .expect("begin plugin draining");
        synchronize_system_plugin_hooks(&mut dispatcher, manager.clone(), &manager.snapshot())
            .expect("remove draining plugin hook");
        assert!(
            dispatcher
                .registry()
                .registration_snapshots(None)
                .is_empty()
        );
    }
}
