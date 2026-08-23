import { describe, expect, it, vi } from "vitest";
import type { ConversationMessagesLoadResponse } from "@/types";

// requestMock captures physical service requests while the public loader retains only in-flight ownership.
// requestMock 捕获物理服务请求，用于验证公共加载器只保留在途所有权。
const { requestMock } = vi.hoisted(() => ({
  requestMock: vi.fn(),
}));

vi.mock("@/service/webui-service-runtime", () => ({
  hasServiceRuntime: () => true,
  serviceClient: {
    request: requestMock,
  },
}));

import { loadConversationMessages } from "@/service/workbench-snapshot-service";

// Deferred exposes one controllable promise for exact in-flight request assertions.
// Deferred 暴露一个可控 Promise，用于精确断言在途请求。
interface Deferred<T> {
  // promise is the unresolved transport result observed by every caller.
  // promise 是所有调用方共同观察的未完成传输结果。
  promise: Promise<T>;
  // resolve completes the retained transport with one authoritative response.
  // resolve 使用一份权威响应完成保留的传输。
  resolve: (value: T) => void;
}

// createDeferred builds one controllable transport promise.
// createDeferred 构造一个可控传输 Promise。
function createDeferred<T>(): Deferred<T> {
  // resolve is assigned synchronously by the Promise constructor before the result is returned.
  // resolve 会在返回结果前由 Promise 构造器同步赋值。
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((nextResolve) => {
    resolve = nextResolve;
  });
  return { promise, resolve };
}

describe("conversation message access singleflight", () => {
  it("shares one physical request across remount-style callers and releases ownership after settlement", async () => {
    // response is intentionally opaque because this test owns transport identity rather than projection shape.
    // response 刻意保持不透明，因为本测试验证传输身份而非投影结构。
    const response = { progress: { status: "ready" } } as ConversationMessagesLoadResponse;
    const firstTransport = createDeferred<ConversationMessagesLoadResponse>();
    requestMock.mockReset();
    requestMock.mockReturnValueOnce(firstTransport.promise);

    const firstCaller = loadConversationMessages("session-a");
    const remountedCaller = loadConversationMessages("session-a");

    expect(requestMock).toHaveBeenCalledTimes(1);
    expect(requestMock).toHaveBeenCalledWith("service_session_messages", { sessionId: "session-a" });

    firstTransport.resolve(response);
    await expect(firstCaller).resolves.toBe(response);
    await expect(remountedCaller).resolves.toBe(response);

    requestMock.mockResolvedValueOnce(response);
    await expect(loadConversationMessages("session-a")).resolves.toBe(response);
    expect(requestMock).toHaveBeenCalledTimes(2);
  });

  it("releases failed ownership so an explicit retry reaches the service", async () => {
    // response remains opaque because retry admission is independent from projection content.
    // response 保持不透明，因为重试准入与投影内容无关。
    const response = { progress: { status: "ready" } } as ConversationMessagesLoadResponse;
    requestMock.mockReset();
    requestMock.mockRejectedValueOnce(new Error("transport failed"));

    await expect(loadConversationMessages("session-b")).rejects.toThrow("transport failed");

    requestMock.mockResolvedValueOnce(response);
    await expect(loadConversationMessages("session-b")).resolves.toBe(response);
    expect(requestMock).toHaveBeenCalledTimes(2);
  });
});
