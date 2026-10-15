import * as React from "react";
import { FileText, Folder, ChevronDown, ChevronRight } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { useI18n } from "@/i18n";
import { formatKnowledgeIndexStatus } from "@/lib/knowledge-index-status";
import { listManagedKnowledgeDocuments, searchManagedKnowledgeLibrary } from "@/service/knowledge-base-service";
import type { KnowledgeLibrarySummary, KnowledgeListEntry } from "@/types/knowledge-base";
import { KnowledgeCurrentDocumentReader } from "./knowledge-document-reader";
import { KnowledgeReadFailure } from "./knowledge-feedback";
import { KNOWLEDGE_MODE_COPY } from "./knowledge-mode";
import { useKnowledgeRead } from "./use-knowledge-read";

// A directory subtree has one exact physical library, canonical path and document-open callback.
// 目录子树具有一个精确物理库、规范路径及文档打开回调。
interface KnowledgeDirectoryOwner {
  // Physical library UUID.
  // 物理库 UUID。
  libraryId: string;
  // Exact canonical logical directory, with empty string denoting root.
  // 精确规范逻辑目录，空字符串表示根。
  path: string;
  // Opens only the selected current document ID in the same library.
  // 仅在相同库中打开选定当前文档身份。
  onDocument: (documentId: string) => void;
}

// Renders one bounded folder page and lazily mounts expanded subtrees.
// 渲染一个有界目录页，并惰性挂载已展开子树。
// Parameters: owner supplies exact library/path/reader callback. Returns read-only hierarchical entries and explicit pagination.
// 参数：owner 提供精确库、路径及阅读回调。返回只读层级条目及显式分页。
function KnowledgeDirectoryChildren({ libraryId, path, onDocument }: KnowledgeDirectoryOwner) {
  // Labels follow the active locale.
  // 标签遵循当前语言。
  const { t } = useI18n();
  // Opaque cursor history preserves exact server-issued continuations.
  // 不透明游标历史保留精确服务签发续读标识。
  const [cursors, setCursors] = React.useState<(string | null)[]>([null]);
  // The current page uses the final explicit cursor only.
  // 当前页仅使用最后一个明确游标。
  const cursor = cursors[cursors.length - 1];
  // The service owns default page size and maximum depth.
  // 默认页大小及最大深度由服务拥有。
  const load = React.useCallback(() => listManagedKnowledgeDocuments({ libraryId, path, cursor, depth: 1 }), [libraryId, path, cursor]);
  // Read generations are scoped by exact library/path/cursor.
  // 读取代次由精确库、路径及游标限定。
  const read = useKnowledgeRead(JSON.stringify([libraryId, path, cursor]), load);
  return <div className="space-y-2">
    {read.loading ? <StreamPlaceholder label={t("knowledge.librariesLoading")} /> : null}
    {read.error !== null ? <KnowledgeReadFailure error={read.error} onRetry={() => { if (cursor === null) void read.refresh(); else setCursors([null]); }} /> : null}
    {read.value !== null ? <>
      <ul className="space-y-1">{read.value.entries.map((entry) => <li key={entry.kind + ":" + entry.path}>
        {entry.kind === "folder" ? <KnowledgeFolderRow libraryId={libraryId} entry={entry} onDocument={onDocument} />
          : <button type="button" className="flex w-full items-start gap-2 rounded px-2 py-1 text-left hover:bg-accent" onClick={() => onDocument(entry.document.id)}><FileText size={15} /><span className="min-w-0 break-words">{entry.document.title}<small className="block text-muted-foreground">{entry.path}</small></span></button>}
      </li>)}</ul>
      {read.value.entries.length === 0 ? <p className="text-sm text-muted-foreground">{t("knowledge.content.empty")}</p> : null}
      <div className="flex gap-2">
        <Button variant="outline" size="sm" disabled={cursors.length === 1} onClick={() => setCursors((current) => current.slice(0, -1))}>{t("knowledge.page.previous")}</Button>
        <Button variant="outline" size="sm" disabled={read.value.nextCursor === null} onClick={() => {
          // Capture this page's exact issued cursor before scheduling the state transition.
          // 在调度状态过渡前捕获此页精确签发游标。
          const next = read.value === null ? null : read.value.nextCursor;
          if (next !== null) setCursors((current) => [...current, next]);
        }}>{t("knowledge.page.next")}</Button>
        <Button variant="ghost" size="sm" onClick={() => { if (cursor === null) void read.refresh(); else setCursors([null]); }}>{t("knowledge.refresh")}</Button>
      </div>
    </> : null}
  </div>;
}

// Renders one exact folder; child reads begin only after conscious expansion.
// 渲染一个精确目录；仅在主动展开后开始子项读取。
// Parameters: entry is a proven folder row, libraryId owns it, onDocument retains the same ceiling. Returns a lazy subtree.
// 参数：entry 为已证明目录行，libraryId 拥有该行，onDocument 保持相同上限。返回惰性子树。
function KnowledgeFolderRow({ entry, libraryId, onDocument }: { entry: Extract<KnowledgeListEntry, { kind: "folder" }>; libraryId: string; onDocument: (documentId: string) => void }) {
  // Expansion belongs to this exact folder/head-mounted row.
  // 展开状态属于此精确目录及变更头挂载行。
  const [expanded, setExpanded] = React.useState(false);
  return <div>
    <button type="button" aria-expanded={expanded} className="flex w-full items-start gap-2 rounded px-2 py-1 text-left hover:bg-accent" onClick={() => setExpanded((current) => !current)}>
      {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}<Folder size={15} /><span className="break-words">{entry.path.slice(entry.path.lastIndexOf("/") + 1)} <small>({entry.documentCount})</small><small className="block text-muted-foreground">{entry.description}</small></span>
    </button>
    {expanded ? <div className="ml-4 border-l pl-3"><KnowledgeDirectoryChildren libraryId={libraryId} path={entry.path} onDocument={onDocument} /></div> : null}
  </div>;
}

// Reads one explicit human query by the library's declared mode.
// 按库声明模式读取一个显式人工查询。
// Parameters: libraryId/query are fixed request inputs; onDocument opens an exact hit ID. Returns partial-status-aware results.
// 参数：libraryId 及 query 为固定请求输入；onDocument 打开精确命中身份。返回感知部分状态的结果。
function KnowledgeSearchResults({ libraryId, query, onDocument }: { libraryId: string; query: string; onDocument: (documentId: string) => void }) {
  // Shared labels never replace service diagnostics.
  // 共享标签绝不替换服务诊断。
  const { t } = useI18n();
  // The server executes only this exact query/library selector.
  // 服务仅执行此精确查询及库选择器。
  const load = React.useCallback(() => searchManagedKnowledgeLibrary({ libraryId, query }), [libraryId, query]);
  // A new submitted query revokes the previous result projection.
  // 新提交查询撤销先前结果投影。
  const read = useKnowledgeRead(JSON.stringify([libraryId, query]), load);
  return <section className="space-y-3">
    {read.loading ? <StreamPlaceholder label={t("knowledge.content.searching")} /> : null}
    {read.error !== null ? <KnowledgeReadFailure error={read.error} onRetry={read.refresh} /> : null}
    {read.value !== null ? <>
      {read.value.libraries.map((status) => <p className="text-sm text-muted-foreground" key={status.libraryId}>{t(KNOWLEDGE_MODE_COPY[status.indexMode].label)} · {t("knowledge.content.capabilities").replace("{fts}", formatKnowledgeIndexStatus(status.ftsStatus, t)).replace("{pending}", String(status.embeddings.pending)).replace("{failed}", String(status.embeddings.failed))}</p>)}
      {read.value.failures.map((failure, index) => <pre role="alert" className="select-text whitespace-pre-wrap break-words text-sm text-destructive" key={index}>{failure.error}</pre>)}
      {read.value.hits.map((hit) => <article className="space-y-2 rounded-md border p-3" key={JSON.stringify([hit.chunkId, hit.contentSha256, hit.revision])}>
        <button type="button" className="text-left font-medium underline" onClick={() => onDocument(hit.documentId)}>{hit.path}</button><small className="block text-muted-foreground">{hit.headingPath}</small><pre className="select-text whitespace-pre-wrap break-words text-sm">{hit.content}</pre>
      </article>)}
      {read.value.hits.length === 0 ? <p className="text-muted-foreground">{t("knowledge.content.noHits")}</p> : null}
    </> : null}
  </section>;
}

// Renders read-only directory discovery, current document reading and declared-mode search.
// 渲染只读目录发现、当前文档阅读及声明模式检索。
// Parameters: library is the current authoritative active summary. Returns no content editor or human ingest action.
// 参数：library 为当前权威活动摘要。返回值不含内容编辑器或人工搬运动作。
export function KnowledgeContent({ library }: { library: KnowledgeLibrarySummary }) {
  // Every label follows the application's locale.
  // 每个标签遵循应用语言。
  const { t } = useI18n();
  // The exact selected document ID owns the independent reader dialog.
  // 精确选定文档身份拥有独立阅读对话框。
  const [documentId, setDocumentId] = React.useState<string | null>(null);
  // Query editing alone never starts inference.
  // 仅编辑查询绝不开始推理。
  const [query, setQuery] = React.useState("");
  // A submitted query includes a user-triggered visit so repeated searches can explicitly refresh.
  // 已提交查询包含用户触发访问序号，使重复查询可显式刷新。
  const [search, setSearch] = React.useState<{ query: string; visit: number; scope: string } | null>(null);
  // Policy/content changes revoke old query projections without automatically issuing paid inference.
  // 策略及内容变化撤销旧查询投影，不自动发起付费推理。
  const searchScope = JSON.stringify([library.id, library.headChangeSeq, library.settings]);
  return <div className="space-y-4">
    <p className="text-sm text-muted-foreground">{t("knowledge.content.readOnly")}</p>
    <KnowledgeDirectoryChildren key={JSON.stringify([library.id, library.headChangeSeq])} libraryId={library.id} path="" onDocument={setDocumentId} />
    {library.settings !== null && library.settings.indexMode !== "browse" ? <>
      <form className="flex gap-2" onSubmit={(event) => { event.preventDefault(); if (query.trim()) setSearch((current) => ({ query: query.trim(), visit: current === null ? 1 : current.visit + 1, scope: searchScope })); }}>
        <Input aria-label={t("knowledge.content.search")} value={query} onChange={(event) => setQuery(event.target.value)} /><Button disabled={!query.trim()} type="submit">{t("knowledge.content.search")}</Button>
      </form>
      <p className="text-sm text-muted-foreground">{t(KNOWLEDGE_MODE_COPY[library.settings.indexMode].description)}</p>
      {search !== null && search.scope === searchScope ? <KnowledgeSearchResults key={search.visit} libraryId={library.id} query={search.query} onDocument={setDocumentId} /> : null}
    </> : null}
    <Dialog open={documentId !== null} onOpenChange={(open) => { if (!open) setDocumentId(null); }}><DialogContent size="xl" layout="document" scrollMode="fixed-footer">
      <DialogHeader><DialogTitle>{t("knowledge.view")} · {library.name}</DialogTitle></DialogHeader>
      <div className="min-h-0 overflow-auto p-4">{documentId !== null ? <KnowledgeCurrentDocumentReader key={JSON.stringify([documentId, library.headChangeSeq])} libraryId={library.id} documentId={documentId} /> : null}</div>
    </DialogContent></Dialog>
  </div>;
}
