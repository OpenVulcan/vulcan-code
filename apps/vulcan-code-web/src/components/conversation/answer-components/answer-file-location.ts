// Path-shape rule for file locations written in answer component cells. It stays out of the parser module so the
// eagerly loaded conversation Markdown can link locations without pulling the parser into the startup bundle.
// 回答组件单元格中文件位置的路径形态规则。它独立于解析器模块，使随启动加载的会话 Markdown 可以链接位置，
// 而不把解析器带入启动包。

/**
 * The file path of a path-shaped location such as `src/a.rs:42`, or null for prose. Models sometimes put prose in a
 * location cell; only a token without spaces or full-width punctuation and with a separator or an extension is a file.
 * The `:line` or `:line:column` suffix is display-only and is stripped.
 * `src/a.rs:42` 这类路径形位置的文件路径；说明文字返回 null。模型有时会在位置单元格写入说明文字；只有不含空白与
 * 全角标点、且带分隔符或扩展名的词元才视为文件。`:行` 或 `:行:列` 后缀仅用于显示，会被去掉。
 */
export function answerLocationPath(location: string): string | null {
  const path = location.trim().replace(/:\d+(?::\d+)?$/u, "");
  if (!/^[^\s\u3000-\u303f\uff00-\uffef]+$/u.test(path) || !/[\\/]|\.[\p{L}\p{N}]+$/u.test(path)) return null;
  return path;
}
