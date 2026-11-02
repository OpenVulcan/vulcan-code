// Web search and page extraction cards, shared by the model-native tools (web_search, web_extract) and the host
// provider targets (vulcan_web_search, vulcan_web_extract), which all return the same stable Markdown report. The
// search head spells out the query's operators; both bodies list their sources and fold long lists.
// 网页搜索与网页提取卡，由模型原生工具（web_search、web_extract）与宿主供应商目标（vulcan_web_search、
// vulcan_web_extract）共用，它们都返回同一份稳定的 Markdown 报告。搜索标题拆出查询运算符；两种正文都列出来源并折叠长列表。
import * as React from "react";
import { ChevronRight, Globe, Search } from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";
import { Button } from "@/ui-kit/primitives/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/ui-kit/primitives/collapsible";
import { ToolFrame, type ToolCardProps } from "../tool-card";
import { listOf, numberOf, objectOf, textOf, type ToolCall } from "../tool-call";
import { ErrorNote, MarkdownResult } from "../views/result-views";

// WebToolKind names which of the two web reports a tool returns.
// WebToolKind 指明工具返回两种网页报告中的哪一种。
export type WebToolKind = "search" | "extract";

// WEB_TOOLS is keyed by canonical tool name; only these tools use the web cards.
// WEB_TOOLS 以规范工具名为键；只有这些工具使用网页卡。
const WEB_TOOLS: Readonly<Record<string, WebToolKind>> = {
  web_search: "search",
  vulcan_web_search: "search",
  web_extract: "extract",
  vulcan_web_extract: "extract",
};

// webToolKind returns the web report a call returns, or null for any other tool.
// webToolKind 返回调用返回的网页报告类型；其他工具返回 null。
export function webToolKind(call: ToolCall): WebToolKind | null {
  return Object.hasOwn(WEB_TOOLS, call.name) ? WEB_TOOLS[call.name] : null;
}

// webArguments reads the tool arguments: model-native executors wrap them as {model, args}, provider targets do not.
// webArguments 读取工具参数：模型原生执行工具以 {model, args} 包装，供应商目标不包装。
function webArguments(call: ToolCall): Record<string, unknown> {
  return objectOf(call.input.args) ?? call.input;
}

// SearchHit is one ranked web search result.
// SearchHit 是一条排序后的网页搜索结果。
export interface SearchHit {
  // Provider rank.
  // 供应商排名。
  rank: number;
  // Source title.
  // 来源标题。
  title: string;
  // Source address.
  // 来源地址。
  url: string;
  // Result snippet.
  // 结果摘要。
  snippet: string;
}

// ExtractedPage is one fetched page.
// ExtractedPage 是一个已抓取的页面。
export interface ExtractedPage {
  // Page address.
  // 页面地址。
  url: string;
  // Readable page text.
  // 可读页面正文。
  content: string;
  // Number of image links found on the page.
  // 页面中发现的图片链接数。
  imageCount: number;
}

// ExtractFailure is one source that could not be fetched.
// ExtractFailure 是一个无法抓取的来源。
export interface ExtractFailure {
  // Source address.
  // 来源地址。
  url: string;
  // Provider error.
  // 供应商错误。
  error: string;
}

// LINK is the host's escaped-label, angle-destination Markdown link
// (crates/vulcan-tools-manager/src/result_markdown.rs source_link).
// LINK 是宿主生成的转义标签、尖括号目标 Markdown 链接。
const LINK = String.raw`\[((?:\\.|[^\]\\])*)\]\(<([^>]*)>\)`;
const HIT_LINE = new RegExp(String.raw`^(\d+)\. ${LINK}$`);
const PAGE_HEAD = new RegExp(String.raw`^## ${LINK}$`);
const FAILURE_LINE = new RegExp(String.raw`^- ${LINK}: (.*)$`);
// SECTION_HEAD marks a further top-level section of a joined composite report.
// SECTION_HEAD 标记拼接复合报告中的另一个顶层段落。
const SECTION_HEAD = /^# (?:Search: |Extracted pages$|Result$)/;

// unescapeLabel reverses the host's label escaping.
// unescapeLabel 还原宿主对标签的转义。
function unescapeLabel(label: string): string {
  return label.replace(/\\(.)/g, "$1");
}

// encodeDestination repeats the host's destination encoding so a label can be compared with its link.
// encodeDestination 重复宿主的目标编码，以便将标签与其链接比较。
function encodeDestination(url: string): string {
  return url.replace(/</g, "%3C").replace(/>/g, "%3E").replace(/ /g, "%20").replace(/\r/g, "%0D").replace(/\n/g, "%0A");
}

// unwrapInlineCode removes the backtick fence the host puts around a query.
// unwrapInlineCode 去掉宿主包裹查询的反引号围栏。
function unwrapInlineCode(text: string): string {
  const match = /^(`+) ?([\s\S]*?) ?\1$/.exec(text.trim());
  return match ? match[2] : text.trim();
}

// reportLines splits a report into lines, or null when it does not start with the expected heading or joins
// several reports.
// reportLines 将报告拆分为行；不以预期标题开头或拼接了多份报告时返回 null。
function reportLines(text: string, heading: RegExp): { head: RegExpExecArray; lines: string[] } | null {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  const head = heading.exec(lines[0] ?? "");
  if (!head || lines.slice(1).some((line) => SECTION_HEAD.test(line))) return null;
  return { head, lines: lines.slice(1) };
}

// searchReport parses the stable "# Search:" Markdown a web search returns; null when the text is not one.
// searchReport 解析网页搜索返回的稳定 "# Search:" Markdown；文本不是该格式时返回 null。
export function searchReport(text: string): { query: string; hits: SearchHit[] } | null {
  const report = reportLines(text, /^# Search: (.*)$/);
  if (!report) return null;
  const hits: SearchHit[] = [];
  let snippet: string[] = [];
  const close = () => {
    const last = hits.at(-1);
    if (last) last.snippet = snippet.join("\n").trim();
    snippet = [];
  };
  for (const line of report.lines) {
    const hit = HIT_LINE.exec(line);
    if (hit) {
      close();
      hits.push({ rank: Number(hit[1]), snippet: "", title: unescapeLabel(hit[2]), url: hit[3] });
    } else {
      snippet.push(line);
    }
  }
  close();
  return { hits, query: unwrapInlineCode(report.head[1]) };
}

// IMAGES_TRAILER is the image link list the host appends to a page.
// IMAGES_TRAILER 是宿主附加在页面之后的图片链接列表。
const IMAGES_TRAILER = /\n*Images:\n((?:- .*(?:\n|$))+)$/;

// extractReport parses the stable "# Extracted pages" Markdown a page extraction returns; null when the text is not
// one.
// extractReport 解析网页提取返回的稳定 "# Extracted pages" Markdown；文本不是该格式时返回 null。
export function extractReport(text: string): { pages: ExtractedPage[]; failures: ExtractFailure[] } | null {
  const report = reportLines(text, /^# Extracted pages$/);
  if (!report) return null;
  const pages: ExtractedPage[] = [];
  const failures: ExtractFailure[] = [];
  let body: string[] = [];
  let inFailures = false;
  const close = () => {
    const page = pages.at(-1);
    if (page && !inFailures) {
      const content = body.join("\n").trim();
      const images = IMAGES_TRAILER.exec(content);
      page.content = images ? content.slice(0, images.index).trim() : content;
      page.imageCount = images ? images[1].trim().split("\n").length : 0;
    }
    body = [];
  };
  for (const line of report.lines) {
    if (line === "## Failed sources") {
      close();
      inFailures = true;
      continue;
    }
    if (inFailures) {
      const failure = FAILURE_LINE.exec(line);
      if (failure) failures.push({ error: failure[3], url: failure[2] });
      else if (line.trim() && failures.length > 0) failures[failures.length - 1].error += `\n${line}`;
      continue;
    }
    const head = PAGE_HEAD.exec(line);
    if (head && encodeDestination(unescapeLabel(head[1])) === head[2]) {
      close();
      pages.push({ content: "", imageCount: 0, url: head[2] });
    } else {
      body.push(line);
    }
  }
  close();
  return { failures, pages };
}

// QueryToken is one piece of a search query: a plain term, a quoted phrase, or a key:value operator; a leading
// minus negates it.
// QueryToken 是搜索查询的一段：普通词、引号短语或 key:value 运算符；前导减号表示排除。
export interface QueryToken {
  // Token shape.
  // 片段形态。
  kind: "term" | "phrase" | "operator";
  // Whether a leading minus excludes it.
  // 前导减号是否将其排除。
  negated: boolean;
  // Lower-cased operator name; empty for terms and phrases.
  // 小写运算符名；普通词与短语为空。
  key: string;
  // Operator value, phrase text or term.
  // 运算符值、短语文本或词。
  value: string;
}

// QUERY_PIECE matches one optionally negated, optionally keyed, possibly quoted piece of a query.
// QUERY_PIECE 匹配查询中一段可带减号、可带键、可带引号的片段。
const QUERY_PIECE = /(-?)(?:([A-Za-z][\w-]*):(?!\/\/))?("[^"]*"?|\S+)/g;

/** Splits a query into terms, phrases and operators the way search engines read it; an address such as
 * https://… stays one term.
 * 按搜索引擎的理解将查询拆分为词、短语与运算符；https://… 之类的地址保持为一个词。 */
export function queryTokens(query: string): QueryToken[] {
  const tokens: QueryToken[] = [];
  for (const match of query.matchAll(QUERY_PIECE)) {
    const [, minus, key, body] = match;
    const quoted = body.startsWith("\"");
    const value = quoted ? body.replace(/^"|"$/g, "") : body;
    if (!value) continue;
    if (key) tokens.push({ key: key.toLowerCase(), kind: "operator", negated: minus === "-", value });
    else if (quoted) tokens.push({ key: "", kind: "phrase", negated: minus === "-", value });
    else tokens.push({ key: "", kind: "term", negated: minus === "-", value });
  }
  return tokens;
}

// OPERATOR_LABELS name the operators the provider tool descriptions document; others show their own key.
// OPERATOR_LABELS 为供应商工具描述中记载的运算符命名；其他运算符显示自身的键。
const OPERATOR_LABELS: Readonly<Record<string, TranslationKey>> = {
  site: "toolCard.web.op.site",
  lang: "toolCard.web.op.lang",
  after: "toolCard.web.op.after",
  before: "toolCard.web.op.before",
  filetype: "toolCard.web.op.filetype",
};

/** Renders a query in the head with its operators and phrases marked.
 * 在标题中渲染查询，并标出运算符与短语。 */
function QueryTarget(props: { tokens: QueryToken[] }) {
  return props.tokens.map((token, index) => (
    <React.Fragment key={index}>
      {index > 0 ? " " : null}
      {token.kind === "term"
        ? `${token.negated ? "-" : ""}${token.value}`
        : (
          <span className="vk-web-op" data-kind={token.kind} data-negated={token.negated ? "" : undefined}>
            {token.kind === "operator" ? <i>{`${token.negated ? "-" : ""}${token.key}:`}</i> : null}
            {token.kind === "phrase" ? `${token.negated ? "-" : ""}"${token.value}"` : token.value}
          </span>
        )}
    </React.Fragment>
  ));
}

/** Lists the query's operators and phrases as labelled facts, so a filter reads without knowing the syntax.
 * 将查询的运算符与短语列为带标签的事实，使不熟悉语法也能读懂过滤条件。 */
function QueryFacts(props: { tokens: QueryToken[] }) {
  const { t } = useI18n();
  const filters = props.tokens.filter((token) => token.kind !== "term" || token.negated);
  if (filters.length === 0) return null;
  const terms = props.tokens.filter((token) => token.kind === "term" && !token.negated).map((token) => token.value).join(" ");
  return (
    <dl className="vk-web-query">
      {terms ? <div><dt>{t("toolCard.web.terms")}</dt><dd>{terms}</dd></div> : null}
      {filters.map((token, index) => {
        const label = token.kind === "phrase" ? t("toolCard.web.op.phrase")
          : token.kind === "term" ? t("toolCard.web.op.exclude")
            : Object.hasOwn(OPERATOR_LABELS, token.key) ? t(OPERATOR_LABELS[token.key]) : token.key;
        return (
          <div data-negated={token.negated ? "" : undefined} key={index}>
            <dt>{token.negated && token.kind !== "term" ? `${t("toolCard.web.op.exclude")} · ${label}` : label}</dt>
            <dd>{token.value}</dd>
          </div>
        );
      })}
    </dl>
  );
}

// FOLD_LIMIT is how many sources a list shows before it folds the rest.
// FOLD_LIMIT 是列表折叠其余来源前显示的来源数。
export const FOLD_LIMIT = 5;

/** Shows the first FOLD_LIMIT items of a long list and a toggle for the rest.
 * 显示长列表的前 FOLD_LIMIT 项，并为其余项提供切换。 */
function FoldedList<T>(props: { items: readonly T[]; render: (item: T, index: number) => React.ReactNode }) {
  const { t } = useI18n();
  const [expanded, setExpanded] = React.useState(false);
  const hidden = Math.max(0, props.items.length - FOLD_LIMIT);
  const shown = expanded || hidden === 0 ? props.items : props.items.slice(0, FOLD_LIMIT);
  return (
    <>
      <ol className="vk-sources">{shown.map(props.render)}</ol>
      {hidden > 0 ? (
        <div className="vk-tool-foot">
          <Button aria-expanded={expanded} className="vk-web-fold" onClick={() => setExpanded((value) => !value)} size="xs" type="button" variant="ghost">
            {expanded ? t("toolCard.web.showFewer") : t("toolCard.web.showMore").replace("{count}", String(hidden))}
          </Button>
        </div>
      ) : null}
    </>
  );
}

// sourceParts splits an address into its host and the rest, for compact source rows.
// sourceParts 将地址拆分为主机与其余部分，用于紧凑的来源行。
export function sourceParts(url: string): { host: string; rest: string } {
  try {
    const parsed = new URL(url);
    const rest = `${parsed.pathname === "/" ? "" : parsed.pathname}${parsed.search}`;
    return { host: parsed.hostname.replace(/^www\./, ""), rest: decodeURI(rest) };
  } catch {
    return { host: url, rest: "" };
  }
}

// safeHref returns the address only when it is a web link.
// safeHref 仅当地址为网页链接时返回该地址。
function safeHref(url: string): string | undefined {
  return /^https?:\/\//i.test(url) ? url : undefined;
}

/** Renders one web search call: the head spells out the query, the body lists ranked sources.
 * 渲染一次网页搜索调用：标题拆出查询，正文列出排序后的来源。 */
export function WebSearchCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const args = webArguments(call);
  const output = call.isError ? "" : call.modelOutput ?? "";
  const report = React.useMemo(() => (output ? searchReport(output) : null), [output]);
  const query = textOf(args.query)?.trim() || report?.query || "";
  const tokens = React.useMemo(() => queryTokens(query), [query]);
  const limit = numberOf(args.max_results);
  const meta: React.ReactNode[] = [];
  if (report) meta.push(<span key="hits">{t("toolCard.provider.hits").replace("{count}", String(report.hits.length))}</span>);
  else if (limit !== null && call.state === "running") meta.push(<span key="limit">{t("toolCard.web.limit").replace("{count}", String(limit))}</span>);
  const model = textOf(call.input.model);
  if (model) meta.push(<span className="vk-truncate" key="model" title={model}>{model}</span>);
  const facts = <QueryFacts tokens={tokens} />;
  const hasFacts = tokens.some((token) => token.kind !== "term" || token.negated);
  return (
    <ToolFrame
      {...props}
      icon={Search}
      meta={meta.length > 0 ? meta : null}
      state={call.state}
      target={<QueryTarget tokens={tokens} />}
      targetTitle={query}
      verb={t("toolCard.provider.search")}
    >
      {call.isError ? <>{hasFacts ? facts : null}<ErrorNote text={call.modelOutput} /></>
        : report ? <>{hasFacts ? facts : null}<SearchHits hits={report.hits} /></>
          : output.trim() ? <>{hasFacts ? facts : null}<MarkdownResult text={output} /></>
            : hasFacts ? facts : null}
    </ToolFrame>
  );
}

/** Lists web search hits as sources.
 * 将网页搜索命中列为来源。 */
function SearchHits(props: { hits: SearchHit[] }) {
  const { t } = useI18n();
  if (props.hits.length === 0) return <p className="vk-tool-note" data-empty="">{t("toolCard.provider.noHits")}</p>;
  return (
    <FoldedList
      items={props.hits}
      render={(hit) => {
        const { host, rest } = sourceParts(hit.url);
        return (
          <li className="vk-source" key={`${hit.rank}:${hit.url}`}>
            <span className="vk-source-rank">{hit.rank}</span>
            <div className="vk-source-main">
              <a className="vk-source-title" href={safeHref(hit.url)} rel="noreferrer" target="_blank" title={hit.url}>{hit.title || hit.url}</a>
              <span className="vk-source-url"><b>{host}</b>{rest}</span>
              {hit.snippet ? <p className="vk-source-snippet">{hit.snippet}</p> : null}
            </div>
          </li>
        );
      }}
    />
  );
}

/** Renders one page extraction call: the head names the pages, the body lists each fetched page and each failure.
 * 渲染一次网页提取调用：标题列出页面，正文列出每个已抓取页面与每个失败来源。 */
export function WebExtractCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const args = webArguments(call);
  const output = call.isError ? "" : call.modelOutput ?? "";
  const report = React.useMemo(() => (output ? extractReport(output) : null), [output]);
  const urls = (listOf(args.urls).filter((url) => typeof url === "string") as string[]).concat(textOf(args.url) ?? []);
  const target = urls.join(" · ");
  const meta: React.ReactNode[] = [];
  if (report) {
    meta.push(<span key="pages">{t("toolCard.provider.pages").replace("{count}", String(report.pages.length))}</span>);
    if (report.failures.length > 0) meta.push(<span className="vk-del" key="failed">{t("toolCard.provider.failed").replace("{count}", String(report.failures.length))}</span>);
  }
  const model = textOf(call.input.model);
  if (model) meta.push(<span className="vk-truncate" key="model" title={model}>{model}</span>);
  return (
    <ToolFrame
      {...props}
      icon={Globe}
      meta={meta.length > 0 ? meta : null}
      state={call.state}
      target={target}
      targetTitle={target}
      verb={t("toolCard.provider.extract")}
    >
      {call.isError ? <ErrorNote text={call.modelOutput} />
        : report ? <ExtractedPages failures={report.failures} pages={report.pages} />
          : output.trim() ? <MarkdownResult text={output} /> : null}
    </ToolFrame>
  );
}

// ExtractEntry is one row of the extraction list: a fetched page or a failed source.
// ExtractEntry 是提取列表的一行：已抓取页面或失败来源。
type ExtractEntry = { page: ExtractedPage } | { failure: ExtractFailure };

/** Lists extracted pages, each opening its text on demand, followed by the sources that failed.
 * 列出已提取页面（各自按需展开正文），随后列出失败的来源。 */
function ExtractedPages(props: { pages: ExtractedPage[]; failures: ExtractFailure[] }) {
  const { t } = useI18n();
  if (props.pages.length === 0 && props.failures.length === 0) return <p className="vk-tool-note" data-empty="">{t("toolCard.provider.noPages")}</p>;
  const entries: ExtractEntry[] = [...props.pages.map((page) => ({ page })), ...props.failures.map((failure) => ({ failure }))];
  return (
    <FoldedList
      items={entries}
      render={(entry, index) => {
        if ("failure" in entry) {
          const { host, rest } = sourceParts(entry.failure.url);
          return (
            <li className="vk-source" data-failed="" key={`failed:${index}:${entry.failure.url}`}>
              <span className="vk-source-url vk-truncate" title={entry.failure.url}><b>{host}</b>{rest}</span>
              <p className="vk-source-snippet">{entry.failure.error}</p>
            </li>
          );
        }
        const { page } = entry;
        const { host, rest } = sourceParts(page.url);
        return (
          <li className="vk-source" key={`${index}:${page.url}`}>
            <Collapsible>
              <CollapsibleTrigger className="vk-source-toggle" disabled={!page.content}>
                <ChevronRight aria-hidden="true" className="vk-source-chev" />
                <span className="vk-source-url vk-truncate" title={page.url}><b>{host}</b>{rest}</span>
                <small>{t("toolCard.provider.chars").replace("{count}", page.content.length.toLocaleString())}</small>
              </CollapsibleTrigger>
              <CollapsibleContent className="vk-source-body">
                <MarkdownResult text={page.content} />
              </CollapsibleContent>
            </Collapsible>
          </li>
        );
      }}
    />
  );
}
