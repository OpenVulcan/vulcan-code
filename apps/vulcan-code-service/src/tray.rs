//! Tray host for the independent Vulcan Code service process.
//! 独立 Vulcan Code 服务进程的托盘宿主。

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tao::{
    event::{Event, StartCause},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
};
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};

use crate::service_runtime::HeadlessService;

mod menu_text;

use menu_text::{TrayText, keys};

/// Stable tray icon identifier owned by the service process.
/// 服务进程持有的稳定托盘图标标识。
const SERVICE_TRAY_ID: &str = "vulcan-code-service";

/// How often a ticker thread wakes the event loop to refresh lifecycle state. The tick comes
/// through the loop proxy because `ControlFlow::WaitUntil` never times out on Linux (tao blocks in
/// GTK until an event arrives), which left an update restart waiting for unrelated input.
/// 计时线程唤醒事件循环以刷新生命周期状态的间隔。计时经循环代理送达，因为 Linux 上
/// `ControlFlow::WaitUntil` 永不超时（tao 在 GTK 中阻塞到有事件为止），曾使更新重启等待无关输入。
const TRAY_STATUS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

/// Main-thread events emitted by native menus and the shutdown coordinator.
/// 原生菜单与关闭协调器发往主线程的事件。
enum TrayEvent {
    /// Native menu action carrying its stable item identity.
    /// 携带稳定菜单项标识的原生操作。
    Menu(MenuEvent),
    /// The service has drained and the native event loop may exit.
    /// 服务已排空，原生事件循环可以退出。
    ShutdownComplete,
    /// Drain failed; keep the tray alive and report the explicit recovery outcome.
    /// 排空失败；保留托盘并报告明确的恢复结果。
    /// Carries the raw reason; the event loop words it in the active language.
    /// 携带原始原因；由事件循环以当前语言组织文案。
    ShutdownFailed(String),
    /// The ticker thread asks for a lifecycle refresh.
    /// 计时线程请求刷新生命周期状态。
    Tick,
}

/// Menu item identifier that opens the desktop client.
/// 打开桌面客户端的菜单项标识。
const MENU_OPEN_CLIENT: &str = "open-client";

/// Menu item identifier that opens the WebUI in the system browser.
/// 在系统浏览器中打开 WebUI 的菜单项标识。
const MENU_OPEN_WEBUI: &str = "open-webui";

/// Menu item identifier that opens the terminal client in a new console window.
/// 在新控制台窗口中打开终端客户端的菜单项标识。
#[cfg(windows)]
const MENU_OPEN_TERMINAL: &str = "open-terminal";

/// Menu item identifier that reveals the service log location.
/// 展示服务日志位置的菜单项标识。
const MENU_OPEN_LOGS: &str = "open-logs";

/// Menu item identifier that reports service status text.
/// 报告服务状态文本的菜单项标识。
const MENU_SERVICE_STATUS: &str = "service-status";

/// Menu item identifier that exits the service through the drain coordinator.
/// 经排空协调器退出服务的菜单项标识。
const MENU_QUIT_SERVICE: &str = "quit-service";

/// Native menu items retained so their text can follow the interface language.
/// 保留的原生菜单项，使其文字能跟随界面语言。
struct TrayMenu {
    /// Disabled status line refreshed from the protected lifecycle.
    /// 根据受保护生命周期刷新的禁用状态行。
    status: MenuItem,
    /// Action items paired with their catalog keys.
    /// 与目录键配对的操作菜单项。
    actions: Vec<(MenuItem, &'static str)>,
}

impl TrayMenu {
    /// Rewrites every action label in the active language.
    /// 以当前语言重写每个操作标签。
    fn relabel(&self, text: &TrayText) {
        for (item, key) in &self.actions {
            item.set_text(text.get(key));
        }
    }
}

/// Shared tray-owned service handles retained for menu actions.
/// 托盘持有的共享服务句柄，供菜单动作使用。
struct TrayServiceHandles {
    /// The exact service owner retained across every shutdown worker and native event.
    /// 跨全部关闭工作器及原生事件保留的精确服务所有者。
    service: Arc<HeadlessService>,
    /// Shared service state used by the shutdown coordinator.
    /// 关闭协调器使用的共享服务状态。
    state: vulcan_service_kernel::SharedServiceBridgeState,
    /// Post-acknowledgement HTTP bridge failure observed by the tray supervisor.
    /// 托盘监督器观察到的确认后 HTTP 桥失败。
    bridge_failure: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    /// Blocks duplicate quit requests while the durable drain is running.
    /// 耐久排空运行期间阻止重复退出请求。
    shutdown_in_progress: Arc<AtomicBool>,
}

/// Runs the service with a desktop tray and no workbench window.
/// 运行带桌面托盘、无工作台窗口的服务。
pub fn run_with_tray() {
    // The original service owner shared by the event loop and its nonblocking shutdown worker.
    // 事件循环及其非阻塞关闭工作器共享的原服务所有者。
    let service = match HeadlessService::start() {
        Ok(Some(service)) => Arc::new(service),
        Ok(None) => {
            eprintln!("[vulcan-code-service] another service already owns this data directory");
            return;
        }
        Err(error) => {
            crate::logging::startup_error(&format!(
                "[vulcan-code-service] tray service startup failed: {error}"
            ));
            std::process::exit(1);
        }
    };
    // Menu state and worker ownership both derive from this exact service instance.
    // 菜单状态及工作器所有权均来自此精确服务实例。
    let handles = TrayServiceHandles {
        service: Arc::clone(&service),
        state: service.headless_state(),
        bridge_failure: service.http_bridge_failure_slot(),
        shutdown_in_progress: Arc::new(AtomicBool::new(false)),
    };
    let event_loop = EventLoopBuilder::<TrayEvent>::with_user_event().build();
    // tao sets the macOS activation policy on the built loop; it only takes effect before `run`.
    // tao 在已构建的事件循环上设置 macOS 激活策略，只在 `run` 之前生效。
    #[cfg(target_os = "macos")]
    let event_loop = {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        let mut event_loop = event_loop;
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
        event_loop
    };
    let proxy = event_loop.create_proxy();
    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(TrayEvent::Menu(event));
    }));
    let tick_proxy = proxy.clone();
    if let Err(error) = std::thread::Builder::new()
        .name(String::from("vulcan-tray-tick"))
        .spawn(move || {
            while tick_proxy.send_event(TrayEvent::Tick).is_ok() {
                std::thread::sleep(TRAY_STATUS_INTERVAL);
            }
        })
    {
        crate::logging::startup_error(&format!(
            "[vulcan-code-service] tray ticker startup failed: {error}"
        ));
        std::process::exit(1);
    }
    // Native menu objects stay exclusively on this OS event-loop thread.
    // 原生菜单对象只保存在当前操作系统事件循环线程。
    let mut native_tray: Option<(TrayIcon, TrayMenu)> = None;
    // text starts in the saved language so the first menu never flashes another one.
    // text 以已保存语言启动，使首个菜单不会闪现其他语言。
    let mut text = TrayText::new();
    text.follow(vulcan_service_kernel::service_ui_locale_from_state(&handles.state).as_deref());
    let mut last_status = String::new();
    let mut bridge_reported = false;
    let mut ready_reported = false;
    event_loop.run(move |event, _, control_flow| {
        // Retain the service owner for the entire native event loop without leaking it manually.
        // 在完整原生事件循环期间保留服务所有者，不再手动泄漏对象。
        let _service_owner = &service;
        match event {
            Event::NewEvents(StartCause::Init) => match create_service_tray(&handles, &text) {
                Ok(tray) => native_tray = Some(tray),
                Err(error) => {
                    tracing::error!("[vulcan-code-service] native tray startup failed: {error}");
                    *control_flow = ControlFlow::ExitWithCode(1);
                }
            },
            Event::UserEvent(TrayEvent::Menu(event)) => match event.id.as_ref() {
                MENU_OPEN_CLIENT => open_desktop_client(&text),
                MENU_OPEN_WEBUI => open_webui_in_browser(&handles, &text),
                #[cfg(windows)]
                MENU_OPEN_TERMINAL => open_terminal_client(&text),
                MENU_OPEN_LOGS => open_log_location(&text),
                MENU_QUIT_SERVICE => quit_service(&handles, &proxy, &text),
                _ => {}
            },
            Event::UserEvent(TrayEvent::ShutdownComplete) => {
                // Drop the icon before ending the OS loop so Explorer removes it immediately.
                // 在结束系统循环之前释放图标，让 Explorer 立即移除它。
                native_tray.take();
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(TrayEvent::ShutdownFailed(reason)) => {
                report_tray_failure(&text.with_reason(keys::SHUTDOWN_FAILED, &reason));
            }
            Event::UserEvent(TrayEvent::Tick) => {
                if let Some((tray, menu)) = &native_tray {
                    if !ready_reported {
                        ready_reported = acknowledge_tray_ready(tray);
                    }
                    // A language saved from any client relabels the menu on the next tick.
                    // 任一客户端保存的语言会在下一次刷新时重写菜单标签。
                    let saved_locale =
                        vulcan_service_kernel::service_ui_locale_from_state(&handles.state);
                    if text.follow(saved_locale.as_deref()) {
                        menu.relabel(&text);
                    }
                    let current_status = tray_service_status_text(&handles, &text);
                    if current_status != last_status {
                        menu.status.set_text(&current_status);
                        last_status = current_status;
                    }
                    if !bridge_reported {
                        // Release the bridge lock before calling the operating system notification API.
                        // 调用操作系统通知接口之前释放桥接锁。
                        let failure = handles.bridge_failure.lock().ok().and_then(|v| v.clone());
                        if let Some(failure) = failure {
                            report_tray_failure(&text.with_reason(keys::BRIDGE_FAILED, &failure));
                            bridge_reported = true;
                        }
                    }
                    // An update restart asks for the same drain as the quit menu item.
                    // 更新重启请求与退出菜单项相同的排空。
                    if handles
                        .service
                        .shutdown_signal()
                        .swap(false, Ordering::AcqRel)
                    {
                        quit_service(&handles, &proxy, &text);
                    }
                }
            }
            Event::MainEventsCleared => {
                if !matches!(*control_flow, ControlFlow::ExitWithCode(_)) {
                    *control_flow = ControlFlow::Wait;
                }
            }
            Event::LoopDestroyed => {
                native_tray.take();
            }
            _ => {}
        }
    });
}

/// Creates the menu and icon on the native event-loop thread from the owned service state.
/// 在原生事件循环线程上根据自有服务状态创建菜单和图标。
/// Returns retained native handles or a concrete OS/icon decoding failure.
/// 返回需保留的原生句柄，或具体的系统及图标解码错误。
/// `text` supplies every label in the active language.
/// `text` 以当前语言提供全部标签。
fn create_service_tray(
    state: &TrayServiceHandles,
    text: &TrayText,
) -> Result<(TrayIcon, TrayMenu), String> {
    let status = MenuItem::with_id(
        MENU_SERVICE_STATUS,
        tray_service_status_text(state, text),
        false,
        None,
    );
    // action_ids lists the enabled items in menu order with the catalog key of each label.
    // action_ids 按菜单顺序列出启用的菜单项及其标签目录键。
    let mut action_ids = vec![
        (MENU_OPEN_CLIENT, keys::OPEN_DESKTOP_CLIENT),
        (MENU_OPEN_WEBUI, keys::OPEN_WEBUI),
    ];
    #[cfg(windows)]
    action_ids.push((MENU_OPEN_TERMINAL, keys::OPEN_TERMINAL_CLIENT));
    action_ids.extend([
        (MENU_OPEN_LOGS, keys::OPEN_LOG_LOCATION),
        (MENU_QUIT_SERVICE, keys::QUIT_SERVICE),
    ]);
    let actions: Vec<(MenuItem, &'static str)> = action_ids
        .into_iter()
        .map(|(id, key)| (MenuItem::with_id(id, text.get(key), true, None), key))
        .collect();
    let menu = Menu::with_items(&[&status]).map_err(|error| error.to_string())?;
    for (item, _) in &actions {
        menu.append(item).map_err(|error| error.to_string())?;
    }
    let tooltip = String::from("Vulcan Code Service");
    #[cfg(feature = "test-support")]
    let tooltip =
        if crate::launch::forwarded_test_root_argument(&std::env::args().collect::<Vec<_>>())
            .is_some()
        {
            format!("Vulcan Code Service Test {}", std::process::id())
        } else {
            tooltip
        };
    let tray = TrayIconBuilder::new()
        .with_id(SERVICE_TRAY_ID)
        .with_icon(decode_tray_icon()?)
        .with_menu(Box::new(menu))
        .with_tooltip(tooltip)
        .with_menu_on_left_click(true)
        .build()
        .map_err(|error| error.to_string())?;
    Ok((tray, TrayMenu { status, actions }))
}

/// Decodes the bundled PNG into the RGBA pixels required by the platform tray library.
/// 将内置 PNG 解码为平台托盘库要求的 RGBA 像素。
/// Returns an icon or a decoding error; no alternate icon paths are attempted.
/// 返回图标或解码错误，不尝试其他图标路径。
fn decode_tray_icon() -> Result<Icon, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!("../icons/icon.png")));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    let size = reader
        .output_buffer_size()
        .ok_or("Tray PNG dimensions exceed the decoder limit")?;
    let mut pixels = vec![0; size];
    let info = reader
        .next_frame(&mut pixels)
        .map_err(|error| error.to_string())?;
    pixels.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => pixels,
        png::ColorType::Rgb => pixels
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => pixels.into_iter().flat_map(|v| [v, v, v, 255]).collect(),
        png::ColorType::GrayscaleAlpha => pixels
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => return Err("Tray PNG palette was not expanded".to_owned()),
    };
    Icon::from_rgba(rgba, info.width, info.height).map_err(|error| error.to_string())
}

/// Acknowledges the live event loop only after native tray registration has succeeded.
/// 仅在原生托盘注册成功后确认实时事件循环就绪。
/// Returns false while Windows has not published the icon rectangle; restart tools keep waiting.
/// Windows 尚未发布图标矩形时返回 false，重启工具继续等待。
fn acknowledge_tray_ready(tray: &TrayIcon) -> bool {
    #[cfg(windows)]
    if tray.rect().is_none() {
        return false;
    }
    #[cfg(not(windows))]
    let _ = tray;
    if let Some(path) = std::env::var_os("VULCAN_CORE_STARTUP_REPORT") {
        let report = serde_json::json!({ "pid": std::process::id(), "trayRegistered": true });
        if let Err(error) = std::fs::write(path, report.to_string()) {
            tracing::error!("[vulcan-code-service] startup acknowledgement failed: {error}");
            return false;
        }
    }
    tracing::info!(
        "[vulcan-code-service] tray ready (pid {})",
        std::process::id()
    );
    true
}

/// Opens the desktop client named by the runtime layout manifest.
/// 打开运行时布局清单声明的桌面客户端。
///
/// # Returns
/// 返回值
/// Returns nothing; a missing or invalid manifest is reported to the user and never falls back
/// to launching a different application.
/// 无返回值；清单缺失或无效时向用户报告，绝不回退去启动另一个应用。
/// `text` words any failure in the active language.
/// `text` 以当前语言组织失败文案。
fn open_desktop_client(text: &TrayText) {
    match resolve_desktop_client_entry() {
        Ok((executable, entry, manifest_path)) => {
            let mut command = std::process::Command::new(&executable);
            command
                .arg(entry)
                .env(crate::runtime_layout::RUNTIME_LAYOUT_ENV, manifest_path);
            match std::env::current_exe() {
                Ok(service_executable) => {
                    command.arg(format!(
                        "--service-executable={}",
                        service_executable.display()
                    ));
                }
                Err(error) => {
                    report_tray_failure(
                        &text.with_reason(keys::DESKTOP_CLIENT_FAILED, &error.to_string()),
                    );
                    return;
                }
            }
            // An isolated tray must reopen its own namespace, never the real user's service.
            // 隔离托盘必须重新打开自身命名空间，绝不连接真实用户服务。
            // test_root_argument keeps the reopened GUI in this Core's exact isolated namespace.
            // test_root_argument 让重新打开的 GUI 继续使用此 Core 的精确隔离命名空间。
            if let Some(test_root_argument) =
                crate::launch::forwarded_test_root_argument(&std::env::args().collect::<Vec<_>>())
            {
                command.arg(test_root_argument);
            }
            vulcan_host_tools::process_creation::apply_windows_release_no_window_to_std_command(
                &mut command,
            );
            if let Err(error) = command.spawn() {
                report_tray_failure(
                    &text.with_reason(keys::DESKTOP_CLIENT_FAILED, &error.to_string()),
                );
            }
        }
        Err(error) => report_tray_failure(&text.with_reason(keys::DESKTOP_CLIENT_FAILED, &error)),
    }
}

/// Resolves the desktop client executable from the validated runtime layout manifest.
/// 从已校验的运行时布局清单解析桌面客户端可执行文件。
///
/// # Returns
/// 返回值
/// The absolute compiled desktop entry path, or a user-facing diagnostic.
/// 返回绝对桌面客户端入口路径，否则返回面向用户的诊断。
fn resolve_desktop_client_entry()
-> Result<(std::path::PathBuf, std::path::PathBuf, std::path::PathBuf), String> {
    use crate::runtime_layout::{
        explicit_runtime_layout_path, read_runtime_layout_manifest_from,
        read_runtime_layout_manifest_with_path, resolve_layout_entry, runtime_layout_root,
    };

    let (manifest, manifest_path) = match explicit_runtime_layout_path()? {
        Some(explicit) => {
            let manifest = read_runtime_layout_manifest_from(&explicit)?;
            (manifest, explicit)
        }
        None => read_runtime_layout_manifest_with_path()?,
    };
    let root = runtime_layout_root(&manifest_path)?;
    let desktop_entry = resolve_layout_entry(&root, &manifest.desktop_entry)?;
    if !desktop_entry.is_file() {
        return Err(format!(
            "Desktop client entry {} declared by {} does not exist.",
            desktop_entry.display(),
            manifest_path.display()
        ));
    }
    let executable = resolve_layout_entry(&root, &manifest.desktop_executable)?;
    if !executable.is_file() {
        return Err(format!(
            "Desktop client executable {} declared by {} does not exist.",
            executable.display(),
            manifest_path.display()
        ));
    }
    Ok((executable, desktop_entry, manifest_path))
}

/// Reveals the service log location with the host file manager.
/// 使用宿主文件管理器展示服务日志位置。
/// `text` words any failure in the active language.
/// `text` 以当前语言组织失败文案。
fn open_log_location(text: &TrayText) {
    let data_root = match crate::service_runtime::authoritative_data_root() {
        Ok(root) => root,
        Err(error) => {
            report_tray_failure(&text.with_reason(keys::LOG_LOCATION_FAILED, &error));
            return;
        }
    };
    if let Err(error) = crate::desktop_integration::open_path_in_system_file_manager(
        &crate::logging::directory(&data_root).to_string_lossy(),
    ) {
        report_tray_failure(&text.with_reason(keys::LOG_LOCATION_FAILED, &error));
    }
}

/// Reports one tray failure to the user without exposing credentials.
/// 在不暴露凭据的前提下向用户报告一次托盘失败。
fn report_tray_failure(message: &str) {
    tracing::error!("[vulcan-code-service] {message}");
    if let Err(error) = crate::desktop_notifications::show_service_failure(message) {
        tracing::error!("[vulcan-code-service] reporting the failure failed: {error}");
    }
}

/// Reports the current tray service status without exposing secrets.
/// 报告当前托盘服务状态，不暴露秘密。
/// `state` owns the protected Core lifecycle and `text` the active language; returns the
/// current native menu label.
/// `state` 持有受保护的 Core 生命周期，`text` 持有当前语言；返回当前原生菜单文字。
fn tray_service_status_text(state: &TrayServiceHandles, text: &TrayText) -> String {
    if let Ok(failure) = state.bridge_failure.lock()
        && let Some(failure) = failure.as_ref()
    {
        return text.with_reason(keys::STATUS_BRIDGE_FAILED, failure);
    }
    // Startup alone remains ready after a failed shutdown; the protected lifecycle projection
    // includes admission and retained recovery failures, so the tray must use that authority.
    // 关闭失败后引导状态仍可能是 ready；受保护生命周期投影包含准入与保留的恢复故障，托盘必须以它为准。
    use vulcan_service_api::ServiceLifecycle;
    let lifecycle =
        vulcan_service_kernel::service_status_snapshot_from_state(&state.state).lifecycle;
    let key = match lifecycle {
        ServiceLifecycle::Ready => keys::STATUS_READY,
        ServiceLifecycle::Starting => keys::STATUS_STARTING,
        ServiceLifecycle::Stopping => keys::STATUS_STOPPING,
        ServiceLifecycle::Recovering => keys::STATUS_RECOVERING,
        ServiceLifecycle::Failed => keys::STATUS_FAILED,
        ServiceLifecycle::RecoveryFailed => keys::STATUS_RECOVERY_FAILED,
    };
    text.get(key).to_owned()
}

/// Opens the WebUI in the system browser only for this tray's live Core instance.
/// 仅为当前托盘持有的实时 Core 实例在系统浏览器中打开 WebUI。
/// `text` words any failure in the active language.
/// `text` 以当前语言组织失败文案。
fn open_webui_in_browser(state: &TrayServiceHandles, text: &TrayText) {
    let Some(base_url) = read_service_base_url(&state.state) else {
        report_tray_failure(text.get(keys::WEBUI_UNPUBLISHED));
        return;
    };
    open_url_in_system_browser(&base_url);
}

/// Returns the discovery record path this Core publishes, honoring an isolated test root.
/// 返回此 Core 发布的发现记录路径，并遵循隔离测试根。
fn service_discovery_path() -> std::path::PathBuf {
    vulcan_service_api::webui_service_discovery_snapshot_path(
        vulcan_runtime_assets::user_home_dir(),
        std::env::temp_dir(),
    )
}

/// Opens the terminal client in a new console window, bound to this Core's discovery root.
/// 在新控制台窗口中打开终端客户端，并绑定到此 Core 的发现根目录。
/// `text` words any failure in the active language.
/// `text` 以当前语言组织失败文案。
#[cfg(windows)]
fn open_terminal_client(text: &TrayText) {
    if let Err(reason) = launch_terminal_client() {
        report_tray_failure(&text.with_reason(keys::TERMINAL_CLIENT_FAILED, &reason));
    }
}

/// Starts the manifest-declared terminal client against the exact discovery root of this Core.
/// 针对此 Core 的精确发现根目录启动清单声明的终端客户端。
/// Returns a diagnostic when the manifest, the executable or the process start fails.
/// 清单、可执行文件或进程启动失败时返回诊断。
#[cfg(windows)]
fn launch_terminal_client() -> Result<(), String> {
    use crate::runtime_layout::{
        explicit_runtime_layout_path, read_runtime_layout_manifest_from,
        read_runtime_layout_manifest_with_path, resolve_layout_entry, runtime_layout_root,
    };

    let (manifest, manifest_path) = match explicit_runtime_layout_path()? {
        Some(explicit) => (read_runtime_layout_manifest_from(&explicit)?, explicit),
        None => read_runtime_layout_manifest_with_path()?,
    };
    let root = runtime_layout_root(&manifest_path)?;
    let executable = resolve_layout_entry(&root, &manifest.tui_executable)?;
    if !executable.is_file() {
        return Err(format!(
            "Terminal client {} declared by {} does not exist.",
            executable.display(),
            manifest_path.display()
        ));
    }
    // The client reads `<root>/code/webui-service.json`, so it gets the root two levels up.
    // 客户端读取 `<root>/code/webui-service.json`，因此传入上两级的根目录。
    let discovery = service_discovery_path();
    let service_root = discovery
        .parent()
        .and_then(std::path::Path::parent)
        .ok_or_else(|| format!("Discovery path {} has no root.", discovery.display()))?;
    vulcan_host_tools::process_creation::new_console_std_command(&executable)
        .arg("--service-root")
        .arg(service_root)
        .spawn()
        .map(drop)
        .map_err(|error| error.to_string())
}

/// Reads the browser-facing base URL after matching the published record to live Core identity.
/// 将已发布记录与实时 Core 身份核对后读取浏览器侧基础地址。
fn read_service_base_url(
    state: &vulcan_service_kernel::SharedServiceBridgeState,
) -> Option<String> {
    let live = vulcan_service_kernel::service_status_snapshot_from_state(state);
    let body = std::fs::read_to_string(service_discovery_path()).ok()?;
    let snapshot: vulcan_service_api::WebuiServiceDiscoverySnapshot =
        serde_json::from_str(&body).ok()?;
    owned_service_base_url(&snapshot, &live)
}

/// Validates a persisted discovery URL against this process's live protected status.
/// 将持久化发现地址与本进程的实时受保护状态核对。
/// `snapshot` is the disk record and `live` is the tray-owned Core status; returns its loopback URL only for the same active generation.
/// `snapshot` 是磁盘记录，`live` 是托盘持有的 Core 状态；仅在同一活动代次时返回回环地址。
fn owned_service_base_url(
    snapshot: &vulcan_service_api::WebuiServiceDiscoverySnapshot,
    live: &vulcan_service_api::ServiceStatusSnapshot,
) -> Option<String> {
    use vulcan_service_api::{
        ServiceLifecycle, WEBUI_SERVICE_DISCOVERY_STATUS_READY,
        WEBUI_SERVICE_DISCOVERY_STATUS_STARTING, WEBUI_SERVICE_LOOPBACK_HOST,
    };

    if !matches!(
        live.lifecycle,
        ServiceLifecycle::Starting | ServiceLifecycle::Ready
    ) || !matches!(
        snapshot.status.as_str(),
        WEBUI_SERVICE_DISCOVERY_STATUS_STARTING | WEBUI_SERVICE_DISCOVERY_STATUS_READY
    ) || snapshot.pid != live.pid
        || snapshot.started_at != live.started_at
        || snapshot.host != WEBUI_SERVICE_LOOPBACK_HOST
        || snapshot.port == 0
        || snapshot.base_url != format!("http://{WEBUI_SERVICE_LOOPBACK_HOST}:{}", snapshot.port)
    {
        return None;
    }
    Some(snapshot.base_url.clone())
}

/// Opens one URL with the host default browser without shell interpretation.
/// 使用宿主默认浏览器打开单个 URL，不经过命令行 Shell 解析。
fn open_url_in_system_browser(url: &str) {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler").arg(url);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(url);
        command
    };
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url);
        command
    };
    vulcan_host_tools::process_creation::apply_windows_release_no_window_to_std_command(
        &mut command,
    );
    if let Err(error) = command.spawn() {
        tracing::error!("[vulcan-code-service] failed to open WebUI: {error}");
    }
}

/// Enters the unified shutdown coordinator and exits only after a successful drain.
/// 进入统一关闭协调器，仅在成功排空后退出。
/// `text` words a coordinator start failure in the active language.
/// `text` 以当前语言组织协调器启动失败文案。
fn quit_service(state: &TrayServiceHandles, proxy: &EventLoopProxy<TrayEvent>, text: &TrayText) {
    if state.shutdown_in_progress.swap(true, Ordering::AcqRel) {
        return;
    }
    tracing::info!("[vulcan-code-service] tray quit requested");
    // Clone the original owner, ensuring its capture outlives the worker and its completion event.
    // 克隆原所有者，保证其采集生命周期覆盖工作器及完成事件。
    let worker_service = Arc::clone(&state.service);
    // The original flag continues to suppress duplicate quit requests during the bounded drain.
    // 原标志继续在有界排空期间抑制重复退出请求。
    let worker_flag = Arc::clone(&state.shutdown_in_progress);
    // Return the concrete result to the same native event loop without blocking its thread.
    // 将具体结果返回同原生事件循环，不阻塞其线程。
    let proxy = proxy.clone();
    // The sole OS worker owns the complete native shutdown and diagnostic acknowledgement boundary.
    // 唯一系统工作器拥有完整原生关闭及诊断回执边界。
    let spawned = std::thread::Builder::new()
        .name(String::from("vulcan-tray-shutdown"))
        .spawn(move || {
            let _ = proxy.send_event(run_shutdown_worker(&worker_service, &worker_flag));
        });
    if let Err(error) = spawned {
        state.shutdown_in_progress.store(false, Ordering::Release);
        report_tray_failure(&text.with_reason(keys::SHUTDOWN_START_FAILED, &error.to_string()));
    }
}

/// Close the same service owner on the worker and return the event after capture settlement.
/// 在工作线程关闭同服务所有者，并在采集收口后返回事件。
/// `service` is the original owner; `shutdown_in_progress` resets only on business shutdown failure.
/// `service` 是原所有者；`shutdown_in_progress` 仅在业务关闭失败时重置。
/// Return ShutdownComplete after the owner boundary, or the actual business failure.
/// 所有者边界结束后返回 ShutdownComplete，否则返回实际业务失败。
fn run_shutdown_worker(service: &HeadlessService, shutdown_in_progress: &AtomicBool) -> TrayEvent {
    tracing::info!("[vulcan-code-service] tray drain started");
    match service.shutdown() {
        Ok(_) => {
            // Discovery cleanup uses this exact service state after its shutdown and capture boundary.
            // 发现清理在关闭及采集边界结束后使用此精确服务状态。
            let state = service.headless_state();
            if let Err(error) = vulcan_service_kernel::remove_owned_webui_service_discovery(&state)
            {
                tracing::error!("[vulcan-code-service] discovery cleanup failed: {error}");
            }
            TrayEvent::ShutdownComplete
        }
        Err(error) => {
            // HeadlessService owns the admission-recovery decision; never perform a second attempt.
            // HeadlessService 拥有准入恢复决定；绝不执行第二次尝试。
            shutdown_in_progress.store(false, Ordering::Release);
            TrayEvent::ShutdownFailed(error)
        }
    }
}

/// Real owner/capture closure cases isolated from process-global subscriber state.
/// 与进程级订阅状态隔离的真实所有者及采集关闭场景。
#[cfg(all(test, feature = "test-support"))]
#[path = "tray/shutdown_tests.rs"]
mod shutdown_tests;

#[cfg(test)]
mod tests {
    use super::owned_service_base_url;

    /// Confirms the authoritative bundled icon decodes without a Tauri image runtime.
    /// 确认权威内置图标可在不使用 Tauri 图像运行时的情况下解码。
    #[test]
    fn bundled_tray_icon_decodes() {
        super::decode_tray_icon().expect("bundled Core tray PNG must decode");
    }
    use vulcan_service_api::{
        ServiceActiveWorkSummary, ServiceLifecycle, ServiceStatusSnapshot,
        webui_service_ready_discovery_snapshot,
    };

    /// Ensures the tray never opens a stale, failed, or rewritten discovery target.
    /// 确保托盘不会打开过期、失败或被改写的发现目标。
    #[test]
    fn webui_menu_uses_only_its_live_loopback_generation() {
        let live = ServiceStatusSnapshot {
            lifecycle: ServiceLifecycle::Ready,
            pid: 42,
            started_at: String::from("123"),
            revision: 1,
            last_failure: None,
            active_work: ServiceActiveWorkSummary::default(),
        };
        let valid = webui_service_ready_discovery_snapshot(17601, 42, "123", "private-token");
        assert_eq!(
            owned_service_base_url(&valid, &live).as_deref(),
            Some("http://127.0.0.1:17601")
        );
        let mut stale_pid = valid.clone();
        stale_pid.pid = 43;
        assert_eq!(owned_service_base_url(&stale_pid, &live), None);
        let mut stale_generation = valid.clone();
        stale_generation.started_at = String::from("122");
        assert_eq!(owned_service_base_url(&stale_generation, &live), None);
        let mut rewritten_url = valid.clone();
        rewritten_url.base_url = String::from("https://example.invalid/");
        assert_eq!(owned_service_base_url(&rewritten_url, &live), None);
        let mut failed = valid.clone();
        failed.status = String::from("failed");
        assert_eq!(owned_service_base_url(&failed, &live), None);
        let mut stopped = live;
        stopped.lifecycle = ServiceLifecycle::Stopping;
        assert_eq!(owned_service_base_url(&valid, &stopped), None);
    }
}
