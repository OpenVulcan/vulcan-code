import * as React from "react";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";

// TerminalOutputToken stores one safe rendered text segment after ANSI parsing.
// TerminalOutputToken 保存 ANSI 解析后的单个安全文本片段。
interface TerminalOutputToken {
  // Plain text payload for this segment.
  // 该片段的纯文本载荷。
  text: string;
  // Inline style synthesized from SGR color codes.
  // 根据 SGR 颜色码生成的内联样式。
  style: React.CSSProperties;
  // CSS classes synthesized from SGR text attributes.
  // 根据 SGR 文本属性生成的 CSS 类。
  className: string;
}

// TerminalAnsiState tracks the current SGR attributes while parsing terminal output.
// TerminalAnsiState 在解析终端输出时跟踪当前 SGR 属性。
interface TerminalAnsiState {
  // Current foreground color, if one is active.
  // 当前启用的前景色。
  color?: string;
  // Current background color, if one is active.
  // 当前启用的背景色。
  backgroundColor?: string;
  // Whether bold text is active.
  // 是否启用粗体。
  bold: boolean;
  // Whether dim text is active.
  // 是否启用弱化。
  dim: boolean;
  // Whether italic text is active.
  // 是否启用斜体。
  italic: boolean;
  // Whether underline text is active.
  // 是否启用下划线。
  underline: boolean;
}

// TerminalAnsiParseOptions controls context-specific ANSI rendering behavior.
// TerminalAnsiParseOptions 控制特定场景下的 ANSI 渲染行为。
interface TerminalAnsiParseOptions {
  // Treat SGR black background as the surrounding terminal default instead of a painted span.
  // 将 SGR 黑色背景视为周围终端默认背景，而不是绘制成 span 背景。
  suppressBlackBackground?: boolean;
}

// TerminalOutputView renders terminal text with safe ANSI SGR styling.
// TerminalOutputView 使用安全的 ANSI SGR 样式渲染终端文本。
export function TerminalOutputView(props: {
  // Terminal text to render.
  // 要渲染的终端文本。
  text: string;
  // Optional CSS class for the pre element.
  // pre 元素使用的可选 CSS 类。
  className?: string;
  // Empty-state label used when text is blank.
  // 文本为空时使用的空状态文案。
  emptyText?: string;
  // Optional keyboard handler for interactive terminal surfaces.
  // 交互式终端表面使用的可选键盘处理器。
  onKeyDown?: React.KeyboardEventHandler<HTMLPreElement>;
  // Optional focus tab index for interactive terminal surfaces.
  // 交互式终端表面使用的可选聚焦 tab 序号。
  tabIndex?: number;
  // Whether black ANSI background should be treated as the terminal surface default.
  // 是否将黑色 ANSI 背景视为终端表面的默认背景。
  suppressBlackBackground?: boolean;
}) {
  const { t } = useI18n();
  const tokens = React.useMemo(
    () => parseAnsiTerminalOutput(props.text, { suppressBlackBackground: props.suppressBlackBackground }),
    [props.suppressBlackBackground, props.text],
  );
  if (tokens.length === 0) {
    return (
      <pre
        className={cn("terminal-output-view", props.className)}
        onKeyDown={props.onKeyDown}
        tabIndex={props.tabIndex}
      >
        {props.emptyText ?? t("toolPresentation.output.empty")}
      </pre>
    );
  }
  return (
    <pre
      className={cn("terminal-output-view", props.className)}
      onKeyDown={props.onKeyDown}
      tabIndex={props.tabIndex}
    >
      {tokens.map((token, index) => (
        <span className={token.className || undefined} key={index} style={token.style}>
          {token.text}
        </span>
      ))}
    </pre>
  );
}

// parseAnsiTerminalOutput converts ANSI SGR sequences into safe React token data.
// parseAnsiTerminalOutput 将 ANSI SGR 序列转换为安全的 React 片段数据。
export function parseAnsiTerminalOutput(text: string, options: TerminalAnsiParseOptions = {}): TerminalOutputToken[] {
  const tokens: TerminalOutputToken[] = [];
  const state = initialAnsiState();
  let buffer = "";
  let index = 0;
  while (index < text.length) {
    const char = text[index];
    if (char !== "\x1b") {
      buffer += char;
      index += 1;
      continue;
    }
    const csiMatch = /^\x1b\[([0-9;:]*)?([A-Za-z])/.exec(text.slice(index));
    if (!csiMatch) {
      index += 1;
      continue;
    }
    flushAnsiBuffer(tokens, buffer, state);
    buffer = "";
    if (csiMatch[2] === "m") {
      applySgrCodes(state, parseSgrCodes(csiMatch[1] ?? ""), options);
    }
    index += csiMatch[0].length;
  }
  flushAnsiBuffer(tokens, buffer, state);
  return tokens;
}

// initialAnsiState returns the default terminal text style.
// initialAnsiState 返回默认终端文本样式。
function initialAnsiState(): TerminalAnsiState {
  return {
    bold: false,
    dim: false,
    italic: false,
    underline: false,
  };
}

// flushAnsiBuffer stores one buffered text segment with the current style.
// flushAnsiBuffer 使用当前样式保存一段已缓冲文本。
function flushAnsiBuffer(tokens: TerminalOutputToken[], text: string, state: TerminalAnsiState): void {
  if (!text) {
    return;
  }
  tokens.push({
    text,
    style: {
      ...(state.color ? { color: state.color } : {}),
      ...(state.backgroundColor ? { backgroundColor: state.backgroundColor } : {}),
    },
    className: cn(
      state.bold && "terminal-ansi-bold",
      state.dim && "terminal-ansi-dim",
      state.italic && "terminal-ansi-italic",
      state.underline && "terminal-ansi-underline",
    ),
  });
}

// parseSgrCodes parses semicolon or colon separated SGR parameter lists.
// parseSgrCodes 解析分号或冒号分隔的 SGR 参数列表。
function parseSgrCodes(raw: string): number[] {
  if (!raw) {
    return [0];
  }
  return raw
    .split(/[;:]/)
    .map((part) => Number.parseInt(part || "0", 10))
    .filter((value) => Number.isFinite(value));
}

// applySgrCodes mutates the active ANSI state for one SGR instruction list.
// applySgrCodes 根据一组 SGR 指令更新当前 ANSI 状态。
function applySgrCodes(state: TerminalAnsiState, codes: number[], options: TerminalAnsiParseOptions): void {
  for (let index = 0; index < codes.length; index += 1) {
    const code = codes[index] ?? 0;
    if (code === 0) {
      Object.assign(state, initialAnsiState());
    } else if (code === 40 && options.suppressBlackBackground) {
      state.backgroundColor = undefined;
    } else if (code === 1) {
      state.bold = true;
    } else if (code === 2) {
      state.dim = true;
    } else if (code === 3) {
      state.italic = true;
    } else if (code === 4) {
      state.underline = true;
    } else if (code === 22) {
      state.bold = false;
      state.dim = false;
    } else if (code === 23) {
      state.italic = false;
    } else if (code === 24) {
      state.underline = false;
    } else if (code === 39) {
      state.color = undefined;
    } else if (code === 49) {
      state.backgroundColor = undefined;
    } else if ((code === 38 || code === 48) && index + 1 < codes.length) {
      const parsed = parseExtendedAnsiColor(codes, index + 1);
      if (parsed) {
        if (code === 38) {
          state.color = parsed.color;
        } else {
          state.backgroundColor = parsed.color;
        }
        index = parsed.nextIndex;
      }
    } else {
      const color = basicAnsiColor(code);
      if (color) {
        if (color.background) {
          state.backgroundColor = color.color;
        } else {
          state.color = color.color;
        }
      }
    }
  }
}

// parseExtendedAnsiColor parses 256-color and true-color SGR color forms.
// parseExtendedAnsiColor 解析 256 色与真彩色 SGR 颜色形式。
function parseExtendedAnsiColor(codes: number[], startIndex: number): { color: string; nextIndex: number } | null {
  const mode = codes[startIndex];
  if (mode === 5 && typeof codes[startIndex + 1] === "number") {
    return {
      color: ansi256Color(codes[startIndex + 1]),
      nextIndex: startIndex + 1,
    };
  }
  if (mode === 2) {
    const red = clampColor(codes[startIndex + 1]);
    const green = clampColor(codes[startIndex + 2]);
    const blue = clampColor(codes[startIndex + 3]);
    return {
      color: `rgb(${red} ${green} ${blue})`,
      nextIndex: startIndex + 3,
    };
  }
  return null;
}

// basicAnsiColor returns CSS colors for common 16-color SGR codes.
// basicAnsiColor 返回常见 16 色 SGR 码对应的 CSS 颜色。
function basicAnsiColor(code: number): { color: string; background: boolean } | null {
  const foreground = [
    "var(--terminal-black)",
    "var(--terminal-red)",
    "var(--terminal-green)",
    "var(--terminal-yellow)",
    "var(--terminal-blue)",
    "var(--terminal-magenta)",
    "var(--terminal-cyan)",
    "var(--terminal-white)",
  ];
  if (code >= 30 && code <= 37) {
    return { color: foreground[code - 30], background: false };
  }
  if (code >= 40 && code <= 47) {
    return { color: foreground[code - 40], background: true };
  }
  if (code >= 90 && code <= 97) {
    return { color: `color-mix(in oklch, ${foreground[code - 90]} 75%, white)`, background: false };
  }
  if (code >= 100 && code <= 107) {
    return { color: `color-mix(in oklch, ${foreground[code - 100]} 75%, white)`, background: true };
  }
  return null;
}

// ansi256Color converts one 256-color palette index into CSS rgb text.
// ansi256Color 将一个 256 色调色板索引转换为 CSS rgb 文本。
function ansi256Color(value: number): string {
  const index = Math.max(0, Math.min(255, value));
  if (index < 16) {
    return ANSI_16_COLOR_RGB[index];
  }
  if (index >= 232) {
    const level = 8 + (index - 232) * 10;
    return `rgb(${level} ${level} ${level})`;
  }
  const cube = index - 16;
  const red = Math.floor(cube / 36);
  const green = Math.floor((cube % 36) / 6);
  const blue = cube % 6;
  return `rgb(${ansiCubeLevel(red)} ${ansiCubeLevel(green)} ${ansiCubeLevel(blue)})`;
}

// ansiCubeLevel converts one 6x6x6 color cube channel into an RGB value.
// ansiCubeLevel 将 6x6x6 色立方通道转换为 RGB 数值。
function ansiCubeLevel(value: number): number {
  return value === 0 ? 0 : 55 + value * 40;
}

// clampColor keeps true-color channels inside the valid CSS rgb range.
// clampColor 将真彩色通道限制在 CSS rgb 有效范围内。
function clampColor(value: number | undefined): number {
  return Math.max(0, Math.min(255, Number.isFinite(value) ? value! : 0));
}

// ANSI_16_COLOR_RGB stores fallback RGB values for the first 16 ANSI colors.
// ANSI_16_COLOR_RGB 保存前 16 个 ANSI 颜色的兜底 RGB 值。
const ANSI_16_COLOR_RGB = [
  "rgb(0 0 0)",
  "rgb(205 49 49)",
  "rgb(13 188 121)",
  "rgb(229 229 16)",
  "rgb(36 114 200)",
  "rgb(188 63 188)",
  "rgb(17 168 205)",
  "rgb(229 229 229)",
  "rgb(102 102 102)",
  "rgb(241 76 76)",
  "rgb(35 209 139)",
  "rgb(245 245 67)",
  "rgb(59 142 234)",
  "rgb(214 112 214)",
  "rgb(41 184 219)",
  "rgb(255 255 255)",
] as const;
