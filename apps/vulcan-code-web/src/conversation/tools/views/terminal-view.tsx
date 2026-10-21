// Terminal transcript view: a prompt line followed by output with ANSI colors mapped onto the kit terminal palette.
// Long output opens on its tail, because the end of a run is where its result is.
// 终端记录视图：一行提示符后接输出，ANSI 颜色映射到套件终端色板。长输出默认显示末尾，因为运行结果在末尾。
import * as React from "react";
import { useWorkspaceFileOpener, type WorkspaceFileOpener } from "@/components/workspace/workspace-file-open-request";
import { useI18n } from "@/i18n";
import { splitTerminalPaths, terminalPathFrom } from "./terminal-paths";

// TERMINAL_TAIL_LINES is how many trailing lines a long output shows before the reader asks for all of it.
// TERMINAL_TAIL_LINES 是长输出在读者要求全部显示前展示的末尾行数。
export const TERMINAL_TAIL_LINES = 400;

// TerminalSpan is one run of text sharing a style.
// TerminalSpan 是共享同一样式的一段文本。
interface TerminalSpan {
  // Text of the run.
  // 片段文本。
  text: string;
  // Kit tone class, or null for the default ink.
  // 套件色调类；默认墨色时为 null。
  tone: string | null;
  // Whether the run is bold.
  // 片段是否加粗。
  bold: boolean;
}

// SGR_TONES maps ANSI foreground codes onto kit terminal tone classes.
// SGR_TONES 将 ANSI 前景色代码映射为套件终端色调类。
const SGR_TONES: Record<number, string | null> = {
  30: null, 31: "t-bad", 32: "t-ok", 33: "t-warn", 34: "t-info", 35: "t-mag", 36: "t-info", 37: null,
  90: "t-dim", 91: "t-bad", 92: "t-ok", 93: "t-warn", 94: "t-info", 95: "t-mag", 96: "t-info", 97: null,
};

// ESCAPE matches CSI sequences (captured when they set graphics) and OSC sequences, which are dropped.
// ESCAPE 匹配 CSI 序列（设置图形属性时捕获）与 OSC 序列，后者被丢弃。
const ESCAPE = /\u001b\[([0-9;?]*)([A-Za-z])|\u001b\][^\u0007\u001b]*(?:\u0007|\u001b\\)|\u001b[@-Z\\-_]/g;

// parseTerminalText splits text into styled spans, keeping only foreground tone and weight.
// parseTerminalText 将文本拆分为带样式的片段，仅保留前景色调与粗细。
export function parseTerminalText(source: string): TerminalSpan[] {
  const spans: TerminalSpan[] = [];
  let tone: string | null = null;
  let bold = false;
  let cursor = 0;
  const push = (text: string) => {
    if (!text) return;
    const last = spans[spans.length - 1];
    if (last && last.tone === tone && last.bold === bold) last.text += text;
    else spans.push({ bold, text, tone });
  };
  for (const match of source.matchAll(ESCAPE)) {
    push(source.slice(cursor, match.index));
    cursor = match.index + match[0].length;
    if (match[2] !== "m") continue;
    const codes = (match[1] || "0").split(";").map((code) => Number(code) || 0);
    for (let index = 0; index < codes.length; index += 1) {
      const code = codes[index];
      if (code === 0) { tone = null; bold = false; }
      else if (code === 1) bold = true;
      else if (code === 2) tone = "t-dim";
      else if (code === 22) bold = false;
      else if (code === 39) tone = null;
      else if (code === 38 || code === 48) index += codes[index + 1] === 2 ? 4 : 2;
      else if (code in SGR_TONES) tone = SGR_TONES[code];
    }
  }
  push(source.slice(cursor));
  return spans;
}

/** Renders one terminal transcript. File paths in the output open in the files panel when the workbench offers
 * one, resolved against cwd (the directory the command ran in) when it is known.
 * 渲染一段终端记录。工作台提供文件面板时，输出中的文件路径可在其中打开；已知 cwd（命令运行目录）时以其解析。
 * prompt is the first line (a command after "$", sent input after ">"); text is the combined output.
 * prompt 是首行（"$" 后为命令，">" 后为发送的输入）；text 是合并后的输出。 */
export function TerminalView(props: { prompt?: string; text: string; emptyText: string; cwd?: string | null }) {
  const { t } = useI18n();
  const openFile = useWorkspaceFileOpener();
  const [showAll, setShowAll] = React.useState(false);
  const lines = React.useMemo(() => props.text.replace(/\r\n?/g, "\n").replace(/\n+$/, "").split("\n"), [props.text]);
  const hidden = showAll ? 0 : Math.max(0, lines.length - TERMINAL_TAIL_LINES);
  const visible = hidden > 0 ? lines.slice(hidden).join("\n") : lines.join("\n");
  const spans = React.useMemo(() => parseTerminalText(visible), [visible]);
  return (
    <pre className="vk-term">
      {props.prompt ? <span className="t-cmd">{props.prompt}{"\n"}</span> : null}
      {hidden > 0 ? (
        <button className="vk-term-more" onClick={() => setShowAll(true)} type="button">
          {t("toolCard.output.hiddenLines").replace("{count}", String(hidden))}
        </button>
      ) : null}
      {props.text.trim()
        ? spans.map((span, index) => (
            <span className={[span.tone, span.bold ? "t-b" : null].filter(Boolean).join(" ") || undefined} key={index}>
              {openFile ? <TerminalPathText cwd={props.cwd} openFile={openFile} text={span.text} title={t("workspaceFiles.showInFilesPanel")} /> : span.text}
            </span>
          ))
        : <span className="t-dim">{props.emptyText}</span>}
    </pre>
  );
}

// TerminalPathText renders one run of output with the file locations in it as buttons that open them.
// TerminalPathText 渲染一段输出，其中的文件位置显示为可打开对应文件的按钮。
function TerminalPathText(props: { text: string; cwd?: string | null; openFile: WorkspaceFileOpener; title: string }) {
  return splitTerminalPaths(props.text).map((piece, index) => typeof piece === "string" ? piece : (
    <button className="vk-path-open" key={index} onClick={() => props.openFile(terminalPathFrom(props.cwd, piece.path), piece.lines)} title={props.title} type="button">
      {piece.text}
    </button>
  ));
}
