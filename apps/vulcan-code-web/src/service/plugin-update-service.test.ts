import { beforeEach, expect, it, vi } from "vitest";
import { abortLocalPluginUpdate, parseLocalPluginUpdateStatus, readLocalPluginUpdates, resumeLocalPluginUpdate } from "./plugin-update-service";
import { hasServiceRuntime, serviceClient } from "./webui-service-runtime";

vi.mock("./webui-service-runtime", () => ({ hasServiceRuntime: vi.fn(), serviceClient: { request: vi.fn() } }));

// status follows LocalPluginUpdateStatus serialization: camel-case outer fields and the store's snake-case candidate.
// status 遵循 LocalPluginUpdateStatus 序列化：外层驼峰字段及存储下划线候选字段。
const status = {
  requestId: "12345678-1234-4234-8234-123456789abc", pluginId: "fixture", phase: "published",
  candidate: { plugin_id: "fixture", generation_id: "12345678-1234-4234-8234-123456789abd", version: "2.0.0", content_fingerprint: "inspected-fingerprint" },
  activationRevision: "18446744073709551615", installation: "succeeded",
};
beforeEach(() => { vi.resetAllMocks(); vi.mocked(hasServiceRuntime).mockReturnValue(true); });

// Stable original identity passes through all service methods without allocating a new request or rounding a revision.
// 稳定原身份通过全部服务方法传递，不分配新请求或舍入修订。
it("uses exact original request parameters and preserves decimal revisions", async () => {
  vi.mocked(serviceClient.request).mockResolvedValueOnce([status]);
  expect((await readLocalPluginUpdates("fixture"))[0].activationRevision).toBe(status.activationRevision);
  expect(serviceClient.request).toHaveBeenLastCalledWith("service_local_plugin_update_requests", { pluginId: "fixture" });
  vi.mocked(serviceClient.request).mockResolvedValueOnce({});
  await resumeLocalPluginUpdate("fixture", status.requestId);
  expect(serviceClient.request).toHaveBeenLastCalledWith("service_resume_local_plugin_update", { pluginId: "fixture", requestId: status.requestId });
  vi.mocked(serviceClient.request).mockResolvedValueOnce({ ...status, phase: "aborted" });
  expect((await abortLocalPluginUpdate("fixture", status.requestId, true)).phase).toBe("aborted");
  expect(serviceClient.request).toHaveBeenLastCalledWith("service_abort_local_plugin_update", { pluginId: "fixture", requestId: status.requestId, acknowledgeExternalEffects: true });
});

// Unknown phases, mixed owners and malformed optional receipts fail closed without disclosing arbitrary response content.
// 未知阶段、混合所有者及损坏可选回执明确失败，不披露任意响应内容。
it("rejects malformed identity and phase instead of guessing aliases or defaults", () => {
  for (const change of [
    { pluginId: "other" }, { phase: "maybe_done" }, { installation: undefined },
    { requestId: status.requestId.toUpperCase() }, { activationRevision: 5 },
    { candidate: { ...status.candidate, plugin_id: "other" } },
  ]) expect(() => parseLocalPluginUpdateStatus({ ...status, ...change }, "fixture")).toThrow();
  expect(parseLocalPluginUpdateStatus({ ...status, configuration: "secret-document" }, "fixture")).not.toHaveProperty("configuration");
});

// Foreground failure never retries, and an acknowledgement for another request cannot become success.
// 前台失败绝不重试，另一请求的确认也不能变成成功。
it("retains errors and rejects a foreign acknowledgement without automatic replay", async () => {
  vi.mocked(serviceClient.request).mockRejectedValueOnce(new Error("response lost"));
  await expect(resumeLocalPluginUpdate("fixture", status.requestId)).rejects.toThrow("response lost");
  expect(serviceClient.request).toHaveBeenCalledTimes(1);
  vi.mocked(serviceClient.request).mockResolvedValueOnce({ ...status, requestId: status.candidate.generation_id });
  await expect(abortLocalPluginUpdate("fixture", status.requestId, true)).rejects.toThrow("another request");
  expect(serviceClient.request).toHaveBeenCalledTimes(2);
});

// Preview mode refuses every service operation rather than synthesizing update state.
// 预览模式拒绝每个服务操作，而不合成更新状态。
it("requires the real service for reads and mutations", async () => {
  vi.mocked(hasServiceRuntime).mockReturnValue(false);
  await expect(readLocalPluginUpdates("fixture")).rejects.toThrow("requires");
  await expect(resumeLocalPluginUpdate("fixture", status.requestId)).rejects.toThrow("requires");
  await expect(abortLocalPluginUpdate("fixture", status.requestId, false)).rejects.toThrow("requires");
  expect(serviceClient.request).not.toHaveBeenCalled();
});

// Duplicate rows and an unconfirmed abandonment cannot authorize another operation.
// 重复行及未确认放弃不能授权另一操作。
it("rejects duplicate requests and a nonterminal abandonment response", async () => {
  vi.mocked(serviceClient.request).mockResolvedValueOnce([status, status]);
  await expect(readLocalPluginUpdates("fixture")).rejects.toThrow("Duplicate");
  vi.mocked(serviceClient.request).mockResolvedValueOnce(status);
  await expect(abortLocalPluginUpdate("fixture", status.requestId, true)).rejects.toThrow("not confirmed");
});
