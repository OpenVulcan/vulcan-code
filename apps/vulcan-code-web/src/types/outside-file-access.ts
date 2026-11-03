// OutsideWorkspaceFileAccess is how a file outside the session's workspace opens: never, after asking, or at once.
// OutsideWorkspaceFileAccess 是会话工作区之外的文件如何打开：从不、询问后或直接打开。
export type OutsideWorkspaceFileAccess = "deny" | "prompt" | "allow";

// SessionOutsideFileAccess is one session's own choice (null follows the system default), the system default and the
// value in effect.
// SessionOutsideFileAccess 是一个会话的自有选择（null 表示跟随系统默认值）、系统默认值及生效值。
export interface SessionOutsideFileAccess {
  access: OutsideWorkspaceFileAccess;
  session: OutsideWorkspaceFileAccess | null;
  system: OutsideWorkspaceFileAccess;
}
