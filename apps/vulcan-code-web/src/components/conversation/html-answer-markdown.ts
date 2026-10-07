import type { ExtraProps } from "react-markdown";
import type { HtmlAnswerMessageContext } from "./html-answer-context";

/** One existing image normalization edit, measured in original UTF-16 coordinates.
 * 一次既有图片规范化编辑，采用原始 UTF-16 坐标。 */
export interface MarkdownSourceEdit {
  /** Replaced original interval and replacement length.
   * 被替换的原始区间及替换长度。 */
  start: number;
  end: number;
  length: number;
}

/** Retains Core authority while the established Markdown normalizer changes source offsets.
 * 既有 Markdown 规范化器改变源码偏移时保留 Core 权威。 */
export interface HtmlAnswerMarkdownScope {
  /** Exact event context, original body and image-normalization edits.
   * 精确事件上下文、原始正文及图片规范化编辑。 */
  message: HtmlAnswerMessageContext;
  original: string;
  edits: readonly MarkdownSourceEdit[];
  /** Streaming fragments can only produce nonexecuting placeholders.
   * 流式片段只能产生不执行的占位卡。 */
  streaming: boolean;
}

/** Reverse an established normalization coordinate; inserted text maps to its source interval.
 * 反向映射既有规范化坐标；插入文本映射到其原始区间。
 * Input is a rendered UTF-16 boundary and ordered edits; return its original boundary.
 * 输入为渲染后的 UTF-16 边界及有序编辑；返回原始边界。 */
export function originalMarkdownOffset(offset: number, edits: readonly MarkdownSourceEdit[]): number {
  let delta = 0;
  for (const edit of edits) {
    const start = edit.start + delta;
    if (offset < start) break;
    if (offset < start + edit.length) return edit.start;
    delta += edit.length - (edit.end - edit.start);
  }
  return offset - delta;
}

/** Recognize only a streaming display hint at an existing block boundary, never an execution identity.
 * 仅在既有块边界识别流式展示提示，绝不产生执行身份。
 * Inspect the bounded opening line so a large raw tail does not need full Markdown parsing.
 * 仅检查有界起始行，使大段原始尾部无需执行全文 Markdown 解析。 */
export function hasStreamingHtmlAnswerHint(block: string): boolean {
  return /^(?:\r?\n)* {0,3}`{3,}vulcan-html(?:[ \t]|$)/u.test(block.slice(0, STREAMING_HTML_HINT_CHARS));
}

// A display hint needs only the marker prefix; metadata validation remains exclusively in Core.
// 展示提示只需标记前缀；元信息校验仍仅属于 Core。
const STREAMING_HTML_HINT_CHARS = 128;

/** Annotate top-level code nodes in the existing sanitized AST; this grants no execution authority.
 * 在既有已清理 AST 中标注顶层代码节点；此标注不授予执行权威。
 * Returns a rehype transformer; quoted/list/nested examples remain normal code.
 * 返回 rehype 转换器；引用、列表及嵌套示例保留普通代码展示。 */
export function rehypeHtmlAnswerCandidates() {
  return (tree: { children: readonly { type: string }[] }): void => {
    for (const child of tree.children) {
      if (child.type !== "element") continue;
      // ReactMarkdown exposes the same HAST Element type used by its rehype pipeline.
      // ReactMarkdown 暴露的 HAST Element 类型与其 rehype 管线一致。
      const node = child as NonNullable<ExtraProps["node"]>;
      if (node.tagName !== "pre") continue;
      const code = node.children[0];
      if (code?.type !== "element" || code.tagName !== "code") continue;
      const classes = code.properties.className;
      if (Array.isArray(classes) && classes.includes("language-vulcan-html")) {
        node.properties.dataHtmlAnswerCandidate = true;
      }
    }
  };
}
