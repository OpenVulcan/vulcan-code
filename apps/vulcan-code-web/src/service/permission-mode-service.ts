import { serviceClient } from "@/service/webui-service-runtime";
import type { PermissionMode } from "@/service/permission-mode";

/** Durable receipt of one session permission mode write.
 * 一次会话权限模式写入的可靠回执。 */
export interface SessionPermissionModeUpdateResponse {
  /** Exact owner of the receipt.
   * 回执的精确归属。 */
  session_id: string;
  /** Durably published document revision.
   * 可靠发布的文档修订。 */
  revision: number;
  /** Authoritative mode after the write.
   * 写入后的权威模式。 */
  permission_mode: PermissionMode;
}

/** Save the permission mode of an existing regular session; the next tool call reads it.
 * 保存已有常规会话的权限模式；下一次工具调用即读取该值。 */
export function setSessionPermissionMode(
  sessionId: string, mode: PermissionMode,
): Promise<SessionPermissionModeUpdateResponse> {
  return serviceClient.request("service_set_session_permission_mode", {
    request: { sessionId, mode },
  });
}
