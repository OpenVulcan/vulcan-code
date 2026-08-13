import { describe, expect, it } from "vitest";
import type {
  ConversationProcessRenderItem,
  ConversationRenderItem,
} from "./conversation-model";
import type { ConversationMessage } from "@/types";
import { conversationTimelineItemKey } from "./conversation-timeline";

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
