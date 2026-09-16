import { describe, expect, it } from "vitest";

import type { ConversationRenderItem } from "@/components/conversation/conversation-model";
import { groupVisibleProcessBodyEntries } from "@/components/conversation/conversation-render-items";

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
    expect(terminal[0]).toMatchObject({ kind: "tool-sequence", active: false });
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
