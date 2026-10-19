// Labelled, colorized JSON block for tool arguments and structured results.
// 用于工具参数与结构化结果的带标签彩色 JSON 区块。
import * as React from "react";

// JSON_TOKEN matches keys, strings, numbers and literals in pretty-printed JSON.
// JSON_TOKEN 匹配格式化 JSON 中的键、字符串、数字与字面量。
const JSON_TOKEN = /("(?:[^"\\]|\\.)*")(\s*:)?|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|\b(true|false|null)\b/g;

/** Renders one value as indented JSON with key, string and number colors.
 * 以带键、字符串与数字着色的缩进 JSON 渲染一个值。 */
export function JsonView(props: { value: unknown; label?: React.ReactNode }) {
  const parts = React.useMemo(() => {
    const text = JSON.stringify(props.value, null, 2) ?? "";
    const nodes: React.ReactNode[] = [];
    let cursor = 0;
    for (const match of text.matchAll(JSON_TOKEN)) {
      if (match.index > cursor) nodes.push(text.slice(cursor, match.index));
      const className = match[1] ? (match[2] ? "j-k" : "j-s") : "j-n";
      nodes.push(<span className={className} key={match.index}>{match[1] ?? match[3] ?? match[4]}</span>);
      if (match[2]) nodes.push(match[2]);
      cursor = match.index + match[0].length;
    }
    nodes.push(text.slice(cursor));
    return nodes;
  }, [props.value]);
  return (
    <div className="vk-json-block">
      {props.label ? <div className="vk-json-label">{props.label}</div> : null}
      <pre className="vk-json">{parts}</pre>
    </div>
  );
}
