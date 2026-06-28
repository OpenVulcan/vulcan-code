import type { DiffFile, DiffLine } from "../types";

// TargetDiffContent stores one target-style hunk content block.
// TargetDiffContent 保存一个目标风格 hunk 内容块。
export type TargetDiffContent =
  | {
      // Content block kind for paired old/new changes.
      // 用于旧/新配对变更的内容块类型。
      type: "change";
      // Number of addition rows in this block.
      // 当前块中的新增行数量。
      additions: number;
      // Number of deletion rows in this block.
      // 当前块中的删除行数量。
      deletions: number;
      // Start index into the file-level addition line array.
      // 指向文件级新增行数组的起始索引。
      additionLineIndex: number;
      // Start index into the file-level deletion line array.
      // 指向文件级删除行数组的起始索引。
      deletionLineIndex: number;
      // Source hunk line index for each addition row.
      // 每条新增行对应的源 hunk 行索引。
      additionSourceLineIndexes: number[];
      // Source hunk line index for each deletion row.
      // 每条删除行对应的源 hunk 行索引。
      deletionSourceLineIndexes: number[];
    }
  | {
      // Content block kind for unchanged context rows.
      // 用于未变上下文行的内容块类型。
      type: "context";
      // Number of context rows in this block.
      // 当前块中的上下文行数量。
      lines: number;
      // Start index into the file-level addition line array.
      // 指向文件级新增行数组的起始索引。
      additionLineIndex: number;
      // Start index into the file-level deletion line array.
      // 指向文件级删除行数组的起始索引。
      deletionLineIndex: number;
      // Source hunk line index for each context row.
      // 每条上下文行对应的源 hunk 行索引。
      sourceLineIndexes: number[];
    };

// TargetDiffHunk stores one target-compatible hunk model.
// TargetDiffHunk 保存一个目标兼容的 hunk 模型。
export interface TargetDiffHunk {
  // Original hunk header text.
  // 原始 hunk 头部文本。
  header: string;
  // Number of collapsed unchanged rows before this hunk.
  // 当前 hunk 前被折叠的未变行数量。
  collapsedBefore: number;
  // First split row index occupied by this hunk body.
  // 当前 hunk 正文占用的首个 split 行索引。
  splitLineStart: number;
  // Number of split rows occupied by this hunk body.
  // 当前 hunk 正文占用的 split 行数量。
  splitLineCount: number;
  // First unified row index occupied by this hunk body.
  // 当前 hunk 正文占用的首个 unified 行索引。
  unifiedLineStart: number;
  // Number of unified rows occupied by this hunk body.
  // 当前 hunk 正文占用的 unified 行数量。
  unifiedLineCount: number;
  // Hunk old-file start line.
  // hunk 旧文件起始行。
  deletionStart: number;
  // Hunk new-file start line.
  // hunk 新文件起始行。
  additionStart: number;
  // Hunk old-file line count from the hunk header.
  // hunk 头中声明的旧文件行数。
  deletionCount: number;
  // Hunk new-file line count from the hunk header.
  // hunk 头中声明的新文件行数。
  additionCount: number;
  // Number of deletion lines materialized from the body.
  // 从正文中物化出的删除行数量。
  deletionLines: number;
  // Number of addition lines materialized from the body.
  // 从正文中物化出的新增行数量。
  additionLines: number;
  // Start index into the file-level deletion line array.
  // 指向文件级删除行数组的起始索引。
  deletionLineIndex: number;
  // Start index into the file-level addition line array.
  // 指向文件级新增行数组的起始索引。
  additionLineIndex: number;
  // Grouped hunk content blocks used by target iteration.
  // 目标迭代使用的分组 hunk 内容块。
  hunkContent: TargetDiffContent[];
  // Whether deletion side ends with no newline.
  // 删除侧是否以无换行结束。
  noEOFCRDeletions?: boolean;
  // Whether addition side ends with no newline.
  // 新增侧是否以无换行结束。
  noEOFCRAdditions?: boolean;
}

// TargetDiffFile stores target-compatible counters and line arrays for one file.
// TargetDiffFile 保存单个文件的目标兼容计数与行数组。
export interface TargetDiffFile {
  // Original parsed project diff file.
  // 原始解析后的项目 diff 文件。
  source: DiffFile;
  // File display name.
  // 文件展示名称。
  name: string;
  // Previous file display name.
  // 旧文件展示名称。
  prevName?: string;
  // Target-compatible hunk models.
  // 目标兼容 hunk 模型。
  hunks: TargetDiffHunk[];
  // File-level addition line array.
  // 文件级新增行数组。
  additionLines: DiffLine[];
  // File-level deletion line array.
  // 文件级删除行数组。
  deletionLines: DiffLine[];
  // Total split visual line count.
  // split 总可视行数。
  splitLineCount: number;
  // Total unified visual line count.
  // unified 总可视行数。
  unifiedLineCount: number;
  // Whether this file payload is partial.
  // 当前文件载荷是否为局部内容。
  isPartial: boolean;
}

// TargetDiffExpandedRegion stores how many collapsed context lines are rendered.
// TargetDiffExpandedRegion 保存折叠上下文中实际渲染的行数量。
export interface TargetDiffExpandedRegion {
  // Number of lines rendered from the collapsed region start.
  // 从折叠区开头渲染的行数。
  fromStart: number;
  // Number of lines rendered from the collapsed region end.
  // 从折叠区末尾渲染的行数。
  fromEnd: number;
  // Original collapsed region size.
  // 原始折叠区大小。
  rangeSize: number;
  // Lines that remain collapsed after expansion.
  // 展开后仍保持折叠的行数。
  collapsedLines: number;
}

// TargetDiffIterationLine stores one target iterator side line reference.
// TargetDiffIterationLine 保存目标迭代器的单侧行引用。
export interface TargetDiffIterationLine {
  // Source file line number.
  // 源文件行号。
  lineNumber: number;
  // Index inside the target file-level side line array.
  // 在目标文件级单侧行数组中的索引。
  lineIndex: number;
  // Whether this line has a no-newline marker.
  // 当前行是否带无换行标记。
  noEOFCR: boolean;
  // Unified visual line index.
  // 合并视图可视行索引。
  unifiedLineIndex: number;
  // Split visual line index.
  // 分栏视图可视行索引。
  splitLineIndex: number;
  // Source line index inside the original hunk when available.
  // 可用时对应原始 hunk 内的源行索引。
  sourceLineIndex?: number;
}

// TargetDiffIterationEvent stores one callback payload from the target iterator.
// TargetDiffIterationEvent 保存目标迭代器的一次回调载荷。
export interface TargetDiffIterationEvent {
  // Event row type.
  // 事件行类型。
  type: "context" | "context-expanded" | "change";
  // Hunk index owning this row.
  // 当前行所属 hunk 索引。
  hunkIndex: number;
  // Hunk model owning this row when present.
  // 当前行所属 hunk 模型。
  hunk?: TargetDiffHunk;
  // Collapsed rows before this emitted row.
  // 当前发射行之前的折叠行数。
  collapsedBefore: number;
  // Collapsed rows after this emitted row.
  // 当前发射行之后的折叠行数。
  collapsedAfter: number;
  // Old-side line reference.
  // 旧侧行引用。
  deletionLine?: TargetDiffIterationLine;
  // New-side line reference.
  // 新侧行引用。
  additionLine?: TargetDiffIterationLine;
}

// TargetDiffRenderRange mirrors the target virtual render range object.
// TargetDiffRenderRange 对齐目标虚拟渲染范围对象。
export interface TargetDiffRenderRange {
  // First target visual line to render.
  // 需要渲染的首条目标可视行。
  startingLine: number;
  // Number of target visual lines to render.
  // 需要渲染的目标可视行数量。
  totalLines: number;
  // Pixel buffer before rendered rows.
  // 已渲染行之前的像素占位。
  bufferBefore: number;
  // Pixel buffer after rendered rows.
  // 已渲染行之后的像素占位。
  bufferAfter: number;
}

// TargetTrailingContextExpansion stores a synthetic tail separator request.
// TargetTrailingContextExpansion 保存一个合成的尾部上下文展开请求。
export interface TargetTrailingContextExpansion {
  // Hunk index that should grow downward.
  // 应该向下扩展的 hunk 索引。
  hunkIndex: number;
  // Exact hidden tail line count computed from backend source file totals.
  // 基于后端源文件总行数计算出的精确隐藏尾部行数。
  collapsedLines: number;
}

// TargetDiffVirtualMetrics mirrors the target VirtualizedFileDiff metric object.
// TargetDiffVirtualMetrics 对齐目标 VirtualizedFileDiff 的度量对象。
export interface TargetDiffVirtualMetrics {
  // Render chunk size in target visual lines.
  // 目标可视行渲染块大小。
  hunkLineCount: number;
  // Fixed line height used by the target virtualizer.
  // 目标虚拟器使用的固定行高。
  lineHeight: number;
  // File header height used by the original target component.
  // 目标原组件使用的文件头高度。
  diffHeaderHeight: number;
  // Hunk separator height.
  // hunk 分隔行高度。
  hunkSeparatorHeight: number;
  // Vertical gap between target file sections.
  // 目标文件段之间的垂直间距。
  fileGap: number;
}

// TARGET_DIFF_VIRTUAL_METRICS keeps the default values from the target bundle.
// TARGET_DIFF_VIRTUAL_METRICS 保留目标包中的默认虚拟化数值。
export const TARGET_DIFF_VIRTUAL_METRICS: TargetDiffVirtualMetrics = Object.freeze({
  hunkLineCount: 50,
  lineHeight: 20,
  diffHeaderHeight: 44,
  hunkSeparatorHeight: 32,
  fileGap: 8,
});

// TARGET_DIFF_FULL_RANGE renders the complete file through the target iterator.
// TARGET_DIFF_FULL_RANGE 通过目标迭代器渲染完整文件。
export const TARGET_DIFF_FULL_RANGE: TargetDiffRenderRange = Object.freeze({
  startingLine: 0,
  totalLines: Number.POSITIVE_INFINITY,
  bufferBefore: 0,
  bufferAfter: 0,
});
