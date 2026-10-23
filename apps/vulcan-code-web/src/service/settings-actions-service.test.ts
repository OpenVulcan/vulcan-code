import { beforeEach, describe, expect, it, vi } from "vitest";
import { mediaDeliverySettingsFixture } from "@/test-fixtures/media-delivery";
import {
  loadPublicAiSessionProviderToolSelections,
  loadPublicAiSettings,
  rebuildPublicAiSessionProviderToolSelections,
  resetPublicAiMediaDeliveryPolicy,
  savePublicAiMediaDeliveryPolicy,
  savePublicAiSessionProviderToolSelections,
  savePublicAiConnectionEnabled,
} from "./settings-actions-service";

// The hoisted transport spy observes the real settings action without invoking a running service.
// 提升的传输间谍观察真实设置动作，不调用运行中的服务。
const transport = vi.hoisted(() => ({ request: vi.fn() }));

vi.mock("@/service/webui-service-runtime", () => ({
  hasServiceRuntime: () => true,
  serviceClient: transport,
}));
vi.mock("@/service/workbench-snapshot-service", () => ({
  loadWorkbenchSnapshot: vi.fn(),
}));

describe("public AI settings snapshot read", () => {
  beforeEach(() => {
    transport.request.mockReset();
  });

  // Readers mounting together share one request; a later read (for example after a save) is never served stale.
  // 同时挂载的读取方共用一次请求；之后的读取（例如保存后）绝不复用旧结果。
  it("shares a read only within the turn that started it", async () => {
    transport.request.mockResolvedValueOnce({ revision: "first" }).mockResolvedValueOnce({ revision: "second" });
    const first = loadPublicAiSettings();
    const second = loadPublicAiSettings();
    expect(await first).toBe(await second);
    expect(transport.request).toHaveBeenCalledTimes(1);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(await loadPublicAiSettings()).toEqual({ revision: "second" });
    expect(transport.request).toHaveBeenCalledTimes(2);
  });
});

describe("media delivery settings action", () => {
  // Do not return the mock function: Vitest treats a returned function as hook cleanup.
  // 不返回模拟函数：Vitest 会把返回的函数作为钩子清理函数执行。
  beforeEach(() => {
    transport.request.mockReset();
  });

  // Reset is an explicit backend command, not a copy of frontend hard-coded defaults.
  // 重置是显式后端命令，不是复制前端硬编码默认值。
  it("restores defaults only through the backend", async () => {
    // The returned token proves the action uses the backend response verbatim.
    // 返回标记证明动作原样使用后端响应。
    const published = { mediaDelivery: mediaDeliverySettingsFixture() };
    transport.request.mockResolvedValue(published);
    expect(await resetPublicAiMediaDeliveryPolicy("read-revision")).toBe(published);
    expect(transport.request).toHaveBeenCalledExactlyOnceWith(
      "service_reset_public_ai_media_delivery_policy", { request: { expected_revision: "read-revision" } },
    );
  });

  // All sixteen fields and explicit zeros must reach the backend without browser-side policy decisions.
  // 全部十六项字段及显式零值必须原样到达后端，不在浏览器作策略决策。
  it("sends one complete policy unchanged and returns the authoritative snapshot", async () => {
    // Different kind and purpose values expose accidental merging or normalization.
    // 不同类型与用途值能够暴露意外合并或规范化。
    const settings = mediaDeliverySettingsFixture();
    settings.policy.independent_extension.video.with_upload_max_bytes = 777;
    settings.policy.conversation.file.without_upload_max_bytes = 123;
    // This token stands for the exact backend snapshot, which the action must return verbatim.
    // 此标记代表精确后端快照，动作必须原样返回。
    const published = { mediaDelivery: settings };
    transport.request.mockResolvedValue(published);
    expect(await savePublicAiMediaDeliveryPolicy(settings.policy, "read-revision")).toBe(published);
    expect(transport.request).toHaveBeenCalledExactlyOnceWith(
      "service_save_public_ai_media_delivery_policy",
      { request: { policy: settings.policy, expected_revision: "read-revision" } },
    );
    expect(settings.policy.conversation.video.with_upload_max_bytes).toBe(0);
    expect(settings.policy.independent_extension.file.without_upload_max_bytes).toBe(0);
  });

  // Backend rejection must remain a failure rather than a synthesized local success snapshot.
  // 后端拒绝必须保持失败，不能合成客户端成功快照。
  it("propagates backend validation or publication failures", async () => {
    transport.request.mockRejectedValue(new Error("media policy publication failed"));
    await expect(savePublicAiMediaDeliveryPolicy(mediaDeliverySettingsFixture().policy, "read-revision"))
      .rejects.toThrow("media policy publication failed");
    expect(transport.request).toHaveBeenCalledTimes(1);
  });
});

describe("credential enabled-state action", () => {
  // Reset the shared transport before observing the exact credential mutation request.
  // 在观察精确凭据变更请求前重置共享传输。
  beforeEach(() => {
    transport.request.mockReset();
  });

  // The browser forwards only the stable connection identity and requested reversible state.
  // 浏览器仅转发稳定连接身份与请求的可逆状态。
  it("forwards the exact enabled-state request and returns the backend snapshot", async () => {
    // published represents the authoritative backend snapshot returned after activation.
    // published 表示激活后后端返回的权威快照。
    const published = { connections: [{ id: "account-a", enabled: false }] };
    transport.request.mockResolvedValue(published);

    expect(await savePublicAiConnectionEnabled({ connectionId: "account-a", enabled: false }))
      .toBe(published);
    expect(transport.request).toHaveBeenCalledExactlyOnceWith(
      "service_save_public_ai_connection_enabled",
      { request: { connectionId: "account-a", enabled: false } },
    );
  });
});

describe("session provider-tool settings response compatibility", () => {
  // Reset the shared transport before each response-boundary assertion.
  // 在每次响应边界断言前重置共享传输。
  beforeEach(() => {
    transport.request.mockReset();
  });

  // Every session-settings response path must upgrade the known pre-media-route payload.
  // 每条会话设置响应路径都必须升级已知的媒体路由字段引入前负载。
  it("normalizes a missing media route map after load, rebuild, and save", async () => {
    // legacySettings is the exact older response shape observed from a stale service process.
    // legacySettings 是从旧服务进程观察到的精确旧响应结构。
    const legacySettings = {
      enabledCapabilities: ["image_understand"],
      selections: {},
      extensionTargetSets: {},
      conflicts: [],
    };
    transport.request.mockResolvedValue(legacySettings);

    // loaded, rebuilt, and saved cover every public session-settings response adapter.
    // loaded、rebuilt 与 saved 覆盖全部公开会话设置响应适配器。
    const loaded = await loadPublicAiSessionProviderToolSelections("conversation-1");
    const rebuilt = await rebuildPublicAiSessionProviderToolSelections("conversation-1");
    const saved = await savePublicAiSessionProviderToolSelections({
      conversationId: "conversation-1",
      enabledCapabilities: ["image_understand"],
      selections: {},
      extensionTargetSets: {},
      extensionModes: {},
      mediaUnderstandingSelections: {},
    });

    expect(loaded.mediaUnderstandingSelections).toEqual({});
    expect(rebuilt.mediaUnderstandingSelections).toEqual({});
    expect(saved.mediaUnderstandingSelections).toEqual({});
    expect(transport.request).toHaveBeenNthCalledWith(
      1,
      "service_public_ai_session_provider_tool_selections",
      { conversationId: "conversation-1" },
    );
    expect(transport.request).toHaveBeenNthCalledWith(
      2,
      "service_rebuild_public_ai_session_provider_tool_selections",
      { conversationId: "conversation-1" },
    );
    expect(transport.request).toHaveBeenNthCalledWith(
      3,
      "service_save_public_ai_session_provider_tool_selections",
      {
        request: {
          conversationId: "conversation-1",
          enabledCapabilities: ["image_understand"],
          selections: {},
          extensionTargetSets: {},
          extensionModes: {},
          mediaUnderstandingSelections: {},
        },
      },
    );
  });
});
