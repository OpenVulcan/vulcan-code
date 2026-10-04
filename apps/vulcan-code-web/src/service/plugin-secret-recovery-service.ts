import { hasServiceRuntime, serviceClient } from "./webui-service-runtime";

// Exact phases follow LegacySecretMutationPhase in the service kernel; change both on wire contract updates.
// 精确阶段遵循服务内核 LegacySecretMutationPhase；线协议契约变化时同步修改两处。
export type PluginSecretMutationPhase =
  | "executing"
  | "credentialMutation"
  | "nativeRefresh"
  | "restored"
  | "committed";

// One non-secret original request observation grants no credential or native recovery authority.
// 单个无秘密原请求观测不授予凭据或原生恢复权威。
export interface PluginSecretMutationStatus {
  // The original service UUID must be passed unchanged to explicit recovery.
  // 原服务 UUID 必须原样传给明确恢复。
  requestId: string;
  // Exact plugin namespace retained before the native credential write.
  // 原生凭据写入之前保留的精确插件命名空间。
  pluginId: string;
  // Exact manifest-declared server identifier.
  // 精确清单声明服务器标识。
  serverId: string;
  // Public manifest key identity contains no credential value.
  // 公共清单键身份不含凭据值。
  secretKey: string;
  // Executing indicates an active exclusive borrow, without guessing whether native effects started.
  // executing 表示活动排他借用，不猜测原生副作用是否开始。
  phase: PluginSecretMutationPhase;
}

// Read retained original requests for pluginId without loading any credential into the renderer.
// 读取 pluginId 的保留原请求，不向渲染器加载任何凭据。
// Returns original UUIDs and non-secret phases; requires the authenticated service runtime.
// 返回原 UUID 及无秘密阶段；要求经过认证的服务运行时。
export async function readPluginSecretMutations(pluginId: string): Promise<PluginSecretMutationStatus[]> {
  if (!hasServiceRuntime()) {
    throw new Error("Plugin credential recovery requires the Vulcan service runtime.");
  }
  return serviceClient.request<PluginSecretMutationStatus[]>("service_plugin_mcp_secret_mutation_requests", { pluginId });
}

// Restore the exact original requestId for pluginId; the service validates retained physical ownership.
// 恢复 pluginId 的精确原 requestId；服务校验保留物理所有权。
// Returns after original native cleanup and preimage restoration, or committed projection, event delivery and retirement.
// 原原生清理及前像恢复后返回，或完成已提交投影、事件投递及退役。
export async function recoverPluginSecretMutation(pluginId: string, requestId: string): Promise<void> {
  if (!hasServiceRuntime()) {
    throw new Error("Plugin credential recovery requires the Vulcan service runtime.");
  }
  await serviceClient.request<null>("service_recover_plugin_mcp_secret_mutation", { pluginId, requestId });
}
