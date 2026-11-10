import type { PublicAiConnection, PublicAiCredentialProfile } from "@/types"

// usageCapabilitySupportsQuotaWindows returns whether one account contract exposes plan windows.
// usageCapabilitySupportsQuotaWindows 返回一个账号契约是否公开套餐额度窗口。
//
// Parameters
// 参数
// `capability` is the exact backend-declared account usage implementation.
// `capability` 是后端声明的精确账号用量实现。
//
// Returns
// 返回值
// True only for usage implementations that produce user-facing quota windows.
// 仅对会产生用户可见额度窗口的用量实现返回 true。
export function usageCapabilitySupportsQuotaWindows(
  capability: PublicAiConnection["usage_capability"],
): boolean {
  return capability === "mini_max_token_plan"
    || capability === "opencode_go"
    || capability === "tavily_credits"
    || capability === "kimi_coding_plan"
    || capability === "command_code_usage"
    || capability === "open_design_wallet"
    || capability === "open_ai_codex_usage"
    || capability === "xiaomi_mimo_token_plan"
}

// usageCapabilityRequiresManagementKey returns whether one registered usage primitive needs a separate credential.
// usageCapabilityRequiresManagementKey 返回一个已注册用量原语是否需要独立凭据。
//
// Parameters
// 参数
// `capability` is the exact manifest-projected account usage primitive.
// `capability` 是清单投影得到的精确账号用量原语。
//
// Returns
// 返回值
// True only when the registered primitive requires a credential separate from model inference.
// 仅当已注册原语需要独立于模型推理的凭据时返回 true。
export function usageCapabilityRequiresManagementKey(
  capability: PublicAiCredentialProfile["account_usage_query"],
): boolean {
  return capability === "open_router_credits"
}
