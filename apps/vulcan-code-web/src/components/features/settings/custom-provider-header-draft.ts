// Parse complete header rows without discarding unfinished input or duplicate declarations.
// 解析完整请求头行，不丢弃未完成输入或重复声明。
// Parameters: exact multiline editor text. Returns a complete header map, or null until corrected.
// 参数：编辑器中的精确多行文本。返回值：完整请求头映射；需要修正时返回 null。
export function parseCustomProviderHeaderDraft(text: string): Record<string, string> | null {
  const entries: Array<[string, string]> = []
  const names = new Set<string>()
  for (const line of text.split("\n")) {
    if (!line.trim()) continue
    const separator = line.indexOf(":")
    if (separator < 1) return null
    const name = line.slice(0, separator).trim()
    const value = line.slice(separator + 1).trim()
    if (!name || !value || names.has(name.toLowerCase())) return null
    names.add(name.toLowerCase())
    entries.push([name, value])
  }
  return Object.fromEntries(entries)
}
