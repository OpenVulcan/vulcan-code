//! Tauri desktop shell for the Vulcan Code service kernel.
//! Vulcan Code 服务内核的 Tauri 桌面壳。

use std::{
    path::Path,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use tauri::Emitter;
use tokio::sync::broadcast;
use vulcan_host_tools::process_creation::apply_windows_release_no_window_to_std_command;
use vulcan_service_api::{ServiceEvent, ServiceEventEmitter, ServiceEventSink};
use vulcan_service_host::webui_service_event_bus_sink;

use vulcan_service_kernel::{
    ServiceShellHost, create_service_bridge_state, emit_initial_service_events,
    service_install_conversation_event_v1_emitter, service_prepare_for_app_shutdown,
    service_resume_after_failed_app_shutdown, spawn_service_bootstrap,
    spawn_webui_service_http_bridge,
};

/// Tauri desktop event sink used only by the desktop shell.
/// 仅桌面壳使用的 Tauri 桌面事件接收器。
struct TauriServiceEventSink {
    /// Tauri application handle used for desktop WebView events.
    /// 用于桌面 WebView 事件的 Tauri 应用句柄。
    app: tauri::AppHandle,
}

/// ShutdownControl coordinates retryable persistence failures with one explicit force-close decision.
/// ShutdownControl 协调可重试的持久化失败与一次明确的强制关闭决定。
struct ShutdownControl {
    /// shutdown_in_progress blocks duplicate close requests while the durable drain is running.
    /// shutdown_in_progress 在耐久排空运行期间阻止重复关闭请求。
    shutdown_in_progress: Arc<AtomicBool>,
    /// shutdown_close_allowed admits exactly one close after the user accepts possible tail loss.
    /// shutdown_close_allowed 仅在用户接受可能丢失尾部记录后允许关闭一次。
    shutdown_close_allowed: Arc<AtomicBool>,
    /// shutdown_force_available records that the latest drain failure awaits an explicit force-close decision.
    /// shutdown_force_available 记录最近一次排空失败正在等待明确的强制关闭决定。
    shutdown_force_available: Arc<AtomicBool>,
}

/// force_close_window admits the already-confirmed native close without restarting persistence.
/// force_close_window 在用户已确认后允许原生关闭，避免再次启动持久化流程。
#[tauri::command]
fn force_close_window(
    window: tauri::Window,
    shutdown_control: tauri::State<'_, ShutdownControl>,
) -> Result<(), String> {
    if !shutdown_control
        .shutdown_force_available
        .swap(false, Ordering::AcqRel)
    {
        return Err(String::from(
            "No failed shutdown is waiting for force-close confirmation",
        ));
    }
    shutdown_control
        .shutdown_close_allowed
        .store(true, Ordering::Release);
    shutdown_control
        .shutdown_in_progress
        .store(false, Ordering::Release);
    if let Err(error) = window.destroy() {
        shutdown_control
            .shutdown_close_allowed
            .store(false, Ordering::Release);
        shutdown_control
            .shutdown_force_available
            .store(true, Ordering::Release);
        return Err(error.to_string());
    }
    Ok(())
}

/// Desktop implementation of host shell capabilities.
/// 宿主 shell 能力的桌面端实现。
struct DesktopServiceShellHost;

impl ServiceShellHost for DesktopServiceShellHost {
    /// Opens one local path with the system file manager.
    /// 使用系统文件管理器打开单个本地路径。
    fn open_path(&self, path: &str) -> Result<(), String> {
        open_path_in_system_file_manager(path)
    }

    /// Opens one URL with the system default browser.
    /// 使用系统默认浏览器打开单个 URL。
    fn open_url(&self, url: &str) -> Result<(), String> {
        open_url_in_system_browser(url)
    }
}

impl ServiceEventSink for TauriServiceEventSink {
    /// Emits one event through the desktop Tauri event channel.
    /// 通过桌面 Tauri 事件通道发送单个事件。
    fn emit_service_event(&self, event: ServiceEvent) {
        if matches!(
            &event,
            ServiceEvent::CommandSessionDisplayChanged(_)
                | ServiceEvent::ProjectTerminalDisplayChanged(_)
        ) {
            // Terminal content is routed only by the client-filtered WebSocket sink.
            // 终端内容仅由按客户端过滤的 WebSocket 接收端路由。
            return;
        }
        let _ = self.app.emit("vulcan://service-event", event);
    }
}

/// Builds one desktop emitter from the concrete frontend sinks.
/// 从具体前端 sink 构建桌面事件发送器。
///
/// # Parameters
/// 参数
/// `app` is the Tauri application handle used for desktop WebView events.
/// `app` 是用于桌面 WebView 事件的 Tauri 应用句柄。
/// `service_event_bus` is the broadcast bus consumed by browser WebSocket clients.
/// `service_event_bus` 是浏览器 WebSocket 客户端消费的广播总线。
///
/// # Returns
/// 返回值
/// A shared event emitter that sends to both desktop and browser frontends.
/// 返回同时发送到桌面端和浏览器端前端的共享事件发送器。
fn service_event_emitter_from_tauri_app(
    app: tauri::AppHandle,
    service_event_bus: broadcast::Sender<ServiceEvent>,
) -> ServiceEventEmitter {
    ServiceEventEmitter::new(vec![
        webui_service_event_bus_sink(service_event_bus),
        Arc::new(TauriServiceEventSink { app }) as Arc<dyn ServiceEventSink>,
    ])
}

/// Runs the Tauri application and registers desktop-only commands.
/// 运行 Tauri 应用并注册仅桌面壳拥有的命令。
pub fn run() {
    let (service_event_bus, _) = broadcast::channel(256);
    let state = create_service_bridge_state(Arc::new(DesktopServiceShellHost));
    let managed_state = Arc::clone(&state);
    let shutdown_state = Arc::clone(&state);
    // shutdown_in_progress closes the repeat-click window while durable writers are draining.
    // shutdown_in_progress 在耐久写入器排空期间关闭重复点击窗口。
    let shutdown_in_progress = Arc::new(AtomicBool::new(false));
    // shutdown_close_allowed lets only a successfully drained recursive close event proceed.
    // shutdown_close_allowed 仅允许已经成功排空后的递归关闭事件继续。
    let shutdown_close_allowed = Arc::new(AtomicBool::new(false));
    // shutdown_force_available exposes only a failed drain to the explicit force-close command.
    // shutdown_force_available 仅向明确的强制关闭命令暴露排空失败状态。
    let shutdown_force_available = Arc::new(AtomicBool::new(false));
    let shutdown_control = ShutdownControl {
        shutdown_in_progress: Arc::clone(&shutdown_in_progress),
        shutdown_close_allowed: Arc::clone(&shutdown_close_allowed),
        shutdown_force_available: Arc::clone(&shutdown_force_available),
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(state)
        .manage(shutdown_control)
        .invoke_handler(tauri::generate_handler![
            force_close_window,
            crate::desktop_notifications::notification_permission,
            crate::desktop_notifications::request_notification_permission,
            crate::desktop_notifications::show_notification,
        ])
        .on_window_event(move |window, event| {
            let tauri::WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };
            if shutdown_close_allowed.load(Ordering::Acquire) {
                return;
            }
            api.prevent_close();
            if shutdown_in_progress.swap(true, Ordering::AcqRel) {
                return;
            }
            shutdown_force_available.store(false, Ordering::Release);
            // The start event makes the bounded durable drain visible before blocking work begins.
            // 开始事件在阻塞排空工作启动前，让有界耐久保存过程对用户可见。
            if let Err(error) = window.emit("vulcan://shutdown-persist-started", ()) {
                eprintln!(
                    "[vulcan-code-tauri] emitting shutdown persistence start failed: {error}"
                );
            }
            // shutdown_worker_state owns the service handles needed by the blocking drain coordinator.
            // shutdown_worker_state 持有阻塞排空协调器所需的服务句柄。
            let shutdown_worker_state = Arc::clone(&shutdown_state);
            // shutdown_worker_in_progress publishes retry eligibility after a failed drain.
            // shutdown_worker_in_progress 在排空失败后发布可重试资格。
            let shutdown_worker_in_progress = Arc::clone(&shutdown_in_progress);
            // shutdown_worker_close_allowed publishes the only normally closable state.
            // shutdown_worker_close_allowed 发布唯一允许正常关闭的状态。
            let shutdown_worker_close_allowed = Arc::clone(&shutdown_close_allowed);
            // shutdown_worker_force_available publishes the explicit force-close recovery state.
            // shutdown_worker_force_available 发布明确的强制关闭恢复状态。
            let shutdown_worker_force_available = Arc::clone(&shutdown_force_available);
            // shutdown_window is the exact window that originated this close request.
            // shutdown_window 是发起当前关闭请求的精确窗口。
            let shutdown_window = window.clone();
            std::thread::spawn(move || {
                match service_prepare_for_app_shutdown(&shutdown_worker_state) {
                    Ok(_) => {
                        shutdown_worker_close_allowed.store(true, Ordering::Release);
                        if let Err(error) = shutdown_window.close() {
                            // Reopen admission because the drained process remains alive after the close failure.
                            // 关闭失败后已排空的进程仍然存活，因此重新开放准入。
                            let recovery_failure =
                                service_resume_after_failed_app_shutdown(&shutdown_worker_state)
                                    .err();
                            shutdown_worker_close_allowed.store(false, Ordering::Release);
                            shutdown_worker_in_progress.store(false, Ordering::Release);
                            shutdown_worker_force_available.store(true, Ordering::Release);
                            let failure = match recovery_failure {
                                Some(recovery_error) => format!(
                                    "{error} Shutdown admission recovery also failed: {recovery_error}"
                                ),
                                None => error.to_string(),
                            };
                            eprintln!(
                                "[vulcan-code-tauri] closing drained app window failed: {failure}"
                            );
                            let _ = shutdown_window
                                .emit("vulcan://shutdown-persist-failed", failure);
                        }
                    }
                    Err(error) => {
                        // Reopen admission before the UI lets the user keep working or retry close.
                        // 在界面允许用户继续工作或重试关闭之前重新开放准入。
                        let recovery_failure =
                            service_resume_after_failed_app_shutdown(&shutdown_worker_state).err();
                        shutdown_worker_in_progress.store(false, Ordering::Release);
                        shutdown_worker_force_available.store(true, Ordering::Release);
                        let failure = match recovery_failure {
                            Some(recovery_error) => format!(
                                "{error} Shutdown admission recovery also failed: {recovery_error}"
                            ),
                            None => error,
                        };
                        eprintln!("[vulcan-code-tauri] app shutdown preparation failed: {failure}");
                        let _ = shutdown_window.emit("vulcan://shutdown-persist-failed", failure);
                    }
                }
            });
        })
        .setup(move |app| {
            let event_emitter = service_event_emitter_from_tauri_app(
                app.handle().clone(),
                service_event_bus.clone(),
            );
            service_install_conversation_event_v1_emitter(&managed_state, event_emitter.clone())?;
            spawn_webui_service_http_bridge(
                event_emitter.clone(),
                Arc::clone(&managed_state),
                service_event_bus.clone(),
            );
            emit_initial_service_events(&managed_state, &event_emitter);
            spawn_service_bootstrap(Arc::clone(&managed_state), event_emitter);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run Vulcan Code Tauri shell");
}

/// Opens one filesystem directory with the host file manager.
/// 使用宿主文件管理器打开单个文件系统目录。
///
/// # Parameters
/// 参数
/// `path` is the local filesystem path to reveal or open.
/// `path` 是需要展示或打开的本地文件系统路径。
///
/// # Returns
/// 返回值
/// `Ok(())` when the operating system accepted the open request.
/// 当操作系统接受打开请求时返回 `Ok(())`。
fn open_path_in_system_file_manager(path: &str) -> Result<(), String> {
    // native_path is the standard host spelling passed to every platform file-manager process.
    // native_path 是传给各平台文件管理器进程的标准宿主路径写法。
    let native_path = vulcan_runtime_assets::native_host_path_text(path);
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        if Path::new(&native_path).is_file() {
            command.arg(format!("/select,{native_path}"));
        } else {
            command.arg(&native_path);
        }
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        if Path::new(&native_path).is_file() {
            command.arg("-R").arg(&native_path);
        } else {
            command.arg(&native_path);
        }
        command
    };

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        if Path::new(&native_path).is_file() {
            if let Some(parent) = Path::new(&native_path).parent() {
                command.arg(parent);
            } else {
                command.arg(&native_path);
            }
        } else {
            command.arg(&native_path);
        }
        command
    };

    apply_windows_release_no_window_to_std_command(&mut command);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Builds one platform browser command without routing URL metacharacters through a shell.
/// 构建平台浏览器命令，避免 URL 元字符再次经过命令行 Shell 解析。
///
/// # Parameters
/// 参数
/// `url` is the complete validated external URL that must remain one process argument.
/// `url` 是必须作为单个进程参数保留的完整已校验外部 URL。
///
/// # Returns
/// 返回值
/// A platform command ready for the common release-window and spawn policies.
/// 返回可应用公共 Release 窗口策略并启动的平台命令。
fn system_browser_command(url: &str) -> Command {
    #[cfg(target_os = "windows")]
    let command = {
        // rundll32 invokes the registered URL protocol handler directly, so `&` remains URL data.
        // rundll32 直接调用已注册的 URL 协议处理器，因此 `&` 保持为 URL 数据。
        let mut command = Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler").arg(url);
        command
    };

    #[cfg(target_os = "macos")]
    let command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    command
}

/// Opens one URL with the host default browser.
/// 使用宿主默认浏览器打开单个 URL。
///
/// # Parameters
/// 参数
/// `url` is the trusted URL to open in the host browser.
/// `url` 是需要在宿主浏览器中打开的可信 URL。
///
/// # Returns
/// 返回值
/// `Ok(())` when the operating system accepted the open request.
/// 当操作系统接受打开请求时返回 `Ok(())`。
fn open_url_in_system_browser(url: &str) -> Result<(), String> {
    let mut command = system_browser_command(url);
    apply_windows_release_no_window_to_std_command(&mut command);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::system_browser_command;
    use std::ffi::OsStr;

    /// Verifies Windows preserves a multi-parameter authorization URL as one process argument.
    /// 验证 Windows 将包含多个参数的授权 URL 完整保留为单个进程参数。
    #[test]
    fn windows_browser_command_preserves_complete_authorization_url() {
        // authorizationUrl reproduces the CommandCode callback and state separator that cmd.exe split.
        // authorizationUrl 复现曾被 cmd.exe 拆分的 CommandCode 回调与 state 分隔符。
        let authorization_url = "https://commandcode.ai/studio/auth/cli?callback=http%3A%2F%2Flocalhost%3A5959%2Fcallback&state=test-state-value";
        let command = system_browser_command(authorization_url);
        let arguments = command.get_args().collect::<Vec<_>>();

        assert_eq!(command.get_program(), OsStr::new("rundll32.exe"));
        assert_eq!(
            arguments,
            vec![
                OsStr::new("url.dll,FileProtocolHandler"),
                OsStr::new(authorization_url),
            ]
        );
    }
}
