use super::*;

/// Which download origins the updater uses (`vulcan-install-core` `fetch::Route`).
/// 更新器使用哪些下载源（`vulcan-install-core` `fetch::Route`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppUpdateRouteSetting {
    /// The international origin, then the mainland China mirror.
    /// 先国际下载源，再中国大陆镜像。
    #[default]
    Auto,
    /// Only the international origin.
    /// 只用国际下载源。
    International,
    /// Only the mainland China mirror.
    /// 只用中国大陆镜像。
    China,
}

/// How often the service checks for a newer version on its own.
/// 服务自行检查更新版本的频率。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppUpdateCheckInterval {
    /// Every hour.
    /// 每小时。
    OneHour,
    /// Every six hours.
    /// 每六小时。
    #[default]
    SixHours,
    /// Every twelve hours.
    /// 每十二小时。
    TwelveHours,
    /// Once a day.
    /// 每天一次。
    OneDay,
}

/// The user's update settings; defaults are applied in memory and never written back.
/// 用户的更新设置；默认值只在内存中补齐，绝不回写。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppUpdateSettings {
    /// Whether preview releases are offered as well; off by default.
    /// 是否同时接收预览版；默认关闭。
    pub preview: bool,
    /// Download origins.
    /// 下载源。
    pub route: AppUpdateRouteSetting,
    /// How often the service checks on its own.
    /// 服务自行检查的频率。
    pub check_interval: AppUpdateCheckInterval,
}

/// Why this process cannot update itself.
/// 本进程无法自行更新的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppUpdateUnavailableReason {
    /// A development build, which only the repository updates.
    /// 开发构建，只能通过仓库更新。
    DevelopmentBuild,
    /// Running outside an installed `versions/<version>` layout, for example from `output-release/`.
    /// 运行于已安装的 `versions/<版本>` 布局之外，例如 `output-release/`。
    NotInstalled,
    /// The installed version has no install record to compare against.
    /// 已安装版本没有可供比对的安装记录。
    InstallRecordMissing,
    /// No release is published for this operating system and architecture.
    /// 没有为本操作系统与架构发布的版本。
    UnsupportedPlatform,
}

/// How an offered version is installed.
/// 所提供版本的安装方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppUpdateKind {
    /// Only changed files are downloaded and the new version is assembled next to the current one.
    /// 只下载变化的文件，并在当前版本旁组装新版本。
    Incremental,
    /// The install epoch changed, so the complete installer must run.
    /// 安装纪元已变化，必须运行完整安装包。
    Full,
    /// The install epoch changed on a root an installer made: only changed files are downloaded
    /// and the new version is assembled next to the current one, then on restart that installer
    /// (the setup program, or the Inno update package on a Windows root the Inno installer made)
    /// switches to it and refreshes what it installed outside the version directory.
    /// 由安装程序创建的根目录上安装纪元已变化：只下载变化的文件并在当前版本旁组装新版本，重启时由该安装
    /// 程序（安装器；Inno 安装包创建的 Windows 根目录上为 Inno 更新包）切换到它并刷新其在版本目录之外
    /// 安装的内容。
    Setup,
}

/// A verified newer release.
/// 已校验的更新版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateOffer {
    /// The offered version.
    /// 所提供的版本。
    pub version: String,
    /// `stable` or `preview`.
    /// `stable` 或 `preview`。
    pub channel: String,
    /// How it is installed.
    /// 安装方式。
    pub kind: AppUpdateKind,
    /// Bytes to download: the changed contents, or the installer.
    /// 需下载的字节数：变化的内容或安装包。
    pub download_bytes: u64,
    /// Release time as `YYYY-MM-DDTHH:MM:SSZ`.
    /// 发布时间，格式为 `YYYY-MM-DDTHH:MM:SSZ`。
    pub released_at: String,
    /// Markdown release notes by language code; empty when the release has none.
    /// 按语言代码列出的 Markdown 更新说明；版本没有说明时为空。
    pub notes: std::collections::BTreeMap<String, String>,
    /// Notes of the releases between the running version and this one that have any, newest first.
    /// 正在运行版本与此版本之间有说明的各发布的说明，从新到旧排列。
    #[serde(default)]
    pub skipped_releases: Vec<AppUpdateReleaseNotes>,
    /// Whether `skippedReleases` reaches back to the running version; false when older releases name
    /// no predecessor or one could not be read, so earlier notes may be missing.
    /// `skippedReleases` 是否回溯到正在运行版本；较早的发布未写明上一版本或某个发布无法读取时为 false，
    /// 此时更早的说明可能缺失。
    #[serde(default)]
    pub skipped_releases_complete: bool,
    /// Whether the app can install it by itself; otherwise the download page is offered.
    /// 应用能否自行安装；不能时提供下载页。
    pub installable: bool,
    /// Whether the setup program owns this installation; an update the app cannot install then
    /// comes from `vulcan-code-setup update` instead of the download page.
    /// 此安装是否由安装器拥有；此时应用无法自行安装的更新改由 `vulcan-code-setup update` 完成，
    /// 不提供下载页。
    #[serde(default)]
    pub setup_managed: bool,
    /// Whether this version already failed to start here and was switched back.
    /// 此版本是否曾在本机启动失败并被切换回去。
    pub previously_failed: bool,
}

/// The release notes of one release an update skips.
/// 一次更新所跳过的某个发布的更新说明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateReleaseNotes {
    /// The release version.
    /// 发布版本。
    pub version: String,
    /// Release date as `YYYY-MM-DD`, when known.
    /// 发布日期，格式为 `YYYY-MM-DD`（若已知）。
    pub date: Option<String>,
    /// Markdown release notes by language code.
    /// 按语言代码列出的 Markdown 更新说明。
    pub notes: std::collections::BTreeMap<String, String>,
}

/// What the updater is doing.
/// 更新器正在进行的工作。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppUpdateActivity {
    /// Nothing.
    /// 空闲。
    #[default]
    Idle,
    /// Reading the channel pointers and the manifest.
    /// 正在读取通道指针与版本清单。
    Checking,
    /// Downloading and assembling the offered version.
    /// 正在下载并组装所提供的版本。
    Downloading,
    /// Checking and restoring the installed files.
    /// 正在检查并恢复已安装的文件。
    Repairing,
    /// Stopping so that the new version can start.
    /// 正在停止，以便新版本启动。
    Restarting,
}

/// Which operation failed.
/// 失败的操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppUpdateOperation {
    /// Checking for a newer version.
    /// 检查新版本。
    Check,
    /// Downloading or assembling.
    /// 下载或组装。
    Download,
    /// Repairing the installed version.
    /// 修复已安装版本。
    Repair,
    /// Restarting or starting the installer.
    /// 重启或启动安装包。
    Restart,
}

/// The last failure, in English (error messages stay out of the catalogs).
/// 最近一次失败，英文描述（错误消息不进入目录）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateError {
    /// The operation that failed.
    /// 失败的操作。
    pub operation: AppUpdateOperation,
    /// Diagnostic text.
    /// 诊断文本。
    pub message: String,
}

/// Bytes transferred by the running download or repair.
/// 进行中的下载或修复已传输的字节数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateProgress {
    /// Bytes received.
    /// 已接收的字节数。
    pub received_bytes: u64,
    /// Bytes expected in total.
    /// 预计总字节数。
    pub total_bytes: u64,
}

/// A version the launcher switched away from because it failed to start.
/// 启动器因启动失败而切换离开的版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateRollback {
    /// The version that did not start.
    /// 未能启动的版本。
    pub failed: String,
    /// The version running instead.
    /// 改为运行的版本。
    pub restored: String,
}

/// How the AppArmor profile the Linux desktop app needs differs from what is installed.
/// Linux 桌面应用所需的 AppArmor 配置与已安装内容的差异。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppUpdateDesktopProfileStatus {
    /// No profile is installed.
    /// 未安装配置。
    Missing,
    /// A profile of Vulcan Code with other content is installed, for example for another root.
    /// 已安装内容不同的 Vulcan Code 配置，例如属于另一个安装根目录。
    Outdated,
}

/// The AppArmor profile without which the Linux desktop app stops when started from the dock or
/// the app menu, on systems that restrict unprivileged user namespaces (Ubuntu 23.10 and later).
/// 在限制非特权用户命名空间的系统（Ubuntu 23.10 及以后）上，缺少即会导致 Linux 桌面应用从程序坞或应用
/// 菜单启动时停止的 AppArmor 配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateDesktopProfile {
    /// What is wrong.
    /// 问题所在。
    pub status: AppUpdateDesktopProfileStatus,
    /// Whether the installation waits for the administrator password.
    /// 安装是否正在等待管理员密码。
    pub installing: bool,
    /// A shell line that installs the profile by hand with `sudo`.
    /// 以 `sudo` 手动安装配置的 Shell 命令行。
    pub command: String,
    /// Why the last installation failed, in English.
    /// 上次安装失败的原因，英文描述。
    pub error: Option<String>,
}

/// Everything the update settings and prompts show.
/// 更新设置与提示所显示的全部内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateSnapshot {
    /// The running version.
    /// 正在运行的版本。
    pub current_version: String,
    /// Why updating is off; `None` when the app can update itself.
    /// 更新关闭的原因；应用可以自行更新时为 `None`。
    pub unavailable_reason: Option<AppUpdateUnavailableReason>,
    /// The effective settings.
    /// 生效的设置。
    pub settings: AppUpdateSettings,
    /// Current work.
    /// 当前工作。
    pub activity: AppUpdateActivity,
    /// Progress of a download or repair.
    /// 下载或修复的进度。
    pub progress: Option<AppUpdateProgress>,
    /// When the last check finished, in milliseconds since the Unix epoch.
    /// 上次检查完成的时间，自 Unix 纪元起的毫秒数。
    pub last_checked_at: Option<u64>,
    /// The newer version found by the last check.
    /// 上次检查找到的更新版本。
    pub offer: Option<AppUpdateOffer>,
    /// A version installed and made current, waiting for a restart.
    /// 已安装并设为当前、等待重启的版本。
    pub ready_version: Option<String>,
    /// Files restored by the last repair.
    /// 上次修复恢复的文件数。
    pub repaired_files: Option<u64>,
    /// The last failure.
    /// 最近一次失败。
    pub error: Option<AppUpdateError>,
    /// A failed start the launcher switched back from, until dismissed.
    /// 启动器切换回去的一次启动失败，直到被关闭提示。
    pub rollback: Option<AppUpdateRollback>,
    /// The AppArmor profile the desktop app still needs on this system; `None` when it needs none.
    /// 桌面应用在本系统上仍需要的 AppArmor 配置；不需要时为 `None`。
    #[serde(default)]
    pub desktop_profile: Option<AppUpdateDesktopProfile>,
    /// Page for downloading an installer by hand.
    /// 手动下载安装包的页面。
    pub download_page_url: String,
}

/// Why the service is about to stop.
/// 服务即将停止的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceRestartReason {
    /// A new version is current; the service starts again from it.
    /// 新版本已成为当前版本；服务从它重新启动。
    Update,
    /// The full installer, or the Inno update package over an assembled version, runs and starts
    /// the app when it finishes.
    /// 完整安装包（或在已组装版本上运行的 Inno 更新包）运行，结束后启动应用。
    Installer,
    /// The setup program finishes the assembled version once the service stopped, then the service
    /// starts again from the current version; clients wait for that service.
    /// 服务停止后由安装器完成已组装的版本，然后服务从当前版本重新启动；客户端等待该服务。
    Setup,
}

/// Sent to every client right before the service stops for an update.
/// 服务因更新停止前发给每个客户端。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRestartingEvent {
    /// Why it stops.
    /// 停止原因。
    pub reason: ServiceRestartReason,
    /// The version that starts next.
    /// 接下来启动的版本。
    pub version: String,
}
