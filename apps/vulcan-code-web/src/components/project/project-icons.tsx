import { Folder, FolderOpen } from "lucide-react";
import type { ProjectSummary } from "@/types";
import {
  DEFAULT_PROJECT_BADGE_CONFIG,
  PROJECT_ICON_MAP,
  ProjectBadgeIcon,
  ProjectIconifyIcon,
  decodeProjectBadgeKey,
  isIconifyProjectIconPack,
} from "./project-icon-model";

export {
  normalizeProjectIconSelection,
  type ProjectIconBadgeShape,
  type ProjectIconPack,
  type ProjectIconSelection,
} from "./project-icon-model";
export { ProjectIconDialog, type ProjectIconDialogTarget } from "./project-icon-dialog";

// ProjectIcon renders either a generated badge, a selected icon, or the default folder state.
// ProjectIcon 渲染生成式徽标、已选图标或默认文件夹状态。
export function ProjectIcon(props: { project: ProjectSummary; isOpen?: boolean; size?: number }) {
  if (props.project.iconPack === "badge") {
    const config = decodeProjectBadgeKey(props.project.iconKey) ?? DEFAULT_PROJECT_BADGE_CONFIG;
    return <ProjectBadgeIcon config={config} size={props.size ?? 16} />;
  }

  if (isIconifyProjectIconPack(props.project.iconPack) && props.project.iconKey) {
    return (
      <ProjectIconifyIcon
        className="project-iconify-icon"
        iconName={`${props.project.iconPack}:${props.project.iconKey}`}
        size={props.size ?? 16}
      />
    );
  }

  if (props.project.iconPack === "lucide" && props.project.iconKey) {
    const option = PROJECT_ICON_MAP.get(props.project.iconKey);
    if (option?.Icon) {
      return <option.Icon size={props.size ?? 16} />;
    }
  }

  return props.isOpen ? <FolderOpen size={props.size ?? 16} /> : <Folder size={props.size ?? 16} />;
}
