import type { ComposerAttachmentSummary } from "@/types";

/** Identifies one rendered attachment at `position`; project positions are view identities, never original-media ownership.
 * 标识`position`处的一个渲染附件；项目位置是视图身份，绝不是原媒体所有权。
 * Staged assets retain their opaque id; existing project occurrences remain distinct even when ResourceIds repeat.
 * 暂存资产保留不透明ID；即使ResourceId重复，既有项目出现也保持独立。
 */
export function composerAttachmentIdentity(attachment: ComposerAttachmentSummary, position: number): string | null {
  if (attachment.projectResourceId != null) {
    if (attachment.assetId != null || !Number.isSafeInteger(attachment.projectResourceId) || attachment.projectResourceId <= 0
      || !Number.isSafeInteger(position) || position < 0) return null;
    return `project-resource:${attachment.projectResourceId}:${position}`;
  }
  if (attachment.promptSource != null) return null;
  // The asset id comes from staging; no file path, digest or latest-model hint can substitute for it.
  // 资产ID来自暂存；文件路径、摘要或最近模型提示均不能替代它。
  const assetId = attachment.assetId?.trim();
  return assetId ? assetId : null;
}

/** Parses the exact Core source shape; null means unknown provenance and malformed claims fail explicitly.
 * 解析精确Core来源形状；空值表示未知来源，格式错误的声明显式失败。
 * Client actions follow Core's 256-byte UTF-8 limit and positions are nonnegative safe integers.
 * 客户端动作遵循Core的256字节UTF-8上限，位置是非负安全整数。
 */
export function parseComposerPromptSource(value: unknown): NonNullable<ComposerAttachmentSummary["promptSource"]> | null {
  if (value === null) return null;
  if (typeof value !== "object" || Array.isArray(value)) throw new Error("Prompt resource source must be an object.");
  // Only the two canonical keys are accepted; model or path claims cannot enter this identity.
  // 只接受两个规范键；模型或路径声明不能进入此身份。
  const source = value as Record<string, unknown>;
  if (Object.keys(source).length !== 2 || !("client_action_id" in source) || !("resource_position" in source)
    || typeof source.client_action_id !== "string" || source.client_action_id.length === 0 || source.client_action_id.trim() !== source.client_action_id
    || new TextEncoder().encode(source.client_action_id).byteLength > 256 || typeof source.resource_position !== "number"
    || !Number.isSafeInteger(source.resource_position) || source.resource_position < 0) {
    throw new Error("Prompt resource source has an invalid action or resource position.");
  }
  return { client_action_id: source.client_action_id, resource_position: source.resource_position };
}
