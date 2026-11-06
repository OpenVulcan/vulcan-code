import * as React from "react";

import { releaseNotesFor } from "@/components/features/app-update/use-app-update";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { PageNotice } from "@/components/page";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import { errorMessage, formatBytes } from "@/lib/display-format";
import type { AppUpdateOffer, AppUpdateOperation, AppUpdateSnapshot } from "@/service/app-update-events";
import { cancelAppUpdate, downloadAppUpdate, installDesktopProfile, restartToUpdate } from "@/service/app-update-service";
import { openExternalUrl } from "@/service/project-actions-service";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { Progress } from "@/ui-kit/primitives/progress";
import "@/ui-kit/styles/features/settings-forms.css";
import "@/ui-kit/styles/features/app-update-notes.css";

// Outcome lines of failed operations.
// 失败操作的结果说明。
const FAILURE_KEYS: Record<AppUpdateOperation, TranslationKey> = {
  check: "settings.appUpdate.failed.check",
  download: "settings.appUpdate.failed.download",
  repair: "settings.appUpdate.failed.repair",
  restart: "settings.appUpdate.failed.restart",
};

// AppUpdateCalls sends updater calls for one surface and keeps the failure of the last one.
// AppUpdateCalls 为一个界面发送更新器调用，并保留最近一次调用的失败原因。
export interface AppUpdateCalls {
  // Sends one call and stores its accepted snapshot; a failure lands in callError.
  // 发送一个调用并保存其已接受的快照；失败写入 callError。
  run: (call: () => Promise<AppUpdateSnapshot>) => Promise<void>;
  // Failure of the last call that did not reach the service; empty when none.
  // 最近一次未到达服务的调用的失败原因；没有时为空。
  callError: string;
  // Reports a failure of a related action, such as opening the download page.
  // 报告相关操作的失败，例如打开下载页面。
  setCallError: (message: string) => void;
}

// useAppUpdateCalls sends updater calls and hands each accepted snapshot to setSnapshot.
// useAppUpdateCalls 发送更新器调用，并把每个已接受的快照交给 setSnapshot。
// Parameters: setSnapshot stores the snapshot a call returned.
// 参数：setSnapshot 保存调用返回的快照。
export function useAppUpdateCalls(setSnapshot: (snapshot: AppUpdateSnapshot) => void): AppUpdateCalls {
  const [callError, setCallError] = React.useState("");
  const run = React.useCallback(async (call: () => Promise<AppUpdateSnapshot>) => {
    setCallError("");
    try {
      setSnapshot(await call());
    } catch (cause) {
      setCallError(errorMessage(cause));
    }
  }, [setSnapshot]);
  return { run, callError, setCallError };
}

// AppUpdateReleaseNotes renders release notes in the interface language with the answer renderer.
// AppUpdateReleaseNotes 以回答渲染器按界面语言渲染发布说明。
// Parameters: notes are the Markdown notes by locale code. Renders nothing when there are none.
// 参数：notes 为按语言代码索引的 Markdown 说明。没有说明时不渲染任何内容。
export function AppUpdateReleaseNotes(props: { notes: Record<string, string> }) {
  const { locale } = useI18n();
  const content = releaseNotesFor(props.notes, locale);
  return content ? <MarkdownMessage content={content} eager /> : null;
}

// AppUpdateReleaseList shows the notes of every version an update brings: the offered version first, then
// each skipped one, newest first, so an update across several versions reads in order.
// AppUpdateReleaseList 显示一次更新带来的每个版本的说明：先是提议的版本，再依次是每个被跳过的版本，
// 从新到旧，使跨越多个版本的更新可以按顺序阅读。
// Parameters: version is the version updated to; offer holds its notes and the skipped releases, or null
// once they are no longer known; currentVersion is the running version.
// 参数：version 为要更新到的版本；offer 含其说明与被跳过的发布，已不可知时为 null；currentVersion 为
// 正在运行的版本。
export function AppUpdateReleaseList(props: { version: string; offer: AppUpdateOffer | null; currentVersion: string }) {
  const { t, locale } = useI18n();
  const releases = [{ version: props.version, notes: props.offer?.notes ?? {} }, ...(props.offer?.skippedReleases ?? [])];
  return <>
    {releases.map((release) => (
      <section className="vk-app-update-notes" data-app-update-notes={release.version} key={release.version}>
        <h3>{formatMessage(t("appUpdate.dialog.notesTitle"), { version: release.version })}</h3>
        {releaseNotesFor(release.notes, locale) !== ""
          ? <AppUpdateReleaseNotes notes={release.notes} />
          : <p className="vk-form-note">{t("appUpdate.dialog.noNotes")}</p>}
      </section>
    ))}
    {props.offer && !props.offer.skippedReleasesComplete ? (
      <p className="vk-form-note" data-app-update-notes-incomplete="">
        {formatMessage(t("appUpdate.dialog.earlierNotesMissing"), { current: props.currentVersion })}
      </p>
    ) : null}
  </>;
}

// AppUpdateErrorNotices shows the failure of the last operation and of the last call.
// AppUpdateErrorNotices 显示最近一次操作与最近一次调用的失败。
// Parameters: snapshot is the latest updater state; calls holds the call failure.
// 参数：snapshot 为最新的更新器状态；calls 持有调用失败。
export function AppUpdateErrorNotices(props: { snapshot: AppUpdateSnapshot; calls: AppUpdateCalls }) {
  const { t } = useI18n();
  const { snapshot, calls } = props;
  return <>
    {snapshot.error ? <SettingsErrorNotice detail={snapshot.error.message} message={t(FAILURE_KEYS[snapshot.error.operation])} /> : null}
    {calls.callError ? <SettingsErrorNotice detail={calls.callError} message={t("settings.appUpdate.requestFailed")} /> : null}
  </>;
}

// AppUpdateDesktopProfileNotice explains the AppArmor profile the Linux desktop app still needs and installs
// it through the system password prompt, with the sudo line for a terminal as the alternative; nothing
// otherwise.
// AppUpdateDesktopProfileNotice 说明 Linux 桌面应用仍需要的 AppArmor 配置，并经系统密码框安装它，同时给出
// 可在终端运行的 sudo 命令作为替代；否则不渲染。
// Parameters: snapshot is the latest updater state; calls sends the install.
// 参数：snapshot 为最新的更新器状态；calls 发送安装。
export function AppUpdateDesktopProfileNotice(props: { snapshot: AppUpdateSnapshot; calls: AppUpdateCalls }) {
  const { t } = useI18n();
  const profile = props.snapshot.desktopProfile;
  if (!profile) return null;
  return <>
    <PageNotice
      data-app-update-desktop-profile=""
      tone="warning"
      title={t(profile.status === "outdated" ? "settings.appUpdate.desktopProfile.outdatedTitle" : "settings.appUpdate.desktopProfile.title")}
      details={profile.command}
      detailsLabel={t("settings.appUpdate.desktopProfile.manual")}
      actions={
        <Button disabled={profile.installing} onClick={() => void props.calls.run(installDesktopProfile)} size="sm">
          {profile.installing ? t("settings.appUpdate.desktopProfile.installing") : t("settings.appUpdate.desktopProfile.install")}
        </Button>
      }
    >
      {t("settings.appUpdate.desktopProfile.body")}
    </PageNotice>
    {profile.error ? <SettingsErrorNotice detail={profile.error} message={t("settings.appUpdate.desktopProfile.failed")} /> : null}
  </>;
}

// AppUpdateProgressRow shows a running download or repair with its cancel action; nothing otherwise.
// AppUpdateProgressRow 显示进行中的下载或修复及其取消操作；否则不渲染。
// Parameters: snapshot is the latest updater state; calls sends the cancel.
// 参数：snapshot 为最新的更新器状态；calls 发送取消。
export function AppUpdateProgressRow(props: { snapshot: AppUpdateSnapshot; calls: AppUpdateCalls }) {
  const { t } = useI18n();
  const { snapshot, calls } = props;
  if (!snapshot.progress) return null;
  return (
    <section className="vk-form-pref" data-app-update-progress>
      <div className="vk-form-pref-text">
        <strong>{snapshot.activity === "repairing" ? t("settings.appUpdate.repairing") : t("settings.appUpdate.downloading")}</strong>
        <p className="vk-form-note">{formatBytes(snapshot.progress.receivedBytes)} / {formatBytes(snapshot.progress.totalBytes)}</p>
        <Progress max={Math.max(snapshot.progress.totalBytes, 1)} value={snapshot.progress.receivedBytes} />
      </div>
      <Button onClick={() => void calls.run(cancelAppUpdate)} size="sm" variant="ghost">{t("settings.appUpdate.cancel")}</Button>
    </section>
  );
}

// AppUpdateOfferRows shows the version ready to start and the offered version with its download action.
// AppUpdateOfferRows 显示已就绪待启动的版本，以及提议版本及其下载操作。
// Parameters: snapshot is the latest updater state; calls sends the actions.
// 参数：snapshot 为最新的更新器状态；calls 发送操作。
export function AppUpdateOfferRows(props: { snapshot: AppUpdateSnapshot; calls: AppUpdateCalls }) {
  const { t } = useI18n();
  const { snapshot, calls } = props;
  const busy = snapshot.activity !== "idle";
  const offer = snapshot.offer;
  return <>
    {snapshot.readyVersion ? (
      <section className="vk-form-pref" data-app-update-ready>
        <div className="vk-form-pref-text">
          <strong>{formatMessage(t("settings.appUpdate.ready"), { version: snapshot.readyVersion })}</strong>
          <p className="vk-form-note">{t("settings.appUpdate.readyHint")}</p>
        </div>
        <Button disabled={busy} onClick={() => void calls.run(restartToUpdate)} size="sm">
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
                : offer.setupManaged
                  ? t("settings.appUpdate.fullSetup")
                  : t("settings.appUpdate.fullManual")}
          </p>
          {offer.previouslyFailed ? <p role="alert" className="vk-field-error">{t("settings.appUpdate.previouslyFailed")}</p> : null}
        </div>
        {offer.installable ? (
          <Button disabled={busy} onClick={() => void calls.run(downloadAppUpdate)} size="sm">{t("settings.appUpdate.download")}</Button>
        ) : offer.setupManaged ? null : (
          <Button onClick={() => void openExternalUrl(snapshot.downloadPageUrl).catch((cause: unknown) => calls.setCallError(errorMessage(cause)))} size="sm" variant="outline">
            {t("settings.appUpdate.openDownloadPage")}
          </Button>
        )}
      </section>
    ) : null}
  </>;
}

// AppUpdateStatusSections shows the update in progress: failures, the version ready to start, the
// offered version with its download action, and the download progress.
// AppUpdateStatusSections 显示进行中的更新：失败、已就绪待启动的版本、提议版本及其下载操作，以及下载进度。
// Parameters: snapshot is the latest updater state; calls sends the actions.
// 参数：snapshot 为最新的更新器状态；calls 发送操作。
export function AppUpdateStatusSections(props: { snapshot: AppUpdateSnapshot; calls: AppUpdateCalls }) {
  return <>
    <AppUpdateErrorNotices calls={props.calls} snapshot={props.snapshot} />
    <AppUpdateOfferRows calls={props.calls} snapshot={props.snapshot} />
    <AppUpdateProgressRow calls={props.calls} snapshot={props.snapshot} />
  </>;
}
