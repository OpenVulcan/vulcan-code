import { describe, expect, it } from "vitest";
import type { ServiceClient } from "@/service/client";
import type { SessionResourceRpcV1 } from "@/service/conversation-session-resources-v1";
import {
  parseSessionDraftV1,
  ServiceSessionDraftClientV1,
} from "@/service/conversation-session-drafts-v1";
import { resourceId } from "@/service/conversation-event-window-store";

/** Creates one request-only service facade for exact draft RPC assertions.
 * 创建一个仅含请求能力的服务门面，用于精确草稿 RPC 断言。
 */
function requestFacade(payload: unknown): {
  service: Pick<ServiceClient, "request">;
  calls: Array<{ method: string; params: unknown }>;
} {
  const calls: Array<{ method: string; params: unknown }> = [];
  return {
    calls,
    service: {
      request: async <T>(method: string, params?: unknown): Promise<T> => {
        calls.push({ method, params });
        return payload as T;
      },
    },
  };
}

/** Creates one deterministic browser-safe resource projection used by draft tests.
 * 创建一份确定性的浏览器安全资源投影，供草稿测试使用。
 */
function resourceFacade(): SessionResourceRpcV1 {
  return {
    list: async () => ({ sessionId: "unused", resources: [] }),
    contentUrl: (sessionId, currentResourceId) => `resource://${sessionId}/${currentResourceId}`,
  };
}

describe("ServiceSessionDraftClientV1", () => {
  it("rebuilds ordered available and missing attachments from ResourceIds", async () => {
    const facade = requestFacade({
      sessionId: "session-a",
      text: "draft text",
      resources: [
        {
          availability: "available",
          resource_id: 7,
          original_name: "diagram.png",
          mime: "image/png",
          size: 42,
        },
        {
          availability: "missing",
          resource_id: 9,
          placeholder: "[MISSING PROJECT RESOURCE]",
        },
      ],
    });
    const client = new ServiceSessionDraftClientV1(facade.service, resourceFacade());

    const draft = await client.read("session-a");

    expect(draft).toEqual({
      sessionId: "session-a",
      text: "draft text",
      attachments: [
        {
          kind: "image",
          fileName: "diagram.png",
          localPath: "resource://session-a/7",
          projectResourceId: resourceId(7),
          byteLen: 42,
          mimeType: "image/png",
        },
        {
          kind: "file",
          fileName: "[MISSING PROJECT RESOURCE]",
          localPath: "resource://session-a/9",
          projectResourceId: resourceId(9),
          byteLen: 0,
          mimeType: "application/octet-stream",
        },
      ],
    });
    expect(facade.calls).toEqual([{
      method: "service_session_draft_v1",
      params: { sessionId: "session-a" },
    }]);
  });

  it("sends one complete ordered draft replacement without deriving paths", async () => {
    const response = {
      sessionId: "session-a",
      text: "updated",
      resources: [{
        availability: "available",
        resource_id: 7,
        original_name: "diagram.png",
        mime: "image/png",
        size: 42,
      }],
    };
    const facade = requestFacade(response);
    const client = new ServiceSessionDraftClientV1(facade.service, resourceFacade());
    const attachment = {
      kind: "image",
      fileName: "diagram.png",
      localPath: "resource://session-a/7",
      projectResourceId: resourceId(7),
      byteLen: 42,
      mimeType: "image/png",
    };

    await client.write("session-a", "updated", [attachment]);

    expect(facade.calls).toEqual([{
      method: "service_set_session_draft_v1",
      params: {
        sessionId: "session-a",
        text: "updated",
        attachments: [attachment],
      },
    }]);
  });

  it("rejects owner drift and physical locator fields", () => {
    expect(() => parseSessionDraftV1({
      sessionId: "session-b",
      text: "draft",
      resources: [],
    }, "session-a", resourceFacade())).toThrow(/owner does not match/);
    expect(() => parseSessionDraftV1({
      sessionId: "session-a",
      text: "draft",
      resources: [{
        availability: "available",
        resource_id: 7,
        original_name: "diagram.png",
        size: 42,
        entity_relative_path: "resources/private.bin",
      }],
    }, "session-a", resourceFacade())).toThrow(/unknown field/);
  });
});
