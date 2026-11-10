import { serviceClient } from "@/service/webui-service-runtime";
import type { PermissionMode, UnattendedConfig } from "@/service/permission-mode";
import type { WorkbenchSnapshot } from "@/types";

// saveDefaultPermissionMode persists mode for future sessions and returns the updated workbench.
// saveDefaultPermissionMode 为未来会话持久化 mode，并返回更新后的工作台。
export function saveDefaultPermissionMode(mode: Exclude<PermissionMode, "read_only">): Promise<WorkbenchSnapshot> {
  return serviceClient.request("service_save_default_permission_mode", { mode });
}

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
  /** Authoritative unattended mode after the write; leaving auto-approve turns it off.
   * 写入后的权威无人值守模式；离开自动审批时会被关闭。 */
  unattended: UnattendedConfig | null;
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

/** Durable receipt of one session unattended mode write.
 * 一次会话无人值守模式写入的可靠回执。 */
export interface SessionUnattendedUpdateResponse {
  /** Exact owner of the receipt.
   * 回执的精确归属。 */
  session_id: string;
  /** Durably published document revision.
   * 可靠发布的文档修订。 */
  revision: number;
  /** Authoritative unattended mode after the write; null while it is off.
   * 写入后的权威无人值守模式；关闭时为 null。 */
  unattended: UnattendedConfig | null;
}

/** Deny policy that prefills the dialog turning unattended mode on.
 * 预填开启无人值守模式对话框的拒绝策略。 */
export interface UnattendedDefault {
  /** Project default, else the system default, else empty.
   * 项目默认值，否则为系统默认值，再否则为空。 */
  denyPolicy: string;
  /** Where the text came from.
   * 文本来源。 */
  source: "project" | "system" | "none";
  /** Longest accepted policy in characters.
   * 允许的最长策略字符数。 */
  maxChars: number;
}

/** Turn unattended mode on with the confirmed policy, or off with null; the next tool call reads it.
 * 以确认的策略开启无人值守模式，或以 null 关闭；下一次工具调用即读取该值。 */
export function setSessionUnattended(
  sessionId: string, unattended: UnattendedConfig | null,
): Promise<SessionUnattendedUpdateResponse> {
  return serviceClient.request("service_set_session_unattended", {
    request: { sessionId, unattended },
  });
}

/** Read the text that prefills a session's unattended dialog; nothing is applied by reading it.
 * 读取预填会话无人值守对话框的文本；读取不会应用任何内容。 */
export function loadSessionUnattendedDefault(sessionId: string): Promise<UnattendedDefault> {
  return serviceClient.request("service_session_unattended_default", {
    request: { sessionId },
  });
}
