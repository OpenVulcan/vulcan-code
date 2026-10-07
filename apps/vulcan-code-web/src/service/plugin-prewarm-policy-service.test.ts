import { beforeEach, expect, it, vi } from "vitest";
import wire from "./__fixtures__/plugin-prewarm-policy-wire.json";
import { executePluginPrewarmPolicyCommand, readPluginPrewarmPolicyStatus } from "./plugin-prewarm-policy-service";
import { parsePluginPrewarmPolicyReceipt } from "./plugin-prewarm-policy-contract";
import { hasServiceRuntime, serviceClient } from "./webui-service-runtime";

vi.mock("./webui-service-runtime", () => ({ hasServiceRuntime: vi.fn(), serviceClient: { request: vi.fn() } }));
// The original real Rust response supplies immutable intent for acknowledgement verification.
// 原真实 Rust 响应为确认结果校验提供不可变意图。
const receipt = parsePluginPrewarmPolicyReceipt(wire.published.update);
const request = receipt.request;
beforeEach(() => { vi.resetAllMocks(); vi.mocked(hasServiceRuntime).mockReturnValue(true); });

// Every command uses one registered RPC and only its declared named wire fields.
// 每个命令使用单个注册 RPC，且只使用已声明具名线协议字段。
it("sends all five exact method shapes and strips display-only fields", async () => {
  vi.mocked(serviceClient.request).mockResolvedValueOnce(wire.published);
  expect(await readPluginPrewarmPolicyStatus()).toEqual(wire.published);
  expect(serviceClient.request).toHaveBeenLastCalledWith("service_plugin_prewarm_policy_status", {});
  // Rich objects may cross a component boundary, but extra fields have no request authority.
  // 丰富对象可跨组件边界，但额外字段不具有请求权威。
  const richRequest = { ...request, resultRevision: receipt.resultRevision, rules: request.rules.map((rule) => ({ ...rule, workspaceRoot: "not-authority" })) };
  vi.mocked(serviceClient.request).mockResolvedValueOnce(receipt);
  expect(await executePluginPrewarmPolicyCommand({ kind: "begin", request: richRequest })).toEqual(receipt);
  expect(serviceClient.request).toHaveBeenLastCalledWith("service_begin_plugin_prewarm_policy_update", { request });
  // Receipt continuations travel through the unified pending-operation entry with the original UUID only.
  // 回执继续仅携带原 UUID，经由统一待处理操作入口传输。
  for (const kind of ["resume", "cancel", "forget"] as const) {
    vi.mocked(serviceClient.request).mockResolvedValueOnce({ catalog: null });
    expect(await executePluginPrewarmPolicyCommand({ kind, request: richRequest })).toBeNull();
    expect(serviceClient.request).toHaveBeenLastCalledWith("service_continue_plugin_pending_operation",
      { kind: "prewarmPolicy", operationId: request.updateId, action: kind });
  }
  expect(serviceClient.request).toHaveBeenCalledTimes(5);
});

// Unknown outcomes and altered original intent remain failures, without any automatic retry or reset.
// 未知结果及被更改原意图保持失败，不作任何自动重试或重置。
it("rejects mismatched receipts and lost responses without replacing the original request", async () => {
  vi.mocked(serviceClient.request).mockRejectedValueOnce(new Error("lost response"));
  await expect(executePluginPrewarmPolicyCommand({ kind: "begin", request })).rejects.toThrow("lost response");
  expect(serviceClient.request).toHaveBeenCalledTimes(1);
  for (const changed of [{ updateId: "another" }, { runtimeId: "replacement" }, { expectedRevision: "newer" },
    { rules: [] }, { rules: request.rules.map((rule) => ({ ...rule, targetIdle: rule.targetIdle + 1 })) },
    { rules: request.rules.map((rule) => ({ ...rule, pluginId: "another" })) },
    { rules: request.rules.map((rule) => ({ ...rule, toolId: "another" })) }]) {
    vi.mocked(serviceClient.request).mockResolvedValueOnce({ ...receipt, request: { ...request, ...changed } });
    await expect(executePluginPrewarmPolicyCommand({ kind: "begin", request })).rejects.toThrow("request mismatch");
  }
  vi.mocked(serviceClient.request).mockResolvedValueOnce(false);
  await expect(executePluginPrewarmPolicyCommand({ kind: "forget", request })).rejects.toThrow("continuation response");
  expect(serviceClient.request).toHaveBeenCalledTimes(9);
});

// Preview-only pages cannot save policy or manufacture service observations.
// 仅预览页面不能保存策略或制造服务观测。
it("requires Core for reads and every mutation", async () => {
  vi.mocked(hasServiceRuntime).mockReturnValue(false);
  await expect(readPluginPrewarmPolicyStatus()).rejects.toThrow("requires");
  for (const kind of ["begin", "resume", "cancel", "forget"] as const) {
    await expect(executePluginPrewarmPolicyCommand({ kind, request })).rejects.toThrow("requires");
  }
  expect(serviceClient.request).not.toHaveBeenCalled();
});
