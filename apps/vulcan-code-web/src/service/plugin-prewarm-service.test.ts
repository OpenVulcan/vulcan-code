import { beforeEach, expect, it, vi } from "vitest";
import wire from "./__fixtures__/plugin-prewarm-wire.json";
import { executePluginPrewarmCommand, readPluginPrewarmCatalog, readPluginPrewarmReceipt, type PluginPrewarmCommand } from "./plugin-prewarm-service";
import { parsePluginPrewarmReceipt } from "./plugin-prewarm-contract";
import { hasServiceRuntime, serviceClient } from "./webui-service-runtime";

vi.mock("./webui-service-runtime", () => ({ hasServiceRuntime: vi.fn(), serviceClient: { request: vi.fn() } }));
// Original wire evidence supplies identities without granting workspace paths client authority.
// 原线协议证据提供身份，但不将工作区路径作为客户端权威。
const receipt = parsePluginPrewarmReceipt(wire.prepared);
const owner = { runtimeId: receipt.runtimeId, ownerSessionId: receipt.ownerSessionId };
const identity = { ...owner, invocationId: receipt.invocationId };
const preparation = { ...owner, pluginId: receipt.pluginId, toolId: receipt.toolId,
  pluginGeneration: receipt.pluginGeneration, targetIdle: receipt.targetIdle };
beforeEach(() => { vi.resetAllMocks(); vi.mocked(hasServiceRuntime).mockReturnValue(true); });

// Seven declared methods share exact identity verification and never transmit display-only fields.
// 七个声明方法共用精确身份校验，绝不传输仅供显示的字段。
it("dispatches exact read and management shapes, including explicit null forget", async () => {
  vi.mocked(serviceClient.request).mockResolvedValueOnce(wire.catalog);
  await readPluginPrewarmCatalog(receipt);
  expect(serviceClient.request).toHaveBeenLastCalledWith("service_plugin_prewarm_targets", owner);
  vi.mocked(serviceClient.request).mockResolvedValueOnce(wire.prepared);
  await readPluginPrewarmReceipt(receipt);
  expect(serviceClient.request).toHaveBeenLastCalledWith("service_plugin_prewarm_status", identity);
  vi.mocked(serviceClient.request).mockResolvedValueOnce(wire.prepared);
  await executePluginPrewarmCommand({ kind: "prepare", request: receipt });
  expect(serviceClient.request).toHaveBeenLastCalledWith("service_prepare_plugin_prewarm", { request: preparation });
  const methods = { start: "service_start_plugin_prewarm", cancel: "service_cancel_plugin_prewarm",
    recover: "service_recover_plugin_prewarm_cleanup", forget: "service_forget_plugin_prewarm" } as const;
  for (const kind of ["start", "cancel", "recover", "forget"] as const) {
    vi.mocked(serviceClient.request).mockResolvedValueOnce(kind === "forget" ? null : wire.completed);
    await executePluginPrewarmCommand({ kind, request: receipt });
    expect(serviceClient.request).toHaveBeenLastCalledWith(methods[kind], identity);
  }
  expect(serviceClient.request).toHaveBeenCalledTimes(7);
});

// A lost or mismatched response cannot be treated as confirmation and must not trigger a service retry.
// 丢失或身份不符的响应不能视为确认，也不能触发服务层重试。
it("rejects mismatched acknowledgements without retrying or inferring cancellation", async () => {
  const command: PluginPrewarmCommand = { kind: "prepare", request: preparation };
  vi.mocked(serviceClient.request).mockRejectedValueOnce(new Error("lost response"));
  await expect(executePluginPrewarmCommand(command)).rejects.toThrow("lost response");
  expect(serviceClient.request).toHaveBeenCalledTimes(1);
  for (const mutation of [{ runtimeId: "other" }, { ownerSessionId: "00000000-0000-4000-8000-000000000002" },
    { pluginGeneration: "18446744073709551615" }, { toolId: "other" }, { pluginId: "other" }, { targetIdle: 0 }]) {
    vi.mocked(serviceClient.request).mockResolvedValueOnce({ ...wire.prepared, ...mutation });
    await expect(executePluginPrewarmCommand(command)).rejects.toThrow("mismatch");
  }
  vi.mocked(serviceClient.request).mockResolvedValueOnce({ ...wire.prepared, invocationId: "other" });
  await expect(readPluginPrewarmReceipt(identity)).rejects.toThrow("invocation mismatch");
  vi.mocked(serviceClient.request).mockResolvedValueOnce(false);
  await expect(executePluginPrewarmCommand({ kind: "forget", request: identity })).rejects.toThrow("expected null");
  vi.mocked(serviceClient.request).mockResolvedValueOnce({ ...wire.catalog, runtimeId: "other" });
  await expect(readPluginPrewarmCatalog(owner)).rejects.toThrow("owner mismatch");
  expect(serviceClient.request).toHaveBeenCalledTimes(10);
});

// Preview mode must fail locally without creating a runtime or preparing work.
// 预览模式必须本地失败，不创建运行时或准备工作。
it("requires an attached service for both reads and writes", async () => {
  vi.mocked(hasServiceRuntime).mockReturnValue(false);
  await expect(readPluginPrewarmCatalog(owner)).rejects.toThrow("requires");
  await expect(readPluginPrewarmReceipt(identity)).rejects.toThrow("requires");
  await expect(executePluginPrewarmCommand({ kind: "prepare", request: preparation })).rejects.toThrow("requires");
  expect(serviceClient.request).not.toHaveBeenCalled();
});
