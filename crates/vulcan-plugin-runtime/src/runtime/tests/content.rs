//! Cross-layer tests for content-only plugins without a Lua execution engine.
//! 不含Lua执行引擎的内容型插件跨层测试。

use super::super::{VulcanPluginLifecycleEvent, VulcanPluginRuntimeManager};
use super::write_hook_package;
use crate::VulcanPluginAvailability;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Writes one runtime-free bundled plugin with a skill and an agent attached to that skill.
/// 写入一个不含运行时、包含 Skill 及绑定该 Skill 的 Agent 的内置插件。
///
/// # Parameters
/// 参数
/// `package` is the exact plugin package root created by the test.
/// `package` 是测试创建的精确插件包根目录。
fn write_content_plugin_package(package: &Path) {
    fs::create_dir_all(package.join("skills/review-helper"))
        .expect("create content skill directory");
    fs::create_dir_all(package.join("agents")).expect("create content agent directory");
    fs::write(
            package.join("skills/review-helper/SKILL.md"),
            "---\nname: review-helper\ndescription: Review one code change.\n---\n\n# Review helper\nInspect the exact diff.",
        )
        .expect("write content skill");
    fs::write(
            package.join("agents/reviewer.md"),
            "---\nname: Content Reviewer\nskills:\n  - review-helper\n---\nReview the requested change.",
        )
        .expect("write content agent");
    fs::write(
            package.join("plugin.yaml"),
            "schema_version: 1\nid: content-plugin\nversion: 1.0.0\ndefault_enabled: true\ndisplay:\n  name: Content Plugin\n  capability: Runtime-free review guidance\n",
        )
        .expect("write content plugin manifest");
}

/// Verifies a runtime-free plugin remains fully usable when no Lua engine exists.
/// 验证不存在 Lua 引擎时，无运行时插件仍保持完整可用。
#[test]
fn content_plugin_operates_without_lua_runtime_or_execution_environment() {
    // temp owns every isolated catalog and runtime directory used by this test.
    // temp 持有本测试使用的全部隔离目录与运行时目录。
    let temp = tempfile::tempdir().expect("create content plugin runtime directory");
    // vulcan_plugin_root contains the only bundled content plugin package.
    // vulcan_plugin_root 包含唯一的内置内容插件包。
    let vulcan_plugin_root = temp.path().join("vulcan_plugins");
    // user_plugin_root is present but intentionally empty.
    // user_plugin_root 存在但刻意保持为空。
    let user_plugin_root = temp.path().join("user_plugins");
    // plugin_runtime_root would receive execution environments if one were incorrectly created.
    // plugin_runtime_root 会在错误创建执行环境时接收相关文件。
    let plugin_runtime_root = temp.path().join("plugin_runtime");
    fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan plugin root");
    fs::create_dir_all(&user_plugin_root).expect("create user plugin root");
    write_content_plugin_package(&vulcan_plugin_root.join("content-plugin"));
    write_hook_package(&vulcan_plugin_root);
    // manager deliberately has no Lua engine or bridge runtime.
    // manager 刻意不包含 Lua 引擎或桥接运行时。
    let manager = VulcanPluginRuntimeManager::without_runtime(
        vulcan_plugin_root,
        user_plugin_root,
        temp.path().join("bridge"),
        temp.path().join("managed_runtime"),
        temp.path().join("plugin_state"),
        plugin_runtime_root.clone(),
    );
    // catalog proves content components publish even when the execution adapter is absent.
    // catalog 证明执行适配器缺失时内容组件仍会发布。
    let catalog = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load content plugin without Lua");
    let plugin = &catalog.plugins["content-plugin"];
    assert_eq!(plugin.availability, VulcanPluginAvailability::Enabled);
    assert!(plugin.runtime_execution_available);
    assert!(
        catalog
            .enabled_skills
            .contains_key("content-plugin:review-helper")
    );
    assert!(
        catalog
            .enabled_agents
            .contains_key("content-plugin:reviewer")
    );
    assert_eq!(plugin.active_session_count, 0);
    let runtime_plugin = &catalog.plugins["hook-plugin"];
    assert!(!runtime_plugin.runtime_execution_available);
    assert!(
        runtime_plugin
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("Lua execution runtime is unavailable"))
    );
    assert!(catalog.enabled_hooks.is_empty());
    assert_eq!(
        manager
            .prepare_plugin_environment("content-plugin")
            .expect("content plugin preparation should be a no-op"),
        plugin.package_root
    );
    // runtime executes asynchronous no-op lifecycle and probe paths deterministically.
    // runtime 确定性执行异步空生命周期与探测路径。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("create content plugin Tokio runtime");
    runtime
        .block_on(manager.invoke_lifecycle("content-plugin", VulcanPluginLifecycleEvent::Enable))
        .expect("content lifecycle should not require Lua");
    runtime
        .block_on(manager.probe_plugin("content-plugin"))
        .expect("content probe should not require Lua");
    assert!(
        runtime
            .block_on(manager.prewarm_enabled_plugin_runtimes())
            .expect("content prewarm should not require Lua")
            .is_empty()
    );
    assert!(!plugin_runtime_root.exists());
    assert_eq!(
        manager
            .active_session_count("content-plugin")
            .expect("actual live reservation count"),
        0
    );
}
