import type { TranslationKey } from "@/i18n-messages";
import type { PublicAiModelCapabilities, PublicAiReasoningEffort } from "@/types";

// PUBLIC_AI_REASONING_EFFORT_LABEL_KEYS maps the closed public effort contract to localized labels.
// PUBLIC_AI_REASONING_EFFORT_LABEL_KEYS 将封闭公共推理档位契约映射为本地化标签。
export const PUBLIC_AI_REASONING_EFFORT_LABEL_KEYS = {
  none: "settings.customModels.effortNone",
  adaptive: "settings.customModels.effortAdaptive",
  minimal: "settings.customModels.effortMinimal",
  low: "settings.customModels.effortLow",
  medium: "settings.customModels.effortMedium",
  high: "settings.customModels.effortHigh",
  xhigh: "settings.customModels.effortExtraHigh",
  max: "settings.customModels.effortMax",
  ultra: "settings.customModels.effortUltra",
} as const satisfies Record<PublicAiReasoningEffort, TranslationKey>;

// isPublicAiReasoningEffort narrows catalog strings to the closed public reasoning contract.
// isPublicAiReasoningEffort 将目录字符串收窄为封闭的公共推理契约。
export function isPublicAiReasoningEffort(value: string): value is PublicAiReasoningEffort {
  return value === "none"
    || value === "adaptive"
    || value === "minimal"
    || value === "low"
    || value === "medium"
    || value === "high"
    || value === "xhigh"
    || value === "max"
    || value === "ultra";
}

// mediaReplacementReasoningEfforts returns every explicit level accepted by one exact route.
// mediaReplacementReasoningEfforts 返回一条精确路由接受的全部显式思考档位。
export function mediaReplacementReasoningEfforts(
  capabilities: PublicAiModelCapabilities,
): PublicAiReasoningEffort[] {
  // supportedEfforts is the signed model route's provider-neutral level list.
  // supportedEfforts 是签名模型路由的供应商无关档位列表。
  const supportedEfforts = (capabilities.reasoning_efforts ?? [])
    .filter(isPublicAiReasoningEffort);
  switch (capabilities.reasoning_control) {
    case "fixed_off":
      return ["none"];
    case "fixed_on":
      return supportedEfforts.filter((effort) => effort !== "none");
    case "toggle":
    case "effort_selection":
    case "provider_adaptive":
      return supportedEfforts;
    default:
      return [];
  }
}

// mediaReplacementReasoningOverrides returns only levels exposed through a user override control.
// mediaReplacementReasoningOverrides 只返回应通过用户覆写控件公开的档位。
export function mediaReplacementReasoningOverrides(
  capabilities: PublicAiModelCapabilities,
): PublicAiReasoningEffort[] {
  switch (capabilities.reasoning_control) {
    case "toggle":
    case "effort_selection":
      return (capabilities.reasoning_efforts ?? []).filter(isPublicAiReasoningEffort);
    case "fixed_off":
    case "fixed_on":
    case "provider_adaptive":
    default:
      return [];
  }
}
