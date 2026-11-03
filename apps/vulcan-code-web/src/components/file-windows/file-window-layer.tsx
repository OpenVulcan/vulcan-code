import React from "react";
import { Expand, FileText, PanelRight, Shrink } from "lucide-react";
import { useI18n } from "@/i18n";
import { DEFAULT_FLOATING_LAYER_SIZE, ProjectFloatingLayer, type ProjectFloatingLayerControls } from "@/components/workspace/project-floating-layer";
import { FloatingWindowHead } from "@/components/workspace/floating-window-head";
import { usePublishedWorkspaceFileOpener } from "@/components/workspace/workspace-file-open-request";
import { DocumentViewer } from "@/components/file-viewers/document-viewer";
import { fileViewerKind } from "@/components/file-viewers/file-viewer-kind";
import { OutsideTextViewer } from "@/components/file-viewers/outside-text-viewer";
import { closeFileWindow, fileWindowChip, minimizeFileWindow, useFileWindows, type FileWindow } from "@/components/file-windows/file-window-store";
import "@/ui-kit/styles/features/workspace-files.css";
import "@/ui-kit/styles/features/document-viewer.css";
import "@/ui-kit/styles/features/file-windows.css";

// FILE_WINDOW_CASCADE_PX and FILE_WINDOW_CASCADE_STEPS step each new window up and left of the previous one, starting
// over after a few windows.
// FILE_WINDOW_CASCADE_PX 与 FILE_WINDOW_CASCADE_STEPS 使每个新窗口位于上一个窗口的左上方，数个窗口后重新开始。
const FILE_WINDOW_CASCADE_PX = 32;
const FILE_WINDOW_CASCADE_STEPS = 6;

// FileWindowLayer renders the file windows of the selected session, each minimizing into its taskbar chip. Windows of
// other sessions stay closed until their session is selected again.
// FileWindowLayer 渲染所选会话的文件窗口，各自最小化收入其任务栏条目。其他会话的窗口在该会话再次被选中前保持关闭。
export default function FileWindowLayer(props: { sessionId: string }) {
  const windows = useFileWindows(props.sessionId);
  return windows.map((window) => <FileWindowFrame key={window.id} sessionId={props.sessionId} window={window} />);
}

// FileWindowFrame is one movable window showing one file read-only: in its viewer, or as text when no viewer reads it
// (a text file outside the workspace).
// FileWindowFrame 是只读显示单个文件的可移动窗口：使用其查看器显示，没有查看器能读取时按文本显示（工作区之外的文本文件）。
function FileWindowFrame(props: { sessionId: string; window: FileWindow }) {
  const { sessionId, window } = props;
  const openFile = usePublishedWorkspaceFileOpener();
  // anchorRef resolves the chip at animation time, so a chip that re-mounted is still found.
  // anchorRef 在动画时解析任务栏条目，使重新挂载的条目仍能被找到。
  const anchorRef = React.useMemo(() => ({
    get current() { return fileWindowChip(sessionId, window.id) as HTMLButtonElement | null; },
  }), [sessionId, window.id]);
  // defaultSize cascades the first placement of each slot, so a new window never hides the previous one exactly.
  // defaultSize 使各槽位的首次位置层叠错开，新窗口不会恰好遮住上一个窗口。
  const defaultSize = React.useMemo(() => ({
    ...DEFAULT_FLOATING_LAYER_SIZE,
    inset: ((Number(window.id.replace(/\D/g, "")) - 1) % FILE_WINDOW_CASCADE_STEPS) * FILE_WINDOW_CASCADE_PX,
  }), [window.id]);
  const kind = fileViewerKind(window.path);
  const separator = Math.max(window.path.lastIndexOf("/"), window.path.lastIndexOf("\\"));
  const name = window.path.slice(separator + 1) || window.path;
  // dock hands the file to the right panel's viewer tab and closes this window.
  // dock 将文件交给右侧面板的查看页签并关闭本窗口。
  const dock = openFile ? () => {
    openFile(window.path, null, "dock");
    closeFileWindow(sessionId, window.id);
  } : null;
  const header = (controls: ProjectFloatingLayerControls) => (
    <FileWindowHead
      controls={controls}
      detail={window.path.slice(0, Math.max(separator, 0))}
      name={name}
      onClose={() => closeFileWindow(sessionId, window.id)}
      onDock={dock}
      onMinimize={() => minimizeFileWindow(sessionId, window.id)}
      path={window.path}
    />
  );
  return (
    <ProjectFloatingLayer
      defaultSize={defaultSize}
      expanded={!window.minimized}
      header={header}
      minimizeAnchorRef={anchorRef}
      projectId={window.id}
      raiseKey={window.raise}
    >
      <div className="vk-file-win workspace-file-preview document">
        {kind ? <DocumentViewer kind={kind} name={name} path={window.path} /> : <OutsideTextViewer path={window.path} />}
      </div>
    </ProjectFloatingLayer>
  );
}

// FileWindowHead is the title row: the file name over its folder, then full-screen reading, docking to the right
// panel and the shared window commands.
// FileWindowHead 是标题行：文件名及其下方所在文件夹，然后是全屏阅读、挂到右侧面板与共享窗口命令。
function FileWindowHead(props: {
  controls: ProjectFloatingLayerControls;
  name: string;
  detail: string;
  path: string;
  onMinimize: () => void;
  onClose: () => void;
  onDock: (() => void) | null;
}) {
  const { t } = useI18n();
  const headRef = React.useRef<HTMLDivElement | null>(null);
  const [fullscreen, setFullscreen] = React.useState(false);
  React.useEffect(() => {
    const update = () => setFullscreen(Boolean(document.fullscreenElement?.contains(headRef.current)));
    document.addEventListener("fullscreenchange", update);
    return () => document.removeEventListener("fullscreenchange", update);
  }, []);
  // toggleFullscreen lets the whole window fill the screen; Esc leaves it as well.
  // toggleFullscreen 使整个窗口占满屏幕；按 Esc 同样可以退出。
  const toggleFullscreen = () => {
    if (fullscreen) void document.exitFullscreen();
    else void headRef.current?.closest<HTMLElement>(".vk-float-win")?.requestFullscreen();
  };
  const action = (label: string, icon: React.ReactNode, onClick: () => void) => (
    <button aria-label={label} className="vk-icon-btn" data-size="sm" onClick={onClick} title={label} type="button">
      {icon}
    </button>
  );
  return (
    <div className="vk-file-win-head" ref={headRef}>
      <FloatingWindowHead
        actions={<>
          {action(t(fullscreen ? "fileWindow.exitFullscreen" : "fileWindow.fullscreen"), fullscreen ? <Shrink /> : <Expand />, toggleFullscreen)}
          {props.onDock ? action(t("fileWindow.dock"), <PanelRight />, props.onDock) : null}
        </>}
        controls={props.controls}
        detail={props.detail}
        hoverTitle={props.path}
        icon={<FileText />}
        labels={{ maximize: t("fileWindow.maximize"), restore: t("fileWindow.restore"), minimize: t("fileWindow.minimize"), close: t("fileWindow.close") }}
        onClose={props.onClose}
        onMinimize={props.onMinimize}
        title={props.name}
      />
    </div>
  );
}
