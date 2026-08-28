import { describe, expect, it, vi } from "vitest";

import {
  clientActionId,
  resourceId,
  type ClientActionId,
  type ConversationEventWindowOwner,
} from "@/service/conversation-event-window-store";
import {
  ConversationUserInputOptimisticConflictError,
  ConversationUserInputOptimisticDisposedError,
  ConversationUserInputOptimisticOverlayStoreV1,
  ConversationUserInputOptimisticOwnerMismatchError,
  type ConversationUserInputSummaryV1,
  type UserInputOptimisticAttachmentDisplayV1,
  type UserInputOptimisticSubmissionV1,
} from "./conversation-user-input-optimistic-overlay-v1";

/** Returns one exact test owner.
 * 返回一个精确测试归属。 */
function owner(sessionId = "session-a", windowGeneration = 1): ConversationEventWindowOwner {
  return {
    session_id: sessionId,
    window_generation: windowGeneration,
  };
}

/** Returns process-local attachment display data with no durable or local path.
 * 返回不含持久化路径或本地路径的进程内附件展示数据。 */
function attachment(displayId = "attachment-1"): UserInputOptimisticAttachmentDisplayV1 {
  return {
    display_id: displayId,
    kind: "image",
    file_name: "diagram.png",
    mime_type: "image/png",
  };
}

/** Builds one optimistic submission owned by an exact browser action.
 * 构建一份由精确浏览器动作持有的乐观提交。 */
function submission(
  currentOwner: ConversationEventWindowOwner,
  currentClientActionId: ClientActionId,
  text = "Inspect this image",
  attachments: readonly UserInputOptimisticAttachmentDisplayV1[] = [attachment()],
): UserInputOptimisticSubmissionV1 {
  return {
    owner: currentOwner,
    client_action_id: currentClientActionId,
    text,
    attachments,
    started_at_ms: 1_787_790_000_000,
  };
}

/** Builds one canonical V1 user_input event carrying the browser ClientActionId.
 * 构建一条携带浏览器 ClientActionId 的 canonical V1 user_input 事件。 */
function userInputEvent(
  currentClientActionId: ClientActionId,
  text = "Inspect this image",
  seq = 1,
): ConversationUserInputSummaryV1 {
  return {
    seq,
    turn_id: 1,
    created_at_ms: 1_787_790_000_010,
    updated_at_ms: 1_787_790_000_010,
    event_revision: 1,
    visibility: "provider",
    event: {
      kind: "user_input",
      client_action_id: currentClientActionId,
      input_id: 1,
      input_source: "root",
      text,
      resource_ids: [resourceId(17)],
    },
  };
}

describe("conversation user-input optimistic overlay v1", () => {
  it("keeps an acknowledged overlay until the matching V1 user_input event arrives", () => {
    // store starts with one exact owner and one browser action.
    // store 从一个精确归属和一个浏览器动作开始。
    const store = new ConversationUserInputOptimisticOverlayStoreV1();
    const currentOwner = owner();
    const actionId = clientActionId("action-ack-then-event");
    store.rebaseOwner(currentOwner);
    store.startSubmission(submission(currentOwner, actionId));

    store.acknowledge(currentOwner, actionId);

    expect(store.snapshot().overlays).toMatchObject([{
      client_action_id: actionId,
      text: "Inspect this image",
      phase: "acknowledged",
    }]);
    expect(store.snapshot().overlays[0]).not.toHaveProperty("seq");

    store.observeUserInput(currentOwner, userInputEvent(actionId));

    expect(store.snapshot().overlays).toEqual([]);
  });

  it("removes an RPC failure according to the current product contract", () => {
    // store owns one visible submission before the RPC rejects.
    // store 在 RPC 拒绝前持有一条可见提交。
    const store = new ConversationUserInputOptimisticOverlayStoreV1();
    const currentOwner = owner();
    const actionId = clientActionId("action-rpc-failure");
    store.rebaseOwner(currentOwner);
    store.startSubmission(submission(currentOwner, actionId));

    store.failRpc(currentOwner, actionId);

    expect(store.snapshot().overlays).toEqual([]);
    // A same-content retry with the same id is idempotent and cannot revive a failed row.
    // 使用相同 id 的同内容重试保持幂等，且不能复活失败行。
    store.startSubmission(submission(currentOwner, actionId));
    expect(store.snapshot().overlays).toEqual([]);
  });

  it("treats exact duplicate actions as idempotent and rejects payload or owner drift", () => {
    // listener proves duplicate registration does not publish a second state.
    // listener 证明重复登记不会发布第二份状态。
    const listener = vi.fn();
    const store = new ConversationUserInputOptimisticOverlayStoreV1();
    const firstOwner = owner();
    const actionId = clientActionId("action-idempotent");
    store.subscribe(listener);
    store.rebaseOwner(firstOwner);
    store.startSubmission(submission(firstOwner, actionId));
    const callsAfterFirstStart = listener.mock.calls.length;

    store.startSubmission(submission(firstOwner, actionId));

    expect(listener).toHaveBeenCalledTimes(callsAfterFirstStart);
    expect(() => store.startSubmission(submission(firstOwner, actionId, "Changed text")))
      .toThrow(ConversationUserInputOptimisticConflictError);

    store.rebaseOwner(owner("session-b", 1));
    expect(() => store.startSubmission(submission(owner("session-b", 1), actionId)))
      .toThrow(ConversationUserInputOptimisticConflictError);
  });

  it("converges when the canonical event arrives before the RPC ACK", () => {
    // store begins the browser action before either asynchronous result wins the race.
    // store 在任一异步结果赢得竞态前启动浏览器动作。
    const store = new ConversationUserInputOptimisticOverlayStoreV1();
    const currentOwner = owner();
    const actionId = clientActionId("action-event-before-ack");
    store.rebaseOwner(currentOwner);
    store.startSubmission(submission(currentOwner, actionId));

    store.observeUserInput(currentOwner, userInputEvent(actionId));
    store.acknowledge(currentOwner, actionId);

    expect(store.snapshot().overlays).toEqual([]);
  });

  it("never redirects or revives an overlay across generation and session rebases", () => {
    // oldOwner owns the original browser action before an edit generation is published.
    // oldOwner 在编辑代际发布前持有原始浏览器动作。
    const store = new ConversationUserInputOptimisticOverlayStoreV1();
    const oldOwner = owner("session-a", 1);
    const newGenerationOwner = owner("session-a", 2);
    const actionId = clientActionId("action-old-generation");
    store.rebaseOwner(oldOwner);
    store.startSubmission(submission(oldOwner, actionId));

    store.rebaseOwner(newGenerationOwner);
    store.acknowledge(oldOwner, actionId);
    store.observeUserInput(oldOwner, userInputEvent(actionId));
    expect(store.snapshot()).toMatchObject({
      owner: newGenerationOwner,
      overlays: [],
    });

    store.rebaseOwner(owner("session-b", 1));
    store.rebaseOwner(oldOwner);
    expect(store.snapshot().overlays).toEqual([]);
  });

  it("ignores unrelated canonical inputs and rejects same-action text drift", () => {
    // store contains one action while another authoritative input enters the same owner.
    // store 包含一个动作，同时另一条权威输入进入相同归属。
    const store = new ConversationUserInputOptimisticOverlayStoreV1();
    const currentOwner = owner();
    const actionId = clientActionId("action-owned");
    store.rebaseOwner(currentOwner);
    store.startSubmission(submission(currentOwner, actionId));

    store.observeUserInput(currentOwner, userInputEvent(clientActionId("action-unrelated")));
    expect(store.snapshot().overlays).toHaveLength(1);
    expect(() => store.observeUserInput(currentOwner, userInputEvent(actionId, "Drifted text")))
      .toThrow(ConversationUserInputOptimisticConflictError);
    expect(store.snapshot().overlays).toHaveLength(1);
  });

  it("keeps attachment display data process-local and path-free", () => {
    // store copies the smallest display-only attachment structure.
    // store 复制最小的仅展示附件结构。
    const store = new ConversationUserInputOptimisticOverlayStoreV1();
    const currentOwner = owner();
    const actionId = clientActionId("action-attachment-display");
    store.rebaseOwner(currentOwner);
    store.startSubmission(submission(currentOwner, actionId));

    const visibleAttachment = store.snapshot().overlays[0]?.attachments[0];
    expect(visibleAttachment).toEqual(attachment());
    expect(visibleAttachment).not.toHaveProperty("localPath");
    expect(visibleAttachment).not.toHaveProperty("resourcePath");
    expect(visibleAttachment).not.toHaveProperty("assetId");
  });

  it("rejects a submission for a non-current owner instead of retargeting it", () => {
    // store owns session-a while the new submission still belongs to session-b.
    // store 持有 session-a，而新提交仍属于 session-b。
    const store = new ConversationUserInputOptimisticOverlayStoreV1();
    const currentOwner = owner("session-a", 1);
    store.rebaseOwner(currentOwner);

    expect(() => store.startSubmission(submission(
      owner("session-b", 1),
      clientActionId("action-wrong-owner"),
    ))).toThrow(ConversationUserInputOptimisticOwnerMismatchError);
    expect(store.snapshot().overlays).toEqual([]);
  });

  it("permanently clears ownership and rejects mutation after disposal", () => {
    // store has one active row and one listener before permanent disposal.
    // store 在永久释放前持有一条活动行和一个监听器。
    const listener = vi.fn();
    const store = new ConversationUserInputOptimisticOverlayStoreV1();
    const currentOwner = owner();
    const actionId = clientActionId("action-disposed");
    store.subscribe(listener);
    store.rebaseOwner(currentOwner);
    store.startSubmission(submission(currentOwner, actionId));

    store.dispose();
    const callsAfterDispose = listener.mock.calls.length;

    expect(store.snapshot()).toEqual({ owner: null, overlays: [], disposed: true });
    expect(() => store.acknowledge(currentOwner, actionId))
      .toThrow(ConversationUserInputOptimisticDisposedError);
    expect(listener).toHaveBeenCalledTimes(callsAfterDispose);
  });

  it("validates the exact Core ClientActionId byte and whitespace contract", () => {
    expect(() => clientActionId(" action-with-padding ")).toThrow();
    expect(() => clientActionId("界".repeat(86))).toThrow();
    expect(clientActionId("界".repeat(85))).toHaveLength(85);
  });
});
