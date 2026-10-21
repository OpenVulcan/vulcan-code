// Terminal path links: turns the file locations on one logical xterm line into links that Ctrl/Cmd+click opens,
// using the same path rules as the transcript terminal view.
// 终端路径链接：将 xterm 一个逻辑行中的文件位置转换为可按 Ctrl/Cmd+点击打开的链接，路径规则与转录终端视图一致。
import type { IBufferLine, ILink } from "@xterm/xterm";
import type { WorkspaceFileLines } from "@/components/workspace/workspace-file-open-request";
import { splitTerminalPaths } from "@/conversation/tools/views/terminal-paths";

// TerminalPathOpener opens a path exactly as the terminal printed it, at the printed line when there is one.
// TerminalPathOpener 按终端输出原样打开路径，有行号时定位到该行。
export type TerminalPathOpener = (path: string, lines: WorkspaceFileLines | null) => void;

// TERMINAL_LINK_MAX_ROWS bounds how many wrapped rows one logical line may span when looking for links.
// TERMINAL_LINK_MAX_ROWS 限定查找链接时一个逻辑行可跨越的折行数。
export const TERMINAL_LINK_MAX_ROWS = 32;

// terminalPathLinks returns the links on one logical line: `rows` are its buffer rows, the first one at buffer line
// `firstY` (1-based), the rest wrapped from it, so a path broken by the terminal width is still one link. Wide
// characters take two columns and cells without text count as spaces, so each link covers exactly the cells its
// text is drawn in.
// terminalPathLinks 返回一个逻辑行上的链接：`rows` 是其缓冲区各行，首行位于缓冲区第 `firstY` 行（从 1 开始），
// 其余由其折行而来，因此被终端宽度截断的路径仍是一个链接。宽字符占两列，无文本的单元格按空格计，
// 因此每个链接恰好覆盖其文本所在的单元格。
export function terminalPathLinks(rows: readonly IBufferLine[], firstY: number, open: TerminalPathOpener): ILink[] {
  let text = "";
  // cells[i] is the cell that draws code unit i of `text`: its 0-based column, 1-based buffer line and width.
  // cells[i] 是绘制 `text` 第 i 个码元的单元格：列（从 0 开始）、缓冲区行（从 1 开始）与宽度。
  const cells: { x: number; y: number; width: number }[] = [];
  rows.forEach((line, row) => {
    for (let x = 0; x < line.length; x += 1) {
      const cell = line.getCell(x);
      const width = cell?.getWidth() ?? 1;
      if (width === 0) continue;
      const chars = cell?.getChars() || " ";
      text += chars;
      for (let unit = 0; unit < chars.length; unit += 1) cells.push({ x, y: firstY + row, width });
    }
  });
  const links: ILink[] = [];
  let offset = 0;
  for (const piece of splitTerminalPaths(text)) {
    const length = typeof piece === "string" ? piece.length : piece.text.length;
    if (typeof piece !== "string") {
      const first = cells[offset];
      const last = cells[offset + length - 1];
      links.push({
        activate: (event) => {
          if (event.ctrlKey || event.metaKey) open(piece.path, piece.lines);
        },
        range: { start: { x: first.x + 1, y: first.y }, end: { x: last.x + last.width, y: last.y } },
        text: piece.text,
      });
    }
    offset += length;
  }
  return links;
}
