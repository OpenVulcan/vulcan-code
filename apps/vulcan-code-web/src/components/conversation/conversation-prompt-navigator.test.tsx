// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ConversationTimelineItem } from "@/components/conversation/conversation-model";
import type { ConversationTurnProjection } from "@/components/conversation/conversation-timeline-projection-store";
import type { ConversationMessage } from "@/types";
import { ConversationPromptNavigator } from "./conversation-prompt-navigator";
import {
  CONVERSATION_PROMPT_NAVIGATOR_PREVIEW_MAX_CHARS,
  CONVERSATION_PROMPT_NAVIGATOR_TITLE_MAX_CHARS,
  createConversationPromptNavigatorProjector,
  markdownToNavigatorPreview,
  projectConversationPromptNavigatorEntries,
  resolveActivePromptNavigatorIndex,
  type ConversationPromptNavigatorEntry,
} from "./conversation-prompt-navigator-model";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));

// message builds one minimal transcript message.
// message 构建一条最小转录消息。
function message(id: string, role: ConversationMessage["role"], body: string): ConversationMessage {
  return { body, conversationId: "conversation-1", createdAt: "1", id, role };
}

// userItem builds one user timeline row.
// userItem 构建一条用户时间线行。
function userItem(id: string, body: string): ConversationTimelineItem {
  const source = message(id, "user", body);
  return {
    kind: "item",
    item: { blocks: [], id, role: "user", sourceMessage: source, sourceMessages: [source], userMessage: source },
  };
}

// finalReplyItem builds one final assistant timeline row.
// finalReplyItem 构建一条最终助手时间线行。
function finalReplyItem(id: string, body: string): ConversationTimelineItem {
  const source = message(id, "assistant", body);
  return {
    kind: "item",
    finalAssistantOutput: true,
    item: { blocks: [], id, role: "assistant", sourceMessage: source, sourceMessages: [source] },
  };
}

// runningProcessItem builds one still-running process row.
// runningProcessItem 构建一条仍在运行的过程行。
function runningProcessItem(id: string): ConversationTimelineItem {
  return {
    kind: "process",
    process: {
      collapsible: true,
      defaultCollapsed: false,
      durationMs: null,
      guidanceItems: [],
      id,
      items: [],
      running: true,
      startedAtMs: null,
      stopped: false,
    },
  };
}

describe("projectConversationPromptNavigatorEntries", () => {
  it("lists only user-led turns with their prompt and final reply preview", () => {
    const turns: ConversationTurnProjection[] = [
      { key: "fork", items: [{ kind: "fork-source" }] },
      { key: "turn-a", items: [userItem("u1", "## User Prompt\n修复\n  登录问题"), finalReplyItem("a1", "# 结论\n- **已修复** `auth`\n\n参见 [文档](https://example.test)")] },
      { key: "turn-b", items: [userItem("u2", "继续"), runningProcessItem("p2")] },
    ];

    const entries = projectConversationPromptNavigatorEntries(turns);

    expect(entries.map((entry) => entry.rowKey)).toEqual(["turn-a", "turn-b"]);
    expect(entries.find((entry) => entry.rowKey === "turn-a")).toMatchObject({
      prompt: "修复 登录问题",
      replyPreview: "结论\n• 已修复 auth\n参见 文档",
      running: false,
    });
    expect(entries.find((entry) => entry.rowKey === "turn-b")).toMatchObject({ prompt: "继续", replyPreview: "", running: true });
  });

  it("bounds prompt and preview text with an ellipsis", () => {
    const entries = projectConversationPromptNavigatorEntries([{
      key: "turn-long",
      items: [
        userItem("u1", "字".repeat(CONVERSATION_PROMPT_NAVIGATOR_TITLE_MAX_CHARS + 5)),
        finalReplyItem("a1", "x".repeat(CONVERSATION_PROMPT_NAVIGATOR_PREVIEW_MAX_CHARS + 5)),
      ],
    }]);
    const [entry] = entries;

    expect(Array.from(entry.prompt)).toHaveLength(CONVERSATION_PROMPT_NAVIGATOR_TITLE_MAX_CHARS + 1);
    expect(entry.prompt.endsWith("…")).toBe(true);
    expect(entry.replyPreview).toHaveLength(CONVERSATION_PROMPT_NAVIGATOR_PREVIEW_MAX_CHARS + 1);
  });

  it("drops fenced code delimiters from reply previews", () => {
    expect(markdownToNavigatorPreview("运行：\n```bash\nls\n```\n完成")).toBe("运行：\nls\n完成");
  });
});

describe("createConversationPromptNavigatorProjector", () => {
  // Unchanged Turns keep their entry and the whole array identity; a changed Turn rebuilds only its own entry.
  // 未变化的 Turn 保留其条目与整个数组身份；变化的 Turn 只重建自身条目。
  it("reuses entries whose source text is unchanged", () => {
    const project = createConversationPromptNavigatorProjector();
    const settled: ConversationTurnProjection = { key: "turn-a", items: [userItem("u1", "修复"), finalReplyItem("a1", "完成")] };
    const running: ConversationTurnProjection = { key: "turn-b", items: [userItem("u2", "继续"), runningProcessItem("p2")] };

    const first = project([settled, running]);
    // Fresh Turn objects with identical text model a streaming update that did not touch these Turns.
    // 文本相同的新 Turn 对象模拟未触及这些 Turn 的流式更新。
    const second = project([{ ...settled, items: [...settled.items] }, { ...running, items: [...running.items] }]);
    expect(second).toBe(first);

    const third = project([settled, { key: "turn-b", items: [userItem("u2", "继续"), finalReplyItem("a2", "好了")] }]);
    expect(third).not.toBe(first);
    expect(third.find((entry) => entry.rowKey === "turn-a")).toBe(first.find((entry) => entry.rowKey === "turn-a"));
    expect(third.find((entry) => entry.rowKey === "turn-b")).toMatchObject({ replyPreview: "好了", running: false });

    const fourth = project([settled]);
    expect(fourth.map((entry) => entry.rowKey)).toEqual(["turn-a"]);
  });
});

describe("resolveActivePromptNavigatorIndex", () => {
  it("selects the last prompt above the active line", () => {
    const tops = [-500, -20, 180, 900];
    expect(resolveActivePromptNavigatorIndex(tops.length, (index) => tops[index], 200)).toBe(tops.indexOf(180));
    expect(resolveActivePromptNavigatorIndex(tops.length, (index) => tops[index], -600)).toBe(0);
    expect(resolveActivePromptNavigatorIndex(0, () => 0, 0)).toBe(-1);
  });
});

describe("ConversationPromptNavigator", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  });

  it("reveals the clicked prompt shell and previews it on hover", async () => {
    const entries: ConversationPromptNavigatorEntry[] = [
      { rowKey: "turn-a", prompt: "修复", replyPreview: "已修复登录问题", running: false },
      { rowKey: "turn-b", prompt: "", replyPreview: "", running: true },
    ];
    const scrollRef = React.createRef<HTMLElement>();
    const onRevealAnchor = vi.fn();

    await act(async () => root.render(
      <>
        <section ref={scrollRef}>
          {entries.map((entry) => <section data-conversation-turn-shell={entry.rowKey} key={entry.rowKey} />)}
        </section>
        <ConversationPromptNavigator entries={entries} onRevealAnchor={onRevealAnchor} scrollRef={scrollRef} />
      </>,
    ));

    const ticks = Array.from(host.querySelectorAll<HTMLButtonElement>(".conversation-prompt-navigator-tick"));
    expect(ticks.map((tick) => tick.getAttribute("aria-label"))).toEqual(["修复", "conversation.promptNavigator.untitled"]);

    const target = ticks.find((tick) => tick.getAttribute("aria-label") === "conversation.promptNavigator.untitled")!;
    await act(async () => target.click());
    expect(onRevealAnchor).toHaveBeenCalledWith(host.querySelector('[data-conversation-turn-shell="turn-b"]'));
    expect(target.getAttribute("aria-current")).toBe("true");

    const first = ticks.find((tick) => tick.getAttribute("aria-label") === "修复")!;
    await act(async () => first.dispatchEvent(new MouseEvent("mouseover", { bubbles: true })));
    expect(host.querySelector(".conversation-prompt-navigator-card-title")?.textContent).toBe("修复");
    expect(host.querySelector(".conversation-prompt-navigator-card-preview")?.textContent).toBe("已修复登录问题");

    await act(async () => target.dispatchEvent(new MouseEvent("mouseover", { bubbles: true })));
    expect(host.querySelector(".conversation-prompt-navigator-card-preview")?.textContent).toBe("conversation.promptNavigator.replyPending");
  });
});
