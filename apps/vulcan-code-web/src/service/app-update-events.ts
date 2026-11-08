// Wire shapes of the in-app updater (vulcan-service-api `app_update`), shared by the transport and the app.
// 应用内更新器的线协议形状（vulcan-service-api `app_update`），由传输层与应用共享。

/** Topic of changed update state.
 * 更新状态变化的主题。 */
export const APP_UPDATE_CHANGED_TOPIC = "app.update.changed";

/** Topic announcing that the Core stops to start an update.
 * 宣布 Core 停止以启动更新的主题。 */
export const SERVICE_RESTARTING_TOPIC = "service.restarting";

/** Download origins. / 下载源。 */
export type AppUpdateRoute = "auto" | "international" | "china";

/** Why this installation cannot update itself. / 本安装无法自行更新的原因。 */
export type AppUpdateUnavailableReason =
  | "developmentBuild"
  | "notInstalled"
  | "installRecordMissing"
  | "unsupportedPlatform";

/** Current work of the updater. / 更新器的当前工作。 */
export type AppUpdateActivity = "idle" | "checking" | "downloading" | "repairing" | "restarting";

/** The operation a failure belongs to. / 失败所属的操作。 */
export type AppUpdateOperation = "check" | "download" | "repair" | "restart";

/** How often the service checks on its own. / 服务自行检查的频率。 */
export type AppUpdateCheckInterval = "oneHour" | "sixHours" | "twelveHours" | "oneDay";

/** The user's update settings. / 用户的更新设置。 */
export interface AppUpdateSettings {
  preview: boolean;
  route: AppUpdateRoute;
  checkInterval: AppUpdateCheckInterval;
}

/** A verified newer release. / 已校验的更新版本。 */
/** Release notes of one release an update skips. / 一次更新所跳过的某个发布的说明。 */
export interface AppUpdateReleaseNotes {
  version: string;
  // Release date as YYYY-MM-DD, when known.
  // 发布日期，格式为 YYYY-MM-DD（若已知）。
  date: string | null;
  // Release notes by locale code, Markdown.
  // 按语言代码索引的发布说明，Markdown 格式。
  notes: Record<string, string>;
}

export interface AppUpdateOffer {
  version: string;
  channel: string;
  // incremental downloads only changed files; setup does too and the setup program finishes it on
  // restart; full runs the installer.
  // incremental 只下载变化的文件；setup 同样如此，并在重启时由安装器完成；full 运行安装包。
  kind: "incremental" | "setup" | "full";
  downloadBytes: number;
  releasedAt: string;
  // Release notes by locale code, Markdown.
  // 按语言代码索引的发布说明，Markdown 格式。
  notes: Record<string, string>;
  // Notes of the releases between the running version and this one, newest first.
  // 正在运行版本与此版本之间各发布的说明，从新到旧排列。
  skippedReleases: AppUpdateReleaseNotes[];
  // False when earlier skipped releases could not be traced, so their notes may be missing.
  // 无法追溯更早的被跳过版本时为 false，其说明可能缺失。
  skippedReleasesComplete: boolean;
  // False when this version must be installed from the download page.
  // 此版本必须从下载页面安装时为 false。
  installable: boolean;
  // True when the setup program owns this installation; an offer it cannot install then points to
  // `vulcan-code-setup update` and offers no download page.
  // 此安装由安装器拥有时为 true；无法自行安装的提议改为提示 `vulcan-code-setup update`，不提供下载页。
  setupManaged: boolean;
  // True when this version already failed to start once and was rolled back.
  // 此版本曾启动失败并被回退时为 true。
  previouslyFailed: boolean;
}

/** The AppArmor profile the Linux desktop app still needs. / Linux 桌面应用仍需要的 AppArmor 配置。 */
export interface AppUpdateDesktopProfile {
  // missing: none is installed; outdated: one of Vulcan Code with other content, such as for another location.
  // missing：未安装；outdated：已安装内容不同的 Vulcan Code 配置，例如属于另一个位置。
  status: "missing" | "outdated";
  // True while the system password prompt is open.
  // 系统密码框打开期间为 true。
  installing: boolean;
  // A shell line that installs the profile by hand with sudo.
  // 以 sudo 手动安装配置的 Shell 命令行。
  command: string;
  // Why the last installation failed, in English.
  // 上次安装失败的原因，英文描述。
  error: string | null;
}

/** Complete updater state. / 完整的更新器状态。 */
export interface AppUpdateSnapshot {
  currentVersion: string;
  unavailableReason: AppUpdateUnavailableReason | null;
  settings: AppUpdateSettings;
  activity: AppUpdateActivity;
  progress: { receivedBytes: number; totalBytes: number } | null;
  lastCheckedAt: number | null;
  offer: AppUpdateOffer | null;
  readyVersion: string | null;
  repairedFiles: number | null;
  error: { operation: AppUpdateOperation; message: string } | null;
  rollback: { failed: string; restored: string } | null;
  // Null when the desktop app needs no AppArmor profile here.
  // 桌面应用在此不需要 AppArmor 配置时为 null。
  desktopProfile: AppUpdateDesktopProfile | null;
  downloadPageUrl: string;
}

// Why the Core stops: a new current version, the full installer, or the setup program finishing an
// assembled version.
// Core 停止的原因：新的当前版本、完整安装包，或由安装器完成已组装的版本。
export type ServiceRestartReason = "update" | "installer" | "setup";

/** Announcement that the Core stops for an update. / Core 为更新而停止的通告。 */
export interface ServiceRestartingEvent {
  reason: ServiceRestartReason;
  version: string;
}

