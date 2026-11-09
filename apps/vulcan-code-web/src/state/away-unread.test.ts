import { afterEach, describe, expect, it } from "vitest";

import type { ConversationSummary } from "@/types";

import {
  INITIAL_SELECTED_RUN_OBSERVATION,
  clearAwayUnread,
  isAwayUnread,
  markAwayUnread,
  observeSelectedRun,
} from "./away-unread";

// conversation builds the minimal summary the observation reads.
function conversation(id: string, status: string): ConversationSummary {
  return { id, title: id, status, lastUpdatedLabel: "now" } as ConversationSummary;
}

// runThenSettle observes a running then settled selected conversation with the given attention.
function runThenSettle(attended: boolean) {
  const running = observeSelectedRun(INITIAL_SELECTED_RUN_OBSERVATION, "a", [conversation("a", "Running")], attended);
  return observeSelectedRun(running.next, "a", [conversation("a", "Completed")], attended);
}

describe("observeSelectedRun", () => {
  it("reports a selected run that settles while the window is unattended", () => {
    expect(runThenSettle(false).settledWhileAway).toBe("a");
  });

  it("ignores a selected run that settles while the window is attended", () => {
    expect(runThenSettle(true).settledWhileAway).toBeNull();
  });

  it("ignores a settle observed across a selection change", () => {
    const running = observeSelectedRun(INITIAL_SELECTED_RUN_OBSERVATION, "a", [conversation("a", "Running")], false);
    const switched = observeSelectedRun(running.next, "b", [conversation("a", "Completed"), conversation("b", "Completed")], false);
    expect(switched.settledWhileAway).toBeNull();
  });
});

describe("away unread marks", () => {
  afterEach(() => clearAwayUnread());

  it("keeps a mark until cleared", () => {
    markAwayUnread("a");
    expect(isAwayUnread("a")).toBe(true);
    clearAwayUnread();
    expect(isAwayUnread("a")).toBe(false);
  });
});
