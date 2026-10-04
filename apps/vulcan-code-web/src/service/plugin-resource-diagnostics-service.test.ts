import { beforeEach, expect, it, vi } from "vitest";
import { readPluginResourceDiagnostics } from "./plugin-resource-diagnostics-service";
import { parsePluginResourceDiagnostics, parsePluginUpdateDrainDiagnostics } from "./plugin-resource-diagnostics-contract";
import { hasServiceRuntime, serviceClient } from "./webui-service-runtime";
import { diagnosticReport, diagnosticResource } from "./__fixtures__/plugin-resource-diagnostics";

vi.mock("./webui-service-runtime", () => ({ hasServiceRuntime: vi.fn(), serviceClient: { request: vi.fn() } }));
// runtime carries only the existing formal parent status contract.
// runtime 仅携带既有正式父级状态契约。
const runtime = { runtimeId: diagnosticReport.runtimeId, policyUpdate: null, resources: { resident: 0, creating: 0, idle: 0, running: 0, retiring: 0 } };
// update preserves catalog transition generations distinct from old physical resource generations.
// update 保留与旧物理资源代次区分的目录转换代次。
const update = { requestId: "request-original", pluginId: "fixture", serviceEpoch: diagnosticReport.serviceEpoch,
  predecessorGeneration: "9007199254740993", drainingGeneration: "9007199254740994", resources: { ...diagnosticReport, resources: [diagnosticResource] } };
beforeEach(() => { vi.clearAllMocks(); vi.mocked(hasServiceRuntime).mockReturnValue(true); });

it("reads the exact parent and plugin without creating a runtime or leaking inputs", async () => {
  vi.mocked(serviceClient.request).mockResolvedValueOnce(runtime).mockResolvedValueOnce(diagnosticReport);
  expect((await readPluginResourceDiagnostics({ kind: "plugin", pluginId: "fixture" })).resources?.pinnedSessionCount).toBe(1);
  expect(serviceClient.request).toHaveBeenNthCalledWith(1, "service_plugin_runtime_status", {});
  expect(serviceClient.request).toHaveBeenLastCalledWith("service_plugin_resource_diagnostics", { runtimeId: "parent-original", pluginId: "fixture" });
  vi.mocked(hasServiceRuntime).mockReturnValue(false);
  await expect(readPluginResourceDiagnostics({ kind: "plugin", pluginId: "fixture" })).rejects.toThrow("attached");
  expect(serviceClient.request).toHaveBeenCalledTimes(2);
});

it("keeps original update identity and treats missing live drain as unavailable", async () => {
  vi.mocked(serviceClient.request).mockResolvedValueOnce(runtime).mockResolvedValueOnce(update);
  expect((await readPluginResourceDiagnostics({ kind: "update", pluginId: "fixture", requestId: "request-original" })).update).toEqual(update);
  expect(serviceClient.request).toHaveBeenLastCalledWith("service_plugin_update_drain_diagnostics", { runtimeId: "parent-original", pluginId: "fixture", requestId: "request-original" });
  vi.mocked(serviceClient.request).mockResolvedValueOnce(runtime).mockResolvedValueOnce(null);
  expect(await readPluginResourceDiagnostics({ kind: "update", pluginId: "fixture", requestId: "request-original" })).toEqual({ resources: null, update: null, uninstall: null });
});

it("preserves original uninstall generation and reports owner absence without substituting a same-name plugin", async () => {
  // uninstall retains its reservation rather than any subsequently installed descriptor.
  // uninstall 保留其预留，而非任何后续安装的描述符。
  const uninstall = { requestId: "uninstall-original", pluginId: "fixture", serviceEpoch: diagnosticReport.serviceEpoch,
    pluginGeneration: diagnosticResource.pluginGeneration, resources: null };
  vi.mocked(serviceClient.request).mockResolvedValueOnce(runtime).mockResolvedValueOnce(uninstall);
  expect((await readPluginResourceDiagnostics({ kind: "uninstall", pluginId: "fixture", requestId: uninstall.requestId })).uninstall).toEqual(uninstall);
  expect(serviceClient.request).toHaveBeenLastCalledWith("service_plugin_uninstall_drain_diagnostics", { runtimeId: "parent-original", pluginId: "fixture", requestId: uninstall.requestId });
  vi.mocked(serviceClient.request).mockResolvedValueOnce(runtime).mockRejectedValueOnce(new Error("Original uninstall owner unavailable"));
  await expect(readPluginResourceDiagnostics({ kind: "uninstall", pluginId: "fixture", requestId: uninstall.requestId })).rejects.toThrow("unavailable");
  expect(serviceClient.request).toHaveBeenCalledTimes(4);
});

it("rejects changed parent, owner and original request identities", async () => {
  for (const changed of [{ ...diagnosticReport, runtimeId: "replacement-parent" }, { ...diagnosticReport, pluginId: "other" }]) {
    vi.mocked(serviceClient.request).mockResolvedValueOnce(runtime).mockResolvedValueOnce(changed);
    await expect(readPluginResourceDiagnostics({ kind: "plugin", pluginId: "fixture" })).rejects.toThrow("changed");
  }
  vi.mocked(serviceClient.request).mockResolvedValueOnce(runtime).mockResolvedValueOnce({ ...update, requestId: "replacement-request" });
  await expect(readPluginResourceDiagnostics({ kind: "update", pluginId: "fixture", requestId: update.requestId })).rejects.toThrow("identity mismatch");
});

it("keeps exact u64 strings and terminal records distinct from live blockers", () => {
  expect(parsePluginResourceDiagnostics(diagnosticReport)).toEqual(diagnosticReport);
  expect(parsePluginUpdateDrainDiagnostics({ ...update, resources: { ...update.resources, resources: [diagnosticResource, { ...diagnosticResource, resourceId: "earlier", pluginGeneration: "1" }] } })?.resources?.resources).toHaveLength(2);
  expect(() => parsePluginUpdateDrainDiagnostics({ ...update, resources: diagnosticReport })).toThrow("drain selection");
});

it("rejects missing nullable fields, unknown phases and business-bearing payloads", () => {
  for (const changed of [{ ...diagnosticResource, ownerSessionId: undefined }, { ...diagnosticResource, corePhase: "invented" },
    { ...diagnosticResource, result: "secret" }, { ...diagnosticResource, errors: [{ stage: "cleanup", code: "busy", message: "secret" }] },
    { ...diagnosticResource, pluginGeneration: 9007199254740993 }]) {
    expect(() => parsePluginResourceDiagnostics({ ...diagnosticReport, resources: [changed] })).toThrow();
  }
});
