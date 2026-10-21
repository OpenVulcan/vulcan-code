import { useProjectFloatingLayerControls } from "@/components/workspace/project-floating-layer";
import { usePublishedWorkspaceFileOpener } from "@/components/workspace/workspace-file-open-request";
import { terminalPathFrom } from "@/conversation/tools/views/terminal-paths";
import type { TerminalPathOpener } from "./terminal-path-links";

// useTerminalPathOpener returns how a terminal inside a floating window opens a printed file location: in the files
// panel, resolved against `cwd`, after restoring a maximized window that would cover the panel. It returns undefined
// while the workbench offers no files panel.
// useTerminalPathOpener 返回浮动窗口内终端打开输出文件位置的方式：以 `cwd` 解析后在文件面板中打开，
// 若窗口最大化会遮住面板则先还原。工作台未提供文件面板时返回 undefined。
export function useTerminalPathOpener(cwd: string | null | undefined): TerminalPathOpener | undefined {
  const openFile = usePublishedWorkspaceFileOpener();
  const windowControls = useProjectFloatingLayerControls();
  if (!openFile) return undefined;
  return (path, lines) => {
    if (windowControls?.maximized) windowControls.toggleMaximized();
    openFile(terminalPathFrom(cwd, path), lines);
  };
}
