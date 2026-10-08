import React from "react";
import { Check, CircleAlert, CircleCheck, CircleX, Info, Play, TriangleAlert } from "lucide-react";
import type { GeneratedMediaAttachment } from "@/components/media/generated-media-types";
import { formatMessage, useI18n } from "@/i18n";
import "@/styles/answer-components.css";
import { AnswerChartView, formatChartNumber } from "./answer-chart";
import type { AnswerComponent, AnswerSeverity, AnswerTonedText, AnswerTreeNode } from "./answer-component-model";
import { AnswerFrame, CopyAction, type AnswerCopyHost } from "./answer-frame";
import { AnswerFlowView, AnswerSequenceView } from "./answer-graphs";
import { markdownTable } from "./answer-markdown-table";

/** Services the conversation Markdown surface lends to answer components.
 * 会话 Markdown 区域提供给回答组件的服务。 */
export interface AnswerComponentHost extends AnswerCopyHost {
  /** Render a container body with the same Markdown pipeline as the reply.
   * 用与回答相同的 Markdown 管线渲染容器正文。 */
  renderMarkdown: (body: string) => React.ReactNode;
  /** Renderable URL of a gallery path, or null when it cannot be resolved.
   * 图库路径的可渲染 URL；无法解析时为 null。 */
  mediaSource: (path: string) => string | null;
  /** Link target for a workspace file reference such as `src/a.rs:42`, or null.
   * 工作区文件引用（如 `src/a.rs:42`）的链接目标；否则为 null。 */
  fileHref: (location: string) => string | null;
  /** Optional media viewer for gallery items.
   * 图库条目可选的媒体查看器。 */
  onOpenMedia?: (attachment: GeneratedMediaAttachment) => void;
}

/** Render one parsed `ui-*` component: framed blocks carry reusable data, embedded blocks read as prose.
 * 渲染一个已解析的 `ui-*` 组件：框架型区块承载可复用数据，融入型区块随正文阅读。 */
export function AnswerComponentView({ component, host }: { component: AnswerComponent; host: AnswerComponentHost }) {
  const { t } = useI18n();
  switch (component.kind) {
    case "kv":
      return (
        <dl className="answer-kv">
          {component.rows.map((row, index) => (
            <React.Fragment key={index}><dt>{row.label}</dt><dd>{row.value}</dd></React.Fragment>
          ))}
        </dl>
      );
    case "metrics":
      return (
        <div className="answer-metrics">
          {component.rows.map((metric, index) => (
            <div className="answer-metric" data-tone={metric.tone} key={index}>
              <b>{metric.value}{metric.unit ? <small>{metric.unit}</small> : null}</b>
              <span>{metric.label}</span>
              {metric.maximum !== undefined ? (
                <div aria-hidden="true" className="answer-meter"><i style={{ width: `${(metric.numeric! / metric.maximum) * 100}%` }} /></div>
              ) : null}
            </div>
          ))}
        </div>
      );
    case "findings": {
      const header = ["#", t("answerComponents.column.severity"), t("answerComponents.column.finding"), t("answerComponents.column.location")];
      const severity = (value: AnswerSeverity) => t(SEVERITY_KEYS[value]);
      const copy = markdownTable(header, component.rows.map((row, index) => [String(index + 1), severity(row.severity), row.text, row.location ?? ""]));
      const located = component.rows.some((row) => row.location);
      return (
        <AnswerFrame copy={copy} host={host}>
          <div className="answer-table-scroll">
            <table className="answer-table">
              <thead><tr>{header.slice(0, located ? 4 : 3).map((cell) => <th key={cell}>{cell}</th>)}</tr></thead>
              <tbody>
                {component.rows.map((row, index) => (
                  <tr key={index}>
                    <td className="answer-table-index">{index + 1}</td>
                    <td><span className="answer-pill" data-tone={row.severity}>{severity(row.severity)}</span></td>
                    <td>{row.text}</td>
                    {located ? <td>{row.location ? <FileLink host={host} location={row.location} /> : null}</td> : null}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </AnswerFrame>
      );
    }
    case "compare": {
      const copy = markdownTable(component.header, component.rows.map((row) => [row.criterion, ...row.cells.map(tonedMarkdown)]));
      return (
        <AnswerFrame copy={copy} host={host}>
          <div className="answer-table-scroll">
            <table className="answer-table answer-compare">
              <thead><tr>{component.header.map((cell, index) => <th key={index}>{cell}</th>)}</tr></thead>
              <tbody>
                {component.rows.map((row, index) => (
                  <tr key={index}>
                    <td>{row.criterion}</td>
                    {row.cells.map((cell, column) => <td data-tone={cell.tone} key={column}><Toned value={cell} /></td>)}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </AnswerFrame>
      );
    }
    case "checklist":
      return (
        <ul className="answer-checklist">
          {component.items.map((item, index) => (
            <li data-status={item.status} key={index}>
              <span aria-hidden="true" className="answer-status" data-status={item.status}>{STATUS_SYMBOLS[item.status]}</span>
              <span>{item.item}</span>
              {item.note ? <small>{item.note}</small> : null}
            </li>
          ))}
        </ul>
      );
    case "steps":
      return (
        <ol className="answer-steps">
          {component.steps.map((step, index) => (
            <li data-status={step.status} key={index}>
              <span aria-hidden="true" className="answer-step-number">{step.status === "done" ? <Check size={13} strokeWidth={3} /> : index + 1}</span>
              <div><b>{step.title}</b>{step.text ? <p>{step.text}</p> : null}</div>
            </li>
          ))}
        </ol>
      );
    case "timeline":
      return (
        <ul className="answer-timeline">
          {component.rows.map((row, index) => (
            <li key={index}>
              <time>{row.time}</time><span aria-hidden="true" className="answer-timeline-dot" />
              <div><b>{row.title}</b>{row.text ? <p>{row.text}</p> : null}</div>
            </li>
          ))}
        </ul>
      );
    case "cards":
      return (
        <div className="answer-cards">
          {component.cards.map((card, index) => (
            <article className="answer-card" key={index}>
              <header>
                <strong>{card.title}</strong>
                {card.tag ? <span className="answer-pill" data-tone={card.tag.tone}><Toned value={card.tag} /></span> : null}
              </header>
              <p>{card.text}</p>
            </article>
          ))}
        </div>
      );
    case "files":
      return (
        <section className="answer-frame answer-files">
          <header className="answer-frame-head">
            <span className="answer-frame-label">
              <strong>{formatMessage(t("answerComponents.fileCount"), { count: component.files.length })}</strong>
              <LineDelta added={component.added} removed={component.removed} />
            </span>
          </header>
          <ul>
            {component.files.map((file, index) => (
              <li key={index}>
                <span className="answer-files-path"><FileLink host={host} location={file.path} />{file.note ? <small>{file.note}</small> : null}</span>
                {file.added !== undefined ? <LineDelta added={file.added} removed={file.removed!} /> : null}
              </li>
            ))}
          </ul>
        </section>
      );
    case "gallery":
      return (
        <div className="answer-gallery">
          {component.items.map((item, index) => <GalleryItem caption={item.caption} host={host} key={index} path={item.path} />)}
        </div>
      );
    case "tree":
      return <div className="answer-frame"><ul className="answer-tree"><TreeNodes nodes={component.nodes} /></ul></div>;
    case "chart":
      return <ChartFrame chart={component.chart} host={host} />;
    case "flow":
      return <AnswerFlowView flow={component} />;
    case "sequence":
      return <AnswerSequenceView sequence={component} />;
    case "callout": {
      const Icon = CALLOUT_ICONS[component.tone];
      return (
        <aside className="answer-callout" data-tone={component.tone}>
          <Icon aria-hidden="true" className="answer-callout-icon" size={16} />
          <div>
            {component.title ? <strong>{component.title}</strong> : null}
            {host.renderMarkdown(component.body)}
          </div>
        </aside>
      );
    }
    case "details":
      return (
        <details className="answer-details">
          <summary>{component.title}</summary>
          <div className="answer-details-body">{host.renderMarkdown(component.body)}</div>
        </details>
      );
    case "tabs":
      return <AnswerTabs host={host} sections={component.sections} />;
    case "columns":
      return (
        <div className="answer-columns">
          {component.sections.map((section, index) => (
            <section key={index}><h4>{section.title}</h4>{host.renderMarkdown(section.body)}</section>
          ))}
        </div>
      );
  }
}

// Catalog keys of finding severities.
// 发现项严重度的目录键。
const SEVERITY_KEYS = {
  high: "answerComponents.severity.high",
  medium: "answerComponents.severity.medium",
  low: "answerComponents.severity.low",
  info: "answerComponents.severity.info",
} as const;

// Symbols of checklist statuses.
// 清单状态的符号。
const STATUS_SYMBOLS = { ok: "✓", no: "✗", warn: "!", todo: "" } as const;

// Icons of callout tones.
// 提示框色调的图标。
const CALLOUT_ICONS = { info: Info, success: CircleCheck, warning: TriangleAlert, danger: CircleX } as const;

// Icons of compare and tag tones.
// 对比与标签色调的图标。
const TONE_ICONS = { ok: CircleCheck, warn: CircleAlert, no: CircleX } as const;

// A toned cell: its tone icon followed by the remaining text.
// 带色调单元格：色调图标后接其余文字。
function Toned({ value }: { value: AnswerTonedText }) {
  const Icon = value.tone ? TONE_ICONS[value.tone] : null;
  return <>{Icon ? <Icon aria-label={value.tone} className="answer-tone-icon" size={14} /> : null}{value.text}</>;
}

// Markdown text of a toned cell for copying.
// 复制用的带色调单元格 Markdown 文本。
function tonedMarkdown(value: AnswerTonedText): string {
  const symbol = value.tone ? STATUS_SYMBOLS[value.tone] : "";
  return [symbol, value.text].filter(Boolean).join(" ");
}

// Added and removed line counts.
// 新增与删除行数。
function LineDelta({ added, removed }: { added: number; removed: number }) {
  return (
    <span className="answer-delta">
      <span data-kind="added">+{formatChartNumber(added)}</span><span data-kind="removed">−{formatChartNumber(removed)}</span>
    </span>
  );
}

// A workspace file reference shown as a path chip; it links only when the host can resolve it.
// 以路径标签显示的工作区文件引用；仅当宿主可解析时才成为链接。
function FileLink({ host, location }: { host: AnswerComponentHost; location: string }) {
  const href = host.fileHref(location);
  return href
    ? <a className="answer-path" href={href} rel="noreferrer" target="_blank">{location}</a>
    : <code className="answer-path">{location}</code>;
}

// Recursive tree rows with plan markers.
// 带计划标记的递归树行。
function TreeNodes({ nodes }: { nodes: AnswerTreeNode[] }) {
  return (
    <>
      {nodes.map((node, index) => (
        <li data-mark={node.mark} key={index}>
          <span className="answer-tree-label">{node.label}</span>
          {node.note ? <small>{node.note}</small> : null}
          {node.children.length > 0 ? <ul><TreeNodes nodes={node.children} /></ul> : null}
        </li>
      ))}
    </>
  );
}

// One gallery image or video; clicking opens the shared media viewer when available.
// 单个图库图片或视频；可用时点击打开共享媒体查看器。
function GalleryItem({ caption, host, path }: { caption?: string; host: AnswerComponentHost; path: string }) {
  const source = host.mediaSource(path);
  const video = /\.(?:mp4|webm|mov|m4v)(?:[?#].*)?$/iu.test(path);
  const name = path.split(/[\\/]/u).filter(Boolean).pop() ?? path;
  const preview = source === null
    ? <code className="answer-path">{path}</code>
    : video
      ? <span className="answer-gallery-video"><video muted preload="metadata" src={source} /><Play aria-hidden="true" size={18} /></span>
      : <img alt={caption ?? name} decoding="async" loading="lazy" src={source} />;
  const open = source !== null && host.onOpenMedia
    ? () => host.onOpenMedia?.({ kind: video ? "video" : "image", fileName: caption ?? name, localPath: source })
    : undefined;
  return (
    <figure>
      {open ? <button className="answer-gallery-media" onClick={open} type="button">{preview}</button> : <div className="answer-gallery-media">{preview}</div>}
      {caption ? <figcaption>{caption}</figcaption> : null}
    </figure>
  );
}

// Tabbed Markdown sections; the first tab starts selected.
// 页签式 Markdown 分节；默认选中第一个页签。
function AnswerTabs({ host, sections }: { host: AnswerComponentHost; sections: { title: string; body: string }[] }) {
  const [selected, setSelected] = React.useState(0);
  const id = React.useId();
  return (
    <div className="answer-tabs">
      <div className="answer-tablist" role="tablist">
        {sections.map((section, index) => (
          <button aria-controls={`${id}-panel`} aria-selected={index === selected} id={`${id}-tab-${index}`} key={index}
            onClick={() => setSelected(index)} role="tab" type="button">{section.title}</button>
        ))}
      </div>
      <div aria-labelledby={`${id}-tab-${selected}`} className="answer-tabpanel" id={`${id}-panel`} role="tabpanel">
        {host.renderMarkdown(sections[selected].body)}
      </div>
    </div>
  );
}

// Chart frame: title strip, chart/data toggle and copy-data action.
// 图表外框：标题条、图表/数据切换与复制数据操作。
function ChartFrame({ chart, host }: { chart: Extract<AnswerComponent, { kind: "chart" }>["chart"]; host: AnswerComponentHost }) {
  const { t } = useI18n();
  const [data, setData] = React.useState(false);
  const header = [chart.categoryLabel, ...chart.series];
  const rows = chart.rows.map((row) => [row.category, ...row.values.map(formatChartNumber)]);
  return (
    <section className="answer-frame answer-chart">
      <header className="answer-frame-head">
        <span className="answer-frame-label">{chart.title ? <strong>{chart.title}</strong> : null}</span>
        <span className="answer-frame-actions">
          <span className="answer-segmented">
            <button aria-pressed={!data} onClick={() => setData(false)} type="button">{t("answerComponents.chartView")}</button>
            <button aria-pressed={data} onClick={() => setData(true)} type="button">{t("answerComponents.dataView")}</button>
          </span>
          <CopyAction host={host} label={t("answerComponents.copyData")} text={markdownTable(header, rows)} />
        </span>
      </header>
      <div className="answer-chart-body">
        {data ? (
          <div className="answer-table-scroll">
            <table className="answer-table">
              <thead><tr>{header.map((cell, index) => <th data-numeric={index > 0 || undefined} key={index}>{cell}</th>)}</tr></thead>
              <tbody>
                {rows.map((row, index) => (
                  <tr key={index}>{row.map((cell, column) => <td data-numeric={column > 0 || undefined} key={column}>{cell}</td>)}</tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : <AnswerChartView chart={chart} />}
      </div>
    </section>
  );
}
