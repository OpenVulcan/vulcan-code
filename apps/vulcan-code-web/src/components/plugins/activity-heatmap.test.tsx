// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import { ActivityHeatmap, type ActivityHeatmapCell } from "./plugin-host-components";

// calendarCells creates a known ordered source range, including all zero-intensity dates unchanged.
// calendarCells 创建已知有序来源范围，包含全部保持不变的零强度日期。
// Parameters: days selects the actual range length. Returns: one exact cell for each consecutive UTC date.
// 参数：days 选择实际范围长度。返回值：每个连续 UTC 日期对应一个准确单元。
function calendarCells(days: number): ActivityHeatmapCell[] {
  return Array.from({ length: days }, (_, index) => {
    const date = new Date(Date.UTC(2026, 7, 14 + index)).toISOString().slice(0, 10);
    return { date, intensity: index % 6, label: `${date}: ${index} tokens` };
  });
}

it.each([7, 30, 90, 365])("renders the complete %i-day range without annual offsets or discarded dates", async (days) => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  const cells = calendarCells(days);
  const container = document.createElement("div"); document.body.appendChild(container);
  const root = createRoot(container);
  try {
    await act(async () => root.render(<TooltipProvider><ActivityHeatmap cells={cells} ariaLabel="Real selected range" /></TooltipProvider>));
    const content = container.querySelector<HTMLElement>(".plugin-activity-heatmap-content")!;
    const buttons = [...content.querySelectorAll('[role="gridcell"]')];
    expect(buttons.map((cell) => cell.getAttribute("aria-label"))).toEqual(cells.map((cell) => cell.label));
    expect(buttons.map((cell) => cell.getAttribute("data-intensity"))).toEqual(cells.map((cell) => String(cell.intensity)));
    expect(content.querySelector<HTMLElement>(".plugin-activity-month-labels span")!.style.gridColumnStart).toBe("1");
    if (days <= 31) {
      expect(content.dataset.layout).toBe("days");
      expect(content.querySelectorAll(".placeholder")).toHaveLength(0);
      expect(content.querySelectorAll(".plugin-activity-day-labels span")).toHaveLength(days);
      expect(content.querySelector<HTMLElement>('[role="grid"]')!.style.gridTemplateColumns).toBe(`repeat(${days}, minmax(18px, 1fr))`);
      if (days === 30) expect(content.querySelectorAll<HTMLElement>(".plugin-activity-month-labels span")[1].style.gridColumnStart).toBe("19");
    } else {
      expect(content.dataset.layout).toBe("weeks");
      expect(content.querySelectorAll(".placeholder")).toHaveLength(5);
      expect(content.querySelector(".plugin-activity-day-labels")).toBeNull();
      expect(content.querySelector<HTMLElement>(".plugin-activity-month-labels")!.style.gridTemplateColumns).toBe(`repeat(${Math.ceil((5 + days) / 7)}, 11px)`);
    }
  } finally { await act(async () => root.unmount()); container.remove(); }
});
