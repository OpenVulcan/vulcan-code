/** Validates the optional unsigned text accepted by the native configuration parsers.
 * 校验原生配置解析器接纳的可选无符号整数字符串。
 * value is the untouched draft; width identifies the Rust u32/u64 field contract.
 * value 为未经改写的草稿；width 指定 Rust u32/u64 字段契约。
 * Returns validity only, without rounding, stripping punctuation, or changing inheritance.
 * 仅返回有效性，不舍入、不删除标点，也不改变继承状态。
 */
export function isOptionalUnsignedDraft(value: string, width: 32 | 64): boolean {
  // Rust's optional parsers trim surrounding whitespace and accept an optional plus sign.
  // Rust 可选解析器去除首尾空白并接纳可选的正号。
  const trimmed = value.trim()
  if (trimmed === "") return true
  if (!/^\+?\d+$/.test(trimmed)) return false
  // Compare normalized decimal strings to avoid both u64 rounding and unbounded BigInt conversion.
  // 比较规范十进制字符串，避免 u64 舍入及无界 BigInt 转换。
  const digits = trimmed.replace(/^\+?0*/, "")
  const maximum = width === 32 ? "4294967295" : "18446744073709551615"
  return digits.length < maximum.length || (digits.length === maximum.length && digits <= maximum)
}
