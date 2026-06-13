// localizedLiteral picks the display literal that matches the active locale.
// localizedLiteral 按当前语言选择展示文本。
export function localizedLiteral(locale: string, zh: string, en: string): string {
  return locale === "zh-CN" ? zh : en
}
