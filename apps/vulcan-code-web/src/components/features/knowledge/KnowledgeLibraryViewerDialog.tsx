import * as React from "react";
import { BookOpen, ChevronDown, ChevronRight, FileText, Folder, LoaderCircle, Maximize2, Minimize2, Search, X } from "lucide-react";
import { toast } from "sonner";

import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useI18n, type TranslationKey } from "@/i18n";
import { listManagedKnowledgeDocumentFolders, listManagedKnowledgeDocuments, readManagedKnowledgeDocument, searchManagedKnowledgeLibrary } from "@/service/knowledge-base-service";
import type { KnowledgeDisplayIndexState, KnowledgeDocumentContent, KnowledgeDocumentSummary, KnowledgeLibrarySummary, KnowledgeSearchResult } from "@/types";

// KNOWLEDGE_VIEWER_PAGE_SIZE bounds each folder request so opening the viewer never reads the complete catalog.
// KNOWLEDGE_VIEWER_PAGE_SIZE 限制每次目录请求，确保打开阅读器时不会读取完整文档目录。
const KNOWLEDGE_VIEWER_PAGE_SIZE = 100;

// KnowledgeBrowseCorpus selects the exact Markdown layer shown by the viewer.
// KnowledgeBrowseCorpus 选择阅读器展示的精确 Markdown 内容层。
type KnowledgeBrowseCorpus = "raw" | "display";

// defaultKnowledgeBrowseCorpus prefers an explicitly published display generation.
// defaultKnowledgeBrowseCorpus 优先选择已明确发布的显示库代次。
// `displayIndex` is the durable display lifecycle state; the return value is the exact initial
// corpus and never silently substitutes raw content for an active display generation.
// `displayIndex` 是持久化显示库生命周期状态；返回值是精确的初始内容层，绝不会为活动显示代次
// 静默替换成原始内容。
export function defaultKnowledgeBrowseCorpus(displayIndex: KnowledgeDisplayIndexState): KnowledgeBrowseCorpus {
  return displayIndex.activeGeneration?.trim() ? "display" : "raw";
}

// KnowledgeDirectoryPage stores one lazily loaded exact-folder page sequence.
// KnowledgeDirectoryPage 保存一个按需加载的精确目录分页序列。
interface KnowledgeDirectoryPage {
  // Authoritative document summaries loaded for this exact folder.
  // 已为当前精确目录加载的权威文档摘要。
  documents: KnowledgeDocumentSummary[];
  // Whether one page request is currently pending.
  // 当前是否有一页请求正在等待。
  loading: boolean;
  // Whether another bounded page can be requested.
  // 是否还可以请求下一有界分页。
  hasMore: boolean;
  // Failed page reads remain distinct from an empty successfully loaded directory.
  // 分页读取失败与成功加载的空目录保持区分。
  error?: string;
}

// KnowledgeDocumentTreeNode represents one folder or Markdown leaf in the viewer tree.
// KnowledgeDocumentTreeNode 表示阅读器目录树中的一个文件夹或 Markdown 叶节点。
export interface KnowledgeDocumentTreeNode {
  // Stable normalized relative path used for expansion and React identity.
  // 用于展开状态和 React 标识的稳定规范化相对路径。
  path: string;
  // Current path-segment display name.
  // 当前路径片段的展示名称。
  name: string;
  // Sorted child folders and Markdown documents.
  // 已排序的子文件夹与 Markdown 文档。
  children: KnowledgeDocumentTreeNode[];
  // Authoritative document summary when this node is a file leaf.
  // 当前节点为文件叶节点时对应的权威文档摘要。
  document?: KnowledgeDocumentSummary;
}

// KnowledgeLibraryViewerDialog renders complete library navigation and Markdown reading in one modal.
// KnowledgeLibraryViewerDialog 在一个弹窗内渲染完整知识库导航与 Markdown 阅读。
// Parameters: props supplies the exact library and open-state callbacks; returns searchable document navigation and original Markdown reading.
// 参数：props 提供精确资料库与打开状态回调；返回可搜索的文档导航和原始 Markdown 阅读界面。
export function KnowledgeLibraryViewerDialog(props: {
  // Selected library whose documents are available to the trusted management surface.
  // 受信任管理界面可浏览其文档的选中知识库。
  library: KnowledgeLibrarySummary | null;
  // Whether the centered viewer dialog is open.
  // 居中阅读弹窗是否打开。
  open: boolean;
  // Called when the viewer requests open-state changes.
  // 阅读器请求改变打开状态时调用。
  onOpenChange: (open: boolean) => void;
}) {
  // t resolves all viewer text in the active WebUI locale.
  // t 使用当前 WebUI 语言解析阅读器全部文案。
  const { t } = useI18n();
  // folderPaths stores only the lightweight distinct directory hierarchy.
  // folderPaths 仅保存轻量的唯一目录层级。
  const [folderPaths, setFolderPaths] = React.useState<string[]>([]);
  // directoryPages stores document metadata only for folders the user has opened.
  // directoryPages 仅保存用户已打开目录的文档元数据。
  const [directoryPages, setDirectoryPages] = React.useState<Record<string, KnowledgeDirectoryPage>>({});
  // catalogLoading covers the initial lightweight folder and root-file requests.
  // catalogLoading 表示初始轻量目录与根文件请求正在执行。
  const [catalogLoading, setCatalogLoading] = React.useState(false);
  // selectedDocument stores the complete authoritative Markdown currently shown on the right.
  // selectedDocument 保存右侧当前展示的完整权威 Markdown。
  const [selectedDocument, setSelectedDocument] = React.useState<KnowledgeDocumentContent | null>(null);
  // browseCorpus stores the explicit Markdown layer used by document reads.
  // browseCorpus 保存文档读取使用的显式 Markdown 内容层。
  const [browseCorpus, setBrowseCorpus] = React.useState<KnowledgeBrowseCorpus>("raw");
  // documentLoading indicates that the selected Markdown body is being loaded.
  // documentLoading 表示选中的 Markdown 正文正在加载。
  const [documentLoading, setDocumentLoading] = React.useState(false);
  // searchText stores the explicit Jieba BM25 query.
  // searchText 保存显式的 Jieba BM25 查询。
  const [searchText, setSearchText] = React.useState("");
  // searchRetrievalMode stores the explicit corpus selected by this management search.
  // searchRetrievalMode 保存本次管理界面搜索显式选择的内容层。
  const [searchRetrievalMode, setSearchRetrievalMode] = React.useState<"raw_bm25" | "display_bm25" | "display_hybrid">("raw_bm25");
  // searchResult stores the latest library-scoped search response.
  // searchResult 保存最近一次知识库范围搜索响应。
  const [searchResult, setSearchResult] = React.useState<KnowledgeSearchResult | null>(null);
  // searching owns the independent content-search request state.
  // searching 持有独立内容搜索请求状态。
  const [searching, setSearching] = React.useState(false);
  // isMaximized switches the standard centered dialog into the shared viewport-filling layout.
  // isMaximized 将标准居中弹窗切换为共享的视口铺满布局。
  const [isMaximized, setIsMaximized] = React.useState(false);
  // previewMode selects rendered Markdown or exact source text.
  // previewMode 选择渲染后的 Markdown 或精确原文。
  const [previewMode, setPreviewMode] = React.useState<"rendered" | "source">("rendered");
  // openDirectoryPaths stores expanded folders in the document tree.
  // openDirectoryPaths 保存文档目录树中已展开的文件夹。
  const [openDirectoryPaths, setOpenDirectoryPaths] = React.useState<ReadonlySet<string>>(new Set());
  // catalogRequestSequence discards stale lazy-directory responses after close or library switch.
  // catalogRequestSequence 在关闭或切换知识库后丢弃过期的延迟目录响应。
  const catalogRequestSequence = React.useRef(0);
  // directoryRequestsInFlight prevents duplicate page reads from rapid repeated clicks.
  // directoryRequestsInFlight 防止快速重复点击产生重复分页读取。
  const directoryRequestsInFlight = React.useRef(new Set<string>());
  // documentRequestSequence discards stale Markdown bodies after a newer selection.
  // documentRequestSequence 在更新选择后丢弃过期 Markdown 正文。
  const documentRequestSequence = React.useRef(0);
  // searchRequestSequence discards stale search responses after close or a newer query.
  // searchRequestSequence 在关闭或更新查询后丢弃过期搜索响应。
  const searchRequestSequence = React.useRef(0);
  // Read errors remain visible in their owning region rather than appearing as empty data.
  // 读取错误保留在所属区域，不被呈现为空数据。
  const [catalogError, setCatalogError] = React.useState("");
  const [documentError, setDocumentError] = React.useState("");
  const [searchError, setSearchError] = React.useState("");
  // reloadCatalog is an explicit retry independent from background summary object refreshes.
  // reloadCatalog 是独立于后台摘要对象刷新的显式重试。
  const [reloadCatalog, setReloadCatalog] = React.useState(0);
  const searchBusy = React.useRef(false);
  // libraryRef supplies the latest snapshot when the stable library identity actually changes.
  // libraryRef 在稳定知识库身份确实变化时提供最新快照。
  const libraryRef = React.useRef(props.library);
  libraryRef.current = props.library;
  const libraryId = props.library?.id;

  // openDocument loads one complete Markdown document and preserves only the newest response.
  // openDocument 加载一份完整 Markdown 文档，并且只保留最新响应。
  const openDocument = React.useCallback(async (documentId: string) => {
    if (!props.library) return;
    // sequence owns this exact document request.
    // sequence 持有本次精确文档请求。
    const sequence = documentRequestSequence.current + 1;
    documentRequestSequence.current = sequence;
    setDocumentLoading(true);
    setDocumentError("");
    setSelectedDocument(null);
    try {
      // content is the authoritative Markdown body returned by the management RPC.
      // content 是管理 RPC 返回的权威 Markdown 正文。
      const content = await readManagedKnowledgeDocument(props.library.id, documentId, browseCorpus);
      if (documentRequestSequence.current !== sequence) return;
      setSelectedDocument(content);
      setPreviewMode("rendered");
    } catch (error) {
      if (documentRequestSequence.current === sequence) setDocumentError(error instanceof Error ? error.message : String(error));
    } finally {
      if (documentRequestSequence.current === sequence) setDocumentLoading(false);
    }
  }, [browseCorpus, props.library]);

  // loadDirectoryPage reads one bounded page for the exact folder selected by the user.
  // loadDirectoryPage 为用户选择的精确目录读取一页有界数据。
  const loadDirectoryPage = React.useCallback(async (folderPath: string, offset: number) => {
    if (!props.library) return;
    // sequence identifies the active dialog catalog generation.
    // sequence 标识当前活动弹窗的目录代次。
    const sequence = catalogRequestSequence.current;
    // requestKey isolates one exact library generation and folder page so stale requests cannot clear current work.
    // requestKey 隔离一个精确知识库代次与目录分页，避免过期请求清除当前任务。
    const requestKey = `${props.library.id}\u0000${sequence}\u0000${folderPath}\u0000${offset}`;
    if (directoryRequestsInFlight.current.has(requestKey)) return;
    directoryRequestsInFlight.current.add(requestKey);
    setDirectoryPages((current) => ({
      ...current,
      [folderPath]: {
        documents: offset === 0 ? [] : (current[folderPath]?.documents ?? []),
        loading: true,
        hasMore: current[folderPath]?.hasMore ?? true,
      },
    }));
    try {
      // page contains metadata only and never includes Markdown bodies.
      // page 仅包含元数据，绝不包含 Markdown 正文。
      const page = await listManagedKnowledgeDocuments(props.library.id, folderPath, KNOWLEDGE_VIEWER_PAGE_SIZE, offset);
      if (catalogRequestSequence.current !== sequence) return;
      setDirectoryPages((current) => ({
        ...current,
        [folderPath]: {
          documents: offset === 0 ? page : [...(current[folderPath]?.documents ?? []), ...page],
          loading: false,
          hasMore: page.length === KNOWLEDGE_VIEWER_PAGE_SIZE,
        },
      }));
    } catch (error) {
      if (catalogRequestSequence.current !== sequence) return;
      setDirectoryPages((current) => ({
        ...current,
        [folderPath]: {
          documents: current[folderPath]?.documents ?? [],
          loading: false,
          hasMore: current[folderPath]?.hasMore ?? true,
          error: error instanceof Error ? error.message : String(error),
        },
      }));
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      directoryRequestsInFlight.current.delete(requestKey);
    }
  }, [props.library]);

  React.useEffect(() => {
    if (!props.open || !libraryRef.current) return;
    // library is the non-null immutable catalog owner captured by this effect.
    // library 是本次副作用捕获的非空不可变目录所有者。
    const library = libraryRef.current;
    // sequence owns the lightweight initial navigation load.
    // sequence 持有轻量的初始导航加载过程。
    const sequence = catalogRequestSequence.current + 1;
    catalogRequestSequence.current = sequence;
    documentRequestSequence.current += 1;
    searchRequestSequence.current += 1;
    setFolderPaths([]);
    setDirectoryPages({});
    directoryRequestsInFlight.current.clear();
    setSelectedDocument(null);
    setBrowseCorpus(defaultKnowledgeBrowseCorpus(library.settings.displayIndex));
    setCatalogLoading(true);
    setCatalogError("");
    setDocumentError("");
    setSearchError("");
    searchBusy.current = false;
    setDocumentLoading(false);
    setSearchText("");
    setSearchRetrievalMode(library.settings.retrievalMode);
    setSearchResult(null);
    setSearching(false);
    setIsMaximized(false);
    setPreviewMode("rendered");
    setOpenDirectoryPaths(new Set());
    void (async () => {
      try {
        // folders and rootDocuments are independent lightweight requests.
        // folders 与 rootDocuments 是相互独立的轻量请求。
        const [folders, rootDocuments] = await Promise.all([
          listManagedKnowledgeDocumentFolders(library.id),
          listManagedKnowledgeDocuments(library.id, "", KNOWLEDGE_VIEWER_PAGE_SIZE, 0),
        ]);
        if (catalogRequestSequence.current !== sequence) return;
        setFolderPaths(folders.filter(Boolean));
        setDirectoryPages({
          "": {
            documents: rootDocuments,
            loading: false,
            hasMore: rootDocuments.length === KNOWLEDGE_VIEWER_PAGE_SIZE,
          },
        });
      } catch (error) {
        if (catalogRequestSequence.current === sequence) setCatalogError(error instanceof Error ? error.message : String(error));
      } finally {
        if (catalogRequestSequence.current === sequence) setCatalogLoading(false);
      }
    })();
    return () => {
      catalogRequestSequence.current += 1;
      directoryRequestsInFlight.current.clear();
      documentRequestSequence.current += 1;
      searchRequestSequence.current += 1;
    };
  }, [libraryId, props.open, reloadCatalog]);

  // loadedDocuments flattens only the folder pages explicitly requested so far.
  // loadedDocuments 仅展开目前已显式请求的目录分页。
  const loadedDocuments = React.useMemo(() => Object.values(directoryPages).flatMap((page) => page.documents), [directoryPages]);
  // documentTree is the normalized folder hierarchy rendered in the left pane.
  // documentTree 是左侧面板渲染的规范化文件夹层级。
  const documentTree = React.useMemo(() => buildKnowledgeDocumentTree(loadedDocuments, folderPaths), [folderPaths, loadedDocuments]);
  // searchHits deduplicates ranked chunks into clickable document rows without loading the catalog.
  // searchHits 将排序切片去重为可点击文档行，且无需加载文档目录。
  const searchHits = React.useMemo(() => {
    const seen = new Set<string>();
    return (searchResult?.hits ?? []).filter((hit) => {
      if (seen.has(hit.documentId)) return false;
      seen.add(hit.documentId);
      return true;
    });
  }, [searchResult]);

  // searchLibrary executes one independent query and filters the tree to matching documents.
  // searchLibrary 执行一次独立查询，并将目录树筛选为匹配文档。
  const searchLibrary = React.useCallback(async () => {
    // query is the normalized non-empty user search text.
    // query 是规范化后的非空用户搜索文本。
    const query = searchText.trim();
    if (!props.library || !query || searchBusy.current) return;
    searchBusy.current = true;
    // sequence owns this exact search request.
    // sequence 持有本次精确搜索请求。
    const sequence = searchRequestSequence.current + 1;
    searchRequestSequence.current = sequence;
    setSearching(true);
    setSearchError("");
    try {
      // result is the authoritative library-scoped Jieba BM25 response.
      // result 是权威的知识库范围 Jieba BM25 响应。
      const result = await searchManagedKnowledgeLibrary(props.library.id, query, searchRetrievalMode);
      if (searchRequestSequence.current !== sequence) return;
      setSearchResult(result);
      if (result.failures.length > 0) toast.error(result.failures.map((failure) => failure.error).join("\n"));
      if (result.hits[0]) void openDocument(result.hits[0].documentId);
    } catch (error) {
      if (searchRequestSequence.current === sequence) setSearchError(error instanceof Error ? error.message : String(error));
    } finally {
      if (searchRequestSequence.current === sequence) { searchBusy.current = false; setSearching(false); }
    }
  }, [openDocument, props.library, searchRetrievalMode, searchText, searching]);

  // toggleDirectory changes expansion and loads the first page only when a folder is opened first.
  // toggleDirectory 改变展开状态，并且仅在目录首次打开时加载第一页。
  const toggleDirectory = React.useCallback((path: string) => {
    const opening = !openDirectoryPaths.has(path);
    setOpenDirectoryPaths((current) => toggleKnowledgeDirectoryPath(current, path));
    if (opening && !directoryPages[path]) void loadDirectoryPage(path, 0);
  }, [directoryPages, loadDirectoryPage, openDirectoryPaths]);

  // clearSearch restores the complete document tree without changing the current document.
  // clearSearch 恢复完整文档目录树且不改变当前文档。
  const clearSearch = React.useCallback(() => {
    searchRequestSequence.current += 1;
    setSearching(false);
    searchBusy.current = false;
    setSearchError("");
    setSearchText("");
    setSearchResult(null);
  }, []);

  // changeBrowseCorpus invalidates any body from the previous layer before the next explicit read.
  // changeBrowseCorpus 在下一次显式读取前使上一内容层的正文失效。
  const changeBrowseCorpus = React.useCallback((corpus: KnowledgeBrowseCorpus) => {
    documentRequestSequence.current += 1;
    setDocumentLoading(false);
    setSelectedDocument(null);
    setDocumentError("");
    setBrowseCorpus(corpus);
  }, []);

  // handleOpenChange invalidates pending work before the parent closes the dialog.
  // handleOpenChange 在父组件关闭弹窗前使等待中的任务失效。
  const handleOpenChange = React.useCallback((open: boolean) => {
    if (!open) {
      catalogRequestSequence.current += 1;
      documentRequestSequence.current += 1;
      searchRequestSequence.current += 1;
      setIsMaximized(false);
    }
    props.onOpenChange(open);
  }, [props.onOpenChange]);

  if (!props.library) return null;
  return (
    <Dialog onOpenChange={handleOpenChange} open={props.open}>
      <DialogContent className={`knowledge-viewer-dialog${isMaximized ? " maximized" : ""}`} showCloseButton={false}>
        <header className="knowledge-viewer-header">
          <span className="knowledge-viewer-title"><BookOpen size={18} /><DialogTitle>{props.library.name}</DialogTitle></span>
          <Select onValueChange={(value) => changeBrowseCorpus(value as KnowledgeBrowseCorpus)} value={browseCorpus}>
            <SelectTrigger aria-label={t("knowledge.experience.browseCorpus")} className="knowledge-viewer-corpus-mode">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="raw">{t("knowledge.experience.rawLibrary")}</SelectItem>
              <SelectItem disabled={!props.library.settings.displayIndex.activeGeneration?.trim()} value="display">{t("knowledge.experience.displayLibrary")}</SelectItem>
            </SelectContent>
          </Select>
          <span className="knowledge-viewer-window-actions">
            <Button aria-label={t(isMaximized ? "knowledge.viewer.restore" : "knowledge.viewer.maximize")} onClick={() => setIsMaximized((current) => !current)} size="icon" type="button" variant="outline">
              {isMaximized ? <Minimize2 size={16} /> : <Maximize2 size={16} />}
            </Button>
            <Button aria-label={t("knowledge.close")} onClick={() => handleOpenChange(false)} size="icon" type="button" variant="outline"><X size={16} /></Button>
          </span>
        </header>
        <div className="knowledge-viewer-search">
          <Search size={16} />
          <Select disabled={searching} onValueChange={(value) => setSearchRetrievalMode(value as "raw_bm25" | "display_bm25" | "display_hybrid")} value={searchRetrievalMode}>
            <SelectTrigger aria-label={t("knowledge.experience.searchLayer")} className="knowledge-viewer-search-mode">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="raw_bm25">{t("knowledge.experience.rawSearch")}</SelectItem>
              <SelectItem value="display_bm25">{t("knowledge.experience.displaySearch")}</SelectItem>
              <SelectItem value="display_hybrid">{t("knowledge.experience.hybridSearch")}</SelectItem>
            </SelectContent>
          </Select>
          <Input aria-label={t("knowledge.search.placeholder")} disabled={searching} onChange={(event) => {
            setSearchText(event.target.value);
            if (!event.target.value.trim()) clearSearch();
          }} onKeyDown={(event) => {
            if (event.key === "Enter") void searchLibrary();
          }} placeholder={t("knowledge.search.placeholder")} value={searchText} />
          {searchResult ? <span className="knowledge-viewer-search-count">{t("knowledge.viewer.searchCount").replace("{count}", String(searchHits.length))}</span> : null}
          {searchResult ? <Button disabled={searching} onClick={clearSearch} type="button" variant="outline">{t("knowledge.viewer.clearSearch")}</Button> : null}
          <Button aria-busy={searching} disabled={searching || !searchText.trim()} onClick={() => void searchLibrary()} type="button">
            {searching ? <LoaderCircle className="animate-spin" size={15} /> : <Search size={15} />}
            {t(searching ? "knowledge.searching" : "knowledge.search")}
          </Button>
        </div>
        {searchError ? <p role="alert" className="px-4 text-sm text-destructive">{searchError}</p> : null}
        {searchResult?.warnings?.map((warning) => <p className="px-4 text-sm text-[var(--vcn-content-warning)]" key={warning}>{warning}</p>)}
        <div className="knowledge-viewer-body">
          <aside className="knowledge-viewer-tree">
            {catalogError ? <div role="alert" className="space-y-2 p-3 text-sm text-destructive"><p>{catalogError}</p><Button variant="outline" onClick={() => setReloadCatalog((value) => value + 1)}>{t("skills.action.refresh")}</Button></div> : null}
            {catalogLoading ? <div className="knowledge-viewer-state"><LoaderCircle className="animate-spin" size={18} />{t("knowledge.viewer.loadingDocuments")}</div> : null}
            {!catalogLoading && searchResult && searchHits.length === 0 ? <div className="knowledge-viewer-state">{t("knowledge.search.empty")}</div> : null}
            {!catalogLoading && !catalogError && !searchResult && documentTree.children.length === 0 ? <div className="knowledge-viewer-state">{t("knowledge.emptyDocuments")}</div> : null}
            {searchResult ? searchHits.map((hit) => (
              <button className={`knowledge-viewer-tree-row file${hit.documentId === selectedDocument?.document.id ? " active" : ""}`} key={hit.documentId} onClick={() => void openDocument(hit.documentId)} title={hit.relativePath} type="button">
                <FileText size={14} /><span>{hit.relativePath}</span>
              </button>
            )) : documentTree.children.map((node) => <KnowledgeDocumentTreeRow activeDocumentId={selectedDocument?.document.id ?? null} directoryPages={directoryPages} key={node.path} node={node} onLoadDirectoryPage={loadDirectoryPage} onOpenDocument={openDocument} onToggleDirectory={toggleDirectory} openDirectoryPaths={openDirectoryPaths} t={t} />)}
            {!searchResult && directoryPages[""]?.loading ? <div className="knowledge-viewer-tree-loading"><LoaderCircle className="animate-spin" size={14} />{t("knowledge.viewer.loadingFolder")}</div> : null}
            {!searchResult && directoryPages[""]?.hasMore && !directoryPages[""]?.loading ? <Button className="knowledge-viewer-load-more" onClick={() => void loadDirectoryPage("", directoryPages[""].documents.length)} size="sm" type="button" variant="ghost">{t("knowledge.viewer.loadMore")}</Button> : null}
          </aside>
          <main className="knowledge-viewer-reader">
            {documentError ? <p role="alert" className="p-4 text-sm text-destructive">{documentError}</p> : null}
            {selectedDocument ? (
              <>
                <header className="knowledge-viewer-document-header">
                  <span><strong>{selectedDocument.document.title}</strong><small>{selectedDocument.document.relativePath}</small></span>
                  <span>
                    <Button onClick={() => setPreviewMode("rendered")} size="sm" type="button" variant={previewMode === "rendered" ? "secondary" : "ghost"}>{t("knowledge.markdown.preview")}</Button>
                    <Button onClick={() => setPreviewMode("source")} size="sm" type="button" variant={previewMode === "source" ? "secondary" : "ghost"}>{t("knowledge.markdown.source")}</Button>
                  </span>
                </header>
                <div className="knowledge-viewer-markdown">
                  {documentLoading ? <div className="knowledge-viewer-state"><LoaderCircle className="animate-spin" size={18} />{t("knowledge.viewer.loadingDocument")}</div> : (previewMode === "rendered" ? <MarkdownMessage content={selectedDocument.content} eager /> : <pre className="knowledge-markdown-source"><code>{selectedDocument.content}</code></pre>)}
                </div>
              </>
            ) : <div className="knowledge-viewer-state"><FileText size={22} />{documentLoading ? t("knowledge.viewer.loadingDocument") : t("knowledge.viewer.selectDocument")}</div>}
          </main>
        </div>
      </DialogContent>
    </Dialog>
  );
}

// KnowledgeDocumentTreeRow recursively renders one folder or Markdown document row.
// KnowledgeDocumentTreeRow 递归渲染一个文件夹或 Markdown 文档行。
// Parameters: props identifies one folder/document, its expansion state and load/select callbacks; returns the recursive navigation row.
// 参数：props 标识一个文件夹或文档、展开状态及加载和选择回调；返回递归导航行。
function KnowledgeDocumentTreeRow(props: {
  // Tree node rendered by this row.
  // 本行渲染的目录树节点。
  node: KnowledgeDocumentTreeNode;
  // Identifier of the document currently displayed on the right.
  // 右侧当前展示文档的标识符。
  activeDocumentId: string | null;
  // Expanded directory path set.
  // 已展开目录路径集合。
  openDirectoryPaths: ReadonlySet<string>;
  // Lazy pages keyed by their exact logical folder paths.
  // 以精确逻辑目录路径为键的延迟分页。
  directoryPages: Readonly<Record<string, KnowledgeDirectoryPage>>;
  // Called when a folder changes open state.
  // 文件夹改变展开状态时调用。
  onToggleDirectory: (path: string) => void;
  // Called when the user explicitly requests another page in one folder.
  // 用户显式请求某个目录的下一页时调用。
  onLoadDirectoryPage: (path: string, offset: number) => Promise<void>;
  // Called when one Markdown leaf is selected.
  // 选择一个 Markdown 叶节点时调用。
  onOpenDocument: (documentId: string) => Promise<void>;
  // Translator used by folder-local loading controls.
  // 目录局部加载控件使用的翻译函数。
  t: (key: TranslationKey) => string;
}) {
  // document is present only when this node is an authoritative Markdown leaf.
  // document 仅在当前节点为权威 Markdown 叶节点时存在。
  const document = props.node.document;
  if (!document) {
    // isOpen resolves the current expansion state for this exact folder.
    // isOpen 解析当前精确文件夹的展开状态。
    const isOpen = props.openDirectoryPaths.has(props.node.path);
    // directoryPage exists only after this folder has been explicitly opened.
    // directoryPage 仅在该目录被显式打开后存在。
    const directoryPage = props.directoryPages[props.node.path];
    return (
      <div className="knowledge-viewer-tree-node">
        <button className="knowledge-viewer-tree-row folder" onClick={() => props.onToggleDirectory(props.node.path)} type="button">
          {isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}<Folder size={15} /><span>{props.node.name}</span>
        </button>
        {isOpen ? (
          <div className="knowledge-viewer-tree-children">
            {props.node.children.map((child) => <KnowledgeDocumentTreeRow {...props} key={child.path} node={child} />)}
            {directoryPage?.loading ? <div className="knowledge-viewer-tree-loading"><LoaderCircle className="animate-spin" size={14} />{props.t("knowledge.viewer.loadingFolder")}</div> : null}
            {directoryPage?.error ? <p role="alert" className="p-2 text-xs text-destructive">{directoryPage.error}</p> : null}
            {directoryPage?.hasMore && !directoryPage.loading ? <Button className="knowledge-viewer-load-more" onClick={() => void props.onLoadDirectoryPage(props.node.path, directoryPage.documents.length)} size="sm" type="button" variant="ghost">{props.t("knowledge.viewer.loadMore")}</Button> : null}
          </div>
        ) : null}
      </div>
    );
  }
  return (
    <button className={`knowledge-viewer-tree-row file${document.id === props.activeDocumentId ? " active" : ""}`} onClick={() => void props.onOpenDocument(document.id)} title={document.relativePath} type="button">
      <FileText size={14} /><span>{props.node.name}</span>
    </button>
  );
}

// buildKnowledgeDocumentTree converts normalized Markdown paths into a stable sorted hierarchy.
// buildKnowledgeDocumentTree 将规范化 Markdown 路径转换为稳定排序的层级结构。
export function buildKnowledgeDocumentTree(documents: KnowledgeDocumentSummary[], folderPaths: string[] = []): KnowledgeDocumentTreeNode {
  // root is the synthetic non-rendered tree container.
  // root 是不会直接渲染的合成目录树容器。
  const root: KnowledgeDocumentTreeNode = { path: "", name: "", children: [] };
  // ensureFolder creates or returns every intermediate directory for one normalized path.
  // ensureFolder 为一个规范路径创建或返回全部中间目录。
  const ensureFolder = (folderPath: string): KnowledgeDocumentTreeNode => {
    const segments = folderPath.split("/").filter(Boolean);
    let current = root;
    for (let index = 0; index < segments.length; index += 1) {
      const path = segments.slice(0, index + 1).join("/");
      const existing = current.children.find((child) => !child.document && child.path === path);
      if (existing) {
        current = existing;
        continue;
      }
      const folder: KnowledgeDocumentTreeNode = { path, name: segments[index], children: [] };
      current.children.push(folder);
      current = folder;
    }
    return current;
  };
  for (const folderPath of folderPaths) ensureFolder(folderPath);
  for (const document of documents) {
    // segments are authoritative slash-normalized relative-path components.
    // segments 是权威的斜杠规范化相对路径片段。
    const segments = document.relativePath.split("/").filter(Boolean);
    if (segments.length === 0) continue;
    // current is the exact document folder already represented in the hierarchy.
    // current 是层级中已经表示的精确文档目录。
    const current = ensureFolder(document.folderPath);
    current.children.push({ path: document.relativePath, name: segments[segments.length - 1], children: [], document });
  }
  sortKnowledgeDocumentTree(root);
  return root;
}

// sortKnowledgeDocumentTree orders folders before files and sorts each group by display name.
// sortKnowledgeDocumentTree 将文件夹排在文件前，并按展示名称排序各组。
function sortKnowledgeDocumentTree(node: KnowledgeDocumentTreeNode): void {
  node.children.sort((left, right) => Number(Boolean(left.document)) - Number(Boolean(right.document)) || left.name.localeCompare(right.name));
  for (const child of node.children) sortKnowledgeDocumentTree(child);
}

// toggleKnowledgeDirectoryPath returns a new expansion set with one folder toggled.
// toggleKnowledgeDirectoryPath 返回切换一个文件夹后的新展开集合。
function toggleKnowledgeDirectoryPath(current: ReadonlySet<string>, path: string): ReadonlySet<string> {
  // next is the immutable replacement set published to React state.
  // next 是发布到 React 状态的不可变替换集合。
  const next = new Set(current);
  if (next.has(path)) next.delete(path); else next.add(path);
  return next;
}
