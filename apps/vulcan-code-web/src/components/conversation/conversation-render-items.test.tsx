import { describe, expect, it } from "vitest";

import type { ConversationRenderItem } from "@/components/conversation/conversation-model";
import { groupVisibleProcessBodyEntries } from "@/components/conversation/conversation-render-items";
import { projectConversationTimelineItems } from "./conversation-timeline";

// toolRenderItem builds one mergeable terminal tool row for sequence-grouping tests.
// toolRenderItem 构建一条可合并的终态工具记录，用于工具序列分组测试。
function toolRenderItem(id: string, status: "completed" | "failed" | "stopped"): ConversationRenderItem {
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
      return groupVisibleProcessBodyEntries(process.id, process.items, running, process.collapsible)
        .find((entry) => entry.kind === "tool-sequence");
    });
    expect(entries[0]).toMatchObject({ kind: "tool-sequence", active: false });
    expect(entries[1]).toEqual(entries[0]);
  });

  it("keeps the live tail open even after every tool completes, until a following assistant establishes the boundary", () => {
    // assistant marks the exact visible boundary after the first tool batch.
    // assistant 标记第一批工具之后的精确可见边界。
    const assistant: ConversationRenderItem = { id: "reply", role: "assistant", sourceMessages: [], blocks: [
      { id: "reply-block", kind: "markdown", title: "", body: "Next step", summary: "" },
    ] };
    // tail remains active without relying on elapsed time or individual tool outcomes.
    // tail 保持活动，不依赖经过时间或单个工具结果。
    const tail = groupVisibleProcessBodyEntries("p", [toolRenderItem("first", "completed")], true);
    expect(tail[0]).toMatchObject({ kind: "tool-sequence", active: true });
    const next = groupVisibleProcessBodyEntries("p", [toolRenderItem("first", "completed"), assistant, toolRenderItem("next", "completed")], true);
    expect(next[0]).toMatchObject({ kind: "tool-sequence", active: false });
    expect(next[2]).toMatchObject({ kind: "tool-sequence", active: true });
    const terminal = groupVisibleProcessBodyEntries("p", [toolRenderItem("first", "completed")], true, false);
    expect(terminal[0]).toMatchObject({ kind: "tool-sequence", active: true });
    const extractedFinal = groupVisibleProcessBodyEntries("p", [toolRenderItem("first", "completed")], false, true);
    expect(extractedFinal[0]).toMatchObject({ kind: "tool-sequence", active: false });
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
    expect(entries[0].completedItems.map((item) => item.id)).toEqual([
      "completed-tool",
      "failed-tool",
      "stopped-tool",
    ]);
    expect(entries[0].runningItems).toEqual([]);
  });
});
