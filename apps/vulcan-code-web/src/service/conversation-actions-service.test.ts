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

describe("conversation submission receipt", () => {
  beforeEach(() => {
    transport.request.mockReset();
  });

  // The accepted input has a durable identity while Core waits for dispatch.
  // 核心等待派发时，已接收输入持有可靠身份。
  it("returns the original accepted waiting receipt without resending", async () => {
    // receipt proves admission without claiming a business API has already started.
    // receipt 证明输入已接收，同时不声称业务 API 已经启动。
    const receipt = { clientSubmissionId: "submission-1", conversationId: "conversation-1", queuedTurnDispatch: "waiting" };
    transport.request.mockResolvedValueOnce(receipt);
    await expect(submit()).resolves.toBe(receipt);
    expect(transport.request).toHaveBeenCalledTimes(1);
    expect(transport.request.mock.calls[0][1]).toMatchObject({ clientSubmissionId: "submission-1" });
  });

  // A transport rejection can occur after the service accepts a request.
  // 服务接收请求后，传输仍可能失败。
  it("surfaces a rejected submission without replaying it", async () => {
    // failure represents the original transport error kept for the caller.
    // failure 表示保留给调用方的原始传输错误。
    const failure = new Error("Response unavailable");
    transport.request.mockRejectedValueOnce(failure);
    await expect(submit()).rejects.toBe(failure);
    expect(transport.request).toHaveBeenCalledTimes(1);
  });
});
