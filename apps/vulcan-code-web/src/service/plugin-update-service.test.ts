import { beforeEach, expect, it, vi } from "vitest";
import { abortLocalPluginUpdate, parseLocalPluginUpdateStatus, readLocalPluginUpdates, resumeLocalPluginUpdate } from "./plugin-update-service";
import { hasServiceRuntime, serviceClient } from "./webui-service-runtime";

vi.mock("./webui-service-runtime", () => ({ hasServiceRuntime: vi.fn(), serviceClient: { request: vi.fn() } }));

// status follows LocalPluginUpdateStatus serialization: camel-case outer fields and the store's snake-case candidate.
// status 遵循 LocalPluginUpdateStatus 序列化：外层驼峰字段及存储下划线候选字段。
const status = {
  requestId: "12345678-1234-4234-8234-123456789abc", pluginId: "fixture", phase: "published",
  candidate: { plugin_id: "fixture", generation_id: "12345678-1234-4234-8234-123456789abd", version: "2.0.0", content_fingerprint: "inspected-fingerprint" },
  activationRevision: "18446744073709551615", installation: "succeeded", effects: [], hasServiceOwner: false,
};
beforeEach(() => { vi.resetAllMocks(); vi.mocked(hasServiceRuntime).mockReturnValue(true); });

// All six durable phases remain distinct from service ownership; only an owned journal-less row may be null.
// 全部六种持久阶段保持独立于服务所有权；仅有所有者且无日志的行可以为空值。
it("preserves every durable phase and the exact boolean service owner observation", () => {
  for (const phase of ["preparing", "prepared", "committing", "published", "completed", "aborted"]) {
    for (const hasServiceOwner of [false, true]) {
      expect(parseLocalPluginUpdateStatus({ ...status, phase, installation: null, hasServiceOwner }, "fixture"))
        .toMatchObject({ phase, hasServiceOwner });
    }
  }
  expect(parseLocalPluginUpdateStatus({ ...status, phase: null, installation: null, hasServiceOwner: true }, "fixture"))
    .toMatchObject({ phase: null, installation: null, effects: [], hasServiceOwner: true });
});

// Missing ownership or phase and fabricated early receipts cannot become an inferred preparing state.
// 缺失所有权或阶段及虚构早期回执不能变成推断的准备中状态。
it("rejects missing phase, malformed ownership and journal-less receipt evidence", () => {
  // Delete each required field to verify actual absence separately from explicit null.
  // 删除每个必需字段，分别验证实际缺失与显式空值。
  const missingPhase: Record<string, unknown> = { ...status };
  delete missingPhase.phase;
  const missingOwner: Record<string, unknown> = { ...status };
  delete missingOwner.hasServiceOwner;
  for (const value of [missingPhase, missingOwner,
    { ...status, phase: undefined }, { ...status, phase: "reserving" },
    { ...status, hasServiceOwner: null }, { ...status, hasServiceOwner: "true" }, { ...status, hasServiceOwner: 1 },
    { ...status, phase: null, hasServiceOwner: false, installation: null },
    { ...status, phase: null, hasServiceOwner: true },
    { ...status, phase: null, hasServiceOwner: true, installation: null, effects: [{ operation: "probe", phase: "started" }] },
    { ...status, phase: null, hasServiceOwner: true, installation: null, effects: undefined },
  ]) expect(() => parseLocalPluginUpdateStatus(value, "fixture")).toThrow();
});

// Candidate observations are exact typed evidence; malformed or duplicated operations cannot authorize recovery.
// 候选观测为精确类型化证据；损坏或重复操作不能授权恢复。
it("preserves sanitized candidate observations and rejects malformed evidence", () => {
  // Extra callback content is excluded from the service projection.
  // 额外回调内容从服务投影中排除。
  const effect = { operation: "probe", phase: "started", callback: "private-result" };
  expect(parseLocalPluginUpdateStatus({ ...status, effects: [effect] }, "fixture").effects)
    .toEqual([{ operation: "probe", phase: "started" }]);
  expect(parseLocalPluginUpdateStatus({ ...status, effects: [{ operation: "drain_predecessor_lua", phase: "started" }] }, "fixture").effects)
    .toEqual([{ operation: "drain_predecessor_lua", phase: "started" }]);
  expect(parseLocalPluginUpdateStatus({ ...status, effects: [{ operation: "disable_predecessor", phase: "observation_failed" }] }, "fixture").effects)
    .toEqual([{ operation: "disable_predecessor", phase: "observation_failed" }]);
  expect(parseLocalPluginUpdateStatus({ ...status, effects: [{ operation: "restore_predecessor", phase: "started" }] }, "fixture").effects)
    .toEqual([{ operation: "restore_predecessor", phase: "started" }]);
  for (const effects of [undefined, null, {}, [null], [{ operation: "install", phase: "succeeded" }],
    [{ operation: "probe", phase: "unknown" }], [effect, effect]]) {
    expect(() => parseLocalPluginUpdateStatus({ ...status, effects }, "fixture")).toThrow();
  }
  expect(() => parseLocalPluginUpdateStatus({ ...status, phase: "preparing", installation: null, effects: [effect] }, "fixture")).toThrow();
});

// Early intent preserves the original identity but cannot claim an installation receipt before target readiness.
// 早期意图保留原身份，但不能在目标就绪之前声称具有安装回执。
it("accepts preparing identities and rejects premature installation evidence", () => {
  expect(parseLocalPluginUpdateStatus({ ...status, phase: "preparing", installation: null }, "fixture")).toMatchObject({
    requestId: status.requestId, phase: "preparing", activationRevision: status.activationRevision,
  });
  expect(() => parseLocalPluginUpdateStatus({ ...status, phase: "preparing" }, "fixture")).toThrow();
});

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
