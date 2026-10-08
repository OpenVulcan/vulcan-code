/** Session answer mode: plain Markdown, or Markdown plus native `ui-*` answer components.
 * 会话回答模式：普通 Markdown，或 Markdown 加原生 `ui-*` 回答组件。 */
export type AnswerMode = "standard" | "interactive";

/** Core-owned desired and prepared answer mode, independent from output styles and execution modes.
 * Core 持有的回答模式期望与准备状态，独立于输出风格与执行模式。 */
export interface AnswerModePreference {
  /** Desired mode for the next business request.
   * 下一业务请求的期望模式。 */
  mode: AnswerMode;
  /** Independent compare-and-set revision.
   * 独立的比较交换修订。 */
  revision: number;
  /** Last frozen request mode; absent before preparation.
   * 最近冻结的请求模式；准备前为空。 */
  prepared: { mode: AnswerMode; revision: number } | null;
}

/** Preference bound to one hydrated session and history generation.
 * 绑定到一个已加载会话及历史代次的偏好。 */
export interface AnswerModeOwner {
  /** Exact session identity.
   * 精确会话身份。 */
  sessionId: string;
  /** Exact history generation.
   * 精确历史代次。 */
  generation: number;
  /** Authoritative persisted preference.
   * 权威持久偏好。 */
  mode: AnswerModePreference;
}

/** Compare revisions within one history generation; return true only for forward progress.
 * 比较同一历史代次内的修订；仅向前推进时返回真。 */
export function isAnswerModeNewer(next: AnswerModePreference, current: AnswerModePreference): boolean {
  if (next.revision !== current.revision) return next.revision > current.revision;
  return next.prepared !== null && (current.prepared === null || next.prepared.revision > current.prepared.revision);
}

/** Whether a saved mode still waits for the next model request to take effect.
 * 已保存模式是否仍等待下一次模型请求才生效。 */
export function answerModeAwaitsRequest(mode: AnswerModePreference): boolean {
  return mode.prepared === null ? mode.mode !== "standard" : mode.prepared.revision !== mode.revision;
}
