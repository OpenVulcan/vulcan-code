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
