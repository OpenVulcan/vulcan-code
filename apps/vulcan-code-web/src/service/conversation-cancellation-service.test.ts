import { beforeEach, describe, expect, it, vi } from "vitest";
import { cancelConversationSubmission, discardComposerAttachmentPlans, planComposerAttachments, reconcileComposerAttachmentDraftLease } from "./conversation-actions-service";

// runtime controls transport availability without reading discovery files or connecting to a live service.
// runtime 控制传输可用性，不读取发现文件，也不连接真实服务。
const runtime = vi.hoisted(() => ({ available: true, request: vi.fn() }));
vi.mock("@/service/webui-service-runtime", () => ({
  hasServiceRuntime: () => runtime.available,
  serviceClient: { request: runtime.request },
}));

beforeEach(() => {
  runtime.available = true;
  runtime.request.mockReset();
});

describe("cancellation settlement service boundary", () => {
  // Discarding a stale view uses a separate RPC and cannot implicitly cancel the original provider operation.
  // 丢弃陈旧视图使用独立RPC，不能隐式取消原供应商操作。
  it("discards view plans without sending a cancellation request", async () => {
    runtime.request.mockResolvedValue(1);
    await expect(discardComposerAttachmentPlans(["view-a"])).resolves.toBe(1);
    expect(runtime.request).toHaveBeenCalledExactlyOnceWith("service_discard_composer_attachment_plans", { transferIds: ["view-a"] });
  });

  // Only the explicit retry caller sets retryUploads; ordinary route planning must never infer it.
  // 只有显式重试调用方设置 retryUploads；普通路线规划绝不推断此权限。
  it("distinguishes automatic planning from explicit upload retry", async () => {
    // selection uses one exact fixture route; no real configuration is read or changed.
    // selection 使用一条精确夹具路线；不读取或修改真实配置。
    const selection = { providerId: "provider-a", modelId: "model-a", connectionId: "connection-a" };
    runtime.request.mockResolvedValue([]);
    await planComposerAttachments("project-a", null, "draft-a", 1, selection, [], true);
    await planComposerAttachments("project-a", null, "draft-a", 2, selection, [], false, true);
    expect(runtime.request.mock.calls.map(([, params]) => params.retryUploads)).toEqual([false, true]);
    // The compression choice is always explicit so the service never guesses the viewer preference.
    // 压缩选择始终显式传递，服务绝不猜测查看者偏好。
    expect(runtime.request.mock.calls.map(([, params]) => params.compressImages)).toEqual([true, false]);
  });
  // Idempotent lease responses preserve the backend's changed flag without inventing a failure.
  // 幂等租约响应保留后端的变化标志，不虚构失败。
  it("accepts an unchanged lease only from an actual service response", async () => {
    runtime.request.mockResolvedValue(false);
    await expect(reconcileComposerAttachmentDraftLease("project-a", null, "draft-a", [])).resolves.toBe(false);
    runtime.available = false;
    await expect(reconcileComposerAttachmentDraftLease("project-a", null, "draft-a", [])).rejects.toThrow("available service runtime");
    expect(runtime.request).toHaveBeenCalledTimes(1);
  });
  // Only an actual RPC may establish that no operation remains registered.
  // 只有实际 RPC 才能确认已无操作登记。
  it("preserves a backend absence acknowledgement", async () => {
    runtime.request.mockResolvedValue(false);
    await expect(cancelConversationSubmission("transfer-a")).resolves.toBe(false);
    expect(runtime.request).toHaveBeenCalledExactlyOnceWith("service_cancel_conversation_submission", {
      clientSubmissionId: "transfer-a",
    });
  });

  // Preview mode is not evidence that a previous provider operation has stopped.
  // 预览模式不能证明此前供应商操作已经停止。
  it("rejects unavailable transport instead of inventing an absence acknowledgement", async () => {
    runtime.available = false;
    await expect(cancelConversationSubmission("transfer-a")).rejects.toThrow("available service runtime");
    expect(runtime.request).not.toHaveBeenCalled();
  });

  // Transport rejection is propagated unchanged so callers keep their source lease.
  // 传输拒绝原样传播，让调用方保留来源租约。
  it("preserves a lost cancellation acknowledgement", async () => {
    // failure is the exact transport error, not a synthesized provider terminal status.
    // failure 是精确传输错误，不是合成的供应商终态。
    const failure = new Error("acknowledgement lost");
    runtime.request.mockRejectedValue(failure);
    await expect(cancelConversationSubmission("transfer-a")).rejects.toBe(failure);
  });
});
