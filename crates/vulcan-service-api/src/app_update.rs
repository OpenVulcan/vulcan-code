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
    /// Whether the app can install it by itself; otherwise the download page is offered.
    /// 应用能否自行安装；不能时提供下载页。
    pub installable: bool,
    /// Whether this version already failed to start here and was switched back.
    /// 此版本是否曾在本机启动失败并被切换回去。
    pub previously_failed: bool,
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
    /// The full installer runs and starts the app when it finishes.
    /// 完整安装包运行，结束后启动应用。
    Installer,
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
