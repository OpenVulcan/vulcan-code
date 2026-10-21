/** Returns safe display text for the host-owned native-media envelope in value, or undefined for ordinary output.
 * 返回 value 中宿主持有原生媒体信封的安全展示文字；普通输出返回 undefined。
 * Binary content and provider references remain available to transport replay rather than the conversation body.
 * 二进制内容及供应商引用保留供传输重放使用，不进入会话正文。 */
export function nativeMediaInputPreview(value: unknown): string | undefined {
  // All media presentation readers share the exact versioned envelope boundary.
  // 所有媒体展示读取器共用精确的版本化信封边界。
  const result = nativeMediaInputEnvelope(value);
  if (!result) return undefined;
  return result.$vulcan_native_media_input === 1 && typeof result.text === "string"
    ? result.text : "The media read result has an invalid format and cannot be displayed.";
}

/** Decodes host-marked `value` once; returns its exact envelope or undefined for unrelated output.
 * 对宿主标记的`value`解码一次；返回精确信封，无关输出返回undefined。
 */
function nativeMediaInputEnvelope(value: unknown): Record<string, unknown> | undefined {
  // Decode only the canonical model-output string, without probing alternate payload fields.
  // 仅解码规范模型输出字符串，不探测其他载荷字段。
  let envelope: unknown = value;
  if (typeof value === "string") {
    try { envelope = JSON.parse(value); } catch { return undefined; }
  }
  if (envelope === null || typeof envelope !== "object" || Array.isArray(envelope)
    || !Object.hasOwn(envelope, "$vulcan_native_media_input")) return undefined;
  return envelope as Record<string, unknown>;
}
