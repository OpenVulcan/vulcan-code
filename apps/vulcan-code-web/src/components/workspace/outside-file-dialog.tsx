import React from "react";
import "@/ui-kit/styles/features/workspace-files.css";
import { useI18n } from "@/i18n";
import { Button } from "@/ui-kit/primitives/button";
import { Checkbox } from "@/ui-kit/primitives/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog";
import { toast } from "@/ui-kit/primitives/toast";
import { readElectronLocalFileBridge, type ElectronLocalFileInspection } from "@/host-shell/electron-bridge";
import { fileViewerKind } from "@/components/file-viewers/file-viewer-kind";
import { openFileWindow } from "@/components/file-windows/file-window-store";
import type { WorkspaceFileActionRequest } from "@/components/workspace/workspace-file-actions";
import { loadSessionOutsideFileAccess, loadWorkspaceFileKind, saveSessionOutsideFileAccess } from "@/service/outside-file-access-service";

// OutsideFileDialog opens a file outside the session's workspace in a file window as the session's outside-file access
// says: "deny" reports that and stops, "allow" opens at once and "prompt" asks first, where the user may stop asking for
// this session. A file no viewer reads, a request without a session, and the user's choice of the default program
// continue with `onDefaultProgram`. The prompt guards against accidental opens; it is not a security boundary.
// OutsideFileDialog 按会话的工作区外文件访问方式在文件窗口中打开会话工作区之外的文件："deny" 提示后停止，"allow" 直接
// 打开，"prompt" 先询问，用户可选择本会话不再询问。没有查看器能读取的文件、没有会话的请求以及用户选择默认程序时，以
// `onDefaultProgram` 继续。该提示用于防止误开，并非安全边界。
export function OutsideFileDialog(props: { request: WorkspaceFileActionRequest; onDone: () => void; onDefaultProgram: () => void }) {
  const { t } = useI18n();
  const { path, sessionId } = props.request;
  // inspection is the desktop's check of the file, or null on the web client; undefined until the dialog is needed.
  // inspection 是桌面端对文件的检查结果，网页客户端为 null；在需要对话框之前为 undefined。
  const [inspection, setInspection] = React.useState<ElectronLocalFileInspection | null | undefined>(undefined);
  const [remember, setRemember] = React.useState(false);
  const [busy, setBusy] = React.useState(false);
  const latest = React.useRef({ props, t });
  latest.current = { props, t };

  // fail reports an error and ends the request.
  // fail 报告错误并结束请求。
  const fail = React.useCallback((error: unknown) => {
    toast.error(latest.current.t("fileActions.failed"), { description: error instanceof Error ? error.message : String(error) });
    latest.current.props.onDone();
  }, []);

  React.useEffect(() => {
    const { props: current, t: translate } = latest.current;
    let cancelled = false;
    void (async () => {
      if (!sessionId) return current.onDefaultProgram();
      const viewable = fileViewerKind(path) !== null || (await loadWorkspaceFileKind(path)) === "code";
      const { access } = await loadSessionOutsideFileAccess(sessionId);
      if (cancelled) return;
      if (access === "deny") {
        toast.info(translate("outsideFile.denied"), { description: path });
        return current.onDone();
      }
      if (!viewable) return current.onDefaultProgram();
      if (access === "allow") {
        openFileWindow(sessionId, path);
        return current.onDone();
      }
      const result = (await readElectronLocalFileBridge()?.inspect(path)) ?? null;
      if (cancelled) return;
      if (result && !result.exists) {
        toast.info(translate("workspaceFiles.openPathMissing"));
        return current.onDone();
      }
      setInspection(result);
    })().catch((error: unknown) => { if (!cancelled) fail(error); });
    return () => { cancelled = true; };
  }, [fail, path, sessionId]);

  // open shows the file in a window, first saving "allow" for this session when the user chose not to be asked again.
  // open 在窗口中显示文件；用户选择不再询问时先为本会话保存 "allow"。
  const open = () => {
    if (!sessionId) return;
    setBusy(true);
    (remember ? saveSessionOutsideFileAccess(sessionId, "allow") : Promise.resolve()).then(() => {
      openFileWindow(sessionId, path);
      props.onDone();
    }, fail);
  };
  const bridge = readElectronLocalFileBridge();
  if (inspection === undefined) return null;
  return (
    <Dialog open onOpenChange={(next) => { if (!next && !busy) props.onDone(); }}>
      <DialogContent size="sm">
        <DialogHeader>
          <DialogTitle>{t("outsideFile.title")}</DialogTitle>
          <DialogDescription>{t("outsideFile.hint")}</DialogDescription>
        </DialogHeader>
        <code className="workspace-file-action-path">{path}</code>
        <label className="workspace-file-action-remember">
          <Checkbox checked={remember} disabled={busy} onCheckedChange={(checked) => setRemember(checked === true)} />
          {t("outsideFile.remember")}
        </label>
        <DialogFooter>
          <Button disabled={busy} onClick={props.onDone} type="button" variant="outline">
            {t("conversation.action.cancel")}
          </Button>
          {bridge && inspection ? (
            <Button disabled={busy} onClick={() => { setBusy(true); bridge.showInFolder(path).then(props.onDone, fail); }} type="button" variant="outline">
              {t("fileActions.showInFolder")}
            </Button>
          ) : null}
          {bridge && inspection?.opensWithDefaultApp ? (
            <Button disabled={busy} onClick={() => { setBusy(true); bridge.openWithDefaultApp(path).then(props.onDone, fail); }} type="button" variant="outline">
              {t("fileActions.openWithDefaultApp")}
            </Button>
          ) : null}
          <Button disabled={busy} onClick={open} type="button">
            {t("fileActions.open")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
