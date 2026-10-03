import { hasServiceRuntime, serviceClient } from "./webui-service-runtime";
import { parsePluginRuntime } from "./plugin-runtime-contract";
import { parsePluginResourceDiagnostics, parsePluginUninstallDrainDiagnostics, parsePluginUpdateDrainDiagnostics } from "./plugin-resource-diagnostics-contract";
import type { PluginDiagnosticsSelection, PluginResourceDiagnostics, PluginUninstallDrainDiagnostics, PluginUpdateDrainDiagnostics } from "@/types/plugin-resource-diagnostics";

// PluginDiagnosticsObservation keeps nullable live resources and the exact original request wrapper separate.
// PluginDiagnosticsObservation 将可空实时资源与精确原请求包装分开保留。
export interface PluginDiagnosticsObservation {
  // resources is null when the original attached parent or drain handle is unavailable.
  // resources 在原附加父级或排空句柄不可用时为空。
  resources: PluginResourceDiagnostics | null;
  // update preserves the observed predecessor identity independently from the current catalog.
  // update 独立于当前目录保留观测前驱身份。
  update: PluginUpdateDrainDiagnostics | null;
  // uninstall preserves the original reservation identity.
  // uninstall 保留原预留身份。
  uninstall: PluginUninstallDrainDiagnostics | null;
}
// readPluginResourceDiagnostics reads selection through its sole declared RPC and validates exact parent and owner.
// readPluginResourceDiagnostics 通过唯一声明 RPC 读取 selection，校验精确父级及所有者。
// Returns metadata only; unattached parents and unavailable original drains remain explicitly absent.
// 仅返回元数据；未附加父级及不可用原排空保持明确缺失。
export async function readPluginResourceDiagnostics(selection: PluginDiagnosticsSelection): Promise<PluginDiagnosticsObservation> {
  if (!hasServiceRuntime()) throw new Error("Plugin diagnostics require an attached Core service");
  // runtime is read first so the subsequent strict request fences the same attached native parent.
  // 首先读取 runtime，使后续严格请求约束同一附加原生父级。
  const runtime = parsePluginRuntime(await serviceClient.request<unknown>("service_plugin_runtime_status", {}));
  if (runtime === null) return { resources: null, update: null, uninstall: null };
  // request carries only the exact protocol fields, without aliases or inferred generations.
  // request 仅携带精确协议字段，没有别名或推断代次。
  const request = { runtimeId: runtime.runtimeId, pluginId: selection.pluginId };
  // observation retains request metadata even when live resources are absent.
  // observation 即使实时资源缺失也保留请求元数据。
  let observation: PluginDiagnosticsObservation;
  if (selection.kind === "plugin") {
    observation = { resources: parsePluginResourceDiagnostics(await serviceClient.request<unknown>("service_plugin_resource_diagnostics", request)), update: null, uninstall: null };
  } else if (selection.kind === "update") {
    // update is null only when the original live predecessor handle is unavailable.
    // update 仅在原实时前驱句柄不可用时为空。
    const update = parsePluginUpdateDrainDiagnostics(await serviceClient.request<unknown>("service_plugin_update_drain_diagnostics", { ...request, requestId: selection.requestId }));
    if (update !== null && (update.pluginId !== selection.pluginId || update.requestId !== selection.requestId)) throw new Error("diagnostics: original update identity mismatch");
    observation = { resources: update === null ? null : update.resources, update, uninstall: null };
  } else {
    // uninstall must remain the actual service-owned original request.
    // uninstall 必须保持实际服务拥有的原请求。
    const uninstall = parsePluginUninstallDrainDiagnostics(await serviceClient.request<unknown>("service_plugin_uninstall_drain_diagnostics", { ...request, requestId: selection.requestId }));
    if (uninstall.pluginId !== selection.pluginId || uninstall.requestId !== selection.requestId) throw new Error("diagnostics: original uninstall identity mismatch");
    observation = { resources: uninstall.resources, update: null, uninstall };
  }
  if (observation.resources !== null && (observation.resources.runtimeId !== runtime.runtimeId || observation.resources.pluginId !== selection.pluginId)) {
    throw new Error("diagnostics: plugin or native parent changed; refresh explicitly");
  }
  if (selection.kind === "plugin" && observation.resources !== null && observation.resources.pluginGeneration !== null) throw new Error("diagnostics: unexpected general generation filter");
  return observation;
}
