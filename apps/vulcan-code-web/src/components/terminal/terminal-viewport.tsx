import React from "react";
import { useI18n } from "@/i18n";

import type { TerminalDisplayChunk } from "@/types";
import type { TerminalPathOpener } from "./terminal-path-links";
import { useTerminalPathOpener } from "./use-terminal-path-opener";

// TerminalViewportProps defines the generic xterm renderer without project or session ownership.
// TerminalViewportProps 定义不包含项目或会话归属的通用 xterm 渲染器。
export interface TerminalViewportProps {
  // Ordered bootstrap or changed output chunks.
  // 有序的首次加载或变化输出分片。
  chunks: TerminalDisplayChunk[];
  // Whether terminal input must remain disabled.
  // 是否必须禁用终端输入。
  readOnly: boolean;
  // Accessible terminal label.
  // 可访问终端标签。
  ariaLabel: string;
  // Whether an interactive terminal should receive focus when its cached panel becomes visible.
  // 缓存面板变为可见时，交互式终端是否应获得焦点。
  autoFocus?: boolean;
  // Optional monotonic request that refocuses an already selected interactive terminal.
  // 用于重新聚焦已选中交互式终端的可选单调请求。
  focusRequestNonce?: number;
  // Optional explicit workspace input owner identity for an interactive project terminal.
  // 交互式项目终端使用的可选明确工作区输入所有者标识。
  inputOwnerId?: string;
  // Optional monotonically sequenced full-screen replacement from a frame-capable backend.
  // 来自帧能力后端的可选单调有序整屏替换。
  screenFrameSeq?: number | null;
  // Renderable plain or VT-formatted content belonging to `screenFrameSeq`.
  // 属于 `screenFrameSeq` 的可渲染纯文本或 VT 格式内容。
  screenText?: string | null;
  // Optional fixed backend row count that disables container-driven fitting when paired with `fixedCols`.
  // 与 `fixedCols` 配对时禁用容器驱动拟合的可选后端固定行数。
  fixedRows?: number | null;
  // Optional fixed backend column count that disables container-driven fitting when paired with `fixedRows`.
  // 与 `fixedRows` 配对时禁用容器驱动拟合的可选后端固定列数。
  fixedCols?: number | null;
  // Called with normal UTF-8 xterm input data.
  // 使用普通 UTF-8 xterm 输入数据调用。
  onData?: (data: string) => void;
  // Called with xterm binary input code units.
  // 使用 xterm 二进制输入码元调用。
  onBinary?: (data: string) => void;
  // Called when fitted character-grid dimensions change.
  // 拟合后的字符网格尺寸变化时调用。
  onResize?: (rows: number, cols: number) => void;
  // Called after xterm accepts every newly delivered chunk through one sequence.
  // xterm 接受截至某一序号的全部新交付分片后调用。
  onChunksRendered?: (lastRenderedSeq: number) => void;
  // Makes printed file locations Ctrl/Cmd+click links that open in the files panel, resolved against cwd.
  // 使输出中的文件位置成为 Ctrl/Cmd+点击链接，以 cwd 解析后在文件面板中打开。
  filePaths?: { cwd?: string | null };
  // Opens a Ctrl/Cmd+clicked file location; TerminalViewport derives it from filePaths.
  // 打开被 Ctrl/Cmd+点击的文件位置；TerminalViewport 由 filePaths 推导。
  onOpenPath?: TerminalPathOpener;
}

// LazyTerminalViewportRuntime keeps xterm outside the workbench startup chunk until a terminal is visible.
// LazyTerminalViewportRuntime 在终端可见前将 xterm 保留在工作台启动分块之外。
const LazyTerminalViewportRuntime = React.lazy(() => import("./terminal-viewport-runtime").then((module) => ({
  default: module.TerminalViewportRuntime,
})));

// TerminalViewport loads the emulator only for a mounted visible terminal surface.
// TerminalViewport 仅为已挂载的可见终端表面加载模拟器。
// Parameters: props supplies the typed viewport contract; returns the lazy emulator with an accessible loading fallback.
// 参数：props 提供类型化视口合同；返回带可访问加载占位的懒加载模拟器。
export function TerminalViewport(props: TerminalViewportProps) {
  const { t } = useI18n();
  const openPath = useTerminalPathOpener(props.filePaths?.cwd);
  return (
    <React.Suspense fallback={<div className="vk-term-viewport" data-loading="" role="status"
      aria-label={t("workspaceAudit.viewportLoading")} data-loading-label={t("workspaceAudit.viewportLoading")} />}>
      <LazyTerminalViewportRuntime {...props} onOpenPath={props.filePaths ? openPath : undefined} />
    </React.Suspense>
  );
}
