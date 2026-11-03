import { X } from "lucide-react";
import { useI18n } from "@/i18n";
import { FileTypeIcon, resolveFileTypeIcon } from "@/components/conversation/conversation-file-type-icon";
import {
  closeFileWindow,
  registerFileWindowChip,
  restoreFileWindow,
  useFileWindows,
} from "@/components/file-windows/file-window-store";
import "@/ui-kit/styles/features/file-windows.css";

// FileWindowTaskbar is the strip above the composer listing the session's file windows; it shows only while a file
// is open. Each chip is where its window minimizes to, and a click brings the window back to the front.
// FileWindowTaskbar 是输入区上方列出会话文件窗口的条带，仅在有文件打开时显示。每个条目是其窗口最小化的去处，点击即将
// 窗口还原到最前。
export default function FileWindowTaskbar(props: { sessionId: string }) {
  const { t } = useI18n();
  const windows = useFileWindows(props.sessionId);
  if (windows.length === 0) return null;
  return (
    <div aria-label={t("fileWindow.taskbar")} className="vk-file-taskbar" role="toolbar">
      {windows.map((window) => {
        const name = window.path.split(/[\\/]/).pop() || window.path;
        return (
          <span className="vk-file-chip" data-minimized={window.minimized ? "true" : undefined} key={window.id}>
            <button
              onClick={() => restoreFileWindow(props.sessionId, window.id)}
              ref={(element) => registerFileWindowChip(props.sessionId, window.id, element)}
              title={window.path}
              type="button"
            >
              <FileTypeIcon icon={resolveFileTypeIcon(window.path)} />
              <span>{name}</span>
            </button>
            <button
              aria-label={t("fileWindow.close")}
              className="vk-file-chip-close"
              onClick={() => closeFileWindow(props.sessionId, window.id)}
              title={t("fileWindow.close")}
              type="button"
            >
              <X />
            </button>
          </span>
        );
      })}
    </div>
  );
}
