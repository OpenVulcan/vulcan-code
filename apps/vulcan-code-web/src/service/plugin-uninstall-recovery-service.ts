import type { LuaSkillsCatalogUpdate } from "@/types";
import { invalidatePersistentPluginUiModules } from "./plugin-ui-module-runtime";
import { hasServiceRuntime, serviceClient } from "./webui-service-runtime";

// The service kernel owns these exact uninstall phases; synchronize when its wire contract changes.
// 服务内核持有这些精确卸载阶段；线协议变化时同步。
export type PluginUninstallPhase = "executing" | "reserved" | "lifecycle" | "publishing" | "collection";

// A retained uninstall remains observable after its plugin disappears from the published catalog.
// 插件从已发布目录消失后，保留卸载仍可观测。
export interface PluginUninstallStatus {
  // requestId identifies the actual original reservation, never a reconstructed request.
  // requestId 标识实际原预留，绝不表示重建请求。
  requestId: string;
  // pluginId is the original namespace, independent of current catalog membership.
  // pluginId 是原命名空间，独立于当前目录成员关系。
  pluginId: string;
  // phase reports the current exclusive borrow or retained continuation stage.
  // phase 报告当前排他借用或保留继续阶段。
  phase: PluginUninstallPhase;
  // Only this explicit service fact permits showing the external effects acknowledgement.
  // 仅此明确服务事实允许显示外部副作用确认。
  requiresExternalEffectsAcknowledgement: boolean;
}

// Parse the declared all-request response; unknown fields never substitute for required identities.
// 解析声明的全量请求响应；未知字段绝不替代必需身份。
// value is untrusted transport JSON; returns validated originals or throws an explicit protocol error.
// value 是不可信传输 JSON；返回已校验原实例，或抛出明确协议错误。
export function parsePluginUninstallRequests(value: unknown): PluginUninstallStatus[] {
  if (!Array.isArray(value)) throw new Error("Invalid plugin uninstallation request response.");
  // identities reject ambiguous repeated originals in the same observation.
  // identities 拒绝同一观测中含糊的重复原实例。
  const identities = new Set<string>();
  return value.map((record: unknown) => {
    if (typeof record !== "object" || record === null || Array.isArray(record)) {
      throw new Error("Invalid plugin uninstallation request record.");
    }
    // fields reads the declared transport names without searching aliases.
    // fields 读取声明传输名称，不搜索别名。
    const fields = record as Record<string, unknown>;
    if (typeof fields.requestId !== "string" || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(fields.requestId)
      || typeof fields.pluginId !== "string" || fields.pluginId.trim().length === 0
      || (fields.phase !== "executing" && fields.phase !== "reserved" && fields.phase !== "lifecycle"
        && fields.phase !== "publishing" && fields.phase !== "collection")
      || typeof fields.requiresExternalEffectsAcknowledgement !== "boolean" || identities.has(fields.requestId)) {
      throw new Error("Invalid plugin uninstallation request identity or phase.");
    }
    identities.add(fields.requestId);
    return { requestId: fields.requestId, pluginId: fields.pluginId, phase: fields.phase,
      requiresExternalEffectsAcknowledgement: fields.requiresExternalEffectsAcknowledgement };
  });
}

// Read every actual service-owned uninstall without requiring a published plugin row.
// 无需已发布插件行即可读取所有实际服务持有卸载。
// No parameters; returns sanitized live originals and performs no mutation.
// 无参数；返回脱敏实时原实例，不执行变更。
export async function readPluginUninstallRequests(): Promise<PluginUninstallStatus[]> {
  if (!hasServiceRuntime()) throw new Error("Plugin uninstallation recovery requires the Vulcan service runtime.");
  return parsePluginUninstallRequests(await serviceClient.request<unknown>("service_local_plugin_uninstall_requests", {}));
}

// Continue only pluginId's original requestId; acknowledgement never bypasses actual resource retirement.
// 仅继续 pluginId 的原 requestId；确认绝不绕过实际资源退役。
// Returns the committed catalog after success and invalidates this plugin's persistent UI modules.
// 成功后返回已提交目录，并使此插件持久 UI 模块失效。
export async function resumePluginUninstallRequest(pluginId: string, requestId: string, acknowledgeExternalEffects = false): Promise<LuaSkillsCatalogUpdate> {
  if (!hasServiceRuntime()) throw new Error("Plugin uninstallation recovery requires the Vulcan service runtime.");
  // update is authoritative only when the original continuation RPC succeeds.
  // update 仅在原继续 RPC 成功时具有权威。
  const update = await serviceClient.request<LuaSkillsCatalogUpdate>("service_resume_local_plugin_uninstall", { pluginId, requestId, acknowledgeExternalEffects });
  invalidatePersistentPluginUiModules(pluginId);
  return update;
}
