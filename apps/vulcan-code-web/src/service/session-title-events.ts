// Wire shape of generated session-title outcomes, shared by the transport and the app toasts.
// 生成会话标题结果的线协议形状，由传输层与应用提示共享。

/** Topic of generated session-title outcomes.
 * 生成会话标题结果的主题。 */
export const SESSION_TITLE_GENERATED_TOPIC = "session.title.generated";

/** Outcome of one title request: the applied title, or a failure that is not retried.
 * 单次标题请求的结果：已应用的标题，或不再重试的失败。 */
export type SessionTitleGenerationOutcome =
  | { kind: "renamed"; title: string }
  | { kind: "failed" };

/** Title generation result of one new session and the model that ran it.
 * 单个新会话的标题生成结果及执行它的模型。 */
export interface SessionTitleGeneratedEvent {
  sessionId: string;
  model: string;
  outcome: SessionTitleGenerationOutcome;
}
