import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import {
  clampFloatingLayout,
  createDefaultProjectFloatingLayout,
  createProjectFloatingGenieKeyframes,
  floatingVisibilityTransitionDirection,
  maximizedFloatingLayout,
  settledFloatingLayerState,
} from "@/components/workspace/project-floating-layer";

describe("project floating layer geometry", () => {
  // Verifies restored or dragged geometry remains wholly inside the current workspace.
  // 验证恢复或拖动后的几何始终完整位于当前工作区内。
  it("clamps position and size to workspace bounds", () => {
    expect(clampFloatingLayout(
      { x: 900, y: -20, width: 700, height: 500 },
      1000,
      600,
    )).toEqual({ x: 300, y: 0, width: 700, height: 500 });
    expect(clampFloatingLayout(
      { x: 10, y: 10, width: 900, height: 700 },
      480,
      300,
    )).toEqual({ x: 0, y: 0, width: 480, height: 300 });
  });

  // Verifies maximize occupies the exact workspace and unchanged remount state does not replay restore animation.
  // 验证最大化精确占满工作区，且重挂载后的未变化状态不会重播还原动画。
  it("maximizes exactly without replaying expansion on remount", () => {
    expect(maximizedFloatingLayout(1440, 900)).toEqual({
      x: 0,
      y: 0,
      width: 1440,
      height: 900,
    });
    expect(floatingVisibilityTransitionDirection(true, true)).toBeNull();
    expect(floatingVisibilityTransitionDirection(false, false)).toBeNull();
    expect(floatingVisibilityTransitionDirection(false, true)).toBe("restore");
    expect(floatingVisibilityTransitionDirection(true, false)).toBe("minimize");
    // componentSource confirms maximize distinguishes a pending native resize from a display-only boundary projection.
    // componentSource 确认最大化会区分待保存原生缩放与仅供显示的边界投影。
    const componentSource = readFileSync(
      new URL("./project-floating-layer.tsx", import.meta.url),
      "utf8",
    );
    expect(componentSource).toContain("const measuredNormalLayout = clampFloatingLayout(");
    expect(componentSource).toContain(
      "floatingLayoutsEqual(expectedRenderedLayoutRef.current, measuredNormalLayout)",
    );
    expect(componentSource).toContain("(parsed.maximized && !parsedRestoreLayout)");
  });

  // Verifies every project starts from the same lower-right rule and container projection never overwrites that preference.
  // 验证每个项目使用相同右下规则启动，且容器投影绝不会覆盖该偏好。
  it("keeps deterministic project geometry below the conversation title bar", () => {
    expect(createDefaultProjectFloatingLayout(1200, 800)).toEqual({
      x: 424,
      y: 310,
      width: 760,
      height: 464,
    });
    // componentSource is the production separation between desired project geometry and current display projection.
    // componentSource 是项目期望几何与当前显示投影之间的生产级隔离合同。
    const componentSource = readFileSync(
      new URL("./project-floating-layer.tsx", import.meta.url),
      "utf8",
    );
    // terminalStyles owns the safe boundary below the original conversation header.
    // terminalStyles 持有原会话标题栏下方的安全边界。
    const terminalStyles = readFileSync(
      new URL("../../styles/terminal.css", import.meta.url),
      "utf8",
    );
    // shellStyles owns the shared conversation-header height inherited by both layout consumers.
    // shellStyles 持有由两个布局使用方共同继承的会话标题栏高度。
    const shellStyles = readFileSync(
      new URL("../../styles/shell.css", import.meta.url),
      "utf8",
    );
    expect(componentSource).toContain("const boundaryChanged = entries.some((entry) => entry.target === layer);");
    expect(componentSource).toContain("if (boundaryChanged) {");
    expect(componentSource).toContain("expectedRenderedLayoutRef.current");
    expect(componentSource).toContain("version: 3");
    expect(terminalStyles).toMatch(
      /\.project-floating-layer\s*\{[^}]*top:\s*var\(--workspace-header-height\);[^}]*right:\s*0;[^}]*bottom:\s*0;[^}]*left:\s*0;/,
    );
    expect(shellStyles).toMatch(/\.workspace\s*\{[^}]*--workspace-header-height:\s*46px;/);
    expect(terminalStyles).not.toMatch(/\.project-floating-layer\s*\{[^}]*inset:\s*0;/);
  });

  // Verifies a top toolbar anchor produces a narrowing Genie funnel and an exact reversible endpoint order.
  // 验证顶部工具栏锚点会生成收窄的精灵漏斗，并具有精确可逆的端点顺序。
  it("builds reversible macOS-style Genie keyframes toward the toolbar button", () => {
    // surface models a floating terminal below the top toolbar.
    // surface 模拟位于顶部工具栏下方的浮动终端。
    const surface = { left: 200, top: 120, width: 800, height: 500 };
    // anchor models the small terminal button immediately left of the right-panel control.
    // anchor 模拟紧邻右侧栏控件左边的小终端按钮。
    const anchor = { left: 920, top: 12, width: 28, height: 28 };
    // minimizing is the ordered window-to-button deformation.
    // minimizing 是从窗口到按钮的有序形变。
    const minimizing = createProjectFloatingGenieKeyframes(surface, anchor, "minimize");
    // restoring is the exact semantic reverse from button to window.
    // restoring 是从按钮到窗口的精确语义反向过程。
    const restoring = createProjectFloatingGenieKeyframes(surface, anchor, "restore");

    expect(minimizing).toHaveLength(4);
    expect(minimizing[0].offset).toBe(0);
    expect(minimizing[3].offset).toBe(1);
    expect(String(minimizing[1].clipPath)).toContain("polygon(");
    expect(String(minimizing[3].transform)).toContain("translate3d(");
    expect(restoring[0].transform).toBe(minimizing[3].transform);
    expect(restoring[3].transform).toBe(minimizing[0].transform);
    expect(restoring[0].clipPath).toBe(minimizing[3].clipPath);
    expect(restoring[3].clipPath).toBe(minimizing[0].clipPath);
  });

  // Verifies interrupted animations always settle and an opening surface never disables pointer input.
  // 验证被中断的动画始终收敛，且正在展开的表面不会禁用指针输入。
  it("settles cancelled animation state without freezing the floating surface", () => {
    expect(settledFloatingLayerState("restore")).toBe("open");
    expect(settledFloatingLayerState("minimize")).toBe("closed");
    // terminalStyles is the production pointer-isolation contract for the floating surface.
    // terminalStyles 是浮动表面的生产指针隔离契约。
    const terminalStyles = readFileSync(
      new URL("../../styles/terminal.css", import.meta.url),
      "utf8",
    );
    // transitionRule covers both animated states and must contain rendering hints only.
    // transitionRule 覆盖两个动画状态，并且只能包含渲染提示。
    const transitionRule = terminalStyles.match(
      /\.project-floating-surface\[data-animation-state="opening"\],\s*\.project-floating-surface\[data-animation-state="closing"\]\s*\{([^}]*)\}/,
    )?.[1] ?? "";
    expect(transitionRule).not.toContain("pointer-events");
    expect(terminalStyles).toMatch(
      /\.project-floating-surface\[data-animation-state="closing"\]\s*\{[^}]*pointer-events:\s*none;/,
    );
    expect(terminalStyles).toMatch(
      /\.project-floating-surface\[data-maximized="true"\]\s*\{[^}]*min-width:\s*0;[^}]*min-height:\s*0;[^}]*resize:\s*none;/,
    );
  });
});
