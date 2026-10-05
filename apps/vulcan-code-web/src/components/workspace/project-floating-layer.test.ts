import { describe, expect, it } from "vitest";

import {
  clampFloatingLayout,
  createDefaultProjectFloatingLayout,
  createProjectFloatingGenieKeyframes,
  floatingVisibilityTransitionDirection,
  maximizedFloatingLayout,
  nextProjectFloatingLayerStackOrder,
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
  });

  // Verifies every project starts from the same deterministic lower-right rule.
  // 验证每个项目都使用相同的确定性右下规则启动。
  it("places a new project window at the deterministic lower-right position", () => {
    expect(createDefaultProjectFloatingLayout(1200, 800)).toEqual({
      x: 424,
      y: 310,
      width: 760,
      height: 464,
    });
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

  // Verifies interrupted animations always settle to the visibility they were heading toward.
  // 验证被中断的动画始终收敛到其目标可见状态。
  it("settles cancelled animation state to its target visibility", () => {
    expect(settledFloatingLayerState("restore")).toBe("open");
    expect(settledFloatingLayerState("minimize")).toBe("closed");
  });

  // Verifies independently mounted terminal windows can raise the most recently activated surface.
  // 验证独立挂载的终端窗口可将最近激活的表面提升到最前方。
  it("assigns monotonically increasing floating window stack order", () => {
    // firstOrder captures one activation before a second overlapping terminal is selected.
    // firstOrder 捕获第二个重叠终端被选择前的一次激活。
    const firstOrder = nextProjectFloatingLayerStackOrder();
    // secondOrder captures the next window activation and must be visually above the first.
    // secondOrder 捕获下一次窗口激活，并且必须在视觉上高于第一个窗口。
    const secondOrder = nextProjectFloatingLayerStackOrder();
    expect(secondOrder).toBeGreaterThan(firstOrder);
  });
});
