import type { CustomProviderModelRow } from "@/components/features/settings/custom-provider-models-state"
import type {
  PublicAiCustomProviderModelDefaults,
  PublicAiModelCapabilityOverrides,
} from "@/types"

// OPENAI_DIRECT_REASONING_EFFORTS is the closed reasoning-level set shown by direct editors.
// OPENAI_DIRECT_REASONING_EFFORTS 是直接编辑器展示的封闭 OpenAI 思考档位集合。
export const OPENAI_DIRECT_REASONING_EFFORTS = ["none", "minimal", "low", "medium", "high", "xhigh", "max"] as const

// resolveDirectCustomProviderDefaults returns an isolated authoritative draft from a saved layer or system baseline.
// resolveDirectCustomProviderDefaults 从已保存层或系统基线返回一份隔离的权威草稿。
export function resolveDirectCustomProviderDefaults(
  systemDefaults: PublicAiCustomProviderModelDefaults,
  saved: PublicAiCustomProviderModelDefaults | undefined,
): PublicAiCustomProviderModelDefaults {
  // source selects the persisted layer when present and otherwise the documented system baseline.
  // source 在持久化层存在时选择该层，否则选择文档化系统基线。
  const source = saved ?? systemDefaults
  return {
    context_limit: source.context_limit,
    output_limit: source.output_limit,
    input_resources: [...(source.input_resources ?? [])],
    structured_output: source.structured_output,
    reasoning: source.reasoning,
    reasoning_control: source.reasoning_control,
    reasoning_efforts: [...source.reasoning_efforts],
    // contextWindowPolicy keeps the provider-level Codex threshold layer intact when other fields are edited.
    // contextWindowPolicy 在编辑其他字段时保持供应商级 Codex 阈值层不丢失。
    context_window_policy: source.context_window_policy,
    request_projection: source.request_projection,
  }
}

// projectOpenAiReasoningEfforts keeps compatible levels or returns the complete standard default set.
// projectOpenAiReasoningEfforts 保留兼容档位，或返回完整标准默认集合。
export function projectOpenAiReasoningEfforts(efforts: readonly string[]): string[] {
  // selected contains only values the direct editors can faithfully round-trip.
  // selected 仅包含直接编辑器能够无损往返的值。
  const selected = OPENAI_DIRECT_REASONING_EFFORTS.filter((effort) => efforts.includes(effort))
  return selected.length > 0 ? selected : [...OPENAI_DIRECT_REASONING_EFFORTS]
}

// directCustomModelSettingsDraft resolves one complete editable model layer from explicit or effective values.
// directCustomModelSettingsDraft 从显式值或有效值解析一层完整可编辑模型设置。
export function directCustomModelSettingsDraft(row: CustomProviderModelRow): PublicAiModelCapabilityOverrides {
  // capabilities is the authoritative runtime model after lower-precedence layers resolve.
  // capabilities 是低优先级层解析后的权威运行时模型。
  const capabilities = (row.capabilityModel ?? row.effectiveModel)?.capabilities
  // reasoningSupport preserves explicit model intent before using the effective catalog.
  // reasoningSupport 在使用有效目录前保留显式模型意图。
  const reasoningSupport = row.capabilityOverrides?.reasoning ?? capabilities?.reasoning ?? "native"
  // reasoningControl preserves the paired control state for explicit disablement.
  // reasoningControl 为显式关闭保留配对控制状态。
  const reasoningControl = row.capabilityOverrides?.reasoning_control ?? capabilities?.reasoning_control ?? "effort_selection"
  // inheritedEfforts supplies declared levels before projection to the direct editor.
  // inheritedEfforts 在投影到直接编辑器前提供已声明档位。
  const inheritedEfforts = row.capabilityOverrides?.reasoning_efforts ?? capabilities?.reasoning_efforts ?? []
  // reasoningEnabled requires both support and control to be active.
  // reasoningEnabled 要求支持与控制均处于启用状态。
  const reasoningEnabled = reasoningSupport !== "unsupported" && reasoningControl !== "fixed_off"
  const requestProjection = row.capabilityOverrides?.request_projection
  return {
    context_limit: row.capabilityOverrides?.context_limit ?? capabilities?.context_limit ?? 200_000,
    output_limit: row.capabilityOverrides?.output_limit ?? capabilities?.output_limit ?? 32_768,
    input_resources: row.capabilityOverrides?.input_resources ?? capabilities?.input_resources ?? [],
    structured_output: row.capabilityOverrides?.structured_output ?? capabilities?.structured_output ?? "json_object",
    reasoning: reasoningEnabled ? "native" : "unsupported",
    reasoning_control: reasoningEnabled ? "effort_selection" : "fixed_off",
    reasoning_efforts: reasoningEnabled
      ? projectOpenAiReasoningEfforts(inheritedEfforts)
      : ["none"],
    // contextWindowPolicy is owned only when this exact model already has a sparse override.
    // contextWindowPolicy 仅在该精确模型已有稀疏覆盖时由模型层持有。
    context_window_policy: row.capabilityOverrides?.context_window_policy ?? undefined,
    ...(requestProjection ? { request_projection: requestProjection } : {}),
  }
}

// directProviderModelSettingsEqual compares the semantic fields owned by both direct editors.
// directProviderModelSettingsEqual 比较两个直接编辑器共同持有的语义字段。
export function directProviderModelSettingsEqual(
  left: PublicAiModelCapabilityOverrides,
  right: PublicAiModelCapabilityOverrides,
): boolean {
  return JSON.stringify(normalizeDirectProviderModelSettings(left))
    === JSON.stringify(normalizeDirectProviderModelSettings(right))
}

// normalizeDirectProviderModelSettings removes collection-order noise from one direct settings record.
// normalizeDirectProviderModelSettings 从一份直接设置记录中移除集合顺序噪声。
function normalizeDirectProviderModelSettings(settings: PublicAiModelCapabilityOverrides) {
  return {
    contextLimit: settings.context_limit ?? null,
    outputLimit: settings.output_limit ?? null,
    inputResources: [...(settings.input_resources ?? [])].sort(),
    structuredOutput: settings.structured_output ?? null,
    reasoning: settings.reasoning ?? null,
    reasoningControl: settings.reasoning_control ?? null,
    reasoningEfforts: [...(settings.reasoning_efforts ?? [])].sort(),
    contextWindowPolicy: settings.context_window_policy ?? null,
    requestProjection: settings.request_projection ?? null,
  }
}
