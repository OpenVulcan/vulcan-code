import type { WorkbenchDeltaEnvelope } from "@/types";

// workbenchEnvelopeRequiresReactProjection returns whether one event changes structure beyond live body text.
// workbenchEnvelopeRequiresReactProjection 返回单条事件是否改变实时正文之外的结构。
//
// Parameters: envelope is one generation-ordered backend workbench event.
// 参数：envelope 是一条已按 generation 排序的后端工作台事件。
// Returns: true when global React workbench state must consume the event.
// 返回值：全局 React 工作台状态必须消费该事件时返回 true。
export function workbenchEnvelopeRequiresReactProjection(envelope: WorkbenchDeltaEnvelope): boolean {
  if (envelope.delta.kind === "transientReasoningStatusChanged") {
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
