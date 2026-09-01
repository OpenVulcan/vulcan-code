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
  it("keeps original prompt positions when equal ResourceIds are reordered and resubmitted", async () => {
    // The two equal resources belong to different original input occurrences.
    // 两个相等资源属于不同原输入出现。
    const resource = { availability: "available", resource_id: 7, original_name: "image.png", mime: "image/png", size: 3 };
    // Their source anchors remain explicit even when the draft order differs from original positions.
    // 即使草稿顺序与原位置不同，其来源锚点仍保持显式。
    const sources = [{ client_action_id: "original-input", resource_position: 1 }, { client_action_id: "original-input", resource_position: 0 }];
    // The service fixture returns the exact current source ordering.
    // 服务夹具返回精确当前来源顺序。
    const facade = requestFacade({ sessionId: "session-a", text: "edited", resources: [resource, resource], stagedAttachmentIds: [null, null], resourceSources: sources });
    // Read and write share the same strict transport contract.
    // 读取和写入共享同一严格传输契约。
    const client = new ServiceSessionDraftClientV1(facade.service, resourceFacade());
    // A real client parse preserves both anchors instead of merging by ResourceId.
    // 真实客户端解析保留两个锚点，不按ResourceId合并。
    const draft = await client.read("session-a");
    expect(draft.attachments.map((attachment) => attachment.promptSource)).toEqual(sources);
    await client.write("session-a", "edited again", draft.attachments);
    expect(facade.calls[1]).toEqual({ method: "service_set_session_draft_v1", params: { sessionId: "session-a", text: "edited again", attachments: draft.attachments } });
  });

  it("rejects malformed source anchors and staged/source ownership conflicts", () => {
    // All malformed cases share otherwise valid project metadata.
    // 全部错误用例共享其他有效项目元数据。
    const response = { sessionId: "session-a", text: "draft", resources: [{ availability: "available", resource_id: 7, original_name: "image.png", mime: "image/png", size: 3 }], stagedAttachmentIds: [null] };
    for (const source of [{ client_action_id: " source ", resource_position: 0 }, { client_action_id: "中".repeat(86), resource_position: 0 }, { client_action_id: "source", resource_position: -1 }, { client_action_id: "source", resource_position: 0.5 }, { client_action_id: "source", resource_position: Number.MAX_SAFE_INTEGER + 1 }, { client_action_id: "source", resource_position: 0, model: "forged" }]) {
      expect(() => parseSessionDraftV1({ ...response, resourceSources: [source] }, "session-a", resourceFacade())).toThrow();
    }
    expect(() => parseSessionDraftV1({ ...response, resourceSources: [] }, "session-a", resourceFacade())).toThrow(/invalid resource sources/);
    expect(() => parseSessionDraftV1({ ...response, stagedAttachmentIds: ["asset"], resourceSources: [{ client_action_id: "source", resource_position: 0 }] }, "session-a", resourceFacade())).toThrow(/conflicting/);
  });

  it("restores exact staged occurrences even when their project ResourceId is shared", () => {
    // Identical resource bytes must not merge two explicit attachment intentions.
    // 相同资源字节不能合并两个显式附件意图。
    const resource = { availability: "available", resource_id: 7, original_name: "diagram.png", mime: "image/png", size: 42 };
    // The service mapping is explicit and chronological, independent of content identity.
    // 服务映射是显式且有时序的，独立于内容身份。
    const draft = parseSessionDraftV1({ sessionId: "session-a", text: "ordered", resources: [resource, resource, resource], stagedAttachmentIds: ["asset-first", null, "asset-second"], resourceSources: [null, null, null] }, "session-a", resourceFacade());
    expect(draft.attachments.map((attachment) => [attachment.assetId ?? null, attachment.projectResourceId ?? null])).toEqual([["asset-first", null], [null, 7], ["asset-second", null]]);
    expect(draft.attachments.every((attachment) => attachment.localPath === "resource://session-a/7")).toBe(true);
  });

  it("rejects incomplete or malformed staged identity mappings without guessing provenance", () => {
    // All cases isolate the explicit identity mapping from otherwise valid resource facts.
    // 全部用例将显式身份映射与其他有效资源事实隔离。
    const response = { sessionId: "session-a", text: "draft", resources: [{ availability: "available", resource_id: 7, original_name: "diagram.png", mime: "image/png", size: 42 }], resourceSources: [null] };
    for (const stagedAttachmentIds of [[], ["asset", "extra"], [""], [" asset "], ["attachment://asset"], [42], ["asset\ninvalid"]]) {
      expect(() => parseSessionDraftV1({ ...response, stagedAttachmentIds }, "session-a", resourceFacade())).toThrow(/invalid staged attachment/);
    }
    expect(() => parseSessionDraftV1({ sessionId: "session-a", text: "draft", resources: [{ availability: "missing", resource_id: 7, placeholder: "missing" }], stagedAttachmentIds: ["asset"], resourceSources: [null] }, "session-a", resourceFacade())).toThrow(/no authoritative resource metadata/);
  });

  it("rebuilds ordered available and missing attachments from ResourceIds", async () => {
    const facade = requestFacade({
      sessionId: "session-a",
      text: "draft text",
      stagedAttachmentIds: [null, null],
      resourceSources: [null, null],
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
      stagedAttachmentIds: [null],
      resourceSources: [null],
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
      stagedAttachmentIds: [],
      resourceSources: [],
      resources: [],
    }, "session-a", resourceFacade())).toThrow(/owner does not match/);
    expect(() => parseSessionDraftV1({
      sessionId: "session-a",
      text: "draft",
      stagedAttachmentIds: [null],
      resourceSources: [null],
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
