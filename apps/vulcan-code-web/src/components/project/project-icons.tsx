import React from "react";
import { Folder, FolderOpen } from "lucide-react";
import type { ProjectSummary } from "@/types";

export {
  normalizeProjectIconSelection,
  type ProjectIconBadgeShape,
  type ProjectIconPack,
  type ProjectIconSelection,
  type ProjectIconDialogTarget,
} from "./project-icon-contract";

// LazyProjectCustomIcon isolates full Iconify and Lucide catalogs from the initial workbench chunk.
// LazyProjectCustomIcon 将完整 Iconify 与 Lucide 目录隔离在初始工作台分块之外。
const LazyProjectCustomIcon = React.lazy(() => import("./project-custom-icon").then((module) => ({ default: module.ProjectCustomIcon })));

// ProjectIcon renders either a generated badge, a selected icon, or the default folder state.
// ProjectIcon 渲染生成式徽标、已选图标或默认文件夹状态。
export function ProjectIcon(props: { project: ProjectSummary; isOpen?: boolean; size?: number }) {
  // fallback preserves the current folder state while an optional custom catalog loads.
  // fallback 在可选自定义目录加载期间保留当前文件夹状态。
  const fallback = props.isOpen ? <FolderOpen size={props.size ?? 16} /> : <Folder size={props.size ?? 16} />;
  if (props.project.iconPack && props.project.iconKey) {
    return (
      <React.Suspense fallback={fallback}>
        <LazyProjectCustomIcon isOpen={props.isOpen} project={props.project} size={props.size ?? 16} />
      </React.Suspense>
    );
  }
  return fallback;
}
