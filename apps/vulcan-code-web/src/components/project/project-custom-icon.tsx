import React from "react"
import type { ProjectSummary } from "@/types"
import { Folder, FolderOpen } from "lucide-react"
import {
  DEFAULT_PROJECT_BADGE_CONFIG,
  ProjectBadgeIcon,
  ProjectIconifyIcon,
  decodeProjectBadgeKey,
  isIconifyProjectIconPack,
  type ProjectIconOption,
  type ProjectIconPack,
} from "./project-icon-model"
import { loadProjectIconOption } from "./project-icon-catalog-loader"

// ProjectCustomIcon renders a persisted custom icon after its catalog chunk has loaded.
// ProjectCustomIcon 在图标目录分块加载后渲染持久化自定义图标。
export function ProjectCustomIcon(props: { isOpen?: boolean; project: ProjectSummary; size: number }) {
  // resolvedOption contains only the persisted icon resolved from its owning deferred catalog.
  // resolvedOption 只包含从对应延迟目录解析出的持久化图标。
  const [resolvedOption, setResolvedOption] = React.useState<ProjectIconOption | null>(null)

  React.useEffect(() => {
    setResolvedOption(null)
    // pack is accepted only when the persisted value names an implemented catalog.
    // pack 仅在持久化值明确指向已实现目录时才会被接受。
    const pack: ProjectIconPack | null = isIconifyProjectIconPack(props.project.iconPack)
      ? props.project.iconPack
      : props.project.iconPack === "lucide"
        ? "lucide"
        : null
    if (!pack || !props.project.iconKey) {
      return
    }
    // cancelled prevents a previous project lookup from overwriting the current icon.
    // cancelled 防止前一个项目的查询结果覆盖当前图标。
    let cancelled = false
    void loadProjectIconOption(pack, props.project.iconKey).then((option) => {
      if (!cancelled) {
        setResolvedOption(option)
      }
    })
    return () => {
      cancelled = true
    }
  }, [props.project.iconKey, props.project.iconPack])

  if (props.project.iconPack === "badge") {
    // config restores the persisted badge or the exact established fallback.
    // config 恢复持久化徽标或精确的既有回退值。
    const config = decodeProjectBadgeKey(props.project.iconKey) ?? DEFAULT_PROJECT_BADGE_CONFIG
    return <ProjectBadgeIcon config={config} size={props.size} />
  }
  if (isIconifyProjectIconPack(props.project.iconPack) && resolvedOption?.iconName) {
    return <ProjectIconifyIcon className="project-iconify-icon" iconName={resolvedOption.iconName} size={props.size} />
  }
  if (props.project.iconPack === "lucide" && resolvedOption?.Icon) {
    return <resolvedOption.Icon size={props.size} />
  }
  return props.isOpen ? <FolderOpen size={props.size} /> : <Folder size={props.size} />
}
