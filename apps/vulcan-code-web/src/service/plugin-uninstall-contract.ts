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

// parsePluginUninstallStatus validates one retained original owned by pluginId; unknown fields never substitute for identities.
// parsePluginUninstallStatus 校验 pluginId 持有的单个保留原实例；未知字段绝不替代身份。
// value is untrusted transport JSON; returns the sanitized original or throws an explicit protocol error.
// value 是不可信传输 JSON；返回脱敏原实例，或抛出明确协议错误。
export function parsePluginUninstallStatus(value: unknown, pluginId: string): PluginUninstallStatus {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error("Invalid plugin uninstallation request record.");
  }
  // fields reads the declared transport names without searching aliases.
  // fields 读取声明传输名称，不搜索别名。
  const fields = value as Record<string, unknown>;
  if (typeof fields.requestId !== "string" || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(fields.requestId)
    || fields.pluginId !== pluginId || pluginId.trim().length === 0
    || (fields.phase !== "executing" && fields.phase !== "reserved" && fields.phase !== "lifecycle"
      && fields.phase !== "publishing" && fields.phase !== "collection")
    || typeof fields.requiresExternalEffectsAcknowledgement !== "boolean") {
    throw new Error("Invalid plugin uninstallation request identity or phase.");
  }
  return { requestId: fields.requestId, pluginId, phase: fields.phase,
    requiresExternalEffectsAcknowledgement: fields.requiresExternalEffectsAcknowledgement };
}
