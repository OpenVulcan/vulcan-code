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
