// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { TurnChangeSummaryV1 } from "@/diff/turn-change-summary-v1";
import { TurnStatsBlock } from "./conversation-blocks";
import { TurnChangeReviewActionsContext, TurnChangeReviewTargetContext } from "./turn-change-review-context";

// host and root isolate one rendered statistics row.
// host 和 root 隔离渲染的一条统计行。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// CHANGES is a Turn that touched more files than the list shows at first.
// CHANGES 是一个改动文件数多于列表初始显示数量的 Turn。
const CHANGES: TurnChangeSummaryV1 = {
  files: Array.from({ length: 7 }, (_, index) => ({
    path: `src/file-${index}.ts`, renamedFrom: null, status: "modified" as const, additions: index + 1, deletions: 1,
  })),
  additions: 28,
  deletions: 7,
};

// mount renders one statistics row for Turn 5 with the given review target and change summary.
// mount 为 Turn 5 渲染一条统计行，带指定审核目标与变更汇总。
async function mount(options: { changes?: TurnChangeSummaryV1; targetTurnId?: number; open?: () => void }) {
  const block = {
    id: "stats", kind: "turn-stats", title: "Turn statistics", summary: "1 calls", body: "",
    rawMetadata: {
      v1_turn_id: 5,
      turn_stats: { provider_call_count: 1, tool_call_count: 2 },
      ...(options.changes ? { turn_changes: options.changes } : {}),
    },
  };
  await act(async () => root.render(
    <I18nProvider initialLocale="en-US">
      <TooltipProvider>
        <TurnChangeReviewActionsContext.Provider value={{ sessionId: "s", openLastTurnChanges: options.open }}>
          <TurnChangeReviewTargetContext.Provider value={{ sessionId: "s", turnId: options.targetTurnId ?? null }}>
            <TurnStatsBlock block={block} />
          </TurnChangeReviewTargetContext.Provider>
        </TurnChangeReviewActionsContext.Provider>
      </TooltipProvider>
    </I18nProvider>,
  ));
}

// toggle returns the change chip, or null when the row has none.
// toggle 返回变更胶囊；该行没有时返回 null。
function toggle(): HTMLButtonElement | null {
  return host.querySelector<HTMLButtonElement>(".turn-changes-toggle");
}

// openList clicks the change chip and waits for the lazily loaded list.
// openList 点击变更胶囊并等待懒加载列表。
async function openList() {
  await act(async () => { toggle()!.click(); });
  await vi.waitFor(() => expect(host.querySelector(".turn-change-files")).not.toBeNull());
}

it("shows no change chip for a Turn without applied changes", async () => {
  await mount({});
  expect(toggle()).toBeNull();
});

it("lists the first files and reveals the rest on request", async () => {
  await mount({ changes: CHANGES });
  expect(toggle()!.getAttribute("aria-expanded")).toBe("false");
  await openList();
  expect(toggle()!.getAttribute("aria-expanded")).toBe("true");
  expect(host.querySelectorAll(".turn-change-files li")).toHaveLength(5);
  await act(async () => { host.querySelector<HTMLButtonElement>(".turn-change-files-more")!.click(); });
  expect(host.querySelectorAll(".turn-change-files li")).toHaveLength(7);
});

it("offers the Git review only for the Turn that review shows", async () => {
  const open = vi.fn();
  await mount({ changes: CHANGES, targetTurnId: 6, open });
  await openList();
  expect(host.querySelector(".turn-change-files-action")).toBeNull();

  await mount({ changes: CHANGES, targetTurnId: 5, open });
  const action = host.querySelector<HTMLButtonElement>(".turn-change-files-action");
  expect(action).not.toBeNull();
  await act(async () => { action!.click(); });
  expect(open).toHaveBeenCalledTimes(1);
});
