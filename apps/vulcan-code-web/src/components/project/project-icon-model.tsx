import React from "react";
import { Icon as IconifyIcon } from "@iconify/react";
import { DEFAULT_PROJECT_BADGE_CONFIG, type ProjectIconBadgeConfig, type ProjectIconPack } from "./project-icon-contract";

// The persisted badge contract lives in project-icon-contract so the sidebar can use it without the icon catalogs.
// 持久化徽标契约位于 project-icon-contract，使侧边栏无需加载图标目录即可使用。
export {
  DEFAULT_PROJECT_BADGE_CONFIG,
  decodeProjectBadgeKey,
  encodeProjectBadgeKey,
  normalizeProjectBadgeText,
  normalizeProjectIconSelection,
  type ProjectIconBadgeConfig,
  type ProjectIconBadgeShape,
  type ProjectIconPack,
  type ProjectIconSelection,
  type ProjectIconTab,
} from "./project-icon-contract";

// ProjectIconOption describes one selectable project icon.
// ProjectIconOption 描述一个可选择的项目图标。
export interface ProjectIconOption {
  // Icon pack that owns this option.
  // 拥有该选项的图标包。
  pack: ProjectIconPack;
  // Stable icon key persisted in the project row.
  // 持久化到项目行中的稳定图标键。
  key: string;
  // User-facing icon label.
  // 面向用户显示的图标名称。
  label: string;
  // Lucide icon component rendered by outline icon options.
  // 线性图标选项渲染使用的 Lucide 图标组件。
  Icon?: React.ComponentType<{ size?: number; className?: string }>;
  // Iconify full icon name for colored icon options.
  // 彩色图标选项使用的 Iconify 完整图标名。
  iconName?: string;
}

// PROJECT_BADGE_COLORS lists quick colors for generated project badges.
// PROJECT_BADGE_COLORS 列出生成式项目徽标的快捷颜色。
export const PROJECT_BADGE_COLORS = [DEFAULT_PROJECT_BADGE_CONFIG.background, "#2563eb", "#0891b2", "#16a34a", "#ca8a04", "#dc2626", "#7c3aed", "#db2777"];

// PROJECT_ICON_STREAM_BATCH_SIZE controls how many icon cells are mounted per incremental pass.
// PROJECT_ICON_STREAM_BATCH_SIZE 控制每次增量挂载多少个图标单元。
export const PROJECT_ICON_STREAM_BATCH_SIZE = 120;

// isIconifyProjectIconPack checks whether a persisted project icon pack belongs to Iconify color sets.
// isIconifyProjectIconPack 检查持久化的项目图标包是否属于 Iconify 彩色图标集。
export function isIconifyProjectIconPack(value: string | null | undefined): value is Extract<ProjectIconPack, "skill-icons" | "material-icon-theme"> {
  return value === "skill-icons" || value === "material-icon-theme";
}

// projectIconPackLabel returns the user-facing name for one project icon pack.
// projectIconPackLabel 返回单个项目图标包的面向用户名称。
export function projectIconPackLabel(pack: ProjectIconPack): string {
  if (pack === "skill-icons") {
    return "Skill Icons";
  }
  if (pack === "material-icon-theme") {
    return "Material Icon Theme";
  }
  return "Lucide";
}

// ProjectBadgeIcon renders the generated project badge as an inline SVG.
// ProjectBadgeIcon 将生成式项目徽标渲染为内联 SVG。
export function ProjectBadgeIcon(props: { config: ProjectIconBadgeConfig; size: number }) {
  const isCircle = props.config.shape === "circle";
  // fontSize uses the 64px badge template scale, matching the visual density of the reference SVG.
  // fontSize 使用 64px 徽标模板比例，匹配参考 SVG 的视觉密度。
  const fontSize = props.config.text.length > 1 ? 36 : 38;
  // labelY follows the reference badge baseline so Latin and CJK glyphs sit at the optical center.
  // labelY 采用参考徽标的文字基线，让拉丁与中文字符落在视觉中心。
  const labelY = props.config.text.length > 1 ? 36 : 35.5;
  return (
    <svg
      aria-hidden="true"
      className="project-badge-icon"
      height={props.size}
      viewBox="0 0 64 64"
      width={props.size}
    >
      {isCircle ? (
        <>
          <circle
            cx="32"
            cy="32"
            fill={props.config.background}
            r="32"
          />
          <circle
            cx="32"
            cy="32"
            fill="none"
            r="29.5"
            stroke={props.config.border}
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth="5"
          />
        </>
      ) : (
        <rect
          fill={props.config.background}
          height="59"
          rx="6"
          stroke={props.config.border}
          strokeWidth="5"
          width="59"
          x="2.5"
          y="2.5"
        />
      )}
      <text
        dominantBaseline="middle"
        fill={props.config.foreground}
        fontFamily="system-ui, -apple-system, BlinkMacSystemFont, Segoe UI, Microsoft YaHei, Noto Sans CJK SC, sans-serif"
        fontSize={fontSize}
        fontWeight="600"
        textAnchor="middle"
        x="32"
        y={labelY}
      >
        {props.config.text}
      </text>
    </svg>
  );
}

// ProjectIconifyIcon renders one iconify project icon without exposing the library to callers.
// ProjectIconifyIcon 渲染单个 Iconify 项目图标，避免调用方直接依赖图标库。
export function ProjectIconifyIcon(props: { iconName: string; size: number; className?: string }) {
  return (
    <IconifyIcon
      aria-hidden="true"
      className={props.className}
      height={props.size}
      icon={props.iconName}
      width={props.size}
    />
  );
}
