// Memory search card. The head names the queries and how many memories came back; the body pages through the
// memories one at a time in rank order, each with the query it answered. Like VMM's own search result it shows no
// score: rerank scores are model-specific, so a raw number would read as a false relevance.
// 记忆搜索卡。标题说明查询及返回的记忆数；正文按排名逐条翻阅记忆，每条附带其回答的查询。与 VMM 自身搜索结果一致
// 不显示评分：重排分由模型自定义尺度，原始数值会被误读为相关度。
import * as React from "react";
import { Brain, ChevronLeft, ChevronRight } from "lucide-react";
import { useI18n } from "@/i18n";
import { Button } from "@/ui-kit/primitives/button";
import { ToolFrame, type ToolCardProps } from "../tool-card";
import { listOf } from "../tool-call";
import { ErrorNote, MarkdownResult } from "../views/result-views";

// MemorySearchPage is one returned memory of a search result.
// MemorySearchPage 是搜索结果中返回的一条记忆。
export interface MemorySearchPage {
  // Query the memory answered.
  // 该记忆所回答的查询。
  query: string;
  // Durable memory id.
  // 长期记忆标识。
  memoryId: string;
  // Readable memory body below its heading.
  // 标题下方的可读记忆正文。
  body: string;
}

// QUERY_HEADING and MEMORY_HEADING match the headings written by the VMM result renderer
// (crates/vulcan-agent-runtime/src/tooling/native/vmm/result_markdown.rs).
// QUERY_HEADING 与 MEMORY_HEADING 匹配 VMM 结果渲染器写出的标题。
const QUERY_HEADING = /^## Query \d+: (`+) ?(.*?) ?\1$/;
const MEMORY_HEADING = /^### Memory (\d+)$/;

/** Splits a memory search result into one page per returned memory, in result order.
 * 将记忆搜索结果按结果顺序拆分为每条记忆一页。 */
export function memorySearchPages(markdown: string): MemorySearchPage[] {
  const pages: MemorySearchPage[] = [];
  let query = "";
  let current: { page: MemorySearchPage; lines: string[] } | null = null;
  const close = () => {
    if (current) pages.push({ ...current.page, body: current.lines.join("\n").trim() });
    current = null;
  };
  for (const line of markdown.split(/\r?\n/)) {
    const queryHeading = QUERY_HEADING.exec(line);
    if (queryHeading) {
      close();
      query = queryHeading[2];
      continue;
    }
    const memoryHeading = MEMORY_HEADING.exec(line);
    if (memoryHeading) {
      close();
      current = { lines: [], page: { body: "", memoryId: memoryHeading[1], query } };
      continue;
    }
    current?.lines.push(line);
  }
  close();
  return pages;
}

/** Renders one vmm_memory_search call.
 * 渲染一次 vmm_memory_search 调用。 */
export function MemorySearchCard(props: ToolCardProps) {
  const { t } = useI18n();
  const { call } = props;
  const pages = React.useMemo(() => memorySearchPages(call.modelOutput ?? ""), [call.modelOutput]);
  const [index, setIndex] = React.useState(0);
  const queries = listOf(call.input.queries).filter((query): query is string => typeof query === "string");
  const target = queries.join(" · ").replace(/\s+/g, " ");
  const page = pages[Math.min(index, pages.length - 1)];
  const finished = call.state === "done" && !call.isError;
  return (
    <ToolFrame
      {...props}
      icon={Brain}
      meta={finished ? <span>{t("toolCard.memory.count").replace("{count}", String(pages.length))}</span> : null}
      state={call.state}
      target={target}
      targetTitle={target}
      verb={t("toolCard.doc.memorySearch")}
    >
      {call.isError ? <ErrorNote text={call.modelOutput} />
        : page ? (
          <div className="vk-memory-page">
            <div className="vk-memory-page-head">
              <span className="vk-memory-page-id">{t("toolCard.memory.id").replace("{id}", page.memoryId)}</span>
              {queries.length > 1 && page.query ? <span className="vk-memory-page-query" title={page.query}>{page.query}</span> : null}
              {pages.length > 1 ? (
                <span className="vk-memory-pager">
                  <Button aria-label={t("toolCard.memory.previous")} disabled={index <= 0}
                    onClick={() => setIndex((value) => Math.max(0, value - 1))} size="icon-xs" type="button" variant="ghost">
                    <ChevronLeft size={12} />
                  </Button>
                  <span>{index + 1} / {pages.length}</span>
                  <Button aria-label={t("toolCard.memory.next")} disabled={index >= pages.length - 1}
                    onClick={() => setIndex((value) => Math.min(pages.length - 1, value + 1))} size="icon-xs" type="button" variant="ghost">
                    <ChevronRight size={12} />
                  </Button>
                </span>
              ) : null}
            </div>
            {page.body ? <MarkdownResult text={page.body} /> : null}
          </div>
        )
        : finished ? <MarkdownResult text={t("toolCard.memory.none")} /> : null}
    </ToolFrame>
  );
}
