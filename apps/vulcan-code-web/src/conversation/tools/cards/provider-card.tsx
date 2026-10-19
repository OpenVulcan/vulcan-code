// Provider card for model-extension tools. A web search shows its hits as a source list; a page extraction lists
// each fetched page, opening its text on demand, and the sources that failed; media understanding shows the answer;
// generation names the prompt and model while the produced files follow the card as media.
// 模型扩展工具的供应商卡。网页搜索以来源列表展示命中；网页提取逐个列出已抓取页面（按需展开正文）与失败来源；
// 媒体理解展示回答；生成类说明提示词与模型，产出的文件以媒体形式紧随卡片之后。
import * as React from "react";
import {
  AudioLines, ChevronRight, Clapperboard, Film, Globe, ImagePlus, Images, Mic, Music, ScanEye, Search, SlidersHorizontal,
  Volume2, type LucideIcon,
} from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/ui-kit/primitives/collapsible";
import { ToolFrame, type ToolCardProps, type ToolFamily } from "../tool-card";
import { listOf, objectOf, textOf, type ToolCall } from "../tool-call";
import { ErrorNote, MarkdownResult } from "../views/result-views";

// ProviderToolSpec fixes how one model-extension tool is named and which arguments describe its request.
// ProviderToolSpec 固定一个模型扩展工具的命名方式以及描述其请求的参数。
interface ProviderToolSpec {
  // Purpose family.
  // 用途家族。
  family: ToolFamily;
  // Head icon.
  // 标题图标。
  icon: LucideIcon;
  // Action verb.
  // 动作词。
  verb: TranslationKey;
  // Arguments that carry the request text, in priority order.
  // 承载请求文本的参数，按优先级排列。
  request: readonly string[];
}

// PROMPT_ARGUMENTS are the request-text arguments shared by generation tools.
// PROMPT_ARGUMENTS 是生成类工具共有的请求文本参数。
const PROMPT_ARGUMENTS = ["prompt", "instruction", "text", "lyrics"] as const;

// UNDERSTAND_ARGUMENTS are the request-text arguments shared by understanding tools.
// UNDERSTAND_ARGUMENTS 是理解类工具共有的请求文本参数。
const UNDERSTAND_ARGUMENTS = ["prompt", "instruction", "task"] as const;

// PROVIDER_TOOLS is keyed by canonical tool name; only these tools use the provider card.
// PROVIDER_TOOLS 以规范工具名为键；只有这些工具使用供应商卡。
const PROVIDER_TOOLS: Readonly<Record<string, ProviderToolSpec>> = {
  web_search: { family: "web", icon: Search, request: ["query"], verb: "toolCard.provider.search" },
  web_extract: { family: "web", icon: Globe, request: [], verb: "toolCard.provider.extract" },
  image_understand: { family: "read", icon: ScanEye, request: UNDERSTAND_ARGUMENTS, verb: "toolCard.provider.understandImage" },
  video_understand: { family: "read", icon: Film, request: UNDERSTAND_ARGUMENTS, verb: "toolCard.provider.understandVideo" },
  audio_understand: { family: "read", icon: AudioLines, request: UNDERSTAND_ARGUMENTS, verb: "toolCard.provider.understandAudio" },
  speech_transcribe: { family: "read", icon: Mic, request: [], verb: "toolCard.provider.transcribe" },
  image_generate: { family: "media", icon: ImagePlus, request: PROMPT_ARGUMENTS, verb: "toolCard.provider.imageGenerate" },
  image_edit: { family: "media", icon: Images, request: PROMPT_ARGUMENTS, verb: "toolCard.provider.imageEdit" },
  video_generate: { family: "media", icon: Clapperboard, request: PROMPT_ARGUMENTS, verb: "toolCard.provider.videoGenerate" },
  video_edit: { family: "media", icon: Film, request: PROMPT_ARGUMENTS, verb: "toolCard.provider.videoEdit" },
  video_extend: { family: "media", icon: Film, request: PROMPT_ARGUMENTS, verb: "toolCard.provider.videoExtend" },
  speech_synthesize: { family: "media", icon: Volume2, request: PROMPT_ARGUMENTS, verb: "toolCard.provider.speechSynthesize" },
  music_generate: { family: "media", icon: Music, request: PROMPT_ARGUMENTS, verb: "toolCard.provider.musicGenerate" },
  music_cover: { family: "media", icon: Music, request: PROMPT_ARGUMENTS, verb: "toolCard.provider.musicCover" },
  get_model_extension_parameters: { family: "read", icon: SlidersHorizontal, request: [], verb: "toolCard.provider.parameters" },
};

// providerToolSpec returns the spec of one model-extension tool, or null for any other tool.
// providerToolSpec 返回模型扩展工具的规格；其他工具返回 null。
export function providerToolSpec(call: ToolCall): ProviderToolSpec | null {
  return Object.hasOwn(PROVIDER_TOOLS, call.name) ? PROVIDER_TOOLS[call.name] : null;
}

// providerArguments reads the model-native arguments: executors wrap them as {model, args}, direct tools do not.
// providerArguments 读取模型原生参数：执行工具以 {model, args} 包装，直连工具不包装。
function providerArguments(call: ToolCall): Record<string, unknown> {
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

// LINK is the host's escaped-label, angle-destination Markdown link.
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

// stripReportHeading removes the "# image understanding" or "# Result" heading the card already names.
// stripReportHeading 移除卡片标题已表达的 "# image understanding" 或 "# Result" 标题。
export function stripReportHeading(text: string): string {
  return text.replace(/^# (?:\w+ understanding|Result)\n+/, "");
}

/** Renders one web search, page extraction, media understanding or generation call.
 * 渲染一次网页搜索、网页提取、媒体理解或生成调用。 */
export function ProviderCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const spec = providerToolSpec(call);
  const args = providerArguments(call);
  const output = call.isError ? "" : call.modelOutput ?? "";
  const search = React.useMemo(() => (call.name === "web_search" && output ? searchReport(output) : null), [call.name, output]);
  const extract = React.useMemo(() => (call.name === "web_extract" && output ? extractReport(output) : null), [call.name, output]);
  const request = (spec?.request ?? []).map((key) => textOf(args[key])?.trim()).find(Boolean) ?? "";
  const model = textOf(call.input.model) ?? (call.name === "get_model_extension_parameters" ? null : textOf(args.model));
  const outputs = call.host?.kind === "generated_attachments" ? listOf(call.host.payload.outputs).length : 0;
  let target = request;
  if (call.name === "web_extract") target = (listOf(args.urls).filter((url) => typeof url === "string") as string[]).concat(textOf(args.url) ?? []).join(" · ");
  if (call.name === "get_model_extension_parameters") target = [textOf(args.capability), textOf(args.model)].filter(Boolean).join(" · ");
  if (!target) target = mediaSource(args);
  const meta: React.ReactNode[] = [];
  if (search) meta.push(<span key="hits">{t("toolCard.provider.hits").replace("{count}", String(search.hits.length))}</span>);
  if (extract) {
    meta.push(<span key="pages">{t("toolCard.provider.pages").replace("{count}", String(extract.pages.length))}</span>);
    if (extract.failures.length > 0) meta.push(<span className="vk-del" key="failed">{t("toolCard.provider.failed").replace("{count}", String(extract.failures.length))}</span>);
  }
  if (outputs > 0) meta.push(<span key="files">{t("toolCard.provider.files").replace("{count}", String(outputs))}</span>);
  if (model) meta.push(<span className="vk-truncate" key="model" title={model}>{model}</span>);
  return (
    <ToolFrame
      {...props}
      icon={spec?.icon ?? Globe}
      meta={meta.length > 0 ? meta : null}
      state={call.state}
      target={target}
      targetTitle={target}
      verb={spec ? t(spec.verb) : call.event.tool_name}
    >
      {call.isError ? <ErrorNote text={call.modelOutput} />
        : search ? <SearchHits hits={search.hits} />
          : extract ? <ExtractedPages failures={extract.failures} pages={extract.pages} />
            : spec?.family === "media" ? (request ? <p className="vk-tool-note">{request}</p> : null)
              : output.trim() ? <MarkdownResult text={stripReportHeading(output)} /> : null}
    </ToolFrame>
  );
}

// MEDIA_SOURCE_ARGUMENTS name the media a request reads, used as the target when it has no request text.
// MEDIA_SOURCE_ARGUMENTS 指明请求读取的媒体；请求没有文本时用作目标。
const MEDIA_SOURCE_ARGUMENTS = ["audio_path", "image_path", "video_path", "source_video_path", "media_paths", "source_paths"] as const;

// mediaSource reads the media a request reads.
// mediaSource 读取请求所读取的媒体。
function mediaSource(args: Record<string, unknown>): string {
  for (const key of MEDIA_SOURCE_ARGUMENTS) {
    const value = args[key];
    const text = typeof value === "string" ? value : listOf(value).filter((item) => typeof item === "string").join(" · ");
    if (text.trim()) return text.trim();
  }
  return "";
}

/** Lists web search hits as sources.
 * 将网页搜索命中列为来源。 */
function SearchHits(props: { hits: SearchHit[] }) {
  const { t } = useI18n();
  if (props.hits.length === 0) return <p className="vk-tool-note">{t("toolCard.provider.noHits")}</p>;
  return (
    <ol className="vk-sources">
      {props.hits.map((hit) => {
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
      })}
    </ol>
  );
}

/** Lists extracted pages, each opening its text on demand, followed by the sources that failed.
 * 列出已提取页面（各自按需展开正文），随后列出失败的来源。 */
function ExtractedPages(props: { pages: ExtractedPage[]; failures: ExtractFailure[] }) {
  const { t } = useI18n();
  if (props.pages.length === 0 && props.failures.length === 0) return <p className="vk-tool-note">{t("toolCard.provider.noPages")}</p>;
  return (
    <ol className="vk-sources">
      {props.pages.map((page, index) => {
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
      })}
      {props.failures.map((failure, index) => {
        const { host, rest } = sourceParts(failure.url);
        return (
          <li className="vk-source" data-failed="" key={`failed:${index}:${failure.url}`}>
            <span className="vk-source-url vk-truncate" title={failure.url}><b>{host}</b>{rest}</span>
            <p className="vk-source-snippet">{failure.error}</p>
          </li>
        );
      })}
    </ol>
  );
}
