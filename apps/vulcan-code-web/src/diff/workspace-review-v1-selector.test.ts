import { describe, expect, it } from "vitest";

import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type {
  ConversationEventBody,
  ConversationEventSummary,
  PersistentChangeSet,
  PersistentLifecycleContent,
} from "@/service/conversation-event-window-store";
import { selectLatestWorkspaceReviewChangeSetV1 } from "./workspace-review-v1-selector";

/** Builds one exact persisted ChangeSet for selector tests.
 * 为 selector 测试构建一份精确持久化 ChangeSet。 */
function changeSet(
  label: string,
  mode: PersistentChangeSet["mode"] = "applied",
  lifecycleContent?: PersistentLifecycleContent,
): PersistentChangeSet {
  return {
    mode,
    summary: label,
    files: [{
      change: lifecycleContent === undefined ? "modify" : "delete",
      path: `${label}.txt`,
      hunks: lifecycleContent === undefined
        ? [{
            before: label,
            after: `${label}-updated`,
            deleted: [{ line: 1, content: label }],
            inserted: [{ line: 1, content: `${label}-updated` }],
          }]
        : [],
      ...(lifecycleContent === undefined ? {} : { lifecycle_content: lifecycleContent }),
    }],
  };
}

/** Builds one canonical ToolCall summary with optional persistent ChangeSet.
 * 构建一条带可选持久化 ChangeSet 的 canonical ToolCall 摘要。 */
function toolSummary(
  seq: number,
  options: {
    apiCallId?: number;
    toolCallId?: number;
    status?: Extract<ConversationEventBody, { kind: "tool_call" }>["status"];
    eventRevision?: number;
    changeset?: PersistentChangeSet;
  } = {},
): ConversationEventSummary {
  return {
    seq,
    turn_id: 1,
    created_at_ms: 10,
    updated_at_ms: 20,
    event_revision: options.eventRevision ?? 1,
    visibility: "provider",
    event: {
      kind: "tool_call",
      api_call_id: options.apiCallId ?? 10,
      tool_call_id: options.toolCallId ?? seq,
      tool_name: "file_edit",
      input: { path: `file-${seq}.txt` },
      status: options.status ?? "completed",
      ...(options.changeset === undefined ? {} : { changeset: options.changeset }),
    },
  };
}

/** Builds one non-tool event that still occupies its canonical seq.
 * 构建一条仍占据 canonical seq 的非工具事件。 */
function displayNoteSummary(seq: number): ConversationEventSummary {
  return {
    seq,
    turn_id: 1,
    created_at_ms: 10,
    updated_at_ms: 10,
    event_revision: 1,
    visibility: "ui_only",
    event: {
      kind: "display_note",
      notice_kind: "goal_status",
      severity: "info",
      reason_code: `note-${seq}`,
      redacted_detail: `note-${seq}`,
    },
  };
}

/** Builds one immutable controller snapshot for an exact owner generation.
 * 为一个精确归属代次构建不可变控制器快照。 */
function snapshot(
  events: readonly ConversationEventSummary[],
  windowGeneration = 1,
  ownerPresent = true,
): ConversationEventWindowControllerSnapshot {
  return {
    owner: ownerPresent
      ? { session_id: "session-review-v1", window_generation: windowGeneration }
      : null,
    service_epoch: ownerPresent ? "service-review-v1" : null,
    memory_revision: ownerPresent ? 1 : null,
    durable_revision: ownerPresent ? 1 : null,
    events,
    older_before_seq: null,
    has_more: false,
    open_turn_id: 1,
    open_turn_start_seq: events[0]?.seq ?? null,
    session_config: null,
  };
}

describe("Workspace Review V1 ChangeSet selector", () => {
  it("returns null without an owner or a ToolCall ChangeSet", () => {
    expect(selectLatestWorkspaceReviewChangeSetV1(snapshot([], 1, false))).toBeNull();
    expect(selectLatestWorkspaceReviewChangeSetV1(snapshot([
      toolSummary(1),
      displayNoteSummary(2),
    ]))).toBeNull();
  });

  it("selects the latest real ToolCall seq and preserves a failed tool's API identity", () => {
    // selected comes from the later failed tool even though an intervening tool has no ChangeSet.
    // selected 来自更晚的失败工具，即使中间工具没有 ChangeSet。
    const selected = selectLatestWorkspaceReviewChangeSetV1(snapshot([
      toolSummary(1, {
        apiCallId: 11,
        toolCallId: 101,
        changeset: changeSet("first"),
      }),
      toolSummary(2, {
        apiCallId: 11,
        toolCallId: 102,
        status: "completed",
      }),
      toolSummary(3, {
        apiCallId: 12,
        toolCallId: 103,
        status: "failed",
        changeset: changeSet("failed-latest"),
      }),
    ]));

    expect(selected).toMatchObject({
      session_id: "session-review-v1",
      window_generation: 1,
      seq: 3,
      event_revision: 1,
      turn_id: 1,
      tool_call: {
        api_call_id: 12,
        tool_call_id: 103,
        status: "failed",
      },
    });
    expect(selected?.tool_call.changeset.summary).toBe("failed-latest");
  });

  it("tracks same-seq ChangeSet evolution only through event_revision", () => {
    // initialSelection and evolvedSelection represent two atomic snapshots of one ToolCall seq.
    // initialSelection 与 evolvedSelection 表示同一 ToolCall seq 的两份原子快照。
    const initialSelection = selectLatestWorkspaceReviewChangeSetV1(snapshot([
      toolSummary(1, {
        eventRevision: 1,
        status: "running",
        changeset: changeSet("preview", "preview"),
      }),
    ]));
    const evolvedSelection = selectLatestWorkspaceReviewChangeSetV1(snapshot([
      toolSummary(1, {
        eventRevision: 2,
        status: "completed",
        changeset: changeSet("applied", "applied"),
      }),
    ]));

    expect(initialSelection).toMatchObject({ seq: 1, event_revision: 1 });
    expect(initialSelection?.tool_call.changeset.mode).toBe("preview");
    expect(evolvedSelection).toMatchObject({ seq: 1, event_revision: 2 });
    expect(evolvedSelection?.tool_call.changeset.mode).toBe("applied");
  });

  it("keeps the updated tail selected after older history is prepended", () => {
    // selectedBeforePrepend is the tail selection before an adjacent historical page arrives.
    // selectedBeforePrepend 是相邻历史分页到达前的尾部选择结果。
    const selectedBeforePrepend = selectLatestWorkspaceReviewChangeSetV1(snapshot([
      displayNoteSummary(4),
      toolSummary(5, { changeset: changeSet("tail") }),
    ]));
    // selectedAfterPrepend includes an older ChangeSet but must retain the higher real seq.
    // selectedAfterPrepend 包含更早 ChangeSet，但必须保留更高真实 seq。
    const selectedAfterPrepend = selectLatestWorkspaceReviewChangeSetV1(snapshot([
      toolSummary(1, { changeset: changeSet("historical") }),
      displayNoteSummary(2),
      displayNoteSummary(3),
      displayNoteSummary(4),
      toolSummary(5, { changeset: changeSet("tail") }),
    ]));

    expect(selectedBeforePrepend?.seq).toBe(5);
    expect(selectedAfterPrepend?.seq).toBe(5);
    expect(selectedAfterPrepend?.tool_call.changeset.summary).toBe("tail");
  });

  it("does not revive a previous-generation selection after rebase", () => {
    // previousGeneration captures a valid selection before edit-generation replacement.
    // previousGeneration 捕获编辑代际替换前的有效选择结果。
    const previousGeneration = selectLatestWorkspaceReviewChangeSetV1(snapshot([
      toolSummary(1, { changeset: changeSet("generation-one") }),
    ], 1));
    // rebasedWithoutChangeSet proves the old selection is not cached into generation two.
    // rebasedWithoutChangeSet 证明旧选择不会被缓存进第二代。
    const rebasedWithoutChangeSet = selectLatestWorkspaceReviewChangeSetV1(snapshot([
      toolSummary(1),
    ], 2));
    // rebasedSelection carries the exact new owner generation once a new ChangeSet exists.
    // rebasedSelection 在新 ChangeSet 存在后携带精确的新归属代次。
    const rebasedSelection = selectLatestWorkspaceReviewChangeSetV1(snapshot([
      toolSummary(1, { changeset: changeSet("generation-two") }),
    ], 2));

    expect(previousGeneration?.window_generation).toBe(1);
    expect(rebasedWithoutChangeSet).toBeNull();
    expect(rebasedSelection).toMatchObject({
      window_generation: 2,
      seq: 1,
      event_revision: 1,
    });
    expect(rebasedSelection?.tool_call.changeset.summary).toBe("generation-two");
  });

  it("preserves truncated lifecycle content without metadata or path reconstruction", () => {
    // truncated is the producer-authored lifecycle body retained inside the ToolCall ChangeSet.
    // truncated 是保留在 ToolCall ChangeSet 内的产出方原始生命周期正文。
    const truncated: PersistentLifecycleContent = {
      mode: "truncated",
      head: "line-1\nline-2",
      tail: "line-99\nline-100",
      total_line_count: 100,
    };
    const selected = selectLatestWorkspaceReviewChangeSetV1(snapshot([
      toolSummary(1, { changeset: changeSet("deleted", "applied", truncated) }),
    ]));

    expect(selected?.tool_call.changeset.files[0]?.lifecycle_content).toEqual(truncated);
  });

  it("rejects malformed event order instead of sorting it into a plausible selection", () => {
    expect(() => selectLatestWorkspaceReviewChangeSetV1(snapshot([
      toolSummary(1, { changeset: changeSet("first") }),
      toolSummary(3, { changeset: changeSet("third") }),
      toolSummary(2, { changeset: changeSet("second") }),
    ]))).toThrow(/requires consecutive seq/);
  });
});
