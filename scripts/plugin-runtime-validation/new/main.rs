//! Complete formal-host consumer; never submits directly to the embedded core.
//! 完整正式宿主消费者；绝不直接向嵌入式核心提交。

#[path = "../common.rs"]
mod common;
use common::{Config, Failure};
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{Receiver, TrySendError, sync_channel},
    },
    time::{Duration, Instant},
};
use vulcan_plugin_runtime::{
    VulcanPluginCapacityPurpose, VulcanPluginRuntimeManager, VulcanPluginRuntimeSettings,
};

/// Sole opt-in environment key for a separate private diagnostic artifact, never a benchmark config field.
/// 独立私有诊断产物的唯一显式环境开关键，绝不是基准配置字段。
const DIAGNOSTIC_JSONL_ENV: &str = "VULCAN_PLUGIN_RUNTIME_DIAGNOSTIC_JSONL";

/// Sole bounded diagnostic message capacity; callback overflow is an explicit capture failure.
/// 唯一有界诊断消息容量；回调溢出是明确捕获失败。
const DIAGNOSTIC_MESSAGE_CAPACITY: usize = 4096;

/// Optional diagnostic capture uses the existing process logger without callback I/O, threads or runtime ownership.
/// 可选诊断捕获使用既有进程日志器，回调不执行 I/O、不新增线程，也不拥有运行时。
struct DiagnosticCapture {
    /// Create-new artifact retained for writing only after the original workload has returned.
    /// 以 create-new 创建的产物，仅在原负载返回后写入。
    output: BufWriter<File>,
    /// Bounded messages delivered by the actual synchronous logger callback.
    /// 实际同步日志回调投递的有界消息。
    messages: Receiver<String>,
    /// Actual discarded message count when the bounded receiver is full.
    /// 有界接收器满时实际丢弃的消息数量。
    dropped: Arc<AtomicUsize>,
    /// Actual disconnected receiver failures, distinct from capacity overflow.
    /// 实际接收器断开失败，与容量溢出区分。
    disconnected: Arc<AtomicBool>,
}

impl DiagnosticCapture {
    /// Read the explicit environment path and register capture before manager initialization.
    /// 读取显式环境路径，并在管理器初始化前注册捕获。
    /// Return None with no channel, file or logger change when unset; reject relative or existing artifact paths.
    /// 未设置时返回 None，不创建通道、文件或改动日志器；拒绝相对或已存在产物路径。
    fn from_environment() -> Result<Option<Self>, Failure> {
        // The missing environment key preserves the original logger-disabled benchmark route.
        // 环境键缺失时保留原日志关闭基准路径。
        let Some(path) = std::env::var_os(DIAGNOSTIC_JSONL_ENV) else {
            return Ok(None);
        };
        // Preserve the exact native path representation supplied by the caller.
        // 保留调用方提供的精确原生路径表示。
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(
                format!("{DIAGNOSTIC_JSONL_ENV} requires an absolute create-new path.").into(),
            );
        }
        // Open before business execution so an existing artifact is rejected without overwriting prior evidence.
        // 在业务执行前打开，使已存在产物被拒绝且不覆盖先前证据。
        let output = BufWriter::new(OpenOptions::new().write(true).create_new(true).open(path)?);
        // No consumer thread is introduced; bounded messages are drained after the original run returns.
        // 不引入消费线程；有界消息在原运行返回后排空。
        let (sender, messages) = sync_channel(DIAGNOSTIC_MESSAGE_CAPACITY);
        // Keep overflow observation independent from original business outcomes.
        // 使溢出观测独立于原业务结果。
        let dropped = Arc::new(AtomicUsize::new(0));
        // Distinguish actual disconnection from a full bounded channel.
        // 将实际断开与有界通道满区分。
        let disconnected = Arc::new(AtomicBool::new(false));
        // Callback receives only transport and fixed diagnostic-counter ownership.
        // 回调仅接收传输及固定诊断计数所有权。
        let callback_dropped = Arc::clone(&dropped);
        // No captured manager, engine, VM or runtime can form a logger ownership cycle.
        // 不捕获管理器、引擎、VM 或运行时，避免形成日志器所有权循环。
        let callback_disconnected = Arc::clone(&disconnected);
        luaskills::set_log_callback(Some(Arc::new(move |event| {
            match sender.try_send(event.message.clone()) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    let _ = callback_dropped.fetch_update(
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                        |previous| Some(previous.saturating_add(1)),
                    );
                }
                Err(TrySendError::Disconnected(_)) => {
                    callback_disconnected.store(true, Ordering::Relaxed);
                }
            }
        })));
        Ok(Some(Self {
            output,
            messages,
            dropped,
            disconnected,
        }))
    }

    /// Write actual version-one messages after logger removal, preserving raw diagnostic JSON and bounded failure facts.
    /// 在日志器移除后写入实际版本一消息，保留原诊断 JSON 及有界失败事实。
    /// run_returned_ok confirms the original public close joined retirement; Err cannot authorize complete retirement evidence.
    /// run_returned_ok 确认原公开关闭已等待退役退出；Err 不能允许完整退役证据声明。
    fn finish(mut self, run_returned_ok: bool) -> Result<(), Failure> {
        // Count actual received log messages separately from selected private diagnostic events.
        // 将实际接收日志消息与选取私有诊断事件分开计数。
        let mut received = 0usize;
        // Only the exact existing diagnostic marker is accepted; ordinary public messages remain outside this artifact.
        // 仅接受精确既有诊断标记；普通公开消息不进入此产物。
        let mut diagnostic_events = 0usize;
        for message in self.messages.try_iter() {
            received += 1;
            if let Ok(event) = serde_json::from_str::<Value>(&message)
                && event.get("luaskills_embedded_diagnostic") == Some(&json!(1))
            {
                writeln!(self.output, "{message}")?;
                diagnostic_events += 1;
            }
        }
        // These counters describe real callback transport outcomes, not configured limits or assumed delivery.
        // 这些计数描述真实回调传输结果，而非配置上限或假定投递。
        let dropped = self.dropped.load(Ordering::Relaxed);
        // A disconnected receiver cannot be silently treated as a successfully empty capture.
        // 已断开接收器不能被静默视为成功的空捕获。
        let disconnected = self.disconnected.load(Ordering::Relaxed);
        writeln!(
            self.output,
            "{}",
            json!({
                "event": "diagnostic_capture",
                "diagnostic_version": 1,
                "capacity": DIAGNOSTIC_MESSAGE_CAPACITY,
                "received_log_messages": received,
                "diagnostic_events": diagnostic_events,
                "dropped_log_messages": dropped,
                "receiver_disconnected": disconnected,
                "original_run_returned_ok": run_returned_ok,
                "retired_delivery_complete": run_returned_ok && dropped == 0 && !disconnected,
                "scope": "optional_diagnostic_only",
            })
        )?;
        self.output.flush()?;
        if dropped != 0 || disconnected {
            return Err(format!("Optional diagnostic capture failed: dropped={dropped}, receiver_disconnected={disconnected}.").into());
        }
        Ok(())
    }
}

impl Drop for DiagnosticCapture {
    /// Clear this standalone consumer's logger even when artifact writing unwinds or returns an error.
    /// 即使产物写入展开或返回错误，也清除此独立消费者的日志器。
    fn drop(&mut self) {
        luaskills::set_log_callback(None);
    }
}
/// Construct the public manager from cfg/root and initialize its exact formal settings; return manager.
/// 使用 cfg、root 构建公开管理器并初始化精确正式设置；返回管理器。
fn build(cfg: &Config, root: &Path) -> Result<VulcanPluginRuntimeManager, Failure> {
    if cfg.owner_cycles > 0 && cfg.group == "public_single" {
        return Err("SingleCall has no retained reusable owner domain for the cycle gate.".into());
    }
    if ![
        "schemaVersion",
        "parent",
        "pluginBudgets",
        "capacityBudgets",
        "automaticPrewarm",
        "maxCapabilityCallMs",
        "shutdownTimeoutMs",
    ]
    .iter()
    .all(|name| cfg.formal_settings.get(name).is_some())
    {
        return Err("All public formal settings fields must be frozen explicitly.".into());
    }
    if (cfg.workload == "settings_get_action" && cfg.group != "formal_ui_action")
        || (cfg.workload != "settings_get_action"
            && !["public_single", "public_reusable", "dedicated_reusable"]
                .contains(&cfg.group.as_str()))
    {
        return Err("Wrong formal consumer group.".into());
    }
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let settings: VulcanPluginRuntimeSettings =
        serde_json::from_value(cfg.formal_settings.clone())?;
    // The explicit workload selects its real plugin and capacity purpose, never a compatible tool fallback.
    // 显式负载选择真实插件及容量用途，绝不选择兼容工具回退。
    let (plugin_id, purpose) = if cfg.workload == "settings_get_action" {
        if !settings
            .plugin_budgets
            .contains_key(common::SETTINGS_GET_PLUGIN)
        {
            return Err("Production UiAction requires an explicit aggregate plugin budget.".into());
        }
        (
            common::SETTINGS_GET_PLUGIN,
            VulcanPluginCapacityPurpose::UiAction,
        )
    } else {
        ("validation-plugin", VulcanPluginCapacityPurpose::ToolCall)
    };
    // Require the declared complete public plugin capacity map; the host's private resolver is not an SDK API.
    // 要求已声明的完整公开插件容量映射；宿主私有解析器不是 SDK API。
    let purposes = settings
        .capacity_budgets
        .get(plugin_id)
        .ok_or("Validation settings require an explicit plugin capacity map.")?;
    // Consume only the exact declared workload purpose; missing declarations never select another purpose.
    // 仅消费精确声明负载用途；缺失声明绝不选择其他用途。
    let capacity = purposes
        .get(&purpose)
        .ok_or("Validation settings require a complete explicit workload capacity.")?;
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let dedicated = capacity.resources.kind == luaskills::runtime::embedded::PoolKind::Dedicated;
    if cfg.workload != "settings_get_action" && dedicated != (cfg.group == "dedicated_reusable") {
        return Err("Frozen group does not match actual ToolCall capacity kind.".into());
    }
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let bundled = common::package(
        cfg,
        root,
        Some(luaskills::ffi_embedded::embedded_core_description().protocol_version),
    )?;
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
    let manager = VulcanPluginRuntimeManager::new(
        Arc::new(common::engine(cfg, root)?),
        bundled,
        user,
        bridge,
        distribution,
        root.join("plugin_state"),
        root.join("plugin_runtime"),
    );
    manager.initialize_formal_runtime(settings)?;
    Ok(manager)
}

/// Observe actual public parent/plugin counters of manager; return stable resource and retained accounting.
/// 观测 manager 的真实公开父级及插件计数；返回稳定资源及保留记账。
fn observe(manager: &VulcanPluginRuntimeManager) -> Result<Value, Failure> {
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let runtime = manager
        .formal_runtime_status()?
        .ok_or("Formal runtime is missing.")?;
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let plugin = manager.formal_plugin_status("validation-plugin")?;
    Ok(
        json!({"runtime_id":runtime.runtime_id,"operations":runtime.operations,"resources":runtime.resources,"plugin":plugin,"cleanup_diagnostics":manager.has_cleanup_diagnostics("validation-plugin")?}),
    )
}

/// Observe manager's original parent and production plugin counters for the actual UiAction purpose.
/// 观测实际 UiAction 用途的 manager 原父级及生产插件计数。
/// Return public snapshots and actual cleanup diagnostics, including absent pre-initialization plugin ownership.
/// 返回公开快照及实际清理诊断，包含初始化前缺失的插件归属。
fn observe_action(manager: &VulcanPluginRuntimeManager) -> Result<Value, Failure> {
    // The same retained parent reports real work and resources before and after physical closure.
    // 相同保留父级报告物理关闭前后的真实工作及资源。
    let runtime = manager
        .formal_runtime_status()?
        .ok_or("Formal parent is absent.")?;
    // Exact production identity selects aggregate counters rather than the synthetic tool namespace.
    // 精确生产身份选择聚合计数，而非合成工具命名空间。
    let plugin = manager.formal_plugin_status(common::SETTINGS_GET_PLUGIN)?;
    Ok(
        json!({"runtime_id":runtime.runtime_id,"operations":runtime.operations,"resources":runtime.resources,"plugin":plugin,"capacity_purpose":"UiAction","cleanup_diagnostics":manager.has_cleanup_diagnostics(common::SETTINGS_GET_PLUGIN)?}),
    )
}

/// Drain manager's exact production plugin within cfg's deadline; return actual fixed UiAction retirement counts.
/// 在 cfg 截止时间内排空 manager 的精确生产插件；返回实际固定 UiAction 退役计数。
/// Retained failures remain errors; original public parent close separately confirms all physical work is finished.
/// 保留失败仍为错误；原公开父级关闭另行确认全部物理工作完成。
async fn drain_action(
    manager: &VulcanPluginRuntimeManager,
    cfg: &Config,
) -> Result<Value, Failure> {
    // The real production registry owns the fixed UiAction independently of reusable tool domains.
    // 真实生产注册表拥有固定 UiAction，独立于复用工具域。
    let report = tokio::time::timeout(
        Duration::from_millis(cfg.cleanup_timeout_ms),
        manager.drain_plugin_sessions(common::SETTINGS_GET_PLUGIN),
    )
    .await??;
    if !report.failures.is_empty() {
        return Err(format!("Production formal UiAction drain failed: {report:?}").into());
    }
    if manager.has_cleanup_diagnostics(common::SETTINGS_GET_PLUGIN)? {
        return Err("Production formal UiAction retains cleanup diagnostics after drain.".into());
    }
    Ok(
        json!({"closed_ui_action_runtimes":report.closed_ui_action_runtimes,"closed_hook_runtimes":report.closed_hook_runtimes,"closed_sessions":report.closed_sessions,"closed_formal_calls":report.closed_formal_calls,"failures":report.failures}),
    )
}

/// Return actual controller-owned domains after invocation; no request counter substitutes for domain creation.
/// 返回调用后真实控制器所有域；不使用请求计数替代域创建。
fn owner_live(manager: &VulcanPluginRuntimeManager, owner: &str) -> Result<Value, Failure> {
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let domains = manager.reusable_tool_domains(owner)?;
    if domains.is_empty()
        || domains
            .iter()
            .any(|domain| domain.pool_id.is_none() || domain.pool_retired)
    {
        return Err("Owner cycle requires one actual live reusable domain; use a reusable group and non-expiring limits.".into());
    }
    Ok(json!({"domains":domains,"calls":manager.formal_calls(owner)?}))
}

/// Acknowledge only manager's retired records owned by owner; unfinished cleanup is an explicit error.
/// 仅确认 manager 中 owner 拥有的已退役记录；未完成清理保持明确错误。
fn acknowledge(manager: &VulcanPluginRuntimeManager, owner: &str) -> Result<(), Failure> {
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    for call in manager.formal_calls(owner)? {
        if !call.retired || call.requires_cleanup_recovery {
            return Err("Invocation returned with unfinished/failed physical cleanup.".into());
        }
        manager.forget_formal_call(&call.invocation_id, owner)?;
    }
    Ok(())
}

/// Drain owner through the full manager API within cfg's deadline; require actual domains/calls disappearance.
/// 在 cfg 截止时间内通过完整管理器 API 排空 owner；要求真实域及调用消失。
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
        return Err(format!("Owner physical drain failed: {:?}", report.failures).into());
    }
    acknowledge(manager, owner)?;
    if !manager.reusable_tool_domains(owner)?.is_empty()
        || !manager.formal_calls(owner)?.is_empty()
        || !manager.formal_sessions(owner)?.is_empty()
    {
        return Err("Drained owner still has retained domains/calls/sessions.".into());
    }
    Ok(())
}

/// Close this original manager parent using public control and poll actual physical termination within cfg.
/// 使用公开控制关闭此原始 manager 父级，并在 cfg 内轮询真实物理终止。
async fn close(manager: &VulcanPluginRuntimeManager, cfg: &Config) -> Result<(), Failure> {
    manager.request_formal_runtime_close()?;
    // Bind the concrete local value for this exact host operation.
    // 为此精确宿主操作绑定具体本地值。
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(cfg.cleanup_timeout_ms))
        .ok_or("Cleanup deadline overflow.")?;
    while !manager.poll_formal_runtime_closed()? {
        if Instant::now() >= deadline {
            return Err("Formal parent failed physical close deadline.".into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Ok(())
}

/// Execute one bounded complete-host run; return failure as a nonzero process exit.
/// 执行单次有限完整宿主运行；以非零进程退出返回失败。
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Failure> {
    // Without explicit opt-in, retain the original complete-host route without diagnostic buffering or file I/O.
    // 未显式启用时，保留原完整宿主路径，不进行诊断缓冲或文件 I/O。
    let Some(diagnostics) = DiagnosticCapture::from_environment()? else {
        return common::run().await;
    };
    // Preserve the original business result before any diagnostic output can fail.
    // 在任何诊断输出可能失败前保留原业务结果。
    let result = common::run().await;
    // Successful original public close joins retirement; callback delivery finishes before clearing its registration.
    // 成功原公开关闭等待退役退出；回调投递在清除其注册前完成。
    luaskills::set_log_callback(None);
    // Failed runs retain partial observations without claiming complete retirement delivery.
    // 失败运行保留局部观测，不声称完整退役投递。
    let capture_result = diagnostics.finish(result.is_ok());
    match result {
        Err(error) => {
            if let Err(capture_error) = capture_result {
                eprintln!(
                    "Optional diagnostic artifact failed after original run failure: {capture_error}"
                );
            }
            Err(error)
        }
        Ok(()) => capture_result,
    }
}
