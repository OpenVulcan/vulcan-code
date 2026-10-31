import type { WorkbenchDeltaEnvelope } from "@/types";

// mergePendingWorkbenchDeltaSignal preserves every unacknowledged structural delta across React batched updates.
// mergePendingWorkbenchDeltaSignal 在 React 批处理更新期间保留每条尚未确认的结构增量。
//
// Parameters: current is the unacknowledged signal, sequence is the next monotonic frontend id, and envelopes are newly flushed deltas.
// 参数：current 是尚未确认的信号，sequence 是下一个单调前端标识，envelopes 是新刷新的增量。
// Returns: one signal containing the complete unacknowledged envelope sequence.
// 返回值：返回包含完整未确认增量序列的单个信号。
export function mergePendingWorkbenchDeltaSignal(
  current: { seq: number; envelopes: WorkbenchDeltaEnvelope[] } | null,
  sequence: number,
  envelopes: WorkbenchDeltaEnvelope[],
): { seq: number; envelopes: WorkbenchDeltaEnvelope[] } {
  return {
    seq: sequence,
    envelopes: current ? [...current.envelopes, ...envelopes] : [...envelopes],
  };
}

// workbenchEnvelopeRequiresReactProjection returns whether one event changes structure beyond live body text.
// workbenchEnvelopeRequiresReactProjection 返回单条事件是否改变实时正文之外的结构。
//
// Parameters: envelope is one generation-ordered backend workbench event.
// 参数：envelope 是一条已按 generation 排序的后端工作台事件。
// Returns: true when global React workbench state must consume the event.
// 返回值：全局 React 工作台状态必须消费该事件时返回 true。
export function workbenchEnvelopeRequiresReactProjection(envelope: WorkbenchDeltaEnvelope): boolean {
  return envelope.delta.kind !== "transientReasoningStatusChanged"
    && envelope.delta.kind !== "toolRequestProgressChanged"
    && envelope.delta.kind !== "contextCompactionPreviewChanged"
    && envelope.delta.kind !== "providerRetryStatusChanged";
}

// workbenchEnvelopeRequiresImmediateLiveFlush identifies request lifecycle boundaries that must bypass hot-delta batching.
// workbenchEnvelopeRequiresImmediateLiveFlush 识别必须绕过热增量批处理的请求生命周期边界。
//
// Parameters: envelope is one generation-ordered backend workbench event.
// 参数：envelope 是一条按 generation 排序的后端工作台事件。
// Returns: true when the live leaf Store must notify subscribers immediately.
// 返回值：实时叶子 Store 必须立即通知订阅方时返回 true。
export function workbenchEnvelopeRequiresImmediateLiveFlush(
  envelope: WorkbenchDeltaEnvelope,
): boolean {
  if (envelope.delta.kind === "toolRequestProgressChanged") {
    // Tool request progress is already throttled by the backend coordinator. Delaying another
    // notification here can let the terminal clear erase every receiving snapshot in one batch.
    // 工具请求进度已经由后端协调器节流；若此处再次延迟通知，终态清除可能在同一批次中抹掉全部接收快照。
    return true;
  }
  return false;
}
