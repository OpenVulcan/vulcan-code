import type { WorkbenchDeltaEnvelope } from "@/types";

// workbenchEnvelopeRequiresReactProjection returns whether one event changes structure beyond live body text.
// workbenchEnvelopeRequiresReactProjection 返回单条事件是否改变实时正文之外的结构。
//
// Parameters: envelope is one generation-ordered backend workbench event.
// 参数：envelope 是一条已按 generation 排序的后端工作台事件。
// Returns: true when global React workbench state must consume the event.
// 返回值：全局 React 工作台状态必须消费该事件时返回 true。
export function workbenchEnvelopeRequiresReactProjection(envelope: WorkbenchDeltaEnvelope): boolean {
  if (
    envelope.delta.kind === "transientReasoningStatusChanged"
    || envelope.delta.kind === "toolRequestProgressChanged"
  ) {
    return false;
  }
  if (envelope.delta.kind !== "appendMessageBody") {
    return true;
  }
  // payload is the exact streamed message update classified by the backend lifecycle.
  // payload 是由后端生命周期分类的精确流式消息更新。
  const payload = envelope.delta.payload;
  if (payload.streamState !== "active") {
    return true;
  }
  return Boolean(payload.initialMessage)
    || typeof payload.currentBody === "string"
    || payload.bodyDelta.length === 0;
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
    const progress = envelope.delta.payload.progress;
    return progress === null || progress.state === "arguments_assembled";
  }
  if (envelope.delta.kind !== "conversationMessagesPatched") {
    return false;
  }
  // runPhase is attached only by the backend runtime-event projection, so these exact phases identify
  // request-ready, request-aborted, tool-started, and tool-finished transcript boundaries without guessing
  // from presentation metadata.
  // runPhase 仅由后端运行时事件投影附加，因此这些精确阶段能够识别请求就绪、请求中止、工具开始和
  // 工具完成的转录边界，无需根据展示元数据猜测。
  const runPhase = envelope.delta.payload.runPhase;
  return runPhase === "ToolCalling"
    || runPhase === "ToolRunning"
    || runPhase === "Requesting";
}
