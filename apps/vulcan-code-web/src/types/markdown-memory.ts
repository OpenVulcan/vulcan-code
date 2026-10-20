// MemoryMode selects the one long-term memory system used by every project.
// MemoryMode 选择所有项目共用的唯一长期记忆系统。
export type MemoryMode = "off" | "vmm" | "markdown";

// MemoryInjectionBudget is the token budget of each injected Markdown memory part.
// MemoryInjectionBudget 是 Markdown 记忆各注入部分的 Token 预算。
export interface MemoryInjectionBudget {
  // User profile, in full.
  // 用户画像（全文）。
  user: number;
  // Space base section, in full.
  // 空间基础区（全文）。
  spaceBase: number;
  // Project base section, in full.
  // 项目基础区（全文）。
  projectBase: number;
  // Space catalog, titles only.
  // 空间目录（仅标题）。
  spaceCatalog: number;
  // Project catalog, titles only.
  // 项目目录（仅标题）。
  projectCatalog: number;
}

// MemoryBudgetPart is one key of MemoryInjectionBudget, also used by usage rows.
// MemoryBudgetPart 是 MemoryInjectionBudget 的一个键，也用于占用行。
export type MemoryBudgetPart = keyof MemoryInjectionBudget;

// MemorySettingsView is the saved memory settings plus the limits the editor needs.
// MemorySettingsView 是已保存的记忆设置及编辑器所需上限。
export interface MemorySettingsView {
  // Global memory mode.
  // 全局记忆模式。
  mode: MemoryMode;
  // Injection budget.
  // 注入预算。
  budget: MemoryInjectionBudget;
  // Default budget restored by the reset action.
  // 重置操作恢复的默认预算。
  defaultBudget: MemoryInjectionBudget;
  // Largest accepted value of one budget part.
  // 单个预算分项可接受的最大值。
  maxPartTokens: number;
  // Usage percentage of a budget part from which pages and write receipts warn.
  // 预算分项达到该用量百分比时，页面与写入回执发出提醒。
  warningPercent: number;
}

// MarkdownMemoryScope is a stable scope key.
// MarkdownMemoryScope 是稳定层键。
export type MarkdownMemoryScope = "user" | "space" | "project";

// MarkdownMemoryNode is one memory node in document order.
// MarkdownMemoryNode 是按文档顺序排列的一个记忆节点。
export interface MarkdownMemoryNode {
  // Stable or path-derived id.
  // 稳定标识或路径派生标识。
  id: string;
  // Heading level, 1 to 4.
  // 标题级别，1 至 4。
  level: number;
  // Title path from the section down to the node.
  // 从分区到本节点的标题路径。
  path: string[];
  // Section key of the top-level ancestor.
  // 顶层祖先的分区键。
  section: string | null;
  // Revision required to change the node.
  // 修改节点所需的修订。
  revision: string;
  // Own Markdown body.
  // 自身 Markdown 正文。
  body: string;
  // Creation time, `YYYY-MM-DDTHH:MMZ`.
  // 创建时间。
  created: string | null;
  // Last update time, `YYYY-MM-DDTHH:MMZ`.
  // 最近更新时间。
  updated: string | null;
  // Conversation that last wrote the node.
  // 最近写入该节点的会话。
  session: string | null;
  // Last writer: `ai` or `user`.
  // 最近写入方：`ai` 或 `user`。
  source: "ai" | "user" | null;
  // Number of nodes below.
  // 其下节点数。
  descendants: number;
}

// MarkdownMemoryUsage is the usage of one injected part.
// MarkdownMemoryUsage 是单个注入部分的占用。
export interface MarkdownMemoryUsage {
  // Budget part.
  // 预算分项。
  part: MemoryBudgetPart;
  // Estimated tokens of the full part.
  // 完整分项的估算 Token。
  tokens: number;
  // Budget of the part.
  // 分项预算。
  budget: number;
  // Titles left out of the injection.
  // 被排除在注入之外的标题。
  omitted: string[];
}

// MarkdownMemoryProject is one memory project of the registry.
// MarkdownMemoryProject 是注册表中的一个记忆项目。
export interface MarkdownMemoryProject {
  // Memory project id.
  // 记忆项目标识。
  id: string;
  // Display name.
  // 显示名。
  name: string;
  // Bound host project ids.
  // 已绑定的宿主项目标识。
  hostProjectIds: string[];
  // Root paths seen for the project.
  // 该项目出现过的根路径。
  paths: string[];
  // Whether memory is on for the project.
  // 该项目是否开启记忆。
  memoryEnabled: boolean;
  // Whether no recorded path exists any more.
  // 是否已不存在任何记录路径。
  orphaned: boolean;
  // Last update time.
  // 最近更新时间。
  updated: string;
}

// MarkdownMemoryOrphan is an orphaned project suggested for the current host project.
// MarkdownMemoryOrphan 是为当前宿主项目建议的孤立项目。
export interface MarkdownMemoryOrphan {
  // Memory project id.
  // 记忆项目标识。
  memoryProjectId: string;
  // Stable reason keys.
  // 稳定原因键。
  reasons: Array<"same_directory_name" | "same_first_commit">;
}

// MarkdownMemorySection is a preset section created with a new scope file.
// MarkdownMemorySection 是新建层文件时创建的预置分区。
export interface MarkdownMemorySection {
  // Scope key.
  // 层键。
  scope: MarkdownMemoryScope;
  // Stable section key.
  // 稳定分区键。
  key: string;
  // Default title written into the file.
  // 写入文件的默认标题。
  title: string;
}

// MarkdownMemoryOverview is everything the Markdown memory page shows.
// MarkdownMemoryOverview 是 Markdown 记忆页面显示的全部内容。
export interface MarkdownMemoryOverview {
  // Saved settings.
  // 已保存设置。
  settings: MemorySettingsView;
  // User scope nodes.
  // 用户层节点。
  user: MarkdownMemoryNode[];
  // Space scope nodes.
  // 空间层节点。
  space: MarkdownMemoryNode[];
  // Project scope nodes of the shown memory project.
  // 所显示记忆项目的项目层节点。
  project: MarkdownMemoryNode[];
  // Memory project shown in the project tab.
  // 项目页签显示的记忆项目。
  memoryProjectId: string | null;
  // Memory project bound to the host project.
  // 绑定到宿主项目的记忆项目。
  boundMemoryProjectId: string | null;
  // Host project of the page.
  // 页面所属宿主项目。
  hostProjectId: string | null;
  // Orphans suggested for linking.
  // 建议关联的孤立项目。
  orphanCandidates: MarkdownMemoryOrphan[];
  // Every memory project.
  // 全部记忆项目。
  projects: MarkdownMemoryProject[];
  // Injection usage of the shown scopes.
  // 所显示各层的注入占用。
  usage: MarkdownMemoryUsage[];
  // Preset sections of every scope.
  // 各层预置分区。
  presetSections: MarkdownMemorySection[];
}

// MarkdownMemoryTarget selects the host project and optional explicit memory project.
// MarkdownMemoryTarget 选择宿主项目及可选的显式记忆项目。
export interface MarkdownMemoryTarget {
  // Host project of the page.
  // 页面所属宿主项目。
  projectId?: string | null;
  // Explicit memory project, such as an orphan.
  // 显式记忆项目，例如孤立项目。
  memoryProjectId?: string | null;
}

// MarkdownMemoryOperation mirrors one `memory_write` operation.
// MarkdownMemoryOperation 对应一个 `memory_write` 操作。
export type MarkdownMemoryOperation =
  | {
      action: "create";
      title: string;
      body?: string;
      scope: MarkdownMemoryScope;
      parentId?: string;
      section?: string;
      newSection?: boolean;
    }
  | { action: "replace"; id: string; expectedRevision: string; body: string }
  | { action: "rename"; id: string; expectedRevision: string; title: string }
  | {
      action: "move";
      id: string;
      expectedRevision: string;
      scope?: MarkdownMemoryScope;
      parentId?: string;
      section?: string;
      newSection?: boolean;
    }
  | { action: "delete"; id: string; expectedRevision: string; withChildren?: boolean };

// MarkdownMemoryFile is the exact text of one memory file.
// MarkdownMemoryFile 是单个记忆文件的精确文本。
export interface MarkdownMemoryFile {
  // File path.
  // 文件路径。
  path: string;
  // Exact text; empty when the file does not exist yet.
  // 精确文本；文件尚不存在时为空。
  text: string;
}

// MarkdownMemoryProjectAction is one change to the memory project registry.
// MarkdownMemoryProjectAction 是对记忆项目注册表的一项变更。
export type MarkdownMemoryProjectAction =
  | { action: "link"; projectId: string; memoryProjectId: string }
  | { action: "create"; projectId: string }
  | { action: "ignore"; projectId: string }
  | { action: "set_enabled"; memoryProjectId: string; enabled: boolean }
  | { action: "rename"; memoryProjectId: string; name: string };
