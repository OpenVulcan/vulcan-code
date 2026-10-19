// Numbered source excerpt, as returned by a file read.
// 带行号的源码摘录，即文件读取返回的内容。

// CodeLine is one numbered source line.
// CodeLine 是一行带行号的源码。
export interface CodeLine {
  // 1-based line number in the file.
  // 文件中以 1 为基准的行号。
  number: number;
  // Line text without its line ending.
  // 不含换行符的行文本。
  text: string;
}

/** Renders numbered lines with a sticky gutter.
 * 渲染带固定行号栏的编号行。 */
export function CodeView(props: { lines: readonly CodeLine[] }) {
  return (
    <div className="vk-lines">
      {props.lines.map((line) => [
        <span key={`n${line.number}`}>{line.number}</span>,
        <span key={`t${line.number}`}>{line.text || " "}</span>,
      ])}
    </div>
  );
}
