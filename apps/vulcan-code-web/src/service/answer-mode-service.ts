import { serviceClient } from "@/service/webui-service-runtime";
import type { AnswerMode, AnswerModePreference } from "@/service/answer-mode";

/** Durable compare-and-set receipt; stale requests return a conflict and current authority.
 * 可靠比较交换回执；过期请求返回冲突及当前权威。 */
export interface AnswerModeUpdateResponse {
  /** Exact owner of the receipt.
   * 回执的精确归属。 */
  session_id: string;
  /** Captured history generation.
   * 捕获的历史代次。 */
  window_generation: number;
  /** Durably published document revision.
   * 可靠发布的文档修订。 */
  revision: number;
  /** Authoritative preference after the serialized operation.
   * 串行操作后的权威偏好。 */
  answer_mode: AnswerModePreference;
  /** Whether the expected preference revision was stale.
   * 预期偏好修订是否过期。 */
  conflict: boolean;
}

/** Save an existing session mode using its last observed revision; return durable authority.
 * 使用最近观察的修订保存已有会话模式；返回可靠权威状态。 */
export function setSessionAnswerMode(
  sessionId: string, mode: AnswerMode, expectedRevision: number,
): Promise<AnswerModeUpdateResponse> {
  return serviceClient.request("service_set_session_answer_mode", {
    request: { sessionId, mode, expectedRevision },
  });
}
