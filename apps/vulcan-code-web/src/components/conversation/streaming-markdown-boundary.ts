// StreamingMarkdownOpenStructure identifies the block structure that currently prevents a safe split.
// StreamingMarkdownOpenStructure 标识当前阻止安全切分的块结构。
export type StreamingMarkdownOpenStructure =
  | "fenced-code"
  | "directive"
  | "html"
  | "table"
  | "list"
  | "blockquote"
  | "indented-code"
  | "paragraph"
  | null;

// StreamingMarkdownLineFragment stores an incomplete line as an O(1) append-only reverse chain.
// StreamingMarkdownLineFragment 将未完成行保存为 O(1) 追加的反向链。
interface StreamingMarkdownLineFragment {
  // Text contributed by one transport batch.
  // 单个传输批次贡献的文本。
  text: string;
  // Earlier text in the same incomplete logical line.
  // 同一未完成逻辑行中的更早文本。
  previous: StreamingMarkdownLineFragment | null;
}

// StreamingMarkdownBoundaryState stores incremental line parsing state across provider deltas.
// StreamingMarkdownBoundaryState 保存跨 Provider 增量的逐行解析状态。
export interface StreamingMarkdownBoundaryState {
  // Total UTF-16 input length received by the scanner.
  // 扫描器已接收的 UTF-16 输入总长度。
  inputLength: number;
  // Reverse-linked fragments of the incomplete final line.
  // 未完成末行的反向链式片段。
  pendingLineTail: StreamingMarkdownLineFragment | null;
  // UTF-16 length of the incomplete final line without materializing it.
  // 无需物化即可获得的未完成末行 UTF-16 长度。
  pendingLineLength: number;
  // Absolute UTF-16 offset where the incomplete line begins.
  // 未完成行开始位置的绝对 UTF-16 偏移。
  pendingLineStart: number;
  // Latest absolute top-level offset safe for an independent Markdown document.
  // 最近一个可作为独立 Markdown 文档的绝对顶层安全偏移。
  lastSafeOffset: number;
  // Current open block structure.
  // 当前打开的块结构。
  openStructure: StreamingMarkdownOpenStructure;
  // Active fenced-code marker character.
  // 活跃 fenced code 围栏字符。
  fenceMarker: "`" | "~" | null;
  // Minimum closing marker length for the active fenced code block.
  // 活跃 fenced code 块要求的最小关闭围栏长度。
  fenceLength: number;
  // Container restored after a nested fenced code block closes.
  // 嵌套 fenced code 块关闭后恢复的容器。
  fenceContainer: "list" | "blockquote" | null;
  // Closing HTML tag required by a raw HTML block, when known.
  // 原始 HTML 块在已知时要求的关闭标签。
  htmlClosingToken: string | null;
  // Safe candidate delayed until a list, quote, or indented continuation is known.
  // 延迟到列表、引用或缩进续行明确后确认的安全候选偏移。
  pendingContainerBoundary: number | null;
  // End offset of the last fully consumed line.
  // 最近一条完整消费行的结束偏移。
  previousLineEnd: number;
  // Last nonblank line used to recognize GFM table delimiters.
  // 用于识别 GFM 表格分隔行的最近非空行。
  previousNonBlankLine: string;
}

// createStreamingMarkdownBoundaryState creates one empty incremental scanner.
// createStreamingMarkdownBoundaryState 创建一个空的增量扫描器。
// Returns: a zero-offset scanner with no open Markdown structure.
// 返回值：一个零偏移且没有打开 Markdown 结构的扫描器。
export function createStreamingMarkdownBoundaryState(): StreamingMarkdownBoundaryState {
  return {
    inputLength: 0,
    pendingLineTail: null,
    pendingLineLength: 0,
    pendingLineStart: 0,
    lastSafeOffset: 0,
    openStructure: null,
    fenceMarker: null,
    fenceLength: 0,
    fenceContainer: null,
    htmlClosingToken: null,
    pendingContainerBoundary: null,
    previousLineEnd: 0,
    previousNonBlankLine: "",
  };
}

// appendStreamingMarkdownBoundary incrementally scans complete lines from one provider fragment.
// appendStreamingMarkdownBoundary 增量扫描单个 Provider 片段中的完整行。
// Parameters: previous is immutable scanner state and delta is the next exact text fragment.
// 参数：previous 是不可变扫描状态，delta 是下一段精确文本片段。
// Returns: the next scanner state without reparsing completed historical lines.
// 返回值：无需重扫已完成历史行的下一份扫描状态。
export function appendStreamingMarkdownBoundary(
  previous: StreamingMarkdownBoundaryState,
  delta: string,
): StreamingMarkdownBoundaryState {
  // state is copied so callers can retain immutable render-session snapshots.
  // state 会被复制，使调用方能够保留不可变渲染会话快照。
  const state: StreamingMarkdownBoundaryState = { ...previous };
  state.inputLength = previous.inputLength + delta.length;
  // cursor scans only the new fragment; prior incomplete text remains linked until one newline arrives.
  // cursor 仅扫描新片段；先前未完成文本保持链式存储，直到换行符到达。
  let cursor = 0;
  while (cursor < delta.length) {
    // newlineIndex locates the next complete line ending.
    // newlineIndex 定位下一条完整行结尾。
    const newlineIndex = delta.indexOf("\n", cursor);
    if (newlineIndex < 0) {
      break;
    }
    // rawLine excludes LF while preserving any CR for exact offset accounting.
    // rawLine 排除 LF，同时保留 CR 以精确计算偏移。
    const fragmentLine = delta.slice(cursor, newlineIndex);
    // rawLine materializes prior fragments exactly once, only for the first completed line in this delta.
    // rawLine 仅为当前增量中的首条完成行一次性物化先前片段。
    const rawLine = cursor === 0 && previous.pendingLineTail
      ? materializePendingLine(previous.pendingLineTail) + fragmentLine
      : fragmentLine;
    // line removes one transport CR before Markdown classification.
    // line 在 Markdown 分类前移除一个传输 CR。
    const line = rawLine.endsWith("\r") ? rawLine.slice(0, -1) : rawLine;
    // lineEnd is the absolute offset immediately after the newline.
    // lineEnd 是换行符之后的绝对偏移。
    const lineEnd = previous.inputLength + newlineIndex + 1;
    processCompleteMarkdownLine(state, line, lineEnd);
    cursor = newlineIndex + 1;
  }
  const trailingText = delta.slice(cursor);
  if (cursor === 0) {
    state.pendingLineTail = appendPendingLineFragment(previous.pendingLineTail, trailingText);
    state.pendingLineLength = previous.pendingLineLength + trailingText.length;
    state.pendingLineStart = previous.pendingLineStart;
  } else {
    state.pendingLineTail = appendPendingLineFragment(null, trailingText);
    state.pendingLineLength = trailingText.length;
    state.pendingLineStart = previous.inputLength + cursor;
  }
  return state;
}

// finishStreamingMarkdownBoundary applies CommonMark EOF semantics to the final incomplete line.
// finishStreamingMarkdownBoundary 将 CommonMark EOF 语义应用到最终不完整行。
// Parameters: previous is the final active scanner state.
// 参数：previous 是最终活跃扫描状态。
// Returns: one sealed scanner whose entire input is a safe terminal boundary.
// 返回值：一份将全部输入视为安全终态边界的封口扫描状态。
export function finishStreamingMarkdownBoundary(
  previous: StreamingMarkdownBoundaryState,
): StreamingMarkdownBoundaryState {
  // state is copied because completing a stream must not mutate earlier snapshots.
  // state 会被复制，因为完成流不能修改更早快照。
  const state: StreamingMarkdownBoundaryState = { ...previous };
  if (state.pendingLineTail) {
    processCompleteMarkdownLine(
      state,
      materializePendingLine(state.pendingLineTail),
      state.inputLength,
    );
  }
  state.pendingLineTail = null;
  state.pendingLineLength = 0;
  state.pendingLineStart = state.inputLength;
  state.lastSafeOffset = state.inputLength;
  state.openStructure = null;
  state.fenceMarker = null;
  state.fenceLength = 0;
  state.fenceContainer = null;
  state.htmlClosingToken = null;
  state.pendingContainerBoundary = null;
  return state;
}

// appendPendingLineFragment adds non-empty text to one incomplete-line chain in constant time.
// appendPendingLineFragment 以常量时间向未完成行链追加非空文本。
//
// Parameters: tail is the latest existing fragment and text is the next transport suffix.
// 参数：tail 是现有最新片段，text 是下一个传输后缀。
// Returns: the new reverse-chain tail, or the unchanged tail for empty text.
// 返回值：新的反向链尾；文本为空时返回原链尾。
function appendPendingLineFragment(
  tail: StreamingMarkdownLineFragment | null,
  text: string,
): StreamingMarkdownLineFragment | null {
  return text.length > 0 ? { text, previous: tail } : tail;
}

// materializePendingLine joins one incomplete line in chronological order exactly once.
// materializePendingLine 按时间顺序一次性拼接一条未完成行。
//
// Parameters: tail is the newest fragment in the reverse-linked line.
// 参数：tail 是反向链式行中的最新片段。
// Returns: the complete logical line text without a newline terminator.
// 返回值：不含换行终止符的完整逻辑行文本。
function materializePendingLine(tail: StreamingMarkdownLineFragment): string {
  const fragments: string[] = [];
  let cursor: StreamingMarkdownLineFragment | null = tail;
  while (cursor) {
    fragments.push(cursor.text);
    cursor = cursor.previous;
  }
  fragments.reverse();
  return fragments.join("");
}

// streamingMarkdownSegmentIsSelfContained rejects reference constructs whose definitions live in later segments.
// streamingMarkdownSegmentIsSelfContained 拒绝定义位于后续分段中的引用结构。
// Parameters: content is one top-level-safe candidate for an independent Markdown root.
// 参数：content 是一个可作为独立 Markdown 根的顶层安全候选段。
// Returns: true only when future reference text cannot change this root's semantics.
// 返回值：仅当未来引用文本无法改变当前根语义时返回 true。
export function streamingMarkdownSegmentIsSelfContained(content: string): boolean {
  // referenceText excludes code contexts where bracket syntax has no Markdown reference semantics.
  // referenceText 排除括号语法不具备 Markdown 引用语义的代码上下文。
  const referenceText = markdownReferenceText(content);
  // definitions contains normalized link and footnote identifiers declared in this candidate.
  // definitions 保存当前候选段声明的正规化链接与脚注标识。
  const definitions = new Set<string>();
  // definitionPattern recognizes top-level CommonMark link and GFM footnote definitions.
  // definitionPattern 识别顶层 CommonMark 链接定义与 GFM 脚注定义。
  const definitionPattern = /^ {0,3}\[([^\]]+)\]:/gmu;
  // definitionMatch iterates exact definitions without modifying candidate content.
  // definitionMatch 遍历精确定义，同时不修改候选内容。
  let definitionMatch: RegExpExecArray | null;
  while ((definitionMatch = definitionPattern.exec(referenceText)) !== null) {
    definitions.add(normalizeReferenceIdentifier(definitionMatch[1] ?? ""));
  }
  if (definitions.size > 0) {
    // Definitions can be referenced by future text, so freezing them in an earlier Markdown root is never stable.
    // 定义可能被未来文本引用，因此不能稳定冻结在更早的 Markdown 根节点中。
    return false;
  }
  // referencePattern recognizes explicit and collapsed reference-style links.
  // referencePattern 识别显式和折叠式引用链接。
  const referencePattern = /\[([^\]]+)\]\[([^\]]*)\]/gu;
  // referenceMatch validates every reference against definitions in the same segment.
  // referenceMatch 根据同段定义验证每个引用。
  let referenceMatch: RegExpExecArray | null;
  while ((referenceMatch = referencePattern.exec(referenceText)) !== null) {
    // identifier uses visible link text for collapsed references.
    // identifier 对折叠引用使用可见链接文本。
    const identifier = normalizeReferenceIdentifier(referenceMatch[2] || referenceMatch[1] || "");
    if (!definitions.has(identifier)) {
      return false;
    }
  }
  // footnotePattern recognizes GFM footnote references but excludes their definition prefix.
  // footnotePattern 识别 GFM 脚注引用，同时排除定义前缀。
  const footnotePattern = /\[\^([^\]]+)\](?!:)/gu;
  // footnoteMatch validates every footnote against a same-segment definition.
  // footnoteMatch 根据同段定义验证每个脚注。
  let footnoteMatch: RegExpExecArray | null;
  while ((footnoteMatch = footnotePattern.exec(referenceText)) !== null) {
    if (!definitions.has(normalizeReferenceIdentifier(`^${footnoteMatch[1] ?? ""}`))) {
      return false;
    }
  }
  return true;
}

// markdownReferenceText removes fenced, indented, and inline code before low-frequency reference validation.
// markdownReferenceText 在低频引用校验前移除 fenced、缩进与行内代码。
//
// Parameters: content is one complete top-level-safe Markdown segment candidate.
// 参数：content 是一个完整且位于顶层安全边界的 Markdown 候选段。
// Returns: text whose remaining brackets can carry Markdown reference semantics.
// 返回值：剩余括号可能具有 Markdown 引用语义的文本。
function markdownReferenceText(content: string): string {
  let fenceMarker: "`" | "~" | null = null;
  let fenceLength = 0;
  let fenceContainer: "list" | "blockquote" | null = null;
  const visibleLines: string[] = [];
  for (const line of content.split("\n")) {
    if (fenceMarker) {
      if (isClosingFence(line, fenceMarker, fenceLength, fenceContainer)) {
        fenceMarker = null;
        fenceLength = 0;
        fenceContainer = null;
      }
      visibleLines.push("");
      continue;
    }
    const listFenceMatch = /^(?: {2,}|\t)(`{3,}|~{3,})/u.exec(line);
    const quoteFenceMatch = /^ {0,3}>\s?(`{3,}|~{3,})/u.exec(line);
    const fenceMatch = listFenceMatch
      ?? quoteFenceMatch
      ?? /^ {0,3}(`{3,}|~{3,})/u.exec(line);
    if (fenceMatch) {
      const markerToken = fenceMatch[1] ?? "```";
      fenceMarker = markerToken[0] === "~" ? "~" : "`";
      fenceLength = markerToken.length;
      fenceContainer = listFenceMatch ? "list" : quoteFenceMatch ? "blockquote" : null;
      visibleLines.push("");
      continue;
    }
    if (/^(?: {4}|\t)/u.test(line)) {
      visibleLines.push("");
      continue;
    }
    visibleLines.push(line.replace(/(`+)([\s\S]*?)\1/gu, ""));
  }
  return visibleLines.join("\n");
}

// processCompleteMarkdownLine advances block state for one complete logical line.
// processCompleteMarkdownLine 为一条完整逻辑行推进块状态。
// Parameters: state is mutable scanner state, line excludes LF, and lineEnd is the absolute end offset.
// 参数：state 是可变扫描状态，line 不含 LF，lineEnd 是绝对结束偏移。
// Returns: nothing; structural fields are updated in place.
// 返回值：无；结构字段会原地更新。
function processCompleteMarkdownLine(
  state: StreamingMarkdownBoundaryState,
  line: string,
  lineEnd: number,
): void {
  // trimmed classifies structural markers without changing absolute offsets.
  // trimmed 对结构标记进行分类，同时不改变绝对偏移。
  const trimmed = line.trim();
  if (state.openStructure === "fenced-code") {
    if (isClosingFence(line, state.fenceMarker, state.fenceLength, state.fenceContainer)) {
      state.openStructure = state.fenceContainer;
      state.fenceMarker = null;
      state.fenceLength = 0;
      if (state.fenceContainer) {
        state.pendingContainerBoundary = lineEnd;
      } else {
        state.lastSafeOffset = lineEnd;
      }
      state.fenceContainer = null;
    }
    finishConsumedLine(state, line, lineEnd);
    return;
  }
  if (state.openStructure === "directive") {
    if (/^ {0,3}:::\s*$/u.test(line)) {
      state.openStructure = null;
      state.lastSafeOffset = lineEnd;
    }
    finishConsumedLine(state, line, lineEnd);
    return;
  }
  if (state.openStructure === "html" && state.htmlClosingToken) {
    if (line.toLowerCase().includes(state.htmlClosingToken)) {
      state.openStructure = null;
      state.htmlClosingToken = null;
      state.lastSafeOffset = lineEnd;
    }
    finishConsumedLine(state, line, lineEnd);
    return;
  }
  if (state.pendingContainerBoundary !== null && trimmed.length > 0) {
    if (lineContinuesContainer(line, state.openStructure)) {
      state.pendingContainerBoundary = null;
    } else {
      state.lastSafeOffset = state.pendingContainerBoundary;
      state.pendingContainerBoundary = null;
      state.openStructure = null;
    }
  }
  if (trimmed.length === 0) {
    if (
      state.openStructure === "list"
      || state.openStructure === "blockquote"
      || state.openStructure === "indented-code"
    ) {
      state.pendingContainerBoundary = lineEnd;
    } else {
      state.lastSafeOffset = lineEnd;
      state.openStructure = null;
      state.htmlClosingToken = null;
    }
    finishConsumedLine(state, line, lineEnd);
    return;
  }
  if (state.openStructure === "table" && !looksLikeTableRow(line)) {
    state.lastSafeOffset = state.previousLineEnd;
    state.openStructure = null;
  }
  // fenceMatch recognizes CommonMark backtick and tilde opening fences.
  // fenceMatch 识别 CommonMark 反引号与波浪号开启围栏。
  const nestedFence = nestedContainerFence(line, state.openStructure);
  const fenceMatch = nestedFence?.match ?? /^ {0,3}(`{3,}|~{3,})/u.exec(line);
  if (fenceMatch) {
    // markerToken is guaranteed by the opening-fence regular expression.
    // markerToken 由开启围栏正则保证存在。
    const markerToken = fenceMatch[1] ?? "```";
    state.openStructure = "fenced-code";
    state.fenceMarker = markerToken[0] === "~" ? "~" : "`";
    state.fenceLength = markerToken.length;
    state.fenceContainer = nestedFence?.container ?? null;
    finishConsumedLine(state, line, lineEnd);
    return;
  }
  if (/^ {0,3}:::(?:\s*|\s+.*)$/u.test(line)) {
    state.openStructure = "directive";
    finishConsumedLine(state, line, lineEnd);
    return;
  }
  // htmlClosingToken selects only raw HTML blocks whose exact close is known.
  // htmlClosingToken 只选择关闭标记明确的原始 HTML 块。
  const htmlClosingToken = rawHtmlClosingToken(line);
  if (htmlClosingToken) {
    if (!line.toLowerCase().includes(htmlClosingToken)) {
      state.openStructure = "html";
      state.htmlClosingToken = htmlClosingToken;
    } else {
      state.lastSafeOffset = lineEnd;
    }
    finishConsumedLine(state, line, lineEnd);
    return;
  }
  if (looksLikeTableDelimiter(line) && state.previousNonBlankLine.includes("|")) {
    state.openStructure = "table";
    finishConsumedLine(state, line, lineEnd);
    return;
  }
  if (looksLikeTableRow(line) && state.openStructure === "table") {
    finishConsumedLine(state, line, lineEnd);
    return;
  }
  if (state.openStructure === "list" && /^(?: {2,}|\t)/u.test(line)) {
    // Indented prose, nested items, and indented code remain inside the active list container.
    // 缩进段落、嵌套条目与缩进代码继续属于当前活跃列表容器。
    finishConsumedLine(state, line, lineEnd);
    return;
  }
  if (/^ {0,3}(?:[-+*]|\d+[.)])\s+/u.test(line)) {
    state.openStructure = "list";
    finishConsumedLine(state, line, lineEnd);
    return;
  }
  if (/^ {0,3}>/u.test(line)) {
    state.openStructure = "blockquote";
    finishConsumedLine(state, line, lineEnd);
    return;
  }
  if (/^(?: {4}|\t)/u.test(line)) {
    state.openStructure = "indented-code";
    finishConsumedLine(state, line, lineEnd);
    return;
  }
  if (/^ {0,3}(?:#{1,6}\s+|(?:[-*_]\s*){3,})/u.test(line)) {
    state.openStructure = null;
    state.lastSafeOffset = lineEnd;
    finishConsumedLine(state, line, lineEnd);
    return;
  }
  if (state.openStructure === "list" || state.openStructure === "blockquote") {
    // Unmarked prose is a CommonMark lazy continuation and remains inside its current container.
    // 无标记普通文本属于 CommonMark 惰性续行，继续位于当前容器内。
    finishConsumedLine(state, line, lineEnd);
    return;
  }
  state.openStructure = "paragraph";
  finishConsumedLine(state, line, lineEnd);
}

// finishConsumedLine stores only the context required to classify the next complete line.
// finishConsumedLine 仅保存分类下一条完整行所需的上下文。
// Parameters: state is mutable context, line is consumed text, and lineEnd is its absolute end.
// 参数：state 是可变上下文，line 是已消费文本，lineEnd 是其绝对结束位置。
// Returns: nothing; previous-line context is updated in place.
// 返回值：无；前一行上下文会原地更新。
function finishConsumedLine(
  state: StreamingMarkdownBoundaryState,
  line: string,
  lineEnd: number,
): void {
  state.previousLineEnd = lineEnd;
  if (line.trim().length > 0) {
    state.previousNonBlankLine = line;
  }
}

// isClosingFence checks marker identity, minimum length, indentation, and trailing whitespace.
// isClosingFence 检查围栏字符、最小长度、缩进与尾随空白。
// Parameters: line is the candidate, marker and minimumLength define the fence, and container defines its prefix.
// 参数：line 是候选行，marker 与 minimumLength 定义围栏，container 定义其前缀。
// Returns: true only for an exact valid closing fence.
// 返回值：仅对精确有效的关闭围栏返回 true。
function isClosingFence(
  line: string,
  marker: "`" | "~" | null,
  minimumLength: number,
  container: "list" | "blockquote" | null,
): boolean {
  if (!marker) {
    return false;
  }
  // closingPattern is constructed only from the closed marker union, not arbitrary input.
  // closingPattern 仅根据封闭的围栏字符联合构造，不接受任意输入。
  const closingPattern = marker === "`"
    ? /^ {0,3}(`{3,})\s*$/u
    : /^ {0,3}(~{3,})\s*$/u;
  // match owns the exact candidate closing marker.
  // match 持有精确候选关闭围栏。
  // candidate removes only the exact container prefix accepted for the opening fence.
  // candidate 仅移除开启围栏所接受的精确容器前缀。
  const candidate = container === "list"
    ? line.replace(/^(?: {2,}|\t)/u, "")
    : container === "blockquote"
      ? line.replace(/^ {0,3}>\s?/u, "")
      : line;
  const match = closingPattern.exec(candidate);
  return Boolean(match && (match[1]?.length ?? 0) >= minimumLength);
}

// nestedContainerFence recognizes fenced code after the exact active list or quote prefix.
// nestedContainerFence 识别精确活跃列表或引用前缀之后的 fenced code。
// Parameters: line is the candidate opening line and structure is the current container.
// 参数：line 是候选开启行，structure 是当前容器。
// Returns: the exact container and regex match, or null when no nested fence opens.
// 返回值：精确容器与正则匹配；没有嵌套围栏开启时返回 null。
function nestedContainerFence(
  line: string,
  structure: StreamingMarkdownOpenStructure,
): { container: "list" | "blockquote"; match: RegExpExecArray } | null {
  if (structure === "list") {
    const match = /^(?: {2,}|\t)(`{3,}|~{3,})/u.exec(line);
    return match ? { container: "list", match } : null;
  }
  if (structure === "blockquote") {
    const match = /^ {0,3}>\s?(`{3,}|~{3,})/u.exec(line);
    return match ? { container: "blockquote", match } : null;
  }
  return null;
}

// lineContinuesContainer conservatively identifies list, quote, and indented continuations after a blank line.
// lineContinuesContainer 保守识别空行后的列表、引用与缩进续行。
// Parameters: line is the next nonblank line and structure is the pending container type.
// 参数：line 是下一条非空行，structure 是待确认容器类型。
// Returns: true when the line must remain inside the pending container.
// 返回值：当前行必须保留在待确认容器中时返回 true。
function lineContinuesContainer(line: string, structure: StreamingMarkdownOpenStructure): boolean {
  if (structure === "list") {
    return /^ {0,3}(?:[-+*]|\d+[.)])\s+/u.test(line) || /^(?: {2,}|\t)/u.test(line);
  }
  if (structure === "blockquote") {
    return /^ {0,3}>/u.test(line);
  }
  if (structure === "indented-code") {
    return /^(?: {4}|\t)/u.test(line);
  }
  return false;
}

// looksLikeTableDelimiter recognizes one GFM delimiter row without parsing inline content.
// looksLikeTableDelimiter 识别一条 GFM 分隔行，且不解析行内内容。
// Parameters: line is one complete logical line.
// 参数：line 是一条完整逻辑行。
// Returns: true when the line has GFM table-delimiter shape.
// 返回值：当前行具有 GFM 表格分隔行形态时返回 true。
function looksLikeTableDelimiter(line: string): boolean {
  return /^ {0,3}\|?\s*:?-{3,}:?\s*(?:\|\s*:?-{3,}:?\s*)+\|?\s*$/u.test(line);
}

// looksLikeTableRow conservatively recognizes a nonblank pipe-delimited table row.
// looksLikeTableRow 保守识别一条非空管道分隔表格行。
// Parameters: line is one complete logical line.
// 参数：line 是一条完整逻辑行。
// Returns: true when the active table can conservatively consume the line.
// 返回值：活跃表格可保守消费当前行时返回 true。
function looksLikeTableRow(line: string): boolean {
  return line.trim().length > 0 && line.includes("|");
}

// rawHtmlClosingToken returns the exact lowercase close marker for supported raw HTML blocks.
// rawHtmlClosingToken 返回受支持原始 HTML 块的精确小写关闭标记。
// Parameters: line is the potential opening line.
// 参数：line 是潜在开启行。
// Returns: its required closing token, or null when the line is not a supported raw block.
// 返回值：所需关闭标记；当前行不是受支持原始块时返回 null。
function rawHtmlClosingToken(line: string): string | null {
  // normalized avoids repeated case-insensitive checks for fixed tokens.
  // normalized 避免对固定标记重复执行大小写无关检查。
  const normalized = line.trimStart().toLowerCase();
  if (normalized.startsWith("<!--")) return "-->";
  if (normalized.startsWith("<?")) return "?>";
  if (normalized.startsWith("<![cdata[")) return "]]>";
  if (normalized.startsWith("<script")) return "</script>";
  if (normalized.startsWith("<style")) return "</style>";
  if (normalized.startsWith("<pre")) return "</pre>";
  if (normalized.startsWith("<textarea")) return "</textarea>";
  return null;
}

// normalizeReferenceIdentifier applies CommonMark case folding and whitespace collapse.
// normalizeReferenceIdentifier 应用 CommonMark 大小写折叠与空白合并。
// Parameters: value is one raw link or footnote identifier.
// 参数：value 是一个原始链接或脚注标识。
// Returns: the normalized identifier used for exact set membership.
// 返回值：用于精确集合判断的正规化标识。
function normalizeReferenceIdentifier(value: string): string {
  return value.trim().replace(/\s+/gu, " ").toLowerCase();
}
