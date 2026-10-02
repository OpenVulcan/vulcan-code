//! Complete legacy manager consumer against the explicitly selected dirty baseline.
//! 面向显式选择脏基线的完整旧管理器消费者。

#[path = "../common.rs"]
mod common;
use common::{Config, Failure};
use luaskills::LuaEngine;
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{Arc, OnceLock, RwLock},
    time::Duration,
};
use vulcan_plugin_runtime::VulcanPluginRuntimeManager;

/// Retain exactly the original legacy engine for truthful public System-lease observations.
/// 恰好保留原始旧引擎，以真实观测公开 System 租约。
static ENGINE: OnceLock<Arc<RwLock<LuaEngine>>> = OnceLock::new();

/// Construct the full old manager from cfg/root and its real fixed bridge; return manager.
/// 从 cfg、root 及真实固定桥构建完整旧管理器；返回管理器。
fn build(cfg: &Config, root: &Path) -> Result<VulcanPluginRuntimeManager, Failure> {
    if cfg.group != "legacy" || cfg.owner_cycles != 0 || cfg.soak {
        return Err("Legacy consumer requires group legacy; reusable owner-cycle/soak gate belongs to the formal consumer.".into());
    }
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let bundled = common::package(cfg, root, None)?;
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let bridge = root.join("system_lua_lib/vulcan-plugin-bridge");
    std::fs::create_dir_all(bridge.join("runtime"))?;
    std::fs::write(bridge.join("dependencies.yaml"), "{}\n")?;
    std::fs::write(
        bridge.join("runtime/bridge.lua"),
        include_str!(env!("VALIDATION_BRIDGE_SOURCE")),
    )?;
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let user = root.join("user_plugins");
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let distribution = root.join("managed_runtime");
    std::fs::create_dir_all(&user)?;
    std::fs::create_dir_all(&distribution)?;
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let engine = Arc::new(RwLock::new(common::engine(cfg, root)?));
    ENGINE
        .set(engine.clone())
        .map_err(|_| "Legacy engine already attached.")?;
    Ok(VulcanPluginRuntimeManager::new(
        engine,
        bundled,
        user,
        bridge,
        distribution,
        root.join("plugin_state"),
        root.join("plugin_runtime"),
    ))
}

/// Observe this original engine's exact active System leases and manager cleanup diagnostics.
/// 观测此原始引擎的精确活动 System 租约及管理器清理诊断。
/// Return available public facts; absent formal counters are null rather than fabricated zeros.
/// 返回可用公开事实；缺失正式计数使用 null，而非伪造零值。
fn observe(manager: &VulcanPluginRuntimeManager) -> Result<Value, Failure> {
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let engine = ENGINE
        .get()
        .ok_or("Legacy engine is absent.")?
        .read()
        .map_err(|_| "Legacy engine poisoned.")?;
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let listed: Value = serde_json::from_str(&engine.list_system_runtime_leases_json("{}")?)?;
    if listed["ok"] != true {
        return Err(format!("System lease observation failed: {listed}").into());
    }
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let leases = listed["leases"]
        .as_array()
        .ok_or("System lease list is not an array.")?;
    Ok(
        json!({"system_leases":leases,"system_lease_count":leases.len(),"cleanup_diagnostics":null,"formal_operations":null,"formal_resources":null,"formal_retained_counts":null}),
    )
}

/// Legacy stateless calls do not expose a retained owner domain; reject that unsupported gate.
/// 旧无状态调用不暴露保留所有者域；拒绝此不支持门禁。
fn owner_live(_manager: &VulcanPluginRuntimeManager, _owner: &str) -> Result<Value, Failure> {
    Err("Legacy public API has no reusable owner domain.".into())
}

/// Observe manager's actual original System leases for the production UiAction; return only public legacy facts.
/// 观测生产 UiAction 的 manager 实际原 System 租约；仅返回公开旧运行时事实。
fn observe_action(manager: &VulcanPluginRuntimeManager) -> Result<Value, Failure> {
    observe(manager)
}

/// Drain manager's exact production plugin within cfg's deadline; return actual UiAction retirement counts.
/// 在 cfg 截止时间内排空 manager 的精确生产插件；返回实际 UiAction 退役计数。
/// Retained failures remain errors; the existing close separately verifies original System lease absence.
/// 保留失败仍为错误；既有 close 另行验证原 System 租约缺失。
async fn drain_action(
    manager: &VulcanPluginRuntimeManager,
    cfg: &Config,
) -> Result<Value, Failure> {
    // The actual plugin owns its persistent UI runtime independently from any synthetic tool owner.
    // 实际插件拥有持久界面运行时，独立于任何合成工具所有者。
    let report = tokio::time::timeout(
        Duration::from_millis(cfg.cleanup_timeout_ms),
        manager.drain_plugin_sessions(common::SETTINGS_GET_PLUGIN),
    )
    .await??;
    if !report.failures.is_empty() {
        return Err(format!("Production legacy UiAction drain failed: {report:?}").into());
    }
    Ok(
        json!({"closed_ui_action_runtimes":report.closed_ui_action_runtimes,"closed_hook_runtimes":report.closed_hook_runtimes,"closed_sessions":report.closed_sessions,"closed_retained_leases":report.closed_retained_leases,"failures":report.failures}),
    )
}

/// Legacy manager owns cleanup inside invoke; no formal acknowledgement API is claimed.
/// 旧管理器在 invoke 内持有清理；不宣称存在正式确认 API。
/// manager/owner need no extra action; return unit.
/// manager、owner 无需附加动作；返回空值。
fn acknowledge(_manager: &VulcanPluginRuntimeManager, _owner: &str) -> Result<(), Failure> {
    Ok(())
}

/// Drain exact owner through the verified old public API within cfg; fail retained cleanup diagnostics.
/// 在 cfg 内通过已核实旧公开 API 排空精确 owner；保留清理诊断时失败。
async fn drain(
    manager: &VulcanPluginRuntimeManager,
    owner: &str,
    cfg: &Config,
) -> Result<(), Failure> {
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let report = tokio::time::timeout(
        Duration::from_millis(cfg.cleanup_timeout_ms),
        manager.drain_owner_sessions(owner),
    )
    .await??;
    if !report.failures.is_empty() {
        return Err(format!("Legacy drain failed: {:?}", report.failures).into());
    }
    Ok(())
}

/// Verify original old bridge leases physically disappeared; old API exposes no formal parent close.
/// 验证原旧桥租约物理消失；旧 API 不暴露正式父级关闭。
/// manager is observed after the workload; cfg adds no unsupported parent claim; return unit or error.
/// 负载后观测 manager；cfg 不添加不支持父级声明；返回空值或错误。
async fn close(manager: &VulcanPluginRuntimeManager, _cfg: &Config) -> Result<(), Failure> {
    if observe(manager)?["system_lease_count"] != 0 {
        return Err("Legacy System leases remain after complete invocation/drain.".into());
    }
    Ok(())
}

/// Execute one bounded full old bridge run; return failure as a nonzero process exit.
/// 执行单次有限完整旧桥运行；以非零进程退出返回失败。
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Failure> {
    common::run().await
}
