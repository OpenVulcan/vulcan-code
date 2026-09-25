// BYTE_UNITS lists the binary size suffixes from smallest to largest; values below the first unit stay exact bytes.
// BYTE_UNITS 按从小到大列出二进制容量单位；低于首个单位的数值保持精确字节显示。
const BYTE_UNITS: ReadonlyArray<readonly [number, string]> = [
  [1024, "KiB"],
  [1024 ** 2, "MiB"],
  [1024 ** 3, "GiB"],
  [1024 ** 4, "TiB"],
];

// formatBytes renders a byte size: exact below 1 KiB, otherwise one decimal with KiB, MiB, GiB or TiB.
// formatBytes 渲染字节大小：1 KiB 以下显示精确值，否则以 KiB、MiB、GiB、TiB 保留一位小数。
//
// Parameters:
// 参数：bytes 是字节数；非有限值与负数按 0 处理。
// Returns the compact label shared by every size display; a zero decimal is dropped (2048 -> "2 KiB")
// and rounding carries into the next unit (1_048_575 -> "1 MiB").
// 返回所有容量显示共用的紧凑文本；零小数会省略（2048 -> "2 KiB"），进位会升到下一单位（1_048_575 -> "1 MiB"）。
export function formatBytes(bytes: number): string {
  const normalized = Number.isFinite(bytes) ? Math.max(0, Math.round(bytes)) : 0;
  if (normalized < BYTE_UNITS[0][0]) {
    return `${normalized} B`;
  }
  for (let index = 0; index < BYTE_UNITS.length; index += 1) {
    const [unit, suffix] = BYTE_UNITS[index];
    const scaled = Math.round((normalized / unit) * 10) / 10;
    if (scaled < 1024 || index === BYTE_UNITS.length - 1) {
      return `${scaled.toFixed(1).replace(/\.0$/u, "")} ${suffix}`;
    }
  }
  return `${normalized} B`;
}

// formatDateTime renders one instant as a short local date and time in the active UI locale.
// formatDateTime 以当前界面语言将一个时间点渲染为简短的本地日期与时间。
//
// Parameters:
// 参数：value 是 Unix 毫秒时间戳或 Date；locale 是当前界面语言（如 zh-CN、en-US）。
// Returns the formatted text, or null when the instant is not a valid date so callers choose their own placeholder.
// 返回格式化文本；时间点无效时返回 null，由调用方选择自己的占位文本。
export function formatDateTime(value: number | Date, locale: string): string | null {
  const date = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(date.getTime())) {
    return null;
  }
  return new Intl.DateTimeFormat(locale, { dateStyle: "short", timeStyle: "short" }).format(date);
}

// errorMessage renders a rejected value for display: an Error's message, otherwise its string form.
// errorMessage 将拒绝值渲染为显示文本：Error 取其 message，否则取其字符串形式。
export function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}
