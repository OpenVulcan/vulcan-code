import { serviceClient } from "@/service/webui-service-runtime";
import type { HtmlAnswerMode } from "@/service/html-answer-mode";
import { resolveBrowserServiceClientId } from "@/service/browser-service-discovery";
import { BrowserServiceRpcError } from "@/service/browserTransport";
import { htmlAnswerErrorLabel } from "./html-answer-errors";
import type { TranslationKey } from "@/i18n-messages";

/** Decode this RPC's declared string-error envelope, preserving stable source/expiry/capacity categories.
 * 解码当前 RPC 声明的字符串错误信封，保留稳定的源码、过期和容量分类。 */
export function htmlAnswerRequestErrorLabel(error: unknown): TranslationKey {
  return error instanceof BrowserServiceRpcError && error.code === "service_rpc_error"
    ? htmlAnswerErrorLabel(error.message) : "htmlAnswers.cannotOpen";
}

/** Exact persisted source selected by Core; no caller-supplied HTML or filesystem path.
 * Core 选定的精确持久源码；不接受调用方提供 HTML 或磁盘路径。 */
export interface HtmlAnswerSourceIdentity {
  /** Exact owner and explicit embedded parent, null for a root session.
   * 精确归属和显式嵌入父会话；根会话为 null。 */
  sessionId: string;
  parentSessionId: string | null;
  /** History generation, event sequence and semantic source revision.
   * 历史代次、事件序号和源码语义修订。 */
  windowGeneration: number;
  eventSeq: number;
  eventRevision: number;
  /** Full Core-issued content identity.
   * Core 签发的完整内容身份。 */
  artifactId: string;
}

/** Ephemeral single-result grant; the viewer URL must never enter logs or persistent state.
 * 瞬态单结果授权；查看地址绝不能进入日志或持久状态。 */
export interface HtmlAnswerViewLink {
  /** Non-secret revocation identity.
   * 非秘密撤销标识。 */
  grantId: string;
  /** One-use fragment capability returned by the connected Core.
   * 已连接 Core 返回的单次片段能力地址。 */
  viewerUrl: string;
  /** Ticket validity in seconds, supplied by Core.
   * Core 提供的票据有效秒数。 */
  expiresInSeconds: number;
}

/** Request a fresh single-result viewer only after an explicit user action; returns its grant.
 * 仅在明确用户操作后申请新的单结果查看器；返回该授权。 */
export function createHtmlAnswerViewLink(source: HtmlAnswerSourceIdentity, locale: string): Promise<HtmlAnswerViewLink> {
  return serviceClient.request("service_create_html_answer_view_link", {
    request: { clientId: resolveBrowserServiceClientId(), source, locale },
  });
}

/** Revoke this client's ticket or active lease by grant ID; return after Core confirms removal.
 * 按授权标识撤销本客户端票据或活动租约；Core 确认移除后返回。 */
export function revokeHtmlAnswerViewLink(grantId: string): Promise<void> {
  return serviceClient.request("service_revoke_html_answer_view_link", {
    request: { clientId: resolveBrowserServiceClientId(), grantId },
  });
}

/** Durable compare-and-set receipt; stale requests return a conflict and current authority.
 * 可靠比较交换回执；过期请求返回冲突及当前权威。 */
export interface HtmlAnswerModeUpdateResponse {
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
  html_answers: HtmlAnswerMode;
  /** Whether the expected preference revision was stale.
   * 预期偏好修订是否过期。 */
  conflict: boolean;
}

/** Save an existing session preference using its last observed revision; return durable authority.
 * 使用最近观察的修订保存已有会话偏好；返回可靠权威状态。 */
export function setSessionHtmlAnswerMode(
  sessionId: string, enabled: boolean, expectedRevision: number,
): Promise<HtmlAnswerModeUpdateResponse> {
  return serviceClient.request("service_set_session_html_answer_mode", {
    request: { sessionId, enabled, expectedRevision },
  });
}
