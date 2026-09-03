import type {
  PublicAiModelCapabilities,
  PublicAiProviderToolCapabilityKind,
  PublicAiResourceKind,
} from "@/types";

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
