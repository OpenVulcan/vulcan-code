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

// MarkdownMemoryExportRequest writes an export package of the chosen scopes into an existing directory.
// MarkdownMemoryExportRequest 将所选层的导出包写入既有目录。
export interface MarkdownMemoryExportRequest {
  // Existing native destination directory.
  // 已有原生目标目录。
  destinationDirectory: string;
  // Whether the user scope is exported.
  // 是否导出用户层。
  user: boolean;
  // Whether the space scope is exported.
  // 是否导出空间层。
  space: boolean;
  // Memory projects whose project scope is exported.
  // 导出其项目层的记忆项目。
  memoryProjectIds: string[];
}

// MarkdownMemoryExportedPackage describes a written export package.
// MarkdownMemoryExportedPackage 描述已写入的导出包。
export interface MarkdownMemoryExportedPackage {
  // Absolute package path.
  // 导出包绝对路径。
  path: string;
  // Package size in bytes.
  // 导出包字节大小。
  byteSize: number;
  // SHA-256 of the package bytes.
  // 导出包字节的 SHA-256。
  sha256: string;
}

// MarkdownMemoryPackageProject is one memory project carried by a package.
// MarkdownMemoryPackageProject 是导出包携带的单个记忆项目。
export interface MarkdownMemoryPackageProject {
  // Memory project id at export time.
  // 导出时的记忆项目标识。
  id: string;
  // Display name.
  // 显示名称。
  name: string;
  // Recorded project paths.
  // 记录的项目路径。
  paths: string[];
  // First commit of the repository, when known.
  // 仓库首个提交（如已知）。
  firstCommit?: string;
}

// MarkdownMemoryProjectTarget is where one packaged project goes.
// MarkdownMemoryProjectTarget 是单个包内项目的去向。
export type MarkdownMemoryProjectTarget =
  | { action: "link"; memoryProjectId: string }
  | { action: "create" }
  | { action: "ignore" };

// MarkdownMemoryProjectMapping pairs a packaged project with its target.
// MarkdownMemoryProjectMapping 将包内项目与其去向配对。
export interface MarkdownMemoryProjectMapping {
  // Packaged project id.
  // 包内项目标识。
  packageProjectId: string;
  // Chosen target.
  // 所选去向。
  target: MarkdownMemoryProjectTarget;
}

// MarkdownMemoryConflictChoice resolves one conflicting note.
// MarkdownMemoryConflictChoice 解决单个冲突笔记。
export type MarkdownMemoryConflictChoice = "keep_local" | "take_imported" | "keep_both";

// MarkdownMemoryConflictResolution names the conflict and its resolution.
// MarkdownMemoryConflictResolution 指明冲突及其解决方式。
export interface MarkdownMemoryConflictResolution {
  // Scope of the conflict.
  // 冲突所在层。
  scope: MarkdownMemoryScope;
  // Packaged project of a project-scope conflict.
  // 项目层冲突所属的包内项目。
  packageProjectId?: string;
  // Conflicting note id.
  // 冲突笔记标识。
  nodeId: string;
  // Chosen resolution.
  // 所选解决方式。
  choice: MarkdownMemoryConflictChoice;
}

// MarkdownMemoryProjectCandidate is a local memory project that looks like a packaged one.
// MarkdownMemoryProjectCandidate 是看起来与包内项目相同的本地记忆项目。
export interface MarkdownMemoryProjectCandidate {
  // Local memory project id.
  // 本地记忆项目标识。
  memoryProjectId: string;
  // Local display name.
  // 本地显示名称。
  name: string;
  // Stable match reasons: same_path, same_first_commit, same_directory_name.
  // 稳定的匹配原因：same_path、same_first_commit、same_directory_name。
  reasons: string[];
}

// MarkdownMemoryPackageProjectPreview is a packaged project with its local candidates.
// MarkdownMemoryPackageProjectPreview 是包内项目及其本地候选。
export interface MarkdownMemoryPackageProjectPreview {
  // Packaged project.
  // 包内项目。
  project: MarkdownMemoryPackageProject;
  // Likely local matches, best first.
  // 可能的本地匹配，最佳在前。
  candidates: MarkdownMemoryProjectCandidate[];
}

// MarkdownMemoryNodeSummary locates one note by id and title path.
// MarkdownMemoryNodeSummary 以标识与标题路径定位单个笔记。
export interface MarkdownMemoryNodeSummary {
  // Note id.
  // 笔记标识。
  id: string;
  // Titles from the section down to the note.
  // 从分区到该笔记的标题。
  path: string[];
}

// MarkdownMemoryNodeSide is one side of a conflict.
// MarkdownMemoryNodeSide 是冲突的一侧。
export interface MarkdownMemoryNodeSide {
  // Title.
  // 标题。
  title: string;
  // Own body.
  // 自身正文。
  body: string;
  // Last update time, when recorded.
  // 最后更新时间（如有记录）。
  updated: string | null;
}

// MarkdownMemoryNodeConflict is a note that differs between the local file and the package.
// MarkdownMemoryNodeConflict 是本地文件与导出包之间存在差异的笔记。
export interface MarkdownMemoryNodeConflict {
  // Note id.
  // 笔记标识。
  id: string;
  // Local title path.
  // 本地标题路径。
  path: string[];
  // Local side.
  // 本地一侧。
  local: MarkdownMemoryNodeSide;
  // Imported side.
  // 导入一侧。
  imported: MarkdownMemoryNodeSide;
  // Which side was updated later.
  // 哪一侧更新较晚。
  newer: "local" | "imported" | "unknown";
}

// MarkdownMemoryScopeMergePreview summarizes the merge of one scope.
// MarkdownMemoryScopeMergePreview 汇总单个层的合并。
export interface MarkdownMemoryScopeMergePreview {
  // Scope.
  // 层。
  scope: MarkdownMemoryScope;
  // Packaged project of a project scope.
  // 项目层所属的包内项目。
  packageProjectId: string | null;
  // Local memory project of a linked project scope.
  // 已关联项目层对应的本地记忆项目。
  memoryProjectId: string | null;
  // Notes identical on both sides.
  // 两侧相同的笔记数。
  same: number;
  // Notes only in the package, added on import.
  // 仅导出包中有、导入时添加的笔记。
  added: MarkdownMemoryNodeSummary[];
  // Notes with a local twin under another id; both are kept.
  // 本地有同路径不同标识的笔记；两者都保留。
  duplicates: MarkdownMemoryNodeSummary[];
  // Notes needing a resolution.
  // 需要解决的笔记。
  conflicts: MarkdownMemoryNodeConflict[];
  // Sections added on import.
  // 导入时新增的分区。
  newSections: string[];
}

// MarkdownMemoryImportPreview is the reviewed merge of a package.
// MarkdownMemoryImportPreview 是导出包的待审合并结果。
export interface MarkdownMemoryImportPreview {
  // SHA-256 the import must match.
  // 导入时必须匹配的 SHA-256。
  sha256: string;
  // Export time recorded by the package.
  // 导出包记录的导出时间。
  exportedAt: string;
  // Packaged projects with their candidates.
  // 包内项目及其候选。
  projects: MarkdownMemoryPackageProjectPreview[];
  // Merge previews of every mapped scope.
  // 每个已指定去向层的合并预览。
  scopes: MarkdownMemoryScopeMergePreview[];
}

// MarkdownMemoryImportReport summarizes an applied import.
// MarkdownMemoryImportReport 汇总已应用的导入。
export interface MarkdownMemoryImportReport {
  // Backup of the replaced files, when any file was replaced.
  // 被替换文件的备份目录（如有文件被替换）。
  backupDirectory: string | null;
  // Notes added.
  // 添加的笔记数。
  added: number;
  // Notes replaced by their imported side.
  // 被导入一侧替换的笔记数。
  replaced: number;
  // Notes left unchanged.
  // 保持不变的笔记数。
  unchanged: number;
  // Memory projects created.
  // 新建的记忆项目。
  createdProjects: string[];
}
