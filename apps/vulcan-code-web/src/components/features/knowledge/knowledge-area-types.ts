// KnowledgeBusyOperation is the locale-independent identity of one serialized page mutation.
// KnowledgeBusyOperation 是一次串行页面变更的语言无关身份。
export type KnowledgeBusyOperation =
  | "create"
  | "settings"
  | "rename"
  | "delete"
  | "repair"
  | "rebuild"
  | "display-build"
  | "vector-build"
  | "refresh"
  | "directory-import"
  | "package-export"
  | "package-import"
  | "package-replace";

// KnowledgeSourcePickerTarget identifies whether directory browsing adds a source or remaps one.
// KnowledgeSourcePickerTarget 标识目录浏览是在新增来源还是重新映射现有来源。
export type KnowledgeSourcePickerTarget = {
  // Existing source identity to remap, or null when adding or synchronizing by path.
  // 要重新映射的现有来源身份；按路径新增或同步时为空。
  sourceId: string | null;
};
