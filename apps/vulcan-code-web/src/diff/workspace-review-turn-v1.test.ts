import { describe, expect, it } from "vitest";

import type {
  ConversationEventSummary,
  PersistentChangeSet,
  PersistentChangeSetFile,
} from "@/service/conversation-event-window-store";
import { resolveLastReviewTurnIdV1, turnChangeSummariesV1 } from "./turn-change-summary-v1";
import { selectLastTurnWorkspaceReviewV1, workspaceReviewSnapshotFromTurnV1 } from "./workspace-review-v1-selector";

/** Builds one modify record with the given inserted and deleted line counts.
 * 构建一条带指定新增、删除行数的修改记录。 */
function modify(path: string, inserted: number, deleted: number): PersistentChangeSetFile {
  return {
    change: "modify",
    path,
    hunks: [{
      before: "",
      after: "",
      deleted: Array.from({ length: deleted }, (_, index) => ({ line: index + 1, content: `old-${index}` })),
      inserted: Array.from({ length: inserted }, (_, index) => ({ line: index + 1, content: `new-${index}` })),
    }],
  };
}

/** Builds one create record carrying the full new content.
 * 构建一条携带完整新内容的创建记录。 */
function create(path: string, lines: number): PersistentChangeSetFile {
  return {
    change: "create",
    path,
    hunks: [],
    lifecycle_content: {
      mode: "full",
      content: Array.from({ length: lines }, (_, index) => `line-${index}`).join("\n"),
      total_line_count: lines,
    },
  };
}

/** Builds one rename record.
 * 构建一条重命名记录。 */
function rename(oldPath: string, newPath: string): PersistentChangeSetFile {
  return { change: "rename", path: newPath, old_path: oldPath, new_path: newPath, hunks: [] };
}

/** Builds one ToolCall event of a Turn carrying the given ChangeSet records.
 * 构建一条属于某 Turn、携带指定 ChangeSet 记录的 ToolCall 事件。 */
function tool(
  seq: number,
  turnId: number,
  files: PersistentChangeSetFile[],
  mode: PersistentChangeSet["mode"] = "applied",
): ConversationEventSummary {
  return {
    seq,
    turn_id: turnId,
    created_at_ms: seq,
    updated_at_ms: seq,
    event_revision: 1,
    visibility: "provider",
    event: {
      kind: "tool_call",
      api_call_id: seq,
      tool_call_id: seq,
      tool_name: "file_edit",
      input: {},
      request_stream_id: `turn-review-${seq}`,
      status: "completed",
      changeset: { mode, files },
    },
  };
}

/** Builds one event of a Turn that changes no file.
 * 构建一条属于某 Turn、未改动文件的事件。 */
function note(seq: number, turnId: number): ConversationEventSummary {
  return {
    seq,
    turn_id: turnId,
    created_at_ms: seq,
    updated_at_ms: seq,
    event_revision: 1,
    visibility: "ui_only",
    event: { kind: "display_note", notice_kind: "goal_status", severity: "info", reason_code: `note-${seq}` },
  };
}

describe("turn change summary", () => {
  it("merges every applied change of a Turn per file, sorted by path", () => {
    const summaries = turnChangeSummariesV1([
      tool(1, 7, [modify("src/b.ts", 3, 1), create("src/a.ts", 4)]),
      tool(2, 7, [modify("src/b.ts", 2, 2)]),
      tool(3, 7, [modify("src/c.ts", 9, 9)], "preview"),
      tool(4, 8, [modify("src/d.ts", 1, 0)]),
    ]);
    expect(summaries.get(7)).toEqual({
      files: [
        { path: "src/a.ts", renamedFrom: null, status: "added", additions: 4, deletions: 0 },
        { path: "src/b.ts", renamedFrom: null, status: "modified", additions: 5, deletions: 3 },
      ],
      additions: 9,
      deletions: 3,
    });
    expect(summaries.get(8)?.files.map((file) => file.path)).toEqual(["src/d.ts"]);
  });

  it("follows a rename so later edits stay on the same file", () => {
    const summary = turnChangeSummariesV1([
      tool(1, 1, [modify("old.ts", 1, 0)]),
      tool(2, 1, [rename("old.ts", "new.ts")]),
      tool(3, 1, [modify("new.ts", 2, 1)]),
    ]).get(1);
    expect(summary?.files).toEqual([
      { path: "new.ts", renamedFrom: "old.ts", status: "renamed", additions: 3, deletions: 1 },
    ]);
  });

  it("omits Turns whose only ChangeSets are previews", () => {
    expect(turnChangeSummariesV1([tool(1, 1, [modify("a.ts", 1, 1)], "preview"), note(2, 1)]).size).toBe(0);
  });
});

describe("last turn review source", () => {
  const history = [tool(1, 1, [modify("a.ts", 1, 0)]), note(2, 1), note(3, 2)];

  it("shows the newest Turn when none is running", () => {
    expect(resolveLastReviewTurnIdV1(history, null)).toBe(2);
  });

  it("keeps the previous Turn while the running Turn has not changed a file", () => {
    expect(resolveLastReviewTurnIdV1([...history, note(4, 3)], 3)).toBe(2);
  });

  it("switches to the running Turn once it applies a change", () => {
    expect(resolveLastReviewTurnIdV1([...history, note(4, 3), tool(5, 3, [modify("b.ts", 1, 0)])], 3)).toBe(3);
  });

  it("projects one review file per changed path with hunks in tool-call order", () => {
    const selection = selectLastTurnWorkspaceReviewV1({
      owner: { session_id: "session", window_generation: 1 },
      service_epoch: "epoch",
      memory_revision: 1,
      durable_revision: 1,
      events: [tool(1, 4, [modify("a.ts", 1, 0)]), tool(2, 4, [modify("a.ts", 2, 1), create("b.ts", 3)])],
      older_before_seq: null,
      has_more: false,
      open_turn_id: null,
      open_turn_start_seq: null,
      session_config: null,
    });
    expect(selection?.turn_id).toBe(4);
    const snapshot = workspaceReviewSnapshotFromTurnV1(selection);
    expect(snapshot.files.map((file) => [file.newPath, file.status, file.additions, file.deletions, file.hunks.length]))
      .toEqual([["a.ts", "modified", 3, 1, 2], ["b.ts", "added", 3, 0, 1]]);
    expect(snapshot.stats).toMatchObject({ files: 2, additions: 6, deletions: 1 });
  });

  it("renders an empty source when the window holds no Turn", () => {
    expect(workspaceReviewSnapshotFromTurnV1(null).files).toEqual([]);
  });
});
