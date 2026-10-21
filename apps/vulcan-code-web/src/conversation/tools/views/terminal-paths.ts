// Terminal file locations: finds the file paths a command printed, such as compiler diagnostics
// ("src/main.rs:12:5", "src\app.ts(4,9)") or listings ("modified: docs/a.md"), so the output can open them.
// 终端文件位置：找出命令输出中的文件路径，如编译诊断（"src/main.rs:12:5"、"src\app.ts(4,9)"）或列表
// （"modified: docs/a.md"），使输出可以打开它们。
import type { WorkspaceFileLines } from "@/components/workspace/workspace-file-open-request";

// TerminalPathPiece is plain output text or a file location found in it.
// TerminalPathPiece 是普通输出文本或其中找到的文件位置。
export type TerminalPathPiece = string | { text: string; path: string; lines: WorkspaceFileLines | null };

// TERMINAL_PATH matches a path with at least one folder and a file extension, optionally followed by a line
// (":12", ":12:5" or "(12,5)"). It must not continue a word, path or URL ("https://x.dev/a.js" never matches).
// TERMINAL_PATH 匹配至少含一层目录且带扩展名的路径，其后可带行号（":12"、":12:5" 或 "(12,5)"）。
// 它不能接在单词、路径或 URL 之后（"https://x.dev/a.js" 永不匹配）。
const TERMINAL_PATH = /(?<![\w.@~/\\:-])((?:[A-Za-z]:[\\/]|\/)?(?:[\w.@~-]+[\\/])+[\w.@~-]*\.[A-Za-z][A-Za-z0-9]{0,9})(?![\w\\/])(?::(\d+)(?::\d+)?|\((\d+)(?:,\d+)?\))?/g;

// splitTerminalPaths cuts text into plain pieces and file locations.
// splitTerminalPaths 将文本切分为普通片段与文件位置。
export function splitTerminalPaths(text: string): TerminalPathPiece[] {
  const pieces: TerminalPathPiece[] = [];
  let cursor = 0;
  for (const match of text.matchAll(TERMINAL_PATH)) {
    const line = Number(match[2] ?? match[3] ?? 0);
    if (match.index > cursor) pieces.push(text.slice(cursor, match.index));
    pieces.push({ text: match[0], path: match[1], lines: line > 0 ? { start: line, end: line } : null });
    cursor = match.index + match[0].length;
  }
  if (cursor < text.length) pieces.push(text.slice(cursor));
  return pieces;
}

// terminalPathFrom resolves a printed path against the directory the command ran in, when that is known and
// absolute; absolute paths and output without a known directory are left as printed.
// terminalPathFrom 在命令运行目录已知且为绝对路径时，以其解析输出中的路径；绝对路径及无已知目录的输出保持原样。
export function terminalPathFrom(cwd: string | null | undefined, path: string): string {
  if (!cwd || /^(?:[A-Za-z]:[\\/]|\/)/.test(path) || !/^(?:[A-Za-z]:[\\/]|\/)/.test(cwd)) return path;
  const separator = cwd.includes("\\") ? "\\" : "/";
  return `${cwd.replace(/[\\/]+$/, "")}${separator}${path.replace(/^\.[\\/]/, "")}`;
}
