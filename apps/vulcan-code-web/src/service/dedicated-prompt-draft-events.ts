// Wire shape of streamed dedicated prompt-draft progress, shared by the transport and the service helper.
// 专职提示词起草流式进度的线协议形状，由传输层与服务辅助函数共享。

/** Topic of streamed prompt-draft progress.
 * 提示词起草流式进度的主题。 */
export const DEDICATED_PROMPT_DRAFT_PROGRESS_TOPIC = "dedicated.prompt_draft.progress";

/** Thinking and output text produced since the previous progress event of one draft request.
 * 单个起草请求自上次进度事件以来产生的思考与输出文本。 */
export interface DedicatedPromptDraftProgressEvent {
  progressId: string;
  reasoningDelta: string;
  outputDelta: string;
}
