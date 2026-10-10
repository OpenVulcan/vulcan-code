import { serviceClient } from "@/service/webui-service-runtime";
import type { DedicatedSessionProfileV1 } from "@/service/conversation-event-window-client";

/** Durable memory-lock receipt of one dedicated session.
 * 单个专职会话的可靠记忆锁定回执。 */
export interface DedicatedMemoryLockResponse {
  /** Exact owner of the receipt.
   * 回执的精确归属。 */
  session_id: string;
  /** Captured history generation.
   * 捕获的历史代次。 */
  window_generation: number;
  /** Durably published document revision.
   * 可靠发布的文档修订。 */
  revision: number;
  /** Seq of the appended lock/unlock note; null when the state was already as requested.
   * 追加的锁定/解锁通知 seq；状态已符合请求时为 null。 */
  note_seq: number | null;
  /** Profile including the updated locked spans.
   * 包含更新后锁定区间的配置。 */
  dedicated: DedicatedSessionProfileV1;
}

/** Locks or unlocks dedicated-session memory between turns.
 * 在回合之间锁定或解锁专职会话记忆。 */
export function setDedicatedMemoryLock(
  sessionId: string, locked: boolean,
): Promise<DedicatedMemoryLockResponse> {
  return serviceClient.request("service_set_dedicated_memory_lock", {
    request: { sessionId, locked },
  });
}

/** Material for drafting or revising a dedicated base prompt on the selected model.
 * 在所选模型上起草或修订专职基础提示词的材料。 */
export interface DedicatedPromptDraftRequest {
  providerId: string;
  connectionId: string;
  modelId: string;
  /** What the assistant must do, in the user's words.
   * 用户描述的助手职责。 */
  requirement: string;
  /** Prompt to revise; omitted when drafting a new one.
   * 待修订的提示词；起草新提示词时省略。 */
  currentPrompt?: string;
  /** Additional instructions for the revision.
   * 修订时的补充说明。 */
  revisionRequest?: string;
}

/** Generated prompt shown for review before the session is created.
 * 创建会话前展示供审阅的生成提示词。 */
export interface DedicatedPromptDraftResponse {
  prompt: string;
  provider_id: string;
  model_id: string;
}

/** Drafts or revises a dedicated base prompt; nothing is written to any session.
 * 起草或修订专职基础提示词；不会写入任何会话。 */
export function draftDedicatedPrompt(
  request: DedicatedPromptDraftRequest,
): Promise<DedicatedPromptDraftResponse> {
  return serviceClient.request("service_draft_dedicated_prompt", { request });
}
