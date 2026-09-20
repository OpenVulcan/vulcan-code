import React from "react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

import type { TerminalViewportProps } from "./terminal-viewport";

// TerminalViewportRuntime owns one stable xterm instance and event-driven ResizeObserver fitting.
// TerminalViewportRuntime 持有一个稳定 xterm 实例，并使用事件驱动的 ResizeObserver 拟合。
// Parameters: props supplies terminal content, input, resize and ownership callbacks; returns one stable xterm host with an independent terminal palette.
// 参数：props 提供终端内容、输入、尺寸及归属回调；返回一个使用独立终端色板的稳定 xterm 宿主。
export function TerminalViewportRuntime(props: TerminalViewportProps) {
  // hostRef identifies the exact DOM node into which xterm opens.
  // hostRef 标识 xterm 打开的精确 DOM 节点。
  const hostRef = React.useRef<HTMLDivElement | null>(null);
  // terminalRef exposes the live renderer to output delivery effects.
  // terminalRef 向输出交付副作用暴露实时渲染器。
  const terminalRef = React.useRef<Terminal | null>(null);
  // lastRenderedSeqRef prevents duplicate writes across React renders.
  // lastRenderedSeqRef 防止 React 重渲染期间重复写入。
  const lastRenderedSeqRef = React.useRef(0);
  // lastScreenFrameSeqRef prevents duplicate full-screen replacements.
  // lastScreenFrameSeqRef 防止重复执行整屏替换。
  const lastScreenFrameSeqRef = React.useRef(0);
  // callbacksRef keeps xterm disposables stable while using current callbacks.
  // callbacksRef 在保持 xterm disposable 稳定的同时使用最新回调。
  const callbacksRef = React.useRef(props);
  callbacksRef.current = props;

  React.useLayoutEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    // fixedGrid retains the backend-owned screen geometry for display-only terminals.
    // fixedGrid 为只负责显示的终端保留后端持有的屏幕几何。
    const fixedGrid = Number.isInteger(callbacksRef.current.fixedRows)
      && Number.isInteger(callbacksRef.current.fixedCols)
      && (callbacksRef.current.fixedRows ?? 0) > 0
      && (callbacksRef.current.fixedCols ?? 0) > 0
      ? {
          rows: callbacksRef.current.fixedRows as number,
          cols: callbacksRef.current.fixedCols as number,
        }
      : null;
    // terminal is the only emulator instance owned by this mounted viewport.
    // terminal 是当前已挂载视口拥有的唯一模拟器实例。
    const terminal = new Terminal({
      allowProposedApi: false,
      cols: fixedGrid?.cols,
      convertEol: false,
      cursorBlink: !callbacksRef.current.readOnly,
      cursorStyle: callbacksRef.current.readOnly ? "bar" : "block",
      disableStdin: callbacksRef.current.readOnly,
      fontFamily: '"Cascadia Mono", "SFMono-Regular", Consolas, "Liberation Mono", monospace',
      fontSize: 13,
      lineHeight: 1.15,
      rows: fixedGrid?.rows,
      scrollback: 10_000,
      // xterm 6 derives its native slider width from the overview ruler width.
      // xterm 6 从概览标尺宽度取得原生滑块宽度。
      overviewRuler: { width: 6, showTopBorder: false, showBottomBorder: false },
      // Keep the emulator palette independent of application light/dark surfaces.
      // 保持模拟器色板独立于应用浅色和深色页面。
      theme: {
        background: "#111318",
        foreground: "#d9dee8",
        cursor: "#f0f3f8",
        cursorAccent: "#111318",
        selectionBackground: "#4e6b955c",
      },
    });
    // fitAddon exists only for interactive or otherwise container-fitted terminals.
    // fitAddon 仅为交互式或其他需要容器拟合的终端创建。
    const fitAddon = fixedGrid ? null : new FitAddon();
    if (fitAddon) terminal.loadAddon(fitAddon);
    terminal.open(host);
    terminalRef.current = terminal;
    // lastDimensions suppresses duplicate backend resize requests.
    // lastDimensions 抑制重复的后端尺寸调整请求。
    let lastDimensions = { rows: 0, cols: 0 };
    // animationFrame coalesces synchronous layout changes without interval polling.
    // animationFrame 合并同步布局变化，不使用定时轮询。
    let animationFrame: number | null = null;
    // resizeObserver remains absent for a backend-sized read-only grid so window resizing cannot reflow it.
    // resizeObserver 在后端定尺寸只读网格下保持为空，使窗口缩放无法重排它。
    let resizeObserver: ResizeObserver | null = null;

    // fit measures the visible host and reports only changed positive dimensions.
    // fit 测量可见宿主，并仅报告变化后的正尺寸。
    const fit = () => {
      animationFrame = null;
      if (host.clientWidth <= 0 || host.clientHeight <= 0) return;
      if (!fitAddon) return;
      // wasAtBottom preserves live-output following without stealing an intentionally scrolled-back viewport.
      // wasAtBottom 保留实时输出跟随，同时不抢占用户主动回看的历史视口。
      const wasAtBottom = terminal.buffer.active.viewportY === terminal.buffer.active.baseY;
      fitAddon.fit();
      const rows = terminal.rows;
      const cols = terminal.cols;
      if (rows <= 0 || cols <= 0) return;
      if (wasAtBottom) terminal.scrollToBottom();
      // refresh repaints the complete fitted viewport even when no subsequent process output arrives.
      // refresh 即使没有后续进程输出，也会重绘完成拟合后的整个可见区。
      terminal.refresh(0, rows - 1);
      if (rows === lastDimensions.rows && cols === lastDimensions.cols) return;
      lastDimensions = { rows, cols };
      callbacksRef.current.onResize?.(rows, cols);
    };
    // scheduleFit batches ResizeObserver notifications into the next paint frame.
    // scheduleFit 将 ResizeObserver 通知批处理到下一绘制帧。
    const scheduleFit = () => {
      if (animationFrame !== null) return;
      animationFrame = window.requestAnimationFrame(fit);
    };
    const dataDisposable = terminal.onData((data) => {
      if (!callbacksRef.current.readOnly) callbacksRef.current.onData?.(data);
    });
    const binaryDisposable = terminal.onBinary((data) => {
      if (!callbacksRef.current.readOnly) callbacksRef.current.onBinary?.(data);
    });
    if (fixedGrid) {
      terminal.refresh(0, fixedGrid.rows - 1);
    } else {
      // resizeObserver reacts only to actual viewport geometry changes for fitted terminals.
      // resizeObserver 仅为拟合型终端响应真实视口几何变化。
      resizeObserver = new ResizeObserver(scheduleFit);
      resizeObserver.observe(host);
      scheduleFit();
    }

    return () => {
      resizeObserver?.disconnect();
      if (animationFrame !== null) window.cancelAnimationFrame(animationFrame);
      dataDisposable.dispose();
      binaryDisposable.dispose();
      terminal.dispose();
      terminalRef.current = null;
      lastRenderedSeqRef.current = 0;
      lastScreenFrameSeqRef.current = 0;
    };
  }, []);

  React.useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal) return;
    terminal.options.disableStdin = props.readOnly;
    terminal.options.cursorBlink = !props.readOnly;
    terminal.options.cursorStyle = props.readOnly ? "bar" : "block";
    if (props.readOnly) terminal.blur();
  }, [props.readOnly]);

  React.useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal || !props.autoFocus || props.readOnly) return;
    terminal.focus();
  }, [props.autoFocus, props.focusRequestNonce, props.readOnly]);

  React.useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal) return;
    const screenFrameSeq = props.screenFrameSeq ?? 0;
    if (screenFrameSeq <= lastScreenFrameSeqRef.current
      || props.screenText === null
      || props.screenText === undefined) return;
    const normalizedScreen = props.screenText.replace(/\r?\n/g, "\r\n");
    terminal.write(`\x1b[2J\x1b[H${normalizedScreen}`);
    lastScreenFrameSeqRef.current = screenFrameSeq;
  }, [props.screenFrameSeq, props.screenText]);

  React.useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal) return;
    let newestRenderedSequence = 0;
    for (const chunk of props.chunks) {
      if (chunk.seq <= lastRenderedSeqRef.current) continue;
      terminal.write(chunk.text);
      lastRenderedSeqRef.current = chunk.seq;
      newestRenderedSequence = chunk.seq;
    }
    if (newestRenderedSequence > 0) {
      callbacksRef.current.onChunksRendered?.(newestRenderedSequence);
    }
  }, [props.chunks]);

  return (
    <div
      ref={hostRef}
      aria-label={props.ariaLabel}
      className="terminal-viewport"
      data-terminal-input-scope={props.readOnly ? undefined : "true"}
      data-workspace-input-terminal-id={props.readOnly ? undefined : props.inputOwnerId}
      onKeyDown={props.readOnly ? undefined : stopTerminalUiPropagation}
      onKeyUp={props.readOnly ? undefined : stopTerminalUiPropagation}
      onPaste={props.readOnly ? undefined : stopTerminalUiPropagation}
      role={props.readOnly ? "region" : "application"}
    />
  );
}

// stopTerminalUiPropagation lets xterm consume its target event before blocking outer WebUI controls.
// stopTerminalUiPropagation 先让 xterm 消费目标事件，再阻止外层 WebUI 控件处理它。
function stopTerminalUiPropagation(event: React.SyntheticEvent): void {
  event.stopPropagation();
}
