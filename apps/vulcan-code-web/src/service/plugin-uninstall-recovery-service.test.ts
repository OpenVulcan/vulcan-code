import { beforeEach, expect, it, vi } from "vitest";
import { parsePluginUninstallRequests, readPluginUninstallRequests, resumePluginUninstallRequest, type PluginUninstallStatus } from "./plugin-uninstall-recovery-service";
import { hasServiceRuntime, serviceClient } from "./webui-service-runtime";
import { invalidatePersistentPluginUiModules } from "./plugin-ui-module-runtime";

vi.mock("./webui-service-runtime", () => ({ hasServiceRuntime: vi.fn(), serviceClient: { request: vi.fn() } }));
vi.mock("./plugin-ui-module-runtime", () => ({ invalidatePersistentPluginUiModules: vi.fn() }));

// original is a retained request independent from any currently published plugin row.
// original 是保留请求，独立于任何当前已发布插件行。
const original: PluginUninstallStatus = { requestId: "12345678-1234-4234-8234-123456789abc", pluginId: "hidden-plugin",
  phase: "collection", requiresExternalEffectsAcknowledgement: false };
beforeEach(() => { vi.resetAllMocks(); vi.mocked(hasServiceRuntime).mockReturnValue(true); });

// The query uses the declared all-request endpoint without a plugin filter or alternate protocol.
// 查询使用声明全量请求端点，不带插件筛选或替代协议。
it("reads every retained original without a plugin row", async () => {
  vi.mocked(serviceClient.request).mockResolvedValue([original]);
  expect(await readPluginUninstallRequests()).toEqual([original]);
  expect(serviceClient.request).toHaveBeenCalledExactlyOnceWith("service_local_plugin_uninstall_requests", {});
});

// Every required field is strict; aliases, unknown phases and duplicate UUIDs cannot grant continuation.
// 每个必需字段均严格校验；别名、未知阶段及重复 UUID 不能授予继续。
it.each([null, {}, [{ ...original, requestId: "unknown" }], [{ ...original, phase: "committed" }],
  [{ ...original, requiresExternalEffectsAcknowledgement: undefined, acknowledge: true }], [original, original]])("rejects malformed original observations %#", (value) => {
  expect(() => parsePluginUninstallRequests(value)).toThrow("Invalid plugin uninstallation");
});

// Success uses the actual exact UUID response and invalidates this plugin's persistent modules once.
// 成功使用实际精确 UUID 响应，并一次使此插件持久模块失效。
it("continues the exact original with false acknowledgement by default and returns its catalog", async () => {
  // catalog is the exact transport response rather than a synthesized success projection.
  // catalog 是精确传输响应，而非合成成功投影。
  const catalog = { plugins: [] };
  vi.mocked(serviceClient.request).mockResolvedValue(catalog);
  expect(await resumePluginUninstallRequest(original.pluginId, original.requestId)).toBe(catalog);
  expect(serviceClient.request).toHaveBeenCalledExactlyOnceWith("service_resume_local_plugin_uninstall",
    { pluginId: original.pluginId, requestId: original.requestId, acknowledgeExternalEffects: false });
  expect(invalidatePersistentPluginUiModules).toHaveBeenCalledExactlyOnceWith(original.pluginId);
});

// A rejected foreground response retains module state and cannot masquerade as successful completion.
// 被拒前台响应保留模块状态，不能冒充成功完成。
it("does not invalidate modules after a failed continuation", async () => {
  vi.mocked(serviceClient.request).mockRejectedValue(new Error("response lost"));
  await expect(resumePluginUninstallRequest(original.pluginId, original.requestId, true)).rejects.toThrow("response lost");
  expect(invalidatePersistentPluginUiModules).not.toHaveBeenCalled();
});

// Unavailable transport fails explicitly instead of using a local simulated registry.
// 不可用传输明确失败，不使用本地模拟注册表。
it("rejects queries and continuation without the actual service", async () => {
  vi.mocked(hasServiceRuntime).mockReturnValue(false);
  await expect(readPluginUninstallRequests()).rejects.toThrow("requires the Vulcan service runtime");
  await expect(resumePluginUninstallRequest(original.pluginId, original.requestId)).rejects.toThrow("requires the Vulcan service runtime");
  expect(serviceClient.request).not.toHaveBeenCalled();
});
