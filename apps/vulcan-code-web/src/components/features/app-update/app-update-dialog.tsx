import * as React from "react";

import { pendingUpdateVersion, type AppUpdateHost } from "@/components/features/app-update/app-update-context";
import {
  AppUpdateReleaseNotes,
  AppUpdateStatusSections,
  useAppUpdateCalls,
} from "@/components/features/app-update/app-update-sections";
import { releaseNotesFor } from "@/components/features/app-update/use-app-update";
import { formatMessage, useI18n } from "@/i18n";
import type { AppUpdateOffer } from "@/service/app-update-events";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import "@/ui-kit/styles/features/app-update-dialog.css";

// AppUpdateDialog previews the release notes of the version to update to and runs the same download,
// progress and restart flow as Settings → Software update, on the workbench's shared updater state.
// AppUpdateDialog 预览要更新到的版本的发布说明，并在工作台共享的更新器状态上运行与“设置 → 软件更新”
// 相同的下载、进度与重启流程。
// Parameters: host is the shared updater state; onClose hides the dialog.
// 参数：host 为共享的更新器状态；onClose 隐藏对话框。
export default function AppUpdateDialog(props: { host: AppUpdateHost; onClose: () => void }) {
  const { t, locale } = useI18n();
  const { snapshot, setSnapshot, openSettings } = props.host;
  const calls = useAppUpdateCalls(setSnapshot);
  // The Core drops the offer once its download is ready; the notes read before stay on screen.
  // 下载就绪后 Core 会移除提议；此前读到的说明继续显示。
  const [shownOffer, setShownOffer] = React.useState<AppUpdateOffer | null>(snapshot?.offer ?? null);
  React.useEffect(() => {
    if (snapshot?.offer) setShownOffer(snapshot.offer);
  }, [snapshot?.offer]);

  const version = pendingUpdateVersion(snapshot);
  if (!snapshot || !version) return null;
  const notes = (snapshot.offer ?? (shownOffer?.version === version ? shownOffer : null))?.notes ?? {};
  const hasNotes = releaseNotesFor(notes, locale) !== "";

  return (
    <Dialog open onOpenChange={(open) => { if (!open) props.onClose(); }}>
      <DialogContent data-app-update-dialog="" scrollMode="fixed-footer" size="lg">
        <DialogHeader>
          <DialogTitle>{formatMessage(t("appUpdate.dialog.title"), { version })}</DialogTitle>
          <DialogDescription>
            {formatMessage(t("appUpdate.dialog.description"), { current: snapshot.currentVersion, version })}
          </DialogDescription>
        </DialogHeader>
        <div className="vk-dialog-body">
          <AppUpdateStatusSections calls={calls} showNotes={false} snapshot={snapshot} />
          <section className="vk-app-update-notes" data-app-update-notes="">
            <h3>{formatMessage(t("appUpdate.dialog.notesTitle"), { version })}</h3>
            {hasNotes
              ? <AppUpdateReleaseNotes notes={notes} />
              : <p className="vk-form-note">{t("appUpdate.dialog.noNotes")}</p>}
          </section>
        </div>
        <DialogFooter showCloseButton>
          <Button onClick={() => { props.onClose(); openSettings(); }} variant="ghost">{t("appUpdate.dialog.openSettings")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
