// Constants shared by the custom provider models dialog and its editors.
// 自定义供应商模型弹窗及其编辑器共享的常量。
import type { PublicAiCustomModelRequestProjection } from "@/types"

// EMPTY_PROJECTION is the safe request projection used until the user declares exact mappings.
// EMPTY_PROJECTION 是用户声明精确映射前使用的安全请求投影。
export const EMPTY_PROJECTION: PublicAiCustomModelRequestProjection = {
  reasoning: {},
  model_parameter_paths: {},
  defaults: {},
  overrides: {},
  delete: [],
}

// DISCOVERY_STATE_LABEL_KEYS maps every closed backend status to one localized label.
// DISCOVERY_STATE_LABEL_KEYS 将每个封闭后端状态映射为一个本地化标签。
export const DISCOVERY_STATE_LABEL_KEYS = {
  not_checked: "settings.customModels.discovery.notChecked",
  ready: "settings.customModels.discovery.ready",
  empty: "settings.customModels.discovery.empty",
  unsupported: "settings.customModels.discovery.unsupported",
  authentication_failed: "settings.customModels.discovery.authenticationFailed",
  rate_limited: "settings.customModels.discovery.rateLimited",
  temporarily_unavailable: "settings.customModels.discovery.temporarilyUnavailable",
  invalid_response: "settings.customModels.discovery.invalidResponse",
  stale: "settings.customModels.discovery.stale",
  failed: "settings.customModels.discovery.failed",
} as const
