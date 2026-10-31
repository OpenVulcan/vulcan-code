import { expect, it } from "vitest";
import type { ConversationSummary } from "@/types";
import { orderProjectSessions, pinsWithSessionOrder, withSubsetOrder } from "./project-session-order";

// row builds one session fixture with the ordering fields under test.
// row 构建一个带待测排序字段的会话测试数据。
function row(id: string, createdAt: number, lastRequestAt: number, manualPosition: number | null = null): ConversationSummary {
  return { id, title: id, status: "idle", createdAt: String(createdAt), lastRequestAt: String(lastRequestAt), manualPosition };
}

const ids = (sessions: ConversationSummary[]) => sessions.map((session) => session.id);

it("orders last-request mode by the latest request, newest first", () => {
  const sessions = [row("old", 1, 10), row("busy", 2, 30), row("new", 3, 20)];
  expect(ids(orderProjectSessions(sessions, "last_request"))).toEqual(["busy", "new", "old"]);
});

it("puts unarranged sessions first by creation in manual mode, then arranged ones by position", () => {
  const sessions = [row("b", 1, 99, 1), row("a", 2, 1, 0), row("fresh", 5, 5), row("fresher", 6, 1)];
  expect(ids(orderProjectSessions(sessions, "manual"))).toEqual(["fresher", "fresh", "a", "b"]);
});

it("reorders a shown subset in place and keeps hidden ids where they were", () => {
  expect(withSubsetOrder(["a", "b", "c", "d"], ["c", "a"])).toEqual(["c", "b", "a", "d"]);
});

it("rewrites only the reordered pinned sessions inside the full pin list", () => {
  const pins = [
    { kind: "session" as const, id: "s1" },
    { kind: "project" as const, id: "p" },
    { kind: "session" as const, id: "s2" },
  ];
  expect(pinsWithSessionOrder(pins, ["s2", "s1"])).toEqual([
    { kind: "session", id: "s2" },
    { kind: "project", id: "p" },
    { kind: "session", id: "s1" },
  ]);
});
