import React from "react";
import "@/ui-kit/styles/features/workspace-files.css";
import { useI18n } from "@/i18n";
import { Button } from "@/ui-kit/primitives/button";
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
import type { WorkspaceFileActionRequest } from "@/components/workspace/workspace-file-actions";

// WorkspaceFileActionDialog runs one file action. The desktop first checks the file: a missing file is reported,
// "open" and "reveal" act at once when allowed, and otherwise a dialog offers the default program (never for
// runnable files) and the file's folder. The web client cannot reach the OS, so it explains that and offers to copy
// the path.
// WorkspaceFileActionDialog 执行单个文件操作。桌面端先检查文件：文件不存在时提示；"open" 与 "reveal" 在允许时立即执行；
// 其余情况弹出对话框，提供默认程序打开（可运行文件永不提供）与所在文件夹。网页客户端无法访问操作系统，因此说明原因并
// 提供复制路径。
export default function WorkspaceFileActionDialog(props: { request: WorkspaceFileActionRequest; onDone: () => void }) {
  const { t } = useI18n();
  const { path, action } = props.request;
  const [inspection, setInspection] = React.useState<ElectronLocalFileInspection | null>(null);
  const [busy, setBusy] = React.useState(false);
  const latest = React.useRef({ onDone: props.onDone, t });
  latest.current = { onDone: props.onDone, t };

  // fail reports an OS refusal or error and ends the request.
  // fail 报告操作系统的拒绝或错误并结束请求。
  const fail = React.useCallback((error: unknown) => {
    toast.error(latest.current.t("fileActions.failed"), { description: error instanceof Error ? error.message : String(error) });
    latest.current.onDone();
  }, []);

  React.useEffect(() => {
    const { onDone, t: translate } = latest.current;
    const bridge = readElectronLocalFileBridge();
    if (!bridge) {
      toast.info(translate("fileActions.unsupportedTitle"), {
        description: path,
        action: { label: translate("fileActions.copyPath"), onClick: () => void navigator.clipboard?.writeText(path) },
      });
      onDone();
      return;
    }
    let cancelled = false;
    void (async () => {
      const result = await bridge.inspect(path);
      if (cancelled) return;
      if (!result.exists) {
        toast.info(translate("workspaceFiles.openPathMissing"));
        onDone();
      } else if (action === "reveal") {
        await bridge.showInFolder(path);
        onDone();
      } else if (action === "open" && result.opensWithDefaultApp) {
        await bridge.openWithDefaultApp(path);
        onDone();
      } else {
        setInspection(result);
      }
    })().catch((error: unknown) => { if (!cancelled) fail(error); });
    return () => { cancelled = true; };
  }, [action, fail, path]);

  // run performs one dialog choice through the OS and closes the dialog when it succeeds.
  // run 经操作系统执行对话框中的一个选择，成功后关闭对话框。
  const run = (operation: (bridgePath: string) => Promise<void>) => {
    setBusy(true);
    operation(path).then(props.onDone, fail);
  };
  const bridge = readElectronLocalFileBridge();
  if (!inspection || !bridge) return null;
  return (
    <Dialog open onOpenChange={(open) => { if (!open && !busy) props.onDone(); }}>
      <DialogContent size="sm">
        <DialogHeader>
          <DialogTitle>{t("fileActions.unsupportedTitle")}</DialogTitle>
          <DialogDescription>
            {inspection.opensWithDefaultApp ? t("fileActions.unsupportedHint") : t("fileActions.runnableHint")}
          </DialogDescription>
        </DialogHeader>
        <code className="workspace-file-action-path">{path}</code>
        <DialogFooter>
          <Button disabled={busy} onClick={props.onDone} type="button" variant="outline">
            {t("conversation.action.cancel")}
          </Button>
          <Button disabled={busy} onClick={() => run(bridge.showInFolder)} type="button" variant={inspection.opensWithDefaultApp ? "outline" : "primary"}>
            {t("fileActions.showInFolder")}
          </Button>
          {inspection.opensWithDefaultApp ? (
            <Button disabled={busy} onClick={() => run(bridge.openWithDefaultApp)} type="button">
              {t("fileActions.openWithDefaultApp")}
            </Button>
          ) : null}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
