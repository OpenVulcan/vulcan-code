import type {
  PublicAiModelCapabilities,
  PublicAiProviderExtensionModeSelection,
  PublicAiProviderToolCapabilityKind,
  PublicAiProviderToolTargetSelection,
  PublicAiResourceKind,
} from "@/types";

// Blocks invalid enablement while preserving the ability to turn off a stale selection.
// 阻止无效启用，同时保留关闭失效选择的能力。
// Parameters: pending is an active mutation, backendAvailable is a concrete route, enabled is persisted state.
// 参数：pending 表示写入进行中，backendAvailable 表示存在具体路由，enabled 是持久化状态。
// Returns true only when the switch must reject every transition; this pure rule imports no settings UI.
// 仅当开关必须拒绝全部状态转换时返回 true；此纯规则不引入设置界面。
export function mediaUnderstandingCapabilityToggleDisabled(
  pending: boolean,
  backendAvailable: boolean,
  enabled: boolean,
): boolean {
  return pending || (!backendAvailable && !enabled);
}

// DIRECT_PROVIDER_TOOL_CAPABILITIES is the closed set configured through one selected target.
// DIRECT_PROVIDER_TOOL_CAPABILITIES 是通过一个选定目标配置的封闭能力集合。
const DIRECT_PROVIDER_TOOL_CAPABILITIES = new Set<PublicAiProviderToolCapabilityKind>([
  "web_search",
  "web_extract",
  "image_understand",
  "video_understand",
  "audio_understand",
]);

// MODEL_EXTENSION_CAPABILITIES is the closed set configured through native multi-selection.
// MODEL_EXTENSION_CAPABILITIES 是通过原生多选配置的封闭能力集合。
const MODEL_EXTENSION_CAPABILITIES = new Set<PublicAiProviderToolCapabilityKind>([
  "image_generate",
  "image_edit",
  "video_generate",
  "video_edit",
  "video_extend",
  "speech_synthesize",
  "speech_transcribe",
  "music_generate",
  "music_cover",
]);

// isDirectProviderToolCapability reports whether one capability uses direct selection.
// isDirectProviderToolCapability 判断一项能力是否使用直连单选。
export function isDirectProviderToolCapability(
  capability: PublicAiProviderToolCapabilityKind,
): boolean {
  return DIRECT_PROVIDER_TOOL_CAPABILITIES.has(capability);
}

// isModelExtensionCapability reports whether one capability uses stable extension dispatch.
// isModelExtensionCapability 判断一项能力是否使用稳定扩展分派。
export function isModelExtensionCapability(
  capability: PublicAiProviderToolCapabilityKind,
): boolean {
  return MODEL_EXTENSION_CAPABILITIES.has(capability);
}

// modelCapabilitiesSupportMediaUnderstanding reports whether one exact conversation route can consume the requested media.
// modelCapabilitiesSupportMediaUnderstanding 判断一条精确会话路由能否消费请求的媒体。
export function modelCapabilitiesSupportMediaUnderstanding(
  capabilities: PublicAiModelCapabilities,
  capability: PublicAiProviderToolCapabilityKind,
): boolean {
  // requiredResource is the sole non-text input required by the fixed understanding facade.
  // requiredResource 是固定理解外壳要求的唯一非文本输入。
  let requiredResource: PublicAiResourceKind;
  switch (capability) {
    case "image_understand":
      requiredResource = "image";
      break;
    case "video_understand":
      requiredResource = "video";
      break;
    case "audio_understand":
      requiredResource = "audio";
      break;
    default:
      return false;
  }
  return capabilities.operations?.includes("conversation_respond") === true
    && capabilities.input_resources?.includes(requiredResource) === true;
}

// providerExtensionModeOf returns one capability's mode; a missing entry is single mode with no model.
// providerExtensionModeOf 返回一项能力的模式；缺失条目即未选模型的单模型模式。
export function providerExtensionModeOf(
  modes: Partial<Record<PublicAiProviderToolCapabilityKind, PublicAiProviderExtensionModeSelection>>,
  capability: PublicAiProviderToolCapabilityKind,
): PublicAiProviderExtensionModeSelection {
  return modes[capability] ?? { mode: "single" };
}

// withProviderExtensionMode returns `modes` with one capability replaced; the default entry is dropped.
// withProviderExtensionMode 返回替换了一项能力的 `modes`；默认条目会被移除。
export function withProviderExtensionMode(
  modes: Partial<Record<PublicAiProviderToolCapabilityKind, PublicAiProviderExtensionModeSelection>>,
  capability: PublicAiProviderToolCapabilityKind,
  selection: PublicAiProviderExtensionModeSelection,
): Partial<Record<PublicAiProviderToolCapabilityKind, PublicAiProviderExtensionModeSelection>> {
  const next = { ...modes };
  if (selection.mode === "single" && !selection.single_target) {
    delete next[capability];
  } else {
    next[capability] = selection;
  }
  return next;
}

// providerExtensionActiveSelections lists the models AI can reach for one capability in its mode.
// providerExtensionActiveSelections 列出一项能力在其当前模式下 AI 可使用的模型。
// Single mode yields at most its fixed model; multi mode yields the stored multi-model set.
// 单模型模式最多给出其固定模型；多模型模式给出已存的多模型集合。
export function providerExtensionActiveSelections(
  modes: Partial<Record<PublicAiProviderToolCapabilityKind, PublicAiProviderExtensionModeSelection>>,
  sets: Partial<Record<PublicAiProviderToolCapabilityKind, PublicAiProviderToolTargetSelection[]>>,
  capability: PublicAiProviderToolCapabilityKind,
): PublicAiProviderToolTargetSelection[] {
  const selection = providerExtensionModeOf(modes, capability);
  if (selection.mode === "multi") return sets[capability] ?? [];
  return selection.single_target ? [selection.single_target] : [];
}
