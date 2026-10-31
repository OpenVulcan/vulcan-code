import type { PublicAiCustomProviderSaveRequest, PublicAiServiceEntry } from "@/types"

// CustomProviderCatalogDraft is the sole editable catalog contract for a custom provider.
// CustomProviderCatalogDraft 是自定义供应商唯一可编辑目录契约。
export type CustomProviderCatalogDraft = Pick<PublicAiCustomProviderSaveRequest,
  "modelDiscoveryProtocol" | "modelDiscoveryMode" | "modelDiscoveryPath">

// Reads the entry-owned catalog from entry, or returns visible defaults for a new provider.
// 从 entry 读取入口持有的目录，或为新供应商返回可见默认值。
// Returns one catalog draft; conversation routes never supply discovery settings.
// 返回唯一目录草稿；会话路由绝不提供目录设置。
export function customProviderCatalogDraft(entry: PublicAiServiceEntry | undefined): CustomProviderCatalogDraft {
  if (!entry) {
    return { modelDiscoveryProtocol: "open_ai", modelDiscoveryMode: "standard", modelDiscoveryPath: "" }
  }
  // protocol is resolved only from the persisted shared catalog format.
  // protocol 仅由已持久化共享目录格式确定。
  let protocol: CustomProviderCatalogDraft["modelDiscoveryProtocol"]
  switch (entry.model_discovery) {
    case "open_ai":
    case "manual":
      protocol = "open_ai"
      break
    case "anthropic_compatible":
      protocol = "anthropic"
      break
    case "gemini":
      protocol = "gemini"
      break
    default:
      throw new Error(`Unsupported custom catalog protocol: ${entry.model_discovery}`)
  }
  return {
    modelDiscoveryProtocol: protocol,
    modelDiscoveryMode: entry.model_discovery === "manual" ? "manual" : entry.model_discovery_path ? "custom_path" : "standard",
    modelDiscoveryPath: entry.model_discovery_path ?? "",
  }
}
