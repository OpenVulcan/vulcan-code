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

/** The user's update settings. / 用户的更新设置。 */
export interface AppUpdateSettings {
  preview: boolean;
  route: AppUpdateRoute;
}

/** A verified newer release. / 已校验的更新版本。 */
export interface AppUpdateOffer {
  version: string;
  channel: string;
  // incremental downloads only changed files; full runs the installer.
  // incremental 只下载变化的文件；full 运行安装包。
  kind: "incremental" | "full";
  downloadBytes: number;
  releasedAt: string;
  // Release notes by locale code, Markdown.
  // 按语言代码索引的发布说明，Markdown 格式。
  notes: Record<string, string>;
  // False when this version must be installed from the download page.
  // 此版本必须从下载页面安装时为 false。
  installable: boolean;
  // True when this version already failed to start once and was rolled back.
  // 此版本曾启动失败并被回退时为 true。
  previouslyFailed: boolean;
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
  downloadPageUrl: string;
}

/** Announcement that the Core stops for an update. / Core 为更新而停止的通告。 */
export interface ServiceRestartingEvent {
  reason: "update" | "installer";
  version: string;
}

