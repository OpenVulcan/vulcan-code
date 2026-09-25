import type { ProjectSummary } from "@/types"

// ProjectIconBadgeShape names the badge silhouette used by generated project icons.
// ProjectIconBadgeShape 命名生成式项目图标使用的徽标轮廓。
export type ProjectIconBadgeShape = "circle" | "square"

// ProjectIconPack names every project icon source supported by the picker.
// ProjectIconPack 命名项目图标选择器支持的每一种图标来源。
export type ProjectIconPack = "skill-icons" | "material-icon-theme" | "lucide"

// ProjectIconTab names the selectable tabs inside the project icon dialog.
// ProjectIconTab 命名项目图标弹窗中的可选择标签页。
export type ProjectIconTab = ProjectIconPack | "badge"

// ProjectIconSelection describes one icon override persisted for a project.
// ProjectIconSelection 描述为项目持久化的单个图标覆盖配置。
export interface ProjectIconSelection {
  // pack is the icon source identifier stored in the project row.
  // pack 是存储在项目行中的图标来源标识。
  pack: string
  // key is the icon name or encoded badge configuration.
  // key 是图标名称或编码后的徽标配置。
  key: string
}

// ProjectIconBadgeConfig stores the generated badge icon appearance.
// ProjectIconBadgeConfig 保存生成式徽标图标的外观。
export interface ProjectIconBadgeConfig {
  // background is the badge fill color.
  // background 是徽标填充颜色。
  background: string
  // border is the badge outline color.
  // border 是徽标轮廓颜色。
  border: string
  // foreground is the badge text color.
  // foreground 是徽标文字颜色。
  foreground: string
  // shape is the badge silhouette.
  // shape 是徽标轮廓。
  shape: ProjectIconBadgeShape
  // text contains one Chinese character or up to two Latin letters.
  // text 包含一个中文字符或最多两个英文字母。
  text: string
}

// ProjectIconDialogTarget stores the project currently editing its icon.
// ProjectIconDialogTarget 保存当前正在编辑图标的项目。
export interface ProjectIconDialogTarget {
  // project is the exact project row being edited.
  // project 是当前正在编辑的精确项目行。
  project: ProjectSummary
}

// DEFAULT_PROJECT_BADGE_CONFIG provides the stable badge normalization fallback.
// DEFAULT_PROJECT_BADGE_CONFIG 提供稳定的徽标规范化回退值。
export const DEFAULT_PROJECT_BADGE_CONFIG: ProjectIconBadgeConfig = {
  shape: "square",
  text: "VC",
  background: "#111827",
  border: "#111827",
  foreground: "#ffffff",
}

// normalizeProjectIconSelection normalizes generated badges without loading icon catalogs.
// normalizeProjectIconSelection 在不加载图标目录的情况下规范化生成式徽标。
export function normalizeProjectIconSelection(icon: ProjectIconSelection): ProjectIconSelection {
  if (icon.pack !== "badge") {
    return icon
  }
  return {
    pack: icon.pack,
    key: encodeProjectBadgeKey(decodeProjectBadgeKey(icon.key) ?? DEFAULT_PROJECT_BADGE_CONFIG),
  }
}

// encodeProjectBadgeKey serializes a generated badge config into the persisted icon key.
// encodeProjectBadgeKey 将生成式徽标配置序列化为持久化图标键。
export function encodeProjectBadgeKey(config: ProjectIconBadgeConfig): string {
  return JSON.stringify(config)
}

// decodeProjectBadgeKey validates one persisted badge configuration.
// decodeProjectBadgeKey 验证一份持久化徽标配置。
export function decodeProjectBadgeKey(value?: string | null): ProjectIconBadgeConfig | null {
  if (!value) {
    return null
  }
  try {
    // parsed is the untrusted persisted badge object.
    // parsed 是不可信的持久化徽标对象。
    const parsed = JSON.parse(value) as Partial<ProjectIconBadgeConfig>
    // shape accepts the current values and the two legacy square aliases.
    // shape 接受当前值以及两个旧版方形别名。
    const shape = parsed.shape === "circle"
      ? "circle"
      : parsed.shape === "square" || parsed.shape === "rounded" || parsed.shape === "sharp"
        ? "square"
        : null
    // text preserves the exact established badge normalization rules.
    // text 保留既有徽标文字规范化规则。
    const text = normalizeProjectBadgeText(parsed.text ?? "")
    if (!shape || !text) {
      return null
    }
    return {
      shape,
      text,
      background: normalizeProjectBadgeColor(parsed.background, DEFAULT_PROJECT_BADGE_CONFIG.background),
      border: normalizeProjectBadgeColor(parsed.border, DEFAULT_PROJECT_BADGE_CONFIG.border),
      foreground: normalizeProjectBadgeColor(parsed.foreground, DEFAULT_PROJECT_BADGE_CONFIG.foreground),
    }
  } catch {
    return null
  }
}

// normalizeProjectBadgeText keeps one Chinese character or two Latin letters.
// normalizeProjectBadgeText 保留一个中文字符或两个英文字母。
export function normalizeProjectBadgeText(value: string): string {
  // trimmed removes surrounding whitespace before character selection.
  // trimmed 在选择字符前移除首尾空白。
  const trimmed = value.trim()
  // chinese selects the first Han character when present.
  // chinese 在存在汉字时选择第一个汉字。
  const chinese = Array.from(trimmed).find((character) => /\p{Script=Han}/u.test(character))
  return chinese ?? trimmed.replace(/[^a-zA-Z]/g, "").slice(0, 2).toUpperCase()
}

// normalizeProjectBadgeColor keeps one exact six-digit hexadecimal color.
// normalizeProjectBadgeColor 保留一个精确的六位十六进制颜色。
function normalizeProjectBadgeColor(value: unknown, fallback: string): string {
  return typeof value === "string" && /^#[0-9a-fA-F]{6}$/.test(value) ? value : fallback
}
