// Markdown document tool card.
// Markdown 文档工具卡。
import * as React from "react";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { ChevronDown, ChevronRight, Code2, FileText, FolderTree, FolderSearch, GitBranch, Search } from "lucide-react";
import { ChangeSetDiffPreview } from "@/components/conversation/conversation-change-set-diff-preview";
import { dispatchConversationManualViewportInteraction } from "@/components/conversation/conversation-layout-events";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { ConversationMeasuredCollapse } from "@/components/conversation/conversation-measured-collapse";
import { FileTypeIcon, resolveFileTypeIcon } from "@/components/conversation/conversation-file-type-icon";
import { hydrateMarkdownToolPresentation, type MarkdownToolPresentation, type MarkdownToolPresentationIcon, type MarkdownToolSummaryTarget } from "@/components/conversation/conversation-tool-presentations";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import { ToolResultBody, type ToolRequestCardProgress, type ToolDetailLoader, activeToolStatusLabel, fileTargetLabel } from "./tool-card-common";

// markdownToolPresentationDetailCache avoids refetching full Markdown-output detail after remounts.
// markdownToolPresentationDetailCache 避免重挂后重复请求完整 Markdown 输出详情。
const markdownToolPresentationDetailCache = new Map<string, MarkdownToolPresentation>();

// MarkdownToolPresentationCard renders exact-tool Markdown output.
// MarkdownToolPresentationCard 渲染精确工具标识对应的 Markdown 输出。
export function MarkdownToolPresentationCard(props: {
  // Canonical input remains inspectable below the returned content without replacing it.
  // 规范输入保留在返回内容下方供检查，不能替代返回内容。
  v1ToolEvent?: ToolCallEvent;
  // Markdown-output presentation model.
  // Markdown 输出展示模型。
  presentation: MarkdownToolPresentation;
  // Whether this Markdown-output card is expanded.
  // 当前 Markdown 输出卡片是否展开。
  expanded: boolean;
  // Optional raw-detail action rendered inside expanded content.
  // 在展开内容内部渲染的可选原始详情操作。
  expandedFooter?: React.ReactNode;
  // Optional request stream state rendered inside this original card.
  // 在当前原始卡片内部渲染的可选请求流状态。
  requestProgress: ToolRequestCardProgress | null;
  // Updates the surface-level expansion state.
  // 更新界面级展开状态。
  onExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Loads one hidden tool-detail record for full Markdown output.
  // 加载单条隐藏工具详情记录用于完整 Markdown 输出。
  onLoadToolDetail?: ToolDetailLoader;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const {
    expanded,
    onExpandedChange,
    onLoadToolDetail,
    presentation,
  } = props;
  const detailCacheKey = markdownToolPresentationDetailCacheKey(presentation);
  const [hydratedPresentation, setHydratedPresentation] = React.useState<MarkdownToolPresentation>(
    () => detailCacheKey ? markdownToolPresentationDetailCache.get(detailCacheKey) ?? presentation : presentation,
  );
  React.useEffect(() => {
    const cached = detailCacheKey ? markdownToolPresentationDetailCache.get(detailCacheKey) : undefined;
    setHydratedPresentation(cached ?? presentation);
  }, [detailCacheKey, presentation]);
  React.useEffect(() => {
    if (!expanded || !onLoadToolDetail || !presentation.detailRef || !detailCacheKey || markdownToolPresentationDetailCache.has(detailCacheKey)) {
      return;
    }
    let cancelled = false;
    void onLoadToolDetail(presentation.detailRef)
      .then((payload) => {
        if (cancelled) {
          return;
        }
        const nextPresentation = hydrateMarkdownToolPresentation(presentation, payload);
        markdownToolPresentationDetailCache.set(detailCacheKey, nextPresentation);
        setHydratedPresentation(nextPresentation);
      })
      .catch(() => {
        if (!cancelled) {
          setHydratedPresentation(presentation);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [detailCacheKey, expanded, onLoadToolDetail, presentation]);
  const markdown = hydratedPresentation.markdown.trim();
  // markdownBlock is a fallback for Markdown tools and unparseable source envelopes.
  // markdownBlock 是 Markdown 工具及无法解析源码封套时的兜底内容。
  const markdownBlock = markdown ? markdownToolCodeBlock(markdown) : "";
  const title = t(hydratedPresentation.titleKey);
  // targetLabel keeps exact paths and line ranges visible while the card is collapsed.
  // targetLabel 在卡片折叠时持续展示精确路径与行范围。
  const targetLabel = markdownToolTargetLabel(hydratedPresentation.summaryTargets, t);
  // status is shown only while request formation or execution remains active.
  // status 仅在请求构造或执行仍进行时显示。
  const status = activeToolStatusLabel(
    props.requestProgress,
    hydratedPresentation.state,
    t("toolPresentation.execution.running"),
  );
  const handleToggle = React.useCallback(() => {
    dispatchConversationManualViewportInteraction();
    onExpandedChange(presentation.id, !expanded);
  }, [expanded, onExpandedChange, presentation.id]);
  return (
    <div className={cn("tool-presentation-card markdown-tool", expanded && "expanded")}>
      <button
        aria-expanded={expanded}
        className="tool-presentation-summary markdown-tool-summary"
        onClick={handleToggle}
        title={targetLabel ? `${title} · ${targetLabel}` : title}
        type="button"
      >
        {hydratedPresentation.toolId === "vulcan_file_list" ? <FolderSearch className="markdown-tool-icon" size={15} /> : <MarkdownToolIcon className="markdown-tool-icon" icon={hydratedPresentation.icon} size={15} />}
        {status ? <span className="tool-presentation-state shimmer">{status}</span> : null}
        <span className="tool-presentation-command">{hydratedPresentation.contentKind === "source" && hydratedPresentation.summaryTargets.length === 1 ? (
          <span className="file-access-target"><span className="file-operation-label">{title}</span>
            <FileTypeIcon icon={resolveFileTypeIcon(hydratedPresentation.summaryTargets[0].path)} />
            <span className="tool-presentation-command-text">{targetLabel}</span>
          </span>
        ) : <span className="tool-presentation-command-text">{title}{targetLabel ? ` · ${targetLabel}` : ""}</span>}</span>
        {expanded ? (
          <ChevronDown className="tool-presentation-toggle" size={15} />
        ) : (
          <ChevronRight className="tool-presentation-toggle" size={15} />
        )}
      </button>
      <ConversationMeasuredCollapse collapseId={`markdown-tool:${presentation.id}`} expanded={expanded}>
        <ToolResultBody>
            <div className="markdown-tool-block">
              {hydratedPresentation.contentKind === "source" && hydratedPresentation.sourceSections.length ? (
                <div className="file-access-detail markdown-tool-source-detail">
                  {hydratedPresentation.sourceSections.map((section) => (
                    <section className="file-access-item" key={section.id}>
                      {hydratedPresentation.sourceSections.length > 1 ? (
                        <div className="file-access-item-heading">{section.path}</div>
                      ) : null}
                      <ChangeSetDiffPreview
                        file={section.diffFile}
                        hunkSeparatorStyle="simple"
                        inlineDiff={false}
                      />
                    </section>
                  ))}
                </div>
              ) : markdownBlock ? (
                <MarkdownMessage content={markdownBlock} eager />
              ) : (
                <div className="markdown-tool-empty">{
                  hydratedPresentation.state === "running"
                    ? t("toolPresentation.execution.running")
                    : t("toolPresentation.output.empty")
                }</div>
              )}
            </div>
        </ToolResultBody>
        {props.expandedFooter}
      </ConversationMeasuredCollapse>
    </div>
  );
}

// markdownToolCodeBlock wraps raw tool output in a Markdown code fence.
// markdownToolCodeBlock 将原始工具输出包裹为 Markdown 代码块。
function markdownToolCodeBlock(markdown: string): string {
  return `\`\`\`markdown\n${markdown}\n\`\`\``;
}

// MarkdownToolIcon renders the icon selected by the exact tool-id adapter.
// MarkdownToolIcon 渲染精确工具标识适配器选择的图标。
function MarkdownToolIcon(props: {
  // Icon slot selected by the presentation adapter.
  // 展示适配器选择的图标槽位。
  icon: MarkdownToolPresentationIcon;
  // CSS class attached to the icon.
  // 附加到图标上的 CSS 类名。
  className?: string;
  // Icon size in pixels.
  // 图标像素尺寸。
  size: number;
}) {
  if (props.icon === "ast_tree") {
    return <GitBranch className={props.className} size={props.size} />;
  }
  if (props.icon === "file_text") {
    return <FileText className={props.className} size={props.size} />;
  }
  if (props.icon === "ast_detail") {
    return <Code2 className={props.className} size={props.size} />;
  }
  if (props.icon === "search") {
    return <Search className={props.className} size={props.size} />;
  }
  return <FolderTree className={props.className} size={props.size} />;
}

// markdownToolStatusLabel returns the localized lifecycle label for Markdown-output cards.
// markdownToolStatusLabel 返回 Markdown 输出卡片的本地化生命周期文案。
// markdownToolPresentationDetailCacheKey returns a cache key for one Markdown-output detail reference.
// markdownToolPresentationDetailCacheKey 返回单条 Markdown 输出详情引用的缓存键。
function markdownToolPresentationDetailCacheKey(presentation: MarkdownToolPresentation): string | null {
  if (!presentation.detailRef) {
    return null;
  }
  return [
    presentation.detailRef.sessionId,
    presentation.detailRef.recordId ?? "",
    presentation.detailRef.toolCallId ?? "",
    presentation.detailRef.seq ?? "",
    presentation.detailRef.messageIndex ?? "",
  ].join(":");
}

// markdownToolTargetLabel formats exact tool targets without deriving paths from result prose.
// markdownToolTargetLabel 在不从结果文本推导路径的前提下格式化精确工具目标。
function markdownToolTargetLabel(
  targets: MarkdownToolSummaryTarget[],
  t: (key: TranslationKey) => string,
): string {
  return targets
    .map((target) => fileTargetLabel(target.path, target.lineRanges, t))
    .join(", ");
}
