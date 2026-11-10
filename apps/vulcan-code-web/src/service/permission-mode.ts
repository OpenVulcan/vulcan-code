/** Session permission mode: which effects run without asking the user.
 * 会话权限模式：哪些效果无需询问用户即可执行。 */
export type PermissionMode = "read_only" | "workspace_write" | "auto_approve" | "full_access";

// DEFAULT_PERMISSION_MODE mirrors the service default in browser-only previews.
// DEFAULT_PERMISSION_MODE 为仅浏览器预览镜像服务默认值。
export const DEFAULT_PERMISSION_MODE: PermissionMode = "workspace_write";

/** Unattended mode of one session: no approval prompt is ever shown. Only exists under auto-approve.
 * 一个会话的无人值守模式：从不显示审批提示。只存在于自动审批模式下。 */
export interface UnattendedConfig {
  /** Deny policy the user confirmed when turning the mode on; empty means none.
   * 用户开启该模式时确认的拒绝策略；为空表示没有。 */
  deny_policy: string;
}
