import type { TranslationKey } from "@/i18n";

// MODEL_EXTENSION_TOOL_TITLE_KEYS is the complete exact display-name contract for AI model-extension tools.
// MODEL_EXTENSION_TOOL_TITLE_KEYS 是 AI 模型扩展工具的完整精确显示名称契约。
const MODEL_EXTENSION_TOOL_TITLE_KEYS = new Map<string, TranslationKey>([
  ["get_model_extension_parameters", "toolPresentation.modelExtension.parameters"],
  ["image_generate", "settings.providerTools.imageGenerate"],
  ["image_edit", "settings.providerTools.imageEdit"],
  ["video_generate", "settings.providerTools.videoGenerate"],
  ["video_edit", "settings.providerTools.videoEdit"],
  ["video_extend", "settings.providerTools.videoExtend"],
  ["speech_synthesize", "settings.providerTools.speechSynthesize"],
  ["speech_transcribe", "settings.providerTools.speechTranscribe"],
  ["music_generate", "settings.providerTools.musicGenerate"],
  ["music_cover", "settings.providerTools.musicCover"],
]);

// modelExtensionToolTitleKey returns the localized title key for one exact AI model-extension tool id.
// modelExtensionToolTitleKey 返回一个精确 AI 模型扩展工具标识对应的本地化标题键。
// Parameters: toolName is the public tool id retained by the conversation event.
// 参数：toolName 是会话事件保留的公开工具标识。
// Returns: the configured translation key, or null when the tool is outside the extension contract.
// 返回值：已配置的翻译键；工具不属于扩展契约时返回 null。
export function modelExtensionToolTitleKey(toolName: string): TranslationKey | null {
  return MODEL_EXTENSION_TOOL_TITLE_KEYS.get(toolName) ?? null;
}

// modelExtensionToolMarkdown formats an exact extension result as a bounded Markdown JSON block.
// modelExtensionToolMarkdown 将精确扩展结果格式化为受限高 Markdown JSON 代码块。
// Parameters: toolName identifies the public tool contract and content is its visible or hydrated result.
// 参数：toolName 标识公开工具契约，content 是其可见或补载结果。
// Returns: pretty JSON Markdown for valid extension JSON, otherwise the original content unchanged.
// 返回值：有效扩展 JSON 对应的格式化 Markdown；否则原样返回内容。
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

// modelExtensionToolValueMarkdown formats one exact structured extension value for the input card.
// modelExtensionToolValueMarkdown 为输入卡格式化一个精确的结构化扩展值。
// Parameters: toolName identifies the public extension contract and value is its persisted input.
// 参数：toolName 标识公开扩展契约，value 是其持久化输入。
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
