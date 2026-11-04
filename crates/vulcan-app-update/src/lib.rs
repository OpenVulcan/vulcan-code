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
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use vulcan_install_core::FormatError;
use vulcan_install_core::assemble::Assembly;
use vulcan_install_core::fetch::{Downloader, Route};
use vulcan_install_core::index::{ComponentIndex, IndexEntry};
use vulcan_install_core::install_root::{
    MAX_STARTUP_ATTEMPTS, ROLLBACK_FILE, Rollback, STARTUP_TRIAL_FILE, StartupTrial, UPDATES_DIR,
};
use vulcan_install_core::layout::installer_key;
use vulcan_install_core::manifest::{DownloadKind, ReleaseManifest};
use vulcan_install_core::names::{Channel, ReleaseVersion};
use vulcan_install_core::record::{InstallRecord, RECORD_FILE_NAME};
use vulcan_install_core::repair::prepare_repair;
use vulcan_install_core::signing::BUILT_IN_KEYS;
use vulcan_install_core::update::{find_update, read_indexes, read_manifest};
use vulcan_service_api::{
    AppUpdateActivity, AppUpdateError, AppUpdateKind, AppUpdateOffer, AppUpdateOperation,
    AppUpdateProgress, AppUpdateRollback, AppUpdateRouteSetting, AppUpdateSettings,
    AppUpdateSnapshot, AppUpdateUnavailableReason, ServiceRestartReason, ServiceRestartingEvent,
};

mod installed;
mod signer;

use installed::Installed;

/// Delay between service start and the first automatic check.
/// 服务启动到首次自动检查之间的延迟。
pub const FIRST_CHECK_DELAY: Duration = Duration::from_secs(60);

/// Interval between automatic checks.
/// 自动检查的间隔。
pub const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

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
    /// Indexes of the installed components, for an incremental update.
    /// 已安装组件的索引，用于增量更新。
    indexes: Vec<ComponentIndex>,
}

/// A version waiting for a restart.
/// 等待重启的版本。
#[derive(Clone)]
struct Ready {
    /// The version.
    /// 版本。
    version: ReleaseVersion,
    /// The verified installer of a full update; `None` for an assembled version.
    /// 全量更新的已校验安装包；已组装版本为 `None`。
    installer: Option<PathBuf>,
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
        Self {
            inner: Arc::new(Inner {
                installed,
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
            let updater = self.clone();
            let spawned = std::thread::Builder::new()
                .name(String::from("vulcan-app-update-schedule"))
                .spawn(move || {
                    std::thread::sleep(FIRST_CHECK_DELAY);
                    loop {
                        if updater.inner.wants_automatic_check() {
                            updater.inner.run(
                                AppUpdateActivity::Checking,
                                AppUpdateOperation::Check,
                                Inner::check,
                            );
                        }
                        std::thread::sleep(CHECK_INTERVAL);
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

    /// Stops the service so that the version made ready starts, or runs the full installer.
    /// 停止服务以启动已就绪的版本，或运行完整安装包。
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
        let (reason, plan) = match &ready.installer {
            Some(program) => (
                ServiceRestartReason::Installer,
                RestartPlan::Installer {
                    program: program.clone(),
                    arguments: INSTALLER_ARGUMENTS
                        .iter()
                        .map(|arg| (*arg).to_owned())
                        .collect(),
                },
            ),
            None => (
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
            download_page_url: DOWNLOAD_PAGE_URL.to_owned(),
        }
    }

    /// Whether the schedule should check now.
    /// 调度此刻是否应检查。
    fn wants_automatic_check(&self) -> bool {
        let state = self.lock();
        state.activity == AppUpdateActivity::Idle && state.ready.is_none()
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
    /// or downloads and verifies its installer.
    /// 下载所提供的版本：在正在运行版本旁组装并设为当前版本，或下载并校验其安装包。
    fn download(
        &self,
        installed: &Installed,
        downloader: &mut Downloader,
    ) -> Result<(), FormatError> {
        let (public, manifest, indexes) = {
            let state = self.lock();
            let offer = state
                .offer
                .as_ref()
                .ok_or_else(|| invalid("no newer version was found"))?;
            (
                offer.public.clone(),
                offer.manifest.clone(),
                offer.indexes.clone(),
            )
        };
        let installer = match public.kind {
            AppUpdateKind::Full => Some(self.download_installer(installed, downloader, &manifest)?),
            AppUpdateKind::Incremental => {
                self.assemble(installed, downloader, &manifest, indexes)?;
                None
            }
        };
        let mut state = self.lock();
        state.ready = Some(Ready {
            version: manifest.version,
            installer,
        });
        state.offer = None;
        Ok(())
    }

    /// Assembles `manifest` next to the running version and makes it current on trial.
    /// 在正在运行的版本旁组装 `manifest`，并将其设为试运行中的当前版本。
    fn assemble(
        &self,
        installed: &Installed,
        downloader: &mut Downloader,
        manifest: &ReleaseManifest,
        indexes: Vec<ComponentIndex>,
    ) -> Result<(), FormatError> {
        let root = &installed.root;
        let lock = root.lock()?;
        if !root
            .version_dir(&manifest.version)
            .join(RECORD_FILE_NAME)
            .is_file()
        {
            let prepared = Assembly::new(root, &lock, manifest, indexes)?.prepare()?;
            self.track(downloader, prepared.download_bytes());
            prepared.complete(downloader)?;
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
        let installer = manifest
            .downloads
            .iter()
            .find(|download| download.kind == DownloadKind::Inno)
            .filter(|_| installed.platform.is_windows())
            .ok_or_else(|| invalid("this version must be installed from the download page"))?;
        let target = installed
            .root
            .path()
            .join(UPDATES_DIR)
            .join("installers")
            .join(&installer.file);
        self.track(downloader, installer.size);
        downloader.download_file(
            &installer_key(&manifest.version, &installer.file),
            &target,
            installer.size,
            installer.sha256,
        )?;
        signer::verify_installer_signer(&target).map_err(|reason| FormatError::Download {
            key: installer.file.clone(),
            reason,
        })?;
        Ok(target)
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
                    installer: None,
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
    let incremental = manifest.install_epoch == installed.install_epoch
        && installed
            .components
            .iter()
            .all(|component| manifest.components.contains_key(component));
    let (kind, download_bytes, installable, indexes) = if incremental {
        let indexes = read_indexes(downloader, &manifest, &installed.components)?;
        let bytes = incremental_bytes(installed, &indexes)?;
        (AppUpdateKind::Incremental, bytes, true, indexes)
    } else {
        let installer = manifest
            .downloads
            .iter()
            .find(|download| download.kind == DownloadKind::Inno)
            .filter(|_| installed.platform.is_windows());
        (
            AppUpdateKind::Full,
            installer.map_or(0, |installer| installer.size),
            installer.is_some(),
            Vec::new(),
        )
    };
    Ok(Offer {
        public: AppUpdateOffer {
            version: manifest.version.to_string(),
            channel: found.channel.to_string(),
            kind,
            download_bytes,
            released_at: manifest.released_at.clone(),
            notes: found.notes.map(|notes| notes.notes).unwrap_or_default(),
            installable,
            previously_failed: false,
        },
        manifest,
        indexes,
    })
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
