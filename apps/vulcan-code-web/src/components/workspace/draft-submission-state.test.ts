import { describe, expect, it } from "vitest";
import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";
import type { ConversationMessage } from "@/types";
import {
  hasAuthoritativeSubmissionCorrelation,
  materializeAcknowledgedDraftSubmission,
  retireMaterializedDraftSubmission,
} from "@/components/workspace/draft-submission-state";

// pendingDraft creates the exact transition fixture shared by draft-submission state tests.
// pendingDraft 创建草稿提交状态测试共享的精确过渡夹具。
//
// Returns one draft containing the requested client correlation identity.
// 返回一条包含指定客户端关联身份的草稿。
function pendingDraft(clientSubmissionId = "client-1"): DraftConversationSelection {
  return {
    id: "draft-project-1",
    projectId: "project-1",
    projectName: "Project 1",
    pendingSubmission: {
      clientSubmissionId,
      prompt: "hello",
      attachments: [],
      startedAt: 100,
      stage: "creating",
    },
  };
}

describe("draft submission materialization", () => {
  it("keeps the optimistic submission under the durable id until the sidebar row arrives", () => {
    expect(materializeAcknowledgedDraftSubmission(
      pendingDraft(),
      "draft-project-1",
      "client-1",
      "conversation-1",
      false,
    )).toEqual({
      ...pendingDraft(),
      id: "conversation-1",
      pendingSubmission: {
        ...pendingDraft().pendingSubmission,
        stage: "preparing",
      },
    });
  });

  it("retires the draft immediately when the authoritative row won the acknowledgement race", () => {
    expect(materializeAcknowledgedDraftSubmission(
      pendingDraft(),
      "draft-project-1",
      "client-1",
      "conversation-1",
      true,
    )).toBeNull();
  });

  it("preserves a newer unrelated draft and retires only the matching transition", () => {
    const current = pendingDraft("client-2");
    expect(materializeAcknowledgedDraftSubmission(
      current,
      "draft-project-1",
      "client-1",
      "conversation-1",
      false,
    )).toBe(current);
    expect(retireMaterializedDraftSubmission(current, "conversation-1")).toBe(current);
    expect(retireMaterializedDraftSubmission({ ...current, id: "conversation-1" }, "conversation-1")).toBeNull();
  });

  it("recognizes the authoritative user message by the exact client submission id", () => {
    const messages: ConversationMessage[] = [{
      id: "message-1",
      conversationId: "conversation-1",
      createdAt: "100",
      role: "user",
      body: "hello",
      metadata: { client_submission_id: "client-1" },
    }];
    expect(hasAuthoritativeSubmissionCorrelation(messages, "client-1")).toBe(true);
    expect(hasAuthoritativeSubmissionCorrelation(messages, "client-2")).toBe(false);
  });
});
