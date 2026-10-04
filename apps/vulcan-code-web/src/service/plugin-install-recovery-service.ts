import { hasServiceRuntime, serviceClient } from "./webui-service-runtime";

// Exact phases mirror LocalPluginInstallPhase in the service kernel and must change with that wire contract.
// 精确阶段映射服务内核 LocalPluginInstallPhase，必须随该线协议契约同步变化。
export type PluginInstallPhase =
  | "executing"
  | "reserved"
  | "packagePublication"
  | "credentialMutation"
  | "nativeValidation"
  | "lifecycle"
  | "publishing"
  | "aborting"
  | "aborted"
  | "committed";

// A sanitized original installation remains queryable even when no plugin row has been published.
// 即使尚未发布插件行，脱敏原安装仍可查询。
export interface PluginInstallStatus {
  // The actual core reservation allocated this sole recovery identity before package copying.
  // 实际核心预留在插件包复制之前分配此唯一恢复身份。
  requestId: string;
  // Exact immutable original plugin namespace selects no latest-plugin fallback.
  // 精确不可变原插件命名空间不选择最新插件备用路径。
  pluginId: string;
  // Executing reports an exclusive borrow without inferring native effects.
  // executing 报告排他借用，不推断原生副作用。
  phase: PluginInstallPhase;
}

// Read retained originals for pluginId through the actual service without exposing credentials or source paths.
// 通过实际服务读取 pluginId 的保留原实例，不暴露凭据或来源路径。
// Returns sanitized UUIDs and phases, including originals retained after an executor panic.
// 返回脱敏 UUID 及阶段，包括执行器 panic 后保留的原实例。
export async function readPluginInstallRequests(pluginId: string): Promise<PluginInstallStatus[]> {
  if (!hasServiceRuntime()) {
    throw new Error("Plugin installation recovery requires the Vulcan service runtime.");
  }
  return serviceClient.request<PluginInstallStatus[]>("service_local_plugin_install_requests", { pluginId });
}

// Recover requestId's exact original for pluginId; acknowledgeExternalEffects applies only to physically retired failed compensation.
// 恢复 pluginId 的 requestId 精确原实例；acknowledgeExternalEffects 仅适用于物理已退役失败补偿。
// Returns after original rollback or committed projection and retirement; acknowledgement never bypasses native cleanup.
// 原回滚或已提交投影及退役后返回；确认绝不绕过原生清理。
export async function recoverPluginInstallRequest(pluginId: string, requestId: string, acknowledgeExternalEffects: boolean): Promise<void> {
  if (!hasServiceRuntime()) {
    throw new Error("Plugin installation recovery requires the Vulcan service runtime.");
  }
  await serviceClient.request<null>("service_recover_local_plugin_install", { pluginId, requestId, acknowledgeExternalEffects });
}
