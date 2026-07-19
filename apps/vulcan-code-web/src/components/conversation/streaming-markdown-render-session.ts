import type {
  ConversationLiveStreamBodySnapshot,
  ConversationLiveStreamDelta,
  ConversationLiveStreamRevision,
} from "@/state/conversation-live-stream-store";
import {
  appendStreamingMarkdownBoundary,
  createStreamingMarkdownBoundaryState,
  finishStreamingMarkdownBoundary,
  streamingMarkdownSegmentIsSelfContained,
  type StreamingMarkdownBoundaryState,
} from "./streaming-markdown-boundary";

// STREAMING_MARKDOWN_SEGMENT_TARGET_BYTES is the normal immutable-segment threshold requested by the UI contract.
// STREAMING_MARKDOWN_SEGMENT_TARGET_BYTES 是界面契约要求的正常不可变分段阈值。
export const STREAMING_MARKDOWN_SEGMENT_TARGET_BYTES = 2 * 1024;

// STREAMING_MARKDOWN_ACTIVE_TAIL_HARD_BYTES bounds repeated full Markdown parsing of an unclosed structure.
// STREAMING_MARKDOWN_ACTIVE_TAIL_HARD_BYTES 限制未闭合结构重复完整 Markdown 解析的规模。
export const STREAMING_MARKDOWN_ACTIVE_TAIL_HARD_BYTES = 16 * 1024;

// UTF8_ENCODER computes bounded candidate sizes without allocating one encoder per transport batch.
// UTF8_ENCODER 计算受限候选大小，避免每个传输批次重复分配编码器。
const UTF8_ENCODER = new TextEncoder();

// StreamingMarkdownSegment is one immutable Markdown document created during the current mount.
// StreamingMarkdownSegment 是当前挂载周期创建的一份不可变 Markdown 文档。
export interface StreamingMarkdownSegment {
  // Stable key derived from message identity and absolute source offsets.
  // 根据消息身份与绝对源码偏移生成的稳定键。
  key: string;
  // Absolute UTF-16 start offset in the complete streamed body.
  // 完整流式正文中的绝对 UTF-16 起始偏移。
  startOffset: number;
  // Absolute UTF-16 end offset in the complete streamed body.
  // 完整流式正文中的绝对 UTF-16 结束偏移。
  endOffset: number;
  // Immutable Markdown source text.
  // 不可变 Markdown 源文本。
  content: string;
}

// StreamingMarkdownTailPart stores raw preview text without rejoining historical open-structure content.
// StreamingMarkdownTailPart 保存原始预览文本，避免重新拼接历史未闭合结构内容。
export interface StreamingMarkdownTailPart {
  // Stable part key based on backend generation and local sequence.
  // 根据后端 generation 与局部序号生成的稳定片段键。
  key: string;
  // Exact text appended by this part.
  // 当前片段追加的精确文本。
  content: string;
  // UTF-8 byte length computed once when this bounded raw chunk changes.
  // 当前受限原始文本块变化时仅计算一次的 UTF-8 字节长度。
  utf8ByteLength: number;
}

// StreamingMarkdownRenderState owns view-only segmentation for one mounted streamed message.
// StreamingMarkdownRenderState 持有单条已挂载流式消息的纯视图分段状态。
export interface StreamingMarkdownRenderState {
  // Stable live-stream cache key.
  // 稳定实时流缓存键。
  streamKey: string;
  // Latest backend generation consumed by this render session.
  // 当前渲染会话已消费的最新后端 generation。
  cursorGeneration: number;
  // Current explicit stream lifecycle.
  // 当前显式流生命周期。
  lifecycle: "active" | "sealed";
  // Cold prefix rendered as one Markdown document for this mount.
  // 当前挂载中作为单个 Markdown 文档渲染的冷前缀。
  baseline: string;
  // Immutable hot segments completed after the cold cutover.
  // 冷切入后完成的不可变热段。
  committedSegments: readonly StreamingMarkdownSegment[];
  // Absolute UTF-16 offset where the current tail begins.
  // 当前尾部开始位置的绝对 UTF-16 偏移。
  tailStartOffset: number;
  // Current tail parts retained for raw preview and one-time completion joins.
  // 为原始预览与一次性完成拼接保留的当前尾部片段。
  tailParts: readonly StreamingMarkdownTailPart[];
  // Materialized tail only while it remains below the Markdown parsing hard limit.
  // 仅在尾部未超过 Markdown 解析硬上限时物化的尾部正文。
  markdownTail: string;
  // UTF-8 size of the current tail.
  // 当前尾部的 UTF-8 大小。
  tailUtf8ByteLength: number;
  // Whether the current open tail must use lightweight raw preview.
  // 当前未闭合尾部是否必须使用轻量原始预览。
  rawPreview: boolean;
  // Whether global reference semantics permanently prohibit independent roots for this mount.
  // 全局引用语义是否永久禁止当前挂载继续创建独立根节点。
  segmentationDisabled: boolean;
  // Incremental top-level Markdown boundary scanner.
  // 增量顶层 Markdown 边界扫描器。
  scanner: StreamingMarkdownBoundaryState;
  // Local sequence for stable raw part keys.
  // 稳定原始片段键使用的局部序号。
  nextPartSequence: number;
}

// createStreamingMarkdownRenderState creates one cold baseline and one active tail from an atomic Store read.
// createStreamingMarkdownRenderState 根据一次原子 Store 读取创建冷基线与活动尾部。
// Parameters: snapshot is one internally consistent full-body Store read.
// 参数：snapshot 是一份内部一致的 Store 全量正文读取。
// Returns: a new mount-local render session with immutable cold history.
// 返回值：一个具有不可变冷历史的新挂载局部渲染会话。
export function createStreamingMarkdownRenderState(
  snapshot: ConversationLiveStreamBodySnapshot,
): StreamingMarkdownRenderState {
  // scanner determines the final top-level safe boundary in the cold body.
  // scanner 确定冷正文中的最终顶层安全边界。
  const scanner = appendStreamingMarkdownBoundary(
    createStreamingMarkdownBoundaryState(),
    snapshot.body,
  );
  // safeOffset keeps any incomplete final structure out of the cold Markdown document.
  // safeOffset 防止任何未完成末尾结构进入冷 Markdown 文档。
  const scannerSafeOffset = snapshot.lifecycle === "sealed"
    ? snapshot.body.length
    : scanner.lastSafeOffset;
  // safeOffset keeps globally reusable reference definitions and unresolved references in one Markdown root.
  // safeOffset 将全局可复用引用定义与未解析引用保留在同一个 Markdown 根节点中。
  const coldPrefixSelfContained = snapshot.lifecycle === "sealed"
    || streamingMarkdownSegmentIsSelfContained(snapshot.body.slice(0, scannerSafeOffset));
  const safeOffset = snapshot.lifecycle === "active"
    && !coldPrefixSelfContained
    ? 0
    : scannerSafeOffset;
  // baseline is rendered once and never changes during this mount.
  // baseline 在当前挂载期间只渲染一次且绝不变化。
  const baseline = snapshot.body.slice(0, safeOffset);
  // tail is the only cold content allowed to remain dynamic.
  // tail 是冷内容中唯一允许继续动态变化的部分。
  const tail = snapshot.body.slice(safeOffset);
  // tailUtf8ByteLength is computed once for the cold suffix.
  // tailUtf8ByteLength 对冷后缀仅计算一次。
  const tailUtf8ByteLength = utf8ByteLength(tail);
  // rawPreview prevents a large incomplete cold suffix from entering ReactMarkdown.
  // rawPreview 防止大型未完成冷后缀进入 ReactMarkdown。
  const rawPreview = snapshot.lifecycle === "active"
    && tailUtf8ByteLength > STREAMING_MARKDOWN_ACTIVE_TAIL_HARD_BYTES;
  return {
    streamKey: snapshot.key,
    cursorGeneration: snapshot.lastGeneration,
    lifecycle: snapshot.lifecycle,
    baseline,
    committedSegments: [],
    tailStartOffset: safeOffset,
    tailParts: tail.length > 0
      ? [{ key: `${snapshot.lastGeneration}:0`, content: tail, utf8ByteLength: tailUtf8ByteLength }]
      : [],
    markdownTail: rawPreview ? "" : tail,
    tailUtf8ByteLength,
    rawPreview,
    segmentationDisabled: snapshot.lifecycle === "active" && !coldPrefixSelfContained,
    scanner: snapshot.lifecycle === "sealed" ? finishStreamingMarkdownBoundary(scanner) : scanner,
    nextPartSequence: 1,
  };
}

// applyStreamingMarkdownDeltas consumes ordered Store fragments without changing prior immutable segments.
// applyStreamingMarkdownDeltas 消费有序 Store 片段，且不修改先前的不可变分段。
// Parameters: previous is mounted state, deltas are ordered updates, and revision is their latest Store view.
// 参数：previous 是挂载状态，deltas 是有序更新，revision 是对应最新 Store 视图。
// Returns: the next immutable render state after append and optional sealing.
// 返回值：追加及可选封口后的下一份不可变渲染状态。
export function applyStreamingMarkdownDeltas(
  previous: StreamingMarkdownRenderState,
  deltas: readonly ConversationLiveStreamDelta[],
  revision: ConversationLiveStreamRevision,
): StreamingMarkdownRenderState {
  // pendingDeltas are coalesced once per Store notification so raw-preview arrays grow by render batches, not tokens.
  // pendingDeltas 每次 Store 通知只合并一次，使原始预览数组按渲染批次而非 token 增长。
  const pendingDeltas = deltas.filter((delta) => delta.generation > previous.cursorGeneration);
  let state = previous;
  if (pendingDeltas.length > 0) {
    state = appendStreamingMarkdownDelta(state, {
      generation: pendingDeltas.at(-1)?.generation ?? previous.cursorGeneration,
      text: pendingDeltas.map((delta) => delta.text).join(""),
    });
  }
  if (revision.lifecycle === "sealed" && state.lifecycle !== "sealed") {
    state = sealStreamingMarkdownRenderState(state, revision.lastGeneration);
  }
  return state;
}

// streamingMarkdownRenderStateBody reconstructs current view text for tests and rare reconciliation only.
// streamingMarkdownRenderStateBody 仅为测试和罕见对账重建当前视图文本。
// Parameters: state is one complete mount-local render session.
// 参数：state 是一份完整挂载局部渲染会话。
// Returns: the exact concatenated source body represented by the state.
// 返回值：当前状态表示的精确拼接源码正文。
export function streamingMarkdownRenderStateBody(state: StreamingMarkdownRenderState): string {
  return state.baseline
    + state.committedSegments.map((segment) => segment.content).join("")
    + state.tailParts.map((part) => part.content).join("");
}

// streamingMarkdownRevisionRequiresRebase identifies authoritative Store baselines not represented by the view cursor.
// streamingMarkdownRevisionRequiresRebase 识别视图游标尚未表示的权威 Store 基线。
//
// Parameters: state is the mounted render session and revision is the latest Store snapshot.
// 参数：state 是已挂载渲染会话，revision 是最新 Store 快照。
// Returns: true when the component must perform one atomic full-body read instead of delta replay.
// 返回值：组件必须执行一次原子全量正文读取而非增量重放时返回 true。
export function streamingMarkdownRevisionRequiresRebase(
  state: StreamingMarkdownRenderState,
  revision: ConversationLiveStreamRevision,
): boolean {
  return revision.key !== state.streamKey
    || revision.baseGeneration >= state.cursorGeneration;
}

// appendStreamingMarkdownDelta appends one fragment to the bounded active tail.
// appendStreamingMarkdownDelta 将一个片段追加到受限活动尾部。
// Parameters: previous is the current session and delta is one coalesced ordered append.
// 参数：previous 是当前会话，delta 是一条已合并有序追加。
// Returns: the next render state after scanning and safe-prefix commitment.
// 返回值：扫描并提交安全前缀后的下一份渲染状态。
function appendStreamingMarkdownDelta(
  previous: StreamingMarkdownRenderState,
  delta: ConversationLiveStreamDelta,
): StreamingMarkdownRenderState {
  // scanner advances only over the new fragment.
  // scanner 只扫描新增片段。
  const scanner = appendStreamingMarkdownBoundary(previous.scanner, delta.text);
  // nextPart stores the exact raw fragment for lightweight preview.
  // nextPart 为轻量预览保存精确原始片段。
  // deltaUtf8ByteLength is shared by tail accounting and bounded raw chunking.
  // deltaUtf8ByteLength 由尾部计数与受限原始文本分块共享。
  const deltaUtf8ByteLength = utf8ByteLength(delta.text);
  const nextPart: StreamingMarkdownTailPart = {
    key: `${delta.generation}:${previous.nextPartSequence}`,
    content: delta.text,
    utf8ByteLength: deltaUtf8ByteLength,
  };
  // tailUtf8ByteLength grows only by the new fragment's bytes.
  // tailUtf8ByteLength 只增加新增片段的字节数。
  const tailUtf8ByteLength = previous.tailUtf8ByteLength + deltaUtf8ByteLength;
  // rawPreview activates once and remains active until a safe segment can be sealed.
  // rawPreview 一旦激活便保持，直到安全分段能够封口。
  const rawPreview = previous.rawPreview
    || tailUtf8ByteLength > STREAMING_MARKDOWN_ACTIVE_TAIL_HARD_BYTES;
  // markdownTail copies only the bounded tail and is discarded in raw-preview mode.
  // markdownTail 只复制受限尾部，并在原始预览模式中丢弃。
  const markdownTail = rawPreview ? "" : previous.markdownTail + delta.text;
  // appended contains the complete active tail as stable parts.
  // appended 以稳定片段保存完整活动尾部。
  const appended: StreamingMarkdownRenderState = {
    ...previous,
    cursorGeneration: delta.generation,
    tailParts: appendStreamingMarkdownTailPart(previous.tailParts, nextPart),
    markdownTail,
    tailUtf8ByteLength,
    rawPreview,
    scanner,
    nextPartSequence: previous.nextPartSequence + 1,
  };
  return commitStreamingMarkdownSafePrefix(appended);
}

// appendStreamingMarkdownTailPart keeps raw-preview DOM and array growth proportional to bounded text chunks.
// appendStreamingMarkdownTailPart 使原始预览 DOM 与数组按受限文本块增长。
//
// Parameters: parts are immutable existing chunks and nextPart carries one coalesced render-batch append.
// 参数：parts 是不可变已有文本块，nextPart 携带一个已合并渲染批次的追加文本。
// Returns: an immutable chunk list whose mutable tail candidate never exceeds the Markdown hard limit.
// 返回值：不可变文本块列表，其中可变尾部候选绝不超过 Markdown 硬上限。
function appendStreamingMarkdownTailPart(
  parts: readonly StreamingMarkdownTailPart[],
  nextPart: StreamingMarkdownTailPart,
): readonly StreamingMarkdownTailPart[] {
  const previousPart = parts.at(-1);
  if (!previousPart) {
    return [nextPart];
  }
  const combinedUtf8ByteLength = previousPart.utf8ByteLength + nextPart.utf8ByteLength;
  if (combinedUtf8ByteLength > STREAMING_MARKDOWN_ACTIVE_TAIL_HARD_BYTES) {
    return [...parts, nextPart];
  }
  return [
    ...parts.slice(0, -1),
    {
      key: previousPart.key,
      content: previousPart.content + nextPart.content,
      utf8ByteLength: combinedUtf8ByteLength,
    },
  ];
}

// commitStreamingMarkdownSafePrefix freezes the latest self-contained safe prefix after the 2KB threshold.
// commitStreamingMarkdownSafePrefix 在达到 2KB 阈值后冻结最近的自包含安全前缀。
// Parameters: state contains the current scanner boundary and dynamic tail.
// 参数：state 包含当前扫描边界与动态尾部。
// Returns: state unchanged or with one new immutable segment and bounded remainder.
// 返回值：原状态，或增加一个不可变段并保留受限余量的新状态。
function commitStreamingMarkdownSafePrefix(
  state: StreamingMarkdownRenderState,
): StreamingMarkdownRenderState {
  if (state.segmentationDisabled) {
    return state;
  }
  if (state.scanner.lastSafeOffset <= state.tailStartOffset) {
    return state;
  }
  // tailText is joined only when raw mode reaches a real safe boundary; bounded mode already owns it.
  // tailText 仅在原始模式到达真实安全边界时拼接；受限模式已经持有该文本。
  const tailText = state.rawPreview
    ? state.tailParts.map((part) => part.content).join("")
    : state.markdownTail;
  // safeLength maps the absolute scanner boundary into the current tail.
  // safeLength 将绝对扫描边界映射到当前尾部。
  const safeLength = state.scanner.lastSafeOffset - state.tailStartOffset;
  if (safeLength <= 0 || safeLength > tailText.length) {
    return state;
  }
  // candidate is the exact prefix proposed as one immutable Markdown document.
  // candidate 是拟封口为单个不可变 Markdown 文档的精确前缀。
  const candidate = tailText.slice(0, safeLength);
  if (utf8ByteLength(candidate) < STREAMING_MARKDOWN_SEGMENT_TARGET_BYTES) {
    return state;
  }
  if (!streamingMarkdownSegmentIsSelfContained(candidate)) {
    // Future text cannot make an earlier independent root observe global reference definitions safely.
    // 未来文本无法让更早的独立根安全观察到全局引用定义。
    return { ...state, segmentationDisabled: true };
  }
  // endOffset is stable because committed text never moves within the raw stream.
  // endOffset 保持稳定，因为已封口文本在原始流中的位置绝不移动。
  const endOffset = state.tailStartOffset + candidate.length;
  // segment owns one immutable source string and stable source-offset key.
  // segment 持有一份不可变源码字符串与稳定源码偏移键。
  const segment: StreamingMarkdownSegment = {
    key: `${state.streamKey}:${state.tailStartOffset}:${endOffset}`,
    startOffset: state.tailStartOffset,
    endOffset,
    content: candidate,
  };
  // remainder remains the only dynamic tail after the commit.
  // remainder 在封口后成为唯一动态尾部。
  const remainder = tailText.slice(safeLength);
  // remainderUtf8ByteLength determines whether the residual structure still requires raw preview.
  // remainderUtf8ByteLength 决定剩余结构是否仍需原始预览。
  const remainderUtf8ByteLength = utf8ByteLength(remainder);
  // remainderRawPreview retains lightweight mode only when the residual tail independently exceeds the hard limit.
  // remainderRawPreview 仅在剩余尾部自身超过硬上限时保持轻量模式。
  const remainderRawPreview = remainderUtf8ByteLength > STREAMING_MARKDOWN_ACTIVE_TAIL_HARD_BYTES;
  return {
    ...state,
    committedSegments: [...state.committedSegments, segment],
    tailStartOffset: endOffset,
    tailParts: remainder.length > 0
      ? [{
          key: `${state.cursorGeneration}:${state.nextPartSequence}`,
          content: remainder,
          utf8ByteLength: remainderUtf8ByteLength,
        }]
      : [],
    markdownTail: remainderRawPreview ? "" : remainder,
    tailUtf8ByteLength: remainderUtf8ByteLength,
    rawPreview: remainderRawPreview,
    nextPartSequence: state.nextPartSequence + 1,
  };
}

// sealStreamingMarkdownRenderState applies EOF semantics and renders the final tail exactly once.
// sealStreamingMarkdownRenderState 应用 EOF 语义，并仅渲染一次最终尾部。
// Parameters: previous is active render state and generation is the terminal Store cursor.
// 参数：previous 是活跃渲染状态，generation 是终态 Store 游标。
// Returns: one sealed render state with the exact final tail materialized.
// 返回值：一份已物化精确最终尾部的封口渲染状态。
function sealStreamingMarkdownRenderState(
  previous: StreamingMarkdownRenderState,
  generation: number,
): StreamingMarkdownRenderState {
  // finalTail is joined once even when a very large open structure used raw preview while active.
  // finalTail 即使活跃期间使用原始预览，也只在结束时拼接一次大型未闭合结构。
  const finalTail = previous.rawPreview
    ? previous.tailParts.map((part) => part.content).join("")
    : previous.markdownTail;
  return {
    ...previous,
    cursorGeneration: Math.max(previous.cursorGeneration, generation),
    lifecycle: "sealed",
    markdownTail: finalTail,
    rawPreview: false,
    scanner: finishStreamingMarkdownBoundary(previous.scanner),
  };
}

// utf8ByteLength computes the exact byte threshold for one bounded candidate or fragment.
// utf8ByteLength 为单个受限候选段或片段计算精确字节阈值。
// Parameters: value is the candidate text.
// 参数：value 是候选文本。
// Returns: its exact UTF-8 byte length.
// 返回值：对应精确 UTF-8 字节长度。
function utf8ByteLength(value: string): number {
  return UTF8_ENCODER.encode(value).byteLength;
}
