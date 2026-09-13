import { Buffer } from "node:buffer";
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import {
  mergeVisibleTerminalChunks,
  pruneRemovedProjectTerminalUiState,
  splitBinaryTerminalInput,
  splitUtf8TerminalInput,
} from "@/components/project/terminal/project-floating-terminal-layer";
import { acquireVisibleTerminalDisplayListener } from "@/components/terminal/terminal-display-listener";
import {
  clearProjectTerminalInputBuffer,
  clearProjectTerminalInputBuffers,
  getOrCreateProjectTerminalInputBuffer,
} from "@/components/project/terminal/project-terminal-input-buffer";

describe("project terminal data contracts", () => {
  // Verifies Unicode fragments never split a code point and preserve exact UTF-8 bytes.
  // 验证 Unicode 分片不会拆开码点，并保留精确 UTF-8 字节。
  it("preserves Unicode code-point boundaries", () => {
    const fragments = splitUtf8TerminalInput("a中🙂b", 4);
    expect(fragments.map((fragment) => fragment.event.utf8Data)).toEqual(["a中", "🙂", "b"]);
    expect(fragments.map((fragment) => fragment.byteLength)).toEqual([4, 4, 1]);
  });

  // Verifies xterm binary code units preserve their exact low-eight-bit representation.
  // 验证 xterm 二进制码元保留其精确低八位表示。
  it("preserves binary low-eight-bit bytes", () => {
    const fragments = splitBinaryTerminalInput(String.fromCharCode(0, 255, 0x1234), 2);
    const decoded = fragments.flatMap((fragment) => [
      ...Buffer.from(fragment.event.binaryDataBase64 ?? "", "base64"),
    ]);
    expect(decoded).toEqual([0, 255, 0x34]);
    expect(fragments.map((fragment) => fragment.byteLength)).toEqual([2, 1]);
  });

  // Verifies unacknowledged input survives visible-pane remounts and explicit close releases it.
  // 验证未确认输入可跨可见面板重挂载保留，且显式关闭会释放它。
  it("retains queued input across pane remounts until explicit close", () => {
    const projectId = "project-buffer-test";
    const terminalId = "terminal-buffer-test";
    clearProjectTerminalInputBuffer(projectId, terminalId);

    const firstMount = getOrCreateProjectTerminalInputBuffer(projectId, terminalId, 4);
    firstMount.pending.push({ seq: 5, event: { utf8Data: "中" }, byteLength: 3 });
    firstMount.pendingBytes = 3;
    firstMount.nextClientSeq = 6;
    firstMount.sending = true;
    firstMount.batchSettled = Promise.resolve();

    const remounted = getOrCreateProjectTerminalInputBuffer(projectId, terminalId, 1);
    expect(remounted).toBe(firstMount);
    expect(remounted.pending).toEqual([
      { seq: 5, event: { utf8Data: "中" }, byteLength: 3 },
    ]);
    expect(remounted.pendingBytes).toBe(3);
    expect(remounted.nextClientSeq).toBe(6);
    expect(remounted.sending).toBe(true);

    clearProjectTerminalInputBuffer(projectId, terminalId);
    const afterClose = getOrCreateProjectTerminalInputBuffer(projectId, terminalId, 9);
    expect(afterClose).not.toBe(firstMount);
    expect(afterClose.pending).toEqual([]);
    expect(afterClose.pendingBytes).toBe(0);
    expect(afterClose.nextClientSeq).toBe(10);
    expect(afterClose.sending).toBe(false);

    const secondTerminal = getOrCreateProjectTerminalInputBuffer(projectId, "terminal-buffer-test-2", 0);
    secondTerminal.pending.push({ seq: 1, event: { utf8Data: "x" }, byteLength: 1 });
    clearProjectTerminalInputBuffers(projectId);
    expect(getOrCreateProjectTerminalInputBuffer(projectId, terminalId, 12)).not.toBe(afterClose);
    expect(getOrCreateProjectTerminalInputBuffer(projectId, "terminal-buffer-test-2", 8)).not.toBe(secondTerminal);

    clearProjectTerminalInputBuffers(projectId);
  });

  // Verifies authoritative project removal prunes stale floating UI and open-request state.
  // 验证权威项目移除会裁剪陈旧浮动界面与打开请求状态。
  it("prunes floating terminal state for removed projects", () => {
    const projectStates = new Map([
      ["retained", { expanded: true }],
      ["removed", { expanded: false }],
    ]);
    const seenRequests = new Map([
      ["retained", 2],
      ["removed", 1],
    ]);

    pruneRemovedProjectTerminalUiState(projectStates, seenRequests, [{ id: "retained" }]);

    expect([...projectStates.keys()]).toEqual(["retained"]);
    expect([...seenRequests.keys()]).toEqual(["retained"]);
  });

  // Verifies the top terminal action uses the same visibility request path as the floating window.
  // 验证顶部终端操作与浮动窗口使用同一条可见性请求链路。
  it("toggles the floating terminal from the top-bar action", () => {
    const componentSource = readFileSync(
      new URL("./project-floating-terminal-layer.tsx", import.meta.url),
      "utf8",
    );
    const workbenchSource = readFileSync(
      new URL("../../workspace/workbench-layout.tsx", import.meta.url),
      "utf8",
    );
    const shellSource = readFileSync(
      new URL("../../workspace/workspace-shell.tsx", import.meta.url),
      "utf8",
    );

    expect(componentSource).toContain('props.openRequest.action === "toggle"');
    expect(componentSource).toContain("onClick={minimizeProjectTerminal}");
    expect(workbenchSource).toContain("action: \"toggle\",");
    expect(workbenchSource).toContain("onToggleProjectTerminal={handleToggleActiveProjectTerminal}");
    expect(shellSource).toContain("onClick={props.onToggleProjectTerminal}");
  });

  // Verifies the running-terminal confirmation is above every dynamically activated floating terminal.
  // 验证运行中终端确认弹窗始终位于动态激活的浮动终端之上。
  it("raises the terminal lifecycle confirmation layer", () => {
    const componentSource = readFileSync(
      new URL("./project-floating-terminal-layer.tsx", import.meta.url),
      "utf8",
    );
    const alertDialogSource = readFileSync(
      new URL("../../../components/ui/alert-dialog.tsx", import.meta.url),
      "utf8",
    );

    expect(componentSource).toContain('<AlertDialogContent layerClassName="z-[1000]">');
    expect(alertDialogSource).toContain("layerClassName?: string");
    expect(alertDialogSource).toContain("<AlertDialogOverlay className={layerClassName} />");
  });

  // Verifies React batching deduplicates output and exposes a bounded-history gap explicitly.
  // 验证 React 批处理去重输出，并明确显示有界历史缺口。
  it("deduplicates pushed chunks and marks an eviction gap", () => {
    const merged = mergeVisibleTerminalChunks(
      [{ seq: 3, stream: "stdout", text: "old" }],
      [
        { seq: 3, stream: "stdout", text: "duplicate" },
        { seq: 4, stream: "stdout", text: "new" },
      ],
      0,
    );
    expect(merged.map((chunk) => chunk.seq)).toEqual([3, 4]);

    const gap = mergeVisibleTerminalChunks(
      [],
      [{ seq: 6, stream: "stdout", text: "retained" }],
      5,
    );
    expect(gap.map((chunk) => chunk.seq)).toEqual([5, 6]);
    expect(gap[0].text).toContain("较早的终端输出");
  });

  // Verifies every nested floating-terminal body keeps a definite height for the absolute xterm viewport.
  // 验证浮动终端的每层嵌套主体都为绝对定位的 xterm 视口保持确定高度。
  it("keeps the floating terminal viewport height chain definite", () => {
    // terminalStyles is the production stylesheet that owns all three nested container selectors.
    // terminalStyles 是持有三层嵌套容器选择器的生产样式表。
    const terminalStyles = readFileSync(
      new URL("../../../styles/terminal.css", import.meta.url),
      "utf8",
    );
    expect(terminalStyles).toMatch(
      /\.project-floating-surface-body,\s*\.project-terminal-body,\s*\.project-terminal-stage,\s*\.project-terminal-pane\s*\{[^}]*width:\s*100%;[^}]*height:\s*100%;/,
    );
  });

  // Verifies one floating terminal frame mounts only its selected xterm and disables the old pane immediately.
  // 验证一个浮动终端框仅挂载选中的 xterm，并立即禁用旧面板。
  it("keeps input, output, visibility, and focus on one selected terminal", () => {
    // componentSource owns selected-pane identity, display attachment, and input routing.
    // componentSource 持有选中面板标识、显示挂接与输入路由。
    const componentSource = readFileSync(
      new URL("./project-floating-terminal-layer.tsx", import.meta.url),
      "utf8",
    );
    // viewportSource owns the repeated-focus request applied to the one interactive xterm.
    // viewportSource 持有应用于唯一交互 xterm 的重复焦点请求。
    const viewportSource = readFileSync(
      new URL("../../terminal/terminal-viewport-runtime.tsx", import.meta.url),
      "utf8",
    );

    expect(componentSource).toContain("data-selected-terminal-id={selectedTerminal?.id}");
    expect(componentSource).toContain("key={selectedTerminal.id}");
    expect(componentSource).not.toContain("catalog.terminals.map((terminal) => (\n                    <ProjectTerminalPane");
    expect(componentSource).toContain('setWorkspaceInputOwner({ kind: "project_terminal", terminalId });');
    expect(componentSource).toContain("terminalFocusRequestNonce: current.terminalFocusRequestNonce + 1");
    expect(componentSource).toContain("readOnly={!props.visible");
    expect(componentSource).toContain('t("workspaceAudit.connecting")');
    expect(viewportSource).toContain("[props.autoFocus, props.focusRequestNonce, props.readOnly]");
  });

  // Verifies a pane hidden during asynchronous topic setup releases the late listener instead of leaking it.
  // 验证面板在异步主题建立期间隐藏时，会释放迟到的监听器而不是泄漏它。
  it("releases a display listener that resolves after tab switch", async () => {
    // resolveSubscription completes the deliberately delayed browser topic subscription.
    // resolveSubscription 完成被刻意延迟的浏览器主题订阅。
    let resolveSubscription: (unsubscribe: () => void) => void = () => {
      throw new Error("Display-listener test subscription was not initialized.");
    };
    // subscription creates the same pending promise shape returned by the browser transport.
    // subscription 创建与浏览器传输返回值相同形态的待完成 Promise。
    const subscription = new Promise<() => void>((resolve) => {
      resolveSubscription = resolve;
    });
    // cancelled models selecting a different terminal before the listener becomes available.
    // cancelled 模拟监听器可用前已选择另一个终端。
    let cancelled = false;
    // releaseCount records disposal of the listener that arrived too late.
    // releaseCount 记录迟到监听器的释放次数。
    let releaseCount = 0;
    // retainedCount records whether an invalid listener reached the active cleanup slot.
    // retainedCount 记录无效监听器是否进入活动清理槽位。
    let retainedCount = 0;
    // acquisition is the in-flight lifecycle operation under test.
    // acquisition 是本测试中的进行中生命周期操作。
    const acquisition = acquireVisibleTerminalDisplayListener(
      () => subscription,
      () => cancelled,
      () => {
        retainedCount += 1;
      },
    );

    cancelled = true;
    resolveSubscription(() => {
      releaseCount += 1;
    });

    await expect(acquisition).resolves.toBe(false);
    expect(releaseCount).toBe(1);
    expect(retainedCount).toBe(0);
  });

  // Verifies a still-visible terminal retains exactly one listener for its normal cleanup path.
  // 验证仍可见的终端恰好保留一个监听器供正常清理路径使用。
  it("retains a display listener while the surface remains visible", async () => {
    // releaseCount records disposal after the owner invokes its retained cleanup handle.
    // releaseCount 记录所有者调用保留的清理句柄后的释放次数。
    let releaseCount = 0;
    // retainedListeners stores the exact cleanup handle accepted by the visible surface.
    // retainedListeners 保存可见表面接收的精确清理句柄。
    const retainedListeners: Array<() => void> = [];

    await expect(acquireVisibleTerminalDisplayListener(
      async () => () => {
        releaseCount += 1;
      },
      () => false,
      (unsubscribe) => retainedListeners.push(unsubscribe),
    )).resolves.toBe(true);

    expect(retainedListeners).toHaveLength(1);
    retainedListeners[0]();
    expect(releaseCount).toBe(1);
  });

  // Verifies terminal switching and lifecycle controls live in the requested vertical sidebar.
  // 验证终端切换与生命周期控制位于要求的左侧竖向列表中。
  it("uses a vertical terminal list with row actions and a current-title header", () => {
    // componentSource is the production component contract guarded by this regression test.
    // componentSource 是本回归测试保护的生产组件契约。
    const componentSource = readFileSync(
      new URL("./project-floating-terminal-layer.tsx", import.meta.url),
      "utf8",
    );
    expect(componentSource).toContain('className="project-terminal-current-title"');
    expect(componentSource).toContain('className="project-terminal-sidebar"');
    expect(componentSource).toContain('className="project-terminal-session-list"');
    expect(componentSource).toContain('aria-orientation="vertical"');
    expect(componentSource).toContain('className="project-terminal-sidebar-add"');
    expect(componentSource).toContain('className="project-terminal-session-actions"');
    expect(componentSource).toContain('title={t("workspaceAudit.stopTitle")}');
    expect(componentSource).toContain('title={t("workspaceAudit.closeTitle")}');
    expect(componentSource).toContain('floatingControls.maximized ? "workspaceAudit.restore" : "workspaceAudit.maximize"');
    expect(componentSource).toContain("projectTerminalUiStateCache");
    expect(componentSource).toContain("projectTerminalSeenRequests");
    expect(componentSource).toContain("sidebarCollapsed: false");
    expect(componentSource).toContain('className="project-terminal-compact-session-list"');
    expect(componentSource).toContain('{index + 1}');
    expect(componentSource).toContain('data-sidebar-collapsed={sidebarCollapsed ? "true" : "false"}');
    expect(componentSource).not.toContain('className="project-terminal-tab-list"');
    expect(componentSource).not.toContain('aria-label="选择项目终端"');
  });

  // Verifies the shell selector has explicit contrast and terminal names may use the available row width.
  // 验证 Shell 选择器具有明确对比度，且终端名称可使用列表行的可用宽度。
  it("keeps shell labels readable and terminal names fully discoverable", () => {
    // terminalStyles is the exact production stylesheet for selector contrast and name wrapping.
    // terminalStyles 是控制选择器对比度与名称换行的生产样式表。
    const terminalStyles = readFileSync(
      new URL("../../../styles/terminal.css", import.meta.url),
      "utf8",
    );
    expect(terminalStyles).toMatch(
      /\.project-terminal-select-trigger\s*\{[^}]*color:\s*var\(--foreground\);[^}]*background:\s*var\(--background\);/,
    );
    expect(terminalStyles).toMatch(
      /\.project-terminal-session-name\s*\{[^}]*overflow-wrap:\s*anywhere;[^}]*white-space:\s*normal;/,
    );
  });

  // Verifies native resize dragging orders immediate latest-wins PTY grids before subsequent input.
  // 验证原生尺寸拖动会在后续输入前排序即时且最新值优先的 PTY 网格。
  it("serializes latest PTY resize requests before terminal input", () => {
    // componentSource is the exact production resize coordinator guarded by this regression test.
    // componentSource 是本回归测试保护的生产尺寸协调器。
    const componentSource = readFileSync(
      new URL("./project-floating-terminal-layer.tsx", import.meta.url),
      "utf8",
    );
    // viewportSource owns exact visible-grid repainting after every xterm fit.
    // viewportSource 持有每次 xterm 拟合后的精确可见网格重绘。
    const viewportSource = readFileSync(
      new URL("../../terminal/terminal-viewport-runtime.tsx", import.meta.url),
      "utf8",
    );
    expect(componentSource).not.toContain("PROJECT_TERMINAL_RESIZE_SETTLE_MS");
    expect(componentSource).not.toContain("resizeSettleTimer");
    expect(componentSource).not.toContain("initialResizeDelivered");
    expect(componentSource).toContain("const resizeSettlement = React.useRef<ProjectTerminalResizeSettlement | null>(null);");
    expect(componentSource).toContain("const resizeBarrier = resizeSettlement.current?.promise;");
    expect(componentSource).toContain("if (resizeBarrier) await resizeBarrier;");
    expect(componentSource).toContain(
      "const projectTerminalResizeRequestTails = new Map<string, Promise<void>>();",
    );
    expect(componentSource).toContain("await resizeProjectTerminalInOrder(");
    expect(componentSource).toContain(".then(() => resizeProjectTerminal(projectId, terminalId, rows, cols));");
    expect(componentSource).toContain("if (!resizeDisposed.current && pendingResize.current) {");
    expect(componentSource).toContain("void flushResize();");
    expect(componentSource).not.toContain("while (pendingResize.current)");
    expect(viewportSource).toContain(
      "const wasAtBottom = terminal.buffer.active.viewportY === terminal.buffer.active.baseY;",
    );
    expect(viewportSource).toContain("if (wasAtBottom) terminal.scrollToBottom();");
    expect(viewportSource).toContain("terminal.refresh(0, rows - 1);");
  });

  // Verifies the toolbar badge uses count-only lifecycle pushes and one revisioned metadata bootstrap.
  // 验证工具栏徽标使用纯数量生命周期推送以及一次带修订号的元数据初始加载。
  it("shows a pushed running-terminal count without subscribing to hidden terminal content", () => {
    // layoutSource owns the count subscription and rejects stale initial-query results.
    // layoutSource 持有数量订阅，并拒绝过期的首次查询结果。
    const layoutSource = readFileSync(
      new URL("../../workspace/workbench-layout.tsx", import.meta.url),
      "utf8",
    );
    // shellSource owns the visible badge attached to the top terminal button.
    // shellSource 持有附着于顶部终端按钮的可见徽标。
    const shellSource = readFileSync(
      new URL("../../workspace/workspace-shell.tsx", import.meta.url),
      "utf8",
    );
    // transportSource maps the count-only service event independently from display content.
    // transportSource 独立于显示内容映射纯数量服务事件。
    const transportSource = readFileSync(
      new URL("../../../service/browserTransport.ts", import.meta.url),
      "utf8",
    );
    expect(layoutSource).toContain('"project.terminal.running-count.changed"');
    expect(layoutSource).toContain("previous.revision > catalog.runningCountRevision");
    expect(layoutSource).toContain("projectTerminalCountListenerReady");
    expect(layoutSource).not.toContain("setInterval(");
    expect(shellSource).toContain('size={props.projectTerminalRunningCount > 0 ? "sm" : "icon-sm"}');
    expect(shellSource).toContain('<Badge aria-hidden="true" variant="secondary">');
    expect(shellSource).toContain("{props.projectTerminalRunningCount}");
    expect(shellSource).toContain("props.projectTerminalRunningCount > 0");
    expect(shellSource).not.toContain('"99+"');
    expect(transportSource).toContain(
      'projectTerminalRunningCountChanged: "project.terminal.running-count.changed"',
    );
    expect(shellSource.indexOf("<Badge")).toBeLessThan(shellSource.indexOf("</Button>"));
  });
});
