// Terminal path links: turns the file locations on one xterm buffer line into links that Ctrl/Cmd+click opens,
// using the same path rules as the transcript terminal view.
// 终端路径链接：将 xterm 缓冲区一行中的文件位置转换为可按 Ctrl/Cmd+点击打开的链接，路径规则与转录终端视图一致。
import type { IBufferLine, ILink } from "@xterm/xterm";
import type { WorkspaceFileLines } from "@/components/workspace/workspace-file-open-request";
import { splitTerminalPaths } from "@/conversation/tools/views/terminal-paths";

// TerminalPathOpener opens a path exactly as the terminal printed it, at the printed line when there is one.
// TerminalPathOpener 按终端输出原样打开路径，有行号时定位到该行。
export type TerminalPathOpener = (path: string, lines: WorkspaceFileLines | null) => void;

// terminalPathLinks returns the links on buffer line `y` (1-based). Wide characters take two columns and cells
// without text count as spaces, so each link covers exactly the cells its text is drawn in.
// terminalPathLinks 返回缓冲区第 `y` 行（从 1 开始）上的链接。宽字符占两列，无文本的单元格按空格计，
// 因此每个链接恰好覆盖其文本所在的单元格。
export function terminalPathLinks(line: IBufferLine, y: number, open: TerminalPathOpener): ILink[] {
  let text = "";
  // columns[i] is the 0-based column of the cell that draws code unit i of `text`; widths[i] is that cell's width.
  // columns[i] 是绘制 `text` 第 i 个码元的单元格所在列（从 0 开始）；widths[i] 是该单元格宽度。
  const columns: number[] = [];
  const widths: number[] = [];
  for (let x = 0; x < line.length; x += 1) {
    const cell = line.getCell(x);
    const width = cell?.getWidth() ?? 1;
    if (width === 0) continue;
    const chars = cell?.getChars() || " ";
    text += chars;
    for (let unit = 0; unit < chars.length; unit += 1) {
      columns.push(x);
      widths.push(width);
    }
  }
  const links: ILink[] = [];
  let offset = 0;
  for (const piece of splitTerminalPaths(text)) {
    const length = typeof piece === "string" ? piece.length : piece.text.length;
    if (typeof piece !== "string") {
      const last = offset + length - 1;
      links.push({
        activate: (event) => {
          if (event.ctrlKey || event.metaKey) open(piece.path, piece.lines);
        },
        range: { start: { x: columns[offset] + 1, y }, end: { x: columns[last] + widths[last], y } },
        text: piece.text,
      });
    }
    offset += length;
  }
  return links;
}
