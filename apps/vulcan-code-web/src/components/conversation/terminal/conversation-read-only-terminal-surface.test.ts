import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { conversationTerminalFloatingIdentity } from "@/components/conversation/terminal/conversation-read-only-terminal-surface";

describe("conversation read-only terminal surface", () => {
  // Verifies every AI terminal receives isolated floating geometry even when display names match.
  // 验证每个 AI 终端即使显示名称相同也会获得隔离的浮动几何。
  it("builds stable owner and terminal scoped floating identities", () => {
    expect(conversationTerminalFloatingIdentity("terminal-a", "session-a"))
      .toBe('["ai-terminal","session-a","terminal-a"]');
    expect(conversationTerminalFloatingIdentity("terminal-a", "session-a"))
      .not.toBe(conversationTerminalFloatingIdentity("terminal-a", "session-b"));
  });

  // Verifies AI terminals reuse the draggable animated window while retaining display-only fixed-grid behavior.
  // 验证 AI 终端复用可拖动动画窗口，同时保持只显示的固定网格行为。
  it("renders independent read-only floating windows without a terminal selector", () => {
    // surfaceSource owns visible-only attachment and floating-window composition.
    // surfaceSource 持有仅可见时挂接与浮动窗口组合。
    const surfaceSource = readFileSync(
      new URL("./conversation-read-only-terminal-surface.tsx", import.meta.url),
      "utf8",
    );
    // viewportSource owns the fixed-grid branch that deliberately omits FitAddon and ResizeObserver.
    // viewportSource 持有刻意省略 FitAddon 与 ResizeObserver 的固定网格分支。
    const viewportSource = readFileSync(
      new URL("../../terminal/terminal-viewport-runtime.tsx", import.meta.url),
      "utf8",
    );

    expect(surfaceSource).toContain("return createPortal(");
    expect(surfaceSource).toContain("<ProjectFloatingLayer");
    expect(surfaceSource).toContain("fixedRows={liveSummary?.screenRows ?? props.summary?.screenRows}");
    expect(surfaceSource).toContain("fixedCols={liveSummary?.screenCols ?? props.summary?.screenCols}");
    expect(surfaceSource).toContain("readOnly");
    expect(surfaceSource).toContain("!props.open || !props.summary || !connectionAvailable || !workspaceHost");
    expect(surfaceSource).not.toContain("<Dialog");
    expect(surfaceSource).not.toContain("project-terminal-sidebar");
    expect(surfaceSource).not.toContain("onResize=");
    expect(viewportSource).toContain("const fitAddon = fixedGrid ? null : new FitAddon();");
    expect(viewportSource).toContain("if (fixedGrid) {");
    expect(viewportSource).toContain("resizeObserver = new ResizeObserver(scheduleFit);");
  });
});
