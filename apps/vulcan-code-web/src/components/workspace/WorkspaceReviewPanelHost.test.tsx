import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import { WorkspaceReviewPanelHost } from "./WorkspaceReviewPanelHost";
import { I18nProvider } from "@/i18n";
import type { DiffPanelSnapshot } from "@/diff/types";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type {
  ConversationEventBody,
  ConversationEventSummary,
  PersistentChangeSet,
} from "@/service/conversation-event-window-store";
import {
  ConversationEventWindowExternalStoreProvider,
  type ConversationEventWindowExternalStore,
} from "@/state/conversation-event-window-react";
import type { WorkbenchSnapshot } from "@/types";

// DiffReviewPanel is reduced to a deterministic probe so this production-host test remains DOM-free.
// DiffReviewPanel 被收窄为确定性探针，使生产 Host 测试无需 DOM 环境。
vi.mock("@/diff/DiffReviewPanel", () => ({
  /** Renders the exact snapshot identity passed by the production Workspace Review host.
   * 渲染生产 Workspace Review Host 传入的精确快照身份。
   *
   * @param props snapshot and empty-state text supplied by the host.
   *              Host 提供的快照与空状态文本。
   * @returns one deterministic test probe.
   *          一个确定性测试探针。
   */
  DiffReviewPanel(props: { snapshot: DiffPanelSnapshot | null; emptyBody: string }) {
    // filePaths exposes the actual typed files selected by the host.
    // filePaths 暴露 Host 实际选中的强类型文件。
    const filePaths = props.snapshot?.files.map((file) => file.newPath).join("|") ?? props.emptyBody;
    return (
      <section data-review-source={props.snapshot?.source ?? "none"}>
        {filePaths}
      </section>
    );
  },
}));

/** Builds one exact typed ChangeSet for the production review host.
 * 为生产审核 Host 构建一份精确的强类型 ChangeSet。
 *
 * @param label file and content identity used by assertions.
 *              断言使用的文件与正文身份。
 * @returns one applied modify ChangeSet.
 *          一份已应用的修改 ChangeSet。
 */
function changeSet(label: string): PersistentChangeSet {
  return {
    mode: "applied",
    summary: label,
    files: [{
      change: "modify",
      path: `${label}.txt`,
      hunks: [{
        before: "context-before",
        after: "context-after",
        deleted: [{ line: 2, content: `${label}-old` }],
        inserted: [{ line: 2, content: `${label}-new` }],
      }],
    }],
  };
}

/** Builds one canonical ToolCall event summary for the production host tests.
 * 为生产 Host 测试构建一条 canonical ToolCall 事件摘要。
 *
 * @param seq canonical event sequence.
 *            canonical 事件序号。
 * @param options exact status, revision, and optional ChangeSet.
 *                精确状态、修订号与可选 ChangeSet。
 * @returns one typed event summary.
 *          一条强类型事件摘要。
 */
function toolSummary(
  seq: number,
  options: {
    status?: Extract<ConversationEventBody, { kind: "tool_call" }>["status"];
    eventRevision?: number;
    changeset?: PersistentChangeSet;
  } = {},
): ConversationEventSummary {
  return {
    seq,
    turn_id: 7,
    created_at_ms: 100,
    updated_at_ms: 100,
    event_revision: options.eventRevision ?? 1,
    visibility: "provider",
    event: {
      kind: "tool_call",
      api_call_id: 9,
      tool_call_id: seq,
      tool_name: "file_edit",
      input: { path: `tool-${seq}.txt` },
      status: options.status ?? "completed",
      ...(options.changeset === undefined ? {} : { changeset: options.changeset }),
    },
  };
}

/** Builds one non-tool event that still occupies canonical sequence space.
 * 构建一条仍占据 canonical 序号空间的非工具事件。
 *
 * @param seq canonical event sequence.
 *            canonical 事件序号。
 * @returns one typed display-note summary.
 *          一条强类型显示备注摘要。
 */
function displayNoteSummary(seq: number): ConversationEventSummary {
  return {
    seq,
    turn_id: 7,
    created_at_ms: 100,
    updated_at_ms: 100,
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

/** Builds one immutable selected-session V1 controller snapshot.
 * 构建一份不可变的选中会话 V1 控制器快照。
 *
 * @param events canonical event window in exact seq order.
 *               按精确 seq 顺序排列的 canonical 事件窗口。
 * @param windowGeneration owner generation for edit/rebase tests.
 *                         编辑与 rebase 测试使用的归属代次。
 * @param ownerPresent whether the selected-session owner exists.
 *                     是否存在选中会话归属。
 * @returns one complete controller snapshot.
 *          一份完整控制器快照。
 */
function snapshot(
  events: readonly ConversationEventSummary[],
  windowGeneration = 1,
  ownerPresent = true,
): ConversationEventWindowControllerSnapshot {
  return {
    owner: ownerPresent
      ? { session_id: "session-review-host", window_generation: windowGeneration }
      : null,
    service_epoch: ownerPresent ? "service-review-host" : null,
    memory_revision: ownerPresent ? windowGeneration : null,
    durable_revision: ownerPresent ? windowGeneration : null,
    events,
    older_before_seq: null,
    has_more: false,
    open_turn_id: 7,
    open_turn_start_seq: events[0]?.seq ?? null,
    session_config: null,
  };
}

/** Builds the minimal workbench facts still needed for project-scoped Git actions.
 * 构建项目级 Git 动作仍然需要的最小工作台事实。
 *
 * @returns one selected conversation and its explicit project.
 *          一条选中会话及其明确项目。
 */
function workbench(): WorkbenchSnapshot {
  return {
    projects: [{ id: "project-review", path: "D:\\workspace-review" }],
    conversations: [{ id: "session-review-host", projectId: "project-review" }],
    messages: [{
      id: "legacy-message-that-must-not-be-read",
      conversationId: "session-review-host",
      role: "assistant",
      content: "```diff\n-old legacy\n+new legacy\n```",
    }],
  } as unknown as WorkbenchSnapshot;
}

/** Renders the real production host through the real V1 external-store React boundary.
 * 通过真实 V1 external-store React 边界渲染真实生产 Host。
 *
 * @param controllerSnapshot atomically published selected-session snapshot.
 *                           原子发布的选中会话快照。
 * @returns static production component markup.
 *          生产组件的静态标记。
 */
function renderHost(controllerSnapshot: ConversationEventWindowControllerSnapshot): string {
  // externalStore exposes the exact snapshot without a legacy transcript fallback.
  // externalStore 暴露精确快照且不提供旧 transcript fallback。
  const externalStore: ConversationEventWindowExternalStore = {
    subscribe: () => () => undefined,
    getSnapshot: () => controllerSnapshot,
    getServerSnapshot: () => controllerSnapshot,
    getCommands: () => ({
      rehydrate: async () => false,
      prependOlder: async () => false,
    }),
  };
  // props supplies only project Git services; conversation review data comes from V1 context.
  // props 只提供项目 Git 服务；会话审核数据来自 V1 context。
  const props: React.ComponentProps<typeof WorkspaceReviewPanelHost> = {
    selectedDraftConversation: null,
    selectedConversationId: "session-review-host",
    workbench: workbench(),
    reviewFocusRequest: { sequence: 0, repositoryKey: null },
    onCopyRawDiff: () => undefined,
    loadWorkspaceDiffSnapshot: async () => {
      throw new Error("workspace diff snapshot must not load during static host rendering");
    },
    loadWorkspaceDiffFileDetail: async () => {
      throw new Error("workspace diff detail must not load during static host rendering");
    },
    loadWorkspaceDiffSearch: async () => {
      throw new Error("workspace diff search must not load during static host rendering");
    },
    runWorkspaceGitFileAction: async () => {
      throw new Error("workspace Git action must not run during static host rendering");
    },
  };
  return renderToStaticMarkup(
    <I18nProvider>
      <ConversationEventWindowExternalStoreProvider store={externalStore}>
        <WorkspaceReviewPanelHost {...props} />
      </ConversationEventWindowExternalStoreProvider>
    </I18nProvider>,
  );
}

describe("WorkspaceReviewPanelHost V1 production source", () => {
  it("renders no conversation review without an owner or without a typed ChangeSet", () => {
    // noOwnerMarkup proves old workbench.messages cannot supply a fallback diff.
    // noOwnerMarkup 证明旧 workbench.messages 无法提供 fallback diff。
    const noOwnerMarkup = renderHost(snapshot([], 1, false));
    // noChangeSetMarkup proves a present owner alone does not invent review content.
    // noChangeSetMarkup 证明仅存在 owner 不会虚构审核内容。
    const noChangeSetMarkup = renderHost(snapshot([toolSummary(1)]));

    expect(noOwnerMarkup).not.toContain("legacy-message-that-must-not-be-read");
    expect(noOwnerMarkup).not.toContain("old legacy");
    expect(noChangeSetMarkup).not.toContain("old legacy");
    expect(noOwnerMarkup).toContain('data-review-source="none"');
    expect(noChangeSetMarkup).toContain('data-review-source="none"');
  });

  it("renders the newest failed ToolCall ChangeSet instead of regrouping by status", () => {
    // markup contains only the later failed tool's typed file path.
    // markup 只包含更晚失败工具的强类型文件路径。
    const markup = renderHost(snapshot([
      toolSummary(1, { changeset: changeSet("completed-earlier") }),
      toolSummary(2),
      toolSummary(3, { status: "failed", changeset: changeSet("failed-latest") }),
    ]));

    expect(markup).toContain("failed-latest.txt");
    expect(markup).not.toContain("completed-earlier.txt");
  });

  it("replaces one same-seq review only when event_revision carries evolved content", () => {
    // previewMarkup represents the first durable form of one ToolCall seq.
    // previewMarkup 表示同一 ToolCall seq 的第一份持久化形态。
    const previewMarkup = renderHost(snapshot([
      toolSummary(1, { eventRevision: 1, changeset: changeSet("revision-one") }),
    ]));
    // evolvedMarkup represents the later in-place semantic revision.
    // evolvedMarkup 表示后续原位语义修订。
    const evolvedMarkup = renderHost(snapshot([
      toolSummary(1, { eventRevision: 2, changeset: changeSet("revision-two") }),
    ]));

    expect(previewMarkup).toContain("revision-one.txt");
    expect(evolvedMarkup).toContain("revision-two.txt");
    expect(evolvedMarkup).not.toContain("revision-one.txt");
  });

  it("does not revive a prior-generation review after an edit rebase", () => {
    // previousMarkup proves generation one had visible review content.
    // previousMarkup 证明第一代存在可见审核内容。
    const previousMarkup = renderHost(snapshot([
      toolSummary(1, { changeset: changeSet("generation-one") }),
    ], 1));
    // rebasedMarkup contains no ChangeSet in generation two and must remain empty.
    // rebasedMarkup 在第二代不含 ChangeSet，必须保持为空。
    const rebasedMarkup = renderHost(snapshot([
      toolSummary(1),
    ], 2));

    expect(previousMarkup).toContain("generation-one.txt");
    expect(rebasedMarkup).not.toContain("generation-one.txt");
    expect(rebasedMarkup).toContain('data-review-source="none"');
  });

  it("keeps the newer tail ChangeSet after adjacent historical prepend", () => {
    // markup contains an older historical ChangeSet and a later tail ChangeSet in canonical order.
    // markup 按 canonical 顺序包含更早历史 ChangeSet 与更晚尾部 ChangeSet。
    const markup = renderHost(snapshot([
      toolSummary(1, { changeset: changeSet("historical") }),
      displayNoteSummary(2),
      displayNoteSummary(3),
      displayNoteSummary(4),
      toolSummary(5, { changeset: changeSet("tail-authority") }),
    ]));

    expect(markup).toContain("tail-authority.txt");
    expect(markup).not.toContain("historical.txt");
  });
});
