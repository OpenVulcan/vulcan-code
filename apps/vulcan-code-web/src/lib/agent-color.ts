// AGENT_COLOR_PALETTE maps the named agent color tokens stored in metadata to CSS colors, covering
// every name the built-in roles declare.
// AGENT_COLOR_PALETTE 将元数据中保存的具名智能体颜色标识映射为 CSS 颜色，涵盖内置角色声明的全部名称。
const AGENT_COLOR_PALETTE: Readonly<Record<string, string>> = {
  amber: "#f59e0b",
  blue: "#3b82f6",
  cyan: "#06b6d4",
  fuchsia: "#d946ef",
  gold: "#ca8a04",
  gray: "#6b7280",
  green: "#22c55e",
  indigo: "#6366f1",
  lime: "#84cc16",
  metallic: "#94a3b8",
  navy: "#1e3a8a",
  neon: "#39ff14",
  orange: "#f97316",
  pink: "#ec4899",
  purple: "#8b5cf6",
  red: "#ef4444",
  rose: "#f43f5e",
  slate: "#64748b",
  teal: "#14b8a6",
  violet: "#7c3aed",
  yellow: "#eab308",
};

// AGENT_COLOR_FALLBACK renders unknown color tokens and non-hex values in native color inputs.
// AGENT_COLOR_FALLBACK 用于渲染未知颜色标识，以及原生颜色输入框中的非十六进制取值。
const AGENT_COLOR_FALLBACK = AGENT_COLOR_PALETTE.gray;

// AGENT_SKILL_DEFAULT_COLOR is the marker color for Agent Skills and categories without a stored color.
// AGENT_SKILL_DEFAULT_COLOR 是未保存颜色的 Agent Skill 与分类所用的标记颜色。
export const AGENT_SKILL_DEFAULT_COLOR = "#64748B";

// colorSwatchValue returns a renderable color for an agent color token or CSS color value.
// colorSwatchValue 为智能体颜色标识或 CSS 色值返回可渲染颜色。
export function colorSwatchValue(value: string): string {
  const trimmed = value.trim();
  if (trimmed.startsWith("#") || trimmed.startsWith("rgb") || trimmed.startsWith("hsl")) return trimmed;
  return AGENT_COLOR_PALETTE[trimmed.toLowerCase()] ?? AGENT_COLOR_FALLBACK;
}

// colorInputValue normalizes metadata colors into the #rrggbb shape required by native color inputs.
// colorInputValue 将元数据颜色规范化为原生颜色输入框要求的 #rrggbb 形式。
export function colorInputValue(value: string): string {
  const renderedColor = colorSwatchValue(value).trim();
  const longHex = renderedColor.match(/^#([0-9a-f]{6})$/iu);
  if (longHex) return `#${longHex[1].toLowerCase()}`;
  const shortHex = renderedColor.match(/^#([0-9a-f])([0-9a-f])([0-9a-f])$/iu);
  if (shortHex) return `#${shortHex[1]}${shortHex[1]}${shortHex[2]}${shortHex[2]}${shortHex[3]}${shortHex[3]}`.toLowerCase();
  return AGENT_COLOR_FALLBACK;
}
