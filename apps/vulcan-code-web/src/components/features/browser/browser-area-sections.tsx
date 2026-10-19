import * as React from "react";
import { Globe2 } from "lucide-react";

import { Button } from "@/ui-kit/primitives/button";
import { PanelTab } from "@/ui-kit/primitives/panel-tab";
import { AlertDialog, AlertDialogContent, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription, AlertDialogFooter, AlertDialogCancel } from "@/ui-kit/primitives/alert-dialog";
import { formatMessage, useI18n } from "@/i18n";
import type {
  CoreBrowserDialog,
  CoreBrowserFileChooser,
  CoreBrowserLease,
  CoreBrowserPage,
  CoreBrowserSession,
} from "@/service/core-browser-service";

// BrowserCloseRequest freezes the exact browser or page the user asked to close.
// BrowserCloseRequest 冻结用户要求关闭的精确浏览器或页面。
export interface BrowserCloseRequest {
  // Core browser that owns the target.
  // 拥有目标的 Core 浏览器。
  session: CoreBrowserSession;
  // Page to close, or null to close the whole browser.
  // 要关闭的页面；为空时关闭整个浏览器。
  page: CoreBrowserPage | null;
  // Plain-text label shown in the confirmation.
  // 确认框中显示的纯文本标签。
  label: string;
}

// CloseRequestSetter opens or clears the close confirmation owned by the panel.
// CloseRequestSetter 打开或清除面板持有的关闭确认。
type CloseRequestSetter = React.Dispatch<React.SetStateAction<BrowserCloseRequest | null>>;

// browserTabTitle presents a compact origin label while the full URL remains in the tooltip.
// browserTabTitle 显示紧凑站点名称，完整地址仍保留在提示中。
// url is one Core-reported page URL; returns a safe plain-text tab label.
// url 是 Core 报告的页面地址；返回安全的纯文本页签文案。
export function browserTabTitle(url: string | null): string {
  if (!url || url === "about:blank") return "about:blank";
  try {
    const parsed = new URL(url);
    return parsed.hostname || url;
  } catch {
    return url;
  }
}

// BrowserInstanceTabs lists concurrent Core browsers when more than one is visible.
// BrowserInstanceTabs 在可见浏览器多于一个时列出并行的 Core 浏览器。
export function BrowserInstanceTabs({ visibleSessions, selectedSessionId, selectedPage, viewedPages, selectSession, setCloseError, setCloseRequest }: {
  // Running and crashed browsers that remain reachable.
  // 仍可访问的运行中与已崩溃浏览器。
  visibleSessions: CoreBrowserSession[];
  // Browser currently observed by the panel.
  // 面板当前观察的浏览器。
  selectedSessionId: string | null;
  // Page currently observed in the selected browser.
  // 选定浏览器中当前观察的页面。
  selectedPage: CoreBrowserPage | null;
  // Last page viewed in each browser, used for tab labels.
  // 每个浏览器最近查看的页面，用于页签标签。
  viewedPages: ReadonlyMap<string, CoreBrowserPage>;
  // Switches observation after releasing this client's previous grant.
  // 释放本客户端旧授权后切换观察对象。
  selectSession: (sessionId: string) => Promise<void>;
  // Clears a previous close failure.
  // 清除上一次关闭失败。
  setCloseError: (message: string | null) => void;
  // Opens the close confirmation.
  // 打开关闭确认。
  setCloseRequest: CloseRequestSetter;
}) {
  // t localizes tab labels and actions.
  // t 本地化页签标签与操作。
  const { t } = useI18n();
  return (
    <div hidden={visibleSessions.length <= 1} data-browser-instance-row className="shrink-0">
      <div className="vk-tab-strip" hidden={visibleSessions.length <= 1} role="tablist" aria-label={t("browser.activeAiBrowsers")}>
        {visibleSessions.map((session, index) => {
          // The browser label is its displayed page, never the last AI navigation URL.
          // 浏览器标签取其显示页面，绝不使用最后一次 AI 导航地址。
          const displayedPage = session.sessionId === selectedSessionId && selectedPage ? selectedPage : viewedPages.get(session.sessionId);
          const label = displayedPage ? displayedPage.title || browserTabTitle(displayedPage.url) : `${t("browser.browser")} ${index + 1}`;
          return <PanelTab key={session.sessionId}
            active={selectedSessionId === session.sessionId}
            closeLabel={`${t("browser.closeBrowser")}: ${label}`}
            icon={<Globe2 />}
            label={<>{label}{session.status === "crashed" ? t("browser.crashed") : null}</>}
            tabProps={{ "data-browser-session-id": session.sessionId, tabIndex: 0 }}
            title={label}
            onClose={() => { setCloseError(null); setCloseRequest({ session, page: null, label }); }}
            onSelect={() => void selectSession(session.sessionId)} />;
        })}
      </div>
    </div>
  );
}

// BrowserPageTabs lists the selected browser's pages when it has more than one.
// BrowserPageTabs 在选定浏览器有多个页面时列出这些页面。
export function BrowserPageTabs({ pages, selectedPageId, selectedSession, selectPage, setCloseError, setCloseRequest }: {
  // Pages reported by Core for the selected browser.
  // Core 为选定浏览器报告的页面。
  pages: CoreBrowserPage[];
  // Page currently observed by the panel.
  // 面板当前观察的页面。
  selectedPageId: string | null;
  // Browser that owns these pages.
  // 拥有这些页面的浏览器。
  selectedSession: CoreBrowserSession;
  // Observes another page after finishing held input on the current one.
  // 在当前页面完成已按下输入后观察另一页面。
  selectPage: (page: CoreBrowserPage) => void;
  // Clears a previous close failure.
  // 清除上一次关闭失败。
  setCloseError: (message: string | null) => void;
  // Opens the close confirmation.
  // 打开关闭确认。
  setCloseRequest: CloseRequestSetter;
}) {
  // t localizes tab labels and actions.
  // t 本地化页签标签与操作。
  const { t } = useI18n();
  return (
    <div hidden={pages.length <= 1} className="vk-tab-strip" role="tablist" aria-label={t("browser.browserPages")}>
      {pages.map((page) => (
        <PanelTab key={page.pageId}
          active={page.pageId === selectedPageId}
          closeLabel={`${t("browser.closePage")}: ${page.title || browserTabTitle(page.url)}`}
          icon={<Globe2 />}
          label={page.title || browserTabTitle(page.url)}
          tabProps={{ "data-browser-page-id": page.pageId, tabIndex: 0 }}
          title={page.title || page.url}
          onClose={() => { setCloseError(null); setCloseRequest({ session: selectedSession, page, label: page.title || browserTabTitle(page.url) }); }}
          onSelect={() => selectPage(page)} />
      ))}
    </div>
  );
}

// BrowserPagePrompts renders the selected page's JavaScript dialogs and file chooser requests.
// BrowserPagePrompts 渲染选定页面的 JavaScript 对话框与文件选择请求。
export function BrowserPagePrompts({ dialogs, fileChoosers, selectedPageId, dialogText, setDialogText, ownsControl, answerDialog, submitFileChooser }: {
  // Open dialogs across the selected browser's pages.
  // 选定浏览器各页面上打开的对话框。
  dialogs: CoreBrowserDialog[];
  // Pending file chooser requests across the selected browser's pages.
  // 选定浏览器各页面上待处理的文件选择请求。
  fileChoosers: CoreBrowserFileChooser[];
  // Page whose prompts are shown.
  // 显示其提示的页面。
  selectedPageId: string | null;
  // Editable answer for a prompt dialog.
  // 提示对话框的可编辑答复。
  dialogText: string;
  // Updates the prompt answer.
  // 更新提示答复。
  setDialogText: (value: string) => void;
  // Whether this UI may answer on the controller's behalf.
  // 本界面是否可以代表控制者作答。
  ownsControl: boolean;
  // Accepts or dismisses one dialog.
  // 接受或关闭一个对话框。
  answerDialog: (dialog: CoreBrowserDialog, accept: boolean) => Promise<void>;
  // Stages and attaches one local file.
  // 暂存并附加一个本地文件。
  submitFileChooser: (chooser: CoreBrowserFileChooser, file: File | undefined) => Promise<void>;
}) {
  // t localizes prompt actions.
  // t 本地化提示操作。
  const { t } = useI18n();
  return (
    <>
      {dialogs.filter((dialog) => dialog.pageId === selectedPageId).map((dialog) => (
        <div key={dialog.dialogId} role="alertdialog" className="flex flex-wrap items-center gap-2 rounded border p-3 text-sm">
          <strong>{dialog.dialogType}</strong><span>{dialog.message}</span>
          {dialog.dialogType === "prompt" ? <input className="rounded border bg-background px-2 py-1"
            value={dialogText} onChange={(event) => setDialogText(event.target.value)} disabled={!ownsControl} /> : null}
          <Button type="button" size="sm" disabled={!ownsControl} onClick={() => void answerDialog(dialog, true)}>
            {t("browser.accept")}
          </Button>
          <Button type="button" size="sm" variant="outline" disabled={!ownsControl}
            onClick={() => void answerDialog(dialog, false)}>{t("browser.dismiss")}</Button>
        </div>
      ))}
      {fileChoosers.filter((chooser) => chooser.pageId === selectedPageId).map((chooser) => (
        <label key={chooser.chooserId} className="flex flex-wrap items-center gap-2 rounded border p-3 text-sm">
          <span>{t("browser.pageRequestsAFile")}</span>
          <input type="file" disabled={!ownsControl} onChange={(event) => void submitFileChooser(chooser, event.target.files?.[0])} />
        </label>
      ))}
    </>
  );
}

// BrowserCloseConfirmDialog confirms closing one exact page or browser.
// BrowserCloseConfirmDialog 确认关闭一个精确页面或浏览器。
export function BrowserCloseConfirmDialog({ closeRequest, setCloseRequest, closePending, closeError, closing, lease, closeSession }: {
  // Frozen close target, or null while the dialog is hidden.
  // 冻结的关闭目标；弹窗隐藏时为空。
  closeRequest: BrowserCloseRequest | null;
  // Clears the target when the user dismisses the dialog.
  // 用户取消弹窗时清除目标。
  setCloseRequest: CloseRequestSetter;
  // Whether a close request is already in flight.
  // 关闭请求是否已在进行中。
  closePending: React.RefObject<boolean>;
  // Latest close failure.
  // 最近的关闭失败。
  closeError: string | null;
  // Whether the confirmation is waiting for Core.
  // 确认是否正在等待 Core。
  closing: boolean;
  // This UI's control grant, used to warn about interrupting another controller.
  // 本界面的控制授权，用于提示将中断其他控制者。
  lease: CoreBrowserLease | null;
  // Closes the confirmed target.
  // 关闭已确认的目标。
  closeSession: () => Promise<void>;
}) {
  // t localizes the confirmation copy.
  // t 本地化确认文案。
  const { t } = useI18n();
  return (
    <AlertDialog open={closeRequest !== null} onOpenChange={(open) => { if (!open && !closePending.current) setCloseRequest(null); }}>
      {closeRequest ? <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{closeRequest?.page ? t("browser.closePage2") : t("browser.closeBrowser2")}</AlertDialogTitle>
          <AlertDialogDescription>
            {closeRequest?.page
              ? (formatMessage(t("browser.closeUnsavedPageChanges"), { label: closeRequest.label }))
              : (formatMessage(t("browser.closeAndAllIts"), { label: closeRequest?.label }))}
            {closeRequest?.session.leaseState === "held" && lease?.session.sessionId !== closeRequest.session.sessionId
              ? t("browser.activeAutomationWillBe") : null}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {closeError ? <p role="alert" className="text-xs text-destructive">{closeError}</p> : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={closing}>{t("browser.cancel")}</AlertDialogCancel>
          <Button type="button" variant="destructive" disabled={closing} onClick={() => void closeSession()}>{closing ? t("browser.closing") : t("browser.confirmClose")}</Button>
        </AlertDialogFooter>
      </AlertDialogContent> : null}
    </AlertDialog>
  );
}
