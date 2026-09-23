//! Tray host for the independent Vulcan Code service process.
//! 独立 Vulcan Code 服务进程的托盘宿主。

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tauri::{
    Emitter, Manager,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};

use crate::service_runtime::HeadlessService;

/// Stable tray icon identifier owned by the service process.
/// 服务进程持有的稳定托盘图标标识。
const SERVICE_TRAY_ID: &str = "vulcan-code-service";

/// Menu item identifier that opens the desktop client.
/// 打开桌面客户端的菜单项标识。
const MENU_OPEN_CLIENT: &str = "open-client";

/// Menu item identifier that opens the WebUI in the system browser.
/// 在系统浏览器中打开 WebUI 的菜单项标识。
const MENU_OPEN_WEBUI: &str = "open-webui";

/// Menu item identifier that reveals the service log location.
/// 展示服务日志位置的菜单项标识。
const MENU_OPEN_LOGS: &str = "open-logs";

/// Menu item identifier that reports service status text.
/// 报告服务状态文本的菜单项标识。
const MENU_SERVICE_STATUS: &str = "service-status";

/// Menu item identifier that exits the service through the drain coordinator.
/// 经排空协调器退出服务的菜单项标识。
const MENU_QUIT_SERVICE: &str = "quit-service";

/// Shared tray-owned service handles retained for menu actions.
/// 托盘持有的共享服务句柄，供菜单动作使用。
struct TrayServiceHandles {
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
    let service = match HeadlessService::start() {
        Ok(Some(service)) => service,
        Ok(None) => {
            // A concurrent launcher lost the ownership race and must exit successfully.
            // 并发启动器输掉所有权竞争，必须以成功状态退出。
            eprintln!("[vulcan-code-service] another service already owns this data directory");
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("[vulcan-code-service] tray service startup failed: {error}");
            std::process::exit(1);
        }
    };
    let state = service.headless_state();
    let shutdown_in_progress = Arc::new(AtomicBool::new(false));
    let bridge_failure = service.http_bridge_failure_slot();
    let handles = TrayServiceHandles {
        state,
        bridge_failure,
        shutdown_in_progress,
    };
    // Box::leak retains service ownership for the process lifetime; the guard must
    // outlive the tray event loop and is released by the OS on process exit.
    // Box::leak 为进程生命周期保留服务所有权；守卫必须比托盘事件循环更长寿，
    // 进程退出时由操作系统释放。
    let _service: &'static HeadlessService = Box::leak(Box::new(service));
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(handles)
        .invoke_handler(tauri::generate_handler![
            crate::desktop_notifications::notification_permission,
            crate::desktop_notifications::request_notification_permission,
            crate::desktop_notifications::show_notification,
        ])
        .setup(|app| {
            let open_client = MenuItem::with_id(
                app,
                MENU_OPEN_CLIENT,
                "Open Desktop Client",
                true,
                None::<&str>,
            )?;
            let open_webui = MenuItem::with_id(
                app,
                MENU_OPEN_WEBUI,
                "Open WebUI in Browser",
                true,
                None::<&str>,
            )?;
            let open_logs =
                MenuItem::with_id(app, MENU_OPEN_LOGS, "Open Log Location", true, None::<&str>)?;
            let status_text = tray_service_status_text(&app.state::<TrayServiceHandles>());
            let status = MenuItem::with_id(
                app,
                MENU_SERVICE_STATUS,
                status_text.clone(),
                false,
                None::<&str>,
            )?;
            let quit =
                MenuItem::with_id(app, MENU_QUIT_SERVICE, "Quit Service", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[&status, &open_client, &open_webui, &open_logs, &quit],
            )?;
            // Isolated test owners include their PID in the tray tooltip so UI automation
            // cannot mistake the user's real Core icon for this disposable process.
            // 隔离测试所有者在托盘提示中带上 PID，使界面自动化不会误点用户真实 Core 的图标。
            let tooltip = String::from("Vulcan Code Service");
            #[cfg(feature = "test-support")]
            let tooltip = if crate::launch::forwarded_test_root_argument(
                &std::env::args().collect::<Vec<_>>(),
            )
            .is_some()
            {
                format!("Vulcan Code Service Test {}", std::process::id())
            } else {
                tooltip
            };
            // Use the same PNG as the former declarative icon, but register it only once.
            // 使用原声明式图标的同一 PNG，但只注册一次托盘图标。
            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png"))?;
            let _tray = TrayIconBuilder::with_id(SERVICE_TRAY_ID)
                .icon(icon)
                .tooltip(tooltip)
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    MENU_OPEN_CLIENT => open_desktop_client(app),
                    MENU_OPEN_WEBUI => open_webui_in_browser(app),
                    MENU_OPEN_LOGS => open_log_location(app),
                    MENU_QUIT_SERVICE => quit_service(app),
                    _ => {}
                })
                .build(app)?;
            // Startup and shutdown recovery can change after setup. Keep the native menu label
            // aligned with the protected lifecycle, and report an HTTP bridge failure once.
            // 引导与关闭恢复可在设置后变化；持续同步原生菜单与受保护生命周期，并且只报告一次 HTTP 桥故障。
            let failure_slot = Arc::clone(&app.state::<TrayServiceHandles>().bridge_failure);
            let status_item = status.clone();
            let app_handle = app.handle().clone();
            std::thread::Builder::new()
                .name(String::from("vulcan-tray-bridge-supervisor"))
                .spawn(move || {
                    let mut last_status = status_text;
                    let mut bridge_reported = false;
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        let current_status =
                            tray_service_status_text(&app_handle.state::<TrayServiceHandles>());
                        let failure = if bridge_reported {
                            None
                        } else {
                            failure_slot.lock().ok().and_then(|slot| slot.clone())
                        };
                        if current_status == last_status && failure.is_none() {
                            continue;
                        }
                        let status_to_show = current_status.clone();
                        let notification_handle = app_handle.clone();
                        let status_item = status_item.clone();
                        let report = failure.clone();
                        match app_handle.run_on_main_thread(move || {
                            let _ = status_item.set_text(status_to_show);
                            if let Some(failure) = report {
                                report_tray_failure(
                                    &notification_handle,
                                    &format!("Core HTTP bridge failed: {failure}"),
                                );
                            }
                        }) {
                            Ok(()) => {
                                last_status = current_status;
                                bridge_reported |= failure.is_some();
                            }
                            Err(error) => {
                                eprintln!(
                                    "[vulcan-code-service] tray status update failed: {error}"
                                );
                            }
                        }
                    }
                })?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build Vulcan Code service tray")
        .run(|_app, event| {
            // HTTP starts before the native shell; acknowledge the tray only after setup and event-loop readiness.
            // HTTP 先于原生外壳启动；仅在托盘设置和事件循环就绪后确认托盘已启动。
            if matches!(event, tauri::RunEvent::Ready) {
                // Windows can reject tray registration while the library still returns a tray handle.
                // Windows 可能拒绝托盘注册，而库仍然返回托盘句柄。
                #[cfg(windows)]
                match _app.tray_by_id(SERVICE_TRAY_ID).map(|tray| tray.rect()) {
                    Some(Ok(Some(rect))) => {
                        eprintln!("[vulcan-code-service] tray registered: {rect:?}");
                    }
                    result => {
                        eprintln!("[vulcan-code-service] tray registration not confirmed: {result:?}");
                        return;
                    }
                }
                eprintln!("[vulcan-code-service] tray ready (pid {})", std::process::id());
                // Restart tools request a separate acknowledgement because Debug owns its visible console streams.
                // 重启工具请求独立确认，因为调试模式拥有可见控制台输出流。
                if let Some(path) = std::env::var_os("VULCAN_CORE_STARTUP_REPORT") {
                    // The acknowledgement carries only this process identity and native registration outcome.
                    // 确认仅包含本进程身份及原生注册结果。
                    let report = serde_json::json!({
                        "pid": std::process::id(),
                        "trayRegistered": true,
                    });
                    if let Err(error) = std::fs::write(path, report.to_string()) {
                        eprintln!("[vulcan-code-service] startup acknowledgement failed: {error}");
                    }
                }
            }
        });
}

/// Opens the desktop client named by the runtime layout manifest.
/// 打开运行时布局清单声明的桌面客户端。
///
/// # Parameters
/// 参数
/// `app` is the tray application handle used to report a visible failure.
/// `app` 是用于报告可见失败的托盘应用句柄。
///
/// # Returns
/// 返回值
/// Returns nothing; a missing or invalid manifest is reported to the user and never falls back
/// to launching a different application.
/// 无返回值；清单缺失或无效时向用户报告，绝不回退去启动另一个应用。
fn open_desktop_client(app: &tauri::AppHandle) {
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
                        app,
                        &format!("Cannot resolve the running service: {error}"),
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
                report_tray_failure(app, &format!("Desktop client launch failed: {error}"));
            }
        }
        Err(error) => report_tray_failure(app, &error),
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
fn open_log_location(app: &tauri::AppHandle) {
    let state = app.state::<TrayServiceHandles>();
    let data_root = match crate::service_runtime::authoritative_data_root() {
        Ok(root) => root,
        Err(error) => {
            report_tray_failure(app, &error);
            return;
        }
    };
    let _ = Arc::clone(&state.state);
    if let Err(error) =
        crate::desktop_integration::open_path_in_system_file_manager(&data_root.to_string_lossy())
    {
        report_tray_failure(app, &format!("Opening the log location failed: {error}"));
    }
}

/// Reports one tray failure to the user without exposing credentials.
/// 在不暴露凭据的前提下向用户报告一次托盘失败。
fn report_tray_failure(app: &tauri::AppHandle, message: &str) {
    eprintln!("[vulcan-code-service] {message}");
    // A native notification is the only user-visible surface the service process owns.
    // 原生通知是服务进程拥有的唯一用户可见表面。
    if let Err(error) = tauri_plugin_notification::NotificationExt::notification(app)
        .builder()
        .title("Vulcan Code Service")
        .body(message)
        .show()
    {
        eprintln!("[vulcan-code-service] reporting the failure failed: {error}");
    }
}

/// Reports the current tray service status without exposing secrets.
/// 报告当前托盘服务状态，不暴露秘密。
/// `state` owns the protected Core lifecycle; returns the current native menu label.
/// `state` 持有受保护的 Core 生命周期；返回当前原生菜单文字。
fn tray_service_status_text(state: &TrayServiceHandles) -> String {
    if let Ok(failure) = state.bridge_failure.lock()
        && let Some(failure) = failure.as_ref()
    {
        return format!("Service: bridge failed ({failure})");
    }
    // Startup alone remains ready after a failed shutdown; the protected lifecycle projection
    // includes admission and retained recovery failures, so the tray must use that authority.
    // 关闭失败后引导状态仍可能是 ready；受保护生命周期投影包含准入与保留的恢复故障，托盘必须以它为准。
    match vulcan_service_kernel::service_status_snapshot_from_state(&state.state).lifecycle {
        vulcan_service_api::ServiceLifecycle::Ready => String::from("Service: ready"),
        vulcan_service_api::ServiceLifecycle::Starting => String::from("Service: starting"),
        vulcan_service_api::ServiceLifecycle::Stopping => String::from("Service: stopping"),
        vulcan_service_api::ServiceLifecycle::Recovering => String::from("Service: recovering"),
        vulcan_service_api::ServiceLifecycle::Failed => String::from("Service: failed"),
        vulcan_service_api::ServiceLifecycle::RecoveryFailed => {
            String::from("Service: recovery failed; restart required")
        }
    }
}

/// Opens the WebUI in the system browser only for this tray's live Core instance.
/// 仅为当前托盘持有的实时 Core 实例在系统浏览器中打开 WebUI。
fn open_webui_in_browser(app: &tauri::AppHandle) {
    let state = app.state::<TrayServiceHandles>();
    let base_url = read_service_base_url(&state.state);
    let Some(base_url) = base_url else {
        eprintln!("[vulcan-code-service] WebUI address is not published yet");
        return;
    };
    open_url_in_system_browser(&base_url);
}

/// Reads the browser-facing base URL after matching the published record to live Core identity.
/// 将已发布记录与实时 Core 身份核对后读取浏览器侧基础地址。
fn read_service_base_url(
    state: &vulcan_service_kernel::SharedServiceBridgeState,
) -> Option<String> {
    let live = vulcan_service_kernel::service_status_snapshot_from_state(state);
    let path = vulcan_service_api::webui_service_discovery_snapshot_path(
        dirs::home_dir(),
        std::env::temp_dir(),
    );
    let body = std::fs::read_to_string(path).ok()?;
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
        eprintln!("[vulcan-code-service] failed to open WebUI: {error}");
    }
}

/// Enters the unified shutdown coordinator and exits only after a successful drain.
/// 进入统一关闭协调器，仅在成功排空后退出。
fn quit_service(app: &tauri::AppHandle) {
    let state = app.state::<TrayServiceHandles>();
    if state.shutdown_in_progress.swap(true, Ordering::AcqRel) {
        return;
    }
    eprintln!("[vulcan-code-service] tray quit requested");
    let worker_state = Arc::clone(&state.state);
    let worker_flag = Arc::clone(&state.shutdown_in_progress);
    let app_handle = app.clone();
    std::thread::spawn(move || {
        eprintln!("[vulcan-code-service] tray drain started");
        match vulcan_service_kernel::service_prepare_for_app_shutdown(&worker_state) {
            Ok(_) => {
                eprintln!("[vulcan-code-service] tray drain completed");
                if let Err(error) =
                    vulcan_service_kernel::remove_owned_webui_service_discovery(&worker_state)
                {
                    eprintln!("[vulcan-code-service] discovery cleanup failed: {error}");
                }
                app_handle.exit(0);
            }
            Err(error) => {
                eprintln!("[vulcan-code-service] tray drain failed: {error}");
                let recovery =
                    vulcan_service_kernel::service_resume_after_failed_app_shutdown(&worker_state)
                        .err();
                worker_flag.store(false, Ordering::Release);
                // The tray is this process's sole visible surface; a local event has no GUI
                // receiver because the workbench lives in another process.
                // 托盘是本进程唯一可见表面；工作台在另一进程中，本地事件没有 GUI 接收者。
                let message = match recovery {
                    Some(recovery_error) => format!(
                        "Shutdown failed: {error} Admission recovery also failed: {recovery_error}"
                    ),
                    None => format!("Shutdown failed: {error}"),
                };
                report_tray_failure(&app_handle, &message);
                let _ = app_handle.emit("vulcan://service-shutdown-failed", message);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::owned_service_base_url;
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
