import { beforeEach, describe, expect, it, vi } from "vitest";
import { submitConversationPrompt } from "./conversation-actions-service";

// The hoisted transport spy observes the real submission contract without invoking a running service.
// 提升的传输间谍观察真实提交契约，不调用运行中的服务。
const transport = vi.hoisted(() => ({ request: vi.fn() }));

vi.mock("@/service/webui-service-runtime", () => ({
  hasServiceRuntime: () => true,
  serviceClient: transport,
}));

// submit sends one ordinary prompt with a fixed client submission identity.
// submit 用固定的 client submission 身份发送一条普通提示词。
function submit(): ReturnType<typeof submitConversationPrompt> {
  return submitConversationPrompt(
    "submission-1",
    "project-1",
    "conversation-1",
    "draft-slot",
    "hello",
    undefined,
    [],
    "build",
    null,
    null,
    null,
  );
}

describe("conversation submission dispatch retry", () => {
  beforeEach(() => {
    transport.request.mockReset();
  });

  // A refused physical dispatch is recoverable only through the identical durable identity.
  // 被拒绝的物理派发只能通过完全相同的可靠身份恢复。
  it("retries the same client submission id after a rejection", async () => {
    vi.useFakeTimers();
    const acknowledgement = { clientSubmissionId: "submission-1", conversationId: "conversation-1" };
    transport.request
      .mockRejectedValueOnce(new Error("physical dispatch admission refused"))
      .mockResolvedValueOnce(acknowledgement);

    const pending = submit();
    await vi.advanceTimersByTimeAsync(500);

    await expect(pending).resolves.toBe(acknowledgement);
    expect(transport.request).toHaveBeenCalledTimes(2);
    expect(transport.request.mock.calls.map((call) => (call[1] as { clientSubmissionId: string }).clientSubmissionId))
      .toEqual(["submission-1", "submission-1"]);
    vi.useRealTimers();
  });

  // The retry is bounded: a second rejection is reported instead of looping.
  // 重试是有界的：第二次拒绝直接上报，不循环。
  it("reports the second rejection without a third attempt", async () => {
    vi.useFakeTimers();
    transport.request
      .mockRejectedValueOnce(new Error("first rejection"))
      .mockRejectedValueOnce(new Error("second rejection"));

    const pending = submit();
    const assertion = expect(pending).rejects.toThrow("second rejection");
    await vi.advanceTimersByTimeAsync(500);
    await assertion;

    expect(transport.request).toHaveBeenCalledTimes(2);
    vi.useRealTimers();
  });

  // An accepted submission is never sent twice.
  // 已接纳的提交绝不发送两次。
  it("does not retry an accepted submission", async () => {
    const acknowledgement = { clientSubmissionId: "submission-1", conversationId: "conversation-1" };
    transport.request.mockResolvedValueOnce(acknowledgement);

    await expect(submit()).resolves.toBe(acknowledgement);
    expect(transport.request).toHaveBeenCalledTimes(1);
  });
});
