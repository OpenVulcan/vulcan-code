import * as React from "react";

import { useAppUpdate, releaseNotesFor } from "@/components/features/app-update/use-app-update";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { PageNotice } from "@/components/page";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import { errorMessage, formatBytes } from "@/lib/display-format";
import type {
  AppUpdateOperation,
  AppUpdateRoute,
  AppUpdateSettings,
  AppUpdateSnapshot,
  AppUpdateUnavailableReason,
} from "@/service/app-update-events";
import {
  cancelAppUpdate,
  checkAppUpdate,
  dismissAppUpdateNotice,
  downloadAppUpdate,
  repairAppInstall,
  restartToUpdate,
  saveAppUpdateSettings,
} from "@/service/app-update-service";
import { openExternalUrl } from "@/service/project-actions-service";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { Progress } from "@/ui-kit/primitives/progress";
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

// Outcome lines of failed operations.
// 失败操作的结果说明。
const FAILURE_KEYS: Record<AppUpdateOperation, TranslationKey> = {
  check: "settings.appUpdate.failed.check",
  download: "settings.appUpdate.failed.download",
  repair: "settings.appUpdate.failed.repair",
  restart: "settings.appUpdate.failed.restart",
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
export function AppUpdateSettingsArea() {
  const { t, locale } = useI18n();
  const { snapshot, loadError, setSnapshot } = useAppUpdate();
  const [callError, setCallError] = React.useState("");

  // run sends one updater call and shows its accepted snapshot; failures stay on the page.
  // run 发送一个更新器调用并显示其已接受的快照；失败信息保留在页面上。
  const run = async (call: () => Promise<AppUpdateSnapshot>) => {
    setCallError("");
    try {
      setSnapshot(await call());
    } catch (cause) {
      setCallError(errorMessage(cause));
    }
  };

  if (!snapshot) {
    return loadError
      ? <SettingsErrorNotice detail={loadError} message={t("settings.appUpdate.loadFailed")} />
      : <p role="status" className="vk-form-note">{t("settings.appUpdate.loading")}</p>;
  }

  const busy = snapshot.activity !== "idle";
  const offer = snapshot.offer;
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

    {snapshot.error ? <SettingsErrorNotice detail={snapshot.error.message} message={t(FAILURE_KEYS[snapshot.error.operation])} /> : null}
    {callError ? <SettingsErrorNotice detail={callError} message={t("settings.appUpdate.requestFailed")} /> : null}

    {snapshot.readyVersion ? (
      <section className="vk-form-pref" data-app-update-ready>
        <div className="vk-form-pref-text">
          <strong>{formatMessage(t("settings.appUpdate.ready"), { version: snapshot.readyVersion })}</strong>
          <p className="vk-form-note">{t("settings.appUpdate.readyHint")}</p>
        </div>
        <Button disabled={busy} onClick={() => void run(restartToUpdate)} size="sm">
          {snapshot.activity === "restarting" ? t("settings.appUpdate.restarting") : t("settings.appUpdate.restart")}
        </Button>
      </section>
    ) : null}

    {offer ? (
      <section className="vk-form-pref" data-app-update-offer>
        <div className="vk-form-pref-text">
          <strong>
            {formatMessage(t("settings.appUpdate.available"), { version: offer.version })}{" "}
            {offer.channel === "preview" ? <Badge variant="warning">{t("settings.appUpdate.preview")}</Badge> : null}
          </strong>
          <p className="vk-form-note">
            {offer.kind === "incremental"
              ? formatMessage(t("settings.appUpdate.incremental"), { size: formatBytes(offer.downloadBytes) })
              : offer.installable
                ? formatMessage(t("settings.appUpdate.full"), { size: formatBytes(offer.downloadBytes) })
                : t("settings.appUpdate.fullManual")}
          </p>
          {offer.previouslyFailed ? <p role="alert" className="vk-field-error">{t("settings.appUpdate.previouslyFailed")}</p> : null}
        </div>
        {offer.installable ? (
          <Button disabled={busy} onClick={() => void run(downloadAppUpdate)} size="sm">{t("settings.appUpdate.download")}</Button>
        ) : (
          <Button onClick={() => void openExternalUrl(snapshot.downloadPageUrl).catch((cause: unknown) => setCallError(errorMessage(cause)))} size="sm" variant="outline">
            {t("settings.appUpdate.openDownloadPage")}
          </Button>
        )}
        {releaseNotesFor(offer.notes, locale) ? <MarkdownMessage content={releaseNotesFor(offer.notes, locale)} eager /> : null}
      </section>
    ) : null}

    {snapshot.progress ? (
      <section className="vk-form-pref" data-app-update-progress>
        <div className="vk-form-pref-text">
          <strong>{snapshot.activity === "repairing" ? t("settings.appUpdate.repairing") : t("settings.appUpdate.downloading")}</strong>
          <p className="vk-form-note">{formatBytes(snapshot.progress.receivedBytes)} / {formatBytes(snapshot.progress.totalBytes)}</p>
          <Progress max={Math.max(snapshot.progress.totalBytes, 1)} value={snapshot.progress.receivedBytes} />
        </div>
        <Button onClick={() => void run(cancelAppUpdate)} size="sm" variant="ghost">{t("settings.appUpdate.cancel")}</Button>
      </section>
    ) : null}

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
