// ProjectMenuAction names project-tree context menu commands.
// ProjectMenuAction 命名项目树右键菜单命令。
export type ProjectMenuAction = "toggle-pin" | "open-directory" | "open-terminal" | "rename" | "set-icon" | "archive-conversations" | "remove-project" | "settings";

// ConversationMenuAction names conversation-tree context menu commands.
// ConversationMenuAction 命名会话树右键菜单命令。
export type ConversationMenuAction =
  | "pin"
  | "rename"
  | "archive"
  | "open-directory"
  | "copy-title"
  | "copy-workdir"
  | "copy-session-id";
