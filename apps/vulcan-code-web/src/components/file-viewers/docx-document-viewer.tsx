import React from "react";
import { Minus, Plus } from "lucide-react";
import "@/ui-kit/styles/features/document-viewer.css";
import { useI18n } from "@/i18n";
import { IconButton } from "@/ui-kit/primitives/button";
import { DocumentViewerFailure, DocumentViewerLoading } from "@/components/file-viewers/document-viewer-states";

// DOCX_ZOOM_STEP is the factor one zoom click scales the pages by.
// DOCX_ZOOM_STEP 是单次缩放点击使页面缩放的倍数。
const DOCX_ZOOM_STEP = 1.2;

// DocxDocumentViewer shows a Word document read-only with docx-preview: pages fit the panel width until the reader
// zooms, embedded HTML chunks are not rendered, and web links open outside the viewer.
// DocxDocumentViewer 用 docx-preview 只读显示 Word 文档：读者缩放前页面适应面板宽度，不渲染内嵌 HTML 片段，网页链接在
// 查看器之外打开。
export default function DocxDocumentViewer(props: { url: string; name: string }) {
  const { t } = useI18n();
  const scrollRef = React.useRef<HTMLDivElement | null>(null);
  const pagesRef = React.useRef<HTMLDivElement | null>(null);
  const [status, setStatus] = React.useState<"loading" | "ready" | "failed">("loading");
  const [error, setError] = React.useState("");
  // zoom is "fit" until the reader zooms by hand.
  // zoom 在读者手动缩放之前为 "fit"。
  const [zoom, setZoom] = React.useState<number | "fit">("fit");
  const [fitZoom, setFitZoom] = React.useState(1);

  React.useEffect(() => {
    const pages = pagesRef.current;
    if (!pages) return;
    const controller = new AbortController();
    setStatus("loading");
    pages.replaceChildren();
    void (async () => {
      const [{ renderAsync }, response] = await Promise.all([
        import("docx-preview"),
        fetch(props.url, { signal: controller.signal }),
      ]);
      if (!response.ok) throw new Error(`${response.status} ${await response.text()}`);
      const data = await response.blob();
      if (controller.signal.aborted) return;
      await renderAsync(data, pages, pages, {
        className: "docx",
        inWrapper: true,
        breakPages: true,
        ignoreLastRenderedPageBreak: true,
        renderAltChunks: false,
        renderChanges: false,
        renderComments: false,
        useBase64URL: true,
        experimental: false,
      });
      if (!controller.signal.aborted) setStatus("ready");
    })().catch((reason: unknown) => {
      if (controller.signal.aborted) return;
      setError(reason instanceof Error ? reason.message : String(reason));
      setStatus("failed");
    });
    return () => controller.abort();
  }, [props.url]);

  // The fit zoom makes the widest page span the panel; it follows panel resizes.
  // 适应缩放使最宽的页面撑满面板，并随面板尺寸变化。
  React.useEffect(() => {
    const scroll = scrollRef.current;
    const pages = pagesRef.current;
    if (!scroll || !pages || status !== "ready") return;
    const measure = () => {
      const widest = Math.max(0, ...Array.from(pages.querySelectorAll<HTMLElement>("section.docx"), (page) => page.offsetWidth));
      if (widest > 0) setFitZoom(Math.min(1, (scroll.clientWidth - 24) / widest));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(scroll);
    return () => observer.disconnect();
  }, [status]);

  const scale = zoom === "fit" ? fitZoom : zoom;
  const openLink = (event: React.MouseEvent) => {
    const anchor = (event.target as Element).closest("a");
    const href = anchor?.getAttribute("href") ?? "";
    if (!anchor || href.startsWith("#")) return;
    event.preventDefault();
    if (/^https?:\/\//i.test(href)) window.open(href, "_blank", "noopener,noreferrer");
  };

  return (
    <div className="vk-docview" data-kind="docx">
      <div className="vk-docview-toolbar" role="toolbar" aria-label={props.name}>
        <IconButton aria-label={t("documentViewer.zoomOut")} disabled={status !== "ready"} onClick={() => setZoom(Math.max(0.25, scale / DOCX_ZOOM_STEP))} title={t("documentViewer.zoomOut")}>
          <Minus />
        </IconButton>
        <IconButton aria-label={t("documentViewer.zoomIn")} disabled={status !== "ready"} onClick={() => setZoom(Math.min(4, scale * DOCX_ZOOM_STEP))} title={t("documentViewer.zoomIn")}>
          <Plus />
        </IconButton>
        <button className="vk-docview-fit" disabled={status !== "ready"} onClick={() => setZoom("fit")} type="button">
          {t("documentViewer.fitWidth")}
        </button>
      </div>
      <div className="vk-docview-body">
        <div className="vk-docview-scroll" data-find-scope="" onClick={openLink} ref={scrollRef}>
          <div className="vk-docview-docx" ref={pagesRef} style={{ zoom: scale }} />
        </div>
        {status === "loading" ? <DocumentViewerLoading /> : null}
        {status === "failed" ? <DocumentViewerFailure detail={error} /> : null}
      </div>
    </div>
  );
}
