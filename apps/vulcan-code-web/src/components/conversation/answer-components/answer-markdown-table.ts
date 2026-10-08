// Serialize a header and rows as a GitHub Markdown table; pipes and line breaks inside cells are escaped.
// 将表头与数据行序列化为 GitHub Markdown 表格；单元格内的竖线与换行会被转义。
// Returns the table text ending with a newline. 返回以换行结尾的表格文本。
export function markdownTable(header: readonly string[], rows: readonly (readonly string[])[]): string {
  const line = (cells: readonly string[]) => `| ${cells.map((cell) => cell.replace(/\|/gu, "\\|").replace(/\r?\n/gu, " ")).join(" | ")} |`;
  return [line(header), `|${" --- |".repeat(header.length)}`, ...rows.map(line), ""].join("\n");
}

// Whether every nonempty cell reads as a number (thousands separators, sign, percent or unit-free).
// 每个非空单元格是否都为数字（允许千分位、正负号与百分号）。
export function numericCells(cells: readonly string[]): boolean {
  const filled = cells.map((cell) => cell.trim()).filter((cell) => cell && cell !== "—" && cell !== "-");
  return filled.length > 0 && filled.every((cell) => /^[+\-−]?[\d,]*\.?\d+%?$/u.test(cell));
}
