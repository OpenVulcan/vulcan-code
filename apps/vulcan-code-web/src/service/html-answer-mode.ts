/** Core-owned desired and prepared HTML answer state, independent from execution modes.
 * Core 持有的 HTML 回答期望与准备状态，独立于执行模式。 */
export interface HtmlAnswerMode {
  /** Desired preference for the next business request.
   * 下一业务请求的期望偏好。 */
  enabled: boolean;
  /** Independent compare-and-set revision.
   * 独立的比较交换修订。 */
  revision: number;
  /** Last frozen request preference; absent before preparation.
   * 最近冻结的请求偏好；准备前为空。 */
  prepared: { enabled: boolean; revision: number } | null;
}

/** Preference bound to one hydrated session and history generation.
 * 绑定到一个已加载会话及历史代次的偏好。 */
export interface HtmlAnswerModeOwner {
  /** Exact session identity.
   * 精确会话身份。 */
  sessionId: string;
  /** Exact history generation.
   * 精确历史代次。 */
  generation: number;
  /** Authoritative persisted preference.
   * 权威持久偏好。 */
  mode: HtmlAnswerMode;
}

/** Compare revisions within one history generation; return true only for forward progress.
 * 比较同一历史代次内的修订；仅向前推进时返回真。 */
export function isHtmlAnswerModeNewer(next: HtmlAnswerMode, current: HtmlAnswerMode): boolean {
  if (next.revision !== current.revision) return next.revision > current.revision;
  return next.prepared !== null && (current.prepared === null || next.prepared.revision > current.prepared.revision);
}
