import type React from "react";
import * as LucideIcons from "lucide-react";
import { Folder, FolderOpen, FolderPlus, LayoutDashboard, MessageSquareText, ScrollText } from "lucide-react";
import type { ProjectIconOption } from "./project-icon-model";

// isLucideIconComponent accepts both plain components and React.forwardRef icon objects.
// isLucideIconComponent 同时接受普通组件与 React.forwardRef 图标对象。
function isLucideIconComponent(value: unknown): boolean {
  if (typeof value === "function") {
    return true;
  }
  return Boolean(value && typeof value === "object" && "render" in value && typeof (value as { render?: unknown }).render === "function");
}

// PROJECT_ICON_OPTIONS contains the Lucide picker entries owned by this deferred chunk.
// PROJECT_ICON_OPTIONS 包含由当前延迟分块独立承载的 Lucide 选择项。
export const PROJECT_ICON_OPTIONS: ProjectIconOption[] = Object.entries(LucideIcons)
  .filter(([name, value]) => /^[A-Z]/.test(name) && !name.endsWith("Icon") && name !== "Icon" && isLucideIconComponent(value))
  .map(([name, Icon]) => ({
    pack: "lucide" as const,
    key: name,
    label: name,
    Icon: Icon as React.ComponentType<{ size?: number; className?: string }>,
  }))
  .sort((left, right) => left.label.localeCompare(right.label));

// PROJECT_ICON_OPTION_MAP resolves current keys and established legacy aliases.
// PROJECT_ICON_OPTION_MAP 同时解析当前图标键与既有旧版别名。
export const PROJECT_ICON_OPTION_MAP = new Map<string, ProjectIconOption>([
  ...PROJECT_ICON_OPTIONS.map((option) => [option.key, option] as const),
  ["folder", { pack: "lucide", key: "Folder", label: "Folder", Icon: Folder }],
  ["folder-open", { pack: "lucide", key: "FolderOpen", label: "FolderOpen", Icon: FolderOpen }],
  ["folder-plus", { pack: "lucide", key: "FolderPlus", label: "FolderPlus", Icon: FolderPlus }],
  ["layout-dashboard", { pack: "lucide", key: "LayoutDashboard", label: "LayoutDashboard", Icon: LayoutDashboard }],
  ["scroll-text", { pack: "lucide", key: "ScrollText", label: "ScrollText", Icon: ScrollText }],
  ["message-square-text", { pack: "lucide", key: "MessageSquareText", label: "MessageSquareText", Icon: MessageSquareText }],
]);
