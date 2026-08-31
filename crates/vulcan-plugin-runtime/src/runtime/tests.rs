//! Shared isolated package, engine, and fixed-bridge fixtures for runtime tests.
//! 运行时测试共享的隔离插件包、引擎与固定桥接夹具。

mod bundled;
mod content;

use super::VulcanPluginRuntimeManager;
use luaskills::{LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Writes one hook-only package whose Lua module retains an invocation counter.
/// 写入一个仅含 hook 的插件包，其 Lua 模块会保留调用计数器。
pub(super) fn write_hook_package(root: &Path) {
    let package = root.join("hook-plugin");
    fs::create_dir_all(package.join("runtime")).expect("create hook runtime directory");
    fs::write(package.join("dependencies.yaml"), "{}\n").expect("write dependencies");
    fs::write(
        package.join("runtime/plugin.bundle.lua"),
        r#"local M = {}
local count = 0
function M.dispatch()
    return '{"ok":true,"result":{}}'
end
function M.shutdown()
    return '{"ok":true,"result":{}}'
end
function M.on_precheck(_args)
    count = count + 1
    return '{"ok":true,"result":{"payload":{"value":{"count":' .. tostring(count) .. '}}}}'
end
function M.ui_action(_args)
    count = count + 100
    return '{"ok":true,"result":{"count":' .. tostring(count) .. '}}'
end
function M.shutdown_hooks(_args)
    return '{"ok":true,"result":{}}'
end
function M.on_disable(_args)
    return '{"ok":false,"error":"enable-rollback-marker"}'
end
return M
"#,
    )
    .expect("write hook runtime module");
    fs::write(
        package.join("plugin.yaml"),
        r#"schema_version: 1
id: hook-plugin
version: 1.0.0
default_enabled: true
display:
  name: Hook Plugin
  capability: Persistent hook fixture
execution:
  dependencies:
    manifest: dependencies.yaml
    install_mode: bundled
lua_runtime:
  bundle: runtime/plugin.bundle.lua
  invoke_function: dispatch
  shutdown_function: shutdown
  hook_shutdown_function: shutdown_hooks
  max_active_sessions: 1
  lifecycle:
    disable: on_disable
  actions:
    - id: ui-action
      function: ui_action
      timeout_ms: 1000
      callers:
        - plugin_ui
  hooks:
    - id: precheck
      point: conversation.turn.precheck
      mode: transform
      function: on_precheck
      timeout_ms: 1000
      failure_policy: stop
"#,
    )
    .expect("write hook manifest");
}

/// Writes one minimal session plugin with a one-second idle timeout.
/// 写入一个空闲超时为一秒的最小会话插件。
pub(super) fn write_idle_session_package(root: &Path) {
    let package = root.join("idle-plugin");
    fs::create_dir_all(package.join("runtime")).expect("create idle runtime directory");
    fs::create_dir_all(package.join("schemas")).expect("create idle schema directory");
    fs::write(package.join("dependencies.yaml"), "{}\n").expect("write dependencies");
    fs::write(
        package.join("runtime/plugin.bundle.lua"),
        r#"local M = {}
function M.dispatch(_args) return '{"ok":true,"result":{"accepted":true}}' end
function M.shutdown(_args) return '{"ok":true,"result":{"closed":true}}' end
return M
"#,
    )
    .expect("write idle runtime module");
    fs::write(
        package.join("schemas/open.json"),
        r#"{"type":"object","properties":{}}"#,
    )
    .expect("write open schema");
    fs::write(
            package.join("schemas/close.json"),
            r#"{"type":"object","properties":{"session_id":{"type":"string"}},"required":["session_id"],"additionalProperties":false}"#,
        )
        .expect("write close schema");
    fs::write(
        package.join("plugin.yaml"),
        r#"schema_version: 1
id: idle-plugin
version: 1.0.0
default_enabled: true
display:
  name: Idle Plugin
  capability: Idle cleanup fixture
execution:
  dependencies:
    manifest: dependencies.yaml
    install_mode: bundled
lua_runtime:
  bundle: runtime/plugin.bundle.lua
  invoke_function: dispatch
  shutdown_function: shutdown
  max_active_sessions: 1
  session_idle_timeout_sec: 1
  tools:
    - id: idle_ping
      group: idle_tools
      description: Run one stateless idle test operation.
      parameters_file: schemas/open.json
      execution:
        kind: stateless
        operation: ping
        timeout_ms: 1000
    - id: idle_open
      group: idle_tools
      description: Open one idle test session.
      parameters_file: schemas/open.json
      execution:
        kind: session_open
        operation: open
        timeout_ms: 1000
    - id: idle_close
      group: idle_tools
      description: Close one idle test session.
      parameters_file: schemas/close.json
      execution:
        kind: session_close
        operation: close
        timeout_ms: 1000
"#,
    )
    .expect("write idle manifest");
}

/// Writes one minimal user-installed package for installation transaction tests.
/// 为安装事务测试写入一个最小用户安装插件包。
///
/// # Parameters
/// 参数
/// `package` is the exact local source directory and `plugin_id` is its stable identity.
/// `package` 是精确本地来源目录，`plugin_id` 是其稳定身份。
pub(super) fn write_user_plugin_package(package: &Path, plugin_id: &str) {
    fs::create_dir_all(package.join("runtime")).expect("create user plugin runtime directory");
    fs::write(package.join("dependencies.yaml"), "{}\n").expect("write user plugin dependencies");
    fs::write(
        package.join("runtime/plugin.bundle.lua"),
        r#"local M = {}
function M.dispatch()
    return '{"ok":true,"result":{}}'
end
function M.shutdown()
    return '{"ok":true,"result":{}}'
end
return M
"#,
    )
    .expect("write user plugin bundle");
    fs::write(
            package.join("plugin.yaml"),
            format!(
                "schema_version: 1\nid: {plugin_id}\nversion: 1.0.0\ndefault_enabled: false\ndisplay:\n  name: Local Plugin\n  capability: Local installation fixture\nexecution:\n  dependencies:\n    manifest: dependencies.yaml\n    install_mode: bundled\nlua_runtime:\n  bundle: runtime/plugin.bundle.lua\n  invoke_function: dispatch\n  shutdown_function: shutdown\n  max_active_sessions: 1\n"
            ),
        )
        .expect("write user plugin manifest");
}

/// Builds one minimal real LuaSkills engine for System lease integration tests.
/// 为系统租约集成测试构建一个最小真实 LuaSkills 引擎。
fn test_engine(runtime_root: &Path) -> LuaEngine {
    // native_runtime_root keeps the integration-test Lua package paths free of Windows verbatim prefixes.
    // native_runtime_root 使集成测试的 Lua package 路径不包含 Windows verbatim 前缀。
    let native_runtime_root = vulcan_runtime_assets::native_host_path(
        &runtime_root
            .canonicalize()
            .expect("canonicalize LuaSkills runtime root"),
    );
    let host_options = LuaRuntimeHostOptions::with_runtime_root(native_runtime_root);
    LuaEngine::new(LuaEngineOptions::new(
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 1,
            idle_ttl_secs: 60,
        },
        host_options,
    ))
    .expect("create LuaSkills test engine")
}

/// Builds one plugin manager with a real fixed bridge package and isolated catalog roots.
/// 使用真实固定桥接包与隔离目录根构建一个插件管理器。
///
/// # Parameters
/// 参数
/// `runtime_root` owns LuaSkills assets and `vulcan_plugin_root` owns real plugin packages.
/// `runtime_root` 持有 LuaSkills 资产，`vulcan_plugin_root` 持有真实插件包。
///
/// # Returns
/// 返回值
/// Returns a manager ready for catalog refresh and lease integration tests.
/// 返回可用于目录刷新与租约集成测试的管理器。
pub(super) fn test_manager(
    runtime_root: &Path,
    vulcan_plugin_root: PathBuf,
) -> VulcanPluginRuntimeManager {
    let bridge_root = runtime_root
        .join("system_lua_lib")
        .join("vulcan-plugin-bridge");
    fs::create_dir_all(bridge_root.join("runtime")).expect("create bridge runtime directory");
    fs::write(bridge_root.join("dependencies.yaml"), "{}\n").expect("write bridge dependencies");
    fs::write(
        bridge_root.join("runtime/bridge.lua"),
        include_str!("../../../../plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua"),
    )
    .expect("write bridge module");
    let user_plugin_root = runtime_root.join("user_plugins");
    let managed_runtime_distribution_root = runtime_root.join("managed_runtime");
    fs::create_dir_all(&user_plugin_root).expect("create user plugin root");
    fs::create_dir_all(&managed_runtime_distribution_root)
        .expect("create managed runtime distribution root");
    VulcanPluginRuntimeManager::new(
        Arc::new(Mutex::new(test_engine(runtime_root))),
        vulcan_plugin_root,
        user_plugin_root,
        bridge_root,
        managed_runtime_distribution_root,
        runtime_root.join("plugin_state"),
        runtime_root.join("plugin_runtime"),
    )
}
