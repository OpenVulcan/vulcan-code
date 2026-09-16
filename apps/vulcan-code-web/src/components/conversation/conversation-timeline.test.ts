import { describe, expect, it } from "vitest";
import type {
  ConversationBlockKind,
  ConversationProcessRenderItem,
  ConversationRenderItem,
} from "./conversation-model";
import type { ConversationMessage } from "@/types";
import {
  conversationTimelineItemKey,
  projectConversationTimelineItems,
} from "./conversation-timeline";

// createRenderItem builds one minimal projected transcript row for identity tests.
// createRenderItem 构建一条用于身份测试的最小投影转录行。
// Parameters: id is the persisted identity and renderKey distinguishes repeated historical occurrences.
// 参数：id 是持久化身份，renderKey 用于区分重复出现的历史实例。
// Returns: a projected assistant row whose body may change without changing its business key.
// 返回值：正文可以变化但业务键保持不变的投影 Assistant 行。
function createRenderItem(id: string, renderKey?: string): ConversationRenderItem {
  // sourceMessage is the exact immutable transcript record represented by this projected row.
  // sourceMessage 是该投影行所表示的精确不可变转录记录。
  const sourceMessage: ConversationMessage = {
    body: "",
    conversationId: "conversation-test",
    id,
    role: "assistant",
  };
  return {
    blocks: [],
    id,
    renderKey,
    role: "assistant",
    sourceMessage,
    sourceMessages: [sourceMessage],
  };
}

// createProcessItem builds one minimal process group for stable-key tests.
// createProcessItem 构建一个用于稳定键测试的最小处理过程分组。
// Parameters: id is the process identity and renderKey is its optional occurrence identity.
// 参数：id 是处理过程身份，renderKey 是其可选实例身份。
// Returns: a complete process projection with no child rows.
// 返回值：不包含子行的完整处理过程投影。
function createProcessItem(id: string, renderKey?: string): ConversationProcessRenderItem {
  return {
    collapsible: false,
    defaultCollapsed: false,
    durationMs: null,
    guidanceItems: [],
    id,
    items: [],
    renderKey,
    running: false,
    startedAtMs: null,
    stopped: false,
  };
}

// createTimelineRenderItem builds one ordered transcript row for turn-projection tests.
// createTimelineRenderItem 为回合投影测试构建一条有序转录行。
// Parameters: id is the row identity, role is the transcript role, blockKind selects its semantic renderer, and rawMetadata carries optional terminal state.
// 参数：id 是行身份，role 是转录角色，blockKind 选择语义渲染器，rawMetadata 携带可选终止状态。
// Returns: a projected row with one block and the matching immutable source message.
// 返回值：包含单个块及对应不可变源消息的投影行。
function createTimelineRenderItem(
  id: string,
  role: ConversationRenderItem["role"],
  blockKind: ConversationBlockKind,
  rawMetadata?: unknown,
): ConversationRenderItem {
  // sourceMessage preserves the same chronological identity consumed by the timeline projector.
  // sourceMessage 保留时间线投影器所消费的同一时间顺序身份。
  const sourceMessage: ConversationMessage = {
    body: id,
    conversationId: "conversation-timeline-order",
    id,
    role,
  };
  return {
    blocks: [{
      body: id,
      id: `${id}-block`,
      kind: blockKind,
      rawMetadata,
      summary: id,
      title: id,
    }],
    id,
    role,
    sourceMessage,
    sourceMessages: [sourceMessage],
    userMessage: role === "user" ? sourceMessage : undefined,
  };
}

describe("conversation timeline business keys", () => {
  it("keeps a normal row key stable across content-only projection updates", () => {
    // first and updated represent two immutable snapshots of the same rendered occurrence.
    // first 与 updated 表示同一个渲染实例的两份不可变快照。
    const first = createRenderItem("assistant-1", "message:assistant-1:occurrence-4");
    const updated = {
      ...first,
      blocks: [{ id: "markdown-1", kind: "markdown" as const, title: "", body: "next", summary: "next" }],
    };

    expect(conversationTimelineItemKey({ kind: "item", item: first })).toBe(
      conversationTimelineItemKey({ kind: "item", item: updated }),
    );
  });

  it("falls back to kind-scoped persisted identities when render keys are absent", () => {
    // item is the shared persisted row used by the normal and final-metadata projections.
    // item 是普通行与最终元信息投影共享的持久化记录。
    const item = createRenderItem("assistant-2");
    // process is a separately namespaced process projection.
    // process 是使用独立命名空间的处理过程投影。
    const process = createProcessItem("process-assistant-2");

    expect(conversationTimelineItemKey({ kind: "item", item })).toBe("item:assistant-2");
    expect(conversationTimelineItemKey({ kind: "final-assistant-meta", item })).toBe(
      "final-assistant-meta:assistant-2",
    );
    expect(conversationTimelineItemKey({ kind: "process", process })).toBe(
      "process:process-assistant-2",
    );
    expect(conversationTimelineItemKey({ kind: "fork-source" })).toBe("fork-source");
  });

  it("prefers occurrence render keys for repeated persisted identities", () => {
    // first and second reuse one persisted id but represent distinct historical occurrences.
    // first 与 second 复用同一持久化标识，但表示不同的历史实例。
    const first = createRenderItem("assistant-reused", "occurrence:first");
    const second = createRenderItem("assistant-reused", "occurrence:second");

    expect(conversationTimelineItemKey({ kind: "item", item: first })).toBe("occurrence:first");
    expect(conversationTimelineItemKey({ kind: "item", item: second })).toBe("occurrence:second");
  });
});

describe("conversation timeline chronological projection", () => {
  it("aligns only the consecutive tools owned by one reasoning prefix", () => {
    // Later assistant content ends this alignment scope; the reasoning icon belongs only to the first tool.
    // 后续助手正文结束当前对齐范围；思考图标仅属于首个工具。
    const timeline = projectConversationTimelineItems([
      createTimelineRenderItem("user", "user", "markdown"),
      createTimelineRenderItem("reasoning", "assistant", "thinking"),
      createTimelineRenderItem("tool-1", "assistant", "tool-result"),
      createTimelineRenderItem("tool-2", "assistant", "tool-result"),
      createTimelineRenderItem("tool-3", "assistant", "tool-result"),
      createTimelineRenderItem("commentary", "assistant", "markdown"),
      createTimelineRenderItem("tool-4", "assistant", "tool-result"),
    ].map((item) => item.blocks[0].kind === "tool-result"
      ? { ...item, blocks: [{ ...item.blocks[0], toolCallId: item.id }] }
      : item), undefined, false);
    // process contains the resulting immutable rows in original content order.
    // process 包含按原始内容顺序排列的不可变结果行。
    const process = timeline.find((entry) => entry.kind === "process");
    if (!process || process.kind !== "process") throw new Error("Missing process");
    expect(process.process.items.map((item) => Boolean(item.reserveThinkingPrefix))).toEqual([true, true, true, false, false]);
    expect(process.process.items.map((item) => Boolean(item.leadingThinking))).toEqual([true, false, false, false, false]);
  });

  it.each(["completed", "stopped"])("attaches intermediate reasoning but preserves final-answer rules for %s turns", (status) => {
    // Distinct reasoning segments must keep their immediate owners, including a stopped assistant ending.
    // 不同思考段必须保持紧邻归属，包括以助手正文结束的已停止回合。
    const commentaryThinking = createTimelineRenderItem("commentary-thinking", "assistant", "thinking");
    const finalThinking = createTimelineRenderItem("final-thinking", "assistant", "thinking");
    const commentary = createTimelineRenderItem("commentary", "assistant", "markdown");
    const final = createTimelineRenderItem("final", "assistant", "markdown");
    const timeline = projectConversationTimelineItems([
      createTimelineRenderItem("user", "user", "markdown"),
      commentaryThinking,
      commentary,
      createTimelineRenderItem("tool", "assistant", "tool-result"),
      finalThinking,
      final,
      createTimelineRenderItem("stats", "system", "turn-stats", { turn_stats: { status } }),
    ], undefined, false);
    // The process projection is the authority for intermediate versus final output.
    // 过程投影是中间输出与最终输出分类的权威。
    const process = timeline.find((entry) => entry.kind === "process");
    if (!process || process.kind !== "process") throw new Error("Missing process");
    expect(process.process.items[0]).toMatchObject({ id: "commentary", leadingThinking: commentaryThinking.blocks });
    expect(process.process.items[0].sourceMessages).toEqual([...commentaryThinking.sourceMessages, ...commentary.sourceMessages]);
    expect(process.process.items[1].leadingThinking).toBeUndefined();
    if (status === "completed") {
      expect(process.process.items).toHaveLength(2);
      expect(timeline.find((entry) => entry.kind === "item" && entry.finalAssistantOutput)).toMatchObject({
        item: { id: final.id, leadingThinking: finalThinking.blocks,
          sourceMessages: [...finalThinking.sourceMessages, ...final.sourceMessages] },
      });
    } else {
      expect(process.process.items[2]).toMatchObject({ id: "final", leadingThinking: finalThinking.blocks });
      expect(timeline.some((entry) => entry.kind === "item" && entry.finalAssistantOutput)).toBe(false);
    }
    expect(commentary.leadingThinking).toBeUndefined();
    expect(final.leadingThinking).toBeUndefined();
  });

  // A direct answer owns its initial reasoning without leaving an empty independent process group.
  // 直接回答承接首次思考，不留下空的独立过程分组。
  it("attaches initial reasoning directly to the final answer", () => {
    // reasoning is retained unchanged in the final row's preview.
    // reasoning 原样保留在最终行的预览中。
    const reasoning = createTimelineRenderItem("initial", "assistant", "thinking");
    const timeline = projectConversationTimelineItems([
      createTimelineRenderItem("user", "user", "markdown"), reasoning,
      createTimelineRenderItem("answer", "assistant", "markdown"),
    ], undefined, false);
    expect(timeline.some((entry) => entry.kind === "process")).toBe(false);
    expect(timeline.find((entry) => entry.kind === "item" && entry.finalAssistantOutput)).toMatchObject({
      item: { id: "answer", leadingThinking: reasoning.blocks },
    });
  });

  it("preserves commentary before later tool and compaction failure events", () => {
    // The assistant Markdown is intermediate commentary because later process events exist.
    // 该 Assistant Markdown 后面仍有处理事件，因此它是中间说明而不是最终回答。
    const compactionFailure = createTimelineRenderItem("compaction-failed", "system", "notice");
    // compactionState is the classifier's authoritative marker for a context-compaction notice.
    // compactionState 是分类器识别上下文压缩通知的权威标记。
    compactionFailure.blocks[0].compactionState = "failed";
    const timeline = projectConversationTimelineItems([
      createTimelineRenderItem("user-1", "user", "markdown"),
      createTimelineRenderItem("assistant-commentary", "assistant", "markdown"),
      createTimelineRenderItem("tool-result", "assistant", "tool-result"),
      compactionFailure,
    ], undefined, false);

    expect(timeline.map((item) => item.kind)).toEqual(["item", "process"]);
    expect(timeline[1]).toMatchObject({
      kind: "process",
      process: {
        items: [
          { id: "assistant-commentary" },
          { id: "tool-result" },
          { id: "compaction-failed" },
        ],
      },
    });
    expect(timeline.some((item) => item.kind === "item" && item.finalAssistantOutput)).toBe(false);
  });

  it("keeps the existing final-answer layout when assistant Markdown is trailing", () => {
    // A trailing assistant Markdown immediately before turn stats is the genuine final answer.
    // 紧邻回合统计之前的末尾 Assistant Markdown 是真正的最终回答。
    const timeline = projectConversationTimelineItems([
      createTimelineRenderItem("user-2", "user", "markdown"),
      createTimelineRenderItem("tool-result-2", "assistant", "tool-result"),
      createTimelineRenderItem("assistant-final", "assistant", "markdown"),
      createTimelineRenderItem("turn-stats", "system", "turn-stats", {
        turn_stats: { status: "completed" },
      }),
    ], undefined, false);

    expect(timeline.map((item) => item.kind)).toEqual([
      "item",
      "process",
      "item",
      "item",
      "final-assistant-meta",
    ]);
    expect(timeline[1]).toMatchObject({
      kind: "process",
      process: { items: [{ id: "tool-result-2" }] },
    });
    expect(timeline[2]).toMatchObject({
      finalAssistantOutput: true,
      item: { id: "assistant-final" },
      kind: "item",
    });
  });
});
