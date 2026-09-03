import type { TranslationKey } from "@/i18n";
import type { PublicAiProviderToolCapabilityKind } from "@/types";

// PROVIDER_TOOL_TITLE_KEYS is exhaustive over every provider capability, so new capabilities cannot omit i18n silently.
// PROVIDER_TOOL_TITLE_KEYS 穷尽每项供应商能力，使新增能力无法静默遗漏 i18n。
const PROVIDER_TOOL_TITLE_KEYS: Readonly<Record<PublicAiProviderToolCapabilityKind, TranslationKey>> = {
  web_search: "settings.providerTools.webSearch",
  web_extract: "settings.providerTools.webExtract",
  image_understand: "settings.aiUnderstandImage",
  video_understand: "settings.aiUnderstandVideo",
  audio_understand: "settings.aiUnderstandAudio",
  image_generate: "settings.providerTools.imageGenerate",
  image_edit: "settings.providerTools.imageEdit",
  video_generate: "settings.providerTools.videoGenerate",
  video_edit: "settings.providerTools.videoEdit",
  video_extend: "settings.providerTools.videoExtend",
  speech_synthesize: "settings.providerTools.speechSynthesize",
  speech_transcribe: "settings.providerTools.speechTranscribe",
  music_generate: "settings.providerTools.musicGenerate",
  music_cover: "settings.providerTools.musicCover",
};

// MODEL_EXTENSION_PARAMETERS_TITLE_KEY is the non-capability title reserved for parameter discovery.
// MODEL_EXTENSION_PARAMETERS_TITLE_KEY 是专用于参数查询的非能力标题。
const MODEL_EXTENSION_PARAMETERS_TITLE_KEY: TranslationKey = "toolPresentation.modelExtension.parameters";

// providerToolCapabilityFromToolName validates an exact public tool id against the exhaustive capability record.
// providerToolCapabilityFromToolName 根据穷尽能力记录校验精确公开工具标识。
// Parameters: toolName is the public tool id retained by the conversation event.
// 参数：toolName 是会话事件保留的公开工具标识。
// Returns: the exact provider capability, or null when the tool is outside that contract.
// 返回值：精确供应商能力；工具不属于该契约时返回 null。
function providerToolCapabilityFromToolName(
  toolName: string,
): PublicAiProviderToolCapabilityKind | null {
  return Object.prototype.hasOwnProperty.call(PROVIDER_TOOL_TITLE_KEYS, toolName)
    ? toolName as PublicAiProviderToolCapabilityKind
    : null;
}

// modelExtensionToolTitleKey returns the localized title key for one exact provider or parameter-discovery tool id.
// modelExtensionToolTitleKey 返回一个精确供应商或参数查询工具标识对应的本地化标题键。
// Parameters: toolName is the public tool id retained by the conversation event.
// 参数：toolName 是会话事件保留的公开工具标识。
// Returns: the configured translation key, or null when the tool is outside the extension contract.
// 返回值：已配置的翻译键；工具不属于扩展契约时返回 null。
export function modelExtensionToolTitleKey(toolName: string): TranslationKey | null {
  if (toolName === "get_model_extension_parameters") {
    return MODEL_EXTENSION_PARAMETERS_TITLE_KEY;
  }
  // capability is the validated provider-tool identity used for exhaustive title lookup.
  // capability 是用于穷尽标题查询的已校验供应商工具身份。
  const capability = providerToolCapabilityFromToolName(toolName);
  return capability ? PROVIDER_TOOL_TITLE_KEYS[capability] : null;
}

// modelExtensionToolCapability returns the provider capability that owns one exact AI extension tool icon.
// modelExtensionToolCapability 返回持有一个精确 AI 扩展工具图标的供应商能力。
// Parameters: toolName is the public tool id retained by the conversation event.
// 参数：toolName 是会话事件保留的公开工具标识。
// Returns: the exact provider capability, or null for parameter discovery and unknown tools.
// 返回值：精确供应商能力；参数查询与未知工具返回 null。
export function modelExtensionToolCapability(
  toolName: string,
): PublicAiProviderToolCapabilityKind | null {
  return providerToolCapabilityFromToolName(toolName);
}

// modelExtensionToolMarkdown formats an exact recognized AI-tool result as a bounded Markdown JSON block.
// modelExtensionToolMarkdown 将精确识别的 AI 工具结果格式化为受限高 Markdown JSON 代码块。
// Parameters: toolName identifies the public AI-tool contract and content is its visible or hydrated result.
// 参数：toolName 标识公开 AI 工具契约，content 是其可见或补载结果。
// Returns: pretty JSON Markdown for valid AI-tool JSON, otherwise the original content unchanged.
// 返回值：有效 AI 工具 JSON 对应的格式化 Markdown；否则原样返回内容。
export function modelExtensionToolMarkdown(toolName: string, content: string): string {
  if (!modelExtensionToolTitleKey(toolName)) {
    return content;
  }
  // trimmedContent removes transport whitespace without changing non-JSON fallback text.
  // trimmedContent 去除传输空白，但不会改变非 JSON 兜底文本。
  const trimmedContent = content.trim();
  if (!trimmedContent) {
    return "";
  }
  try {
    // payload is the exact JSON value returned by the extension tool.
    // payload 是扩展工具返回的精确 JSON 值。
    const payload: unknown = JSON.parse(trimmedContent);
    return `\`\`\`json\n${JSON.stringify(payload, null, 2)}\n\`\`\``;
  } catch {
    return content;
  }
}

// modelExtensionToolValueMarkdown formats one exact structured AI-tool value for the input card.
// modelExtensionToolValueMarkdown 为输入卡格式化一个精确的结构化 AI 工具值。
// Parameters: toolName identifies the public AI-tool contract and value is its persisted input.
// 参数：toolName 标识公开 AI 工具契约，value 是其持久化输入。
// Returns: bounded JSON Markdown, the original string, or an empty string when no input exists.
// 返回值：受限高 JSON Markdown、原始字符串，或输入不存在时的空字符串。
export function modelExtensionToolValueMarkdown(toolName: string, value: unknown): string {
  if (!modelExtensionToolTitleKey(toolName) || value === null || value === undefined) {
    return "";
  }
  // content preserves exact string inputs and serializes structured JSON values once.
  // content 保留精确字符串输入，并仅序列化一次结构化 JSON 值。
  const content = typeof value === "string" ? value : JSON.stringify(value);
  return modelExtensionToolMarkdown(toolName, content);
}
