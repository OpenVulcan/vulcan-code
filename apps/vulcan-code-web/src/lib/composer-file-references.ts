// The composer writes this heading only at the start of its generated reference block.
// 输入区只在自动生成的文件引用块开头写入此标题。
const referenceHeading = "Referenced files:\n";

/** Appends the ordered, deduplicated file paths or URIs to the model-facing prompt.
 * 将按序去重的文件路径或 URI 追加到模型可见提示词。
 * `text` is the user's original text. Returns the wire prompt with the generated block at the end.
 * `text` 为用户原文。返回末尾带生成引用块的请求提示词。 */
export function appendComposerFileReferences(text: string, references: readonly string[]): string {
  return references.length
    ? [text, referenceHeading + JSON.stringify([...new Set(references)], null, 2)].filter(Boolean).join("\n\n")
    : text;
}

/** Separates an exact composer-generated trailing file block from the displayed prompt.
 * 从展示提示词中分离格式精确匹配的末尾文件引用块。
 * `prompt` is the persisted or pending wire text. Returns its body and ordered reference list.
 * `prompt` 是已持久化或待处理的请求文本。返回正文和有序引用列表。 */
export function splitComposerFileReferences(prompt: string): { text: string; references: string[] } {
  // Match the final block at a paragraph boundary, including submissions containing only references.
  // 匹配段落边界上的最后一个引用块，包括仅有引用的提交。
  const delimiter = "\n\n" + referenceHeading;
  // A prompt may mention the heading in its own body before the generated block.
  // 用户正文可能在生成引用块之前提及这一标题。
  const offset = prompt.lastIndexOf(delimiter);
  if (offset < 0 && !prompt.startsWith(referenceHeading)) return { text: prompt, references: [] };
  // Use the same canonical encoder as the writer so malformed or ordinary prose survives untouched.
  // 使用与写入方相同的规范编码，保留损坏的块及普通正文。
  const text = offset < 0 ? "" : prompt.slice(0, offset);
  try {
    // The JSON must be a nonempty list of actual locators from the composer.
    // JSON 必须是输入区提供的真实地址的非空列表。
    const values: unknown = JSON.parse(prompt.slice(offset < 0 ? referenceHeading.length : offset + delimiter.length));
    if (Array.isArray(values) && values.length && values.every((value: unknown) =>
      typeof value === "string" && /^(?:[a-z][a-z\d+.-]*:|[/\\])/i.test(value))
      && appendComposerFileReferences(text, values) === prompt) {
      return { text, references: values };
    }
  } catch {
    // This candidate is regular prompt text when the suffix cannot be parsed.
    // 后缀无法解析时，候选块仍属于普通提示词。
  }
  return { text: prompt, references: [] };
}
