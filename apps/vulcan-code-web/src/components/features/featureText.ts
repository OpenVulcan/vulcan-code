// formatSkillText replaces simple named placeholders in one localized feature string.
// formatSkillText 替换功能区本地化字符串中的简单命名占位符。
export function formatSkillText(template: string, values: Record<string, string | number>): string {
  return template.replace(/\{(\w+)\}/g, (match, key) => String(values[key] ?? match))
}
