//! Service runtime ownership: lock, data root, event bus, and shutdown coordination.
//! 服务运行时归属：锁、数据根、事件总线与关闭协调。

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::broadcast;
use vulcan_service_api::ServiceEventEmitter;
use vulcan_service_host::webui_service_event_bus_sink;

use vulcan_service_kernel::{
    ServiceShellHost, SharedServiceBridgeState, create_service_bridge_state,
    emit_initial_service_events, service_install_conversation_event_v1_emitter,
    service_prepare_for_app_shutdown, service_resume_after_failed_app_shutdown,
};

use crate::launch::{SERVICE_NAME, service_launch_description};

/// Prints the build description without creating locks, databases, or services.
/// 输出构建描述，不创建锁、数据库或服务。
pub fn print_service_description() -> Result<(), String> {
    let executable_path = std::env::current_exe()
        .map_err(|error| format!("failed to read service executable path: {error}"))?;
    let data_root = authoritative_data_root()?;
    let description = service_launch_description(executable_path, data_root);
    serde_json::to_writer_pretty(std::io::stdout(), &description)
        .map_err(|error| format!("failed to write service description: {error}"))?;
    println!();
    Ok(())
}

/// Runs the headless service without tray or WebView until a termination signal arrives.
/// 运行无头服务，不初始化托盘或 WebView，直至收到终止信号。
pub fn run_headless() -> ! {
    let runtime = match HeadlessService::start() {
        Ok(Some(runtime)) => runtime,
        Ok(None) => {
            // Losing the ownership race is a successful no-op for a concurrent launcher.
            // 对并发启动器而言，输掉所有权竞争是一次成功的无操作。
            eprintln!("[{SERVICE_NAME}] another service already owns this data directory");
            std::process::exit(0);
        }
        Err(error) => {
            crate::logging::startup_error(&format!(
                "[{SERVICE_NAME}] headless startup failed: {error}"
            ));
            std::process::exit(1);
        }
    };
    let shutdown = runtime.shutdown_signal();
    // Retain the Windows/Unix signal driver throughout every durable drain attempt. A second
    // signal during a failed drain must not bypass the coordinator and discard dirty state.
    // 在每次可靠排空期间保留 Windows/Unix 信号驱动；失败排空期间的第二个信号不得绕过协调器丢失脏状态。
    let signal_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("headless signal runtime must build");
    let mut monitor_bridge = true;
    loop {
        // exit_reason preserves whether the owner stopped intentionally or lost its HTTP bridge.
        // exit_reason 保留所有者是主动停止还是 HTTP 桥故障的区别。
        let exit_reason = wait_for_headless_exit(&runtime, &signal_runtime, monitor_bridge);
        shutdown.store(true, Ordering::Release);
        match runtime.shutdown() {
            Ok(_) => {
                // Preserve a terminal failed discovery record for bridge-loss diagnostics;
                // an intentional stop revokes the owned ready generation before exit.
                // 保留桥故障的终态发现记录以便诊断；主动停止则在退出前撤销本实例的 ready 代次。
                if !matches!(exit_reason, HeadlessExitReason::HttpBridgeFailure)
                    && let Err(error) =
                        vulcan_service_kernel::remove_owned_webui_service_discovery(&runtime.state)
                {
                    tracing::error!("[{SERVICE_NAME}] discovery cleanup failed: {error}");
                }
                std::process::exit(match exit_reason {
                    HeadlessExitReason::TerminationSignal
                    | HeadlessExitReason::ShutdownRequested => 0,
                    HeadlessExitReason::HttpBridgeFailure => 1,
                })
            }
            Err(error) => {
                // The in-memory session may still contain an unflushed candidate. Keep its owner
                // alive so an operator can repair storage and signal again, even after HTTP loss.
                // 内存会话可能仍有未刷盘候选；即使 HTTP 已断，也要保留所有者以便修复存储后再次发信号。
                tracing::error!(
                    "[{SERVICE_NAME}] headless shutdown failed; service remains alive: {error}"
                );
                shutdown.store(false, Ordering::Release);
                monitor_bridge = !matches!(exit_reason, HeadlessExitReason::HttpBridgeFailure);
            }
        }
    }
}

/// Distinguishes an intentional headless stop from a failed HTTP bridge.
/// 区分无桌面服务的主动停止与 HTTP 桥故障。
enum HeadlessExitReason {
    /// The operating system delivered a normal termination signal.
    /// 操作系统投递了正常终止信号。
    TerminationSignal,
    /// The owner explicitly requested shutdown.
    /// 所有者明确请求关闭。
    ShutdownRequested,
    /// The supervised HTTP bridge stopped unexpectedly.
    /// 受监督的 HTTP 桥意外停止。
    HttpBridgeFailure,
}

/// Blocks until a termination signal, owner shutdown, or HTTP bridge failure.
/// 阻塞直至收到终止信号、所有者关闭请求或 HTTP 桥故障。
///
/// # Parameters
/// 参数
/// `runtime` owns the live bridge and shutdown flag.
/// `runtime` 持有活动桥及关闭标志。
/// `signal_runtime` keeps the OS signal driver installed across failed drain attempts.
/// `signal_runtime` 在失败排空的重试间保持系统信号驱动已安装。
/// `monitor_bridge` suppresses repeat notifications after that bridge already failed.
/// `monitor_bridge` 在桥已故障后抑制重复故障通知。
///
/// # Returns
/// 返回值
/// Returns the exact cause so failure cannot be reported as a successful service stop.
/// 返回精确原因，防止将故障误报为服务正常停止。
fn wait_for_headless_exit(
    runtime: &HeadlessService,
    signal_runtime: &tokio::runtime::Runtime,
    monitor_bridge: bool,
) -> HeadlessExitReason {
    // signal_runtime polls the OS signal and bridge health together. A blocking health loop
    // would never register the signal handler while the HTTP bridge remains healthy.
    // signal_runtime 同时轮询系统信号与桥健康度。阻塞式健康循环会使健康服务永远无法注册信号处理器。
    signal_runtime.block_on(async {
        tokio::select! {
            _ = wait_for_termination_signal() => HeadlessExitReason::TerminationSignal,
            reason = async {
                if !monitor_bridge {
                    std::future::pending::<()>().await;
                }
                loop {
                    if runtime.shutdown_signal().load(Ordering::Acquire) {
                        break HeadlessExitReason::ShutdownRequested;
                    }
                    if let Some(failure) = runtime.observed_http_bridge_failure() {
                        tracing::error!("[{SERVICE_NAME}] HTTP bridge failed after acknowledgement: {failure}");
                        break HeadlessExitReason::HttpBridgeFailure;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
            } => reason,
        }
    })
}

/// Returns the authoritative user data root for the current build profile.
/// 返回当前构建配置的权威用户数据根。
pub fn authoritative_data_root() -> Result<std::path::PathBuf, String> {
    let home = vulcan_runtime_assets::user_home_dir()
        .ok_or_else(|| String::from("failed to resolve user home directory"))?;
    Ok(vulcan_runtime_assets::user_vulcan_code_root_for_home(&home))
}

/// Installs the isolated integration-test root before any user path resolution.
/// 在任何用户路径解析之前安装隔离集成测试根。
///
/// # Parameters
/// 参数
/// `args` are the full process arguments including the executable name.
/// `args` 是包含可执行文件名的完整进程参数。
///
/// # Returns
/// 返回值
/// `Ok(true)` when a test root was installed, `Ok(false)` without one, or a diagnostic.
/// 安装测试根时返回 `Ok(true)`，无测试根时返回 `Ok(false)`，否则返回诊断。
#[cfg(feature = "test-support")]
pub fn install_isolated_test_root_from_args(args: &[String]) -> Result<bool, String> {
    use crate::launch::{ARG_TEST_ROOT, test_root_argument_value};

    let mut values = Vec::new();
    let mut pending_value = false;
    for arg in args.iter().skip(1) {
        if pending_value {
            values.push(arg.clone());
            pending_value = false;
            continue;
        }
        if arg == ARG_TEST_ROOT {
            pending_value = true;
            continue;
        }
        if let Some(value) = test_root_argument_value(arg) {
            values.push(value.to_string());
        }
    }
    if pending_value {
        return Err(String::from(
            "--test-root requires an existing absolute directory value",
        ));
    }
    if values.len() > 1 {
        return Err(String::from("--test-root may be supplied only once"));
    }
    let Some(value) = values.into_iter().next() else {
        return Ok(false);
    };
    if value.trim().is_empty() {
        return Err(String::from(
            "--test-root requires an existing absolute directory value",
        ));
    }
    let root = std::path::PathBuf::from(value);
    vulcan_runtime_assets::test_support::install_isolated_process_root(&root)?;
    Ok(true)
}

/// Returns the WebUI static root declared by the validated runtime layout manifest.
/// 返回已校验运行时布局清单声明的 WebUI 静态根。
///
/// # Returns
/// 返回值
/// `Ok(Some(root))` when a validated manifest declares an existing WebUI root, `Ok(None)` in
/// debug builds without a manifest, or a diagnostic when a required release asset is missing.
/// 清单声明了存在的 WebUI 根时返回 `Ok(Some(root))`；调试构建无清单时返回 `Ok(None)`；
/// 必需发布资源缺失时返回诊断。
fn service_webui_root() -> Result<Option<std::path::PathBuf>, String> {
    use crate::runtime_layout::{
        read_runtime_layout_manifest_from, read_runtime_layout_manifest_with_path,
        resolve_layout_entry, runtime_layout_root,
    };

    let manifest_and_path = match crate::runtime_layout::explicit_runtime_layout_path()? {
        Some(explicit) => {
            let manifest = read_runtime_layout_manifest_from(&explicit)?;
            Some((manifest, explicit))
        }
        None => match read_runtime_layout_manifest_with_path() {
            Ok(found) => Some(found),
            Err(error) => {
                // Debug builds may legitimately run straight from source without a manifest.
                // 调试构建可以合法地直接从源码运行而无需清单。
                if cfg!(debug_assertions) {
                    return Ok(None);
                }
                return Err(error);
            }
        },
    };
    let Some((manifest, manifest_path)) = manifest_and_path else {
        return Ok(None);
    };
    // A release argument must never silently run an artifact built by the other profile.
    // 发布参数绝不允许静默运行另一个配置构建的产物。
    let expected_profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    if manifest.profile != expected_profile {
        return Err(format!(
            "runtime layout {} declares profile `{}` but this build is `{expected_profile}`",
            manifest_path.display(),
            manifest.profile
        ));
    }
    let root = runtime_layout_root(&manifest_path)?;
    let webui_root = resolve_layout_entry(&root, &manifest.webui_root)?;
    if !webui_root.is_dir() {
        return Err(format!(
            "WebUI root {} declared by {} does not exist",
            webui_root.display(),
            manifest_path.display()
        ));
    }
    Ok(Some(webui_root))
}

/// Resolves the browser host runtime used to start one host process per Core browser session.
/// 解析用于为每个 Core 浏览器会话启动一个宿主进程的浏览器宿主运行时。
///
/// # Returns
/// 返回值
/// The runtime configuration, or a diagnostic when the build cannot locate a browser host.
/// 返回运行时配置；构建无法定位浏览器宿主时返回诊断。
pub(crate) fn service_browser_host_runtime(
    data_root: &std::path::Path,
) -> Result<vulcan_service_kernel::browser::BrowserHostRuntimeConfig, String> {
    use crate::runtime_layout::{
        read_runtime_layout_manifest_from, read_runtime_layout_manifest_with_path,
        resolve_layout_entry, runtime_layout_root,
    };
    // Explicit distributions are authoritative; source debug launches use the workspace package.
    // 显式发行物具有权威性；源码调试启动使用工作区发行包。
    let (manifest, manifest_path) = match crate::runtime_layout::explicit_runtime_layout_path()? {
        Some(explicit) => (read_runtime_layout_manifest_from(&explicit)?, explicit),
        None => read_runtime_layout_manifest_with_path()?,
    };
    // Only the shipped manifest selects Node and Chromium; legacy user downloads cannot shadow it.
    // 仅由随包清单选择 Node 与 Chromium；旧用户下载不能覆盖它。
    let root = runtime_layout_root(&manifest_path)?;
    let executable = resolve_layout_entry(&root, &manifest.browser_host_executable)?;
    let entry = resolve_layout_entry(&root, &manifest.browser_host_entry)?;
    let chromium_entry = manifest.chromium_executable.as_deref()
        .ok_or("Built-in Chromium is missing. Repair the application installation (developers: run make sync and rebuild).")?;
    let chromium_executable = resolve_layout_entry(&root, chromium_entry)?;
    for file in [&executable, &entry, &chromium_executable] {
        if !file.is_file() {
            return Err(format!(
                "Built-in browser component is missing: {}. Repair the application installation (developers: run make sync and rebuild).",
                file.display()
            ));
        }
    }
    Ok(vulcan_service_kernel::browser::BrowserHostRuntimeConfig {
        executable,
        entry,
        chromium_executable,
        profiles_root: data_root.join("browser-profiles"),
        staging_root: data_root.join("browser-staging"),
        discovery_timeout: std::time::Duration::from_secs(30),
        request_timeout: std::time::Duration::from_secs(20),
    })
}

/// Clears browser data that cannot be reattached after a Core restart.
/// 清理 Core 重启后无法重新挂接的浏览器数据。
///
/// `data_root` is held by the exclusive service lock and `config` supplies the three temporary
/// roots. Returns an error if any existing root cannot be proven inside the owned data root or
/// cannot be removed; the caller logs it while allowing the service to start.
/// `data_root` 已由服务排他锁持有，`config` 提供三个临时根目录。若现有目录无法证明位于该数据根内，
/// 或无法移除，则返回错误；调用方记录错误并继续启动服务。
fn clear_orphaned_browser_data(
    data_root: &std::path::Path,
    config: &vulcan_service_kernel::browser::BrowserHostRuntimeConfig,
) -> Result<(), String> {
    // The canonical locked root bounds every startup deletion.
    // 已规范化的排他根目录界定每次启动清理的删除范围。
    let owned_root = data_root
        .canonicalize()
        .map_err(|error| format!("failed to resolve service data root: {error}"))?;
    // Persistent profiles survive Core restarts. Staging, ephemeral profiles, and control files
    // belong to in-memory sessions, so no new Core process can validly reattach them.
    // 持久画像跨 Core 重启保留；暂存、临时画像和控制文件属于内存会话，新 Core 无法有效重新挂接。
    let temporary_roots = [
        &config.staging_root,
        &config.profiles_root.join("ephemeral"),
        &config.profiles_root.join("control"),
    ];
    // Each candidate is checked independently against the locked service data root.
    // 每个候选目录均独立与已锁定的服务数据根进行核对。
    for directory in temporary_roots {
        // Canonicalization exposes a path redirected by a link before recursive removal.
        // 规范化在递归删除前揭示被目录链接重定向的路径。
        let resolved = match directory.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "failed to resolve browser temporary root {}: {error}",
                    directory.display()
                ));
            }
        };
        if resolved == owned_root || !resolved.starts_with(&owned_root) {
            return Err(format!(
                "browser temporary root is outside the owned data directory: {}",
                directory.display()
            ));
        }
        std::fs::remove_dir_all(directory).map_err(|error| {
            format!(
                "failed to clear browser temporary root {}: {error}",
                directory.display()
            )
        })?;
    }
    Ok(())
}

/// Starts the periodic Core browser host reaper.
/// 启动周期性 Core 浏览器宿主回收器。
///
/// # Parameters
/// 参数
/// `state` is the shared service state owning the browser manager.
/// `state` 是拥有浏览器管理器的共享服务状态。
///
/// # Design note / 设计说明
/// Host liveness is a process fact, so it is sampled on a dedicated thread rather than inside the
/// HTTP dispatch path. Sampling never holds the manager state lock while a process is inspected.
/// 宿主存活是进程事实，因此在专用线程上采样，而不是放在 HTTP 派发路径内。采样在检查进程期间绝不
/// 持有管理器状态锁。
fn spawn_browser_host_reaper(state: SharedServiceBridgeState) {
    let _ = std::thread::Builder::new()
        .name(String::from("vulcan-browser-host-reaper"))
        .spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let manager = match vulcan_service_kernel::browser::service_browser_manager(&state) {
                    Ok(manager) => manager,
                    Err(_) => return,
                };
                if let Err(error) = manager.expire_control_leases() {
                    tracing::error!("[vulcan-code-service] browser lease cleanup failed: {error}");
                }
                match manager.reap_hosts() {
                    Ok(retired) => {
                        for projection in retired {
                            // A recovered expected close also appears in the reap batch; only an
                            // unexpected exit should be reported as a crash.
                            // 恢复中的预期关闭也会进入回收批次；只有意外退出才报告为崩溃。
                            if projection.status
                                == vulcan_service_kernel::browser::BrowserSessionStatus::Crashed
                            {
                                tracing::error!(
                                    "[vulcan-code-service] browser host for session {} crashed; a new generation requires an explicit reopen",
                                    projection.session_id
                                );
                            }
                        }
                    }
                    Err(error) => {
                        tracing::error!("[vulcan-code-service] browser host liveness check failed: {error}");
                    }
                }
            }
        });
}

/// Desktop implementation of host shell capabilities shared by tray and headless modes.
/// 托盘与无头模式共享的宿主 shell 桌面实现。
struct ServiceShellHostImpl {
    /// Shutdown request flag of the owning service, watched by the headless loop and the tray.
    /// 所属服务的关闭请求标志，由无头循环与托盘监视。
    shutdown_requested: Arc<AtomicBool>,
}

impl ServiceShellHost for ServiceShellHostImpl {
    /// Opens one local path with the system file manager.
    /// 使用系统文件管理器打开单个本地路径。
    fn open_path(&self, path: &str) -> Result<(), String> {
        crate::desktop_integration::open_path_in_system_file_manager(path)
    }

    /// Opens one URL with the system default browser.
    /// 使用系统默认浏览器打开单个 URL。
    fn open_url(&self, url: &str) -> Result<(), String> {
        crate::desktop_integration::open_url_in_system_browser(url)
    }

    /// Records what starts after the drain and asks the owner loop to drain.
    /// 记录排空后启动的内容，并请求所有者循环排空。
    fn restart_service(&self, plan: vulcan_app_update::RestartPlan) -> Result<(), String> {
        crate::restart::request(plan, &self.shutdown_requested)
    }
}

/// Owned headless service lifetime: lock guard, state, emitter, and shutdown flags.
/// 拥有的无头服务生命周期：锁守卫、状态、发送器与关闭标志。
pub struct HeadlessService {
    /// Exclusive data-root ownership retained until process exit.
    /// 直至进程退出保留的排他数据根所有权。
    _guard: vulcan_service_host::instance::ServiceInstanceGuard,
    /// Shared service state owned by this process.
    /// 本进程拥有的共享服务状态。
    state: SharedServiceBridgeState,
    /// Owned HTTP bridge thread retained until the service shuts down.
    /// 服务关闭前一直保留的自有 HTTP 桥线程。
    _http_bridge: vulcan_service_kernel::WebuiServiceHttpBridgeHandle,
    /// Shutdown request flag shared with the signal waiter.
    /// 与信号等待器共享的关闭请求标志。
    shutdown_requested: Arc<AtomicBool>,
    /// One immutable native diagnostic subscription survives every plugin and parent generation.
    /// 唯一不可变原生诊断订阅跨越全部插件和父级代次。
    native_diagnostics: crate::native_diagnostics::NativeDiagnosticCapture,
    /// Test-only one-shot failure before the durable coordinator starts.
    /// 仅测试使用、在可靠协调器启动前触发的一次性故障。
    #[cfg(feature = "test-support")]
    test_fail_shutdown_once: AtomicBool,
}

impl HeadlessService {
    /// Returns the shared service state owned by the tray host.
    /// 返回托盘宿主拥有的共享服务状态。
    #[cfg(feature = "desktop-tray")]
    pub fn headless_state(&self) -> SharedServiceBridgeState {
        Arc::clone(&self.state)
    }

    /// Returns the post-acknowledgement HTTP bridge failure observed by the supervisor.
    /// 返回监督器观察到的确认后 HTTP 桥失败。
    pub fn observed_http_bridge_failure(&self) -> Option<String> {
        self._http_bridge.observed_failure()
    }

    /// Borrows the owned HTTP bridge failure slot for an external supervisor.
    /// 为外部监督器借出自有 HTTP 桥失败槽。
    #[cfg(feature = "desktop-tray")]
    pub fn http_bridge_failure_slot(&self) -> std::sync::Arc<std::sync::Mutex<Option<String>>> {
        self._http_bridge.failure_slot()
    }

    /// Starts the headless service after acquiring exclusive data-root ownership.
    /// 获取排他数据根所有权后启动无头服务。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(Some(service))` when this process became the owner, `Ok(None)` when another process
    /// already owns the same data root, or a diagnostic for a non-contention startup failure.
    /// 本进程成为所有者时返回 `Ok(Some(service))`；另一个进程已拥有同一数据根时返回 `Ok(None)`；
    /// 非竞争类启动失败返回诊断。
    pub fn start() -> Result<Option<Self>, String> {
        let data_root = authoritative_data_root()?;
        let args: Vec<String> = std::env::args().collect();
        // A replacement started by an update keeps trying while the old owner drains and exits.
        // 由更新启动的替代进程在旧所有者排空并退出期间持续尝试。
        let deadline = crate::launch::await_owner_exit_requested(&args)
            .then(|| std::time::Instant::now() + crate::launch::OWNER_EXIT_WAIT);
        let guard = loop {
            let guard = vulcan_service_host::instance::ServiceInstanceGuard::acquire(&data_root)
                .map_err(|error| format!("failed to acquire service lock: {error}"))?;
            match deadline {
                Some(deadline) if guard.is_none() && std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                _ => break guard,
            }
        };
        let Some(guard) = guard else {
            // A concurrent launcher lost the race: the launcher treats this as "already running",
            // so this process must exit successfully without touching the live instance.
            // 并发启动器竞争失败：启动器将其视为“已在运行”，因此本进程必须在不触碰活动实例的前提下
            // 以成功状态退出。
            return Ok(None);
        };
        // Only the data-root owner may rotate its logs; install before any service workers start.
        // 仅数据根所有者可以轮转日志，在任何服务线程启动前安装日志接收器。
        crate::logging::initialize(&data_root)?;
        // A login registration follows the version that owns the data root after an update.
        // 更新后，登录注册跟随拥有数据根的版本。
        crate::user_service::refresh_registration();
        // Install once before state construction or any native worker can admit an operation.
        // state 构造或任何原生工作线程准入操作前一次性安装。
        let native_diagnostics = crate::native_diagnostics::NativeDiagnosticCapture::install()?;
        let (service_event_bus, _) =
            broadcast::channel(vulcan_service_host::WEBUI_SERVICE_EVENT_BUS_CAPACITY);
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let state = create_service_bridge_state(Arc::new(ServiceShellHostImpl {
            shutdown_requested: Arc::clone(&shutdown_requested),
        }));
        let event_emitter = ServiceEventEmitter::new(vec![webui_service_event_bus_sink(
            service_event_bus.clone(),
        )]);
        service_install_conversation_event_v1_emitter(&state, event_emitter.clone())?;
        // The browser host runtime is configured before any session can be created, so a session
        // is never delivered without the background browser that owns it.
        // 在任何会话可被创建之前配置浏览器宿主运行时，因此绝不会交付没有后台浏览器拥有的会话。
        // Shipped components are checked locally; sessions never download a replacement browser.
        // 随包组件只进行本地检查；会话绝不下载替代浏览器。
        let browser_manager = vulcan_service_kernel::browser::service_browser_manager(&state)?;
        let browser_config = service_browser_host_runtime(&data_root);
        browser_manager.configure_runtime_installation(
            None,
            Some(Arc::new({
                let runtime_data_root = data_root.clone();
                move || service_browser_host_runtime(&runtime_data_root)
            })),
            browser_config.as_ref().err().cloned(),
        )?;
        spawn_browser_host_reaper(Arc::clone(&state));
        match browser_config {
            Ok(config) => {
                if let Err(error) = clear_orphaned_browser_data(&data_root, &config) {
                    tracing::error!(
                        "[vulcan-code-service] browser temporary-data cleanup failed: {error}"
                    );
                }
                match vulcan_service_kernel::browser::service_browser_manager(&state) {
                    Ok(manager) => {
                        if let Err(error) = manager.configure_host_runtime(config) {
                            tracing::error!(
                                "[vulcan-code-service] browser host configuration failed: {error}"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::error!(
                            "[vulcan-code-service] browser manager access failed: {error}"
                        );
                    }
                }
            }
            Err(error) => {
                // A missing browser host is reported, never silently replaced by a browser-less
                // session that would look functional but could never show a page.
                // 缺失浏览器宿主会被如实报告，绝不被“看起来可用但永远无法显示页面”的无浏览器会话取代。
                tracing::error!("[vulcan-code-service] Core browser is unavailable: {error}");
            }
        }
        // The HTTP bridge must become reachable only after browser capability is resolved.
        // 浏览器能力完成解析后才允许 HTTP 桥对外可达。
        let http_bridge = vulcan_service_kernel::spawn_webui_service_http_bridge(
            event_emitter.clone(),
            Arc::clone(&state),
            service_event_bus,
            service_webui_root()?,
        )?;
        emit_initial_service_events(&state, &event_emitter);
        vulcan_service_kernel::spawn_service_bootstrap(Arc::clone(&state), event_emitter);
        Ok(Some(Self {
            _guard: guard,
            state,
            _http_bridge: http_bridge,
            shutdown_requested,
            native_diagnostics,
            #[cfg(feature = "test-support")]
            test_fail_shutdown_once: AtomicBool::new(
                std::env::var("VULCAN_TEST_FAIL_FIRST_HEADLESS_SHUTDOWN")
                    .ok()
                    .as_deref()
                    == Some("1"),
            ),
        }))
    }

    /// Returns the shared shutdown flag observed by the termination waiter.
    /// 返回终止等待器观察的共享关闭标志。
    pub fn shutdown_signal(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown_requested)
    }

    /// Runs the unified shutdown coordinator and resumes admission on failure.
    /// 运行统一关闭协调器，失败时恢复准入。
    pub fn shutdown(&self) -> Result<Vec<uuid::Uuid>, String> {
        #[cfg(feature = "test-support")]
        if self.test_fail_shutdown_once.swap(false, Ordering::AcqRel) {
            return Err(String::from("injected first headless shutdown failure"));
        }
        match service_prepare_for_app_shutdown(&self.state) {
            Ok(stopped) => {
                // A requested update starts only after every session was drained.
                // 请求的更新只在所有会话排空之后启动。
                crate::restart::launch_pending();
                // Kernel success includes the original manager's real execution and retirement joins.
                // 内核成功结果包含原管理器真实执行与退役 join。
                // Telemetry failures cannot reopen native resources already closed.
                // 遥测失败不能重新打开已经关闭的原生资源。
                if self
                    .native_diagnostics
                    .finish_after_native_shutdown()
                    .is_err()
                {
                    // Persist only a fixed safe classification; the collector's checkpoint owns details.
                    // 仅持久化固定安全分类；细节由采集器检查点拥有。
                    tracing::error!(
                        target: "vulcan_code_service::native_diagnostics",
                        event = "native_capture_incomplete",
                        reason = "capture_tail_or_persistence_unconfirmed",
                        "Native diagnostic capture is incomplete after successful native shutdown"
                    );
                }
                Ok(stopped)
            }
            Err(error) => {
                let recovery = service_resume_after_failed_app_shutdown(&self.state).err();
                match recovery {
                    Some(recovery_error) => Err(format!(
                        "{error} Shutdown admission recovery also failed: {recovery_error}"
                    )),
                    None => Err(error),
                }
            }
        }
    }
}

/// Blocks until the OS delivers a normal termination signal.
/// 阻塞直至操作系统投递正常终止信号。
#[cfg(unix)]
async fn wait_for_termination_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut terminate = signal(SignalKind::terminate()).expect("SIGTERM handler must install");
    let mut interrupt = signal(SignalKind::interrupt()).expect("SIGINT handler must install");
    tokio::select! {
        _ = terminate.recv() => {},
        _ = interrupt.recv() => {},
    }
}

/// Blocks until the OS delivers Ctrl-C or Ctrl-Break on Windows.
/// 在 Windows 上阻塞直至收到 Ctrl-C 或 Ctrl-Break。
#[cfg(windows)]
async fn wait_for_termination_signal() {
    let mut ctrl_c = tokio::signal::windows::ctrl_c().expect("Ctrl-C handler must install");
    let mut ctrl_break =
        tokio::signal::windows::ctrl_break().expect("Ctrl-Break handler must install");
    tokio::select! {
        _ = ctrl_c.recv() => {},
        _ = ctrl_break.recv() => {},
    }
}

/// Parses launch arguments and builds the description; test-only seam for main().
/// 解析启动参数并构建描述；main() 的仅测试接缝。
#[cfg(test)]
pub(crate) fn describe_for_args(
    args: &[String],
    executable_path: std::path::PathBuf,
    data_root: std::path::PathBuf,
) -> Result<crate::launch::ServiceLaunchDescription, String> {
    crate::launch::parse_service_launch_args(args)?;
    Ok(service_launch_description(executable_path, data_root))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies restart cleanup removes only unreattachable browser data inside the owned root.
    /// 验证重启清理只删除归属数据根内无法重新挂接的浏览器数据。
    #[test]
    fn restart_cleanup_preserves_persistent_browser_profile() {
        // The temporary directory is the exact owned service root for this fixture.
        // 临时目录是本夹具精确拥有的服务数据根。
        let root = tempfile::tempdir().expect("isolated service data root");
        let data_root = root.path();
        // These roots have the same relative ownership as the production layout.
        // 这些根目录与正式布局具有相同的相对归属。
        let profiles_root = data_root.join("browser-profiles");
        let staging_root = data_root.join("browser-staging");
        // The fixture creates each disposable root and a persistent profile to distinguish them.
        // 夹具同时创建每个可丢弃根目录与一个持久画像，以区分两种生命周期。
        for directory in [
            &staging_root,
            &profiles_root.join("ephemeral"),
            &profiles_root.join("control"),
            &profiles_root.join("persistent"),
        ] {
            std::fs::create_dir_all(directory).expect("create browser fixture root");
            std::fs::write(directory.join("marker"), b"fixture")
                .expect("write browser fixture marker");
        }
        // The runtime configuration points only into the isolated owned root.
        // 运行时配置仅指向隔离的已归属根目录。
        let config = vulcan_service_kernel::browser::BrowserHostRuntimeConfig {
            executable: data_root.join("host"),
            entry: data_root.join("entry"),
            chromium_executable: data_root.join("chromium"),
            profiles_root: profiles_root.clone(),
            staging_root: staging_root.clone(),
            discovery_timeout: std::time::Duration::from_secs(1),
            request_timeout: std::time::Duration::from_secs(1),
        };
        clear_orphaned_browser_data(data_root, &config).expect("remove orphaned data");
        assert!(!staging_root.exists());
        assert!(!profiles_root.join("ephemeral").exists());
        assert!(!profiles_root.join("control").exists());
        assert!(profiles_root.join("persistent").join("marker").is_file());
    }

    /// Verifies cleanup refuses a configured root outside this Core's locked data directory.
    /// 验证清理拒绝位于当前 Core 已锁定数据目录之外的配置根目录。
    #[test]
    fn restart_cleanup_rejects_an_outside_directory() {
        // The two independent temporary roots prove an outside directory is never deleted.
        // 两个独立临时根目录证明外部目录绝不会被删除。
        let owned = tempfile::tempdir().expect("owned data root");
        let outside = tempfile::tempdir().expect("outside root");
        // The marker would disappear if cleanup crossed the owned-root boundary.
        // 若清理越过已归属根目录边界，此标记文件便会消失。
        let marker = outside.path().join("marker");
        std::fs::write(&marker, b"preserve").expect("write outside marker");
        // Only the staging root is deliberately outside the otherwise valid configuration.
        // 只有暂存根目录刻意指向有效配置之外。
        let config = vulcan_service_kernel::browser::BrowserHostRuntimeConfig {
            executable: owned.path().join("host"),
            entry: owned.path().join("entry"),
            chromium_executable: owned.path().join("chromium"),
            profiles_root: owned.path().join("browser-profiles"),
            staging_root: outside.path().to_path_buf(),
            discovery_timeout: std::time::Duration::from_secs(1),
            request_timeout: std::time::Duration::from_secs(1),
        };
        assert!(clear_orphaned_browser_data(owned.path(), &config).is_err());
        assert!(marker.is_file());
    }

    /// Verifies the test seam rejects launch modes before building a description.
    /// 验证测试接缝在构建描述前拒绝非法启动模式。
    #[test]
    fn describe_seam_rejects_conflicting_modes() {
        let args = vec![
            String::from("vulcan-code-service"),
            String::from("--service"),
            String::from("--headless"),
        ];
        assert!(
            describe_for_args(
                &args,
                std::path::PathBuf::from("service"),
                std::path::PathBuf::from("data"),
            )
            .is_err()
        );
    }

    /// Verifies describe alone never resolves a launch mode.
    /// 验证单独的 describe 永不解析出启动模式。
    #[test]
    fn describe_alone_reports_missing_mode() {
        let args = vec![
            String::from("vulcan-code-service"),
            String::from("--describe"),
        ];
        assert_eq!(crate::launch::service_describe_requested(&args), Ok(true));
        assert!(crate::launch::parse_service_launch_args(&args).is_err());
    }

    /// Verifies isolated test roots are accepted in test-support builds.
    /// 验证隔离测试根在 test-support 构建中被接受。
    #[test]
    fn isolated_test_root_argument_is_accepted() {
        let args = vec![
            String::from("vulcan-code-service"),
            String::from("--service"),
            String::from("--test-root=C:/isolation/root"),
        ];
        assert!(crate::launch::parse_service_launch_args_impl(&args, true).is_ok());
        assert!(crate::launch::parse_service_launch_args_impl(&args, false).is_err());
    }

    /// Verifies the test-root value form is recognized exactly.
    /// 验证 test-root 值形式被精确识别。
    #[test]
    fn isolated_test_root_value_form_is_recognized() {
        assert_eq!(
            crate::launch::test_root_argument_value("--test-root=C:/isolation/root"),
            Some("C:/isolation/root")
        );
        assert_eq!(crate::launch::test_root_argument_value("--service"), None);
    }

    /// Verifies duplicate test roots are rejected before path resolution.
    /// 验证重复 test-root 在路径解析前被拒绝。
    #[cfg(feature = "test-support")]
    #[test]
    fn duplicate_test_roots_are_rejected() {
        let args = vec![
            String::from("vulcan-code-service"),
            String::from("--service"),
            String::from("--test-root=C:/a"),
            String::from("--test-root=C:/b"),
        ];
        assert!(install_isolated_test_root_from_args(&args).is_err());
    }
}
