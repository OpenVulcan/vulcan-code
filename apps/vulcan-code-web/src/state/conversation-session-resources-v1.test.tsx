// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { resourceId, type ResourceId } from "@/service/conversation-event-window-store";
import type { SessionResourceRpcV1 } from "@/service/conversation-session-resources-v1";
import {
  useConversationSessionResourcesV1,
  type ConversationSessionResourceCatalogV1,
} from "./conversation-session-resources-v1";

/** Deferred exposes one externally controlled resource-list response.
 * Deferred 暴露一个由外部控制的资源目录响应。 */
interface Deferred<T> {
  /** Promise observed by the resource hook.
   * 资源 Hook 观察到的 Promise。 */
  promise: Promise<T>;
  /** Resolves the resource-list response.
   * 完成资源目录响应。 */
  resolve: (value: T) => void;
}

/** Creates a deterministic promise without timers.
 * 创建一个不使用计时器的确定性 Promise。
 *
 * @returns one externally controlled promise.
 *          一个可从外部控制的 Promise。 */
function deferred<T>(): Deferred<T> {
  // resolvePromise is assigned by the Promise constructor and retained by the test.
  // resolvePromise 由 Promise 构造器赋值并由测试保留。
  let resolvePromise!: (value: T) => void;
  // promise is the exact asynchronous value observed by production code.
  // promise 是生产代码观察到的精确异步值。
  const promise = new Promise<T>((resolve) => { resolvePromise = resolve; });
  return { promise, resolve: resolvePromise };
}

// client records metadata requests while returning deterministic resource facts.
// client 记录元数据请求，并返回确定的资源事实。
const client: SessionResourceRpcV1 = {
  list: vi.fn(),
  contentUrl: (sessionId, currentResourceId) => `/sessions/${sessionId}/resources/${currentResourceId}`,
};
// catalog exposes the latest committed hook state to assertions.
// catalog 向断言暴露最新提交的 Hook 状态。
let catalog: ConversationSessionResourceCatalogV1;
// root and host own one isolated React tree per test.
// root 与 host 为每项测试持有一棵隔离 React 树。
let root: Root;
let host: HTMLDivElement;

/** Harness mounts the exact session-resource hook used by V1 conversation rendering.
 * Harness 挂载 V1 会话渲染使用的精确会话资源 Hook。
 *
 * @param props session owner and ordered resource identities.
 *              会话所有者与有序资源身份。
 * @returns no visible test markup.
 *          不返回可见测试标记。 */
function Harness(props: { sessionId?: string; resourceIds: readonly ResourceId[] }) {
  catalog = useConversationSessionResourcesV1(
    props.sessionId ?? "session-streaming",
    props.resourceIds,
    client,
  );
  return null;
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.resetAllMocks();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => { root.unmount(); });
  host.remove();
  vi.unstubAllGlobals();
});

describe("useConversationSessionResourcesV1", () => {
  // Streaming event projections rebuild arrays but must retain an already loaded identical catalog.
  // 流式事件投影会重建数组，但必须保留已经加载且内容相同的目录。
  it("does not reload or clear resources when only the resource-id array reference changes", async () => {
    // imageResourceId represents the persisted user-image attachment visible during streaming.
    // imageResourceId 表示流式期间可见的持久化用户图片附件。
    const imageResourceId = resourceId(17);
    vi.mocked(client.list).mockResolvedValueOnce({
      sessionId: "session-streaming",
      resources: [{
        availability: "available",
        resource_id: imageResourceId,
        original_name: "streaming-image.png",
        mime: "image/png",
        size: 128,
      }],
    });

    await act(async () => { root.render(<Harness resourceIds={[imageResourceId]} />); });
    expect(client.list).toHaveBeenCalledTimes(1);
    expect(catalog.status).toBe("ready");
    expect(catalog.resources.get(imageResourceId)?.availability).toBe("available");

    // unexpectedReload keeps any erroneous second request pending so a loading-state regression is observable.
    // unexpectedReload 让错误的第二次请求保持等待，使加载态回归可被观察。
    const unexpectedReload = deferred<Awaited<ReturnType<SessionResourceRpcV1["list"]>>>();
    vi.mocked(client.list).mockReturnValueOnce(unexpectedReload.promise);
    await act(async () => { root.render(<Harness resourceIds={[imageResourceId]} />); });

    expect(client.list).toHaveBeenCalledTimes(1);
    expect(catalog.status).toBe("ready");
    expect(catalog.resources.get(imageResourceId)?.availability).toBe("available");
  });

  // A genuinely different ordered resource set must still start one new metadata request.
  // 真正不同的有序资源集合仍必须发起一次新的元数据请求。
  it("reloads metadata when the resource values change", async () => {
    // firstResourceId is the already loaded attachment identity.
    // firstResourceId 是已经加载的附件身份。
    const firstResourceId = resourceId(21);
    // secondResourceId proves that a real set expansion still crosses the RPC boundary.
    // secondResourceId 证明真实集合扩展仍会跨越 RPC 边界。
    const secondResourceId = resourceId(22);
    vi.mocked(client.list)
      .mockResolvedValueOnce({
        sessionId: "session-streaming",
        resources: [{
          availability: "available",
          resource_id: firstResourceId,
          original_name: "first.png",
          mime: "image/png",
          size: 64,
        }],
      })
      .mockResolvedValueOnce({
        sessionId: "session-streaming",
        resources: [
          {
            availability: "available",
            resource_id: firstResourceId,
            original_name: "first.png",
            mime: "image/png",
            size: 64,
          },
          {
            availability: "available",
            resource_id: secondResourceId,
            original_name: "second.png",
            mime: "image/png",
            size: 96,
          },
        ],
      });

    await act(async () => { root.render(<Harness resourceIds={[firstResourceId]} />); });
    await act(async () => {
      root.render(<Harness resourceIds={[firstResourceId, secondResourceId]} />);
    });

    expect(client.list).toHaveBeenCalledTimes(2);
    expect(client.list).toHaveBeenLastCalledWith(
      "session-streaming",
      [firstResourceId, secondResourceId],
    );
    expect(catalog.status).toBe("ready");
    expect(catalog.resources.has(secondResourceId)).toBe(true);
  });
});
