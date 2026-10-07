import { beforeEach, expect, it, vi } from "vitest";
import prewarmWire from "./__fixtures__/plugin-prewarm-policy-wire.json";
import runtimeWire from "./__fixtures__/plugin-runtime-wire.json";
import { continuePluginPendingOperation, parsePluginPendingOperations, readPluginPendingOperations } from "./plugin-pending-operations-service";
import { invalidatePersistentPluginUiModules } from "./plugin-ui-module-runtime";
import { hasServiceRuntime, serviceClient } from "./webui-service-runtime";

vi.mock("./webui-service-runtime", () => ({ hasServiceRuntime: vi.fn(), serviceClient: { request: vi.fn() } }));
vi.mock("./plugin-ui-module-runtime", () => ({ invalidatePersistentPluginUiModules: vi.fn() }));

// update follows LocalPluginUpdateStatus serialization: camel-case outer fields and the store's snake-case candidate.
// update 遵循 LocalPluginUpdateStatus 序列化：外层驼峰字段及存储下划线候选字段。
const update = {
  requestId: "12345678-1234-4234-8234-123456789abc", pluginId: "fixture", phase: "committing", hasServiceOwner: false,
  candidate: { plugin_id: "fixture", generation_id: "12345678-1234-4234-8234-123456789abd", version: "2.0.0", content_fingerprint: "inspected" },
  activationRevision: "7", installation: "succeeded", effects: [],
};
// uninstall is a retained removal whose plugin may already be gone from the catalog.
// uninstall 是保留移除，其插件可能已不在目录中。
const uninstall = { requestId: "12345678-1234-4234-8234-123456789abe", pluginId: "hidden-plugin", phase: "collection",
  requiresExternalEffectsAcknowledgement: true };
const capacity = runtimeWire.runtime.policyUpdate;
const prewarm = prewarmWire.published.update;
// listed is one registry response carrying every kind with the identities the kernel derives for it.
// listed 是携带全部类别的单次登记表响应，每类身份均按内核推导。
const listed = [
  { kind: "update", operationId: update.requestId, pluginId: "fixture", detail: update,
    actions: [{ action: "abort", requiresAcknowledgement: true }, { action: "resume", requiresAcknowledgement: false }] },
  { kind: "uninstall", operationId: uninstall.requestId, pluginId: uninstall.pluginId, detail: uninstall,
    actions: [{ action: "resume", requiresAcknowledgement: true }] },
  { kind: "capacityPolicy", operationId: capacity.request.updateId, pluginId: capacity.request.pluginId, detail: capacity,
    actions: [{ action: "resume", requiresAcknowledgement: false }] },
  { kind: "prewarmPolicy", operationId: prewarm.request.updateId, pluginId: null, detail: prewarm,
    actions: [{ action: "forget", requiresAcknowledgement: false }] },
];
beforeEach(() => { vi.resetAllMocks(); vi.mocked(hasServiceRuntime).mockReturnValue(true); });

// One read lists every kind through the single registry method, in service order.
// 单次读取通过唯一登记表方法按服务顺序列出全部类别。
it("reads every kind from the single registry method", async () => {
  vi.mocked(serviceClient.request).mockResolvedValueOnce(listed);
  const operations = await readPluginPendingOperations();
  expect(serviceClient.request).toHaveBeenCalledExactlyOnceWith("service_plugin_pending_operations", {});
  expect(operations.map((operation) => [operation.kind, operation.operationId, operation.pluginId])).toEqual(
    listed.map((operation) => [operation.kind, operation.operationId, operation.pluginId]));
  expect(operations.map((operation) => operation.actions)).toEqual(listed.map((operation) => operation.actions));
});

// Identity must agree with its detail, actions must belong to the kind, and rows must be unique.
// 身份必须与其详情一致，动作必须属于该类别，且各行必须唯一。
it.each([
  { ...listed[0], operationId: uninstall.requestId },
  { ...listed[0], pluginId: "other" },
  { ...listed[1], actions: [{ action: "abort", requiresAcknowledgement: true }] },
  { ...listed[1], actions: [{ action: "resume" }] },
  { ...listed[2], pluginId: "other" },
  { ...listed[3], pluginId: "fixture" },
  { ...listed[3], actions: [{ action: "forget", requiresAcknowledgement: false }, { action: "forget", requiresAcknowledgement: false }] },
  { ...listed[3], kind: "credentials" },
])("rejects a row whose identity, kind or actions disagree %#", (row) => {
  expect(() => parsePluginPendingOperations([row])).toThrow();
});
it("rejects a repeated operation and a non-list response", () => {
  expect(() => parsePluginPendingOperations([listed[0], listed[0]])).toThrow("Duplicate");
  expect(() => parsePluginPendingOperations({})).toThrow();
});

// Continuations go out exactly once with the declared fields; a catalog is returned only when the service sent one.
// 继续请求按声明字段精确发送一次；仅当服务发送目录时才返回目录。
it("sends the exact continuation once and returns the published catalog", async () => {
  // catalog is the exact transport response rather than a synthesized success projection.
  // catalog 是精确传输响应，而非合成成功投影。
  const catalog = { revision: 3, plugins: [] };
  const continuation = { kind: "update", operationId: update.requestId, pluginId: "fixture", action: "abort", acknowledgeExternalEffects: true } as const;
  vi.mocked(serviceClient.request).mockResolvedValueOnce({ catalog });
  expect(await continuePluginPendingOperation(continuation)).toBe(catalog);
  expect(serviceClient.request).toHaveBeenCalledExactlyOnceWith("service_continue_plugin_pending_operation", continuation);
  vi.mocked(serviceClient.request).mockResolvedValueOnce({ catalog: null });
  expect(await continuePluginPendingOperation({ kind: "prewarmPolicy", operationId: prewarm.request.updateId, action: "forget" })).toBeNull();
  expect(invalidatePersistentPluginUiModules).not.toHaveBeenCalled();
});

// Only a successful removal retires the plugin's persistent UI modules; a lost response retains them.
// 仅成功移除才使插件持久 UI 模块退役；响应丢失时保留它们。
it("invalidates persistent modules only after a successful removal", async () => {
  const continuation = { kind: "uninstall", operationId: uninstall.requestId, pluginId: uninstall.pluginId, action: "resume", acknowledgeExternalEffects: true } as const;
  vi.mocked(serviceClient.request).mockRejectedValueOnce(new Error("response lost"));
  await expect(continuePluginPendingOperation(continuation)).rejects.toThrow("response lost");
  expect(invalidatePersistentPluginUiModules).not.toHaveBeenCalled();
  vi.mocked(serviceClient.request).mockResolvedValueOnce({ catalog: { revision: 4, plugins: [] } });
  await continuePluginPendingOperation(continuation);
  expect(invalidatePersistentPluginUiModules).toHaveBeenCalledExactlyOnceWith(uninstall.pluginId);
  expect(serviceClient.request).toHaveBeenCalledTimes(2);
});

// Malformed acknowledgements fail instead of being treated as success.
// 畸形确认结果失败，而不被视为成功。
it.each([null, {}, [], { catalog: [] }, { catalog: "yes" }])("rejects a malformed continuation response %#", async (response) => {
  vi.mocked(serviceClient.request).mockResolvedValueOnce(response);
  await expect(continuePluginPendingOperation({ kind: "capacityPolicy", operationId: capacity.request.updateId, action: "resume" })).rejects.toThrow();
});

// Preview mode refuses every registry access rather than synthesizing pending state.
// 预览模式拒绝所有登记表访问，而不合成待处理状态。
it("requires the real service for reads and continuations", async () => {
  vi.mocked(hasServiceRuntime).mockReturnValue(false);
  await expect(readPluginPendingOperations()).rejects.toThrow("require");
  await expect(continuePluginPendingOperation({ kind: "prewarmPolicy", operationId: prewarm.request.updateId, action: "forget" })).rejects.toThrow("require");
  expect(serviceClient.request).not.toHaveBeenCalled();
});
