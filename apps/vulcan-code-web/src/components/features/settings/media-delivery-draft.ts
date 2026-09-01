import type { PublicAiMediaDeliveryPolicy, PublicAiMediaDeliverySettingsSnapshot, PublicAiMediaInlineThresholds, PublicAiMediaPurposePolicy } from "@/types";

// MEDIA_DELIVERY_PURPOSES enumerates the two independent backend-owned settings domains.
// MEDIA_DELIVERY_PURPOSES 枚举两个独立的后端设置领域。
export const MEDIA_DELIVERY_PURPOSES = ["conversation", "independent_extension"] as const;
// MEDIA_DELIVERY_KINDS preserves the four explicit modality fields; PDF belongs to file.
// MEDIA_DELIVERY_KINDS 保留四个显式模态字段；PDF 属于 file。
export const MEDIA_DELIVERY_KINDS = ["image", "audio", "video", "file"] as const;
// MediaDeliveryPurpose names one exact editable purpose, never an AI-controlled request mode.
// MediaDeliveryPurpose 命名一个精确可编辑用途，绝不是 AI 控制的请求模式。
export type MediaDeliveryPurpose = typeof MEDIA_DELIVERY_PURPOSES[number];
// MediaDeliveryKind names one backend modality field.
// MediaDeliveryKind 命名一个后端模态字段。
export type MediaDeliveryKind = typeof MEDIA_DELIVERY_KINDS[number];
// MediaDeliveryThresholdField selects one exact real-upload availability case.
// MediaDeliveryThresholdField 选择一个精确的真实上传可用性分支。
export type MediaDeliveryThresholdField = keyof PublicAiMediaInlineThresholds;
// MediaDeliveryPurposeDraft holds editable KiB strings without changing their backend field identities.
// MediaDeliveryPurposeDraft 保存可编辑的 KiB 字符串，不改变后端字段身份。
export type MediaDeliveryPurposeDraft = Record<MediaDeliveryKind, Record<MediaDeliveryThresholdField, string>> & { max_total_encoded_bytes: string };
// MediaDeliveryPolicyDraft retains all sixteen limits and both budgets as independent inputs.
// MediaDeliveryPolicyDraft 将全部十六项限制和两个预算保留为独立输入。
export type MediaDeliveryPolicyDraft = Record<MediaDeliveryPurpose, MediaDeliveryPurposeDraft>;

// MediaDeliveryDraftError identifies the exact invalid field and a localizable reason.
// MediaDeliveryDraftError 标识精确无效字段及可本地化原因。
export class MediaDeliveryDraftError extends Error {
  // Exact stable editor field id, including purpose and media kind when applicable.
  // 精确稳定编辑字段标识，在适用时包含用途及媒体类型。
  readonly field: string;
  // Closed validation cause; backend transport failures are not converted into this type.
  // 封闭校验原因；后端传输失败不会被转换为此类型。
  readonly code: "number" | "limit" | "fraction";

  // Constructs a typed error for field and code; returns no normalized or guessed value.
  // 根据 field 和 code 构建强类型错误，不返回规范化或猜测值。
  constructor(field: string, code: MediaDeliveryDraftError["code"]) {
    super("Invalid media delivery settings input");
    this.field = field;
    this.code = code;
  }
}

// purposeDraft converts exact policy byte values to lossless KiB strings; it supplies no defaults.
// purposeDraft 把精确策略字节值转换为无损 KiB 字符串，不提供默认值。
function purposeDraft(policy: PublicAiMediaPurposePolicy): MediaDeliveryPurposeDraft {
  // thresholds converts both cases for one modality without coupling their values.
  // thresholds 转换一个模态的两个分支，不使它们的值产生耦合。
  const thresholds = (value: PublicAiMediaInlineThresholds) => ({
    with_upload_max_bytes: String(value.with_upload_max_bytes / 1024),
    without_upload_max_bytes: String(value.without_upload_max_bytes / 1024),
  });
  return {
    image: thresholds(policy.image), audio: thresholds(policy.audio),
    video: thresholds(policy.video), file: thresholds(policy.file),
    max_total_encoded_bytes: String(policy.max_total_encoded_bytes / 1024),
  };
}

// createMediaDeliveryDraft returns an independent editor buffer from the server's complete policy.
// createMediaDeliveryDraft 根据服务器完整策略返回独立编辑缓冲区。
export function createMediaDeliveryDraft(policy: PublicAiMediaDeliveryPolicy): MediaDeliveryPolicyDraft {
  return { conversation: purposeDraft(policy.conversation), independent_extension: purposeDraft(policy.independent_extension) };
}

// parseKiB validates value against the server's maximum for field and returns exact integer bytes.
// parseKiB 按服务器为 field 提供的最大值校验 value，返回精确整数字节。
// Empty, negative, exponential, oversized and sub-byte values fail without coercing them to zero.
// 空值、负数、指数、超限及不足整字节的值会失败，不把它们强制转换为零。
function parseKiB(value: string, maximum: number, field: string): number {
  // text is only whitespace-trimmed; an absent number never inherits another field's setting.
  // text 仅去除两端空白；缺失数字绝不继承另一字段的设置。
  const text = value.trim();
  if (!/^\d+(?:\.\d+)?$/.test(text)) throw new MediaDeliveryDraftError(field, "number");
  // The validated decimal grammar permits only one optional fractional component, never exponent notation.
  // 已校验十进制语法只允许一个可选小数部分，绝不允许指数记法。
  const [wholeText, fractionText = ""] = text.split(".");
  // Every integer byte expressed in KiB has at most ten non-trailing decimal places because 1024 is 2^10.
  // 因为1024等于2^10，每个整数字节的KiB表示最多有十位非末尾零的小数。
  // Reject finer decimals before Number conversion, which would silently round long user input to an integer.
  // 在Number转换前拒绝更细小数，防止长用户输入被静默舍入为整数。
  const fraction = fractionText.replace(/0+$/, "");
  if (fraction.length > 10) throw new MediaDeliveryDraftError(field, "fraction");
  // Fixed-scale integer arithmetic stays below 2^53 and proves divisibility without floating point rounding.
  // 固定刻度整数运算保持在2^53以下，无需浮点舍入即可证明整除性。
  const scaledFractionBytes = Number(fraction.padEnd(10, "0")) * 1024;
  if (scaledFractionBytes % 10_000_000_000 !== 0) throw new MediaDeliveryDraftError(field, "fraction");
  // The backend ceiling bounds the whole part; very large integer input is rejected instead of normalized.
  // 后端上限约束整数部分；过大整数输入会被拒绝，而非规范化。
  const bytes = Number(wholeText) * 1024 + scaledFractionBytes / 10_000_000_000;
  if (!Number.isSafeInteger(bytes) || bytes > maximum) throw new MediaDeliveryDraftError(field, "limit");
  return bytes;
}

// parsePurpose validates one independent draft against settings and returns the exact backend shape.
// parsePurpose 根据 settings 校验一个独立草稿，返回精确后端结构。
function parsePurpose(draft: MediaDeliveryPurposeDraft, settings: PublicAiMediaDeliverySettingsSnapshot, purpose: MediaDeliveryPurpose): PublicAiMediaPurposePolicy {
  // thresholds uses the same server ceiling for both availability cases, without merging values.
  // thresholds 对两个可用性分支使用相同服务器上限，但不合并值。
  const thresholds = (kind: MediaDeliveryKind): PublicAiMediaInlineThresholds => ({
    with_upload_max_bytes: parseKiB(draft[kind].with_upload_max_bytes, settings.maxInlineRawBytes, `${purpose}-${kind}-with_upload_max_bytes`),
    without_upload_max_bytes: parseKiB(draft[kind].without_upload_max_bytes, settings.maxInlineRawBytes, `${purpose}-${kind}-without_upload_max_bytes`),
  });
  return {
    image: thresholds("image"), audio: thresholds("audio"), video: thresholds("video"), file: thresholds("file"),
    max_total_encoded_bytes: parseKiB(draft.max_total_encoded_bytes, settings.maxTotalEncodedBytes, `${purpose}-max_total_encoded_bytes`),
  };
}

// parseMediaDeliveryDraft validates every field and returns one complete policy with the server-owned version.
// parseMediaDeliveryDraft 校验每个字段，返回保留服务器版本的一份完整策略。
// Throws MediaDeliveryDraftError for invalid input; no partial policy is submitted.
// 输入无效时抛出 MediaDeliveryDraftError，不提交部分策略。
export function parseMediaDeliveryDraft(draft: MediaDeliveryPolicyDraft, settings: PublicAiMediaDeliverySettingsSnapshot): PublicAiMediaDeliveryPolicy {
  return {
    version: settings.policy.version,
    conversation: parsePurpose(draft.conversation, settings, "conversation"),
    independent_extension: parsePurpose(draft.independent_extension, settings, "independent_extension"),
  };
}
