//! LuaSkills runtime bootstrap helpers layered on top of the in-process storage engine.
//! 构建在进程内存储引擎之上的 LuaSkills 运行时引导辅助模块。

use crate::model::SessionId;
use crate::project_temp_maintenance;
use crate::runtime_assets;
use crate::tool_result_format::{
    EffectiveBudgetScope, HostRenderOptions, HostTextRenderInput, HostToolOverflowMode,
    ToolResultBudgetSnapshot, render_tool_result_text,
};
use crate::tooling::change_set::{
    change_set_host_verification_notice, luaskills_change_set_host_result_capabilities,
};
use futures_channel::oneshot;
use luaskills::runtime_options::LuaRuntimeRunLuaPoolConfig;
use luaskills::skill::manager::collect_effective_skill_instances_from_roots;
use luaskills::{
    LuaEngine, LuaEngineOptions, LuaInvocationContext, LuaRuntimeCapabilityOptions,
    LuaRuntimeDatabaseProviderMode, LuaRuntimeHostOptions, LuaRuntimeManagedRuntimeConfig,
    LuaVmPoolConfig, RuntimeClientInfo, RuntimeEntryDescriptor, RuntimeInvocationResult,
    RuntimeRequestContext, RuntimeSkillRoot, SkillInstallRequest, SkillInstallSourceType,
    SkillManagementAuthority, SkillManager, SkillManagerConfig, SkillMeta, SkillUninstallOptions,
    ToolCacheConfig,
};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use vulcan_luaskills_config::{LuaSkillsRuntimeSettings, load_runtime_settings};

pub mod database_bridge;
pub mod database_callbacks;
pub mod host_database_providers;
mod runlua;

/// Process-local sequence used to correlate all diagnostics from one LuaSkills host invocation.
/// 用于关联同一次 LuaSkills 宿主调用全部诊断的进程内序列。
static LUASKILLS_HOST_INVOCATION_SEQUENCE: AtomicU64 = AtomicU64::new(1);

mod schema;
mod types;

#[cfg(test)]
mod concurrency_tests;

pub use runlua::{
    RUNLUA_SESSION_TOOL_NAME, RUNLUA_TOOL_NAME, build_runlua_tool_candidates,
    set_runlua_ai_tools_enabled,
};
pub use schema::{
    LUASKILL_SID_PARAMETER_NAME, LUASKILLS_HOST_MANAGED_SID_PREFIX, PWD_PARAMETER_NAME,
    luaskills_entry_args_schema,
};
pub use types::*;
use types::{
    RootSkillBootstrapConfig, RootSkillBootstrapEntry, RunLuaOwnedSession, RunLuaSessionRegistry,
};

/// Bootstraps one minimal LuaSkills runtime after the in-process data plane is ready.
/// 在进程内数据平面就绪后引导一个最小化的 LuaSkills 运行时。
pub async fn bootstrap_luaskills_runtime() -> Result<LuaSkillsBootstrapReport, String> {
    run_luaskills_blocking_task("luaskills-bootstrap", move || {
        bootstrap_luaskills_runtime_blocking()
    })
}

/// Bootstraps one minimal LuaSkills runtime inside one blocking host thread.
/// 在单个阻塞宿主线程内部引导一个最小化的 LuaSkills 运行时。
fn bootstrap_luaskills_runtime_blocking() -> Result<LuaSkillsBootstrapReport, String> {
    let runtime_root = application_luaskills_runtime_root().ok_or_else(|| {
        String::from("未找到 LuaSkills 活动运行根，请先执行 make sync 并从 output 目录启动应用。")
    })?;
    // runtime_root pins every derived host path to one absolute native directory identity while removing Windows verbatim prefixes before the path reaches LuaSkills.
    // runtime_root 将所有派生宿主路径固定到一个绝对原生目录身份，并在路径进入 LuaSkills 前移除 Windows verbatim 前缀。
    let runtime_root =
        runtime_assets::native_host_path(&fs::canonicalize(&runtime_root).map_err(|error| {
            format!(
                "规范化 LuaSkills 运行时根 `{}` 失败：{error}",
                runtime_root.display()
            )
        })?);
    // output_config_directory is the only runtime configuration root accepted by Vulcan Code.
    // output_config_directory 是 Vulcan Code 唯一接受的运行时配置根目录。
    let output_config_directory = runtime_assets::resolve_output_config_directory()
        .ok_or_else(|| String::from("未找到 Vulcan output/config 目录。"))?;
    // runtime_settings is the Vulcan-owned startup configuration shared with the service engine.
    // runtime_settings 是与服务引擎共享的 Vulcan 自有启动配置。
    let runtime_settings = load_runtime_settings(&output_config_directory)?;
    // managed_runtime_distribution_root is shared with host consumers outside LuaSkills.
    // managed_runtime_distribution_root 与 LuaSkills 之外的宿主消费方共享。
    let managed_runtime_distribution_root =
        runtime_assets::resolve_managed_runtime_distribution_root()
            .ok_or_else(|| String::from("未找到 Vulcan 公共 Python/Node 发行根目录。"))?;
    fs::create_dir_all(&managed_runtime_distribution_root).map_err(|error| {
        format!(
            "创建公共 Python/Node 发行根 `{}` 失败：{error}",
            managed_runtime_distribution_root.display()
        )
    })?;
    // managed_runtime_distribution_root uses the same native-path boundary required by LuaSkills host options.
    // managed_runtime_distribution_root 使用 LuaSkills 宿主选项要求的同一原生路径边界。
    let managed_runtime_distribution_root = runtime_assets::native_host_path(
        &fs::canonicalize(&managed_runtime_distribution_root).map_err(|error| {
            format!(
                "规范化公共 Python/Node 发行根 `{}` 失败：{error}",
                managed_runtime_distribution_root.display()
            )
        })?,
    );
    let skill_roots = ensure_runtime_root_layout(&runtime_root)?;
    // skill_config_root is the single host-owned package configuration root shared by every LuaSkills engine.
    // skill_config_root 是所有 LuaSkills 引擎共享的唯一宿主持有技能包配置根目录。
    let skill_config_root = crate::data_plane::application_luaskills_config_root()?;
    let system_lua_lib_dir = runtime_root.join("system_lua_lib");
    let host_options = build_host_options(
        &runtime_root,
        &managed_runtime_distribution_root,
        &skill_config_root,
        &runtime_settings,
    );
    let pool_config = LuaVmPoolConfig {
        min_size: runtime_settings.vm_pool.min_size,
        max_size: runtime_settings.vm_pool.max_size,
        idle_ttl_secs: runtime_settings.vm_pool.idle_ttl_secs,
    };
    let mut engine = LuaEngine::new(LuaEngineOptions::new(pool_config, host_options))
        .map_err(|error| format!("创建 LuaSkills 引擎失败：{error}"))?;

    ensure_configured_root_skills_installed(&skill_roots, &mut engine)?;

    engine
        .load_from_roots(&skill_roots)
        .map_err(|error| format!("加载 LuaSkills 技能根失败：{error}"))?;

    let tool_entries = list_host_visible_luaskills_entries(&engine)?;
    let entry_count = tool_entries.len();
    let engine = Arc::new(RwLock::new(engine));
    Ok(LuaSkillsBootstrapReport {
        handle: LuaSkillsRuntimeHandle {
            engine,
            runtime_root,
            system_lua_lib_dir,
            skill_roots,
            entry_count,
            tool_entries: Arc::new(Mutex::new(tool_entries)),
            skill_call_admission: Arc::new(Semaphore::new(
                runtime_settings.vm_pool.max_size.max(1),
            )),
            runlua_call_admission: Arc::new(Semaphore::new(
                runtime_settings.run_lua_pool.max_size.max(1),
            )),
            runlua_session_call_admission: Arc::new(Semaphore::new(
                luaskills::MAX_RUNTIME_SESSION_LEASES_PER_MANAGER,
            )),
            runlua_ai_tool_enabled: Arc::new(AtomicBool::new(
                runtime_settings.ai_tools.enable_run_lua,
            )),
            runlua_session_ai_tool_enabled: Arc::new(AtomicBool::new(
                runtime_settings.ai_tools.enable_run_lua_session,
            )),
            runlua_sessions: Arc::new(Mutex::new(RunLuaSessionRegistry::default())),
        },
    })
}

/// Execute one LuaSkills host operation on a dedicated blocking thread and return its result.
/// 在专用阻塞线程上执行单次 LuaSkills 宿主操作，并返回其结果。
fn run_luaskills_blocking_task<T, F>(thread_name: &str, operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    std::thread::Builder::new()
        .name(thread_name.to_string())
        .spawn(operation)
        .map_err(|error| format!("启动 LuaSkills 阻塞线程失败：{error}"))?
        .join()
        .map_err(|panic_payload| {
            let panic_message = if let Some(message) = panic_payload.downcast_ref::<String>() {
                message.clone()
            } else if let Some(message) = panic_payload.downcast_ref::<&'static str>() {
                (*message).to_string()
            } else {
                String::from("未知 panic")
            };
            format!("LuaSkills 阻塞线程执行失败：{panic_message}")
        })?
}

/// Execute one LuaSkills host operation on a dedicated blocking thread without stalling the async caller.
/// 在专用阻塞线程上执行单次 LuaSkills 宿主操作，并避免阻塞异步调用方。
async fn run_luaskills_blocking_task_async_with_diagnostics<T, F>(
    thread_name: &str,
    operation_label: String,
    admission: Arc<Semaphore>,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(Arc<AtomicU8>, u64) -> Result<T, String> + Send + 'static,
{
    // stage distinguishes queueing, engine-lock contention, skill execution, and terminal delivery.
    // stage 用于区分排队、引擎锁竞争、技能执行与终态投递。
    let stage = Arc::new(AtomicU8::new(0));
    let thread_name = thread_name.to_string();
    // invocation_id correlates queue, lock, body, and result-delivery diagnostics for one call.
    // invocation_id 关联一次调用的排队、锁、技能体和结果投递诊断。
    let invocation_id = LUASKILLS_HOST_INVOCATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    // started_at includes admission wait so queue pressure remains observable before any OS thread exists.
    // started_at 包含入场等待，使系统线程尚未创建时的排队压力仍可观测。
    let started_at = Instant::now();
    // permit_future queues only the async caller and is cancelled without creating a worker thread.
    // permit_future 只排队异步调用方；取消时不会创建工作线程。
    let permit_future = admission.acquire_owned();
    tokio::pin!(permit_future);
    // next_diagnostic_delay emits once at five seconds, again at thirty seconds, then once per minute.
    // next_diagnostic_delay 在五秒、三十秒以及之后每分钟输出一次诊断。
    let mut next_diagnostic_delay = Duration::from_secs(5);
    let permit = loop {
        tokio::select! {
            permit = &mut permit_future => {
                break permit.map_err(|_| format!(
                    "LuaSkills 调用 `{operation_label}` 的宿主入场器已关闭。"
                ))?;
            }
            _ = tokio::time::sleep(next_diagnostic_delay) => {
                let elapsed_ms = started_at.elapsed().as_millis();
                eprintln!(
                    "[luaskills-runtime][invoke-still-running] invocation_id={invocation_id} operation={operation_label} stage=host_admission_wait elapsed_ms={elapsed_ms}"
                );
                next_diagnostic_delay = if elapsed_ms < 30_000 {
                    Duration::from_secs(25)
                } else {
                    Duration::from_secs(60)
                };
            }
        }
    };

    let (sender, receiver) = oneshot::channel();
    let worker_stage = Arc::clone(&stage);

    std::thread::Builder::new()
        .name(thread_name.clone())
        .spawn(move || {
            // permit stays owned by the worker even if the async waiter is cancelled.
            // 即使异步等待方被取消，permit 仍由工作线程持有。
            let _permit = permit;
            worker_stage.store(1, Ordering::Release);
            let operation_stage = Arc::clone(&worker_stage);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                operation(operation_stage, invocation_id)
            }))
            .map_err(|panic_payload| {
                let panic_message = if let Some(message) = panic_payload.downcast_ref::<String>() {
                    message.clone()
                } else if let Some(message) = panic_payload.downcast_ref::<&'static str>() {
                    (*message).to_string()
                } else {
                    String::from("未知 panic")
                };
                format!("LuaSkills 阻塞线程执行失败：{panic_message}")
            })
            .and_then(|value| value);
            worker_stage.store(3, Ordering::Release);
            let _ = sender.send(result);
        })
        .map_err(|error| format!("启动 LuaSkills 阻塞线程失败：{error}"))?;

    // receiver remains pinned to the same worker result while diagnostics wake independently.
    // receiver 在诊断独立唤醒期间始终绑定同一工作线程结果。
    let mut receiver = receiver;
    // Reset diagnostics after admission while preserving total elapsed time in emitted measurements.
    // 入场后重置诊断周期，同时在输出测量中保留总耗时。
    next_diagnostic_delay = Duration::from_secs(5);
    loop {
        tokio::select! {
            result = &mut receiver => {
                return result
                    .map_err(|_| format!("LuaSkills 阻塞线程 `{thread_name}` 意外断开。"))?;
            }
            _ = tokio::time::sleep(next_diagnostic_delay) => {
                let elapsed_ms = started_at.elapsed().as_millis();
                let stage_label = match stage.load(Ordering::Acquire) {
                    0 => "thread_queued",
                    1 => "engine_read_lock_wait",
                    2 => "skill_body_or_host_call",
                    3 => "result_delivery",
                    4 => "blocking_runtime_operation",
                    5 => "engine_write_lock_wait",
                    _ => "unknown",
                };
                eprintln!(
                    "[luaskills-runtime][invoke-still-running] invocation_id={invocation_id} operation={operation_label} stage={stage_label} elapsed_ms={elapsed_ms}"
                );
                next_diagnostic_delay = if elapsed_ms < 30_000 {
                    Duration::from_secs(25)
                } else {
                    Duration::from_secs(60)
                };
            }
        }
    }
}

/// Executes one unlabeled LuaSkills host operation through the shared stage watchdog.
/// 通过共享阶段看门狗执行一次无独立标签的 LuaSkills 宿主操作。
///
/// # Parameters
/// 参数
/// `thread_name` is both the worker-thread name and stable diagnostic label.
/// `thread_name` 同时作为工作线程名称与稳定诊断标签。
/// `operation` is the blocking LuaSkills operation executed on that worker.
/// `operation` 是在该工作线程上执行的阻塞 LuaSkills 操作。
///
/// # Returns
/// 返回值
/// The operation result or the exact worker startup/delivery failure.
/// 返回操作结果或精确的工作线程启动/投递失败。
async fn run_luaskills_blocking_task_async<T, F>(
    thread_name: &str,
    admission: Arc<Semaphore>,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    run_luaskills_blocking_task_async_with_diagnostics(
        thread_name,
        thread_name.to_string(),
        admission,
        move |stage, _invocation_id| {
            stage.store(4, Ordering::Release);
            operation()
        },
    )
    .await
}

/// Ensures the configured host-managed ROOT skills are installed before the runtime catalog is loaded.
/// 在加载运行时技能目录之前，确保配置声明的宿主管理 ROOT 技能已经完成安装。
fn ensure_configured_root_skills_installed(
    skill_roots: &[RuntimeSkillRoot],
    engine: &mut LuaEngine,
) -> Result<(), String> {
    let root_target = resolve_named_skill_root(skill_roots, "ROOT")?;
    let configured_skills = load_root_skill_bootstrap_entries()?;
    if configured_skills.is_empty() {
        return Ok(());
    }

    let installed_root_skill_ids =
        collect_effective_skill_instances_from_roots(std::slice::from_ref(&root_target))
            .map_err(|error| format!("解析 ROOT 层技能目录失败：{error}"))?
            .into_iter()
            .map(|instance| instance.skill_id)
            .collect::<std::collections::BTreeSet<_>>();

    let mut installed_any_skill = false;
    for configured_skill in configured_skills {
        if installed_root_skill_ids.contains(configured_skill.skill_id.as_str()) {
            continue;
        }

        // The source type is resolved before the install request is built, so a disabled
        // source type fails with the skill identifier attached instead of a bare policy
        // error, and no partially-built request reaches the engine.
        // 来源类型在构建安装请求之前解析，因此被停用的来源类型会带着技能标识失败，而不是一条
        // 裸的策略错误，并且不会有半成品请求到达引擎。
        let resolved_source_type = configured_skill
            .source_type
            .into_runtime_source_type()
            .map_err(|error| format!("ROOT 引导技能 `{}`：{error}", configured_skill.skill_id))?;
        let install_request = SkillInstallRequest {
            skill_id: Some(configured_skill.skill_id.clone()),
            source: Some(configured_skill.source.clone()),
            source_type: resolved_source_type,
        };
        engine
            .system_install_skill_in_root(
                skill_roots,
                &root_target,
                SkillManagementAuthority::System,
                &install_request,
            )
            .map_err(|error| {
                format!(
                    "安装 ROOT 默认技能 `{}` 失败：{error}",
                    configured_skill.skill_id
                )
            })?;
        installed_any_skill = true;
    }

    if installed_any_skill {
        engine
            .reload_from_roots(skill_roots)
            .map_err(|error| format!("刷新 LuaSkills 技能根失败：{error}"))?;
    }

    Ok(())
}

/// Loads the declarative host-managed ROOT skill bootstrap list only from `output/config`.
/// 仅从 `output/config` 加载声明式宿主管理 ROOT 技能引导列表。
fn load_root_skill_bootstrap_entries() -> Result<Vec<RootSkillBootstrapEntry>, String> {
    let config_path = runtime_assets::resolve_output_config_directory()
        .ok_or_else(|| String::from("未找到 Vulcan output/config 目录。"))?
        .join("luaskills_root_skills.yaml");
    if !config_path.exists() {
        return Ok(Vec::new());
    }
    if !config_path.is_file() {
        return Err(format!(
            "ROOT 技能引导配置不是有效文件：{}",
            config_path.display()
        ));
    }

    let config_text = fs::read_to_string(&config_path).map_err(|error| {
        format!(
            "读取 ROOT 技能引导配置失败（{}）：{error}",
            config_path.display()
        )
    })?;
    let config: RootSkillBootstrapConfig = serde_yaml::from_str(&config_text).map_err(|error| {
        format!(
            "解析 ROOT 技能引导配置失败（{}）：{error}",
            config_path.display()
        )
    })?;

    let mut entries = Vec::with_capacity(config.root_skills.len());
    for entry in config.root_skills {
        let skill_id = entry.skill_id.trim().to_string();
        let source = entry.source.trim().to_string();
        if skill_id.is_empty() {
            return Err(format!(
                "ROOT 技能引导配置存在空 skill_id（{}）。",
                config_path.display()
            ));
        }
        if source.is_empty() {
            return Err(format!(
                "ROOT 技能引导配置 `{}` 的 source 不能为空。",
                skill_id
            ));
        }
        entries.push(RootSkillBootstrapEntry {
            skill_id,
            source,
            source_type: entry.source_type,
        });
    }

    Ok(entries)
}

/// Returns the stable runtime root used by the embedded LuaSkills host integration.
/// 返回嵌入式 LuaSkills 宿主集成使用的稳定运行时根目录。
pub fn application_luaskills_runtime_root() -> Option<PathBuf> {
    runtime_assets::resolve_luaskills_runtime_root()
}

/// Rebuild the visible LuaSkills catalog from the current runtime roots without mutating lifecycle state.
/// 在不修改生命周期状态的前提下，根据当前运行时根重建可见 LuaSkills 清单。
pub async fn load_luaskills_catalog(
    handle: LuaSkillsRuntimeHandle,
) -> Result<LuaSkillsCatalog, String> {
    run_luaskills_blocking_task("luaskills-catalog", move || {
        load_luaskills_catalog_blocking(&handle)
    })
}

/// Rebuild the visible LuaSkills catalog inside one blocking host thread.
/// 在单个阻塞宿主线程内部重建可见 LuaSkills 技能目录清单。
fn load_luaskills_catalog_blocking(
    handle: &LuaSkillsRuntimeHandle,
) -> Result<LuaSkillsCatalog, String> {
    let engine = lock_runtime_engine(handle)?;
    build_luaskills_catalog(handle, &engine)
}

/// Execute one host-side LuaSkills management action and rebuild the visible catalog afterwards.
/// 执行单次宿主侧 LuaSkills 管理动作，并在完成后重建可见清单。
pub async fn perform_luaskills_management_action(
    handle: LuaSkillsRuntimeHandle,
    action: LuaSkillsManagementAction,
) -> Result<LuaSkillsManagementReport, String> {
    run_luaskills_blocking_task("luaskills-management", move || {
        perform_luaskills_management_action_blocking(&handle, action)
    })
}

/// Execute one host-side LuaSkills management action inside one blocking host thread.
/// 在单个阻塞宿主线程内部执行单次宿主侧 LuaSkills 管理动作。
fn perform_luaskills_management_action_blocking(
    handle: &LuaSkillsRuntimeHandle,
    action: LuaSkillsManagementAction,
) -> Result<LuaSkillsManagementReport, String> {
    // write_lock_operation stays free of skill ids and source locators while keeping one slow
    // write-lock wait attributable to the management action that requested it.
    // write_lock_operation 不含技能标识与来源定位值，同时让一次写锁慢等待可归因到发起它的管理动作。
    let write_lock_operation = action.diagnostic_label();
    // write_lock_started_at measures only host write-lock contention against in-flight readers.
    // write_lock_started_at 仅测量宿主写锁相对在途读锁的竞争耗时。
    let write_lock_started_at = Instant::now();
    let mut engine = lock_runtime_engine_mut(handle)?;
    let write_lock_wait_ms = write_lock_started_at.elapsed().as_millis();
    if write_lock_wait_ms >= 1_000 {
        eprintln!(
            "[luaskills-runtime][engine-write-lock-slow] operation={write_lock_operation} elapsed_ms={write_lock_wait_ms}"
        );
    }
    let message = match action {
        LuaSkillsManagementAction::Refresh => {
            engine
                .reload_from_roots(&handle.skill_roots)
                .map_err(|error| format!("刷新 LuaSkills 运行时失败：{error}"))?;
            String::from("LuaSkills 技能目录已刷新。")
        }
        LuaSkillsManagementAction::Install {
            skill_id,
            source,
            source_type,
        } => {
            let user_root = ordinary_user_root(handle)?;
            // The source type is resolved before the install request is built, so a
            // disabled source type fails before any partial request reaches the engine.
            // 来源类型在构建安装请求之前解析，因此被停用的来源类型会在任何半成品请求到达
            // 引擎之前失败。
            let resolved_source_type = source_type.into_runtime_source_type()?;
            let request = SkillInstallRequest {
                skill_id: normalize_optional_field(skill_id),
                source: normalize_optional_field(Some(source)),
                source_type: resolved_source_type,
            };
            let result = engine
                .install_skill_in_root(&handle.skill_roots, &user_root, &request)
                .map_err(|error| format!("安装 LuaSkills 技能失败：{error}"))?;
            result.message
        }
        LuaSkillsManagementAction::Update { skill_id } => {
            let target_root = target_root_for_skill(handle, &skill_id)?;
            let request = SkillInstallRequest {
                skill_id: Some(skill_id.clone()),
                source: None,
                source_type: SkillInstallSourceType::Github,
            };
            let result = if target_root.name.eq_ignore_ascii_case("ROOT") {
                engine
                    .system_update_skill_in_root(
                        &handle.skill_roots,
                        &target_root,
                        SkillManagementAuthority::System,
                        &request,
                    )
                    .map_err(|error| format!("更新 LuaSkills 技能失败：{error}"))?
            } else {
                engine
                    .update_skill_in_root(&handle.skill_roots, &target_root, &request)
                    .map_err(|error| format!("更新 LuaSkills 技能失败：{error}"))?
            };
            result.message
        }
        LuaSkillsManagementAction::Enable { skill_id } => {
            let target_root = target_root_for_skill(handle, &skill_id)?;
            if target_root.name.eq_ignore_ascii_case("ROOT") {
                engine
                    .system_enable_skill(
                        &handle.skill_roots,
                        SkillManagementAuthority::System,
                        &skill_id,
                    )
                    .map_err(|error| format!("启用 LuaSkills 技能失败：{error}"))?;
            } else {
                engine
                    .enable_skill(&handle.skill_roots, &skill_id)
                    .map_err(|error| format!("启用 LuaSkills 技能失败：{error}"))?;
            }
            format!("已启用技能 `{skill_id}`。")
        }
        LuaSkillsManagementAction::Disable { skill_id } => {
            let target_root = target_root_for_skill(handle, &skill_id)?;
            if target_root.name.eq_ignore_ascii_case("ROOT") {
                engine
                    .system_disable_skill_in_roots(
                        &handle.skill_roots,
                        SkillManagementAuthority::System,
                        &skill_id,
                        Some("disabled from vulcan-code"),
                    )
                    .map_err(|error| format!("停用 LuaSkills 技能失败：{error}"))?;
            } else {
                engine
                    .disable_skill_in_roots(
                        &handle.skill_roots,
                        &skill_id,
                        Some("disabled from vulcan-code"),
                    )
                    .map_err(|error| format!("停用 LuaSkills 技能失败：{error}"))?;
            }
            format!("已停用技能 `{skill_id}`。")
        }
        LuaSkillsManagementAction::Uninstall { skill_id } => {
            let target_root = ordinary_uninstall_target_root_for_skill(handle, &skill_id)?;
            let result = engine
                .uninstall_skill_in_root(
                    &handle.skill_roots,
                    &target_root,
                    &skill_id,
                    &SkillUninstallOptions::default(),
                )
                .map_err(|error| format!("卸载 LuaSkills 技能失败：{error}"))?;
            result.message
        }
    };

    // Force a post-action reload because LuaSkills update/install can finalize filesystem state after its internal pre-commit reload.
    // 强制执行操作后重载，因为 LuaSkills 更新/安装可能会在其内部提交前重载之后才完成文件系统状态落定。
    engine
        .reload_from_roots(&handle.skill_roots)
        .map_err(|error| format!("同步 LuaSkills 管理操作后的运行时入口失败：{error}"))?;
    // Public runtime leases are replaced by reload, so host ownership must be invalidated atomically with it.
    // 公开运行时租约会被 reload 替换，因此宿主归属必须与其同步失效。
    runlua::clear_runlua_session_registry(handle);

    let catalog = build_luaskills_catalog(handle, &engine)?;
    Ok(LuaSkillsManagementReport { message, catalog })
}

/// List every LuaSkills runtime entry currently visible to the host tool surface.
/// 列出当前对宿主工具表面可见的全部 LuaSkills 运行时入口。
pub fn list_luaskills_tool_entries(
    handle: &LuaSkillsRuntimeHandle,
) -> Result<Vec<RuntimeEntryDescriptor>, String> {
    handle
        .tool_entries
        .lock()
        .map(|entries| entries.clone())
        .map_err(|_| String::from("LuaSkills 工具入口缓存锁已损坏，无法读取工具表。"))
}

/// One compact LuaSkills help summary matched to a runtime entry.
/// 匹配到单个运行时入口的一份紧凑 LuaSkills 帮助摘要。
#[derive(Debug, Clone)]
pub struct LuaSkillsEntryHelpSummary {
    /// Resolved help flow name used for the entry.
    /// 当前入口使用的已解析帮助流程名称。
    pub flow_name: String,
    /// Human-readable help node description.
    /// 帮助节点的人类可读说明。
    pub description: String,
    /// Rendered help body returned by LuaSkills.
    /// LuaSkills 返回的已渲染帮助正文。
    pub content: String,
}

/// Render the best matching LuaSkills help text for one runtime entry.
/// 为单个运行时入口渲染最匹配的 LuaSkills 帮助文本。
pub fn render_luaskills_entry_help_summary(
    handle: &LuaSkillsRuntimeHandle,
    entry: &RuntimeEntryDescriptor,
) -> Result<Option<LuaSkillsEntryHelpSummary>, String> {
    let engine = lock_runtime_engine(handle)?;
    let flow_name = entry.local_name.trim();
    if flow_name.is_empty() {
        return Ok(None);
    }
    let Some(detail) = engine
        .render_skill_help_detail_for_authority(
            SkillManagementAuthority::System,
            &entry.skill_id,
            flow_name,
            None,
        )
        .ok()
        .flatten()
    else {
        return Ok(None);
    };
    Ok(Some(LuaSkillsEntryHelpSummary {
        flow_name: detail.flow_name,
        description: detail.description,
        content: detail.content,
    }))
}

/// Call one LuaSkills runtime entry on a dedicated blocking thread and return the structured result.
/// 在专用阻塞线程上调用单个 LuaSkills 运行时入口，并返回结构化结果。
pub async fn call_luaskills_tool_entry(
    handle: LuaSkillsRuntimeHandle,
    tool_name: String,
    args: Value,
    invocation_context: Option<LuaInvocationContext>,
) -> Result<RuntimeInvocationResult, String> {
    let operation_label = tool_name.clone();
    let admission = Arc::clone(&handle.skill_call_admission);
    run_luaskills_blocking_task_async_with_diagnostics(
        "luaskills-tool-call",
        operation_label,
        admission,
        move |stage, invocation_id| {
        // lock_started_at measures only host read-lock contention for the shared LuaSkills engine.
        // lock_started_at 仅测量共享 LuaSkills 引擎的宿主读锁竞争耗时。
        let lock_started_at = Instant::now();
        let engine = lock_runtime_engine(&handle)?;
        let lock_wait_ms = lock_started_at.elapsed().as_millis();
        if lock_wait_ms >= 1_000 {
                eprintln!(
                    "[luaskills-runtime][engine-read-lock-slow] invocation_id={invocation_id} operation={tool_name} elapsed_ms={lock_wait_ms}"
                );
        }
        stage.store(2, Ordering::Release);
        // invocation_started_at separates engine contention from time spent inside Lua or one host call.
        // invocation_started_at 将引擎竞争耗时与 Lua 或宿主调用内部耗时分离。
        let invocation_started_at = Instant::now();
        let result = engine.call_skill(&tool_name, &args, invocation_context.as_ref());
        let invocation_elapsed_ms = invocation_started_at.elapsed().as_millis();
        if invocation_elapsed_ms >= 1_000 {
                eprintln!(
                    "[luaskills-runtime][skill-call-slow] invocation_id={invocation_id} operation={tool_name} elapsed_ms={invocation_elapsed_ms} status={}",
                if result.is_ok() { "ok" } else { "error" }
            );
        }
        result
        },
    )
    .await
}

/// Render one LuaSkills invocation result into the final host-managed text returned to the model.
/// 将单次 LuaSkills 调用结果渲染为返回给模型的最终宿主管理文本。
///
/// # Parameters
/// # 参数
/// - `handle`: Runtime handle that supplies skill and shared template roots.
/// - `handle`：提供技能模板根目录和共享模板根目录的运行时句柄。
/// - `tool_name`: Canonical tool name used to resolve the client budget.
/// - `tool_name`：用于解析客户端预算的规范工具名称。
/// - `skill_name`: Optional skill identity used to select a local overflow template.
/// - `skill_name`：用于选择本地超限模板的可选技能标识。
/// - `result`: Exact LuaSkills runtime invocation result.
/// - `result`：精确的 LuaSkills 运行时调用结果。
/// - `workspace_root`: Optional project root used to derive the lazy spill path.
/// - `workspace_root`：用于派生延迟超限路径的可选项目根目录。
///
/// # Returns
/// # 返回值
/// Returns rendered model text, or the exact host rendering and persistence failure.
/// 返回渲染后的模型文本，或精确的宿主渲染与持久化失败。
pub fn render_luaskills_invocation_result_text(
    handle: &LuaSkillsRuntimeHandle,
    tool_name: &str,
    skill_name: Option<&str>,
    result: &RuntimeInvocationResult,
    workspace_root: Option<&Path>,
) -> Result<String, String> {
    let client_budget = build_default_luaskills_budget_snapshot(tool_name);
    let spill_root = workspace_root
        .filter(|path| !path.as_os_str().is_empty())
        .map(project_temp_maintenance::resolve_project_temp_dir);
    let render_options = HostRenderOptions {
        spill_root,
        template_skill_roots: handle
            .skill_roots
            .iter()
            .map(|root| root.skills_dir.clone())
            .collect(),
        template_resources_root: Some(handle.runtime_root.join("resources")),
    };

    render_tool_result_text(
        &host_text_render_input_from_runtime_result(result),
        skill_name,
        &client_budget,
        &render_options,
    )
}

/// Convert one LuaSkills runtime result into the host-owned generic text render input.
/// 将单条 LuaSkills 运行时结果转换为宿主自有的通用文本渲染输入。
fn host_text_render_input_from_runtime_result(
    result: &RuntimeInvocationResult,
) -> HostTextRenderInput {
    let mut content = result.content.clone();
    if let Some(host_result) = result.host_result.as_ref()
        && let Some(verification_notice) = (host_result.kind == "change_set")
            .then(|| change_set_host_verification_notice(&host_result.payload))
            .flatten()
    {
        if !content.trim().is_empty() {
            content.push_str("\n\n");
        }
        content.push_str(&verification_notice);
    }
    let normalized = normalize_host_render_text(&content);
    HostTextRenderInput {
        content,
        overflow_mode: result.overflow_mode.map(|mode| match mode {
            luaskills::ToolOverflowMode::Truncate => HostToolOverflowMode::Truncate,
            luaskills::ToolOverflowMode::Page => HostToolOverflowMode::Page,
        }),
        template_hint: result.template_hint.clone(),
        content_bytes: normalized.len(),
        content_lines: if normalized.is_empty() {
            0
        } else {
            normalized.split('\n').count()
        },
    }
}

/// Build one default LuaSkills invocation context used by host-managed tool calls.
/// 构建一份供宿主管理工具调用使用的默认 LuaSkills 调用上下文。
pub fn build_luaskills_tool_invocation_context(
    session_id: Option<SessionId>,
    transport_name: &str,
    tool_name: &str,
) -> LuaInvocationContext {
    let request_context = RuntimeRequestContext {
        request_id: Some(format!("luaskills-tool:{tool_name}")),
        client_name: Some(String::from("vulcan-code")),
        transport_name: Some(transport_name.to_string()),
        session_id: session_id.map(|value| value.to_string()),
        client_info: Some(RuntimeClientInfo {
            kind: Some(String::from("desktop")),
            name: Some(String::from("VulcanCode")),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
        client_capabilities: luaskills_change_set_host_result_capabilities(),
    };
    let client_budget = build_default_luaskills_client_budget(tool_name);

    LuaInvocationContext::new(
        Some(request_context),
        client_budget,
        Value::Object(Map::new()),
    )
}

/// Normalizes host-render text so byte and line metrics stay deterministic after host-side warning injection.
/// 对宿主渲染文本进行规范化，使宿主侧注入警告后的字节与行数统计保持确定性。
fn normalize_host_render_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Builds one default client-budget snapshot for host-driven LuaSkills tool calls with unlimited lines and a 200KB byte ceiling.
/// 为宿主驱动的 LuaSkills 工具调用构建一份默认客户端预算快照，采用无限行与 200KB 字节上限。
fn build_default_luaskills_client_budget(tool_name: &str) -> Value {
    let budget = build_default_luaskills_budget_snapshot(tool_name);
    json!({
        "client_name": "vulcan-code",
        "tool_name": tool_name,
        "matched_client_pattern": "vulcan-code",
        "tool_result": {
            "bytes": budget.tool_result.bytes,
            "lines": budget.tool_result.lines
        },
        "file_read": {
            "bytes": budget.file_read.bytes,
            "lines": budget.file_read.lines
        },
        "tool_config": {}
    })
}

/// Builds one strongly typed default budget snapshot reused by host-side result rendering and Lua invocation context injection.
/// 构建一份强类型默认预算快照，供宿主侧结果渲染与 Lua 调用上下文注入复用。
fn build_default_luaskills_budget_snapshot(_tool_name: &str) -> ToolResultBudgetSnapshot {
    ToolResultBudgetSnapshot {
        tool_result: EffectiveBudgetScope {
            bytes: 200_000,
            lines: -1,
        },
        file_read: EffectiveBudgetScope {
            bytes: 200_000,
            lines: -1,
        },
    }
}

/// Ensures the minimum runtime directory layout exists and returns the mounted skill roots.
/// 确保最小运行时目录布局存在，并返回需要挂载的技能根集合。
fn ensure_runtime_root_layout(runtime_root: &Path) -> Result<Vec<RuntimeSkillRoot>, String> {
    let temp_dir = runtime_root.join("temp");
    let resources_dir = runtime_root.join("resources");
    let lua_packages_dir = runtime_root.join("lua_packages");
    let libs_root = runtime_root.join("libs");
    // BinRoot follows LuaSkills 0.4.6 fixed host-tool layout directly under runtime_root/bin.
    // BinRoot 遵循 LuaSkills 0.4.6 固定宿主工具布局，直接位于 runtime_root/bin。
    let bin_root = runtime_root.join("bin");
    // DependenciesDir stores runtime-managed dependency payloads beside the fixed skills root.
    // DependenciesDir 用于在固定 skills 根旁保存运行时托管的依赖载荷。
    let dependencies_dir = runtime_root.join("dependencies");
    // DatabasesDir stores file-backed databases when a runtime path backend is selected.
    // DatabasesDir 用于在选择路径型后端时保存文件型数据库。
    let databases_dir = runtime_root.join("databases");
    // ConfigDir owns the default unified skill_config.json resolved from runtime_root.
    // ConfigDir 拥有从 runtime_root 推导出的默认统一 skill_config.json。
    let config_dir = runtime_root.join("config");
    let state_dir = runtime_root.join("state");
    let download_root = temp_dir.join("downloads");
    let root_skills_dir = runtime_root.join("skills");
    // user_skills_dir is normalized before it becomes a LuaSkills RuntimeSkillRoot.
    // user_skills_dir 在成为 LuaSkills RuntimeSkillRoot 前完成原生路径规范化。
    let user_skills_dir = runtime_assets::native_host_path(&default_user_skill_root());
    let system_lua_lib_dir = runtime_root.join("system_lua_lib");

    for directory in [
        runtime_root,
        temp_dir.as_path(),
        resources_dir.as_path(),
        lua_packages_dir.as_path(),
        libs_root.as_path(),
        bin_root.as_path(),
        dependencies_dir.as_path(),
        databases_dir.as_path(),
        config_dir.as_path(),
        state_dir.as_path(),
        download_root.as_path(),
        root_skills_dir.as_path(),
        user_skills_dir.as_path(),
        system_lua_lib_dir.as_path(),
    ] {
        fs::create_dir_all(directory).map_err(|error| {
            format!(
                "创建 LuaSkills 运行时目录失败（{}）：{error}",
                directory.display()
            )
        })?;
    }

    Ok(vec![
        RuntimeSkillRoot {
            name: String::from("ROOT"),
            skills_dir: root_skills_dir,
        },
        RuntimeSkillRoot {
            name: String::from("USER"),
            skills_dir: user_skills_dir,
        },
    ])
}

/// Builds one host option object that routes both LuaSkills databases through this process's own engine.
/// 构建一份宿主选项对象，使 LuaSkills 的两个数据库都经由本进程自身的引擎。
fn build_host_options(
    runtime_root: &Path,
    managed_runtime_distribution_root: &Path,
    skill_config_root: &Path,
    runtime_settings: &LuaSkillsRuntimeSettings,
) -> LuaRuntimeHostOptions {
    // lua_runtime_root enforces the native-path invariant at the final LuaSkills HostOptions boundary.
    // lua_runtime_root 在最终 LuaSkills HostOptions 边界强制执行原生路径不变量。
    let lua_runtime_root = runtime_assets::native_host_path(runtime_root);
    // lua_managed_runtime_distribution_root applies the same invariant to managed runtime discovery.
    // lua_managed_runtime_distribution_root 对受管运行时发现路径应用相同不变量。
    let lua_managed_runtime_distribution_root =
        runtime_assets::native_host_path(managed_runtime_distribution_root);
    let mut host_options = LuaRuntimeHostOptions::with_runtime_root(lua_runtime_root.clone());
    host_options.skill_config_root = Some(runtime_assets::native_host_path(skill_config_root));
    host_options.managed_runtime_distribution_root = Some(lua_managed_runtime_distribution_root);
    host_options.managed_runtime_environment_root =
        Some(lua_runtime_root.join("dependencies").join("envs"));
    host_options.managed_runtime_config = LuaRuntimeManagedRuntimeConfig {
        worker_pool_max_size_per_environment: runtime_settings
            .managed_runtime
            .worker_pool_max_size_per_environment,
        worker_idle_ttl_secs: runtime_settings.managed_runtime.worker_idle_ttl_secs,
        persistent_session_limit_per_engine: runtime_settings
            .managed_runtime
            .persistent_session_limit_per_engine,
        persistent_session_default_buffer_limit_bytes_per_stream: runtime_settings
            .managed_runtime
            .persistent_session_default_buffer_limit_bytes_per_stream,
        invoke_default_timeout_ms: runtime_settings.managed_runtime.invoke_default_timeout_ms,
    };
    host_options.allow_network_download = runtime_settings.network.allow_downloads;
    host_options.github_base_url = runtime_settings.network.github_base_url.clone();
    host_options.github_api_base_url = runtime_settings.network.github_api_base_url.clone();
    host_options.official_skill_hub_base_url =
        runtime_settings.network.official_skill_hub_base_url.clone();
    host_options.enable_private_url_skill_install =
        runtime_settings.network.enable_private_url_skill_install;
    host_options.private_skill_source_allowlist = runtime_settings
        .network
        .private_skill_source_allowlist
        .clone();
    host_options.default_text_encoding =
        runtime_settings.capabilities.default_text_encoding.clone();
    host_options.cache_config = Some(ToolCacheConfig {
        max_entries: runtime_settings.tool_cache.max_entries,
        default_ttl_secs: runtime_settings.tool_cache.default_ttl_secs,
        max_ttl_secs: runtime_settings.tool_cache.max_ttl_secs,
    });
    host_options.runlua_pool_config = Some(LuaRuntimeRunLuaPoolConfig {
        min_size: runtime_settings.run_lua_pool.min_size,
        max_size: runtime_settings.run_lua_pool.max_size,
        idle_ttl_secs: runtime_settings.run_lua_pool.idle_ttl_secs,
    });
    host_options.capabilities = LuaRuntimeCapabilityOptions {
        enable_skill_management_bridge: runtime_settings
            .capabilities
            .enable_skill_management_bridge,
        enable_managed_io_compat: runtime_settings.capabilities.enable_managed_io_compat,
    };
    // The mode comes from the host-provider policy rather than a local literal. It deliberately remains
    // `HostCallback` even before installation, so a broken startup order fails loudly instead of selecting the
    // retired external-controller path.
    // 模式取自宿主 provider 策略而非本地字面量。它在安装前也刻意保持 `HostCallback`，使错误的启动顺序响亮
    // 失败，而不是选择已退役的外部控制器路径。
    //
    // The LuaSkills `space_controller` options stay at their crate defaults. That mode talks to an external
    // controller process, which Vulcan Code neither ships nor supports any more, so nothing fills them in.
    // LuaSkills 的 `space_controller` 选项保持该 crate 的默认值。该模式与外部控制器进程通信，而 Vulcan Code
    // 已不再随附也不再支持它，因此这里不填充它们。
    host_options.sqlite_provider_mode = host_database_providers::provider_mode();
    host_options.lancedb_provider_mode = host_database_providers::provider_mode();
    host_options
}

/// Returns the mutable USER skill root stored under the current user's VulcanCode data directory.
/// 返回存放在当前用户 VulcanCode 数据目录下的可变 USER 技能根目录。
fn default_user_skill_root() -> PathBuf {
    crate::data_plane::application_data_root()
        .join("luaskills-user")
        .join("skills")
}

/// Acquire one immutable runtime-engine guard from the shared host handle.
/// 从共享宿主句柄中获取一份只读运行时引擎锁守卫。
///
/// Multiple read guards may coexist, so independent skill, runLua and plugin calls overlap instead
/// of being serialized by the host. The guard must not be held across `.await`.
/// 多份读守卫可以并存，因此相互独立的技能、runLua 与插件调用可以重叠，而不再被宿主串行化。
/// 该守卫不得跨越 `.await` 持有。
fn lock_runtime_engine(
    handle: &LuaSkillsRuntimeHandle,
) -> Result<std::sync::RwLockReadGuard<'_, LuaEngine>, String> {
    handle
        .engine
        .read()
        .map_err(|_| String::from("LuaSkills 运行时读锁已损坏，无法读取技能目录。"))
}

/// Acquire one mutable runtime-engine guard from the shared host handle.
/// 从共享宿主句柄中获取一份可变运行时引擎锁守卫。
///
/// The write guard waits for every in-flight read guard to drain, which is exactly the engine
/// change barrier required by upstream methods that take `&mut LuaEngine`.
/// 写守卫会等待所有在途读守卫排空，这正是取 `&mut LuaEngine` 的上游方法所需的引擎变更屏障。
fn lock_runtime_engine_mut(
    handle: &LuaSkillsRuntimeHandle,
) -> Result<std::sync::RwLockWriteGuard<'_, LuaEngine>, String> {
    handle
        .engine
        .write()
        .map_err(|_| String::from("LuaSkills 运行时写锁已损坏，无法执行技能管理操作。"))
}

/// Build one normalized skill catalog from the active engine and the current runtime root layout.
/// 根据活动引擎与当前运行时根布局构建一份规范化技能清单。
fn build_luaskills_catalog(
    handle: &LuaSkillsRuntimeHandle,
    engine: &LuaEngine,
) -> Result<LuaSkillsCatalog, String> {
    let visible_entries = list_host_visible_luaskills_entries(engine)?;
    refresh_luaskills_tool_entry_cache(handle, &visible_entries)?;
    let mut tools_by_skill = visible_entries.into_iter().fold(
        BTreeMap::<String, Vec<LuaSkillsCatalogTool>>::new(),
        |mut grouped_tools, entry| {
            grouped_tools
                .entry(entry.skill_id.clone())
                .or_default()
                .push(LuaSkillsCatalogTool {
                    canonical_name: entry.canonical_name,
                    local_name: entry.local_name,
                    description: entry.description,
                });
            grouped_tools
        },
    );
    let resolved_skills = collect_effective_skill_instances_from_roots(&handle.skill_roots)
        .map_err(|error| format!("解析 LuaSkills 生效技能目录失败：{error}"))?;

    let mut skills = Vec::with_capacity(resolved_skills.len());
    for resolved in resolved_skills {
        let meta = read_skill_meta_from_directory(&resolved.actual_dir, &resolved.skill_id)?;
        let manager = build_skill_manager_for_root(handle, &resolved.root_name)?;
        let enabled = manager
            .is_skill_enabled(&resolved.skill_id)
            .map_err(|error| format!("读取技能启用状态失败（{}）：{error}", resolved.skill_id))?;
        let disabled_reason = manager
            .disabled_record(&resolved.skill_id)
            .map_err(|error| format!("读取技能停用记录失败（{}）：{error}", resolved.skill_id))?
            .and_then(|record| record.reason);
        let install_record = manager
            .install_record(&resolved.skill_id)
            .map_err(|error| format!("读取技能安装记录失败（{}）：{error}", resolved.skill_id))?;

        let (managed, source_type, source_locator) = match install_record {
            Some(record) => (
                record.managed,
                Some(record.source.source_type),
                Some(record.source.locator),
            ),
            None => (false, None, None),
        };
        let mut tools = tools_by_skill
            .remove(&resolved.skill_id)
            .unwrap_or_default();
        tools.sort_by(|left, right| {
            left.local_name
                .cmp(&right.local_name)
                .then_with(|| left.canonical_name.cmp(&right.canonical_name))
        });

        skills.push(LuaSkillsCatalogSkill {
            skill_id: resolved.skill_id.clone(),
            display_name: meta.name.clone(),
            version: meta.version().to_string(),
            root_name: resolved.root_name.clone(),
            skill_dir: runtime_assets::native_host_path(&resolved.actual_dir),
            enabled,
            disabled_reason,
            managed,
            source_type,
            source_locator,
            entry_count: tools.len(),
            tools,
            system_owned: resolved.root_name.eq_ignore_ascii_case("ROOT"),
        });
    }

    skills.sort_by(|left, right| {
        left.root_name
            .cmp(&right.root_name)
            .then_with(|| left.skill_id.cmp(&right.skill_id))
    });

    Ok(LuaSkillsCatalog {
        runtime_root: runtime_assets::native_host_path(&handle.runtime_root),
        system_lua_lib_dir: runtime_assets::native_host_path(&handle.system_lua_lib_dir),
        entry_count: skills.iter().map(|skill| skill.entry_count).sum(),
        skills,
    })
}

/// Refreshes the provider-facing LuaSkills entry cache from one freshly enumerated entry list.
/// 根据刚枚举出的入口列表刷新面向 Provider 的 LuaSkills 入口缓存。
fn refresh_luaskills_tool_entry_cache(
    handle: &LuaSkillsRuntimeHandle,
    entries: &[RuntimeEntryDescriptor],
) -> Result<(), String> {
    let mut cached_entries = handle
        .tool_entries
        .lock()
        .map_err(|_| String::from("LuaSkills 工具入口缓存锁已损坏，无法刷新工具表。"))?;
    *cached_entries = entries.to_vec();
    Ok(())
}

/// Lists the LuaSkills runtime entries currently visible to the host-controlled System authority.
/// 列出当前对宿主控制 System authority 可见的 LuaSkills 运行时入口。
/// List entries visible to the trusted host authority.
/// 列出可信宿主权限可见的入口。
fn list_host_visible_luaskills_entries(
    engine: &LuaEngine,
) -> Result<Vec<RuntimeEntryDescriptor>, String> {
    engine
        .list_entries_for_authority(SkillManagementAuthority::System)
        .map_err(|error| format!("读取 LuaSkills 宿主可见工具失败：{error}"))
}

/// Resolves the ordinary USER root used by the desktop panel for managed install actions.
/// 解析桌面面板用于受管安装动作的普通 USER 根。
fn ordinary_user_root(handle: &LuaSkillsRuntimeHandle) -> Result<RuntimeSkillRoot, String> {
    resolve_named_skill_root(&handle.skill_roots, "USER")
}

/// Resolves the effective target root that currently owns one visible skill identifier.
/// 解析当前拥有单个可见技能标识符的生效目标根目录。
fn target_root_for_skill(
    handle: &LuaSkillsRuntimeHandle,
    skill_id: &str,
) -> Result<RuntimeSkillRoot, String> {
    let resolved = collect_effective_skill_instances_from_roots(&handle.skill_roots)
        .map_err(|error| format!("解析技能目录失败：{error}"))?
        .into_iter()
        .find(|instance| instance.skill_id == skill_id)
        .ok_or_else(|| format!("未找到技能 `{skill_id}` 对应的生效目录。"))?;
    resolve_named_skill_root(&handle.skill_roots, &resolved.root_name)
}

/// Resolves the uninstall target root and blocks attempts to remove host-owned ROOT skills.
/// 解析卸载目标根目录，并拦截对宿主自有 ROOT 技能的移除操作。
fn ordinary_uninstall_target_root_for_skill(
    handle: &LuaSkillsRuntimeHandle,
    skill_id: &str,
) -> Result<RuntimeSkillRoot, String> {
    let resolved = target_root_for_skill(handle, skill_id)?;
    if resolved.name.eq_ignore_ascii_case("ROOT") {
        return Err(format!(
            "技能 `{skill_id}` 当前位于 ROOT 层，由宿主内建控制，不允许卸载。"
        ));
    }
    Ok(resolved)
}

/// Build one skill-manager instance that mirrors the runtime root layout used by the embedded host.
/// 构建一个与嵌入式宿主运行时根布局保持一致的技能管理器实例。
///
/// This manager serves read-only catalog metadata only: every caller uses it for
/// `is_skill_enabled`, `disabled_record`, or `install_record`, and never for an
/// install, update, or download. Network download is therefore disabled here rather
/// than left at a permissive default, so the read-only property is enforced by
/// construction instead of relying on no future caller adding a download.
/// 本管理器只服务只读目录元数据：所有调用方都只用它做 `is_skill_enabled`、
/// `disabled_record` 或 `install_record`，绝不用于安装、更新或下载。因此这里禁用网络下载，
/// 而不是留下一处宽松默认值，使只读属性由构造强制，而不是依赖未来没有调用方新增下载。
fn build_skill_manager_for_root(
    handle: &LuaSkillsRuntimeHandle,
    root_name: &str,
) -> Result<SkillManager, String> {
    let skill_root = handle
        .skill_roots
        .iter()
        .find(|root| root.name == root_name)
        .cloned()
        .ok_or_else(|| format!("未找到技能根 `{root_name}`。"))?;
    let lifecycle_root = skill_root
        .skills_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| skill_root.skills_dir.clone())
        .join("state");
    Ok(SkillManager::new(SkillManagerConfig {
        skill_root,
        lifecycle_root,
        download_cache_root: handle.runtime_root.join("temp").join("downloads"),
        // Catalog metadata never downloads, so this manager is built without that
        // capability instead of inheriting a permissive default.
        // 目录元数据从不下载，因此本管理器在构建时就不具备该能力，而不是继承宽松默认值。
        allow_network_download: false,
        github_base_url: None,
        github_api_base_url: None,
        official_skill_hub_base_url: None,
        enable_private_url_skill_install: false,
        private_skill_source_allowlist: Vec::new(),
    }))
}

/// Read and normalize one public skill manifest from the specified physical skill directory.
/// 从指定物理技能目录读取并规范化一份公开技能清单。
fn read_skill_meta_from_directory(skill_dir: &Path, skill_id: &str) -> Result<SkillMeta, String> {
    let manifest_path = skill_dir.join("skill.yaml");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("读取技能清单失败（{}）：{error}", manifest_path.display()))?;
    let mut meta: SkillMeta = serde_yaml::from_str(&manifest_text)
        .map_err(|error| format!("解析技能清单失败（{}）：{error}", manifest_path.display()))?;
    meta.bind_directory_skill_id(skill_id.to_string());
    Ok(meta)
}

/// Trim one optional free-form input field and drop it when the result is empty.
/// 规范化单个可选自由输入字段，并在结果为空时将其丢弃。
fn normalize_optional_field(value: Option<String>) -> Option<String> {
    value
        .map(|field| field.trim().to_string())
        .filter(|field| !field.is_empty())
}

/// Resolves one named runtime skill root from the current root chain.
/// 从当前技能根链中解析指定名称的单个运行时技能根。
fn resolve_named_skill_root(
    skill_roots: &[RuntimeSkillRoot],
    root_name: &str,
) -> Result<RuntimeSkillRoot, String> {
    skill_roots
        .iter()
        .find(|root| root.name.eq_ignore_ascii_case(root_name))
        .cloned()
        .ok_or_else(|| format!("当前运行时缺少 {root_name} 技能根。"))
}
