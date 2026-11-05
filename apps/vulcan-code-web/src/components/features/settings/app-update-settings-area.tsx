import { useAppUpdate } from "@/components/features/app-update/use-app-update";
import { AppUpdateStatusSections, useAppUpdateCalls } from "@/components/features/app-update/app-update-sections";
import { PageNotice } from "@/components/page";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import type { AppUpdateRoute, AppUpdateSettings, AppUpdateUnavailableReason } from "@/service/app-update-events";
import { checkAppUpdate, dismissAppUpdateNotice, repairAppInstall, saveAppUpdateSettings } from "@/service/app-update-service";
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

// AppUpdateSettingsArea shows the running version and every update action: check, download,
// restart, repair, the rollback notice and the update settings.
// AppUpdateSettingsArea 显示正在运行的版本及全部更新操作：检查、下载、重启、修复、回退提示与更新设置。
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
  const saveSettings = (settings: AppUpdateSettings) => void run(() => saveAppUpdateSettings(settings));
  const lastChecked = snapshot.lastCheckedAt === null
    ? t("settings.appUpdate.neverChecked")
    : new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(new Date(snapshot.lastCheckedAt));

  return <>
    <section className="vk-form-pref" data-app-update-current>
      <div className="vk-form-pref-text">
        <strong>{t("settings.appUpdate.currentVersion")} <span className="vk-mono">{snapshot.currentVersion}</span></strong>
        <p className="vk-form-note">
          {snapshot.unavailableReason ? t(UNAVAILABLE_KEYS[snapshot.unavailableReason]) : formatMessage(t("settings.appUpdate.lastChecked"), { time: lastChecked })}
        </p>
      </div>
      {snapshot.unavailableReason ? null : (
        <Button disabled={busy} onClick={() => void run(checkAppUpdate)} size="sm" variant="outline">
          {snapshot.activity === "checking" ? t("settings.appUpdate.checking") : t("settings.appUpdate.check")}
        </Button>
      )}
    </section>

    {snapshot.rollback ? (
      <PageNotice tone="warning" title={formatMessage(t("settings.appUpdate.rollback.title"), { version: snapshot.rollback.failed })}
        actions={<Button onClick={() => void run(dismissAppUpdateNotice)} size="sm" variant="ghost">{t("settings.appUpdate.rollback.dismiss")}</Button>}>
        {formatMessage(t("settings.appUpdate.rollback.body"), { restored: snapshot.rollback.restored })}
      </PageNotice>
    ) : null}

    <AppUpdateStatusSections calls={calls} showNotes snapshot={snapshot} />

    {snapshot.unavailableReason ? null : <>
      <section className="vk-form-pref">
        <div className="vk-form-pref-text">
          <strong>{t("settings.appUpdate.previewChannel")}</strong>
          <p className="vk-form-note">{t("settings.appUpdate.previewChannelHint")}</p>
        </div>
        <Switch aria-label={t("settings.appUpdate.previewChannel")} checked={snapshot.settings.preview}
          onCheckedChange={(preview) => saveSettings({ ...snapshot.settings, preview })} />
      </section>
      <section className="vk-form-pref">
        <div className="vk-form-pref-text">
          <strong>{t("settings.appUpdate.route")}</strong>
          <p className="vk-form-note">{t("settings.appUpdate.routeHint")}</p>
        </div>
        <Select value={snapshot.settings.route} onValueChange={(route) => saveSettings({ ...snapshot.settings, route: route as AppUpdateRoute })}>
          <SelectTrigger aria-label={t("settings.appUpdate.route")} className="vk-form-w-md"><SelectValue /></SelectTrigger>
          <SelectContent>
            {(Object.keys(ROUTE_KEYS) as AppUpdateRoute[]).map((route) => <SelectItem key={route} value={route}>{t(ROUTE_KEYS[route])}</SelectItem>)}
          </SelectContent>
        </Select>
      </section>
      <section className="vk-form-pref" data-app-update-repair>
        <div className="vk-form-pref-text">
          <strong>{t("settings.appUpdate.repair")}</strong>
          <p className="vk-form-note">
            {snapshot.repairedFiles === null ? t("settings.appUpdate.repairHint") : formatMessage(t("settings.appUpdate.repaired"), { count: snapshot.repairedFiles })}
          </p>
        </div>
        <Button disabled={busy} onClick={() => void run(repairAppInstall)} size="sm" variant="outline">{t("settings.appUpdate.repairAction")}</Button>
      </section>
    </>}
  </>;
}
