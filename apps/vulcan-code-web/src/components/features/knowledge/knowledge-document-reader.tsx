import * as React from "react";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { Button } from "@/components/ui/button";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { useI18n } from "@/i18n";
import { readKnowledgeChangeDocument, readManagedKnowledgeDocument } from "@/service/knowledge-base-service";
import type { KnowledgeChangeSide, KnowledgeDocumentContent } from "@/types/knowledge-base";
import { KnowledgeReadFailure } from "./knowledge-feedback";
import { useKnowledgeRead } from "./use-knowledge-read";

// Exact physical library/document owner for current read-only content.
// 当前只读内容的精确物理库及文档所有者。
interface KnowledgeDocumentOwner {
  // Owning library UUID.
  // 所属库 UUID。
  libraryId: string;
  // Stable document UUID, never a display title or guessed path.
  // 稳定文档 UUID，绝非展示标题或推测路径。
  documentId: string;
}

// Reads the current document through its exact owner; returns a shared bounded reader.
// 通过精确所有者读取当前文档；返回共享有界阅读器。
// Parameters: libraryId/documentId identify one current document. No content can be edited here.
// 参数：libraryId 及 documentId 标识一个当前文档。此处不能编辑内容。
export function KnowledgeCurrentDocumentReader({ libraryId, documentId }: KnowledgeDocumentOwner) {
  // The loader uses only the declared document ID selector.
  // 加载器仅使用声明文档身份选择器。
  const load = React.useCallback((startLine: number) => readManagedKnowledgeDocument({ libraryId, documentId, startLine }), [libraryId, documentId]);
  // Owner changes remount line pagination before another document can be requested.
  // 所有者变化在请求另一文档之前重新挂载行分页。
  const scope = JSON.stringify([libraryId, documentId]);
  return <KnowledgeDocumentWindow key={scope} scope={scope} loadWindow={load} />;
}

// An immutable history side extends the exact physical owner without a current-content alternative.
// 不可变历史侧别扩展精确物理所有者，不提供当前内容替代。
interface KnowledgeHistoricalOwner extends KnowledgeDocumentOwner {
  // Exact immutable change sequence.
  // 精确不可变变更序号。
  seq: number;
  // Exact before or after image.
  // 精确前镜像或后镜像。
  side: KnowledgeChangeSide;
}

// Reads the exact historical side; returns the same line-window interaction as current content.
// 读取精确历史侧别；返回与当前内容相同的行窗口交互。
// Parameters: owner supplies library/document/change/side IDs. Missing images remain explicit service errors.
// 参数：owner 提供库、文档、变更及侧别身份。缺失镜像保持明确服务错误。
export function KnowledgeHistoricalDocumentReader({ libraryId, documentId, seq, side }: KnowledgeHistoricalOwner) {
  // Every historical call goes only to the history-body endpoint.
  // 每次历史调用仅访问历史正文端点。
  const load = React.useCallback((startLine: number) => readKnowledgeChangeDocument({ libraryId, documentId, seq, side, startLine }), [libraryId, documentId, seq, side]);
  // Each immutable side owns an independent pagination lifetime.
  // 每个不可变侧别拥有独立分页寿命。
  const scope = JSON.stringify([libraryId, documentId, seq, side]);
  return <KnowledgeDocumentWindow key={scope} scope={scope} loadWindow={load} />;
}

// A body window consumes a single declared current/history loader.
// 正文窗口消费单个声明的当前或历史加载器。
interface KnowledgeDocumentWindowProps {
  // Immutable owner identity for this mounted read surface.
  // 此挂载读取区域的不可变所有者身份。
  scope: string;
  // Reads an inclusive first line; the service owns the maximum window size.
  // 读取包含边界首行；最大窗口大小由服务拥有。
  loadWindow: (startLine: number) => Promise<KnowledgeDocumentContent>;
}

// Renders exact text or the existing shared Markdown pipeline with explicit line pagination.
// 使用精确文本或既有共享 Markdown 链路渲染，并提供显式行分页。
// Parameters: props supplies one stable loader and owner. Returns metadata plus one selectable body window.
// 参数：props 提供一个稳定加载器及所有者。返回元数据及一个可选择正文窗口。
function KnowledgeDocumentWindow({ scope, loadWindow }: KnowledgeDocumentWindowProps) {
  // All visible labels use shared catalogs.
  // 全部可见标签使用共享目录。
  const { t } = useI18n();
  // Prior starts are explicit service continuations; page size is never duplicated in this client.
  // 先前首行均为明确服务续读值；此客户端不复制页大小。
  const [starts, setStarts] = React.useState([1]);
  // Source preserves original whitespace; rendered mode reuses conversation-markdown.
  // 源码保留原空白；渲染模式复用 conversation-markdown。
  const [rendered, setRendered] = React.useState(false);
  // The last explicit start owns the current body read.
  // 最后一个明确首行拥有当前正文读取。
  const start = starts[starts.length - 1];
  // Stable loader identity changes only with the exact owner or requested window.
  // 稳定加载器身份仅随精确所有者或请求窗口变化。
  const load = React.useCallback(() => loadWindow(start), [loadWindow, start]);
  // Superseded windows cannot repaint the current document or side.
  // 已替代窗口不能重绘当前文档或侧别。
  const read = useKnowledgeRead(JSON.stringify([scope, start]), load);
  return <section className="min-w-0 space-y-3">
    {read.loading ? <StreamPlaceholder label={t("knowledge.viewer.loadingDocument")} /> : null}
    {read.error !== null ? <KnowledgeReadFailure error={read.error} onRetry={() => { if (start === 1) void read.refresh(); else setStarts([1]); }} /> : null}
    {read.value !== null ? <>
      <header><strong>{read.value.document.title}</strong><p className="break-words text-sm text-muted-foreground">{read.value.document.path}</p><p className="select-text text-sm">{read.value.document.summary}</p></header>
      <details><summary>{t("knowledge.history.metadata")}</summary><pre className="max-h-64 select-text overflow-auto whitespace-pre-wrap break-words text-xs">{JSON.stringify(read.value.document, null, 2)}</pre></details>
      <div className="flex flex-wrap items-center gap-2 text-sm">
        <span>{t("knowledge.reader.revision").replace("{revision}", String(read.value.document.revision))}</span>
        <span>{read.value.document.lineCount === 0 ? t("knowledge.reader.empty") : t("knowledge.reader.range").replace("{start}", String(read.value.startLine)).replace("{end}", String(read.value.endLine)).replace("{total}", String(read.value.document.lineCount))}</span>
        <Button size="sm" variant={rendered ? "outline" : "secondary"} onClick={() => setRendered(false)}>{t("knowledge.reader.source")}</Button>
        <Button size="sm" variant={rendered ? "secondary" : "outline"} onClick={() => setRendered(true)}>{t("knowledge.reader.markdown")}</Button>
      </div>
      <div className="knowledge-document-window select-text overflow-auto rounded-md border p-3">{rendered ? <MarkdownMessage content={read.value.content} eager /> : <pre className="whitespace-pre-wrap break-words text-sm"><code>{read.value.content}</code></pre>}</div>
      <div className="flex gap-2">
        <Button size="sm" variant="outline" disabled={starts.length === 1} onClick={() => setStarts((current) => current.slice(0, -1))}>{t("knowledge.page.previous")}</Button>
        <Button size="sm" variant="outline" disabled={read.value.nextLine === null} onClick={() => {
          // Capture the validated continuation before the state updater runs.
          // 在状态更新器运行前捕获已验证续读值。
          const next = read.value === null ? null : read.value.nextLine;
          if (next !== null) setStarts((current) => [...current, next]);
        }}>{t("knowledge.page.next")}</Button>
      </div>
    </> : null}
  </section>;
}
