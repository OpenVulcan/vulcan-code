import React from "react";
import { ChevronDown, ChevronUp, Minus, Plus } from "lucide-react";
import type { PDFDocumentLoadingTask } from "pdfjs-dist";
import type { EventBus, PDFFindController, PDFViewer } from "pdfjs-dist/web/pdf_viewer.mjs";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import "pdfjs-dist/web/pdf_viewer.css";
import "@/ui-kit/styles/features/document-viewer.css";
import { useI18n } from "@/i18n";
import { IconButton } from "@/ui-kit/primitives/button";
import { Input } from "@/ui-kit/primitives/input";
import { DocumentViewerFailure, DocumentViewerLoading } from "@/components/file-viewers/document-viewer-states";
import { PDFJS_DATA_BASE_PATH } from "@/components/file-viewers/pdfjs-data-path";

// PDF_ZOOM_STEP is the factor one zoom click scales the page by.
// PDF_ZOOM_STEP 是单次缩放点击使页面缩放的倍数。
const PDF_ZOOM_STEP = 1.2;

// loadPdfModules loads pdf.js and its viewer layer once; the viewer layer reads the library from globalThis.
// loadPdfModules 只加载一次 pdf.js 及其查看器层；查看器层从 globalThis 读取该库。
let pdfModules: Promise<{
  pdfjs: typeof import("pdfjs-dist");
  viewer: typeof import("pdfjs-dist/web/pdf_viewer.mjs");
}> | null = null;
function loadPdfModules() {
  pdfModules ??= (async () => {
    const pdfjs = await import("pdfjs-dist");
    pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;
    (globalThis as { pdfjsLib?: unknown }).pdfjsLib = pdfjs;
    return { pdfjs, viewer: await import("pdfjs-dist/web/pdf_viewer.mjs") };
  })();
  return pdfModules;
}

// pdfDataUrl returns the absolute URL of one pdf.js data folder (CMaps, fonts, wasm decoders, ICC profiles).
// pdfDataUrl 返回单个 pdf.js 数据文件夹（CMap、字体、wasm 解码器、ICC 配置）的绝对 URL。
function pdfDataUrl(folder: string): string {
  return new URL(`${import.meta.env.BASE_URL}${PDFJS_DATA_BASE_PATH}/${folder}/`, window.location.href).href;
}

// PdfDocumentViewer shows a PDF read-only with pdf.js: pages fit the panel width, text is selectable, links open
// outside the viewer and the toolbar zooms, pages and searches.
// PdfDocumentViewer 用 pdf.js 只读显示 PDF：页面适应面板宽度、文字可选择、链接在查看器之外打开，工具栏提供缩放、
// 页码与搜索。
export default function PdfDocumentViewer(props: { url: string; name: string }) {
  const { t } = useI18n();
  const containerRef = React.useRef<HTMLDivElement | null>(null);
  const viewerRef = React.useRef<HTMLDivElement | null>(null);
  const stateRef = React.useRef<{ viewer: PDFViewer; eventBus: EventBus; find: PDFFindController } | null>(null);
  const [status, setStatus] = React.useState<"loading" | "ready" | "failed">("loading");
  const [error, setError] = React.useState("");
  const [page, setPage] = React.useState({ current: 0, total: 0 });
  const [query, setQuery] = React.useState("");
  const [matches, setMatches] = React.useState({ current: 0, total: 0 });

  React.useEffect(() => {
    const container = containerRef.current;
    const viewerElement = viewerRef.current;
    if (!container || !viewerElement) return;
    let task: PDFDocumentLoadingTask | null = null;
    let disposed = false;
    setStatus("loading");
    void loadPdfModules().then(async ({ pdfjs, viewer: layer }) => {
      if (disposed) return;
      const eventBus = new layer.EventBus();
      const linkService = new layer.PDFLinkService({ eventBus, externalLinkTarget: layer.LinkTarget.BLANK, externalLinkRel: "noopener noreferrer" });
      const find = new layer.PDFFindController({ eventBus, linkService });
      const viewer = new layer.PDFViewer({ container, viewer: viewerElement, eventBus, linkService, findController: find });
      linkService.setViewer(viewer);
      eventBus.on("pagesinit", () => { viewer.currentScaleValue = "page-width"; });
      eventBus.on("pagechanging", (event: { pageNumber: number }) => setPage((value) => ({ ...value, current: event.pageNumber })));
      eventBus.on("updatefindmatchescount", (event: { matchesCount: { current: number; total: number } }) => setMatches(event.matchesCount));
      eventBus.on("updatefindcontrolstate", (event: { matchesCount: { current: number; total: number } }) => setMatches(event.matchesCount));
      stateRef.current = { viewer, eventBus, find };
      task = pdfjs.getDocument({
        url: props.url,
        cMapUrl: pdfDataUrl("cmaps"),
        cMapPacked: true,
        standardFontDataUrl: pdfDataUrl("standard_fonts"),
        wasmUrl: pdfDataUrl("wasm"),
        iccUrl: pdfDataUrl("iccs"),
        enableXfa: false,
      });
      const document = await task.promise;
      if (disposed) return;
      viewer.setDocument(document);
      linkService.setDocument(document);
      setPage({ current: 1, total: document.numPages });
      setStatus("ready");
    }).catch((reason: unknown) => {
      if (disposed) return;
      setError(reason instanceof Error ? reason.message : String(reason));
      setStatus("failed");
    });
    return () => {
      disposed = true;
      stateRef.current = null;
      void task?.destroy();
    };
  }, [props.url]);

  // Fit to the new width when the panel resizes, unless the reader zoomed by hand.
  // 面板尺寸变化时适应新宽度，除非读者手动缩放过。
  React.useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const observer = new ResizeObserver(() => {
      const viewer = stateRef.current?.viewer;
      if (viewer && viewer.currentScaleValue === "page-width") viewer.currentScaleValue = "page-width";
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  const zoom = (factor: number) => {
    const viewer = stateRef.current?.viewer;
    if (viewer) viewer.currentScale = Math.min(8, Math.max(0.25, viewer.currentScale * factor));
  };
  const search = React.useCallback((findPrevious: boolean, again: boolean) => {
    stateRef.current?.eventBus.dispatch("find", {
      source: null,
      type: again ? "again" : "",
      query,
      caseSensitive: false,
      entireWord: false,
      highlightAll: true,
      findPrevious,
      matchDiacritics: false,
    });
  }, [query]);
  // Typing searches from the top again.
  // 输入时重新从头搜索。
  React.useEffect(() => {
    if (status === "ready") search(false, false);
  }, [search, status]);

  return (
    <div className="vk-docview" data-kind="pdf">
      <div className="vk-docview-toolbar" role="toolbar" aria-label={props.name}>
        <span className="vk-docview-pages">{page.total > 0 ? `${page.current} / ${page.total}` : ""}</span>
        <IconButton aria-label={t("documentViewer.zoomOut")} disabled={status !== "ready"} onClick={() => zoom(1 / PDF_ZOOM_STEP)} title={t("documentViewer.zoomOut")}>
          <Minus />
        </IconButton>
        <IconButton aria-label={t("documentViewer.zoomIn")} disabled={status !== "ready"} onClick={() => zoom(PDF_ZOOM_STEP)} title={t("documentViewer.zoomIn")}>
          <Plus />
        </IconButton>
        <button
          className="vk-docview-fit"
          disabled={status !== "ready"}
          onClick={() => { if (stateRef.current) stateRef.current.viewer.currentScaleValue = "page-width"; }}
          type="button"
        >
          {t("documentViewer.fitWidth")}
        </button>
        <Input
          aria-label={t("documentViewer.find")}
          className="vk-docview-find"
          density="compact"
          disabled={status !== "ready"}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              search(event.shiftKey, true);
            }
          }}
          placeholder={t("documentViewer.find")}
          type="search"
          value={query}
        />
        {query && matches.total > 0 ? <span className="vk-docview-matches">{`${matches.current} / ${matches.total}`}</span> : null}
        <IconButton aria-label={t("documentViewer.findPrevious")} disabled={!query} onClick={() => search(true, true)} title={t("documentViewer.findPrevious")}>
          <ChevronUp />
        </IconButton>
        <IconButton aria-label={t("documentViewer.findNext")} disabled={!query} onClick={() => search(false, true)} title={t("documentViewer.findNext")}>
          <ChevronDown />
        </IconButton>
      </div>
      <div className="vk-docview-body">
        <div className="vk-docview-pdf" ref={containerRef}>
          <div className="pdfViewer" ref={viewerRef} />
        </div>
        {status === "loading" ? <DocumentViewerLoading /> : null}
        {status === "failed" ? <DocumentViewerFailure detail={error} /> : null}
      </div>
    </div>
  );
}
