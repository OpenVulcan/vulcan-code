import { describe, expect, it } from "vitest";
import type { ServiceClient } from "@/service/client";

import {
  ServiceSessionResourceClientV1,
  parseSessionResourceListV1Response,
} from "@/service/conversation-session-resources-v1";
import { resourceId } from "@/service/conversation-event-window-store";

/** Creates one request-only service facade for exact payload assertions.
 * 创建一个仅含请求能力的服务门面以进行精确载荷断言。 */
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

describe("ServiceSessionResourceClientV1", () => {
  it("preserves ordered repeated resource identities", async () => {
    const facade = requestFacade({
      sessionId: "session-a",
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
        {
          availability: "available",
          resource_id: 7,
          original_name: "diagram.png",
          mime: "image/png",
          size: 42,
        },
      ],
    });
    const client = new ServiceSessionResourceClientV1(facade.service);

    const response = await client.list("session-a", [resourceId(7), resourceId(9), resourceId(7)]);

    expect(response.resources.map((entry) => entry.resource_id)).toEqual([7, 9, 7]);
    expect(facade.calls).toEqual([{
      method: "service_session_resource_list_v1",
      params: {
        sessionId: "session-a",
        resourceIds: [7, 9, 7],
      },
    }]);
  });

  it("rejects identity drift instead of reordering a response", () => {
    expect(() => parseSessionResourceListV1Response({
      sessionId: "session-a",
      resources: [{
        availability: "missing",
        resource_id: 8,
        placeholder: "[MISSING PROJECT RESOURCE]",
      }],
    }, "session-a", [resourceId(7)])).toThrow(/out of request order/);
  });

  it("rejects physical locator fields returned by a drifting service", () => {
    expect(() => parseSessionResourceListV1Response({
      sessionId: "session-a",
      resources: [{
        availability: "available",
        resource_id: 7,
        original_name: "diagram.png",
        size: 42,
        entity_relative_path: "resources/private.bin",
      }],
    }, "session-a", [resourceId(7)])).toThrow(/unknown field/);
  });

  it("rejects missing, unsafe, and oversized identities", () => {
    expect(() => resourceId(0)).toThrow(/positive JavaScript-safe integer/);
    expect(() => resourceId(Number.MAX_SAFE_INTEGER + 1)).toThrow(/positive JavaScript-safe integer/);
    expect(() => parseSessionResourceListV1Response({
      sessionId: "session-a",
      resources: [{
        availability: "available",
        resource_id: 7,
        original_name: "diagram.png",
        size: Number.MAX_SAFE_INTEGER + 1,
      }],
    }, "session-a", [resourceId(7)])).toThrow(/non-negative safe integer/);
  });
});
