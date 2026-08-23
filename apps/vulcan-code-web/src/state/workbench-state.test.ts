import { describe, expect, it } from "vitest";

import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench";
import {
  applySessionAddonsChangedResponse,
  mergeConversationHistoryPageResponse,
  mergeConversationHistoryPageResponseWithSubagentSurfaceMessageCache,
  mergeWorkbenchSnapshot,
} from "@/state/workbench-state";
import type { ConversationHistoryPageResponse, ConversationMessage } from "@/types";

// Verifies a lower revision remains stale inside one service instance.
// 验证同一服务实例内较低版本仍会被视为过期。
describe("mergeWorkbenchSnapshot service instance ordering", () => {
  it("rejects a lower revision without an instance reset", () => {
    const current = {
      ...browserFallbackSnapshot(),
      appVersion: "old-instance",
      revision: 900,
    };
    const incoming = {
      ...browserFallbackSnapshot(),
      appVersion: "new-instance",
      revision: 3,
    };

    expect(mergeWorkbenchSnapshot(current, incoming)).toBe(current);
  });

  // Verifies one confirmed service-instance transition can establish a new revision baseline.
  // 验证一次已确认的服务实例切换可以建立新的版本基线。
  it("accepts a lower revision after a confirmed instance reset", () => {
    const current = {
      ...browserFallbackSnapshot(),
      appVersion: "old-instance",
      revision: 900,
    };
    const incoming = {
      ...browserFallbackSnapshot(),
      appVersion: "new-instance",
      revision: 3,
    };

    const merged = mergeWorkbenchSnapshot(current, incoming, {
      allowServiceRevisionReset: true,
    });

    expect(merged).not.toBe(current);
    expect(merged.revision).toBe(3);
    expect(merged.appVersion).toBe("new-instance");
  });
});

// Verifies a context-window node is sufficient to retain one session addon projection.
// 验证上下文窗口节点足以保留一条会话附属投影。
describe("applySessionAddonsChangedResponse context window", () => {
  it("retains a node-id-only session addon response", () => {
    const current = browserFallbackSnapshot();
    const next = applySessionAddonsChangedResponse(current, {
      conversationId: "conversation-a",
      revision: (current.revision ?? 0) + 1,
      sessionContextCompactionWindowNodeId: "compression-window-2",
    });

    expect(next.sessionAddonsByConversation).toEqual([{
      conversationId: "conversation-a",
      revision: (current.revision ?? 0) + 1,
      sessionContextCompactionWindowNodeId: "compression-window-2",
    }]);
  });
});

// Builds one minimal visible transcript message for deterministic history-page tests.
// 为确定性历史分页测试构建一条最小可见转录消息。
// Parameters: conversationId owns the message; id is the stable row identity; body is visible text.
// 参数：conversationId 是消息所属会话；id 是稳定行标识；body 是可见正文。
// Returns: one complete conversation message accepted by the workbench model.
// 返回值：返回一条工作台模型可接受的完整会话消息。
function historyMessage(conversationId: string, id: string, body: string): ConversationMessage {
  return {
    id,
    conversationId,
    role: "assistant",
    body,
  };
}

// Builds one exact older-history response with page-local cursor facts.
// 构建一份带分页本地游标事实的精确更早历史响应。
// Parameters: sessionId owns the page; revision is the service revision; messages are ordered oldest first.
// 参数：sessionId 是分页所属会话；revision 是服务版本；messages 按从旧到新排序。
// Returns: one response matching the public WebUI history-page contract.
// 返回值：返回一份符合 WebUI 公共历史分页契约的响应。
function historyPageResponse(
  sessionId: string,
  revision: number,
  messages: ConversationMessage[],
): ConversationHistoryPageResponse {
  // workbench supplies the required public snapshot shape without unrelated business state.
  // workbench 提供所需的公共快照结构，且不携带无关业务状态。
  const workbench = browserFallbackSnapshot();
  return {
    sessionId,
    workbench: {
      ...workbench,
      revision,
      hydratedConversationWindows: [{
        sessionId,
        subagentMetadataLoaded: true,
        earliestOrdinal: 10,
        hasMoreHistory: false,
      }],
    },
    messages,
    earliestOrdinal: 10,
    hasMoreHistory: false,
  };
}

// Verifies history pages use a conversation-scoped prepend instead of full-snapshot revision replacement.
// 验证历史分页使用会话范围前置合并，而不是完整快照版本替换。
describe("mergeConversationHistoryPageResponse", () => {
  it("prepends unseen target rows despite a stale response revision and preserves concurrent state", () => {
    // sessionId identifies the exact conversation receiving the older page.
    // sessionId 标识接收更早分页的精确会话。
    const sessionId = "conversation-a";
    // otherSessionId proves unrelated rows survive and cannot block a target-scoped duplicate id.
    // otherSessionId 证明无关行会被保留，且不能用相同标识阻止目标范围消息。
    const otherSessionId = "conversation-b";
    // current models a newer live snapshot with one target row and one unrelated id collision.
    // current 模拟包含一条目标行与一条无关标识冲突行的较新实时快照。
    const current = {
      ...browserFallbackSnapshot(),
      revision: 100,
      messages: [
        historyMessage(otherSessionId, "older-1", "other conversation id collision"),
        historyMessage(sessionId, "current-1", "current"),
      ],
      hydratedConversationWindows: [{
        sessionId,
        subagentMetadataLoaded: true,
        earliestOrdinal: 300,
        hasMoreHistory: true,
      }],
    };
    // response is intentionally stale globally while carrying an authoritative target-local older page.
    // response 在全局版本上故意过期，但携带权威的目标本地更早分页。
    const response = historyPageResponse(sessionId, 20, [
      historyMessage(sessionId, "older-1", "older one"),
      historyMessage(sessionId, "older-2", "older two"),
    ]);

    // result contains only the minimal target page and cursor mutation.
    // result 仅包含最小化的目标分页与游标变更。
    const result = mergeConversationHistoryPageResponse(current, response);

    expect(result.addedMessageCount).toBe(2);
    expect(result.workbench.revision).toBe(100);
    expect(result.workbench.messages.filter((message) => message.conversationId === sessionId)
      .map((message) => message.id)).toEqual(["older-1", "older-2", "current-1"]);
    expect(result.workbench.messages.filter((message) => message.conversationId === otherSessionId)
      .map((message) => message.id)).toEqual(["older-1"]);
    expect(result.workbench.hydratedConversationWindows?.find((window) => window.sessionId === sessionId))
      .toEqual({
        sessionId,
        subagentMetadataLoaded: true,
        earliestOrdinal: 10,
        hasMoreHistory: false,
      });
  });

  it("does not duplicate an already merged history page", () => {
    // sessionId identifies the exact duplicate-page target.
    // sessionId 标识精确的重复分页目标。
    const sessionId = "conversation-a";
    // older is the stable row already present in the current target window.
    // older 是当前目标窗口中已经存在的稳定行。
    const older = historyMessage(sessionId, "older-1", "older");
    // current already contains both the older row and a newer row.
    // current 已经同时包含更早行与较新行。
    const current = {
      ...browserFallbackSnapshot(),
      messages: [older, historyMessage(sessionId, "current-1", "current")],
    };

    // result must report zero additions while still accepting the final pagination cursor.
    // result 必须报告零新增，同时仍接受最终分页游标。
    const result = mergeConversationHistoryPageResponse(
      current,
      historyPageResponse(sessionId, 1, [older]),
    );

    expect(result.addedMessageCount).toBe(0);
    expect(result.workbench.messages.map((message) => message.id)).toEqual(["older-1", "current-1"]);
  });

  it("does not reconcile unrelated subagent cache entries for a regular conversation page", () => {
    // sessionId identifies a regular parent conversation with no child-cache ownership.
    // sessionId 标识不拥有子代理缓存的普通父会话。
    const sessionId = "conversation-a";
    // cachedChildMessages models an independently retained child surface that this page must not inspect or clear.
    // cachedChildMessages 模拟当前分页不得检查或清理的独立保留子界面。
    const cachedChildMessages = [historyMessage("child-a", "child-1", "cached child")];
    // cache owns one unrelated child key before the regular page merge.
    // cache 在普通分页合并前持有一个无关子代理键。
    const cache = new Map([["child-a", cachedChildMessages]]);
    // current contains only the regular conversation surface being paged.
    // current 只包含正在分页的普通会话界面。
    const current = {
      ...browserFallbackSnapshot(),
      messages: [historyMessage(sessionId, "current-1", "current")],
    };

    // result applies the parent page without running global child-cache injection or retirement.
    // result 应用父会话分页，且不运行全局子代理缓存注入或淘汰。
    const result = mergeConversationHistoryPageResponseWithSubagentSurfaceMessageCache(
      current,
      historyPageResponse(sessionId, 1, [historyMessage(sessionId, "older-1", "older")]),
      cache,
    );

    expect(result.addedMessageCount).toBe(1);
    expect(cache.get("child-a")).toBe(cachedChildMessages);
  });

  it("updates only the exact cached child surface when that child loads an older page", () => {
    // childId is the exact child cache key receiving the older page.
    // childId 是接收更早分页的精确子代理缓存键。
    const childId = "child-a";
    // unrelatedChildMessages must retain object identity because its cache key is outside the page scope.
    // unrelatedChildMessages 必须保留对象身份，因为其缓存键不属于当前分页范围。
    const unrelatedChildMessages = [historyMessage("child-b", "child-b-1", "unrelated child")];
    // cache contains both the target child and one unrelated child before the exact update.
    // cache 在精确更新前同时包含目标子代理与一个无关子代理。
    const cache = new Map([
      [childId, [historyMessage(childId, "current-1", "current child")]],
      ["child-b", unrelatedChildMessages],
    ]);
    // current is the currently projected child window.
    // current 是当前已投影的子代理窗口。
    const current = {
      ...browserFallbackSnapshot(),
      messages: [historyMessage(childId, "current-1", "current child")],
    };

    // result prepends the target page and refreshes only the exact target cache value.
    // result 前置目标分页，并只刷新精确目标缓存值。
    const result = mergeConversationHistoryPageResponseWithSubagentSurfaceMessageCache(
      current,
      historyPageResponse(childId, 1, [historyMessage(childId, "older-1", "older child")]),
      cache,
    );

    expect(result.addedMessageCount).toBe(1);
    expect(cache.get(childId)?.map((message) => message.id)).toEqual(["older-1", "current-1"]);
    expect(cache.get("child-b")).toBe(unrelatedChildMessages);
  });
});
