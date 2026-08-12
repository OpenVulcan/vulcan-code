import React from "react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

import type { TerminalViewportProps } from "./terminal-viewport";

// TerminalViewportRuntime owns one stable xterm instance and event-driven ResizeObserver fitting.
// TerminalViewportRuntime 持有一个稳定 xterm 实例，并使用事件驱动的 ResizeObserver 拟合。
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
    // terminal is the only emulator instance owned by this mounted viewport.
    // terminal 是当前已挂载视口拥有的唯一模拟器实例。
    const terminal = new Terminal({
      allowProposedApi: false,
      convertEol: false,
      cursorBlink: !callbacksRef.current.readOnly,
      cursorStyle: callbacksRef.current.readOnly ? "bar" : "block",
      disableStdin: callbacksRef.current.readOnly,
      fontFamily: '"Cascadia Mono", "SFMono-Regular", Consolas, "Liberation Mono", monospace',
      fontSize: 13,
      lineHeight: 1.15,
      scrollback: 10_000,
      theme: {
        background: "#111318",
        foreground: "#d9dee8",
        cursor: "#f0f3f8",
        cursorAccent: "#111318",
        selectionBackground: "#4e6b955c",
      },
    });
    // fitAddon converts container pixels into the terminal character grid.
    // fitAddon 将容器像素转换为终端字符网格。
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(host);
    terminalRef.current = terminal;
    // lastDimensions suppresses duplicate backend resize requests.
    // lastDimensions 抑制重复的后端尺寸调整请求。
    let lastDimensions = { rows: 0, cols: 0 };
    // animationFrame coalesces synchronous layout changes without interval polling.
    // animationFrame 合并同步布局变化，不使用定时轮询。
    let animationFrame: number | null = null;

    // fit measures the visible host and reports only changed positive dimensions.
    // fit 测量可见宿主，并仅报告变化后的正尺寸。
    const fit = () => {
      animationFrame = null;
      if (host.clientWidth <= 0 || host.clientHeight <= 0) return;
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
    // resizeObserver reacts only to actual viewport geometry changes.
    // resizeObserver 仅响应真实视口几何变化。
    const resizeObserver = new ResizeObserver(scheduleFit);
    resizeObserver.observe(host);
    const dataDisposable = terminal.onData((data) => {
      if (!callbacksRef.current.readOnly) callbacksRef.current.onData?.(data);
    });
    const binaryDisposable = terminal.onBinary((data) => {
      if (!callbacksRef.current.readOnly) callbacksRef.current.onBinary?.(data);
    });
    scheduleFit();

    return () => {
      resizeObserver.disconnect();
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
