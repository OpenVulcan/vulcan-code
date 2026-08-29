import type { ConversationDetailRef, ToolDetailPayload } from "@/components/conversation/conversation-detail";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import type { TranslationKey } from "@/i18n";
import type {
  FileAccessLineRangePresentation,
  FileChangeDiffRow,
  MarkdownToolContentKind,
  MarkdownToolPresentation,
  MarkdownToolPresentationIcon,
  MarkdownToolSourceSection,
  MarkdownToolSummaryTarget,
  ToolPresentationDisplayState,
} from "./types";
import {
  argumentsFromMetadata,
  arrayValue,
  compactHash,
  fixedToolName,
  numberValue,
  objectValue,
  stringValue,
} from "./presentation-value-readers";
import { fileChangeDiffFile } from "./file-change-diff-presentation";

// MarkdownToolSummaryKind selects the exact argument contract used by one collapsed summary.
// MarkdownToolSummaryKind 为一个折叠摘要选择精确的参数契约。
type MarkdownToolSummaryKind = "none" | "file-list" | "file-read" | "markdown-menu";

// MarkdownOutputToolConfig describes one exact tool id that should use the Markdown-output card.
// MarkdownOutputToolConfig 描述一个应使用 Markdown 输出卡片的精确工具标识。
interface MarkdownOutputToolConfig {
  // Stable id prefix used for expansion and cache keys.
  // 用于展开与缓存键的稳定标识前缀。
  idPrefix: string;
  // User-visible title translation key.
  // 用户可见标题的翻译键。
  titleKey: TranslationKey;
  // Icon slot used by the shared Markdown-output card.
  // 共享 Markdown 输出卡片使用的图标槽位。
  icon: MarkdownToolPresentationIcon;
  // Expanded content renderer selected for this exact tool id.
  // 为当前精确工具标识选择的展开内容渲染器。
  contentKind: MarkdownToolContentKind;
  // Exact argument contract used to build the collapsed target summary.
  // 用于构建折叠目标摘要的精确参数契约。
  summaryKind: MarkdownToolSummaryKind;
}

// MARKDOWN_OUTPUT_TOOL_CONFIGS is keyed by concrete tool id, never by display text.
// MARKDOWN_OUTPUT_TOOL_CONFIGS 只按明确工具标识索引，绝不按展示文本匹配。
const MARKDOWN_OUTPUT_TOOL_CONFIGS = new Map<string, MarkdownOutputToolConfig>([
  ["vulcan_file_list", {
    contentKind: "markdown",
    icon: "folder_tree",
    idPrefix: "file-list",
    summaryKind: "file-list",
    titleKey: "toolPresentation.fileList.title",
  }],
  ["vulcan_file_read", {
    contentKind: "source",
    icon: "file_text",
    idPrefix: "file-read",
    summaryKind: "file-read",
    titleKey: "toolPresentation.fileRead.title",
  }],
  ["vulcan_codekit_ast_tree", {
    contentKind: "markdown",
    icon: "ast_tree",
    idPrefix: "codekit-ast-tree",
    summaryKind: "none",
    titleKey: "toolPresentation.codekit.astTree.title",
  }],
  ["vulcan_codekit_ast_detail", {
    contentKind: "markdown",
    icon: "ast_detail",
    idPrefix: "codekit-ast-detail",
    summaryKind: "none",
    titleKey: "toolPresentation.codekit.astDetail.title",
  }],
  ["vulcan_codekit_rg", {
    contentKind: "markdown",
    icon: "search",
    idPrefix: "codekit-rg",
    summaryKind: "none",
    titleKey: "toolPresentation.codekit.rg.title",
  }],
  ["vulcan_codekit_node_source", {
    contentKind: "markdown",
    icon: "ast_detail",
    idPrefix: "codekit-node-source",
    summaryKind: "none",
    titleKey: "toolPresentation.codekit.nodeSource.title",
  }],
  ["vulcan_codekit_markdown_menu", {
    contentKind: "markdown",
    icon: "folder_tree",
    idPrefix: "codekit-markdown-menu",
    summaryKind: "markdown-menu",
    titleKey: "toolPresentation.codekit.markdownMenu.title",
  }],
]);

// hydrateMarkdownToolPresentation merges a hidden tool detail payload into an existing Markdown-output presentation.
// hydrateMarkdownToolPresentation 将隐藏工具详情载荷合并到现有 Markdown 输出展示模型。
export function hydrateMarkdownToolPresentation(
  presentation: MarkdownToolPresentation,
  payload: ToolDetailPayload | null,
): MarkdownToolPresentation {
  if (!payload?.body.trim()) {
    return presentation;
  }
  return {
    ...presentation,
    markdown: payload.body,
    sourceSections: markdownToolSourceSections(
      presentation.toolId,
      payload.body,
      presentation.id,
      presentation.summaryTargets,
    ),
  };
}

// resolveMarkdownToolPresentation recognizes only the configured concrete tool ids.
// resolveMarkdownToolPresentation 只识别已配置的明确工具标识。
export function resolveMarkdownToolPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  detailRef?: ConversationDetailRef,
): MarkdownToolPresentation | null {
  const toolId = fixedToolName(blocks);
  if (!toolId) {
    return null;
  }
  const config = MARKDOWN_OUTPUT_TOOL_CONFIGS.get(toolId);
  if (!config) {
    return null;
  }
  // markdown is the visible result snapshot available before hidden-detail hydration.
  // markdown 是隐藏详情水合前可用的可见结果快照。
  const markdown = visibleMarkdownToolOutput(blocks);
  // presentationId remains stable from request formation through terminal completion.
  // presentationId 从请求构造到终态完成始终保持稳定。
  const presentationId = markdownToolPresentationId(blocks, config.idPrefix);
  // summaryTargets are derived once from the exact invocation and reused by source parsing.
  // summaryTargets 从精确调用中仅派生一次，并由源码解析复用。
  const summaryTargets = markdownToolSummaryTargets(blocks, config.summaryKind);
  return {
    contentKind: config.contentKind,
    detailRef,
    icon: config.icon,
    id: presentationId,
    kind: "markdown_tool_output",
    markdown,
    state: displayState,
    sourceSections: markdownToolSourceSections(toolId, markdown, presentationId, summaryTargets),
    summaryTargets,
    titleKey: config.titleKey,
    toolId,
  };
}

// markdownToolSummaryTargets extracts only fields declared by the selected concrete tool contract.
// markdownToolSummaryTargets 仅提取所选具体工具契约明确声明的字段。
function markdownToolSummaryTargets(
  blocks: ToolExecutionConversationBlock[],
  summaryKind: MarkdownToolSummaryKind,
): MarkdownToolSummaryTarget[] {
  // argumentsValue is the latest normalized invocation argument object.
  // argumentsValue 是最新的规范化调用参数对象。
  const argumentsValue = markdownToolArguments(blocks);
  if (!argumentsValue || summaryKind === "none") {
    return [];
  }
  if (summaryKind === "file-read") {
    return markdownFileReadTargets(argumentsValue);
  }
  if (summaryKind === "file-list") {
    // path follows the file-list contract; PWD is authoritative only when path is omitted.
    // path 遵循文件列表契约；仅当 path 省略时 PWD 才是权威目标。
    const path = stringValue(argumentsValue.path) ?? stringValue(argumentsValue.PWD);
    return path ? [{ lineRanges: [], numbered: null, path }] : [];
  }
  // path supports LF-separated mixed file and directory targets in the Markdown-menu contract.
  // path 在 Markdown 目录契约中支持以 LF 分隔的混合文件与目录目标。
  const path = stringValue(argumentsValue.path);
  return path
    ? path.split(/\r?\n/).map((value) => value.trim()).filter(Boolean).map((value) => ({ lineRanges: [], numbered: null, path: value }))
    : [];
}

// markdownToolArguments returns the latest exact invocation argument object.
// markdownToolArguments 返回最新的精确调用参数对象。
function markdownToolArguments(
  blocks: ToolExecutionConversationBlock[],
): Record<string, unknown> | null {
  for (let index = blocks.length - 1; index >= 0; index -= 1) {
    // argumentsValue is read only from the normalized arguments boundary.
    // argumentsValue 仅从规范化 arguments 边界读取。
    const argumentsValue = objectValue(argumentsFromMetadata(blocks[index]?.rawMetadata));
    if (argumentsValue) {
      return argumentsValue;
    }
  }
  return null;
}

// markdownFileReadTargets maps the single-file or batch file-read contract without path guessing.
// markdownFileReadTargets 在不猜测路径的前提下映射单文件或批量文件读取契约。
function markdownFileReadTargets(
  argumentsValue: Record<string, unknown>,
): MarkdownToolSummaryTarget[] {
  // singlePath is the root-level file argument used by one-file reads.
  // singlePath 是单文件读取使用的根级 file 参数。
  const singlePath = stringValue(argumentsValue.file);
  // defaultNumbered follows the root file-read numbered default shared by batch items.
  // defaultNumbered 遵循批量条目共享的根级文件读取行号默认值。
  const defaultNumbered = argumentsValue.numbered !== false;
  if (singlePath) {
    return [{
      lineRanges: markdownFileReadLineRanges(argumentsValue),
      numbered: defaultNumbered,
      path: singlePath,
    }];
  }
  return arrayValue(argumentsValue.files)
    .map((value) => objectValue(value))
    .map<MarkdownToolSummaryTarget | null>((file) => {
      // path is the exact batch-item file field.
      // path 是精确的批量条目 file 字段。
      const path = stringValue(file?.file);
      return path && file
        ? {
            lineRanges: markdownFileReadLineRanges(file),
            numbered: file.numbered === false ? false : defaultNumbered,
            path,
          }
        : null;
    })
    .filter((target): target is MarkdownToolSummaryTarget => Boolean(target));
}

// markdownFileReadLineRanges normalizes structured segments or the declared legacy lines_rule.
// markdownFileReadLineRanges 规范化结构化 segments 或契约声明的旧版 lines_rule。
function markdownFileReadLineRanges(
  value: Record<string, unknown>,
): FileAccessLineRangePresentation[] {
  // segments uses the preferred {start,count} contract.
  // segments 使用首选的 {start,count} 契约。
  const segments = arrayValue(value.segments)
    .map((entry) => objectValue(entry))
    .map<FileAccessLineRangePresentation | null>((segment) => {
      // start and count are exact positive integers from the schema.
      // start 与 count 是 Schema 中精确定义的正整数。
      const start = numberValue(segment?.start);
      const count = numberValue(segment?.count);
      return start !== null && start > 0 && count !== null && count > 0
        ? { endLine: start + count - 1, startLine: start }
        : null;
    })
    .filter((range): range is FileAccessLineRangePresentation => Boolean(range));
  if (segments.length) {
    return segments;
  }
  // linesRule is the LF-separated legacy start,count contract.
  // linesRule 是以 LF 分隔的旧版 start,count 契约。
  const linesRule = stringValue(value.lines_rule);
  if (!linesRule) {
    return [];
  }
  return linesRule.split(/\r\n|\r|\n/)
    .map((line) => /^(\d+)\s*,\s*(\d+)$/.exec(line.trim()))
    .map<FileAccessLineRangePresentation | null>((match) => {
      // start and count are parsed only after the exact legacy shape matches.
      // start 与 count 仅在精确匹配旧版形状后解析。
      const start = match ? Number.parseInt(match[1], 10) : 0;
      const count = match ? Number.parseInt(match[2], 10) : 0;
      return start > 0 && count > 0
        ? { endLine: start + count - 1, startLine: start }
        : null;
    })
    .filter((range): range is FileAccessLineRangePresentation => Boolean(range));
}

// markdownToolSourceSections parses only the documented vulcan-file-read text envelope.
// markdownToolSourceSections 仅解析已声明的 vulcan-file-read 文本封套。
function markdownToolSourceSections(
  toolId: string,
  output: string,
  presentationId: string,
  summaryTargets: MarkdownToolSummaryTarget[],
): MarkdownToolSourceSection[] {
  if (toolId !== "vulcan_file_read" || !output.trim()) {
    return [];
  }
  // pendingSections accumulates exact file headers and their numbered source rows.
  // pendingSections 累积精确文件头及其带行号源码行。
  const pendingSections: Array<{
    numbered: boolean;
    path: string;
    rangeEnd: number | null;
    rows: FileChangeDiffRow[];
    nextLine: number | null;
  }> = [];
  // lines preserves the host-returned order across single and batch reads.
  // lines 保留宿主在单文件与批量读取中的返回顺序。
  const lines = output.split(/\r?\n/);
  // currentSection owns rows until the next exact file header.
  // currentSection 持有源码行，直到出现下一个精确文件头。
  let currentSection: {
    numbered: boolean;
    path: string;
    rangeEnd: number | null;
    rows: FileChangeDiffRow[];
    nextLine: number | null;
  } | null = null;
  // currentTargetIndex follows the authoritative batch item marker when one is present.
  // currentTargetIndex 在存在批量条目标记时遵循其权威索引。
  let currentTargetIndex: number | null = null;
  for (const line of lines) {
    if (
      currentSection
      && !currentSection.numbered
      && currentSection.nextLine !== null
      && currentSection.rangeEnd !== null
      && currentSection.nextLine <= currentSection.rangeEnd
    ) {
      // Unnumbered ranges consume their declared row count before protocol-marker recognition.
      // 无行号范围会先消费其声明行数，再识别协议标记。
      currentSection.rows.push({
        content: line,
        kind: "context",
        lineNumber: currentSection.nextLine,
      });
      currentSection.nextLine += 1;
      continue;
    }
    // batchMatch keeps successful sections aligned even when an earlier batch item returned an error.
    // batchMatch 即使更早的批量条目返回错误，也能保持成功源码段与请求项对齐。
    const batchMatch = /^--- File (\d+)\/\d+ ---$/.exec(line);
    if (batchMatch) {
      currentTargetIndex = Number.parseInt(batchMatch[1], 10) - 1;
      continue;
    }
    // headerMatch follows the concrete [File:... Lines:... Showing:... Segments:... Clipped:...] envelope.
    // headerMatch 遵循具体的 [File:... Lines:... Showing:... Segments:... Clipped:...] 封套。
    const headerMatch = /^\[File:(.+?) Lines:\d+ Bytes:\d+ Newline:\S+ Showing:((?:L\d+-L\d+(?:,L\d+-L\d+)*)|none) Segments:\d+ Clipped:(?:true|false)\]$/i.exec(line);
    if (headerMatch) {
      if (currentSection) {
        pendingSections.push(currentSection);
      }
      // ranges are the exact visible ranges declared by the host header.
      // ranges 是宿主文件头声明的精确可见范围。
      const ranges = fileReadHeaderRanges(headerMatch[2]);
      // firstRange initializes unnumbered output inference from the returned header.
      // firstRange 使用返回文件头初始化无行号输出的行号推导。
      const firstRange = ranges[0] ?? null;
      currentSection = {
        nextLine: firstRange?.startLine ?? null,
        numbered: summaryTargets[currentTargetIndex ?? pendingSections.length]?.numbered !== false,
        path: headerMatch[1],
        rangeEnd: firstRange?.endLine ?? null,
        rows: [],
      };
      continue;
    }
    if (!currentSection) {
      continue;
    }
    // segmentMatch advances unnumbered output to the next exact returned segment.
    // segmentMatch 将无行号输出推进到下一段精确返回范围。
    const segmentMatch = /^--- L\d+\+\d+ Showing:L(\d+)-L(\d+) Clipped:(?:true|false) ---$/.exec(line);
    if (segmentMatch) {
      currentSection.nextLine = Number.parseInt(segmentMatch[1], 10);
      currentSection.rangeEnd = Number.parseInt(segmentMatch[2], 10);
      continue;
    }
    if (currentSection.numbered) {
      // sourceMatch removes only the host-inserted separator after L<number>:.
      // sourceMatch 仅移除宿主在 L<number>: 后插入的分隔空格。
      const sourceMatch = /^L(\d+): ?(.*)$/.exec(line);
      if (!sourceMatch) {
        continue;
      }
      currentSection.rows.push({
        content: sourceMatch[2],
        kind: "context",
        lineNumber: Number.parseInt(sourceMatch[1], 10),
      });
      continue;
    }
  }
  if (currentSection) {
    pendingSections.push(currentSection);
  }
  return pendingSections.flatMap((section, index) => {
    // diffFile is a context-only source model and never carries edit counts.
    // diffFile 是纯上下文源码模型，绝不携带编辑计数。
    const diffFile = fileChangeDiffFile({
      added: 0,
      index,
      operation: "mixed",
      path: section.path,
      removed: 0,
      rows: section.rows,
    });
    return diffFile
      ? [{
          diffFile,
          id: `${presentationId}:source:${index}:${compactHash(section.path)}`,
          path: section.path,
        }]
      : [];
  });
}

// fileReadHeaderRanges parses the exact comma-separated Showing ranges from a file-read header.
// fileReadHeaderRanges 解析文件读取头中精确的逗号分隔 Showing 范围。
function fileReadHeaderRanges(
  showing: string | undefined,
): FileAccessLineRangePresentation[] {
  if (!showing || showing.toLowerCase() === "none") {
    return [];
  }
  return showing.split(",")
    .map((value) => /^L(\d+)-L(\d+)$/.exec(value))
    .map<FileAccessLineRangePresentation | null>((match) => {
      // startLine and endLine are accepted only after the exact header range shape matches.
      // startLine 与 endLine 仅在精确匹配文件头范围形状后接受。
      const startLine = match ? Number.parseInt(match[1], 10) : 0;
      const endLine = match ? Number.parseInt(match[2], 10) : 0;
      return startLine > 0 && endLine >= startLine
        ? { endLine, startLine }
        : null;
    })
    .filter((range): range is FileAccessLineRangePresentation => Boolean(range));
}

// visibleMarkdownToolOutput returns any inline output already present on visible blocks.
// visibleMarkdownToolOutput 返回可见块中已经存在的内联输出。
function visibleMarkdownToolOutput(blocks: ToolExecutionConversationBlock[]): string {
  return blocks
    .map((block) => block.body.trim())
    .find(Boolean)
    ?? "";
}

// markdownToolPresentationId builds a stable id from the fixed tool call identity.
// markdownToolPresentationId 基于固定工具调用身份构建稳定标识。
function markdownToolPresentationId(blocks: ToolExecutionConversationBlock[], prefix: string): string {
  const identity = blocks
    .map((block) => block.toolCallId ?? block.id)
    .filter(Boolean)
    .join(":");
  return `${prefix}:${compactHash(identity || prefix)}`;
}
