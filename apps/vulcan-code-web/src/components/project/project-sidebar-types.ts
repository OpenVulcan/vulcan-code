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
  | "copy-markdown"
  | "copy-workdir"
  | "copy-session-id";

// ProjectFolderStart says why the project folder picker opens and what follows the chosen folder.
// ProjectFolderStart 说明项目文件夹选择器为何打开，以及选好文件夹之后做什么。
export interface ProjectFolderStart {
  // Group that receives the project; omitted adds it without placing it in a group.
  // 接收该项目的分组；省略时不放入分组。
  groupId?: string;
  // Whether the added project immediately opens a new unsent conversation.
  // 新项目是否立即打开一个未发送的新会话。
  startConversation?: boolean;
}
