import React from "react";
import { Icon as IconifyIcon } from "@iconify/react";

// ProjectIconBadgeShape names the badge silhouette used by generated project icons.
// ProjectIconBadgeShape 命名生成式项目图标使用的徽标轮廓。
export type ProjectIconBadgeShape = "circle" | "square";

// ProjectIconPack names every project icon source supported by the picker.
// ProjectIconPack 命名项目图标选择器支持的每一种图标来源。
export type ProjectIconPack = "skill-icons" | "material-icon-theme" | "lucide";

// ProjectIconTab names the selectable tabs inside the project icon dialog.
// ProjectIconTab 命名项目图标弹窗中的可选择标签页。
export type ProjectIconTab = ProjectIconPack | "badge";

// ProjectIconSelection describes one icon override persisted for a project.
// ProjectIconSelection 描述为项目持久化的单个图标覆盖配置。
export interface ProjectIconSelection {
  // Icon pack identifier stored in the project row.
  // 存储在项目行中的图标包标识。
  pack: string;
  // Icon key or encoded badge configuration stored in the project row.
  // 存储在项目行中的图标键或编码后的徽标配置。
  key: string;
}

// ProjectIconBadgeConfig stores the generated badge icon appearance.
// ProjectIconBadgeConfig 保存生成式徽标图标的外观。
export interface ProjectIconBadgeConfig {
  // Badge shape used by the generated SVG.
  // 生成 SVG 使用的徽标形状。
  shape: ProjectIconBadgeShape;
  // One Chinese character or up to two Latin letters.
  // 一个中文字符或最多两个英文字母。
  text: string;
  // Badge background color.
  // 徽标背景色。
  background: string;
  // Badge border color.
  // 徽标边框色。
  border: string;
  // Badge text color.
  // 徽标文字色。
  foreground: string;
}

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

// DEFAULT_PROJECT_BADGE_CONFIG provides the first generated badge preview.
// DEFAULT_PROJECT_BADGE_CONFIG 提供生成式徽标的初始预览配置。
export const DEFAULT_PROJECT_BADGE_CONFIG: ProjectIconBadgeConfig = {
  shape: "square",
  text: "VC",
  background: "#111827",
  border: "#111827",
  foreground: "#ffffff",
};

// PROJECT_BADGE_COLORS lists quick colors for generated project badges.
// PROJECT_BADGE_COLORS 列出生成式项目徽标的快捷颜色。
export const PROJECT_BADGE_COLORS = ["#111827", "#2563eb", "#0891b2", "#16a34a", "#ca8a04", "#dc2626", "#7c3aed", "#db2777"];

// PROJECT_ICON_STREAM_BATCH_SIZE controls how many icon cells are mounted per incremental pass.
// PROJECT_ICON_STREAM_BATCH_SIZE 控制每次增量挂载多少个图标单元。
export const PROJECT_ICON_STREAM_BATCH_SIZE = 120;

// encodeProjectBadgeKey serializes a generated badge config into the persisted icon key.
// encodeProjectBadgeKey 将生成式徽标配置序列化为持久化图标键。
export function encodeProjectBadgeKey(config: ProjectIconBadgeConfig): string {
  return JSON.stringify(config);
}

// decodeProjectBadgeKey restores a generated badge config from persistence.
// decodeProjectBadgeKey 从持久化内容还原生成式徽标配置。
export function decodeProjectBadgeKey(value?: string | null): ProjectIconBadgeConfig | null {
  if (!value) {
    return null;
  }
  try {
    const parsed = JSON.parse(value) as Partial<ProjectIconBadgeConfig>;
    const shape = normalizeProjectBadgeShape(parsed.shape);
    const text = normalizeProjectBadgeText(parsed.text ?? "");
    if (!shape || !text) {
      return null;
    }
    return {
      shape,
      text,
      background: normalizeCssColor(parsed.background, DEFAULT_PROJECT_BADGE_CONFIG.background),
      border: normalizeCssColor(parsed.border, DEFAULT_PROJECT_BADGE_CONFIG.border),
      foreground: normalizeCssColor(parsed.foreground, DEFAULT_PROJECT_BADGE_CONFIG.foreground),
    };
  } catch {
    return null;
  }
}

// normalizeProjectBadgeShape keeps new badges on the simplified circle/square model.
// normalizeProjectBadgeShape 将新徽标限制在简化后的圆形/方形模型。
function normalizeProjectBadgeShape(value: unknown): ProjectIconBadgeShape | null {
  if (value === "circle") {
    return "circle";
  }
  if (value === "square" || value === "rounded" || value === "sharp") {
    return "square";
  }
  return null;
}

// normalizeProjectIconSelection normalizes generated badge selections before persistence.
// normalizeProjectIconSelection 在持久化前规范化生成式徽标选择。
export function normalizeProjectIconSelection(icon: ProjectIconSelection): ProjectIconSelection {
  if (icon.pack !== "badge") {
    return icon;
  }
  return {
    pack: icon.pack,
    key: encodeProjectBadgeKey(decodeProjectBadgeKey(icon.key) ?? DEFAULT_PROJECT_BADGE_CONFIG),
  };
}

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

// normalizeProjectBadgeText keeps one Chinese character or two Latin letters.
// normalizeProjectBadgeText 保留一个中文字符或两个英文字母。
export function normalizeProjectBadgeText(value: string): string {
  const trimmed = value.trim();
  const chinese = Array.from(trimmed).find((char) => /\p{Script=Han}/u.test(char));
  if (chinese) {
    return chinese;
  }
  return trimmed.replace(/[^a-zA-Z]/g, "").slice(0, 2).toUpperCase();
}

// normalizeCssColor keeps a simple hex color or falls back to a known value.
// normalizeCssColor 保留简单十六进制颜色，或回退到已知值。
function normalizeCssColor(value: unknown, fallback: string): string {
  if (typeof value !== "string") {
    return fallback;
  }
  return /^#[0-9a-fA-F]{6}$/.test(value) ? value : fallback;
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
