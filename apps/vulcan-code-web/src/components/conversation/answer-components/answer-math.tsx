import React from "react";
import katex from "katex";
import "katex/dist/katex.min.css";
import "@/ui-kit/styles/answer-math.css";
import type { AnswerComponent } from "./answer-component-model";

// KaTeX typesetting for `ui-math` and inline `` $`…`$ `` spans. This module and KaTeX's stylesheet and fonts
// load only when a reply contains math. KaTeX's output is static markup: `trust: false` keeps commands such
// as `\href` and `\includegraphics` inert.
// `ui-math` 与行内 `` $`…`$ `` 的 KaTeX 排版。本模块及 KaTeX 的样式与字体仅在回答含公式时加载。KaTeX 输出为
// 静态标记：`trust: false` 使 `\href`、`\includegraphics` 等命令失效。

type MathComponent = Extract<AnswerComponent, { kind: "math" }>;

// Typeset `tex`, or return KaTeX's parse error.
// 排版 `tex`，或返回 KaTeX 的解析错误。
function typeset(tex: string, display: boolean): { html: string } | { error: string } {
  try {
    return {
      html: katex.renderToString(tex, {
        displayMode: display,
        throwOnError: true,
        strict: "ignore",
        trust: false,
        output: "htmlAndMathml",
        maxSize: 20,
        maxExpand: 500,
      }),
    };
  } catch (error) {
    return { error: error instanceof Error ? error.message : String(error) };
  }
}

/** One formula; source that KaTeX cannot parse stays visible as code with the error on hover.
 * 单个公式；KaTeX 无法解析的源码保持为代码显示，悬停可见错误。 */
export function MathFormula({ display = false, tex }: { display?: boolean; tex: string }) {
  const result = React.useMemo(() => typeset(tex, display), [display, tex]);
  if ("error" in result) {
    return <code className="vk-a-math-source" data-display={display || undefined} title={result.error}>{tex}</code>;
  }
  return <span className={display ? "vk-a-math-display" : "vk-a-math-inline"} dangerouslySetInnerHTML={{ __html: result.html }} />;
}

/** `ui-math`: optional title, displayed equations and a symbol legend. `ui-math`：可选标题、独立显示的公式及符号图例。 */
export function AnswerMathView({ math }: { math: MathComponent }) {
  return (
    <figure className="vk-a-math">
      {math.title ? <figcaption>{math.title}</figcaption> : null}
      <div className="vk-a-math-equations">
        {math.equations.map((equation, index) => <MathFormula display key={index} tex={equation} />)}
      </div>
      {math.symbols.length > 0 ? (
        <dl className="vk-a-math-legend">
          {math.symbols.map((symbol, index) => (
            <React.Fragment key={index}>
              <dt><MathFormula tex={symbol.symbol} /></dt>
              <dd>{symbol.meaning}</dd>
            </React.Fragment>
          ))}
        </dl>
      ) : null}
    </figure>
  );
}
