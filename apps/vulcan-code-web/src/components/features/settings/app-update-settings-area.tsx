import { useAppUpdate } from "@/components/features/app-update/use-app-update";
import {
  AppUpdateDesktopProfileNotice,
  AppUpdateErrorNotices,
  AppUpdateOfferRows,
  AppUpdateProgressRow,
  AppUpdateReleaseList,
  useAppUpdateCalls,
} from "@/components/features/app-update/app-update-sections";
import { PageBody, PageNotice, PageRow, PageSection } from "@/components/page";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import type { AppUpdateCheckInterval, AppUpdateRoute, AppUpdateSettings, AppUpdateUnavailableReason } from "@/service/app-update-events";
import { formatBytes } from "@/lib/display-format";
import { cancelAppUpdate, checkAppUpdate, dismissAppUpdateNotice, repairAppInstall, saveAppUpdateSettings } from "@/service/app-update-service";
import { Button } from "@/ui-kit/primitives/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { Switch } from "@/ui-kit/primitives/switch";
import "@/ui-kit/styles/features/settings-forms.css";

// Labels of the reasons updating is off.
// 更新关闭原因的标签。
const UNAVAILABLE_KEYS: Record<AppUpdateUnavailableReason, TranslationKey> = {
  developmentBuild: "settings.appUpdate.unavailable.developmentBuild",
  notInstalled: "settings.appUpdate.unavailable.notInstalled",
  installRecordMissing: "settings.appUpdate.unavailable.installRecordMissing",
  unsupportedPlatform: "settings.appUpdate.unavailable.unsupportedPlatform",
};

// Labels of the download routes.
// 下载线路的标签。
const ROUTE_KEYS: Record<AppUpdateRoute, TranslationKey> = {
  auto: "settings.appUpdate.route.auto",
  international: "settings.appUpdate.route.international",
  china: "settings.appUpdate.route.china",
};

// Labels of the automatic check intervals, shortest first.
// 自动检查间隔的标签，从短到长。
const CHECK_INTERVAL_KEYS: Record<AppUpdateCheckInterval, TranslationKey> = {
  oneHour: "settings.appUpdate.checkInterval.oneHour",
  sixHours: "settings.appUpdate.checkInterval.sixHours",
  twelveHours: "settings.appUpdate.checkInterval.twelveHours",
  oneDay: "settings.appUpdate.checkInterval.oneDay",
};

// AppUpdateSettingsArea shows, top to bottom, the running version with its check and repair actions,
// the update settings, and last the update found with the notes of every version it brings.
// AppUpdateSettingsArea 自上而下显示：正在运行的版本及其检查与修复操作、更新设置，最后是找到的更新及其带来
// 的每个版本的说明。
// Parameters: serviceInstanceEpoch reads the state again after a reconnect, such as the restart into a
// new version.
// 参数：serviceInstanceEpoch 在重连（例如重启进入新版本）后重新读取状态。
export function AppUpdateSettingsArea(props: { serviceInstanceEpoch: number }) {
  const { t, locale } = useI18n();
  const { snapshot, loadError, setSnapshot } = useAppUpdate(props.serviceInstanceEpoch);
  const calls = useAppUpdateCalls(setSnapshot);
  const { run } = calls;

  if (!snapshot) {
    return loadError
      ? <SettingsErrorNotice detail={loadError} message={t("settings.appUpdate.loadFailed")} />
      : <p role="status" className="vk-form-note">{t("settings.appUpdate.loading")}</p>;
  }

  const busy = snapshot.activity !== "idle";
  const available = snapshot.unavailableReason === null;
  const saveSettings = (settings: AppUpdateSettings) => void run(() => saveAppUpdateSettings(settings));
  const lastChecked = snapshot.lastCheckedAt === null
    ? t("settings.appUpdate.neverChecked")
    : new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(new Date(snapshot.lastCheckedAt));
  // A repair reports its progress beside the repair action; a download beside the version it fetches.
  // 修复在修复操作旁报告进度；下载在其获取的版本旁报告进度。
  const repairing = snapshot.activity === "repairing";
  const showsUpdate = snapshot.offer !== null || snapshot.readyVersion !== null || (snapshot.progress !== null && !repairing);

  return <PageBody>
    <PageSection
      data-app-update-current
      layout="rows"
      title={<>{t("settings.appUpdate.currentVersion")} <span className="vk-mono">{snapshot.currentVersion}</span></>}
      description={available ? formatMessage(t("settings.appUpdate.lastChecked"), { time: lastChecked }) : t(UNAVAILABLE_KEYS[snapshot.unavailableReason!])}
      actions={available ? (
        <Button disabled={busy} onClick={() => void run(checkAppUpdate)} size="sm" variant="outline">
          {snapshot.activity === "checking" ? t("settings.appUpdate.checking") : t("settings.appUpdate.check")}
        </Button>
      ) : undefined}
    >
      {available ? (
        <PageRow data-app-update-repair label={t("settings.appUpdate.repair")} description={repairing && snapshot.progress
          ? `${t("settings.appUpdate.repairing")} · ${formatBytes(snapshot.progress.receivedBytes)} / ${formatBytes(snapshot.progress.totalBytes)}`
          : snapshot.repairedFiles === null ? t("settings.appUpdate.repairHint") : formatMessage(t("settings.appUpdate.repaired"), { count: snapshot.repairedFiles })}>
          {repairing
            ? <Button onClick={() => void run(cancelAppUpdate)} size="sm" variant="ghost">{t("settings.appUpdate.cancel")}</Button>
            : <Button disabled={busy} onClick={() => void run(repairAppInstall)} size="sm" variant="outline">{t("settings.appUpdate.repairAction")}</Button>}
        </PageRow>
      ) : null}
    </PageSection>

    {snapshot.rollback ? (
      <PageNotice tone="warning" title={formatMessage(t("settings.appUpdate.rollback.title"), { version: snapshot.rollback.failed })}
        actions={<Button onClick={() => void run(dismissAppUpdateNotice)} size="sm" variant="ghost">{t("settings.appUpdate.rollback.dismiss")}</Button>}>
        {formatMessage(t("settings.appUpdate.rollback.body"), { restored: snapshot.rollback.restored })}
      </PageNotice>
    ) : null}

    <AppUpdateErrorNotices calls={calls} snapshot={snapshot} />

    <AppUpdateDesktopProfileNotice calls={calls} snapshot={snapshot} />

    {available ? (
      <PageSection layout="rows" title={t("settings.appUpdate.settingsTitle")}>
        <PageRow label={t("settings.appUpdate.checkInterval")} description={t("settings.appUpdate.checkIntervalHint")}>
          <Select value={snapshot.settings.checkInterval}
            onValueChange={(checkInterval) => saveSettings({ ...snapshot.settings, checkInterval: checkInterval as AppUpdateCheckInterval })}>
            <SelectTrigger aria-label={t("settings.appUpdate.checkInterval")} className="vk-form-w-md"><SelectValue /></SelectTrigger>
            <SelectContent>
              {(Object.keys(CHECK_INTERVAL_KEYS) as AppUpdateCheckInterval[]).map((interval) => (
                <SelectItem key={interval} value={interval}>{t(CHECK_INTERVAL_KEYS[interval])}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </PageRow>
        <PageRow label={t("settings.appUpdate.previewChannel")} description={t("settings.appUpdate.previewChannelHint")}>
          <Switch aria-label={t("settings.appUpdate.previewChannel")} checked={snapshot.settings.preview}
            onCheckedChange={(preview) => saveSettings({ ...snapshot.settings, preview })} />
        </PageRow>
        <PageRow label={t("settings.appUpdate.route")} description={t("settings.appUpdate.routeHint")}>
          <Select value={snapshot.settings.route} onValueChange={(route) => saveSettings({ ...snapshot.settings, route: route as AppUpdateRoute })}>
            <SelectTrigger aria-label={t("settings.appUpdate.route")} className="vk-form-w-md"><SelectValue /></SelectTrigger>
            <SelectContent>
              {(Object.keys(ROUTE_KEYS) as AppUpdateRoute[]).map((route) => <SelectItem key={route} value={route}>{t(ROUTE_KEYS[route])}</SelectItem>)}
            </SelectContent>
          </Select>
        </PageRow>
      </PageSection>
    ) : null}

    {showsUpdate ? (
      <PageSection data-app-update-found title={t("settings.appUpdate.updateTitle")}>
        <AppUpdateOfferRows calls={calls} snapshot={snapshot} />
        {repairing ? null : <AppUpdateProgressRow calls={calls} snapshot={snapshot} />}
        {snapshot.offer ? <AppUpdateReleaseList currentVersion={snapshot.currentVersion} offer={snapshot.offer} version={snapshot.offer.version} /> : null}
      </PageSection>
    ) : null}
  </PageBody>;
}
