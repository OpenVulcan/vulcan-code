import { beforeEach, describe, expect, it, vi } from "vitest";
import { mediaDeliverySettingsFixture } from "@/test-fixtures/media-delivery";
import { resetPublicAiMediaDeliveryPolicy, savePublicAiMediaDeliveryPolicy } from "./settings-actions-service";

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
