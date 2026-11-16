//! The installer window drives the same operations as the terminal frontend.
//! 安装器窗口驱动与终端界面相同的操作。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use iced::{Subscription, Task, window};
use vulcan_i18n::{SETUP, Translator};
use vulcan_install_core::fetch::Route;
use vulcan_install_core::names::{Channel, Platform};

use crate::args::{Command, InstallOptions, UninstallOptions};
use crate::report::{Change, Decision, Outcome, Privileged, Reporter, Review, Stage};
use crate::screen::{filled, text as label};
use crate::{install, selfupdate, uninstall};

mod view;

/// An installer's route as shown in the localized selection list.
/// 本地化选择列表中显示的安装器下载线路。
#[derive(Clone, Debug, PartialEq, Eq)]
struct RouteChoice(Route, String);

impl std::fmt::Display for RouteChoice {
    /// Writes the translated route label to the formatter.
    /// 将已翻译的线路标签写入格式化器。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.1)
    }
}

/// Choices editable from the setup window.
/// 安装窗口中可编辑的选择。
#[derive(Clone, Copy, Debug)]
enum OptionField {
    /// Install the desktop application.
    /// 安装桌面应用。
    Desktop,
    /// Add the Start menu shortcut.
    /// 添加开始菜单快捷方式。
    Menu,
    /// Add the desktop shortcut.
    /// 添加桌面快捷方式。
    Shortcut,
    /// Add commands to the user's PATH.
    /// 将命令添加到当前用户的 PATH。
    Path,
    /// Run the service at logon.
    /// 登录时启动服务。
    Service,
    /// Select the preview channel.
    /// 选择预览通道。
    Preview,
    /// Select the managed browser component.
    /// 选择受管理浏览器组件。
    Browser,
    /// Remove settings, sessions and memory after explicit confirmation.
    /// 经明确确认后移除设置、会话及记忆。
    Purge,
}

/// User input and periodic receipt of installer events.
/// 用户输入与定期接收安装器事件。
#[derive(Clone, Debug)]
enum Message {
    /// Changes the installation directory.
    /// 修改安装目录。
    Directory(String),
    /// Opens the native directory chooser.
    /// 打开系统目录选择器。
    Browse,
    /// Applies the path chosen by the native directory chooser.
    /// 应用系统目录选择器返回的路径。
    Browsed(Result<Option<PathBuf>, String>),
    /// Changes the download source.
    /// 修改下载源。
    Route(RouteChoice),
    /// Applies the initial HTTPS latency probe result.
    /// 应用安装首页的 HTTPS 响应测速结果。
    RouteMeasured(Route),
    /// Changes an install option.
    /// 修改安装选项。
    Option(OptionField, bool),
    /// Starts the selected operation.
    /// 开始所选操作。
    Start(Operation),
    /// Opens the removal options for the selected installation.
    /// 打开所选安装的卸载选项。
    SelectRemoval,
    /// Returns from the removal options to the maintenance page.
    /// 从卸载选项返回维护页面。
    LeaveRemoval,
    /// Toggles editing the selected installation's existing options.
    /// 切换编辑所选安装的现有选项。
    EditInstallation,
    /// Edits the service port range before installing.
    /// 安装前编辑服务端口范围。
    PortRange(String),
    /// Toggles the optional service settings page.
    /// 切换可选服务设置页面。
    Advanced,
    /// Answers the active review.
    /// 回答当前审阅。
    Answer(bool),
    /// Confirms terminating the processes shown on the current page.
    /// 确认强制结束当前页面列出的进程。
    ForceClose,
    /// Polls background work.
    /// 轮询后台工作。
    Poll,
    /// Moves the window from a pointer press not handled by a control.
    /// 在控件未处理指针按下时移动窗口。
    DragWindow,
    /// Minimizes the installer to the taskbar.
    /// 将安装器最小化到任务栏。
    MinimizeWindow,
    /// Closes the window when safe.
    /// 在安全时关闭窗口。
    Close,
    /// Returns to the installation page.
    /// 返回安装页面。
    Home,
    /// Starts the desktop application installed by the last operation.
    /// 启动上一次操作安装的桌面应用。
    Launch,
}

/// The requested installation operation.
/// 请求执行的安装操作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    /// Install or change the installation choices.
    /// 安装或修改安装选项。
    Install,
    /// Update an installed copy.
    /// 更新现有安装。
    Update,
    /// Remove an installed copy.
    /// 移除现有安装。
    Uninstall,
}

/// Events produced by the single installation worker.
/// 由单个安装后台线程产生的事件。
enum Event {
    /// A preparation or installation phase began.
    /// 开始准备或安装阶段。
    Stage(Stage),
    /// The verified download byte counts.
    /// 已校验下载的字节计数。
    Progress(u64, u64),
    /// A cancellation token for pre-commit downloads.
    /// 提交前下载的取消标志。
    Cancel(Arc<AtomicBool>),
    /// The real installation plan and its answer channel.
    /// 真实安装计划及答复通道。
    Review(Box<Review>, Sender<Decision>),
    /// Removal confirmation and its answer channel.
    /// 卸载确认及答复通道。
    Remove(bool, Sender<bool>),
    /// Process list, whether force is offered, and the decision channel.
    /// 进程列表、是否提供强制关闭，以及答复通道。
    WaitForExit(Vec<String>, bool, Sender<ExitDecision>),
    /// Cleanup warnings returned by the shared uninstall engine.
    /// 共用卸载引擎返回的清理警告。
    Warnings(Vec<String>),
    /// The completed operation and result or child exit code.
    /// 已完成操作及结果或子进程退出码。
    Finished(Result<Option<Box<Outcome>>, String>, Option<i32>),
}

/// Bridges the shared installer's reports to the window.
/// 将共享安装器的进度报告传递给窗口。
struct GuiReporter(Sender<Event>);

/// The decision to recheck, force close the listed processes, or cancel.
/// 重新检查、强制关闭已列出的进程或取消操作的决定。
enum ExitDecision {
    /// Scan again after the user closed the programs normally.
    /// 用户正常退出程序后重新扫描。
    Retry,
    /// Terminate the processes specifically shown for approval.
    /// 强制关闭已明确列出并获准的进程。
    Force,
    /// Cancel before changing the installation.
    /// 在变更安装之前取消。
    Cancel,
}

impl Reporter for GuiReporter {
    /// Waits for verified exit, requesting separate consent before any force stop.
    /// 等待并验证进程退出，任何强制停止都须单独确认。
    fn before_change(&self, root: &std::path::Path, _change: Change) -> Result<(), String> {
        let mut present = crate::processes::scan(root)?;
        let mut offer_force = false;
        while !present.is_empty() {
            let names = present
                .iter()
                .map(|process| format!("{} · PID {}", process.program.display(), process.pid))
                .collect();
            let (send, receive) = mpsc::channel();
            self.0
                .send(Event::WaitForExit(names, offer_force, send))
                .map_err(|error| error.to_string())?;
            present = match receive.recv().map_err(|error| error.to_string())? {
                ExitDecision::Retry => crate::processes::scan(root)?,
                ExitDecision::Force => crate::processes::force(root, &present)?,
                ExitDecision::Cancel => return Err(String::from(crate::CANCELLED)),
            };
            offer_force = true;
        }
        Ok(())
    }

    /// Publishes a new stage to the window.
    /// 向窗口发布新阶段。
    fn stage(&self, stage: Stage) {
        let _ = self.0.send(Event::Stage(stage));
    }

    /// Publishes the transfer progress to the window.
    /// 向窗口发布下载进度。
    fn progress(&self, received: u64, total: u64) {
        let _ = self.0.send(Event::Progress(received, total));
    }

    /// Shares a cancellable transfer's flag.
    /// 共享可取消下载的标志。
    fn cancel_with(&self, flag: Arc<AtomicBool>) {
        let _ = self.0.send(Event::Cancel(flag));
    }

    /// Waits on the worker for the window to approve or decline the exact plan.
    /// 在工作线程等待窗口批准或拒绝确切计划。
    fn review(&self, review: &Review) -> Result<Decision, String> {
        let (send, receive) = mpsc::channel();
        self.0
            .send(Event::Review(Box::new(review.clone()), send))
            .map_err(|error| error.to_string())?;
        receive.recv().map_err(|error| error.to_string())
    }

    /// Uses the Linux desktop authentication agent for the engine's AppArmor requests.
    /// 通过 Linux 桌面鉴权代理处理安装引擎的 AppArmor 请求。
    fn privileged(&self, request: &Privileged) -> bool {
        #[cfg(target_os = "linux")]
        return graphical_privilege(request);
        #[cfg(not(target_os = "linux"))]
        {
            let _ = request;
            false
        }
    }
}

/// Runs the supported AppArmor operations with desktop authentication and no terminal agent.
/// 通过桌面鉴权执行受支持的 AppArmor 操作，关闭终端代理。
#[cfg(target_os = "linux")]
fn graphical_privilege(request: &Privileged) -> bool {
    request.commands.iter().all(|command| {
        // Pin system executable paths before asking for elevation.
        // 请求提权前固定系统可执行文件的路径。
        let Some((program, args)) = command.split_first() else {
            return false;
        };
        let executable = match program.as_str() {
            "install" => "/usr/bin/install",
            "apparmor_parser" => "/sbin/apparmor_parser",
            "rm" => "/usr/bin/rm",
            _ => return false,
        };
        let mut process = if crate::integrate::running_as_root() {
            std::process::Command::new(executable)
        } else {
            let mut process = std::process::Command::new("/usr/bin/pkexec");
            process.args(["--disable-internal-agent", executable]);
            process
        };
        process
            .args(args)
            .stdin(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

/// A pending confirmation with the typed reply its worker expects.
/// 待确认操作及工作线程所需的有类型回复。
enum Confirmation {
    /// Exact install review and its response.
    /// 确切安装审阅及其答复。
    Install(Box<Review>, Sender<Decision>),
    /// Removal consent, including separate consent for purging user data.
    /// 卸载同意，包括单独确认清除用户数据。
    Remove(bool, Sender<bool>),
    /// The process list and an outstanding request to retry or force close.
    /// 进程列表及待回答的重新检查或强制关闭请求。
    WaitForExit(Vec<String>, bool, Sender<ExitDecision>),
}

/// The window state and its one running operation.
/// 窗口状态及唯一运行中的操作。
struct Setup {
    /// Localized setup text.
    /// 本地化安装器文案。
    language: Translator,
    /// The current platform used by the installation engine.
    /// 安装引擎使用的当前系统平台。
    platform: Platform,
    /// The original Unix console executable in the portable installer.
    /// 便携安装器内嵌的原始 Unix 终端可执行文件。
    console_image: Option<&'static [u8]>,
    /// The command's prefilled installation options.
    /// 来自命令的预填安装选项。
    options: InstallOptions,
    /// The original options saved while editing an installed copy.
    /// 编辑现有安装期间保留的原始选项。
    before_edit: Option<InstallOptions>,
    /// Editable installation directory.
    /// 可编辑的安装目录。
    directory: String,
    /// Requested removal data policy.
    /// 请求的卸载数据策略。
    purge: bool,
    /// The requested initial action, when the caller named one.
    /// 调用方指定的初始操作。
    initial: Option<Operation>,
    /// Whether the displayed directory is a recognized existing installation.
    /// 显示的目录是否为已识别的现有安装。
    installed: bool,
    /// The installed or edited download route displayed by the selector.
    /// 选择器显示的已安装或已编辑下载线路。
    route: Route,
    /// The latest measured default, absent until the probe has been scheduled.
    /// 测得的默认线路，尚未安排测速时为空。
    route_recommendation: Option<Route>,
    /// Whether the install is still registered to Inno.
    /// 安装是否仍由 Inno 登记。
    legacy: bool,
    /// The installed product version, when a current pointer exists.
    /// 存在当前版本指针时的已安装产品版本。
    installed_version: Option<String>,
    /// The reusable product logo decoded from the shipped artwork.
    /// 从随包图稿解码并复用的产品标志。
    brand: Option<iced::widget::image::Handle>,
    /// The current operation, retained across its review and progress screens.
    /// 当前操作，在审阅和进度页面间保留。
    operation: Option<Operation>,
    /// Whether declining the current review returns to the editable page.
    /// 拒绝当前审阅时是否返回编辑页面。
    returning_to_options: bool,
    /// Whether the user approved the current operation's first confirmation.
    /// 用户是否已通过当前操作的首次确认。
    approved: bool,
    /// Whether the optional service settings page is open.
    /// 是否正在显示可选服务设置页。
    advanced: bool,
    /// Whether data deletion was declined separately from removing the application.
    /// 是否在卸载应用外单独拒绝删除用户数据。
    kept_data: bool,
    /// The worker's event stream.
    /// 后台线程的事件流。
    worker: Option<Receiver<Event>>,
    /// The last reported operation phase.
    /// 最近报告的操作阶段。
    stage: Option<Stage>,
    /// The current byte count and total.
    /// 当前字节数及总量。
    progress: Option<(u64, u64)>,
    /// The flag for the currently cancellable download.
    /// 当前可取消下载的标志。
    cancel: Option<Arc<AtomicBool>>,
    /// The current prompt awaiting an answer.
    /// 正在等待答复的提示。
    confirmation: Option<Confirmation>,
    /// The last operation's completion text or error.
    /// 最近操作的结果或错误。
    result: Option<Result<Option<Box<Outcome>>, String>>,
    /// Diagnostics that accompany a successful operation or a folder picker error.
    /// 随成功操作显示的诊断信息，或目录选择错误。
    warnings: Vec<String>,
    /// Exit code copied back to the desktop or terminal caller.
    /// 返回给桌面或终端调用方的退出码。
    code: Arc<std::sync::atomic::AtomicI32>,
}

/// Fits the compact default and resize limit to the primary monitor's usable desktop.
/// 根据主显示器可用工作区调整紧凑默认尺寸及缩放下限。
fn window_bounds(
    work_area: Option<(iced::Point, iced::Size)>,
) -> (iced::Size, iced::Size, window::Position) {
    // Default logical dimensions leave room for the taskbar even on 768-pixel displays.
    // 默认逻辑尺寸在高 768 像素的屏幕上仍为任务栏保留空间。
    let preferred = iced::Size::new(800.0, 560.0);
    // The scrollable body keeps the title and actions visible at the resize limit.
    // 可滚动内容区在缩放下限时保持标题及操作按钮可见。
    let minimum = iced::Size::new(620.0, 390.0);
    let Some((origin, available)) = work_area else {
        return (preferred, minimum, window::Position::Centered);
    };
    // Preserve a small margin between the frame and the work-area boundary.
    // 在窗口边缘与系统工作区边界间保留少量空隙。
    let margin = 12.0;
    let fitted = iced::Size::new(
        preferred
            .width
            .min((available.width - margin * 2.0).max(1.0)),
        preferred
            .height
            .min((available.height - margin * 2.0).max(1.0)),
    );
    let limit = iced::Size::new(
        minimum.width.min(fitted.width),
        minimum.height.min(fitted.height),
    );
    let position = window::Position::Specific(iced::Point::new(
        origin.x + (available.width - fitted.width) / 2.0,
        origin.y + (available.height - fitted.height) / 2.0,
    ));
    (fitted, limit, position)
}

/// Opens the Iced window with the parsed command, returning the operation's exit code.
/// 按已解析命令打开 Iced 窗口，并返回操作退出码。
pub fn run(command: Command, console_image: Option<&'static [u8]>) -> i32 {
    let Some(platform) = Platform::current() else {
        startup_error("this platform is not supported");
        return 1;
    };
    // Preserve the same exit status as the unattended and terminal interfaces.
    // 保留与无人值守及终端界面一致的退出码。
    let code = Arc::new(std::sync::atomic::AtomicI32::new(
        crate::CANCELLED_EXIT_CODE,
    ));
    let state_code = Arc::clone(&code);
    // Decode the same brand mark used by the WebUI once for both surfaces.
    // WebUI 的同款品牌标志只解码一次，供两处界面使用。
    let pixels = view::brand_pixels();
    // Size and center the window in usable desktop coordinates.
    // 按可用桌面坐标计算窗口尺寸和居中位置。
    #[cfg(windows)]
    let work_area = crate::windows::setup_work_area();
    #[cfg(unix)]
    let work_area = None;
    let (size, minimum, position) = window_bounds(work_area);
    // Explorer and the native window show the same application mark.
    // 资源管理器和原生窗口显示同一应用标志。
    let settings = window::Settings {
        size,
        min_size: Some(minimum),
        position,
        decorations: false,
        icon: pixels.as_ref().and_then(|image| {
            let icon =
                image::imageops::resize(image, 48, 48, image::imageops::FilterType::Triangle);
            window::icon::from_rgba(icon.into_raw(), 48, 48).ok()
        }),
        exit_on_close_request: false,
        ..window::Settings::default()
    };
    // The renderer reuses one cached image handle for the lifetime of the window.
    // 渲染器在窗口生命周期内复用同一缓存图像句柄。
    let brand = pixels.map(|image| {
        iced::widget::image::Handle::from_rgba(image.width(), image.height(), image.into_raw())
    });
    let result = iced::application(
        move || {
            let mut setup = Setup::new(
                command.clone(),
                Arc::clone(&state_code),
                brand.clone(),
                platform,
                console_image,
            );
            let probe = setup.probe_route();
            (setup, probe)
        },
        Setup::update,
        Setup::view,
    )
    .title(|setup: &Setup| String::from(setup.text("setup.title")))
    .theme(|_: &Setup| view::theme())
    .default_font(view::BODY_FONT)
    .window(settings)
    .subscription(Setup::subscription)
    .run();
    if let Err(error) = result {
        startup_error(&error.to_string());
        return 1;
    }
    code.load(Ordering::Relaxed)
}

/// Reports an error when the graphical window cannot be created.
/// 无法创建图形窗口时报告错误。
fn startup_error(error: &str) {
    #[cfg(windows)]
    crate::windows::setup_error(error);
    #[cfg(unix)]
    {
        eprintln!("{error}");
        let _ = rfd::MessageDialog::new()
            .set_title(label(&Translator::new(&[&SETUP]), "setup.title"))
            .set_level(rfd::MessageLevel::Error)
            .set_description(error)
            .show();
    }
}

impl Setup {
    /// Builds the initial window from the parsed command and the recorded install.
    /// 依据已解析命令及安装记录构建初始窗口。
    fn new(
        command: Command,
        code: Arc<std::sync::atomic::AtomicI32>,
        brand: Option<iced::widget::image::Handle>,
        platform: Platform,
        console_image: Option<&'static [u8]>,
    ) -> Self {
        let mut language = Translator::new(&[&SETUP]);
        language.set_locale(vulcan_i18n::system_locale());
        let (options, initial, purge) = match command {
            Command::Install(options) => (options, None, false),
            Command::Update(common) => (
                InstallOptions {
                    common,
                    ..InstallOptions::default()
                },
                Some(Operation::Update),
                false,
            ),
            Command::Uninstall(options) => (
                InstallOptions {
                    common: options.common,
                    ..InstallOptions::default()
                },
                Some(Operation::Uninstall),
                options.purge,
            ),
            _ => (InstallOptions::default(), None, false),
        };
        let root = install::resolve_root(options.common.prefix.as_deref(), platform);
        let (directory, result) = match root {
            Ok(path) => (path.display().to_string(), None),
            Err(error) => (String::new(), Some(Err(error))),
        };
        // Show directly supplied advanced options so the user can review them.
        // 展开显式传入的高级选项，方便用户审阅。
        let advanced = options.port_range.is_some();
        let mut setup = Self {
            language,
            platform,
            console_image,
            options,
            before_edit: None,
            directory,
            purge,
            initial,
            installed: false,
            route: Route::Auto,
            route_recommendation: None,
            legacy: false,
            installed_version: None,
            brand,
            operation: None,
            returning_to_options: false,
            approved: false,
            advanced,
            kept_data: true,
            worker: None,
            stage: None,
            progress: None,
            cancel: None,
            confirmation: None,
            result,
            warnings: Vec::new(),
            code,
        };
        setup.inspect();
        setup
    }

    /// Inspects the entered location so the window can label the available actions.
    /// 检查输入的安装目录，使窗口能够标明可执行操作。
    fn inspect(&mut self) {
        use crate::root::{Current, Ownership};
        // An empty text input is not a request to inspect the process working directory.
        // 路径输入留空时不检查进程工作目录。
        let directory = self.directory.trim();
        if directory.is_empty() {
            self.installed = false;
            self.legacy = false;
            self.installed_version = None;
            self.route = self
                .options
                .common
                .route
                .or(self.route_recommendation)
                .unwrap_or_default();
            return;
        }
        let root = PathBuf::from(directory);
        let ownership = crate::root::ownership(&root);
        self.legacy = self.platform.is_windows() && matches!(ownership, Ok(Ownership::Inno(_)));
        self.installed =
            self.legacy || matches!(ownership, Ok(Ownership::Owned(_) | Ownership::Legacy));
        self.installed_version = if self.installed {
            match crate::root::current(&vulcan_install_core::install_root::InstallRoot::new(&root))
            {
                Ok(Current::Version(version)) => Some(version.to_string()),
                _ => None,
            }
        } else {
            None
        };
        self.route = self.options.common.route.unwrap_or(match ownership {
            Ok(Ownership::Owned(state)) => state.route,
            _ if !self.installed => self.route_recommendation.unwrap_or_default(),
            _ => Route::Auto,
        });
    }

    /// Starts one background latency probe when the current route is automatic.
    /// 当前使用自动线路时，启动一次后台延迟探测。
    fn probe_route(&mut self) -> Task<Message> {
        if self.route != Route::Auto
            || self.route_recommendation.is_some()
            || self.initial == Some(Operation::Uninstall)
        {
            return Task::none();
        }
        // Mark the probe as started so edits to the directory field cannot start more probes.
        // 标记已启动测速，防止编辑目录输入时重复发起探测。
        self.route_recommendation = Some(Route::Auto);
        Task::perform(
            async { crate::route::recommendation() },
            Message::RouteMeasured,
        )
    }

    /// Looks up a label in the shared setup catalog.
    /// 从共享安装器文案目录查找标签。
    fn text<'a>(&'a self, key: &'a str) -> &'a str {
        label(&self.language, key)
    }

    /// Polls the worker while active and handles the operating system's close button.
    /// 运行期间轮询后台线程，同时处理系统关闭按钮。
    fn subscription(&self) -> Subscription<Message> {
        let close = window::close_requests().map(|_| Message::Close);
        if self.worker.is_some() {
            Subscription::batch([
                close,
                iced::time::every(Duration::from_millis(80)).map(|_| Message::Poll),
            ])
        } else {
            close
        }
    }

    /// Applies an input or worker notification, returning an optional close task.
    /// 应用输入或工作通知，返回可选的关闭任务。
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Directory(value) => {
                self.directory = value;
                self.inspect();
                return self.probe_route();
            }
            Message::Browse => {
                // The native dialog is modal to the same HWND as the installer.
                // 系统对话框对安装器自身的 HWND 保持模态。
                let directory = PathBuf::from(self.directory.trim());
                #[cfg(unix)]
                return Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_directory(directory)
                            .pick_folder()
                            .await
                            .map(|folder| folder.path().to_path_buf())
                    },
                    |directory| Message::Browsed(Ok(directory)),
                );
                #[cfg(windows)]
                return window::latest()
                    .and_then(move |id| {
                        // The task can be evaluated more than once, while the dialog owns one path.
                        // 任务可求值多次，每个对话框持有独立路径。
                        let directory = directory.clone();
                        window::run(id, move |owner| {
                            use window::raw_window_handle::RawWindowHandle;
                            // Extract the window handle on the same event thread as the modal dialog.
                            // 在运行模态对话框的同一事件线程上读取窗口句柄。
                            let handle =
                                owner.window_handle().map_err(|error| error.to_string())?;
                            let RawWindowHandle::Win32(handle) = handle.as_raw() else {
                                return Err(String::from(
                                    "the installer window has no Win32 handle",
                                ));
                            };
                            crate::windows::select_directory(handle.hwnd, &directory)
                                .map_err(|error| error.to_string())
                        })
                    })
                    .map(Message::Browsed);
            }
            Message::Browsed(Ok(Some(directory))) => {
                self.directory = directory.display().to_string();
                self.inspect();
                return self.probe_route();
            }
            Message::Browsed(Err(error)) => self.warnings.push(error),
            Message::Browsed(Ok(None)) => {}
            Message::Route(value) => {
                self.route = value.0;
                self.options.common.route = Some(value.0);
                return self.probe_route();
            }
            Message::RouteMeasured(route) => {
                self.route_recommendation = Some(route);
                // The result may arrive after the user picked a route or an existing installation.
                // 测速结果可能在用户改选线路或选择现有安装后才送达。
                if !self.installed && self.options.common.route.is_none() {
                    self.route = route;
                }
            }
            Message::PortRange(value) => {
                self.options.port_range = Some(value).filter(|range| !range.trim().is_empty());
            }
            Message::Advanced => self.advanced = !self.advanced,
            Message::Option(field, value) => self.set_option(field, value),
            Message::Start(operation) => self.start(operation),
            Message::SelectRemoval => {
                self.initial = Some(Operation::Uninstall);
                self.purge = false;
            }
            Message::LeaveRemoval => {
                self.initial = None;
                self.purge = false;
                return self.probe_route();
            }
            Message::EditInstallation => self.edit_installation(),
            Message::Answer(yes) => match self.confirmation.take() {
                Some(Confirmation::Install(_, send)) => {
                    self.returning_to_options = !yes;
                    self.approved = yes;
                    let _ = send.send(if yes {
                        Decision::Continue
                    } else {
                        Decision::Cancel
                    });
                }
                Some(Confirmation::Remove(purge, send)) => {
                    self.returning_to_options = !yes && !purge;
                    self.approved = yes || purge;
                    if purge {
                        self.kept_data = !yes;
                    }
                    let _ = send.send(yes);
                }
                Some(Confirmation::WaitForExit(_, _, send)) => {
                    self.returning_to_options = !yes;
                    let _ = send.send(if yes {
                        ExitDecision::Retry
                    } else {
                        ExitDecision::Cancel
                    });
                }
                None => (),
            },
            Message::ForceClose => {
                if matches!(
                    self.confirmation,
                    Some(Confirmation::WaitForExit(_, true, _))
                ) && let Some(Confirmation::WaitForExit(_, _, send)) = self.confirmation.take()
                {
                    let _ = send.send(ExitDecision::Force);
                }
            }
            Message::Poll => {
                while let Some(worker) = &self.worker {
                    match worker.try_recv() {
                        Ok(event) => {
                            if self.receive(event) {
                                return window::latest().and_then(window::close);
                            }
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => {
                            self.receive(Event::Finished(
                                Err(String::from("the installation worker stopped unexpectedly")),
                                None,
                            ));
                        }
                    }
                }
            }
            Message::DragWindow => {
                // Start the native move on the window thread before the pointer is released.
                // 在指针松开前，由窗口线程启动系统拖动。
                #[cfg(unix)]
                return window::latest().and_then(window::drag);
                #[cfg(windows)]
                return window::latest().and_then(|id| {
                    window::run(id, |owner| {
                        use window::raw_window_handle::RawWindowHandle;
                        if let Ok(handle) = owner.window_handle()
                            && let RawWindowHandle::Win32(handle) = handle.as_raw()
                        {
                            crate::windows::drag_setup(handle.hwnd);
                        }
                    })
                    .discard()
                });
            }
            Message::MinimizeWindow => {
                return window::latest().and_then(|id| window::minimize(id, true));
            }
            Message::Close => {
                if self.worker.is_none() {
                    return window::latest().and_then(window::close);
                }
                if self.confirmation.is_some() {
                    return self.update(Message::Answer(false));
                }
                if self.stage.is_some_and(Stage::cancellable)
                    && let Some(cancel) = &self.cancel
                {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            Message::Home => {
                self.result = None;
                self.warnings.clear();
                self.inspect();
                return self.probe_route();
            }
            Message::Launch => {
                if let Some(Ok(Some(outcome))) = &self.result
                    && outcome.desktop.is_some()
                {
                    if let Err(error) = crate::launcher::launch_desktop(&outcome.directory) {
                        self.warnings.push(error);
                    } else {
                        return window::latest().and_then(window::close);
                    }
                }
            }
        }
        Task::none()
    }

    /// Stores the selected install option without changing the user's installation yet.
    /// 保存所选安装选项，暂不更改用户的安装。
    fn set_option(&mut self, field: OptionField, value: bool) {
        match field {
            OptionField::Desktop => self.options.gui = Some(value),
            OptionField::Menu => self.options.menu = Some(value),
            OptionField::Shortcut => self.options.desktop_shortcut = Some(value),
            OptionField::Path => self.options.path = Some(value),
            OptionField::Service => {
                self.options.service = Some(value);
                if !value {
                    self.options.linger = Some(false);
                }
            }
            OptionField::Purge => self.purge = value,
            OptionField::Preview => {
                self.options.channel = Some(if value {
                    Channel::Preview
                } else {
                    Channel::Stable
                })
            }
            OptionField::Browser => {
                if let Ok(browser) =
                    vulcan_install_core::names::ComponentName::new(crate::args::BROWSER_COMPONENT)
                {
                    self.options.with.retain(|name| name != &browser);
                    if value {
                        self.options.with.push(browser);
                    }
                }
            }
        }
    }

    /// Loads the actual recorded choices into the editor, or restores the management view.
    /// 把实际记录的选项载入编辑器，或恢复管理界面。
    fn edit_installation(&mut self) {
        if let Some(original) = self.before_edit.take() {
            self.options = original;
            self.advanced = false;
            self.inspect();
            return;
        }
        let root = PathBuf::from(self.directory.trim());
        let choices = crate::root::installable(&root)
            .and_then(|owner| install::recorded_choices(&root, &owner))
            .and_then(|state| {
                install::Choices::resolve(&self.options, state.as_ref(), self.platform, true)
            });
        match choices {
            Ok(choices) => {
                self.before_edit = Some(self.options.clone());
                self.options.gui = Some(choices.gui);
                self.options.menu = Some(choices.menu);
                self.options.desktop_shortcut = Some(choices.desktop_shortcut);
                self.options.path = Some(choices.path);
                self.options.service = Some(choices.service);
                self.options.linger = Some(choices.linger);
                self.options.channel = Some(choices.channel);
                self.options.with = choices.with;
            }
            Err(error) => self.warnings.push(error),
        }
    }

    /// Starts the shared operation on a worker so rendering and confirmations stay responsive.
    /// 在后台线程启动共享操作，使渲染与确认保持响应。
    fn start(&mut self, operation: Operation) {
        if self.worker.is_some() || self.directory.trim().is_empty() {
            return;
        }
        let mut options = self.options.clone();
        options.common.prefix = Some(PathBuf::from(self.directory.trim()));
        options.common.route = Some(self.route);
        // Match the terminal wizard's whitespace normalization before validating a port range.
        // 校验端口范围前，采用与终端向导一致的空白字符处理方式。
        options.port_range = options.port_range.map(|value| value.trim().to_owned());
        let purge = self.purge;
        let console_image = self.console_image;
        let (sender, receiver) = mpsc::channel();
        self.worker = Some(receiver);
        self.stage = None;
        self.progress = None;
        self.cancel = None;
        self.result = None;
        self.warnings.clear();
        self.operation = Some(operation);
        self.returning_to_options = false;
        self.approved = false;
        self.kept_data = true;
        // Keep the sender alive until the operation has returned or unwound.
        // 在操作正常返回或异常展开之前保留发送端。
        let worker = std::thread::Builder::new()
            .name(String::from("vulcan-setup"))
            .spawn(move || {
                let completion = worker_completion(operation, || {
                    run_worker(operation, options, purge, console_image, &sender)
                });
                let _ = sender.send(completion);
            });
        if let Err(error) = worker {
            self.receive(Event::Finished(
                Err(format!("cannot start the installation worker: {error}")),
                None,
            ));
        }
    }

    /// Updates window fields from a report and returns whether a restarted child finished.
    /// 根据报告更新窗口字段，并返回重启的子进程是否已经结束。
    fn receive(&mut self, event: Event) -> bool {
        match event {
            Event::Stage(stage) => self.stage = Some(stage),
            Event::Progress(done, total) => self.progress = Some((done, total)),
            Event::Cancel(flag) => self.cancel = Some(flag),
            Event::Review(review, send) => {
                self.confirmation = Some(Confirmation::Install(review, send))
            }
            Event::Remove(purge, send) => {
                self.confirmation = Some(Confirmation::Remove(purge, send))
            }
            Event::WaitForExit(names, offer_force, send) => {
                self.confirmation = Some(Confirmation::WaitForExit(names, offer_force, send))
            }
            Event::Warnings(warnings) => self.warnings.extend(warnings),
            Event::Finished(result, code) => {
                self.worker = None;
                self.confirmation = None;
                self.cancel = None;
                self.code.store(
                    code.unwrap_or_else(|| match &result {
                        Ok(_) => 0,
                        Err(error) if error == crate::CANCELLED => crate::CANCELLED_EXIT_CODE,
                        Err(_) => 1,
                    }),
                    Ordering::Relaxed,
                );
                if self.returning_to_options
                    && matches!(&result, Err(error) if error == crate::CANCELLED)
                {
                    self.operation = None;
                } else {
                    self.result = Some(result);
                }
                self.returning_to_options = false;
                return code.is_some();
            }
        }
        false
    }

    /// Names the current worker stage using the setup catalog.
    /// 使用安装器文案目录命名当前工作阶段。
    fn stage_text(&self) -> String {
        if let Some(Stage::Checking(channel)) = self.stage {
            return filled(
                &self.language,
                "setup.stage.checking",
                &[("channel", channel.as_str())],
            );
        }
        String::from(self.text(match self.stage {
            Some(Stage::Preparing) => "setup.stage.preparing",
            Some(Stage::Downloading) => "setup.stage.downloading",
            Some(Stage::Switching) => "setup.stage.switching",
            Some(Stage::Integrating) => "setup.stage.integrating",
            Some(Stage::Service) => "setup.stage.service",
            Some(Stage::Cleaning) => "setup.stage.cleaning",
            _ => "setup.working.title",
        }))
    }
}

/// Returns the completion of `work` or an error with the operation and panic message.
/// 返回 `work` 的完成事件，或包含操作类型和异常消息的错误。
fn worker_completion(operation: Operation, work: impl FnOnce() -> Event) -> Event {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
        Ok(event) => event,
        Err(panic) => {
            // String and str are the standard message types produced by Rust's panic macros.
            // String 和 str 是 Rust 异常宏产生的标准消息类型。
            let message = match panic.downcast::<String>() {
                Ok(message) => *message,
                Err(panic) => match panic.downcast::<&'static str>() {
                    Ok(message) => String::from(*message),
                    Err(_) => String::from("no panic message available"),
                },
            };
            Event::Finished(
                Err(format!("the {operation:?} worker panicked: {message}")),
                None,
            )
        }
    }
}

/// Runs `operation` with `options` and the optional bundled `console_image`.
/// 使用 `options` 和可选的内嵌终端镜像 `console_image` 执行 `operation`。
/// Relays prompts via `sender`, applies `purge` during removal, and returns the completion event.
/// 通过 `sender` 转发交互请求，卸载时应用 `purge`，并返回完成事件。
fn run_worker(
    operation: Operation,
    #[cfg_attr(unix, allow(unused_mut))] mut options: InstallOptions,
    purge: bool,
    console_image: Option<&'static [u8]>,
    sender: &Sender<Event>,
) -> Event {
    let reporter = GuiReporter(sender.clone());
    let result = if operation == Operation::Uninstall {
        let options = UninstallOptions {
            common: options.common,
            purge,
        };
        uninstall::remove(&options, &reporter, |approval| {
            let (answer, receive) = mpsc::channel();
            sender
                .send(Event::Remove(
                    approval == uninstall::Confirmation::Purge,
                    answer,
                ))
                .map_err(|error| error.to_string())?;
            receive.recv().map_err(|error| error.to_string())
        })
        .map(|warnings| {
            let _ = sender.send(Event::Warnings(warnings));
            None
        })
    } else {
        // The Unix portable launcher is updated by replacing its platform package.
        // Unix 便携启动器通过替换对应平台包进行更新。
        #[cfg(unix)]
        let preparation: Result<selfupdate::Outcome, String> = Ok(selfupdate::Outcome::Continue);
        #[cfg(windows)]
        let preparation = prepare_gui(&mut options, operation);
        match preparation {
            Ok(selfupdate::Outcome::Continue) => install::graphical_run(
                &options,
                operation == Operation::Update,
                &reporter,
                console_image,
            )
            .map(|outcome| Some(Box::new(outcome))),
            Ok(selfupdate::Outcome::Finished(code)) => {
                return Event::Finished(Ok(None), Some(code));
            }
            Err(error) => Err(error),
        }
    };
    Event::Finished(result, None)
}

/// Uses the shared self-update path and ensures a replacement process reopens the GUI.
/// 使用共享自更新流程，并确保替换进程重新打开 GUI。
#[cfg(windows)]
fn prepare_gui(
    options: &mut InstallOptions,
    operation: Operation,
) -> Result<selfupdate::Outcome, String> {
    let platform = Platform::WindowsX64;
    let (channel, route) = install::preferences(&options.common, options.channel, platform);
    let route = route.unwrap_or_default();
    options.common.route = Some(route);
    let mut arguments = vec![
        String::from(if operation == Operation::Update {
            "update"
        } else {
            "install"
        }),
        String::from("--ui=gui"),
    ];
    if let Some(prefix) = &options.common.prefix {
        arguments.extend([String::from("--prefix"), prefix.display().to_string()]);
    }
    arguments.push(format!("--route={}", crate::args::route_name(route)));
    if operation == Operation::Install {
        arguments.push(format!("--channel={}", channel.as_str()));
        arguments.push(String::from(if options.gui.unwrap_or(true) {
            "--gui"
        } else {
            "--no-gui"
        }));
        for (option, name) in [
            (options.menu, "menu"),
            (options.desktop_shortcut, "desktop-shortcut"),
            (options.path, "path"),
            (options.service, "service"),
        ] {
            if let Some(on) = option {
                arguments.push(format!("--{}{name}", if on { "" } else { "no-" }));
            }
        }
        for name in &options.with {
            arguments.push(format!("--with={}", name.as_str()));
        }
        if let Some(port) = &options.port_range {
            arguments.push(format!("--port-range={port}"));
        }
    }
    selfupdate::check(
        platform,
        channel,
        route,
        options.common.self_update,
        &arguments,
        selfupdate::Surface::Graphical,
    )
}

#[cfg(test)]
mod window_tests {
    use super::window_bounds;
    use iced::{Point, Size, window::Position};

    /// A measured default selects a fresh install's route without replacing a manual choice.
    /// 测速结果可以设置首次安装的默认线路，同时保留用户手动选择。
    #[test]
    fn measured_default_preserves_a_manual_route() {
        use super::{Command, InstallOptions, Message, Platform, Route, RouteChoice, Setup};
        use std::sync::{Arc, atomic::AtomicI32};

        let fixture = tempfile::tempdir().expect("isolated install directory");
        let mut options = InstallOptions::default();
        options.common.prefix = Some(fixture.path().join("fresh"));
        let mut setup = Setup::new(
            Command::Install(options),
            Arc::new(AtomicI32::new(0)),
            None,
            Platform::current().expect("supported installer platform"),
            None,
        );
        let _ = setup.update(Message::RouteMeasured(Route::China));
        assert_eq!(setup.route, Route::China);
        let _ = setup.update(Message::Route(RouteChoice(
            Route::International,
            String::new(),
        )));
        let _ = setup.update(Message::RouteMeasured(Route::China));
        assert_eq!(setup.route, Route::International);
        assert_eq!(setup.options.common.route, Some(Route::International));
    }

    /// Verifies a worker panic reaches the window through an error event with its cause.
    /// 验证线程异常通过带有具体原因的错误事件到达窗口。
    #[test]
    fn worker_panic_produces_an_error_event() {
        use super::{Event, Operation, worker_completion};

        let event = worker_completion(Operation::Install, || panic!("test worker failure"));
        assert!(matches!(event, Event::Finished(Err(error), None)
            if error.contains("test worker failure")));
    }

    /// Runs the reported preview and China combination to the signed review.
    /// 用报告中的预览通道和中国大陆节点组合运行至签名审阅。
    #[cfg(windows)]
    #[test]
    #[ignore = "requires the live China release feed"]
    fn china_preview_reaches_review() {
        use super::{Command, Event, Operation, Setup};
        use std::sync::{Arc, atomic::AtomicI32};
        use std::time::Duration;
        use vulcan_install_core::names::Platform;

        let fixture = tempfile::tempdir().expect("isolated reinstall directory");
        let prefix = format!("--prefix={}", fixture.path().join("reinstall").display());
        let arguments = [
            "install",
            "--ui=gui",
            "--channel=preview",
            "--route=china",
            "--with=browser",
            "--gui",
            "--menu",
            "--desktop-shortcut",
            "--path",
            "--no-service",
            &prefix,
        ]
        .map(String::from);
        let command = crate::args::parse(&arguments).expect("reported options");
        assert!(matches!(command, Command::Install(_)));
        let mut setup = Setup::new(
            command,
            Arc::new(AtomicI32::new(0)),
            None,
            Platform::WindowsX64,
            None,
        );
        setup.start(Operation::Install);
        let worker = setup.worker.expect("started installer");
        loop {
            match worker
                .recv_timeout(Duration::from_secs(30))
                .expect("installer event")
            {
                Event::Review(review, reply) => {
                    eprintln!("Signed review for {} via the China route", review.version);
                    reply
                        .send(super::Decision::Cancel)
                        .expect("decline the reviewed install");
                    break;
                }
                Event::Finished(Err(error), _) => panic!("installer failed before review: {error}"),
                _ => (),
            }
        }
        assert!(matches!(
            worker.recv_timeout(Duration::from_secs(5)).expect("cancelled worker"),
            Event::Finished(Err(error), _) if error == crate::CANCELLED
        ));
    }

    /// Drives the production close confirmation with an actively running, disposable old install.
    /// 使用实际运行且可丢弃的旧版安装验证生产关闭确认流程。
    #[test]
    #[ignore = "requires a running legacy process in an isolated VULCAN_SETUP_RUNNING_TEST_ROOT"]
    fn running_release_requires_a_second_confirmation() {
        use super::{Change, Event, ExitDecision, GuiReporter, Reporter, mpsc};
        use std::path::PathBuf;
        use std::time::Duration;

        let root = PathBuf::from(
            std::env::var_os("VULCAN_SETUP_RUNNING_TEST_ROOT").expect("isolated installation root"),
        );
        assert!(
            !crate::processes::scan(&root)
                .expect("scan fixture")
                .is_empty()
        );
        let (publish, events) = mpsc::channel();
        let running_root = root.clone();
        let worker = std::thread::spawn(move || {
            GuiReporter(publish).before_change(&running_root, Change::Apply)
        });
        let Event::WaitForExit(first, false, answer) = events
            .recv_timeout(Duration::from_secs(15))
            .expect("manual close prompt")
        else {
            panic!("the first prompt should request normal exit");
        };
        answer.send(ExitDecision::Retry).expect("request a recheck");
        let Event::WaitForExit(again, true, answer) = events
            .recv_timeout(Duration::from_secs(15))
            .expect("separate force prompt")
        else {
            panic!("running programs should require force approval");
        };
        assert_eq!(first, again);
        assert!(
            !crate::processes::scan(&root)
                .expect("scan before approval")
                .is_empty()
        );
        answer
            .send(ExitDecision::Force)
            .expect("approve force only for the test");
        worker
            .join()
            .expect("worker exits")
            .expect("verified closure");
        assert!(
            crate::processes::scan(&root)
                .expect("scan after approval")
                .is_empty()
        );
    }

    /// Checks the compact defaults and reduced work areas for scaled 1366×768 displays.
    /// 验证紧凑默认尺寸以及 1366×768 屏幕缩放后的较小工作区。
    #[test]
    fn installer_fits_scaled_work_areas() {
        // A high-DPI display can have less usable logical space than the preferred size.
        // 高 DPI 屏幕的可用逻辑空间可能小于窗口首选尺寸。
        for scale in [1.0, 1.25, 1.5] {
            // Model a work area with the taskbar above the window.
            // 模拟任务栏位于窗口上方的工作区。
            let origin = Point::new(0.0, 48.0);
            let area = Size::new(1366.0 / scale, 768.0 / scale - origin.y);
            let (size, minimum, Position::Specific(position)) = window_bounds(Some((origin, area)))
            else {
                panic!("an available work area should provide an explicit position");
            };
            assert!(size.width <= 800.0 && size.height <= 560.0);
            assert!(minimum.width <= size.width && minimum.height <= size.height);
            assert!(position.x >= origin.x && position.y >= origin.y);
            assert!(position.x + size.width <= origin.x + area.width);
            assert!(position.y + size.height <= origin.y + area.height);
            if scale == 1.0 {
                assert_eq!(size, Size::new(800.0, 560.0));
            }
        }
    }
}
