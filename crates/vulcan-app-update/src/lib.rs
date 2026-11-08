//! In-app updates of an installed Vulcan Code (plan section 8).
//! 已安装 Vulcan Code 的应用内更新（方案第八节）。
//!
//! The service checks, downloads, verifies and assembles; clients only show the state and confirm.
//! Everything happens on worker threads, one operation at a time, and every state change is
//! reported to the [`UpdateHost`]. Updating is off outside an installed release layout, with the
//! reason in the snapshot.
//!
//! 由服务检查、下载、校验与组装；客户端只显示状态并确认。所有工作都在工作线程上进行，每次只运行一个操作，
//! 每次状态变化都报告给 [`UpdateHost`]。在已安装的发行布局之外更新功能关闭，原因写在快照中。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use vulcan_install_core::assemble::Assembly;
use vulcan_install_core::fetch::{Downloader, Route};
use vulcan_install_core::index::{ComponentIndex, IndexEntry};
use vulcan_install_core::install_root::{
    MAX_STARTUP_ATTEMPTS, ROLLBACK_FILE, Rollback, SETUP_FAILURE_FILE, SETUP_LOG_FILE,
    STARTUP_TRIAL_FILE, SetupFailure, StartupTrial, UPDATES_DIR,
};
use vulcan_install_core::layout::installer_key;
use vulcan_install_core::manifest::{DownloadKind, InnoUpdate, ReleaseManifest};
use vulcan_install_core::names::{Channel, ReleaseVersion};
use vulcan_install_core::record::{InstallRecord, RECORD_FILE_NAME};
use vulcan_install_core::repair::prepare_repair;
use vulcan_install_core::signing::BUILT_IN_KEYS;
use vulcan_install_core::update::{
    find_update, read_indexes, read_manifest, read_skipped_release_notes,
};
use vulcan_install_core::{FormatError, Sha256Digest};
use vulcan_service_api::{
    AppUpdateActivity, AppUpdateCheckInterval, AppUpdateDesktopProfile, AppUpdateError,
    AppUpdateKind, AppUpdateOffer, AppUpdateOperation, AppUpdateProgress, AppUpdateReleaseNotes,
    AppUpdateRollback, AppUpdateRouteSetting, AppUpdateSettings, AppUpdateSnapshot,
    AppUpdateUnavailableReason, ServiceRestartReason, ServiceRestartingEvent,
};

mod desktop_profile;
mod installed;
mod signer;

use desktop_profile::DesktopProfile;
use installed::Installed;

/// Delay between service start and the first automatic check.
/// 服务启动到首次自动检查之间的延迟。
pub const FIRST_CHECK_DELAY: Duration = Duration::from_secs(60);

/// How often the schedule wakes to see whether an automatic check is due, so a changed interval
/// applies without a restart.
/// 调度醒来查看自动检查是否到期的频率，使修改后的间隔无需重启即可生效。
const SCHEDULE_TICK: Duration = Duration::from_secs(60);

/// Time between automatic checks for each interval setting.
/// 每种间隔设置对应的自动检查间隔时间。
pub fn check_interval(setting: AppUpdateCheckInterval) -> Duration {
    const HOUR: u64 = 60 * 60;
    Duration::from_secs(match setting {
        AppUpdateCheckInterval::OneHour => HOUR,
        AppUpdateCheckInterval::SixHours => 6 * HOUR,
        AppUpdateCheckInterval::TwelveHours => 12 * HOUR,
        AppUpdateCheckInterval::OneDay => 24 * HOUR,
    })
}

/// How often a running download or repair reports its progress.
/// 进行中的下载或修复报告进度的频率。
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);

/// Page for downloading an installer by hand.
/// 手动下载安装包的页面。
pub const DOWNLOAD_PAGE_URL: &str = "https://openvulcan.com/download";

/// Arguments of an unattended Inno Setup run (plan section 18.3); `/RELAUNCH=1` makes the
/// installer start the app when it finishes (`RelaunchRequested` in `scripts/windows-installer.mjs`).
/// 无人值守运行 Inno Setup 的参数（方案 18.3 节）；`/RELAUNCH=1` 让安装包结束后启动应用
/// （`scripts/windows-installer.mjs` 中的 `RelaunchRequested`）。
pub const INSTALLER_ARGUMENTS: [&str; 4] =
    ["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/RELAUNCH=1"];

/// The path of `executable` that keeps working across updates: `<root>/current/<relative>` on
/// platforms whose current version is a link; `None` outside an installed layout or on Windows,
/// where the launcher is the stable entry.
/// `executable` 跨更新始终有效的路径：当前版本为链接的平台上为 `<root>/current/<relative>`；在已安装布局
/// 之外或在 Windows 上（启动器才是稳定入口）为 `None`。
pub fn stable_executable(executable: &Path) -> Option<PathBuf> {
    let installed = installed::detect(executable, vulcan_product_info::version()).ok()?;
    (!installed.platform.is_windows()).then(|| {
        installed
            .root
            .path()
            .join(vulcan_install_core::install_root::CURRENT_LINK)
            .join(&installed.executable)
    })
}

/// The install root holding `executable`, and whether `executable` belongs to its current version;
/// `None` outside an installed layout.
/// 包含 `executable` 的安装根目录，以及 `executable` 是否属于其当前版本；在已安装布局之外为 `None`。
pub fn installed_root(executable: &Path) -> Option<(PathBuf, bool)> {
    let installed = installed::detect(executable, vulcan_product_info::version()).ok()?;
    let current = installed
        .root
        .current_version()
        .ok()
        .flatten()
        .is_some_and(|current| current == installed.version);
    Some((installed.root.path().to_path_buf(), current))
}

/// What the service process does for the updater.
/// 服务进程为更新器所做的事。
pub trait UpdateHost: Send + Sync + 'static {
    /// Reports a changed snapshot to every client.
    /// 将变化后的快照报告给每个客户端。
    fn changed(&self, snapshot: &AppUpdateSnapshot);

    /// Tells every client about `event`, then stops the service and starts `plan`; returns once
    /// the stop is under way.
    /// 将 `event` 告知每个客户端，然后停止服务并启动 `plan`；停止开始进行后返回。
    fn restart(&self, event: ServiceRestartingEvent, plan: RestartPlan) -> Result<(), String>;
}

/// What starts once the service stopped.
/// 服务停止后启动的内容。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestartPlan {
    /// The service of the new current version, in the same launch mode, once this one exited.
    /// 新当前版本的服务，以相同启动模式，在本进程退出后启动。
    Service {
        /// Its executable.
        /// 其可执行文件。
        executable: PathBuf,
    },
    /// The full installer, run detached.
    /// 完整安装包，分离运行。
    Installer {
        /// The verified installer.
        /// 已校验的安装包。
        program: PathBuf,
        /// Its arguments.
        /// 其参数。
        arguments: Vec<String>,
    },
    /// The setup program of this root, run once this process exited ([`setup_command`]); it
    /// switches to the version assembled for it and refreshes its integration, and the service of
    /// the current version starts afterwards.
    /// 本根目录的安装器，在本进程退出后运行（[`setup_command`]）；它切换到为其组装的版本并刷新集成，
    /// 之后启动当前版本的服务。
    Setup {
        /// The version assembled for it.
        /// 为其组装的版本。
        version: String,
        /// The channel that offered the version, `stable` or `preview`.
        /// 提供该版本的通道，`stable` 或 `preview`。
        channel: String,
    },
}

/// How an offered version is installed on this root.
/// 所提供版本在本根目录上的安装方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Method {
    /// Assembled next to the running version and made current at once.
    /// 在正在运行的版本旁组装并立即设为当前版本。
    Switch,
    /// Assembled next to the running version; the setup program makes it current on restart.
    /// 在正在运行的版本旁组装；重启时由安装器设为当前版本。
    Setup,
    /// Assembled next to the running version; the Inno update package makes it current on restart.
    /// 在正在运行的版本旁组装；重启时由 Inno 更新包设为当前版本。
    InnoUpdate,
    /// The full Windows installer runs on restart.
    /// 重启时运行完整的 Windows 安装包。
    Installer,
    /// The app cannot install it.
    /// 应用无法安装它。
    Manual,
}

/// What a root offers for installing a version, as [`install_method`] weighs it.
/// 根目录为安装某版本所具备的条件，供 [`install_method`] 权衡。
#[derive(Clone, Copy, Debug)]
struct Situation {
    /// The version keeps the running install epoch.
    /// 该版本沿用正在运行的安装纪元。
    same_epoch: bool,
    /// The version publishes every installed component.
    /// 该版本发布了每个已安装组件。
    components_present: bool,
    /// The setup program owns the root.
    /// 根目录由安装器拥有。
    setup_owned: bool,
    /// The root holds its setup program.
    /// 根目录中有安装器程序。
    setup_program_present: bool,
    /// The platform is Windows, where only the installer that made a root may refresh it.
    /// 平台为 Windows，只有创建该根目录的安装程序可以刷新它。
    windows: bool,
    /// The version publishes a full Windows installer.
    /// 该版本发布了完整的 Windows 安装包。
    has_installer: bool,
    /// The version publishes the Inno update package.
    /// 该版本发布了 Inno 更新包。
    has_update_package: bool,
}

/// Chooses how a version installs. Assembly reuses every file the running version already has by
/// its digest, so it is preferred whenever the root can be finished without a full installer: the
/// same epoch is switched at once; across epochs the setup program finishes the roots it owns, a
/// root no installer made has nothing outside its versions to refresh, and on a Windows root the
/// Inno installer made the Inno update package finishes it, since only Inno rewrites its own
/// registration. The full installer remains only for a Windows root that cannot be assembled or a
/// version without the update package.
/// 选择版本的安装方式。组装按摘要复用正在运行版本已有的每个文件，因此只要根目录无需完整安装包即可完成就
/// 优先组装：同一纪元立即切换；跨纪元时安装器完成它拥有的根目录，不由安装程序创建的根目录在版本之外没有
/// 需要刷新的内容，Inno 安装包创建的 Windows 根目录由 Inno 更新包完成，因为只有 Inno 能改写自身的注册
/// 信息。完整安装包只留给无法组装的 Windows 根目录或没有更新包的版本。
fn install_method(situation: Situation) -> Method {
    let Situation {
        same_epoch,
        components_present,
        setup_owned,
        setup_program_present,
        windows,
        has_installer,
        has_update_package,
    } = situation;
    if components_present && same_epoch {
        Method::Switch
    } else if setup_owned {
        if components_present && setup_program_present {
            Method::Setup
        } else {
            Method::Manual
        }
    } else if windows {
        if components_present && has_update_package {
            Method::InnoUpdate
        } else if has_installer {
            Method::Installer
        } else {
            Method::Manual
        }
    } else if components_present {
        Method::Switch
    } else {
        Method::Manual
    }
}

/// The setup program that finishes an update of the root holding `executable`, and its arguments:
/// `update`, or `install --channel preview` for a preview version, since the setup program
/// otherwise looks only at the channel recorded when it installed.
/// 完成包含 `executable` 的根目录更新的安装器及其参数：`update`；预览版本使用
/// `install --channel preview`，否则安装器只查看安装时记录的通道。
pub fn setup_command(executable: &Path, channel: &str) -> Result<(PathBuf, Vec<String>), String> {
    let installed = detect_running(executable)?;
    Ok((
        installed.root.setup_program(),
        setup_arguments(installed.root.path(), channel),
    ))
}

/// The arguments of [`setup_command`] for the root at `root`.
/// [`setup_command`] 对位于 `root` 的根目录使用的参数。
fn setup_arguments(root: &Path, channel: &str) -> Vec<String> {
    let mut arguments: Vec<String> = if channel == Channel::Preview.as_str() {
        vec![
            String::from("install"),
            String::from("--yes"),
            String::from("--channel"),
            Channel::Preview.as_str().to_owned(),
        ]
    } else {
        vec![String::from("update"), String::from("--yes")]
    };
    arguments.push(String::from("--prefix"));
    arguments.push(plain_path(root).display().to_string());
    arguments
}

/// Records how a setup program run started with [`setup_command`] ended for `version`, so the next
/// service start reports a failure; `outcome` is its exit status, or why it could not run.
/// 记录以 [`setup_command`] 启动的安装器运行对 `version` 的结果，使下一次服务启动报告失败；`outcome`
/// 为其退出状态，或其无法运行的原因。
///
/// The run succeeded when it exited cleanly and the current version reached `version`; the setup
/// program may have found an even newer one.
/// 运行干净退出且当前版本达到 `version` 时视为成功；安装器可能找到了更新的版本。
pub fn finish_setup_run(
    executable: &Path,
    version: &str,
    outcome: Result<std::process::ExitStatus, String>,
) -> Result<(), String> {
    let installed = detect_running(executable)?;
    let version: ReleaseVersion = version.parse().map_err(|error| format!("{error}"))?;
    let root = &installed.root;
    let current = root.current_version().map_err(|error| error.to_string())?;
    let reached = current.is_some_and(|current| current >= version);
    let reason = setup_failure(outcome.map(|status| status.success()), reached);
    let lock = root.lock().map_err(|error| error.to_string())?;
    match reason {
        Some(reason) => {
            root.write_marker(&lock, SETUP_FAILURE_FILE, &SetupFailure { version, reason })
        }
        None => root.remove_marker(&lock, SETUP_FAILURE_FILE),
    }
    .map_err(|error| error.to_string())
}

/// Why a setup program run failed: `exited` tells whether it exited successfully (or why it could
/// not run), `reached` whether the current version reached the assembled one; `None` on success.
/// 安装器运行失败的原因：`exited` 表示是否成功退出（或无法运行的原因），`reached` 表示当前版本是否已达到
/// 组装的版本；成功时为 `None`。
fn setup_failure(exited: Result<bool, String>, reached: bool) -> Option<String> {
    match exited {
        Err(error) => Some(format!("it could not run: {error}")),
        Ok(false) => Some(String::from("it exited with a failure")),
        Ok(true) if !reached => Some(String::from("it finished without switching to the version")),
        Ok(true) => None,
    }
}

/// Where a setup program run started by the updater writes its output.
/// 由更新器启动的安装器运行写出输出的位置。
pub fn setup_log(executable: &Path) -> Option<PathBuf> {
    let installed = detect_running(executable).ok()?;
    Some(installed.root.path().join(UPDATES_DIR).join(SETUP_LOG_FILE))
}

/// The same executable inside the current version of the root holding `executable`.
/// 包含 `executable` 的根目录当前版本中的同一可执行文件。
pub fn current_executable(executable: &Path) -> Option<PathBuf> {
    let installed = detect_running(executable).ok()?;
    let current = installed.root.current_version().ok()??;
    Some(installed.executable_of(&current))
}

/// The installed layout of the running process at `executable`.
/// 运行于 `executable` 的进程所在的已安装布局。
fn detect_running(executable: &Path) -> Result<Installed, String> {
    installed::detect(executable, vulcan_product_info::version())
        .map_err(|reason| format!("updating is unavailable: {reason:?}"))
}

/// `path` without the Windows verbatim prefix that canonicalizing adds, so the setup program
/// records the root as users write it.
/// 去掉规范化时添加的 Windows 逐字前缀的 `path`，使安装器以用户书写的形式记录根目录。
fn plain_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(share) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{share}"))
    } else if let Some(local) = text.strip_prefix(r"\\?\") {
        PathBuf::from(local)
    } else {
        path.to_path_buf()
    }
}

/// The update subsystem of one service process.
/// 一个服务进程的更新子系统。
#[derive(Clone)]
pub struct AppUpdater {
    /// Shared state of the subsystem.
    /// 子系统的共享状态。
    inner: Arc<Inner>,
}

/// Shared state of an [`AppUpdater`].
/// [`AppUpdater`] 的共享状态。
struct Inner {
    /// The installed layout, or why there is none.
    /// 已安装布局，或没有布局的原因。
    installed: Result<Installed, AppUpdateUnavailableReason>,
    /// The AppArmor profile the desktop app of this installation needs on Linux.
    /// 本安装的桌面应用在 Linux 上所需的 AppArmor 配置。
    desktop_profile: Option<DesktopProfile>,
    /// The service process.
    /// 服务进程。
    host: Box<dyn UpdateHost>,
    /// User agent of every request.
    /// 每个请求的 User-Agent。
    user_agent: String,
    /// Mutable state.
    /// 可变状态。
    state: Mutex<State>,
}

/// Mutable state of an [`AppUpdater`].
/// [`AppUpdater`] 的可变状态。
#[derive(Default)]
struct State {
    /// Effective settings.
    /// 生效的设置。
    settings: AppUpdateSettings,
    /// Current work.
    /// 当前工作。
    activity: AppUpdateActivity,
    /// Byte counter, its value when the transfer started, and the expected total.
    /// 字节计数器、传输开始时的值与预计总量。
    progress: Option<(Arc<AtomicU64>, u64, u64)>,
    /// Cancels the running transfer.
    /// 取消进行中的传输。
    cancel: Option<Arc<AtomicBool>>,
    /// When the last check finished.
    /// 上次检查完成的时间。
    last_checked_at: Option<u64>,
    /// When the last check, automatic or manual, finished, for scheduling the next one.
    /// 上次检查（自动或手动）完成的时刻，用于安排下一次检查。
    last_check_finished: Option<Instant>,
    /// The newer version and what installing it needs.
    /// 更新版本及安装它所需的内容。
    offer: Option<Offer>,
    /// A version waiting for a restart.
    /// 等待重启的版本。
    ready: Option<Ready>,
    /// Files restored by the last repair.
    /// 上次修复恢复的文件数。
    repaired_files: Option<u64>,
    /// The last failure.
    /// 最近一次失败。
    error: Option<AppUpdateError>,
    /// A failed start the launcher switched back from.
    /// 启动器切换回去的一次启动失败。
    rollback: Option<Rollback>,
    /// The origin that answered last.
    /// 最近一次应答的下载源。
    preferred_origin: Option<String>,
    /// Whether the desktop profile is being installed, waiting for the password.
    /// 桌面配置是否正在安装（等待密码）。
    installing_desktop_profile: bool,
    /// Why the last installation of the desktop profile failed.
    /// 上次安装桌面配置失败的原因。
    desktop_profile_error: Option<String>,
}

/// A newer version found by a check.
/// 检查找到的更新版本。
struct Offer {
    /// What clients see.
    /// 客户端看到的内容。
    public: AppUpdateOffer,
    /// Its verified manifest.
    /// 其已校验的版本清单。
    manifest: ReleaseManifest,
    /// Indexes of the installed components, for an assembled update.
    /// 已安装组件的索引，用于组装的更新。
    indexes: Vec<ComponentIndex>,
    /// The channel that offered it.
    /// 提供它的通道。
    channel: Channel,
    /// How it installs on this root; never [`Method::Manual`] for an installable offer.
    /// 它在本根目录上的安装方式；可安装的提议绝不为 [`Method::Manual`]。
    method: Method,
}

/// A version waiting for a restart.
/// 等待重启的版本。
#[derive(Clone)]
struct Ready {
    /// The version.
    /// 版本。
    version: ReleaseVersion,
    /// What the restart runs.
    /// 重启时运行的内容。
    apply: Apply,
}

/// What a restart runs to start a version made ready.
/// 重启时为启动已就绪版本所运行的内容。
#[derive(Clone)]
enum Apply {
    /// The service of the version already made current.
    /// 已设为当前版本的服务。
    Switched,
    /// The verified full installer.
    /// 已校验的完整安装包。
    Installer(PathBuf),
    /// The setup program, which makes the assembled version current; the offer's channel.
    /// 安装器，由它将已组装的版本设为当前版本；附提议所属的通道。
    Setup(Channel),
}

impl AppUpdater {
    /// Creates the updater of the service running from `executable`, reading the settings saved
    /// by the user; nothing runs until [`Self::start`].
    /// 为从 `executable` 运行的服务创建更新器，读取用户保存的设置；在 [`Self::start`] 之前不运行任何工作。
    pub fn new(
        executable: &Path,
        settings: AppUpdateSettings,
        user_agent: String,
        host: Box<dyn UpdateHost>,
    ) -> Self {
        let installed = installed::detect(executable, vulcan_product_info::version());
        let desktop_profile = installed.as_ref().ok().and_then(DesktopProfile::of);
        Self {
            inner: Arc::new(Inner {
                installed,
                desktop_profile,
                host,
                user_agent,
                state: Mutex::new(State {
                    settings,
                    ..State::default()
                }),
            }),
        }
    }

    /// Completes the startup handshake, cleans up old versions and schedules the automatic checks.
    /// 完成启动握手，清理旧版本，并安排自动检查。
    pub fn start(&self) {
        if let Ok(installed) = &self.inner.installed {
            if let Err(error) = self.inner.finish_startup(installed) {
                tracing::warn!("[app-update] startup bookkeeping failed: {error}");
            }
            // The manual command in the snapshot names the prepared profile, so it is written now.
            // 快照中的手动命令写明已准备的配置，因此现在写出它。
            if let Some(profile) = &self.inner.desktop_profile
                && profile.status().is_some()
                && let Err(error) = profile.stage()
            {
                tracing::warn!("[app-update] could not prepare the AppArmor profile: {error}");
            }
            let updater = self.clone();
            let spawned = std::thread::Builder::new()
                .name(String::from("vulcan-app-update-schedule"))
                .spawn(move || {
                    let first_check = Instant::now() + FIRST_CHECK_DELAY;
                    loop {
                        std::thread::sleep(SCHEDULE_TICK);
                        if updater.inner.wants_automatic_check(first_check) {
                            updater.inner.run(
                                AppUpdateActivity::Checking,
                                AppUpdateOperation::Check,
                                Inner::check,
                            );
                        }
                    }
                });
            if let Err(error) = spawned {
                tracing::warn!("[app-update] could not schedule update checks: {error}");
            }
        }
        self.inner.notify();
    }

    /// The current snapshot.
    /// 当前快照。
    pub fn snapshot(&self) -> AppUpdateSnapshot {
        self.inner.snapshot()
    }

    /// Applies new settings to the following operations.
    /// 将新设置应用于之后的操作。
    pub fn set_settings(&self, settings: AppUpdateSettings) -> AppUpdateSnapshot {
        self.inner.lock().settings = settings;
        self.inner.notify();
        self.snapshot()
    }

    /// Starts a check in the background.
    /// 在后台开始检查。
    pub fn check(&self) -> Result<AppUpdateSnapshot, String> {
        self.spawn(
            AppUpdateActivity::Checking,
            AppUpdateOperation::Check,
            Inner::check,
        )
    }

    /// Starts downloading the offered version in the background.
    /// 在后台开始下载所提供的版本。
    pub fn download(&self) -> Result<AppUpdateSnapshot, String> {
        if self.inner.lock().offer.is_none() {
            return Err(String::from("no newer version was found"));
        }
        self.spawn(
            AppUpdateActivity::Downloading,
            AppUpdateOperation::Download,
            Inner::download,
        )
    }

    /// Starts repairing the running version in the background.
    /// 在后台开始修复正在运行的版本。
    pub fn repair(&self) -> Result<AppUpdateSnapshot, String> {
        self.spawn(
            AppUpdateActivity::Repairing,
            AppUpdateOperation::Repair,
            Inner::repair,
        )
    }

    /// Cancels a running download or repair; what is complete is kept for the next attempt.
    /// 取消进行中的下载或修复；已完成的部分保留给下一次尝试。
    pub fn cancel(&self) -> AppUpdateSnapshot {
        if let Some(cancel) = &self.inner.lock().cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        self.snapshot()
    }

    /// Clears the notice about a version that failed to start.
    /// 清除有关某版本启动失败的提示。
    pub fn dismiss_rollback(&self) -> Result<AppUpdateSnapshot, String> {
        let installed = self.inner.available()?;
        let lock = installed.root.lock().map_err(|error| error.to_string())?;
        installed
            .root
            .remove_marker(&lock, ROLLBACK_FILE)
            .map_err(|error| error.to_string())?;
        self.inner.lock().rollback = None;
        self.inner.notify();
        Ok(self.snapshot())
    }

    /// Starts installing the AppArmor profile the desktop app needs; the administrator password is
    /// asked through polkit, and the snapshot reports the outcome. Other update work goes on
    /// meanwhile.
    /// 开始安装桌面应用所需的 AppArmor 配置；管理员密码经 polkit 询问，快照报告结果。其间其他更新工作照常进行。
    pub fn install_desktop_profile(&self) -> Result<AppUpdateSnapshot, String> {
        if self
            .inner
            .desktop_profile
            .as_ref()
            .and_then(DesktopProfile::status)
            .is_none()
        {
            return Err(String::from(
                "the desktop app needs no AppArmor profile on this system",
            ));
        }
        {
            let mut state = self.inner.lock();
            if state.installing_desktop_profile {
                return Err(String::from(
                    "the AppArmor profile is already being installed",
                ));
            }
            state.installing_desktop_profile = true;
            state.desktop_profile_error = None;
        }
        let inner = Arc::clone(&self.inner);
        let spawned = std::thread::Builder::new()
            .name(String::from("vulcan-app-update-profile"))
            .spawn(move || {
                let result = inner
                    .desktop_profile
                    .as_ref()
                    .map_or(Ok(()), DesktopProfile::install);
                let mut state = inner.lock();
                state.installing_desktop_profile = false;
                if let Err(error) = result {
                    tracing::warn!("[app-update] installing the AppArmor profile failed: {error}");
                    state.desktop_profile_error = Some(error);
                }
                drop(state);
                inner.notify();
            });
        if let Err(error) = spawned {
            self.inner.lock().installing_desktop_profile = false;
            return Err(format!(
                "could not start installing the AppArmor profile: {error}"
            ));
        }
        self.inner.notify();
        Ok(self.snapshot())
    }

    /// Stops the service so that the version made ready starts, or runs the full installer or the
    /// setup program.
    /// 停止服务以启动已就绪的版本，或运行完整安装包或安装器。
    pub fn restart(&self) -> Result<AppUpdateSnapshot, String> {
        let installed = self.inner.available()?;
        let ready = {
            let mut state = self.inner.lock();
            if state.activity != AppUpdateActivity::Idle {
                return Err(String::from("another update operation is running"));
            }
            let ready = state
                .ready
                .clone()
                .ok_or_else(|| String::from("no new version is ready"))?;
            state.activity = AppUpdateActivity::Restarting;
            ready
        };
        self.inner.notify();
        let (reason, plan) = match &ready.apply {
            Apply::Installer(program) => (
                ServiceRestartReason::Installer,
                RestartPlan::Installer {
                    program: program.clone(),
                    arguments: INSTALLER_ARGUMENTS
                        .iter()
                        .map(|arg| (*arg).to_owned())
                        .collect(),
                },
            ),
            Apply::Setup(channel) => (
                ServiceRestartReason::Setup,
                RestartPlan::Setup {
                    version: ready.version.to_string(),
                    channel: channel.as_str().to_owned(),
                },
            ),
            Apply::Switched => (
                ServiceRestartReason::Update,
                RestartPlan::Service {
                    executable: installed.executable_of(&ready.version),
                },
            ),
        };
        let event = ServiceRestartingEvent {
            reason,
            version: ready.version.to_string(),
        };
        if let Err(message) = self.inner.host.restart(event, plan) {
            let mut state = self.inner.lock();
            state.activity = AppUpdateActivity::Idle;
            state.error = Some(AppUpdateError {
                operation: AppUpdateOperation::Restart,
                message: message.clone(),
            });
            drop(state);
            self.inner.notify();
            return Err(message);
        }
        Ok(self.snapshot())
    }

    /// Runs `work` on a worker thread once no other operation runs.
    /// 在没有其他操作运行时，于工作线程上运行 `work`。
    fn spawn(
        &self,
        activity: AppUpdateActivity,
        operation: AppUpdateOperation,
        work: fn(&Inner, &Installed, &mut Downloader) -> Result<(), FormatError>,
    ) -> Result<AppUpdateSnapshot, String> {
        self.inner.available()?;
        self.inner.claim(activity)?;
        let inner = Arc::clone(&self.inner);
        std::thread::Builder::new()
            .name(String::from("vulcan-app-update"))
            .spawn(move || inner.run_claimed(operation, work))
            .map_err(|error| {
                self.inner.lock().activity = AppUpdateActivity::Idle;
                format!("could not start the update worker: {error}")
            })?;
        Ok(self.snapshot())
    }
}

impl Inner {
    /// Locks the mutable state, recovering it when a worker panicked.
    /// 锁定可变状态；工作线程崩溃时恢复它。
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The installed layout, or the reason updating is off.
    /// 已安装布局，或更新关闭的原因。
    fn available(&self) -> Result<&Installed, String> {
        self.installed
            .as_ref()
            .map_err(|reason| format!("updating is unavailable: {reason:?}"))
    }

    /// Reports the current snapshot.
    /// 报告当前快照。
    fn notify(&self) {
        self.host.changed(&self.snapshot());
    }

    /// Builds the snapshot.
    /// 构建快照。
    fn snapshot(&self) -> AppUpdateSnapshot {
        let state = self.lock();
        AppUpdateSnapshot {
            current_version: vulcan_product_info::version().to_owned(),
            unavailable_reason: self.installed.as_ref().err().copied(),
            settings: state.settings,
            activity: state.activity,
            progress: state
                .progress
                .as_ref()
                .map(|(counter, start, total)| AppUpdateProgress {
                    received_bytes: counter
                        .load(Ordering::Relaxed)
                        .saturating_sub(*start)
                        .min(*total),
                    total_bytes: *total,
                }),
            last_checked_at: state.last_checked_at,
            offer: state.offer.as_ref().map(|offer| AppUpdateOffer {
                previously_failed: state
                    .rollback
                    .as_ref()
                    .is_some_and(|rollback| rollback.failed.to_string() == offer.public.version),
                ..offer.public.clone()
            }),
            ready_version: state.ready.as_ref().map(|ready| ready.version.to_string()),
            repaired_files: state.repaired_files,
            error: state.error.clone(),
            rollback: state.rollback.as_ref().map(|rollback| AppUpdateRollback {
                failed: rollback.failed.to_string(),
                restored: rollback.restored.to_string(),
            }),
            desktop_profile: self.desktop_profile.as_ref().and_then(|profile| {
                Some(AppUpdateDesktopProfile {
                    status: profile.status()?,
                    installing: state.installing_desktop_profile,
                    command: profile.manual_command(),
                    error: state.desktop_profile_error.clone(),
                })
            }),
            download_page_url: DOWNLOAD_PAGE_URL.to_owned(),
        }
    }

    /// Whether the schedule should check now: the updater is idle with nothing waiting for a
    /// restart, and the chosen interval has passed since the last check, or `first_check` has
    /// come when there was none.
    /// 调度此刻是否应检查：更新器空闲且没有等待重启的版本，并且距上次检查已过所选间隔；从未检查时则
    /// 以到达 `first_check` 为准。
    fn wants_automatic_check(&self, first_check: Instant) -> bool {
        let state = self.lock();
        let due = state.last_check_finished.map_or(first_check, |finished| {
            finished + check_interval(state.settings.check_interval)
        });
        state.activity == AppUpdateActivity::Idle && state.ready.is_none() && Instant::now() >= due
    }

    /// Marks `activity` as running, failing when another operation runs.
    /// 将 `activity` 标记为进行中；其他操作进行中时失败。
    fn claim(&self, activity: AppUpdateActivity) -> Result<(), String> {
        let mut state = self.lock();
        if state.activity != AppUpdateActivity::Idle {
            return Err(String::from("another update operation is running"));
        }
        state.activity = activity;
        state.error = None;
        Ok(())
    }

    /// Claims `activity` and runs `work` on this thread; a busy updater skips it.
    /// 声明 `activity` 并在当前线程运行 `work`；更新器忙碌时跳过。
    fn run(
        &self,
        activity: AppUpdateActivity,
        operation: AppUpdateOperation,
        work: fn(&Self, &Installed, &mut Downloader) -> Result<(), FormatError>,
    ) {
        if self.claim(activity).is_ok() {
            self.run_claimed(operation, work);
        }
    }

    /// Runs `work` for the claimed activity, reporting progress until it ends.
    /// 为已声明的工作运行 `work`，并在结束前持续报告进度。
    fn run_claimed(
        &self,
        operation: AppUpdateOperation,
        work: fn(&Self, &Installed, &mut Downloader) -> Result<(), FormatError>,
    ) {
        self.notify();
        let result = match &self.installed {
            Ok(installed) => self.downloader().and_then(|mut downloader| {
                let done = AtomicBool::new(false);
                let outcome = std::thread::scope(|scope| {
                    scope.spawn(|| {
                        while !done.load(Ordering::Relaxed) {
                            std::thread::sleep(PROGRESS_INTERVAL);
                            if self.lock().progress.is_some() && !done.load(Ordering::Relaxed) {
                                self.notify();
                            }
                        }
                    });
                    let outcome = work(self, installed, &mut downloader);
                    done.store(true, Ordering::Relaxed);
                    outcome
                });
                self.lock().preferred_origin = Some(downloader.preferred_origin().to_owned());
                downloader.settle(outcome)
            }),
            Err(reason) => Err(FormatError::Download {
                key: String::new(),
                reason: format!("updating is unavailable: {reason:?}"),
            }),
        };
        let mut state = self.lock();
        state.activity = AppUpdateActivity::Idle;
        state.progress = None;
        state.cancel = None;
        if operation == AppUpdateOperation::Check {
            state.last_checked_at = Some(now_ms());
            state.last_check_finished = Some(Instant::now());
        }
        match result {
            Ok(()) => {}
            Err(FormatError::Cancelled) => {}
            Err(error) => {
                tracing::warn!("[app-update] {operation:?} failed: {error}");
                state.error = Some(AppUpdateError {
                    operation,
                    message: error.to_string(),
                });
            }
        }
        drop(state);
        self.notify();
    }

    /// A downloader for the configured route, starting with the origin that answered last.
    /// 按配置线路创建的下载器，从最近应答的下载源开始。
    fn downloader(&self) -> Result<Downloader, FormatError> {
        let (route, preferred) = {
            let state = self.lock();
            (state.settings.route, state.preferred_origin.clone())
        };
        let route = match route {
            AppUpdateRouteSetting::Auto => Route::Auto,
            AppUpdateRouteSetting::International => Route::International,
            AppUpdateRouteSetting::China => Route::China,
        };
        let mut downloader = Downloader::new(route.origins(), &self.user_agent)?;
        if let Some(origin) = preferred {
            downloader.prefer(&origin);
        }
        self.lock().cancel = Some(downloader.cancel_flag());
        Ok(downloader)
    }

    /// Reports the transfer progress of `downloader` against `total` bytes from now on.
    /// 此后按 `total` 字节报告 `downloader` 的传输进度。
    fn track(&self, downloader: &Downloader, total: u64) {
        let counter = downloader.received_counter();
        let start = counter.load(Ordering::Relaxed);
        self.lock().progress = Some((counter, start, total));
    }

    /// Reads the pointers and describes the newest version above the running one.
    /// 读取指针并描述高于正在运行版本的最新版本。
    fn check(&self, installed: &Installed, downloader: &mut Downloader) -> Result<(), FormatError> {
        let preview = self.lock().settings.preview;
        let channels: &[Channel] = if preview {
            &[Channel::Stable, Channel::Preview]
        } else {
            &[Channel::Stable]
        };
        let found = find_update(
            downloader,
            BUILT_IN_KEYS,
            installed.platform,
            channels,
            &installed.version,
        )?;
        let offer = match found {
            None => None,
            Some(found) => Some(describe(installed, downloader, found)?),
        };
        let mut state = self.lock();
        // A version made ready stays ready; a newer offer can still be downloaded on top of it.
        // 已就绪的版本保持就绪；更新的版本仍可在其基础上下载。
        state.offer = offer.filter(|offer| {
            state
                .ready
                .as_ref()
                .is_none_or(|ready| offer.manifest.version > ready.version)
        });
        Ok(())
    }

    /// Downloads the offered version: assembles it next to the running one and makes it current,
    /// assembles it for the setup program or the Inno update package to finish, or downloads and
    /// verifies the full installer.
    /// 下载所提供的版本：在正在运行版本旁组装并设为当前版本；组装后交由安装器或 Inno 更新包完成；或下载并
    /// 校验完整安装包。
    fn download(
        &self,
        installed: &Installed,
        downloader: &mut Downloader,
    ) -> Result<(), FormatError> {
        let (method, manifest, indexes, channel) = {
            let state = self.lock();
            let offer = state
                .offer
                .as_ref()
                .ok_or_else(|| invalid("no newer version was found"))?;
            (
                offer.method,
                offer.manifest.clone(),
                offer.indexes.clone(),
                offer.channel,
            )
        };
        let apply = match method {
            Method::Installer => {
                Apply::Installer(self.download_installer(installed, downloader, &manifest)?)
            }
            Method::Switch => {
                self.assemble(installed, downloader, &manifest, indexes, true, 0)?;
                Apply::Switched
            }
            Method::Setup => {
                if !installed.root.setup_program().is_file() {
                    return Err(invalid(
                        "the setup program of this installation is missing; run `vulcan-code-setup update`",
                    ));
                }
                self.assemble(installed, downloader, &manifest, indexes, false, 0)?;
                Apply::Setup(channel)
            }
            Method::InnoUpdate => {
                let package = update_package(installed, &manifest)
                    .ok_or_else(|| invalid("this version publishes no Inno update package"))?;
                // One progress covers the changed files and the package. An assembled version whose
                // package then fails is reused by the next attempt, or pruned at the next start.
                // 同一进度涵盖变化的文件与更新包。若组装后更新包失败，已组装的版本供下次尝试复用，或在下次启动时
                // 清理。
                self.assemble(
                    installed,
                    downloader,
                    &manifest,
                    indexes,
                    false,
                    package.size,
                )?;
                Apply::Installer(download_signed_installer(
                    installed,
                    downloader,
                    &manifest,
                    &package.file,
                    package.size,
                    package.sha256,
                )?)
            }
            Method::Manual => return Err(invalid("this version cannot be installed here")),
        };
        let mut state = self.lock();
        state.ready = Some(Ready {
            version: manifest.version,
            apply,
        });
        state.offer = None;
        Ok(())
    }

    /// Assembles `manifest` next to the running version and, when `switch` is set, makes it current
    /// on trial; otherwise the setup program or the Inno update package switches to it on restart.
    /// The progress it starts also counts `then_bytes` the caller downloads next.
    /// 在正在运行的版本旁组装 `manifest`；`switch` 为真时将其设为试运行中的当前版本，否则由安装器或 Inno
    /// 更新包在重启时切换到它。它开始的进度同时计入调用方随后下载的 `then_bytes`。
    fn assemble(
        &self,
        installed: &Installed,
        downloader: &mut Downloader,
        manifest: &ReleaseManifest,
        indexes: Vec<ComponentIndex>,
        switch: bool,
        then_bytes: u64,
    ) -> Result<(), FormatError> {
        let root = &installed.root;
        let lock = root.lock()?;
        if root
            .version_dir(&manifest.version)
            .join(RECORD_FILE_NAME)
            .is_file()
        {
            self.track(downloader, then_bytes);
        } else {
            let prepared = Assembly::new(root, &lock, manifest, indexes)?.prepare()?;
            self.track(downloader, prepared.download_bytes() + then_bytes);
            prepared.complete(downloader)?;
        }
        if !switch {
            return Ok(());
        }
        root.switch_to(&lock, &manifest.version)?;
        root.write_marker(
            &lock,
            STARTUP_TRIAL_FILE,
            &StartupTrial {
                version: manifest.version.clone(),
                previous: installed.version.clone(),
                attempts: 0,
                max_attempts: MAX_STARTUP_ATTEMPTS,
            },
        )
    }

    /// Downloads and verifies the full installer of `manifest`.
    /// 下载并校验 `manifest` 的完整安装包。
    fn download_installer(
        &self,
        installed: &Installed,
        downloader: &mut Downloader,
        manifest: &ReleaseManifest,
    ) -> Result<PathBuf, FormatError> {
        let installer = full_installer(installed, manifest).ok_or_else(|| {
            invalid(if installed.root.setup_owned() {
                "this installation is managed by vulcan-code-setup; run `vulcan-code-setup update`"
            } else {
                "this version must be installed from the download page"
            })
        })?;
        self.track(downloader, installer.size);
        download_signed_installer(
            installed,
            downloader,
            manifest,
            &installer.file,
            installer.size,
            installer.sha256,
        )
    }

    /// Hashes every installed file and restores the missing or changed ones.
    /// 计算每个已安装文件的哈希，并恢复缺失或被改动的文件。
    fn repair(
        &self,
        installed: &Installed,
        downloader: &mut Downloader,
    ) -> Result<(), FormatError> {
        let root = &installed.root;
        let lock = root.lock()?;
        let manifest = read_manifest(
            downloader,
            BUILT_IN_KEYS,
            &installed.version,
            installed.platform,
        )?;
        let indexes = read_indexes(downloader, &manifest, &installed.components)?;
        let prepared = prepare_repair(root, &lock, &manifest, indexes)?;
        self.track(downloader, prepared.download_bytes());
        let repaired = prepared.complete(downloader)?;
        self.lock().repaired_files = Some(repaired as u64);
        Ok(())
    }

    /// Ends the trial of the running version, keeps a rollback notice that names it, notes a newer
    /// version already made current, and removes versions no longer needed.
    /// 结束正在运行版本的试运行，保留指明它的回退提示，记录已设为当前的更新版本，并删除不再需要的版本。
    fn finish_startup(&self, installed: &Installed) -> Result<(), FormatError> {
        let root = &installed.root;
        let lock = root.lock()?;
        if let Ok(Some(trial)) = root.read_marker::<StartupTrial>(STARTUP_TRIAL_FILE)
            && trial.version == installed.version
        {
            root.remove_marker(&lock, STARTUP_TRIAL_FILE)?;
        }
        // A setup program run that could not finish is reported once.
        // 未能完成的安装器运行只报告一次。
        if let Ok(Some(failure)) = root.read_marker::<SetupFailure>(SETUP_FAILURE_FILE) {
            self.lock().error = Some(AppUpdateError {
                operation: AppUpdateOperation::Restart,
                message: format!(
                    "the setup program did not finish {}: {}; its output is in {}",
                    failure.version,
                    failure.reason,
                    root.path().join(UPDATES_DIR).join(SETUP_LOG_FILE).display()
                ),
            });
        }
        root.remove_marker(&lock, SETUP_FAILURE_FILE)?;
        match root.read_marker::<Rollback>(ROLLBACK_FILE) {
            Ok(Some(rollback)) if rollback.restored == installed.version => {
                self.lock().rollback = Some(rollback);
            }
            _ => root.remove_marker(&lock, ROLLBACK_FILE)?,
        }
        match root.current_version()? {
            Some(current) if current == installed.version => {
                let removed = root.prune_versions(&lock, &installed.version)?;
                if !removed.is_empty() {
                    tracing::info!("[app-update] removed old versions {removed:?}");
                }
            }
            Some(current) if current > installed.version => {
                self.lock().ready = Some(Ready {
                    version: current,
                    apply: Apply::Switched,
                });
            }
            _ => {}
        }
        Ok(())
    }
}

/// Describes a found version: how it installs and how much it downloads.
/// 描述找到的版本：安装方式与下载量。
fn describe(
    installed: &Installed,
    downloader: &mut Downloader,
    found: vulcan_install_core::update::Offer,
) -> Result<Offer, FormatError> {
    let manifest = found.manifest;
    let skipped =
        read_skipped_release_notes(downloader, BUILT_IN_KEYS, &manifest, &installed.version);
    if let Some(error) = &skipped.error {
        tracing::warn!(
            "[app-update] notes of the releases before {} stopped early: {error}",
            manifest.version
        );
    }
    let installer = full_installer(installed, &manifest);
    let package = update_package(installed, &manifest);
    let method = install_method(Situation {
        same_epoch: manifest.install_epoch == installed.install_epoch,
        components_present: installed
            .components
            .iter()
            .all(|component| manifest.components.contains_key(component)),
        setup_owned: installed.root.setup_owned(),
        setup_program_present: installed.root.setup_program().is_file(),
        windows: installed.platform.is_windows(),
        has_installer: installer.is_some(),
        has_update_package: package.is_some(),
    });
    let (kind, download_bytes, installable, indexes) = match method {
        Method::Switch | Method::Setup | Method::InnoUpdate => {
            let indexes = read_indexes(downloader, &manifest, &installed.components)?;
            let bytes = incremental_bytes(installed, &indexes)?;
            let (kind, bytes) = match (method, package) {
                (Method::Switch, _) => (AppUpdateKind::Incremental, bytes),
                (Method::InnoUpdate, Some(package)) => (AppUpdateKind::Setup, bytes + package.size),
                _ => (AppUpdateKind::Setup, bytes),
            };
            (kind, bytes, true, indexes)
        }
        Method::Installer => (
            AppUpdateKind::Full,
            installer.map_or(0, |installer| installer.size),
            true,
            Vec::new(),
        ),
        Method::Manual => (AppUpdateKind::Full, 0, false, Vec::new()),
    };
    Ok(Offer {
        public: AppUpdateOffer {
            version: manifest.version.to_string(),
            channel: found.channel.to_string(),
            kind,
            download_bytes,
            released_at: manifest.released_at.clone(),
            notes: found.notes.map(|notes| notes.notes).unwrap_or_default(),
            skipped_releases: skipped
                .releases
                .into_iter()
                .map(|release| AppUpdateReleaseNotes {
                    version: release.version.to_string(),
                    date: release.date,
                    notes: release.notes,
                })
                .collect(),
            skipped_releases_complete: skipped.complete,
            installable,
            setup_managed: installed.root.setup_owned(),
            previously_failed: false,
        },
        manifest,
        indexes,
        channel: found.channel,
        method,
    })
}

/// The full installer the app may run here: the Windows Inno installer, never over a root the setup
/// program owns, where it would add a second uninstall entry and leave the setup state stale.
/// 应用在此可运行的完整安装包：Windows 的 Inno 安装包；绝不用于安装器拥有的根目录，否则会产生第二个
/// 卸载项并使安装器状态失真。
fn full_installer<'a>(
    installed: &Installed,
    manifest: &'a ReleaseManifest,
) -> Option<&'a vulcan_install_core::manifest::Download> {
    manifest
        .downloads
        .iter()
        .find(|download| download.kind == DownloadKind::Inno)
        .filter(|_| installed.platform.is_windows() && !installed.root.setup_owned())
}

/// The Inno update package the app may run here, under the same rule as [`full_installer`].
/// 应用在此可运行的 Inno 更新包，规则与 [`full_installer`] 相同。
fn update_package<'a>(
    installed: &Installed,
    manifest: &'a ReleaseManifest,
) -> Option<&'a InnoUpdate> {
    manifest
        .inno_update
        .as_ref()
        .filter(|_| installed.platform.is_windows() && !installed.root.setup_owned())
}

/// Downloads the installer `file` of `manifest` into `updates\installers`, checked against the
/// manifest's size and SHA-256, then verifies its Authenticode signer.
/// 将 `manifest` 的安装包 `file` 下载到 `updates\installers`，按清单的大小与 SHA-256 校验，再校验其
/// Authenticode 签名者。
fn download_signed_installer(
    installed: &Installed,
    downloader: &mut Downloader,
    manifest: &ReleaseManifest,
    file: &str,
    size: u64,
    sha256: Sha256Digest,
) -> Result<PathBuf, FormatError> {
    let target = installed
        .root
        .path()
        .join(UPDATES_DIR)
        .join("installers")
        .join(file);
    downloader.download_file(
        &installer_key(&manifest.version, file),
        &target,
        size,
        sha256,
    )?;
    signer::verify_installer_signer(&target).map_err(|reason| FormatError::Download {
        key: file.to_owned(),
        reason,
    })?;
    Ok(target)
}

/// Bytes an incremental update downloads: the frames of contents the running version lacks.
/// 增量更新的下载字节数：正在运行版本缺少的内容所在的帧。
fn incremental_bytes(
    installed: &Installed,
    indexes: &[ComponentIndex],
) -> Result<u64, FormatError> {
    let record = InstallRecord::read(&installed.root.version_dir(&installed.version))?;
    let local = record.files_by_digest();
    let mut frames = HashSet::new();
    let mut bytes = 0_u64;
    for index in indexes {
        for entry in &index.entries {
            if let IndexEntry::File {
                sha256,
                pack,
                offset,
                length,
                ..
            } = entry
                && !local.contains_key(sha256)
                && frames.insert((*pack, *offset))
            {
                bytes += length;
            }
        }
    }
    Ok(bytes)
}

/// An invalid-state failure.
/// 无效状态失败。
fn invalid(reason: &str) -> FormatError {
    FormatError::Download {
        key: String::new(),
        reason: reason.to_owned(),
    }
}

/// Milliseconds since the Unix epoch.
/// 自 Unix 纪元起的毫秒数。
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A root of the running epoch with every component published, before any variation.
    /// 沿用运行纪元且发布了全部组件的根目录，作为各变化的起点。
    fn situation() -> Situation {
        Situation {
            same_epoch: true,
            components_present: true,
            setup_owned: false,
            setup_program_present: false,
            windows: false,
            has_installer: false,
            has_update_package: false,
        }
    }

    #[test]
    fn the_same_epoch_is_assembled_and_switched_on_every_root() {
        for (setup_owned, windows) in [(false, false), (true, false), (false, true), (true, true)] {
            let method = install_method(Situation {
                setup_owned,
                setup_program_present: setup_owned,
                windows,
                has_installer: windows,
                has_update_package: windows,
                ..situation()
            });
            assert_eq!(
                method,
                Method::Switch,
                "setup {setup_owned}, windows {windows}"
            );
        }
    }

    #[test]
    fn a_new_epoch_on_a_setup_root_is_assembled_for_the_setup_program() {
        for windows in [false, true] {
            // An Inno package is never run over a root the setup program owns.
            // 绝不在安装器拥有的根目录上运行 Inno 安装包。
            let owned = Situation {
                same_epoch: false,
                setup_owned: true,
                setup_program_present: true,
                windows,
                has_installer: windows,
                has_update_package: windows,
                ..situation()
            };
            assert_eq!(install_method(owned), Method::Setup);
            // Without its program nothing here can finish the root.
            // 缺少安装器程序时，这里没有任何东西能完成该根目录。
            let missing = Situation {
                setup_program_present: false,
                ..owned
            };
            assert_eq!(install_method(missing), Method::Manual);
        }
    }

    #[test]
    fn a_new_epoch_runs_the_installer_only_on_a_root_it_made() {
        let inno = Situation {
            same_epoch: false,
            windows: true,
            has_installer: true,
            ..situation()
        };
        assert_eq!(install_method(inno), Method::Installer);
        let unpublished = Situation {
            has_installer: false,
            ..inno
        };
        assert_eq!(install_method(unpublished), Method::Manual);
        // A Unix root no installer made has nothing outside its versions to refresh.
        // 不由安装程序创建的 Unix 根目录在版本之外没有需要刷新的内容。
        let unix = Situation {
            same_epoch: false,
            ..situation()
        };
        assert_eq!(install_method(unix), Method::Switch);
    }

    #[test]
    fn a_new_epoch_on_an_inno_root_is_assembled_for_the_update_package() {
        let inno = Situation {
            same_epoch: false,
            windows: true,
            has_installer: true,
            has_update_package: true,
            ..situation()
        };
        assert_eq!(install_method(inno), Method::InnoUpdate);
        assert_eq!(
            install_method(Situation {
                has_installer: false,
                ..inno
            }),
            Method::InnoUpdate
        );
        // A version published without the package falls back to the full installer.
        // 未发布更新包的版本退回完整安装包。
        assert_eq!(
            install_method(Situation {
                has_update_package: false,
                ..inno
            }),
            Method::Installer
        );
        // A root that cannot be assembled falls back to the full installer as well.
        // 无法组装的根目录同样退回完整安装包。
        assert_eq!(
            install_method(Situation {
                components_present: false,
                ..inno
            }),
            Method::Installer
        );
    }

    #[test]
    fn a_missing_component_cannot_be_assembled() {
        let missing = Situation {
            components_present: false,
            ..situation()
        };
        assert_eq!(install_method(missing), Method::Manual);
        assert_eq!(
            install_method(Situation {
                setup_owned: true,
                setup_program_present: true,
                ..missing
            }),
            Method::Manual
        );
        assert_eq!(
            install_method(Situation {
                windows: true,
                has_installer: true,
                ..missing
            }),
            Method::Installer
        );
    }

    #[test]
    fn the_setup_program_runs_unattended_on_the_offered_channel() {
        let root = Path::new("/opt/vulcan-code");
        let stable = setup_arguments(root, "stable");
        assert_eq!(stable[..2], ["update", "--yes"]);
        assert!(!stable.iter().any(|arg| arg == "--channel"));
        let preview = setup_arguments(root, "preview");
        assert_eq!(preview[0], "install");
        let channel = preview.iter().position(|arg| arg == "--channel");
        assert_eq!(
            channel
                .and_then(|at| preview.get(at + 1))
                .map(String::as_str),
            Some("preview")
        );
        for arguments in [stable, preview] {
            assert!(arguments.iter().any(|arg| arg == "--yes"));
            let prefix = arguments.iter().position(|arg| arg == "--prefix");
            assert_eq!(
                prefix
                    .and_then(|at| arguments.get(at + 1))
                    .map(String::as_str),
                Some("/opt/vulcan-code")
            );
        }
    }

    #[test]
    fn verbatim_prefixes_are_dropped_from_the_root() {
        assert_eq!(
            plain_path(Path::new(r"\\?\C:\Users\me\VulcanCode")),
            PathBuf::from(r"C:\Users\me\VulcanCode")
        );
        assert_eq!(
            plain_path(Path::new(r"\\?\UNC\server\share\VulcanCode")),
            PathBuf::from(r"\\server\share\VulcanCode")
        );
        assert_eq!(
            plain_path(Path::new("/opt/vulcan-code")),
            PathBuf::from("/opt/vulcan-code")
        );
    }

    #[test]
    fn a_setup_run_succeeds_only_when_it_reached_the_version() {
        assert_eq!(setup_failure(Ok(true), true), None);
        assert!(setup_failure(Ok(true), false).is_some());
        assert!(setup_failure(Ok(false), true).is_some());
        assert!(setup_failure(Err(String::from("not found")), true).is_some());
    }
}
