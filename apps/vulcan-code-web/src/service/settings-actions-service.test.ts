import { beforeEach, describe, expect, it, vi } from "vitest";
import { mediaDeliverySettingsFixture } from "@/test-fixtures/media-delivery";
import type { PublicAiInferenceRoute, VceEmbeddingSaveConfirmation } from "@/types";
import {
  loadPublicAiSessionProviderToolSelections,
  rebuildPublicAiSessionProviderToolSelections,
  resetPublicAiMediaDeliveryPolicy,
  savePublicAiMediaDeliveryPolicy,
  savePublicAiSessionProviderToolSelections,
  savePublicAiInferenceRoute,
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

describe("VCE inference route transaction request", () => {
  // Each assertion observes one isolated production service action.
  // 每项断言观察一个隔离的生产服务动作。
  beforeEach(() => { transport.request.mockReset(); });

  // Preserve both wire naming conventions and the exact reviewed per-project decisions.
  // 保留两种线路命名约定及精确已审查逐项目决定。
  it("forwards explicit dimensions and confirmation with the unchanged route", async () => {
    // Account and action identifiers are authoritative, not derived from model display names.
    // 账号及动作标识是权威值，不由模型展示名称推导。
    const route: PublicAiInferenceRoute = {
      provider_id: "provider", service_entry_id: "entry", connection_id: "account-beta", model_id: "same-model-name",
      binding: { kind: "action", action_binding_id: "embedding-action" },
      operation: "embedding_create", usage_scope: "internal_background",
    };
    // Different project decisions must stay in the same optimistic save request.
    // 不同项目决定必须保留在同一个乐观保存请求中。
    const confirmation: VceEmbeddingSaveConfirmation = {
      preflightRevision: "reviewed-revision",
      decisions: [{ projectId: "project-a", decision: "compatible" }, { projectId: "project-b", decision: "rebuild" }],
    };
    // Identity proves the service returns the actual backend response, not a locally assembled projection.
    // 对象身份用于证明服务返回实际后端响应，而非本地组装的投影。
    const published = { inferenceProfiles: { vce: { embedding: route, embedding_dimensions: 2048 } } };
    transport.request.mockResolvedValue(published);
    expect(await savePublicAiInferenceRoute({ consumerProfileId: "vce", operation: "embedding_create", route,
      embeddingDimensions: 2048, vceEmbeddingConfirmation: confirmation })).toBe(published);
    expect(transport.request).toHaveBeenCalledExactlyOnceWith("service_save_public_ai_inference_route", {
      request: { consumerProfileId: "vce", operation: "embedding_create", route,
        embeddingDimensions: 2048, vceEmbeddingConfirmation: confirmation },
    });
  });

  // Clearing is an explicit null route; rejection cannot trigger a retry or default reconstruction.
  // 清空是明确的空路由；拒绝不能触发重试或默认值重建。
  it("preserves explicit removal and propagates its exact rejection once", async () => {
    // The backend owns this stale-review failure and its identity must survive the service adapter.
    // 后端持有此过期审查失败，其身份必须穿过服务适配器保持不变。
    const rejection = new Error("VCE preflight revision changed");
    transport.request.mockRejectedValue(rejection);
    await expect(savePublicAiInferenceRoute({ consumerProfileId: "vce", operation: "embedding_create", route: null }))
      .rejects.toBe(rejection);
    expect(transport.request).toHaveBeenCalledExactlyOnceWith("service_save_public_ai_inference_route", {
      request: { consumerProfileId: "vce", operation: "embedding_create", route: null },
    });
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
          mediaUnderstandingSelections: {},
        },
      },
    );
  });
});
