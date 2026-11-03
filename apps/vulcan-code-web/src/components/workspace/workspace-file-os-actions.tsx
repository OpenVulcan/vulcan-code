import React from "react";
import { useI18n } from "@/i18n";
import { readElectronLocalFileBridge } from "@/host-shell/electron-bridge";
import { Button } from "@/ui-kit/primitives/button";
import { toast } from "@/ui-kit/primitives/toast";

// WorkspaceFileOsActions offers what can still be done with a file the panel cannot show. The desktop opens it with its
// default program (never a runnable file, which only gets its folder) or shows it in its folder; the web client cannot
// reach the OS, so it offers to copy the path.
// WorkspaceFileOsActions 为面板无法显示的文件提供仍可执行的操作。桌面端用默认程序打开（可运行文件永不打开，只提供所在
// 文件夹）或在文件夹中显示；网页客户端无法访问操作系统，因此提供复制路径。
export function WorkspaceFileOsActions(props: { path: string }) {
  const { t } = useI18n();
  const bridge = readElectronLocalFileBridge();
  // opensWithDefaultApp is the desktop's check of the file; null until known.
  // opensWithDefaultApp 是桌面端对文件的检查结果；未知前为 null。
  const [opensWithDefaultApp, setOpensWithDefaultApp] = React.useState<boolean | null>(null);
  React.useEffect(() => {
    setOpensWithDefaultApp(null);
    if (!bridge) return;
    let current = true;
    bridge.inspect(props.path).then((result) => {
      if (current) setOpensWithDefaultApp(result.exists && result.opensWithDefaultApp);
    }, () => {
      if (current) setOpensWithDefaultApp(false);
    });
    return () => { current = false; };
  }, [bridge, props.path]);
  // run hands the file to the OS and reports a refusal or error.
  // run 将文件交给操作系统，并报告拒绝或错误。
  const run = (operation: (path: string) => Promise<void>) => {
    operation(props.path).catch((error: unknown) => {
      toast.error(t("fileActions.failed"), { description: error instanceof Error ? error.message : String(error) });
    });
  };
  if (!bridge) {
    return (
      <div className="workspace-file-os-actions">
        <Button onClick={() => void navigator.clipboard?.writeText(props.path)} size="sm" type="button" variant="outline">
          {t("fileActions.copyPath")}
        </Button>
      </div>
    );
  }
  return (
    <div className="workspace-file-os-actions">
      {opensWithDefaultApp ? (
        <Button onClick={() => run(bridge.openWithDefaultApp)} size="sm" type="button">
          {t("fileActions.openWithDefaultApp")}
        </Button>
      ) : null}
      <Button onClick={() => run(bridge.showInFolder)} size="sm" type="button" variant="outline">
        {t("fileActions.showInFolder")}
      </Button>
    </div>
  );
}
