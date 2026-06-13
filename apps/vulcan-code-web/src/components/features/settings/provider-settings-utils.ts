import type { ProviderModelSummary } from "@/types"

// sameStringSet compares two string lists as unordered sets.
// sameStringSet 将两个字符串列表作为无序集合比较。
export function sameStringSet(left: string[], right: string[]): boolean {
  if (left.length !== right.length) {
    return false
  }
  const rightSet = new Set(right)
  return left.every((item) => rightSet.has(item))
}

// nullablePositiveInteger parses a positive integer field while treating blanks as null.
// nullablePositiveInteger 解析正整数输入，并将空值视为 null。
export function nullablePositiveInteger(value: string): number | null {
  const trimmed = value.trim()
  if (!trimmed) {
    return null
  }
  const parsed = Number.parseInt(trimmed, 10)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null
}

// formatProviderModelMeta renders the compact capability line for a model row.
// formatProviderModelMeta 渲染模型行的紧凑能力信息。
export function formatProviderModelMeta(model: ProviderModelSummary, text: (zh: string, en: string) => string): string {
  const modalities = model.inputModalities?.length ? model.inputModalities.join("+") : "text"
  const limits = [
    model.contextLimit ? `${text("上下文", "context")} ${model.contextLimit}` : null,
    model.inputLimit ? `${text("输入", "input")} ${model.inputLimit}` : null,
    model.outputLimit ? `${text("输出", "output")} ${model.outputLimit}` : null,
  ].filter(Boolean)
  return [model.id, model.family, modalities, ...limits].filter(Boolean).join(" · ")
}

// localizedModalityLabel returns the compact label for one model modality direction.
// localizedModalityLabel 返回单个模型模态方向的紧凑标签。
export function localizedModalityLabel(direction: "input" | "output", value: string, text: (zh: string, en: string) => string): string {
  const suffix = direction === "input" ? text("输入", "input") : text("输出", "output")
  switch (value) {
    case "text":
      return text(`文本${suffix}`, `Text ${suffix}`)
    case "image":
      return text(`图片${suffix}`, `Image ${suffix}`)
    case "audio":
      return text(`音频${suffix}`, `Audio ${suffix}`)
    case "video":
      return text(`视频${suffix}`, `Video ${suffix}`)
    case "pdf":
      return text(`PDF / 文件${suffix}`, `PDF / file ${suffix}`)
    default:
      return value
  }
}

// localizedLiteral picks the display literal that matches the active locale.
// localizedLiteral 按当前语言选择展示文本。
export function localizedLiteral(locale: string, zh: string, en: string): string {
  return locale === "zh-CN" ? zh : en
}
