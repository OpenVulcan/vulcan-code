import { describe, expect, it } from "vitest";

import type { ConversationRenderItem } from "@/components/conversation/conversation-model";
import { groupVisibleProcessBodyEntries } from "@/components/conversation/conversation-render-items";
import { projectConversationTimelineItems } from "./conversation-timeline";

// toolRenderItem builds one mergeable tool row for sequence-grouping tests.
// toolRenderItem 构建一条可合并的工具记录，用于工具序列分组测试。
function toolRenderItem(id: string, status: "running" | "completed" | "failed" | "stopped"): ConversationRenderItem {
  return {
    id,
    role: "assistant",
    blocks: [{
      id: `${id}-block`,
      kind: "tool-call",
      title: "Tool call",
      body: "",
      summary: "",
      status,
      toolCallId: `${id}-call`,
      toolEvidence: true,
      toolName: "shell_command",
    }],
    sourceMessages: [],
  };
}

describe("groupVisibleProcessBodyEntries", () => {
  it("keeps the same fold boundary when the timeline extracts the final answer", () => {
    // rows replay a user turn through reasoning, tools, and final assistant output.
    // rows 重放用户回合中的思考、工具与最终助手输出。
    const rows: ConversationRenderItem[] = [
      { id: "user", role: "user", sourceMessages: [], blocks: [
        { id: "user-body", kind: "markdown", title: "", body: "Inspect", summary: "" },
      ] },
      toolRenderItem("first", "completed"),
      { id: "reasoning", role: "assistant", sourceMessages: [], blocks: [
        { id: "reasoning-body", kind: "thinking", title: "", body: "Checking", summary: "" },
      ] },
      toolRenderItem("second", "completed"),
      { id: "answer", role: "assistant", sourceMessages: [], blocks: [
        { id: "answer-body", kind: "markdown", title: "", body: "Done", summary: "" },
      ] },
    ];
    // entries compare live and terminal timeline projections without assuming fixed row indexes.
    // entries 比较实时与终态时间线投影，不假定固定行位置。
    const entries = [true, false].map((running) => {
      const processEntry = projectConversationTimelineItems(rows, undefined, running)
        .find((entry) => entry.kind === "process");
      if (!processEntry || processEntry.kind !== "process") throw new Error("Expected a projected process");
      const process = processEntry.process;
      return groupVisibleProcessBodyEntries(process.id, process.items, running)
        .find((entry) => entry.kind === "tool-sequence");
    });
    expect(entries[0]).toMatchObject({ kind: "tool-sequence", runningItems: [] });
    expect(entries[1]).toEqual(entries[0]);
  });

  it("lists only running tools as running items, keeping reasoning in the run body", () => {
    // reasoning sits between tools and must stay in order without counting as a running tool.
    // reasoning 位于工具之间，必须保持顺序且不算作运行中工具。
    const reasoning: ConversationRenderItem = { id: "reasoning", role: "assistant", sourceMessages: [], blocks: [
      { id: "reasoning-body", kind: "thinking", title: "", body: "Checking", summary: "" },
    ] };
    const [entry] = groupVisibleProcessBodyEntries(
      "p",
      [toolRenderItem("first", "completed"), reasoning, toolRenderItem("second", "running")],
      true,
    );
    if (entry?.kind !== "tool-sequence") throw new Error("Expected one grouped tool sequence.");
    expect(entry.items.map((item) => item.id)).toEqual(["first", "reasoning", "second"]);
    expect(entry.runningItems.map((item) => item.id)).toEqual(["second"]);
  });
  // Failed and stopped terminal rows belong to the same fold as successful rows after reload.
  // 重载后，失败与已停止的终态记录应和成功记录归入同一个折叠组。
  it("groups every terminal outcome without keeping failures outside", () => {
    const entries = groupVisibleProcessBodyEntries(
      "process-1",
      [
        toolRenderItem("completed-tool", "completed"),
        toolRenderItem("failed-tool", "failed"),
        toolRenderItem("stopped-tool", "stopped"),
      ],
      false,
    );

    expect(entries).toHaveLength(1);
    expect(entries[0]?.kind).toBe("tool-sequence");
    if (entries[0]?.kind !== "tool-sequence") {
      throw new Error("Expected one grouped tool sequence.");
    }
    expect(entries[0].items.map((item) => item.id)).toEqual([
      "completed-tool",
      "failed-tool",
      "stopped-tool",
    ]);
    expect(entries[0].runningItems).toEqual([]);
  });
});
